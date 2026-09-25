use gpui::{Font, FontFallbacks, FontWeight, Rgba, font, rgba};

/// ChatGPT's computed CSS uses `-apple-system, system-ui, "Segoe UI", sans-serif`.
/// On macOS CDP reports `.SF NS` for Latin glyphs and PingFang SC for Simplified
/// Chinese. GPUI maps this special family to `.AppleSystemUIFont`; the explicit
/// CJK fallback keeps mixed Chinese/English runs on the same platform stack.
pub const UI_FONT_FAMILY: &str = ".SystemUIFont";
pub const UI_CJK_FALLBACK_FAMILY: &str = "PingFang SC";
// CDP's platform-font probe resolves ChatGPT's `ui-monospace` stack to Menlo
// (PostScript face Menlo-Regular) on macOS.
pub const UI_MONOSPACE_FONT_FAMILY: &str = "Menlo";
/// Live ChatGPT body/`font-normal` token; explicit 400-weight controls stay 400.
pub const UI_BODY_FONT_WEIGHT: FontWeight = FontWeight(430.0);
/// Shared outer inset for main and side conversation content. Toolbar chrome
/// remains full width; messages and the floating composer use this gutter.
pub const CHAT_CONTENT_HORIZONTAL_GUTTER: f32 = 24.0;

pub fn ui_font() -> Font {
    let mut font = font(UI_FONT_FAMILY);
    font.weight = UI_BODY_FONT_WEIGHT;
    font.fallbacks = Some(FontFallbacks::from_fonts(vec![
        UI_CJK_FALLBACK_FAMILY.to_owned(),
    ]));
    font
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    pub fn from_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("light") {
            Self::Light
        } else {
            Self::Dark
        }
    }
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub surface: Rgba,
    /// Live CDP file editor canvas and plain text, separate from the panel chrome.
    pub file_editor_surface: Rgba,
    pub file_editor_text: Rgba,
    /// The sidebar's actual translucent paint, matching the Electron shell.
    pub sidebar_surface: Rgba,
    pub surface_under: Rgba,
    pub elevated: Rgba,
    pub model_picker_surface: Rgba,
    pub project_dialog_surface: Rgba,
    pub control: Rgba,
    pub control_soft: Rgba,
    pub sidebar_hover: Rgba,
    /// `--color-border` / `border-default` hairlines around the sidebar: the
    /// footer's top rule, the main surface's left edge and the thin scrollbar
    /// thumb all share it.
    pub sidebar_hairline: Rgba,
    pub sidebar_icon_muted: Rgba,
    /// Product-title foreground for the locally loaded OpenAI Sans face.
    pub sidebar_title_text: Rgba,
    /// Secondary sidebar foreground measured from the ChatGPT desktop app.
    pub sidebar_text_muted: Rgba,
    pub text: Rgba,
    /// Foreground and fill of user-authored message bubbles.
    pub user_message_text: Rgba,
    pub user_message_surface: Rgba,
    pub sidebar_text: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    /// Primary Markdown foreground. Kept semantic so CDP-derived values can be
    /// tuned without coupling assistant content to the surrounding shell.
    pub markdown_text: Rgba,
    pub markdown_link: Rgba,
    pub markdown_file_link: Rgba,
    pub markdown_inline_code_text: Rgba,
    pub markdown_inline_code_surface: Rgba,
    pub markdown_code_surface: Rgba,
    pub markdown_code_header_surface: Rgba,
    pub markdown_code_border: Rgba,
    pub markdown_syntax_comment: Rgba,
    pub markdown_syntax_keyword: Rgba,
    pub markdown_syntax_literal: Rgba,
    pub markdown_syntax_string: Rgba,
    pub markdown_syntax_variable: Rgba,
    pub markdown_syntax_attribute: Rgba,
    pub markdown_syntax_name: Rgba,
    pub markdown_syntax_error: Rgba,
    pub markdown_action_hover: Rgba,
    pub markdown_blockquote_border: Rgba,
    pub markdown_table_border_strong: Rgba,
    pub markdown_table_border_subtle: Rgba,
    pub markdown_table_header_surface: Rgba,
    pub markdown_rule: Rgba,
    /// Shell/tool card fill measured from ChatGPT's `bg-secondary-soft-alpha`.
    pub command_surface: Rgba,
    /// Proposed-plan card surfaces measured independently from command cards.
    pub plan_surface: Rgba,
    pub plan_border: Rgba,
    pub plan_progress_surface: Rgba,
    /// Shell/tool card outline measured from ChatGPT's `border-strong`.
    pub command_border: Rgba,
    /// Shell `text-codex-description`: primary text at 70% opacity.
    pub command_text: Rgba,
    /// Precomposited tertiary text used by Shell prefixes and status labels.
    pub command_muted: Rgba,
    pub home_mark: Rgba,
    pub border: Rgba,
    pub accent: Rgba,
    pub warning: Rgba,
    pub effort: Rgba,
    pub button: Rgba,
    pub button_text: Rgba,
    pub profile_menu_shadow: Rgba,
    pub settings_panel: Rgba,
    pub settings_switch_off: Rgba,
    /// Command menu / chat search overlay, panel, and row tokens measured from
    /// the live ChatGPT desktop app (`.codex-dialog-overlay`, `[cmdk-root]`).
    pub chat_search_overlay: Rgba,
    pub chat_search_surface: Rgba,
    pub chat_search_border: Rgba,
    /// Sidebar project hover card. ChatGPT paints the card with
    /// `bg-surface-elevated-secondary/90`, its `--color-border` ring, and the
    /// shared `shadow-xl-spread`; the card reuses `text`, `text_tertiary`,
    /// `border`, and `sidebar_hover` for every inner element.
    pub project_hover_surface: Rgba,
    /// The command menu keeps its own primary foreground. In dark mode it is
    /// pure white even though the surrounding native shell uses a softened
    /// #dfdfdf body foreground.
    pub chat_search_text: Rgba,
    pub chat_search_row_hover: Rgba,
    pub chat_search_description: Rgba,
    pub chat_search_hint_surface: Rgba,
    /// Surface of the inline message editor secondary button (dark: white 3%, light: white 96%).
    pub edit_button_surface: Rgba,
    /// Conversation user-message navigation rail. The markers print ChatGPT's
    /// `--color-codex-description`; the hover card reuses the elevated
    /// secondary surface at 95% and the shared `shadow-xl-spread`.
    pub navigation_rail_marker: Rgba,
    pub navigation_rail_surface: Rgba,
    /// Keyboard hints in the sidebar account menu print ChatGPT's
    /// `--color-codex-description`.
    pub account_menu_shortcut: Rgba,
    pub settings_search: Rgba,
    pub settings_accent: Rgba,
    pub settings_description: Rgba,
    pub settings_control: Rgba,
    pub settings_button: Rgba,
}

