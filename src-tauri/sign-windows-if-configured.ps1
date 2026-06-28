param(
  [Parameter(Mandatory = $true)]
  [string]$Path
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Path)) {
  throw "Cannot sign missing file: $Path"
}

$thumbprint = $env:WINDOWS_CODESIGN_THUMBPRINT
if ([string]::IsNullOrWhiteSpace($thumbprint)) {
  Write-Host "Windows code signing skipped for $Path because WINDOWS_CODESIGN_THUMBPRINT is not set."
  exit 0
}

$signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
if (-not $signtool) {
  $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\x64\\signtool\.exe$" } |
    Select-Object -First 1
}
if (-not $signtool) {
  throw "signtool.exe was not found."
}
$signtoolPath = if ($signtool.Source) { $signtool.Source } else { $signtool.FullName }

$timestampUrl = if ($env:WINDOWS_CODESIGN_TIMESTAMP_URL) {
  $env:WINDOWS_CODESIGN_TIMESTAMP_URL
} else {
  "http://timestamp.digicert.com"
}

& $signtoolPath sign `
  /fd SHA256 `
  /td SHA256 `
  /tr $timestampUrl `
  /sha $thumbprint `
  $Path

if ($LASTEXITCODE -ne 0) {
  throw "signtool failed for $Path with exit code $LASTEXITCODE"
}
