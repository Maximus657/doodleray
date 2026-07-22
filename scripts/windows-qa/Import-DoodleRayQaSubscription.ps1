param(
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md"),
    [string] $SubscriptionSecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-test-subscription-url.txt"),
    [string] $PscpPath = "C:\Program Files\PuTTY\pscp.exe"
)

# One-time canonical subscription import on a fresh stand, through the
# QA control surface (no CDP form automation, no secret in committed logs).
# The subscription URL travels: local secret file -> pscp -> stand temp file
# -> loopback control-surface query -> frontend import. The temp file is
# deleted afterwards; the URL is never echoed to output.

$ErrorActionPreference = "Stop"

function Get-SecretField {
    param([string] $Text, [string] $Name)
    $match = [regex]::Match($Text, "(?m)^\s*(?:-\s*)?$([regex]::Escape($Name))\s*:\s*(\S+)\s*$")
    if (-not $match.Success) { return $null }
    return $match.Groups[1].Value
}

if (-not (Test-Path -LiteralPath $SubscriptionSecretPath)) {
    throw "Subscription secret file not found: $SubscriptionSecretPath"
}

$secretText = Get-Content -LiteralPath $SecretPath -Raw
$hostName = Get-SecretField $secretText "host"
$userName = Get-SecretField $secretText "login_user"
$password = Get-SecretField $secretText "login_password"
$hostKey = Get-SecretField $secretText "ssh_hostkey"
if (-not $hostKey) { $hostKey = $env:DOODLERAY_PLAY2GO_HOSTKEY }
if (-not $hostName -or -not $userName -or -not $password -or -not $hostKey) {
    throw "Secret file must contain host, login_user, login_password, ssh_hostkey."
}

$remoteSecret = "C:\DoodleRayQA\codex-run\qa-subscription.tmp"
& $PscpPath -batch -hostkey $hostKey -pw $password $SubscriptionSecretPath "${userName}@${hostName}:$remoteSecret" | Out-Null
if ($LASTEXITCODE -ne 0) { throw "failed to upload subscription secret (pscp exit $LASTEXITCODE)" }

$helpers = Get-Content (Join-Path $PSScriptRoot "CdpQaHelpers.ps1") -Raw

$remoteBody = @'
$secretFile = "C:\DoodleRayQA\codex-run\qa-subscription.tmp"
try {
    $subUrl = (Get-Content -LiteralPath $secretFile -Raw).Trim()
    if (-not $subUrl) { throw "empty subscription secret" }

    if (-not (Get-Process DoodleRay -ErrorAction SilentlyContinue)) {
        $launched = Start-AppWithCdp
        Add-Step "launch_app" $launched ""
    }
    $controlReady = $false
    for ($i = 0; $i -lt 15; $i++) {
        if (Test-QaControlAvailable) { $controlReady = $true; break }
        Start-Sleep -Seconds 2
    }
    Add-Step "qa_control_ready" $controlReady ""

    $imported = $false
    if ($controlReady) {
        $encoded = [uri]::EscapeDataString($subUrl)
        Invoke-QaControl "/import-subscription?url=$encoded" | Out-Null
        $deadline = (Get-Date).AddSeconds(45)
        while ((Get-Date) -lt $deadline) {
            $status = Invoke-QaControl "/status" 5
            if ($status.frontend -and
                [int]$status.frontend.subscriptions_count -gt 0 -and
                [int]$status.frontend.servers_count -gt 0) {
                $imported = $true
                break
            }
            Start-Sleep -Seconds 2
        }

        # Fallback for older QA builds that do not publish frontend snapshot:
        # a protected connect can only start when the imported subscription
        # produced at least one server.
        if (-not $imported) {
            Invoke-QaControl "/switch-mode?mode=tun" | Out-Null
            Start-Sleep -Seconds 2
            Invoke-QaControl "/connect" | Out-Null
            $deadline = (Get-Date).AddSeconds(90)
            while ((Get-Date) -lt $deadline) {
                $svc = Get-ServiceStatus
                if ($svc -and @("connecting", "connected") -contains ([string]$svc.state)) { $imported = $true }
                if ($svc -and ([string]$svc.state) -eq "connected") { break }
                Start-Sleep -Seconds 3
            }
            Start-QaDisconnect | Out-Null
            Start-Sleep -Seconds 5
        }
    }
    Add-Step "subscription_present_after_import" $imported "verified via QA frontend snapshot; connect probe fallback for older QA builds"
} finally {
    Remove-Item -LiteralPath $secretFile -Force -ErrorAction SilentlyContinue
}

$allOk = @($steps | Where-Object { -not $_.ok }).Count -eq 0
$result = [pscustomobject]@{ ok = $allOk; steps = $steps }
$result | ConvertTo-Json -Depth 8
if (-not $allOk) { exit 1 }
'@

$remoteScript = $helpers + "`n" + $remoteBody
& (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $remoteScript -SecretPath $SecretPath
exit $LASTEXITCODE
