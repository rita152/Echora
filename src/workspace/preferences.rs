//! Versioned UI preferences and atomic local persistence.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::agent::{ProjectId, ThreadId, ThreadSectionId};

const PREFERENCES_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPreferences {
    #[serde(default)]
    pub language: crate::i18n::Language,
    #[serde(default = "preferences_version")]
    pub version: u32,
    #[serde(default)]
    pub pinned_section_id: Option<ThreadSectionId>,
    #[serde(default)]
    pub collapsed_project_ids: BTreeSet<ProjectId>,
    #[serde(default)]
    pub pinned_collapsed: bool,
    #[serde(default)]
    pub projects_collapsed: bool,
    #[serde(default)]
    pub recent_collapsed: bool,
    #[serde(default)]
    pub review: ReviewPreferences,
    #[serde(default)]
    pub skip_side_chat_close_confirmation: bool,
    #[serde(default)]
    pub activity: ActivityPreferences,
    /// Chats with a turn the user has not opened since it finished or asked
    /// for a response. The app-server has no read state, so the app keeps it,
    /// as the reference keeps its own `unread-thread-ids-by-host`.
    #[serde(default)]
    pub unread_thread_ids: BTreeSet<ThreadId>,
    /// The Pull Requests detail panel's width ratio, as the reference stores
    /// `app-shell:right-panel-width:v3`; absent until the separator is dragged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_requests_detail_ratio: Option<PanelRatio>,
    /// What a message sent while a turn runs does. Kept locally: the reference
    /// stores its own copy in the Codex config's desktop section, which is the
    /// desktop app's setting, not app-server state this client should write.
    #[serde(default)]
    pub follow_up_mode: FollowUpMode,
    /// Where `/review` runs, as the reference's Git setting "Review delivery".
    #[serde(default)]
    pub review_delivery: ReviewDelivery,
    /// Custom sidebar sections in the order shown. The sections and their
    /// chats are app-server state; their order, like the reference's
    /// `sectionOrder`, is kept locally. New sections are appended.
    #[serde(default)]
    pub section_order: Vec<ThreadSectionId>,
    /// Projects placed in a custom section. App-server sections hold threads
    /// only, so project membership is local, as the reference keeps its
    /// section item keys.
    #[serde(default)]
    pub section_projects: BTreeMap<ThreadSectionId, Vec<ProjectId>>,
    #[serde(default)]
    pub collapsed_section_ids: BTreeSet<ThreadSectionId>,
    /// Pull requests attached to threads, mirrored from the server's
    /// attachments and read only when the server has none (an older CLI),
    /// like the reference's `pull-request-attachment-records-v3`: at most
    /// [`ATTACHMENT_RECORD_THREAD_LIMIT`] threads, most recently written last.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pull_request_attachment_records: Vec<PullRequestAttachmentRecord>,
    /// When backfill first ran here, in epoch milliseconds; used only when the
    /// ChatGPT app has not recorded its own cutoff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request_backfill_cutoff_at: Option<i64>,
    /// Threads backfill is done with here (found, not found, or a PR the user
    /// removed), at most [`BACKFILL_COMPLETED_LIMIT`], oldest first. Joined
    /// with the ChatGPT app's own list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pull_request_backfill_completed: Vec<ThreadId>,
    /// The user unpinned the thread summary panel. Stored inverted so the
    /// default is pinned, like the reference's global
    /// `app-shell-summary-panel-pinned` (default true).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub summary_panel_unpinned: bool,
    /// Summary panel sections the user expanded or collapsed, by section key;
    /// absent keys use the section's default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub summary_section_expanded: BTreeMap<String, bool>,
}

pub const ATTACHMENT_RECORD_THREAD_LIMIT: usize = 100;
pub const ATTACHMENT_RECORD_PER_THREAD_LIMIT: usize = 100;
pub const BACKFILL_COMPLETED_LIMIT: usize = 100;

/// One thread's pull requests as last read (or written locally when the server
/// has no attachments).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestAttachmentRecord {
    pub thread_id: ThreadId,
    pub pull_requests: Vec<RecordedPullRequest>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedPullRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_branch: Option<String>,
    /// Epoch milliseconds; the newest is the thread's current pull request.
    pub touched_at: i64,
}

/// `Inline` starts the review in the current chat; `Detached` starts a new
/// chat for it in the same project. Inline is the reference's default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewDelivery {
    #[default]
    Inline,
    Detached,
}

/// Follow-up behavior while a turn runs. `Steer` is the default because the
/// reference instance read `steer` for this account; ⌘⏎ inverts it per message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FollowUpMode {
    Queue,
    #[default]
    Steer,
}

impl FollowUpMode {
    pub fn inverted(self) -> Self {
        match self {
            Self::Queue => Self::Steer,
            Self::Steer => Self::Queue,
        }
    }
}

/// A resizable panel's position between its minimum (0) and maximum (1)
/// width. Always finite, so equality is total.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PanelRatio(pub f32);

impl Eq for PanelRatio {}

/// The activity view's `Show` options. Defaults match the reference: the
/// Priority section on, the Pinned section and scheduled task runs off.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActivityPreferences {
    pub show_priority: bool,
    pub show_pinned: bool,
    pub show_scheduled: bool,
}

impl Default for ActivityPreferences {
    fn default() -> Self {
        Self {
            show_priority: true,
            show_pinned: false,
            show_scheduled: false,
        }
    }
}

