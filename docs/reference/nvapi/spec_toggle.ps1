param([string]$Target = 'NV Surround', [ValidateSet('on','off','read')][string]$Want = 'read')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$A = [Windows.Automation.AutomationElement]
Start-Process 'ms-settings:display-advanced'
Start-Sleep 4
$root = $A::RootElement
$w = $root.FindFirst('Children', (New-Object Windows.Automation.PropertyCondition($A::NameProperty, 'Settings')))
$cb = $w.FindFirst('Descendants', (New-Object Windows.Automation.PropertyCondition($A::AutomationIdProperty, 'SystemSettings_Display_AdvancedDisplaySettingsTargetSelectionAllTargets_ComboBox')))
if (-not $cb) { $ad = $w.FindFirst('Descendants', (New-Object Windows.Automation.AndCondition((New-Object Windows.Automation.PropertyCondition($A::NameProperty, 'Advanced display')), (New-Object Windows.Automation.PropertyCondition($A::ControlTypeProperty, [Windows.Automation.ControlType]::Button))))); $ad.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke(); Start-Sleep 3; $cb = $w.FindFirst('Descendants', (New-Object Windows.Automation.PropertyCondition($A::AutomationIdProperty, 'SystemSettings_Display_AdvancedDisplaySettingsTargetSelectionAllTargets_ComboBox'))) }
$cb.GetCurrentPattern([Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
Start-Sleep 1
$items = $cb.FindAll('Descendants', (New-Object Windows.Automation.PropertyCondition($A::ControlTypeProperty, [Windows.Automation.ControlType]::ListItem)))
$hit = $null
foreach ($i in $items) { if ($i.Current.Name -like "*$Target*") { $hit = $i } }
if (-not $hit) { "items: " + (($items | % { $_.Current.Name }) -join ' | '); throw "target $Target not in list" }
$hit.GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern).Select()
Start-Sleep 3
$all = $w.FindAll('Descendants', [Windows.Automation.Condition]::TrueCondition)
$tg = $null
foreach ($e in $all) { if ($e.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $e.Current.AutomationId -match 'Specializ|RemoveDisplay|RemoveFromDesktop') { $tg = $e } }
if (-not $tg) { foreach ($e in $all) { if ($e.Current.Name -like '*Remove display from desktop*' -and $e.Current.ControlType -eq [Windows.Automation.ControlType]::Button) { $tg = $e } } }
if (-not $tg) { ($all | % { "$($_.Current.ControlType.ProgrammaticName)|$($_.Current.Name)|$($_.Current.AutomationId)" } | Select-String 'Toggle|Remove|Special') ; throw 'toggle not found' }
$tp = $tg.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)
$state = $tp.Current.ToggleState
"selected=$($hit.Current.Name) toggle=$($tg.Current.AutomationId) state=$state"
if (($Want -eq 'on' -and $state -ne 'On') -or ($Want -eq 'off' -and $state -eq 'On')) { $tp.Toggle(); Start-Sleep 4; "toggled -> $($tp.Current.ToggleState)" }


