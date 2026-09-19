//! Tokens and geometry for the Pull Requests page.
//!
//! Every value is read from the running ChatGPT/Codex desktop application over
//! CDP (`scripts/cdp_capture_pull_requests.mjs`) and recorded in
//! `artifacts/pull-requests-reference/`. Light values are listed first, then
//! the `data-theme="dark"` value the application resolves for the same node.

use gpui::Rgba;

use crate::theme::ThemeMode;

/// Toolbar height shared by the list and detail panes (`h-toolbar`).
pub const TOOLBAR_HEIGHT: f32 = 46.0;
/// The list column width measured at a 1440 px window (276 → 793.92).
pub const LIST_PANE_WIDTH: f32 = 518.0;
/// `px-5` around list content and `px-5` in the detail scroll body.
pub const PANE_PADDING: f32 = 20.0;
/// The reference reserves a scrollbar gutter on the right of the list body, so
/// rows end 11px before the pane edge (794 - 762.92 in the reference layout).
pub const SCROLLBAR_GUTTER: f32 = 11.0;
pub const ROW_HEIGHT: f32 = 62.0;
pub const ROW_RADIUS: f32 = 15.0;

fn light(is_light: bool, light: u32, dark: u32) -> Rgba {
    let value = if is_light { light } else { dark };
    gpui::rgba(value)
}

#[derive(Clone, Copy)]
pub struct PrTheme {
    /// `bg-surface`: the pane background (light `#ffffff`, dark `rgb(24,24,24)`).
    pub surface: Rgba,
    /// `text-primary` (light `rgb(26,28,31)`, dark `rgb(223,223,223)`).
    pub text: Rgba,
    /// `text-secondary` (light `rgba(26,28,31,0.494)`, dark `rgba(255,255,255,0.498)`).
    pub text_muted: Rgba,
    /// `bg-secondary-soft-alpha` at 5%: selected tabs, `Chat`, icon buttons.
    pub control: Rgba,
    /// The same control at 10%, used for hover and the active filter button.
    pub control_hover: Rgba,
    /// `border-primary` outline (`rgba(26,28,31,0.08)` / `rgba(255,255,255,0.082)`).
    pub border: Rgba,
    /// Row hover / selection fill (`rgba(26,28,31,0.047)`).
    pub row_hover: Rgba,
    /// Search-field fill and outline.
    pub field_surface: Rgba,
    pub field_border: Rgba,
    /// Muted icon foreground (`rgba(26,28,31,0.65)`).
    pub icon_muted: Rgba,
    /// Primary filled button (light `rgb(26,28,31)` on white text).
    pub inverted_surface: Rgba,
    pub inverted_text: Rgba,
    /// Popover surface: `rgba(255,255,255,0.9)` / `rgba(45,45,45,0.9)`.
    pub menu_surface: Rgba,
    /// Opaque popover surface used by the reviewer picker (the reference
    /// renders that panel solid: white / `rgb(45,45,45)`).
    pub popover_surface: Rgba,
    /// Activity-card surface: the reference lifts cards off the pane with
    /// `rgb(31,31,31)` in dark mode (white in light mode).
    pub card_surface: Rgba,
    pub menu_hover: Rgba,
    pub menu_shadow: Rgba,
    /// Diff colors measured from the reference diff viewer.
    pub diff_added_surface: Rgba,
    pub diff_deleted_surface: Rgba,
    pub diff_added_emphasis: Rgba,
    pub diff_deleted_emphasis: Rgba,
    pub diff_added_text: Rgba,
    pub diff_deleted_text: Rgba,
    pub diff_gutter_text: Rgba,
    pub diff_context_text: Rgba,
    pub diff_expander_surface: Rgba,
    pub diff_header_surface: Rgba,
    /// File-tree status accents: `M` badge (light `rgb(146,59,15)`,
    /// dark `rgb(239,140,87)`) and `A` badge (light `rgb(72,160,77)`,
    /// dark `rgb(107,198,127)`), measured from the reference tree rows.
    pub status_modified: Rgba,
    pub status_added: Rgba,
    /// `+x` and `-y` counts.
    pub additions_text: Rgba,
    pub deletions_text: Rgba,
    /// Merged-state accent used by the activity feed (`rgb(137,83,239)`).
    pub merged_accent: Rgba,
    /// Syntax colors of the reference diff viewer, measured from its rendered
    /// tokens (`scripts/audit_pull_requests_colors.py` records the probe).
    pub syntax_plain: Rgba,
    pub syntax_keyword: Rgba,
    pub syntax_type: Rgba,
    pub syntax_name: Rgba,
    pub syntax_string: Rgba,
    pub syntax_operator: Rgba,
    pub syntax_comment: Rgba,
    pub syntax_error: Rgba,
    /// Focus ring used by text fields and editors.
    pub focus_ring: Rgba,
    /// Warning copy (failed loads and destructive outcomes).
    pub warning: Rgba,
}

