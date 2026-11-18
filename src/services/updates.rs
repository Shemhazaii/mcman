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
use crate::core::mods::{list_mod_rows, mod_file_path, parse_mod_name};
use crate::core::links;
use crate::providers::curseforge;
use crate::providers::http::{modrinth_get, modrinth_post, curseforge_post};
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

fn normalized_version(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = Vec::new();
    for raw in lower.split(|c: char| c == '+' || c == '_' || c == '-' || c == ' ' || c == ',').filter(|t| !t.is_empty()) {
        let t = raw.trim_start_matches("mc");
        if t.is_empty() { continue; }
        if matches!(t, "neoforge" | "forge" | "fabric" | "quilt" | "bukkit" | "paper" | "spigot" | "release") {
            continue;
        }
        if t.starts_with("build.") { continue; }
        if is_mc_version_token(t) { continue; }
        let t = t.trim_start_matches('v');
        if t.is_empty() { continue; }
        out.push(t.to_string());
    }
    out.join(" ")
}

fn is_mc_version_token(t: &str) -> bool {
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() >= 2 && parts[0] == "1" {
        if let Ok(second) = parts[1].parse::<u32>() {
            return second >= 16;
        }
    }
    false
}

fn version_parts(s: &str) -> Vec<u64> {
    let mut out = Vec::new();
    for token in normalized_version(s).split_whitespace() {
        for part in token.split('.') {
            let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() { break; }
            if let Ok(n) = digits.parse::<u64>() { out.push(n); } else { break; }
        }
    }
    out
}

fn version_newer(latest: &str, installed: &str) -> bool {
    let a = normalized_version(latest);
    let b = normalized_version(installed);
    if a.is_empty() || b.is_empty() || a == b { return false; }
    let pa = version_parts(&a);
    let pb = version_parts(&b);
    if !pa.is_empty() && !pb.is_empty() {
        return pa > pb;
    }
    a > b
}

