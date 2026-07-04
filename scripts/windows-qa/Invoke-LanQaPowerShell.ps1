param(
    [string] $TargetName,
    [string] $HostName,
    [string] $Username,
    [string] $Password,

    [Parameter(Mandatory = $true, ParameterSetName = "Command")]
    [string] $Command,

    [Parameter(Mandatory = $true, ParameterSetName = "Script")]
    [string] $ScriptPath,

    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\lan-qa-hosts.json"),
    [switch] $Interactive,
    [int] $TimeoutSec = 1800,
    [string] $RemoteScratch = "C:\DoodleRayQA\codex-run"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Get-PropertyValue {
    param($Object, [string[]] $Names)

    foreach ($name in $Names) {
        $property = $Object.PSObject.Properties |
            Where-Object { $_.Name -ieq $name } |
            Select-Object -First 1
        if ($property -and $null -ne $property.Value -and [string]$property.Value -ne "") {
            return $property.Value
        }
    }
    return $null
}

function Get-LanHostConfig {
    param([string] $Path, [string] $Name)

    $fallbackPaths = @(
        $Path,
        "D:\DoodleRayAPP\secrets\lan-qa-hosts.json"
    ) | Select-Object -Unique

    $existingPath = $null
    foreach ($candidate in $fallbackPaths) {
        if ($candidate -and (Test-Path -LiteralPath $candidate)) {
            $existingPath = $candidate
            break
        }
    }
    if (-not $existingPath) {
        return $null
    }

    $json = Get-Content -LiteralPath $existingPath -Raw | ConvertFrom-Json
    $items = @($json)
    if ($Name) {
        $match = $items | Where-Object {
            (Get-PropertyValue $_ @("name", "Name")) -eq $Name -or
            (Get-PropertyValue $_ @("host", "Host")) -eq $Name
        } | Select-Object -First 1
        if (-not $match) {
            throw "LAN QA target '$Name' not found in $existingPath"
        }
        return $match
    }

    if ($items.Count -ne 1) {
        throw "LAN QA config has $($items.Count) hosts; pass -TargetName."
    }
    return $items[0]
}

if (-not $HostName -or -not $Username -or -not $Password) {
    $config = Get-LanHostConfig -Path $SecretPath -Name $TargetName
    if ($config) {
        if (-not $HostName) { $HostName = [string](Get-PropertyValue $config @("host", "Host", "hostname", "HostName")) }
        if (-not $Username) { $Username = [string](Get-PropertyValue $config @("username", "Username", "user", "User")) }
        if (-not $Password) { $Password = [string](Get-PropertyValue $config @("password", "Password", "pass", "Pass")) }
    }
}

if (-not $Password -and $env:DOODLERAY_LAN_QA_PASSWORD) {
    $Password = $env:DOODLERAY_LAN_QA_PASSWORD
}

if (-not $HostName -or -not $Username -or -not $Password) {
    throw "HostName, Username, and Password are required. Put them in secrets/lan-qa-hosts.json or pass them as parameters."
}

if ($PSCmdlet.ParameterSetName -eq "Script") {
    if (-not (Test-Path -LiteralPath $ScriptPath)) {
        throw "Script not found: $ScriptPath"
    }
    $Command = Get-Content -LiteralPath $ScriptPath -Raw
}

$share = "\\$HostName\C$"
$remoteScratchUnc = "\\$HostName\C$" + $RemoteScratch.Substring(2)
$runId = "lanqa-" + (Get-Date -Format "yyyyMMdd-HHmmss") + "-" + ([guid]::NewGuid().ToString("N").Substring(0, 8))
$remoteScript = Join-Path $RemoteScratch "$runId.ps1"
$remoteWrapper = Join-Path $RemoteScratch "$runId.wrapper.ps1"
$remoteOut = Join-Path $RemoteScratch "$runId.out.log"
$remoteErr = Join-Path $RemoteScratch "$runId.err.log"
$remoteDone = Join-Path $RemoteScratch "$runId.done.json"
$uncScript = "\\$HostName\C$" + $remoteScript.Substring(2)
$uncWrapper = "\\$HostName\C$" + $remoteWrapper.Substring(2)
$uncOut = "\\$HostName\C$" + $remoteOut.Substring(2)
$uncErr = "\\$HostName\C$" + $remoteErr.Substring(2)
$uncDone = "\\$HostName\C$" + $remoteDone.Substring(2)
$taskName = "DoodleRayLanQa-$runId"

cmd /c "net use $share /delete /y >nul 2>nul" | Out-Null
$netUse = cmd /c "net use $share /user:$Username $Password" 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "Failed to connect to $share as $Username. net use exit=$LASTEXITCODE. $($netUse -join ' ')"
}

