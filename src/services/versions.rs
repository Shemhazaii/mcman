//! Version discovery, identification, and update application.

use std::fs;
use serde_json::{json, Value};

use crate::services::download;
use crate::core::errors::AppError;
use crate::core::hashing;
use crate::core::instances::{instance_dir, instance_loader_mc, mod_dir};
use crate::core::mods::{mod_file_path, parse_mod_name};
use crate::core::progress;
use crate::providers::apply_loader_mc_override;
use crate::providers::curseforge;
use crate::providers::http::{curseforge_post, modrinth_get, modrinth_post};
use crate::core::state::APP;
use crate::services::updates;

fn version_params(loader: &str, mcver: &str) -> Vec<(&'static str, String)> {
    let mut params = Vec::new();
    if !mcver.is_empty() { params.push(("game_versions", json!([mcver]).to_string())); }
    if !loader.is_empty() { params.push(("loaders", json!([loader]).to_string())); }
    params
}

/// Identify one jar by SHA-1 (Modrinth) or fingerprint (CurseForge) and
/// compute its latest compatible version.
pub fn identify_file(instance_name: &str, filename: &str) -> Option<Value> {
    let inst = instance_dir(instance_name).ok()?;
    let (loader, mcver) = instance_loader_mc(instance_name).ok()?;
    let path = mod_file_path(&inst, filename).ok()?;
    let mut hash_cache = hashing::load_hash_cache();

    let mut item = json!({
        "file": filename,
        "known": false,
        "project": null,
        "source": null,
        "installed": null,
        "latest": null,
        "update": null,
        "latest_url": null,
        "latest_filename": null,
    });

    if let Ok(hash) = hashing::cached_sha1(&path, &mut hash_cache) {
        if let Ok(Value::Object(o)) = modrinth_post(
            "/version_files",
            &json!({ "hashes": [hash], "algorithm": "sha1" }),
        ) {
            if let Some(v) = o.values().next() {
                item["known"] = json!(true);
                item["source"] = json!("modrinth");
                item["project"] = v["project_id"].clone();
                item["installed_name"] = v.get("name").cloned().unwrap_or(Value::Null);
                let version_number = v.get("version_number").and_then(|x| x.as_str()).unwrap_or("");
                if !version_number.is_empty() { item["installed"] = json!(version_number); }
            }
        }
    }

    if item["known"] != json!(true) && APP.curseforge_key().is_some() {
        if let Ok(fp) = hashing::cached_fingerprint(&path, &mut hash_cache) {
            if let Ok(res) = curseforge_post("/fingerprints", &json!({ "fingerprints": [fp] })) {
                let data = res.get("data").cloned().unwrap_or_else(|| json!({}));
                let matches = data.get("exactMatches").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                if let Some(entry) = matches.first() {
                    let id = entry.get("id").cloned()
                        .or_else(|| entry.get("file").and_then(|f| f.get("modId")).cloned())
                        .unwrap_or(Value::Null);
                    if id != Value::Null {
                        item["known"] = json!(true);
                        item["source"] = json!("curseforge");
                        item["project"] = id.clone();
                        item["installed_name"] = entry.get("file")
                            .and_then(|f| f.get("displayName")).cloned().unwrap_or(Value::Null);

                        let available = curseforge::files(&id, &mcver, &loader, 25).unwrap_or_default();
                        match available.first() {
                            Some(latest) => {
                                let (_, parsed) = parse_mod_name(latest["fileName"].as_str().unwrap_or(""));
                                let version = if !parsed.is_empty() {
                                    parsed
                                } else {
                                    latest.get("displayName").and_then(|v| v.as_str()).unwrap_or("").to_string()
                                };
                                item["latest"] = json!(version);
                                item["latest_url"] = json!(curseforge::download_url(latest));
                                item["latest_filename"] = latest["fileName"].clone();
                            }
                            None => {
                                if curseforge::blocked(&id) { item["blocked"] = json!(true); }
                            }
                        }
                    }
                }
            }
        }
    }

    if item["known"] == json!(true)
        && item["source"] == json!("modrinth")
        && !loader.is_empty()
    {
        let pid = item["project"].as_str().unwrap_or("").to_string();
        if let Ok(Value::Array(vs)) = modrinth_get(
            &format!("/project/{pid}/version"),
            &version_params(&loader, &mcver),
        ) {
            if let Some(latest) = vs.into_iter().next() {
                item["latest"] = latest["version_number"].clone();
                let files = latest.get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                if let Some(primary) = crate::services::dependencies::pick_primary_file(&files) {
                    item["latest_url"] = primary["url"].clone();
                    item["latest_filename"] = primary["filename"].clone();
                }
            }
        }
    }

    hashing::save_hash_cache(&hash_cache);

    let installed = item.get("installed").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let latest = item.get("latest").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if !latest.is_empty() && latest != installed { item["update"] = json!(latest); }

    if item["known"] == json!(true) && item["project"] != Value::Null {
        Some(item)
    } else {
        None
    }
}

