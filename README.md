# Tether

Screen-time budgets, schedules and website blocking for Windows, for yourself
or for someone you look after. Everything stays on the PC: no account, no
cloud.

[![CI](https://github.com/distru3/tether/actions/workflows/ci.yml/badge.svg)](https://github.com/distru3/tether/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## What it does

- **Budgets**: a daily amount of time for an app, a category (Games,
  Social…) or all screen time, with different amounts on some days.
- **Block screen**: when a budget runs out, the app is covered by Tether's
  block screen until the day resets. The app keeps running behind it, so
  nothing is lost. A grown-up can add 15 minutes with the PIN.
- **Schedules**: bedtime or homework time, when only apps you always allow
  can be used.
- **Websites**: block sites in every browser (hosts file) and turn on Family
  DNS (Cloudflare's family filter) for adult and harmful sites.
- **Timer**: a small timer over apps with a budget, chimes at 15, 10, 5 and 1
  minute left, and a peek shortcut (Ctrl+Alt+T).
- **Today and Activity**: time left, a strip of the day, the week against the
  limits.
- **PIN**: loosening anything needs the PIN; raising a limit also waits 24
  hours.
- English and Arabic, light and dark.

What it does **not** do is described honestly in
[`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md).

## Install

Download `Tether_<version>_x64-setup.exe` and run it (administrator rights are
needed for the background service). Setup then asks who Tether is for,
suggests starting budgets, and sets a PIN. Uninstall from Windows Settings;
it offers to delete the data and to restore your previous DNS.

## Develop

Windows 10/11, Rust (pinned in `rust-toolchain.toml`), Visual Studio 2022 C++
build tools, Node 20.

```powershell
npm run dev                       # agent, session helper and UI (./dev.ps1)
./dev.ps1 -BackendOnly            # without the UI
./packaging/build-installer.ps1   # release installer
```

Checks (the same as CI):

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ui; npx tsc --noEmit; npm run build
```

`AGENTS.md` has the rules for working in this repository, including how to run
the checks from Linux by cross-compiling.

## Documentation

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md): how it works.
- [`docs/IPC_CATALOG.md`](docs/IPC_CATALOG.md): every request between the parts.
- [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md): what is and isn't enforced.
- [`docs/UI_SPECIFICATION.md`](docs/UI_SPECIFICATION.md) and
  [`docs/DESIGN_SYSTEM.md`](docs/DESIGN_SYSTEM.md): screens, look and voice.
- [`docs/adr/`](docs/adr/): decisions and their reasons.
- [`packaging/README.md`](packaging/README.md): the installer.

## License

MIT.
