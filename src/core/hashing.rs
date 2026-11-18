//! File hashing and cache-backed verification.
//!
//! Modrinth identification uses SHA-1; CurseForge identification uses its
//! MurmurHash2 fingerprint. Both are cached by path, size, and mtime.

use std::fs;
use std::io::Read;
use std::path::Path;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};

use crate::core::errors::AppError;
use crate::core::state::{APP, read_json};

pub fn sha1_file(path: &Path) -> Result<String, AppError> {
    let mut hasher = Sha1::new();
    let mut file = fs::File::open(path).map_err(AppError::from)?;
    let mut buf = vec![0u8; 262144];
    loop {
        let n = file.read(&mut buf).map_err(|e| AppError::Internal(e.to_string()))?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn size_and_mtime(path: &Path) -> Result<(u64, i64), AppError> {
    let md = fs::metadata(path).map_err(AppError::from)?;
    let mtime = md.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0);
    Ok((md.len(), mtime))
}

pub fn load_hash_cache() -> Value {
    let v = read_json(&APP.state_dir.join("hashes.json"));
    if v.is_object() { v } else { json!({}) }
}

pub fn save_hash_cache(cache: &Value) {
    let _ = fs::create_dir_all(&APP.state_dir);
    let _ = fs::write(APP.state_dir.join("hashes.json"), cache.to_string());
}

/// SHA-1 for a path, reusing the cache when size and mtime match.
pub fn cached_sha1(path: &Path, cache: &mut Value) -> Result<String, AppError> {
    let (size, mtime) = size_and_mtime(path)?;
    let key = path.display().to_string();

    if let Some(entry) = cache.get(&key) {
        if entry.get("sha1").and_then(|v| v.as_str()).is_some()
            && entry.get("size").and_then(|v| v.as_u64()) == Some(size)
            && entry.get("mtime").and_then(|v| v.as_i64()) == Some(mtime)
        {
            return Ok(entry["sha1"].as_str().unwrap().to_string());
        }
    }

    let hash = sha1_file(path)?;
    cache[key] = json!({ "size": size, "mtime": mtime, "sha1": hash });
    Ok(hash)
}

/// CurseForge fingerprint: MurmurHash2 with seed 1 over whitespace-stripped bytes.
pub fn murmur2(data: &[u8]) -> u32 {
    let data: Vec<u8> = data.iter().cloned()
        .filter(|b| *b != 9 && *b != 10 && *b != 13 && *b != 32)
        .collect();

    let m: u32 = 0x5BD1E995;
    let r = 24;
    let mut h: u32 = 1u32 ^ (data.len() as u32);
    let (mut i, mut n) = (0usize, data.len());

    while n >= 4 {
        let mut k: u32 = data[i] as u32
            | ((data[i + 1] as u32) << 8)
            | ((data[i + 2] as u32) << 16)
            | ((data[i + 3] as u32) << 24);
        k = k.wrapping_mul(m);
        k ^= k >> r;
        k = k.wrapping_mul(m);
        h = h.wrapping_mul(m);
        h ^= k;
        i += 4;
        n -= 4;
    }
    if n >= 3 { h ^= (data[i + 2] as u32) << 16; }
    if n >= 2 { h ^= (data[i + 1] as u32) << 8; }
    if n >= 1 { h ^= data[i] as u32; h = h.wrapping_mul(m); }

    h ^= h >> 13;
    h = h.wrapping_mul(m);
    h ^= h >> 15;
    h
}

/// CurseForge fingerprint for a path, cached by size and mtime.
pub fn cached_fingerprint(path: &Path, cache: &mut Value) -> Result<u32, AppError> {
    let (size, mtime) = size_and_mtime(path)?;
    let key = path.display().to_string();

    if let Some(entry) = cache.get(&key) {
        if entry.get("fp").and_then(|v| v.as_u64()).is_some()
            && entry.get("size").and_then(|v| v.as_u64()) == Some(size)
            && entry.get("mtime").and_then(|v| v.as_i64()) == Some(mtime)
        {
            return Ok(entry["fp"].as_u64().unwrap() as u32);
        }
    }

    let raw = fs::read(path).map_err(|e| AppError::Internal(e.to_string()))?;
    let fp = murmur2(&raw);
    match cache.get_mut(&key) {
        Some(entry)
            if entry.get("size").and_then(|v| v.as_u64()) == Some(size)
            && entry.get("mtime").and_then(|v| v.as_i64()) == Some(mtime)
            && entry.get("sha1").is_some() =>
        {
            entry["fp"] = json!(fp);
        }
        _ => {
            cache[key] = json!({ "size": size, "mtime": mtime, "sha1": null, "fp": fp });
        }
    }
    Ok(fp)
}
