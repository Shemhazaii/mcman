//! Process-wide state for mcman.
//!
//! Holds the data root, the persisted configuration file, and the in-memory
//! caches used by the HTTP layer (update results, batch-update status, and live
//! download progress). All shared maps are guarded by mutexes so handlers can
//! run concurrently.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use serde_json::{json, Value};

/// Application-wide state shared by every request handler.
pub struct App {
    /// Directory holding persistent JSON caches and the user configuration.
    pub state_dir: PathBuf,
    /// Minecraft server root; an empty path means "not configured".
    pub root: RwLock<PathBuf>,
    /// Last completed update check per instance name.
    pub updates: Mutex<HashMap<String, Value>>,
    /// Progress of a batch "update all" run per instance name.
    pub allupd: Mutex<HashMap<String, Value>>,
    /// Live download progress keyed by "<kind>:<instance>" job id.
    pub progress: Mutex<HashMap<String, Value>>,
}

/// Lazily initialised singleton. The root is resolved from `MCMAN_ROOT`, then
/// the saved config, then platform defaults.
pub static APP: LazyLock<App> = LazyLock::new(|| {
    let state_dir = std::env::var("MCMAN_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = if cfg!(windows) {
                std::env::var("USERPROFILE")
            } else {
                std::env::var("HOME")
            };
            home.map(PathBuf::from).unwrap_or_default().join(".local/share/mcman")
        });
    let _ = fs::create_dir_all(&state_dir);

    let mut root = PathBuf::new();
    if let Ok(r) = std::env::var("MCMAN_ROOT") {
        if Path::new(&r).is_dir() {
            root = PathBuf::from(r);
        }
    }
    if root.as_os_str().is_empty() {
        if let Some(r) = read_json(&state_dir.join("config.json"))
            .get("root")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            if Path::new(&r).is_dir() {
                root = PathBuf::from(r);
            }
        }
    }
    if root.as_os_str().is_empty() {
        if cfg!(windows) {
            let home = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_default();
            for cand in [home.join("minecraft"), home.join("Documents").join("minecraft")] {
                if cand.is_dir() {
                    root = cand;
                    break;
                }
            }
        } else {
            let p = PathBuf::from("/mnt/minecraft");
            if p.is_dir() {
                root = p;
            }
        }
    }

    App {
        state_dir,
        root: RwLock::new(root),
        updates: Mutex::new(HashMap::new()),
        allupd: Mutex::new(HashMap::new()),
        progress: Mutex::new(HashMap::new()),
    }
});

/// Read a JSON object from disk; missing or malformed files become `{}`.
pub fn read_json(path: &Path) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}))
}

/// Trim trailing separators, preserving "/" for an empty path.
pub fn normalize_path(p: &str) -> String {
    let mut s = p.trim_end_matches('/').trim_end_matches('\\').to_string();
    if s.is_empty() {
        s = "/".to_string();
    }
    s
}

/// Current wall-clock time as fractional Unix seconds (used in cache stamps).
pub fn now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

impl App {
    /// Path of the saved configuration file.
    pub fn config_path(&self) -> PathBuf {
        self.state_dir.join("config.json")
    }

    /// Load the configuration, always returning a JSON object.
    pub fn config(&self) -> Value {
        let v = read_json(&self.config_path());
        if v.is_object() { v } else { json!({}) }
    }

    /// Persist the configuration.
    pub fn save_config(&self, v: &Value) {
        let _ = fs::create_dir_all(&self.state_dir);
        let _ = fs::write(self.config_path(), v.to_string());
    }

    /// Configured root as a display string, or `None` when unset.
    pub fn root_string(&self) -> Option<String> {
        let r = self.root.read().unwrap();
        if r.as_os_str().is_empty() { None } else { Some(r.display().to_string()) }
    }

    /// Configured root as a path, or `None` when unset.
    pub fn root_path(&self) -> Option<PathBuf> {
        let r = self.root.read().unwrap();
        if r.as_os_str().is_empty() { None } else { Some(r.clone()) }
    }

    /// CurseForge API key from config, falling back to `CF_API_KEY`.
    pub fn curseforge_key(&self) -> Option<String> {
        self.config()
            .get("curseforge_api_key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("CF_API_KEY").ok().filter(|s| !s.is_empty()))
    }
}
