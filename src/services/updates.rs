//! Update checking, status snapshots, and batch update orchestration.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use regex::Regex;
use serde_json::{json, Map, Value};

use crate::core::errors::AppError;
use crate::core::hashing;
use crate::core::instances::{instance_dir, instance_loader_mc};
use crate::core::mods::{list_mod_rows, mod_file_path};
use crate::providers::curseforge;
use crate::providers::http::modrinth_get;
use crate::core::state::{APP, now_seconds, read_json};
use crate::services::versions;

pub const UPDATE_TTL_SECONDS: f64 = 6.0 * 3600.0;
pub const LATEST_TTL_SECONDS: f64 = 3600.0;

static SAFE_NAME_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\w.-]").unwrap());

fn root_hash_prefix() -> String {
    let root = APP.root_string().unwrap_or_default();
    let mut hasher = sha1::Sha1::new();
    use sha1::Digest;
    hasher.update(root.as_bytes());
    let hex: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    hex[..8].to_string()
}

/// Persisted update snapshot for one instance.
pub fn cache_path(name: &str) -> PathBuf {
    APP.state_dir.join(format!(
        "updates-{}-{}.json",
        root_hash_prefix(),
        SAFE_NAME_PATTERN.replace_all(name, "_")
    ))
}

fn latest_cache_path() -> PathBuf {
    APP.state_dir.join("latestcache.json")
}

fn load_latest_cache() -> Value {
    let v = read_json(&latest_cache_path());
    if v.is_object() { v } else { json!({}) }
}

fn save_latest_cache(cache: &Value) {
    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(latest_cache_path(), cache.to_string());
}

/// Loaded snapshots for the current root; interrupted "checking" states are ignored.
pub fn load_update_cache() {
    let prefix = format!("updates-{}-", root_hash_prefix());
    let rd = match fs::read_dir(&APP.state_dir) { Ok(r) => r, Err(_) => return };
    let mut updates = APP.updates.lock().unwrap();

    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) && name.ends_with(".json") {
            if let Ok(text) = fs::read_to_string(e.path()) {
                if let Ok(data) = serde_json::from_str::<Value>(&text) {
                    if data.get("status") == Some(&json!("checking")) { continue; }
                    updates.insert(name[prefix.len()..name.len() - 5].to_string(), data);
                }
            }
        }
    }
}

pub fn update_status(name: &str) -> Value {
    APP.updates.lock().unwrap()
        .get(name)
        .cloned()
        .unwrap_or_else(|| json!({ "status": "idle" }))
}

/// Start (or return) an update check. Stale results trigger a fresh scan.
pub fn check_updates(name: &str, force: bool) -> Value {
    {
        let updates = APP.updates.lock().unwrap();
        if let Some(current) = updates.get(name) {
            if current["status"] == json!("checking") { return current.clone(); }
            if !force
                && now_seconds() - current.get("ts").and_then(|v| v.as_f64()).unwrap_or(0.0)
                    < UPDATE_TTL_SECONDS
            {
                let stale = match (
                    current.get("fp").and_then(|v| v.as_str()),
                    mods_fingerprint(name),
                ) {
                    (Some(fp), Some(now)) => fp != now,
                    _ => true,
                };
                if !stale { return current.clone(); }
            }
        }
    }

    let key = name.to_string();
    let snapshot = json!({
        "status": "checking",
        "ts": now_seconds(),
        "phase": "scanning",
        "done": 0,
        "total": 0,
    });
    APP.updates.lock().unwrap().insert(key.clone(), snapshot.clone());
    std::thread::spawn(move || run_update_check(key));
    snapshot
}

fn all_mod_rows(inst: &Path) -> Vec<Value> {
    let mut rows = list_mod_rows(inst, "mods");
    rows.extend(list_mod_rows(inst, "plugins"));
    rows
}

/// Cheap staleness fingerprint: filename|size|mtime for enabled jars.
fn mods_fingerprint(name: &str) -> Option<String> {
    let inst = instance_dir(name).ok()?;
    let mut parts: Vec<String> = all_mod_rows(&inst).iter()
        .filter(|m| m["disabled"] != json!(true))
        .map(|m| format!(
            "{}|{}|{}",
            m["file"].as_str().unwrap_or(""),
            m["size"].as_u64().unwrap_or(0),
            m["mtime"].as_i64().unwrap_or(0)
        ))
        .collect();
    parts.sort();
    Some(parts.join("\n"))
}

