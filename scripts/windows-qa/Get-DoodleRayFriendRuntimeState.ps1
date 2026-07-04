param(
    [string] $OutPath = "C:\DoodleRayQA\evidence\friend-runtime-state.json",
    [switch] $CleanupDoodleRayOwnedEngines
)

$ErrorActionPreference = "Continue"
$ProgressPreference = "SilentlyContinue"

function Get-ServiceJson {
    $serviceExe = "C:\Program Files\DoodleRay\DoodleRayService.exe"
    if (-not (Test-Path -LiteralPath $serviceExe)) { return $null }
    $raw = (& $serviceExe status 2>&1 | Out-String).Trim()
    try { return $raw | ConvertFrom-Json } catch { return [pscustomobject]@{ raw = $raw } }
}

function Get-WinInet {
    Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings" -ErrorAction SilentlyContinue |
        Select-Object ProxyEnable, ProxyServer, ProxyOverride, AutoConfigURL, AutoDetect
}

$engineProcesses = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue | ForEach-Object {
    $path = $null
    try { $path = $_.Path } catch {}
    [pscustomobject]@{
        Name = $_.ProcessName
        Id = $_.Id
        Path = $path
        DoodleRayOwned = ([string]$path).StartsWith("C:\Program Files\DoodleRay", [System.StringComparison]::OrdinalIgnoreCase)
    }
})

if ($CleanupDoodleRayOwnedEngines) {
    foreach ($proc in $engineProcesses | Where-Object { $_.DoodleRayOwned }) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Seconds 3
}

$result = [pscustomobject]@{
    at = (Get-Date).ToString("o")
    user = (whoami)
    service = Get-ServiceJson
    winInet = Get-WinInet
    doodleRayUi = @(Get-Process DoodleRay -ErrorAction SilentlyContinue | Select-Object ProcessName, Id, Path)
    engines = @(Get-Process xray, sing-box -ErrorAction SilentlyContinue | ForEach-Object {
        $path = $null
        try { $path = $_.Path } catch {}
        [pscustomobject]@{
            Name = $_.ProcessName
            Id = $_.Id
            Path = $path
            DoodleRayOwned = ([string]$path).StartsWith("C:\Program Files\DoodleRay", [System.StringComparison]::OrdinalIgnoreCase)
        }
    })
    adapter = @(Get-NetAdapter -Name "DoodleRay Tunnel" -ErrorAction SilentlyContinue | Select-Object Name,Status,InterfaceDescription)
    marker = (Test-Path "C:\ProgramData\DoodleRay\runtime\active-session.marker")
    statsquery = @(Get-CimInstance Win32_Process -Filter "Name = 'xray.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -match "api\s+statsquery" } |
        Select-Object ProcessId, CommandLine)
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutPath) | Out-Null
$result | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $OutPath -Encoding UTF8
$result | ConvertTo-Json -Depth 10
