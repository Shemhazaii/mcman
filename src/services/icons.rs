//! Mod icon discovery and caching.

use std::fs;
use std::path::PathBuf;
use serde_json::{json, Value};

use crate::core::links::links_cache;
use crate::providers::http::{curseforge_post, modrinth_get};
use crate::core::state::{APP, read_json};
use crate::services::updates;

fn icons_path() -> PathBuf {
    APP.state_dir.join("icons.json")
}

/// Persistent `"source:id" -> icon URL` cache.
pub fn icons_cache() -> Value {
    let v = read_json(&icons_path());
    if v.is_object() { v } else { json!({}) }
}

fn save_icons(cache: &Value) {
    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(icons_path(), cache.to_string());
}

/// Attach icon URLs to mod rows, batching unknown project lookups.
pub fn attach_icons(instance_name: &str, rows: &mut [Value]) {
    let status = updates::update_status(instance_name);
    let items = status.get("items").and_then(|v| v.as_object());
    let links = links_cache();
    let mut cache = icons_cache();

    let mut need_modrinth: Vec<String> = Vec::new();
    let mut need_curseforge: Vec<u64> = Vec::new();
    let mut slug_lookups: Vec<(usize, String)> = Vec::new();
    let mut row_keys: Vec<(usize, String, String)> = Vec::new();

    for (i, row) in rows.iter_mut().enumerate() {
        let file = row.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let base = file.strip_suffix(".disabled").unwrap_or(&file).to_string();

        let mut source = String::new();
        let mut project_id = String::new();

        if let Some(item) = items.and_then(|m| m.get(&file).or_else(|| m.get(&base))) {
            source = item.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if let Some(p) = item.get("project") {
                project_id = p.as_u64().map(|v| v.to_string())
                    .unwrap_or_else(|| p.as_str().unwrap_or("").to_string());
            }
        }

        if project_id.is_empty() {
            if let Some(entry) = links.get(format!("{instance_name}|{file}"))
                .or_else(|| links.get(format!("{instance_name}|{base}"))) {
                source = entry.get("source").and_then(|v| v.as_str()).unwrap_or(&source).to_string();
                project_id = entry.get("project").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
        }

        if project_id.is_empty() {
            if let Some(url) = row.get("url").and_then(|v| v.as_str()) {
                if source == "modrinth" {
                    if let Some(slug) = url.split("/mod/").nth(1) {
                        if !slug.is_empty() { slug_lookups.push((i, slug.to_string())); }
                    }
                }
            }
        }

        if !project_id.is_empty() && source != "curseforge" {
            let key = format!("modrinth:{project_id}");
            if cache.get(&key).and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false) {
                row["icon"] = cache.get(&key).cloned().unwrap_or(json!(""));
            } else {
                need_modrinth.push(project_id.clone());
            }
            row_keys.push((i, "modrinth".to_string(), project_id.clone()));
        } else if !project_id.is_empty() {
            let key = format!("curseforge:{project_id}");
            if cache.get(&key).and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false) {
                row["icon"] = cache.get(&key).cloned().unwrap_or(json!(""));
            } else if let Ok(n) = project_id.parse::<u64>() {
                need_curseforge.push(n);
            }
            row_keys.push((i, "curseforge".to_string(), project_id.clone()));
        }
    }

    need_modrinth.sort();
    need_modrinth.dedup();
    for chunk in need_modrinth.chunks(50) {
        if let Ok(v) = modrinth_get("/projects", &[("ids", json!(chunk).to_string())]) {
            for p in v.as_array().cloned().unwrap_or_default() {
                let id = p.get("id").and_then(|x| x.as_str()).unwrap_or("");
                let icon = p.get("icon_url").and_then(|x| x.as_str()).unwrap_or("");
                if !id.is_empty() && !icon.is_empty() {
                    cache[format!("modrinth:{id}")] = json!(icon);
                }
            }
        }
    }

    need_curseforge.sort();
    need_curseforge.dedup();
    for chunk in need_curseforge.chunks(50) {
        if let Ok(v) = curseforge_post("/mods", &json!({ "modIds": chunk })) {
            for m in v.get("data").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                let id = m.get("id").and_then(|x| x.as_u64()).map(|x| x.to_string()).unwrap_or_default();
                let icon = m.get("logo").and_then(|l| l.get("thumbnailUrl")).and_then(|x| x.as_str())
                    .or_else(|| m.get("logo").and_then(|l| l.get("url")).and_then(|x| x.as_str()))
                    .unwrap_or("");
                if !id.is_empty() && !icon.is_empty() {
                    cache[format!("curseforge:{id}")] = json!(icon);
                }
            }
        }
    }

    for (i, slug) in slug_lookups {
        if let Ok(p) = modrinth_get(&format!("/project/{slug}"), &[]) {
            let id = p.get("id").and_then(|x| x.as_str()).unwrap_or("");
            let icon = p.get("icon_url").and_then(|x| x.as_str()).unwrap_or("");
            if !id.is_empty() && !icon.is_empty() {
                cache[format!("modrinth:{id}")] = json!(icon);
                rows[i]["icon"] = json!(icon);
            }
        }
    }

    for (i, source, project_id) in row_keys {
        let key = format!("{source}:{project_id}");
        if let Some(v) = cache.get(&key).and_then(|x| x.as_str()) {
            rows[i]["icon"] = json!(v);
        }
    }

    save_icons(&cache);
}
