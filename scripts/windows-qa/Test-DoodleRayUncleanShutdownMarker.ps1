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

Restart-Service DoodleRayTunnelService -Force
Start-Sleep -Seconds 4
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
