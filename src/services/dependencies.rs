//! Dependency resolution for Modrinth and CurseForge installs.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::Path;
use serde_json::{json, Value};

use crate::services::download;
use crate::core::errors::AppError;
use crate::core::hashing;
use crate::core::instances::mod_dir;
use crate::core::links;
use crate::core::progress;
use crate::providers::curseforge;
use crate::providers::http::modrinth_get;
use crate::core::state::{APP, read_json};

/// Pick the primary file from a Modrinth version, falling back to the first.
pub fn pick_primary_file(files: &[Value]) -> Option<Value> {
    files.iter().find(|f| f["primary"] == json!(true))
        .or_else(|| files.first())
        .cloned()
}

fn record_dependency(instance_name: &str, filename: &str, parent: &str, source: &str) {
    let path = APP.state_dir.join("depgraph.json");
    let mut graph = read_json(&path);
    if !graph.is_object() { graph = json!({}); }
    let entry = graph.as_object_mut().unwrap()
        .entry(instance_name.to_string())
        .or_insert_with(|| json!({}));
    entry[filename] = json!({ "for": parent, "source": source });
    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(path, graph.to_string());
}

fn installed_sets(instance: &Path) -> (HashSet<String>, HashSet<String>) {
    let mut names = HashSet::new();
    let mut sha1s = HashSet::new();
    let hash_cache = hashing::load_hash_cache();

    if let Ok(rd) = fs::read_dir(instance.join("mods")) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() { continue; }
            names.insert(p.file_name().unwrap().to_string_lossy().to_lowercase());
            if let Some(entry) = hash_cache.get(p.display().to_string()) {
                if let Some(s) = entry.get("sha1").and_then(|v| v.as_str()) {
                    sha1s.insert(s.to_lowercase());
                }
            }
        }
    }
    if let Ok(rd) = fs::read_dir(instance.join("plugins")) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() { continue; }
            names.insert(p.file_name().unwrap().to_string_lossy().to_lowercase());
            if let Some(entry) = hash_cache.get(p.display().to_string()) {
                if let Some(s) = entry.get("sha1").and_then(|v| v.as_str()) {
                    sha1s.insert(s.to_lowercase());
                }
            }
        }
    }
    (names, sha1s)
}

fn dependency_present(
    names: &HashSet<String>,
    sha1s: &HashSet<String>,
    filename: &str,
    sha1: &str,
) -> bool {
    let lower = filename.to_lowercase();
    if names.contains(&lower) || names.contains(&(lower.clone() + ".disabled")) {
        return true;
    }
    !sha1.is_empty() && sha1s.contains(&sha1.to_lowercase())
}

