<#
.SYNOPSIS
  E2E on the real SNV PC (self-hosted runner `snv-wall-bridge`, session 1 of `Resolume`):
  installs the CI bundle over the production install and checks that the wall comes back.
.EXAMPLE
  e2e/snv/verify.ps1 -Bundle snv-bundle
#>
param(
    [Parameter(Mandatory)] [string]$Bundle,
    [int]$OkWithinS = 180,
    [string]$Root = 'C:\ProgramData\WallBridge'
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Step([string]$m) { Write-Host ('{0:HH:mm:ss} {1}' -f (Get-Date), $m) }

# 1. The bundle as the updater sees it: zip + SHA256SUMS.
$sums = (Get-Content (Join-Path $Bundle 'SHA256SUMS') -Raw).Trim()
$hash, $name = $sums -split '\s+'
$zip = Join-Path $Bundle $name
$got = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
if ($got -ne $hash) { throw "SHA256 mismatch for ${name}: $got, SHA256SUMS $hash" }
$dir = Join-Path $env:RUNNER_TEMP ([IO.Path]::GetFileNameWithoutExtension($name))
if (Test-Path $dir) { Remove-Item $dir -Recurse -Force }
Expand-Archive $zip -DestinationPath $dir
$version = (Get-Content (Join-Path $dir 'VERSION') -Raw).Trim()
Step "bundle $name, version $version, sha ok"

# 2. Install over production (the real install path: tasks, rollback copy, restart).
& (Join-Path $dir 'install.ps1') -Source $dir
Step 'installed'

# 3. The installed engine reports this version and a healthy wall.
$deadline = (Get-Date).AddSeconds($OkWithinS)
$s = $null
while ((Get-Date) -lt $deadline) {
    try { $s = Invoke-RestMethod 'http://127.0.0.1:8930/api/status' -TimeoutSec 3 } catch { $s = $null }
    if ($s -and $s.version -eq $version -and $s.state -eq 'ok') { break }
    Start-Sleep -Seconds 3
}
if (-not $s -or $s.version -ne $version -or $s.state -ne 'ok') {
    throw "wall not ok within $OkWithinS s after install: $($s | ConvertTo-Json -Depth 6 -Compress)"
}
if (-not $s.display.specialized -or $s.display.width -ne 7680 -or -not $s.sender.connected) {
    throw "status ok but the wall is not live: $($s | ConvertTo-Json -Depth 6 -Compress)"
}
Step "status ok: version $($s.version), specialized, 7680, sender $($s.sender.name) connected"

# 4. The engine runs from this version's folder without a window on the operator's desktop.
$app = Join-Path $Root "app\$version"
$engine = @(Get-Process wall-bridge -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq (Join-Path $app 'wall-bridge.exe') })
if ($engine.Count -ne 1) { throw "expected one engine from $app, found $($engine.Count)" }
if ($engine[0].MainWindowHandle -ne [IntPtr]::Zero) { throw "engine has a window: '$($engine[0].MainWindowTitle)'" }
Step "engine pid $($engine[0].Id), no window"

# 5. The tray runs from this version's folder and its icon is promoted to the taskbar.
$trayExe = Join-Path $app 'wall-bridge-tray.exe'
$tray = @(Get-Process wall-bridge-tray -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $trayExe })
if ($tray.Count -ne 1) { throw "expected one tray from $app, found $($tray.Count)" }
$promoted = $null
$deadline = (Get-Date).AddSeconds(60)
while ((Get-Date) -lt $deadline) {
    $promoted = Get-ChildItem 'HKCU:\Control Panel\NotifyIconSettings' -ErrorAction SilentlyContinue |
        ForEach-Object { Get-ItemProperty $_.PSPath } |
        Where-Object { $_.ExecutablePath -eq $trayExe } |
        Select-Object -First 1
    if ($promoted -and $promoted.PSObject.Properties['IsPromoted'] -and $promoted.IsPromoted -eq 1) { break }
    Start-Sleep -Seconds 3
}
if (-not $promoted -or -not $promoted.PSObject.Properties['IsPromoted'] -or $promoted.IsPromoted -ne 1) {
    throw "tray icon of $trayExe is not promoted to the taskbar"
}
Step "tray pid $($tray[0].Id), icon promoted"

# 6. The dashboard serves this version.
$html = (Invoke-WebRequest 'http://127.0.0.1:8930/' -UseBasicParsing -TimeoutSec 5).Content
if ($html -notmatch [regex]::Escape("v$version")) { throw "dashboard does not show v$version" }
Step "dashboard shows v$version"
Step 'SNV E2E passed'
