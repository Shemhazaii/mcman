//! HTTP surface. Route paths and response shapes are preserved exactly.

use axum::extract::{Path as AxPath, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::services::cart;
use crate::services::config_files;
use crate::ui::desktop;
use crate::core::errors::{AppError, GetError, PostError};
use crate::core::instances;
use crate::core::mods;
use crate::providers::{curseforge, modrinth};
use crate::core::state::APP;
use crate::services::trash;
use crate::services::updates;
use crate::services::versions;

const PAGE: &str = include_str!("../../static/index.html");

pub fn router() -> Router {
    Router::new()
        .route("/", get(page_handler))
        .route("/_next/{*path}", get(next_asset_handler))
        .route("/api/instances", get(instances_handler))
        .route("/api/instances/{name}", post(override_handler))
        .route("/api/instances/{name}/mods", get(mods_handler))
        .route("/api/instances/{name}/trash", get(trash_list_handler))
        .route("/api/updates/{name}", get(updates_handler))
        .route("/api/updates/{name}/cached", get(updates_cached_handler))
        .route("/api/update/all/{name}", get(update_all_status_handler))
        .route("/api/mod/versions", get(versions_handler))
        .route("/api/modrinth/search", get(search_handler))
        .route("/api/search", get(search_handler))
        .route("/api/sources", get(sources_handler))
        .route("/api/open", get(open_handler))
        .route("/api/config/root", get(root_get_handler).post(root_set_handler))
        .route("/api/root/pick", post(root_pick_handler))
        .route("/api/browse", get(browse_handler))
        .route("/api/diff", get(diff_handler))
        .route("/api/mods/toggle", post(toggle_handler))
        .route("/api/mods/trash", post(trash_handler))
        .route("/api/mods/trash_selected", post(trash_selected_handler))
        .route("/api/mods/restore", post(restore_handler))
        .route("/api/mods/purge", post(purge_handler))
        .route("/api/mods/purge_all", post(purge_all_handler))
        .route("/api/config/orphans", get(orphans_handler))
        .route("/api/config/orphans/trash", post(orphans_trash_handler))
        .route("/api/update/apply", post(apply_handler))
        .route("/api/update/all", post(update_all_handler))
        .route("/api/update/selected", post(update_selected_handler))
        .route("/api/modrinth/install", post(install_handler))
        .route("/api/mods/install", post(install_handler))
        .route("/api/config/curseforge", post(curseforge_key_handler))
        .route("/api/config/open", get(config_open_handler))
        .route("/api/config/folder", get(config_folder_handler))
        .route("/api/progress/{name}", get(progress_handler))
        .route("/api/cart/{name}", get(cart_handler))
        .route("/api/cart/{name}/add", post(cart_add_handler))
        .route("/api/cart/{name}/remove", post(cart_remove_handler))
        .route("/api/cart/{name}/clear", post(cart_clear_handler))
        .route("/api/cart/{name}/install_all", post(cart_install_all_handler))
}

async fn run_blocking(
    f: impl FnOnce() -> Result<Value, GetError> + Send + 'static,
) -> Result<Json<Value>, GetError> {
    let result = tokio::task::spawn_blocking(f).await
        .map_err(|_| GetError(AppError::Internal("worker panic".into())))??;
    Ok(Json(result))
}

async fn run_blocking_post(
    f: impl FnOnce() -> Result<Value, PostError> + Send + 'static,
) -> Result<Json<Value>, PostError> {
    let result = tokio::task::spawn_blocking(f).await
        .map_err(|_| PostError(AppError::Internal("worker panic".into())))??;
    Ok(Json(result))
}

async fn page_handler() -> Response {
    ([("content-type", "text/html; charset=utf-8")], PAGE).into_response()
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_lowercase().as_str() {
        "js" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "map" => "application/json; charset=utf-8",
        "woff2" => "font/woff2",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// Serve Next.js static-export assets referenced by index.html as `/_next/...`.
async fn next_asset_handler(AxPath(path): AxPath<String>) -> Response {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static").join("_next");
    let base_canon = match std::fs::canonicalize(&base) {
        Ok(p) => p,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let candidate = base.join(&path);
    let canon = match std::fs::canonicalize(&candidate) {
        Ok(p) => p,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if !canon.starts_with(&base_canon) || !canon.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let data = match tokio::fs::read(&canon).await {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    ([("content-type", content_type(&canon))], data).into_response()
}

async fn instances_handler() -> Result<Json<Value>, GetError> {
    run_blocking(|| Ok::<_, GetError>(Value::Array(instances::list_instances()))).await
}

async fn mods_handler(AxPath(name): AxPath<String>) -> Result<Json<Value>, GetError> {
    run_blocking(move || {
        let mut rows = mods::get_mod_rows(&name).map_err(GetError)?;
        crate::core::links::attach_urls(&name, &mut rows);
        crate::services::icons::attach_icons(&name, &mut rows);
        config_files::attach_configs(&name, &mut rows);
        Ok::<_, GetError>(Value::Array(rows))
    }).await
}

async fn trash_list_handler(AxPath(name): AxPath<String>) -> Result<Json<Value>, GetError> {
    run_blocking(move || trash::list_trash(&name).map(Value::Array).map_err(GetError)).await
}

async fn updates_handler(
    AxPath(name): AxPath<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, GetError> {
    let force = query.contains_key("force");
    run_blocking(move || Ok::<_, GetError>(updates::check_updates(&name, force))).await
}

/// Cached update state without starting a new hash scan.
async fn updates_cached_handler(AxPath(name): AxPath<String>) -> Json<Value> {
    Json(updates::update_status(&name))
}

async fn update_all_status_handler(AxPath(name): AxPath<String>) -> Json<Value> {
    Json(updates::update_all_status(&name))
}

/// Live download progress for the instance: install and update jobs.
async fn progress_handler(AxPath(name): AxPath<String>) -> Json<Value> {
    let progress = APP.progress.lock().unwrap();
    let install = progress.get(&format!("install:{name}")).cloned();
    let update = progress.get(&format!("update:{name}")).cloned();
    Json(json!({ "install": install, "update": update }))
}

async fn versions_handler(Query(query): Query<HashMap<String, String>>) -> Result<Json<Value>, GetError> {
    let instance = query.get("instance").cloned().unwrap_or_default();
    let file = query.get("file").cloned().unwrap_or_default();
    let mc = query.get("mc").cloned().unwrap_or_default();
    let loader = query.get("loader").cloned().unwrap_or_default();
    run_blocking(move || {
        versions::mod_versions(&instance, &file, &mc, &loader)
            .map(Value::Array)
            .map_err(GetError)
    }).await
}

async fn search_handler(Query(query): Query<HashMap<String, String>>) -> Result<Json<Value>, GetError> {
    let search_query = query.get("q").cloned().unwrap_or_default();
    let instance = query.get("instance").cloned().unwrap_or_default();
    let source = query.get("source").cloned().unwrap_or_else(|| "modrinth".into());
    let limit: usize = query.get("limit").and_then(|v| v.parse().ok()).unwrap_or(20);
    let mc = query.get("mc").cloned().unwrap_or_default();
    let loader = query.get("loader").cloned().unwrap_or_default();

    run_blocking(move || {
        let result = if source == "curseforge" {
            curseforge::search(&search_query, &instance, limit, &mc, &loader)
        } else {
            modrinth::search(&search_query, &instance, limit, &mc, &loader)
        };
        result.map(Value::Array).map_err(GetError)
    }).await
}

async fn sources_handler() -> Json<Value> {
    Json(json!({
        "modrinth": true,
        "curseforge": APP.curseforge_key().is_some(),
    }))
}

/// Open a mod page in the host browser.
async fn open_handler(Query(query): Query<HashMap<String, String>>) -> Result<Json<Value>, GetError> {
    let url = query.get("url").cloned().unwrap_or_default();
    let allowed = url.starts_with("https://modrinth.com/")
        || url.starts_with("https://www.curseforge.com/");
    if !allowed {
        return Err(GetError(AppError::BadRequest(
            "url not allowed — only modrinth.com/curseforge.com".into()
        )));
    }

    run_blocking(move || {
        match desktop::open_external(&url) {
            Ok(()) => Ok(json!({ "opened": url })),
            Err(e) => Err(GetError(AppError::BadRequest(e))),
        }
    }).await
}

async fn config_open_handler(Query(query): Query<HashMap<String, String>>) -> Result<Json<Value>, GetError> {
    let instance = query.get("instance").cloned().unwrap_or_default();
    let file = query.get("file").cloned().unwrap_or_default();
    run_blocking(move || {
        config_files::open_config_file(&instance, &file)
            .map(|p| json!({ "opened": p }))
            .map_err(GetError)
    }).await
}

async fn config_folder_handler(Query(query): Query<HashMap<String, String>>) -> Result<Json<Value>, GetError> {
    let instance = query.get("instance").cloned().unwrap_or_default();
    let dir = query.get("dir").cloned().unwrap_or_default();
    run_blocking(move || {
        config_files::open_config_dir(&instance, &dir)
            .map(|p| json!({ "opened": p }))
            .map_err(GetError)
    }).await
}

async fn root_get_handler() -> Json<Value> {
    Json(json!({ "root": APP.root_string() }))
}

async fn browse_handler(Query(query): Query<HashMap<String, String>>) -> Result<Json<Value>, GetError> {
    let path = query.get("path").cloned().unwrap_or_default();
    run_blocking(move || instances::browse(&path).map_err(GetError)).await
}

async fn diff_handler(Query(query): Query<HashMap<String, String>>) -> Result<Json<Value>, GetError> {
    let a = query.get("a").cloned().unwrap_or_default();
    let b = query.get("b").cloned().unwrap_or_default();
    run_blocking(move || mods::compare_instances(&a, &b).map_err(GetError)).await
}

fn required_field(body: &Value, key: &str) -> Result<String, PostError> {
    body.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| PostError(AppError::BadRequest(format!("missing field {key}"))))
}

async fn toggle_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let (instance, file) = (required_field(&body, "instance")?, required_field(&body, "file")?);
    run_blocking_post(move || {
        mods::toggle_mod(&instance, &file)
            .map(|f| json!({ "file": f }))
            .map_err(PostError)
    }).await
}

async fn trash_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let (instance, file) = (required_field(&body, "instance")?, required_field(&body, "file")?);
    run_blocking_post(move || {
        let config_count = config_files::owned_files(&instance, &file).len();
        trash::trash_mod(&instance, &file)
            .map(|f| json!({ "file": f, "configs": config_count }))
            .map_err(PostError)
    }).await
}

async fn restore_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let (instance, file) = (required_field(&body, "instance")?, required_field(&body, "file")?);
    let kind = body.get("kind").and_then(|v| v.as_str()).unwrap_or("jar").to_string();
    run_blocking_post(move || {
        trash::restore_mod(&instance, &file, &kind)
            .map(|f| json!({ "file": f, "kind": kind }))
            .map_err(PostError)
    }).await
}

async fn purge_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let (instance, file) = (required_field(&body, "instance")?, required_field(&body, "file")?);
    let kind = body.get("kind").and_then(|v| v.as_str()).unwrap_or("jar").to_string();
    run_blocking_post(move || trash::purge_mod(&instance, &file, &kind).map_err(PostError)).await
}

fn required_files(body: &Value) -> Result<Vec<String>, PostError> {
    let files: Vec<String> = body.get("files")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();

    if files.is_empty() {
        return Err(PostError(AppError::BadRequest("missing field files".into())));
    }
    Ok(files)
}

async fn trash_selected_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let instance = required_field(&body, "instance")?;
    let files = required_files(&body)?;
    run_blocking_post(move || trash::trash_mods(&instance, &files).map_err(PostError)).await
}

async fn purge_all_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let instance = required_field(&body, "instance")?;
    run_blocking_post(move || trash::purge_all(&instance).map_err(PostError)).await
}

