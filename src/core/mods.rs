//! Mod file parsing, listing, toggling, and instance comparison.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use regex::Regex;
use serde_json::{json, Value};

use crate::core::errors::AppError;
use crate::core::instances::instance_dir;
use crate::core::state::{APP, read_json};

static JAR_FILE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\.jar(\.disabled)?$").unwrap());
static DUPLICATE_COUNTER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*\(\d+\)$").unwrap());
static VERSION_SUFFIX_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+?)[-_ ]v?(\d+(?:\.\d+)+(?:[A-Za-z0-9.+\-_]*)?)$").unwrap());
static TIMESTAMP_SUFFIX_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.\d{10}$").unwrap());

/// Split a jar filename into a display name and version.
pub fn parse_mod_name(filename: &str) -> (String, String) {
    let base = JAR_FILE_PATTERN.replace(filename, "");
    let base = DUPLICATE_COUNTER_PATTERN.replace(&base, "");
    if let Some(c) = VERSION_SUFFIX_PATTERN.captures(&base) {
        let name = c[1].replace('_', " ").trim().to_string();
        let version = c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
        return (name, version);
    }
    (base.replace('_', " ").trim().to_string(), String::new())
}

/// List jar/plugin files in one subdirectory.
pub fn list_mod_rows(instance: &Path, subdir: &str) -> Vec<Value> {
    let d = instance.join(subdir);
    let mut out = Vec::new();
    let rd = match fs::read_dir(&d) { Ok(r) => r, Err(_) => return out };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let fname = e.file_name().to_string_lossy().to_string();
        if !JAR_FILE_PATTERN.is_match(&fname) { continue; }
        let md = match e.metadata() { Ok(m) => m, Err(_) => continue };
        if !md.is_file() { continue; }
        let (name, version) = parse_mod_name(&fname);
        let mtime = md.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        out.push(json!({
            "file": fname, "name": name, "version": version,
            "disabled": fname.ends_with(".disabled"),
            "size": md.len(), "mtime": mtime,
            "kind": if subdir == "mods" { "mod" } else { "plugin" }
        }));
    }
    out
}

/// Mod rows for an instance, enriched with dependency-parent metadata.
pub fn get_mod_rows(name: &str) -> Result<Vec<Value>, AppError> {
    let inst = instance_dir(name)?;
    let mut rows = list_mod_rows(&inst, "mods");
    rows.extend(list_mod_rows(&inst, "plugins"));

    let dep_graph = read_json(&APP.state_dir.join("depgraph.json"));
    if let Some(d) = dep_graph.get(name) {
        for r in rows.iter_mut() {
            let file = r["file"].as_str().unwrap_or_default();
            if let Some(e) = d.get(file) {
                r["dep_for"] = e.get("for").cloned().unwrap_or(Value::Null);
            }
        }
    }
    Ok(rows)
}

/// Locate a mod file in `mods/` or `plugins/`.
pub fn mod_file_path(instance: &Path, filename: &str) -> Result<PathBuf, AppError> {
    crate::core::instances::validate_name(filename)?;
    for sub in ["mods", "plugins"] {
        let p = instance.join(sub).join(filename);
        if p.exists() { return Ok(p); }
    }
    Err(AppError::NotFound(filename.to_string()))
}

/// Enable or disable one mod by renaming it with/without `.disabled`.
pub fn toggle_mod(name: &str, filename: &str) -> Result<String, AppError> {
    let inst = instance_dir(name)?;
    let p = mod_file_path(&inst, filename)?;
    let current = p.file_name().unwrap().to_string_lossy().to_string();
    let new_name = if current.to_lowercase().ends_with(".disabled") {
        current[..current.len() - ".disabled".len()].to_string()
    } else {
        format!("{current}.disabled")
    };
    let new_path = p.with_file_name(&new_name);
    fs::rename(&p, &new_path).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(new_name)
}

/// Compare the mod sets of two instances by display name.
pub fn compare_instances(a: &str, b: &str) -> Result<Value, AppError> {
    let mods_a = get_mod_rows(a)?;
    let mods_b = get_mod_rows(b)?;

    let mut by_a: HashMap<String, &Value> = HashMap::new();
    let mut by_b: HashMap<String, &Value> = HashMap::new();
    for m in &mods_a {
        by_a.insert(parse_mod_name(m["file"].as_str().unwrap_or("")).0.to_lowercase(), m);
    }
    for m in &mods_b {
        by_b.insert(parse_mod_name(m["file"].as_str().unwrap_or("")).0.to_lowercase(), m);
    }

    let mut keys: Vec<String> = by_a.keys().chain(by_b.keys()).cloned().collect();
    keys.sort();
    keys.dedup();

    let mut rows = Vec::new();
    for k in keys {
        let x = by_a.get(&k);
        let y = by_b.get(&k);
        match (x, y) {
            (Some(x), Some(y)) if x["version"] != y["version"] => rows.push(json!({
                "type": "version", "name": x["name"], "a": x["version"], "b": y["version"],
                "ad": x["disabled"], "bd": y["disabled"]
            })),
            (Some(x), None) => rows.push(json!({
                "type": "only_a", "name": x["name"], "a": x["version"], "b": null,
                "ad": x["disabled"], "bd": null
            })),
            (None, Some(y)) => rows.push(json!({
                "type": "only_b", "name": y["name"], "a": null, "b": y["version"],
                "ad": null, "bd": y["disabled"]
            })),
            _ => {}
        }
    }
    Ok(json!({ "a": a, "b": b, "rows": rows }))
}

/// Remove the numeric timestamp suffix used when restoring trashed files.
pub fn strip_trash_timestamp(filename: &str) -> String {
    TIMESTAMP_SUFFIX_PATTERN.replace(filename, "").to_string()
}
