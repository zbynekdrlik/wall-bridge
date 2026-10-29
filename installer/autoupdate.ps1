<#
.SYNOPSIS
  wall-bridge autoupgrade (SYSTEM task, hourly). Downloads any time, installs only 03:00-05:00,
  rolls back when the new engine is not healthy within 30 s. Any error is logged, exit 0.
#>
param(
    [string]$Root = 'C:\ProgramData\WallBridge',
    [string]$Repo = 'zbynekdrlik/wall-bridge',
    [string]$User = 'Resolume',
    [datetime]$Now = (Get-Date)
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'WallBridge.Update.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'WallBridge.Install.psm1') -Force

function Log([string]$m) { Write-InstallLog $Root "update: $m" }

try {
    if (Test-Path (Join-Path $Root 'autoupdate.disabled')) { Log 'disabled by autoupdate.disabled'; exit 0 }
    $channel = 'stable'
    $chFile = Join-Path $Root 'channel.txt'
    if (Test-Path $chFile) { $channel = (Get-Content $chFile -Raw).Trim() }
    $api = if ($channel -eq 'dev') { "https://api.github.com/repos/$Repo/releases/tags/dev-latest" } else { "https://api.github.com/repos/$Repo/releases/latest" }
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $rel = Invoke-RestMethod $api -Headers @{ 'User-Agent' = 'wall-bridge-autoupdate' } -TimeoutSec 30
    $zip = @($rel.assets | Where-Object { Get-AssetVersion $_.name }) | Select-Object -First 1
    $sums = @($rel.assets | Where-Object { $_.name -eq 'SHA256SUMS' }) | Select-Object -First 1
    if (-not $zip -or -not $sums) { Log "release $($rel.tag_name) has no zip/SHA256SUMS"; exit 0 }
    $candidate = Get-AssetVersion $zip.name
    $currentFile = Join-Path $Root 'current.txt'
    $current = if (Test-Path $currentFile) { (Get-Content $currentFile -Raw).Trim() } else { '' }
    $decision = Get-UpdateDecision -Current $current -Candidate $candidate
    if ($decision -ne 'install') { Log "channel $channel current $current candidate $candidate -> $decision"; exit 0 }

    $dl = Join-Path $Root 'downloads'
    New-Item -ItemType Directory -Force $dl | Out-Null
    $zipPath = Join-Path $dl $zip.name
    if (-not (Test-Path $zipPath)) {
        Invoke-WebRequest $zip.browser_download_url -OutFile "$zipPath.part" -UseBasicParsing -TimeoutSec 300
        Move-Item "$zipPath.part" $zipPath -Force
        Log "downloaded $($zip.name)"
    }
    $sumText = (Invoke-WebRequest $sums.browser_download_url -UseBasicParsing -TimeoutSec 30).Content
    if ($sumText -is [byte[]]) { $sumText = [Text.Encoding]::UTF8.GetString($sumText) }
    $want = Get-ExpectedHash -Sums $sumText -Name $zip.name
    $got = (Get-FileHash $zipPath -Algorithm SHA256).Hash.ToLower()
    if ($want -ne $got) { Log "hash mismatch for $($zip.name): want $want got $got, deleting"; Remove-Item $zipPath -Force; exit 0 }

    if (-not (Test-InWindow $Now)) { Log "$candidate downloaded and verified, waiting for the 03:00-05:00 window"; exit 0 }

    $src = Join-Path $dl "wall-bridge-$candidate"
    if (Test-Path $src) { Remove-Item $src -Recurse -Force }
    Expand-Archive $zipPath $src
    Log "installing $candidate over $current"
    & (Join-Path $src 'install.ps1') -Source $src -Root $Root -User $User

    $ok = $false
    foreach ($i in 1..15) {
        Start-Sleep 2
        try {
            $st = Invoke-RestMethod 'http://127.0.0.1:8930/api/status' -TimeoutSec 2
            if (Test-Healthy $st $candidate) { $ok = $true; break }
        } catch { }
    }
    if ($ok) { Log "$candidate healthy"; exit 0 }

    Log "$candidate not healthy within 30 s, rolling back to $current"
    $prev = Join-Path $Root "app\$current"
    if ($current -and (Test-Path $prev)) {
        & (Join-Path $PSScriptRoot 'install.ps1') -Source $prev -Root $Root -User $User
        Log "rolled back to $current"
    } else {
        Log "no previous version to roll back to"
    }
} catch {
    Log "error: $($_.Exception.Message)"
}
exit 0
