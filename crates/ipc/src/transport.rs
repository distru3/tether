//! Local transports for the IPC protocol.
//!
//! Framing ([`crate::read_message`] / [`crate::write_message`]) is deliberately
//! transport-agnostic; this module supplies the byte streams it runs over.
//!
//! # Security posture
//!
//! The transport is local only. Never bind a TCP socket. On Windows the server
//! side lives in the agent, which in production runs as LocalSystem from a
//! Windows service. That is exactly why the pipe is created with an EXPLICIT
//! security descriptor ([`win::CLIENT_PIPE_SDDL`]): the default descriptor of a
//! LocalSystem process can deny connect rights to ordinary user processes,
//! leaving every session-helper and dashboard connection rejected before any
//! protocol byte moves — a silent-deafness failure that looks identical to
//! "agent not running" from the outside. The SDDL grants SYSTEM and
//! Administrators full access plus generic read/write to Authenticated Users:
//! sufficient for unprivileged clients to connect and exchange frames, still
//! restricted to local logons because named pipes of this form are
//! machine-local and never reachable over the network.
//!
//! Why AU read/write is an acceptable grant here: the pipe namespace is
//! per-machine, so remote attackers are not in scope; frames are
//! length-prefixed JSON parsed by the IPC layer, not executed; and the
//! protocol itself gates every mutation behind the PIN (see the auth request in
//! [`crate`]) — read/write socket access buys an attacker only what an
//! unprivileged local user already has.
//!
//! The one-request-per-connection model is deliberate: the agent is
//! authoritative and the UI is a thin client, so each UI command opens a
//! connection, sends one frame and reads one response. No state machine, no
//! half-open sessions, no way for a hung client to pin the agent.

use std::io;

/// Failure to establish a transport. Distinct from [`crate::IpcError`], which
/// covers what happens *after* bytes start moving.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("transport not supported on this platform: {0}")]
    Unsupported(&'static str),
}

pub type Result<T> = std::result::Result<T, TransportError>;

/// One end of a local byte stream. Implemented by a Windows named pipe here;
/// a Unix domain socket lands with the Linux port.
#[cfg(windows)]
pub use win::PipeStream;

#[cfg(not(windows))]
pub type PipeStream = UnsupportedStream;

/// Unix: the agent has not shipped on Linux yet, so the transport is a stub
/// that always fails. Keeps the agent's server loop honest: it sees the same
/// "no transport" shape it would see on a Windows box without a pipe.
#[cfg(not(windows))]
#[derive(Debug)]
pub struct UnsupportedStream;

#[cfg(not(windows))]
impl io::Read for UnsupportedStream {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("transport unavailable on this platform"))
    }
}

#[cfg(not(windows))]
impl io::Write for UnsupportedStream {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("transport unavailable on this platform"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("transport unavailable on this platform"))
    }
}

/// Server side: wait for one client, returning the stream to serve.
///
/// One connection per call. The agent calls this in a loop, so a new pipe
/// instance is created for each UI command.
#[cfg(windows)]
pub fn server_accept(name: &str) -> Result<PipeStream> {
    win::accept(name)
}

/// Client side: connect to the agent's pipe.
///
/// Fails with `ERROR_FILE_NOT_FOUND` when the agent is not running; the UI
/// maps that to `agent_connected: false`.
#[cfg(windows)]
pub fn client_connect(name: &str) -> Result<PipeStream> {
    win::connect(name)
}

#[cfg(not(windows))]
pub fn server_accept(_name: &str) -> Result<PipeStream> {
    Err(TransportError::Unsupported("unix domain sockets"))
}

#[cfg(not(windows))]
pub fn client_connect(_name: &str) -> Result<PipeStream> {
    Err(TransportError::Unsupported("unix domain sockets"))
}

