//! The reference's viewer header (`_header_1oxlz`, `_capsule_14ans`): the
//! pill-shaped control groups the Changes and Files tabs float over their
//! content. Values are ChatGPT 26.930's computed styles in both themes.

use gpui::{BoxShadow, Div, Rgba, Styled, div, px, rgba};

use crate::theme::{Theme, ThemeMode};

/// The header row: 32px capsules in 8px of padding, above the content's 1px
/// top rule.
pub const HEIGHT: f32 = 48.0;
/// A capsule control (`aspect-square`, `rounded-full`).
pub const CONTROL_SIZE: f32 = 28.0;
/// Below this header width the reference folds Refresh, Word wrap, Collapse
/// and the diff layout into the options menu
/// (`@container review-header (min-width: 625px)`).
pub const WIDE_MIN_WIDTH: f32 = 625.0;

/// A capsule: a 2px inset group of controls on the viewer control surface.
pub fn capsule(mode: ThemeMode) -> Div {
    div()
        .h(px(32.0))
        .flex_none()
        .p(px(2.0))
        .flex()
        .items_center()
        .gap(px(2.0))
        .rounded_full()
        .bg(surface(mode))
        .shadow(shadow(mode))
}

/// `_controlSurface`: `surface-elevated-secondary` in dark mode, and
/// `--color-background-viewer-control` (white at 96%) in light mode.
pub fn surface(mode: ThemeMode) -> Rgba {
    match mode {
        ThemeMode::Dark => Theme::for_mode(mode).control,
        ThemeMode::Light => rgba(0xfffffff5),
    }
}

/// Dark mode: the half-pixel border ring and `--shadow-xl-spread`'s drop
/// (`0 8px 16px -4px #0000001f`). Light mode: `--shadow-viewer-surface`
/// (`0 0 0 1px` at 4.7% and `0 4px 16px #0000000d`).
pub fn shadow(mode: ThemeMode) -> Vec<BoxShadow> {
    match mode {
        ThemeMode::Dark => vec![
            BoxShadow::new(px(0.0), px(0.0), rgba(0xffffff15).into()).spread_radius(px(0.5)),
            BoxShadow::new(px(0.0), px(8.0), rgba(0x0000001f).into())
                .blur_radius(px(16.0))
                .spread_radius(px(-4.0)),
        ],
        ThemeMode::Light => vec![
            BoxShadow::new(px(0.0), px(0.0), rgba(0x1a1c1f0c).into()).spread_radius(px(1.0)),
            BoxShadow::new(px(0.0), px(4.0), rgba(0x0000000d).into()).blur_radius(px(16.0)),
        ],
    }
}

/// A toggled-on control (`aria-pressed`): the accent glyph on an accent wash,
/// as `(foreground, background)`.
pub fn pressed(mode: ThemeMode) -> (Rgba, Rgba) {
    match mode {
        ThemeMode::Dark => (rgba(0x83c3ffff), rgba(0x1b252fff)),
        ThemeMode::Light => (rgba(0x339cffff), rgba(0xebf5ffff)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressed_controls_use_the_theme_accent() {
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            assert_eq!(pressed(mode).0, Theme::for_mode(mode).accent);
        }
    }
}
