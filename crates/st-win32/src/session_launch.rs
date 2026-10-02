//! Start a program on the interactive user's desktop from a SYSTEM service.
//!
//! The agent uses this to bring the per-user session helper back after it
//! exits (crash, or ended in Task Manager): without the helper there are no
//! focus reports and therefore no enforcement. `WTSQueryUserToken` needs
//! `SeTcbPrivilege`, which only LocalSystem holds, so this works from the
//! service and fails harmlessly (`Err`) from a console-mode dev agent.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken};
use windows::Win32::System::Threading::{
    CreateProcessAsUserW, CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION,
    STARTUPINFOW,
};

/// What [`launch_in_console_session`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchOutcome {
    /// The process was created in the active console session.
    Started,
    /// Nobody is signed in at the console (login screen, or between users).
    NoUser,
}

/// Where the session helper may live relative to `own_exe`: the same
/// directories [`crate::is_trusted_peer`] accepts (own dir, `bin\`, parent),
/// in that order.
pub fn helper_candidates(own_exe: &Path, file_name: &str) -> Vec<PathBuf> {
    let Some(dir) = own_exe.parent() else {
        return Vec::new();
    };
    let mut out = vec![dir.join(file_name), dir.join("bin").join(file_name)];
    if let Some(parent) = dir.parent() {
        out.push(parent.join(file_name));
    }
    out
}

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// Start `exe` (no arguments) as the user signed in at the console, on
/// their interactive desktop, with their environment.
pub fn launch_in_console_session(exe: &Path) -> std::io::Result<LaunchOutcome> {
    // SAFETY: plain Win32 calls; every handle and the environment block are
    // released on all paths below.
    unsafe {
        let session = WTSGetActiveConsoleSessionId();
        if session == u32::MAX {
            return Ok(LaunchOutcome::NoUser);
        }
        let mut token = HANDLE::default();
        if WTSQueryUserToken(session, &mut token).is_err() {
            let err = std::io::Error::last_os_error();
            // ERROR_NO_TOKEN (1008): the console session has no signed-in user.
            if err.raw_os_error() == Some(1008) {
                return Ok(LaunchOutcome::NoUser);
            }
            return Err(err);
        }

        let mut env: *mut c_void = std::ptr::null_mut();
        let env_ok = CreateEnvironmentBlock(&mut env, token, false).is_ok();

        let app = wide(exe.as_os_str());
        // CreateProcessW may write to the command line buffer.
        let mut cmdline = wide(format!("\"{}\"", exe.display()).as_ref());
        let workdir = exe.parent().map(|d| wide(d.as_os_str()));
        let mut desktop = wide("winsta0\\default".as_ref());
        let startup = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            lpDesktop: PWSTR(desktop.as_mut_ptr()),
            ..Default::default()
        };
        let mut info = PROCESS_INFORMATION::default();

        let created = CreateProcessAsUserW(
            token,
            PCWSTR(app.as_ptr()),
            PWSTR(cmdline.as_mut_ptr()),
            None,
            None,
            false,
            CREATE_NO_WINDOW
                | if env_ok {
                    CREATE_UNICODE_ENVIRONMENT
                } else {
                    Default::default()
                },
            if env_ok {
                Some(env as *const c_void)
            } else {
                None
            },
            workdir
                .as_ref()
                .map_or(PCWSTR::null(), |w| PCWSTR(w.as_ptr())),
            &startup,
            &mut info,
        );
        let result = match created {
            Ok(()) => {
                let _ = CloseHandle(info.hThread);
                let _ = CloseHandle(info.hProcess);
                Ok(LaunchOutcome::Started)
            }
            Err(_) => Err(std::io::Error::last_os_error()),
        };

        if env_ok {
            let _ = DestroyEnvironmentBlock(env);
        }
        let _ = CloseHandle(token);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_mirror_the_trusted_peer_directories() {
        let own = Path::new(r"C:\Program Files\Tether\screentime-agent.exe");
        let found = helper_candidates(own, "screentime-session.exe");
        assert_eq!(
            found,
            vec![
                PathBuf::from(r"C:\Program Files\Tether\screentime-session.exe"),
                PathBuf::from(r"C:\Program Files\Tether\bin\screentime-session.exe"),
                PathBuf::from(r"C:\Program Files\screentime-session.exe"),
            ]
        );
        for candidate in &found {
            assert!(crate::is_trusted_peer(
                candidate,
                own,
                &["screentime-session.exe"]
            ));
        }
    }
}
