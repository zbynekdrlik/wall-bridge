# wall-bridge — production design

Ticket: #2. Status: approved by the owner 29.10.2026 (#2).

## Intent

The SNV TV/LED wall (3 screens, NVIDIA Surround 7680×1080@59.978 on an RTX 3070 Ti) shows Resolume Arena's Spout output `Arena - Screen 2`. The wall does not show it through the Windows desktop. wall-bridge takes the wall off the desktop (Display.Core "specialized" display) and paces each Arena picture onto the wall's vblanks itself. This removed the 25 fps judder that 4 weeks of other attempts did not fix. History: #1 (body + history comment).

The MVP (`src/main.rs`, `src/dc.rs`) proved the method. It runs by hand on SNV today. The owner now wants a production app (owner, 29.10.2026, #2):

- A Rust app/service built to our standards: CI, E2E coverage, well designed.
- A tray icon and autoupgrade, like the owner's other live apps.
- High robustness.
- It keeps the Surround wall in its defined layout and restores it through NVAPI if the layout falls apart.
- The wall is a dedicated display off the Windows desktop.

Owner's fear: a repeat of 4 weeks of detours. The working recipe is the bridge + Windows 11 25H2 + NVIDIA 581.57 + Surround. Keep it.

## Goals

1. **The wall is always on the bridge, with no human.** This covers a reboot, a logon, a crash, an Arena restart, a Spout sender that disappears and comes back, and a Surround collapse.
2. **Picture quality stays MVP-equal or better:** ~300 new pictures per 10 s on ytfast, 0 missed vblanks, latency ≤ the MVP (+2–3 ms vs Arena direct).
3. **The Surround layout is held to a stored definition.** The service detects drift or collapse and restores the layout through NVAPI by itself.
4. **The wall display stays specialized (off the desktop).** The service owns that state and applies it itself. There is no UIA clicking in Settings.
5. **House-standard app.**
   - Tray: airuleset `rules/rust-web-tray.md`, reference iemmixer `crates/iem-tray`.
   - Autoupgrade, installer, release channels and self-hosted E2E on the real PC: devbridge pattern.
   - Coverage floor file and version job: fohmixer pattern.
   - A local web dashboard that shows the version.
6. **State is published for SongPlayer.** The interface is agreed with the songplayer session over `peer_send` before it is built (#1 backlog 3, songplayer#259).

## Non-goals

- No change to Arena's rendering. No patching of Arena or SongPlayer output.
- No hardware purchase.
- No 3 separate outputs: they are not frame-locked. The owner decided to stay on Surround.
- No other venues or presets for now. One wall definition on one PC (SNV). Config allows more later, but none is built.
- Later backlog items are not in this design: Arena at a lower resolution with bridge upscale (#1 item 4), and −1 frame latency (#1 item 5). They stay open on #1.

## Anti-goals (failure even if it "works")

- The wall silently falls back to a desktop display, or goes black, while the service reports healthy.
- The service toggles specialization or re-forms Surround during normal operation when nothing is broken. Every topology change risks the Arena/NVIDIA trouble listed in #1.
- An autoupgrade swaps the binary during an event and the wall drops. See "Upgrade safety".
- The service changes the operator's Arena composition, starts clips, or restarts Arena.
- Driver or Windows updates are triggered or allowed by the service.

## Constraints and facts (measured, see #1)

- **Machine:** `win-resolume` 10.77.9.201, Windows 11 IoT Enterprise 25H2, NVIDIA 581.57 (clean), RTX 3070 Ti.
  - Wall outputs: A = NVAPI 0x80061086 (DP), B = 0x80061087 (DP), C = 0x80061083 (HDMI).
  - 0x80061081 = NZXT Signal HD60 grabber passthrough (measurement path, stays on the desktop).
- **Access:** only through the `win-resolume` MCP. Never ssh. A self-hosted GitHub Actions runner already exists on the PC (scheduled task `GitHubActionsRunner`).
- **Display.Core `CreateScanoutSource` on the Surround span:** works on 25H2, failed on 23H2.
- **Session:** the MVP runs in the interactive session 1 (user `Resolume`). It is unproven from session 0 (SYSTEM service). `Resolume` logs on automatically since the first install (owner, 29.10.). The registry value `AutoAdminLogon` reads 0, so the auto-logon works through another mechanism; do not "fix" it. Arena itself needs the logged-in session.
- **Surround collapse:** any topology change (monitor plugged in, resolution test, driver install) turns the wall into one 3840×2160@29/30 display 0x80061088.
  - Re-forming works ONLY from the "3 separate displays" state, never from a collapsed span.
  - Proven script: `diag\wall-bridge\nvapi_grid_reform_keep.py` (pinned 1×3 blob `cycle-20261013-111323.log.pinned.bin` + 1×1 grid for 0x80061081).
  - Read-only probe: `nvapi_grid_read.py`.
- **Spout:**
  - Shared memory `SharedTextureInfo`: 280 B, named after the sender.
  - Mutex `<name>_SpoutAccessMutex`; sender list `SpoutSenderNames`.
  - After an Arena restart the handle changes; the MVP keeps the dead handle (frozen picture, `new=0`).
- **Arena:** stalls (~12.5 fps, no Spout updates) when a display output in its preset disappears.
  - Today the preset `snv surround 7680` still has the display outputs `LG wall` (Display 13 = the wall span) and `Screen 1` (Display 12 = grabber), plus Spout `Screen 2`.
  - Proposal pending owner OK on #2: drop `LG wall` so Arena reaches the wall only through Spout.
  - Hypothesis: this removes the stall, proven by an A/B/A test.
  - The service design assumes the Spout-only preset and never restarts Arena.
- **Owner rules:**
  - Clips only from his Stream Deck.
  - No permanent Arena change without his OK.
  - Event mode = hands off.
  - Never switch the bridge off just because a test ended.
  - Write "zmerané X, hypotéza Y".
  - Plain Slovak to the owner.
  - Measure on the grabber with the arrivals method, never click → photon.

## Architecture

A Cargo workspace. Decision logic lives in pure crates that build, test and mutate on Linux. Windows FFI lives in thin crates. This is the house rule from the fohmixer and iemmixer trays.

| Crate | Kind | Does |
|---|---|---|
| `wb-core` | pure | Config + wall definition types. Pacing scheduler (the MVP's arrival-phase / queue logic, extracted and unit-tested). Health state machine. Status model (JSON). Upgrade decision rules. |
| `wb-spout` | Windows | Spout sender discovery and open. Handle-change and stall detection with re-open, with no process restart (#1 item 1). |
| `wb-scanout` | Windows | Display.Core: find the target by StableMonitorId substring, acquire it, create the scanout source, wait for vblank, present. |
| `wb-topology` | Windows | NVAPI Mosaic read / validate / apply (a Rust FFI port of the proven Python scripts), the collapse → separate → re-form sequence, and the specialization read/apply. |
| `wb-service` | bin | The engine loop (Spout → pacing → scanout), the topology keeper, the axum status API + dashboard, rolling logs. |
| `wb-tray` | bin | Tauri 2 tray per iemmixer `crates/iem-tray`. |
| `wb-e2e` | bin | E2E checks run on the real PC (devbridge `devbridge-e2e` pattern). |

### Process model

- **Engine (`wb-service`)** runs where Display.Core is proven: the interactive user session, started by a Task Scheduler task at the logon of `Resolume` (fohmixer/iemmixer pattern), with restart-on-failure.
- **Plan step 1 is a spike:** run the MVP's Display.Core path from session 0 (SYSTEM scheduled task).
  - If it works, the engine becomes a real SYSTEM service or startup task (devbridge/dantesync pattern) and no longer depends on a logon.
  - The result is recorded on #2 and picks the variant.
- **A watchdog** restarts the engine if it dies or its heartbeat stops. Scheduled-task restart settings, or the SYSTEM updater task re-checks every minute.
- **Single instance:** a named mutex. A second instance exits.

### Engine behaviour

- **Steady state:** the MVP pacing is unchanged. Each new picture is shown at vblank `belongs + 2`, and repeats fill the gaps. Same metrics as today's `window` line.
- **Spout loss:** the sender is gone, the handle changed, or there are no new pictures for N s while the sender list says it is alive.
  - The engine re-opens the sender and keeps the last picture on the wall meanwhile (never black for a missing sender).
  - Status shows `sender: lost` with its age.
- **Display target lost** (specialization dropped or the display vanished): the engine re-acquires the target after the topology keeper restores it.
- **Startup order:**
  1. Topology OK (Surround matches the definition).
  2. The wall is specialized.
  3. Acquire the target.
  4. Open Spout.
  5. Run.

### Topology keeper

- **Wall definition:** a TOML file in `C:\ProgramData\WallBridge\config.toml`.
  - Surround grid 1×3, the NVAPI display ids in order A, B, C, resolution 7680×1080, refresh 59.978.
  - The pinned grid blob (or its fields), the wall's StableMonitorId substring (`GSM779465795`), the Spout sender name.
  - The grabber display (0x80061081) stays its own 1×1 grid and stays on the desktop.
- **Check:** read-only, every 10 s and on display-change events.
- **On drift or collapse:**
  1. Go from the collapsed span to separate displays.
  2. Re-form the 1×3 grid.
  3. Verify by reading it back.
  4. Re-specialize.
  5. The engine re-acquires.

  Every step is logged as a state transition.
- **Safety:**
  - Back-off and a max-attempts counter. After repeated failure, stop and show `topology: failed, needs attention` in the tray, dashboard and status, instead of looping.
  - No topology action while the topology already matches.
  - The proven Python sequence is the reference. The Rust port must reproduce it step for step and is tested against the real PC (see E2E).
- **Specialization:** set through an API or registry mechanism, not UIA on Settings. Which mechanism is left to the builder (spike).
  - Known facts: the Settings toggle "Remove display from desktop" works; Windows renumbers displays after a toggle.
  - The proven fallback is `spec_toggle.ps1`. It may stay as a temporary path only if the spike fails, recorded on #2.

### Status API, dashboard, SongPlayer interface

- **axum** on a fixed port. My call: 8930, next to SongPlayer's 8920. LAN-reachable like the siblings.
- **`GET /api/status`:**
  - `version`, `state` (`ok` / `degraded` / `failed`)
  - `display` (specialized, target acquired, mode)
  - `topology` (matches definition, last restore)
  - `sender` (name, connected, last picture age ms)
  - `pacing` (new / repeats / missed vblanks over the last 10 s, arrival p50/p90)
  - `uptime`

  Field names are proposed to songplayer over `peer_send` first. The interface is agreed before it is built.
- **Dashboard:** one static page served by axum. It shows the status above and the version (`v<semver>`, build-time injected, verified by a DOM read after deploy, per `version-on-dashboard`).

### Tray

airuleset `rust-web-tray`: Status (tooltip + menu), Open wall-bridge (dashboard URL), Copy URL, Exit (tray only, the engine keeps running). Autostart at logon. Tauri 2 `tray-icon` + single-instance, as in iemmixer `crates/iem-tray`. Polls `/api/status`.

### Install and autoupgrade (devbridge pattern)

- **Installer:** NSIS installer built in CI. Installs to `C:\ProgramData\WallBridge\` (my call, the owner left the folder to this project). Registers the engine task, the tray Run key and the updater task.
- **Moving old files:** the helper scripts move out of SongPlayer's diag folder into `WallBridge\tools\`: `displays.ps1`, the NVAPI Python scripts as reference, `capture\grab_ts.py`, `capture\lat_pair.py`.
- **Updater:** a SYSTEM task at startup + every 6 h, reading GitHub releases (`dev-latest` for dev, stable from `main`).
  - Patch-only on the same major.minor.
  - SHA256 check; the previous version is kept for rollback.
  - Kill-switch file `C:\ProgramData\WallBridge\autoupdate.disabled`.
  - Any error is a no-op.
- **Upgrade safety:** an upgrade restarts the engine, which is a short wall blink.
  - The updater installs only when status shows no picture change for the last X min, or in a configured quiet window. My call, builder may refine.
  - It never retries in a loop.
  - It verifies after install: the new engine is healthy within 30 s, else roll back.
- **Cutover from the MVP:** the installer stops the hand-started `wb2.exe` and takes over. The old folder is left in place until the owner confirms (data is not deleted without asking).

### Logging

`tracing` + `tracing_appender` daily rolling files, 14 kept, in `C:\ProgramData\WallBridge\logs`. One structured line per 10 s pacing window (as the MVP), and every state transition, topology action, Spout re-open and upgrade step. The `--arrivals` measurement stays as an opt-in, bounded flag (#1 item 6).

## Testing and CI

- **Build:** CI on GitHub `windows-latest` (house pattern), plus Linux jobs for the pure crates. Local dev keeps the existing `x86_64-pc-windows-gnu` cross-build on dev1.
- **CI jobs:** fmt, clippy, test (Linux + Windows), coverage floor (`cargo llvm-cov` vs `.github/coverage-floor`, start at the level reached, never lowered), diff-scoped `cargo mutants --in-diff`, version check (dev > main), Windows build + NSIS, release (`dev-latest` / stable).
- **Unit tests (pure crates):**
  - Pacing scheduler fed with recorded MVP arrival traces (`arrB.txt` exists on SNV): 25 fps and 30 fps material, phase jitter.
  - Health state machine.
  - Topology decision (matches / drift / collapse → actions).
  - Upgrade decision rules.
  - Status JSON shape.
- **E2E on SNV** (self-hosted runner on the real PC, devbridge pattern):
  - Install/upgrade over the running version with the tray up.
  - `/api/status` healthy, version matches the release.
  - Engine shows ~300 new pictures / 10 s with 0 missed vblanks over 60 s.
  - Spout re-open: kill and restart a test Spout sender (not Arena), status recovers with no engine restart.
  - Dashboard version by Playwright DOM read, zero console errors.
- **Destructive topology E2E** (collapse → restore) changes the live wall. It runs as a manually dispatched workflow, outside events, never on every push. My call; the owner allows short black-wall tests outside events.
- **Picture proof after cutover:** the grabber/arrivals measurement from #1 repeated once, "zmerané X".

## Calls made after the builder check (mine, owner can override)

- **Reboot scope.** `Resolume` logs on automatically. wall-bridge recovers on its own as soon as that session exists and Arena sends Spout. Starting Arena is not wall-bridge's job: SongPlayer's `SP-ArenaLaunch` task and the operator own it.
- **Event mode.** No event signal is read. A collapsed or drifted wall is already broken, so the keeper restores right away. The keeper never touches a topology that matches the definition.
- **Upgrade gate.** The updater installs only in a night window, 03:00–05:00 local, by default. Outside the window it only downloads. The kill-switch file still applies.
- **Release channel.** SNV follows stable (`main`) through the updater. Dev builds reach SNV only through the CI E2E job on the self-hosted runner, which installs over the production install. A short blink outside events is fine per the owner. The updater never downgrades: a dev build sits there until a stable release has a higher version.
- **E2E feed.**
  - On every dev push: install, version, `/api/status` healthy, sender connected, 0 missed vblanks over 60 s on the live Arena feed, dashboard DOM version check, zero console errors.
  - The rate check (~300 new / 10 s) and the Spout re-open check need moving content and a sender that can be killed. CI starts its own test Spout sender with fixed 25/30 fps content, and the engine is pointed at it for ≤ 60 s, then back to Arena.
  - Those checks run only in the manually dispatched hardware workflow, together with the destructive topology test, outside events.
- **OS updates.** The service does not change Windows Update or driver policy. The blocks set on 28.10. stay as they are, see #1 history. The dashboard and status show a warning when the OS build or NVIDIA driver drifts from 25H2 / 581.57.
- **Preset dependency.** Everything except the topology keeper's restore path can be built before the preset answer. If the owner says no, the restore path also handles a stalled Arena by status only (`arena: stalled, restart needed`), and the service still never restarts Arena.

## Left to the builder

- Exact NVAPI FFI approach (`nvapi` crate vs own bindings), as long as it reproduces the proven sequence.
- Specialization mechanism (spike).
- Session 0 vs logon-task variant (spike result decides).
- Quiet-window rule details for upgrades.
- Port choice, if 8930 is taken.

## Owner decisions (29.10.2026)

- Design approved.
- Drop `LG wall` from Arena preset `snv surround 7680`: yes. Done 29.10. 23:3x by editing the preset file (backup `C:\ProgramData\WallBridge\backup\arena-20261029\`). Arena reads the preset at start, so the change is live from the next Arena start. The A/B/A stall test runs after that start.
- Auto-logon of `Resolume` is already active since the first install. Nothing to change.
