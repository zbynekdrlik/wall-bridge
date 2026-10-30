# Self-hosted runner on SNV

- Folder: `C:\actions-runner-wall-bridge` (runner 2.338.0), name `snv-wall-bridge`, labels `self-hosted, snv, wall-bridge`. Registered 30.10.2026.
- Start: scheduled task `GitHubActionsRunner-wall-bridge`, user `Resolume`, Interactive, RunLevel Highest. Triggers: logon of `Resolume` and every 5 min. `start-runner.vbs` quits when the listener already runs, then starts `start-runner.bat` hidden.
- Jobs run in session 1, the same session as Arena and Spout.
- songplayer's runner (`C:\actions-runner`, task `GitHubActionsRunner`) is separate; do not touch it.
- Health: `_diag\Runner_*.log` shows `Listening for Jobs`. The log is held open: read it with a shared `FileStream`. The repo token cannot list runners (403).
- Re-register: the owner takes a token from Settings → Actions → Runners → New self-hosted runner (Windows, the `--token` in Configure). Send it through `airuleset.py secret request`, then run `config.cmd --unattended --replace …` through `secret exec`.