/// Windows named-pipe implementation.
#[cfg(windows)]
mod win {
    use super::{Result, TransportError};
    use std::io::{self, Read, Write};
    use windows::core::{Error as WinError, PCWSTR};
    // HLOCAL/LocalFree: the SDDL converter allocates its descriptor with
    // LocalAlloc, so the documented counterpart free lives in Foundation too.
    use windows::Win32::Foundation::{
        CloseHandle, LocalFree, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA,
        ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED, GENERIC_READ,
        GENERIC_WRITE, HANDLE, HLOCAL, INVALID_HANDLE_VALUE,
    };
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    /// SDDL for the server side of `\\.\pipe\screentime`.
    ///
    /// `D:P` starts a PROTECTED DACL: the inherited ACEs of whatever process
    /// created the pipe are discarded, so the descriptor below is the whole
    /// truth regardless of who runs the agent (a service's inherited ACL would
    /// otherwise lock normal users out entirely — see the module docs).
    ///
    /// * `(A;;FA;;;SY)` / `(A;;FA;;;BA)` — SYSTEM and Administrators: full
    ///   control. The privileged agent must always be able to manage its own
    ///   pipe even if a future hardening pass tightens user rights.
    /// * `(A;;GRGW;;;AU)` — Authenticated Users: generic read + write. This is
    ///   precisely "may open the pipe and exchange framed JSON"; it does not
    ///   include WRITE_DAC (cannot re-grant rights) or any of the service/pipe
    ///   management bits. Every mutating request still has to clear the
    ///   protocol-level PIN check, so this grant buys an unprivileged local
    ///   client nothing it could not do through the intended UI.
    pub(super) const CLIENT_PIPE_SDDL: &str = "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;GRGW;;;AU)";

    /// Full NT path for a pipe name.
    fn pipe_path(name: &str) -> String {
        format!(r"\\.\pipe\{name}")
    }

    /// Encode a string as a null-terminated UTF-16 buffer, returning a `PCWSTR`
    /// into it. The `Vec` must outlive the call so it is returned alongside.
    fn wide(name: &str) -> (Vec<u16>, PCWSTR) {
        let mut buf: Vec<u16> = name.encode_utf16().collect();
        buf.push(0);
        let ptr = PCWSTR(buf.as_ptr());
        (buf, ptr)
    }

