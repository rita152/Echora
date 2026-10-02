//! Capture-only browser states (`--browser-state=`), so the panel can be
//! compared with the reference without driving it by hand.

use std::path::PathBuf;

use gpui::{Context, Window};

use super::{BrowserPanel, PanelMenu};

const DEFAULT_URL: &str = "https://example.net/";
/// What the address capture types: the reference's "exa" → "example.net".
const TYPED_ADDRESS: &str = "exa";
const FIND_QUERY: &str = "domain";

#[derive(Default)]
pub(super) struct CaptureState {
    typed: Option<String>,
    find: bool,
    after_load: Option<String>,
    snapshot: Option<PathBuf>,
}

impl BrowserPanel {
    /// `new-tab`, `address`, `page`, `menu`, `clear-data`, `find`, `zoom`,
    /// `tab-menu`, `downloads` or `error`; `snapshot` also writes the native
    /// page to a PNG once it has loaded.
    pub fn capture_state(
        &mut self,
        state: &str,
        url: Option<String>,
        snapshot: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.capture.snapshot = snapshot;
        let url = url.unwrap_or_else(|| DEFAULT_URL.to_owned());
        match state {
            "new-tab" => self.focus_address_pending = true,
            "address" => {
                self.focus_address_pending = true;
                self.capture.typed = Some(TYPED_ADDRESS.to_owned());
            }
            // A closed local port is refused at once, without the network.
            "error" => self.navigate(
                if url == DEFAULT_URL {
                    "http://127.0.0.1:59999/".to_owned()
                } else {
                    url
                },
                cx,
            ),
            "page" => self.navigate(url, cx),
            other => {
                self.capture.after_load = Some(other.to_owned());
                self.navigate(url, cx);
            }
        }
        cx.notify();
    }

    /// Runs the part of a capture state that needs the window.
    pub(super) fn apply_capture_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.address_state.focused
            && let Some(typed) = self.capture.typed.take()
        {
            let end = typed.len();
            self.address.update(cx, |input, cx| {
                input.set_text_with_selection(typed, end..end, cx)
            });
            self.address_state.typed.clear();
            self.address_changed(cx);
        }
        if self.capture.find {
            self.capture.find = false;
            self.open_find(window, cx);
            if let Some(input) = self.find.input.clone() {
                input.update(cx, |input, cx| {
                    input.set_text_with_selection(
                        FIND_QUERY,
                        FIND_QUERY.len()..FIND_QUERY.len(),
                        cx,
                    )
                });
            }
            self.run_find(FIND_QUERY.to_owned(), false);
        }
    }

    /// The page finished loading: open what the state shows over it, and
    /// write the page snapshot.
    pub(super) fn capture_page_loaded(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.capture.after_load.take() {
            match state.as_str() {
                "menu" => self.menu = Some(PanelMenu::Options),
                "clear-data" => self.menu = Some(PanelMenu::ClearData),
                "downloads" => self.menu = Some(PanelMenu::Downloads),
                "find" => self.capture.find = true,
                "zoom" => {
                    // As if hovered: the banner stays for the capture.
                    self.zoom_banner_hovered = true;
                    self.step_zoom(1, cx);
                }
                "tab-menu" => {
                    if let (Some(tab), Some(bounds)) =
                        (self.active_tab(), self.anchor_bounds("tabs"))
                    {
                        self.menu = Some(PanelMenu::Tab(tab.id));
                        self.menu_anchor =
                            Some(bounds.origin + gpui::point(gpui::px(40.), gpui::px(26.)));
                    }
                }
                _ => {}
            }
        }
        let Some(path) = self.capture.snapshot.take() else {
            return;
        };
        let Some(view) = self.active_view() else {
            return;
        };
        let page_frame = self.page_frame.clone();
        let holes = self.holes.clone();
        cx.spawn(async move |_, cx| {
            // Let the page paint before its snapshot.
            cx.background_executor()
                .timer(std::time::Duration::from_millis(400))
                .await;
            let frame = page_frame.get();
            cx.update(|_| {
                view.snapshot_png(move |png| {
                    if let Some(png) = png {
                        let _ = std::fs::write(&path, png);
                    }
                    if let Some(frame) = frame {
                        // The GPUI overlays the page leaves uncovered.
                        let holes: Vec<_> = holes
                            .borrow()
                            .iter()
                            .map(|hole| {
                                serde_json::json!({
                                    "x": hole.x, "y": hole.y,
                                    "width": hole.width, "height": hole.height,
                                    "cornerRadius": hole.corner_radius,
                                })
                            })
                            .collect();
                        let meta = serde_json::json!({
                            "x": frame.x, "y": frame.y,
                            "width": frame.width, "height": frame.height,
                            "bottomCornerRadius": frame.bottom_corner_radius,
                            "holes": holes,
                        });
                        let _ = std::fs::write(
                            format!("{}.json", path.display()),
                            serde_json::to_vec_pretty(&meta).unwrap_or_default(),
                        );
                    }
                });
            });
        })
        .detach();
    }
}
