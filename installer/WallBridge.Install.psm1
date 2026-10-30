# wall-bridge install logic. install.ps1 is the entry point; the functions live here so
# Pester tests them on the CI Windows runner.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:EngineTask = 'WallBridge Engine'
$script:WatchdogTask = 'WallBridge Watchdog'
$script:TrayTask = 'WallBridge Tray'
$script:UpdateTask = 'WallBridge Update'
$script:TaskPath = '\WallBridge\'

function Write-InstallLog([string]$Root, [string]$Message) {
    $line = '{0:yyyy-MM-ddTHH:mm:ss} {1}' -f (Get-Date), $Message
    Write-Host $line
    New-Item -ItemType Directory -Force (Join-Path $Root 'logs') | Out-Null
    Add-Content -Path (Join-Path $Root 'logs\install.log') -Value $line
}

# Versions to delete: everything except the current one and the rollback target (the
# version installed before, else the newest other one).
function Get-PrunableVersions([string[]]$Versions, [string]$Current, [string]$Previous = '') {
    $rollback = if ($Previous -and $Previous -ne $Current -and $Versions -contains $Previous) {
        $Previous
    } else {
        $Versions | Where-Object { $_ -ne $Current } | Sort-Object { [version]$_ } -Descending | Select-Object -First 1
    }
    $keep = @($Current, $rollback) | Where-Object { $_ }
    @($Versions | Where-Object { $keep -notcontains $_ })
}

function Get-DefaultConfig {
@'
# wall-bridge configuration. Every key is optional; these are the defaults.
# [api]
# port = 8930
# [sender]
# name = "Arena - Screen 2"
# [wall]
# monitor = "GSM779465795"
# width = 7680
# height = 1080
# [pacing]
# guard_us = 3500
# ring = 5
'@
}

function Install-AppFiles([string]$Source, [string]$Root, [string]$Version) {
    $dir = Join-Path $Root "app\$Version"
    New-Item -ItemType Directory -Force $dir | Out-Null
    $currentFile = Join-Path $Root 'current.txt'
    $previous = if (Test-Path $currentFile) { (Get-Content $currentFile -Raw).Trim() } else { '' }
    # A rollback installs from app\<version> itself: nothing to copy then.
    if ((Resolve-Path $Source).Path.TrimEnd('\') -ne (Resolve-Path $dir).Path.TrimEnd('\')) {
        Copy-Item (Join-Path $Source '*') $dir -Recurse -Force
    }
    Set-Content -Path $currentFile -Value $Version -NoNewline
    $config = Join-Path $Root 'config.toml'
    if (-not (Test-Path $config)) {
        Set-Content -Path $config -Value (Get-DefaultConfig)
        Write-InstallLog $Root "wrote default $config"
    }
    $versions = @(Get-ChildItem (Join-Path $Root 'app') -Directory | ForEach-Object Name)
    foreach ($old in Get-PrunableVersions -Versions $versions -Current $Version -Previous $previous) {
        Remove-Item (Join-Path $Root "app\$old") -Recurse -Force
        Write-InstallLog $Root "pruned app\$old"
    }
    $dir
}

# The engine runs in the desktop user's session: Spout names are session-local
# (docs/spikes/2026-10-session0.md).
function Register-EngineTask([string]$Exe, [string]$User) {
    $action = New-ScheduledTaskAction -Execute $Exe -WorkingDirectory (Split-Path $Exe)
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $User
    $principal = New-ScheduledTaskPrincipal -UserId $User -LogonType Interactive -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -StartWhenAvailable -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew `
        -RestartCount 9999 -RestartInterval (New-TimeSpan -Minutes 1)
    Register-ScheduledTask -TaskName $script:EngineTask -TaskPath $script:TaskPath -Action $action `
        -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
}

# A SYSTEM task that starts the engine task every minute when no engine process runs
# (covers a hang-free exit the restart settings do not see, e.g. a kill).
function Register-WatchdogTask([string]$Root) {
    $script = Join-Path $Root 'watchdog.ps1'
    Set-Content -Path $script -Value @"
`$ErrorActionPreference = 'Stop'
if (-not (Get-Process -Name 'wall-bridge' -ErrorAction SilentlyContinue)) {
    Add-Content -Path '$Root\logs\watchdog.log' -Value ('{0:yyyy-MM-ddTHH:mm:ss} engine not running, starting it' -f (Get-Date))
    Start-ScheduledTask -TaskPath '$($script:TaskPath)' -TaskName '$($script:EngineTask)'
}
"@
    $action = New-ScheduledTaskAction -Execute 'powershell.exe' `
        -Argument "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$script`""
    $trigger = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(1) `
        -RepetitionInterval (New-TimeSpan -Minutes 1)
    $principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Minutes 1) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $script:WatchdogTask -TaskPath $script:TaskPath -Action $action `
        -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
}

# The tray icon starts at the desktop user's logon (airuleset rust-web-tray). No restart
# policy: Exit in its menu is a deliberate choice.
function Register-TrayTask([string]$Exe, [string]$User) {
    $action = New-ScheduledTaskAction -Execute $Exe -WorkingDirectory (Split-Path $Exe)
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $User
    $principal = New-ScheduledTaskPrincipal -UserId $User -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $script:TrayTask -TaskPath $script:TaskPath -Action $action `
        -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
}

# Autoupgrade: SYSTEM, at startup and every hour (installs only 03:00-05:00, see autoupdate.ps1).
function Register-UpdateTask([string]$AppDir, [string]$Root, [string]$User) {
    $script = Join-Path $AppDir 'autoupdate.ps1'
    $action = New-ScheduledTaskAction -Execute 'powershell.exe' `
        -Argument "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$script`" -Root `"$Root`" -User `"$User`""
    $hourly = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(5) -RepetitionInterval (New-TimeSpan -Hours 1)
    $boot = New-ScheduledTaskTrigger -AtStartup
    $principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Minutes 30) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $script:UpdateTask -TaskPath $script:TaskPath -Action $action `
        -Trigger @($hourly, $boot) -Principal $principal -Settings $settings -Force | Out-Null
}

function Stop-Engines([string]$Root) {
    Stop-ScheduledTask -TaskPath $script:TaskPath -TaskName $script:EngineTask -ErrorAction SilentlyContinue
    foreach ($p in Get-Process -Name 'wall-bridge', 'wall-bridge-tray', 'wb2' -ErrorAction SilentlyContinue) {
        Write-InstallLog $Root "stopping $($p.ProcessName) pid $($p.Id)"
        try {
            Stop-Process -Id $p.Id -Force -ErrorAction Stop
        } catch {
            # It may exit on its own meanwhile (the single-instance exit of a second engine).
            if (Get-Process -Id $p.Id -ErrorAction SilentlyContinue) { throw }
            Write-InstallLog $Root "pid $($p.Id) already exited"
        }
    }
}

Export-ModuleMember -Function Get-PrunableVersions, Get-DefaultConfig, Install-AppFiles, `
    Register-EngineTask, Register-WatchdogTask, Register-TrayTask, Register-UpdateTask, Stop-Engines, `
    Write-InstallLog
