param(
  [switch]$KeepProxyServer
)

$ErrorActionPreference = "Stop"

$internetSettings = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings"

Set-ItemProperty -Path $internetSettings -Name ProxyEnable -Type DWord -Value 0

if (-not $KeepProxyServer) {
  foreach ($name in @("ProxyServer", "ProxyOverride", "AutoConfigURL")) {
    Remove-ItemProperty -Path $internetSettings -Name $name -ErrorAction SilentlyContinue
  }
}

$signature = @"
using System;
using System.Runtime.InteropServices;

public static class WinInetNotify {
  [DllImport("wininet.dll", SetLastError = true)]
  public static extern bool InternetSetOption(IntPtr hInternet, int dwOption, IntPtr lpBuffer, int dwBufferLength);
}
"@

if (-not ([System.Management.Automation.PSTypeName]"WinInetNotify").Type) {
  Add-Type -TypeDefinition $signature
}

# INTERNET_OPTION_SETTINGS_CHANGED = 39, INTERNET_OPTION_REFRESH = 37.
[void][WinInetNotify]::InternetSetOption([IntPtr]::Zero, 39, [IntPtr]::Zero, 0)
[void][WinInetNotify]::InternetSetOption([IntPtr]::Zero, 37, [IntPtr]::Zero, 0)

Write-Host "WinINet proxy disabled for current user."
