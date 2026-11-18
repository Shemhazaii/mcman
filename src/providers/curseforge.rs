//! CurseForge search, fingerprint matching, and installation.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;
use regex::Regex;
use serde_json::{json, Map, Value};

use crate::services::dependencies;
use crate::services::download;
use crate::core::errors::AppError;
use crate::core::hashing;
use crate::core::instances::{instance_dir, instance_loader_mc, mod_dir};
use crate::core::links;
use crate::core::mods;
use crate::core::progress;
use crate::providers::apply_loader_mc_override;
use crate::providers::http::{
    curseforge_get, curseforge_headers, curseforge_post,
};
use crate::core::state::{APP, read_json};

static HTML_TAG_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
static NON_WORD_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\W+").unwrap());

/// Resolve a download URL, falling back to the old forgecdn file layout.
pub fn download_url(file: &Value) -> String {
    if let Some(url) = file.get("downloadUrl").and_then(|v| v.as_str()) {
        if !url.is_empty() { return url.to_string(); }
    }
    let id = file["id"].as_u64().unwrap_or(0);
    let name = file["fileName"].as_str().unwrap_or("");
    format!(
        "https://edge.forgecdn.net/files/{}/{}/{}",
        id / 1000,
        id % 1000,
        urlencoding::encode(name)
    )
}

fn category_names(project: &Value) -> HashSet<String> {
    let mut names = HashSet::new();
    if let Some(arr) = project.get("allowedCategories").and_then(|v| v.as_array()) {
        for c in arr {
            let name = if c.is_object() {
                c.get("gameVersionSlug").and_then(|g| g.get("name")).and_then(|v| v.as_str())
                    .or_else(|| c.get("name").and_then(|v| v.as_str()))
            } else {
                c.as_str()
            };
            if let Some(n) = name { names.insert(n.to_lowercase()); }
        }
    }
    names
}

fn loader_token(loader: &str) -> Option<&'static str> {
    match loader {
        "fabric" => Some("fabric"),
        "quilt" => Some("quilt"),
        "neoforge" => Some("neoforge"),
        "forge" => Some("forge"),
        "paper" => Some("paper"),
        "spigot" => Some("spigot"),
        _ => None,
    }
}

fn loader_categories(loader: &str) -> Option<HashSet<String>> {
    match loader {
        "fabric" => Some(["fabric", "quilt"].into_iter().map(|s| s.to_string()).collect()),
        "neoforge" => Some(["neoforge", "forge"].into_iter().map(|s| s.to_string()).collect()),
        "forge" => Some(["forge"].into_iter().map(|s| s.to_string()).collect()),
        "paper" => Some(["bukkit", "spigot", "paper"].into_iter().map(|s| s.to_string()).collect()),
        _ => None,
    }
}

/// CurseForge hides projects whose authors opted out: status 9 and redacted name.
pub fn blocked(mod_id: &Value) -> bool {
    let id = match mod_id.as_u64() {
        Some(v) => v,
        None => match mod_id.as_str().and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => return false,
        },
    };
    match curseforge_get(&format!("/mods/{id}"), &[]) {
        Ok(res) => {
            let data = res.get("data").cloned().unwrap_or(Value::Null);
            let status = data.get("status").and_then(|v| v.as_u64()).unwrap_or(0);
            let redacted = data.get("name").and_then(|v| v.as_str())
                .map(|n| n == format!("project-{id}"))
                .unwrap_or(false);
            status == 9 || redacted
        }
        Err(_) => false,
    }
}

pub fn no_versions_message(title: &str) -> String {
    format!(
        "No versions for '{title}' are available. The author likely blocked third-party launchers — update manually from the CurseForge website."
    )
}

/// Fetch the newest files for one project and filter by loader.
pub fn files(mod_id: &Value, mcver: &str, loader: &str, limit: usize) -> Result<Vec<Value>, AppError> {
    let id = mod_id.as_u64().map(|v| v.to_string()).unwrap_or_else(|| mod_id.as_str().unwrap_or("").to_string());
    let mut params: Vec<(&str, String)> = vec![
        ("pageSize", limit.min(50).to_string()),
        ("sortField", "1".into()),
        ("sortOrder", "desc".into()),
    ];
    if !mcver.is_empty() { params.push(("gameVersion", mcver.to_string())); }

    let data = curseforge_get(&format!("/mods/{id}/files"), &params)?
        .get("data").cloned().unwrap_or_else(|| json!([]));
    let mut out: Vec<Value> = data.as_array().cloned().unwrap_or_default();

    if let Some(token) = loader_token(loader) {
        let filtered: Vec<Value> = out.iter().filter(|d| {
            d.get("gameVersions").and_then(|v| v.as_array()).map(|versions| {
                versions.iter().any(|g| {
                    g.as_str().map(|s| s.to_lowercase()).unwrap_or_default() == token
                })
            }).unwrap_or(false)
        }).cloned().collect();
        if !filtered.is_empty() { out = filtered; }
    }
    Ok(out)
}