async fn update_selected_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let instance = required_field(&body, "instance")?;
    let files = required_files(&body)?;
    run_blocking_post(move || updates::start_update_selected(&instance, &files).map_err(PostError)).await
}

async fn orphans_handler(Query(query): Query<HashMap<String, String>>) -> Result<Json<Value>, GetError> {
    let instance = query.get("instance").cloned().unwrap_or_default();
    run_blocking(move || {
        Ok::<_, GetError>(Value::Array(
            config_files::orphans(&instance).into_iter().map(|s| json!(s)).collect()
        ))
    }).await
}

async fn orphans_trash_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let instance = required_field(&body, "instance")?;
    let files = body.get("files")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect::<Vec<String>>());
    run_blocking_post(move || trash::trash_orphans(&instance, files).map_err(PostError)).await
}

async fn apply_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let instance = required_field(&body, "instance")?;
    let file = required_field(&body, "file")?;
    let mc = body.get("mc").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let loader = body.get("loader").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let version = body.get("version").and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    });

    let key = crate::core::progress::start_job("update", &instance, file.as_str(), 1, true);
    run_blocking_post(move || {
        match versions::apply_update(
            &instance,
            &file,
            version.as_deref(),
            &mc,
            &loader,
            Some(&key),
        ) {
            Ok(new_file) => {
                crate::core::progress::finish_done(&format!("update:{instance}"));
                Ok(json!({ "file": new_file }))
            }
            Err(e) => {
                crate::core::progress::fail_job(&format!("update:{instance}"), e.to_string());
                Err(PostError(e))
            }
        }
    }).await
}

