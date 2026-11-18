//! Mod page links and link discovery.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use serde_json::{json, Map, Value};

use crate::core::hashing;
use crate::core::instances::instance_dir;
use crate::core::mods::mod_file_path;
use crate::providers::curseforge;
use crate::providers::http::{curseforge_post, modrinth_post};
use crate::core::state::{APP, read_json};
use crate::services::updates;

fn links_path() -> PathBuf {
    APP.state_dir.join("links.json")
}

/// Persistent mapping of `"instance|file"` to `{url, source, project}`.
pub fn links_cache() -> Value {
    let v = read_json(&links_path());
    if v.is_object() { v } else { json!({}) }
}

pub fn save_link(instance: &str, file: &str, url: &str, source: &str, project: Option<&str>) {
    if url.is_empty() { return; }
    let mut links = links_cache();
    links[format!("{instance}|{file}")] = json!({
        "url": url,
        "source": source,
        "project": project.unwrap_or(""),
    });
    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(links_path(), links.to_string());
}

/// Attach a clickable mod-page URL to each row.
///
/// Uses the update cache and link registry first; unknown jars are resolved
/// lazily by SHA-1 (Modrinth) and then CurseForge fingerprint, and cached.
pub fn attach_urls(instance_name: &str, rows: &mut [Value]) {
    let status = updates::update_status(instance_name);
    let items = status.get("items").and_then(|v| v.as_object());
    let mut links = links_cache();
    let mut missing: Vec<String> = Vec::new();

    for row in rows.iter_mut() {
        let file = row.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let key = format!("{instance_name}|{file}");
        let mut url: Option<String> = None;
        let mut source: Option<String> = None;

        if let Some(entry) = links.get(&key) {
            if let Some(u) = entry.get("url").and_then(|v| v.as_str()) {
                if !u.is_empty() { url = Some(u.to_string()); }
            }
            if let Some(s) = entry.get("source").and_then(|v| v.as_str()) {
                source = Some(s.to_string());
            }
        }

        if url.is_none() {
            if let Some(item) = items.and_then(|m| m.get(&file)) {
                match item.get("source").and_then(|v| v.as_str()) {
                    Some("modrinth") => {
                        if let Some(p) = item.get("project").and_then(|v| v.as_str()) {
                            if !p.is_empty() {
                                url = Some(format!("https://modrinth.com/mod/{p}"));
                            }
                        }
                    }
                    Some("curseforge") => {
                        if let Some(u) = item.get("url").and_then(|v| v.as_str()) {
                            if !u.is_empty() { url = Some(u.to_string()); }
                        } else if let Some(p) = item.get("project") {
                            if let Some(u) = curseforge::mod_url(&p) { url = Some(u); }
                        }
                    }
                    _ => {}
                }
                source = item.get("source").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
        }

        if let Some(u) = &url {
            links[key.clone()] = json!({ "url": u, "source": source.clone().unwrap_or_default() });
        } else {
            missing.push(file.clone());
        }

        row["url"] = json!(url);
        row["source"] = json!(source);
    }

    if !missing.is_empty() {
        if let Ok(inst) = instance_dir(instance_name) {
            let mut hash_cache = hashing::load_hash_cache();
            let mut hashes: HashMap<String, String> = HashMap::new();

            for file in &missing {
                if let Ok(path) = mod_file_path(&inst, file) {
                    if let Ok(h) = hashing::cached_sha1(&path, &mut hash_cache) {
                        hashes.insert(file.clone(), h);
                    }
                }
            }
            hashing::save_hash_cache(&hash_cache);

            let mut hash_list: Vec<String> = hashes.values().cloned().collect();
            hash_list.sort();
            hash_list.dedup();
            let mut by_hash: Map<String, Value> = Map::new();

            for chunk in hash_list.chunks(100) {
                if let Ok(v) = modrinth_post("/version_files", &json!({
                    "hashes": chunk,
                    "algorithm": "sha1",
                })) {
                    if let Some(o) = v.as_object() {
                        for (k, val) in o { by_hash.insert(k.clone(), val.clone()); }
                    }
                }
            }

            for file in &missing {
                let key = format!("{instance_name}|{file}");
                if links.get(&key).is_some() { continue; }
                if let Some(h) = hashes.get(file) {
                    if let Some(v) = by_hash.get(h) {
                        if let Some(pid) = v.get("project_id").and_then(|x| x.as_str()) {
                            if !pid.is_empty() {
                                links[key] = json!({
                                    "url": format!("https://modrinth.com/mod/{pid}"),
                                    "source": "modrinth",
                                });
                            }
                        }
                    }
                }
            }

            let still_missing: Vec<String> = missing.iter()
                .filter(|f| links.get(format!("{instance_name}|{f}")).is_none())
                .cloned()
                .collect();

            if !still_missing.is_empty() && APP.curseforge_key().is_some() {
                let mut fingerprints: HashMap<String, u32> = HashMap::new();
                for file in &still_missing {
                    if let Ok(path) = mod_file_path(&inst, file) {
                        if let Ok(fp) = hashing::cached_fingerprint(&path, &mut hash_cache) {
                            fingerprints.insert(file.clone(), fp);
                        }
                    }
                }
                hashing::save_hash_cache(&hash_cache);

                let mut values: Vec<u32> = fingerprints.values().cloned().collect();
                values.sort();
                values.dedup();
                let mut by_fp: HashMap<u32, Value> = HashMap::new();

                for chunk in values.chunks(100) {
                    if let Ok(res) = curseforge_post("/fingerprints", &json!({ "fingerprints": chunk })) {
                        let data = res.get("data").cloned().unwrap_or_else(|| json!({}));
                        let fp_list = data.get("exactFingerprints").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                        let matches = data.get("exactMatches").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                        for (fp_value, entry) in fp_list.iter().zip(matches.iter()) {
                            let id = entry.get("id").cloned()
                                .or_else(|| entry.get("file").and_then(|f| f.get("modId")).cloned())
                                .unwrap_or(Value::Null);
                            if let Some(fp) = fp_value.as_u64() { by_fp.insert(fp as u32, id); }
                        }
                    }
                }

                for file in &still_missing {
                    let key = format!("{instance_name}|{file}");
                    if let Some(fp) = fingerprints.get(file) {
                        if let Some(id) = by_fp.get(fp) {
                            if let Some(u) = curseforge::mod_url(id) {
                                links[key] = json!({ "url": u, "source": "curseforge" });
                            }
                        }
                    }
                }
            }
        }
    }

    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(links_path(), links.to_string());

    for row in rows.iter_mut() {
        let file = row.get("file").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(entry) = links.get(format!("{instance_name}|{file}")) {
            if let Some(u) = entry.get("url").and_then(|v| v.as_str()) {
                if !u.is_empty() { row["url"] = json!(u); }
            }
            if let Some(s) = entry.get("source").and_then(|v| v.as_str()) {
                row["source"] = json!(s);
            }
        }
    }
}
