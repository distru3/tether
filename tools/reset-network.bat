@echo off
:: ============================================================================
:: Screentime Emergency Network Recovery
:: ============================================================================
::
:: Double-click this file (or run from an elevated command prompt) to undo
:: every network change the screentime agent may have made:
::
::   1. Strips all managed entries from the Windows hosts file
::   2. Resets every network adapter's DNS back to DHCP
::   3. Removes all Screentime firewall rules
::   4. Deletes browser DoH enterprise policy registry keys
::
:: This script needs NO internet, NO Rust, NO cargo. It works from Safe Mode,
:: from a USB drive, from anywhere. It auto-requests elevation if needed.
::
:: Safe to run multiple times — every operation is idempotent.
:: ============================================================================

:: --- Auto-elevate if not already running as admin ---
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo [!] Requesting administrator privileges...
    powershell -Command "Start-Process '%~f0' -Verb RunAs"
    exit /b
)

echo.
echo  ======================================================
echo   SCREENTIME EMERGENCY NETWORK RECOVERY
echo  ======================================================
echo.

:: --- 1. Clean the hosts file ---
echo [1/4] Cleaning hosts file...
set "HOSTS=%SystemRoot%\System32\drivers\etc\hosts"
set "HOSTS_BAK=%SystemRoot%\System32\drivers\etc\hosts.screentime-backup"
set "HOSTS_TMP=%SystemRoot%\System32\drivers\etc\hosts.screentime-clean"

if not exist "%HOSTS%" (
    echo      Hosts file not found at %HOSTS% — skipping.
    goto :dns_reset
)

:: Back up before touching anything
copy /Y "%HOSTS%" "%HOSTS_BAK%" >nul 2>&1
echo      Backed up to %HOSTS_BAK%

:: Use fast streaming line-by-line parsing (works instantly even if hosts is massive)
powershell -NoProfile -Command ^
  "try {" ^
  "  $lines = [System.IO.File]::ReadAllLines('%HOSTS%');" ^
  "  $out = New-Object System.Collections.Generic.List[string];" ^
  "  $inside = $false;" ^
  "  $found = $false;" ^
  "  foreach ($line in $lines) {" ^
  "    if ($line.Trim() -eq '# >>> screentime managed block >>>') { $inside = $true; $found = $true; continue; }" ^
  "    if ($line.Trim() -eq '# <<< screentime managed block <<<') { $inside = $false; continue; }" ^
  "    if (-not $inside) { $out.Add($line); }" ^
  "  }" ^
  "  if ($found) {" ^
  "    [System.IO.File]::WriteAllLines('%HOSTS%', $out);" ^
  "    Write-Host '      Removed screentime managed block from hosts file.';" ^
  "  } else {" ^
  "    Write-Host '      No screentime entries found in hosts file (already clean).';" ^
  "  }" ^
  "} catch {" ^
  "  Write-Host '      Failed to clean hosts file: ' $_.Exception.Message;" ^
  "}"

:: --- 2. Reset DNS on all adapters to DHCP ---
:dns_reset
echo.
echo [2/4] Resetting DNS on all network adapters to DHCP...

:: Get all adapter names and reset each one
for /f "tokens=1,* delims=:" %%a in ('netsh interface ipv4 show interfaces ^| findstr /r "connected"') do (
    for /f "tokens=4,*" %%x in ("%%b") do (
        netsh interface ipv4 set dnsservers name="%%x" source=dhcp >nul 2>&1
    )
)

:: Also explicitly reset common adapter names (the loop above may miss some)
for %%A in ("Wi-Fi" "Ethernet" "Ethernet 2" "Ethernet 3" "Local Area Connection") do (
    netsh interface ipv4 set dnsservers name=%%A source=dhcp >nul 2>&1
)
echo      All adapters reset to DHCP DNS.

:: --- 3. Remove Screentime firewall rules ---
echo.
echo [3/4] Removing Screentime firewall rules...
for %%R in (
    "Screentime-Block-DoT-TCP"
    "Screentime-Block-DoT-UDP"
    "Screentime-Block-Outbound-DNS"
    "Screentime-Allow-Upstream-DNS"
) do (
    netsh advfirewall firewall delete rule name=%%R >nul 2>&1
)
echo      Firewall rules removed.

:: --- 4. Remove browser DoH policy registry keys ---
echo.
echo [4/4] Removing browser DoH enterprise policies...

:: Google Chrome
reg delete "HKLM\SOFTWARE\Policies\Google\Chrome" /v DnsOverHttpsMode /f >nul 2>&1

:: Microsoft Edge
reg delete "HKLM\SOFTWARE\Policies\Microsoft\Edge" /v DnsOverHttpsMode /f >nul 2>&1

:: Brave
reg delete "HKLM\SOFTWARE\Policies\BraveSoftware\Brave" /v DnsOverHttpsMode /f >nul 2>&1

:: Firefox
reg delete "HKLM\SOFTWARE\Policies\Mozilla\Firefox\DNSOverHTTPS" /v Enabled /f >nul 2>&1
reg delete "HKLM\SOFTWARE\Policies\Mozilla\Firefox\DNSOverHTTPS" /v Locked /f >nul 2>&1

echo      Browser policies removed.

:: --- Done ---
echo.
echo  ======================================================
echo   RECOVERY COMPLETE
echo  ======================================================
echo.
echo  Your network should now be back to normal.
echo  If websites still don't load, try:
echo    1. Open Settings ^> Network ^> Wi-Fi ^> your network
echo    2. Set DNS to "Automatic (DHCP)"
echo    3. Run: ipconfig /flushdns
echo.
echo  Press any key to flush DNS cache and exit...
pause >nul
ipconfig /flushdns
echo.
echo  Done. You can close this window.
pause

