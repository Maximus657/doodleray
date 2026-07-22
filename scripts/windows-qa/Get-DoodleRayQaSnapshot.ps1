param(
    [ValidateSet("proxy", "tun", "cleanup", "generic")]
    [string] $Mode = "generic"
)

$ErrorActionPreference = "Continue"
$ProgressPreference = "SilentlyContinue"

function Invoke-CurlText {
    param([string[]] $ArgsList)

    try {
        $output = & curl.exe @ArgsList 2>&1
        return ($output | Out-String).Trim()
    } catch {
        return $_.Exception.Message
    }
}

function ConvertTo-IpPrefix {
    param([string] $Text)

    if (-not $Text) {
        return $null
    }

    $match = [regex]::Match($Text, "\b\d{1,3}(?:\.\d{1,3}){3}\b")
    if ($match.Success) {
        $parts = $match.Value.Split(".")
        return "$($parts[0]).$($parts[1]).x.x"
    }

    if ($Text.Length -le 120) {
        return $Text
    }
    return $Text.Substring(0, 120)
}

function Get-HttpProxyPort {
    param($InternetSettings)

    $proxyServer = [string] $InternetSettings.ProxyServer
    if (-not $proxyServer) {
        return $null
    }

    if ($proxyServer -match "(?i)(?:^|;)https?=127\.0\.0\.1:(\d+)(?:;|$)") {
        return [int] $Matches[1]
    }

    if ($proxyServer -match "127\.0\.0\.1:(\d+)") {
        return [int] $Matches[1]
    }

    return $null
}

function Get-ServiceStatus {
    $serviceExe = "C:\Program Files\DoodleRay\DoodleRayService.exe"
    if (-not (Test-Path -LiteralPath $serviceExe)) {
        return @{ raw = "missing service exe"; parsed = $null }
    }

    $raw = (& $serviceExe status 2>&1 | Out-String).Trim()
    try {
        return @{ raw = $raw; parsed = ($raw | ConvertFrom-Json) }
    } catch {
        return @{ raw = $raw; parsed = $null }
    }
}

function Get-DnsProbe {
    try {
        $dns = Resolve-DnsName captive.apple.com -Type A -ErrorAction Stop |
            Where-Object { $_.IPAddress } |
            Select-Object -First 3 -ExpandProperty IPAddress
        return @{ ok = $true; addresses = @($dns | ForEach-Object { ConvertTo-IpPrefix $_ }) }
    } catch {
        return @{ ok = $false; error = $_.Exception.Message }
    }
}

$inet = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" -ErrorAction SilentlyContinue
$httpPort = Get-HttpProxyPort -InternetSettings $inet
$service = Get-ServiceStatus

$loopbackListeners = Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
    Where-Object {
        $_.LocalAddress -in @("127.0.0.1", "::1") -and
        ($_.OwningProcess -in @(
            (Get-Process xray, sing-box, DoodleRay -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
        ))
    } |
    Sort-Object LocalPort |
    Select-Object LocalAddress, LocalPort, OwningProcess

$processes = Get-Process DoodleRay, DoodleRayService, xray, sing-box -ErrorAction SilentlyContinue |
    Sort-Object ProcessName, Id |
    Select-Object ProcessName, Id, Path

$routes = Get-NetRoute -AddressFamily IPv4 -ErrorAction SilentlyContinue |
    Where-Object {
        $_.DestinationPrefix -in @("0.0.0.0/0", "0.0.0.0/1", "128.0.0.0/1") -or
        $_.InterfaceAlias -like "*DoodleRay*"
    } |
    Sort-Object DestinationPrefix, RouteMetric |
    Select-Object DestinationPrefix, InterfaceAlias, InterfaceIndex, NextHop, RouteMetric

$interfaces = Get-NetIPInterface -AddressFamily IPv4 -ErrorAction SilentlyContinue |
    Where-Object { $_.InterfaceAlias -like "*DoodleRay*" -or $_.InterfaceAlias -like "*Ethernet*" -or $_.InterfaceAlias -like "*Wi-Fi*" } |
    Sort-Object InterfaceMetric |
    Select-Object InterfaceAlias, InterfaceIndex, InterfaceMetric, ConnectionState, NlMtu

$directIp = Invoke-CurlText @("--max-time", "25", "-sS", "https://api.ipify.org")
$directApple = Invoke-CurlText @("--max-time", "25", "-sS", "-o", "NUL", "-w", "%{http_code} %{size_download}", "https://captive.apple.com/hotspot-detect.html")
$directTwoIp = Invoke-CurlText @("--max-time", "25", "-sS", "https://2ip.ru")

$proxyProbes = $null
if ($httpPort) {
    $proxy = "http://127.0.0.1:$httpPort"
    $proxyProbes = [pscustomobject]@{
        ipPrefix = ConvertTo-IpPrefix (Invoke-CurlText @("--max-time", "35", "-sS", "--proxy", $proxy, "https://api.ipify.org"))
        apple = Invoke-CurlText @("--max-time", "35", "-sS", "-o", "NUL", "-w", "%{http_code} %{size_download}", "--proxy", $proxy, "https://captive.apple.com/hotspot-detect.html")
        twoIpObservedPrefix = ConvertTo-IpPrefix (Invoke-CurlText @("--max-time", "35", "-sS", "--proxy", $proxy, "https://2ip.ru"))
        telegramHttpCode = Invoke-CurlText @("--max-time", "35", "-sS", "-o", "NUL", "-w", "%{http_code}", "--proxy", $proxy, "https://api.telegram.org")
        discordGatewayHttpCode = Invoke-CurlText @("--max-time", "35", "-sS", "-o", "NUL", "-w", "%{http_code}", "--proxy", $proxy, "https://discord.com/api/v10/gateway")
    }
}

[pscustomobject]@{
    mode = $Mode
    timestampUtc = (Get-Date).ToUniversalTime().ToString("o")
    winInet = [pscustomobject]@{
        proxyEnable = $inet.ProxyEnable
        proxyServer = $inet.ProxyServer
        proxyOverride = $inet.ProxyOverride
        httpPort = $httpPort
    }
    service = $service.parsed
    serviceRawPresent = [bool] $service.raw
    processes = $processes
    loopbackListeners = $loopbackListeners
    routes = $routes
    interfaces = $interfaces
    dns = Get-DnsProbe
    directProbes = [pscustomobject]@{
        ipPrefix = ConvertTo-IpPrefix $directIp
        apple = $directApple
        twoIpObservedPrefix = ConvertTo-IpPrefix $directTwoIp
    }
    proxyProbes = $proxyProbes
} | ConvertTo-Json -Depth 12

