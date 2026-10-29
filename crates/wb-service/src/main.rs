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
        // The topology keeper comes with plan task C8; until then the layout is not checked.
        topology: TopologyState {
            matches: true,
            last_restore_ms: None,
            failures: 0,
            gave_up: false,
        },
        sender: wb_core::sender::SenderWatch::new(&cfg.sender.name).state(0),
        pacing: Default::default(),
        warnings: vec!["Surround keeper not active yet (plan C8)".into()],
    }));

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

#[cfg(not(windows))]
fn main() {
    eprintln!("wall-bridge runs on Windows only");
    std::process::exit(1);
}