async fn update_all_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let instance = required_field(&body, "instance")?;
    run_blocking_post(move || updates::start_update_all(&instance).map_err(PostError)).await
}

async fn install_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let instance = required_field(&body, "instance")?;
    let project_id = body.get("project_id").cloned().unwrap_or(Value::Null);
    let source = body.get("source").and_then(|v| v.as_str()).unwrap_or("modrinth").to_string();
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let deps = body.get("deps").map(|v| v != &Value::Bool(false)).unwrap_or(true);
    let mc = body.get("mc").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let loader = body.get("loader").and_then(|v| v.as_str()).unwrap_or("").to_string();

    run_blocking_post(move || {
        let result = if source == "curseforge" {
            curseforge::install(&instance, &project_id, deps, &mc, &loader, &title, None)
        } else {
            modrinth::install(&instance, &project_id, deps, &mc, &loader, &title, None)
        };
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                crate::core::progress::fail_job(&format!("install:{instance}"), e.to_string());
                Err(PostError(e))
            }
        }
    }).await
}

async fn cart_handler(AxPath(name): AxPath<String>) -> Result<Json<Value>, GetError> {
    run_blocking(move || {
        cart::list_items(&name)
            .map(|items| json!({ "items": items }))
            .map_err(GetError)
    }).await
}

