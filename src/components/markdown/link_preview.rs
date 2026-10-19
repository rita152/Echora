//! The card ChatGPT opens under a hovered web link, and the dashed underline
//! the link shows meanwhile.
//!
//! Chat Markdown is rendered statelessly, so the card's state lives here, the
//! way the text selection keeps its own: a link reports pointer changes, and
//! every change that shows, hides or fills a card refreshes the window, which
//! re-renders the cached conversation view.

use std::{cell::RefCell, collections::HashMap, path::PathBuf, time::Duration};

use gpui::{
    App, Bounds, BoxShadow, FontWeight, MouseButton, ObjectFit, Pixels, Rgba, SharedString,
    Subscription, TextLayout, Window, canvas, deferred, div, point, prelude::*, px, rgba, size,
};

use crate::{components::icons::icon, link_preview::PageMetadata, theme::Theme};

/// Radix tooltip delay the reference gives its link card (measured ~0.72 s,
/// including polling).
const OPEN_DELAY: Duration = Duration::from_millis(700);
/// The card is gone ~0.1 s after the pointer leaves both link and card.
const CLOSE_DELAY: Duration = Duration::from_millis(100);
const CARD_WIDTH: f32 = 360.0;
/// `max-h-[min(--spacing(80), …)]`.
const CARD_MAX_HEIGHT: f32 = 320.0;
const CARD_RADIUS: f32 = 15.0;
/// `px-3.5` on the header and the text block.
const CARD_PADDING_X: f32 = 14.0;
/// `h-toolbar`.
const HEADER_HEIGHT: f32 = 46.0;
/// `max-h-48` on the preview image.
const IMAGE_MAX_HEIGHT: f32 = 192.0;
/// Radix `sideOffset` between the link's text box and the card's wrapper; the
/// card itself keeps a further `m-px`.
const SIDE_OFFSET: f32 = 4.0;
const CARD_MARGIN: f32 = 1.0;

#[derive(Clone)]
enum PageState {
    Loading,
    Ready {
        metadata: PageMetadata,
        image: Option<(PathBuf, Option<(u32, u32)>)>,
    },
    Failed,
}

#[derive(Default)]
struct PreviewState {
    /// The link under the pointer.
    hovered_link: Option<u64>,
    /// Whether the pointer is over the open card.
    card_hovered: bool,
    /// The link whose card is showing.
    open: Option<u64>,
    open_serial: u64,
    close_serial: u64,
    escape: Option<Subscription>,
    pages: HashMap<String, PageState>,
}

thread_local! {
    static STATE: RefCell<PreviewState> = RefCell::new(PreviewState::default());
}

/// Colours of the card, resolved from the shell theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LinkPreviewPalette {
    surface: Rgba,
    ring: Rgba,
    divider: Rgba,
    text: Rgba,
    secondary: Rgba,
}

impl LinkPreviewPalette {
    pub(super) fn new(theme: Theme) -> Self {
        Self {
            surface: theme.link_preview_surface,
            ring: theme.link_preview_ring,
            divider: theme.link_preview_divider,
            text: theme.text,
            // `text-secondary`: the primary foreground at 65%.
            secondary: Rgba {
                a: 0.65,
                ..theme.text
            },
        }
    }
}

/// Only http(s) links have a page to preview.
pub(super) fn previews(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Pointer entered or left the link `key` pointing at `url`.
pub(super) fn link_hovered(key: u64, url: &str, hovered: bool, window: &mut Window, cx: &mut App) {
    let serial = STATE.with_borrow_mut(|state| {
        if hovered {
            state.hovered_link = Some(key);
            if state.open == Some(key) {
                return None;
            }
            state.open_serial += 1;
            Some(state.open_serial)
        } else {
            if state.hovered_link == Some(key) {
                state.hovered_link = None;
            }
            None
        }
    });
    if !hovered {
        schedule_close(window, cx);
        return;
    }
    let Some(serial) = serial else {
        return;
    };
    // A hover card does not take focus. Its Escape handler must run before
    // the focused composer or transcript handles the key, even though the
    // card itself is outside that element's keyboard dispatch path.
    let window_id = window.window_handle().window_id();
    let escape = cx.intercept_keystrokes(move |event, window, cx| {
        if window.window_handle().window_id() == window_id && event.keystroke.key == "escape" {
            let open = STATE.with_borrow(|state| state.open.is_some());
            dismiss(window);
            if open {
                cx.stop_propagation();
            }
        }
    });
    STATE.with_borrow_mut(|state| state.escape = Some(escape));
    let url = url.to_owned();
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(OPEN_DELAY).await;
            let _ = cx.update(|window, cx| {
                let opened = STATE.with_borrow_mut(|state| {
                    let current = state.open_serial == serial && state.hovered_link == Some(key);
                    if current {
                        state.open = Some(key);
                        state.card_hovered = false;
                    }
                    current
                });
                if opened {
                    load_page(url, window, cx);
                    window.refresh();
                }
            });
        })
        .detach();
}

