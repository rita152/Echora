//! Sticky section headings for the sidebar activity view.
//!
//! GPUI has no `position: sticky`, so a section lays out its heading and rows
//! normally and then, while prepainting inside the scroll container, moves the
//! heading and clips the rows the way the reference's CSS does
//! (`_heading_89zmk`, `_clipWindow_89zmk`, `_clipContent_89zmk`, measured over
//! CDP on ChatGPT 26.917):
//!
//! * the heading sticks 6 px below the scroll header mask, which starts 1 px
//!   (the list's top padding) below the scrollport, plus the 4 px header
//!   spacing once the list is scrolled, and it leaves with its section;
//! * rows clip 4 px above the heading's bottom edge, a line that moves down
//!   those 4 px as the section scrolls out;
//! * a heading pushed out by its section fades over its first 8 px past a line
//!   2 px below the mask start.
//!
//! Everything is computed from the frame's own layout, so the heading never
//! lags the scroll by a frame.

use gpui::{
    AnyElement, App, Bounds, ContentMask, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Style, Window, point, px, relative,
};

/// `--height-token-row`: the sticky heading box.
pub(super) const STICKY_HEADING_HEIGHT: f32 = 30.0;
/// `top: calc(var(--sidebar-scroll-header-mask-start) + var(--spacing) * 1.5)`.
const STICKY_TOP: f32 = 6.0;
/// The list's `pt-[var(--sidebar-scroll-content-top-padding)]`.
const SCROLL_CONTENT_TOP_PADDING: f32 = 1.0;
/// `--sidebar-scroll-header-spacing` once content is under the header.
const SCROLLED_HEADER_SPACING: f32 = 4.0;
/// `--spacing`: the clip window's negative margin and its travel.
const CLIP_SPACING: f32 = 4.0;
/// `--sidebar-scroll-header-mask-distance` in the sticky-section mode.
const FADE_INSET: f32 = 2.0;
/// `animation-range: exit 0% exit calc(var(--spacing) * 2)`.
const FADE_DISTANCE: f32 = 8.0;

/// Where a section's heading and rows land for one frame, in window
/// coordinates. `viewport_top` is the scroll container's top edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct StickyFrame {
    pub heading_top: f32,
    pub rows_clip_top: f32,
    pub heading_opacity: f32,
}