/// Install required Modrinth dependencies breadth-first.
pub fn install_modrinth_dependencies(
    instance: &Path,
    version: &Value,
    loader: &str,
    mcver: &str,
    parent_file: &str,
    key: Option<&str>,
) -> Result<Vec<String>, AppError> {
    let (mut names, mut sha1s) = installed_sets(instance);
    let mut added = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, Option<String>)> = VecDeque::new();

    for dep in version.get("dependencies").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
        if dep["dependency_type"] == json!("required") {
            if let Some(pid) = dep.get("project_id").and_then(|v| v.as_str()) {
                queue.push_back((
                    pid.to_string(),
                    dep.get("version_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                ));
            }
        }
    }

    let mut guard = 0;
    while let Some((project_id, version_id)) = queue.pop_front() {
        guard += 1;
        if guard >= 50 { break; }
        if project_id.is_empty() || seen.contains(&project_id) { continue; }
        seen.insert(project_id.clone());

        let resolved: Option<Value> = if let Some(vid) = version_id {
            match modrinth_get(&format!("/version/{vid}"), &[]) {
                Ok(Value::Array(a)) => a.into_iter().next(),
                Ok(Value::Null) => None,
                Ok(v) => Some(v),
                Err(_) => None,
            }
        } else {
            let params = {
                let mut p = Vec::new();
                if !mcver.is_empty() { p.push(("game_versions", json!([mcver]).to_string())); }
                if !loader.is_empty() { p.push(("loaders", json!([loader]).to_string())); }
                p
            };
            match modrinth_get(&format!("/project/{project_id}/version"), &params) {
                Ok(Value::Array(a)) => a.into_iter().next(),
                _ => None,
            }
        };

        let resolved = match resolved { Some(v) => v, None => continue };
        for dep in resolved.get("dependencies").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
            if dep["dependency_type"] == json!("required") {
                if let Some(p2) = dep.get("project_id").and_then(|v| v.as_str()) {
                    queue.push_back((
                        p2.to_string(),
                        dep.get("version_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    ));
                }
            }
        }

        let files = resolved.get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let primary = match pick_primary_file(&files) { Some(p) => p, None => continue };
        let sha1 = primary.get("hashes").and_then(|h| h.get("sha1"))
            .and_then(|v| v.as_str()).unwrap_or("").to_string();
        let filename = primary["filename"].as_str().unwrap_or("").to_string();

        if dependency_present(&names, &sha1s, &filename, &sha1) { continue; }
        let dest = mod_dir(instance)
            .unwrap_or_else(|| instance.join("mods"))
            .join(&filename);
        if dest.exists() { continue; }
        if download::download_file(primary["url"].as_str().unwrap_or(""), &dest, &[], key).is_err() {
            continue;
        }

        progress::finish_file_if_key(key);
        names.insert(filename.to_lowercase());
        if !sha1.is_empty() { sha1s.insert(sha1.to_lowercase()); }
        added.push(filename.clone());

        let instance_name = instance.file_name().unwrap().to_string_lossy().to_string();
        let dep_name = resolved.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let dep_version = resolved.get("version_number").and_then(|v| v.as_str()).unwrap_or("");
        links::save_link(
            &instance_name,
            &filename,
            &format!("https://modrinth.com/mod/{project_id}"),
            "modrinth",
            Some(&project_id),
            Some(dep_name),
            Some(dep_version),
        );
        record_dependency(&instance_name, &filename, parent_file, "modrinth");
    }
    Ok(added)
}

fn curseforge_sha1(file: &Value) -> String {
    file.get("hashes").and_then(|v| v.as_array()).and_then(|arr| {
        arr.iter().find(|h| h["algo"] == json!(1))
            .and_then(|h| h.get("value"))
            .and_then(|v| v.as_str())
    }).unwrap_or("").to_string()
}

/// Install required CurseForge dependencies breadth-first.
pub fn install_curseforge_dependencies(
    instance: &Path,
    file: &Value,
    loader: &str,
    mcver: &str,
    parent_file: &str,
    key: Option<&str>,
) -> Result<Vec<String>, AppError> {
    let (mut names, mut sha1s) = installed_sets(instance);
    let mut added = Vec::new();
    let mut guard = 0;
    let mut seen: HashSet<u64> = HashSet::new();

    if let Some(m) = file.get("modId").and_then(|v| v.as_u64()) { seen.insert(m); }
    let mut queue: VecDeque<u64> = VecDeque::new();
    for dep in file.get("dependencies").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
        if dep["relationType"] == json!(3) {
            if let Some(m) = dep.get("modId").and_then(|v| v.as_u64()) { queue.push_back(m); }
        }
    }

    while let Some(mod_id) = queue.pop_front() {
        guard += 1;
        if guard >= 50 { break; }
        if seen.contains(&mod_id) { continue; }
        seen.insert(mod_id);

        let available = match curseforge::files(&json!(mod_id), mcver, loader, 25) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let chosen = match available.first() { Some(f) => f, None => continue };

        for dep in chosen.get("dependencies").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
            if dep["relationType"] == json!(3) {
                if let Some(m) = dep.get("modId").and_then(|v| v.as_u64()) { queue.push_back(m); }
            }
        }

        let sha1 = curseforge_sha1(chosen);
        let filename = chosen["fileName"].as_str().unwrap_or("").to_string();
        if dependency_present(&names, &sha1s, &filename, &sha1) { continue; }
        let dest = mod_dir(instance)
            .unwrap_or_else(|| instance.join("mods"))
            .join(&filename);
        if dest.exists() { continue; }
        if download::download_file(
            &curseforge::download_url(chosen),
            &dest,
            &crate::providers::http::curseforge_headers().unwrap_or_default(),
            key,
        ).is_err() {
            continue;
        }

        progress::finish_file_if_key(key);
        names.insert(filename.to_lowercase());
        if !sha1.is_empty() { sha1s.insert(sha1.to_lowercase()); }
        added.push(filename.clone());

        let instance_name = instance.file_name().unwrap().to_string_lossy().to_string();
        let dep_name = chosen.get("displayName").and_then(|v| v.as_str()).unwrap_or("");
        let dep_version = {
            let (_, v) = crate::core::mods::parse_mod_name(chosen["fileName"].as_str().unwrap_or(""));
            if !v.is_empty() {
                v
            } else if !dep_name.is_empty() {
                dep_name.to_string()
            } else {
                "".to_string()
            }
        };
        if let Some(url) = curseforge::mod_url(&json!(mod_id)) {
            links::save_link(&instance_name, &filename, &url, "curseforge", Some(&mod_id.to_string()), Some(dep_name), Some(&dep_version));
        }
        record_dependency(&instance_name, &filename, parent_file, "curseforge");
    }
    Ok(added)
}


