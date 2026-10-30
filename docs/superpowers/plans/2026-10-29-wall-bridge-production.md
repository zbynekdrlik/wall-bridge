# wall-bridge production Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the MVP (`src/main.rs`, `src/dc.rs`) into a production Windows app that keeps the SNV wall on the bridge with no human. The app includes a tray, an installer, autoupgrade, a Surround keeper and CI/E2E.

**Architecture:** A Cargo workspace.
- Pure crates (`wb-core`) hold every decision and are tested and mutated on Linux.
- Thin Windows crates wrap Spout, Display.Core and NVAPI.
- One engine binary (`wb-service`) runs the pacing loop, the topology keeper and an axum status API.
- A Tauri tray, an NSIS installer and a PowerShell updater follow the devbridge / iemmixer house pattern.

**Tech Stack:** Rust 2021, `windows` 0.58, axum 0.7, tokio, serde/toml, tracing + tracing-appender, Tauri 2 (tray only), NSIS, PowerShell 5.1 + Pester, GitHub Actions (ubuntu + windows-latest + a self-hosted runner on SNV).

**Spec:** `docs/superpowers/specs/2026-10-29-wall-bridge-production-design.md`. Ticket #2.

## Global Constraints

- **Machine:** `win-resolume` 10.77.9.201, Win11 IoT Ent 25H2, NVIDIA 581.57. Reach it ONLY through the `win-resolume` MCP or the self-hosted runner, never ssh.
- **Wall:** Surround span 7680×1080@59.978. StableMonitorId substring `GSM779465795`. NVAPI display ids A=0x80061086, B=0x80061087, C=0x80061083. Grabber 0x80061081 stays a 1×1 grid on the desktop.
- **Sender:** Spout sender `Arena - Screen 2`, mutex `<name>_SpoutAccessMutex`, sender list `SpoutSenderNames`, 280 B `SharedTextureInfo`.
- **Install root:** `C:\ProgramData\WallBridge\`. Config `config.toml`, logs `logs\`, kill-switch `autoupdate.disabled`.
- **Status API:** port 8930, `GET /api/status`, dashboard `GET /`.
- **Pacing:** each new picture shows at vblank `belongs + 2`; ring of 5 textures; guard 3500 µs before the next vblank.
- **Quality floor:** 0 missed vblanks in steady state; ~300 new / 10 s on ytfast; latency ≤ MVP.
- **Never:**
  - restart Arena, start clips, or change the Arena composition;
  - change the topology while it matches the definition;
  - change Windows Update or driver policy.
- **Version:** bump first on dev, dev > main, one version in `[workspace.package]`.
- **Logs:** daily rolling, 14 files kept.
- **Tests:** no skipped or ignored tests; mocks only for external network services.
- **Owner-facing text:** plain Slovak.

## Review Focus

1. **Arena restarts while the engine runs** (new share handle, the sender is briefly missing). Expected: the last picture holds, the sender re-opens, new pictures resume, and the process does not restart. Test: Task 3 `reopen_on_handle_change`, `hold_while_missing`.
2. **Arena shows a static scene** (a sender is alive, but no new pictures come for minutes). Expected: NOT treated as a lost sender; status `ok`, `last_picture_age` grows. Test: Task 3 `static_scene_is_not_loss`.
3. **Sender resolution differs from the wall** (e.g. Arena at 3840×540 later). Expected: no crash, picture top-left as the MVP does; status shows the sender size. Test: Task 4 `size_mismatch_reported`.
4. **Topology read fails transiently** (an NVAPI call errors once). Expected: no restore action from a single failed read; only a confirmed mismatch triggers a restore. Test: Task 8 `failed_read_is_not_drift`.
5. **The updater runs while the engine is unhealthy, or outside the window.** Expected: no install outside 03:00–05:00; after an install, roll back if the engine is not `ok` within 30 s. Test: Task 10 Pester `outside_window_no_install`, `rollback_when_unhealthy`.

---

## Milestone A: service on the wall (priority, the owner: "ship the service first")

### Task 1: Workspace, version, CI skeleton

**Files:**
- Modify: `Cargo.toml` (becomes the workspace root; `[workspace.package] version = "0.2.0"`).
- Move: `src/main.rs`, `src/dc.rs` → `crates/wb-mvp/src/` (kept building until Task 6 replaces it, then deleted).
- Create: `crates/wb-core/{Cargo.toml,src/lib.rs}`
- Create: `.github/workflows/ci.yml`, `.github/coverage-floor`, `scripts/check_version.py` (copy from fohmixer, base `origin/main`).
- Modify: `CLAUDE.md` (Playbook router, build commands).

**Interfaces:**
- Produces: workspace members `crates/*`; CI jobs `fmt`, `clippy`, `test-linux`, `coverage`, `mutants`, `version`, `windows-build`, `all-pass`.

- [ ] **Step 1:** Bump to 0.2.0 and commit alone (`chore: bump version to 0.2.0`).
- [ ] **Step 2:** Restructure into the workspace. Check that `cargo build --release --target x86_64-pc-windows-gnu -p wb-mvp` still builds.
- [ ] **Step 3:** Write `ci.yml`.
  - Jobs: `fmt`, `clippy -D warnings`, `test-linux` (`cargo test -p wb-core`), `coverage` (`cargo llvm-cov -p wb-core --json` vs `.github/coverage-floor`, start value 90), `mutants` (`cargo mutants --in-diff` on PRs, pure crates only), `version` (PRs to main), `windows-build` (windows-latest, `cargo build --release --workspace`), `all-pass`.
  - `concurrency: { group: "ci-${{ github.workflow }}-${{ github.ref }}", cancel-in-progress: true }`. No `continue-on-error`.
- [ ] **Step 4:** Push, monitor CI to all green.
- [ ] **Step 5:** Commit (`ci: workspace + CI skeleton`).

### Task 2: Pacer (pure) extracted from the MVP

**Files:**
- Create: `crates/wb-core/src/pacing.rs`, `crates/wb-core/tests/pacing.rs`, `crates/wb-core/tests/data/arrB-25fps.txt` (a 60 s slice of `C:\ProgramData\SongPlayer\diag\wall-bridge\mvp\arrB.txt`, fetched via MCP FileRead).

**Interfaces:**
- Produces:
  - `pub struct Pacer` with `Pacer::new(ring_len: usize) -> Pacer`
  - `fn arrive(&mut self, off_vb: f64, vb_index: u64) -> Option<usize>`
    - `off_vb` = time since the last vblank in periods.
    - Returns the ring slot to copy into, or `None` when no slot is free (an MVP drop).
  - `fn on_vblank(&mut self, vb_index: u64) -> Show`, where `enum Show { New(usize), Repeat(Option<usize>) }`.
  - `fn window(&mut self) -> PacingWindow` returns and resets the counters: `new, repeats, drops, runs: [u64; 9], interval_vb: f64, phase_mean: f64, queue: usize`.
- The algorithm is the MVP's (`src/main.rs` lines ~578–635 before the move):
  - Circular mean of `off.fract()` with α = 0.03.
  - `d = ((off - mean + 1.5) mod 1) - 0.5`, `m = off - d`.
  - `belongs = vb-1 | vb | vb+1` for `m < 0 | m >= 1 | otherwise`.
  - Show at `belongs + 2`.
  - `interval_vb` EMA α = 0.05 over 0.5 < iv < 6.

- [ ] **Step 1: Write failing tests**
  - `steady_30fps_alternates_2_vblanks`: arrivals every 2.0 vb at phase 0.3 → after warm-up, runs[2] ≥ 95 % of runs, 0 drops.
  - `jitter_near_boundary_keeps_cadence`: phase alternating 0.97 / 0.03 → no run of 1 or 3 caused by the jitter.
  - `trace_25fps_matches_mvp`: replay `arrB-25fps.txt` (columns `epoch_us vb_index show_at`), recompute `off` from the epoch deltas → the `show_at` values match the file in ≥ 99 % of lines.
  - `ring_full_reports_drop`: 6 arrivals with no vblank → the 6th `arrive` returns `None`, `window().drops == 1`.
- [ ] **Step 2:** `cargo test -p wb-core pacing` → FAIL (no `Pacer`).
- [ ] **Step 3:** Implement `pacing.rs`.
- [ ] **Step 4:** Tests pass, then `cargo mutants -p wb-core --file crates/wb-core/src/pacing.rs` → 0 survivors.
- [ ] **Step 5:** Commit.

### Task 3: Sender watch, health and status model (pure)

**Files:**
- Create: `crates/wb-core/src/{sender.rs,health.rs,status.rs,config.rs}`, tests next to each in `crates/wb-core/tests/`.

**Interfaces:**
- Produces:
  - `SenderWatch::new(cfg: &SenderCfg)`
    - `fn tick(&mut self, now_ms: u64, seen: Option<SenderSeen>, new_picture: bool) -> SenderAction`
    - `SenderSeen { handle: u32, width: u32, height: u32, listed: bool }`
    - `enum SenderAction { Keep, Open(u32), Reopen(u32), Hold }`
  - `SenderState { name, connected, width, height, last_picture_age_ms }`
  - `Health::evaluate(&Inputs) -> State`, where `enum State { Ok, Degraded(Vec<Reason>), Failed(Vec<Reason>) }`
  - `Status` (serde, the `/api/status` body):
    - `version, state, uptime_s`
    - `display { specialized, target_acquired, width, height, hz }`
    - `topology { matches, last_restore_ms, failures }`
    - `sender: SenderState`
    - `pacing: PacingWindow`
    - `warnings: Vec<String>` (OS/driver drift)
  - `Config::load(path: &Path) -> Result<Config>` with defaults from Global Constraints; `SenderCfg { name, check_every_vb: 30 }`.
- [ ] **Step 1: Write failing tests**
  - `reopen_on_handle_change`: handle 0x40002c42 → 0x40003000 → `Reopen(0x40003000)`.
  - `hold_while_missing`: `seen = None` → `Hold`, `connected = false`, no panic.
  - `static_scene_is_not_loss`: listed, same handle, no new picture for 600 s → `Keep`, health `Ok`.
  - `status_json_shape`: serialize a sample `Status` → JSON has exactly the agreed keys (snapshot string in the test).
  - `config_defaults`: an empty TOML → port 8930, sender `Arena - Screen 2`, monitor `GSM779465795`.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Pass + mutants 0 survivors.
- [ ] **Step 5:** Send the `Status` JSON shape to songplayer via `peer_send`. Record the agreed shape on #2 (ROZHODNUTÉ) before Task 6 exposes it. Commit.

### Task 4: Windows I/O crates: `wb-spout` and `wb-scanout`

**Files:**
- Create: `crates/wb-spout/src/lib.rs` (port of MVP `read_sender`, `list_senders`, mutex lock, signature sampling).
- Create: `crates/wb-scanout/src/lib.rs` (port of `dc.rs`).

**Interfaces:**
- Produces:
  - `wb_spout::Reader::new(device: &ID3D11Device, name: &str)`
    - `fn seen(&self) -> Option<SenderSeen>`
    - `fn open(&mut self, handle: u32) -> Result<()>`
    - `fn poll_new(&mut self, ctx, dst: &ID3D11Texture2D) -> Result<bool>` (true = a new picture was copied into `dst`)
    - `fn size(&self) -> (u32, u32)`
  - `wb_scanout::Scanout::open(monitor_substr: &str, device: &ID3D11Device) -> Result<Scanout>`
    - `fn wait_vblank(&self) -> Result<()>`
    - `fn show(&mut self, ctx, src) -> Result<()>`
    - `fn mode(&self) -> (u32, u32, f64)`
  - `wb_scanout::list_targets() -> Result<Vec<String>>`
- [ ] **Step 1:** Port the code with no behaviour change. Errors are `windows::core::Result` with a step name, as in `dc.rs::step`.
- [ ] **Step 2:** Windows integration test `crates/wb-spout/tests/sender.rs::size_mismatch_reported`. It runs on windows-latest: start an in-process test sender (shared texture + 280 B mapping + mutex) of 3840×540, `seen()` returns that size, and `poll_new` copies with no error.
- [ ] **Step 3:** CI `windows-build` runs `cargo test -p wb-spout`. Green.
- [ ] **Step 4:** Commit.

### Task 5: Spike: session 0 vs user session (on SNV)

**Files:**
- Create: `docs/spikes/2026-10-session0.md`.

- [ ] **Step 1:** Build `wb-mvp` with `--seconds 20 --log spike.jsonl` and run it through a SYSTEM scheduled task (`/ru SYSTEM`) while the production MVP is stopped for ≤ 30 s. Restart the production MVP right after (owner rule: never leave the bridge off).
- [ ] **Step 2:** Record `dc_open` or the error. Pick the variant:
  - works → engine = SYSTEM startup task;
  - fails → engine = logon task of `Resolume`.
- [ ] **Step 3:** Write the result as ROZHODNUTÉ on #2 and in the spike doc. Commit.

### Task 6: `wb-service` engine + status API + logs

**Files:**
- Create: `crates/wb-service/src/{main.rs,engine.rs,api.rs,logging.rs,instance.rs}`, `crates/wb-service/static/index.html`.
- Delete: `crates/wb-mvp` (after Task 9 cutover is proven).

**Interfaces:**
- Consumes: Tasks 2–4.
- Produces:
  - Binary `wall-bridge.exe` with flags `--config <path>` (default `C:\ProgramData\WallBridge\config.toml`) and `--arrivals <path>` (opt-in, size-capped 50 MB then stops writing).
  - `engine::run(cfg, status: Arc<RwLock<Status>>) -> !` runs on a dedicated `THREAD_PRIORITY_HIGHEST` thread with `timeBeginPeriod(1)`.
  - `api::router(status) -> axum::Router` serves `/api/status` and `/` (dashboard showing `v{version}`, injected via `env!("CARGO_PKG_VERSION")`).
- [ ] **Step 1:** Unit test `api_status_returns_version` (axum `oneshot`, Linux): the body `version` equals `CARGO_PKG_VERSION`.
- [ ] **Step 2:** Implement the engine.
  - Startup order: scanout open → reader open → loop.
  - Each vblank: `on_vblank` → show.
  - Between vblanks: poll the sender every ~1 ms until the guard.
  - `SenderWatch.tick` every 30 vb.
  - One `tracing` info line per 10 s window with the same fields as the MVP `window` event.
  - Scanout loss → re-open with back-off 1/2/5/10 s, status `Degraded`.
- [ ] **Step 3:** Single instance via named mutex `Global\WallBridgeEngine`; a second instance logs and exits 0.
- [ ] **Step 4:** `logging.rs`: `tracing_appender::rolling` daily, prefix `engine`, `max_log_files(14)`, in `C:\ProgramData\WallBridge\logs`.
- [ ] **Step 5:** CI green. Commit.

### Task 7: Own self-hosted runner on SNV

**Files:**
- Create: `docs/runner.md`.

- [ ] **Step 1:** `gh api -X POST repos/zbynekdrlik/wall-bridge/actions/runners/registration-token`.
- [ ] **Step 2:** Through MCP: install a second runner instance in `C:\actions-runner-wall-bridge\`.
  - Labels `self-hosted,snv,wall-bridge`.
  - Started like the existing songplayer runner: an interactive scheduled task of `Resolume`, RunLevel Highest, so jobs run in session 1.
  - Do not touch `C:\actions-runner\` (songplayer's).
- [ ] **Step 3:** `gh api repos/zbynekdrlik/wall-bridge/actions/runners` → the runner is `online`. Record on #2.

### Task 8 is in Milestone C (topology); Milestone A continues with Task 9.

### Task 9: Install script, cutover and E2E on SNV

**Files:**
- Create: `installer/install.ps1`, `installer/uninstall-mvp.ps1`, `installer/tests/install.Tests.ps1`.
- Create: `crates/wb-e2e/src/main.rs`.
- Modify: `.github/workflows/ci.yml` (jobs `e2e-deploy`, `e2e-test` on `[self-hosted, snv, wall-bridge]`, dev pushes only).

**Interfaces:**
- Consumes: the Task 5 variant, Task 6 binary.
- Produces:
  - `install.ps1 -Source <dir>`: copies to `C:\ProgramData\WallBridge\app\<version>\`, keeps the previous version, and switches `current`.
  - It registers the engine task (Task 5 variant, restart on failure every 1 min, unlimited) and a watchdog check, writes a default `config.toml` if missing, and starts the engine.
  - `wb-e2e` exits 0 only if all checks pass.
- [ ] **Step 1:** Pester tests: `keeps_previous_version`, `config_not_overwritten`, `task_registered_with_restart`.
- [ ] **Step 2:** `uninstall-mvp.ps1` stops `wb2.exe` and leaves `C:\ProgramData\SongPlayer\diag\wall-bridge\` in place (no data deletion without the owner).
- [ ] **Step 3:** `wb-e2e` checks against `http://127.0.0.1:8930/api/status`:
  - `version` == the built version;
  - `display.target_acquired` and `specialized`;
  - `sender.connected`;
  - 6 consecutive windows with `missed_vblanks == 0`;
  - Playwright `e2e/dashboard.spec.ts`: the DOM shows `v<version>`, zero console errors.
- [ ] **Step 4:** Push → `e2e-deploy` installs over the MVP (first run) → `e2e-test` green.
- [ ] **Step 5:** Measure after cutover: a grabber/arrivals run as in #1 (`grab_ts.py` + `lat_pair.py` with `--arrivals`). Post "zmerané X" on #2. Check `#1` item 7 is superseded.
- [ ] **Step 6:** Release `dev-latest` from dev, stable from main (`release.yml`, assets: zip + `SHA256SUMS` + `VERSION`).

## Milestone B: tray, installer, autoupgrade

### Task 10: Tray + NSIS + updater

**Files:**
- Create: `crates/wb-tray/` (copy the structure of iemmixer `crates/iem-tray`: pure `view.rs` / `poll.rs` tested on Linux, Tauri glue Windows-only).
- Create: `installer/nsis/` (or Tauri NSIS bundle as in devbridge `crates/devbridge-app/tauri.conf.json`), `installer/autoupdate.ps1`, `installer/tests/autoupdate.Tests.ps1`.

**Interfaces:**
- Tray menu (airuleset `rust-web-tray`):
  - Status (tooltip + first item: `Stena: OK` / `Stena: problém – <reason>`)
  - `Otvoriť wall-bridge` → `http://10.77.9.201:8930/`
  - `Kopírovať URL`
  - `Koniec` (tray only)
- The tray polls `/api/status` every 5 s with a 2 s timeout. Autostart via the HKLM Run value `WallBridgeTray`.
- `autoupdate.ps1`:
  - SYSTEM task at startup + every 6 h; channel stable.
  - Download any time; install only 03:00–05:00 local.
  - SHA256 check; patch-only on the same major.minor; never downgrade.
  - Kill-switch file.
  - After install, poll status 30 s → roll back to `app\<previous>` if not `ok`.
  - Any error → log + exit 0.
- [ ] **Step 1:** Linux tests for the tray view: `ok_status_label`, `down_when_unreachable`, `degraded_shows_reason`.
- [ ] **Step 2:** Pester: `outside_window_no_install`, `rollback_when_unhealthy`, `no_downgrade`, `killswitch_respected`.
- [ ] **Step 3:** Implement, CI builds the installer, E2E installs it on SNV and asserts the tray process runs in session 1 and the updater task exists.
- [ ] **Step 4:** Commit; tick on #2.

## Milestone C: Surround keeper and specialization

### Task 8: Topology decision (pure) + NVAPI/specialization (Windows) + keeper

**Files:**
- Create: `crates/wb-core/src/topology.rs`, `crates/wb-topology/src/{nvapi.rs,mosaic.rs,special.rs}`, `crates/wb-service/src/keeper.rs`.
- Reference: `C:\ProgramData\SongPlayer\diag\wall-bridge\nvapi_grid_reform_keep.py`, `nvapi_grid_read.py`, blob `cycle-20261013-111323.log.pinned.bin`, `spec_toggle.ps1` (fetch via MCP FileRead into `docs/reference/` first).

**Interfaces:**
- Produces:
  - `topology::decide(def: &WallDef, read: Result<Observed, ReadErr>, history: &KeeperHistory) -> KeeperAction`
    - `enum KeeperAction { None, Restore(Vec<Step>), GiveUp }`
    - `Step = Separate | FormGrid | VerifyGrid | Specialize | ReacquireScanout`
  - `wb_topology::read() -> Result<Observed>`
  - `wb_topology::apply(step: Step, def: &WallDef) -> Result<()>`
  - `special::is_specialized(monitor_substr) -> Result<bool>`, `special::set(monitor_substr, on: bool) -> Result<()>`
- [ ] **Step 1: Write failing tests**
  - `matching_topology_no_action`
  - `collapsed_span_restores_via_separate` (observed single 3840×2160 display 0x80061088 → `[Separate, FormGrid, VerifyGrid, Specialize, ReacquireScanout]`)
  - `failed_read_is_not_drift` (one `Err` → `None`; three consecutive → still `None`, status warning)
  - `gives_up_after_3_failed_restores_in_30_min`
  - `grabber_grid_untouched`
- [ ] **Step 2:** Implement the pure decision. Mutants 0 survivors.
- [ ] **Step 3:** Port the NVAPI calls step for step from the Python reference (same call order, same pinned blob). Pick the specialization mechanism by spike (an API or registry path, never UIA). Record the choice on #2. Fallback `spec_toggle.ps1` only if the spike fails, recorded as ROZHODNUTÉ.
- [ ] **Step 4:** Keeper loop in `wb-service`: read every 10 s + on `WM_DISPLAYCHANGE`, act only on a `Restore` decision, and log each step as a state transition.
- [ ] **Step 5:** `hardware.yml` (workflow_dispatch only, runner `snv`), outside events:
  1. A/B/A Arena stall test: toggle specialization off/on, Spout must keep ~300 new / 10 s. This proves the `LG wall` removal; needs one Arena start after 29.10.
  2. Collapse → restore: force separate displays, the keeper re-forms, the wall is back within 60 s.
  3. Test-sender rate and re-open checks (≤ 60 s on a 25 fps test sender, then back to Arena).

  Run it once and post the measured values on #2.
- [ ] **Step 6:** Commit; tick on #2.

## Milestone D: close-out

- [ ] SongPlayer reads `/api/status` (songplayer#259); confirm over `peer_send`.
- [ ] Move `displays.ps1`, `grab_ts.py`, `lat_pair.py` and the NVAPI reference scripts into `C:\ProgramData\WallBridge\tools\` via the installer.
- [ ] Close #1 items 1, 2, 3, 6, 7 with evidence; items 4 and 5 stay open on #1.
- [ ] Playbook: `.claude/rules/snv-windows.md` (MCP gotchas, Arena quit dialog, preset facts) and router in `CLAUDE.md`.