pub(super) fn sticky_frame(
    viewport_top: f32,
    scrolled: bool,
    section_top: f32,
    section_height: f32,
) -> StickyFrame {
    let mask_start = if scrolled {
        SCROLLED_HEADER_SPACING
    } else {
        0.0
    };
    let stuck = viewport_top + SCROLL_CONTENT_TOP_PADDING + mask_start + STICKY_TOP;
    let last = section_top + section_height - STICKY_HEADING_HEIGHT;
    let heading_top = section_top.max(stuck).min(last.max(section_top));

    // The rows' view timeline is inset by the stuck heading (the scrollport
    // edge here ignores the list's padding, unlike sticky positioning).
    let rows_top = section_top + STICKY_HEADING_HEIGHT;
    let rows_height = (section_height - STICKY_HEADING_HEIGHT).max(0.0);
    let inset = viewport_top + mask_start + STICKY_TOP + STICKY_HEADING_HEIGHT;
    let progress = if rows_height > 0.0 {
        ((inset - rows_top) / rows_height).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let rows_clip_top = if progress > 0.0 {
        inset - CLIP_SPACING + CLIP_SPACING * progress
    } else {
        rows_top - CLIP_SPACING
    };

    let fade_line = viewport_top + mask_start + FADE_INSET;
    let heading_opacity =
        ((heading_top - (fade_line - FADE_DISTANCE)) / FADE_DISTANCE).clamp(0.0, 1.0);
    StickyFrame {
        heading_top,
        rows_clip_top,
        heading_opacity,
    }
}

/// A heading plus its rows, laid out as a column.
pub(super) struct StickySection {
    id: ElementId,
    heading: AnyElement,
    rows: AnyElement,
    scrolled: bool,
}

pub(super) fn sticky_section(
    id: impl Into<ElementId>,
    heading: impl IntoElement,
    rows: impl IntoElement,
    scrolled: bool,
) -> StickySection {
    StickySection {
        id: id.into(),
        heading: heading.into_any_element(),
        rows: rows.into_any_element(),
        scrolled,
    }
}

impl IntoElement for StickySection {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

pub(super) struct StickyPrepaint {
    rows_mask: ContentMask<Pixels>,
    heading_opacity: f32,
}

impl Element for StickySection {
    type RequestLayoutState = ();
    type PrepaintState = StickyPrepaint;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let heading = self.heading.request_layout(window, cx);
        let rows = self.rows.request_layout(window, cx);
        let mut style = Style {
            display: gpui::Display::Flex,
            flex_direction: gpui::FlexDirection::Column,
            flex_shrink: 0.0,
            ..Style::default()
        };
        style.size.width = relative(1.0).into();
        (window.request_layout(style, [heading, rows], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> StickyPrepaint {
        // The mask here is the scroll container's, trimmed by `TopClip` once
        // the list is scrolled; sticky positions count from the container.
        let viewport = window.content_mask().bounds;
        let header_clip = if self.scrolled {
            SCROLLED_HEADER_CLIP
        } else {
            0.0
        };
        let frame = sticky_frame(
            f32::from(viewport.origin.y) - header_clip,
            self.scrolled,
            f32::from(bounds.origin.y),
            f32::from(bounds.size.height),
        );
        let rows_mask = ContentMask {
            bounds: Bounds::from_corners(
                point(bounds.origin.x, px(frame.rows_clip_top)),
                point(bounds.right(), bounds.bottom().max(px(frame.rows_clip_top))),
            ),
        };
        window.with_content_mask(Some(rows_mask), |window| {
            self.rows.prepaint(window, cx);
        });
        let shift = point(px(0.0), px(frame.heading_top) - bounds.origin.y);
        window.with_element_offset(shift, |window| {
            window.with_element_opacity(Some(frame.heading_opacity), |window| {
                self.heading.prepaint(window, cx);
            })
        });
        StickyPrepaint {
            rows_mask,
            heading_opacity: frame.heading_opacity,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut StickyPrepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Rows first, so the heading paints above the few pixels of row that
        // may peek under its lower edge.
        window.with_content_mask(Some(prepaint.rows_mask), |window| {
            self.rows.paint(window, cx);
        });
        window.with_element_opacity(Some(prepaint.heading_opacity), |window| {
            self.heading.paint(window, cx);
        });
    }
}

/// The list's own header mask: once content is under the header the first
/// 4 px of the scroll area are transparent and the next 2 px fade in. GPUI
/// has no mask images, so the rows are clipped half way through that fade.
pub(super) const SCROLLED_HEADER_CLIP: f32 = SCROLLED_HEADER_SPACING + FADE_INSET / 2.0;

/// Clips its child's top edge by `inset` (in addition to any clip already in
/// effect).
pub(super) struct TopClip {
    child: AnyElement,
    inset: f32,
}

pub(super) fn top_clip(child: impl IntoElement, inset: f32) -> TopClip {
    TopClip {
        child: child.into_any_element(),
        inset,
    }
}

impl IntoElement for TopClip {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for TopClip {
    type RequestLayoutState = ();
    type PrepaintState = ContentMask<Pixels>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let child = self.child.request_layout(window, cx);
        let mut style = Style {
            display: gpui::Display::Flex,
            flex_direction: gpui::FlexDirection::Column,
            flex_grow: 1.0,
            flex_basis: gpui::Length::Definite(px(0.0).into()),
            ..Style::default()
        };
        style.min_size.height = px(0.0).into();
        (window.request_layout(style, [child], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> ContentMask<Pixels> {
        let mask = ContentMask {
            bounds: Bounds::from_corners(
                point(bounds.origin.x, bounds.origin.y + px(self.inset)),
                point(bounds.right(), bounds.bottom()),
            ),
        };
        window.with_content_mask(Some(mask), |window| {
            self.child.prepaint(window, cx);
        });
        mask
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        mask: &mut ContentMask<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(*mask), |window| self.child.paint(window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Numbers below are the reference's own readings (CDP, 1470x923 window,
    // sidebar scroll container at y = 116): the Priority section is 67 px
    // tall at y = 225, the Thursday section 139 px tall at y = 308.
    #[test]
    fn headings_stick_and_leave_with_their_section_like_the_reference() {
        let at = |scroll: f32, top: f32, height: f32| {
            sticky_frame(116.0, scroll > 0.0, top - scroll, height)
        };
        // Resting list: everything in flow.
        let rest = at(0.0, 308.0, 139.0);
        assert_eq!(rest.heading_top, 308.0);
        assert_eq!(rest.heading_opacity, 1.0);
        assert_eq!(rest.rows_clip_top, 334.0);
        // scrollTop 100: Priority (in flow at 125) already sticks at 127.
        assert_eq!(at(100.0, 225.0, 67.0).heading_top, 127.0);
        // scrollTop 140: Priority pushed up to 122 by its section's end.
        let pushed = at(140.0, 225.0, 67.0);
        assert_eq!(pushed.heading_top, 122.0);
        assert_eq!(pushed.heading_opacity, 1.0);
        // scrollTop 160: pushed to 102 and fully faded.
        assert_eq!(at(160.0, 225.0, 67.0).heading_opacity, 0.0);
        // scrollTop 200: Thursday stuck at 127, rows clipped from 152.67.
        let stuck = at(200.0, 308.0, 139.0);
        assert_eq!(stuck.heading_top, 127.0);
        assert!((stuck.rows_clip_top - 152.66).abs() < 0.02, "{stuck:?}");
        // scrollTop 280: still stuck, the clip line has moved 1.6 px.
        let later = at(280.0, 308.0, 139.0);
        assert_eq!(later.heading_top, 127.0);
        assert!((later.rows_clip_top - 155.6).abs() < 0.02, "{later:?}");
    }
}