/// Search CurseForge with loader/version filters.
pub fn search(
    query: &str,
    instance_name: &str,
    limit: usize,
    mc_override: &str,
    loader_override: &str,
) -> Result<Vec<Value>, AppError> {
    let (loader, mcver) = apply_loader_mc_override(
        instance_loader_mc(instance_name)?,
        mc_override,
        loader_override,
    );

    let mut params: Vec<(&str, String)> = vec![
        ("gameId", "432".into()),
        ("searchFilter", query.to_string()),
        ("pageSize", limit.min(50).to_string()),
        ("sortField", "1".into()),
        ("sortOrder", "desc".into()),
    ];
    if !mcver.is_empty() { params.push(("gameVersion", mcver.clone())); }

    let mod_loader = match loader.as_str() {
        "fabric" => Some("Fabric"),
        "quilt" => Some("Quilt"),
        "neoforge" => Some("NeoForge"),
        "forge" => Some("Forge"),
        "paper" => Some("Paper"),
        _ => None,
    };
    if let Some(ml) = mod_loader {
        if !mcver.is_empty() { params.push(("modLoaderType", ml.to_string())); }
    }

    let results: Vec<Value> = if query.trim().chars().all(|c| c.is_ascii_digit()) && !query.trim().is_empty() {
        let data = curseforge_get(&format!("/mods/{}", query.trim()), &[])?
            .get("data").cloned().unwrap_or(Value::Null);
        if data.is_object() { vec![data] } else { vec![] }
    } else {
        let tokens: Vec<String> = NON_WORD_PATTERN.split(query.to_lowercase().as_str())
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .collect();
        let mut out: Vec<Value> = Vec::new();
        let mut seen: HashSet<u64> = HashSet::new();

        for page in 0..3 {
            let mut page_params = params.clone();
            for p in page_params.iter_mut() {
                if p.0 == "pageSize" { p.1 = "50".into(); }
            }
            page_params.push(("index", (page * 50).to_string()));

            let batch = curseforge_get("/mods/search", &page_params)?
                .get("data").cloned().unwrap_or_else(|| json!([]));
            let batch: Vec<Value> = batch.as_array().cloned().unwrap_or_default();
            if batch.is_empty() { break; }

            out.extend(batch.iter().filter(|p| {
                let id = p["id"].as_u64().unwrap_or(0);
                let hit = !tokens.is_empty() && tokens.iter().all(|t| {
                    p.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase().contains(t)
                        || p.get("slug").and_then(|v| v.as_str()).unwrap_or("").to_lowercase().contains(t)
                });
                !seen.contains(&id) && hit
            }).cloned());

            seen.extend(batch.iter().map(|p| p["id"].as_u64().unwrap_or(0)));
            if out.len() >= limit { break; }
        }
        out
    };

    let mut out = Vec::new();
    let wanted_categories = loader_categories(&loader);
    for project in results {
        let categories = category_names(&project);
        if !categories.is_empty() {
            if let Some(wanted) = &wanted_categories {
                if categories.is_disjoint(wanted) { continue; }
            }
        }

        let summary = HTML_TAG_PATTERN.replace_all(
            project.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
            "",
        );
        let icon = project.get("logo").and_then(|l| l.get("thumbnailUrl")).and_then(|v| v.as_str())
            .or_else(|| project.get("logo").and_then(|l| l.get("url")).and_then(|v| v.as_str()))
            .unwrap_or("");

        out.push(json!({
            "project_id": project["id"],
            "slug": project.get("slug"),
            "title": project.get("name"),
            "icon": icon,
            "downloads": project.get("popularity"),
            "desc": summary.chars().take(160).collect::<String>(),
            "author": project.get("authors").and_then(|a| a.get(0)).and_then(|a| a.get("name")).cloned()
                .unwrap_or(Value::String(String::new())),
        }));
    }
    Ok(out)
}

