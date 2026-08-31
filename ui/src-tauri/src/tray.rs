//! System tray icon and emergency safety controls for Screentime.
//!
//! Provides a taskbar notification area ("show hidden icons") tray icon that
//! allows the user to:
//! 1. Open / Focus the dashboard.
//! 2. Run emergency network reset (undo all DNS, hosts, firewall, and browser changes) silently.
//! 3. Stop all Screentime services and terminate the background agent/session silently.
//! 4. Safely quit the application.

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use std::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(windows)]
fn silent_cmd(program: &str, args: &[&str]) {
    let _ = Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

/// Run emergency network cleanup from the UI host process completely silently.
pub fn run_emergency_network_reset() {
    #[cfg(windows)]
    {
        // 1. Clean hosts file
        let hosts_path = std::path::Path::new(r"C:\Windows\System32\drivers\etc\hosts");
        if let Ok(content) = std::fs::read_to_string(hosts_path) {
            if content.contains("# >>> screentime managed block >>>") {
                let mut out = String::new();
                let mut inside = false;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed == "# >>> screentime managed block >>>" {
                        inside = true;
                        continue;
                    }
                    if trimmed == "# <<< screentime managed block <<<" {
                        inside = false;
                        continue;
                    }
                    if !inside {
                        out.push_str(line);
                        out.push('\n');
                    }
                }
                let _ = std::fs::write(hosts_path, out);
            }
        }

        // 2. Reset common network adapter DNS settings to DHCP silently
        for adapter in &[
            "Wi-Fi",
            "Ethernet",
            "Ethernet 2",
            "Ethernet 3",
            "Local Area Connection",
        ] {
            silent_cmd(
                "netsh",
                &[
                    "interface",
                    "ipv4",
                    "set",
                    "dnsservers",
                    &format!("name={adapter}"),
                    "source=dhcp",
                ],
            );
        }

        // 3. Delete Screentime firewall rules silently
        for rule in &[
            "Screentime-Block-DoT-TCP",
            "Screentime-Block-DoT-UDP",
            "Screentime-Block-Outbound-DNS",
            "Screentime-Allow-Upstream-DNS",
        ] {
            silent_cmd(
                "netsh",
                &[
                    "advfirewall",
                    "firewall",
                    "delete",
                    "rule",
                    &format!("name={rule}"),
                ],
            );
        }

        // 4. Remove browser DoH registry policies silently
        silent_cmd(
            "reg",
            &[
                "delete",
                r"HKLM\SOFTWARE\Policies\Google\Chrome",
                "/v",
                "DnsOverHttpsMode",
                "/f",
            ],
        );
        silent_cmd(
            "reg",
            &[
                "delete",
                r"HKLM\SOFTWARE\Policies\Microsoft\Edge",
                "/v",
                "DnsOverHttpsMode",
                "/f",
            ],
        );
        silent_cmd(
            "reg",
            &[
                "delete",
                r"HKLM\SOFTWARE\Policies\BraveSoftware\Brave",
                "/v",
                "DnsOverHttpsMode",
                "/f",
            ],
        );
        silent_cmd(
            "reg",
            &[
                "delete",
                r"HKLM\SOFTWARE\Policies\Mozilla\Firefox\DNSOverHTTPS",
                "/v",
                "Enabled",
                "/f",
            ],
        );
        silent_cmd(
            "reg",
            &[
                "delete",
                r"HKLM\SOFTWARE\Policies\Mozilla\Firefox\DNSOverHTTPS",
                "/v",
                "Locked",
                "/f",
            ],
        );

        // 5. Flush local DNS resolver cache silently
        silent_cmd("ipconfig", &["/flushdns"]);
    }
}

/// Stop all background processes (agent, session) and close the app silently without cmd flashing.
pub fn stop_all_services(app: &AppHandle) {
    // First reset network so no stale DNS/hosts locks remain.
    run_emergency_network_reset();

    #[cfg(windows)]
    {
        // Stop the session sampling front silently
        silent_cmd("taskkill", &["/F", "/IM", "screentime-session.exe"]);

        // Stop the agent service if registered
        silent_cmd("sc.exe", &["stop", "ScreentimeAgent"]);

        // Kill agent console instance if running
        silent_cmd("taskkill", &["/F", "/IM", "screentime-agent.exe"]);
    }

    app.exit(0);
}

/// Initialize the system tray icon with menu actions and click handlers.
pub fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let open_item = MenuItem::with_id(app, "open", "Open Screentime", true, None::<&str>)?;
    let reset_item = MenuItem::with_id(
        app,
        "reset_net",
        "Emergency Reset Network",
        true,
        None::<&str>,
    )?;
    let stop_item = MenuItem::with_id(
        app,
        "stop_all",
        "Exit & Stop All Services",
        true,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Screentime UI", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open_item, &reset_item, &stop_item, &quit_item])?;

    let Some(icon) = app.default_window_icon().cloned() else {
        tracing::warn!("Default window icon missing; skipping tray creation");
        return Ok(());
    };

    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Screentime")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "reset_net" => {
                run_emergency_network_reset();
            }
            "stop_all" => {
                stop_all_services(app);
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let is_visible = window.is_visible().unwrap_or(false);
                    if is_visible {
                        let _ = window.set_focus();
                    } else {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
