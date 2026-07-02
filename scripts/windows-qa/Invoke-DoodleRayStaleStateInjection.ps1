param(
    [ValidateSet("inject", "cleanup")]
    [string] $Action = "inject",

    [switch] $WinInet,
    [switch] $Nrpt,
    [switch] $Routes,
    [string] $SecretPath = (Join-Path $PSScriptRoot "..\..\secrets\doodlevpn-server-access.md")
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

if (-not $WinInet -and -not $Nrpt -and -not $Routes) {
    $WinInet = $true
    $Nrpt = $true
    $Routes = $true
}

$winInetLiteral = if ($WinInet) { '$true' } else { '$false' }
$nrptLiteral = if ($Nrpt) { '$true' } else { '$false' }
$routesLiteral = if ($Routes) { '$true' } else { '$false' }

$remoteScript = @"
`$ErrorActionPreference = "Continue"
`$ProgressPreference = "SilentlyContinue"
`$action = "$Action"
`$result = [ordered]@{
  action = `$action
  winInet = "skipped"
  nrpt = "skipped"
  routes = "skipped"
}

if ($winInetLiteral) {
  `$key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings"
  if (`$action -eq "inject") {
    Set-ItemProperty -Path `$key -Name ProxyEnable -Value 1 -ErrorAction SilentlyContinue
    Set-ItemProperty -Path `$key -Name ProxyServer -Value "http=127.0.0.1:9;https=127.0.0.1:9" -ErrorAction SilentlyContinue
    Set-ItemProperty -Path `$key -Name ProxyOverride -Value "<local>;*.local" -ErrorAction SilentlyContinue
    `$result.winInet = "injected"
  } else {
    Set-ItemProperty -Path `$key -Name ProxyEnable -Value 0 -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path `$key -Name ProxyServer -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path `$key -Name ProxyOverride -ErrorAction SilentlyContinue
    `$result.winInet = "cleaned"
  }
}

if ($nrptLiteral -and (Get-Command Add-DnsClientNrptRule -ErrorAction SilentlyContinue)) {
  if (`$action -eq "inject") {
    try {
      Add-DnsClientNrptRule -Namespace ".doodleray-stale-test.invalid" -NameServers "127.0.0.1" -Comment "DoodleRay stale QA test" -ErrorAction Stop
      `$result.nrpt = "injected"
    } catch {
      `$result.nrpt = "inject_failed: `$(`$_.Exception.Message)"
    }
  } else {
    `$removed = 0
    `$rules = @(Get-DnsClientNrptRule -ErrorAction SilentlyContinue | Where-Object {
      (`$_.Namespace -match 'doodleray-stale-test') -or (`$_.Comment -match 'DoodleRay stale QA test')
    })
    foreach (`$rule in `$rules) {
      try {
        Remove-DnsClientNrptRule -Name `$rule.Name -Force -ErrorAction Stop
        `$removed += 1
      } catch {}
    }
    `$result.nrpt = "cleaned:`$removed"
  }
}

if ($routesLiteral) {
  `$adapter = Get-NetAdapter -Name "DoodleRay Tunnel" -ErrorAction SilentlyContinue | Select-Object -First 1
  if (`$adapter) {
    if (`$action -eq "inject") {
      try {
        New-NetRoute -InterfaceIndex `$adapter.ifIndex -DestinationPrefix "203.0.113.0/24" -NextHop "0.0.0.0" -RouteMetric 9999 -PolicyStore ActiveStore -ErrorAction Stop | Out-Null
        `$result.routes = "injected"
      } catch {
        `$result.routes = "inject_failed: `$(`$_.Exception.Message)"
      }
    } else {
      `$removed = 0
      `$routes = @(Get-NetRoute -InterfaceIndex `$adapter.ifIndex -DestinationPrefix "203.0.113.0/24" -ErrorAction SilentlyContinue)
      foreach (`$route in `$routes) {
        try {
          Remove-NetRoute -InterfaceIndex `$adapter.ifIndex -DestinationPrefix `$route.DestinationPrefix -NextHop `$route.NextHop -Confirm:`$false -ErrorAction Stop
          `$removed += 1
        } catch {}
      }
      `$result.routes = "cleaned:`$removed"
    }
  } else {
    `$result.routes = "adapter_absent"
  }
}

[pscustomobject]`$result | ConvertTo-Json -Depth 8
"@

& (Join-Path $PSScriptRoot "Invoke-Play2GoPowerShell.ps1") -Command $remoteScript -SecretPath $SecretPath
exit $LASTEXITCODE
