<#
.SYNOPSIS
    Builds the Tether installer (Tether_<version>_x64-setup.exe) on Windows.

.DESCRIPTION
    1. Builds the agent and session helper in release mode.
    2. Copies them into ui/src-tauri/bin, which tauri.conf.json bundles.
    3. Runs `tauri build --bundles nsis`, which builds the dashboard and wraps
       everything in the styled NSIS installer (ui/src-tauri/installer/).

    Needs the Rust MSVC toolchain and Node.js. Tauri downloads NSIS itself on
    first use.

.EXAMPLE
    ./packaging/build-installer.ps1
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    Write-Host '== Building the agent and session helper (release)'
    cargo build --release -p st-agent -p st-session
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

    $bin = Join-Path $root 'ui\src-tauri\bin'
    New-Item -ItemType Directory -Force $bin | Out-Null
    Copy-Item 'target\release\screentime-agent.exe', 'target\release\screentime-session.exe' $bin -Force

    Push-Location 'ui'
    try {
        if (-not (Test-Path 'node_modules')) {
            Write-Host '== Installing UI dependencies'
            npm ci
            if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
        }
        Write-Host '== Building the app and the installer'
        npx tauri build --bundles nsis
        if ($LASTEXITCODE -ne 0) { throw 'tauri build failed' }
    }
    finally { Pop-Location }

    $setup = Get-ChildItem 'target\release\bundle\nsis\*-setup.exe' |
        Sort-Object LastWriteTime | Select-Object -Last 1
    Write-Host "== Installer: $($setup.FullName)"
}
finally { Pop-Location }
