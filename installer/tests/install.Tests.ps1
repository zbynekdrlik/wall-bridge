BeforeAll {
    # These tests register and delete tasks under \WallBridge\: never on a box with an install.
    if (Test-Path 'C:\ProgramData\WallBridge\current.txt') { throw 'refusing to run on a machine with wall-bridge installed' }
    Import-Module (Join-Path $PSScriptRoot '..\WallBridge.Install.psm1') -Force
}

Describe 'Get-PrunableVersions' {
    It 'keeps the current and the previous version' {
        $r = Get-PrunableVersions -Versions @('0.1.9', '0.2.0', '0.2.1', '0.10.0') -Current '0.10.0' -Previous '0.2.0'
        $r | Sort-Object | Should -Be @('0.1.9', '0.2.1')
    }
    It 'keeps the newest other version when there is no previous' {
        $r = Get-PrunableVersions -Versions @('0.2.0', '0.2.1', '0.2.2') -Current '0.2.1'
        $r | Should -Be @('0.2.0')
    }
    It 'prunes nothing with one version' {
        @(Get-PrunableVersions -Versions @('0.2.0') -Current '0.2.0').Count | Should -Be 0
    }
}

Describe 'Install-AppFiles' {
    BeforeEach {
        $root = Join-Path $TestDrive ([guid]::NewGuid())
        $src = Join-Path $TestDrive 'src'
        New-Item -ItemType Directory -Force $src | Out-Null
        Set-Content (Join-Path $src 'wall-bridge.exe') 'x'
    }
    It 'keeps_previous_version' {
        Install-AppFiles -Source $src -Root $root -Version '0.2.0' | Out-Null
        Install-AppFiles -Source $src -Root $root -Version '0.2.1' | Out-Null
        Install-AppFiles -Source $src -Root $root -Version '0.2.2' | Out-Null
        (Get-ChildItem (Join-Path $root 'app') -Directory).Name | Sort-Object | Should -Be @('0.2.1', '0.2.2')
        Get-Content (Join-Path $root 'current.txt') | Should -Be '0.2.2'
    }
    It 'config_not_overwritten' {
        New-Item -ItemType Directory -Force $root | Out-Null
        Set-Content (Join-Path $root 'config.toml') "[api]`nport = 9000"
        Install-AppFiles -Source $src -Root $root -Version '0.2.0' | Out-Null
        Get-Content (Join-Path $root 'config.toml') -Raw | Should -Match 'port = 9000'
    }
    It 'reinstalls from its own app folder (rollback)' {
        # Review C1: rollback runs install.ps1 -Source app\<current> into the same Root.
        Install-AppFiles -Source $src -Root $root -Version '0.2.0' | Out-Null
        $own = Join-Path $root 'app\0.2.0'
        { Install-AppFiles -Source $own -Root $root -Version '0.2.0' } | Should -Not -Throw
        Test-Path (Join-Path $own 'wall-bridge.exe') | Should -BeTrue
    }
    It 'keeps the version it came from as the rollback' {
        Install-AppFiles -Source $src -Root $root -Version '0.2.5' | Out-Null
        Install-AppFiles -Source $src -Root $root -Version '0.2.1' | Out-Null
        Install-AppFiles -Source $src -Root $root -Version '0.2.3' | Out-Null
        (Get-ChildItem (Join-Path $root 'app') -Directory).Name | Sort-Object | Should -Be @('0.2.1', '0.2.3')
    }
    It 'writes a default config when missing' {
        Install-AppFiles -Source $src -Root $root -Version '0.2.0' | Out-Null
        Get-Content (Join-Path $root 'config.toml') -Raw | Should -Match '# port = 8930'
    }
}

Describe 'Register-EngineTask' {
    AfterAll {
        Unregister-ScheduledTask -TaskPath '\WallBridge\' -TaskName 'WallBridge Engine' -Confirm:$false -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskPath '\WallBridge\' -TaskName 'WallBridge Watchdog' -Confirm:$false -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskPath '\WallBridge\' -TaskName 'WallBridge Tray' -Confirm:$false -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskPath '\WallBridge\' -TaskName 'WallBridge Update' -Confirm:$false -ErrorAction SilentlyContinue
    }
    It 'update task runs hourly and at startup as SYSTEM' {
        Register-UpdateTask -AppDir 'C:\x\app\0.2.0' -Root 'C:\x' -User $env:USERNAME
        $t = Get-ScheduledTask -TaskPath '\WallBridge\' -TaskName 'WallBridge Update'
        $t.Principal.UserId | Should -Be 'SYSTEM'
        @($t.Triggers).Count | Should -Be 2
        ($t.Triggers | ForEach-Object { $_.CimClass.CimClassName }) | Should -Contain 'MSFT_TaskBootTrigger'
        $t.Triggers[0].Repetition.Interval | Should -Be 'PT1H'
        $t.Actions[0].Arguments | Should -Match 'autoupdate.ps1'
    }
    It 'tray starts at logon, no restart policy' {
        Register-TrayTask -Exe 'C:\Windows\System32\cmd.exe' -User $env:USERNAME
        $t = Get-ScheduledTask -TaskPath '\WallBridge\' -TaskName 'WallBridge Tray'
        $t.Triggers[0].CimClass.CimClassName | Should -Be 'MSFT_TaskLogonTrigger'
        $t.Principal.LogonType | Should -Be 'Interactive'
        $t.Settings.RestartCount | Should -Be 0
    }
    It 'task_registered_with_restart' {
        Register-EngineTask -Exe 'C:\Windows\System32\cmd.exe' -User $env:USERNAME
        $t = Get-ScheduledTask -TaskPath '\WallBridge\' -TaskName 'WallBridge Engine'
        $t.Settings.RestartCount | Should -Be 9999
        $t.Settings.RestartInterval | Should -Be 'PT1M'
        $t.Settings.ExecutionTimeLimit | Should -Be 'PT0S'
        $t.Principal.LogonType | Should -Be 'Interactive'
        $t.Triggers[0].CimClass.CimClassName | Should -Be 'MSFT_TaskLogonTrigger'
    }
    It 'watchdog runs every minute as SYSTEM' {
        $root = Join-Path $TestDrive 'wd'
        New-Item -ItemType Directory -Force $root | Out-Null
        Register-WatchdogTask -Root $root
        $t = Get-ScheduledTask -TaskPath '\WallBridge\' -TaskName 'WallBridge Watchdog'
        $t.Principal.UserId | Should -Be 'SYSTEM'
        $t.Triggers[0].Repetition.Interval | Should -Be 'PT1M'
        Get-Content (Join-Path $root 'watchdog.ps1') -Raw | Should -Match "Start-ScheduledTask -TaskPath '\\WallBridge\\' -TaskName 'WallBridge Engine'"
    }
}

