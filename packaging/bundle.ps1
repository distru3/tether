<#
.SYNOPSIS
    Bundled NSIS install story (Story B in packaging/README.md): builds the
    daemon exes in release mode, then runs the Tauri bundler with a config
    overlay that carries the daemons into the installer.

.DESCRIPTION
    Why an overlay instead of putting "resources" straight into tauri.conf.json:
    tauri-build validates resource paths on EVERY cargo invocation of
    ui/src-tauri (clippy, check, test all run its build script), so referencing
    target\release\*.exe there breaks every fresh clone and CI until a release
    build happens to exist. The overlay is applied only here, at bundling time,
    when the files are guaranteed present.

    PowerShell 5.1 compatible. Produces NSIS setup output under
    ui/src-tauri/target/release/bundle/nsis/.

.NOTES
    The generated installer still does NOT register the service or autostart;
    see packaging/README.md for the TODO list (postinstall/preuninstall/signing).
#>
$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2

function Fail([string]$Message) {
    Write-Error $Message -ErrorAction Continue
    exit 1
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Fail "cargo was not found on PATH; install the Rust toolchain first."
}

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Overlay = Join-Path $RepoRoot "ui\src-tauri\tauri.bundle.conf.json"

if (-not (Test-Path -LiteralPath $Overlay)) {
    Fail "Bundle overlay config missing: $Overlay"
}

Push-Location $RepoRoot
try {
    Write-Host "Building release daemons..."
    cargo build --release -p st-agent -p st-session
    if ($LASTEXITCODE -ne 0) {
        Fail "cargo build --release failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}

Write-Host "Bundling NSIS installer (with daemon resources via overlay)..."
Push-Location (Join-Path $RepoRoot "ui")
try {
    # "--" hands the extra flag through npm to tauri; the overlay deep-merges
    # over tauri.conf.json, adding only the bundle.resources map.
    npm run tauri build -- --config src-tauri/tauri.bundle.conf.json
    if ($LASTEXITCODE -ne 0) {
        Fail "tauri build failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}

Write-Host ""
Write-Host "== Bundle complete. Installer output: ui\src-tauri\target\release\bundle\nsis\ =="
Write-Host "Reminder: the installer ships inert binaries; service registration +"
Write-Host "autostart postinstall are still TODO (see packaging/README.md)."
