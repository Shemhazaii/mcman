//! Streaming file download with progress reporting and zip sanity checks.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::core::errors::AppError;
use crate::core::progress;
use crate::providers::http::CLIENT;
use crate::providers::http::USER_AGENT;

/// Download a URL to `dest`, writing through a `.part` file first.
pub fn download_file(
    url: &str,
    dest: &Path,
    headers: &[(&'static str, String)],
    key: Option<&str>,
) -> Result<(), AppError> {
    let mut request = CLIENT.get(url).header("User-Agent", USER_AGENT);
    for (k, v) in headers { request = request.header(*k, v.clone()); }

    let mut response = request.send().map_err(|e| AppError::Internal(e.to_string()))?;
    if !response.status().is_success() {
        return Err(AppError::Internal(format!("download HTTP {}", response.status().as_u16())));
    }

    let total = response.headers().get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    let current = dest.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    if let Some(k) = key { progress::begin_file(k, &current, total); }

    let tmp = PathBuf::from(format!("{}.part", dest.display()));
    {
        let mut file = fs::File::create(&tmp).map_err(|e| AppError::Internal(e.to_string()))?;
        // A zero-length buffer would make read() return EOF immediately.
        let mut buf = vec![0u8; 262144];
        let mut done: u64 = 0;
        loop {
            let n = response.read(&mut buf).map_err(|e| AppError::Internal(e.to_string()))?;
            if n == 0 { break; }
            std::io::Write::write_all(&mut file, &buf[..n])
                .map_err(|e| AppError::Internal(e.to_string()))?;
            done += n as u64;
            if let Some(k) = key { progress::update_bytes(k, done); }
        }
    }

    // Mod jars are zip archives, so reject empty or non-PK downloads.
    let valid = match fs::metadata(&tmp) {
        Ok(md) if md.len() > 0 => {
            let mut head = [0u8; 2];
            fs::File::open(&tmp)
                .and_then(|mut f| f.read_exact(&mut head))
                .map(|_| head == *b"PK")
                .unwrap_or(false)
        }
        _ => false,
    };
    if !valid {
        let _ = fs::remove_file(&tmp);
        return Err(AppError::Internal(format!("download kosong/rusak (bukan arsip zip) — {url}")));
    }

    fs::rename(&tmp, dest).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(())
}
