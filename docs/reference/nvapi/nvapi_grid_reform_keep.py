"""Wall repair (20.10.2026): the SNV span reads as one 1x1 grid of a 7680x1080 display
(0x80061088) since the driver swaps / reboots, and the left screen shows nothing while NVAPI
calls it active. Set the grid pinned on 13.10.2026 (1x3 2560x1080@60: 0x80061086 DP left,
0x80061087 DP middle, 0x80061083 HDMI right, cycle-20261013-111323.log.pinned.bin) so the
driver forms the span anew and trains each link. Logs every step.
usage: python nvapi_grid_reform.py <log> <pinned.bin>"""
import ctypes
import json
import sys
import time
from ctypes import c_uint32, c_int32, POINTER, byref

log_path, pinned_path = sys.argv[1], sys.argv[2]
logf = open(log_path, "a", encoding="utf-8")


def log(*a):
    line = time.strftime("%H:%M:%S ") + " ".join(str(x) for x in a)
    print(line)
    logf.write(line + "\n")
    logf.flush()


nv = ctypes.WinDLL("nvapi64.dll")
qi = nv.nvapi_QueryInterface
qi.restype = ctypes.c_void_p
qi.argtypes = [c_uint32]


def fn(fid, *args):
    return ctypes.CFUNCTYPE(c_int32, *args)(qi(fid))


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


GRID_VER = ctypes.sizeof(Grid) | (2 << 16)
log("NvAPI_Initialize", fn(0x0150E828)())
enum_grids = fn(0xDF2887AF, POINTER(Grid), POINTER(c_uint32))
set_grids = fn(0x4D959A89, POINTER(Grid), c_uint32, c_uint32)


def read():
    n = c_uint32(0)
    enum_grids(None, byref(n))
    g = (Grid * max(1, n.value))()
    for x in g:
        x.version = GRID_VER
    r = enum_grids(g, byref(n))
    return r, [{"rows": g[i].rows, "cols": g[i].columns,
                "ids": ["0x%08x" % g[i].displays[k].displayId for k in range(g[i].displayCount)],
                "mode": "%dx%d@%d" % (g[i].displaySettings.width, g[i].displaySettings.height,
                                      g[i].displaySettings.freq)} for i in range(n.value)]


r, desc = read()
log("before r=%d" % r, json.dumps(desc))
raw = open(pinned_path, "rb").read()
if len(raw) != ctypes.sizeof(Grid):
    log("pinned grid has %d bytes, expected %d; nothing done" % (len(raw), ctypes.sizeof(Grid)))
    sys.exit(3)
pinned = Grid.from_buffer_copy(raw)
want = ["0x%08x" % pinned.displays[k].displayId for k in range(pinned.displayCount)]
log("pinned %dx%d" % (pinned.rows, pinned.columns), want,
    "%dx%d@%d" % (pinned.displaySettings.width, pinned.displaySettings.height, pinned.displaySettings.freq))
if pinned.rows != 1 or pinned.columns != 3 or pinned.displayCount != 3:
    log("pinned grid is not 1x3; nothing done")
    sys.exit(3)
t = time.perf_counter()
n0 = c_uint32(0); enum_grids(None, byref(n0)); cur = (Grid * max(1, n0.value))()
for x in cur: x.version = GRID_VER
enum_grids(cur, byref(n0))
extra = [cur[i] for i in range(n0.value) if cur[i].displayCount == 1 and cur[i].displays[0].displayId == 0x80061081]
arr = (Grid * (1 + len(extra)))(pinned, *extra)
log("extra grids", ["0x%08x" % g.displays[0].displayId for g in extra])
r = set_grids(arr, len(arr), 0)
log("SetDisplayGrids(pinned 1x3) r=%d in %.1f s" % (r, time.perf_counter() - t))
time.sleep(3)
log("after", json.dumps(read()[1]))
sys.exit(0 if r == 0 else 4)


