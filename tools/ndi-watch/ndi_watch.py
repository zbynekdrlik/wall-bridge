"""Watch NDI sources for frame gaps and log them as JSON lines (ticket #15).

Runs on dev1, on the SNV LAN, so the production PCs carry no extra load beyond one more
NDI connection on the sender. Each source gets its own receiver thread.

    ndi_watch.py --log-dir DIR "SONGS-SNV (Arena - BG)=10.77.9.212:5961" ...

Log: DIR/ndi-watch.YYYY-MM-DD.jsonl (UTC). Events: start, connect, format, gap, stall
(no frame for STALL_S), resume, minute, error.
"""

import argparse
import json
import os
import sys
import threading
import time
from datetime import datetime, timezone

import NDIlib as ndi

from gaps import Tracker

STALL_S = 1.0  # no frame for this long: log a stall, log the resume with its length

_lock = threading.Lock()


def utc(ts: float) -> str:
    return datetime.fromtimestamp(ts, timezone.utc).isoformat(timespec="milliseconds")


class Log:
    def __init__(self, directory: str):
        self.dir = directory
        os.makedirs(directory, exist_ok=True)

    def write(self, source: str, event: str, ts: float | None = None, **fields) -> None:
        ts = time.time() if ts is None else ts
        rec = {"t": utc(ts), "source": source, "event": event, **fields}
        line = json.dumps(rec)
        day = utc(ts)[:10]
        with _lock:
            with open(os.path.join(self.dir, f"ndi-watch.{day}.jsonl"), "a") as f:
                f.write(line + "\n")
            print(line, flush=True)


def watch(name: str, url: str, log: Log) -> None:
    src = ndi.Source()
    src.ndi_name = name
    src.url_address = url
    cfg = ndi.RecvCreateV3()
    cfg.color_format = ndi.RECV_COLOR_FORMAT_FASTEST
    cfg.bandwidth = ndi.RECV_BANDWIDTH_HIGHEST
    cfg.ndi_recv_name = "wall-bridge ndi-watch (dev1)"
    recv = ndi.recv_create_v3(cfg)
    if recv is None:
        log.write(name, "error", reason="recv_create_v3 returned None")
        return
    ndi.recv_connect(recv, src)
    log.write(name, "connect", url=url)

    tracker = Tracker()
    fmt = None
    last_frame = time.time()
    stalled = False
    while True:
        kind, v, a, m = ndi.recv_capture_v2(recv, 200)
        now = time.time()
        if kind == ndi.FRAME_TYPE_VIDEO:
            cur = (v.xres, v.yres, v.frame_rate_N, v.frame_rate_D, str(v.FourCC))
            if cur != fmt:
                fmt = cur
                tracker.set_rate(v.frame_rate_N, v.frame_rate_D)
                log.write(name, "format", now, xres=cur[0], yres=cur[1],
                          rate_n=cur[2], rate_d=cur[3], fourcc=cur[4])
            if stalled:
                log.write(name, "resume", now, stalled_ms=round((now - last_frame) * 1000))
                stalled = False
            gap = tracker.feed(now, v.timestamp)
            if gap:
                log.write(name, "gap", now, side=gap.side, recv_ms=round(gap.recv_ms, 1),
                          send_ms=round(gap.send_ms, 1), expected_ms=round(gap.expected_ms, 2),
                          timecode=v.timecode)
            last_frame = now
            ndi.recv_free_video_v2(recv, v)
        elif kind == ndi.FRAME_TYPE_AUDIO:
            ndi.recv_free_audio_v2(recv, a)
        elif kind == ndi.FRAME_TYPE_METADATA:
            ndi.recv_free_metadata(recv, m)
        elif kind == ndi.FRAME_TYPE_STATUS_CHANGE:
            log.write(name, "status_change", now,
                      connections=ndi.recv_get_no_connections(recv))
        if not stalled and now - last_frame > STALL_S:
            stalled = True
            tracker.reset()
            log.write(name, "stall", now, since=utc(last_frame),
                      connections=ndi.recv_get_no_connections(recv))
        for s in tracker.take_finished():
            log.write(name, "minute", s.start, frames=s.frames, gaps=s.gaps,
                      sender_gaps=s.sender_gaps, max_recv_ms=round(s.max_recv_ms, 1),
                      max_send_ms=round(s.max_send_ms, 1))


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--log-dir", required=True)
    p.add_argument("sources", nargs="+", help='"NDI NAME=host:port"')
    args = p.parse_args()
    if not ndi.initialize():
        print("NDI initialize failed", file=sys.stderr)
        return 1
    log = Log(args.log_dir)
    log.write("-", "start", sources=args.sources, stall_s=STALL_S, pid=os.getpid())
    threads = []
    for spec in args.sources:
        name, _, url = spec.rpartition("=")
        if not name or not url:
            print(f"bad source {spec!r}, want NAME=host:port", file=sys.stderr)
            return 2
        t = threading.Thread(target=watch, args=(name, url, log), name=name, daemon=True)
        t.start()
        threads.append(t)
    while all(t.is_alive() for t in threads):
        time.sleep(5)
    log.write("-", "error", reason="a watcher thread ended")
    return 1


if __name__ == "__main__":
    sys.exit(main())