/// A click on the link (or Escape) dismisses its card at once, as Radix
/// closes a tooltip on pointer down.
pub(super) fn dismiss(window: &mut Window) {
    let closed = STATE.with_borrow_mut(|state| {
        state.open_serial += 1;
        state.card_hovered = false;
        state.escape = None;
        state.open.take().is_some()
    });
    if closed {
        window.refresh();
    }
}

fn card_hovered(hovered: bool, window: &mut Window, cx: &mut App) {
    STATE.with_borrow_mut(|state| state.card_hovered = hovered);
    if !hovered {
        schedule_close(window, cx);
    }
}

fn schedule_close(window: &mut Window, cx: &mut App) {
    let serial = STATE.with_borrow_mut(|state| {
        state.close_serial += 1;
        state.close_serial
    });
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(CLOSE_DELAY).await;
            let _ = cx.update(|window, _| {
                let closed = STATE.with_borrow_mut(|state| {
                    let stale = state.close_serial == serial
                        && state.open.is_some()
                        && state.hovered_link != state.open
                        && !state.card_hovered;
                    if stale {
                        state.open = None;
                        state.escape = None;
                    }
                    stale
                });
                if closed {
                    window.refresh();
                }
            });
        })
        .detach();
}

fn load_page(url: String, window: &mut Window, cx: &mut App) {
    let known = STATE.with_borrow_mut(|state| {
        let known = state
            .pages
            .get(&url)
            .is_some_and(|page| !matches!(page, PageState::Failed));
        if !known {
            state.pages.insert(url.clone(), PageState::Loading);
        }
        known
    });
    if known {
        return;
    }
    let fetch = url.clone();
    let task = cx.background_executor().spawn(async move {
        let metadata = crate::link_preview::fetch_metadata(&fetch)?;
        let image = metadata
            .image_url
            .as_deref()
            .and_then(|image| crate::link_preview::fetch_image(image).ok())
            .map(|path| {
                let dimensions = crate::media::read_image_dimensions(&path).ok().flatten();
                (path, dimensions)
            });
        anyhow::Ok(PageState::Ready { metadata, image })
    });
    window
        .spawn(cx, async move |cx| {
            let page = task.await.unwrap_or(PageState::Failed);
            let _ = cx.update(|window, _| {
                let showing = STATE.with_borrow_mut(|state| {
                    state.pages.insert(url, page);
                    state.open.is_some()
                });
                if showing {
                    window.refresh();
                }
            });
        })
        .detach();
}

