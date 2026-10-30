"""Wall measurement (#259): dissolve the Surround span into three 1x1 grids, wait,
re-form it with the grid read at the start (the pinned A/B/C order), and log every step.
usage: python nvapi_grid_cycle.py <log> <dissolved_secs>"""
import ctypes
import json
import sys
import time
from ctypes import c_uint32, c_int32, POINTER, byref

log_path, hold = sys.argv[1], float(sys.argv[2])
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
    out = []
    for i in range(n.value):
        x = g[i]
        out.append({"rows": x.rows, "cols": x.columns,
                    "ids": ["0x%08x" % x.displays[k].displayId for k in range(x.displayCount)],
                    "mode": "%dx%d@%d" % (x.displaySettings.width, x.displaySettings.height, x.displaySettings.freq)})
    return r, g, n.value, out


r, start, n0, desc = read()
log("start grids r=%d" % r, json.dumps(desc))
if n0 != 1 or start[0].displayCount != 3:
    log("not the expected 1x3 span; nothing done")
    sys.exit(3)
pinned = Grid()
ctypes.memmove(byref(pinned), byref(start[0]), ctypes.sizeof(Grid))
open(log_path + ".pinned.bin", "wb").write(bytes(pinned))

sep = (Grid * 3)()
for i in range(3):
    s = sep[i]
    s.version = GRID_VER
    s.rows = 1
    s.columns = 1
    s.displayCount = 1
    s.displays[0] = pinned.displays[i]
    s.displaySettings = pinned.displaySettings
t = time.perf_counter()
r = set_grids(sep, 3, 0)
log("SetDisplayGrids(3 x 1x1) r=%d in %.1f s" % (r, time.perf_counter() - t))
log("after dissolve", json.dumps(read()[3]))
time.sleep(hold)

t = time.perf_counter()
r = set_grids(byref(pinned), 1, 0)
log("SetDisplayGrids(pinned 1x3) r=%d in %.1f s" % (r, time.perf_counter() - t))
log("after re-form", json.dumps(read()[3]))