impl Theme {
    /// Electron disables its native backdrop for inactive macOS windows and
    /// windows whose physical dimensions reach 3840 × 2160 (either orientation).
    /// Paint the same surface-under beneath the 70% tint in those states.
    pub fn for_window(mode: ThemeMode, active: bool, width: f32, height: f32, scale: f32) -> Self {
        let mut theme = Self::for_mode(mode);
        if cfg!(target_os = "macos")
            && (!active
                || (width.max(height) * scale >= 3840.0 && width.min(height) * scale >= 2160.0))
        {
            // Use the captured Chromium composite, including its 8-bit paint
            // rounding: 70% white over #f6f6f6 is #fdfdfd in the app's PNG;
            // 70% #282828 over #141414 is #222222.
            theme.sidebar_surface = match mode {
                ThemeMode::Light => rgba(0xfdfdfdff),
                ThemeMode::Dark => rgba(0x222222ff),
            };
        }
        theme
    }

    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => Self {
                surface: rgba(0xffffffff),
                file_editor_surface: rgba(0xffffffff),
                file_editor_text: rgba(0x0d0d0dff),
                // Live ChatGPT CDP: color(srgb 1 1 1 / 0.7), composited once
                // over Electron's macOS Menu material. Preserve the exact alpha
                // and neutral white instead of compensating for one backdrop.
                sidebar_surface: Rgba {
                    a: 0.7,
                    ..rgba(0xffffffff)
                },
                // Opaque secondary surface used where a sticky overlay must
                // mask scrolling content instead of resampling the material.
                surface_under: rgba(0xf6f6f6ff),
                elevated: rgba(0xffffffff),
                model_picker_surface: rgba(0xfafafaff),
                project_dialog_surface: rgba(0xfafafaff),
                control: rgba(0xffffffff),
                control_soft: rgba(0xffffffff),
                // chat-reference: --color-background-primary-ghost-hover
                // rgba(26, 28, 31, 0.053), quantized to an 8-bit alpha.
                sidebar_hover: rgba(0x1a1c1f0e),
                // chat-reference: rgba(26, 28, 31, 0.08).
                sidebar_hairline: rgba(0x1a1c1f14),
                sidebar_icon_muted: rgba(0x1a1c1f7f),
                sidebar_title_text: rgba(0x1a1c1fd9),
                sidebar_text_muted: rgba(0x1a1c1f7f),
                text: rgba(0x1a1c1fff),
                user_message_text: rgba(0xffffffff),
                user_message_surface: rgba(0x000000ff),
                sidebar_text: rgba(0x1a1c1fd9),
                text_secondary: rgba(0x5d5d5dff),
                text_tertiary: rgba(0x1a1c1f7e),
                markdown_text: rgba(0x1a1c1fff),
                markdown_link: rgba(0x2c67c5ff),
                // CDP: color-mix(in srgb, #2c67c5 80%, #1a1c1f 20%).
                markdown_file_link: rgba(0x2858a4ff),
                // Values below were resolved from the live ChatGPT desktop
                // app over CDP. Keeping them semantic prevents theme colors
                // from changing the Markdown node geometry.
                markdown_inline_code_text: rgba(0x1a1c1fff),
                markdown_inline_code_surface: rgba(0x1a1c1f18),
                markdown_code_surface: rgba(0x1a1c1f0c),
                markdown_code_header_surface: rgba(0xffffffff),
                markdown_code_border: rgba(0x1a1c1f0c),
                // highlight.js semantic colors from ChatGPT's Codex theme.
                markdown_syntax_comment: rgba(0x4f4f4fff),
                markdown_syntax_keyword: rgba(0xab4f7aff),
                markdown_syntax_literal: rgba(0xac4f23ff),
                markdown_syntax_string: rgba(0x3a843fff),
                markdown_syntax_variable: rgba(0x643caeff),
                markdown_syntax_attribute: rgba(0xb8802bff),
                markdown_syntax_name: rgba(0x1f4e94ff),
                markdown_syntax_error: rgba(0xba2623ff),
                markdown_action_hover: rgba(0x1a1c1f0e),
                markdown_blockquote_border: rgba(0x1a1c1f1e),
                markdown_table_border_strong: rgba(0x1a1c1f1e),
                markdown_table_border_subtle: rgba(0x1a1c1f0c),
                markdown_table_header_surface: rgba(0x00000000),
                markdown_rule: rgba(0x1a1c1f1e),
                command_surface: rgba(0x0000000d),
                plan_surface: rgba(0xffffffff),
                plan_border: rgba(0x1a1c1f14),
                plan_progress_surface: rgba(0xffffffff),
                command_border: rgba(0x00000028),
                command_text: rgba(0x1a1c1fb3),
                command_muted: rgba(0x898989ff),
                home_mark: rgba(0xb8b9baff),
                border: rgba(0x1a1c1f14),
                accent: rgba(0x339cffff),
                warning: rgba(0xe25507ff),
                effort: rgba(0x924ff7ff),
                button: rgba(0x1a1c1fff),
                button_text: rgba(0xffffffff),
                // chat-reference: --shadow-xl, 0 8px 16px -4px #0000001f.
                profile_menu_shadow: rgba(0x0000001f),
                // Settings interaction surfaces intentionally reuse the main
                // pane background so the two views cannot drift by theme.
                settings_panel: rgba(0xffffffff),
                settings_switch_off: rgba(0x1a1c1f1a),
                // CDP: overlay electron:bg-[#00000022]; [cmdk-root] #ffffff with
                // a transparent 1px border; ghost-hover 0.055; description 0.494.
                chat_search_overlay: rgba(0x00000022),
                chat_search_surface: rgba(0xffffffff),
                chat_search_border: rgba(0x00000000),
                // CDP light: --color-surface-elevated-secondary #ffffff at 90%.
                project_hover_surface: rgba(0xffffffe6),
                chat_search_text: rgba(0x1a1c1fff),
                chat_search_row_hover: rgba(0x1a1c1f0e),
                chat_search_description: rgba(0x1a1c1f7e),
                chat_search_hint_surface: rgba(0x1a1c1f1a),
                edit_button_surface: rgba(0xfffffff5),
                // CDP light: `--color-codex-description` rgba(26, 28, 31, .494).
                navigation_rail_marker: rgba(0x1a1c1f7e),
                // `bg-surface-elevated-secondary/95` over the #ffffff pane.
                navigation_rail_surface: rgba(0xffffffff),
                // CDP: rgba(26, 28, 31, 0.494).
                account_menu_shortcut: rgba(0x1a1c1f7e),
                // ChatGPT settings search surface at the reference capture
                // resolves to #f2f2f2 on the light shell.
                settings_search: rgba(0xf2f2f2ff),
                settings_accent: rgba(0x539af8ff),
                settings_description: rgba(0x1a1c1fa6),
                settings_control: rgba(0xf7f7f7ff),
                settings_button: rgba(0xf0f0f0ff),
            },
            ThemeMode::Dark => Self {
                surface: rgba(0x181818ff),
                file_editor_surface: rgba(0x111111ff),
                file_editor_text: rgba(0xfcfcfcff),
                // Live ChatGPT CDP: color(srgb 0.156863 0.156863 0.156863 / 0.7).
                sidebar_surface: Rgba {
                    a: 0.7,
                    ..rgba(0x282828ff)
                },
                surface_under: rgba(0x222222ff),
                elevated: rgba(0x363636ff),
                // Resolved result of elevated-secondary/90 over #181818.
                model_picker_surface: rgba(0x2c2c2cff),
                // Captured opaque result of elevated-secondary/90 over the
                // canonical new-conversation background.
                project_dialog_surface: rgba(0x2b2b2bff),
                control: rgba(0x2d2d2dff),
                control_soft: rgba(0x2d2d2dff),
                // chat-reference: --color-background-primary-ghost-hover
                // rgba(255, 255, 255, 0.078), quantized to an 8-bit alpha.
                sidebar_hover: rgba(0xffffff14),
                // chat-reference: rgba(255, 255, 255, 0.082).
                sidebar_hairline: rgba(0xffffff15),
                sidebar_icon_muted: rgba(0xffffff7f),
                sidebar_title_text: rgba(0xdfdfdfd9),
                sidebar_text_muted: rgba(0xffffff7f),
                text: rgba(0xdfdfdfff),
                user_message_text: rgba(0xffffffff),
                user_message_surface: rgba(0x323232d9),
                sidebar_text: rgba(0xdfdfdfd9),
                text_secondary: rgba(0xc3c3c3ff),
                text_tertiary: rgba(0xffffff80),
                markdown_text: rgba(0xffffffff),
                markdown_link: rgba(0x2c67c5ff),
                // CDP: color-mix(in srgb, #2c67c5 80%, #ffffff 20%).
                markdown_file_link: rgba(0x5685d1ff),
                markdown_inline_code_text: rgba(0xffffffff),
                markdown_inline_code_surface: rgba(0xffffff1b),
                markdown_code_surface: rgba(0xffffff0d),
                markdown_code_header_surface: rgba(0x181818ff),
                markdown_code_border: rgba(0xffffff0b),
                markdown_syntax_comment: rgba(0xb9b9b9ff),
                markdown_syntax_keyword: rgba(0xf8a6c8ff),
                markdown_syntax_literal: rgba(0xf1a275ff),
                markdown_syntax_string: rgba(0x83d197ff),
                markdown_syntax_variable: rgba(0xb897f4ff),
                markdown_syntax_attribute: rgba(0xf9dc78ff),
                markdown_syntax_name: rgba(0x63a8f8ff),
                markdown_syntax_error: rgba(0xff8583ff),
                markdown_action_hover: rgba(0xffffff14),
                markdown_blockquote_border: rgba(0xffffff28),
                markdown_table_border_strong: rgba(0xffffff28),
                markdown_table_border_subtle: rgba(0xffffff0b),
                markdown_table_header_surface: rgba(0x00000000),
                markdown_rule: rgba(0xffffff28),
                // CDP: rgba(255, 255, 255, .05) and .157 respectively.
                command_surface: rgba(0xffffff0d),
                plan_surface: rgba(0x232323ff),
                plan_border: rgba(0xffffff15),
                plan_progress_surface: rgba(0x272727ff),
                command_border: rgba(0xffffff28),
                command_text: rgba(0xdfdfdfb3),
                command_muted: rgba(0x929292ff),
                home_mark: rgba(0x565656ff),
                border: rgba(0xffffff14),
                accent: rgba(0x83c3ffff),
                warning: rgba(0xff8549ff),
                effort: rgba(0xad7bf9ff),
                button: rgba(0xdfdfdfff),
                button_text: rgba(0x2d2d2dff),
                profile_menu_shadow: rgba(0x0000001f),
                // Settings cards sit on the #181818 page surface with a
                // slightly raised #232323 fill in ChatGPT's dark shell.
                settings_panel: rgba(0x232323ff),
                settings_switch_off: rgba(0xffffff1a),
                // CDP dark: [cmdk-root] #2d2d2d, border rgba(255,255,255,0.082),
                // ghost-hover rgba(255,255,255,0.08), description 0.498.
                chat_search_overlay: rgba(0x00000022),
                chat_search_surface: rgba(0x2d2d2dff),
                chat_search_border: rgba(0xffffff15),
                // CDP dark: --color-surface-elevated-secondary #2d2d2d at 90%.
                project_hover_surface: rgba(0x2d2d2de6),
                chat_search_text: rgba(0xffffffff),
                chat_search_row_hover: rgba(0xffffff14),
                chat_search_description: rgba(0xffffff7f),
                chat_search_hint_surface: rgba(0xffffff1a),
                edit_button_surface: rgba(0xffffff08),
                // CDP dark: `--color-codex-description` rgba(255, 255, 255, .498).
                navigation_rail_marker: rgba(0xffffff7f),
                // `bg-surface-elevated-secondary/95` (#2d2d2d) over the
                // #181818 pane, resolved so the card never depends on a
                // backdrop GPUI cannot sample.
                navigation_rail_surface: rgba(0x2c2c2cff),
                // CDP: rgba(255, 255, 255, 0.498).
                account_menu_shortcut: rgba(0xffffff7f),
                // The sidebar search is a little brighter than the page;
                // the management search has its own #2d2d2d fill.
                settings_search: rgba(0x2e2e2eff),
                settings_accent: rgba(0x539af8ff),
                settings_description: rgba(0xdfdfdfa6),
                settings_control: rgba(0x262626ff),
                settings_button: rgba(0x292929ff),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Theme, ThemeMode, UI_CJK_FALLBACK_FAMILY, UI_FONT_FAMILY, ui_font};

