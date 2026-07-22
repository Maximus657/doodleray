param(
    # Optional local installer to publish first; otherwise the RC already at
    # C:\DoodleRayQA\artifacts\DoodleRay-v6-rc-setup.exe is used.
    [string] $LocalInstaller,
    [switch] $AllowUnsignedLocalRc,
    [switch] $SkipUpdatePath,
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md")
)

# One-command full QA pass for a (possibly fresh) Windows QA stand.
# Chains the committed harnesses in dependency order and stops on the first
# failing stage. Prerequisites on a fresh stand: OpenSSH server reachable with
# the fields in secrets/doodlevpn-server-access.md, an interactive admin
# session (autologon or connected RDP session left logged in), and the
# an interactive admin session (autologon or connected RDP session left logged
# in). Everything else (QA dirs, CDP launcher, CDP scheduled task, canonical
# subscription import through ignored secrets) is bootstrapped here.

$ErrorActionPreference = "Stop"

function Invoke-Stage {
    param([string] $Name, [scriptblock] $Action)
    Write-Host "=== STAGE: $Name ==="
    # A stage that succeeds without running a native command would otherwise
    # inherit a stale $LASTEXITCODE from a previous stage and false-fail.
    $global:LASTEXITCODE = 0
    & $Action
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Stage failed: $Name (exit $LASTEXITCODE)"
        exit $LASTEXITCODE
    }
}

$unsignedArgs = @{}
if ($AllowUnsignedLocalRc) { $unsignedArgs.AllowUnsignedLocalRc = $true }

# --- Stage 0: bootstrap stand QA scaffolding --------------------------------
$bootstrap = @'
$ErrorActionPreference = "Continue"
New-Item -ItemType Directory -Force -Path C:\DoodleRayQA\artifacts, C:\DoodleRayQA\evidence | Out-Null
$qaToken = [Guid]::NewGuid().ToString("N")
Set-Content -Path "C:\DoodleRayQA\qa-control-token.txt" -Value $qaToken -Encoding ASCII
$cmdPath = "C:\DoodleRayQA\start-doodleray-cdp.cmd"
# Always rewrite: the launcher must enable both CDP (visual smoke) and the
# QA control surface (primary automation channel) on the stand.
$lines = @(
    "@echo off",
    "set `"WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333 --remote-allow-origins=*`"",
    "set `"DOODLERAY_QA_CONTROL=1`"",
    "set `"DOODLERAY_QA_TOKEN=$qaToken`"",
    "start `"`" /D `"C:\Program Files\DoodleRay`" `"C:\Program Files\DoodleRay\DoodleRay.exe`""
)
Set-Content -Path $cmdPath -Value $lines -Encoding ASCII
if (-not (Get-ScheduledTask -TaskName "DoodleRayCodexCDP" -ErrorAction SilentlyContinue)) {
    $action = New-ScheduledTaskAction -Execute $cmdPath
    $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Highest
    Register-ScheduledTask -TaskName "DoodleRayCodexCDP" -Action $action -Principal $principal -Force | Out-Null
}
[pscustomobject]@{
    ok = $true
    cdpCmd = (Test-Path $cmdPath)
    cdpTask = [bool](Get-ScheduledTask -TaskName "DoodleRayCodexCDP" -ErrorAction SilentlyContinue)
} | ConvertTo-Json
'@
Invoke-Stage "bootstrap-stand" {
    & (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $bootstrap -SecretPath $SecretPath
}

# --- Stage 1: publish installer (optional) ----------------------------------
if ($LocalInstaller) {
    Invoke-Stage "publish-installer" {
        & (Join-Path $PSScriptRoot "Publish-DoodleRayQaInstaller.ps1") -LocalInstaller $LocalInstaller -SecretPath $SecretPath
    }
}

# --- Stage 2: install gate with stale WinINet injection ----------------------
Invoke-Stage "install-gate" {
    & (Join-Path $PSScriptRoot "Invoke-DoodleRayV6QaGate.ps1") -InjectStaleWinInet @unsignedArgs -SecretPath $SecretPath
}

# --- Stage 3: unclean-shutdown marker crash simulation -----------------------
Invoke-Stage "unclean-shutdown-marker" {
    & (Join-Path $PSScriptRoot "Test-DoodleRayUncleanShutdownMarker.ps1") -SecretPath $SecretPath
}

# --- Stage 4: previous-version update paths ----------------------------------
if (-not $SkipUpdatePath) {
    foreach ($from in @("5.4.3", "5.4.4")) {
        Invoke-Stage "update-path-$from" {
            & (Join-Path $PSScriptRoot "Invoke-DoodleRayUpdatePathQa.ps1") -FromVersion $from @unsignedArgs -SecretPath $SecretPath
        }
    }
    Invoke-Stage "update-path-5.4.5-broken-state" {
        & (Join-Path $PSScriptRoot "Invoke-DoodleRayUpdatePathQa.ps1") -FromVersion 5.4.5 -InjectStaleWinInet -InjectCorporatePac @unsignedArgs -SecretPath $SecretPath
    }
    Invoke-Stage "update-path-5.9.1-current-production" {
        & (Join-Path $PSScriptRoot "Invoke-DoodleRayUpdatePathQa.ps1") -FromVersion 5.9.1 -InjectStaleWinInet -InjectCorporatePac @unsignedArgs -SecretPath $SecretPath
    }
}

# --- Stage 5: active-VPN-during-update ---------------------------------------
Invoke-Stage "import-subscription-before-active-update" {
    & (Join-Path $PSScriptRoot "Import-DoodleRayQaSubscription.ps1") -SecretPath $SecretPath
}

Invoke-Stage "active-vpn-during-update" {
    & (Join-Path $PSScriptRoot "Invoke-DoodleRayActiveUpdateQa.ps1") @unsignedArgs -SecretPath $SecretPath
}

# --- Stage 6: full UI pass over CDP ------------------------------------------
Invoke-Stage "import-subscription-before-ui-pass" {
    & (Join-Path $PSScriptRoot "Import-DoodleRayQaSubscription.ps1") -SecretPath $SecretPath
}

Invoke-Stage "rc-ui-cdp-pass" {
    & (Join-Path $PSScriptRoot "Invoke-DoodleRayRc3UiCdpPass.ps1") -SecretPath $SecretPath
}

Invoke-Stage "dual-stack-split-routing" {
    & (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -ScriptPath (Join-Path $PSScriptRoot "Invoke-DoodleRaySplitRoutingDnsQa.ps1") -SecretPath $SecretPath
}

# --- Stage 7: targeted reliability scenarios ---------------------------------
Invoke-Stage "stale-state-repair" {
    & (Join-Path $PSScriptRoot "Test-DoodleRayStaleStateRepair.ps1") -SecretPath $SecretPath
}

Invoke-Stage "auto-fallback-protected-to-browsers" {
    & (Join-Path $PSScriptRoot "Test-DoodleRayAutoFallback.ps1") -SecretPath $SecretPath
}

# --- Stage 8: deep snapshot baseline ------------------------------------------
Invoke-Stage "deep-snapshot" {
    & (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -ScriptPath (Join-Path $PSScriptRoot "Get-DoodleRayDeepQaSnapshot.ps1") -SecretPath $SecretPath
}

Write-Host "=== FULL STAND QA COMPLETE ==="
Write-Host "Evidence on stand: C:\DoodleRayQA\evidence (redact before committing summaries)."
exit 0