async fn cart_add_handler(AxPath(name): AxPath<String>, Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    run_blocking_post(move || cart::add_item(&name, body).map_err(PostError)).await
}

async fn cart_remove_handler(
    AxPath(name): AxPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, PostError> {
    let source = required_field(&body, "source")?;
    let project_id = required_field(&body, "project_id")?;
    run_blocking_post(move || cart::remove_item(&name, &source, &project_id).map_err(PostError)).await
}

async fn cart_clear_handler(AxPath(name): AxPath<String>) -> Result<Json<Value>, PostError> {
    run_blocking_post(move || cart::clear(&name).map_err(PostError)).await
}

async fn cart_install_all_handler(AxPath(name): AxPath<String>) -> Result<Json<Value>, PostError> {
    run_blocking_post(move || cart::install_all(&name).map_err(PostError)).await
}

async fn root_set_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let path = required_field(&body, "path")?;
    run_blocking_post(move || {
        instances::set_root(&path)
            .map(|r| json!({ "root": r }))
            .map_err(PostError)
    }).await
}

async fn root_pick_handler(Json(body): Json<Value>) -> Result<Json<Value>, PostError> {
    let start = body.get("start").and_then(|v| v.as_str()).unwrap_or("").to_string();
    run_blocking_post(move || {
        let path = desktop::native_folder_picker(&start).map_err(PostError)?;
        let root = instances::set_root(&path).map_err(PostError)?;
        Ok(json!({ "root": root, "picked": path }))
    }).await
}

async fn curseforge_key_handler(Json(body): Json<Value>) -> Json<Value> {
    let mut config = APP.config();
    let key = body.get("key").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();

    if key.is_empty() {
        config.as_object_mut().unwrap().remove("curseforge_api_key");
    } else {
        config["curseforge_api_key"] = json!(key);
    }
    APP.save_config(&config);
    Json(json!({ "ok": true, "curseforge": APP.curseforge_key().is_some() }))
}

async fn override_handler(AxPath(name): AxPath<String>, Json(body): Json<Value>) -> Json<Value> {
    let loader = body.get("loader").cloned().unwrap_or(Value::Null);
    let mcver = body.get("mcver").cloned().unwrap_or(Value::Null);
    instances::save_override(&name, &loader, &mcver);
    Json(json!({ "ok": true }))
}


