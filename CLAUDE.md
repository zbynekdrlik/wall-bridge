# wall-bridge — Project Instructions

A Windows process on the SNV Resolume PC (`win-resolume`, 10.77.9.201). It reads Resolume Arena's Spout output (`Arena - Screen 2`, 7680×1080) and shows it on the LED/TV wall (NVIDIA Surround 7680×1080@60).

How it works:
- It takes the wall display off the Windows desktop (Display.Core "specialized" display).
- It paces each picture onto the wall's vblanks itself.

It started as an MVP inside songplayer (zbynekdrlik/songplayer#259, 29.10.2026). Measured there:
- the old judder on ytfast 25 fps songs is gone;
- it adds ~2–3 ms latency compared with Arena's direct output.

## Build
- Rust, `windows` 0.58, cross-compiled on dev1 for `x86_64-pc-windows-gnu`. The target and mingw are provided system-wide; do not install toolchains in the account.
- Workspace: pure logic in `crates/wb-core` (tested on Linux: `cargo test -p wb-core`), Windows crates beside it.
- `cargo build --release --target x86_64-pc-windows-gnu --workspace`
- Plan: `docs/superpowers/plans/2026-10-29-wall-bridge-production.md`, ticket #2.

## Deploy / test
- Through the `win-resolume` MCP (remoteos gateway).
- Today's MVP binary and helper scripts live in `C:\ProgramData\SongPlayer\diag\wall-bridge\` (`wb2.exe`, `spec_toggle.ps1`, `displays.ps1`).
- Grabber capture scripts live in `C:\ProgramData\SongPlayer\diag\capture\`.
- Moving to its own folder (e.g. `C:\ProgramData\WallBridge\`) is this project's decision.
- Run by hand today: `wb2.exe --sender "Arena - Screen 2" --dc GSM779465795 --seconds 86400`.

## Owner rules (carried over from songplayer)
- Never change the operator's Arena composition permanently without his OK.
- Clips run only from his Stream Deck.
- Event mode = hands off.

## Related projects
- songplayer: SongPlayer's wall report must learn the bridge state, because the wall is no longer a desktop display.
- Reach it through the peer channel (`peer_send`).