/// The card for link `key`, when it is open: placed under the link's text box,
/// whose bottom lies `text_bottom` below the link element's top.
pub(super) fn card(
    key: u64,
    url: &str,
    github: bool,
    text_bottom: f32,
    palette: LinkPreviewPalette,
) -> Option<gpui::AnyElement> {
    let page = STATE.with_borrow(|state| {
        (state.open == Some(key))
            .then(|| state.pages.get(url).cloned().unwrap_or(PageState::Loading))
    })?;
    let host = crate::link_preview::display_host(url).unwrap_or_else(|| url.to_owned());
    let open_url = url.to_owned();
    let header = div()
        .h(px(HEADER_HEIGHT))
        .flex_none()
        .px(px(CARD_PADDING_X))
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            icon(
                if github {
                    "markdown-github"
                } else {
                    "markdown-link-globe"
                },
                palette.secondary.into(),
            )
            .size(px(16.0))
            .flex_none(),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .truncate()
                .text_size(px(crate::typography::ui_size(12.0)))
                .line_height(px(16.0))
                .text_color(palette.secondary)
                .child(host),
        )
        .child(
            div()
                .id("markdown-link-preview-open")
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.0))
                .cursor_default()
                .debug_selector(|| "MARKDOWN_LINK_PREVIEW_OPEN".to_owned())
                .text_size(px(crate::typography::ui_size(12.0)))
                .line_height(px(16.0))
                .text_color(palette.text)
                .child(crate::i18n::text("打开"))
                .child(
                    icon("pr-open-browser", palette.text.into())
                        .size(px(16.0))
                        .flex_none(),
                )
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    dismiss(window);
                    super::open_web_link(&open_url, window, cx);
                }),
        );
    let body = match page {
        PageState::Ready { metadata, image } if !metadata.is_empty() => {
            let mut body = div()
                .id("markdown-link-preview-body")
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .border_t(px(1.0))
                .border_color(palette.divider);
            if let Some((path, dimensions)) = image {
                let height = dimensions
                    .filter(|(width, height)| *width > 0 && *height > 0)
                    .map_or(IMAGE_MAX_HEIGHT, |(width, height)| {
                        (CARD_WIDTH * height as f32 / width as f32).min(IMAGE_MAX_HEIGHT)
                    });
                body = body.child(
                    gpui::img(path)
                        .flex_none()
                        .w_full()
                        .h(px(height))
                        .object_fit(ObjectFit::Cover)
                        .border_b(px(1.0))
                        .border_color(palette.divider),
                );
            }
            // A scroll container lays its content out at max-content width,
            // so the column takes the card's inner width explicitly.
            let mut text = div()
                .w(px(CARD_WIDTH - 2.0 * CARD_PADDING_X))
                .flex()
                .flex_col()
                .gap(px(4.0));
            if let Some(title) = metadata.title {
                text = text.child(
                    div()
                        .text_size(px(crate::typography::ui_size(13.0)))
                        .line_height(px(17.875))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(palette.text)
                        .line_clamp(2)
                        .text_ellipsis()
                        .child(title),
                );
            }
            if let Some(description) = metadata.description {
                text = text.child(
                    div()
                        .text_size(px(crate::typography::ui_size(13.0)))
                        .line_height(px(17.875))
                        .text_color(palette.secondary)
                        .line_clamp(2)
                        .text_ellipsis()
                        .child(description),
                );
            }
            Some(
                body.child(
                    div()
                        .flex_none()
                        .px(px(CARD_PADDING_X))
                        .py(px(16.0))
                        .flex()
                        .items_start()
                        .gap(px(10.0))
                        .child(text),
                ),
            )
        }
        _ => None,
    };
    // `ring-[0.5px] ring-border` twice, then `shadow-xl-spread`'s drop.
    let ring = BoxShadow::new(px(0.0), px(0.0), palette.ring.into()).spread_radius(px(0.5));
    let card = div()
        .id("markdown-link-preview")
        .occlude()
        .w(px(CARD_WIDTH))
        .max_h(px(CARD_MAX_HEIGHT))
        .m(px(CARD_MARGIN))
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(CARD_RADIUS))
        .bg(palette.surface)
        .shadow(vec![
            ring.clone(),
            ring,
            BoxShadow::new(px(0.0), px(8.0), rgba(0x0000001f).into())
                .blur_radius(px(16.0))
                .spread_radius(px(-4.0)),
        ])
        .text_color(palette.text)
        .font_weight(FontWeight::NORMAL)
        .cursor_default()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(|_, _, cx| cx.stop_propagation())
        .on_hover(|hovered, window, cx| card_hovered(*hovered, window, cx))
        .debug_selector(|| "MARKDOWN_LINK_PREVIEW".to_owned())
        .child(header)
        .children(body);
    Some(
        div()
            .absolute()
            .left_0()
            .top(px(text_bottom + SIDE_OFFSET))
            .child(
                deferred(
                    gpui::anchored()
                        .snap_to_window_with_margin(px(8.0))
                        .child(card),
                )
                .with_priority(1),
            )
            .into_any_element(),
    )
}

/// Chromium's dashed text decoration: dashes three times the line's thickness,
/// gaps of about twice it, stretched so that a line starts and ends on a dash.
fn dash_segments(start: f32, length: f32, thickness: f32) -> Vec<(f32, f32)> {
    if length <= 0.0 {
        return Vec::new();
    }
    let dash = thickness * 3.0;
    let gap = thickness * 2.0;
    if length <= dash {
        return vec![(start, length)];
    }
    let count = ((length + gap) / (dash + gap)).round().max(2.0);
    let gap = (length - count * dash) / (count - 1.0);
    (0..count as usize)
        .map(|index| (start + index as f32 * (dash + gap), dash))
        .collect()
}

