//! Trash / restore lifecycle for mod jars and their config files.
//!
//! Trashed jars and configs live under `<root>/.deleted/<instance>/`. Config
//! moves are tracked in `trashmeta.json` so restoring a jar also restores the
//! configs that belonged only to it.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use serde_json::{json, Value};

use crate::services::config_files;
use crate::core::errors::AppError;
use crate::core::instances::{instance_dir, validate_name};
use crate::core::mods::strip_trash_timestamp;
use crate::core::state::{APP, read_json};

/// Validate a bare filename (no path components).
pub fn validate_file_name(f: &str) -> Result<(), AppError> {
    if f.is_empty() || f.contains('/') || f.contains('\\') || f.contains("..") || f == "." {
        return Err(AppError::BadRequest(format!("bad name: {f:?}")));
    }
    Ok(())
}

/// Instance trash directory: `<root>/.deleted/<name>`.
pub fn trash_dir(name: &str) -> Result<PathBuf, AppError> {
    validate_name(name)?;
    let root = APP.root_path().ok_or_else(|| AppError::NotFound("no root".into()))?;
    Ok(root.join(".deleted").join(name))
}

fn trash_config_dir(name: &str) -> Result<PathBuf, AppError> {
    Ok(trash_dir(name)?.join("config"))
}

static TRASH_METADATA_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn metadata_path() -> PathBuf {
    APP.state_dir.join("trashmeta.json")
}

fn metadata_configs(name: &str, filename: &str) -> Vec<String> {
    let v = read_json(&metadata_path());
    v.get(name)
        .and_then(|i| i.get(filename))
        .and_then(|e| e.get("configs"))
        .and_then(|c| c.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default()
}

fn metadata_put(name: &str, filename: &str, configs: &[String]) {
    let _guard = TRASH_METADATA_LOCK.lock().unwrap();
    let mut store = read_json(&metadata_path());
    if !store.is_object() { store = json!({}); }
    let mut instance = store.get(name).cloned().unwrap_or_else(|| json!({}));
    if !instance.is_object() { instance = json!({}); }
    instance[filename] = json!({ "configs": configs });
    store[name] = instance;
    let _ = fs::write(metadata_path(), store.to_string());
}

fn metadata_drop(name: &str, filename: &str) {
    let _guard = TRASH_METADATA_LOCK.lock().unwrap();
    let mut store = read_json(&metadata_path());
    let obj = match store.as_object_mut() { Some(o) => o, None => return };
    let mut empty = false;
    if let Some(instance) = obj.get_mut(name) {
        if let Some(o) = instance.as_object_mut() {
            o.remove(filename);
            empty = o.is_empty();
        }
    }
    if empty { obj.remove(name); }
    let _ = fs::write(metadata_path(), store.to_string());
}

fn metadata_drop_instance(name: &str) {
    let _guard = TRASH_METADATA_LOCK.lock().unwrap();
    let mut store = read_json(&metadata_path());
    if let Some(o) = store.as_object_mut() { o.remove(name); }
    let _ = fs::write(metadata_path(), store.to_string());
}

/// Move one config file into the trash, preserving its relative path.
fn move_config_to_trash(name: &str, relative: &str) -> Result<(), AppError> {
    if relative.is_empty() || relative.contains("..") || relative.starts_with('/') {
        return Err(AppError::BadRequest("bad config path".into()));
    }
    let inst = instance_dir(name)?;
    let src = inst.join("config").join(relative);
    if !src.is_file() { return Ok(()); }
    let dst = trash_config_dir(name)?.join(relative);
    if let Some(parent) = dst.parent() { fs::create_dir_all(parent)?; }
    if dst.exists() { let _ = fs::remove_file(&dst); }
    fs::rename(&src, &dst)
        .or_else(|_| fs::copy(&src, &dst).and_then(|_| fs::remove_file(&src)))?;
    Ok(())
}

/// Move a jar into the trash, adding a timestamp suffix on collisions.
fn stash_jar(path: &Path, trash_dir: &Path, original_name: &str) -> Result<String, AppError> {
    let mut stored = original_name.to_string();
    let mut dest = trash_dir.join(&stored);
    if dest.exists() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        stored = format!("{original_name}.{ts}");
        dest = trash_dir.join(&stored);
    }
    fs::rename(path, &dest)
        .or_else(|_| fs::copy(path, &dest).and_then(|_| fs::remove_file(path)))?;
    Ok(stored)
}

