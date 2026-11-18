//! Desktop integration helpers: opening files/browsers, native folder picker,
//! and the Tauri webview window.

use std::process::Command;
use std::path::Path;

use crate::core::errors::AppError;
use crate::core::state::APP;

/// Open a local file or directory in the host desktop.
pub fn open_path(path: &str) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        let result = Command::new("cmd").args(["/C", "start", "", path]).spawn();
        return if result.is_ok() {
            Ok(())
        } else {
            Err(AppError::BadRequest("cannot open path (cmd start failed)".into()))
        };
    }

    #[cfg(all(target_family = "unix", not(target_os = "macos")))]
    {
        let environment = gui_environment();
        let apply_env = |command: &mut Command| {
            if let Some(env) = &environment {
                for (k, v) in env { command.env(k, v); }
            }
        };

        let is_dir = Path::new(path).is_dir();
        let direct_programs = if is_dir {
            ["dolphin", "nautilus", "thunar"]
        } else {
            ["kate", "gedit", "mousepad"]
        };
        for program in direct_programs {
            let mut command = Command::new(program);
            command.arg(path);
            apply_env(&mut command);
            if command.spawn().is_ok() { return Ok(()); }
        }

        let mut gio = Command::new("gio");
        gio.args(["open", path]);
        apply_env(&mut gio);
        if let Ok(output) = gio.output() {
            if output.status.success() { return Ok(()); }
        }

        let mut xdg = Command::new("xdg-open");
        xdg.arg(path);
        apply_env(&mut xdg);
        if xdg.spawn().is_ok() { return Ok(()); }

        return Err(AppError::BadRequest("cannot open path (no desktop app available)".into()));
    }

    #[cfg(target_os = "macos")]
    {
        let result = Command::new("open").arg(path).spawn();
        return if result.is_ok() {
            Ok(())
        } else {
            Err(AppError::BadRequest("cannot open path (open failed)".into()))
        };
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_family = "unix")))]
    {
        let _ = path;
        Err(AppError::BadRequest("unsupported platform".into()))
    }
}

/// Open an external URL in the host browser.
pub fn open_external(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        return match Command::new("cmd").args(["/C", "start", "", url]).spawn() {
            Ok(_) => Ok(()),
            Err(_) => Err("cannot launch browser (cmd start failed)".into()),
        };
    }

    #[cfg(target_os = "macos")]
    {
        return match Command::new("open").arg(url).spawn() {
            Ok(_) => Ok(()),
            Err(_) => Err("cannot launch browser (open failed)".into()),
        };
    }

    #[cfg(all(target_family = "unix", not(target_os = "macos")))]
    {
        if let Ok(browser) = std::env::var("BROWSER") {
            let browser = browser.trim();
            if !browser.is_empty() && Command::new(browser).arg(url).spawn().is_ok() {
                return Ok(());
            }
        }
        for candidate in ["brave", "firefox", "chromium", "google-chrome-stable", "google-chrome"] {
            if Command::new(candidate).arg(url).spawn().is_ok() { return Ok(()); }
        }
        if Command::new("xdg-open").arg(url).spawn().is_ok() { return Ok(()); }
        return Err("no browser found to open the mod page".into());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_family = "unix")))]
    {
        let _ = url;
        Err("unsupported platform".into())
    }
}

#[cfg(not(windows))]
fn x11_alive(display: &str) -> bool {
    let number = display.trim_start_matches(':').split('.').next().unwrap_or("");
    !number.is_empty() && Path::new(&format!("/tmp/.X11-unix/X{number}")).exists()
}

#[cfg(not(windows))]
fn wayland_alive(socket: &str) -> bool {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".into());
    Path::new(&format!("{dir}/{socket}")).exists()
}

/// Resolve a live GUI session, falling back to env vars from same-user desktop
/// processes when the server was started headless.
#[cfg(not(windows))]
fn gui_environment() -> Option<Vec<(String, String)>> {
    use std::os::unix::fs::MetadataExt;

    let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let keys = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "XDG_CURRENT_DESKTOP",
    ];
    let keep = |k: &str, v: &str| -> bool {
        match k {
            "DISPLAY" => x11_alive(v),
            "WAYLAND_DISPLAY" => wayland_alive(v),
            "XAUTHORITY" => Path::new(v).exists(),
            _ => true,
        }
    };

    let mut mine: Vec<(String, String)> = Vec::new();
    for k in keys {
        if let Some(v) = get(k) {
            if keep(k, &v) { mine.push((k.to_string(), v)); }
        }
    }
    if mine.iter().any(|(k, _)| k == "DISPLAY" || k == "WAYLAND_DISPLAY") {
        return Some(mine);
    }

    let my_uid = std::fs::metadata("/proc/self").ok().map(|m| m.uid());
    let rd = std::fs::read_dir("/proc").ok()?;
    let mut candidates: Vec<(String, Option<u32>)> = Vec::new();

    for e in rd.flatten() {
        let pid = e.file_name().to_string_lossy().to_string();
        if pid.is_empty() || !pid.bytes().all(|c| c.is_ascii_digit()) { continue; }
        let uid = std::fs::metadata(format!("/proc/{pid}")).ok().map(|m| m.uid());
        if my_uid.is_some() && uid != my_uid { continue; }
        candidates.push((pid, uid));
    }

    candidates.sort_by_key(|(pid, _)| {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        let comm = comm.trim();
        match comm {
            "plasmashell" | "kwin_wayland" | "kwin_x11" | "Xwayland" => 0,
            _ => 1,
        }
    });

    for (pid, _) in candidates {
        let bytes = match std::fs::read(format!("/proc/{pid}/environ")) { Ok(b) => b, Err(_) => continue };
        let mut found: Vec<(String, String)> = Vec::new();
        for kv in bytes.split(|b| *b == 0) {
            let s = String::from_utf8_lossy(kv);
            if let Some(i) = s.find('=') {
                let (k, v) = (&s[..i], &s[i + 1..]);
                if keys.contains(&k) && !v.is_empty() && keep(k, v) {
                    found.push((k.to_string(), v.to_string()));
                }
            }
        }
        if found.iter().any(|(k, _)| k == "DISPLAY" || k == "WAYLAND_DISPLAY") {
            return Some(found);
        }
    }
    None
}

