# Packaging Tether (Windows)

Two supported installation stories. Both assume the same runtime shape:
`screentime-agent.exe` runs elevated (as a Windows service), one
`screentime-session.exe` per login session samples the desktop and talks to
the agent over the named pipe `\\.\pipe\screentime`, and the Tauri dashboard
(Tether, `screentime-ui.exe`) is a plain per-user app that issues one-shot pipe commands.

---

## Story A — Manual / elevated install

This is the manual developer flow.

### 1. Build release binaries

From the repository root:

```powershell
cargo build --release -p st-agent -p st-session
```

Outputs land in `target\release\screentime-agent.exe` and `target\release\screentime-session.exe`.

### 2. Register the agent as a Windows service (elevation required)

```powershell
$fso = New-Object -ComObject Scripting.FileSystemObject
$short = $fso.GetFile('target\release\screentime-agent.exe').ShortPath

sc.exe create ScreentimeAgent type= own start= auto binPath= $short DisplayName= "Screentime Agent"
Set-ItemProperty HKLM:\SYSTEM\CurrentControlSet\Services\ScreentimeAgent -Name ImagePath -Value "$short --service" -Type ExpandString
sc.exe start ScreentimeAgent
```

### 3. Register the session helper for logon autostart (no elevation)

The helper writes itself into `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`:

```powershell
& "target\release\screentime-session.exe" --autostart on
```

---

## Story B — Bundled NSIS installer (Official)

One `Tether_<version>_x64-setup.exe` that installs all three binaries, registers
the service, adds the session helper to startup and starts tracking. Tauri 2
generates it from a customized NSIS template.

### Building the installer

On Windows, from the repository root:

```powershell
./packaging/build-installer.ps1
```

It runs `cargo build --release -p st-agent -p st-session`, copies both
binaries into `ui/src-tauri/bin/` (bundled through `tauri.conf.json >
bundle > resources`), then `npx tauri build --bundles nsis`. The installer
lands in `target\release\bundle\nsis\`.

### What the installer looks like

Plum and orange like the app (`docs/DESIGN_SYSTEM.md`):

- **Welcome / finish**: a plum sidebar with the Tether wordmark, the icon
  inside an orange ring and a day strip; titles in Unbounded, text in Rubik.
- **Other pages**: white header with the app icon, page titles in Unbounded
  and a muted subtitle.
- **Wording**: says what gets installed (the app and a background service that
  keeps limits working after a restart) and what happens next (setup asks who
  it's for, budgets and a PIN). English and Arabic, chosen from the Windows
  display language.
- **Uninstaller**: "Remove Tether", with "Also delete usage history and
  settings" and, only while Family DNS is on, "Turn off Family DNS and
  restore the previous network DNS".

The fonts are embedded and registered privately for the installer process
(`AddFontResourceEx` with `FR_PRIVATE`); nothing is installed system-wide.

### Files (`ui/src-tauri/installer/`)

| File | Role |
|---|---|
| `installer.nsi` | Tauri 2.9 NSIS template (`bundle > windows > nsis > template`) plus Tether's page text, the uninstall options and the font loading calls. Keep it in step with the Tauri version when upgrading. |
| `hooks.nsh` | `installerHooks`: stop/start the service, HKLM Run entry, RivaTuner profiles, start tracking as the signed-in user, restore DNS on uninstall. Defines `TETHER_INSTALLER_DIR`, which the template uses to find the files below. |
| `style.nsh` | Colours, fonts, header/sidebar settings, the title-font and layout tweaks for the welcome and finish pages. |
| `strings.nsh` | Every Tether string, English and Arabic (`LangString`). Add a language here and in `tauri.conf.json > ... > nsis > languages`. |
| `fonts/` | Rubik Regular/SemiBold (Latin + Arabic) and Unbounded SemiBold (Latin), subset, with their OFL licences. |
| `art/` | `sidebar.bmp` and `header.bmp`, drawn by `art/make_art.py` (Pillow) from `icons/icon.png` and the design tokens. Re-run it after changing the icon or palette. |

### How the hooks work

- **Pre-install**: stops `ScreentimeAgent` and closes the helper and the app so
  files are not locked.
- **Post-install**: `screentime-agent.exe --install` (when the service is
  already registered, from an earlier version or another folder, it is
  re-pointed at this install's agent instead of failing) and `sc start` (if the
  service does not start, the page says so and asks for a restart instead of
  claiming it runs); one HKLM
  `Run` value `ScreentimeSession` (every user, at logon); RivaTuner
  "do not hook" profiles when RTSS is present; records the installer language
  for the uninstaller; starts the session helper as the signed-in user
  (`nsis_tauri_utils::RunAsUser`), so tracking begins without a sign-out.
- **Pre-uninstall**: stops the service, restores DNS if chosen, closes the
  helper and the app, removes the Run values, then
  `screentime-agent.exe --uninstall`.

### Previewing the installer without Windows

The template can be rendered and compiled with Linux `makensis` (package
`nsis`) and run under 32-bit Wine on a virtual display, which is how the
styling was checked. Wine draws Arabic punctuation at the wrong end of
right-to-left labels (NSIS's own Arabic strings too); check Arabic on Windows.
