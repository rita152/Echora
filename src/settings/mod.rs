mod spec;
mod view;

pub use spec::PageSpec;
pub use view::{
    ChangeFollowUpMode, ChangeLanguage, ChangeReviewDelivery, ChangeTheme, CloseSettings,
    ConfigSaveFinished, OpenSettingsFile, SettingsView,
};

pub fn pages() -> impl Iterator<Item = &'static PageSpec> {
    spec::PAGES.iter()
}

pub fn page(slug: &str) -> Option<&'static PageSpec> {
    pages().find(|page| page.slug == slug)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::pages;

    #[test]
    fn catalog_lists_only_pages_backed_by_real_data_in_navigation_order() {
        let actual: Vec<_> = pages().map(|page| page.slug).collect();
        assert_eq!(
            actual,
            [
                "general-settings",
                "appearance",
                "agent",
                "personalization",
                "plugins-settings",
                "hooks-settings",
                "git-settings",
            ]
        );
        assert_eq!(actual.iter().copied().collect::<HashSet<_>>().len(), 7);
    }
}

#[cfg(test)]
mod localization_tests {
    #[test]
    fn settings_navigation_labels_have_english_translations() {
        let previous = crate::i18n::language();
        crate::i18n::set_language(crate::i18n::Language::English);
        for page in crate::settings::pages() {
            let english = crate::i18n::text(page.label);
            assert!(
                !english
                    .chars()
                    .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                "Untranslated: {}",
                page.label
            );
        }
        crate::i18n::set_language(previous);
    }
}