/// Trash one mod, moving configs that only it owns into the trash metadata.
pub fn trash_mod(name: &str, filename: &str) -> Result<String, AppError> {
    let inst = instance_dir(name)?;
    let path = crate::core::mods::mod_file_path(&inst, filename)?;

    // Resolve ownership before moving the jar; the config index keys on it.
    let snapshot = config_files::snapshot(name);
    let shared = config_files::claimed_by_others_in(&snapshot, filename);
    let owned = config_files::owned_in(&snapshot, filename);

    let tdir = trash_dir(name)?;
    fs::create_dir_all(&tdir)?;
    let original = path.file_name().unwrap().to_string_lossy().to_string();
    let stored = stash_jar(&path, &tdir, &original)?;

    let mut moved: Vec<String> = Vec::new();
    for rel in owned {
        if shared.contains(&rel) { continue; }
        if move_config_to_trash(name, &rel).is_ok() { moved.push(rel); }
    }
    if !moved.is_empty() { metadata_put(name, &stored, &moved); }
    Ok(stored)
}

/// Trash a batch of mods using one config snapshot.
pub fn trash_mods(name: &str, files: &[String]) -> Result<Value, AppError> {
    validate_name(name)?;
    let inst = instance_dir(name)?;
    let snapshot = config_files::snapshot(name);
    let tdir = trash_dir(name)?;
    fs::create_dir_all(&tdir)?;

    let mut moved: Vec<String> = Vec::new();
    let mut errors: Vec<Value> = Vec::new();
    let mut config_count = 0usize;

    for f in files {
        validate_file_name(f)?;
        let path = match crate::core::mods::mod_file_path(&inst, f) {
            Ok(p) => p,
            Err(e) => {
                errors.push(json!({ "file": f, "error": e.to_string() }));
                continue;
            }
        };
        let shared = config_files::claimed_by_others_in(&snapshot, f);
        let owned = config_files::owned_in(&snapshot, f);
        let original = path.file_name().unwrap().to_string_lossy().to_string();

        match stash_jar(&path, &tdir, &original) {
            Ok(stored) => {
                let mut got: Vec<String> = Vec::new();
                for rel in owned {
                    if shared.contains(&rel) { continue; }
                    if move_config_to_trash(name, &rel).is_ok() { got.push(rel); }
                }
                config_count += got.len();
                if !got.is_empty() { metadata_put(name, &stored, &got); }
                moved.push(stored);
            }
            Err(e) => errors.push(json!({ "file": f, "error": e.to_string() })),
        }
    }
    Ok(json!({ "moved": moved, "configs": config_count, "errors": errors }))
}

fn entry_json(file: String, md: fs::Metadata, kind: &str) -> Value {
    let mtime = md.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    json!({ "file": file, "size": md.len(), "mtime": mtime, "kind": kind })
}

/// List trashed jars and config files for an instance.
pub fn list_trash(name: &str) -> Result<Vec<Value>, AppError> {
    validate_name(name)?;
    let tdir = match APP.root_path() {
        Some(root) => root.join(".deleted").join(name),
        None => return Ok(Vec::new()),
    };

    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(&tdir) {
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let md = match e.metadata() { Ok(m) => m, Err(_) => continue };
            if !md.is_file() { continue; }
            out.push(entry_json(e.file_name().to_string_lossy().to_string(), md, "jar"));
        }
    }

    let cdir = tdir.join("config");
    let mut stack = vec![(cdir.clone(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let rd = match fs::read_dir(&dir) { Ok(r) => r, Err(_) => continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 4 { stack.push((p, depth + 1)); }
                continue;
            }
            let md = match e.metadata() { Ok(m) => m, Err(_) => continue };
            let rel = p.strip_prefix(&cdir).unwrap_or(&p).display().to_string();
            out.push(entry_json(rel, md, "config"));
        }
    }
    Ok(out)
}

fn restore_config(name: &str, relative: &str) -> Result<String, AppError> {
    validate_name(name)?;
    if relative.is_empty() || relative.contains("..") || relative.starts_with('/') {
        return Err(AppError::BadRequest("bad config path".into()));
    }
    let tdir = trash_config_dir(name)?;
    let src = tdir.join(relative);
    let canonical_trash = fs::canonicalize(&tdir).map_err(AppError::from)?;
    let canonical = fs::canonicalize(&src).map_err(AppError::from)?;
    if !canonical.starts_with(&canonical_trash) {
        return Err(AppError::BadRequest("path outside trash".into()));
    }
    if !canonical.is_file() { return Err(AppError::NotFound(relative.to_string())); }

    let inst = instance_dir(name)?;
    let dest = inst.join("config").join(relative);
    if dest.exists() { return Err(AppError::BadRequest(format!("exists: {relative}"))); }
    if let Some(parent) = dest.parent() { fs::create_dir_all(parent)?; }
    fs::rename(&canonical, &dest)
        .or_else(|_| fs::copy(&canonical, &dest).and_then(|_| fs::remove_file(&canonical)))?;
    Ok(relative.to_string())
}

