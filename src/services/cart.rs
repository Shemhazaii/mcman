//! Per-instance mod shopping cart.

use std::fs;
use std::path::PathBuf;
use serde_json::{json, Value};

use crate::core::errors::AppError;
use crate::services::icons;
use crate::core::instances::instance_dir;
use crate::core::progress;
use crate::providers::http::{curseforge_post, modrinth_get};
use crate::providers::{curseforge as cf, modrinth as mr};
use crate::core::state::{APP, read_json};

fn cart_path(name: &str) -> PathBuf {
    let root = APP.root_string().unwrap_or_default();
    let mut hasher = sha1::Sha1::new();
    use sha1::Digest;
    hasher.update(root.as_bytes());
    let hex: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    let sanitized = name.replace(|c: char| !(c.is_alphanumeric() || c == '.' || c == '-' || c == '_'), "_");
    APP.state_dir.join(format!("cart-{}-{}.json", &hex[..8], sanitized))
}

fn read_cart(name: &str) -> Value {
    let v = read_json(&cart_path(name));
    if v.is_object() { v } else { json!({ "items": [] }) }
}

fn write_cart(name: &str, value: &Value) {
    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(cart_path(name), value.to_string());
}

fn project_id_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// List cart items, resolving missing icons from the cache/provider.
pub fn list_items(instance_name: &str) -> Result<Vec<Value>, AppError> {
    instance_dir(instance_name)?;
    let stored = read_cart(instance_name);
    let mut items = stored.get("items").and_then(|a| a.as_array()).cloned().unwrap_or_default();
    let mut cache = icons::icons_cache();

    for item in items.iter_mut() {
        let source = item.get("source").and_then(|x| x.as_str()).unwrap_or("");
        let pid = project_id_string(item.get("project_id").unwrap_or(&Value::Null));
        if pid.is_empty() { continue; }

        let key = format!("{source}:{pid}");
        let mut icon = cache.get(&key).and_then(|x| x.as_str()).unwrap_or("").to_string();

        if icon.is_empty() {
            if source == "modrinth" {
                if let Ok(p) = modrinth_get(&format!("/project/{pid}"), &[]) {
                    icon = p.get("icon_url").and_then(|x| x.as_str()).unwrap_or("").to_string();
                }
            } else if source == "curseforge" {
                if let Ok(n) = pid.parse::<u64>() {
                    if let Ok(v) = curseforge_post("/mods", &json!({ "modIds": [n] })) {
                        if let Some(m) = v.get("data").and_then(|x| x.as_array()).and_then(|a| a.first()) {
                            icon = m.get("logo").and_then(|l| l.get("thumbnailUrl")).and_then(|x| x.as_str())
                                .or_else(|| m.get("logo").and_then(|l| l.get("url")).and_then(|x| x.as_str()))
                                .unwrap_or("").to_string();
                        }
                    }
                }
            }
            if !icon.is_empty() { cache[key.clone()] = json!(icon.clone()); }
        }

        if !icon.is_empty() { item["icon"] = json!(icon); }
    }

    let _ = fs::write(APP.state_dir.join("icons.json"), cache.to_string());
    Ok(items)
}

pub fn add_item(instance_name: &str, item: Value) -> Result<Value, AppError> {
    instance_dir(instance_name)?;
    let source = item.get("source").and_then(|v| v.as_str()).unwrap_or("");
    if source != "modrinth" && source != "curseforge" {
        return Err(AppError::BadRequest("source harus modrinth atau curseforge".into()));
    }

    let pid = project_id_string(item.get("project_id").unwrap_or(&Value::Null));
    if pid.is_empty() { return Err(AppError::BadRequest("project_id kosong".into())); }

    let mut cart = read_cart(instance_name);
    let mut items: Vec<Value> = cart.get("items").and_then(|a| a.as_array()).cloned().unwrap_or_default();

    if items.iter().any(|x| {
        x.get("source").and_then(|v| v.as_str()).unwrap_or("") == source
            && project_id_string(x.get("project_id").unwrap_or(&Value::Null)) == pid
    }) {
        let existing = items.iter().find(|x| {
            x.get("source").and_then(|v| v.as_str()).unwrap_or("") == source
                && project_id_string(x.get("project_id").unwrap_or(&Value::Null)) == pid
        }).cloned().unwrap_or(Value::Null);
        return Ok(json!({ "duplicate": true, "item": existing }));
    }

    let normalized = json!({
        "source": source,
        "project_id": pid,
        "slug": item.get("slug").and_then(|v| v.as_str()).unwrap_or(""),
        "title": item.get("title").and_then(|v| v.as_str()).unwrap_or(""),
        "desc": item.get("desc").and_then(|v| v.as_str()).unwrap_or(""),
        "author": item.get("author").and_then(|v| v.as_str()).unwrap_or(""),
        "downloads": item.get("downloads").cloned().unwrap_or(Value::Null),
        "icon": item.get("icon").and_then(|v| v.as_str()).unwrap_or(""),
        "mc": item.get("mc").and_then(|v| v.as_str()).unwrap_or(""),
        "loader": item.get("loader").and_then(|v| v.as_str()).unwrap_or(""),
        "deps": item.get("deps").and_then(|v| v.as_bool()).unwrap_or(true),
    });

    items.push(normalized.clone());
    cart["items"] = json!(items);
    write_cart(instance_name, &cart);
    Ok(json!({ "duplicate": false, "item": normalized }))
}

