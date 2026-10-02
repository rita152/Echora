/// One entry of the settings navigation. The page body is rendered by its view;
/// the slug is what `--settings-page` and `SettingsView::select` accept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSpec {
    pub slug: &'static str,
    pub label: &'static str,
}

/// Only pages backed by real data: each one reads or writes the app-server
/// configuration, the plugin and hook directories, or a persisted UI preference.
pub const PAGES: &[PageSpec] = &[
    PageSpec {
        slug: "general-settings",
        label: "常规",
    },
    PageSpec {
        slug: "appearance",
        label: "外观",
    },
    PageSpec {
        slug: "agent",
        label: "配置",
    },
    PageSpec {
        slug: "personalization",
        label: "个性化",
    },
    PageSpec {
        slug: "plugins-settings",
        label: "插件",
    },
    PageSpec {
        slug: "hooks-settings",
        label: "钩子",
    },
    PageSpec {
        slug: "git-settings",
        label: "Git",
    },
];
