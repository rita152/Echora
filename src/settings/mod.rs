mod catalog_account;
mod catalog_coding;
mod catalog_personal;
mod spec;
mod view;

pub use spec::{ControlSpec, PageKind, PageSpec, RowSpec, SectionSpec};
pub use view::{ChangeLanguage, ChangeTheme, CloseSettings, ConfigSaveFinished, SettingsView};

pub fn pages() -> impl Iterator<Item = &'static PageSpec> {
    catalog_personal::PAGES
        .iter()
        .chain(catalog_account::PAGES.iter())
        .chain(catalog_coding::PAGES.iter())
}

pub fn page(slug: &str) -> Option<&'static PageSpec> {
    pages().find(|page| page.slug == slug)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::pages;

    #[test]
    fn catalog_contains_all_reference_pages_in_navigation_order() {
        let actual: Vec<_> = pages().map(|page| page.slug).collect();
        assert_eq!(
            actual,
            [
                "general-settings",
                "profile",
                "appearance",
                "voice",
                "agent",
                "personalization",
                "keyboard-shortcuts",
                "computer-use",
                "chronicle",
                "appshots",
                "plugins-settings",
                "browser-use",
                "hooks-settings",
                "connections",
                "git-settings",
                "local-environments",
                "worktrees",
                "data-controls",
            ]
        );
        assert_eq!(actual.iter().copied().collect::<HashSet<_>>().len(), 18);
        assert!(pages().all(|page| !page.sections.is_empty()));
    }
}

#[cfg(test)]
mod localization_tests {
    #[test]
    fn settings_navigation_and_core_catalog_have_english_translations() {
        let previous = crate::i18n::language();
        crate::i18n::set_language(crate::i18n::Language::English);
        fn check(source: &str) {
            if source
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
            {
                let english = crate::i18n::text(source);
                assert!(
                    !english
                        .chars()
                        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                    "Untranslated: {source}"
                );
            }
        }
        for page in crate::settings::pages() {
            check(page.label);
            check(page.intro);
            for section in page.sections {
                check(section.title);
                if [
                    "general-settings",
                    "appearance",
                    "keyboard-shortcuts",
                    "agent",
                    "browser-use",
                ]
                .contains(&page.slug)
                {
                    check(section.subtitle);
                    for row in section.rows {
                        check(row.title);
                        check(row.subtitle);
                        match row.control {
                            crate::settings::ControlSpec::Button(value)
                            | crate::settings::ControlSpec::Select(value)
                            | crate::settings::ControlSpec::Value(value)
                            | crate::settings::ControlSpec::Shortcut(value)
                            | crate::settings::ControlSpec::Danger(value) => check(value),
                            crate::settings::ControlSpec::Segmented(values, _) => {
                                values.iter().for_each(|value| check(value))
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        crate::i18n::set_language(previous);
    }
}
