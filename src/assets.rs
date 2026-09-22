//! Runtime asset resolution for the GPUI shell.
//!
//! Icons, cursors, and other files under `assets/` are read from disk, so the
//! directory has to be found again at run time. A capture bundle is routinely
//! copied out of the worktree that built it, and two worktrees publish bundles
//! with the same name, so the base is resolved in a fixed order and every
//! candidate is reported. When nothing is found the shell keeps running but
//! says so: silently painting every icon blank is exactly the failure this
//! module exists to make impossible.

use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::Result;
use gpui::{AssetSource, SharedString};

/// Hard override: when set, it is the only candidate. Point it at the directory
/// that contains `icons/`. Useful for tests and for pointing a relocated bundle
/// at assets kept somewhere else; a wrong value fails loudly.
pub const ASSETS_DIR_ENV: &str = "GPUI_ASSETS_DIR";
/// The directory that has to exist for the base to count as usable.
pub const ASSETS_PROBE: &str = "icons";

/// Where the resolved base came from, in resolution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetOrigin {
    /// `GPUI_ASSETS_DIR`.
    Environment,
    /// `…/GPUI Capture.app/Contents/Resources/assets`, next to the executable.
    BundleResources,
    /// `assets` beside the executable.
    ExecutableDirectory,
    /// The worktree directory this binary was compiled in.
    CompiledWorktree,
    /// `assets` below the process working directory.
    WorkingDirectory,
}

impl AssetOrigin {
    pub fn label(self) -> &'static str {
        match self {
            Self::Environment => ASSETS_DIR_ENV,
            Self::BundleResources => "bundle Contents/Resources/assets",
            Self::ExecutableDirectory => "directory next to the executable",
            Self::CompiledWorktree => "worktree of the build",
            Self::WorkingDirectory => "working directory",
        }
    }
}

/// One candidate path that was considered, with the reason it was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetCandidate {
    pub origin: AssetOrigin,
    pub path: PathBuf,
    pub usable: bool,
}

/// The resolved base plus everything that was tried, for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetStatus {
    pub base: Option<PathBuf>,
    pub origin: Option<AssetOrigin>,
    pub candidates: Vec<AssetCandidate>,
}

impl AssetStatus {
    pub fn is_missing(&self) -> bool {
        self.base.is_none()
    }

    /// One line for logs, banners, and capture sidecars.
    pub fn summary(&self) -> String {
        match (&self.base, self.origin) {
            (Some(base), Some(origin)) => {
                format!("{} ({})", base.display(), origin.label())
            }
            _ => "assets directory not found".to_owned(),
        }
    }

    /// The candidate list, in the order that was tried.
    pub fn tried_summary(&self) -> String {
        self.candidates
            .iter()
            .map(|candidate| {
                format!(
                    "{} ({})",
                    candidate.path.display(),
                    candidate.origin.label()
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Every candidate, in resolution order. The compiled worktree is the most
/// reliable path for `cargo run`, but a packaged bundle has to win over the
/// machine that built it, so the bundle layout comes first.
pub fn candidates_from(
    environment: Option<PathBuf>,
    executable: Option<&Path>,
    worktree: &Path,
    working_directory: Option<&Path>,
) -> Vec<(AssetOrigin, PathBuf)> {
    // An explicit override is authoritative: if it is wrong, say so instead of
    // silently loading assets from somewhere else.
    if let Some(from_environment) = environment {
        return vec![(AssetOrigin::Environment, from_environment)];
    }
    let mut candidates = Vec::new();
    if let Some(directory) = executable.and_then(Path::parent) {
        // `…/GPUI Capture.app/Contents/MacOS/binary` → `…/Contents/Resources`.
        candidates.push((
            AssetOrigin::BundleResources,
            directory.join("../Resources/assets"),
        ));
        candidates.push((AssetOrigin::ExecutableDirectory, directory.join("assets")));
    }
    candidates.push((AssetOrigin::CompiledWorktree, worktree.join("assets")));
    if let Some(directory) = working_directory {
        let path = directory.join("assets");
        if !candidates.iter().any(|(_, candidate)| *candidate == path) {
            candidates.push((AssetOrigin::WorkingDirectory, path));
        }
    }
    candidates
}

/// Picks the first candidate that actually contains `assets/icons`, so a
/// half-copied bundle is rejected instead of loading nothing.
pub fn resolve_from(candidates: Vec<(AssetOrigin, PathBuf)>) -> AssetStatus {
    let mut status = AssetStatus {
        base: None,
        origin: None,
        candidates: Vec::new(),
    };
    for (origin, path) in candidates {
        if status
            .candidates
            .iter()
            .any(|candidate| candidate.path == path)
        {
            continue;
        }
        let usable = path.join(ASSETS_PROBE).is_dir();
        if status.base.is_none() && usable {
            // Keep the literal path for diagnostics, but hand the loader a
            // normalized one so `..` never reaches `fs::read`.
            status.base = Some(fs::canonicalize(&path).unwrap_or_else(|_| path.clone()));
            status.origin = Some(origin);
        }
        status.candidates.push(AssetCandidate {
            origin,
            path,
            usable,
        });
    }
    status
}

/// Resolves against this process: environment, executable, build worktree, and
/// working directory, in that order.
pub fn resolve() -> AssetStatus {
    let worktree = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let executable = std::env::current_exe().ok();
    let working_directory = std::env::current_dir().ok();
    resolve_from(candidates_from(
        std::env::var_os(ASSETS_DIR_ENV).map(PathBuf::from),
        executable.as_deref(),
        &worktree,
        working_directory.as_deref(),
    ))
}

/// Resolved once: the answer cannot change while the process runs.
pub fn status() -> &'static AssetStatus {
    static STATUS: OnceLock<AssetStatus> = OnceLock::new();
    STATUS.get_or_init(resolve)
}

/// `AssetSource` for the resolved base.
///
/// The loader has no fallback of its own: when [`AssetStatus::base`] is `None`
/// every read fails, so "the banner is shown" and "the icons are blank" are the
/// same condition by construction instead of two behaviours that can drift.
pub struct Assets {
    base: Option<PathBuf>,
}

impl Assets {
    pub fn load_from(status: &AssetStatus) -> Self {
        Self {
            base: status.base.clone(),
        }
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let Some(base) = self.base.as_ref() else {
            anyhow::bail!("assets directory is missing; {path} cannot be loaded")
        };
        fs::read(base.join(path))
            .map(Cow::Owned)
            .map(Some)
            .map_err(Into::into)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let Some(base) = self.base.as_ref() else {
            anyhow::bail!("assets directory is missing; {path} cannot be listed")
        };
        Ok(fs::read_dir(base.join(path))?
            .filter_map(|entry| {
                entry
                    .ok()?
                    .file_name()
                    .into_string()
                    .ok()
                    .map(SharedString::from)
            })
            .collect())
    }
}

/// The warning that must never be silent. Returned instead of printed so the
/// shell can both log it and show it on screen.
pub fn missing_warning(status: &AssetStatus) -> String {
    format!(
        "assets directory not found; icons and other assets will not render. Tried: {}",
        status.tried_summary()
    )
}

#[cfg(test)]
#[path = "assets/tests.rs"]
mod tests;
