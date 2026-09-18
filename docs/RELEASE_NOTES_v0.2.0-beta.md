# Tether (v0.2.0-beta)

> **A high-performance, private-by-default screen-time analytics workspace, limit enforcer, and web filter for Windows.**
>
> Understand where your attention goes, establish healthy digital boundaries, and enforce hard limits on distracting apps and websites—with **100% local storage**, **zero cloud telemetry**, **no subscriptions**, and **zero gaming performance loss**.

---

## What is Tether?

Most screen-time tools either upload your browsing history to cloud servers for monthly subscriptions, or rely on simple browser extensions that can be bypassed by opening an incognito window.

**Tether** runs locally on your Windows PC as a system-level tool:
- **100% Private & Offline**: All your activity history, limits, and rules stay strictly on your computer in a local SQLite database (`%ProgramData%\screentime`). Nothing ever leaves your machine.
- **Whole-System Coverage**: Tracks any Windows application—browsers, code editors, 3D games, office suites, Discord, and background media players.
- **Fail-Closed Protection**: Active limits persist across system reboots and app restarts. There are no sneaky restart loopholes.
- **Process Freezing over Termination**: When an app budget runs out, Tether pauses the application in place rather than terminating it, keeping unsaved documents, tabs, and game progress safe.
- **Anti-Impulse Friction**: Tightening limits or turning them on happens immediately. Relaxing or increasing a limit requires a 24-hour cooldown queue, preventing impulsive overrides.

---

## Key Features

### 📊 Real-Time Screen Time Analytics
- **1 Hz Precision Sampling**: Live-tracks your active foreground window and automatically pauses when you step away (`GetLastInputInfo` idle detection).
- **Interactive Daily Timeline**: Visual 24-hour timeline bar detailing exactly when apps were used throughout the day, complete with category color swatches and active limit markers.
- **Automatic & Custom Categorization**: Organize apps into *Productivity*, *Development*, *Communication*, *Design*, *Entertainment*, *Gaming*, and *Social*.
- **7-Day Trend Insights**: Compare today’s screen time against your weekly average with variance badges and daily breakdown charts.

### ⏱️ Daily Limits & Smart Enforcement
- **App & Category Budgets**: Assign daily screen time quotas to specific programs (`Discord.exe`, `Chrome.exe`) or whole categories (*Gaming*, *Social*).
- **Weekday vs. Weekend Schedules**: Grant different time budgets depending on the day of the week (e.g. 1 hour on weekdays, 3 hours on weekends).
- **Argon2id Vault & Recovery**: Lock limit settings and emergency overrides behind a master PIN. A cryptographic single-use recovery code is provided during setup in case you forget the PIN.
- **Safe Suspension Overlay**: When time runs out, a sleek obsidian card appears over the application. You can quit the app, unlock an extension with your PIN, or wait until tomorrow's reset.

### 🎮 Gaming HUD & Universal Timer Peek
- **Zero FPS Drop in 3D Games**: Built using DirectComposition and DirectX 11 hardware flip swapchains (`DXGI_SWAP_EFFECT_FLIP_DISCARD`). Keeps 3D games in 100% Hardware Independent Flip with zero composition lag and uncompromised VRR (FreeSync / G-Sync).
- **Anti-Hooking Protection (RTSS / MSI Afterburner)**: Official `RTSSHooksCompatibility` PE export and profile exclusions prevent RivaTuner and third-party gaming hooks from latching onto or obscuring the HUD pill with OSD statistics.
- **Buttery Smooth 120 FPS Entrance Animation**: Floating timer slides into view with a silky 120 Hz cubic ease-out deceleration curve modeled after the native Windows 11 volume flyout, using 1 ms system timer precision and jitter-free DWM positioning.
- **Universal Peek Shortcut (`Ctrl + Alt + T`)**: Press `Ctrl + Alt + T` anywhere—over full-screen games, video streams, or desktop apps—to pop up your remaining daily budget for 4 seconds without interrupting what you are doing. The hotkey can be remapped in Settings.
- **Harmonic Audio Milestone Chimes**: Elegant, non-intrusive audio chimes sound at 15m, 10m, 5m, and 1m thresholds so you are never caught off guard.

### 🚫 Web Filtering & Security Protection
- **Native Hosts File Enforcement**: Instantly blocks distracting or harmful domains across all browsers and apps with zero latency and zero background proxy overhead by routing blocked hostnames to `0.0.0.0`.
- **Cloudflare Family DNS**: One-click system-wide adult content and malware protection configured cleanly on your network adapters (`1.1.1.3` / `1.0.0.3`), with full automatic backup and restoration to your original DNS settings.
- **DoH / DoT Policy Hardening**: Windows enterprise policy keys prevent browsers from silently bypassing system filtering via encrypted DNS.
- **Custom Blocklists & Domain Rules**: Easily subscribe to curated blocklists or add custom domain rules (including wildcard subdomains).