/// The hover underline of a link: `text-decoration: underline dashed` at
/// `0.5px`, `text-underline-offset: 2px`, drawn under every wrapped line of
/// `layout`. GPUI only draws solid or wavy underlines, so the dashes are
/// painted here, and only while the link's group is hovered.
pub(super) fn dashed_underline(layout: TextLayout, color: Rgba, group: SharedString) -> gpui::Div {
    div()
        .absolute()
        .inset_0()
        .invisible()
        .group_hover(group, |style| style.visible())
        .child(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let thickness = 0.5f32.max(1.0 / window.scale_factor());
                    for bounds in underline_runs(&layout, thickness) {
                        window.paint_quad(gpui::fill(bounds, color));
                    }
                },
            )
            .size_full(),
        )
}

fn underline_runs(layout: &TextLayout, thickness: f32) -> Vec<Bounds<Pixels>> {
    let origin = layout.bounds().origin;
    let line_height = layout.line_height();
    let mut runs = Vec::new();
    let mut top = origin.y;
    for line in layout.line_layouts() {
        let unwrapped = &line.unwrapped_layout;
        let baseline =
            (line_height - unwrapped.ascent - unwrapped.descent) / 2.0 + unwrapped.ascent;
        let glyph_x = |run_ix: usize, glyph_ix: usize| {
            unwrapped
                .runs
                .get(run_ix)
                .and_then(|run| run.glyphs.get(glyph_ix))
                .map_or(unwrapped.width, |glyph| glyph.position.x)
        };
        let mut starts = vec![px(0.0)];
        starts.extend(
            line.wrap_boundaries()
                .iter()
                .map(|boundary| glyph_x(boundary.run_ix, boundary.glyph_ix)),
        );
        for (row, start) in starts.iter().enumerate() {
            let end = starts.get(row + 1).copied().unwrap_or(unwrapped.width);
            // Glyphs sit on the floored baseline (see the vendored line
            // painter); the offset is measured from where they are drawn.
            let y = (top + line_height * row as f32 + baseline).floor() + px(2.0);
            for (x, width) in dash_segments(0.0, f32::from(end - *start), thickness) {
                runs.push(Bounds::new(
                    point(origin.x + px(x), y),
                    size(px(width), px(thickness)),
                ));
            }
        }
        top += line_height * starts.len() as f32;
    }
    runs
}

