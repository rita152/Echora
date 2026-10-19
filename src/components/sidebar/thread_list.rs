//! Thread rows share their parent's scrollport. Reserve the complete height,
//! then build only the rows intersecting that scrollport (plus two neighbours).
//! Section headers, drop targets and pagination keep their existing geometry.
use super::*;
use std::ops::Range;

const OVERSCAN: usize = 2;

fn visible_range(count: usize, top: f32, viewport_top: f32, viewport_bottom: f32) -> Range<usize> {
    let stride = ROW_HEIGHT + ROW_GAP;
    if count == 0 || top >= viewport_bottom || top + count as f32 * stride <= viewport_top {
        return 0..0;
    }
    let start = (((viewport_top - top).max(0.) / stride).floor() as usize).saturating_sub(OVERSCAN);
    let end = (((viewport_bottom - top).max(0.) / stride).ceil() as usize + OVERSCAN).min(count);
    start.min(end)..end
}

impl SidebarView {
    pub(super) fn cached_threads(
        &self,
        key: &str,
        build: impl FnOnce() -> Vec<ThreadSummary>,
    ) -> Rc<Vec<ThreadSummary>> {
        if let Some(threads) = self.thread_lists.borrow().get(key) {
            return threads.clone();
        }
        let threads = Rc::new(build());
        self.thread_lists
            .borrow_mut()
            .insert(key.to_owned(), threads.clone());
        threads
    }

    pub(super) fn ordered_threads(&self) -> Rc<Vec<ThreadSummary>> {
        self.cached_threads("ordered", || {
            self.snapshot
                .preferences
                .sidebar
                .ordered(&self.snapshot.recent_threads)
                .into_iter()
                .cloned()
                .collect()
        })
    }

    pub(super) fn windowed_threads(
        &self,
        key: String,
        threads: Rc<Vec<ThreadSummary>>,
        count: usize,
        placement: ThreadRowPlacement,
        theme: Theme,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let count = count.min(threads.len());
        let height = (count as f32 * (ROW_HEIGHT + ROW_GAP) - ROW_GAP).max(0.);
        let view = cx.entity();
        div()
            .id(format!("thread-viewport-{key}"))
            .h(px(height))
            .w_full()
            .flex_none()
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let viewport = window.content_mask().bounds;
                        let range = visible_range(
                            count,
                            bounds.top().into(),
                            viewport.top().into(),
                            viewport.bottom().into(),
                        );
                        range
                            .map(|index| {
                                gpui::performance::event("SidebarView.thread_row");
                                let mut row = view
                                    .update(cx, |view, cx| {
                                        view.thread_row(
                                            &threads[index],
                                            placement,
                                            theme,
                                            window,
                                            cx,
                                        )
                                        .w(bounds.size.width)
                                    })
                                    .into_any_element();
                                let origin = point(
                                    bounds.left(),
                                    bounds.top() + px(index as f32 * (ROW_HEIGHT + ROW_GAP)),
                                );
                                row.prepaint_as_root(
                                    origin,
                                    size(bounds.size.width, px(ROW_HEIGHT)).into(),
                                    window,
                                    cx,
                                );
                                row
                            })
                            .collect::<Vec<_>>()
                    },
                    |_, rows, window, cx| {
                        for mut row in rows {
                            row.paint(window, cx);
                        }
                    },
                )
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_work_is_bounded_for_hundred_thousand_threads() {
        let stride = ROW_HEIGHT + ROW_GAP;
        for count in [100, 1_000, 10_000, 100_000] {
            let range = visible_range(count, -40. * stride, 0., 600.);
            assert!(range.len() <= (600. / stride).ceil() as usize + 2 * OVERSCAN + 1);
            assert!(range.contains(&40));
            assert!(visible_range(count, 700., 0., 600.).is_empty());
            let end = visible_range(count, -(count as f32 - 2.) * stride, 0., 600.);
            assert_eq!(end.end, count);
        }
    }
}