    /// Build the SECURITY_ATTRIBUTES handed to `CreateNamedPipeW`, converting
    /// [`CLIENT_PIPE_SDDL`] into a real security descriptor.
    ///
    /// WHY per-call instead of cached in a static: the descriptor only has to
    /// outlive the kernel's snapshot during pipe creation, and building it is
    /// one `LocalAlloc` — negligible next to accepting a connection. That keeps
    /// ownership trivial (free right after `CreateNamedPipeW`) instead of
    /// inventing a process-lifetime holder with unsafe `Send`/`Sync`.
    ///
    /// Non-inheritable on purpose: the pipe handle must not leak into child
    /// processes of the agent; clients always open the pipe by name.
    pub(super) fn client_pipe_security_attributes() -> Result<SECURITY_ATTRIBUTES> {
        // SDDL input must be UTF-16 for the W variant; keep the buffer alive
        // until after the conversion call below.
        let (_keep_alive, sddl) = wide(CLIENT_PIPE_SDDL);

        let mut descriptor = PSECURITY_DESCRIPTOR(std::ptr::null_mut());
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl,
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(|e| TransportError::Io(io::Error::new(io::ErrorKind::PermissionDenied, e)))?;

        // The API documents a NULL descriptor as impossible when it reports
        // success, but a null pointer passed into CreateNamedPipeW would
        // silently fall back to the DEFAULT security descriptor — exactly the
        // deafness this code exists to prevent. Fail loudly instead.
        if descriptor.0.is_null() {
            return Err(TransportError::Io(io::Error::other(
                "SDDL conversion succeeded but produced a null security descriptor",
            )));
        }

        Ok(SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        })
    }

    /// Release a security descriptor allocated by the SDDL conversion above.
    /// The converter allocates with `LocalAlloc`, so `LocalFree` is the
    /// documented counterpart; errors are ignored because a failed free leaks
    /// one block at worst and there is no meaningful recovery.
    fn free_security_descriptor(sa: &SECURITY_ATTRIBUTES) {
        if !sa.lpSecurityDescriptor.is_null() {
            unsafe {
                LocalFree(HLOCAL(sa.lpSecurityDescriptor));
            }
        }
    }

    /// Create one server-side instance of the pipe and wait for a client.
    ///
    /// Byte mode, not message mode: the IPC framing already length-prefixes, so
    /// relying on the OS to frame messages would be duplicating the job.
    ///
    /// The pipe carries an explicit protected DACL (see [`CLIENT_PIPE_SDDL`])
    /// because the agent runs as LocalSystem in production, where the DEFAULT
    /// descriptor can reject connects from unprivileged session helpers —
    /// surfacing only as a deaf pipe with no error on either side.
    pub(super) fn accept(name: &str) -> Result<PipeStream> {
        let (_keep_alive, path) = wide(&pipe_path(name));
        let access = PIPE_ACCESS_DUPLEX as FILE_FLAGS_AND_ATTRIBUTES;
        let sa = client_pipe_security_attributes()?;
        let handle = unsafe {
            CreateNamedPipeW(
                path,
                access,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                16 * 1024,
                16 * 1024,
                0,
                Some(&sa),
            )
        };
        // The kernel snapshots the descriptor while creating the pipe instance,
        // so our copy is dead weight the moment CreateNamedPipeW returns —
        // success or failure alike.
        free_security_descriptor(&sa);
        if handle == INVALID_HANDLE_VALUE {
            return Err(TransportError::Io(io::Error::last_os_error()));
        }

        let connected = unsafe { ConnectNamedPipe(handle, None) };
        match connected {
            Ok(()) => Ok(PipeStream { handle }),
            Err(e) if e.code() == ERROR_PIPE_CONNECTED.into() => {
                // The client connected between CreateNamedPipeW and
                // ConnectNamedPipe; that is a successful accept.
                Ok(PipeStream { handle })
            }
            Err(e) => {
                unsafe {
                    let _ = DisconnectNamedPipe(handle);
                    let _ = CloseHandle(handle);
                }
                Err(TransportError::Io(io::Error::new(
                    io::ErrorKind::ConnectionRefused,
                    e,
                )))
            }
        }
    }

    /// Client side: open the pipe. `ERROR_FILE_NOT_FOUND` means the agent is
    /// not running, surfaced as an io error the UI can inspect.
    ///
    /// Two transient conditions are retried across a generous window (~1 s):
    ///
    /// * `ERROR_FILE_NOT_FOUND` — no instance exists right now. Even a healthy
    ///   agent has microsecond gaps between handing one accepted connection to
    ///   its worker and creating the next listening instance.
    /// * `ERROR_PIPE_BUSY` — instances exist but all are occupied. The classic
    ///   named-pipe client mistake is treating this as fatal; the documented
    ///   remedy is to wait (`WaitNamedPipe`) and reopen.
    pub(super) fn connect(name: &str) -> Result<PipeStream> {
        const ATTEMPTS: u32 = 50;
        const RETRY_DELAY_MS: u64 = 20;
        let full = pipe_path(name);
        let mut last_err = None;
        for _ in 0..ATTEMPTS {
            let (_keep_alive, path) = wide(&full);
            match unsafe {
                CreateFileW(
                    path,
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    None,
                )
            } {
                Ok(handle) if handle != INVALID_HANDLE_VALUE => {
                    return Ok(PipeStream { handle });
                }
                Ok(_) => return Err(TransportError::Io(io::Error::last_os_error())),
                Err(e) => {
                    let code = e.code();
                    if code != ERROR_FILE_NOT_FOUND.into() && code != ERROR_PIPE_BUSY.into() {
                        return Err(TransportError::Io(io::Error::new(
                            io::ErrorKind::NotFound,
                            e,
                        )));
                    }
                    last_err = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                }
            }
        }
        Err(TransportError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            last_err.unwrap_or_else(|| {
                WinError::from_hresult(windows::core::HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0))
            }),
        )))
    }

    #[derive(Debug)]
    pub struct PipeStream {
        handle: HANDLE,
    }

    impl PipeStream {
        fn read_raw(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let mut read: u32 = 0;
            unsafe { ReadFile(self.handle, Some(buf), Some(&mut read), None) }
                .map_err(win_io_error)?;
            Ok(read as usize)
        }

        fn write_raw(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut written: u32 = 0;
            unsafe { WriteFile(self.handle, Some(buf), Some(&mut written), None) }
                .map_err(win_io_error)?;
            Ok(written as usize)
        }
    }

    impl Read for PipeStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.read_raw(buf)
        }
    }

    impl Write for PipeStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.write_raw(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Drop for PipeStream {
        fn drop(&mut self) {
            // Close only. `DisconnectNamedPipe` would discard any buffered
            // response the client has not read yet (and this is a one-shot
            // connection, so there is no instance to reuse anyway).
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }

    /// A clean peer close or reset reads as EOF, so the framing layer's
    /// "connection closed" handling works unchanged.
    fn win_io_error(e: WinError) -> io::Error {
        let code = e.code();
        if code == ERROR_BROKEN_PIPE.into()
            || code == ERROR_NO_DATA.into()
            || code == ERROR_PIPE_NOT_CONNECTED.into()
        {
            io::Error::new(io::ErrorKind::UnexpectedEof, e)
        } else {
            io::Error::other(e)
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::win::{client_pipe_security_attributes, CLIENT_PIPE_SDDL};
    use super::*;
    use std::io::{Read, Write};
    // HLOCAL/LocalFree: the SDDL converter allocates its descriptor with
    // LocalAlloc, so the documented counterpart free lives in Foundation too.
    use windows::Win32::Foundation::{LocalFree, BOOL, HLOCAL};
    use windows::Win32::Security::{
        IsValidSecurityDescriptor, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    };

    /// The SDDL is the security contract for the whole IPC surface; assert its
    /// ACE layout so an accidental edit (dropping the protected flag, granting
    /// AU more than read/write) fails loudly here instead of silently widening
    /// production access.
    #[test]
    fn pipe_sddl_is_protected_dacl_with_system_admin_full_and_users_read_write() {
        assert_eq!(
            CLIENT_PIPE_SDDL,
            "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;GRGW;;;AU)"
        );
        assert!(
            CLIENT_PIPE_SDDL.starts_with("D:P"),
            "DACL must be protected"
        );
        assert!(
            CLIENT_PIPE_SDDL.contains("(A;;FA;;;SY)"),
            "SYSTEM needs full control"
        );
        assert!(
            CLIENT_PIPE_SDDL.contains("(A;;FA;;;BA)"),
            "Administrators need full control"
        );
        assert!(
            CLIENT_PIPE_SDDL.contains("(A;;GRGW;;;AU)"),
            "Authenticated Users get connect-level access only"
        );
        // Exactly one AU grant: no second, looser user ACE hiding at the end.
        assert_eq!(CLIENT_PIPE_SDDL.matches(";;;AU)").count(), 1);
    }

    /// The SA must convert from SDDL into a well-formed, non-null descriptor —
    /// a null descriptor passed to CreateNamedPipeW would silently revert to
    /// the DEFAULT security descriptor and reintroduce the LocalSystem
    /// deafness this module guards against. Pure builder check; no pipe needed.
    #[test]
    fn client_security_attributes_build_from_sddl_with_valid_non_null_descriptor() {
        let sa = client_pipe_security_attributes().expect("SECURITY_ATTRIBUTES from SDDL");
        assert!(!sa.lpSecurityDescriptor.is_null());
        assert_eq!(
            sa.nLength as usize,
            std::mem::size_of::<SECURITY_ATTRIBUTES>()
        );
        assert_eq!(sa.bInheritHandle, BOOL(0), "handle must not be inheritable");

        // Ask Windows itself whether the converted descriptor parses as valid;
        // this catches malformed SDDL that conversion happened to accept.
        unsafe {
            assert_ne!(
                IsValidSecurityDescriptor(PSECURITY_DESCRIPTOR(sa.lpSecurityDescriptor)),
                BOOL(0),
                "converted security descriptor must be valid"
            );
            LocalFree(HLOCAL(sa.lpSecurityDescriptor));
        }
    }

    /// Spin up a server thread, connect from the main thread, and confirm a
    /// request round-trips over a real named pipe.
    #[test]
    fn request_round_trips_over_a_named_pipe() {
        let name = format!("screentime_test_{}", std::process::id());
        let server_name = name.clone();

        let server = std::thread::spawn(move || {
            let mut stream = server_accept(&server_name).expect("accept");
            let mut buf = [0u8; 16];
            let n = stream.read(&mut buf).expect("read");
            stream.write_all(&buf[..n]).expect("write back");
        });

        // The client retries on FILE_NOT_FOUND, so the server may still be
        // creating its pipe instance when we connect.
        let mut stream = client_connect(&name).expect("connect");
        let payload = b"ping";
        stream.write_all(payload).expect("write");
        let mut buf = [0u8; 16];
        let n = stream.read(&mut buf).expect("read");
        assert_eq!(&buf[..n], payload);

        server.join().expect("server thread");
    }

    #[test]
    fn connect_fails_when_no_server_is_listening() {
        let name = format!("screentime_nobody_{}", std::process::id());
        let err = client_connect(&name).expect_err("no agent running");
        assert!(matches!(err, TransportError::Io(_)));
    }
}
