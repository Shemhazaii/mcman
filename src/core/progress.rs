//! Live download-progress records exposed by `/api/progress/{name}`.

use serde_json::{json, Value};

use crate::core::state::{APP, now_seconds};

/// Create a job record and return its key (`"<kind>:<instance>"`).
pub fn start_job(kind: &str, instance: &str, label: &str, total: u64, fixed_total: bool) -> String {
    let key = format!("{kind}:{instance}");
    APP.progress.lock().unwrap().insert(
        key.clone(),
        json!({
            "status": "running",
            "kind": kind,
            "instance": instance,
            "label": label,
            "phase": "preparing",
            "cur": Value::Null,
            "files_done": 0,
            "files_total": total,
            "bytes_done": 0,
            "bytes_total": Value::Null,
            "fixed_total": fixed_total,
            "error": Value::Null,
            "ts": now_seconds(),
        }),
    );
    key
}

pub fn set_phase(key: &str, phase: &str) {
    let mut progress = APP.progress.lock().unwrap();
    if let Some(entry) = progress.get_mut(key) {
        entry["phase"] = json!(phase);
    }
}

/// Mark the start of one file download.
pub fn begin_file(key: &str, current: &str, total_bytes: Option<u64>) {
    let mut progress = APP.progress.lock().unwrap();
    if !progress.contains_key(key) {
        progress.insert(key.to_string(), json!({
            "status": "running", "kind": "install", "instance": "", "label": "",
            "phase": "preparing", "cur": Value::Null, "files_done": 0, "files_total": 0,
            "bytes_done": 0, "bytes_total": Value::Null, "fixed_total": false,
            "error": Value::Null, "ts": now_seconds(),
        }));
    }
    let entry = progress.get_mut(key).unwrap();
    let fixed = entry.get("fixed_total").and_then(|v| v.as_bool()).unwrap_or(true);
    if !fixed {
        let total = entry.get("files_total").and_then(|v| v.as_u64()).unwrap_or(0);
        entry["files_total"] = json!(total + 1);
    }
    entry["cur"] = json!(current);
    entry["status"] = json!("running");

    let phase = entry.get("phase").and_then(|v| v.as_str()).unwrap_or("download");
    if phase != "dependencies" { entry["phase"] = json!("download"); }
    entry["bytes_done"] = json!(0);
    entry["bytes_total"] = total_bytes.map(|b| json!(b)).unwrap_or(Value::Null);
    entry["error"] = Value::Null;
}

pub fn update_bytes(key: &str, done: u64) {
    let mut progress = APP.progress.lock().unwrap();
    if let Some(entry) = progress.get_mut(key) {
        entry["bytes_done"] = json!(done);
    }
}

/// Mark one file download as finished.
pub fn finish_file(key: &str) {
    let mut progress = APP.progress.lock().unwrap();
    if let Some(entry) = progress.get_mut(key) {
        let done = entry.get("files_done").and_then(|v| v.as_u64()).unwrap_or(0);
        entry["files_done"] = json!(done + 1);
        entry["cur"] = Value::Null;
        entry["bytes_done"] = json!(0);
        entry["bytes_total"] = Value::Null;
    }
}

fn finish_job_inner(key: &str, status: &str, error: Option<String>) {
    let mut progress = APP.progress.lock().unwrap();
    if let Some(entry) = progress.get_mut(key) {
        entry["status"] = json!(status);
        entry["phase"] = json!(if status == "done" {
            "done"
        } else if status == "error" {
            "error"
        } else {
            "running"
        });
        entry["cur"] = Value::Null;
        entry["error"] = error.map(|s| json!(s)).unwrap_or(Value::Null);
        entry["bytes_done"] = json!(0);
        entry["bytes_total"] = Value::Null;
    }
}

pub fn finish_job(key: &str, status: &str, error: Option<String>) {
    finish_job_inner(key, status, error);
}

pub fn fail_job(key: &str, message: String) {
    finish_job_inner(key, "error", Some(message));
}

pub fn finish_done(key: &str) {
    finish_job_inner(key, "done", None);
}

pub fn finish_file_if_key(key: Option<&str>) {
    if let Some(k) = key { finish_file(k); }
}
