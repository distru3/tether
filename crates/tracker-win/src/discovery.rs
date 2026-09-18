//! Proactive Windows application discovery.
//!
//! Scans installed applications across Start Menu shortcuts and the Windows
//! Uninstall registry without requiring the applications to be running.
//! Normalizes paths to guarantee 100% parity with live foreground window tracking.

use std::collections::HashMap;
use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use st_core::model::AppKey;
use st_ipc::DiscoveredAppDto;
use tracing::debug;

use windows::core::{w, Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, MAX_PATH};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, STGM,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_DWORD, REG_EXPAND_SZ,
    REG_SZ, REG_VALUE_TYPE,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

/// Discover installed applications on the Windows machine.
///
/// Discovers Win32 desktop apps via:
/// 1. Start Menu shortcut (.lnk) traversal (All Users and Current User).
/// 2. Windows Uninstall Registry (HKLM 64-bit, HKLM 32-bit, and HKCU).
pub fn scan_installed_apps() -> Vec<DiscoveredAppDto> {
    let mut discovered: HashMap<AppKey, DiscoveredAppDto> = HashMap::new();

    // 1. Scan Start Menu shortcuts
    scan_start_menu(&mut discovered);

    // 2. Scan Registry Uninstall keys
    scan_uninstall_registry(&mut discovered);

    // 3. Scan Windows GameConfigStore (games registered by DirectX / Game Bar)
    scan_game_config_store(&mut discovered);

    debug!(
        total = discovered.len(),
        "proactive app discovery completed"
    );

    discovered.into_values().collect()
}

/// Helper to convert a string to a null-terminated wide string.
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Helper to convert an OsString to a null-terminated wide string.
fn os_to_wide(s: &OsString) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// Normalizes and expands 8.3 short paths to full canonical Windows paths.
fn canonicalize_path(raw: &str) -> String {
    let wide_raw = to_wide(raw);
    let mut buf = vec![0u16; 1024];
    let len = unsafe {
        windows::Win32::Storage::FileSystem::GetLongPathNameW(
            PCWSTR(wide_raw.as_ptr()),
            Some(&mut buf),
        )
    };
    let resolved = if len > 0 && (len as usize) < buf.len() {
        String::from_utf16_lossy(&buf[..len as usize])
    } else {
        raw.to_string()
    };
    resolved.replace('/', "\\").to_lowercase()
}

/// Checks if an executable filename looks like an uninstaller, updater, or helper.
fn is_noise_executable(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("unins")
        || lower.contains("uninstall")
        || lower.contains("installer")
        || lower.contains("setup")
        || lower.contains("update")
        || lower.contains("patcher")
        || lower.contains("crashpad")
        || lower.contains("vcredist")
        || lower.contains("dxsetup")
        || lower == "helper.exe"
        || lower == "cmd.exe"
        || lower == "powershell.exe"
        || lower == "conhost.exe"
}

// ---------------------------------------------------------------------------
// Start Menu (.lnk) Scanner
// ---------------------------------------------------------------------------

