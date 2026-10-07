# ADR 0004: The Linux version

Date: 2026-10-07
Status: Accepted (plan). Phases L0 to L5 below track implementation.

## Context

Linux has been a stub: `st-tracker-linux` and `st-enforce-linux` return
`Unsupported`, the IPC transport has no Unix implementation, and nothing
packages or supervises the processes. The stubs' implementation notes (and
older docs) describe freezing blocked apps; that is **not** the design. The
Windows version blocks with an overlay (`BlockOverlay.tsx`, driven by the
session helper) and only terminates an app when the person chooses Quit.
The Linux version mirrors what Windows actually does.

Owner decisions (2026-10-07):

- Desktops: X11, GNOME (Wayland) and KDE Plasma (Wayland).
- Oldest supported: GNOME 45+, Plasma 6, any X11 (Ubuntu 24.04+, Fedora 40+,
  Debian 13, Mint 22, Kubuntu 24.10+).
- Blocking: the overlay, as on Windows. No freezing.
- Distribution: `.deb` and `.rpm`, published through Tether's own apt and
  dnf repositories so updates arrive with system updates.
- GNOME: Tether's extension adds its own top-bar indicator.

## Decision

Same three processes; Linux-native plumbing.

| Part | Windows | Linux |
|---|---|---|
| Agent | Windows Service (SYSTEM) | systemd system service (root, hardened unit); state in `/var/lib/tether` |
| Agent IPC | `\\.\pipe\screentime` | `/run/tether/agent.sock` (socket-activated). Peer = `SO_PEERCRED` pid, then `/proc/<pid>/exe` must be under `/usr/lib/tether/` (root-owned), the Linux `peer_is_trusted`. The PIN gate (`auth.rs`) is unchanged. |
| Session helper | Run key; agent relaunches it (ADR 0003) | systemd user unit bound to `graphical-session.target`, `Restart=always`; the agent still relaunches it when reports stop; XDG autostart fallback |
| Overlay bridge | `\\.\pipe\screentime_overlay_bridge` | `$XDG_RUNTIME_DIR/tether/overlay.sock`, both ends peer-checked |
| Block screen + timer | Tauri overlay; native D2D HUD | Both Tauri windows; kept on top by the compositor connector (below) |
| App identity | exe path / AUMID | `Sandboxed` (Flatpak/Snap id from `/proc/<pid>/cgroup`), else `LinuxDesktop` (Wayland app id / `WM_CLASS` matched to a `.desktop` file), else `LinuxExe`. Proton games: `steam_app_<id>`. |
| Categories | name/path heuristics | `.desktop` `Categories=` first, heuristics second |
| Quit | terminate the process tree | terminate the app's systemd scope (`app-*.scope`), else its process tree |
| Websites | hosts file | `/etc/hosts` (rendering shared with Windows) |
| Family DNS | adapter DNS | systemd-resolved drop-in (`/etc/systemd/resolved.conf.d/`), removed on disable |
| DoH/DoT lockdown | browser policies + firewall | Chrome/Chromium/Firefox managed policies + an `nftables` table blocking 853 |

**Compositor connectors** (only the compositor knows the focused window and
may keep a window above others):

- X11: `x11rb` reads `_NET_ACTIVE_WINDOW`, `_NET_WM_PID`, `WM_CLASS`; idle
  from XScreenSaver; overlay above via `_NET_WM_STATE_ABOVE` + fullscreen.
- GNOME: a Tether Shell extension (ESM, GNOME 45+) exposes the focused
  window over D-Bus, keeps Tether's overlay and timer above everything, and
  adds the top-bar indicator.
- KDE: a KWin script (Plasma 6) reports focus and keeps Tether's windows
  above.
- Idle/lock: Mutter IdleMonitor, `org.freedesktop.ScreenSaver`, logind
  `LockedHint`.
- Anything else: tracking is reported unavailable, with the reason.

Other: the tray is a menu on Linux (no click events), plus the GNOME
indicator; the peek hotkey uses the GlobalShortcuts portal (direct grab on
X11). Anyone with `sudo` can disable Tether, as an administrator can on
Windows; setup says a supervised account should not have `sudo`.

## Phases

- **L0 Foundation**: Unix transport + peer check, Linux paths, systemd units,
  Linux build and CI green; remove the stale freeze design (docs, stub
  notes, unused `freeze`/`thaw`).
- **L1 X11 end to end**: tracking, idle, identity, categories, overlay and
  timer, Quit. Tested on Xvfb with a window manager.
- **L2 GNOME and KDE Wayland**: extension, KWin script, "sign out and back
  in to finish" state.
- **L3 Websites**: hosts, Family DNS, policies, nftables, full restore.
- **L4 Packages**: `.deb`/`.rpm` with install scripts; signed apt and dnf
  repositories.
- **L5 Polish**: tray menu and indicator, Linux wording in Settings and setup,
  Arabic strings.

## Consequences

- Two small non-Rust components (GNOME extension in JS, KWin script) ship in
  the packages and need their own tests and version checks.
- GNOME loads a newly installed extension only after signing out and in.
- No AppImage or Flatpak build: neither can install the root service.
