//! Colours of the reference's in-app browser (ChatGPT 26.924), read from its
//! CSS custom properties in both themes.

use gpui::{Rgba, rgba};

use crate::theme::ThemeMode;

#[derive(Clone, Copy)]
pub struct BrowserTheme {
    pub dark: bool,
    /// `bg-surface`: the toolbar.
    pub toolbar: Rgba,
    /// `bg-surface-canvas`: the New tab page.
    pub canvas: Rgba,
    /// `--color-background-viewer-control`: capsules and the address pill.
    pub control: Rgba,
    /// `--color-background-primary-ghost-hover`.
    pub ghost_hover: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    /// `bg-info-soft`: the loading bar.
    pub info_soft: Rgba,
    /// `bg-primary-soft-alpha`: New tab tool rows.
    pub tool_row: Rgba,
    /// The shortcut chip: the text colour at 6.5%.
    pub kbd: Rgba,
    /// `bg-primary-soft`: the address suggestions.
    pub dropdown: Rgba,
    /// `border-primary-outline`.
    pub dropdown_border: Rgba,
    /// `bg-surface-elevated-secondary/90`: menus.
    pub menu: Rgba,
    /// The find bar's surface, as Echora's chat find bar.
    pub find: Rgba,
}

impl BrowserTheme {
    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self {
                dark: true,
                toolbar: rgba(0x181818ff),
                canvas: rgba(0x171717ff),
                control: rgba(0x363636f5),
                ghost_hover: rgba(0xffffff14),
                border: rgba(0xffffff15),
                text: rgba(0xdfdfdfff),
                text_secondary: rgba(0xdfdfdfa6),
                text_tertiary: rgba(0xffffff7f),
                info_soft: rgba(0x83c3ff4d),
                tool_row: rgba(0xffffff08),
                kbd: rgba(0xdfdfdf11),
                dropdown: rgba(0x2d2d2df5),
                dropdown_border: rgba(0xffffff28),
                menu: rgba(0x2d2d2de6),
                find: rgba(0x141414ff),
            },
            ThemeMode::Light => Self {
                dark: false,
                toolbar: rgba(0xffffffff),
                canvas: rgba(0xfafafaff),
                control: rgba(0xfffffff5),
                ghost_hover: rgba(0x1a1c1f0e),
                border: rgba(0x1a1c1f14),
                text: rgba(0x1a1c1fff),
                text_secondary: rgba(0x1a1c1fa6),
                text_tertiary: rgba(0x1a1c1f7e),
                info_soft: rgba(0x339cff4d),
                tool_row: rgba(0xfffffff5),
                kbd: rgba(0x1a1c1f11),
                dropdown: rgba(0xfffffff5),
                dropdown_border: rgba(0x1a1c1f1e),
                menu: rgba(0xffffffe6),
                find: rgba(0xf6f6f6ff),
            },
        }
    }

    /// `--shadow-viewer-surface`: a 1px 5% ring and a soft 16px drop.
    pub fn control_shadow(&self) -> Vec<gpui::BoxShadow> {
        vec![
            gpui::BoxShadow::new(gpui::px(0.), gpui::px(0.), rgba(0x0000000d).into())
                .spread_radius(gpui::px(1.)),
            gpui::BoxShadow::new(gpui::px(0.), gpui::px(4.), rgba(0x0000000d).into())
                .blur_radius(gpui::px(16.)),
        ]
    }

    /// `ring-[0.5px]` plus `--shadow-xl-spread`'s drop.
    pub fn menu_shadow(&self) -> Vec<gpui::BoxShadow> {
        vec![
            gpui::BoxShadow::new(gpui::px(0.), gpui::px(0.), self.border.into())
                .spread_radius(gpui::px(0.5)),
            gpui::BoxShadow::new(gpui::px(0.), gpui::px(8.), rgba(0x0000001f).into())
                .blur_radius(gpui::px(16.))
                .spread_radius(gpui::px(-4.)),
        ]
    }

    /// Tailwind's `shadow-lg` under the address suggestions.
    pub fn dropdown_shadow(&self) -> Vec<gpui::BoxShadow> {
        vec![
            gpui::BoxShadow::new(gpui::px(0.), gpui::px(10.), rgba(0x0000001a).into())
                .blur_radius(gpui::px(15.))
                .spread_radius(gpui::px(-3.)),
            gpui::BoxShadow::new(gpui::px(0.), gpui::px(4.), rgba(0x0000001a).into())
                .blur_radius(gpui::px(6.))
                .spread_radius(gpui::px(-4.)),
        ]
    }
}
