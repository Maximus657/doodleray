param(
    [ValidateSet("baseline", "proxy", "tun", "cleanup", "generic")]
    [string] $Mode = "generic"
)

$ErrorActionPreference = "Continue"
$ProgressPreference = "SilentlyContinue"

function ConvertTo-IpPrefix {
    param([string] $Text)

    if (-not $Text) {
        return $null
    }

    $v4 = [regex]::Match($Text, "\b\d{1,3}(?:\.\d{1,3}){3}\b")
    if ($v4.Success) {
        $parts = $v4.Value.Split(".")
        return "$($parts[0]).$($parts[1]).x.x"
    }

    $v6 = [regex]::Match($Text, "\b[0-9a-fA-F]{1,4}(?::[0-9a-fA-F]{0,4}){2,}\b")
    if ($v6.Success) {
        $parts = $v6.Value.Split(":") | Where-Object { $_ }
        if ($parts.Count -ge 2) {
            return "$($parts[0]):$($parts[1])::x"
        }
        return "ipv6-redacted"
    }

    if ($Text.Length -le 120) {
        return $Text
    }
    return $Text.Substring(0, 120)
}

function Invoke-CurlText {
    param([string[]] $ArgsList)

    try {
        $output = & curl.exe @ArgsList 2>&1 | ForEach-Object { $_.ToString() }
        return ($output -join "`n").Trim()
    } catch {
        return $_.Exception.Message
    }
}

function Invoke-CurlMetric {
    param(
        [string] $Url,
        [string] $Proxy = "",
        [int] $MaxTime = 15
    )

    $args = @(
        "--max-time", [string] $MaxTime,
        "--silent",
        "--show-error",
        "--output", "NUL",
        "--write-out", "%{http_code} %{size_download} %{time_total} %{remote_ip}",
        $Url
    )
    if ($Proxy) {
        $args = @(
            "--max-time", [string] $MaxTime,
            "--silent",
            "--show-error",
            "--output", "NUL",
            "--write-out", "%{http_code} %{size_download} %{time_total} %{remote_ip}",
            "--proxy", $Proxy,
            $Url
        )
    }

    $raw = Invoke-CurlText $args
    $metricLine = @($raw -split "`n" | Where-Object { $_ -match "^\d{3}\s+\d+\s+" } | Select-Object -Last 1)
    if (-not $metricLine) {
        $metricLine = $raw
    }
    $parts = ([string] $metricLine) -split "\s+"
    [pscustomobject]@{
        status = $parts[0]
        bytes = $parts[1]
        seconds = $parts[2]
        remotePrefix = ConvertTo-IpPrefix $parts[3]
        raw = if ($raw.Length -le 160) { $raw } else { $raw.Substring(0, 160) }
    }
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

function Get-ServiceJson {
    $serviceExe = "C:\Program Files\DoodleRay\DoodleRayService.exe"
    if (-not (Test-Path -LiteralPath $serviceExe)) {
        return [pscustomobject]@{ raw = "missing service exe"; parsed = $null }
    }

    $raw = (& $serviceExe status 2>&1 | Out-String).Trim()
    try {
        return [pscustomobject]@{ raw = $raw; parsed = ($raw | ConvertFrom-Json) }
    } catch {
        return [pscustomobject]@{ raw = $raw; parsed = $null }
    }
}

function Get-ScText {
    param([string[]] $ArgsList)

    try {
        return ((& sc.exe @ArgsList 2>&1) | Out-String).Trim()
    } catch {
        return $_.Exception.Message
    }
}

function Get-WebView2Runtime {
    $paths = @(
        "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F1D5F15F-ED90-45BF-B50B-388A0BEAF1FD}",
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F1D5F15F-ED90-45BF-B50B-388A0BEAF1FD}",
        "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F1D5F15F-ED90-45BF-B50B-388A0BEAF1FD}",
        "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
    )

    foreach ($path in $paths) {
        $value = Get-ItemProperty -LiteralPath $path -ErrorAction SilentlyContinue
        if ($value) {
            return [pscustomobject]@{
                present = $true
                source = $path
                version = $value.pv
                name = $value.name
            }
        }
    }

    $fixedRuntimeCandidates = @(
        "C:\Program Files\DoodleRay\WebView2Runtime",
        "C:\Program Files\DoodleRay\Microsoft.WebView2.FixedVersionRuntime",
        "C:\Program Files (x86)\Microsoft\EdgeWebView\Application",
        "C:\Program Files\Microsoft\EdgeWebView\Application"
    )
    foreach ($candidate in $fixedRuntimeCandidates) {
        if (Test-Path -LiteralPath $candidate) {
            $browserExe = Get-ChildItem -LiteralPath $candidate -Recurse -Filter "msedgewebview2.exe" -ErrorAction SilentlyContinue |
                Select-Object -First 1
            return [pscustomobject]@{
                present = $true
                source = $candidate
                version = if ($browserExe) { $browserExe.VersionInfo.FileVersion } else { "directory-present" }
                name = "WebView2 runtime directory"
            }
        }
    }

    return [pscustomobject]@{
        present = $false
        source = $null
        version = $null
        name = $null
    }
}

