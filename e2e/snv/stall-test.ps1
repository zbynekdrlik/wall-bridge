<#
.SYNOPSIS
  A/B/A Arena stall test on the real SNV PC (C8, ticket #2). Runs in Resolume's session 1.
  A: the wall is live. B: the wall is put back on the Windows desktop, as Windows does after a
  topology change; the keeper must take it off again by itself. A: the wall is live again.
  Arena must keep running through B: it answers its REST API, its process responds, and new
  pictures flow at once after the wall is back. The wall shows the desktop for about a minute.
.EXAMPLE
  e2e/snv/stall-test.ps1
#>
param(
    [string]$Root = 'C:\ProgramData\WallBridge',
    [int]$BackWithinS = 180
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Step([string]$m) { Write-Host ('{0:HH:mm:ss} {1}' -f (Get-Date), $m) }
function Status { Invoke-RestMethod 'http://127.0.0.1:8930/api/status' -TimeoutSec 3 }
function ArenaAlive {
    $p = @(Get-Process Arena -ErrorAction SilentlyContinue)
    if ($p.Count -ne 1) { return "Arena processes: $($p.Count)" }
    if (-not $p[0].Responding) { return "Arena pid $($p[0].Id) not responding" }
    $t = [Diagnostics.Stopwatch]::StartNew()
    try { Invoke-RestMethod 'http://127.0.0.1:8090/api/v1/product' -TimeoutSec 3 | Out-Null } catch { return "Arena REST: $_" }
    if ($t.ElapsedMilliseconds -gt 2000) { return "Arena REST took $($t.ElapsedMilliseconds) ms" }
    return $null
}

# A
$s = Status
if ($s.state -ne 'ok') { throw "A: wall not ok before the test: $($s | ConvertTo-Json -Depth 6 -Compress)" }
$arena = (Get-Process Arena).Id
$bad = ArenaAlive
if ($bad) { throw "A: $bad" }
Step "A: ok, Arena pid $arena"

# B
$current = (Get-Content (Join-Path $Root 'current.txt') -Raw).Trim()
$toggle = Join-Path $Root "app\$current\tools\spec_toggle.ps1"
$started = Get-Date
$startedMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $toggle -Want off
Get-Process SystemSettings -ErrorAction SilentlyContinue | Where-Object { $_.StartTime -ge $started } | Stop-Process -Force
Step 'B: wall put back on the desktop'
$deadline = (Get-Date).AddSeconds($BackWithinS)
$sawDesktop = $false
$back = $null
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 2
    $bad = ArenaAlive
    if ($bad) { throw "B: $bad" }
    try { $s = Status } catch { continue }
    if (-not $s.display.specialized) { $sawDesktop = $true }
    # Back only after the keeper's own restore: a single sample can still show the old state.
    $restored = $s.topology.last_restore_ms -and $s.topology.last_restore_ms -gt $startedMs
    if ($sawDesktop -and $restored -and $s.state -eq 'ok' -and $s.display.specialized -and $s.sender.last_picture_age_ms -lt 1000) { $back = Get-Date; break }
}
if (-not $sawDesktop) { throw 'B: the wall never showed as on the desktop' }
if (-not $back) { throw "B: wall not back within $BackWithinS s: $($s | ConvertTo-Json -Depth 6 -Compress)" }
Step ("B: keeper took the wall off the desktop again, ok after {0:N0} s" -f ($back - $started).TotalSeconds)

# A
if ((Get-Process Arena).Id -ne $arena) { throw "A: Arena was restarted (pid $arena -> $((Get-Process Arena).Id))" }
$w0 = (Status).pacing
Start-Sleep -Seconds 12
$s = Status
$bad = ArenaAlive
if ($bad) { throw "A: $bad" }
if ($s.state -ne 'ok' -or -not $s.display.specialized -or $s.display.width -ne 7680 -or $s.pacing.new -lt 100) {
    throw "A: not live after the test: $($s | ConvertTo-Json -Depth 6 -Compress)"
}
Step "A: ok, specialized 7680, $($s.pacing.new) new pictures / window, missed $($s.pacing.missed_vblanks), Arena pid $arena unchanged"
Step 'A/B/A stall test passed'
