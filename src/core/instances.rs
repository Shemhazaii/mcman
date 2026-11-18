//! Instance discovery and root selection.
//!
//! An "instance" is a directory containing `mods/` or `plugins/`. Loader and
//! Minecraft version are detected from the server jar name, with optional
//! per-instance overrides from `overrides.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use regex::Regex;
use serde_json::{json, Value};

use crate::core::errors::AppError;
use crate::core::state::{APP, read_json};
use crate::services::updates;

pub const SKIP_DIRS: &[&str] = &[".deleted", ".Trash-1000"];

static FABRIC_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"fabric-server-mc\.(\d+\.\d+(?:\.\d+)?)-loader").unwrap());
static YOUER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"youer-(\d+\.\d+(?:\.\d+)?)-").unwrap());
static PAPER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(paper|purpur|pufferfish)-(\d+\.\d+(?:\.\d+)?)-").unwrap());

/// Directory that holds this instance's jars: `mods/` when it exists, else
/// `plugins/` (Velocity proxies have no `mods/`). `None` when neither exists.
pub fn mod_dir(instance: &Path) -> Option<PathBuf> {
    let m = instance.join("mods");
    if m.is_dir() { return Some(m); }
    let p = instance.join("plugins");
    if p.is_dir() { return Some(p); }
    None
}

/// Reject names or filenames that could escape the instance directory.
pub fn validate_name(s: &str) -> Result<(), AppError> {
    if s.is_empty() || s.contains('/') || s.contains('\\') || s.starts_with('.') || s.contains("..") {
        return Err(AppError::BadRequest(format!("bad name: {s:?}")));
    }
    Ok(())
}

/// Detect server loader, Minecraft version, and the matching server jar.
pub fn detect_loader(instance: &Path) -> (String, String, String) {
    let rd = match fs::read_dir(instance) { Ok(r) => r, Err(_) => return ("unknown".into(), "".into(), "".into()) };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let n = e.file_name().to_string_lossy().to_string();
        if !n.ends_with(".jar") { continue; }
        if let Some(c) = FABRIC_PATTERN.captures(&n) {
            return ("fabric".into(), c[1].to_string(), n);
        }
        if let Some(c) = YOUER_PATTERN.captures(&n) {
            return ("neoforge".into(), c[1].to_string(), n);
        }
        if let Some(c) = PAPER_PATTERN.captures(&n) {
            return (c[1].to_string(), c[2].to_string(), n);
        }
        if n.starts_with("velocity-") {
            return ("velocity".into(), String::new(), n);
        }
    }
    ("unknown".into(), String::new(), String::new())
}

/// Load per-instance loader/Minecraft-version overrides.
pub fn load_overrides() -> Value {
    let v = read_json(&APP.state_dir.join("overrides.json"));
    if v.is_object() { v } else { json!({}) }
}

/// Save a per-instance loader/Minecraft-version override.
pub fn save_override(name: &str, loader: &Value, mcver: &Value) {
    let p = APP.state_dir.join("overrides.json");
    let mut ov = read_json(&p);
    if !ov.is_object() { ov = json!({}); }
    ov[name] = json!({ "loader": loader, "mcver": mcver });
    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(p, ov.to_string());
}

/// Resolve an instance name to its directory. A root whose own basename is the
/// requested name is accepted when that directory contains `mods/` or `plugins/`.
pub fn instance_dir(name: &str) -> Result<PathBuf, AppError> {
    validate_name(name)?;
    let root = APP.root_path().ok_or_else(|| AppError::NotFound("no root set".into()))?;
    let root_name = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    if name == root_name
        && !root.join(&root_name).is_dir()
        && (root.join("mods").is_dir() || root.join("plugins").is_dir())
    {
        return Ok(root);
    }
    let d = root.join(name);
    if !d.is_dir() {
        return Err(AppError::NotFound(name.to_string()));
    }
    Ok(d)
}

/// Loader and Minecraft version for one instance, with overrides applied.
pub fn instance_loader_mc(name: &str) -> Result<(String, String), AppError> {
    let inst = instance_dir(name)?;
    let (mut loader, mut mcver, _) = detect_loader(&inst);
    let ov = load_overrides();
    if let Some(o) = ov.get(name) {
        if let Some(l) = o.get("loader").and_then(|v| v.as_str()) { if !l.is_empty() { loader = l.into(); } }
        if let Some(v) = o.get("mcver").and_then(|v| v.as_str()) { if !v.is_empty() { mcver = v.into(); } }
    }
    Ok((loader, mcver))
}

