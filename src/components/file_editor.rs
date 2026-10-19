mod reference_render;

use std::{
    cell::Cell,
    ops::Range,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui::{
    Animation, AnimationExt, AnyElement, App, Bounds, ClipboardItem, Context, CursorStyle,
    ElementId, ElementInputHandler, EntityInputHandler, FocusHandle, Focusable, KeyDownEvent,
    MouseButton, Pixels, Point, ShapedLine, TextRun, UTF16Selection, Window, canvas, div, fill,
    point, prelude::*, px, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::{Theme, ThemeMode, UI_MONOSPACE_FONT_FAMILY, ui_font};

const FONT_SIZE: f32 = 12.;
const GUTTER: f32 = 46.;
gpui::actions!(
    file_editor,
    [
        EditorCopy,
        EditorSubmitInverted,
        EditorCut,
        EditorPaste,
        EditorSelectAll,
        EditorUndo,
        EditorRedo,
        EditorSave,
        EditorBackspace,
        EditorDelete,
        EditorEnter,
        EditorLineBreak,
        EditorTab
    ]
);
pub fn init(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-c", EditorCopy, Some("ApprovalPreview")),
        gpui::KeyBinding::new("cmd-a", EditorSelectAll, Some("ApprovalPreview")),
        gpui::KeyBinding::new("cmd-c", EditorCopy, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-x", EditorCut, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-v", EditorPaste, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-a", EditorSelectAll, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-z", EditorUndo, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-shift-z", EditorRedo, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-s", EditorSave, Some("FileEditor")),
        gpui::KeyBinding::new("backspace", EditorBackspace, Some("FileEditor")),
        gpui::KeyBinding::new("delete", EditorDelete, Some("FileEditor")),
        gpui::KeyBinding::new("enter", EditorEnter, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-enter", EditorSubmitInverted, Some("FileEditor")),
        gpui::KeyBinding::new("shift-enter", EditorLineBreak, Some("FileEditor")),
        gpui::KeyBinding::new("tab", EditorTab, Some("FileEditor")),
    ]);
}
#[derive(Clone)]
struct Snapshot {
    text: String,
    references: Vec<crate::agent::AgentInputReference>,
    anchor: usize,
    cursor: usize,
}
#[derive(Default)]
pub struct Buffer {
    pub text: String,
    references: Vec<crate::agent::AgentInputReference>,
    anchor: usize,
    cursor: usize,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}
impl Buffer {
    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }
    fn snap_reference(&self, offset: usize, forward: bool) -> usize {
        self.references
            .iter()
            .find(|r| r.range.start < offset && offset < r.range.end)
            .map_or(
                offset,
                |r| if forward { r.range.end } else { r.range.start },
            )
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            references: self.references.clone(),
            anchor: self.anchor,
            cursor: self.cursor,
        }
    }
    fn restore(&mut self, s: Snapshot) {
        self.text = s.text;
        self.references = s.references;
        self.anchor = s.anchor;
        self.cursor = s.cursor;
    }
    fn replace(&mut self, range: Range<usize>, text: &str, checkpoint: bool) {
        if checkpoint {
            self.undo.push(self.snapshot());
            self.redo.clear();
            while self.undo.len() > 100
                || (self.undo.len() > 1
                    && self.undo.iter().map(|s| s.text.len()).sum::<usize>() > 16 * 1024 * 1024)
            {
                self.undo.remove(0);
            }
        }
        let mut range = range;
        // An inline reference is one editing unit, including replacements made
        // by native IME and selection APIs. Never retain half a reference.
        for reference in &self.references {
            if range.start < reference.range.end && range.end > reference.range.start {
                range.start = range.start.min(reference.range.start);
                range.end = range.end.max(reference.range.end);
            }
        }
        self.references.retain_mut(|reference| {
            if reference.range.start < range.end && reference.range.end > range.start {
                return false;
            }
            if reference.range.start >= range.end {
                reference.range.start = reference.range.start - range.len() + text.len();
                reference.range.end = reference.range.end - range.len() + text.len();
            }
            true
        });
        self.text.replace_range(range.clone(), text);
        self.cursor = range.start + text.len();
        self.anchor = self.cursor;
    }
    fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(s);
        }
    }
    fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(s);
        }
    }
    fn byte_offset_from_utf16(&self, offset: usize) -> usize {
        from_utf16(&self.text, offset)
    }
    fn to_utf16(&self, offset: usize) -> usize {
        self.text[..offset].encode_utf16().count()
    }
}
fn from_utf16(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units + c.len_utf16() > offset {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}
fn previous(text: &str, offset: usize) -> usize {
    text[..offset]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(i, _)| i)
}
fn next(text: &str, offset: usize) -> usize {
    text[offset..]
        .graphemes(true)
        .next()
        .map_or(offset, |g| offset + g.len())
}

pub struct ReferenceActivated(pub crate::agent::AgentInputReference);
impl gpui::EventEmitter<ReferenceActivated> for FileEditor {}

