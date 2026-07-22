param(
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md")
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

# Service-only verification of the v6 unclean-shutdown marker:
# 1. plant a synthetic active-session.marker in the locked runtime dir;
# 2. hard-kill DoodleRayTunnelService (crash simulation: SCM stop would run
#    owned cleanup and correctly clear the marker, so Restart-Service must
#    NOT be used here);
# 3. after restart, expect status JSON to publish previous_unclean_shutdown
#    and the marker file to be consumed;
# 4. expect a clean Restart-Service afterwards to publish no marker again.
$remoteScript = @"
`$ErrorActionPreference = "Stop"
`$ProgressPreference = "SilentlyContinue"

`$serviceExe = "C:\Program Files\DoodleRay\DoodleRayService.exe"
if (-not (Test-Path -LiteralPath `$serviceExe)) {
    throw "DoodleRayService.exe is not installed"
}

function Get-StatusJson {
    `$raw = (& `$serviceExe status 2>&1 | Out-String).Trim()
    return `$raw | ConvertFrom-Json
}

function Wait-ServiceRunning {
    param([int] `$TimeoutSec = 30)
    `$deadline = (Get-Date).AddSeconds(`$TimeoutSec)
    while ((Get-Date) -lt `$deadline) {
        `$service = Get-Service DoodleRayTunnelService -ErrorAction SilentlyContinue
        if (`$service -and `$service.Status -eq "Running") { return `$true }
        Start-Sleep -Seconds 1
    }
    return `$false
}

function Restart-DoodleRayServiceForQa {
    `$service = Get-Service DoodleRayTunnelService -ErrorAction SilentlyContinue
    if (`$service -and `$service.Status -eq "Running") {
        try {
            Stop-Service DoodleRayTunnelService -ErrorAction Stop
        } catch {
            # After a hard-kill/SCM recovery Windows can report StopService
            # failure while the service is already transitioning. Wait first,
            # then fall back to killing the service process only for QA.
        }
        `$deadline = (Get-Date).AddSeconds(20)
        while ((Get-Date) -lt `$deadline) {
            `$service = Get-Service DoodleRayTunnelService -ErrorAction SilentlyContinue
            if (-not `$service -or `$service.Status -eq "Stopped") { break }
            Start-Sleep -Seconds 1
        }
        `$service = Get-Service DoodleRayTunnelService -ErrorAction SilentlyContinue
        if (`$service -and `$service.Status -ne "Stopped") {
            `$pid = Get-CimInstance Win32_Service -Filter "Name = 'DoodleRayTunnelService'" |
                Select-Object -ExpandProperty ProcessId
            if (`$pid) {
                Stop-Process -Id `$pid -Force -ErrorAction SilentlyContinue
                Start-Sleep -Seconds 3
            }
        }
    }
    Start-Service DoodleRayTunnelService
    if (-not (Wait-ServiceRunning 30)) {
        throw "DoodleRayTunnelService did not restart"
    }
    Start-Sleep -Seconds 4
}

`$markerPath = "C:\ProgramData\DoodleRay\runtime\active-session.marker"
New-Item -ItemType Directory -Force -Path "C:\ProgramData\DoodleRay\runtime" | Out-Null
Set-Content -LiteralPath `$markerPath -Value "op_id=qa-synthetic-crash;generation=99;started_at_ms=1751400000000" -Encoding ASCII

`$serviceProcess = Get-CimInstance Win32_Service -Filter "Name = 'DoodleRayTunnelService'" |
    Select-Object -ExpandProperty ProcessId
if (-not `$serviceProcess) {
    throw "DoodleRayTunnelService has no process id (service not running?)"
}
Stop-Process -Id `$serviceProcess -Force

`$recovered = `$false
for (`$i = 0; `$i -lt 12; `$i++) {
    Start-Sleep -Seconds 5
    `$service = Get-Service DoodleRayTunnelService -ErrorAction SilentlyContinue
    if (`$service -and `$service.Status -eq "Running") {
        `$recovered = `$true
        break
    }
}
if (-not `$recovered) {
    Start-Service DoodleRayTunnelService
    Start-Sleep -Seconds 4
}

`$statusAfterCrash = Get-StatusJson
`$markerStillPresent = Test-Path -LiteralPath `$markerPath

Restart-DoodleRayServiceForQa
`$statusAfterCleanRestart = Get-StatusJson

[pscustomobject]@{
    ok = (
        [bool] `$statusAfterCrash.previous_unclean_shutdown -and
        -not `$markerStillPresent -and
        -not `$statusAfterCleanRestart.previous_unclean_shutdown
    )
    scmAutoRecovered = `$recovered
    previousUncleanShutdownAfterCrash = `$statusAfterCrash.previous_unclean_shutdown
    markerConsumed = -not `$markerStillPresent
    previousUncleanShutdownAfterCleanRestart = `$statusAfterCleanRestart.previous_unclean_shutdown
    serviceVersion = `$statusAfterCrash.service_version
    serviceState = `$statusAfterCrash.state
} | ConvertTo-Json -Depth 6
"@

& (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $remoteScript -SecretPath $SecretPath
exit $LASTEXITCODE
