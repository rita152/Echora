//! Inline artwork is display-only: source offsets, clipboard text and RPC input
//! continue to use the semantic reference's unmodified label.
use super::*;
use crate::{agent::AgentInputReference, components::icons::icon};
use gpui::AvailableSpace;

// One em + one quarter em + one hair space reserve 18.9px at the
// composer's 14px size, matching the reference's 16px icon and 3px gap.
pub(super) const ICON_SPACE: &str = "\u{2003}\u{2005}\u{200a}";

pub(super) fn color(reference: &AgentInputReference, mode: ThemeMode) -> gpui::Rgba {
    let theme = Theme::for_mode(mode);
    let Some(value) = reference.brand_color.as_deref().and_then(|s| {
        let s = s.strip_prefix('#')?;
        (s.len() == 6)
            .then(|| u32::from_str_radix(s, 16).ok())
            .flatten()
    }) else {
        return theme.accent;
    };
    let mut color = gpui::rgb(value);
    if mode == ThemeMode::Dark {
        // Keep the package's hue legible on the dark composer.
        color.r = color.r * 0.44 + theme.text.r * 0.56;
        color.g = color.g * 0.44 + theme.text.g * 0.56;
        color.b = color.b * 0.44 + theme.text.b * 0.56;
    }
    color
}

impl FileEditor {
    pub(super) fn reference_at(&self, point: Point<Pixels>, window: &Window) -> Option<usize> {
        let bounds = self.bounds?;
        if !bounds.contains(&point) {
            return None;
        }
        let source = self.offset_at(point, window);
        self.buffer
            .references
            .iter()
            .position(|r| r.range.contains(&source))
    }
    pub(super) fn reference_icons(
        &self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        let mut icons = Vec::new();
        for reference in &self.buffer.references {
            let index = self.row_for(reference.range.start);
            let Some(row) = self.rows.get(index) else {
                continue;
            };
            let y = bounds.top() + px(index as f32 * self.line_height() - self.scroll);
            if y + px(self.line_height()) <= bounds.top() || y >= bounds.bottom() {
                continue;
            }
            let origin = point(
                bounds.left()
                    + px(self.gutter())
                    + row
                        .line(window)
                        .x_for_index(row.display(reference.range.start)),
                y + px((self.line_height() - 16.) / 2.),
            );
            let tint = color(reference, self.mode);
            let glyph = if reference.kind == crate::agent::AgentInputReferenceKind::Thread {
                "side-chat"
            } else {
                "command-init"
            };
            let mut element = if let Some(path) = &reference.icon {
                crate::components::icons::package_icon(path.clone(), tint.into())
            } else {
                icon(glyph, tint.into()).size(px(16.)).into_any_element()
            };
            element.prepaint_as_root(
                origin,
                size(
                    AvailableSpace::Definite(px(16.)),
                    AvailableSpace::Definite(px(16.)),
                ),
                window,
                cx,
            );
            icons.push(element);
        }
        icons
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inline_artwork_does_not_enter_clipboard_text_and_metadata_survives_editing() {
        let mut app = gpui::TestApp::new();
        app.update(init);
        let mut window = app.open_window(|_, cx| FileEditor::composer(ThemeMode::Dark, cx));
        let reference = AgentInputReference {
            range: 0..0,
            name: "PDF".into(),
            path: "plugin://pdf@local".into(),
            label: "PDF".into(),
            kind: crate::agent::AgentInputReferenceKind::Plugin,
            icon: Some("/tmp/missing-icon.png".into()),
            brand_color: Some("#DC2626".into()),
        };
        window.update(|e, w, cx| {
            e.insert_reference(0..0, reference, cx);
            e.buffer.anchor = 0;
            e.buffer.cursor = 3;
            e.copy(cx);
            assert_eq!(
                cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
                Some("PDF")
            );
            e.move_to(4, false, cx);
            e.paste(cx);
            assert_eq!(e.text(), "PDF PDF");
            assert_eq!(e.references().len(), 2);
            assert_eq!(e.references()[1].brand_color.as_deref(), Some("#DC2626"));
            e.undo(cx);
            assert_eq!(e.references().len(), 1);
            e.redo(cx);
            assert_eq!(e.references().len(), 2);
            e.focus.focus(w, cx);
        });
        window.draw();
        window.update(|e, window, _| {
            let row = &e.rows[0];
            assert_eq!(row.source(row.display(0)), 0);
            assert!(f32::from(row.line(window).x_for_index(row.display(1))) > 16.);
            assert_eq!(row.source(row.line(window).closest_index_for_x(px(5.))), 0);
        });
        window.simulate_keystrokes("backspace");
        window.read(|e, _| {
            assert_eq!(e.text(), "PDF ");
            assert_eq!(e.references().len(), 1);
        });
    }
}