function Get-VcRuntimeState {
    $system32 = Join-Path $env:WINDIR "System32\vcruntime140.dll"
    $syswow64 = Join-Path $env:WINDIR "SysWOW64\vcruntime140.dll"
    [pscustomobject]@{
        system32 = [bool](Test-Path -LiteralPath $system32)
        syswow64 = [bool](Test-Path -LiteralPath $syswow64)
        system32Version = if (Test-Path -LiteralPath $system32) { (Get-Item -LiteralPath $system32).VersionInfo.FileVersion } else { $null }
        syswow64Version = if (Test-Path -LiteralPath $syswow64) { (Get-Item -LiteralPath $syswow64).VersionInfo.FileVersion } else { $null }
    }
}

function Get-SignatureSummary {
    param([string] $Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return [pscustomobject]@{ path = $Path; present = $false; status = "missing"; signer = $null }
    }

    $sig = Get-AuthenticodeSignature -LiteralPath $Path -ErrorAction SilentlyContinue
    [pscustomobject]@{
        path = $Path
        present = $true
        status = [string] $sig.Status
        signer = if ($sig.SignerCertificate) { $sig.SignerCertificate.Subject } else { $null }
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash
    }
}

function Get-NrptSummary {
    try {
        $policies = @(Get-DnsClientNrptPolicy -Effective -ErrorAction Stop)
        return [pscustomobject]@{
            ok = $true
            count = $policies.Count
            doodlerayLikeCount = @($policies | Where-Object {
                ($_.Comment -match "DoodleRay") -or ($_.Namespace -match "DoodleRay")
            }).Count
            namespaces = @($policies | Select-Object -First 8 | ForEach-Object {
                if ($_.Namespace) { "[namespace-redacted]" } else { "[empty]" }
            })
        }
    } catch {
        return [pscustomobject]@{ ok = $false; error = $_.Exception.Message }
    }
}

function Test-WebSocketProbe {
    param([string] $Uri = "wss://ws.postman-echo.com/raw")

    try {
        $clientType = [System.Net.WebSockets.ClientWebSocket]
        $socket = [System.Net.WebSockets.ClientWebSocket]::new()
        $cts = [System.Threading.CancellationTokenSource]::new()
        $cts.CancelAfter([TimeSpan]::FromSeconds(10))
        $task = $socket.ConnectAsync([Uri] $Uri, $cts.Token)
        [void] $task.Wait(12000)
        $state = [string] $socket.State
        $socket.Dispose()
        return [pscustomobject]@{ ok = ($state -eq "Open"); state = $state; target = "postman-echo" }
    } catch {
        return [pscustomobject]@{ ok = $false; state = "failed"; target = "postman-echo"; error = $_.Exception.Message }
    }
}

