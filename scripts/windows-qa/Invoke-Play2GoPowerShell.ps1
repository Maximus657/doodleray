param(
    [Parameter(Mandatory = $true, ParameterSetName = "Command")]
    [string] $Command,

    [Parameter(Mandatory = $true, ParameterSetName = "Script")]
    [string] $ScriptPath,

    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md"),
    [string] $PlinkPath = "C:\Program Files\PuTTY\plink.exe",
    [string] $PscpPath = "C:\Program Files\PuTTY\pscp.exe",
    [string] $RemoteScratch = "C:\DoodleRayQA\codex-run"
)

$ErrorActionPreference = "Stop"

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

if (-not (Test-Path -LiteralPath $SecretPath)) {
    throw "Secret file not found: $SecretPath"
}

if (-not (Test-Path -LiteralPath $PlinkPath)) {
    throw "PuTTY plink.exe not found: $PlinkPath"
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

if (-not $hostName -or -not $userName -or -not $password) {
    throw "Secret file must contain host, login_user, and login_password fields."
}

if (-not $hostKey) {
    throw "Set ssh_hostkey in the secret file or DOODLERAY_PLAY2GO_HOSTKEY in the environment."
}

if ($PSCmdlet.ParameterSetName -eq "Script") {
    $Command = Get-Content -LiteralPath $ScriptPath -Raw
}

$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($Command))
$sshTarget = "$userName@$hostName"

if ($encoded.Length -lt 7000) {
    & $PlinkPath -ssh $sshTarget -pw $password -batch -hostkey $hostKey "powershell -NoProfile -EncodedCommand $encoded"
    exit $LASTEXITCODE
}

$remotePrep = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes("New-Item -ItemType Directory -Force -Path '$RemoteScratch' | Out-Null"))
& $PlinkPath -ssh $sshTarget -pw $password -batch -hostkey $hostKey "powershell -NoProfile -EncodedCommand $remotePrep"
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$localTemp = Join-Path $env:TEMP ("doodleray-play2go-" + [guid]::NewGuid().ToString("N") + ".ps1")
$remoteName = [IO.Path]::GetFileName($localTemp)
$remoteScript = ($RemoteScratch.TrimEnd("\") + "\" + $remoteName)
try {
    Set-Content -LiteralPath $localTemp -Value $Command -Encoding UTF8
    & $PscpPath -batch -hostkey $hostKey -pw $password $localTemp "${sshTarget}:$remoteScript"
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
    & $PlinkPath -ssh $sshTarget -pw $password -batch -hostkey $hostKey "powershell -NoProfile -ExecutionPolicy Bypass -File `"$remoteScript`""
} finally {
    Remove-Item -LiteralPath $localTemp -Force -ErrorAction SilentlyContinue
}
exit $LASTEXITCODE
