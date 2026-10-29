BeforeAll {
    Import-Module (Join-Path $PSScriptRoot '..\WallBridge.Install.psm1') -Force
}

Describe 'Get-PrunableVersions' {
    It 'keeps the current and the newest other version' {
        $r = Get-PrunableVersions -Versions @('0.1.9', '0.2.0', '0.2.1', '0.10.0') -Current '0.10.0'
        $r | Sort-Object | Should -Be @('0.1.9', '0.2.0')
    }
    It 'keeps a rollback even when current is older than the newest dir' {
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