/// List available versions for one mod.
pub fn mod_versions(
    instance_name: &str,
    filename: &str,
    mc_override: &str,
    loader_override: &str,
) -> Result<Vec<Value>, AppError> {
    let (loader, mcver) = apply_loader_mc_override(
        instance_loader_mc(instance_name)?,
        mc_override,
        loader_override,
    );

    let status = APP.updates.lock().unwrap().get(instance_name).cloned().unwrap_or(Value::Null);
    let mut item = status.get("items").and_then(|i| i.get(filename)).cloned();

    if item.as_ref().map(|x| x["known"] != json!(true) || x["project"] == Value::Null).unwrap_or(true) {
        item = identify_file(instance_name, filename).or(item);
    }

    let item = item
        .filter(|x| x["known"] == json!(true) && x["project"] != Value::Null)
        .ok_or_else(|| AppError::BadRequest("mod tidak dikenali di Modrinth/CurseForge".into()))?;

    let mut out = Vec::new();

    if item["source"] == json!("curseforge") {
        if item.get("blocked").and_then(|v| v.as_bool()) == Some(true) {
            let title = item.get("installed_name").and_then(|v| v.as_str()).map(|s| s.to_string())
                .unwrap_or_else(|| filename.trim_end_matches(".jar").trim_end_matches(".jar.disabled").to_string());
            return Err(AppError::BadRequest(curseforge::no_versions_message(&title)));
        }

        let project = item["project"].clone();
        for f in curseforge::files(&project, &mcver, &loader, 50)? {
            if f.get("isAvailable").map(|v| *v == json!(false)).unwrap_or(false) { continue; }

            let release_type = match f.get("releaseType").and_then(|v| v.as_u64()) {
                Some(1) => json!("release"),
                Some(2) => json!("beta"),
                Some(3) => json!("alpha"),
                _ => f.get("releaseType").cloned().unwrap_or(Value::String(String::new())),
            };

            let (parsed, _) = parse_mod_name(f["fileName"].as_str().unwrap_or(""));
            let version = if !parsed.is_empty() {
                parsed
            } else {
                f.get("displayName").and_then(|v| v.as_str()).unwrap_or(f["fileName"].as_str().unwrap_or(""))
                    .to_string()
            };
            let date = f.get("fileDate").and_then(|v| v.as_str()).unwrap_or("")
                .chars().take(10).collect::<String>();

            out.push(json!({
                "id": f["id"].as_u64().map(|v| v.to_string()).unwrap_or_default(),
                "version": version,
                "filename": f["fileName"],
                "date": date,
                "type": release_type,
                "url": curseforge::download_url(&f),
            }));
        }

        if out.is_empty() && curseforge::blocked(&project) {
            let title = item.get("installed_name").and_then(|v| v.as_str()).map(|s| s.to_string())
                .unwrap_or_else(|| filename.trim_end_matches(".jar").trim_end_matches(".jar.disabled").to_string());
            return Err(AppError::BadRequest(curseforge::no_versions_message(&title)));
        }
    } else {
        let project = item["project"].as_str().unwrap_or("").to_string();
        let versions = modrinth_get(&format!("/project/{project}/version"), &version_params(&loader, &mcver))?;
        let versions: Vec<Value> = versions.as_array().cloned().unwrap_or_default();

        for v in versions.into_iter().take(60) {
            let files = v.get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let primary = match crate::services::dependencies::pick_primary_file(&files) {
                Some(p) => p,
                None => continue,
            };
            let date = v.get("date_published").and_then(|x| x.as_str())
                .or_else(|| v.get("date").and_then(|x| x.as_str()))
                .unwrap_or("").chars().take(10).collect::<String>();

            out.push(json!({
                "id": v["id"],
                "version": v.get("version_number").cloned().unwrap_or_else(|| v["name"].clone()),
                "filename": primary["filename"],
                "date": date,
                "type": v.get("version_type").cloned().unwrap_or(Value::String(String::new())),
                "url": primary["url"],
            }));
        }
    }

    Ok(out)
}

