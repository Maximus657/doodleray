param(
    [ValidateSet("Local", "Lan", "Play2Go")]
    [string] $Target = "Local",
    [int] $Attempts = 3,
    [string] $InstallerPath = "C:\DoodleRayQA\artifacts\DoodleRay_5.9.0_x64-setup.exe",
    [string] $SubscriptionPath = "C:\DoodleRayQA\secrets\doodlevpn-test-subscription-url.txt",
    [string] $EvidenceRoot = "C:\DoodleRayQA\evidence\connect-perf",
    [switch] $SkipInstall,
    [switch] $RemoteInner
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Invoke-SelfRemote {
    param([ValidateSet("Lan", "Play2Go")] [string] $RemoteTarget)

    $scriptText = Get-Content -LiteralPath $PSCommandPath -Raw
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($scriptText))
    $skipInstallArg = if ($SkipInstall) { "-SkipInstall" } else { "" }
    $command = @"
`$ErrorActionPreference = "Stop"
`$ProgressPreference = "SilentlyContinue"
`$scriptText = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String('$encoded'))
`$scriptPath = 'C:\DoodleRayQA\codex-run\Invoke-DoodleRayConnectPerfQa.ps1'
New-Item -ItemType Directory -Force -Path (Split-Path -Parent `$scriptPath) | Out-Null
Set-Content -LiteralPath `$scriptPath -Value `$scriptText -Encoding UTF8
& `$scriptPath -Target Local -RemoteInner -Attempts $Attempts -InstallerPath '$InstallerPath' -SubscriptionPath '$SubscriptionPath' -EvidenceRoot '$EvidenceRoot' $skipInstallArg
exit `$LASTEXITCODE
"@

    if ($RemoteTarget -eq "Lan") {
        & (Join-Path $PSScriptRoot "Invoke-LanQaPowerShell.ps1") -Command $command -TimeoutSec 3600
        exit $LASTEXITCODE
    }

    & (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $command -TimeoutSec 3600
    exit $LASTEXITCODE
}

if (-not $RemoteInner -and $Target -ne "Local") {
    Invoke-SelfRemote -RemoteTarget $Target
}

$runId = Get-Date -Format "yyyyMMdd-HHmmss"
$evidence = Join-Path $EvidenceRoot $runId
New-Item -ItemType Directory -Force -Path $evidence | Out-Null

$steps = New-Object System.Collections.Generic.List[object]
$attemptRows = New-Object System.Collections.Generic.List[object]

function Save-Json {
    param([string] $Name, $Object)
    $Object | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $evidence $Name) -Encoding UTF8
}

function Add-Step {
    param([string] $Name, [bool] $Ok, [string] $Detail = "")
    $steps.Add([pscustomobject]@{
        step = $Name
        ok = $Ok
        detail = $Detail
        at = (Get-Date).ToString("o")
    })
    Save-Json "progress.json" $steps
}

function Invoke-QaControl {
    param([string] $Route, [int] $TimeoutSec = 20)
    try {
        return Invoke-RestMethod "http://127.0.0.1:48765$Route" -TimeoutSec $TimeoutSec
    } catch {
        return [pscustomobject]@{ ok = $false; error = $_.Exception.Message }
    }
}

function Wait-QaControl {
    param([int] $TimeoutSec = 90)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $status = Invoke-QaControl "/status" 5
        if ($status -and $status.PSObject.Properties["app_version"]) { return $status }
        Start-Sleep -Seconds 2
    }
    return $null
}

function Test-ConnectedStatus {
    param($Status)
    if (-not $Status) { return $false }
    if ([bool]$Status.app_connected) { return $true }
    if ($Status.frontend -and [string]$Status.frontend.status -eq "connected") { return $true }
    return ($Status.service -and [string]$Status.service.state -eq "connected")
}

function Wait-Connected {
    param([int] $TimeoutSec = 210)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $status = Invoke-QaControl "/status" 8
        if (Test-ConnectedStatus $status) { return $status }
        Start-Sleep -Seconds 2
    }
    return (Invoke-QaControl "/status" 8)
}

function Wait-Disconnected {
    param([int] $TimeoutSec = 90)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $status = Invoke-QaControl "/status" 8
        $front = if ($status.frontend) { [string]$status.frontend.status } else { "" }
        $service = if ($status.service) { [string]$status.service.state } else { "" }
        if (-not [bool]$status.app_connected -and ($front -eq "" -or $front -eq "disconnected") -and ($service -eq "" -or $service -in @("disconnected", "failed"))) {
            return $status
        }
        Invoke-QaControl "/disconnect" 8 | Out-Null
        Start-Sleep -Seconds 3
    }
    return (Invoke-QaControl "/status" 8)
}

function Get-TimingValue {
    param($Timings, [string] $Name)
    foreach ($entry in @($Timings)) {
        if ($entry -is [array] -and $entry.Count -ge 2 -and [string]$entry[0] -eq $Name) {
            return [int64]$entry[1]
        }
        if ($entry.PSObject.Properties["Item1"] -and [string]$entry.Item1 -eq $Name) {
            return [int64]$entry.Item2
        }
    }
    return $null
}

function Get-Median {
    param([object[]] $Values)
    $numbers = @($Values | Where-Object { $null -ne $_ } | ForEach-Object { [double]$_ } | Sort-Object)
    if ($numbers.Count -eq 0) { return $null }
    $middle = [int]($numbers.Count / 2)
    if ($numbers.Count % 2 -eq 1) { return $numbers[$middle] }
    return [math]::Round(($numbers[$middle - 1] + $numbers[$middle]) / 2, 2)
}

function Get-CleanupSnapshot {
    $winInet = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" -ErrorAction SilentlyContinue |
        Select-Object ProxyEnable, ProxyServer, ProxyOverride, AutoConfigURL, AutoDetect
    [pscustomobject]@{
        at = (Get-Date).ToString("o")
        winInet = $winInet
        engines = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue | Select-Object ProcessName, Id, Path)
        statsquery = @(Get-CimInstance Win32_Process -Filter "Name = 'xray.exe'" -ErrorAction SilentlyContinue |
            Where-Object { $_.CommandLine -match "api\s+statsquery" } |
            Select-Object ProcessId, CommandLine)
        adapters = @(Get-NetAdapter -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like "*DoodleRay*" -or $_.InterfaceDescription -like "*Wintun*" } |
            Select-Object Name, Status, InterfaceDescription, ifIndex)
        nrpt = @(Get-DnsClientNrptRule -ErrorAction SilentlyContinue |
            Where-Object { ($_.Comment -match "DoodleRay") -or ($_.Namespace -match "DoodleRay") } |
            Select-Object Namespace, Comment)
        marker = (Test-Path "C:\ProgramData\DoodleRay\runtime\active-session.marker")
    }
}

function Start-DoodleRayQaApp {
    $appExe = "C:\Program Files\DoodleRay\DoodleRay.exe"
    if (-not (Test-Path -LiteralPath $appExe)) { throw "DoodleRay.exe missing: $appExe" }
    Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3
    $env:DOODLERAY_QA_CONTROL = "1"
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9333 --remote-allow-origins=*"
    Start-Process -FilePath $appExe -WorkingDirectory (Split-Path -Parent $appExe)
    return (Wait-QaControl 90)
}

function Ensure-Subscription {
    $status = Invoke-QaControl "/status" 8
    if ($status.frontend -and [int]$status.frontend.subscriptions_count -gt 0 -and [int]$status.frontend.servers_count -gt 0) {
        return $status
    }
    if (-not (Test-Path -LiteralPath $SubscriptionPath)) { return $status }
    $subUrl = (Get-Content -LiteralPath $SubscriptionPath -Raw).Trim()
    if (-not $subUrl) { return $status }
    $encoded = [uri]::EscapeDataString($subUrl)
    Invoke-QaControl "/import-subscription?url=$encoded" 30 | Out-Null
    $deadline = (Get-Date).AddSeconds(90)
    while ((Get-Date) -lt $deadline) {
        $status = Invoke-QaControl "/status" 8
        if ($status.frontend -and [int]$status.frontend.subscriptions_count -gt 0 -and [int]$status.frontend.servers_count -gt 0) {
            return $status
        }
        Start-Sleep -Seconds 3
    }
    return $status
}

function Run-ConnectAttempt {
    param(
        [string] $Mode,
        [string] $Label,
        [int] $Index,
        [switch] $DisconnectBefore
    )

    if ($DisconnectBefore) {
        Invoke-QaControl "/disconnect" 8 | Out-Null
        Wait-Disconnected 90 | Out-Null
    }
    Invoke-QaControl "/switch-mode?mode=$Mode" 15 | Out-Null
    Start-Sleep -Milliseconds 700

    $wall = [System.Diagnostics.Stopwatch]::StartNew()
    Invoke-QaControl "/connect" 20 | Out-Null
    $status = Wait-Connected 240
    $wall.Stop()

    $service = $status.service
    $timings = if ($service) { $service.timings_ms } else { @() }
    $totalConnect = Get-TimingValue $timings "total_connect"
    $row = [pscustomobject]@{
        label = $Label
        mode = $Mode
        index = $Index
        ok = (Test-ConnectedStatus $status)
        wall_ms = [int64]$wall.ElapsedMilliseconds
        total_connect_ms = $totalConnect
        service_state = if ($service) { [string]$service.state } else { $null }
        service_verdict = if ($service) { [string]$service.health_verdict } else { $null }
        adapter_backend = if ($service) { $service.adapter_probe_backend } else { $null }
        route_backend = if ($service) { $service.route_probe_backend } else { $null }
        powershell_fallback_count = if ($service) { [int]$service.powershell_fallback_count } else { $null }
        singbox_check_ms = if ($service) { $service.singbox_check_ms } else { $null }
        xray_spawn_ms = if ($service) { $service.xray_spawn_ms } else { $null }
        timings_ms = $timings
    }
    $attemptRows.Add($row)
    Save-Json ("attempt-{0}-{1}.json" -f $Label, $Index) $status
    return $row
}

function Run-RepairAttempt {
    param(
        [string] $Label,
        [int] $Index
    )

    $wall = [System.Diagnostics.Stopwatch]::StartNew()
    $repair = Invoke-QaControl "/repair-runtime?reason=$Label" 45
    $status = Invoke-QaControl "/status" 8
    $wall.Stop()

    $service = $status.service
    $timings = if ($service) { $service.timings_ms } else { @() }
    $repairTiming = Get-TimingValue $timings "repair:$Label"
    $row = [pscustomobject]@{
        label = $Label
        mode = "tun"
        index = $Index
        ok = ((Test-ConnectedStatus $status) -and [bool]$repair.ok)
        wall_ms = [int64]$wall.ElapsedMilliseconds
        total_connect_ms = $repairTiming
        service_state = if ($service) { [string]$service.state } else { $null }
        service_verdict = if ($service) { [string]$service.health_verdict } else { $null }
        adapter_backend = if ($service) { $service.adapter_probe_backend } else { $null }
        route_backend = if ($service) { $service.route_probe_backend } else { $null }
        powershell_fallback_count = if ($service) { [int]$service.powershell_fallback_count } else { $null }
        singbox_check_ms = if ($service) { $service.singbox_check_ms } else { $null }
        xray_spawn_ms = if ($service) { $service.xray_spawn_ms } else { $null }
        timings_ms = $timings
    }
    $attemptRows.Add($row)
    Save-Json ("attempt-{0}-{1}.json" -f $Label, $Index) ([pscustomobject]@{ repair = $repair; status = $status })
    return $row
}

$transcript = Join-Path $evidence "transcript.log"
Start-Transcript -LiteralPath $transcript -Force | Out-Null

try {
    $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    Add-Step "admin_context" $isAdmin "user=$(whoami)"
    if (-not $isAdmin) { throw "Run from elevated PowerShell." }

    if (-not $SkipInstall) {
        if (-not (Test-Path -LiteralPath $InstallerPath)) { throw "Installer not found: $InstallerPath" }
        $install = Start-Process -FilePath $InstallerPath -ArgumentList "/S" -Wait -PassThru
        Add-Step "silent_install" ($install.ExitCode -eq 0) "exit=$($install.ExitCode)"
        Start-Sleep -Seconds 8
    } else {
        Add-Step "silent_install" $true "skipped"
    }

    if (Test-Path (Join-Path $PSScriptRoot "Get-DoodleRayLanConflictInventory.ps1")) {
        Save-Json "baseline-conflicts.json" (& (Join-Path $PSScriptRoot "Get-DoodleRayLanConflictInventory.ps1") | ConvertFrom-Json)
    }
    Save-Json "baseline-cleanup.json" (Get-CleanupSnapshot)

    $status = Start-DoodleRayQaApp
    Add-Step "qa_control_ready" ([bool]$status) $(if ($status) { "app=$($status.app_version)" } else { "timeout" })
    if (-not $status) { throw "QA control did not become ready." }

    $subStatus = Ensure-Subscription
    $subOk = ($subStatus.frontend -and [int]$subStatus.frontend.servers_count -gt 0)
    Add-Step "subscription_ready" $subOk $(if ($subStatus.frontend) { "servers=$($subStatus.frontend.servers_count)" } else { "no frontend snapshot" })
    if (-not $subOk) { throw "Subscription/server list is not ready." }

    for ($i = 1; $i -le $Attempts; $i++) {
        Run-ConnectAttempt -Mode "browsers" -Label "browsers" -Index $i -DisconnectBefore | Out-Null
    }
    for ($i = 1; $i -le $Attempts; $i++) {
        Run-ConnectAttempt -Mode "tun" -Label "tun-cold" -Index $i -DisconnectBefore | Out-Null
    }
    if (Test-ConnectedStatus (Invoke-QaControl "/status" 8)) {
        for ($i = 1; $i -le $Attempts; $i++) {
            Run-RepairAttempt -Label "tun-warm-reassert" -Index $i | Out-Null
        }
    }
    for ($i = 1; $i -le $Attempts; $i++) {
        Run-ConnectAttempt -Mode "tun" -Label "tun-reconnect" -Index $i -DisconnectBefore | Out-Null
    }

    Run-ConnectAttempt -Mode "browsers" -Label "transition-proxy" -Index 1 -DisconnectBefore | Out-Null
    Run-ConnectAttempt -Mode "tun" -Label "transition-proxy-to-tun" -Index 1 | Out-Null
    Run-ConnectAttempt -Mode "browsers" -Label "transition-tun-to-proxy" -Index 1 | Out-Null

    $bundle = Invoke-QaControl "/export-bundle" 90
    Save-Json "support-bundle-result.json" $bundle

    Invoke-QaControl "/disconnect" 10 | Out-Null
    Wait-Disconnected 120 | Out-Null
    Get-Process DoodleRay -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 5
    Save-Json "final-cleanup.json" (Get-CleanupSnapshot)

    $groups = $attemptRows | Group-Object label | ForEach-Object {
        [pscustomobject]@{
            label = $_.Name
            count = $_.Count
            ok_count = @($_.Group | Where-Object ok).Count
            median_total_connect_ms = Get-Median @($_.Group | ForEach-Object { $_.total_connect_ms })
            median_wall_ms = Get-Median @($_.Group | ForEach-Object { $_.wall_ms })
            max_powershell_fallback_count = (@($_.Group | ForEach-Object { $_.powershell_fallback_count }) | Measure-Object -Maximum).Maximum
            adapter_backends = @($_.Group | ForEach-Object { $_.adapter_backend } | Sort-Object -Unique)
            route_backends = @($_.Group | ForEach-Object { $_.route_backend } | Sort-Object -Unique)
        }
    }

    $thresholdFailures = New-Object System.Collections.Generic.List[string]
    foreach ($group in $groups) {
        if ($group.ok_count -ne $group.count) {
            $thresholdFailures.Add("$($group.label): only $($group.ok_count)/$($group.count) attempts connected")
        }
        if ($group.label -eq "browsers" -and $group.median_wall_ms -gt 2500) {
            $thresholdFailures.Add("browsers median wall $($group.median_wall_ms)ms > 2500ms")
        }
        if ($group.label -eq "tun-cold" -and $group.median_total_connect_ms -gt 7000) {
            $thresholdFailures.Add("tun cold median service $($group.median_total_connect_ms)ms > 7000ms")
        }
        if ($group.label -eq "tun-reconnect" -and $group.median_total_connect_ms -gt 4000) {
            $thresholdFailures.Add("tun reconnect median service $($group.median_total_connect_ms)ms > 4000ms")
        }
        if ($group.label -eq "tun-warm-reassert" -and $group.median_total_connect_ms -gt 2500) {
            $thresholdFailures.Add("tun warm reassert median service $($group.median_total_connect_ms)ms > 2500ms")
        }
    }

    Save-Json "attempts.json" $attemptRows
    Save-Json "summary.json" ([pscustomobject]@{
        ok = ($thresholdFailures.Count -eq 0)
        evidence = $evidence
        thresholds = [pscustomobject]@{
            browsers_wall_median_ms = 2500
            tun_cold_service_median_ms = 7000
            tun_reconnect_service_median_ms = 4000
            tun_warm_reassert_service_median_ms = 2500
        }
        groups = $groups
        failures = $thresholdFailures
        steps = $steps
    })
} catch {
    Add-Step "fatal_exception" $false $_.Exception.Message
    Save-Json "summary.json" ([pscustomobject]@{
        ok = $false
        evidence = $evidence
        failures = @($_.Exception.Message)
        steps = $steps
        attempts = $attemptRows
    })
} finally {
    Stop-Transcript | Out-Null
}

$summaryText = Get-Content -LiteralPath (Join-Path $evidence "summary.json") -Raw
$summaryText
$summary = $summaryText | ConvertFrom-Json
if (-not [bool]$summary.ok) { exit 1 }