fn bootstrap_index(
    name: &str,
    inst: &Path,
    mods: &[Value],
    links: &mut Value,
    prev_items: Option<&Map<String, Value>>,
) -> Result<(), AppError> {
    let mut missing: Vec<String> = Vec::new();

    for m in mods {
        let file = m["file"].as_str().unwrap().to_string();
        let key = format!("{name}|{file}");
        if let Some(entry) = links.get(&key) {
            let source = entry.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let project = entry.get("project").and_then(|v| v.as_str()).unwrap_or("");
            if source == "unknown" || !project.is_empty() { continue; }
        }

        if let Some(pi) = prev_items.and_then(|m| m.get(&file)) {
            if pi.get("known") == Some(&json!(true)) {
                let project = pi.get("project").and_then(|v| v.as_u64()).map(|v| v.to_string())
                    .or_else(|| pi.get("project").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .unwrap_or_default();
                if !project.is_empty() {
                    let source = if project.chars().all(|c| c.is_ascii_digit()) { "curseforge" } else { "modrinth" };
                    let url = if source == "curseforge" {
                        curseforge::mod_url(&json!(project)).unwrap_or_default()
                    } else {
                        format!("https://modrinth.com/mod/{project}")
                    };
                    links[key] = json!({
                        "url": url,
                        "source": source,
                        "project": project,
                        "name": pi["name"],
                        "version": pi["installed"],
                        "unknown": false,
                    });
                    continue;
                }
            }
        }

        missing.push(file);
    }

    if missing.is_empty() {
        links::save_links(links);
        return Ok(());
    }

    let mut hash_cache = hashing::load_hash_cache();
    let mut hashes: HashMap<String, String> = HashMap::new();
    let total = missing.len() as u64;
    set_check_phase(name, "hashing", 0, total, None);

    for (i, file) in missing.iter().enumerate() {
        set_check_phase(name, "hashing", i as u64, total, Some(file.clone()));
        if let Ok(path) = mod_file_path(inst, file) {
            if let Ok(h) = hashing::cached_sha1(&path, &mut hash_cache) {
                hashes.insert(file.clone(), h);
            }
        }
        set_check_phase(name, "hashing", (i as u64) + 1, total, Some(file.clone()));
    }
    hashing::save_hash_cache(&hash_cache);

    let mut by_hash: Map<String, Value> = Map::new();
    let mut hash_list: Vec<String> = hashes.values().cloned().collect();
    hash_list.sort();
    hash_list.dedup();

    for chunk in hash_list.chunks(100) {
        if let Ok(v) = modrinth_post("/version_files", &json!({ "hashes": chunk, "algorithm": "sha1" })) {
            if let Some(o) = v.as_object() {
                for (k, val) in o { by_hash.insert(k.clone(), val.clone()); }
            }
        }
    }

    for file in &missing {
        let key = format!("{name}|{file}");
        if links.get(&key).is_some() { continue; }
        if let Some(h) = hashes.get(file) {
            if let Some(v) = by_hash.get(h) {
                let pid = v.get("project_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                if !pid.is_empty() {
                    let version = v.get("version_number").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    links[key] = json!({
                        "url": format!("https://modrinth.com/mod/{pid}"),
                        "source": "modrinth",
                        "project": pid,
                        "name": "",
                        "version": version,
                        "unknown": false,
                    });
                }
            }
        }
    }

    let still_missing: Vec<String> = missing.iter()
        .filter(|f| links.get(format!("{name}|{f}")).is_none())
        .cloned()
        .collect();

    if !still_missing.is_empty() && APP.curseforge_key().is_some() {
        let mut fingerprints: HashMap<String, u32> = HashMap::new();
        for file in &still_missing {
            if let Ok(path) = mod_file_path(inst, file) {
                if let Ok(fp) = hashing::cached_fingerprint(&path, &mut hash_cache) {
                    fingerprints.insert(file.clone(), fp);
                }
            }
        }
        hashing::save_hash_cache(&hash_cache);

        let mut values: Vec<u32> = fingerprints.values().cloned().collect();
        values.sort();
        values.dedup();
        let mut by_fp: HashMap<u32, (Value, Option<String>)> = HashMap::new();

        for chunk in values.chunks(100) {
            if let Ok(res) = curseforge_post("/fingerprints", &json!({ "fingerprints": chunk })) {
                let data = res.get("data").cloned().unwrap_or_else(|| json!({}));
                let fp_list = data.get("exactFingerprints").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let matches = data.get("exactMatches").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                for (fp_value, entry) in fp_list.iter().zip(matches.iter()) {
                    let id = entry.get("id").cloned()
                        .or_else(|| entry.get("file").and_then(|f| f.get("modId")).cloned())
                        .unwrap_or(Value::Null);
                    let display = entry.get("file").and_then(|f| f.get("displayName"))
                        .and_then(|v| v.as_str()).map(|s| s.to_string());
                    if let Some(fp) = fp_value.as_u64() { by_fp.insert(fp as u32, (id, display)); }
                }
            }
        }

        for file in &still_missing {
            let key = format!("{name}|{file}");
            if links.get(&key).is_some() { continue; }
            if let Some(fp) = fingerprints.get(file) {
                if let Some((id, display)) = by_fp.get(fp) {
                    let pid = id.as_u64().map(|x| x.to_string()).unwrap_or_default();
                    if let Some(url) = curseforge::mod_url(id) {
                        let (_, ver) = parse_mod_name(file);
                        links[key] = json!({
                            "url": url,
                            "source": "curseforge",
                            "project": pid,
                            "name": display.clone().unwrap_or_default(),
                            "version": ver,
                            "unknown": false,
                        });
                    }
                }
            }
        }
    }

    for file in &missing {
        let key = format!("{name}|{file}");
        if links.get(&key).is_some() { continue; }
        let (n, v) = parse_mod_name(file);
        links[key] = json!({
            "url": "",
            "source": "unknown",
            "project": "",
            "name": n,
            "version": v,
            "unknown": true,
        });
    }

    links::save_links(links);
    Ok(())
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
    let mut links = links::links_cache();
    let prev = update_status(name);
    let prev_items = prev.get("items").and_then(|v| v.as_object());

    bootstrap_index(name, &inst, &mods, &mut links, prev_items)?;

    let mut items: Map<String, Value> = Map::new();
    let mut project_keys: Vec<(String, String)> = Vec::new();
    let mut project_names: HashMap<String, String> = HashMap::new();

    for m in &mods {
        let filename = m["file"].as_str().unwrap().to_string();
        let mut source = String::new();
        let mut project = String::new();
        let mut installed = m["version"].clone();
        let mut display_name = m["name"].clone();

        if let Some(link) = links.get(format!("{name}|{filename}")) {
            source = link.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
            project = link.get("project").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if let Some(v) = link.get("version").and_then(|v| v.as_str()) {
                if !v.is_empty() { installed = json!(v); }
            }
            if let Some(n) = link.get("name").and_then(|v| v.as_str()) {
                if !n.is_empty() { display_name = json!(n); }
            }
        }

        if project.is_empty() {
            if let Some(prev_item) = prev_items.and_then(|m| m.get(&filename)) {
                if prev_item.get("known") == Some(&json!(true)) {
                    source = prev_item.get("source").and_then(|v| v.as_str()).unwrap_or(&source).to_string();
                    project = prev_item.get("project").and_then(|v| v.as_u64()).map(|v| v.to_string())
                        .or_else(|| prev_item.get("project").and_then(|v| v.as_str()).map(|s| s.to_string()))
                        .unwrap_or_default();
                }
            }
        }

        if !project.is_empty() {
            source = if project.chars().all(|c| c.is_ascii_digit()) {
                "curseforge".to_string()
            } else {
                "modrinth".to_string()
            };
        }

        let known = !project.is_empty();

        let mut item = json!({
            "file": filename,
            "name": display_name,
            "installed": installed,
            "known": known,
            "project": if known { json!(project) } else { Value::Null },
            "latest": null,
            "update": null,
            "latest_url": null,
            "latest_filename": null,
            "source": if known { json!(source) } else { Value::Null },
        });

        if known {
            project_keys.push((source.clone(), project.clone()));
            project_names.insert(project.clone(), filename.clone());
        }
        items.insert(filename, item);
    }

    project_keys.sort();
    project_keys.dedup();

    let mut names = links::names_cache();
    let mut modrinth_ids: Vec<String> = project_keys.iter()
        .filter(|(_, pid)| !pid.chars().all(|c| c.is_ascii_digit()))
        .map(|(_, pid)| pid.clone())
        .collect();
    let mut curseforge_ids: Vec<Value> = project_keys.iter()
        .filter(|(_, pid)| pid.chars().all(|c| c.is_ascii_digit()) && !pid.is_empty())
        .filter_map(|(_, pid)| pid.parse::<u64>().ok().map(|v| json!(v)))
        .collect();
    modrinth_ids.sort();
    modrinth_ids.dedup();
    curseforge_ids.sort_by_key(|v| v.as_u64().unwrap_or(0));
    curseforge_ids.dedup();

    for chunk in modrinth_ids.chunks(50) {
        if let Ok(v) = modrinth_get("/projects", &[("ids", json!(chunk).to_string())]) {
            for p in v.as_array().cloned().unwrap_or_default() {
                let id = p.get("id").and_then(|x| x.as_str()).unwrap_or("");
                let title = p.get("title").and_then(|x| x.as_str()).unwrap_or("");
                if !id.is_empty() && !title.is_empty() {
                    names[format!("modrinth:{id}")] = json!(title);
                }
            }
        }
    }

    for chunk in curseforge_ids.chunks(50) {
        if let Ok(res) = curseforge_post("/mods", &json!({ "modIds": chunk })) {
            for m in res.get("data").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                let id = m.get("id").and_then(|x| x.as_u64()).map(|x| x.to_string()).unwrap_or_default();
                let title = m.get("name").and_then(|x| x.as_str()).unwrap_or("");
                if !id.is_empty() && !title.is_empty() {
                    names[format!("curseforge:{id}")] = json!(title);
                }
            }
        }
    }
    links::save_names(&names);

    for item in items.values_mut() {
        let pid = item.get("project").and_then(|p| p.as_str()).unwrap_or("").to_string();
        if !pid.is_empty() {
            let key = links::project_cache_key(&pid);
            if let Some(n) = names.get(&key).and_then(|v| v.as_str()) {
                if !n.is_empty() { item["name"] = json!(n); }
            }
        }
    }

    let mut latest: HashMap<String, Value> = HashMap::new();
    if !loader.is_empty() {
        let mut latest_cache = load_latest_cache();
        let project_count = project_keys.len() as u64;
        set_check_phase(name, "latest", 0, project_count, None);

        for (i, (source, pid)) in project_keys.iter().enumerate() {
            let cache_key = format!("{source}|{pid}|{loader}|{mcver}");
            let mut resolved: Option<Value> = None;

            if let Some(entry) = latest_cache.get(&cache_key) {
                let ts = entry.get("ts").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if now_seconds() - ts < LATEST_TTL_SECONDS {
                    resolved = entry.get("version").cloned();
                }
            }

            if resolved.is_none() {
                if source == "modrinth" {
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
                                resolved = Some(v);
                            }
                        }
                        _ => {}
                    }
                } else if source == "curseforge" && APP.curseforge_key().is_some() {
                    if let Ok(list) = curseforge::files(&json!(pid), &mcver, &loader, 25) {
                        if let Some(f) = list.first() {
                            let version = {
                                let (_, v) = parse_mod_name(f["fileName"].as_str().unwrap_or(""));
                                if !v.is_empty() {
                                    v
                                } else {
                                    f.get("displayName").and_then(|x| x.as_str()).unwrap_or("").to_string()
                                }
                            };
                            resolved = Some(json!({
                                "version_number": version,
                                "files": [f],
                            }));
                        }
                    }
                }
            }

            if let Some(v) = resolved {
                latest_cache[cache_key] = json!({ "ts": now_seconds(), "version": v.clone() });
                latest.insert(format!("{source}|{pid}"), v);
            }
            let current = project_names.get(pid).cloned().unwrap_or(pid.clone());
            set_check_phase(name, "latest", (i as u64) + 1, project_count, Some(current));
        }
        save_latest_cache(&latest_cache);
    }

    for item in items.values_mut() {
        let pid = item.get("project").and_then(|p| p.as_str()).unwrap_or("").to_string();
        let source = item.get("source").and_then(|p| p.as_str()).unwrap_or("").to_string();
        if let Some(lv) = latest.get(&format!("{source}|{pid}")) {
            item["latest"] = lv["version_number"].clone();
            let files = lv.get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            if source == "curseforge" {
                if let Some(primary) = files.first() {
                    item["latest_url"] = json!(curseforge::download_url(primary));
                    item["latest_filename"] = primary["fileName"].clone();
                }
            } else if let Some(primary) = crate::services::dependencies::pick_primary_file(&files) {
                item["latest_url"] = primary["url"].clone();
                item["latest_filename"] = primary["filename"].clone();
            }
            let installed = item["installed"].as_str().unwrap_or("");
            let latest_version = lv["version_number"].as_str().unwrap_or("");
            if version_newer(latest_version, installed) {
                item["update"] = lv["version_number"].clone();
            }
        }
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
