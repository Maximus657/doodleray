param(
    [Parameter(Mandatory = $true)]
    [string] $LocalInstaller,

    [string] $RemotePath = "C:\DoodleRayQA\artifacts\DoodleRay-v6-rc-setup.exe",
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md"),
    [string] $PscpPath = "C:\Program Files\PuTTY\pscp.exe"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Get-SecretField {
    param(
        [Parameter(Mandatory = $true)] [string] $Text,
        [Parameter(Mandatory = $true)] [string] $Name
    )

    $match = [regex]::Match($Text, "(?m)^\s*(?:-\s*)?$([regex]::Escape($Name))\s*:\s*(\S+)\s*$")
    if (-not $match.Success) {
        return $null
    }
    return $match.Groups[1].Value
}

if (-not (Test-Path -LiteralPath $LocalInstaller)) {
    throw "Local installer not found: $LocalInstaller"
}
if (-not (Test-Path -LiteralPath $SecretPath)) {
    throw "Secret file not found: $SecretPath"
}
if (-not (Test-Path -LiteralPath $PscpPath)) {
    throw "PuTTY pscp.exe not found: $PscpPath"
}

$secretText = Get-Content -LiteralPath $SecretPath -Raw
$hostName = Get-SecretField -Text $secretText -Name "host"
$userName = Get-SecretField -Text $secretText -Name "login_user"
$password = Get-SecretField -Text $secretText -Name "login_password"
$hostKey = Get-SecretField -Text $secretText -Name "ssh_hostkey"
if (-not $hostKey) {
    $hostKey = $env:DOODLERAY_PLAY2GO_HOSTKEY
}
if (-not $hostName -or -not $userName -or -not $password -or -not $hostKey) {
    throw "Secret file must contain host, login_user, login_password, and ssh_hostkey, or set DOODLERAY_PLAY2GO_HOSTKEY."
}

$remoteDir = Split-Path -Parent $RemotePath
$mkdir = "New-Item -ItemType Directory -Force -Path '$remoteDir' | Out-Null"
& (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $mkdir -SecretPath $SecretPath
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$target = "${userName}@${hostName}:$RemotePath"
& $PscpPath -batch -hostkey $hostKey -pw $password $LocalInstaller $target
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

[pscustomobject]@{
    uploaded = $true
    remotePath = $RemotePath
    sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $LocalInstaller).Hash
} | ConvertTo-Json
