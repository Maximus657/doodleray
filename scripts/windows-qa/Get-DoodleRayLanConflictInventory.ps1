$ErrorActionPreference = "Continue"
$ProgressPreference = "SilentlyContinue"

function Select-Interesting {
    param($Items, [string[]] $Patterns)
    @($Items | Where-Object {
        $text = ($_ | Out-String)
        foreach ($pattern in $Patterns) {
            if ($text -match $pattern) { return $true }
        }
        return $false
    })
}

$patterns = @(
    "DoodleRay",
    "zapret",
    "goodbye",
    "goodbyedpi",
    "winws",
    "nfqws",
    "windivert",
    "happ",
    "wireguard",
    "wintun",
    "openvpn",
    "tap",
    "tailscale",
    "netbird",
    "zerotier",
    "outline",
    "clash",
    "mihomo",
    "v2ray",
    "xray",
    "sing-box",
    "nekoray",
    "hiddify",
    "adguard",
    "warp",
    "cloudflare",
    "proxifier",
    "proxycap"
)

$internetSettings = Get-ItemProperty -LiteralPath "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" -ErrorAction SilentlyContinue
$winHttp = (& netsh winhttp show proxy 2>&1 | Out-String).Trim()
$services = Get-CimInstance Win32_Service -ErrorAction SilentlyContinue |
    Select-Object Name, DisplayName, State, StartMode, PathName
$processes = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
    Select-Object Name, ProcessId, CommandLine
$drivers = Get-CimInstance Win32_SystemDriver -ErrorAction SilentlyContinue |
    Select-Object Name, DisplayName, State, StartMode, PathName
$adapters = Get-NetAdapter -ErrorAction SilentlyContinue |
    Select-Object Name, InterfaceDescription, Status, MacAddress, LinkSpeed
$routes = Get-NetRoute -AddressFamily IPv4 -ErrorAction SilentlyContinue |
    Where-Object { $_.DestinationPrefix -eq "0.0.0.0/0" -or $_.InterfaceAlias -match "DoodleRay|Wintun|WireGuard|TAP|VPN|Happ" } |
    Select-Object DestinationPrefix, InterfaceAlias, InterfaceIndex, NextHop, RouteMetric
$dns = Get-DnsClientServerAddress -ErrorAction SilentlyContinue |
    Select-Object InterfaceAlias, AddressFamily, ServerAddresses
$nrpt = @(Get-DnsClientNrptRule -ErrorAction SilentlyContinue |
    Select-Object Namespace, Comment, NameServers)
$installed = @()
foreach ($path in @(
    "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*"
)) {
    $installed += Get-ItemProperty -Path $path -ErrorAction SilentlyContinue |
        Select-Object DisplayName, DisplayVersion, Publisher, InstallLocation
}

$os = Get-ComputerInfo -ErrorAction SilentlyContinue
$networkProfiles = Get-NetConnectionProfile -ErrorAction SilentlyContinue |
    Select-Object Name, InterfaceAlias, NetworkCategory, IPv4Connectivity, IPv6Connectivity
$firewallProfiles = Get-NetFirewallProfile -ErrorAction SilentlyContinue |
    Select-Object Name, Enabled, DefaultInboundAction, DefaultOutboundAction

[pscustomobject]@{
    collectedAt = (Get-Date).ToString("o")
    host = $env:COMPUTERNAME
    user = (whoami)
    isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    os = [pscustomobject]@{
        product = $os.WindowsProductName
        version = $os.WindowsVersion
        build = $os.OsBuildNumber
    }
    networkProfiles = $networkProfiles
    firewallProfiles = $firewallProfiles
    winInet = [pscustomobject]@{
        proxyEnable = $internetSettings.ProxyEnable
        proxyServerSet = [bool]$internetSettings.ProxyServer
        proxyOverrideSet = [bool]$internetSettings.ProxyOverride
        autoConfigUrlSet = [bool]$internetSettings.AutoConfigURL
        autoDetect = $internetSettings.AutoDetect
    }
    winHttp = $winHttp
    adapters = $adapters
    defaultRoutes = $routes
    dns = $dns
    nrpt = [pscustomobject]@{
        count = $nrpt.Count
        interesting = Select-Interesting $nrpt $patterns
    }
    interestingServices = Select-Interesting $services $patterns
    interestingProcesses = Select-Interesting $processes $patterns
    interestingDrivers = Select-Interesting $drivers $patterns
    interestingInstalledApps = Select-Interesting $installed $patterns
} | ConvertTo-Json -Depth 8