try {
    New-Item -ItemType Directory -Force -Path $remoteScratchUnc | Out-Null
    Set-Content -LiteralPath $uncScript -Value $Command -Encoding UTF8

    $wrapper = @"
`$ErrorActionPreference = "Continue"
`$ProgressPreference = "SilentlyContinue"
`$exitCode = 0
try {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$remoteScript" *> "$remoteOut"
    `$exitCode = `$LASTEXITCODE
    if (`$null -eq `$exitCode) { `$exitCode = 0 }
} catch {
    `$exitCode = 1
    (`$_ | Out-String) | Set-Content -LiteralPath "$remoteErr" -Encoding UTF8
}
[pscustomobject]@{
    done = `$true
    exitCode = `$exitCode
    host = `$env:COMPUTERNAME
    user = (whoami)
    finished = (Get-Date).ToString("o")
    script = "$remoteScript"
} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath "$remoteDone" -Encoding UTF8
exit `$exitCode
"@
    Set-Content -LiteralPath $uncWrapper -Value $wrapper -Encoding UTF8

    $start = (Get-Date).AddMinutes(1).ToString("HH:mm")
    $taskArgs = @(
        "/Create", "/S", $HostName, "/U", $Username, "/P", $Password,
        "/TN", $taskName,
        "/TR", "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$remoteWrapper`"",
        "/SC", "ONCE", "/ST", $start,
        "/RL", "HIGHEST",
        "/RU", $Username, "/RP", $Password,
        "/F"
    )
    if ($Interactive) {
        $taskArgs += "/IT"
    }
    $create = & schtasks.exe @taskArgs 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "schtasks create failed exit=$LASTEXITCODE. $($create -join ' ')"
    }

    $run = & schtasks.exe /Run /S $HostName /U $Username /P $Password /TN $taskName 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "schtasks run failed exit=$LASTEXITCODE. $($run -join ' ')"
    }

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        if (Test-Path -LiteralPath $uncDone) {
            $done = Get-Content -LiteralPath $uncDone -Raw | ConvertFrom-Json
            $outText = if (Test-Path -LiteralPath $uncOut) { Get-Content -LiteralPath $uncOut -Raw } else { "" }
            $errText = if (Test-Path -LiteralPath $uncErr) { Get-Content -LiteralPath $uncErr -Raw } else { "" }
            [pscustomobject]@{
                ok = ([int]$done.exitCode -eq 0)
                host = $HostName
                task = $taskName
                exitCode = [int]$done.exitCode
                finished = $done.finished
                remoteOut = $remoteOut
                remoteErr = $remoteErr
                remoteDone = $remoteDone
                stdout = $outText
                stderr = $errText
            } | ConvertTo-Json -Depth 5
            exit ([int]$done.exitCode)
        }
        Start-Sleep -Seconds 2
    }
    throw "Timed out waiting for $taskName after $TimeoutSec seconds."
} finally {
    & schtasks.exe /Delete /S $HostName /U $Username /P $Password /TN $taskName /F > $null 2>&1
    cmd /c "net use $share /delete /y >nul 2>nul" | Out-Null
}
