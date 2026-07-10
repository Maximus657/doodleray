<#
Targeted repro/verification harness for the ERROR_PIPE_BUSY (Win32 231)
"tunnel service pipe" failure ("FULL COMPUTER COMPONENTS NOT INSTALLED OR NOT
READY... ALL PIPE INSTANCES ARE BUSY").

Named pipes have a fixed pool of server instances (PIPE_WORKERS in
src-tauri/src/bin/service.rs). CreateFile fails immediately with
ERROR_PIPE_BUSY when every instance is busy - it does not queue like a socket
connect. This harness fires bursts of concurrent raw pipe clients at the
service pipe and reports how many hit ERROR_PIPE_BUSY, using two client
strategies:

  -ClientMode Legacy : one CreateFile attempt, no wait - matches the
                       pre-fix src-tauri/src/ipc.rs client exactly.
  -ClientMode Fixed  : CreateFile, and on ERROR_PIPE_BUSY calls WaitNamedPipe
                       before retrying - matches the fixed client.

Run this on the Play2Go stand (or a clean VM) against a real installed
DoodleRayTunnelService, not on a developer machine:

  .\scripts\windows-qa\Invoke-Play2GoPowerShell.ps1 `
      -ScriptPath .\scripts\windows-qa\Test-DoodleRayPipeBusyRepro.ps1
#>
param(
    [ValidateSet("Legacy", "Fixed")]
    [string] $ClientMode = "Fixed",

    [int] $Concurrency = 24,
    [int] $Rounds = 5,
    [int] $HoldMs = 40,
    [string] $PipeName = "\\.\pipe\DoodleRay.TunnelService.v1"
)

$ErrorActionPreference = "Stop"

Add-Type -Namespace DoodleRayQa -Name PipeNative -MemberDefinition @"
[System.Runtime.InteropServices.DllImport("kernel32.dll", SetLastError = true, CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern Microsoft.Win32.SafeHandles.SafeFileHandle CreateFile(
    string lpFileName, uint dwDesiredAccess, uint dwShareMode, System.IntPtr lpSecurityAttributes,
    uint dwCreationDisposition, uint dwFlagsAndAttributes, System.IntPtr hTemplateFile);

[System.Runtime.InteropServices.DllImport("kernel32.dll", SetLastError = true, CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern bool WaitNamedPipe(string lpNamedPipeName, uint nTimeOut);
"@

# Sanity check: fail fast with a clear message instead of a wall of per-client
# errors if the service pipe does not exist at all.
$probe = [DoodleRayQa.PipeNative]::CreateFile($PipeName, [uint32]0x80000000, [uint32]0, [System.IntPtr]::Zero, [uint32]3, [uint32]0, [System.IntPtr]::Zero)
if ($probe.IsInvalid) {
    $probeErr = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
    if ($probeErr -ne 231) {
        throw "Cannot reach $PipeName (Win32 error $probeErr) - is DoodleRayTunnelService running?"
    }
}
else {
    $probe.Dispose()
}

$clientScript = {
    param($Mode, $PipeName, $HoldMs)

    [uint32] $GENERIC_READ = 0x80000000
    [uint32] $GENERIC_WRITE = 0x40000000
    [uint32] $OPEN_EXISTING = 3
    $ERROR_PIPE_BUSY = 231
    $deadline = (Get-Date).AddSeconds(5)
    $sw = [System.Diagnostics.Stopwatch]::StartNew()

    while ($true) {
        $handle = [DoodleRayQa.PipeNative]::CreateFile(
            $PipeName, ($GENERIC_READ -bor $GENERIC_WRITE), [uint32]0, [System.IntPtr]::Zero,
            $OPEN_EXISTING, [uint32]0, [System.IntPtr]::Zero)

        if (-not $handle.IsInvalid) {
            try {
                Start-Sleep -Milliseconds $HoldMs
                $stream = New-Object System.IO.FileStream($handle, [System.IO.FileAccess]::ReadWrite, 1, $false)
                try {
                    $payload = [System.Text.Encoding]::UTF8.GetBytes('{"type":"get_status"}')
                    $stream.Write([BitConverter]::GetBytes([uint32]$payload.Length), 0, 4)
                    $stream.Write($payload, 0, $payload.Length)
                    $stream.Flush()

                    $lenBuf = New-Object byte[] 4
                    $read = 0
                    while ($read -lt 4) {
                        $n = $stream.Read($lenBuf, $read, 4 - $read)
                        if ($n -le 0) { throw "pipe closed while reading response length" }
                        $read += $n
                    }
                    $respLen = [BitConverter]::ToUInt32($lenBuf, 0)
                    $respBuf = New-Object byte[] $respLen
                    $read = 0
                    while ($read -lt $respLen) {
                        $n = $stream.Read($respBuf, $read, $respLen - $read)
                        if ($n -le 0) { throw "pipe closed while reading response body" }
                        $read += $n
                    }
                    return [pscustomobject]@{
                        success    = $true
                        win32Error = 0
                        elapsedMs  = $sw.Elapsed.TotalMilliseconds
                        waited     = ($sw.Elapsed.TotalMilliseconds -gt ($HoldMs + 75))
                    }
                } finally {
                    $stream.Dispose()
                }
            } finally {
                $handle.Dispose()
            }
        }

        $err = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()

        if ($Mode -eq "Fixed" -and $err -eq $ERROR_PIPE_BUSY) {
            $remainingMs = [Math]::Max(0, ($deadline - (Get-Date)).TotalMilliseconds)
            if ($remainingMs -le 0) {
                return [pscustomobject]@{ success = $false; win32Error = $err; elapsedMs = $sw.Elapsed.TotalMilliseconds; waited = $true }
            }
            [void][DoodleRayQa.PipeNative]::WaitNamedPipe($PipeName, [uint32]$remainingMs)
            continue
        }

        return [pscustomobject]@{ success = $false; win32Error = $err; elapsedMs = $sw.Elapsed.TotalMilliseconds; waited = $false }
    }
}

Write-Host "DoodleRay pipe-busy repro harness: mode=$ClientMode concurrency=$Concurrency rounds=$Rounds pipe=$PipeName"

$allResults = New-Object System.Collections.Generic.List[object]
$clientScriptText = $clientScript.ToString()

for ($round = 1; $round -le $Rounds; $round++) {
    $pool = [runspacefactory]::CreateRunspacePool(1, $Concurrency)
    $pool.Open()
    $handles = New-Object System.Collections.Generic.List[object]

    for ($i = 0; $i -lt $Concurrency; $i++) {
        $ps = [powershell]::Create()
        $ps.RunspacePool = $pool
        [void]$ps.AddScript($clientScriptText).AddArgument($ClientMode).AddArgument($PipeName).AddArgument($HoldMs)
        $handles.Add([pscustomobject]@{ Pipeline = $ps; Async = $ps.BeginInvoke() })
    }

    foreach ($h in $handles) {
        $result = $h.Pipeline.EndInvoke($h.Async)
        $allResults.Add($result)
        $h.Pipeline.Dispose()
    }
    $pool.Close()
    $pool.Dispose()

    $roundResults = $allResults[($allResults.Count - $Concurrency)..($allResults.Count - 1)]
    $roundFail = ($roundResults | Where-Object { -not $_.success }).Count
    $roundBusy = ($roundResults | Where-Object { $_.win32Error -eq 231 }).Count
    Write-Host ("round {0}: {1}/{2} failed, {3} raw ERROR_PIPE_BUSY" -f $round, $roundFail, $Concurrency, $roundBusy)
}

$totalFail = ($allResults | Where-Object { -not $_.success }).Count
$totalBusy = ($allResults | Where-Object { $_.win32Error -eq 231 }).Count
$totalWaited = ($allResults | Where-Object { $_.waited }).Count

[pscustomobject]@{
    clientMode                    = $ClientMode
    concurrency                   = $Concurrency
    rounds                        = $Rounds
    totalAttempts                 = $allResults.Count
    totalFailures                 = $totalFail
    totalErrorPipeBusy            = $totalBusy
    clientsThatWaitedAndSucceeded = $totalWaited
    maxElapsedMs                  = ($allResults | Measure-Object -Property elapsedMs -Maximum).Maximum
} | ConvertTo-Json