/// Gives `url` a loaded page, so a test card never fetches over the network.
#[cfg(test)]
pub(super) fn seed_page(url: &str, metadata: PageMetadata) {
    STATE.with_borrow_mut(|state| {
        state.pages.insert(
            url.to_owned(),
            PageState::Ready {
                metadata,
                image: None,
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeMode;

    struct LinkFixture {
        focus: gpui::FocusHandle,
        opened: Vec<String>,
    }

    impl Render for LinkFixture {
        fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
            div()
                .track_focus(&self.focus)
                .on_action(cx.listener(
                    |this, link: &crate::components::browser::OpenLinkInBrowser, _, cx| {
                        this.opened.push(link.url.clone());
                        cx.stop_propagation();
                    },
                ))
                .size_full()
                .p(px(40.0))
                .child(super::super::render_assistant_markdown(
                    "依据：[仓库 README](https://example.invalid/readme)。",
                    Theme::for_mode(ThemeMode::Dark),
                    "link-preview-test",
                ))
        }
    }

    /// The card opens after the delay, stays while the pointer is on it, and
    /// closes once the pointer leaves both, on Escape, and on a click.
    #[gpui::test]
    fn the_card_follows_the_pointer_like_the_reference(cx: &mut gpui::TestAppContext) {
        seed_page(
            "https://example.invalid/readme",
            PageMetadata {
                title: Some("Example".to_owned()),
                ..PageMetadata::default()
            },
        );
        let handle = cx.add_window(|window, cx| {
            let focus = cx.focus_handle();
            focus.focus(window, cx);
            LinkFixture {
                focus,
                opened: Vec::new(),
            }
        });
        let mut visual = gpui::VisualTestContext::from_window(handle.into(), cx);
        let card = |visual: &mut gpui::VisualTestContext| {
            visual.run_until_parked();
            visual.update(|window, cx| window.draw(cx).clear(cx));
            visual.debug_bounds("MARKDOWN_LINK_PREVIEW")
        };
        assert_eq!(card(&mut visual), None);
        let link = visual
            .debug_bounds("MARKDOWN_LINK")
            .expect("the link records its bounds");
        let none = gpui::Modifiers::default();
        let away = point(px(5.0), px(300.0));

        visual.simulate_mouse_move(link.center(), None, none);
        visual.executor().advance_clock(OPEN_DELAY / 2);
        assert_eq!(card(&mut visual), None, "the card waits for its delay");
        visual.executor().advance_clock(OPEN_DELAY);
        let opened = card(&mut visual).expect("the card opens after the delay");
        assert_eq!(f32::from(opened.size.width), CARD_WIDTH);
        assert!(
            opened.origin.y > link.origin.y,
            "the card hangs below the link"
        );

        // Down into the card: it stays.
        visual.simulate_mouse_move(opened.center(), None, none);
        visual.executor().advance_clock(CLOSE_DELAY * 3);
        assert!(
            card(&mut visual).is_some(),
            "the card stays under the pointer"
        );
        visual.simulate_click(opened.center(), none);
        assert!(card(&mut visual).is_some(), "the card body is not a link");
        visual.update(|window, cx| {
            assert!(
                window
                    .root::<LinkFixture>()
                    .flatten()
                    .unwrap()
                    .read(cx)
                    .opened
                    .is_empty()
            );
        });

        visual.simulate_mouse_move(away, None, none);
        visual.executor().advance_clock(CLOSE_DELAY * 2);
        assert_eq!(card(&mut visual), None, "leaving both closes the card");

        visual.simulate_mouse_move(link.center(), None, none);
        visual.executor().advance_clock(OPEN_DELAY * 2);
        assert!(card(&mut visual).is_some());
        visual.simulate_keystrokes("escape");
        assert_eq!(card(&mut visual), None, "Escape closes the card");

        visual.simulate_mouse_move(away, None, none);
        visual.simulate_mouse_move(link.center(), None, none);
        visual.executor().advance_clock(OPEN_DELAY * 2);
        assert!(card(&mut visual).is_some());
        let open = visual.debug_bounds("MARKDOWN_LINK_PREVIEW_OPEN").unwrap();
        visual.simulate_click(open.center(), none);
        assert_eq!(card(&mut visual), None, "Open dismisses the card");
        visual.update(|window, cx| {
            let fixture = window.root::<LinkFixture>().flatten().unwrap();
            assert!(
                fixture.read(cx).focus.is_focused(window),
                "hover never takes focus"
            );
            assert_eq!(fixture.read(cx).opened, ["https://example.invalid/readme"]);
        });

        visual.simulate_mouse_move(away, None, none);
        visual.executor().advance_clock(CLOSE_DELAY * 2);
        visual.simulate_mouse_move(link.center(), None, none);
        visual.executor().advance_clock(OPEN_DELAY * 2);
        assert!(card(&mut visual).is_some());
        visual.simulate_click(link.center(), none);
        assert_eq!(
            card(&mut visual),
            None,
            "a click on the link closes the card"
        );
    }

    #[test]
    fn dashes_start_and_end_on_the_line_ends() {
        // The reference's 89.4px label at a 1px decoration: 18 dashes of 3px.
        let dashes = dash_segments(0.0, 89.4, 1.0);
        assert_eq!(dashes.len(), 18);
        assert_eq!(dashes[0], (0.0, 3.0));
        let (last_x, last_width) = *dashes.last().unwrap();
        assert!((last_x + last_width - 89.4).abs() < 1e-3);
        let gap = dashes[1].0 - dashes[0].1;
        assert!((gap - 2.08).abs() < 0.01, "gap {gap}");
    }

    #[test]
    fn a_short_label_is_one_dash() {
        assert_eq!(dash_segments(0.0, 1.0, 0.5), vec![(0.0, 1.0)]);
        assert!(dash_segments(0.0, 0.0, 0.5).is_empty());
    }

    #[test]
    fn only_web_links_preview() {
        assert!(previews("https://github.com/a"));
        assert!(previews("http://example.com"));
        assert!(!previews("/Users/a/file.rs"));
        assert!(!previews("mailto:a@b.c"));
    }
}