/// Apply a specific version or the latest version, then move the old jar to trash.
pub fn apply_update(
    instance_name: &str,
    filename: &str,
    version_id: Option<&str>,
    mc_override: &str,
    loader_override: &str,
    key: Option<&str>,
) -> Result<String, AppError> {
    let status = APP.updates.lock().unwrap().get(instance_name).cloned().unwrap_or(Value::Null);
    let mut item = status.get("items").and_then(|i| i.get(filename)).cloned();

    if item.as_ref().map(|x| x["known"] != json!(true) || x["project"] == Value::Null).unwrap_or(true) {
        item = identify_file(instance_name, filename).or(item);
    }

    let item = item
        .filter(|x| x["known"] == json!(true))
        .ok_or_else(|| AppError::BadRequest(format!("no update info for {filename}")))?;

    let new_version: Option<String>;
    let mut newest: Option<Value> = None;

    let (url, target_name) = if let Some(vid) = version_id {
        let all = mod_versions(instance_name, filename, mc_override, loader_override)?;
        let chosen = all.iter().find(|x| x["id"].as_str().unwrap_or("") == vid)
            .ok_or_else(|| AppError::BadRequest(format!(
                "versi {vid} tidak ditemukan (di luar 50-60 versi terbaru)"
            )))?;
        new_version = chosen.get("version").and_then(|x| x.as_str()).map(String::from);
        newest = all.first().cloned();
        (
            chosen["url"].as_str().unwrap_or("").to_string(),
            chosen["filename"].as_str().unwrap_or("").to_string(),
        )
    } else if item.get("latest_url").and_then(|v| v.as_str()).is_some() {
        new_version = item.get("latest").and_then(|x| x.as_str()).map(String::from);
        (
            item["latest_url"].as_str().unwrap().to_string(),
            item["latest_filename"].as_str().unwrap_or("").to_string(),
        )
    } else {
        return Err(AppError::BadRequest(format!("no update info for {filename}")));
    };

    let inst = instance_dir(instance_name)?;
    let dest = mod_dir(&inst)
        .ok_or_else(|| AppError::BadRequest("instance has no mods or plugins dir".into()))?
        .join(&target_name);
    if dest.exists() {
        return Err(AppError::BadRequest(format!("FileExistsError: {target_name}")));
    }

    let headers = if item["source"] == json!("curseforge") {
        crate::providers::http::curseforge_headers()?
    } else {
        vec![]
    };

    download::download_file(&url, &dest, &headers, key)?;
    progress::finish_file_if_key(key);
    if let Some(k) = key { progress::set_phase(k, "swapping"); }

    let old_path = mod_file_path(&inst, filename)?;
    let tdir = crate::services::trash::trash_dir(instance_name)?;
    fs::create_dir_all(&tdir).map_err(|e| AppError::Internal(e.to_string()))?;
    let old_dest = tdir.join(old_path.file_name().unwrap());
    fs::rename(&old_path, &old_dest)
        .or_else(|_| fs::copy(&old_path, &old_dest).and_then(|_| fs::remove_file(&old_path)))
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if let Some(k) = key { progress::set_phase(k, "cache"); }

    let mut updates = APP.updates.lock().unwrap();
    let entry = updates.entry(instance_name.to_string())
        .or_insert_with(|| json!({ "status": "done", "ts": 0, "items": {} }));

    if let Some(items) = entry.get_mut("items").and_then(|i| i.as_object_mut()) {
        let mut migrated = items.remove(filename).unwrap_or_else(|| json!({
            "file": filename,
            "known": true,
            "installed": item["installed"].clone(),
            "project": item["project"].clone(),
            "source": item["source"].clone(),
            "latest": null,
            "update": null,
            "latest_url": null,
            "latest_filename": null,
        }));

        migrated["file"] = json!(&target_name);
        if let Some(head) = &newest {
            migrated["latest"] = head["version"].clone();
            migrated["latest_filename"] = head["filename"].clone();
            migrated["latest_url"] = head["url"].clone();
        }
        if let Some(v) = &new_version { migrated["installed"] = json!(v); }

        let up_to_date = migrated.get("latest_filename").and_then(|v| v.as_str()) == Some(target_name.as_str())
            || (migrated.get("latest").and_then(|v| v.as_str()).is_some()
                && migrated.get("latest").and_then(|v| v.as_str())
                    == migrated.get("installed").and_then(|v| v.as_str()));
        migrated["update"] = if up_to_date {
            Value::Null
        } else {
            migrated.get("latest").cloned().unwrap_or(Value::Null)
        };

        items.insert(target_name.clone(), migrated);
    }

    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(updates::cache_path(instance_name), entry.to_string());
    Ok(target_name)
}
