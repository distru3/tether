//! Local transports for the IPC protocol.
//!
//! Framing ([`crate::read_message`] / [`crate::write_message`]) is deliberately
//! transport-agnostic; this module supplies the byte streams it runs over.
//!
//! # Security posture
//!
//! The transport is local only. Never bind a TCP socket. On Windows the server
//! side lives in the agent (a privileged service in production), so the pipe is
//! created by the agent and the default DACL inherits from it; a hostile
//! caller cannot open a pipe that was never created. Real peer authentication
//! (pipe ACLs on Windows, `SO_PEERCRED` on Linux) is layered in when the agent
//! runs at a raised privilege; during M1 both processes run as the same user.
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
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_CONNECTED,
        ERROR_PIPE_NOT_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

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

    /// Create one server-side instance of the pipe and wait for a client.
    ///
    /// Byte mode, not message mode: the IPC framing already length-prefixes, so
    /// relying on the OS to frame messages would be duplicating the job.
    pub(super) fn accept(name: &str) -> Result<PipeStream> {
        let (_keep_alive, path) = wide(&pipe_path(name));
        let access = PIPE_ACCESS_DUPLEX as FILE_FLAGS_AND_ATTRIBUTES;
        let handle = unsafe {
            CreateNamedPipeW(
                path,
                access,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                16 * 1024,
                16 * 1024,
                0,
                None,
            )
        };
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
    /// A server-side pipe instance may not exist yet (the agent can be mid-way
    /// through `accept`), so `ERROR_FILE_NOT_FOUND` is retried briefly before
    /// giving up.
    pub(super) fn connect(name: &str) -> Result<PipeStream> {
        let full = pipe_path(name);
        let mut last_err = None;
        for _ in 0..5 {
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
                    if e.code() != ERROR_FILE_NOT_FOUND.into() {
                        return Err(TransportError::Io(io::Error::new(
                            io::ErrorKind::NotFound,
                            e,
                        )));
                    }
                    last_err = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(20));
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
    use super::*;
    use std::io::{Read, Write};

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
