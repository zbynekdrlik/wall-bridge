# Display diagnostics: dot-source, then List / Stage <dev> <w> <h> <hz> <x> <y> <primary> / Commit.
$ErrorActionPreference = "Stop"
Add-Type @'
using System; using System.Runtime.InteropServices;
public class DM {
 [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
 public struct DEVMODE { [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string dmDeviceName; public short dmSpecVersion, dmDriverVersion, dmSize, dmDriverExtra; public int dmFields; public int dmPositionX, dmPositionY; public int dmDisplayOrientation, dmDisplayFixedOutput; public short dmColor, dmDuplex, dmYResolution, dmTTOption, dmCollate; [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string dmFormName; public short dmLogPixels; public int dmBitsPerPel, dmPelsWidth, dmPelsHeight, dmDisplayFlags, dmDisplayFrequency, dmICMMethod, dmICMIntent, dmMediaType, dmDitherType, dmReserved1, dmReserved2, dmPanningWidth, dmPanningHeight; }
 [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
 public struct DD { public int cb; [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string DeviceName; [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string DeviceString; public int StateFlags; [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string DeviceID; [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string DeviceKey; }
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool EnumDisplayDevices(string d, int i, ref DD dd, int f);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool EnumDisplaySettings(string d, int m, ref DEVMODE dm);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int ChangeDisplaySettingsEx(string d, ref DEVMODE dm, IntPtr h, int f, IntPtr l);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int ChangeDisplaySettingsEx(string d, IntPtr dm, IntPtr h, int f, IntPtr l);
}
'@
function List {
  for ($i = 0; $i -lt 16; $i++) {
    $d = New-Object DM+DD; $d.cb = [Runtime.InteropServices.Marshal]::SizeOf($d)
    if (-not [DM]::EnumDisplayDevices([NullString]::Value, $i, [ref]$d, 0)) { continue }
    $m = New-Object DM+DD; $m.cb = $d.cb; [void][DM]::EnumDisplayDevices($d.DeviceName, 0, [ref]$m, 0)
    $dm = New-Object DM+DEVMODE; $dm.dmSize = [Runtime.InteropServices.Marshal]::SizeOf($dm)
    $cur = [DM]::EnumDisplaySettings($d.DeviceName, -1, [ref]$dm)
    "{0} key={1} flags=0x{2:x} mon='{3}' {4}x{5}@{6} pos={7},{8}" -f $d.DeviceName, ($d.DeviceKey -replace '.*\\',''), $d.StateFlags, $m.DeviceString, $dm.dmPelsWidth, $dm.dmPelsHeight, $dm.dmDisplayFrequency, $dm.dmPositionX, $dm.dmPositionY
  }
}
function Stage($dev, $w, $h, $hz, $x, $y, $primary) {
  $dm = New-Object DM+DEVMODE; $dm.dmSize = [Runtime.InteropServices.Marshal]::SizeOf($dm)
  [void][DM]::EnumDisplaySettings($dev, -1, [ref]$dm)
  $dm.dmPelsWidth = $w; $dm.dmPelsHeight = $h; $dm.dmDisplayFrequency = $hz; $dm.dmPositionX = $x; $dm.dmPositionY = $y
  $dm.dmFields = 0x20 -bor 0x80000 -bor 0x100000 -bor 0x400000
  $f = 0x1 -bor 0x10000000  # CDS_UPDATEREGISTRY | CDS_NORESET
  if ($primary) { $f = $f -bor 0x10 }
  $r = [DM]::ChangeDisplaySettingsEx($dev, [ref]$dm, [IntPtr]::Zero, $f, [IntPtr]::Zero)
  "stage $dev ${w}x${h}@$hz at $x,$y -> $r"
}
function Commit { "commit -> " + [DM]::ChangeDisplaySettingsEx([NullString]::Value, [IntPtr]::Zero, [IntPtr]::Zero, 0, [IntPtr]::Zero) }
