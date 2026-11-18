//! External mod repositories.
//!
//! `http` owns the shared blocking client and low-level request helpers;
//! `modrinth` and `curseforge` expose provider-specific search/install logic.

pub mod curseforge;
pub mod http;
pub mod modrinth;

/// Apply UI-supplied loader/Minecraft-version overrides to the detected values.
pub fn apply_loader_mc_override(
    detected: (String, String),
    mc_override: &str,
    loader_override: &str,
) -> (String, String) {
    let (mut loader, mut mcver) = detected;
    if !loader_override.is_empty() { loader = loader_override.to_string(); }
    if !mc_override.is_empty() { mcver = mc_override.to_string(); }
    (loader, mcver)
}
