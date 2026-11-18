//! mcman — Minecraft server mod manager.
//!
//! Starts the HTTP API, optional Tauri desktop window, and attaches to an
//! already-running instance when the port is occupied.

mod core;
mod providers;
mod services;
mod ui;
mod web;

use std::net::TcpListener;

use crate::ui::desktop::{open_browser, run_desktop};
use crate::core::state::APP;

/// Read the value following `flag` in argv.
fn arg_value(args: &[String], flag: &str) -> Option<String> {
    let index = args.iter().position(|a| a == flag)?;
    args.get(index + 1).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut port: u16 = arg_value(&args, "--port")
        .or_else(|| std::env::var("MCMAN_PORT").ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(8765);

    if let Some(root) = arg_value(&args, "--root") {
        if let Err(e) = crate::core::instances::set_root(&root) {
            eprintln!("bad --root {root}: {e:?}");
        }
    }

    crate::services::updates::load_update_cache();

    // Launched from an AppImage / desktop entry without a mode flag: open the
    // native window rather than a bare headless server.
    let no_mode_flag = !args.iter().any(|a| {
        matches!(a.as_str(), "--desktop" | "-d" | "--server" | "-s" | "--port")
    });
    let desktop_mode = args.iter().any(|a| a == "--desktop" || a == "-d")
        || (no_mode_flag && std::env::var("APPIMAGE").is_ok());

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(_) => {
            if probe_mcman(port) {
                println!("mcman already running on http://127.0.0.1:{port} — attaching");
                if desktop_mode {
                    run_desktop(port);
                    std::process::exit(0);
                } else {
                    open_browser(port);
                    std::process::exit(0);
                }
            }

            let mut bound = None;
            for candidate in port + 1..port + 20 {
                if let Ok(l) = TcpListener::bind(("127.0.0.1", candidate)) {
                    println!("port {port} busy, using {candidate} instead");
                    port = candidate;
                    bound = Some(l);
                    break;
                }
            }

            match bound {
                Some(l) => l,
                None => {
                    eprintln!("cannot bind 127.0.0.1:{port} (and next 19 ports): Address already in use");
                    std::process::exit(1);
                }
            }
        }
    };

    println!(
        "mcman on http://127.0.0.1:{port}  root={}",
        APP.root_string().unwrap_or_else(|| "(none)".into())
    );

    std::thread::spawn(move || {
        let _ = listener.set_nonblocking(true);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            if let Err(e) = axum::serve(listener, web::api::router()).await {
                eprintln!("server error: {e}");
            }
        });
    });

    if desktop_mode {
        std::thread::sleep(std::time::Duration::from_millis(500));
        run_desktop(port);
        std::process::exit(0);
    } else {
        loop { std::thread::park(); }
    }
}

/// Does this port answer like an mcman server?
fn probe_mcman(port: u16) -> bool {
    use std::io::{Read, Write};
    if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(1500)));
        let _ = stream.write_all(b"GET /api/instances HTTP/1.0\r\n\r\n");
        let mut response = String::new();
        let _ = stream.read_to_string(&mut response);
        response.contains("\"has_mods\"")
    } else {
        false
    }
}
