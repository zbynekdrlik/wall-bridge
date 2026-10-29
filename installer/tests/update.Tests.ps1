BeforeAll {
    Import-Module (Join-Path $PSScriptRoot '..\WallBridge.Update.psm1') -Force
}

Describe 'Test-InWindow' {
    It 'outside_window_no_install' {
        Test-InWindow (Get-Date '2026-10-30 02:59') | Should -BeFalse
        Test-InWindow (Get-Date '2026-10-30 05:00') | Should -BeFalse
        Test-InWindow (Get-Date '2026-10-30 14:00') | Should -BeFalse
    }
    It 'inside the window' {
        Test-InWindow (Get-Date '2026-10-30 03:00') | Should -BeTrue
        Test-InWindow (Get-Date '2026-10-30 04:59') | Should -BeTrue
    }
}

Describe 'Get-UpdateDecision' {
    It 'installs a newer patch' { Get-UpdateDecision '0.2.0' '0.2.1' | Should -Be 'install' }
    It 'no_downgrade' {
        Get-UpdateDecision '0.2.3' '0.2.1' | Should -Be 'not-newer'
        Get-UpdateDecision '0.2.3' '0.2.3' | Should -Be 'not-newer'
    }
    It 'patch only' {
        Get-UpdateDecision '0.2.3' '0.3.0' | Should -Be 'not-a-patch'
        Get-UpdateDecision '0.2.3' '1.2.4' | Should -Be 'not-a-patch'
    }
    It 'compares numerically' { Get-UpdateDecision '0.2.9' '0.2.10' | Should -Be 'install' }
    It 'fresh box installs' { Get-UpdateDecision '' '0.2.0' | Should -Be 'install' }
    It 'no release' { Get-UpdateDecision '0.2.0' $null | Should -Be 'no-release' }
}

Describe 'release assets' {
    It 'reads the version from the zip name' {
        Get-AssetVersion 'wall-bridge-0.2.13.zip' | Should -Be '0.2.13'
        Get-AssetVersion 'SHA256SUMS' | Should -BeNullOrEmpty
        Get-AssetVersion 'wall-bridge-0.2.zip' | Should -BeNullOrEmpty
    }
    It 'finds the expected hash' {
        $s = "ABCDEF  wall-bridge-0.2.1.zip`n123  other.zip`n"
        Get-ExpectedHash $s 'wall-bridge-0.2.1.zip' | Should -Be 'abcdef'
        Get-ExpectedHash $s 'missing.zip' | Should -BeNullOrEmpty
    }
}

Describe 'Test-Healthy' {
    It 'rollback_when_unhealthy' {
        $bad = [pscustomobject]@{ version = '0.2.1'; display = [pscustomobject]@{ target_acquired = $false } }
        Test-Healthy $bad '0.2.1' | Should -BeFalse
        $old = [pscustomobject]@{ version = '0.2.0'; display = [pscustomobject]@{ target_acquired = $true } }
        Test-Healthy $old '0.2.1' | Should -BeFalse
        Test-Healthy $null '0.2.1' | Should -BeFalse
    }
    It 'healthy new version' {
        $good = [pscustomobject]@{ version = '0.2.1'; display = [pscustomobject]@{ target_acquired = $true } }
        Test-Healthy $good '0.2.1' | Should -BeTrue
    }
}

Describe 'killswitch_respected' {
    It 'exits before any network call when autoupdate.disabled exists' {
        $root = Join-Path $TestDrive 'ks'
        New-Item -ItemType Directory -Force $root | Out-Null
        Set-Content (Join-Path $root 'autoupdate.disabled') ''
        & (Join-Path $PSScriptRoot '..\autoupdate.ps1') -Root $root -Repo 'invalid/does-not-exist'
        Get-Content (Join-Path $root 'logs\install.log') -Raw | Should -Match 'disabled by autoupdate.disabled'
    }
}