    #[test]
    fn global_ui_font_uses_the_chatgpt_macos_stack() {
        let font = ui_font();
        assert_eq!(font.family.as_ref(), UI_FONT_FAMILY);
        assert_eq!(font.weight, super::UI_BODY_FONT_WEIGHT);
        assert_eq!(
            font.fallbacks.expect("CJK fallback").fallback_list(),
            &[UI_CJK_FALLBACK_FAMILY.to_owned()]
        );
    }

    #[test]
    fn shell_surfaces_match_chatgpt_computed_colors() {
        let dark = Theme::for_mode(ThemeMode::Dark);
        let light = Theme::for_mode(ThemeMode::Light);

        assert_eq!(light.surface, gpui::rgba(0xffffffff));
        assert_eq!(dark.surface, gpui::rgba(0x181818ff));
        assert_eq!(light.settings_panel, light.surface);
        assert_eq!(dark.settings_panel, gpui::rgba(0x232323ff));
        for (theme, tint) in [(light, 255.0 / 255.0), (dark, 40.0 / 255.0)] {
            assert_eq!(theme.sidebar_surface.r, tint);
            assert_eq!(theme.sidebar_surface.g, tint);
            assert_eq!(theme.sidebar_surface.b, tint);
            assert_eq!(theme.sidebar_surface.a, 0.7);
        }
    }

