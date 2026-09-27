//! GitHub avatar images for the Pull Requests page.
//!
//! The reference draws each row's author avatar from `avatarUrl(size: 48)`.
//! This application has no HTTP client for GPUI images, so an avatar is
//! downloaded once with the system `curl` into the user cache directory and
//! drawn from disk afterwards.

use std::{fs, path::PathBuf, process::Command, time::Duration};

use anyhow::{Result, bail};

use crate::git_review::process;

const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

fn cache_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("GPUI_AVATAR_CACHE") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Caches/GPUI/pull-request-avatars");
    }
    if let Some(cache) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(cache).join("gpui/pull-request-avatars");
    }
    std::env::temp_dir().join("gpui-pull-request-avatars")
}

/// FNV-1a, so a URL keeps its cache file across builds.
fn file_name(url: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The cached image for `url`, when it has been downloaded before.
pub fn cached(url: &str) -> Option<PathBuf> {
    let path = cache_dir().join(file_name(url));
    path.is_file().then_some(path)
}

/// Downloads `url` into the cache (once) and returns the file.
pub fn fetch(url: &str) -> Result<PathBuf> {
    if let Some(path) = cached(url) {
        return Ok(path);
    }
    if !url.starts_with("https://") {
        bail!("refusing to fetch a non-HTTPS avatar");
    }
    let directory = cache_dir();
    fs::create_dir_all(&directory)?;
    let path = directory.join(file_name(url));
    let temporary = directory.join(format!("{}.{}.part", file_name(url), std::process::id()));
    let mut command = Command::new("curl");
    command
        .args([
            "--fail",
            "--silent",
            "--location",
            "--max-time",
            "15",
            "--output",
        ])
        .arg(&temporary)
        .arg(url);
    let output = process::run(&mut command, None, FETCH_TIMEOUT)?;
    if !output.status.success() {
        let _ = fs::remove_file(&temporary);
        bail!("curl exited with {}", output.status);
    }
    fs::rename(&temporary, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_names_are_stable_and_distinct() {
        let a = file_name("https://avatars.githubusercontent.com/u/1?s=48&v=4");
        let b = file_name("https://avatars.githubusercontent.com/u/2?s=48&v=4");
        assert_eq!(
            a,
            file_name("https://avatars.githubusercontent.com/u/1?s=48&v=4")
        );
        assert_ne!(a, b);
        assert_eq!(a.len(), 16);
    }
}
