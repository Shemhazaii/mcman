//! Modrinth search and installation.

use serde_json::{json, Value};

use crate::services::dependencies;
use crate::services::download;
use crate::core::errors::AppError;
use crate::core::instances::{instance_dir, instance_loader_mc, mod_dir};
use crate::core::links;
use crate::core::progress;
use crate::providers::apply_loader_mc_override;
use crate::providers::http::modrinth_get;

fn version_params(loader: &str, mcver: &str) -> Vec<(&'static str, String)> {
    let mut params = Vec::new();
    if !mcver.is_empty() { params.push(("game_versions", json!([mcver]).to_string())); }
    if !loader.is_empty() { params.push(("loaders", json!([loader]).to_string())); }
    params
}

/// Search Modrinth for mods matching the instance loader/version.
pub fn search(
    query: &str,
    instance_name: &str,
    limit: usize,
    mc_override: &str,
    loader_override: &str,
) -> Result<Vec<Value>, AppError> {
    instance_dir(instance_name)?;
    let (loader, mcver) = apply_loader_mc_override(
        instance_loader_mc(instance_name)?,
        mc_override,
        loader_override,
    );

    let mut facets: Vec<Value> = Vec::new();
    if !mcver.is_empty() { facets.push(json!([format!("versions:{mcver}")])); }
    if matches!(loader.as_str(), "fabric" | "neoforge" | "forge" | "paper") {
        facets.push(json!([format!("categories:{loader}")]));
    }

    let params = vec![
        ("q", query.to_string()),
        ("query", query.to_string()),
        ("limit", limit.to_string()),
        ("facets", json!(facets).to_string()),
    ];

    let response = modrinth_get("/search", &params)?;
    let mut out = Vec::new();
    for hit in response.get("hits").and_then(|v| v.as_array()).cloned().unwrap_or_default() {
        out.push(json!({
            "project_id": hit["project_id"],
            "slug": hit["slug"],
            "title": hit["title"],
            "icon": hit.get("icon_url").and_then(|v| v.as_str()).unwrap_or(""),
            "downloads": hit.get("downloads"),
            "desc": hit.get("description").and_then(|v| v.as_str()).unwrap_or("")
                .chars().take(160).collect::<String>(),
            "author_id": hit.get("author_id"),
        }));
    }
    Ok(out)
}

/// Download the latest compatible version from Modrinth, optionally with deps.
pub fn install(
    instance_name: &str,
    project_id: &serde_json::Value,
    with_deps: bool,
    mc_override: &str,
    loader_override: &str,
    title: &str,
    batch_key: Option<&str>,
) -> Result<Value, AppError> {
    let inst = instance_dir(instance_name)?;
    let jar_dir = mod_dir(&inst)
        .ok_or_else(|| AppError::BadRequest("instance has no mods or plugins dir".into()))?;

    let pid = match project_id {
        Value::String(s) => s.clone(),
        v => v.to_string(),
    };
    let label = if title.is_empty() { format!("Modrinth {pid}") } else { title.to_string() };

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

    let versions_response = modrinth_get(
        &format!("/project/{pid}/version"),
        &version_params(&loader, &mcver),
    )?;
    let versions: Vec<Value> = versions_response.as_array().cloned().unwrap_or_default();
    if versions.is_empty() {
        let msg = "no version found for this loader/mc version".to_string();
        if own { if let Some(k) = key { progress::finish_job(k, "error", Some(msg.clone())); } }
        return Err(AppError::BadRequest(msg));
    }

    let files = versions[0].get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let primary = match dependencies::pick_primary_file(&files) {
        Some(p) => p,
        None => {
            let msg = "no file in latest version".to_string();
            if own { if let Some(k) = key { progress::finish_job(k, "error", Some(msg.clone())); } }
            return Err(AppError::BadRequest(msg));
        }
    };

    let dest = jar_dir.join(primary["filename"].as_str().unwrap());
    if dest.exists() {
        let msg = format!("FileExistsError: {}", dest.file_name().unwrap().to_string_lossy());
        if own { if let Some(k) = key { progress::finish_job(k, "error", Some(msg.clone())); } }
        return Err(AppError::BadRequest(msg));
    }

    download::download_file(primary["url"].as_str().unwrap(), &dest, &[], key)?;
    if let Some(k) = key {
        progress::finish_file(k);
        progress::set_phase(k, "installing");
    }

    let fname = dest.file_name().unwrap().to_string_lossy().to_string();
    let installed_version = versions[0].get("version_number").and_then(|v| v.as_str()).unwrap_or("");
    let project_title = match modrinth_get(&format!("/project/{pid}"), &[]) {
        Ok(v) => v.get("title").and_then(|x| x.as_str()).unwrap_or(title).to_string(),
        Err(_) => title.to_string(),
    };
    links::save_link(
        instance_name,
        &fname,
        &format!("https://modrinth.com/mod/{pid}"),
        "modrinth",
        Some(&pid),
        Some(&project_title),
        Some(installed_version),
    );

    let mut out = json!({ "file": fname });
    if with_deps {
        if let Some(k) = key { progress::set_phase(k, "dependencies"); }
        match dependencies::install_modrinth_dependencies(
            &inst,
            &versions[0],
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