### 🎨 4 Distinct Solid Modern Themes
- **Lightweight Desktop Shell**: Built with Tauri 2 and React 18, consuming less than 40 MB of RAM.
- **Obsidian Onyx**: Pure pitch carbon base (`#090A0F`), warm carbon panels (`#12141A`), and radiant amber gold accents (`#F59E0B`).
- **Warm Sandstone**: Non-glare warm sand / parchment canvas (`#F5F2EB`), pure alabaster cards, artisan terracotta accents (`#C2410C`), and deep espresso typography (`#1C1917`).
- **Classic Dark & Classic Light**: Cool neutral slate + electric indigo, and pure crisp white + royal blue.
- **Zero Glassmorphism**: 100% solid, opaque tactile surfaces with crisp 1px border contrast.
- **Synchronized Overlays**: Overlays dynamically match your active theme preference in real-time.
- **English & Arabic Localization**: Full bilingual support with right-to-left (RTL) layout switching.

---

## How to Install and Get Started

### 1. Download & Install
1. Download **`Tether_0.2.0-beta_x64-setup.exe`** from the **Assets** section below.
2. Run the setup executable. Windows will ask for Administrator elevation to configure the background tracking service.
3. Once installation completes, Tether launches automatically, and the helper is set to start quietly on login.

### 2. View Your Usage
- Launch **Tether** from the Start Menu or Desktop.
- The **Overview** tab displays today’s total active time, your top used applications, and the 24-hour visual timeline.

### 3. Add Your First Daily Limit
1. Navigate to the **Limits** tab on the left sidebar.
2. Click **+ Add Limit**.
3. Choose whether you want to limit a specific application (e.g. `chrome.exe`) or an entire category (e.g. `Gaming`).
4. Set your daily quota in minutes or hours.
5. *(Optional)* Expand **Weekday Schedules** to configure different limits for individual days.
6. Click **Save Limit**.

### 4. Check Remaining Time Anytime (`Ctrl + Alt + T`)
- Press **`Ctrl + Alt + T`** anywhere on your computer.
- A floating capsule will briefly appear in the top-right corner of your screen showing your remaining allowance, then dismiss automatically after 4 seconds.

### 5. Secure Your Settings (Optional)
- Open **Settings** → **Security & Vault**.
- Click **Set Master PIN** to prevent modifying or disabling limits during weak moments.
- Be sure to write down your **Recovery Code**—it is the only way to reset a forgotten PIN!

### 6. Clean Uninstallation
- If you ever wish to uninstall, run the uninstaller from Windows **Installed Apps**.
- The uninstaller includes dedicated checkboxes to **Delete all application data and history** and **Restore network DNS settings**, ensuring your computer is left in its exact original state.

---

## What’s New in v0.2.0-beta

- **Autostart & Instant Tracking on Install**: `screentime-session.exe` registers in Windows HKLM and HKCU Run keys and launches immediately post-install so tracking begins right away without requiring a system reboot.
- **Pre-Install Process & Service Clean Lock**: The installer cleanly stops background services and terminates running helper instances before copying binaries, preventing file-in-use errors during upgrades.
- **Discovered Apps Path Canonicalization & Basename Matching**: Resolved 8.3 short paths (`PROGRA~1`) and linked running executables to existing discovered shortcut rows, ensuring user limits apply reliably without duplicate database entries.
- **Solid Modern Design (Zero Glassmorphism)**: Completely eliminated all glassmorphic blur and muddy purple colors in favor of crisp, solid **Obsidian Dark** and **Titanium Light** themes with high-contrast borders.
- **Draggable Timer HUD**: Freely move the timer overlay across the screen without stealing focus from games or typing applications (`WS_EX_NOACTIVATE`). Your custom position is saved to `%LOCALAPPDATA%\screentime\hud_pos.json` and remembered across restarts.
- **Windows Volume Flyout-Style Entrance Animation**: The timer HUD smoothly slides down from the top (or slides up from the bottom) using a 240 ms cubic deceleration curve when applications become active.
- **Theme-Synchronized Overlays**: Both the hardware MPO HUD and the React secondary block overlay dynamically adapt to the active theme preference.
- **Solid Tactile Block Overlay**: Redesigned block screen with solid card surfaces, tactile numeric keypad, and crisp 1px borders.
- **Hardware Multiplane Overlay (MPO) Gaming HUD**: DirectComposition flip swapchain eliminates DWM composition drops in full-screen games, keeping FreeSync/G-Sync and driver frame limiters (Radeon Chill / Nvidia Max Frame Rate) active.
- **Universal Timer Peek (`Ctrl + Alt + T`)**: On-demand 4-second peek mode works even with the continuous floating overlay disabled.
- **Harmonic Audio Alerts**: High-fidelity sound chimes at 15m, 10m, 5m, and 1m intervals.
- **Automatic DHCP DNS Restoration**: Tether captures whether network adapters were on DHCP (Automatic) and cleanly restores true automatic DHCP mode on disable or uninstall.
- **Uninstaller DNS & Data Restore**: Added interactive checkboxes in the uninstaller to automatically restore previous DNS configurations and purge all local data.
