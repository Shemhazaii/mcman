//! Mod page links and link discovery.

use std::fs;
use std::path::PathBuf;
use serde_json::{json, Value};

use crate::providers::curseforge;
use crate::core::state::{APP, read_json};
use crate::services::updates;

fn links_path() -> PathBuf {
    APP.state_dir.join("links.json")
}

fn names_path() -> PathBuf {
    APP.state_dir.join("names.json")
}

/// Persistent `"source:project" -> official project title` cache.
pub fn names_cache() -> Value {
    let v = read_json(&names_path());
    if v.is_object() { v } else { json!({}) }
}

pub fn save_names(cache: &Value) {
    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(names_path(), cache.to_string());
}

/// CurseForge project IDs are numeric; Modrinth IDs are alphanumeric slugs.
pub fn project_cache_key(project: &str) -> String {
    if project.chars().all(|c| c.is_ascii_digit()) {
        format!("curseforge:{project}")
    } else {
        format!("modrinth:{project}")
    }
}

pub fn save_links(cache: &Value) {
    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(links_path(), cache.to_string());
}

/// Persistent mapping of `"instance|file"` to `{url, source, project, name, version}`.
pub fn links_cache() -> Value {
    let v = read_json(&links_path());
    if v.is_object() { v } else { json!({}) }
}

pub fn save_link(
    instance: &str,
    file: &str,
    url: &str,
    source: &str,
    project: Option<&str>,
    name: Option<&str>,
    version: Option<&str>,
) {
    if url.is_empty() { return; }
    let mut links = links_cache();
    links[format!("{instance}|{file}")] = json!({
        "url": url,
        "source": source,
        "project": project.unwrap_or(""),
        "name": name.unwrap_or(""),
        "version": version.unwrap_or(""),
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
    let names = names_cache();

    for row in rows.iter_mut() {
        let file = row.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let key = format!("{instance_name}|{file}");
        let mut url: Option<String> = None;
        let mut source: Option<String> = None;
        let mut project: Option<String> = None;
        let mut display_name: Option<String> = None;
        let mut display_version: Option<String> = None;

        if let Some(entry) = links.get(&key) {
            if let Some(u) = entry.get("url").and_then(|v| v.as_str()) {
                if !u.is_empty() { url = Some(u.to_string()); }
            }
            if let Some(s) = entry.get("source").and_then(|v| v.as_str()) {
                source = Some(s.to_string());
            }
            if let Some(p) = entry.get("project").and_then(|v| v.as_str()) {
                if !p.is_empty() { project = Some(p.to_string()); }
            }
            if let Some(n) = entry.get("name").and_then(|v| v.as_str()) {
                if !n.is_empty() { display_name = Some(n.to_string()); }
            }
            if let Some(v) = entry.get("version").and_then(|v| v.as_str()) {
                if !v.is_empty() { display_version = Some(v.to_string()); }
            }
        }

        if let Some(item) = items.and_then(|m| m.get(&file)) {
            if source.is_none() {
                source = item.get("source").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
            if let Some(p) = item.get("project") {
                let pid = p.as_u64().map(|v| v.to_string())
                    .or_else(|| p.as_str().map(|s| s.to_string()))
                    .filter(|s| !s.is_empty());
                if pid.is_some() { project = pid; }
            }
            if display_name.is_none() {
                if let Some(n) = item.get("installed_name").and_then(|v| v.as_str()) {
                    if !n.is_empty() { display_name = Some(n.to_string()); }
                }
            }
            if url.is_none() {
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
            }
        }

        if project.is_none() {
            if let Some(u) = &url {
                if let Some(slug) = u.split("/mod/").nth(1) {
                    if !slug.is_empty() { project = Some(slug.to_string()); }
                }
            }
        }

        if display_name.is_none() {
            if let Some(p) = &project {
                let key = project_cache_key(p);
                if let Some(n) = names.get(&key).and_then(|v| v.as_str()) {
                    if !n.is_empty() { display_name = Some(n.to_string()); }
                }
            }
        }

        if let Some(u) = &url {
            links[key.clone()] = json!({
                "url": u,
                "source": source.clone().unwrap_or_default(),
                "project": project.clone().unwrap_or_default(),
                "name": display_name.clone().unwrap_or_default(),
                "version": display_version.clone().unwrap_or_default(),
            });
        } else {
            // Unknown jars are intentionally not resolved here: listing must not
            // hash large jars or call providers synchronously. The update check
            // fills project IDs; a later list call builds URLs from the cache.
        }

        row["url"] = json!(url);
        row["source"] = json!(source);
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
            if let Some(n) = entry.get("name").and_then(|v| v.as_str()) {
                if !n.is_empty() { row["name"] = json!(n); }
            }
            if let Some(v) = entry.get("version").and_then(|v| v.as_str()) {
                if !v.is_empty() { row["version"] = json!(v); }
            }
        }
    }
}