fn run_update_check(name: String) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_update_check_inner(&name)));
    let mut updates = APP.updates.lock().unwrap();
    match result {
        Ok(Ok((items, scan_fingerprint))) => {
            let status = json!({
                "status": "done",
                "ts": now_seconds(),
                "items": items,
                "fp": scan_fingerprint,
            });
            updates.insert(name.clone(), status.clone());
            let _ = fs::create_dir_all(&APP.state_dir);
            let _ = fs::write(cache_path(&name), status.to_string());
        }
        Ok(Err(e)) => {
            updates.insert(name.clone(), json!({
                "status": "error",
                "ts": now_seconds(),
                "error": e.to_string(),
            }));
        }
        Err(_) => {
            updates.insert(name.clone(), json!({
                "status": "error",
                "ts": now_seconds(),
                "error": "update check crashed unexpectedly",
            }));
        }
    }
}

fn set_error(name: &str, message: String) {
    let mut updates = APP.updates.lock().unwrap();
    if let Some(status) = updates.get_mut(name) {
        status["error"] = json!(message);
    }
}

fn set_check_phase(name: &str, phase: &str, done: u64, total: u64, cur: Option<String>) {
    let mut updates = APP.updates.lock().unwrap();
    if let Some(entry) = updates.get_mut(name) {
        if entry.get("status") == Some(&json!("checking")) {
            entry["phase"] = json!(phase);
            entry["done"] = json!(done);
            entry["total"] = json!(total);
            entry["cur"] = cur.map(Value::String).unwrap_or(Value::Null);
        }
    }
}

