# Packaging Screentime (Windows)

Two supported installation stories. Both assume the same runtime shape:
`screentime-agent.exe` runs elevated (as a Windows service), one
`screentime-session.exe` per login session samples the desktop and talks to
the agent over the named pipe `\\.\pipe\screentime`, and the Tauri dashboard
(`Screentime`) is a plain per-user app that issues one-shot pipe commands.

---

## Story A — Manual / elevated install (works today)

Everything below is scripted in [`install.ps1`](./install.ps1); this section
documents what it does so you can also do it by hand.

### 1. Build release binaries

From the repository root:

```powershell
cargo build --release -p st-agent -p st-session
```

Outputs land in `target\release\screentime-agent.exe` and
`target\release\screentime-session.exe`.

> Rebuilding while a binary is running fails with "Access is denied
> (os error 5)". Stop `screentime-agent` / `screentime-session` first.

### 2. Copy executables to the install directory

Default layout used by this repo's tooling:

```
C:\Program Files\Screentime\
    screentime-agent.exe      (runs as LocalSystem, service)
    screentime-session.exe    (runs as the logged-in user)
```

```powershell
Copy-Item target\release\screentime-agent.exe   "C:\Program Files\Screentime\"
Copy-Item target\release\screentime-session.exe "C:\Program Files\Screentime\"
```

### 3. Register the agent as a Windows service (elevation required)

`sc.exe` syntax is unforgiving: there must be a **space after each `=`**, and
the binary path must be quoted because of `Program Files`. The `--service`
flag is **mandatory and rides outside the quotes** — without it the SCM's
launch falls into console mode (a daemon that never talks to the service
controller) and every start times out with System-log event 7009:

```powershell
# PowerShell 5.1 re-quotes arguments containing embedded quotes + spaces, so
# hand this line to cmd instead (or just use .\packaging\install.ps1):
cmd /D /C 'sc.exe create ScreentimeAgent type= own start= auto binPath= "C:\Program Files\Screentime\screentime-agent.exe" --service DisplayName= "Screentime Agent"'
sc.exe failure ScreentimeAgent reset= 86400 actions= restart/60000/restart/60000/restart/60000
sc.exe start ScreentimeAgent
```

Caveats worth knowing before you deviate:

- **Elevation**: `sc.exe create/config/failure` require an administrator
  token; from a non-elevated shell they fail with access-denied.
- The agent's *service mode* (running under LocalSystem, SDDL on the pipe,
  etc.) is owned by the service team; this registration only points SCM at
  the executable and restarts it on failure. If the agent requires extra
  arguments or environment (`SCREENTIME_DATA_DIR`) those belong in
  `binPath` / service registry config here later.
- Updating an existing service: `sc.exe stop ScreentimeAgent`, then
  `sc.exe config ScreentimeAgent binPath= "..."`, then `sc.exe start`.
- To remove: stop the helper(s) first, `sc.exe delete ScreentimeAgent`.

### 4. Register the session helper for logon autostart (no elevation)

The helper writes itself into `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
under the value name `ScreentimeSession` — per-user, no elevation, survives
reboots, cleaned up with the profile:

```powershell
& "C:\Program Files\Screentime\screentime-session.exe" --autostart on
```

Run this once per user account (or let users do it). `--autostart status`
prints the registered path, `--autostart off` removes the entry idempotently.
Both work even while a helper instance is already running; the CLI never
trips the single-instance guard. Exit code `0` = success, `1` = failure with
a one-line stderr reason, and `2` remains reserved for the duplicate-startup
refusal of the sampling loop.

> Note: `--autostart on` records whatever path the binary was launched from,
> so always run it against the *installed* copy, not a dev-tree build.

### 5. Where logs land

| Binary | Location |
| --- | --- |
| `screentime-agent` | `<data-dir>\logs\screentime-agent.log.<date>` where `<data-dir>` is `%ProgramData%\screentime` unless `SCREENTIME_DATA_DIR` overrides it |
| `screentime-session` | `%LOCALAPPDATA%\screentime\logs\screentime-session.log.<date>` (or `<SCREENTIME_DATA_DIR>\logs` when that variable is set for the user's environment) |

Logs roll daily and the newest 14 files are kept.

### 6. Verify the installation

```powershell
# Agent is up and serving
Get-Service ScreentimeAgent
Test-Path "\\.\pipe\screentime"

# Session helper started at logon and connected
Get-Process screentime-session
Get-Content "$env:LOCALAPPDATA\screentime\logs\screentime-session.log.*" -Tail 20
# expect: "connected to agent", periodic ingest acks

# Autostart entry exists
reg.exe query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v ScreentimeSession
```

---

## Story B — Bundled NSIS installer (scaffolding; NOT complete)

### What exists today

- `ui/src-tauri/tauri.conf.json` targets NSIS (`targets: ["nsis"]`) with the
  existing icon list — valid for plain `npm run tauri build`, which yields an
  installer for the dashboard alone.
- `ui/src-tauri/tauri.bundle.conf.json` is a **config overlay** carrying the
  two daemon executables from `target/release/` into `bin/` inside the install
  directory.
- `packaging/bundle.ps1` ties it together: builds the daemons in release mode,
  then runs `tauri build --config src-tauri/tauri.bundle.conf.json`.

**Why an overlay instead of `resources` in the base config:** tauri-build
validates resource paths on *every* cargo invocation of `ui/src-tauri`
(`cargo check`, clippy, tests all execute its build script). Referencing
`target/release/*.exe` there breaks fresh clones and CI until a release build
happens to exist. The overlay applies the paths only at bundling time, when
`bundle.ps1` has just built them.

Run it:

```powershell
packaging\bundle.ps1
```

### What the bundle deliberately does NOT do yet

- **TODO: postinstall hook.** NSIS must, after install: register
  `ScreentimeAgent` as a service (Story A step 3) and run
  `screentime-session.exe --autostart on` for the installing user (step 4).
  Without it, the bundle ships inert binaries.
- **TODO: preuninstall hook.** Stop/remove the service, delete the HKCU Run
  value, terminate running helpers.
- **TODO: code signing.** An unprivileged sampler that draws overlays and a
  service binary both live happier signed; nothing signs anything today.
- **TODO: full icon set.** The config lists one PNG; NSIS wants `.ico`
  variants (`bundle.icon` accepts them when generated).
- **Honest limitation:** an end-to-end `bundle.ps1` run producing an installer
  has not been exercised in CI; the overlay merge syntax follows Tauri v2's
  documented `--config` behaviour but the produced `.exe` setup should be
  smoke-tested before trusting it. Treat Story B as scaffolding until the
  TODOs above land.
