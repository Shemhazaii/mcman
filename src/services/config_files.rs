//! Config-file indexing for mods.
//!
//! Each jar's own metadata (`fabric.mod.json` or `mods.toml`) yields a mod id;
//! that id is matched against config files under `<instance>/config`. The index
//! is persisted in `configmap.json`, keyed by a fingerprint of the jar/config
//! set so bulk operations do not rescan slow mounts per mod.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use regex::Regex;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};

use crate::ui::desktop::open_path;
use crate::core::errors::AppError;
use crate::core::instances::instance_dir;
use crate::core::state::{APP, read_json};

static BUILD_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

static CONFIG_EXTENSIONS: &[&str] =
    &["toml", "json", "json5", "yaml", "yml", "properties", "cfg", "conf", "ini", "hjson"];

static MOD_ID_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^\s*modId\s*=\s*"([^"]+)""#).unwrap());
static FIRST_MOD_ID_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)\[\[mods\]\].*?modId\s*=\s*"([^"]+)""#).unwrap());

fn normalize_id(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// Derive a fallback id from the jar filename by stripping loader/version tokens.
fn fallback_id(filename: &str) -> String {
    let mut base = filename.to_lowercase();
    base = base.replace(".jar.disabled", "").replace(".jar", "");
    let tokens: Vec<String> = base
        .split(|c| c == '-' || c == '_' || c == '+' || c == ' ')
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect();
    let all = tokens.clone();
    let mut keep: Vec<String> = Vec::new();
    for t in &tokens {
        let lower = t.to_lowercase();
        let looks_like_version = Regex::new(r"^v?\d+(\.\d+)*([a-z0-9.]*)$").unwrap().is_match(&lower);
        let is_loader = ["neoforge", "forge", "fabric", "quilt", "mc", "release", "youer", "all"]
            .contains(&lower.as_str());
        if looks_like_version || is_loader { break; }
        keep.push(t.to_string());
    }
    let chosen = if keep.is_empty() { all } else { keep };
    chosen.join("").to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn read_at(file: &mut fs::File, offset: u64, len: usize) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    if len == 0 { return Some(Vec::new()); }
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; len];
    let mut read = 0usize;
    while read < len {
        let n = file.read(&mut buf[read..]).ok()?;
        if n == 0 { return None; }
        read += n;
    }
    Some(buf)
}

/// Minimal ZIP reader that reads only the central directory and one member.
fn zip_entry(path: &Path, wanted_name: &str) -> Option<Vec<u8>> {
    let mut file = fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    if size < 22 { return None; }

    let tail_len = std::cmp::min(65557usize, size as usize);
    let tail_offset = size - tail_len as u64;
    let tail = read_at(&mut file, tail_offset, tail_len)?;

    let eocd_signature = [0x50u8, 0x4b, 0x05, 0x06];
    let mut eocd = None;
    for i in (0..tail.len().saturating_sub(3)).rev() {
        if tail[i..i + 4] == eocd_signature { eocd = Some(i); break; }
    }
    let e = eocd?;
    if e + 22 > tail.len() { return None; }

    let entry_count = u16::from_le_bytes(tail[e + 10..e + 12].try_into().ok()?);
    let central_size = u32::from_le_bytes(tail[e + 12..e + 16].try_into().ok()?);
    let central_offset = u32::from_le_bytes(tail[e + 16..e + 20].try_into().ok()?);
    let central = read_at(&mut file, central_offset as u64, central_size as usize)?;

    let mut p = 0usize;
    for _ in 0..entry_count {
        if p + 46 > central.len() || central[p..p + 4] != [0x50, 0x4b, 0x01, 0x02] { return None; }
        let method = u16::from_le_bytes(central[p + 10..p + 12].try_into().ok()?);
        let compressed_size = u32::from_le_bytes(central[p + 20..p + 24].try_into().ok()?);
        let name_len = u16::from_le_bytes(central[p + 28..p + 30].try_into().ok()?) as usize;
        let extra_len = u16::from_le_bytes(central[p + 30..p + 32].try_into().ok()?) as usize;
        let comment_len = u16::from_le_bytes(central[p + 32..p + 34].try_into().ok()?) as usize;
        let local_offset = u32::from_le_bytes(central[p + 42..p + 46].try_into().ok()?) as u64;
        if p + 46 + name_len > central.len() { return None; }

        let name = std::str::from_utf8(&central[p + 46..p + 46 + name_len]).ok()?;
        if name == wanted_name {
            let local_header = read_at(&mut file, local_offset, 30)?;
            let local_name_len = u16::from_le_bytes(local_header[26..28].try_into().ok()?) as usize;
            let local_extra_len = u16::from_le_bytes(local_header[28..30].try_into().ok()?) as usize;
            let start = local_offset + 30 + local_name_len as u64 + local_extra_len as u64;
            let raw = read_at(&mut file, start, compressed_size as usize)?;
            if method == 0 { return Some(raw); }
            if method == 8 {
                let mut out = Vec::new();
                let mut decoder = flate2::read::DeflateDecoder::new(raw.as_slice());
                if decoder.read_to_end(&mut out).is_err() { return None; }
                return Some(out);
            }
            return None;
        }
        p += 46 + name_len + extra_len + comment_len;
        if p >= central.len() { return None; }
    }
    None
}