fn run_update_check_inner(name: &str) -> Result<(Map<String, Value>, String), AppError> {
    let inst = instance_dir(name)?;
    let (loader, mcver) = instance_loader_mc(name)?;

    let all = all_mod_rows(&inst);
    let scan_fingerprint = {
        let mut parts: Vec<String> = all.iter()
            .filter(|m| m["disabled"] != json!(true))
            .map(|m| format!(
                "{}|{}|{}",
                m["file"].as_str().unwrap_or(""),
                m["size"].as_u64().unwrap_or(0),
                m["mtime"].as_i64().unwrap_or(0)
            ))
            .collect();
        parts.sort();
        parts.join("\n")
    };

    let mods: Vec<Value> = all.into_iter().filter(|m| m["disabled"] != json!(true)).collect();
    let mut hash_cache = hashing::load_hash_cache();
    let mut hashes: HashMap<String, String> = HashMap::new();
    let mod_count = mods.len() as u64;

    set_check_phase(name, "hashing", 0, mod_count, None);
    for (i, m) in mods.iter().enumerate() {
        let filename = m["file"].as_str().unwrap().to_string();
        set_check_phase(name, "hashing", i as u64, mod_count, Some(filename.clone()));
        let path = match mod_file_path(&inst, &filename) {
            Ok(p) => p,
            Err(e) => {
                set_error(name, format!("hash path for {filename}: {e}"));
                set_check_phase(name, "hashing", (i as u64) + 1, mod_count, Some(filename));
                continue;
            }
        };
        if let Ok(h) = hashing::cached_sha1(&path, &mut hash_cache) {
            hashes.insert(filename.clone(), h);
        }
        set_check_phase(name, "hashing", (i as u64) + 1, mod_count, Some(filename));
    }
    hashing::save_hash_cache(&hash_cache);

    let mut by_hash: Map<String, Value> = Map::new();
    let mut hash_list: Vec<String> = hashes.values().cloned().collect();
    hash_list.sort();
    hash_list.dedup();
    let chunk_count = ((hash_list.len() + 99) / 100) as u64;

    set_check_phase(name, "version_files", 0, chunk_count, None);
    for (i, chunk) in hash_list.chunks(100).enumerate() {
        match crate::providers::http::modrinth_post(
            "/version_files",
            &json!({ "hashes": chunk, "algorithm": "sha1" }),
        ) {
            Ok(Value::Object(o)) => by_hash.extend(o),
            Ok(_) => {}
            Err(e) => set_error(name, format!("version_files: {e}")),
        }
        set_check_phase(name, "version_files", (i as u64) + 1, chunk_count, Some(format!("chunk {}/{}", i + 1, chunk_count)));
    }

    let mut items: Map<String, Value> = Map::new();
    for m in &mods {
        let filename = m["file"].as_str().unwrap();
        let hash = hashes.get(filename);
        let version = hash.and_then(|h| by_hash.get(h));

        let mut item = json!({
            "file": filename,
            "name": m["name"],
            "installed": m["version"],
            "known": version.is_some(),
            "project": null,
            "latest": null,
            "update": null,
            "latest_url": null,
            "latest_filename": null,
            "source": if version.is_some() { Value::String("modrinth".into()) } else { Value::Null },
        });

        if let Some(v) = version {
            item["project"] = v["project_id"].clone();
            let version_number = v.get("version_number").and_then(|x| x.as_str()).unwrap_or("");
            item["installed"] = if !version_number.is_empty() {
                json!(version_number)
            } else {
                m["version"].clone()
            };
            item["installed_name"] = v.get("name").cloned().unwrap_or(Value::Null);
        }
        items.insert(filename.to_string(), item);
    }

    let project_names: HashMap<String, String> = items.values()
        .filter_map(|it| {
            let pid = it.get("project").and_then(|p| p.as_str()).map(|s| s.to_string())?;
            let display = it.get("installed_name")
                .and_then(|n| n.as_str())
                .or_else(|| it.get("name").and_then(|n| n.as_str()))
                .unwrap_or("").to_string();
            Some((pid, display))
        })
        .collect();

    let mut projects: Vec<Value> = items.values()
        .filter_map(|it| it.get("project").cloned())
        .filter(|p| *p != Value::Null)
        .collect();
    projects.sort_by_key(|p| p.to_string());
    projects.dedup();

    let mut latest: HashMap<String, Value> = HashMap::new();
    if !loader.is_empty() {
        let mut latest_cache = load_latest_cache();
        let project_count = projects.len() as u64;
        set_check_phase(name, "latest", 0, project_count, None);

        for (i, project) in projects.iter().enumerate() {
            let pid = project.as_str().unwrap_or("").to_string();
            let cache_key = format!("{pid}|{loader}|{mcver}");
            let mut resolved: Option<Value> = None;

            if let Some(entry) = latest_cache.get(&cache_key) {
                let ts = entry.get("ts").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if now_seconds() - ts < LATEST_TTL_SECONDS {
                    resolved = entry.get("version").cloned();
                }
            }

            if resolved.is_none() {
                match modrinth_get(
                    &format!("/project/{pid}/version"),
                    &{
                        let mut p = Vec::new();
                        if !mcver.is_empty() { p.push(("game_versions", json!([mcver]).to_string())); }
                        if !loader.is_empty() { p.push(("loaders", json!([loader]).to_string())); }
                        p
                    },
                ) {
                    Ok(Value::Array(vs)) => {
                        if let Some(v) = vs.into_iter().next() {
                            latest_cache[cache_key] = json!({ "ts": now_seconds(), "version": v.clone() });
                            resolved = Some(v);
                        }
                    }
                    _ => {}
                }
            }

            if let Some(v) = resolved { latest.insert(pid.clone(), v); }
            let current = project_names.get(&pid).cloned().unwrap_or(pid.clone());
            set_check_phase(name, "latest", (i as u64) + 1, project_count, Some(current));
        }
        save_latest_cache(&latest_cache);
    }

    for item in items.values_mut() {
        let pid = item.get("project").and_then(|p| p.as_str()).unwrap_or("").to_string();
        if let Some(lv) = latest.get(&pid) {
            item["latest"] = lv["version_number"].clone();
            let files = lv.get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            if let Some(primary) = crate::services::dependencies::pick_primary_file(&files) {
                item["latest_url"] = primary["url"].clone();
                item["latest_filename"] = primary["filename"].clone();
            }
            if lv["version_number"] != item["installed"] {
                item["update"] = lv["version_number"].clone();
            }
        }
    }

    if APP.curseforge_key().is_some() {
        set_check_phase(name, "curseforge", 0, 0, None);
        curseforge::match_unknown(&inst, &mods, &mut items, &mut hash_cache, &loader, &mcver, name);
    }

    Ok((items, scan_fingerprint))
}

