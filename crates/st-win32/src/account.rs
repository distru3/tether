//! Which Windows account a process runs as (docs/adr/0005-per-account-usage.md).
//!
//! The agent tells accounts apart by the token of the process on the other
//! end of the pipe, never by anything the process says about itself.

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{LocalFree, HANDLE, HLOCAL};
use windows::Win32::NetworkManagement::NetManagement::{
    NetApiBufferFree, NetUserGetInfo, USER_INFO_10,
};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, LookupAccountSidW, TokenUser, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::System::Threading::OpenProcessToken;

use crate::{open_process_query, wide_to_string, OwnedProcessHandle};

/// A process's account: its SID (stable) and a name to show (the account's
/// full name when it has one, else its user name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessUser {
    pub sid: String,
    pub name: String,
}

/// The account process `pid` runs as. Needs the right to query the process
/// and its token: always true for SYSTEM, and for a user's own processes.
pub fn process_user(pid: u32) -> windows::core::Result<ProcessUser> {
    let process = open_process_query(pid)?;
    let mut raw = HANDLE::default();
    // SAFETY: `process` is a live handle; `raw` receives a new token handle on
    // success, which is wrapped in RAII immediately (CloseHandle works for
    // tokens as for processes).
    unsafe { OpenProcessToken(process.as_raw(), TOKEN_QUERY, &mut raw)? };
    let token = unsafe { OwnedProcessHandle::from_raw(raw) };

    // First call measures; the error it returns (insufficient buffer) is
    // expected. A u64 buffer keeps TOKEN_USER's pointer field aligned.
    let mut len = 0u32;
    // SAFETY: a null buffer with length 0 only reports the needed size.
    let _ = unsafe { GetTokenInformation(token.as_raw(), TokenUser, None, 0, &mut len) };
    let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
    // SAFETY: `buf` holds at least `len` bytes and outlives every use of the
    // SID pointer inside it below.
    unsafe {
        GetTokenInformation(
            token.as_raw(),
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            (buf.len() * 8) as u32,
            &mut len,
        )?
    };
    // SAFETY: on success the buffer starts with a TOKEN_USER.
    let sid_ptr = unsafe { (*(buf.as_ptr() as *const TOKEN_USER)).User.Sid };

    let mut text = PWSTR::null();
    // SAFETY: `sid_ptr` points into `buf`; the string is LocalAlloc'd and
    // freed right after copying.
    let sid = unsafe {
        ConvertSidToStringSidW(sid_ptr, &mut text)?;
        let sid = text.to_string().unwrap_or_default();
        let _ = LocalFree(HLOCAL(text.0.cast()));
        sid
    };

    let mut name = [0u16; 256];
    let mut name_len = name.len() as u32;
    let mut domain = [0u16; 256];
    let mut domain_len = domain.len() as u32;
    let mut kind = SID_NAME_USE::default();
    // SAFETY: both buffers are writable for the lengths passed.
    let user_name = unsafe {
        LookupAccountSidW(
            PCWSTR::null(),
            sid_ptr,
            PWSTR(name.as_mut_ptr()),
            &mut name_len,
            PWSTR(domain.as_mut_ptr()),
            &mut domain_len,
            &mut kind,
        )
    }
    .map(|()| wide_to_string(&name))
    .unwrap_or_default();

    let display = full_name(&user_name).filter(|n| !n.trim().is_empty());
    Ok(ProcessUser {
        name: display.unwrap_or_else(|| {
            if user_name.is_empty() {
                sid.clone()
            } else {
                user_name
            }
        }),
        sid,
    })
}

/// A local account's full name ("Sam Rivera" rather than "sam"), if set.
fn full_name(user_name: &str) -> Option<String> {
    if user_name.is_empty() {
        return None;
    }
    let wide: Vec<u16> = user_name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut buf: *mut u8 = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated; on success `buf` points to a
    // USER_INFO_10 owned by the API until NetApiBufferFree.
    let status = unsafe { NetUserGetInfo(PCWSTR::null(), PCWSTR(wide.as_ptr()), 10, &mut buf) };
    if status != 0 || buf.is_null() {
        return None;
    }
    // SAFETY: level 10 returns USER_INFO_10; the string is copied before the
    // buffer is freed.
    unsafe {
        let info = &*(buf as *const USER_INFO_10);
        let name = info.usri10_full_name.to_string().ok();
        let _ = NetApiBufferFree(Some(buf as *const _));
        name
    }
}