impl PrTheme {
    pub fn for_mode(mode: ThemeMode) -> Self {
        let is_light = mode == ThemeMode::Light;
        Self {
            surface: light(is_light, 0xffffffff, 0x181818ff),
            text: light(is_light, 0x1a1c1fff, 0xdfdfdfff),
            text_muted: if is_light {
                gpui::rgba(0x1a1c1f7e)
            } else {
                gpui::rgba(0xffffff7f)
            },
            control: if is_light {
                gpui::rgba(0x1a1c1f0d)
            } else {
                gpui::rgba(0xdfdfdf0d)
            },
            control_hover: if is_light {
                gpui::rgba(0x1a1c1f1a)
            } else {
                gpui::rgba(0xdfdfdf1a)
            },
            border: if is_light {
                gpui::rgba(0x1a1c1f14)
            } else {
                gpui::rgba(0xffffff15)
            },
            row_hover: if is_light {
                gpui::rgba(0x1a1c1f0c)
            } else {
                gpui::rgba(0xdfdfdf0c)
            },
            field_surface: if is_light {
                gpui::rgba(0xffffffdc)
            } else {
                gpui::rgba(0x2d2d2dff)
            },
            field_border: if is_light {
                gpui::rgba(0x1a1c1f1e)
            } else {
                gpui::rgba(0xffffff28)
            },
            icon_muted: if is_light {
                gpui::rgba(0x1a1c1fa6)
            } else {
                gpui::rgba(0xdfdfdfa6)
            },
            inverted_surface: light(is_light, 0x1a1c1fff, 0xdfdfdfff),
            inverted_text: light(is_light, 0xffffffff, 0x2d2d2dff),
            menu_surface: if is_light {
                gpui::rgba(0xffffffe6)
            } else {
                gpui::rgba(0x2d2d2de6)
            },
            popover_surface: light(is_light, 0xffffffff, 0x2d2d2dff),
            card_surface: light(is_light, 0xffffffff, 0x1f1f1fff),
            menu_hover: if is_light {
                gpui::rgba(0x1a1c1f0e)
            } else {
                gpui::rgba(0xffffff14)
            },
            menu_shadow: gpui::rgba(0x0000001f),
            // Measured from the reference diff viewer (light: #e9f4e8 / #f8e7e3
            // rows with #eff7ee / #faedea number cells; dark: the same tones on
            // the dark surface).
            diff_added_surface: light(is_light, 0xe9f4e8ff, 0x233125ff),
            diff_deleted_surface: light(is_light, 0xf8e7e3ff, 0x38201cff),
            diff_added_emphasis: light(is_light, 0xeff7eeff, 0x162017ff),
            diff_deleted_emphasis: light(is_light, 0xfaedeaff, 0x25140fff),
            diff_added_text: light(is_light, 0x00a240ff, 0x00c853ff),
            diff_deleted_text: light(is_light, 0xba2623ff, 0xff6b63ff),
            diff_gutter_text: light(is_light, 0x585858ff, 0xa1a1a1ff),
            diff_context_text: light(is_light, 0x0d0d0dff, 0xfcfcfcff),
            diff_expander_surface: light(is_light, 0xf4f4f4ff, 0x2f2f2fff),
            // The reference file header sits on the pane surface.
            diff_header_surface: light(is_light, 0xffffffff, 0x181818ff),
            status_modified: light(is_light, 0x923b0fff, 0xef8c57ff),
            status_added: light(is_light, 0x48a04dff, 0x6bc67fff),
            // Detail branch row, measured from the reference DOM:
            // light rgb(0,162,64) / rgb(186,38,35), dark rgb(64,201,119) / rgb(250,66,62).
            additions_text: light(is_light, 0x00a240ff, 0x40c977ff),
            deletions_text: light(is_light, 0xba2623ff, 0xfa423eff),
            merged_accent: light(is_light, 0x8953efff, 0xb18cffff),
            // Measured token colors of the reference diff viewer. Light values
            // are the rendered `rgb(...)` of its tokens; dark values are the
            // same tokens with the app switched to dark.
            syntax_plain: light(is_light, 0x0d0d0dff, 0xfcfcfcff),
            syntax_keyword: light(is_light, 0xd53538ff, 0xf67576ff),
            syntax_type: light(is_light, 0xbd5800ff, 0xfa994cff),
            syntax_name: light(is_light, 0x751ed9ff, 0xb06dffff),
            syntax_string: light(is_light, 0x008809ff, 0x85df7bff),
            syntax_operator: light(is_light, 0x0071eaff, 0x6dcbf4ff),
            syntax_comment: light(is_light, 0x666666ff, 0x999999ff),
            syntax_error: light(is_light, 0xba2623ff, 0xff6b63ff),
            focus_ring: light(is_light, 0x0a84ffff, 0x3b9effff),
            warning: light(is_light, 0xba2623ff, 0xff8583ff),
        }
    }
}
