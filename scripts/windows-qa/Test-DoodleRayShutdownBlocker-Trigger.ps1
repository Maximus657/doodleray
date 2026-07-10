<#
Repro/verification harness for "DoodleRay VPN is preventing shutdown"
together with a powershell.exe Application Error (0xc0000142).

Root cause: the main window's CloseRequested handler swallowed real OS
session-ending requests into "hide to tray" (no way to tell a user click on
X from Windows ending the session), and full_cleanup()'s orphaned-process
sweep spawned a PowerShell child with an unbounded blocking wait - late in
OS shutdown teardown that spawn can itself crash (STATUS_DLL_INIT_FAILED)
and pop a crash dialog that blocks the app's exit until dismissed.

This is the "trigger" half: launches the installed DoodleRay.exe and issues
a real Windows restart, so GetSystemMetrics(SM_SHUTTINGDOWN) genuinely
becomes true for the DoodleRay process, matching production. The
connection will drop when the box restarts - reconnect afterward and run
Test-DoodleRayShutdownBlocker-Verify.ps1.

Reboots the shared QA stand - expected/sanctioned by mandatory QA scenario
9 in docs/windows-pc-qa-play2go.md ("Reboot the server, launch DoodleRay,
and verify...").
#>
param(
    [string] $AppPath = "C:\Program Files\DoodleRay\DoodleRay.exe",
    [int] $RestartDelaySeconds = 15
)

$ErrorActionPreference = "Stop"

Get-Process -Name DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

if (-not (Test-Path -LiteralPath $AppPath)) {
    throw "DoodleRay.exe not found at $AppPath - is it installed?"
}

Start-Process -FilePath $AppPath
Start-Sleep -Seconds 5
$proc = Get-Process -Name DoodleRay -ErrorAction SilentlyContinue
if (-not $proc) {
    throw "DoodleRay.exe did not start from $AppPath"
}

$bootTimeBefore = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime
$triggerTime = Get-Date

$result = [pscustomobject]@{
    doodleRayPid      = $proc.Id
    triggerTimeUtc    = $triggerTime.ToUniversalTime().ToString("o")
    bootTimeBeforeUtc = $bootTimeBefore.ToUniversalTime().ToString("o")
    restartDelaySeconds = $RestartDelaySeconds
}
$result | ConvertTo-Json
$result | ConvertTo-Json | Out-File -FilePath "C:\DoodleRayQA\shutdown-blocker-trigger.json" -Encoding utf8 -Force

shutdown /r /t $RestartDelaySeconds /c "DoodleRay QA shutdown-blocker repro"
