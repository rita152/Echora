//! Application UI language. This module has no GUI or adapter dependencies.
//! Only app-owned labels belong here; never translate messages, paths, or server data.

#[path = "i18n/catalog.rs"]
mod catalog;
#[cfg(test)]
#[path = "i18n/tests.rs"]
mod tests;

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
#[cfg(not(test))]
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Language {
    #[serde(rename = "en")]
    English = 1,
    #[serde(rename = "zh-CN")]
    SimplifiedChinese = 2,
    #[default]
    #[serde(rename = "auto", other)]
    Auto = 0,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::Auto, Self::English, Self::SimplifiedChinese];

    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "en" | "en-US" | "en-GB" => Some(Self::English),
            "zh" | "zh-CN" | "zh-Hans" => Some(Self::SimplifiedChinese),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => text("自动检测"),
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
        }
    }

    pub fn resolve(self, system_locale: &str) -> Self {
        match self {
            Self::Auto => {
                let language = system_locale
                    .split(['-', '_', '.', '@'])
                    .next()
                    .unwrap_or("");
                if language.eq_ignore_ascii_case("zh") {
                    Self::SimplifiedChinese
                } else {
                    Self::English
                }
            }
            explicit => explicit,
        }
    }
}

// Initialize before creating any views. Tests use a per-thread setting so
// localization checks do not race existing Chinese presentation tests.
#[cfg(not(test))]
static LANGUAGE: AtomicU8 = AtomicU8::new(Language::Auto as u8);
#[cfg(test)]
thread_local! {
    static LANGUAGE: std::cell::Cell<Language> = const {
        std::cell::Cell::new(Language::SimplifiedChinese)
    };
}

pub fn language() -> Language {
    #[cfg(test)]
    return LANGUAGE.get();
    #[cfg(not(test))]
    match LANGUAGE.load(Ordering::Relaxed) {
        1 => Language::English,
        2 => Language::SimplifiedChinese,
        _ => Language::Auto,
    }
}

pub fn set_language(language: Language) {
    #[cfg(test)]
    LANGUAGE.set(language);
    #[cfg(not(test))]
    LANGUAGE.store(language as u8, Ordering::Relaxed);
}

pub fn is_english() -> bool {
    static SYSTEM_LOCALE: OnceLock<String> = OnceLock::new();
    match language() {
        Language::Auto => {
            Language::Auto.resolve(SYSTEM_LOCALE.get_or_init(system_locale)) == Language::English
        }
        explicit => explicit == Language::English,
    }
}

fn system_locale() -> String {
    // GUI launches do not reliably inherit LANG. Read the user's primary
    // macOS application language before falling back to process locale values.
    #[cfg(target_os = "macos")]
    if let Ok(output) = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        && output.status.success()
        && let Some(locale) = primary_apple_language(&String::from_utf8_lossy(&output.stdout))
    {
        return locale.to_owned();
    }
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| "en".into())
}

#[cfg(any(target_os = "macos", test))]
fn primary_apple_language(value: &str) -> Option<&str> {
    value
        .trim()
        .strip_prefix('(')?
        .strip_suffix(')')?
        .split(',')
        .map(|value| value.trim().trim_matches('"'))
        .find(|value| !value.is_empty())
}

/// Resolve a known source label. Unknown strings are preserved exactly.
/// Call only for application UI text, not arbitrary conversation content.
pub fn text(source: &str) -> &str {
    if is_english() {
        catalog::english(source).unwrap_or(source)
    } else {
        source
    }
}

/// Both templates remain literals so Rust validates formatting, including
/// captured variables, argument ordering, and format specifiers in both locales.
macro_rules! format {
    ($zh:literal => $en:literal $(, $($args:tt)*)?) => {
        if $crate::i18n::is_english() {
            std::format!($en $(, $($args)*)?)
        } else {
            std::format!($zh $(, $($args)*)?)
        }
    };
}
pub(crate) use format;
