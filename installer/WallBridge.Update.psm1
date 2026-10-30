# wall-bridge autoupgrade decisions. autoupdate.ps1 runs them; Pester tests them.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Install only inside the night window [StartHour, EndHour) local time.
function Test-InWindow([datetime]$Now, [int]$StartHour = 3, [int]$EndHour = 5) {
    $Now.Hour -ge $StartHour -and $Now.Hour -lt $EndHour
}

# 'install' or the reason not to: patch-only on the same major.minor, never a downgrade.
function Get-UpdateDecision([string]$Current, [string]$Candidate) {
    if (-not $Candidate) { return 'no-release' }
    $c = [version]$Candidate
    if (-not $Current) { return 'install' }
    $i = [version]$Current
    if ($c -le $i) { return 'not-newer' }
    if ($c.Major -ne $i.Major -or $c.Minor -ne $i.Minor) { return 'not-a-patch' }
    'install'
}

# Version from a release asset name 'wall-bridge-<v>.zip'.
function Get-AssetVersion([string]$Name) {
    if ($Name -match '^wall-bridge-(\d+\.\d+\.\d+)\.zip$') { return $Matches[1] }
    $null
}

# SHA256SUMS line for the zip: '<hex>  <name>'.
function Get-ExpectedHash([string]$Sums, [string]$Name) {
    foreach ($line in $Sums -split "`n") {
        $p = $line.Trim() -split '\s+'
        if ($p.Count -eq 2 -and $p[1] -eq $Name) { return $p[0].ToLower() }
    }
    $null
}

# Healthy after an install: the new version answers and holds the wall.
function Test-Healthy($Status, [string]$Version) {
    $null -ne $Status -and $Status.version -eq $Version -and $Status.display.target_acquired -eq $true
}

# `[api] port` from config.toml (the engine's status port), default 8930.
function Get-ApiPort([string]$ConfigPath) {
    if (-not (Test-Path $ConfigPath)) { return 8930 }
    $section = ''
    foreach ($line in Get-Content $ConfigPath) {
        $l = ($line -replace '#.*$', '').Trim()
        if ($l -match '^\[(.+)\]$') { $section = $Matches[1].Trim(); continue }
        if ($section -eq 'api' -and $l -match '^port\s*=\s*(\d+)$') { return [int]$Matches[1] }
    }
    8930
}

Export-ModuleMember -Function Test-InWindow, Get-UpdateDecision, Get-AssetVersion, Get-ExpectedHash, Test-Healthy, Get-ApiPort
