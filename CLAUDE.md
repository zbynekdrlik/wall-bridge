# wall-bridge — Project Instructions

A Windows process on the SNV Resolume PC (`win-resolume`, 10.77.9.201). It reads Resolume Arena's Spout output (`Arena - Screen 2`, 7680×1080) and shows it on the LED/TV wall (NVIDIA Surround 7680×1080@60).

How it works:
- It takes the wall display off the Windows desktop (Display.Core "specialized" display).
- It paces each picture onto the wall's vblanks itself.

It started as an MVP inside songplayer (zbynekdrlik/songplayer#259, 29.10.2026). Measured there:
- the old judder on ytfast 25 fps songs is gone;
- it adds ~2–3 ms latency compared with Arena's direct output.

## Build
- Rust 2021 workspace, `windows` 0.58. Pure logic lives in `crates/wb-core` and is tested and mutated on Linux; the Windows crates sit beside it.
  - `wb-spout`: Spout reader.
  - `wb-scanout`: Display.Core.
  - `wb-topology`: NVAPI Mosaic.
  - `wb-service`: `wall-bridge.exe`, the engine, keeper and API.
  - `wb-tray`: `wall-bridge-tray.exe`.
- Local cross-build on dev1: `cargo build --release --target x86_64-pc-windows-gnu --workspace`. The target and mingw are system-wide; do not install toolchains in the account.
- Local checks before a push: `cargo fmt --all --check`, `cargo clippy -p wb-core -p wb-service -p wb-tray -p wb-topology --all-targets -- -D warnings`, the same clippy with `--target x86_64-pc-windows-gnu`, `cargo test -p wb-core -p wb-service -p wb-tray -p wb-topology`.
- CI (`.github/workflows/ci.yml`) is the merge gate:
  - fmt, clippy, Linux tests, coverage floor (`.github/coverage-floor`, wb-core), diff-scoped mutants (wb-core), version check;
  - Windows build + tests, Pester installer tests, Playwright dashboard test (`e2e/`);
  - release: `dev-latest` from dev, `v<version>` from main.
- Spec: `docs/superpowers/specs/2026-10-29-wall-bridge-production-design.md`. Plan: `docs/superpowers/plans/2026-10-29-wall-bridge-production.md`. Ticket #2.

## Deploy / test
- SNV (`win-resolume`, 10.77.9.201) is reached only through the `win-resolume` MCP.
- Install / upgrade: download a release zip (verify `SHA256SUMS`), expand it, run `install.ps1 -Source <dir>`.
  - It installs into `C:\ProgramData\WallBridge\app\<version>` and keeps one rollback version.
  - It registers the tasks `\WallBridge\` Engine (logon of `Resolume`, restart every minute), Tray, Watchdog (SYSTEM, every minute) and Update (SYSTEM, hourly).
- Autoupgrade follows the stable release, installs only 03:00–05:00, and rolls back when unhealthy. `autoupdate.disabled` stops it; `channel.txt` = `dev` follows `dev-latest`.
- Status: `http://10.77.9.201:8930/api/status`, dashboard `http://10.77.9.201:8930/`. Logs: `C:\ProgramData\WallBridge\logs\` (engine, install, watchdog); tray log `%LOCALAPPDATA%\WallBridge\logs`.
- Diagnostics on the PC: `wall-bridge.exe --topology read` (and `separate` for a restore test, wall goes dark).
- The old MVP stays in `C:\ProgramData\SongPlayer\diag\wall-bridge\mvp\` for rollback. Grabber capture scripts: `C:\ProgramData\SongPlayer\diag\capture\`.

## Playbook router
- SNV / Windows / Arena gotchas → `.claude/rules/snv-windows.md` (auto-loads on its `paths:`)

## Owner rules (carried over from songplayer)
- Never change the operator's Arena composition permanently without his OK.
- Clips run only from his Stream Deck.
- Event mode = hands off.

## Related projects
- songplayer: SongPlayer's wall report must learn the bridge state, because the wall is no longer a desktop display.
- Reach it through the peer channel (`peer_send`).