pub fn remove_item(instance_name: &str, source: &str, project_id: &str) -> Result<Value, AppError> {
    instance_dir(instance_name)?;
    let mut cart = read_cart(instance_name);
    let mut items: Vec<Value> = cart.get("items").and_then(|a| a.as_array()).cloned().unwrap_or_default();
    let before = items.len();

    items.retain(|x| !(x.get("source").and_then(|v| v.as_str()).unwrap_or("") == source
        && project_id_string(x.get("project_id").unwrap_or(&Value::Null)) == project_id));

    cart["items"] = json!(items);
    write_cart(instance_name, &cart);
    Ok(json!({ "removed": before - items.len(), "remaining": items.len() }))
}

pub fn clear(instance_name: &str) -> Result<Value, AppError> {
    instance_dir(instance_name)?;
    write_cart(instance_name, &json!({ "items": [] }));
    Ok(json!({ "remaining": 0 }))
}

pub fn install_all(instance_name: &str) -> Result<Value, AppError> {
    let items = list_items(instance_name)?;
    if items.is_empty() { return Err(AppError::BadRequest("keranjang kosong".into())); }

    let key = progress::start_job(
        "install",
        instance_name,
        &format!("keranjang — {} mod", items.len()),
        0,
        false,
    );
    progress::set_phase(&key, "preparing");

    let mut installed: Vec<Value> = Vec::new();
    let mut errors: Vec<Value> = Vec::new();

    for item in items.iter() {
        let source = item.get("source").and_then(|v| v.as_str()).unwrap_or("modrinth");
        let pid = project_id_string(item.get("project_id").unwrap_or(&Value::Null));
        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let deps = item.get("deps").and_then(|v| v.as_bool()).unwrap_or(true);
        let mc = item.get("mc").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let loader = item.get("loader").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let result = if source == "curseforge" {
            cf::install(instance_name, &json!(pid), deps, &mc, &loader, &title, Some(&key))
        } else {
            mr::install(instance_name, &json!(pid), deps, &mc, &loader, &title, Some(&key))
        };

        match result {
            Ok(v) => installed.push(json!({
                "source": source, "project_id": pid, "title": title, "result": v
            })),
            Err(e) => errors.push(json!({
                "source": source, "project_id": pid, "title": title, "error": e.to_string()
            })),
        }
    }

    if installed.is_empty() {
        progress::finish_job(&key, "error", Some("semua mod gagal diunduh".into()));
    } else {
        progress::finish_job(&key, "done", None);
    }

    let done_keys: Vec<(String, String)> = installed.iter().map(|x| (
        x.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        project_id_string(x.get("project_id").unwrap_or(&Value::Null)),
    )).collect();

    let mut cart = read_cart(instance_name);
    let mut remaining: Vec<Value> = cart.get("items").and_then(|a| a.as_array()).cloned().unwrap_or_default();
    remaining.retain(|x| {
        let s = x.get("source").and_then(|v| v.as_str()).unwrap_or("");
        let p = project_id_string(x.get("project_id").unwrap_or(&Value::Null));
        !done_keys.iter().any(|(ds, dp)| ds == s && *dp == p)
    });
    cart["items"] = json!(remaining);
    write_cart(instance_name, &cart);

    Ok(json!({
        "installed": installed,
        "errors": errors,
        "total": items.len(),
        "cart_remaining": remaining.len(),
    }))
}
