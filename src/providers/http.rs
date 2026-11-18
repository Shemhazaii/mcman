//! Shared HTTP plumbing for Modrinth and CurseForge.

use std::io::Read;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use reqwest::blocking::{Client, Response};
use serde_json::Value;

use crate::core::errors::AppError;
use crate::core::state::APP;

pub const MODRINTH_API: &str = "https://api.modrinth.com/v2";
pub const CURSEFORGE_API: &str = "https://api.curseforge.com/v1";
pub const USER_AGENT: &str = "mcman/0.2 (local minecraft server mod manager)";

/// Shared blocking client with a generous timeout for large jar downloads.
pub static CLIENT: LazyLock<Client> =
    LazyLock::new(|| Client::builder().timeout(Duration::from_secs(120)).build().unwrap());

static LAST_MODRINTH_CALL: LazyLock<Mutex<Instant>> =
    LazyLock::new(|| Mutex::new(Instant::now() - Duration::from_secs(5)));

/// Keep Modrinth calls at least 250ms apart to avoid rate limiting.
fn throttle_modrinth() {
    let mut last = LAST_MODRINTH_CALL.lock().unwrap();
    let wait = Duration::from_millis(250).checked_sub(last.elapsed()).unwrap_or_default();
    if !wait.is_zero() { std::thread::sleep(wait); }
    *last = Instant::now();
}

/// Send a request and decode a JSON response, with provider-aware error text.
fn send_request(request: reqwest::blocking::RequestBuilder, is_curseforge: bool) -> Result<Value, AppError> {
    let mut response: Response = request.send().map_err(|e| AppError::Internal(e.to_string()))?;
    let status = response.status();

    if is_curseforge
        && (status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED)
    {
        let body = response.text().unwrap_or_default();
        return Err(AppError::BadRequest(format!(
            "CurseForge menolak request (HTTP {}) — key salah/kedaluwarsa: {}",
            status.as_u16(),
            body
        )));
    }
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(AppError::Internal(format!(
            "HTTP {}: {}",
            status.as_u16(),
            &body[..body.len().min(200)]
        )));
    }

    let mut buf = Vec::new();
    response.read_to_end(&mut buf).map_err(|e| AppError::Internal(e.to_string()))?;
    serde_json::from_slice(&buf).map_err(|e| AppError::Internal(format!("bad json: {e}")))
}

pub fn modrinth_get(path: &str, params: &[(&str, String)]) -> Result<Value, AppError> {
    throttle_modrinth();
    let mut request = CLIENT.get(format!("{MODRINTH_API}{path}")).header("User-Agent", USER_AGENT);
    if !params.is_empty() { request = request.query(params); }
    send_request(request, false)
}

pub fn modrinth_post(path: &str, body: &Value) -> Result<Value, AppError> {
    throttle_modrinth();
    send_request(
        CLIENT.post(format!("{MODRINTH_API}{path}")).header("User-Agent", USER_AGENT).json(body),
        false,
    )
}

pub fn curseforge_headers() -> Result<Vec<(&'static str, String)>, AppError> {
    let key = APP.curseforge_key().ok_or_else(|| AppError::BadRequest(
        "CurseForge API key belum diset — buat gratis di console.curseforge.com (API keys), lalu klik \"CurseForge key\" di header".into()
    ))?;
    Ok(vec![("x-api-key", key)])
}

pub fn curseforge_get(path: &str, params: &[(&str, String)]) -> Result<Value, AppError> {
    let mut request = CLIENT.get(format!("{CURSEFORGE_API}{path}")).header("User-Agent", USER_AGENT);
    for (k, v) in curseforge_headers()? { request = request.header(k, v); }
    if !params.is_empty() { request = request.query(params); }
    send_request(request, true)
}

pub fn curseforge_post(path: &str, body: &Value) -> Result<Value, AppError> {
    let mut request = CLIENT.post(format!("{CURSEFORGE_API}{path}")).header("User-Agent", USER_AGENT).json(body);
    for (k, v) in curseforge_headers()? { request = request.header(k, v); }
    send_request(request, true)
}