pub fn update_all_status(name: &str) -> Value {
    APP.allupd.lock().unwrap()
        .get(name)
        .cloned()
        .unwrap_or_else(|| json!({ "status": "idle" }))
}

/// Start updating every mod with an available update.
pub fn start_update_all(name: &str) -> Result<Value, AppError> {
    let status = APP.updates.lock().unwrap().get(name).cloned().unwrap_or(Value::Null);
    if status.get("status").and_then(|v| v.as_str()) != Some("done") {
        return Err(AppError::BadRequest("run update check first".into()));
    }

    let todo: Vec<String> = status.get("items").and_then(|i| i.as_object()).map(|m| {
        m.iter()
            .filter(|(_, it)| {
                it.get("update").and_then(|v| v.as_str()).is_some()
                    && it.get("latest_url").and_then(|v| v.as_str()).is_some()
            })
            .map(|(f, _)| f.clone())
            .collect()
    }).unwrap_or_default();

    start_batch(name, todo, Vec::new())
}

/// Start updating only selected mods.
pub fn start_update_selected(name: &str, picked: &[String]) -> Result<Value, AppError> {
    let status = APP.updates.lock().unwrap().get(name).cloned().unwrap_or(Value::Null);
    if status.get("status").and_then(|v| v.as_str()) != Some("done") {
        return Err(AppError::BadRequest("run update check first".into()));
    }

    let items = status.get("items").and_then(|i| i.as_object());
    let mut todo: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    for f in picked {
        let eligible = items.and_then(|m| m.get(f)).map(|it| {
            it.get("update").and_then(|v| v.as_str()).is_some()
                && it.get("latest_url").and_then(|v| v.as_str()).is_some()
        }).unwrap_or(false);
        if eligible { todo.push(f.clone()); } else { skipped.push(f.clone()); }
    }

    if todo.is_empty() {
        return Err(AppError::BadRequest("tidak ada mod terpilih yang punya update".into()));
    }
    start_batch(name, todo, skipped)
}

fn start_batch(name: &str, todo: Vec<String>, skipped: Vec<String>) -> Result<Value, AppError> {
    {
        let all_updates = APP.allupd.lock().unwrap();
        if let Some(current) = all_updates.get(name) {
            if current["status"] == json!("running") { return Ok(current.clone()); }
        }
    }

    let key = crate::core::progress::start_job(
        "update",
        name,
        &format!("{} mod", todo.len()),
        todo.len() as u64,
        true,
    );
    let status = json!({
        "status": "running",
        "total": todo.len(),
        "done": 0,
        "current": null,
        "results": [],
        "skipped": skipped,
    });
    APP.allupd.lock().unwrap().insert(name.to_string(), status.clone());

    let instance_name = name.to_string();
    std::thread::spawn(move || {
        for f in &todo {
            {
                let mut all_updates = APP.allupd.lock().unwrap();
                all_updates.get_mut(&instance_name).unwrap()["current"] = json!(f);
            }

            let result = match versions::apply_update(&instance_name, f, None, "", "", Some(&key)) {
                Ok(new_file) => json!({ "file": f, "ok": new_file }),
                Err(e) => json!({ "file": f, "error": e.to_string() }),
            };

            let mut all_updates = APP.allupd.lock().unwrap();
            let current = all_updates.get_mut(&instance_name).unwrap();
            current["results"].as_array_mut().unwrap().push(result);
            current["done"] = json!(current["done"].as_u64().unwrap_or(0) + 1);
        }

        let mut all_updates = APP.allupd.lock().unwrap();
        {
            let current = all_updates.get_mut(&instance_name).unwrap();
            current["status"] = json!("done");
            current["current"] = Value::Null;
        }

        crate::core::progress::finish_done(&key);

        let status = APP.updates.lock().unwrap().get(&instance_name).cloned();
        if let Some(status) = status {
            let _ = fs::create_dir_all(&APP.state_dir);
            let _ = fs::write(cache_path(&instance_name), status.to_string());
        }
    });

    Ok(status)
}