Describe 'Stop-Engines' {
    BeforeAll {
        $root = Join-Path $TestDrive 'stop'
        # A process that exits between the listing and Stop-Process (install at 09:42 on 30.10.
        # failed on "Cannot find a process with the process identifier 11212").
        $gone = Start-Process -FilePath 'cmd.exe' -ArgumentList '/c exit' -PassThru -WindowStyle Hidden
        $gone.WaitForExit()
        $gonePid = $gone.Id
        Mock -ModuleName WallBridge.Install Stop-ScheduledTask {}
        Mock -ModuleName WallBridge.Install Get-Process { [pscustomobject]@{ ProcessName = 'wall-bridge'; Id = $gonePid } } -ParameterFilter { $Name }
    }
    It 'tolerates a process that already exited' {
        { Stop-Engines -Root $root } | Should -Not -Throw
        Get-Content (Join-Path $root 'logs\install.log') -Raw | Should -Match "pid $gonePid already exited"
    }
}

Describe 'Stop-Engines waits' {
    BeforeAll {
        $root = Join-Path $TestDrive 'wait'
        # Stop-Process returns before the process is gone; a reinstall of the same version then
        # failed on SNV 30.10.: "Copy-Item : The process cannot access the file ...\app\0.2.6\
        # wall-bridge.exe because it is being used by another process".
        $script:slow = Start-Process -FilePath 'ping.exe' -ArgumentList '-n 60 127.0.0.1' -PassThru -WindowStyle Hidden
        $slowPid = $script:slow.Id
        Mock -ModuleName WallBridge.Install Stop-ScheduledTask {}
        Mock -ModuleName WallBridge.Install Get-Process { [pscustomobject]@{ ProcessName = 'wall-bridge'; Id = $slowPid } } -ParameterFilter { $Name }
    }
    AfterAll { Stop-Process -Id $script:slow.Id -Force -ErrorAction SilentlyContinue }
    It 'fails when a stopped process does not exit' {
        # Stop-Process that has not taken effect yet: the process is still there.
        Mock -ModuleName WallBridge.Install Stop-Process {}
        { Stop-Engines -Root $root -WaitS 2 } | Should -Throw "*pid $($script:slow.Id) did not exit*"
    }
    It 'returns only after the process is gone' {
        Stop-Engines -Root $root -WaitS 15
        $script:slow.HasExited | Should -BeTrue
    }
}

Describe 'Invoke-WithEngineRestore' {
    BeforeAll {
        $root = Join-Path $TestDrive 'restore'
        Mock -ModuleName WallBridge.Install Start-ScheduledTask {}
    }
    It 'starts the engine and the tray again when the install fails, then fails' {
        # SNV 30.10. 14:36: the install stopped the engine and the tray, Copy-Item failed, and the
        # tray stayed off (the watchdog restarts only the engine).
        { Invoke-WithEngineRestore -Root $root -Action { throw 'copy failed' } } | Should -Throw '*copy failed*'
        Should -Invoke -ModuleName WallBridge.Install Start-ScheduledTask -Times 1 -Exactly -ParameterFilter { $TaskName -eq 'WallBridge Engine' }
        Should -Invoke -ModuleName WallBridge.Install Start-ScheduledTask -Times 1 -Exactly -ParameterFilter { $TaskName -eq 'WallBridge Tray' }
        Get-Content (Join-Path $root 'logs\install.log') -Raw | Should -Match 'install failed: copy failed'
    }
    It 'starts nothing when the install succeeds' {
        Invoke-WithEngineRestore -Root $root -Action { 'fine' } | Should -Be 'fine'
        Should -Invoke -ModuleName WallBridge.Install Start-ScheduledTask -Times 0 -Exactly
    }
}