fn scan_start_menu(discovered: &mut HashMap<AppKey, DiscoveredAppDto>) {
    // Initialize COM for this thread so IShellLink can be used
    let com_init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

    let mut roots: Vec<PathBuf> = Vec::new();

    if let Ok(program_data) = std::env::var("ProgramData") {
        roots.push(
            PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }

    if let Ok(app_data) = std::env::var("APPDATA") {
        roots.push(
            PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }

    for root in roots {
        if !root.exists() {
            continue;
        }
        collect_lnk_recursive(&root, discovered);
    }

    if com_init.is_ok() {
        unsafe { CoUninitialize() };
    }
}

fn collect_lnk_recursive(dir: &Path, discovered: &mut HashMap<AppKey, DiscoveredAppDto>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_lnk_recursive(&path, discovered);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("lnk") {
            if let Some((target_path, display_name)) = resolve_lnk(&path) {
                let target_lower = canonicalize_path(&target_path);
                let file_name = Path::new(&target_path)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("");

                if !file_name.ends_with(".exe") || is_noise_executable(file_name) {
                    continue;
                }

                let key = AppKey::windows_exe(&target_lower);
                discovered
                    .entry(key.clone())
                    .or_insert_with(|| DiscoveredAppDto {
                        key,
                        display_name,
                        publisher: None,
                    });
            }
        }
    }
}

fn resolve_lnk(lnk_path: &Path) -> Option<(String, String)> {
    let wide_path = os_to_wide(&lnk_path.as_os_str().to_os_string());

    let stem = lnk_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    unsafe {
        let shell_link: IShellLinkW = match CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
        {
            Ok(sl) => sl,
            Err(_) => return None,
        };

        let persist_file: IPersistFile = match shell_link.cast() {
            Ok(pf) => pf,
            Err(_) => return None,
        };

        // STGM_READ = 0
        if persist_file
            .Load(PCWSTR(wide_path.as_ptr()), STGM(0))
            .is_err()
        {
            return None;
        }

        let mut path_buf = [0u16; MAX_PATH as usize];
        if shell_link
            .GetPath(&mut path_buf, std::ptr::null_mut(), 0)
            .is_err()
        {
            return None;
        }

        let len = path_buf
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(path_buf.len());
        if len == 0 {
            return None;
        }

        let mut target_str = String::from_utf16_lossy(&path_buf[..len]);

        // Handle Squirrel installer shortcuts (e.g. Update.exe --processStart Discord.exe)
        let mut args_buf = [0u16; MAX_PATH as usize];
        if shell_link.GetArguments(&mut args_buf).is_ok() {
            let args_len = args_buf
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(args_buf.len());
            let args_str = String::from_utf16_lossy(&args_buf[..args_len]);
            if target_str.to_ascii_lowercase().ends_with("update.exe") {
                if let Some(pos) = args_str.find("--processStart") {
                    let rest = args_str[pos + "--processStart".len()..].trim();
                    let target_exe = rest
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim_matches('"');
                    if !target_exe.is_empty() {
                        let base_dir = Path::new(&target_str).parent();
                        if let Some(dir) = base_dir {
                            if let Some(resolved) = find_squirrel_target(dir, target_exe) {
                                target_str = resolved;
                            }
                        }
                    }
                }
            }
        }

        if Path::new(&target_str).is_file() {
            Some((target_str, stem))
        } else {
            None
        }
    }
}

/// Squirrel apps place real versions under `app-<version>/<target_exe>`
fn find_squirrel_target(dir: &Path, exe_name: &str) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut candidate_dirs: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("app-") {
                candidate_dirs.push(p);
            }
        }
    }

    // Sort to pick the latest version
    candidate_dirs.sort();
    if let Some(latest) = candidate_dirs.last() {
        let exe_path = latest.join(exe_name);
        if exe_path.is_file() {
            return exe_path.to_str().map(|s| s.to_string());
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Registry Uninstall Scanner
// ---------------------------------------------------------------------------

fn scan_uninstall_registry(discovered: &mut HashMap<AppKey, DiscoveredAppDto>) {
    let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall");

    // HKLM 64-bit
    scan_reg_hive(
        HKEY_LOCAL_MACHINE,
        subkey,
        KEY_READ | KEY_WOW64_64KEY,
        discovered,
    );
    // HKLM 32-bit (Wow6432Node)
    scan_reg_hive(
        HKEY_LOCAL_MACHINE,
        subkey,
        KEY_READ | KEY_WOW64_32KEY,
        discovered,
    );
    // HKCU
    scan_reg_hive(HKEY_CURRENT_USER, subkey, KEY_READ, discovered);
}

fn scan_reg_hive(
    root: HKEY,
    subkey: PCWSTR,
    flags: windows::Win32::System::Registry::REG_SAM_FLAGS,
    discovered: &mut HashMap<AppKey, DiscoveredAppDto>,
) {
    let mut hkey = HKEY::default();
    let res = unsafe { RegOpenKeyExW(root, subkey, 0, flags, &mut hkey) };
    if res != ERROR_SUCCESS {
        return;
    }

    let mut index = 0u32;
    let mut name_buf = [0u16; 256];

    loop {
        let mut name_len = name_buf.len() as u32;
        let enum_res = unsafe {
            RegEnumKeyExW(
                hkey,
                index,
                PWSTR(name_buf.as_mut_ptr()),
                &mut name_len,
                None,
                PWSTR::null(),
                None,
                None,
            )
        };

        if enum_res != ERROR_SUCCESS {
            break;
        }

        let sub_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
        inspect_app_key(hkey, &sub_name, flags, discovered);
        index += 1;
    }

    unsafe {
        let _ = RegCloseKey(hkey);
    }
}

fn inspect_app_key(
    parent: HKEY,
    sub_name: &str,
    flags: windows::Win32::System::Registry::REG_SAM_FLAGS,
    discovered: &mut HashMap<AppKey, DiscoveredAppDto>,
) {
    let sub_wide = to_wide(sub_name);
    let mut app_key = HKEY::default();
    let res = unsafe { RegOpenKeyExW(parent, PCWSTR(sub_wide.as_ptr()), 0, flags, &mut app_key) };
    if res != ERROR_SUCCESS {
        return;
    }

    // Skip system components or updates
    if query_reg_dword(app_key, "SystemComponent").unwrap_or(0) == 1 {
        unsafe {
            let _ = RegCloseKey(app_key);
        }
        return;
    }

    if query_reg_string(app_key, "ParentKeyName").is_some() {
        unsafe {
            let _ = RegCloseKey(app_key);
        }
        return;
    }

    let display_name = match query_reg_string(app_key, "DisplayName") {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => {
            unsafe {
                let _ = RegCloseKey(app_key);
            }
            return;
        }
    };

    let publisher = query_reg_string(app_key, "Publisher").map(|p| p.trim().to_string());

    // Resolve target executable
    let mut target_exe: Option<String> = None;

    if let Some(icon) = query_reg_string(app_key, "DisplayIcon") {
        if let Some(cleaned) = clean_display_icon(&icon, &display_name) {
            target_exe = Some(cleaned);
        }
    }

    if target_exe.is_none() {
        if let Some(install_loc) = query_reg_string(app_key, "InstallLocation") {
            let loc_path = Path::new(&install_loc);
            if loc_path.is_dir() {
                target_exe = find_main_exe_in_dir(loc_path, &display_name);
            }
        }
    }

    if let Some(exe_path) = target_exe {
        let target_lower = canonicalize_path(&exe_path);
        let file_name = Path::new(&exe_path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("");

        if !file_name.ends_with(".exe") || is_noise_executable(file_name) {
            unsafe {
                let _ = RegCloseKey(app_key);
            }
            return;
        }

        let key = AppKey::windows_exe(&target_lower);
        discovered
            .entry(key.clone())
            .and_modify(|d| {
                if d.publisher.is_none() && publisher.is_some() {
                    d.publisher = publisher.clone();
                }
            })
            .or_insert_with(|| DiscoveredAppDto {
                key,
                display_name,
                publisher,
            });
    }

    unsafe {
        let _ = RegCloseKey(app_key);
    }
}

/// Cleans a `DisplayIcon` registry value into an actual file path.
///
/// Registry DisplayIcon often looks like:
/// - `"C:\Program Files\App\app.exe",0`
/// - `C:\Program Files\App\app.exe,-1`
/// - `"C:\Program Files\App\app.exe"`
/// - `"C:\Users\User\AppData\Local\App\app.ico"`
fn clean_display_icon(raw: &str, display_name: &str) -> Option<String> {
    let mut s = raw.trim();

    // Strip trailing icon indices (e.g. `,0` or `,-1`)
    if let Some((path_part, _)) = s.split_once(',') {
        s = path_part.trim();
    }

    // Strip surrounding quotes
    s = s.trim_matches('"').trim();

    if s.ends_with(".exe") && Path::new(s).is_file() {
        return Some(canonicalize_path(s));
    }

    // If icon is an .ico or in the app directory, search its parent directory for the exe
    let path = Path::new(s);
    if let Some(parent) = path.parent() {
        if parent.is_dir() {
            if let Some(found) = find_main_exe_in_dir(parent, display_name) {
                return Some(canonicalize_path(&found));
            }
        }
    }

    None
}

/// If DisplayIcon is missing, searches InstallLocation for a prominent `.exe`.
fn find_main_exe_in_dir(dir: &Path, display_name: &str) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut candidates: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("exe") {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !is_noise_executable(name) {
                candidates.push(path);
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    // Exact match with display name (e.g. "Discord" -> "Discord.exe")
    let display_lower = display_name.to_ascii_lowercase();
    for cand in &candidates {
        let stem = cand
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if stem == display_lower || display_lower.contains(&stem) {
            return cand.to_str().map(|s| s.to_string());
        }
    }

    // If only one candidate executable exists in the directory, assume it's the main app
    if candidates.len() == 1 {
        return candidates[0].to_str().map(|s| s.to_string());
    }

    None
}

fn query_reg_string(key: HKEY, value_name: &str) -> Option<String> {
    let wide_name = to_wide(value_name);
    unsafe {
        let mut byte_len = 0u32;
        let res = RegQueryValueExW(
            key,
            PCWSTR(wide_name.as_ptr()),
            None,
            None,
            None,
            Some(&mut byte_len),
        );
        if res != ERROR_SUCCESS || byte_len == 0 {
            return None;
        }

        let mut data = vec![0u8; byte_len as usize];
        let mut kind = REG_VALUE_TYPE::default();
        let res = RegQueryValueExW(
            key,
            PCWSTR(wide_name.as_ptr()),
            None,
            Some(&mut kind),
            Some(data.as_mut_ptr()),
            Some(&mut byte_len),
        );
        if res != ERROR_SUCCESS {
            return None;
        }

        if kind != REG_SZ && kind != REG_EXPAND_SZ {
            return None;
        }

        let units: Vec<u16> = data[..byte_len as usize]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let text = String::from_utf16_lossy(&units);
        Some(text.trim_end_matches('\0').to_string())
    }
}

fn query_reg_dword(key: HKEY, value_name: &str) -> Option<u32> {
    let wide_name = to_wide(value_name);
    unsafe {
        let mut val = 0u32;
        let mut kind = REG_VALUE_TYPE::default();
        let mut byte_len = std::mem::size_of::<u32>() as u32;
        let res = RegQueryValueExW(
            key,
            PCWSTR(wide_name.as_ptr()),
            None,
            Some(&mut kind),
            Some(&mut val as *mut u32 as *mut u8),
            Some(&mut byte_len),
        );
        if res != ERROR_SUCCESS || kind != REG_DWORD {
            return None;
        }
        Some(val)
    }
}

// ---------------------------------------------------------------------------
// Windows GameConfigStore Scanner
// ---------------------------------------------------------------------------

fn scan_game_config_store(discovered: &mut HashMap<AppKey, DiscoveredAppDto>) {
    let subkey = w!("System\\GameConfigStore\\Children");
    let mut hkey = HKEY::default();
    let res = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey, 0, KEY_READ, &mut hkey) };
    if res != ERROR_SUCCESS {
        return;
    }

    let mut index = 0u32;
    let mut name_buf = [0u16; 256];
    loop {
        let mut name_len = name_buf.len() as u32;
        let enum_res = unsafe {
            RegEnumKeyExW(
                hkey,
                index,
                PWSTR(name_buf.as_mut_ptr()),
                &mut name_len,
                None,
                PWSTR::null(),
                None,
                None,
            )
        };
        if enum_res != ERROR_SUCCESS {
            break;
        }

        let sub_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
        inspect_game_config_key(hkey, &sub_name, discovered);
        index += 1;
    }

    unsafe {
        let _ = RegCloseKey(hkey);
    }
}

fn inspect_game_config_key(
    parent: HKEY,
    sub_name: &str,
    discovered: &mut HashMap<AppKey, DiscoveredAppDto>,
) {
    let wide_sub = to_wide(sub_name);
    let mut child = HKEY::default();
    let res = unsafe { RegOpenKeyExW(parent, PCWSTR(wide_sub.as_ptr()), 0, KEY_READ, &mut child) };
    if res != ERROR_SUCCESS {
        return;
    }

    let game_type = query_reg_dword(child, "Type");
    let exe_path = query_reg_string(child, "MatchedExeFullPath");
    let title = query_reg_string(child, "Title");

    unsafe {
        let _ = RegCloseKey(child);
    }

    // Type 1 indicates an application recognized as a game by Windows DirectX / Game Bar
    if game_type == Some(1) {
        if let Some(path) = exe_path {
            let path_trimmed = path.trim().trim_matches('"');
            if path_trimmed.ends_with(".exe") && Path::new(path_trimmed).is_file() {
                let target_lower = canonicalize_path(path_trimmed);
                let file_name = Path::new(&target_lower)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if !file_name.is_empty() && !is_noise_executable(file_name) {
                    let display_name =
                        title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
                            Path::new(path_trimmed)
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Game")
                                .to_string()
                        });
                    let key = AppKey::windows_exe(&target_lower);
                    discovered
                        .entry(key.clone())
                        .and_modify(|d| {
                            if d.publisher.is_none() {
                                d.publisher = Some("Game".to_string());
                            }
                        })
                        .or_insert_with(|| DiscoveredAppDto {
                            key,
                            display_name,
                            publisher: Some("Game".to_string()),
                        });
                }
            }
        }
    }
}
