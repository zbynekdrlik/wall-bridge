<#
.SYNOPSIS
  Installs or upgrades wall-bridge from a folder holding wall-bridge.exe.
.EXAMPLE
  install.ps1 -Source C:\temp\wall-bridge-0.2.0
#>
param(
    [Parameter(Mandatory)] [string]$Source,
    [string]$Root = 'C:\ProgramData\WallBridge',
    [string]$User = 'Resolume',
    [switch]$NoStart
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'WallBridge.Install.psm1') -Force

$exeSrc = Join-Path $Source 'wall-bridge.exe'
if (-not (Test-Path $exeSrc)) { throw "no wall-bridge.exe in $Source" }
$version = (& $exeSrc --version).Trim()
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw "unexpected version output '$version'" }
Write-InstallLog $Root "installing $version from $Source"

Stop-Engines $Root
$dir = Install-AppFiles -Source $Source -Root $Root -Version $version
$exe = Join-Path $dir 'wall-bridge.exe'
Register-EngineTask -Exe $exe -User $User
Register-WatchdogTask -Root $Root
$tray = Join-Path $dir 'wall-bridge-tray.exe'
if (Test-Path $tray) { Register-TrayTask -Exe $tray -User $User }
if (Test-Path (Join-Path $dir 'autoupdate.ps1')) { Register-UpdateTask -AppDir $dir -Root $Root -User $User }
Write-InstallLog $Root "tasks registered for $exe (user $User)"

if (-not $NoStart) {
    Start-ScheduledTask -TaskPath '\WallBridge\' -TaskName 'WallBridge Engine'
    Write-InstallLog $Root 'engine task started'
    if (Test-Path $tray) {
        Start-ScheduledTask -TaskPath '\WallBridge\' -TaskName 'WallBridge Tray'
        Write-InstallLog $Root 'tray task started'
    }
}
