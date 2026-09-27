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
/// `px-5` around list content and `px-5` in the detail scroll body.
pub const PANE_PADDING: f32 = 20.0;
/// The reference's `[scrollbar-gutter:stable]`: content ends 11px before the
/// pane edge (592.84 → 581.84).
pub const SCROLLBAR_GUTTER: f32 = 11.0;
pub const ROW_RADIUS: f32 = 15.0;
/// The app-shell header toolbar sits 7px inside the list pane on the left and
/// 8px on the right (241 → 248, 825.84 → 833.84).
pub const LIST_TOOLBAR_INSET_LEFT: f32 = 7.0;
pub const LIST_TOOLBAR_INSET_RIGHT: f32 = 8.0;
/// `pt-panel pb-2` around the 32px search pill.
pub const LIST_SEARCH_ROW_HEIGHT: f32 = 60.0;
/// App-shell detail panel sizing (`app-shell:right-panel-width:v3`): the
/// panel spans `320 + ratio * (max - 320)` where `max = main - 352`.
/// The detail page column (`--thread-content-max-width`).
pub const DETAIL_CONTENT_MAX_WIDTH: f32 = 768.0;
/// From this detail width the header centers its tabs between the title and
/// the actions (`@container/app-shell-detail-panel (min-width: 900px)`).
pub const DETAIL_WIDE_HEADER: f32 = 900.0;
pub const DETAIL_MIN_WIDTH: f32 = 320.0;
pub const DETAIL_MAX_INSET: f32 = 352.0;
/// Radix menu geometry (`contentWidth="menuNarrow"`, `app-menu-item`).
pub const MENU_NARROW_WIDTH: f32 = 208.0;
pub const MENU_SUBMENU_WIDTH: f32 = 180.0;
pub const MENU_ROW_HEIGHT: f32 = 28.5625;
pub const MENU_ICON_SIZE: f32 = 16.0;
pub const MENU_CHEVRON_SIZE: f32 = 16.0;
pub const MENU_TEXT_SIZE: f32 = 13.0;
pub const MENU_LINE_HEIGHT: f32 = 18.5714;
/// Classic scroller thumb: 6px wide, 2px from the edge, inset 3px/6px.
pub const SCROLLBAR_THUMB_WIDTH: f32 = 6.0;
pub const SCROLLBAR_THUMB_INSET_RIGHT: f32 = 2.0;
pub const SCROLLBAR_TRACK_INSET_TOP: f32 = 3.0;
pub const SCROLLBAR_TRACK_INSET_BOTTOM: f32 = 6.0;
pub const SCROLLBAR_THUMB_MIN_LENGTH: f32 = 18.0;

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
    /// `hover:bg-primary-ghost-hover`: row and ghost-button hover
    /// (`rgba(26,28,31,0.055)` / `rgba(255,255,255,0.08)`).
    pub row_hover: Rgba,
    /// `bg-primary-soft-active`: the selected row (`rgba(26,28,31,0.047)` /
    /// `rgba(255,255,255,0.05)`).
    pub row_selected: Rgba,
    /// `text-purple`, `text-chart-red/green/yellow`: pull request state glyphs.
    pub purple: Rgba,
    pub chart_red: Rgba,
    pub chart_green: Rgba,
    pub chart_yellow: Rgba,
    /// The active filter's badge (`#0285FF` in both themes).
    pub filter_badge: Rgba,
    /// Loading skeleton bars.
    pub skeleton: Rgba,
    /// Menu chevrons (`text-tertiary`).
    pub menu_icon: Rgba,
    /// Classic scroller thumb, measured from the reference frame.
    pub scrollbar_thumb: Rgba,
    /// `border-subtle` under section headers (`rgba(26,28,31,0.049)` /
    /// `rgba(255,255,255,0.043)`).
    pub border_subtle: Rgba,
    /// `bg-primary-soft-alpha`: activity cards and their icon wells
    /// (`rgba(255,255,255,0.96)` / `rgba(255,255,255,0.032)`).
    pub soft_alpha: Rgba,
    /// `bg-surface-secondary` (`#f6f6f6` / `#141414`).
    pub surface_secondary: Rgba,
    /// Comment composers (`--composer-background-color` over the surface).
    pub composer_surface: Rgba,
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
    /// Tooltip surface and text: inverted in light (`rgb(26,28,31)` on
    /// white), the elevated surface in dark.
    pub tooltip_surface: Rgba,
    pub tooltip_text: Rgba,
    /// A selected app-shell tab (`bg-surface-elevated-secondary
    /// dark:bg-primary-ghost-hover`), opaque over the surface: white / 8%.
    pub tab_selected_surface: Rgba,
    pub menu_hover: Rgba,
    pub menu_shadow: Rgba,
    /// Diff colors: the computed styles of the reference diff viewer's rows
    /// (`[data-line-type]` code and number cells), converted from `lab()`.
    pub diff_added_surface: Rgba,
    pub diff_deleted_surface: Rgba,
    pub diff_added_gutter: Rgba,
    pub diff_deleted_gutter: Rgba,
    /// Line numbers and the 4px change bars of changed rows.
    pub diff_added_text: Rgba,
    pub diff_deleted_text: Rgba,
    /// Word highlights (`[data-diff-span]`).
    pub diff_added_word: Rgba,
    pub diff_deleted_word: Rgba,
    pub diff_gutter_text: Rgba,
    pub diff_context_text: Rgba,
    pub diff_expander_surface: Rgba,
    pub diff_header_surface: Rgba,
    /// File-tree git status accents (`--trees-git-*-color`) measured from the
    /// reference rows: modified `#923b0f` / `#ff8549`, added `#00a240` /
    /// `#40c977`.
    /// The file tree panel's 0.5px ring.
    pub tree_panel_ring: Rgba,
    pub status_modified: Rgba,
    pub status_added: Rgba,
    /// `+x` and `-y` counts.
    pub additions_text: Rgba,
    pub deletions_text: Rgba,
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
                gpui::rgba(0x1a1c1f0e)
            } else {
                gpui::rgba(0xffffff14)
            },
            row_selected: if is_light {
                gpui::rgba(0x1a1c1f0c)
            } else {
                gpui::rgba(0xffffff0d)
            },
            purple: light(is_light, 0x924ff7ff, 0xad7bf9ff),
            chart_red: light(is_light, 0xe02e2aff, 0xff6764ff),
            chart_green: light(is_light, 0x00a240ff, 0x40c977ff),
            chart_yellow: light(is_light, 0xffc300ff, 0xffd240ff),
            filter_badge: gpui::rgba(0x0285ffff),
            skeleton: if is_light {
                gpui::rgba(0x1a1c1f0d)
            } else {
                gpui::rgba(0xffffff0d)
            },
            menu_icon: if is_light {
                gpui::rgba(0x1a1c1f7e)
            } else {
                gpui::rgba(0xffffff7f)
            },
            scrollbar_thumb: light(is_light, 0xedededff, 0x2b2b2bff),
            soft_alpha: if is_light {
                gpui::rgba(0xfffffff5)
            } else {
                gpui::rgba(0xffffff08)
            },
            surface_secondary: light(is_light, 0xf6f6f6ff, 0x141414ff),
            composer_surface: light(is_light, 0xffffffff, 0x363636ff),
            border_subtle: if is_light {
                gpui::rgba(0x1a1c1f0d)
            } else {
                gpui::rgba(0xffffff0b)
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
            tooltip_surface: light(is_light, 0x1a1c1fff, 0x2d2d2dff),
            tooltip_text: light(is_light, 0xffffffff, 0xdfdfdfff),
            tab_selected_surface: light(is_light, 0xffffffff, 0x2a2a2aff),
            // `data-highlighted`: rgba(26,28,31,0.055) / rgba(255,255,255,0.08).
            menu_hover: if is_light {
                gpui::rgba(0x1a1c1f0e)
            } else {
                gpui::rgba(0xffffff14)
            },
            menu_shadow: gpui::rgba(0x0000001f),
            // The reference `lab()` row colors in sRGB (light / dark).
            diff_added_surface: light(is_light, 0xe7f4e7ff, 0x1f3124ff),
            diff_deleted_surface: light(is_light, 0xfce6e2ff, 0x3b1f1aff),
            diff_added_gutter: light(is_light, 0xedf7edff, 0x132017ff),
            diff_deleted_gutter: light(is_light, 0xfdece9ff, 0x28130eff),
            diff_added_text: light(is_light, 0x00a240ff, 0x40c977ff),
            diff_deleted_text: light(is_light, 0xba2623ff, 0xfa423eff),
            // `rgb(from … / .15)` in light, `/ .2` in dark.
            diff_added_word: light(is_light, 0x00a24026, 0x40c97733),
            diff_deleted_word: light(is_light, 0xba262326, 0xfa423e33),
            diff_gutter_text: light(is_light, 0x585858ff, 0xa1a1a1ff),
            diff_context_text: light(is_light, 0x0d0d0dff, 0xfcfcfcff),
            diff_expander_surface: light(is_light, 0xf3f3f3ff, 0x2f2f2fff),
            // The reference file header sits on the pane surface.
            diff_header_surface: light(is_light, 0xffffffff, 0x181818ff),
            tree_panel_ring: light(is_light, 0x1a1c1f1e, 0xffffff28),
            status_modified: light(is_light, 0x923b0fff, 0xff8549ff),
            status_added: light(is_light, 0x00a240ff, 0x40c977ff),
            // Detail branch row, measured from the reference DOM:
            // light rgb(0,162,64) / rgb(186,38,35), dark rgb(64,201,119) / rgb(250,66,62).
            additions_text: light(is_light, 0x00a240ff, 0x40c977ff),
            deletions_text: light(is_light, 0xba2623ff, 0xfa423eff),
            focus_ring: light(is_light, 0x0a84ffff, 0x3b9effff),
            warning: light(is_light, 0xba2623ff, 0xff8583ff),
        }
    }
}