fn jar_mod_id(path: &Path) -> Option<String> {
    if let Some(bytes) = zip_entry(path, "fabric.mod.json") {
        let text = String::from_utf8_lossy(&bytes);
        let text = text.trim_start_matches('\u{feff}').trim_start();
        if let Ok(v) = serde_json::from_str::<Value>(text) {
            if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                return Some(id.to_string());
            }
        }
    }
    for name in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Some(bytes) = zip_entry(path, name) {
            let text = String::from_utf8_lossy(&bytes);
            if let Some(c) = FIRST_MOD_ID_PATTERN.captures(&text) {
                return Some(c[1].to_string());
            }
            if let Some(c) = MOD_ID_PATTERN.captures(&text) {
                return Some(c[1].to_string());
            }
        }
    }
    None
}

/// Config files relative to `<instance>/config`, with a normalized key and kind.
/// Kind is `"root"` for files directly in config/ and `"dir"` for nested ones.
fn list_config_files(instance: &Path) -> Vec<(String, String, String)> {
    let base = instance.join("config");
    let mut out = Vec::new();
    if !base.is_dir() { return out; }

    let mut stack = vec![(base.clone(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let rd = match fs::read_dir(&dir) { Ok(r) => r, Err(_) => continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let path = e.path();
            if path.is_dir() {
                if depth < 3 { stack.push((path.clone(), depth + 1)); }
                continue;
            }
            let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
            if !CONFIG_EXTENSIONS.contains(&ext.as_str()) { continue; }

            let rel = path.strip_prefix(&base).unwrap_or(&path).display().to_string();
            let lower = rel.to_lowercase();
            if lower.ends_with(".pem") || lower.ends_with(".key") || lower.contains("translations") {
                continue;
            }

            let base_name = name.rsplitn(2, '.').last().unwrap_or(&name);
            let key = if depth == 0 {
                normalize_id(base_name)
            } else {
                normalize_id(rel.split('/').next().unwrap_or(""))
            };
            let kind = if depth == 0 { "root".to_string() } else { "dir".to_string() };
            out.push((rel, key, kind));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn candidate_configs(id: &str, files: &[(String, String, String)]) -> Vec<String> {
    let normalized = normalize_id(id);
    if normalized.is_empty() { return Vec::new(); }

    let suffixes = ["common", "server", "client", "general", "startup", "pre", "post", "config", "defaults"];
    let mut root = Vec::new();
    let mut nested = Vec::new();

    for (rel, key, kind) in files {
        if kind == "root" {
            if *key == normalized {
                root.push(rel.clone());
            } else if key.starts_with(&normalized) {
                let rest = &key[normalized.len()..];
                if suffixes.iter().any(|s| rest == *s) { root.push(rel.clone()); }
            }
        } else if *key == normalized {
            nested.push(rel.clone());
        }
    }

    let priority = |p: &String| {
        let f = p.rsplit('/').next().unwrap_or("").to_lowercase();
        if f.contains("server") { 0 }
        else if f.contains("common") { 1 }
        else if f.contains("client") { 2 }
        else if f.contains("startup") { 3 }
        else { 4 }
    };
    root.sort_by_key(priority);
    nested.sort();

    let mut all = root;
    all.extend(nested);
    all.truncate(50);
    all
}

fn entry_config_map(entry: &Value) -> HashMap<String, Vec<String>> {
    let map = entry.get("map").cloned().unwrap_or(json!({}));
    let mut out = HashMap::new();
    if let Some(o) = map.as_object() {
        for (k, v) in o {
            let list: Vec<String> = v.as_array().unwrap_or(&vec![]).iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect();
            out.insert(k.clone(), list);
        }
    }
    out
}

/// Build (or reuse) the config index for one instance.
pub fn ensure_index(name: &str) -> HashMap<String, Vec<String>> {
    let instance = match instance_dir(name) { Ok(p) => p, Err(_) => return HashMap::new() };
    let cache_path = APP.state_dir.join("configmap.json");
    let store = read_json(&cache_path);
    let entry = store.get(name).cloned().unwrap_or(Value::Null);

    let jars: Vec<PathBuf> = {
        let mut v: Vec<PathBuf> = Vec::new();
        for sub in ["mods", "plugins"] {
            let dir = instance.join(sub);
            if let Ok(rd) = fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    let low = n.to_lowercase();
                    if low.ends_with(".jar") || low.ends_with(".jar.disabled") {
                        v.push(e.path());
                    }
                }
            }
        }
        v.sort();
        v
    };
    let files = list_config_files(&instance);

    let mut fingerprint_parts: Vec<String> = Vec::new();
    for p in &jars {
        fingerprint_parts.push(format!(
            "{}:{}",
            p.file_name().unwrap().to_string_lossy(),
            fs::metadata(p).map(|m| m.len()).unwrap_or(0)
        ));
    }
    for (rel, _, _) in &files { fingerprint_parts.push(rel.clone()); }
    let mut hasher = Sha1::new();
    hasher.update(fingerprint_parts.join("\n").as_bytes());
    let fingerprint = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();

    if entry.get("fp").and_then(|v| v.as_str()) == Some(fingerprint.as_str())
        && entry.get("ids").is_some()
    {
        return entry_config_map(&entry);
    }

    let _guard = BUILD_LOCK.lock().unwrap();
    // Re-check after acquiring the lock.
    let mut store2 = read_json(&cache_path);
    let entry2 = store2.get(name).cloned().unwrap_or(Value::Null);
    if entry2.get("fp").and_then(|v| v.as_str()) == Some(fingerprint.as_str())
        && entry2.get("ids").is_some()
    {
        return entry_config_map(&entry2);
    }

    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    let mut ids: HashMap<String, String> = HashMap::new();
    for p in &jars {
        let fname = p.file_name().unwrap().to_string_lossy().to_string();
        let id = jar_mod_id(p).unwrap_or_else(|| fallback_id(&fname));
        ids.insert(fname.clone(), id.clone());
        if id.is_empty() { continue; }
        let candidates = candidate_configs(&id, &files);
        if !candidates.is_empty() { map.insert(fname, candidates); }
    }

    let mut map_json = serde_json::Map::new();
    for (k, v) in &map { map_json.insert(k.clone(), json!(v)); }
    let mut ids_json = serde_json::Map::new();
    for (k, v) in &ids { ids_json.insert(k.clone(), json!(v)); }
    store2[name] = json!({ "fp": fingerprint, "map": map_json, "ids": ids_json });
    let _ = fs::write(cache_path, store2.to_string());
    map
}

static CLAIM_SUFFIXES: &[&str] =
    &["common", "server", "client", "general", "startup", "pre", "post", "config", "defaults"];

fn claims(id: &str, key: &str, kind: &str) -> bool {
    if id.is_empty() { return false; }
    if kind == "root" {
        if key == id { return true; }
        if key.starts_with(id) {
            let rest = &key[id.len()..];
            return CLAIM_SUFFIXES.iter().any(|s| rest == *s);
        }
        return false;
    }
    key == id || key.starts_with(id)
}

fn id_map(name: &str) -> HashMap<String, String> {
    ensure_index(name);
    let cache_path = APP.state_dir.join("configmap.json");
    let store = read_json(&cache_path);
    let entry = store.get(name).cloned().unwrap_or(Value::Null);
    let mut out = HashMap::new();
    if let Some(o) = entry.get("ids").and_then(|v| v.as_object()) {
        for (k, v) in o {
            if let Some(s) = v.as_str() { out.insert(k.clone(), s.to_string()); }
        }
    }
    out
}

/// Snapshot of jar ids and config files, built once for bulk operations.
pub struct IdSnapshot {
    pub ids: HashMap<String, String>,
    pub files: Vec<(String, String, String)>,
}

pub fn snapshot(name: &str) -> IdSnapshot {
    let ids = id_map(name);
    let files = instance_dir(name).map(|p| list_config_files(&p)).unwrap_or_default();
    IdSnapshot { ids, files }
}

/// Config files owned by one jar according to the snapshot.
pub fn owned_in(snapshot: &IdSnapshot, filename: &str) -> Vec<String> {
    let id = snapshot.ids.get(filename).map(|s| normalize_id(s)).unwrap_or_default();
    if id.is_empty() { return Vec::new(); }
    snapshot.files.iter()
        .filter(|(_, key, kind)| claims(&id, key, kind))
        .map(|t| t.0.clone())
        .collect()
}

/// Config files that also match another installed jar in the snapshot.
pub fn claimed_by_others_in(snapshot: &IdSnapshot, filename: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (rel, key, kind) in &snapshot.files {
        for (f, id) in &snapshot.ids {
            if f == filename { continue; }
            if claims(&normalize_id(id), key, kind) { out.push(rel.clone()); break; }
        }
    }
    out
}

/// Config files owned by one jar, without the 50-entry UI cap.
pub fn owned_files(name: &str, filename: &str) -> Vec<String> {
    let ids = id_map(name);
    let id = ids.get(filename).map(|s| normalize_id(s)).unwrap_or_default();
    if id.is_empty() { return Vec::new(); }
    let inst = match instance_dir(name) { Ok(p) => p, Err(_) => return Vec::new() };
    list_config_files(&inst).into_iter()
        .filter(|(_, key, kind)| claims(&id, key, kind))
        .map(|t| t.0)
        .collect()
}

/// Config files claimed by no installed mod id.
pub fn orphans(name: &str) -> Vec<String> {
    let inst = match instance_dir(name) { Ok(p) => p, Err(_) => return Vec::new() };
    let ids: Vec<String> = id_map(name).values().map(|s| normalize_id(s)).collect();
    list_config_files(&inst).into_iter()
        .filter(|(_, key, kind)| !ids.iter().any(|i| claims(i, key, kind)))
        .map(|t| t.0)
        .collect()
}

/// Attach `configs` lists to mod rows for the UI.
pub fn attach_configs(name: &str, rows: &mut [Value]) {
    let map = ensure_index(name);
    for r in rows.iter_mut() {
        let file = r.get("file").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(list) = map.get(file) {
            r["configs"] = json!(list);
        } else {
            r["configs"] = json!([]);
        }
    }
}

/// Open a config file in the host desktop.
pub fn open_config_file(name: &str, relative: &str) -> Result<String, AppError> {
    crate::core::instances::validate_name(name)?;
    let inst = instance_dir(name)?;
    let config_dir = inst.join("config");
    if relative.is_empty() || relative.contains("..") || relative.starts_with('/') {
        return Err(AppError::BadRequest("bad config path".into()));
    }
    let path = config_dir.join(relative);
    let canonical_config = fs::canonicalize(&config_dir).map_err(AppError::from)?;
    let canonical = fs::canonicalize(&path).map_err(AppError::from)?;
    if !canonical.starts_with(&canonical_config) {
        return Err(AppError::BadRequest("path outside config dir".into()));
    }
    if !canonical.is_file() { return Err(AppError::NotFound(relative.to_string())); }
    open_path(&canonical.display().to_string())?;
    Ok(canonical.display().to_string())
}

/// Open a config directory in the host desktop.
pub fn open_config_dir(name: &str, relative: &str) -> Result<String, AppError> {
    crate::core::instances::validate_name(name)?;
    let inst = instance_dir(name)?;
    let config_dir = inst.join("config");
    if relative.is_empty() {
        open_path(&config_dir.display().to_string())?;
        return Ok(config_dir.display().to_string());
    }
    if relative.contains("..") || relative.starts_with('/') {
        return Err(AppError::BadRequest("bad dir path".into()));
    }
    let path = config_dir.join(relative);
    let canonical_config = fs::canonicalize(&config_dir).map_err(AppError::from)?;
    let canonical = fs::canonicalize(&path).map_err(AppError::from)?;
    if !canonical.starts_with(&canonical_config) {
        return Err(AppError::BadRequest("path outside config dir".into()));
    }
    if !canonical.is_dir() { return Err(AppError::NotFound(relative.to_string())); }
    open_path(&canonical.display().to_string())?;
    Ok(canonical.display().to_string())
}
