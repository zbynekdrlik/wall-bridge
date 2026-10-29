//! `wall-bridge.exe [--config <path>]`: the engine plus the status API.

#[cfg(windows)]
fn main() {
    use std::path::PathBuf;
    use std::sync::{Arc, RwLock};
    use tracing::{error, info};
    use wb_core::config::Config;
    use wb_core::health::State;
    use wb_core::status::{DisplayState, Status, TopologyState};

    const ROOT: &str = r"C:\ProgramData\WallBridge";

    let mut config_path = PathBuf::from(ROOT).join("config.toml");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => config_path = PathBuf::from(args.next().expect("--config <path>")),
            "--version" => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                return;
            }
            // Diagnostics for tests on the real PC: print the Mosaic grids, or dissolve the
            // wall into separate displays (what the keeper's first restore step does).
            "--topology" => {
                let what = args.next().unwrap_or_default();
                let cfg = Config::load(&config_path).unwrap_or_default();
                std::process::exit(topology_cli(&what, &cfg));
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }

    let _log = match wb_service::logging::init(&PathBuf::from(ROOT).join("logs")) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("logging: {e}");
            std::process::exit(1);
        }
    };
    let cfg = match Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            error!(err = %e, "config");
            std::process::exit(1);
        }
    };
    info!(version = env!("CARGO_PKG_VERSION"), config = %config_path.display(), ?cfg, "wall-bridge starting");

    let _instance = match wb_service::instance::acquire() {
        Ok(Some(i)) => i,
        Ok(None) => {
            info!("another wall-bridge engine is running, exiting");
            return;
        }
        Err(e) => {
            error!(err = ?e, "single-instance mutex");
            std::process::exit(1);
        }
    };

    let status = Arc::new(RwLock::new(Status {
        version: env!("CARGO_PKG_VERSION").into(),
        state: State::Failed(vec![wb_core::health::Reason::DisplayNotAcquired]),
        uptime_s: 0,
        display: DisplayState {
            specialized: false,
            target_acquired: false,
            width: 0,
            height: 0,
            hz: 0.0,
        },
        // Unknown until the keeper's first read (a few ms after start).
        topology: TopologyState {
            matches: false,
            last_restore_ms: None,
            failures: 0,
            gave_up: false,
        },
        sender: wb_core::sender::SenderWatch::new(&cfg.sender.name).state(0),
        pacing: Default::default(),
        warnings: vec![],
    }));

    if cfg.topology.enabled {
        let (kcfg, kstatus) = (cfg.clone(), status.clone());
        std::thread::Builder::new()
            .name("keeper".into())
            .spawn(move || wb_service::keeper::run(kcfg, kstatus))
            .expect("keeper thread");
    } else {
        info!("surround keeper disabled in config");
        status.write().expect("status lock").topology.matches = true;
    }

    let api_status = status.clone();
    let port = cfg.api.port;
    std::thread::Builder::new()
        .name("api".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(async move {
                let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
                match tokio::net::TcpListener::bind(addr).await {
                    Ok(l) => {
                        info!(%addr, "status api listening");
                        if let Err(e) = axum::serve(l, wb_service::api::router(api_status)).await {
                            error!(err = ?e, "status api stopped");
                        }
                    }
                    Err(e) => error!(%addr, err = ?e, "status api bind failed"),
                }
            });
        })
        .expect("api thread");

    if let Err(e) = wb_service::engine::run(cfg, status) {
        error!(err = ?e, "engine failed to start");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn topology_cli(what: &str, cfg: &wb_core::config::Config) -> i32 {
    use wb_topology::{Nvapi, RawGrid, PINNED};
    let nv = match Nvapi::load() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let read = match nv.read_raw() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let grids: Vec<_> = read.iter().map(RawGrid::to_grid).collect();
    println!("driver {:?}", nv.driver_version());
    for g in &grids {
        println!(
            "grid {}x{} {}x{}@{} ids {:x?}",
            g.rows, g.cols, g.width, g.height, g.freq, g.ids
        );
    }
    let def = cfg.topology.wall_def();
    println!("matches definition: {}", def.matches(&grids));
    match what {
        "read" => 0,
        "separate" => {
            let pinned = RawGrid::from_bytes(PINNED).expect("pinned grid");
            let want = def.separate_grids(&grids);
            let raw: Vec<RawGrid> = want
                .iter()
                .map(|g| {
                    read.iter()
                        .find(|r| {
                            g.ids.len() == 1
                                && def.keep_single.contains(&g.ids[0])
                                && r.to_grid() == *g
                        })
                        .copied()
                        .unwrap_or_else(|| RawGrid::from_grid(g, &pinned))
                })
                .collect();
            match nv.set_raw(&raw) {
                Ok(()) => {
                    println!("separated into {} grids", raw.len());
                    0
                }
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        other => {
            eprintln!("--topology read|separate, got '{other}'");
            2
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("wall-bridge runs on Windows only");
    std::process::exit(1);
}
