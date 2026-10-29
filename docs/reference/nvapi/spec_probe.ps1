$ErrorActionPreference = "Stop"
Add-Type @'
using System; using System.Runtime.InteropServices;
public class SP {
 [StructLayout(LayoutKind.Sequential)] public struct LUID { public uint Low; public int High; }
 [StructLayout(LayoutKind.Sequential)] public struct SRC { public LUID adapterId; public uint id; public uint modeInfoIdx; public uint statusFlags; }
 [StructLayout(LayoutKind.Sequential)] public struct RAT { public uint n; public uint d; }
 [StructLayout(LayoutKind.Sequential)] public struct TGT { public LUID adapterId; public uint id; public uint modeInfoIdx; public uint outputTechnology; public uint rotation; public uint scaling; public RAT refreshRate; public uint scanLineOrdering; public int targetAvailable; public uint statusFlags; }
 [StructLayout(LayoutKind.Sequential)] public struct PATH { public SRC sourceInfo; public TGT targetInfo; public uint flags; }
 [StructLayout(LayoutKind.Sequential)] public struct MODE { public uint infoType; public uint id; public LUID adapterId; [MarshalAs(UnmanagedType.ByValArray, SizeConst=48)] public byte[] body; }
 [StructLayout(LayoutKind.Sequential)] public struct HDR { public uint type; public uint size; public LUID adapterId; public uint id; }
 [StructLayout(LayoutKind.Sequential)] public struct GETSPEC { public HDR header; public uint value; }
 [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct TNAME { public HDR header; public uint flags; public uint outputTechnology; public ushort edidManufactureId; public ushort edidProductCodeId; public uint connectorInstance; [MarshalAs(UnmanagedType.ByValTStr, SizeConst=64)] public string monitorFriendlyDeviceName; [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string monitorDevicePath; }
 [DllImport("user32.dll")] public static extern int GetDisplayConfigBufferSizes(uint f, out uint np, out uint nm);
 [DllImport("user32.dll")] public static extern int QueryDisplayConfig(uint f, ref uint np, [Out] PATH[] p, ref uint nm, [Out] MODE[] m, IntPtr t);
 [DllImport("user32.dll")] public static extern int DisplayConfigGetDeviceInfo(ref GETSPEC r);
 [DllImport("user32.dll")] public static extern int DisplayConfigGetDeviceInfo(ref TNAME r);
}
'@
$np = 0; $nm = 0
[void][SP]::GetDisplayConfigBufferSizes(2, [ref]$np, [ref]$nm)
$p = New-Object SP+PATH[] $np; $m = New-Object SP+MODE[] $nm
for ($i = 0; $i -lt $nm; $i++) { $m[$i].body = New-Object byte[] 48 }
$r = [SP]::QueryDisplayConfig(2, [ref]$np, $p, [ref]$nm, $m, [IntPtr]::Zero)
"query=$r paths=$np"
for ($i = 0; $i -lt $np; $i++) {
  $t = $p[$i].targetInfo
  $n = New-Object SP+TNAME; $n.header.type = 2; $n.header.size = [Runtime.InteropServices.Marshal]::SizeOf($n); $n.header.adapterId = $t.adapterId; $n.header.id = $t.id
  $rn = [SP]::DisplayConfigGetDeviceInfo([ref]$n)
  $g = New-Object SP+GETSPEC; $g.header.type = 12; $g.header.size = [Runtime.InteropServices.Marshal]::SizeOf($g); $g.header.adapterId = $t.adapterId; $g.header.id = $t.id
  $rg = [SP]::DisplayConfigGetDeviceInfo([ref]$g)
  "target id={0} name='{1}' tech={2} | nameR={5} getSpec r={3} value=0x{4:x} (bit0 enabled, bit1 availForMonitor, bit2 availForSystem)" -f $t.id, $n.monitorFriendlyDeviceName, $t.outputTechnology, $rg, $g.value, $rn
}