function Test-StunUdpProbe {
    param(
        [string] $HostName = "stun.l.google.com",
        [int] $Port = 19302
    )

    try {
        $addresses = [System.Net.Dns]::GetHostAddresses($HostName) |
            Where-Object { $_.AddressFamily -eq [System.Net.Sockets.AddressFamily]::InterNetwork }
        if (-not $addresses -or $addresses.Count -eq 0) {
            return [pscustomobject]@{ ok = $false; target = "stun"; error = "no IPv4 address" }
        }

        $txid = New-Object byte[] 12
        [System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($txid)
        $packet = New-Object byte[] 20
        $packet[0] = 0x00
        $packet[1] = 0x01
        $packet[2] = 0x00
        $packet[3] = 0x00
        $packet[4] = 0x21
        $packet[5] = 0x12
        $packet[6] = 0xA4
        $packet[7] = 0x42
        [Array]::Copy($txid, 0, $packet, 8, 12)

        $udp = [System.Net.Sockets.UdpClient]::new()
        $udp.Client.ReceiveTimeout = 8000
        $endpoint = [System.Net.IPEndPoint]::new($addresses[0], $Port)
        [void] $udp.Send($packet, $packet.Length, $endpoint)
        $remote = [System.Net.IPEndPoint]::new([System.Net.IPAddress]::Any, 0)
        $response = $udp.Receive([ref] $remote)
        $udp.Dispose()
        return [pscustomobject]@{
            ok = ($response.Length -ge 20)
            target = "stun"
            bytes = $response.Length
            remotePrefix = ConvertTo-IpPrefix $remote.Address.ToString()
        }
    } catch {
        return [pscustomobject]@{ ok = $false; target = "stun"; error = $_.Exception.Message }
    }
}

function Get-DnsResolutionProbe {
    param([string] $Name)

    try {
        $resolved = @(Resolve-DnsName -Name $Name -Type A -DnsOnly -QuickTimeout -ErrorAction Stop |
            Where-Object { $_.IPAddress } |
            Select-Object -First 4 -ExpandProperty IPAddress)
        return [pscustomobject]@{
            ok = ($resolved.Count -gt 0)
            name = $Name
            addresses = @($resolved | ForEach-Object { ConvertTo-IpPrefix $_ })
        }
    } catch {
        return [pscustomobject]@{ ok = $false; name = $Name; error = $_.Exception.Message }
    }
}

$appDir = "C:\Program Files\DoodleRay"
$internetSettings = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" -ErrorAction SilentlyContinue
$httpPort = Get-HttpProxyPort -InternetSettings $internetSettings
$proxyUrl = if ($httpPort) { "http://127.0.0.1:$httpPort" } else { "" }
$service = Get-ServiceJson
$processes = Get-Process DoodleRay, DoodleRayService, xray, sing-box -ErrorAction SilentlyContinue |
    Sort-Object ProcessName, Id |
    Select-Object ProcessName, Id, Path, CPU, WorkingSet64
$processIds = @($processes | Select-Object -ExpandProperty Id)

$defaultRoutesV4 = @(Get-NetRoute -AddressFamily IPv4 -DestinationPrefix "0.0.0.0/0" -ErrorAction SilentlyContinue |
    Sort-Object RouteMetric |
    Select-Object DestinationPrefix, InterfaceAlias, InterfaceIndex, NextHop, RouteMetric)
$defaultRoutesV6 = @(Get-NetRoute -AddressFamily IPv6 -DestinationPrefix "::/0" -ErrorAction SilentlyContinue |
    Sort-Object RouteMetric |
    Select-Object DestinationPrefix, InterfaceAlias, InterfaceIndex, NextHop, RouteMetric)
$doodleRoutes = @(Get-NetRoute -ErrorAction SilentlyContinue |
    Where-Object { $_.InterfaceAlias -like "*DoodleRay*" } |
    Select-Object -First 80 DestinationPrefix, AddressFamily, InterfaceAlias, InterfaceIndex, NextHop, RouteMetric)
$routeCanaries = @("104.26.13.205", "142.251.20.113", "162.159.136.232") | ForEach-Object {
    $ip = $_
    $best = Find-NetRoute -RemoteIPAddress $ip -ErrorAction SilentlyContinue | Select-Object -First 1
    [pscustomobject]@{
        targetPrefix = ConvertTo-IpPrefix $ip
        interfaceAlias = $best.InterfaceAlias
        interfaceIndex = $best.InterfaceIndex
        routeMetric = $best.RouteMetric
    }
}

$files = @(
    "DoodleRay.exe",
    "DoodleRayService.exe",
    "sing-box.exe",
    "wintun.dll",
    "xray-core\xray.exe"
) | ForEach-Object { Get-SignatureSummary (Join-Path $appDir $_) }

$listenPorts = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
    Where-Object {
        $_.LocalAddress -in @("127.0.0.1", "::1") -and
        (($processIds -contains $_.OwningProcess) -or ($_.LocalPort -eq $httpPort))
    } |
    Sort-Object LocalPort |
    Select-Object LocalAddress, LocalPort, OwningProcess)

