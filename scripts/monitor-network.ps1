param(
  [int]$DurationSeconds = 3600,
  [int]$IntervalSeconds = 2,
  [string]$OutDir = "$PSScriptRoot\..\logs"
)

$ErrorActionPreference = "SilentlyContinue"
$ProgressPreference = "SilentlyContinue"

function New-LogPath {
  if (-not (Test-Path -LiteralPath $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
  }
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  Join-Path $OutDir "network-monitor-$stamp.jsonl"
}

function Test-TcpPort {
  param([string]$HostName, [int]$Port, [int]$TimeoutMs = 1200)
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $client = New-Object System.Net.Sockets.TcpClient
  try {
    $async = $client.BeginConnect($HostName, $Port, $null, $null)
    $ok = $async.AsyncWaitHandle.WaitOne($TimeoutMs)
    if (-not $ok) {
      return @{ ok = $false; ms = $sw.ElapsedMilliseconds; error = "timeout" }
    }
    $client.EndConnect($async)
    return @{ ok = $true; ms = $sw.ElapsedMilliseconds; error = $null }
  } catch {
    return @{ ok = $false; ms = $sw.ElapsedMilliseconds; error = $_.Exception.Message }
  } finally {
    $client.Close()
    $sw.Stop()
  }
}

function Test-DnsNameFast {
  param([string]$Name)
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  try {
    $records = Resolve-DnsName -Name $Name -Type A -ErrorAction Stop | Select-Object -First 3
    return @{
      ok = $true
      ms = $sw.ElapsedMilliseconds
      answers = @($records | ForEach-Object { $_.IPAddress })
      error = $null
    }
  } catch {
    return @{ ok = $false; ms = $sw.ElapsedMilliseconds; answers = @(); error = $_.Exception.Message }
  } finally {
    $sw.Stop()
  }
}

function Test-PingFast {
  param([string]$HostName, [int]$Count = 2)
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  try {
    $items = Test-Connection -ComputerName $HostName -Count $Count -ErrorAction Stop
    $latencies = @($items | ForEach-Object { [int64]$_.Latency })
    return @{
      ok = $true
      ms = $sw.ElapsedMilliseconds
      count = $latencies.Count
      min_ms = if ($latencies.Count -gt 0) { ($latencies | Measure-Object -Minimum).Minimum } else { $null }
      max_ms = if ($latencies.Count -gt 0) { ($latencies | Measure-Object -Maximum).Maximum } else { $null }
      avg_ms = if ($latencies.Count -gt 0) { [math]::Round(($latencies | Measure-Object -Average).Average, 1) } else { $null }
      error = $null
    }
  } catch {
    return @{ ok = $false; ms = $sw.ElapsedMilliseconds; count = 0; min_ms = $null; max_ms = $null; avg_ms = $null; error = $_.Exception.Message }
  } finally {
    $sw.Stop()
  }
}

function Get-DoodleRayServiceStatus {
  $exe = "C:\Program Files\DoodleRay\DoodleRayService.exe"
  if (-not (Test-Path -LiteralPath $exe)) {
    return @{ ok = $false; error = "service exe missing" }
  }
  try {
    $raw = & $exe status 2>&1
    $json = ($raw -join "`n") | ConvertFrom-Json
    return @{
      ok = $true
      state = $json.state
      phase = $json.phase
      active_op_id = $json.active_op_id
      error = $json.error
      timings_ms = $json.timings_ms
    }
  } catch {
    return @{ ok = $false; error = $_.Exception.Message }
  }
}

function Get-RouteSnapshot {
  $routes = @()
  $routes += Get-NetRoute -DestinationPrefix "0.0.0.0/0" -ErrorAction SilentlyContinue
  $routes += Get-NetRoute -DestinationPrefix "::/0" -ErrorAction SilentlyContinue
  @($routes | Select-Object DestinationPrefix, InterfaceAlias, InterfaceIndex, NextHop, RouteMetric, State)
}

function Get-AdapterSnapshot {
  $adapters = Get-NetAdapter |
    Where-Object {
      $_.Name -like "*DoodleRay*" -or
      $_.Name -like "*happ*" -or
      $_.Name -eq "Ethernet" -or
      $_.InterfaceDescription -like "*tun*" -or
      $_.InterfaceDescription -like "*Wintun*" -or
      $_.InterfaceDescription -like "*sing*"
    } |
    Select-Object Name, Status, InterfaceDescription, InterfaceIndex, LinkSpeed
  @($adapters)
}

function Get-DnsSnapshot {
  $dns = Get-DnsClientServerAddress |
    Where-Object {
      $_.InterfaceAlias -like "*DoodleRay*" -or
      $_.InterfaceAlias -like "*happ*" -or
      $_.InterfaceAlias -eq "Ethernet"
    } |
    Select-Object InterfaceAlias, AddressFamily, ServerAddresses
  @($dns)
}

function Get-ProcessSnapshot {
  $names = @(
    "DoodleRay", "DoodleRayService", "xray", "sing-box",
    "happ", "Happ", "Windscribe", "WindscribeService",
    "TslGame", "TslGame_BE", "ExecPubg", "BEService", "steam", "SteamService"
  )
  $items = Get-Process -Name $names -ErrorAction SilentlyContinue |
    Select-Object ProcessName, Id, Path, CPU, StartTime
  @($items)
}

function Get-ListenPortsSnapshot {
  $processIds = @(Get-Process -Name DoodleRay,DoodleRayService,xray,sing-box -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
  if ($processIds.Count -eq 0) { return @() }
  $ports = Get-NetTCPConnection -State Listen |
    Where-Object { $_.LocalAddress -eq "127.0.0.1" -and $_.OwningProcess -in $processIds } |
    Select-Object LocalAddress, LocalPort, OwningProcess
  @($ports)
}

$logPath = New-LogPath
$pidPath = [System.IO.Path]::ChangeExtension($logPath, ".pid")
$metaPath = [System.IO.Path]::ChangeExtension($logPath, ".meta.txt")

"$PID" | Set-Content -LiteralPath $pidPath -Encoding ASCII
@(
  "started=$(Get-Date -Format o)"
  "duration_seconds=$DurationSeconds"
  "interval_seconds=$IntervalSeconds"
  "log=$logPath"
  "pid=$PID"
) | Set-Content -LiteralPath $metaPath -Encoding UTF8

$started = Get-Date
$nextSlowSample = Get-Date
$end = $started.AddSeconds($DurationSeconds)

while ((Get-Date) -lt $end) {
  $now = Get-Date
  $slow = $now -ge $nextSlowSample
  if ($slow) {
    $nextSlowSample = $now.AddSeconds(10)
  }

  $sample = [ordered]@{
    ts = $now.ToString("o")
    elapsed_ms = [int64](($now - $started).TotalMilliseconds)
    service = Get-DoodleRayServiceStatus
    adapters = Get-AdapterSnapshot
    routes = Get-RouteSnapshot
    dns_servers = Get-DnsSnapshot
    tcp = [ordered]@{
      cloudflare_443 = Test-TcpPort "1.1.1.1" 443
      google_dns_53 = Test-TcpPort "8.8.8.8" 53
      discord_443 = Test-TcpPort "gateway.discord.gg" 443
    }
    ping = [ordered]@{
      gateway = Test-PingFast "192.168.0.1" 2
      cloudflare = Test-PingFast "1.1.1.1" 2
      google_dns = Test-PingFast "8.8.8.8" 2
      discord = Test-PingFast "gateway.discord.gg" 2
    }
    dns = [ordered]@{
      cloudflare = Test-DnsNameFast "cloudflare.com"
      discord = Test-DnsNameFast "gateway.discord.gg"
    }
    processes = if ($slow) { Get-ProcessSnapshot } else { @() }
    listen_ports = if ($slow) { Get-ListenPortsSnapshot } else { @() }
  }

  $sample | ConvertTo-Json -Depth 8 -Compress | Add-Content -LiteralPath $logPath -Encoding UTF8
  Start-Sleep -Seconds $IntervalSeconds
}

@(
  "finished=$(Get-Date -Format o)"
  "log=$logPath"
  "pid=$PID"
) | Add-Content -LiteralPath $metaPath -Encoding UTF8
