//! `wall-bridge-tray.exe`: the tray icon next to the engine (airuleset rust-web-tray).
//! Status in the tooltip and the first menu line, Open wall-bridge, Copy URL, Exit
//! (ends the tray only; the engine keeps running).
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    shell::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("wall-bridge-tray runs on Windows only");
    std::process::exit(1);
}

#[cfg(windows)]
mod shell {
    use std::sync::mpsc;
    use std::time::Duration;
    use std::time::Instant;
    use tracing::{error, info, warn};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
    use wb_tray::promote::{to_promote, IconEntry};
    use wb_tray::view::{icon_rgba, view, Light, Status, View};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
    use winreg::RegKey;

    const POLL_EVERY: Duration = Duration::from_secs(5);
    const POLL_TIMEOUT: Duration = Duration::from_secs(2);
    const LOCAL_STATUS: &str = "http://127.0.0.1:8930/api/status";
    /// The taskbar may not be ready yet right after logon (SNV 30.10.2026).
    const ICON_RETRY: Duration = Duration::from_secs(5);
    /// Explorer adds the icon's settings entry some time after the icon appears, and the user
    /// can hide it again: check this often.
    const PROMOTE_EVERY: Duration = Duration::from_secs(10);
    const ICON_SETTINGS: &str = r"Control Panel\NotifyIconSettings";

    /// The LAN URL people open from other machines (the dashboard).
    fn lan_url() -> String {
        std::env::var("WALLBRIDGE_URL").unwrap_or_else(|_| "http://10.77.9.201:8930/".into())
    }

    fn icon(light: Light) -> Icon {
        Icon::from_rgba(icon_rgba(light), 16, 16).expect("16x16 rgba icon")
    }

    fn fetch(agent: &ureq::Agent) -> Option<Status> {
        match agent.get(LOCAL_STATUS).call() {
            Ok(r) => match r.into_json::<Status>() {
                Ok(s) => Some(s),
                Err(e) => {
                    warn!(err = ?e, "status body did not parse");
                    None
                }
            },
            Err(e) => {
                warn!(err = %e, "status request failed");
                None
            }
        }
    }

    fn icon_entries() -> std::io::Result<Vec<IconEntry>> {
        let root =
            RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(ICON_SETTINGS, KEY_READ)?;
        Ok(root
            .enum_keys()
            .filter_map(Result::ok)
            .filter_map(|key| {
                let k = root.open_subkey_with_flags(&key, KEY_READ).ok()?;
                let path: String = k.get_value("ExecutablePath").ok()?;
                let promoted: Option<u32> = k.get_value("IsPromoted").ok();
                Some(IconEntry {
                    key,
                    path,
                    promoted,
                })
            })
            .collect())
    }

    /// Move this exe's icon out of the hidden overflow.
    fn keep_visible(exe: &str) {
        let entries = match icon_entries() {
            Ok(e) => e,
            Err(e) => {
                warn!(err = %e, "notify icon settings unreadable");
                return;
            }
        };
        for key in to_promote(&entries, exe) {
            let set = RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey_with_flags(format!(r"{ICON_SETTINGS}\{key}"), KEY_SET_VALUE)
                .and_then(|k| k.set_value("IsPromoted", &1u32));
            match set {
                Ok(()) => info!(key, exe, "tray icon promoted to the taskbar"),
                Err(e) => error!(key, err = %e, "tray icon promotion failed"),
            }
        }
    }

    fn open_in_browser(url: &str) {
        let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        let r = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(wide.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            )
        };
        if r.0 as isize <= 32 {
            error!(url, code = r.0 as isize, "could not open the browser");
        } else {
            info!(url, "opened in the browser");
        }
    }

    fn copy(url: &str) {
        match arboard::Clipboard::new().and_then(|mut c| c.set_text(url.to_string())) {
            Ok(()) => info!(url, "URL copied"),
            Err(e) => error!(url, err = ?e, "clipboard write failed"),
        }
    }

    pub fn run() {
        let dir =
            std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into()))
                .join("WallBridge")
                .join("logs");
        let _ = std::fs::create_dir_all(&dir);
        let (writer, _guard) =
            tracing_appender::non_blocking(tracing_appender::rolling::daily(&dir, "tray.log"));
        tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(writer)
            .init();
        info!(version = env!("CARGO_PKG_VERSION"), "tray starting");

        // One tray per session.
        let _mutex = unsafe { CreateMutexW(None, true, w!("Local\\WallBridgeTray")) };
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            info!("a tray already runs in this session, exiting");
            return;
        }

        let url = lan_url();
        let status_item = MenuItem::new("wall-bridge: …", false, None);
        let open_item = MenuItem::new("Otvoriť wall-bridge", true, None);
        let copy_item = MenuItem::new(format!("Kopírovať URL ({url})"), true, None);
        let exit_item = MenuItem::new("Koniec (iba ikona)", true, None);
        let menu = Menu::new();
        let built = menu.append_items(&[
            &status_item,
            &PredefinedMenuItem::separator(),
            &open_item,
            &copy_item,
            &PredefinedMenuItem::separator(),
            &exit_item,
        ]);
        if let Err(e) = built {
            error!(err = ?e, "menu");
            return;
        }
        let tray: TrayIcon = loop {
            match TrayIconBuilder::new()
                .with_menu(Box::new(menu.clone()))
                .with_tooltip("wall-bridge")
                .with_icon(icon(Light::Grey))
                .build()
            {
                Ok(t) => break t,
                Err(e) => {
                    warn!(err = ?e, retry_s = ICON_RETRY.as_secs(), "tray icon not created yet, retrying");
                    std::thread::sleep(ICON_RETRY);
                }
            }
        };
        info!("tray icon created");
        let exe = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let mut promoted_at: Option<Instant> = None;

        // Status polling off the UI thread.
        let (tx, rx) = mpsc::channel::<Option<Status>>();
        std::thread::spawn(move || {
            let agent = ureq::AgentBuilder::new().timeout(POLL_TIMEOUT).build();
            loop {
                if tx.send(fetch(&agent)).is_err() {
                    return;
                }
                std::thread::sleep(POLL_EVERY);
            }
        });

        let mut shown: Option<View> = None;
        let mut msg = MSG::default();
        loop {
            // Pump Win32 messages for the tray window and the menu.
            unsafe {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                if ev.id == open_item.id() {
                    open_in_browser(&url);
                } else if ev.id == copy_item.id() {
                    copy(&url);
                } else if ev.id == exit_item.id() {
                    info!("exit from the menu (tray only, the engine keeps running)");
                    return;
                }
            }
            if promoted_at.is_none_or(|t| t.elapsed() >= PROMOTE_EVERY) {
                keep_visible(&exe);
                promoted_at = Some(Instant::now());
            }
            while let Ok(st) = rx.try_recv() {
                let v = view(st.as_ref());
                if shown.as_ref() != Some(&v) {
                    info!(line = %v.line, light = ?v.light, "status changed");
                    let _ = tray.set_icon(Some(icon(v.light)));
                    let _ = tray.set_tooltip(Some(&v.line));
                    status_item.set_text(&v.line);
                    shown = Some(v);
                }
            }
            unsafe {
                // Sleep until a message arrives or 100 ms pass.
                let _ = MsgWaitForMultipleObjects(None, false, 100, QS_ALLINPUT);
            }
        }
    }
}
