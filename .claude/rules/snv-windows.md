---
paths:
  - "crates/wb-service/**"
  - "crates/wb-spout/**"
  - "crates/wb-scanout/**"
  - "crates/wb-topology/**"
  - "installer/**"
---

# SNV / Windows / Arena gotchas (measured, keep)

- **Session:** Spout names (`SpoutSenderNames`, the 280 B sender mapping, `<name>_SpoutAccessMutex`) are session-local.
  - A SYSTEM / session 0 process sees `senders: []`, so the engine must run in `Resolume`'s session 1 (`docs/spikes/2026-10-session0.md`).
- **NVAPI and specialization:** `NvAPI_Mosaic_EnumDisplayGrids` lists NO grid for a span removed from the desktop.
  - Only the grabber (0x80061081) shows up.
  - Do not read "no 1x3 grid" as a collapse; the keeper uses the engine's live target instead.
- **The vblank path must never block:**
  - A blocking `Map()` of the probe staging or a long wait on Arena's mutex near the end of the budget ran past the vblank: 23–76 missed / 10 s whenever Arena's pictures arrived just after our vblank. The MVP had the same problem.
  - The probe uses `D3D11_MAP_FLAG_DO_NOT_WAIT`, and the mutex wait is bounded by the time left.
  - No console logging and no `list_targets` on that path.
- **Missed vblanks:** count them from the time between `WaitForVBlank` returns. The MVP's swapchain counter is always 0 in Display.Core mode.
- **Quitting Arena:** a graceful quit raises "Do you really want to quit? All unsaved progress will be lost" (Quit / Cancel / Save & Quit).
  - Never answer it for the operator. Cancel with Escape on the dialog.
  - Arena reads its Advanced Output preset at start and does not rewrite it on quit.
- **GUI clicks in Arena via MCP:** risky, a misclick can start a clip (clips only from the owner's Stream Deck). Prefer file / REST changes.
- **MCP Shell:** output over ~70 kB is cut off, so filter on the PC.
  - For files and secrets use `~/.claude/work-products/snv_mcp_shell.py` (talks to the gateway directly; `{ENV:NAME}` placeholders keep secrets off the transcript).
- **Binaries to SNV:** SNV pulls from a release URL, or from a temporary `python3 -m http.server` on dev1 (10.77.9.195). Never base64 a multi-MB exe through MCP.
- **Arena restarts (from songplayer, 30.10.):**
  - Ask the songplayer session (`peer_send`) first about anything Arena / SNV you do not know.
  - SongPlayer's wall guard (`GET :8920/api/v1/wall`, `guard.last`) ends Arena when its BugSplat "Crash Report" window shows, or when Arena hangs 30 s. A teardown crash after Save & Quit is common; the save comes first.
  - The operator's AutoHotkey `NL_STARTUP.ahk` relaunches Arena ~1 s after it ends. Not always: check, never assume. Never start a second instance; wait only for the OLD pid to go.
  - After a relaunch nothing plays: the wall is black until the operator presses a scene. Call it "nespustený klip", never "odpojený". No script or service ever starts or connects clips.
  - A kill saves nothing; Save & Quit makes the running state permanent. Before a kill, dump `GET :8090/api/v1/composition`.
  - Tell the camera-box session the time of every Arena relaunch (Process Lasso / power plan). If it is offline, tell the owner it was not told.
  - Safe order: specialize, restart Arena, connect scene, start bridge. A display vanishing under a running Arena stalls it.
  - The grabber shows Arena's direct output unless routed through the bridge: compare latency only on the same path.
- **NVAPI with a specialized wall:** `--topology separate` cannot touch the wall while it is off the desktop (NVAPI does not list it). A restore test first returns the wall to the desktop (`spec_toggle.ps1 -Want off`, run in `Resolume`'s session through a one-shot interactive task).