/// Native OS folder picker: kdialog -> zenity on Linux, PowerShell on Windows.
pub fn native_folder_picker(start: &str) -> Result<String, AppError> {
    let start = if start.is_empty() {
        APP.config().get("root").and_then(|v| v.as_str()).unwrap_or("/").to_string()
    } else {
        start.to_string()
    };

    #[cfg(windows)]
    {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $f=New-Object System.Windows.Forms.FolderBrowserDialog; $f.SelectedPath='{start}'; $f.Description='Pilih folder instance'; if($f.ShowDialog() -eq 'OK'){{ Write-Output $f.SelectedPath }}"
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", &script])
            .output()
            .map_err(|_| AppError::BadRequest("native picker unavailable".into()))?;
        if !output.status.success() { return Err(AppError::BadRequest("dibatalkan".into())); }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() { return Err(AppError::BadRequest("dibatalkan".into())); }
        return Ok(path);
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::process::ExitStatusExt;

        let environment = match gui_environment() {
            Some(v) => v,
            None => return Err(AppError::BadRequest(
                "native picker unavailable (no live display session — use the built-in browser)".into()
            )),
        };

        let mut kdialog = Command::new("kdialog");
        kdialog.args(["--getexistingdirectory", &start]);
        for (k, v) in &environment { kdialog.env(k, v); }
        if !environment.iter().any(|(k, _)| k == "WAYLAND_DISPLAY") {
            kdialog.env("QT_QPA_PLATFORM", "xcb");
            kdialog.env_remove("XDG_SESSION_TYPE");
        }

        let run_zenity = || -> Result<std::process::Output, AppError> {
            let mut zenity = Command::new("zenity");
            zenity.args(["--file-selection", "--directory", &format!("--filename={start}/")]);
            if let Some(env) = gui_environment() {
                zenity.env_clear();
                zenity.env("PATH", std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()));
                for (k, v) in env { zenity.env(k, v); }
            }
            zenity.output().map_err(|_| AppError::BadRequest(
                "native picker unavailable (install kdialog/zenity, or use the built-in browser)".into()
            ))
        };

        let output = match kdialog.output() {
            Ok(o) if o.status.signal().is_none() => o,
            _ => run_zenity()?,
        };

        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if !message.is_empty() {
                return Err(AppError::BadRequest(format!("native picker unavailable: {message}")));
            }
            return Err(AppError::BadRequest("dibatalkan".into()));
        }
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
}

/// Open the server URL in the default browser.
pub fn open_browser(port: u16) {
    let url = format!("http://127.0.0.1:{port}");
    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd").args(["/C", "start", "", &url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(&url).spawn();
    #[cfg(all(target_family = "unix", not(target_os = "macos")))]
    let _ = Command::new("xdg-open").arg(&url).spawn();
}

/// Print and persist a desktop-mode log line.
pub fn desktop_log(message: &str) {
    println!("{message}");
    let path = APP.state_dir.join("desktop.log");
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = writeln!(file, "{} {message}", unix_timestamp_string());
    }
}

fn unix_timestamp_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string()
}

/// Run the Tauri desktop window, with browser fallback and safe-mode retry.
pub fn run_desktop(port: u16) {
    #[cfg(target_family = "unix")]
    if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
        desktop_log("webview unavailable (no display session); opening in default browser");
        open_browser(port);
        return;
    }

    // Bundled AppImage GTK/WebKit can crash on some GPU stacks; disable DMABUF.
    if std::env::var("APPIMAGE").is_ok() || std::env::var("APPDIR").is_ok() {
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    match desktop_tauri(port) {
        Ok(()) => {}
        Err(e) => {
            desktop_log(&format!("webview attempt failed: {e} — retrying in safe mode (software GL)"));
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
            std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
            match desktop_tauri(port) {
                Ok(()) => {}
                Err(e2) => {
                    desktop_log(&format!("webview safe-mode also failed: {e2}; opening in default browser"));
                    open_browser(port);
                }
            }
        }
    }
}

fn desktop_tauri(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let url: url::Url = format!("http://127.0.0.1:{port}").parse()?;
    let app = tauri::Builder::default().build(tauri::generate_context!())?;
    let _window = tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::External(url))
        .title("mcman — Minecraft mod manager")
        .inner_size(1320.0, 860.0)
        .min_inner_size(1100.0, 700.0)
        .icon(tauri::include_image!("icons/128x128.png"))?
        .build()?;
    app.run(|_app, _event| {});
    Ok(())
}
