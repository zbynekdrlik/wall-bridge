# Latency measurement, step 1 (from songplayer#259). Usage: python grab_ts.py <seconds> <events.json>
# Needs numpy and the NZXT grabber; ffmpeg from SongPlayer's tool cache.
import subprocess, sys, json, re, threading, time
import numpy as np

# Capture the grabber, record per-frame change in the text bands, and give each change event
# an epoch time: pts (relative) mapped by the smallest observed read-minus-pts offset.
FF = r"C:\ProgramData\SongPlayer\cache\tools\ffmpeg.exe"
secs = int(sys.argv[1])
out = sys.argv[2]
W, H = 720, 480
cmd = [FF, "-hide_banner", "-loglevel", "info", "-use_wallclock_as_timestamps", "1",
       "-f", "dshow", "-rtbufsize", "300M", "-video_size", "720x480", "-framerate", "60",
       "-i", "video=NZXT Signal HD60 Video", "-t", str(secs), "-vf", "showinfo",
       "-f", "rawvideo", "-pix_fmt", "gray", "-"]
p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
pts = []
def err():
    for line in p.stderr:
        m = re.search(rb"pts_time:([0-9.]+)", line)
        if m:
            pts.append(float(m.group(1)))
th = threading.Thread(target=err); th.start()
prev = None
diffs = []
reads = []
while True:
    buf = p.stdout.read(W * H)
    if len(buf) < W * H:
        break
    reads.append(time.time())
    f = np.frombuffer(buf, np.uint8).reshape(H, W).astype(np.int16)
    if prev is not None:
        diffs.append(float(np.abs(f[40:160] - prev[40:160]).mean()))
    prev = f
p.wait(); th.join()
k = min(len(reads), len(pts))
anchor = min(reads[i] - pts[i] for i in range(k))
n = min(len(diffs), k - 1)
events = []
last = -100
for i in range(n):
    if diffs[i] > 2.0:
        if i - last > 30:
            events.append({"t_ms": round((anchor + pts[i + 1]) * 1000.0, 1), "d": round(diffs[i], 1)})
        last = i
json.dump({"anchor": anchor, "frames": n, "events": events}, open(out, "w"))
print(json.dumps({"frames": n, "events": len(events)}))
