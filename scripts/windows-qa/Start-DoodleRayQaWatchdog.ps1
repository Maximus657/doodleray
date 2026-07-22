param(
    [int] $TimeoutSeconds = 600
)

$payload = @"
Start-Sleep -Seconds $TimeoutSeconds
try {
    & "C:\Program Files\DoodleRay\DoodleRayService.exe" stop | Out-Null
} catch {}
try {
    Stop-Process -Name xray,sing-box -Force -ErrorAction SilentlyContinue
} catch {}
try {
    `$key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings"
    Set-ItemProperty -Path `$key -Name ProxyEnable -Value 0 -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path `$key -Name ProxyServer -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path `$key -Name ProxyOverride -ErrorAction SilentlyContinue
} catch {}
"@

$encodedPayload = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($payload))
Start-Process -FilePath powershell.exe `
    -ArgumentList @("-NoProfile", "-EncodedCommand", $encodedPayload) `
    -WindowStyle Hidden

[pscustomobject]@{
    watchdogStarted = $true
    timeoutSeconds = $TimeoutSeconds
} | ConvertTo-Json

