# Shared remote-side helpers for CDP-driven QA passes on the Play2Go stand.
# This file is not executed locally: orchestrator scripts read it raw and
# prepend it to their remote script body (the SSH wrapper uploads one script).
# Keep it ASCII-only: the uploaded file is UTF-8 without BOM and remote
# Windows PowerShell 5.1 would misread non-ASCII bytes.

$ErrorActionPreference = "Continue"
$ProgressPreference = "SilentlyContinue"

$steps = New-Object System.Collections.Generic.List[object]

function Add-Step {
    param([string] $Name, [bool] $Ok, $Detail)
    $steps.Add([pscustomobject]@{ step = $Name; ok = $Ok; detail = $Detail })
}

function Get-ServiceStatus {
    $raw = (& "C:\Program Files\DoodleRay\DoodleRayService.exe" status 2>&1 | Out-String).Trim()
    try { return $raw | ConvertFrom-Json } catch { return $null }
}

function Get-WinInet {
    Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" |
        Select-Object ProxyEnable, ProxyServer, ProxyOverride, AutoConfigURL
}

function Get-StatsQueryOrphanCount {
    @(Get-CimInstance Win32_Process -Filter "name = 'xray.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -match "api\s+statsquery" }).Count
}

# --- Minimal CDP client over ClientWebSocket -------------------------------
$script:CdpWs = $null
$script:CdpId = 0
$script:CdpCt = [System.Threading.CancellationToken]::None

function Connect-Cdp {
    for ($i = 0; $i -lt 30; $i++) {
        try {
            $targets = Invoke-RestMethod "http://127.0.0.1:9333/json" -TimeoutSec 3
            $page = @($targets | Where-Object { $_.type -eq "page" }) | Select-Object -First 1
            if ($page) {
                $ws = New-Object System.Net.WebSockets.ClientWebSocket
                $ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, $script:CdpCt).GetAwaiter().GetResult()
                $script:CdpWs = $ws
                return $true
            }
        } catch {}
        Start-Sleep -Seconds 2
    }
    return $false
}

function Invoke-CdpEval {
    param([string] $Expression, [int] $TimeoutSec = 20)
    $script:CdpId++
    try {
        $payload = @{
            id = $script:CdpId
            method = "Runtime.evaluate"
            params = @{ expression = $Expression; returnByValue = $true; awaitPromise = $true }
        } | ConvertTo-Json -Depth 8 -Compress
        $bytes = [Text.Encoding]::UTF8.GetBytes($payload)
        $seg = New-Object "System.ArraySegment[byte]" -ArgumentList @(,$bytes)
        $script:CdpWs.SendAsync($seg, [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $script:CdpCt).GetAwaiter().GetResult() | Out-Null

        $deadline = (Get-Date).AddSeconds($TimeoutSec)
        $buffer = New-Object byte[] 262144
        while ((Get-Date) -lt $deadline) {
            $sb = New-Object System.Text.StringBuilder
            do {
                $rseg = New-Object "System.ArraySegment[byte]" -ArgumentList @(,$buffer)
                $res = $script:CdpWs.ReceiveAsync($rseg, $script:CdpCt).GetAwaiter().GetResult()
                [void]$sb.Append([Text.Encoding]::UTF8.GetString($buffer, 0, $res.Count))
            } while (-not $res.EndOfMessage)
            try { $msg = $sb.ToString() | ConvertFrom-Json } catch { continue }
            if ($msg.id -eq $script:CdpId) {
                if ($msg.result.exceptionDetails) {
                    return [pscustomobject]@{ error = $msg.result.exceptionDetails.text }
                }
                return $msg.result.result.value
            }
        }
        return [pscustomobject]@{ error = "cdp eval timeout" }
    } catch {
        return [pscustomobject]@{ error = "cdp transport: $($_.Exception.Message)" }
    }
}

function Wait-CdpCondition {
    param([string] $Expression, [int] $TimeoutSec = 60, [int] $PollSec = 2)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $value = Invoke-CdpEval -Expression $Expression -TimeoutSec 10
        if ($value -eq $true) { return $true }
        Start-Sleep -Seconds $PollSec
    }
    return $false
}

function Start-AppWithCdp {
    schtasks /Run /TN DoodleRayCodexCDP | Out-Null
    for ($i = 0; $i -lt 30; $i++) {
        Start-Sleep -Seconds 2
        if ((Test-NetConnection 127.0.0.1 -Port 9333 -WarningAction SilentlyContinue).TcpTestSucceeded) { break }
    }
    if (-not (Connect-Cdp)) { return $false }
    return (Wait-CdpCondition 'document.querySelector("#connect-button") !== null' 40)
}

function Wait-ServiceState {
    param([string[]] $States, [int] $TimeoutSec = 60)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $s = Get-ServiceStatus
        if ($s -and $States -contains ([string]$s.state)) { return $s }
        Start-Sleep -Seconds 3
    }
    return (Get-ServiceStatus)
}

# UI expressions (ASCII/structure only; no locale strings).
$exprConnected = 'document.querySelector("#connect-button").className.includes("animate-vpn-connected")'
$exprNotConnected = '!document.querySelector("#connect-button").className.includes("animate-vpn-connected")'
$exprOpenDrawer = '(() => { const d = document.querySelector("[data-open]"); if (d && d.getAttribute("data-open") === "true") return "already-open"; const t = [...document.querySelectorAll("button[aria-expanded]")].find(b => b.querySelector(".lucide-chevron-down, svg.lucide-chevron-down")); if (!t) return "no-toggle"; t.click(); return "opened"; })()'

function Get-ModeClickExpr([int] $Index) {
    '(() => { const cards = document.querySelectorAll(".drawer-collapse .grid > div"); if (cards.length < 3) return "no-cards"; const btn = cards[' + $Index + '].querySelector("button"); if (!btn) return "no-btn"; if (btn.disabled) return "busy"; btn.click(); return "clicked"; })()'
}

# Mode card order in DashboardControlsDrawer: 0=protected, 1=browsers, 2=manual.
function Switch-Mode {
    param([int] $CardIndex, [string] $Label)
    $open = Invoke-CdpEval $exprOpenDrawer
    Start-Sleep -Seconds 1
    $click = Invoke-CdpEval (Get-ModeClickExpr $CardIndex)
    return "drawer=$open click=$click"
}