/// Restore a jar or config file from the trash.
pub fn restore_mod(name: &str, filename: &str, kind: &str) -> Result<String, AppError> {
    if kind == "config" { return restore_config(name, filename); }

    validate_name(name)?;
    validate_file_name(filename)?;
    let src = trash_dir(name)?.join(filename);
    if !src.is_file() { return Err(AppError::NotFound(filename.to_string())); }

    let inst = instance_dir(name)?;
    let bare = strip_trash_timestamp(filename);
    let mut subdir = "mods";
    if !inst.join("mods").is_dir() && inst.join("plugins").is_dir() { subdir = "plugins"; }
    let dest = inst.join(subdir).join(&bare);
    if dest.exists() { return Err(AppError::BadRequest(format!("exists: {bare}"))); }

    fs::rename(&src, &dest)
        .or_else(|_| fs::copy(&src, &dest).and_then(|_| fs::remove_file(&src)))?;

    for rel in metadata_configs(name, filename) {
        let config_src = trash_config_dir(name)?.join(&rel);
        if !config_src.is_file() { continue; }
        let config_dest = inst.join("config").join(&rel);
        if config_dest.exists() { continue; }
        if let Some(parent) = config_dest.parent() { fs::create_dir_all(parent)?; }
        fs::rename(&config_src, &config_dest)
            .or_else(|_| fs::copy(&config_src, &config_dest).and_then(|_| fs::remove_file(&config_src)))?;
    }
    metadata_drop(name, filename);
    Ok(bare)
}

/// Permanently delete one trashed jar plus its tracked configs.
pub fn purge_mod(name: &str, filename: &str, kind: &str) -> Result<Value, AppError> {
    validate_name(name)?;
    if kind == "config" {
        if filename.is_empty() || filename.contains("..") || filename.starts_with('/') {
            return Err(AppError::BadRequest("bad config path".into()));
        }
        let tdir = trash_config_dir(name)?;
        let p = tdir.join(filename);
        let canonical_trash = fs::canonicalize(&tdir).map_err(AppError::from)?;
        let canonical = fs::canonicalize(&p).map_err(AppError::from)?;
        if !canonical.starts_with(&canonical_trash) {
            return Err(AppError::BadRequest("path outside trash".into()));
        }
        if !canonical.is_file() { return Err(AppError::NotFound(filename.to_string())); }
        fs::remove_file(&canonical)?;
        return Ok(json!({ "purged": [filename], "count": 1 }));
    }

    validate_file_name(filename)?;
    let tdir = trash_dir(name)?;
    let p = tdir.join(filename);
    if !p.is_file() { return Err(AppError::NotFound(filename.to_string())); }
    let mut gone = vec![filename.to_string()];
    fs::remove_file(&p)?;
    for rel in metadata_configs(name, filename) {
        let c = tdir.join("config").join(&rel);
        if c.is_file() {
            let _ = fs::remove_file(&c);
            gone.push(rel);
        }
    }
    metadata_drop(name, filename);
    Ok(json!({ "purged": gone, "count": gone.len() }))
}

/// Permanently delete every trashed file for one instance.
pub fn purge_all(name: &str) -> Result<Value, AppError> {
    validate_name(name)?;
    let tdir = trash_dir(name)?;
    let mut gone: Vec<String> = Vec::new();

    if tdir.is_dir() {
        if let Ok(rd) = fs::read_dir(&tdir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() { continue; }
                if fs::remove_file(&p).is_ok() {
                    gone.push(e.file_name().to_string_lossy().to_string());
                }
            }
        }
        let cdir = tdir.join("config");
        if cdir.is_dir() {
            let mut stack = vec![cdir.clone()];
            while let Some(dir) = stack.pop() {
                if let Ok(rd) = fs::read_dir(&dir) {
                    for e in rd.flatten() {
                        let p = e.path();
                        if p.is_dir() { stack.push(p); continue; }
                        let rel = p.strip_prefix(&cdir).unwrap_or(&p).display().to_string();
                        if fs::remove_file(&p).is_ok() { gone.push(rel); }
                    }
                }
            }
            let _ = fs::remove_dir_all(&cdir);
        }
    }
    metadata_drop_instance(name);
    Ok(json!({ "purged": gone, "count": gone.len() }))
}

/// Move unclaimed config files into the trash.
pub fn trash_orphans(name: &str, only: Option<Vec<String>>) -> Result<Value, AppError> {
    validate_name(name)?;
    let all = config_files::orphans(name);
    let picked: Vec<String> = match only {
        Some(list) => {
            let set: HashSet<String> = list.into_iter().collect();
            all.into_iter().filter(|r| set.contains(r)).collect()
        }
        None => all,
    };

    let mut moved = Vec::new();
    for rel in picked {
        if move_config_to_trash(name, &rel).is_ok() { moved.push(rel); }
    }
    Ok(json!({
        "moved": moved,
        "count": moved.len(),
        "remaining": config_files::orphans(name)
    }))
}
