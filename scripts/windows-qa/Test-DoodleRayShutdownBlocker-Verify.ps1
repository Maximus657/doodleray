<#
Verification half of the shutdown-blocker repro harness. Run this after
reconnecting to the stand post-reboot (triggered by
Test-DoodleRayShutdownBlocker-Trigger.ps1).

Reports:
  - rebootConfirmed: the machine's LastBootUpTime moved forward past the
    trigger time (proves a real restart happened, not a no-op).
  - shutdownWallClockSeconds: trigger time to new boot time - a rough proxy
    for "how long the shutdown+boot cycle took" (a blocked shutdown that
    needed the HungAppTimeout / a stuck crash dialog to be force-cleared
    shows up as an unusually long gap here versus a clean baseline).
  - powershellCrashEvents: Application-log "Application Error" (1000) or
    "Windows Error Reporting" (1001) events for powershell.exe timestamped
    between the trigger and the new boot time - direct evidence of the
    STATUS_DLL_INIT_FAILED crash this fix targets. Should be empty when the
    fix is present.
  - staleDoodleRayProcesses: any DoodleRay/xray/sing-box process still
    running immediately after boot before anything is relaunched (should be
    none - nothing should auto-start into a broken state).
#>
param(
    [string] $TriggerRecordPath = "C:\DoodleRayQA\shutdown-blocker-trigger.json"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $TriggerRecordPath)) {
    throw "Trigger record not found at $TriggerRecordPath - run Test-DoodleRayShutdownBlocker-Trigger.ps1 first."
}

$trigger = Get-Content -LiteralPath $TriggerRecordPath -Raw | ConvertFrom-Json
$triggerTime = [datetime]::Parse($trigger.triggerTimeUtc).ToUniversalTime()
$bootTimeBefore = [datetime]::Parse($trigger.bootTimeBeforeUtc).ToUniversalTime()
$bootTimeAfter = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime()

$rebootConfirmed = $bootTimeAfter -gt $bootTimeBefore
$shutdownWallClockSeconds = ($bootTimeAfter - $triggerTime).TotalSeconds

$windowEnd = $bootTimeAfter.AddMinutes(2)
$crashEvents = @()
try {
    $crashEvents = Get-WinEvent -FilterHashtable @{
        LogName   = "Application"
        Id        = 1000, 1001
        StartTime = $triggerTime
        EndTime   = $windowEnd
    } -ErrorAction Stop | Where-Object { $_.Message -match "powershell" } |
        Select-Object TimeCreated, Id, @{n = "summary"; e = { ($_.Message -split "`n")[0] } }
} catch [Exception] {
    # Get-WinEvent throws when there are zero matches - that is the
    # passing case, not a script error.
    $crashEvents = @()
}

$staleProcessNames = "DoodleRay", "xray", "sing-box"
$staleProcesses = Get-Process -ErrorAction SilentlyContinue |
    Where-Object { $staleProcessNames -contains $_.ProcessName } |
    Select-Object ProcessName, Id, StartTime

[pscustomobject]@{
    triggerTimeUtc            = $triggerTime.ToString("o")
    bootTimeBeforeUtc         = $bootTimeBefore.ToString("o")
    bootTimeAfterUtc          = $bootTimeAfter.ToString("o")
    rebootConfirmed           = $rebootConfirmed
    shutdownWallClockSeconds  = [Math]::Round($shutdownWallClockSeconds, 1)
    powershellCrashEventCount = $crashEvents.Count
    powershellCrashEvents     = $crashEvents
    staleProcessesAfterBoot   = $staleProcesses
} | ConvertTo-Json -Depth 4
