param(
    [string]$Version,
    [string]$Asset
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$runtime = Get-Content (Join-Path $repoRoot "runtime-versions.json") -Raw | ConvertFrom-Json
if ([string]::IsNullOrWhiteSpace($Version)) { $Version = $runtime.xray.version }
if ([string]::IsNullOrWhiteSpace($Asset)) { $Asset = $runtime.xray.assets.windows_amd64.name }
$tmpDir = Join-Path $env:TEMP "doodleray-xray-$Version"
$zipPath = Join-Path $tmpDir $Asset
$digestPath = "$zipPath.dgst"
$destDir = Join-Path $repoRoot "src-tauri\xray-core"
$baseUrl = "https://github.com/XTLS/Xray-core/releases/download/$Version"

New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

Write-Host "Downloading $Asset from XTLS/Xray-core $Version..."
Invoke-WebRequest -Uri "$baseUrl/$Asset" -OutFile $zipPath
Invoke-WebRequest -Uri "$baseUrl/$Asset.dgst" -OutFile $digestPath

$expectedHash = (Select-String -Path $digestPath -Pattern "^SHA2-256=\s*([a-fA-F0-9]+)" |
    Select-Object -First 1).Matches.Groups[1].Value.ToLowerInvariant()
$actualHash = (Get-FileHash -Algorithm SHA256 $zipPath).Hash.ToLowerInvariant()

if ($expectedHash -ne $actualHash) {
    throw "SHA256 mismatch for $Asset. Expected $expectedHash, got $actualHash."
}

if ($Version -eq $runtime.xray.version -and $Asset -eq $runtime.xray.assets.windows_amd64.name) {
    $pinnedHash = $runtime.xray.assets.windows_amd64.sha256.ToLowerInvariant()
    if ($pinnedHash -ne $actualHash) {
        throw "Pinned SHA256 mismatch for $Asset. Expected $pinnedHash, got $actualHash."
    }
}

if (Test-Path $destDir) {
    Remove-Item -LiteralPath $destDir -Recurse -Force
}

New-Item -ItemType Directory -Force -Path $destDir | Out-Null
Expand-Archive -LiteralPath $zipPath -DestinationPath $destDir -Force

& (Join-Path $destDir "xray.exe") version
