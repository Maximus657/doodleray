param(
    [string]$QaBaseUrl = "http://127.0.0.1:48765",
    [string]$EvidenceDir = "C:\DoodleRayQA\evidence",
    [string]$ControllerIp = $env:DOODLERAY_QA_CONTROLLER_IP
)

$ErrorActionPreference = "Continue"
if ([string]::IsNullOrWhiteSpace($env:DOODLERAY_QA_TOKEN) -and (Test-Path "C:\DoodleRayQA\qa-control-token.txt")) {
    $env:DOODLERAY_QA_TOKEN = (Get-Content "C:\DoodleRayQA\qa-control-token.txt" -Raw).Trim()
}
New-Item -ItemType Directory -Force $EvidenceDir | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$evidence = Join-Path $EvidenceDir "split-routing-dns-$stamp.json"

function Invoke-QaApi([string]$Path) {
    Invoke-RestMethod -Uri "$QaBaseUrl$Path" -Headers @{ "X-DoodleRay-QA-Token" = $env:DOODLERAY_QA_TOKEN } -TimeoutSec 20
}

function Get-QaStatus {
    Invoke-QaApi "/status"
}

function Wait-QaState([string]$State, [int]$TimeoutSec = 30) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    $last = $null
    while ((Get-Date) -lt $deadline) {
        try {
            $last = Get-QaStatus
            if ($last.frontend.status -eq $State -and (
                ($State -eq "connected" -and $last.service.state -eq "connected") -or
                ($State -ne "connected" -and $last.service.state -ne "connected")
            )) {
                return $last
            }
        } catch {}
        Start-Sleep -Milliseconds 500
    }
    return $last
}

function Invoke-CurlText {
    param([string[]]$CurlArgs)
    $output = & curl.exe @CurlArgs 2>&1 | Out-String
    [pscustomobject]@{
        exit = $LASTEXITCODE
        output = $output.Trim()
    }
}

function Get-FirstIpv4([string]$Text) {
    $match = [regex]::Match($Text, "\b(?:\d{1,3}\.){3}\d{1,3}\b")
    if ($match.Success) { $match.Value } else { $null }
}

function Add-ManagementRoute([string]$Ip) {
    $result = [ordered]@{
        attempted = $false
        added = $false
        controllerIp = $Ip
        detail = $null
    }
    if (-not $Ip -or $Ip -notmatch "^\d+\.\d+\.\d+\.\d+$") {
        return [pscustomobject]$result
    }

    $result.attempted = $true
    try {
        $best = Find-NetRoute -RemoteIPAddress $Ip -ErrorAction Stop |
            Where-Object { $_.InterfaceAlias -ne "DoodleRay Tunnel" } |
            Select-Object -First 1
        if (-not $best) {
            $result.detail = "no non-DoodleRay route found"
            return [pscustomobject]$result
        }

        $gateway = $best.NextHop
        if (-not $gateway -or $gateway -eq "0.0.0.0") {
            $gateway = (Get-NetRoute -DestinationPrefix "0.0.0.0/0" -ErrorAction Stop |
                Sort-Object RouteMetric |
                Select-Object -First 1).NextHop
        }

        $routeOutput = route.exe ADD $Ip MASK 255.255.255.255 $gateway METRIC 1 IF $best.InterfaceIndex 2>&1 | Out-String
        $result.added = ($LASTEXITCODE -eq 0 -or $routeOutput -match "already exists")
        $result.detail = "gw=$gateway if=$($best.InterfaceIndex) out=$($routeOutput.Trim())"
    } catch {
        $result.detail = $_.Exception.Message
    }
    [pscustomobject]$result
}

if (-not $ControllerIp -and $env:SSH_CLIENT) {
    $ControllerIp = ($env:SSH_CLIENT -split "\s+")[0]
}

$managementRoute = Add-ManagementRoute $ControllerIp

try { Invoke-QaApi "/disconnect" | Out-Null } catch {}
Wait-QaState "disconnected" 30 | Out-Null