$curlVersion = Invoke-CurlText @("--version")
$curlHttp3 = ($curlVersion -match "(?i)HTTP3|quiche|ngtcp2|nghttp3")

# Controlled QUIC/HTTP3 probe. Verdict stays "unverified" unless a real
# HTTP/3-only request succeeds; DoodleRay must not claim QUIC without this.
$quicProbe = [pscustomobject]@{
    verdict = "unverified-no-tooling"
    detail = "system curl does not support HTTP/3; QUIC coverage is not claimed"
}
if ($curlHttp3) {
    $quicResult = Invoke-CurlText @("--http3-only", "--max-time", "15", "-sS", "-o", "NUL", "-w", "%{http_code}", "https://cloudflare-quic.com/")
    if ($quicResult -match "^(2|3)\d\d$") {
        $quicProbe = [pscustomobject]@{
            verdict = "verified"
            detail = "HTTP/3-only request returned status $quicResult"
        }
    } else {
        $quicProbe = [pscustomobject]@{
            verdict = "failed"
            detail = "HTTP/3-only request did not succeed: $quicResult"
        }
    }
}

$directIp = ConvertTo-IpPrefix (Invoke-CurlText @("--max-time", "20", "-sS", "https://api.ipify.org"))
$directIpv6 = ConvertTo-IpPrefix (Invoke-CurlText @("--max-time", "15", "-sS", "https://api64.ipify.org"))

$proxyProbes = $null
if ($proxyUrl) {
    $proxyProbes = [pscustomobject]@{
        ipPrefix = ConvertTo-IpPrefix (Invoke-CurlText @("--max-time", "25", "-sS", "--proxy", $proxyUrl, "https://api.ipify.org"))
        ipv6Prefix = ConvertTo-IpPrefix (Invoke-CurlText @("--max-time", "15", "-sS", "--proxy", $proxyUrl, "https://api64.ipify.org"))
        apple = Invoke-CurlMetric -Url "https://captive.apple.com/hotspot-detect.html" -Proxy $proxyUrl
        google204 = Invoke-CurlMetric -Url "https://www.gstatic.com/generate_204" -Proxy $proxyUrl
        telegram = Invoke-CurlMetric -Url "https://api.telegram.org" -Proxy $proxyUrl
        discordGateway = Invoke-CurlMetric -Url "https://discord.com/api/v10/gateway" -Proxy $proxyUrl
        openAi = Invoke-CurlMetric -Url "https://chat.openai.com" -Proxy $proxyUrl
        claude = Invoke-CurlMetric -Url "https://claude.ai" -Proxy $proxyUrl
        sse = Invoke-CurlMetric -Url "https://stream.wikimedia.org/v2/stream/recentchange" -Proxy $proxyUrl -MaxTime 10
    }
}

$autoDetect = $internetSettings.AutoDetect
$autoConfigStatus = if ($internetSettings.AutoConfigURL) { "present-redacted" } else { "empty" }