/// List every instance under the configured root.
pub fn list_instances() -> Vec<Value> {
    let mut out = Vec::new();
    let root = match APP.root_path() { Some(r) => r, None => return out };
    let rd = match fs::read_dir(&root) { Ok(r) => r, Err(_) => return out };
    let mut candidates: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    candidates.sort_by_key(|p| p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());

    let root_name = root.file_name().unwrap_or_default();
    if !root.join(root_name).is_dir()
        && (root.join("mods").is_dir() || root.join("plugins").is_dir())
    {
        candidates.insert(0, root.clone());
    }

    let overrides = load_overrides();
    for d in candidates {
        let name = d.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if SKIP_DIRS.contains(&name.as_str()) { continue; }
        let has_mods = d.join("mods").is_dir();
        let has_plugins = d.join("plugins").is_dir();
        if !has_mods && !has_plugins { continue; }

        let (mut loader, mut mcver, jar) = detect_loader(&d);
        let mut port = String::new();
        let mut motd = String::new();
        if let Ok(raw) = fs::read(d.join("server.properties")) {
            for line in String::from_utf8_lossy(&raw).lines() {
                if let Some(v) = line.strip_prefix("server-port=") { port = v.trim().to_string(); }
                else if let Some(v) = line.strip_prefix("motd=") { motd = v.trim().to_string(); }
            }
        }
        if let Some(o) = overrides.get(&name) {
            if let Some(l) = o.get("loader").and_then(|v| v.as_str()) { if !l.is_empty() { loader = l.into(); } }
            if let Some(v) = o.get("mcver").and_then(|v| v.as_str()) { if !v.is_empty() { mcver = v.into(); } }
        }

        out.push(json!({
            "name": name, "loader": loader, "mcver": mcver, "jar": jar,
            "port": port, "motd": motd, "has_mods": has_mods, "has_plugins": has_plugins
        }));
    }
    out
}

/// Set the data root, persist it, and reset the update cache.
pub fn set_root(path: &str) -> Result<String, AppError> {
    let p = PathBuf::from(crate::core::state::normalize_path(path));
    if !p.is_dir() { return Err(AppError::NotFound(p.display().to_string())); }
    *APP.root.write().unwrap() = p.clone();
    let mut cfg = APP.config();
    cfg["root"] = json!(p.display().to_string());
    APP.save_config(&cfg);
    APP.updates.lock().unwrap().clear();
    updates::load_update_cache();
    Ok(p.display().to_string())
}

/// Browser-style directory listing used by the root picker UI.
pub fn browse(path: &str) -> Result<Value, AppError> {
    if cfg!(windows) && path.is_empty() {
        let mut dirs = Vec::new();
        for c in b'A'..=b'Z' {
            let d = format!("{}:\\", c as char);
            if Path::new(&d).exists() { dirs.push(json!({ "name": d.clone(), "path": d })); }
        }
        return Ok(json!({ "path": "", "parent": null, "drives": true, "dirs": dirs }));
    }

    let p = if path.is_empty() { PathBuf::from("/") } else { PathBuf::from(path) };
    if !p.is_dir() { return Err(AppError::NotFound(path.to_string())); }

    let parent = if p.parent().is_none() || p.parent() == Some(&p) {
        if cfg!(windows) { json!("") } else { Value::Null }
    } else {
        json!(p.parent().unwrap().display().to_string())
    };

    let rd = fs::read_dir(&p).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            AppError::BadRequest(format!("permission denied: {path}"))
        } else {
            AppError::from(e)
        }
    })?;

    let mut dirs: Vec<(String, Value)> = Vec::new();
    for e in rd.flatten() {
        if !e.path().is_dir() { continue; }
        let name = e.file_name().to_string_lossy().to_string();
        dirs.push((name.to_lowercase(), json!({
            "name": name,
            "path": e.path().display().to_string(),
            "hidden": name.starts_with('.')
        })));
    }
    dirs.sort_by_key(|(k, _)| k.clone());
    Ok(json!({
        "path": p.display().to_string(),
        "parent": parent,
        "dirs": dirs.into_iter().map(|(_, v)| v).collect::<Vec<Value>>()
    }))
}