$directProbe = Invoke-CurlText -CurlArgs @("-4", "--connect-timeout", "8", "--max-time", "15", "--noproxy", "*", "-sS", "https://api.ipify.org")
$directIp = Get-FirstIpv4 $directProbe.output

Invoke-QaApi "/clear-custom-routing-rules" | Out-Null
Invoke-QaApi "/add-routing-rule?type=exe&routeAction=direct&value=msedge.exe" | Out-Null
Invoke-QaApi "/add-routing-rule?type=exe&routeAction=direct&value=msedgewebview2.exe" | Out-Null
Invoke-QaApi "/add-routing-rule?type=exe&routeAction=direct&value=msedge_proxy.exe" | Out-Null
Invoke-QaApi "/switch-mode?mode=tun" | Out-Null
$connect = Invoke-QaApi "/connect"
$status = Wait-QaState "connected" 45
if (-not $status) {
    $status = Get-QaStatus
}

$tunProbe = Invoke-CurlText -CurlArgs @("-4", "--connect-timeout", "8", "--max-time", "18", "--noproxy", "*", "-sS", "https://api.ipify.org")
$tunIp = Get-FirstIpv4 $tunProbe.output

$authProbe = Invoke-CurlText -CurlArgs @("-4", "-I", "--connect-timeout", "8", "--max-time", "18", "--noproxy", "*", "https://auth.openai.com")
$authDnsError = ($authProbe.output -match "(?i)could not resolve host|resolving timed out|getaddrinfo|enotfound|name or service not known")

$edgePath = @(
    "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe",
    "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
$edgeRaw = $null
$edgeIp = $null
$edgeExit = $null
if ($edgePath) {
    $edgeProfile = Join-Path $env:TEMP "doodleray-edge-qa-$stamp"
    New-Item -ItemType Directory -Force $edgeProfile | Out-Null
    $edgeRaw = & $edgePath --headless=new --disable-gpu --no-first-run --user-data-dir=$edgeProfile --dump-dom https://api.ipify.org 2>&1 | Out-String
    $edgeExit = $LASTEXITCODE
    $edgeIp = Get-FirstIpv4 $edgeRaw
    Remove-Item $edgeProfile -Recurse -Force -ErrorAction SilentlyContinue
}

$wininet = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" |
    Select-Object ProxyEnable, ProxyServer, ProxyOverride, AutoDetect, AutoConfigURL
$dnsServers = Get-DnsClientServerAddress -ErrorAction SilentlyContinue |
    Select-Object InterfaceAlias, AddressFamily, ServerAddresses
$ipv6Default = Get-NetRoute -AddressFamily IPv6 -DestinationPrefix "::/0" -ErrorAction SilentlyContinue |
    Select-Object InterfaceAlias, InterfaceIndex, NextHop, RouteMetric, State

$authCompact = $authProbe.output -replace "\s+", " "
if ($authCompact.Length -gt 600) {
    $authCompact = $authCompact.Substring(0, 600)
}

$result = [pscustomobject]@{
    ok = (
        $status.frontend.status -eq "connected" -and
        $status.service.state -eq "connected" -and
        $directIp -and
        $tunIp -and
        $tunIp -ne $directIp -and
        $edgeIp -eq $directIp -and
        -not $authDnsError -and
        $status.service.proxy_compat_state -eq "disabled_for_direct_app_exclusions"
    )
    managementRoute = $managementRoute
    directIp = $directIp
    tunIp = $tunIp
    edgeIp = $edgeIp
    edgeExit = $edgeExit
    edgeUsesDirect = ($edgeIp -eq $directIp)
    tunUsesVpn = ($tunIp -and $directIp -and $tunIp -ne $directIp)
    authDnsError = $authDnsError
    authExit = $authProbe.exit
    authOutput = $authCompact
    wininet = $wininet
    dnsServers = $dnsServers
    ipv6Default = $ipv6Default
    status = $status
    connect = $connect
    evidence = $evidence
}

$result | ConvertTo-Json -Depth 10 | Set-Content -Encoding UTF8 $evidence
$result
