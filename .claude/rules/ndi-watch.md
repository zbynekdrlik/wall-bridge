---
paths:
  - "tools/ndi-watch/**"
---

# ndi-watch (ticket #15)

- Watches the two NDI sources Arena on songs-snv (10.77.9.212) sends to Arena on resolume-snv: `SONGS-SNV (Arena - BG)` :5961 and `SONGS-SNV (Arena - TXT)` :5962, both 1920x1080 UYVA at 60 fps (9.11.).
- It runs on dev1 (10.77.9.195, same LAN), not on the production PCs. Receiving at the highest bandwidth reuses the sender's single encode; the lowest bandwidth would make the songs-snv laptop encode an extra proxy stream.
- Service: systemd `--user` unit `ndi-watch` (linger is on). The unit file is a symlink from `~/.config/systemd/user/` to `tools/ndi-watch/ndi-watch.service`, so it runs the dev checkout.
  - Venv: `~/.local/share/ndi-watch/venv` (`pip install -r tools/ndi-watch/requirements.txt`). `ndi-python` bundles libndi; nothing system-wide.
  - `systemctl --user` needs `XDG_RUNTIME_DIR=/run/user/$(id -u)` in this shell.
- Log: `~/.local/state/ndi-watch/ndi-watch.YYYY-MM-DD.jsonl` (UTC).
  - `gap side=sender`: the sender's own timestamps jumped too, so songs-snv produced no frames. `side=transport`: frames were stamped on time but arrived late.
  - `side=unknown`: a frame had no sender timestamp. `loop_ms` on a gap: the longest time ndi-watch itself spent outside `recv_capture` since the previous frame; a gap close to it is on dev1, not on the network.
  - `stall`/`resume`: no frame for 1 s or more, then its length. `minute`: frames, gaps and max intervals per minute.
  - At 60 fps a gap is over 41.7 ms (2.5 frames); single 42-43 ms gaps also occur on an idle picture and are noise. A visible freeze is 150 ms or more.
- Tests: `python3 -m unittest -v test_gaps` in `tools/ndi-watch` (pure, no NDI needed); CI runs them in `test-linux`.