/// Resolve a project's website URL, cached in `urlcache.json`.
pub fn mod_url(mod_id: &Value) -> Option<String> {
    let pid = mod_id.as_u64().map(|v| v.to_string())
        .unwrap_or_else(|| mod_id.as_str().unwrap_or("").to_string());
    if pid.is_empty() { return None; }

    let cache_path = APP.state_dir.join("urlcache.json");
    let cache = read_json(&cache_path);
    if let Some(u) = cache.get(format!("cf:{pid}")).and_then(|v| v.as_str()) {
        if !u.is_empty() { return Some(u.to_string()); }
    }

    let data = curseforge_get(&format!("/mods/{pid}"), &[]).ok()?;
    let project = data.get("data")?;
    let url = project.get("links").and_then(|l| l.get("websiteUrl"))
        .and_then(|v| v.as_str()).map(|s| s.to_string())?;

    let mut refreshed = read_json(&cache_path);
    refreshed[format!("cf:{pid}")] = json!(url);
    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(cache_path, refreshed.to_string());
    Some(url)
}

/// Download and install one CurseForge project, optionally with dependencies.
pub fn install(
    instance_name: &str,
    mod_id: &Value,
    with_deps: bool,
    mc_override: &str,
    loader_override: &str,
    title: &str,
    batch_key: Option<&str>,
) -> Result<Value, AppError> {
    let inst = instance_dir(instance_name)?;
    let jar_dir = mod_dir(&inst)
        .ok_or_else(|| AppError::BadRequest("instance has no mods or plugins dir".into()))?;

    let pid = mod_id.as_u64().map(|v| v.to_string())
        .unwrap_or_else(|| mod_id.as_str().unwrap_or("?").to_string());
    let label = if title.is_empty() { format!("CurseForge project {pid}") } else { title.to_string() };

    let own_key = if batch_key.is_none() {
        Some(progress::start_job("install", instance_name, &label, 0, false))
    } else {
        None
    };
    let key: Option<&str> = batch_key.or(own_key.as_deref());
    let own = own_key.is_some();

    if let Some(k) = key { progress::set_phase(k, "resolving"); }

    let (loader, mcver) = apply_loader_mc_override(
        instance_loader_mc(instance_name)?,
        mc_override,
        loader_override,
    );
    let available = files(mod_id, &mcver, &loader, 25)?;
    if available.is_empty() {
        if blocked(mod_id) {
            let msg = no_versions_message(&format!("CurseForge project {pid}"));
            if own { if let Some(k) = key { progress::finish_job(k, "error", Some(msg.clone())); } }
            return Err(AppError::BadRequest(msg));
        }
        let msg = "tidak ada file untuk loader/MC version ini".to_string();
        if own { if let Some(k) = key { progress::finish_job(k, "error", Some(msg.clone())); } }
        return Err(AppError::BadRequest(msg));
    }

    let chosen = available.iter().find(|f| {
        !jar_dir.join(f["fileName"].as_str().unwrap_or("")).exists()
    });
    let chosen = match chosen {
        Some(c) => c,
        None => {
            let msg = format!("FileExistsError: {}", available[0]["fileName"]);
            if own { if let Some(k) = key { progress::finish_job(k, "error", Some(msg.clone())); } }
            return Err(AppError::BadRequest(msg));
        }
    };

    let dest = jar_dir.join(chosen["fileName"].as_str().unwrap());
    download::download_file(&download_url(chosen), &dest, &curseforge_headers()?, key)?;
    if let Some(k) = key {
        progress::finish_file(k);
        progress::set_phase(k, "installing");
    }

    let fname = dest.file_name().unwrap().to_string_lossy().to_string();
    let installed_version = {
        let (_, v) = mods::parse_mod_name(chosen["fileName"].as_str().unwrap_or(""));
        if !v.is_empty() {
            v
        } else {
            chosen.get("displayName").and_then(|x| x.as_str()).unwrap_or("").to_string()
        }
    };
    let project_title = match curseforge_get(&format!("/mods/{pid}"), &[]) {
        Ok(res) => res.get("data").and_then(|d| d.get("name")).and_then(|v| v.as_str()).unwrap_or(title).to_string(),
        Err(_) => title.to_string(),
    };
    if let Some(url) = mod_url(mod_id) {
        links::save_link(instance_name, &fname, &url, "curseforge", Some(&pid), Some(&project_title), Some(&installed_version));
    }

    let mut out = json!({ "file": fname });
    if with_deps {
        if let Some(k) = key { progress::set_phase(k, "dependencies"); }
        match dependencies::install_curseforge_dependencies(
            &inst,
            chosen,
            &loader,
            &mcver,
            &fname,
            key,
        ) {
            Ok(d) => out["deps"] = json!(d),
            Err(e) => out["deps_error"] = json!(e.to_string()),
        }
        if let Some(k) = key { progress::set_phase(k, "installing"); }
    }

    if own { if let Some(k) = key { progress::finish_job(k, "done", None); } }
    Ok(out)
}

