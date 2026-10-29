"""Read-only NVAPI probe: the current Mosaic/Surround grids and connected displays.
Calls only NvAPI_Initialize, NvAPI_Mosaic_EnumDisplayGrids, NvAPI_EnumPhysicalGPUs,
NvAPI_GPU_GetConnectedDisplayIds, NvAPI_DISP_GetDisplayIdByDisplayName. Changes nothing."""
import ctypes
import sys
from ctypes import c_uint32, c_int32, POINTER, byref

nv = ctypes.WinDLL("nvapi64.dll")
qi = nv.nvapi_QueryInterface
qi.restype = ctypes.c_void_p
qi.argtypes = [c_uint32]


def fn(fid, *args):
    p = qi(fid)
    if not p:
        raise SystemExit("nvapi function 0x%08x not available" % fid)
    return ctypes.CFUNCTYPE(c_int32, *args)(p)


def ver(size, v):
    return size | (v << 16)


class Disp(ctypes.Structure):
    _fields_ = [("version", c_uint32), ("displayId", c_uint32), ("overlapX", c_int32),
                ("overlapY", c_int32), ("rotation", c_int32), ("cloneGroup", c_uint32),
                ("pixelShiftType", c_int32)]


class Setting(ctypes.Structure):
    _fields_ = [("version", c_uint32), ("width", c_uint32), ("height", c_uint32),
                ("bpp", c_uint32), ("freq", c_uint32)]


class Grid(ctypes.Structure):
    _fields_ = [("version", c_uint32), ("rows", c_uint32), ("columns", c_uint32),
                ("displayCount", c_uint32), ("flags", c_uint32), ("displays", Disp * 64),
                ("displaySettings", Setting)]


class DispIds(ctypes.Structure):
    _fields_ = [("version", c_uint32), ("connectorType", c_int32), ("displayId", c_uint32),
                ("flags", c_uint32)]


init = fn(0x0150E828)
r = init()
print("NvAPI_Initialize", r)
if r != 0:
    sys.exit(1)

enum_grids = fn(0xDF2887AF, POINTER(Grid), POINTER(c_uint32))
count = c_uint32(0)
r = enum_grids(None, byref(count))
print("EnumDisplayGrids count query", r, "count", count.value)
grids = (Grid * max(1, count.value))()
for g in grids:
    g.version = ver(ctypes.sizeof(Grid), 2)
r = enum_grids(grids, byref(count))
print("EnumDisplayGrids", r, "sizeof(Grid)", ctypes.sizeof(Grid))
for i in range(count.value):
    g = grids[i]
    s = g.displaySettings
    print("grid %d: %dx%d displays=%d flags=0x%x setting %dx%d %dbpp %dHz" % (
        i, g.rows, g.columns, g.displayCount, g.flags, s.width, s.height, s.bpp, s.freq))
    for k in range(g.displayCount):
        d = g.displays[k]
        print("   [%d] displayId=0x%08x overlap=(%d,%d) rot=%d" % (k, d.displayId, d.overlapX, d.overlapY, d.rotation))

enum_gpus = fn(0xE5AC921F, POINTER(ctypes.c_void_p * 64), POINTER(c_uint32))
gpus = (ctypes.c_void_p * 64)()
n = c_uint32(0)
print("EnumPhysicalGPUs", enum_gpus(byref(gpus), byref(n)), n.value)
get_ids = fn(0x0078DBA2, ctypes.c_void_p, POINTER(DispIds), POINTER(c_uint32), c_uint32)
for gi in range(n.value):
    cnt = c_uint32(0)
    get_ids(gpus[gi], None, byref(cnt), 0)
    arr = (DispIds * max(1, cnt.value))()
    for a in arr:
        a.version = ver(ctypes.sizeof(DispIds), 3)
    r = get_ids(gpus[gi], arr, byref(cnt), 0)
    print("GPU %d connected display ids (%d) r=%d" % (gi, cnt.value, r))
    for a in arr[: cnt.value]:
        f = a.flags
        print("   displayId=0x%08x conn=%d active=%d osVisible=%d connected=%d physical=%d" % (
            a.displayId, a.connectorType, (f >> 2) & 1, (f >> 4) & 1, (f >> 6) & 1, (f >> 17) & 1))

by_name = fn(0xAE457190, ctypes.c_char_p, POINTER(c_uint32))
for name in (b"\\\\.\\DISPLAY1", b"\\\\.\\DISPLAY2", b"\\\\.\\DISPLAY3", b"\\\\.\\DISPLAY5"):
    did = c_uint32(0)
    r = by_name(name, byref(did))
    print(name.decode(), "->", r, "0x%08x" % did.value)