pub enum EditorEvent {
    Changed,
    SelectionChanged,
    /// Wrapped composer height changed after layout.
    HeightChanged,
    Save,
    Submit,
    /// ⌘⏎ in a composer: send with the opposite follow-up behavior.
    SubmitInverted,
}
pub struct FileEditor {
    pub buffer: Buffer,
    pub mode: ThemeMode,
    language: Option<String>,
    prose_label: Option<String>,
    placeholder: String,
    /// `--composer-editor-placeholder-opacity`: 1 for form composers, 0.5
    /// for the default ProseMirror placeholder.
    placeholder_opacity: f32,
    /// Font size and line height overriding the prose defaults.
    metrics: Option<(f32, f32)>,
    composer: bool,
    read_only: bool,
    preview_collapsed: bool,
    focus: FocusHandle,
    marked: Option<Range<usize>>,
    rows: Vec<Row>,
    layout_width: f32,
    layout_metrics: (f32, f32),
    layout_dirty: bool,
    scroll: f32,
    bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    hovered_reference: Option<usize>,
    reference_press: Option<(usize, Point<Pixels>)>,
    ensure_cursor: bool,
    desired_x: Option<f32>,
    last_input: Option<(Instant, usize)>,
    caret_blink_generation: u64,
    input_error: Option<&'static str>,
}
struct Row {
    line: ShapedLine,
    map: Vec<(usize, usize)>,
    number: Option<usize>,
}
impl Row {
    fn start(&self) -> usize {
        self.map.first().unwrap().1
    }
    fn end(&self) -> usize {
        self.map.last().unwrap().1
    }
    fn source(&self, display: usize) -> usize {
        self.map
            .iter()
            .min_by_key(|(d, _)| d.abs_diff(display))
            .unwrap()
            .1
    }
    fn display(&self, source: usize) -> usize {
        self.map
            .iter()
            .min_by_key(|(_, s)| s.abs_diff(source))
            .unwrap()
            .0
    }
}
impl gpui::EventEmitter<EditorEvent> for FileEditor {}
impl Focusable for FileEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl FileEditor {
    pub fn new(
        text: String,
        language: Option<String>,
        mode: ThemeMode,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            buffer: Buffer {
                text,
                ..Default::default()
            },
            language,
            prose_label: None,
            placeholder: String::new(),
            placeholder_opacity: 1.0,
            metrics: None,
            composer: false,
            read_only: false,
            preview_collapsed: false,
            mode,
            focus: cx.focus_handle(),
            marked: None,
            rows: Vec::new(),
            layout_width: 0.,
            layout_metrics: (0.0, 0.0),
            layout_dirty: true,
            scroll: 0.,
            bounds: None,
            selecting: false,
            hovered_reference: None,
            reference_press: None,
            ensure_cursor: false,
            desired_x: None,
            last_input: None,
            caret_blink_generation: 0,
            input_error: None,
        }
    }
    /// Reuse the editor's native IME, wrapping, selection and undo for review text.
    pub fn prose(mode: ThemeMode, label: &str, cx: &mut Context<Self>) -> Self {
        let mut editor = Self::new(String::new(), None, mode, cx);
        editor.prose_label = Some(label.into());
        editor.placeholder = label.into();
        editor
    }
    pub fn composer(mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        let mut editor = Self::prose(mode, "侧边聊天输入框", cx);
        editor.placeholder = "随心输入".into();
        editor.composer = true;
        editor
    }

    /// A selectable command preview. It never registers a text-input handler
    /// or emits edits, and shares the editor's Unicode-aware hit testing.
    pub fn approval_preview(text: String, mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        let mut editor = Self::new(text, None, mode, cx);
        editor.prose_label = Some(crate::i18n::text("命令预览，只读").to_owned());
        editor.read_only = true;
        editor.preview_collapsed = true;
        editor
    }

    pub fn set_preview_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        if self.preview_collapsed == expanded {
            self.preview_collapsed = !expanded;
            self.scroll = 0.0;
            cx.notify();
        }
    }

    pub fn visual_line_count(&self) -> usize {
        self.rows.len().max(self.buffer.text.lines().count()).max(1)
    }
    pub fn composer_height(&self) -> f32 {
        (self.rows.len().max(2) as f32 * self.line_height()).clamp(44.0, 220.0)
    }
    pub fn text(&self) -> &str {
        &self.buffer.text
    }
    fn placeholder_text(&self) -> &str {
        crate::i18n::text(&self.placeholder)
    }
    fn accessible_label(&self) -> &str {
        crate::i18n::text(
            self.prose_label
                .as_deref()
                .unwrap_or("文件内容，可直接编辑并自动保存"),
        )
    }
    pub fn set_accessible_name(&mut self, label: impl Into<String>) {
        self.prose_label = Some(label.into());
    }
    /// Uses `font_size` type on a `line_height` line box, like the Pull
    /// Requests composers' 14/28 textareas.
    pub fn set_text_metrics(&mut self, font_size: f32, line_height: f32, cx: &mut Context<Self>) {
        self.metrics = Some((font_size, line_height));
        cx.notify();
    }
    pub fn set_placeholder(&mut self, placeholder: impl Into<String>, cx: &mut Context<Self>) {
        self.placeholder = placeholder.into();
        cx.notify();
    }
    pub fn set_placeholder_opacity(&mut self, opacity: f32, cx: &mut Context<Self>) {
        self.placeholder_opacity = opacity;
        cx.notify();
    }
    pub fn set_text_silently(&mut self, text: &str, cx: &mut Context<Self>) {
        self.reload(text.into(), cx);
    }
    /// The caret's byte offset.
    pub fn cursor(&self) -> usize {
        self.buffer.cursor
    }
    pub fn selection_is_empty(&self) -> bool {
        self.buffer.selection().is_empty()
    }
    /// Replaces a byte range as one undoable edit, leaving the caret after
    /// the new text.
    pub fn replace_range(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        if self.read_only
            || range.end > self.buffer.text.len()
            || !self.buffer.text.is_char_boundary(range.start)
            || !self.buffer.text.is_char_boundary(range.end)
        {
            return;
        }
        self.marked = None;
        self.buffer.replace(range, text, true);
        self.changed(cx);
    }
    pub fn references(&self) -> &[crate::agent::AgentInputReference] {
        &self.buffer.references
    }

    pub fn set_references(
        &mut self,
        references: Vec<crate::agent::AgentInputReference>,
        cx: &mut Context<Self>,
    ) {
        let mut end = 0;
        self.buffer.references = references
            .into_iter()
            .filter(|r| {
                r.range.start < r.range.end
                    && r.range.start >= end
                    && self.buffer.text.get(r.range.clone()) == Some(r.label.as_str())
                    && {
                        end = r.range.end;
                        true
                    }
            })
            .collect();
        self.layout_dirty = true;
        cx.notify();
    }

    pub fn insert_reference(
        &mut self,
        range: Range<usize>,
        mut reference: crate::agent::AgentInputReference,
        cx: &mut Context<Self>,
    ) {
        if self.read_only || self.buffer.text.get(range.clone()).is_none() || self.composing() {
            return;
        }
        let start = range.start;
        self.buffer
            .replace(range, &format!("{} ", reference.label), true);
        reference.range = start..start + reference.label.len();
        self.buffer.references.push(reference);
        self.buffer.references.sort_by_key(|r| r.range.start);
        self.changed(cx);
    }

    fn line_height(&self) -> f32 {
        if let Some((font_size, line_height)) = self.metrics {
            return crate::typography::ui_leading(font_size, line_height);
        }
        if self.read_only {
            crate::typography::code_size() * 1.5
        } else if self.composer {
            20.0
        } else if self.prose_label.is_some() {
            crate::typography::ui_leading(13.0, 22.75)
        } else {
            crate::typography::diff_line_height().round()
        }
    }
    fn font_size(&self) -> f32 {
        if let Some((font_size, _)) = self.metrics {
            return crate::typography::ui_size(font_size);
        }
        if self.read_only {
            crate::typography::code_size()
        } else if self.composer {
            crate::typography::prose_size(14.0)
        } else if self.prose_label.is_some() {
            crate::typography::ui_size(13.0)
        } else {
            crate::typography::code_size()
        }
    }
    fn gutter(&self) -> f32 {
        if self.prose_label.is_some() {
            0.0
        } else {
            GUTTER * crate::typography::code_size() / FONT_SIZE
        }
    }
    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.layout_dirty = true;
        cx.notify();
    }
    pub fn reload(&mut self, text: String, cx: &mut Context<Self>) {
        let cursor = self.buffer.cursor.min(text.len());
        self.buffer = Buffer {
            text,
            ..Default::default()
        };
        self.buffer.cursor = cursor;
        while !self.buffer.text.is_char_boundary(self.buffer.cursor) {
            self.buffer.cursor -= 1;
        }
        self.buffer.anchor = self.buffer.cursor;
        self.marked = None;
        self.layout_dirty = true;
        self.reset_caret_blink();
        cx.notify();
    }
    pub fn can_undo(&self) -> bool {
        !self.buffer.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.buffer.redo.is_empty()
    }
    pub fn composing(&self) -> bool {
        self.marked.is_some()
    }
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if self.can_undo() {
            self.last_input = None;
            self.buffer.undo();
            self.marked = None;
            self.changed(cx);
        }
    }
    pub fn redo(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if self.can_redo() {
            self.last_input = None;
            self.buffer.redo();
            self.marked = None;
            self.changed(cx);
        }
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.hovered_reference = None;
        self.reference_press = None;
        self.layout_dirty = true;
        self.ensure_cursor = true;
        self.desired_x = None;
        self.reset_caret_blink();
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    fn reset_caret_blink(&mut self) {
        self.caret_blink_generation = self.caret_blink_generation.wrapping_add(1);
    }
    fn accepts(&mut self, range: &Range<usize>, text: &str, cx: &mut Context<Self>) -> bool {
        if self.read_only {
            return false;
        }
        if self.buffer.text.len() - range.len() + text.len()
            > super::file_io::MAX_TEXT_BYTES as usize
        {
            self.input_error = Some(crate::i18n::text("输入后文件将超过 2 MB，请缩小粘贴内容"));
            cx.notify();
            return false;
        }
        let mut value = self.buffer.text.clone();
        value.replace_range(range.clone(), text);
        if value.lines().any(|line| line.len() > 65_536) {
            self.input_error = Some(crate::i18n::text("单行不能超过 64 KB，请拆分长行"));
            cx.notify();
            return false;
        }
        self.input_error = None;
        true
    }

    fn replace(&mut self, text: &str, cx: &mut Context<Self>) {
        self.last_input = None;
        let range = self.marked.clone().unwrap_or(self.buffer.selection());
        if !self.accepts(&range, text, cx) {
            return;
        }
        self.marked = None;
        self.buffer.replace(range, text, true);
        self.changed(cx);
    }
    fn copy(&self, cx: &mut Context<Self>) {
        let range = self.buffer.selection();
        if !range.is_empty() {
            let references = self
                .buffer
                .references
                .iter()
                .filter(|r| r.range.start >= range.start && r.range.end <= range.end)
                .cloned()
                .map(|mut r| {
                    r.range.start -= range.start;
                    r.range.end -= range.start;
                    r
                })
                .collect::<Vec<_>>();
            let metadata = serde_json::json!({"echoraInlineReferences":references});
            cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
                self.buffer.text[range].into(),
                metadata,
            ));
        }
    }
    fn paste(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let Some(text) = item.text() else {
            return;
        };
        let text = text.replace("\r\n", "\n");
        let references = item
            .metadata()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| v.get("echoraInlineReferences").cloned())
            .and_then(|v| serde_json::from_value::<Vec<crate::agent::AgentInputReference>>(v).ok())
            .unwrap_or_default();
        let start = self.buffer.selection().start;
        self.replace(&text, cx);
        if self.buffer.text.get(start..start + text.len()) != Some(&text) {
            return;
        }
        for mut reference in references {
            if reference.range.start >= reference.range.end
                || text.get(reference.range.clone()) != Some(reference.label.as_str())
                || reference.name.is_empty()
                || reference.path.is_empty()
            {
                continue;
            }
            reference.range.start += start;
            reference.range.end += start;
            if self
                .buffer
                .references
                .iter()
                .any(|r| r.range.start < reference.range.end && r.range.end > reference.range.start)
            {
                continue;
            }
            self.buffer.references.push(reference);
        }
        self.buffer.references.sort_by_key(|r| r.range.start);
        self.changed(cx);
    }
    fn delete(&mut self, back: bool, cx: &mut Context<Self>) {
        self.last_input = None;
        let mut r = self.buffer.selection();
        if r.is_empty() {
            if back {
                r.start = previous(&self.buffer.text, r.start)
            } else {
                r.end = next(&self.buffer.text, r.end)
            }
        }
        if !r.is_empty() {
            self.buffer.replace(r, "", true);
            self.marked = None;
            self.changed(cx);
        }
    }
    fn move_to(&mut self, offset: usize, select: bool, cx: &mut Context<Self>) {
        self.last_input = None;
        let offset = self
            .buffer
            .references
            .iter()
            .find(|r| r.range.start < offset && offset < r.range.end)
            .map_or(offset, |r| {
                if offset < self.buffer.cursor {
                    r.range.start
                } else {
                    r.range.end
                }
            });
        self.buffer.cursor = offset;
        if !select {
            self.buffer.anchor = offset;
        }
        self.ensure_cursor = true;
        self.marked = None;
        self.reset_caret_blink();
        cx.emit(EditorEvent::SelectionChanged);
        cx.notify();
    }
    fn row_for(&self, offset: usize) -> usize {
        self.rows
            .iter()
            .rposition(|r| r.start() <= offset)
            .unwrap_or(0)
    }
    pub fn go_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let offset = self
            .buffer
            .text
            .split_inclusive('\n')
            .take(line.saturating_sub(1))
            .map(str::len)
            .sum();
        self.move_to(offset, false, cx);
    }
    fn handle_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        if self.composer
            && k.key == "tab"
            && !k.modifiers.control
            && !k.modifiers.platform
            && !k.modifiers.alt
            && !self.composing()
        {
            if k.modifiers.shift {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            cx.stop_propagation();
            return;
        }
        if self.prose_label.is_some() && k.modifiers.platform && k.key == "enter" {
            return;
        }
        let text = &self.buffer.text;
        let cursor = self.buffer.cursor;
        let select = k.modifiers.shift;
        let line_start = text[..cursor].rfind('\n').map_or(0, |i| i + 1);
        let line_end = text[cursor..].find('\n').map_or(text.len(), |i| cursor + i);
        let target = match k.key.as_str() {
            "left" if k.modifiers.platform => line_start,
            "right" if k.modifiers.platform => line_end,
            "left" if k.modifiers.alt => {
                let mut p = previous(text, cursor);
                while p > line_start && text[p..].chars().next().is_some_and(char::is_whitespace) {
                    p = previous(text, p);
                }
                while p > line_start
                    && text[..p]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                {
                    p = previous(text, p);
                }
                p
            }
            "right" if k.modifiers.alt => {
                let mut p = next(text, cursor);
                while p < line_end
                    && text[p..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                {
                    p = next(text, p);
                }
                while p < line_end && text[p..].chars().next().is_some_and(char::is_whitespace) {
                    p = next(text, p);
                }
                p
            }
            "left" => {
                if !select && !self.buffer.selection().is_empty() {
                    self.buffer.selection().start
                } else {
                    previous(text, cursor)
                }
            }
            "right" => {
                if !select && !self.buffer.selection().is_empty() {
                    self.buffer.selection().end
                } else {
                    next(text, cursor)
                }
            }
            "home" => line_start,
            "end" => line_end,
            "up" if k.modifiers.platform => 0,
            "down" if k.modifiers.platform => text.len(),
            "up" | "down" | "pageup" | "pagedown" => {
                if self.rows.is_empty() {
                    return;
                }
                let row = self.row_for(cursor);
                let x = self.desired_x.unwrap_or_else(|| {
                    f32::from(
                        self.rows[row]
                            .line
                            .x_for_index(self.rows[row].display(cursor)),
                    )
                });
                self.desired_x = Some(x);
                let step = if k.key.starts_with("page") {
                    self.bounds.map_or(10, |b| {
                        (f32::from(b.size.height) / self.line_height()) as usize
                    })
                } else {
                    1
                };
                let index = if matches!(k.key.as_str(), "up" | "pageup") {
                    row.saturating_sub(step)
                } else {
                    (row + step).min(self.rows.len() - 1)
                };
                self.rows[index].source(self.rows[index].line.closest_index_for_x(px(x)))
            }
            _ => return,
        };
        if !matches!(k.key.as_str(), "up" | "down" | "pageup" | "pagedown") {
            self.desired_x = None;
        }
        self.move_to(target, select, cx);
        cx.stop_propagation();
    }
    fn offset_at(&self, p: Point<Pixels>) -> usize {
        let Some(b) = self.bounds else {
            return 0;
        };
        if self.rows.is_empty() {
            return 0;
        }
        let row = ((f32::from(p.y - b.top()) + self.scroll).max(0.) / self.line_height()) as usize;
        let row = &self.rows[row.min(self.rows.len() - 1)];
        row.source(
            row.line
                .closest_index_for_x((p.x - b.left() - px(self.gutter())).max(px(0.))),
        )
    }
    fn prepare(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.bounds = Some(bounds);
        let width = (f32::from(bounds.size.width)
            - self.gutter()
            - if self.read_only { 0.0 } else { 16.0 })
        .max(24.);
        let metrics = (self.font_size(), self.line_height());
        if self.layout_dirty
            || self.layout_metrics != metrics
            || (self.layout_width - width).abs() > 0.5
        {
            let old_height = self.composer_height();
            let old_rows = self.rows.len();
            self.rows.clear();
            self.layout_width = width;
            self.layout_metrics = metrics;
            self.layout_dirty = false;
            let theme = Theme::for_mode(self.mode);
            let mut font = ui_font();
            font.family = UI_MONOSPACE_FONT_FAMILY.into();
            // The reference Files viewer shares the diff viewer's Codex
            // code theme.
            let spans = super::diff_syntax::code_spans(
                &self.buffer.text,
                self.language.as_deref(),
                self.mode,
            );
            let mut source = 0;
            let mut span_index = 0;
            for (number, raw) in self.buffer.text.split('\n').enumerate() {
                let mut display = String::new();
                let mut map = Vec::new();
                let mut runs: Vec<TextRun> = Vec::new();
                for (offset, g) in raw.grapheme_indices(true) {
                    let start = source + offset;
                    map.push((display.len(), start));
                    if self
                        .buffer
                        .references
                        .iter()
                        .any(|r| r.range.start == start)
                    {
                        display.push_str(reference_render::ICON_SPACE);
                        let mut run = window
                            .text_style()
                            .to_run(reference_render::ICON_SPACE.len());
                        run.font = ui_font();
                        runs.push(run);
                        map.push((display.len(), start));
                    }
                    while span_index + 1 < spans.len() && spans[span_index].0.end <= start {
                        span_index += 1;
                    }
                    let expanded = if g == "\t" {
                        "    "
                    } else if g == "\r" {
                        ""
                    } else {
                        g
                    };
                    display.push_str(expanded);
                    let mut run = spans
                        .get(span_index)
                        .map(|s| s.1.clone())
                        .unwrap_or(TextRun {
                            len: 0,
                            font: font.clone(),
                            color: theme.text.into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        });
                    if self.prose_label.is_some() {
                        run.font = ui_font();
                        run.color = theme.text.into();
                    }
                    if self.read_only {
                        run.font.family = UI_MONOSPACE_FONT_FAMILY.into();
                        run.font.weight = gpui::FontWeight::MEDIUM;
                        run.color = theme.text_tertiary.into();
                    }
                    if let Some(reference) = self
                        .buffer
                        .references
                        .iter()
                        .find(|r| r.range.contains(&start))
                    {
                        run.color = reference_render::color(reference, self.mode).into();
                        run.font.weight = gpui::FontWeight::MEDIUM;
                    }
                    run.len = expanded.len();
                    if self
                        .marked
                        .as_ref()
                        .is_some_and(|r| r.start <= start && start < r.end)
                    {
                        run.underline = Some(gpui::UnderlineStyle {
                            thickness: px(1.),
                            color: None,
                            wavy: false,
                        });
                    }
                    runs.push(run);
                }
                map.push((display.len(), source + raw.len()));
                let full = window.text_system().shape_line(
                    display.clone().into(),
                    px(self.font_size()),
                    &runs,
                    None,
                );
                let mut start = 0;
                let mut first = true;
                loop {
                    let desired = full.closest_index_for_x(full.x_for_index(start) + px(width));
                    let mut end = map
                        .iter()
                        .filter(|(d, _)| *d > start && *d <= desired)
                        .map(|(d, _)| *d)
                        .next_back()
                        .unwrap_or_else(|| {
                            map.iter()
                                .find(|(d, _)| *d > start)
                                .map_or(display.len(), |(d, _)| *d)
                        });
                    if f32::from(full.width() - full.x_for_index(start)) <= width {
                        end = display.len();
                    }
                    // Keep an inline reference on one row when it fits there;
                    // exceptionally long labels still wrap without changing identity.
                    if let Some((_, source_end)) = map.iter().find(|(d, _)| *d == end)
                        && let Some(reference) = self
                            .buffer
                            .references
                            .iter()
                            .find(|r| r.range.start < *source_end && *source_end < r.range.end)
                        && let Some((reference_start, _)) =
                            map.iter().find(|(_, s)| *s == reference.range.start)
                        && *reference_start > start
                    {
                        end = *reference_start;
                    }
                    let mut run_start = 0;
                    let row_runs = runs
                        .iter()
                        .filter_map(|r| {
                            let a = run_start;
                            run_start += r.len;
                            let overlap = start.max(a)..end.min(run_start);
                            if overlap.start < overlap.end {
                                let mut r = r.clone();
                                r.len = overlap.len();
                                Some(r)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    let line = window.text_system().shape_line(
                        display[start..end].to_string().into(),
                        px(self.font_size()),
                        &row_runs,
                        None,
                    );
                    let row_map = map
                        .iter()
                        .filter(|(d, _)| *d >= start && *d <= end)
                        .map(|(d, s)| (d - start, *s))
                        .collect();
                    self.rows.push(Row {
                        line,
                        map: row_map,
                        number: first.then_some(number + 1),
                    });
                    first = false;
                    if end == display.len() {
                        break;
                    }
                    start = end;
                }
                source += raw.len() + 1;
            }
            if self.composer && self.composer_height() != old_height {
                cx.emit(EditorEvent::HeightChanged);
                cx.notify();
            }
            if self.read_only && self.rows.len() != old_rows {
                cx.notify();
            }
        }
        let height = f32::from(bounds.size.height);
        if self.ensure_cursor {
            let top = self.row_for(self.buffer.cursor) as f32 * self.line_height();
            if top < self.scroll {
                self.scroll = top;
            } else if top + self.line_height() > self.scroll + height {
                self.scroll = top + self.line_height() - height;
            }
            self.ensure_cursor = false;
        }
        self.scroll = self.scroll.clamp(
            0.,
            (self.rows.len() as f32 * self.line_height() + if self.composer { 0.0 } else { 16.0 }
                - height)
                .max(0.),
        );
        if self.preview_collapsed {
            self.scroll = 0.0;
        }
        let _ = cx;
    }
    fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
        caret_visible: bool,
    ) {
        let theme = Theme::for_mode(self.mode);
        if self.buffer.text.is_empty() && !self.placeholder.is_empty() {
            let placeholder = self.placeholder_text();
            let mut run = window.text_style().to_run(placeholder.len());
            run.font = ui_font();
            run.color = gpui::Rgba {
                a: theme.text_tertiary.a * self.placeholder_opacity,
                ..theme.text_tertiary
            }
            .into();
            let line = window.text_system().shape_line(
                placeholder.to_owned().into(),
                px(self.font_size()),
                &[run],
                None,
            );
            let _ = line.paint(
                bounds.origin,
                px(self.line_height()),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
        }
        let selection = self.buffer.selection();
        let start = (self.scroll / self.line_height()).floor() as usize;
        let count = (f32::from(bounds.size.height) / self.line_height()).ceil() as usize + 1;
        for (index, row) in self.rows.iter().enumerate().skip(start).take(count) {
            let y = bounds.top() + px(index as f32 * self.line_height() - self.scroll);
            let origin = point(bounds.left() + px(self.gutter()), y);
            if !selection.is_empty() && selection.end > row.start() && selection.start <= row.end()
            {
                let left = row
                    .line
                    .x_for_index(row.display(selection.start.max(row.start())));
                let mut right = row
                    .line
                    .x_for_index(row.display(selection.end.min(row.end())));
                if selection.end > row.end() {
                    right += px(7.);
                }
                window.paint_quad(fill(
                    Bounds::new(
                        origin + point(left, px(0.)),
                        size((right - left).max(px(2.)), px(self.line_height())),
                    ),
                    theme.accent.alpha(0.18),
                ));
            }
            if let Some(number) = row.number.filter(|_| self.prose_label.is_none()) {
                let mut font = ui_font();
                font.family = UI_MONOSPACE_FONT_FAMILY.into();
                let text = number.to_string();
                let line = window.text_system().shape_line(
                    text.clone().into(),
                    px(self.font_size()),
                    &[TextRun {
                        len: text.len(),
                        font,
                        color: theme.text_tertiary.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                );
                let _ = line.paint(
                    point(bounds.left() + px(self.gutter() - 12.) - line.width(), y),
                    px(self.line_height()),
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
            let _ = row.line.paint(
                origin,
                px(self.line_height()),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
            if let Some(reference) = self
                .hovered_reference
                .and_then(|i| self.buffer.references.get(i))
                && reference.range.end > row.start()
                && reference.range.start < row.end()
            {
                let left = row
                    .line
                    .x_for_index(row.display(reference.range.start.max(row.start())));
                let right = row
                    .line
                    .x_for_index(row.display(reference.range.end.min(row.end())));
                window.paint_quad(fill(
                    Bounds::new(
                        origin + point(left, px(self.line_height() - 2.)),
                        size(right - left, px(1.)),
                    ),
                    reference_render::color(reference, self.mode),
                ));
            }
            if !self.read_only
                && self.focus.is_focused(window)
                && selection.is_empty()
                && caret_visible
                && index == self.row_for(self.buffer.cursor)
            {
                let x = row.line.x_for_index(row.display(self.buffer.cursor));
                window.paint_quad(fill(
                    Bounds::new(origin + point(x, px(3.)), size(px(1.), px(16.))),
                    theme.text,
                ));
            }
        }
        // A frame snapped to device pixels can sit a fraction under its
        // lines; only real overflow shows the thumb.
        if !self.preview_collapsed
            && self.rows.len() as f32 * self.line_height() > f32::from(bounds.size.height) + 0.5
        {
            let height = f32::from(bounds.size.height);
            let total = self.rows.len() as f32 * self.line_height() + 16.;
            let thumb = (height * height / total).max(24.);
            let top = self.scroll / (total - height) * (height - thumb);
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.right() - px(6.), bounds.top() + px(top)),
                    size(px(4.), px(thumb)),
                ),
                theme.text.alpha(0.2),
            ));
        }
    }
}
impl Render for FileEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let paint_entity = entity.clone();
        div()
            .id("file-editor")
            .when(self.prose_label.is_none(), |d| {
                d.bg(Theme::for_mode(self.mode).file_editor_surface)
            })
            .relative()
            .size_full()
            .overflow_hidden()
            .cursor(if self.hovered_reference.is_some() {
                CursorStyle::PointingHand
            } else {
                CursorStyle::IBeam
            })
            .key_context(if self.read_only {
                "ApprovalPreview"
            } else {
                "FileEditor"
            })
            .track_focus(&self.focus)
            .role(gpui::Role::TextInput)
            .aria_label(self.accessible_label().to_owned())
            .aria_placeholder(self.placeholder_text().to_owned())
            .aria_value(self.buffer.text.clone())
            .on_key_down(cx.listener(Self::handle_key))
            .on_action(cx.listener(|s, _: &EditorCopy, _, cx| {
                s.copy(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorCut, _, cx| {
                s.copy(cx);
                if !s.buffer.selection().is_empty() {
                    s.replace("", cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorPaste, _, cx| {
                s.paste(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorSelectAll, _, cx| {
                s.buffer.anchor = 0;
                s.buffer.cursor = s.buffer.text.len();
                cx.notify();
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorUndo, _, cx| {
                s.undo(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorRedo, _, cx| {
                s.redo(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_, _: &EditorSave, _, cx| {
                cx.emit(EditorEvent::Save);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorBackspace, _, cx| {
                s.delete(true, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorDelete, _, cx| {
                s.delete(false, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorSubmitInverted, _, cx| {
                if !s.composer {
                    cx.propagate();
                    return;
                }
                if !s.composing() {
                    cx.emit(EditorEvent::SubmitInverted);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorEnter, _, cx| {
                if s.composer {
                    if !s.composing() {
                        cx.emit(EditorEvent::Submit);
                    }
                    cx.stop_propagation();
                    return;
                }
                let start = s.buffer.text[..s.buffer.cursor]
                    .rfind('\n')
                    .map_or(0, |i| i + 1);
                let indent = s.buffer.text[start..s.buffer.cursor]
                    .chars()
                    .take_while(|c| matches!(c, ' ' | '\t'))
                    .collect::<String>();
                s.replace(&format!("\n{indent}"), cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorLineBreak, _, cx| {
                if !s.composing() {
                    s.replace("\n", cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorTab, window, cx| {
                if s.composer {
                    window.focus_next(cx);
                } else {
                    s.replace("    ", cx);
                }
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, e: &gpui::MouseDownEvent, w, cx| {
                    s.focus.focus(w, cx);
                    s.reference_press = (!e.modifiers.shift && e.click_count == 1)
                        .then(|| s.reference_at(e.position).map(|i| (i, e.position)))
                        .flatten();
                    s.reset_caret_blink();
                    let raw = s.offset_at(e.position);
                    let p = s.buffer.snap_reference(raw, raw >= s.buffer.cursor);
                    if !e.modifiers.shift {
                        s.buffer.anchor = p;
                    }
                    s.buffer.cursor = p;
                    s.marked = None;
                    s.selecting = true;
                    s.desired_x = None;
                    if e.click_count == 2 {
                        let text = &s.buffer.text;
                        let mut a = p;
                        let mut b = p;
                        while a > 0
                            && text[..a]
                                .chars()
                                .next_back()
                                .is_some_and(|c| c.is_alphanumeric() || c == '_')
                        {
                            a = previous(text, a);
                        }
                        while b < text.len()
                            && text[b..]
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_alphanumeric() || c == '_')
                        {
                            b = next(text, b);
                        }
                        s.buffer.anchor = a;
                        s.buffer.cursor = b;
                    } else if e.click_count >= 3 {
                        let a = s.buffer.text[..p].rfind('\n').map_or(0, |i| i + 1);
                        let b = s.buffer.text[p..]
                            .find('\n')
                            .map_or(s.buffer.text.len(), |i| p + i + 1);
                        s.buffer.anchor = a;
                        s.buffer.cursor = b;
                    }
                    s.buffer.anchor = s.buffer.snap_reference(s.buffer.anchor, false);
                    s.buffer.cursor = s.buffer.snap_reference(s.buffer.cursor, true);
                    cx.emit(EditorEvent::SelectionChanged);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_hover(cx.listener(|s, hovered, _, cx| {
                if !hovered {
                    s.hovered_reference = None;
                    cx.notify();
                }
            }))
            .on_mouse_move(cx.listener(|s, e: &gpui::MouseMoveEvent, _, cx| {
                let hover = s.reference_at(e.position);
                if s.hovered_reference != hover {
                    s.hovered_reference = hover;
                    cx.notify();
                }
                if s.selecting {
                    if let Some(b) = s.bounds {
                        if e.position.y < b.top() {
                            s.scroll = (s.scroll - s.line_height()).max(0.);
                        } else if e.position.y > b.bottom() {
                            s.scroll += s.line_height();
                        }
                    }
                    let raw = s.offset_at(e.position);
                    s.buffer.cursor = s.buffer.snap_reference(raw, raw >= s.buffer.anchor);
                    cx.emit(EditorEvent::SelectionChanged);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, e: &gpui::MouseUpEvent, _, cx| {
                    s.selecting = false;
                    if let Some((index, position)) = s.reference_press.take()
                        && (f32::from(e.position.x - position.x)).abs() < 3.
                        && (f32::from(e.position.y - position.y)).abs() < 3.
                        && s.reference_at(e.position) == Some(index)
                        && let Some(reference) = s.buffer.references.get(index)
                    {
                        cx.emit(ReferenceActivated(reference.clone()));
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|s, _, _, _| {
                    s.selecting = false;
                    s.reference_press = None;
                }),
            )
            .on_scroll_wheel(cx.listener(|s, e: &gpui::ScrollWheelEvent, _, cx| {
                if s.preview_collapsed {
                    return;
                }
                s.scroll -= f32::from(e.delta.pixel_delta(px(s.line_height())).y);
                s.ensure_cursor = false;
                cx.notify();
                cx.stop_propagation();
            }))
            .when_some(self.input_error, |e, error| {
                e.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .p(px(8.))
                        .bg(Theme::for_mode(self.mode).surface)
                        .text_color(Theme::for_mode(self.mode).warning)
                        .child(error),
                )
            })
            .child({
                let caret_visible = Rc::new(Cell::new(true));
                let caret_visible_for_paint = caret_visible.clone();
                let canvas = canvas(
                    move |b, w, cx| {
                        entity.update(cx, |s, cx| {
                            s.prepare(b, w, cx);
                            s.reference_icons(b, w, cx)
                        })
                    },
                    move |b, icons, w, cx| {
                        let caret_visible = caret_visible_for_paint.get();
                        paint_entity.update(cx, |s, cx| {
                            if !s.read_only {
                                w.handle_input(
                                    &s.focus,
                                    ElementInputHandler::new(b, paint_entity.clone()),
                                    cx,
                                );
                            }
                            s.paint(b, w, cx, caret_visible);
                        });
                        for mut icon in icons {
                            icon.paint(w, cx);
                        }
                    },
                )
                .size_full();
                let canvas: AnyElement = if self.read_only
                    || !self.focus.is_focused(window)
                    || !window.is_window_active()
                    || !self.buffer.selection().is_empty()
                {
                    canvas.into_any_element()
                } else {
                    canvas
                        .with_animation(
                            ElementId::named_usize(
                                "file-editor-caret-blink",
                                self.caret_blink_generation as usize,
                            ),
                            Animation::new(Duration::from_millis(1_000))
                                .repeat()
                                .with_max_fps(2.0),
                            move |canvas, progress| {
                                caret_visible.set(progress < 0.5);
                                canvas
                            },
                        )
                        .into_any_element()
                };
                canvas
            })
    }
}
impl EntityInputHandler for FileEditor {
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        self.focus.is_focused(window)
    }

    fn text_for_range(
        &mut self,
        r: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let r =
            self.buffer.byte_offset_from_utf16(r.start)..self.buffer.byte_offset_from_utf16(r.end);
        *adjusted = Some(self.buffer.to_utf16(r.start)..self.buffer.to_utf16(r.end));
        Some(self.buffer.text[r].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let r = self.buffer.selection();
        Some(UTF16Selection {
            range: self.buffer.to_utf16(r.start)..self.buffer.to_utf16(r.end),
            reversed: self.buffer.cursor < self.buffer.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|r| self.buffer.to_utf16(r.start)..self.buffer.to_utf16(r.end))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        self.changed(cx);
    }
    fn replace_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let checkpoint = self.marked.is_none();
        let r = r
            .map(|r| {
                self.buffer.byte_offset_from_utf16(r.start)
                    ..self.buffer.byte_offset_from_utf16(r.end)
            })
            .or(self.marked.take())
            .unwrap_or(self.buffer.selection());
        if !self.accepts(&r, text, cx) {
            return;
        }
        let now = Instant::now();
        let group = checkpoint
            && r.is_empty()
            && !text.contains('\n')
            && self.last_input.is_some_and(|(time, end)| {
                now.duration_since(time) < Duration::from_millis(750) && end == r.start
            });
        self.buffer.replace(r, text, checkpoint && !group);
        self.last_input = (!text.contains('\n') && checkpoint).then_some((now, self.buffer.cursor));
        self.changed(cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let checkpoint = self.marked.is_none();
        let r = r
            .map(|r| {
                self.buffer.byte_offset_from_utf16(r.start)
                    ..self.buffer.byte_offset_from_utf16(r.end)
            })
            .or(self.marked.take())
            .unwrap_or(self.buffer.selection());
        if !self.accepts(&r, text, cx) {
            return;
        }
        self.last_input = None;
        let start = r.start;
        self.buffer.replace(r, text, checkpoint);
        self.marked = (!text.is_empty()).then_some(start..start + text.len());
        if let Some(r) = selected {
            self.buffer.anchor = start + from_utf16(text, r.start);
            self.buffer.cursor = start + from_utf16(text, r.end);
        }
        self.changed(cx);
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let b = self.bounds?;
        let p = self.buffer.byte_offset_from_utf16(r.start);
        let index = self.row_for(p);
        let row = self.rows.get(index)?;
        Some(Bounds::new(
            point(
                b.left() + px(self.gutter()) + row.line.x_for_index(row.display(p)),
                b.top() + px(index as f32 * self.line_height() - self.scroll),
            ),
            size(px(1.), px(self.line_height())),
        ))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.buffer.to_utf16(self.offset_at(p)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_size_change_reflows_existing_unicode_editor_without_changing_selection_or_text() {
        let original = crate::typography::sizes();
        let mut app = gpui::TestApp::new();
        let source = "中文 English 🦀 long_code_identifier ".repeat(100);
        let mut window = app.open_window_with_options(gpui::WindowOptions::default(), |_, cx| {
            FileEditor::new(source.clone(), Some("rust".into()), ThemeMode::Dark, cx)
        });
        window.draw();
        let old_rows = window.read(|editor, _| editor.rows.len());
        window.update(|editor, _, _| {
            editor.buffer.anchor = 0;
            editor.buffer.cursor = "中文 English 🦀".len();
        });
        crate::typography::set_sizes(original.with_value(true, 24.0).unwrap());
        window.update(|_, _, cx| cx.notify());
        window.draw();
        window.update(|editor, _, cx| {
            assert!(
                editor.rows.len() > old_rows,
                "existing shaped rows must be invalidated"
            );
            assert_eq!(editor.buffer.text, source);
            editor.copy(cx);
            assert_eq!(
                cx.read_from_clipboard()
                    .and_then(|item| item.text())
                    .as_deref(),
                Some("中文 English 🦀")
            );
            // Every wrapped run still maps to valid UTF-8 source boundaries.
            for row in &editor.rows {
                assert!(source.is_char_boundary(row.start()));
                assert!(source.is_char_boundary(row.end()));
            }
        });
        crate::typography::set_sizes(original);
    }

    #[test]
    fn approval_preview_keeps_unicode_selection_copy_and_never_edits_the_command() {
        let mut app = gpui::TestApp::new();
        app.update(init);
        let text = "echo 中文 🦀\nprintf 'unchanged'\n";
        let mut window = app.open_window_with_options(gpui::WindowOptions::default(), |_, cx| {
            FileEditor::approval_preview(text.into(), ThemeMode::Dark, cx)
        });
        window.draw();
        window.update(|editor, window, cx| {
            window.focus(&editor.focus, cx);
            editor.buffer.anchor = 0;
            editor.buffer.cursor = editor.buffer.text.len();
            editor.copy(cx);
            assert_eq!(
                cx.read_from_clipboard()
                    .and_then(|item| item.text())
                    .as_deref(),
                Some(text)
            );
            editor.replace_text_in_range(Some(0..4), "changed", window, cx);
            editor.replace_and_mark_text_in_range(None, "输入", Some(0..2), window, cx);
            editor.replace("delete selection", cx);
            editor.undo(cx);
            editor.redo(cx);
            assert_eq!(editor.text(), text);
            assert!(!editor.composing());
            assert!(!editor.can_undo());
        });
    }

    #[test]
    fn unfocused_editor_does_not_schedule_continuous_animation_frames() {
        let mut app = gpui::TestApp::new();
        let mut window = app.open_window(|_, cx| FileEditor::composer(ThemeMode::Dark, cx));
        for _ in 0..5 {
            window.draw();
            window.update(|_, window, cx| {
                assert_eq!(
                    window.simulate_next_frame(cx),
                    0,
                    "an unfocused editor must not request the next display frame"
                );
            });
        }
    }

    #[test]
    fn side_composer_commits_ime_before_enter_and_preserves_multiline_undo() {
        let mut app = gpui::TestApp::new();
        app.update(init);
        let submissions = std::rc::Rc::new(std::cell::Cell::new(0));
        let observed = submissions.clone();
        let mut window = app.open_window_with_options(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(Bounds::new(
                    point(px(0.0), px(0.0)),
                    size(px(260.0), px(140.0)),
                ))),
                ..Default::default()
            },
            |_, cx| {
                let entity = cx.entity();
                cx.subscribe(&entity, move |_, _, event: &EditorEvent, _| {
                    if matches!(event, EditorEvent::Submit) {
                        observed.set(observed.get() + 1);
                    }
                })
                .detach();
                FileEditor::composer(ThemeMode::Dark, cx)
            },
        );
        window.draw();
        window.simulate_click(point(px(20.0), px(10.0)), MouseButton::Left);
        window.update(|editor, window, cx| {
            editor.replace_and_mark_text_in_range(None, "ni", Some(0..2), window, cx)
        });
        window.simulate_keystrokes("enter");
        assert_eq!(
            submissions.get(),
            0,
            "IME candidate confirmation must not submit a side prompt"
        );
        window.update(|editor, window, cx| editor.replace_text_in_range(None, "你好", window, cx));
        window.simulate_keystrokes("shift-enter");
        window.simulate_input("第二行 👋");
        assert_eq!(
            window.read(|editor, _| editor.text().to_owned()),
            "你好\n第二行 👋"
        );
        window.simulate_keystrokes("enter");
        assert_eq!(submissions.get(), 1);
        window.simulate_keystrokes("cmd-z");
        assert_eq!(window.read(|editor, _| editor.text().to_owned()), "你好\n");
        window.simulate_keystrokes("cmd-shift-z");
        assert_eq!(
            window.read(|editor, _| editor.text().to_owned()),
            "你好\n第二行 👋"
        );
        window.update(|_, window, cx| {
            window.resize(size(px(260.0), px(44.0)));
            window.bounds_changed(cx);
        });
        window.simulate_keystrokes("cmd-a");
        window.simulate_input("第一行\n第二行\n第三行\n第四行");
        window.draw();
        let expanded_height = window.read(|editor, _| editor.composer_height());
        window.update(|_, window, cx| {
            window.resize(size(px(260.0), px(expanded_height)));
            window.bounds_changed(cx);
        });
        window.draw();
        assert_eq!(
            window.read(|editor, _| editor.scroll),
            0.0,
            "growing to fit all lines must reveal the first line"
        );
    }
    #[test]
    fn native_editor_handles_wrapping_selection_clipboard_and_ime() {
        let mut app = gpui::TestApp::new();
        app.update(init);
        let mut window = app.open_window_with_options(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(260.), px(140.)),
                ))),
                ..Default::default()
            },
            |_, cx| {
                FileEditor::new(
                    "中文👩‍💻\n    value\n".into(),
                    Some("rs".into()),
                    ThemeMode::Light,
                    cx,
                )
            },
        );
        window.draw();
        window.simulate_click(point(px(50.), px(10.)), MouseButton::Left);
        window.simulate_keystrokes("cmd-a");
        window.simulate_input("αβ\n    中文👋");
        window.simulate_keystrokes("enter");
        window.simulate_input("next");
        assert_eq!(
            window.read(|e, _| e.buffer.text.clone()),
            "αβ\n    中文👋\n    next"
        );
        window.simulate_keystrokes("cmd-z cmd-z");
        assert_eq!(window.read(|e, _| e.buffer.text.clone()), "αβ\n    中文👋");
        window.simulate_keystrokes("shift-left cmd-c");
        assert_eq!(
            app.read_from_clipboard().and_then(|v| v.text()),
            Some("👋".into())
        );
        window.update(|e, w, cx| {
            e.replace_and_mark_text_in_range(None, "ni", Some(0..2), w, cx);
            e.replace_and_mark_text_in_range(None, "你", Some(1..1), w, cx);
            e.replace_text_in_range(None, "你好", w, cx);
            assert_eq!(e.buffer.text, "αβ\n    中文你好");
            e.undo(cx);
            assert_eq!(e.buffer.text, "αβ\n    中文👋");
            e.reload("中文👋".repeat(100), cx);
            e.go_to_line(1, cx);
        });
        window.draw();
        window.read(|e, _| {
            assert!(e.rows.len() > 10);
            assert!(
                e.rows
                    .iter()
                    .all(|r| e.buffer.text.is_char_boundary(r.start())
                        && e.buffer.text.is_char_boundary(r.end()))
            );
        });
        window.simulate_keystrokes("cmd-down");
        window.draw();
        assert!(window.read(|e, _| e.scroll > 0.));
    }

    #[test]
    fn composer_language_changes_labels_without_changing_the_draft() {
        use crate::i18n::{self, Language};
        let previous = i18n::language();
        i18n::set_language(Language::English);
        let mut app = gpui::TestApp::new();
        let mut window = app.open_window(|_, cx| FileEditor::composer(ThemeMode::Light, cx));
        window.update(|editor, _, cx| {
            editor.set_accessible_name("聊天输入框");
            editor.set_text_silently("English draft — 中文", cx);
        });
        window.read(|editor, _| {
            assert_eq!(editor.placeholder_text(), "Ask anything");
            assert_eq!(editor.accessible_label(), "Chat input");
        });
        i18n::set_language(Language::SimplifiedChinese);
        window.read(|editor, _| {
            assert_eq!(editor.placeholder_text(), "随心输入");
            assert_eq!(editor.accessible_label(), "聊天输入框");
            assert_eq!(editor.text(), "English draft — 中文");
        });
        i18n::set_language(previous);
    }

    #[test]
    fn unicode_selection_replacement_and_undo() {
        let mut b = Buffer {
            text: "中文👩‍💻\nabc".into(),
            ..Default::default()
        };
        assert_eq!(from_utf16(&b.text, 2), 6);
        assert_eq!(next(&b.text, 6), 17);
        assert_eq!(previous(&b.text, 17), 6);
        b.replace(6..17, "👋", true);
        assert_eq!(b.text, "中文👋\nabc");
        b.undo();
        assert_eq!(b.text, "中文👩‍💻\nabc");
        b.redo();
        assert_eq!(b.text, "中文👋\nabc");
    }
    #[test]
    fn new_edit_invalidates_redo() {
        let mut b = Buffer::default();
        b.replace(0..0, "a", true);
        b.undo();
        b.replace(0..0, "b", true);
        b.redo();
        assert_eq!(b.text, "b");
    }
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    use crate::agent::{AgentInputReference, AgentInputReferenceKind};
    fn buffer() -> Buffer {
        Buffer {
            text: "你好 OpenAI Docs 后文".into(),
            references: vec![AgentInputReference {
                range: 7..18,
                name: "openai-docs".into(),
                path: "/skills/OpenAI Docs/SKILL.md".into(),
                label: "OpenAI Docs".into(),
                kind: AgentInputReferenceKind::Skill,
                icon: None,
                brand_color: None,
            }],
            anchor: 7,
            cursor: 18,
            ..Default::default()
        }
    }
    #[test]
    fn deleting_part_of_a_chip_removes_it_atomically_and_undo_restores_identity() {
        let mut b = buffer();
        b.replace(10..11, "", true);
        assert_eq!(b.text, "你好  后文");
        assert!(b.references.is_empty());
        b.undo();
        assert_eq!(b.text, "你好 OpenAI Docs 后文");
        assert_eq!(b.references[0].path, "/skills/OpenAI Docs/SKILL.md");
        b.redo();
        assert!(b.references.is_empty());
    }
    #[test]
    fn typing_before_and_after_a_chip_moves_only_the_appropriate_utf8_ranges() {
        let mut b = buffer();
        b.replace(0..0, "👋", true);
        assert_eq!(b.references[0].range, 11..22);
        b.replace(22..22, "!", true);
        assert_eq!(b.references[0].range, 11..22);
        assert_eq!(&b.text[b.references[0].range.clone()], "OpenAI Docs");
        assert_eq!(b.snap_reference(15, false), 11);
        assert_eq!(b.snap_reference(15, true), 22);
        b.replace(15..15, "x", true);
        assert!(b.references.is_empty());
        assert_eq!(b.text, "👋你好 x! 后文");
    }
}