/// Match unknown jars to CurseForge via fingerprints and fill update items.
pub fn match_unknown(
    inst: &Path,
    mods: &[Value],
    items: &mut Map<String, Value>,
    hash_cache: &mut Value,
    loader: &str,
    mcver: &str,
    instance_name: &str,
) {
    let unknown: Vec<&Value> = mods.iter().filter(|m| {
        items.get(m["file"].as_str().unwrap())
            .map(|it| it["known"] != json!(true))
            .unwrap_or(false)
    }).collect();
    if unknown.is_empty() { return; }

    let mut fingerprints: HashMap<String, u32> = HashMap::new();
    for m in unknown.iter() {
        let path = mods::mod_file_path(inst, m["file"].as_str().unwrap())
            .unwrap_or_else(|_| inst.join("mods").join(m["file"].as_str().unwrap()));
        if let Ok(fp) = hashing::cached_fingerprint(&path, hash_cache) {
            fingerprints.insert(m["file"].as_str().unwrap().to_string(), fp);
        }
    }
    hashing::save_hash_cache(hash_cache);

    let mut by_fingerprint: HashMap<u32, (Value, Option<String>)> = HashMap::new();
    let mut values: Vec<u32> = fingerprints.values().cloned().collect();
    values.sort();
    values.dedup();

    for chunk in values.chunks(100) {
        match curseforge_post("/fingerprints", &json!({ "fingerprints": chunk })) {
            Ok(res) => {
                let data = res.get("data").cloned().unwrap_or_else(|| json!({}));
                let fp_list = data.get("exactFingerprints").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let matches = data.get("exactMatches").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                for (fp_value, entry) in fp_list.iter().zip(matches.iter()) {
                    let id = entry.get("id").cloned()
                        .or_else(|| entry.get("file").and_then(|f| f.get("modId")).cloned())
                        .unwrap_or(Value::Null);
                    let display = entry.get("file").and_then(|f| f.get("displayName"))
                        .and_then(|v| v.as_str()).map(|s| s.to_string());
                    if let Some(fp) = fp_value.as_u64() {
                        by_fingerprint.insert(fp as u32, (id, display));
                    }
                }
            }
            Err(e) => {
                let mut updates = APP.updates.lock().unwrap();
                if let Some(status) = updates.get_mut(instance_name) {
                    status["error"] = json!(format!("curseforge: {e}"));
                }
            }
        }
    }

    for m in unknown.iter() {
        let fname = m["file"].as_str().unwrap();
        let fp = match fingerprints.get(fname) { Some(f) => *f, None => continue };
        let (mod_id, display) = match by_fingerprint.get(&fp) {
            Some(v) => (v.0.clone(), v.1.clone()),
            None => continue,
        };
        let item = match items.get_mut(fname) { Some(it) => it, None => continue };

        item["known"] = json!(true);
        item["source"] = json!("curseforge");
        item["project"] = mod_id.clone();
        item["installed_name"] = display.map(Value::String).unwrap_or(Value::Null);
        if let Some(url) = mod_url(&mod_id) { item["url"] = json!(url); }

        let available = files(&mod_id, mcver, loader, 25).unwrap_or_default();
        let latest = match available.first() {
            Some(l) => l,
            None => {
                if blocked(&mod_id) { item["blocked"] = json!(true); }
                continue;
            }
        };

        let latest_version = {
            let (_, v) = mods::parse_mod_name(latest["fileName"].as_str().unwrap_or(""));
            if !v.is_empty() { v } else {
                latest.get("displayName").and_then(|v| v.as_str()).unwrap_or("").to_string()
            }
        };
        item["latest"] = json!(latest_version);
        item["latest_url"] = json!(download_url(latest));
        item["latest_filename"] = latest["fileName"].clone();

        let (_, installed_version) = mods::parse_mod_name(fname);
        let installed = if !installed_version.is_empty() {
            installed_version
        } else {
            item["installed"].as_str().unwrap_or("").to_string()
        };
        if !latest_version.is_empty() && latest_version != installed {
            item["update"] = json!(latest_version);
        }
    }
}