    #[test]
    fn sidebar_foreground_alpha_matches_the_desktop_app() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let theme = Theme::for_mode(mode);
            assert!((theme.sidebar_text.a - 217.0 / 255.0).abs() < f32::EPSILON);
            assert!((theme.sidebar_title_text.a - 217.0 / 255.0).abs() < f32::EPSILON);
            assert!((theme.sidebar_text_muted.a - 127.0 / 255.0).abs() < f32::EPSILON);
            assert!((theme.sidebar_icon_muted.a - 127.0 / 255.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn native_sidebar_material_follows_focus_and_physical_window_size() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            assert_eq!(
                Theme::for_window(mode, true, 1440.0, 900.0, 2.0)
                    .sidebar_surface
                    .a,
                0.7
            );
            assert_eq!(
                Theme::for_window(mode, false, 1440.0, 900.0, 2.0)
                    .sidebar_surface
                    .a,
                1.0
            );
            assert_eq!(
                Theme::for_window(mode, true, 1920.0, 1080.0, 2.0)
                    .sidebar_surface
                    .a,
                1.0
            );
            assert_eq!(
                Theme::for_window(mode, true, 1080.0, 1920.0, 2.0)
                    .sidebar_surface
                    .a,
                1.0
            );
            assert_eq!(
                Theme::for_window(mode, true, 1920.0, 1079.0, 2.0)
                    .sidebar_surface
                    .a,
                0.7
            );
        }
        let dark = Theme::for_window(ThemeMode::Dark, false, 1440.0, 900.0, 2.0);
        assert!((dark.sidebar_surface.r - 34.0 / 255.0).abs() < 1e-6);
        let light = Theme::for_window(ThemeMode::Light, false, 1440.0, 900.0, 2.0);
        assert!((light.sidebar_surface.r - 253.0 / 255.0).abs() < 1e-6);
    }
}