impl ActivityPreferences {
    /// `Restore defaults` is offered only once a choice differs from them.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReviewPreferences {
    pub split: bool,
    pub wrap: bool,
    pub load_files: bool,
    pub rich: bool,
    pub words: bool,
    pub ignore_whitespace: bool,
    pub tree_open: bool,
}
impl Default for ReviewPreferences {
    fn default() -> Self {
        Self {
            split: false,
            wrap: false,
            load_files: true,
            rich: false,
            words: false,
            ignore_whitespace: false,
            tree_open: true,
        }
    }
}

fn preferences_version() -> u32 {
    PREFERENCES_VERSION
}

impl UiPreferences {
    pub(super) fn current() -> Self {
        Self {
            version: PREFERENCES_VERSION,
            ..Self::default()
        }
    }
}

#[derive(Debug)]
pub(super) struct PreferenceStore {
    path: PathBuf,
    write_serial: AtomicU64,
}

impl PreferenceStore {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path,
            write_serial: AtomicU64::new(1),
        }
    }

    pub(super) fn load(&self) -> Result<UiPreferences, String> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(UiPreferences::current());
            }
            Err(error) => {
                return Err(
                    crate::i18n::format!("无法读取 UI 偏好：{error}" => "Could not read UI preferences: {error}"),
                );
            }
        };
        let preferences: UiPreferences =
            serde_json::from_slice(&bytes).map_err(|error| crate::i18n::format!("UI 偏好格式无效：{error}" => "Invalid UI preferences: {error}"))?;
        if preferences.version != PREFERENCES_VERSION {
            return Ok(UiPreferences::current());
        }
        Ok(preferences)
    }

    pub(super) fn save(&self, preferences: &UiPreferences) -> Result<(), String> {
        let Some(parent) = self.path.parent() else {
            return Err(crate::i18n::text("UI 偏好路径缺少父目录").to_owned());
        };
        fs::create_dir_all(parent).map_err(|error| crate::i18n::format!("无法创建 UI 偏好目录：{error}" => "Could not create UI preferences directory: {error}"))?;
        let serial = self.write_serial.fetch_add(1, Ordering::Relaxed);
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ui-preferences.json");
        let temporary = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            serial
        ));
        let write_result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
                .map_err(|error| crate::i18n::format!("无法创建 UI 偏好临时文件：{error}" => "Could not create temporary UI preferences file: {error}"))?;
            let bytes = serde_json::to_vec_pretty(preferences)
                .map_err(|error| crate::i18n::format!("无法序列化 UI 偏好：{error}" => "Could not serialize UI preferences: {error}"))?;
            file.write_all(&bytes)
                .map_err(|error| crate::i18n::format!("无法写入 UI 偏好：{error}" => "Could not write UI preferences: {error}"))?;
            file.write_all(b"\n")
                .map_err(|error| crate::i18n::format!("无法完成 UI 偏好写入：{error}" => "Could not finish writing UI preferences: {error}"))?;
            file.sync_all()
                .map_err(|error| crate::i18n::format!("无法同步 UI 偏好：{error}" => "Could not sync UI preferences: {error}"))?;
            fs::rename(&temporary, &self.path)
                .map_err(|error| crate::i18n::format!("无法原子替换 UI 偏好：{error}" => "Could not atomically replace UI preferences: {error}"))?;
            if let Ok(directory) = File::open(parent) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }
}

pub(super) fn default_preferences_path() -> PathBuf {
    if let Some(path) = std::env::var_os("GPUI_UI_PREFERENCES_PATH") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library/Application Support/GPUI")
            .join("ui-preferences.json");
    }
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config)
            .join("gpui")
            .join("ui-preferences.json");
    }
    PathBuf::from(".gpui-ui-preferences.json")
}

pub fn preferred_language() -> crate::i18n::Language {
    PreferenceStore::new(default_preferences_path())
        .load()
        .unwrap_or_default()
        .language
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_and_future_language_preferences_remain_readable() {
        for json in [
            r#"{"version":1,"pinned_collapsed":true}"#,
            r#"{"version":1,"language":"future-language","pinned_collapsed":true}"#,
        ] {
            let preferences: UiPreferences = serde_json::from_str(json).unwrap();
            assert_eq!(preferences.language, crate::i18n::Language::Auto);
            assert!(preferences.pinned_collapsed);
        }
        for language in crate::i18n::Language::ALL {
            let preferences = UiPreferences {
                language,
                ..UiPreferences::current()
            };
            let json = serde_json::to_string(&preferences).unwrap();
            assert_eq!(
                serde_json::from_str::<UiPreferences>(&json).unwrap(),
                preferences
            );
        }
    }

    #[test]
    fn version_mismatch_does_not_reuse_old_preferences() {
        let directory = std::env::temp_dir().join(format!(
            "gpui-workspace-preferences-{}-{}",
            std::process::id(),
            PreferenceStore::new(PathBuf::new())
                .write_serial
                .fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("preferences.json");
        fs::write(&path, br#"{"version":999,"pinned_section_id":"stale"}"#).unwrap();
        let loaded = PreferenceStore::new(path.clone()).load().unwrap();
        assert_eq!(loaded, UiPreferences::current());
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn preferences_are_written_as_complete_versioned_json() {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let directory = std::env::temp_dir().join(format!(
            "gpui-workspace-atomic-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let path = directory.join("preferences.json");
        let store = PreferenceStore::new(path.clone());
        let mut preferences = UiPreferences::current();
        preferences.pinned_section_id = Some("section-1".into());
        preferences.language = crate::i18n::Language::English;
        store.save(&preferences).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.ends_with(b"\n"));
        assert_eq!(
            serde_json::from_slice::<UiPreferences>(&bytes).unwrap(),
            preferences
        );
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