[pscustomobject]@{
    mode = $Mode
    timestampUtc = (Get-Date).ToUniversalTime().ToString("o")
    os = [pscustomobject]@{
        caption = (Get-CimInstance Win32_OperatingSystem).Caption
        build = (Get-CimInstance Win32_OperatingSystem).BuildNumber
        architecture = (Get-CimInstance Win32_OperatingSystem).OSArchitecture
        freeMemoryMb = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1024, 0)
    }
    prerequisites = [pscustomobject]@{
        webView2 = Get-WebView2Runtime
        vcRuntime = Get-VcRuntimeState
        curlHttp3Support = [bool] $curlHttp3
    }
    quicProbe = $quicProbe
    signatures = $files
    service = [pscustomobject]@{
        status = $service.parsed
        query = Get-ScText @("query", "DoodleRayTunnelService")
        qfailure = Get-ScText @("qfailure", "DoodleRayTunnelService")
        qsidtype = Get-ScText @("qsidtype", "DoodleRayTunnelService")
    }
    proxy = [pscustomobject]@{
        winInet = [pscustomobject]@{
            proxyEnable = $internetSettings.ProxyEnable
            proxyServerStatus = if ($internetSettings.ProxyServer -match "127\.0\.0\.1") { "loopback" } elseif ($internetSettings.ProxyServer) { "non-loopback-redacted" } else { "empty" }
            proxyOverrideStatus = if ($internetSettings.ProxyOverride) { "present-redacted" } else { "empty" }
            autoDetect = $autoDetect
            autoConfigUrlStatus = $autoConfigStatus
            httpPort = $httpPort
        }
        winHttp = (netsh winhttp show proxy 2>&1 | Out-String).Trim()
    }
    networkIndicator = [pscustomobject]@{
        profiles = @(Get-NetConnectionProfile -ErrorAction SilentlyContinue |
            Select-Object Name, InterfaceAlias, IPv4Connectivity, IPv6Connectivity, NetworkCategory)
        ncsi = [pscustomobject]@{
            settings = Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Services\NlaSvc\Parameters\Internet" -ErrorAction SilentlyContinue |
                Select-Object EnableActiveProbing, ActiveWebProbeHost, ActiveWebProbePath, ActiveDnsProbeHost
            apple = Invoke-CurlMetric -Url "https://captive.apple.com/hotspot-detect.html"
            microsoft204 = Invoke-CurlMetric -Url "http://www.msftconnecttest.com/connecttest.txt" -MaxTime 10
        }
    }
    processes = $processes
    loopbackListeners = $listenPorts
    routes = [pscustomobject]@{
        defaultV4 = $defaultRoutesV4
        defaultV6 = $defaultRoutesV6
        doodleRay = $doodleRoutes
        canaries = $routeCanaries
    }
    dns = [pscustomobject]@{
        clientServers = @(Get-DnsClientServerAddress -ErrorAction SilentlyContinue |
            Select-Object InterfaceAlias, AddressFamily, @{Name = "ServerCount"; Expression = { @($_.ServerAddresses).Count } })
        nrpt = Get-NrptSummary
        google = Get-DnsResolutionProbe "www.google.com"
        apple = Get-DnsResolutionProbe "captive.apple.com"
        telegram = Get-DnsResolutionProbe "api.telegram.org"
    }
    directProbes = [pscustomobject]@{
        ipPrefix = $directIp
        ipv6Prefix = $directIpv6
        apple = Invoke-CurlMetric -Url "https://captive.apple.com/hotspot-detect.html"
        google204 = Invoke-CurlMetric -Url "https://www.gstatic.com/generate_204"
        twoIpObservedPrefix = ConvertTo-IpPrefix (Invoke-CurlText @("--max-time", "20", "-sS", "https://2ip.ru"))
        telegram = Invoke-CurlMetric -Url "https://api.telegram.org"
        discordGateway = Invoke-CurlMetric -Url "https://discord.com/api/v10/gateway"
        openAi = Invoke-CurlMetric -Url "https://chat.openai.com"
        claude = Invoke-CurlMetric -Url "https://claude.ai"
        sse = Invoke-CurlMetric -Url "https://stream.wikimedia.org/v2/stream/recentchange" -MaxTime 10
    }
    proxyProbes = $proxyProbes
    protocolProbes = [pscustomobject]@{
        webSocket = Test-WebSocketProbe
        udpStun = Test-StunUdpProbe
    }
} | ConvertTo-Json -Depth 18
