//! Editable sketches with a shared vector document for native painting and PNG export.
use super::{ComposerView, sketch_image::ImageBackground, toast::ToastKind};
use crate::theme::{Theme, ThemeMode};
use gpui::{
    AnyElement, Bounds, Context, FocusHandle, Focusable, MouseButton, PathBuilder, Pixels, Point,
    Role, TextRun, Window, canvas, div, point, prelude::*, px, rgb,
};
use std::{cell::Cell, collections::HashMap, path::PathBuf, rc::Rc};

const SIDE: f32 = 760.;
const PALETTE: [u32; 13] = [
    0xffffff, 0x6b7280, 0x92400e, 0xdc2626, 0xf97316, 0xf59e0b, 0x16a34a, 0x0d9488, 0x06b6d4,
    0x2563eb, 0x4f46e5, 0x9333ea, 0xdb2777,
];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum Tool {
    Select,
    #[default]
    Pen,
    Text,
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Triangle,
    Diamond,
    Star,
    Heart,
    Eraser,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Mark {
    tool: Tool,
    color: u32,
    width: f32,
    points: Vec<(f32, f32)>,
    text: String,
}
impl Mark {
    fn bounds(&self) -> (f32, f32, f32, f32) {
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in &self.points {
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
        }
        if self.tool == Tool::Text {
            b.2 = b.0
                + self
                    .text
                    .lines()
                    .map(|l| l.chars().count())
                    .max()
                    .unwrap_or(0) as f32
                    * self.width
                    * 0.6;
            b.3 = b.1 + self.text.lines().count().max(1) as f32 * self.width * 1.25;
        }
        b
    }
    fn hit(&self, p: (f32, f32)) -> bool {
        if self.tool == Tool::Text {
            let (x, y, r, b) = self.bounds();
            return p.0 >= x - 4. && p.0 <= r + 4. && p.1 >= y - 4. && p.1 <= b + 4.;
        }
        let radius = (self.width / 2.).max(8.);
        self.geometry().windows(2).any(|line| {
            let (a, b) = (line[0], line[1]);
            let d = (b.0 - a.0, b.1 - a.1);
            let length = d.0 * d.0 + d.1 * d.1;
            let t = if length > 0. {
                ((p.0 - a.0) * d.0 + (p.1 - a.1) * d.1) / length
            } else {
                0.
            }
            .clamp(0., 1.);
            (p.0 - a.0 - t * d.0).hypot(p.1 - a.1 - t * d.1) <= radius
        })
    }
    fn geometry(&self) -> Vec<(f32, f32)> {
        if self.points.len() < 2 {
            return self.points.clone();
        }
        let a = self.points[0];
        let b = *self.points.last().unwrap();
        match self.tool {
            Tool::Rectangle => vec![a, (b.0, a.1), b, (a.0, b.1), a],
            Tool::Ellipse => (0..=96)
                .map(|i| {
                    let t = std::f32::consts::TAU * i as f32 / 96.;
                    (
                        (a.0 + b.0) / 2. + (b.0 - a.0) / 2. * t.cos(),
                        (a.1 + b.1) / 2. + (b.1 - a.1) / 2. * t.sin(),
                    )
                })
                .collect(),
            Tool::Line => vec![a, b],
            Tool::Arrow => {
                let angle = (b.1 - a.1).atan2(b.0 - a.0);
                let length = (self.width * 3.).max(12.);
                let wing = |delta: f32| {
                    (
                        b.0 - length * (angle + delta).cos(),
                        b.1 - length * (angle + delta).sin(),
                    )
                };
                vec![a, b, wing(0.5), b, wing(-0.5)]
            }
            Tool::Triangle => vec![
                ((a.0 + b.0) / 2., a.1),
                b,
                (a.0, b.1),
                ((a.0 + b.0) / 2., a.1),
            ],
            Tool::Diamond => vec![
                ((a.0 + b.0) / 2., a.1),
                (b.0, (a.1 + b.1) / 2.),
                ((a.0 + b.0) / 2., b.1),
                (a.0, (a.1 + b.1) / 2.),
                ((a.0 + b.0) / 2., a.1),
            ],
            Tool::Star => (0..=10)
                .map(|i| {
                    let t = std::f32::consts::PI * i as f32 / 5. - std::f32::consts::FRAC_PI_2;
                    let radius = if i % 2 == 0 { 0.5 } else { 0.22 };
                    (
                        (a.0 + b.0) / 2. + (b.0 - a.0) * radius * t.cos(),
                        (a.1 + b.1) / 2. + (b.1 - a.1) * radius * t.sin(),
                    )
                })
                .collect(),
            Tool::Heart => (0..=96)
                .map(|i| {
                    let t = std::f32::consts::TAU * i as f32 / 96.;
                    let x = 16. * t.sin().powi(3);
                    let y =
                        13. * t.cos() - 5. * (2. * t).cos() - 2. * (3. * t).cos() - (4. * t).cos();
                    (
                        (a.0 + b.0) / 2. + (b.0 - a.0) * x / 32.,
                        a.1 + (17. - y) * (b.1 - a.1) / 30.,
                    )
                })
                .collect(),
            _ => self.points.clone(),
        }
    }
}
#[derive(Clone, Default)]
struct SketchDocument {
    marks: Vec<Mark>,
    background: Option<ImageBackground>,
}

#[derive(Default)]
pub(super) struct SketchState {
    pub(super) open: bool,
    marks: Vec<Mark>,
    baseline: Vec<Mark>,
    discard_confirm: bool,
    discard_choice: usize,
    undo: Vec<Vec<Mark>>,
    redo: Vec<Vec<Mark>>,
    tool: Tool,
    color: Option<u32>,
    width: Option<f32>,
    active: Option<usize>,
    selected: Option<usize>,
    last: Option<(f32, f32)>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    focus: Option<FocusHandle>,
    focus_pending: bool,
    shape_menu: bool,
    width_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    color_panel_open: bool,
    text_input: Option<gpui::Entity<crate::components::file_editor::FileEditor>>,
    text_position: Option<(f32, f32)>,
    documents: HashMap<PathBuf, SketchDocument>,
    background: Option<ImageBackground>,
    editing_path: Option<PathBuf>,
}
impl SketchState {
    fn logical_height(&self) -> f32 {
        self.background
            .as_ref()
            .map_or(SIDE, ImageBackground::logical_height)
    }
    fn can_save(&self) -> bool {
        self.background.is_some() || !self.marks.is_empty()
    }
    fn checkpoint(&mut self) {
        if self.undo.last() != Some(&self.marks) {
            self.undo.push(self.marks.clone());
            if self.undo.len() > 128 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
    }
    fn undo(&mut self) {
        if let Some(marks) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.marks, marks));
            self.selected = None;
            self.active = None;
        }
    }
    fn redo(&mut self) {
        if let Some(marks) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.marks, marks));
            self.selected = None;
            self.active = None;
        }
    }
    fn position(&self, p: Point<Pixels>) -> Option<(f32, f32)> {
        let b = self.bounds.get()?;
        Some((
            (f32::from(p.x - b.origin.x) * SIDE / f32::from(b.size.width)).clamp(0., SIDE),
            (f32::from(p.y - b.origin.y) * self.logical_height() / f32::from(b.size.height))
                .clamp(0., self.logical_height()),
        ))
    }
    fn down(&mut self, p: (f32, f32)) {
        self.last = Some(p);
        if self.tool == Tool::Select {
            self.selected = self.marks.iter().rposition(|m| m.hit(p));
            if self.selected.is_some() {
                self.checkpoint();
            }
            return;
        }
        self.checkpoint();
        if self.tool == Tool::Eraser {
            self.marks.retain(|m| !m.hit(p));
            return;
        }
        if self.tool == Tool::Text {
            self.text_position = Some((p.0, (p.1 - 15.).max(0.)));
            return;
        }
        self.marks.push(Mark {
            tool: self.tool,
            color: self.color.unwrap_or(0xffffff),
            width: self.width.unwrap_or(12.),
            points: vec![p, p],
            text: String::new(),
        });
        self.active = Some(self.marks.len() - 1);
    }
    fn drag(&mut self, p: (f32, f32)) {
        if self.last.is_none() {
            return;
        }
        if self.tool == Tool::Eraser {
            self.marks.retain(|m| !m.hit(p));
        } else if self.tool == Tool::Select {
            if let (Some(i), Some(last)) = (self.selected, self.last) {
                for point in &mut self.marks[i].points {
                    point.0 += p.0 - last.0;
                    point.1 += p.1 - last.1;
                }
            }
        } else if let Some(i) = self.active {
            if self.tool == Tool::Pen {
                self.marks[i].points.push(p);
            } else {
                self.marks[i].points[1] = p;
            }
        }
        self.last = Some(p);
    }
}
impl ComposerView {
    pub(super) fn open_sketch(&mut self, cx: &mut Context<Self>) {
        self.sketch
            .color
            .get_or_insert(if self.mode == ThemeMode::Dark {
                0xffffff
            } else {
                0x000000
            });
        self.sketch.marks.clear();
        self.sketch.baseline.clear();
        self.sketch.undo.clear();
        self.sketch.redo.clear();
        self.sketch.editing_path = None;
        self.sketch.background = None;
        self.sketch.shape_menu = false;
        self.sketch.selected = None;
        self.sketch.active = None;
        self.sketch.text_position = None;
        self.sketch.discard_confirm = false;
        self.sketch.open = true;
        self.sketch.focus_pending = true;
        if self.sketch.focus.is_none() {
            self.sketch.focus = Some(cx.focus_handle());
        }
        if self.sketch.text_input.is_none() {
            let mode = self.mode;
            let input = cx.new(|cx| {
                let mut input = crate::components::file_editor::FileEditor::prose(mode, "", cx);
                input.set_text_metrics(24., 30., cx);
                input
            });
            self.sketch.text_input = Some(input);
        }
        cx.notify();
    }
    pub(super) fn reopen_sketch(&mut self, path: &std::path::Path, cx: &mut Context<Self>) -> bool {
        let Some(document) = self.sketch.documents.get(path).cloned() else {
            return false;
        };
        self.open_sketch(cx);
        self.sketch.baseline = document.marks.clone();
        self.sketch.marks = document.marks;
        self.sketch.background = document.background;
        self.sketch.undo.clear();
        self.sketch.redo.clear();
        self.sketch.active = None;
        self.sketch.selected = None;
        self.sketch.editing_path = Some(path.to_owned());
        true
    }
    pub(crate) fn can_annotate_attachment(&self, path: &std::path::Path) -> bool {
        self.prompt_context
            .files
            .iter()
            .any(|file| file.image && file.path == path)
    }
    pub(crate) fn annotate_attachment(
        &mut self,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.can_annotate_attachment(path) {
            return false;
        }
        if self.reopen_sketch(path, cx) {
            return true;
        }
        match ImageBackground::read(path) {
            Ok(background) => {
                self.open_sketch(cx);
                self.sketch.background = Some(background);
                self.sketch.editing_path = Some(path.to_owned());
                self.sketch.color = Some(0xdc2626);
                self.sketch.tool = Tool::Pen;
                true
            }
            Err(error) => {
                self.show_toast(ToastKind::Danger, error.to_string(), cx);
                false
            }
        }
    }
    fn request_close_sketch(&mut self, cx: &mut Context<Self>) {
        self.finish_sketch_text(cx);
        if self.sketch.marks != self.sketch.baseline {
            self.sketch.discard_confirm = true;
            self.sketch.discard_choice = 0;
            self.sketch.focus_pending = true;
        } else {
            self.sketch.open = false;
            self.focus_prompt_pending = true;
        }
        cx.notify();
    }
    fn choose_sketch_discard(&mut self, discard: bool, cx: &mut Context<Self>) {
        self.sketch.discard_confirm = false;
        if discard {
            self.sketch.open = false;
            self.sketch.marks = self.sketch.baseline.clone();
            self.focus_prompt_pending = true;
        } else {
            self.sketch.focus_pending = true;
        }
        cx.notify();
    }

    fn finish_sketch_text(&mut self, cx: &mut Context<Self>) {
        let Some(position) = self.sketch.text_position.take() else {
            return;
        };
        let input = self.sketch.text_input.as_ref().unwrap();
        let text = input.read(cx).text().to_owned();
        if !text.is_empty() {
            self.sketch.marks.push(Mark {
                tool: Tool::Text,
                color: self.sketch.color.unwrap_or(0xffffff),
                width: 24.,
                points: vec![position],
                text,
            });
        }
        input.update(cx, |input, cx| {
            let len = input.text().len();
            input.replace_range(0..len, "", cx);
        });
        self.sketch.focus_pending = true;
        cx.notify();
    }
    fn save_sketch(&mut self, cx: &mut Context<Self>) {
        self.finish_sketch_text(cx);
        if !self.sketch.can_save() {
            return;
        }
        if self
            .sketch
            .editing_path
            .as_ref()
            .is_some_and(|path| !self.can_annotate_attachment(path))
        {
            self.sketch.open = false;
            cx.notify();
            return;
        }
        let background = if self.mode == ThemeMode::Dark {
            0x212121
        } else {
            0xffffff
        };
        let result = export_png(
            &self.sketch.marks,
            background,
            self.sketch.background.as_ref(),
        )
        .and_then(|bytes| {
            let root = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("Home directory unavailable"))?
                .join("Library/Application Support/Echora/sketches");
            std::fs::create_dir_all(&root)?;
            let path = root.join(format!(
                "Sketch-{}.png",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ));
            std::fs::write(&path, bytes)?;
            Ok(path)
        });
        match result {
            Ok(path) => {
                // Each save is immutable: queued messages and undo retain their prior PNG.
                self.sketch.documents.insert(
                    path.clone(),
                    SketchDocument {
                        marks: self.sketch.marks.clone(),
                        background: self.sketch.background.clone(),
                    },
                );
                if let Some(old) = self.sketch.editing_path.replace(path.clone()) {
                    if let Some(file) = self
                        .prompt_context
                        .files
                        .iter_mut()
                        .find(|file| file.path == old)
                    {
                        file.path = path;
                        file.image = true;
                        self.draft_revision = self.draft_revision.wrapping_add(1);
                    }
                } else {
                    self.attach_paths(vec![path], cx);
                }
                self.sketch.open = false;
                self.focus_prompt_pending = true;
            }
            Err(error) => self.show_toast(ToastKind::Danger, error.to_string(), cx),
        }
        cx.notify();
    }
    fn set_sketch_width_at(&mut self, position: Point<Pixels>) {
        if let Some(bounds) = self.sketch.width_bounds.get() {
            let y = f32::from(position.y - bounds.origin.y);
            let width = (48. - (y - 16.) / 144. * 46.).round().clamp(2., 48.);
            self.sketch.width = Some(width);
            if let Some(i) = self.sketch.selected {
                self.sketch.marks[i].width = width;
            }
        }
    }
    fn open_sketch_color(&mut self, cx: &mut Context<Self>) {
        if self.sketch.color_panel_open {
            return;
        }
        self.sketch.color_panel_open = true;
        self.sketch.checkpoint();
        show_color_panel(self.sketch.color.unwrap_or(0xa855f7));
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(40))
                    .await;
                let Ok(open) = this.update(cx, |this, cx| {
                    let Some(color) = color_panel_value() else {
                        this.sketch.color_panel_open = false;
                        return false;
                    };
                    this.sketch.color = Some(color);
                    if let Some(i) = this.sketch.selected {
                        this.sketch.marks[i].color = color;
                    }
                    cx.notify();
                    true
                }) else {
                    break;
                };
                if !open {
                    break;
                }
            }
        })
        .detach();
    }
    pub(super) fn render_sketch(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.sketch.open {
            return None;
        }
        if self.sketch.focus_pending {
            self.sketch.focus_pending = false;
            if let Some(focus) = &self.sketch.focus {
                focus.focus(window, cx);
            }
        }
        let theme = Theme::for_mode(self.mode);
        let background = if self.mode == ThemeMode::Dark {
            0x212121
        } else {
            0xffffff
        };
        let logical_height = self.sketch.logical_height();
        let image_backed = self.sketch.background.is_some();
        let canvas_top = if image_backed { 48. } else { 0. };
        let chrome_height = if image_backed { 96. } else { 0. };
        let scale = ((f32::from(window.viewport_size().height) - 80. - chrome_height).max(80.)
            / logical_height)
            .min((f32::from(window.viewport_size().width) - 80.).max(80.) / SIDE)
            .min(1.);
        let side = SIDE * scale;
        let height = logical_height * scale;
        let editor_width = if image_backed {
            side.max(480.)
                .min(f32::from(window.viewport_size().width) - 40.)
        } else {
            side
        };
        let canvas_left = (editor_width - side) / 2.;
        let image = self
            .sketch
            .background
            .as_ref()
            .map(|background| background.preview.clone());
        let marks = self.sketch.marks.clone();
        let bounds = self.sketch.bounds.clone();
        let selected = self.sketch.selected;
        let paint = canvas(
            move |b, _, _| {
                bounds.set(Some(b));
            },
            move |b, _, window, cx| {
                let scale = f32::from(b.size.width) / SIDE;
                for (i, mark) in marks.iter().enumerate() {
                    if mark.tool == Tool::Text {
                        for (row, text) in mark.text.split('\n').enumerate() {
                            let run = TextRun {
                                len: text.len(),
                                font: crate::theme::ui_font(),
                                color: rgb(mark.color).into(),
                                ..Default::default()
                            };
                            let line = window.text_system().shape_line(
                                text.to_owned().into(),
                                px(mark.width * scale),
                                &[run],
                                None,
                            );
                            let p = mark.points[0];
                            let _ = line.paint(
                                b.origin
                                    + point(
                                        px(p.0 * scale),
                                        px((p.1 + row as f32 * mark.width * 1.25) * scale),
                                    ),
                                px(mark.width * 1.25 * scale),
                                gpui::TextAlign::Left,
                                None,
                                window,
                                cx,
                            );
                        }
                    } else {
                        let mut builder = PathBuilder::stroke(px(mark.width * scale));
                        for (i, (x, y)) in mark.geometry().into_iter().enumerate() {
                            let p = b.origin + point(px(x * scale), px(y * scale));
                            if i == 0 {
                                builder.move_to(p);
                            } else {
                                builder.line_to(p);
                            }
                        }
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, rgb(mark.color));
                        }
                        // Round caps and joins, shared with the SVG export.
                        let diameter = px(mark.width * scale);
                        for (x, y) in mark.geometry() {
                            let mut dot = gpui::fill(
                                Bounds::new(
                                    b.origin
                                        + point(
                                            px(x * scale) - diameter / 2.,
                                            px(y * scale) - diameter / 2.,
                                        ),
                                    gpui::size(diameter, diameter),
                                ),
                                rgb(mark.color),
                            );
                            dot.corner_radii = (diameter / 2.).into();
                            window.paint_quad(dot);
                        }
                    }
                    if selected == Some(i) {
                        let (x, y, r, d) = mark.bounds();
                        let mut path = PathBuilder::stroke(px(1.));
                        for (i, p) in [
                            (x - 4., y - 4.),
                            (r + 4., y - 4.),
                            (r + 4., d + 4.),
                            (x - 4., d + 4.),
                            (x - 4., y - 4.),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let p = b.origin + point(px(p.0 * scale), px(p.1 * scale));
                            if i == 0 {
                                path.move_to(p);
                            } else {
                                path.line_to(p);
                            }
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(path, rgb(0x339cff));
                        }
                    }
                }
            },
        )
        .size_full();
        let tools = [
            (Tool::Select, "选择", "Select", "sketch-select"),
            (Tool::Pen, "画笔", "Pen", "sketch-pen"),
            (Tool::Text, "文本", "Text", "sketch-text"),
            (Tool::Rectangle, "形状", "Shapes", "sketch-shapes"),
            (Tool::Eraser, "橡皮擦", "Eraser", "sketch-eraser"),
        ];
        let toolbar = div()
            .absolute()
            .top(px(8.))
            .left(px(editor_width / 2. - 78.))
            .h(px(36.))
            .px(px(4.))
            .rounded_full()
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.text.alpha(0.08))
            .flex()
            .items_center()
            .children(
                tools
                    .into_iter()
                    .enumerate()
                    .map(|(i, (tool, zh, en, glyph))| {
                        let selected = self.sketch.tool == tool
                            || (tool == Tool::Rectangle
                                && matches!(
                                    self.sketch.tool,
                                    Tool::Line
                                        | Tool::Ellipse
                                        | Tool::Arrow
                                        | Tool::Triangle
                                        | Tool::Diamond
                                        | Tool::Star
                                        | Tool::Heart
                                ));
                        div()
                            .id(("sketch-tool", i))
                            .role(Role::Button)
                            .aria_label(if crate::i18n::is_english() { en } else { zh })
                            .size(px(28.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .when(selected, |d| d.bg(theme.text.alpha(0.08)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.finish_sketch_text(cx);
                                if tool == Tool::Rectangle {
                                    this.sketch.shape_menu = !this.sketch.shape_menu;
                                }
                                this.sketch.tool = tool;
                                cx.notify();
                            }))
                            .child(
                                super::super::icons::icon(glyph, theme.text.into()).size(px(16.)),
                            )
                    }),
            );
        Some(
            div()
                .id("sketch-overlay")
                .absolute()
                .inset_0()
                .bg(if self.mode == ThemeMode::Dark { gpui::rgba(0x00000020) } else { gpui::rgba(0xffffff00) })
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("sketch-editor")
                        .role(Role::Dialog)
                        .aria_label(if self.sketch.background.is_some() { crate::i18n::format!("图片标注"=>"Image markup") } else { crate::i18n::format!("草图编辑器"=>"Sketch editor") })
                        .track_focus(self.sketch.focus.as_ref().unwrap())
                        .on_action(cx.listener(|this, _: &crate::app::DismissPermissionUi, _, cx| {
                            if this.sketch.discard_confirm {
                                this.choose_sketch_discard(false, cx);
                            } else if this.sketch.text_position.is_some() {
                                this.finish_sketch_text(cx);
                            } else {
                                this.request_close_sketch(cx);
                            }
                            cx.stop_propagation();
                        }))
                        .relative()
                        .w(px(editor_width))
                        .h(px(height + chrome_height))
                        .rounded(px(20.))
                        .overflow_hidden()
                        .bg(if image_backed { theme.elevated } else { rgb(background) })
                        .when(self.mode == ThemeMode::Light, |d| d.border_1().border_color(theme.text.alpha(0.05)).shadow(vec![gpui::BoxShadow::new(px(0.), px(2.), gpui::rgba(0x0000000d).into()).blur_radius(px(5.))]))
                        .text_color(theme.text)
                        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                            let key = event.keystroke.key.as_str();
                            let cmd = event.keystroke.modifiers.platform;
                            if this.sketch.discard_confirm {
                                match key {
                                    "escape"=>this.choose_sketch_discard(false,cx),
                                    "tab"|"up"|"down"=>{this.sketch.discard_choice=1-this.sketch.discard_choice;cx.notify();},
                                    "enter"|"space"=>this.choose_sketch_discard(this.sketch.discard_choice==1,cx),
                                    _=>{},
                                }
                                cx.stop_propagation();return;
                            }
                            if this.sketch.text_position.is_some() {
                                if key == "escape" {this.finish_sketch_text(cx);cx.stop_propagation();}
                                return;
                            }
                            if key == "escape" {
                                this.request_close_sketch(cx);
                            } else if cmd && key == "z" {
                                if event.keystroke.modifiers.shift {
                                    this.sketch.redo();
                                } else {
                                    this.sketch.undo();
                                }
                            } else if matches!(key, "backspace" | "delete") {
                                if let Some(i) = this.sketch.selected.take() {
                                    this.sketch.checkpoint();
                                    this.sketch.marks.remove(i);
                                }
                            } else if this.sketch.text_position.is_some() {
                                return;
                            } else {
                                this.sketch.tool = match key {
                                    "v" => Tool::Select,
                                    "p" => Tool::Pen,
                                    "t" => Tool::Text,
                                    "r" => Tool::Rectangle,
                                    "o" => Tool::Ellipse,
                                    "l" => Tool::Line,
                                    "e" => Tool::Eraser,
                                    _ => return,
                                };
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }))
                        .child(
                            div()
                                .id("sketch-canvas")
                                .role(Role::Image)
                                .aria_label(if self.sketch.background.is_some() { crate::i18n::format!("图片标注画布"=>"Image markup canvas") } else { crate::i18n::format!("草图画布"=>"Sketch canvas") })
                                .absolute().top(px(canvas_top)).left(px(canvas_left)).w(px(side)).h(px(height))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        |this, event: &gpui::MouseDownEvent, window, cx| {
                                            this.finish_sketch_text(cx);
                                            if let Some(p) = this.sketch.position(event.position) {
                                                this.sketch.down(p);
                                            }
                                            if this.sketch.text_position.is_some()
                                                && let Some(input) = &this.sketch.text_input {
                                                    input
                                                        .read(cx)
                                                        .focus_handle(cx)
                                                        .focus(window, cx);
                                                }
                                            cx.stop_propagation();
                                            cx.notify();
                                        },
                                    ),
                                )
                                .on_mouse_move(cx.listener(
                                    |this, event: &gpui::MouseMoveEvent, _, cx| {
                                        if event.pressed_button == Some(MouseButton::Left)
                                            && let Some(p) = this.sketch.position(event.position) {
                                                this.sketch.drag(p);
                                                cx.notify();
                                            }
                                    },
                                ))
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.sketch.active = None;
                                        this.sketch.last = None;
                                        if this.sketch.undo.last()==Some(&this.sketch.marks) {this.sketch.undo.pop();}
                                        cx.notify();
                                    }),
                                )
                                .relative()
                                .when_some(image, |canvas, image| canvas.child(gpui::img(image).absolute().inset_0().size_full().object_fit(gpui::ObjectFit::Fill)))
                                .child(paint),
                        )
                        .child(toolbar)
                        .child(
                            div()
                                .id("sketch-close")
                                .role(Role::Button)
                                .aria_label(
                                    if self.sketch.background.is_some() { crate::i18n::format!("取消标注"=>"Cancel markup") } else { crate::i18n::format!("关闭草图编辑器"=>"Close sketch editor") },
                                )
                                .absolute()
                                .left(px(8.))
                                .top(px(8.))
                                .size(px(32.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.request_close_sketch(cx);
                                }))
                                .child("×"),
                        )
                        .child(div().absolute().right(px(8.)).top(px(8.)).flex().children(
                            [false, true].into_iter().map(|redo| {
                                div()
                                    .id(if redo { "sketch-redo" } else { "sketch-undo" })
                                    .role(Role::Button)
                                    .aria_label(if redo {
                                        crate::i18n::text("重做")
                                    } else {
                                        crate::i18n::text("撤销")
                                    })
                                    .size(px(32.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .opacity(
                                        if if redo {
                                            self.sketch.redo.is_empty()
                                        } else {
                                            self.sketch.undo.is_empty()
                                        } {
                                            0.3
                                        } else {
                                            1.
                                        },
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if redo {
                                            this.sketch.redo();
                                        } else {
                                            this.sketch.undo();
                                        }
                                        cx.notify();
                                    }))
                                    .child(super::super::icons::icon(if redo { "sketch-redo" } else { "sketch-undo" },theme.text.into()).size(px(16.)))
                            }),
                        ))
                        .when(self.sketch.shape_menu, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .top(px(50.))
                                    .left(px(editor_width / 2. - 144.))
                                    .p(px(8.))
                                    .rounded(px(12.))
                                    .bg(theme.elevated)
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .flex()
                                    .children(
                                        [
                                            (Tool::Line, "sketch-line"),
                                            (Tool::Arrow, "sketch-arrow"),
                                            (Tool::Rectangle, "sketch-rectangle"),
                                            (Tool::Ellipse, "sketch-ellipse"),
                                            (Tool::Triangle, "sketch-triangle"),
                                            (Tool::Diamond, "sketch-diamond"),
                                            (Tool::Star, "sketch-star"),
                                            (Tool::Heart, "sketch-heart"),
                                        ]
                                        .into_iter()
                                        .enumerate()
                                        .map(
                                            |(i, (tool, label))| {
                                                div()
                                                    .id(("sketch-shape", i))
                                                    .role(Role::Button)
                                                    .aria_label(crate::i18n::text(["直线","箭头","矩形","椭圆","三角形","菱形","星形","心形"][i]))
                                                    .flex().items_center().justify_center()
                                                    .size(px(32.))
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.sketch.tool = tool;
                                                        this.sketch.shape_menu = false;
                                                        cx.stop_propagation();
                                                        cx.notify();
                                                    }))
                                                    .child(super::super::icons::icon(label,theme.text.into()).size(px(16.)))
                                            },
                                        ),
                                    ),
                            )
                        })
                        .child({
                            let bounds = self.sketch.width_bounds.clone();
                            let track_color = theme.text.alpha(0.2);
                            div().id("sketch-width-slider").role(Role::Slider).aria_label(crate::i18n::format!("笔画宽度"=>"Stroke width")).aria_value(format!("{}",self.sketch.width.unwrap_or(12.)))
                                .absolute().left(px(8.)).top(px(canvas_top + height/2.-88.)).w(px(32.)).h(px(176.)).rounded_full().bg(theme.elevated).cursor_pointer()
                                .on_mouse_down(MouseButton::Left,cx.listener(|this,e: &gpui::MouseDownEvent,_,cx| {this.sketch.checkpoint();this.set_sketch_width_at(e.position);cx.stop_propagation();cx.notify();}))
                                .on_mouse_move(cx.listener(|this,e: &gpui::MouseMoveEvent,_,cx| {if e.pressed_button==Some(MouseButton::Left){this.set_sketch_width_at(e.position);cx.stop_propagation();cx.notify();}}))
                                .child(canvas(move |b,_,_| {bounds.set(Some(b));},move |b,_,window,_| {
                                    let mut path = PathBuilder::fill();
                                    path.move_to(b.origin + point(px(6.), px(8.)));
                                    path.line_to(b.origin + point(px(26.), px(8.)));
                                    path.line_to(b.origin + point(px(16.), px(168.)));
                                    path.close();
                                    if let Ok(path) = path.build() { window.paint_path(path, track_color); }
                                }).size_full())
                                .child(div().absolute().left(px(8.)).top(px((48.-self.sketch.width.unwrap_or(12.))/46.*144.+8.)).size(px(16.)).rounded_full().bg(gpui::rgb(0xffffff)).border_1().border_color(gpui::rgba(0x00000033)))
                        })
                        .child(
                            div()
                                .absolute()
                                .bottom(px(14.))
                                .left(px((editor_width - (PALETTE.len()+1) as f32 * 28.) / 2.))
                                .flex()
                                .gap(px(8.))
                                .child(div().id("sketch-custom-color").role(Role::Button).aria_label(crate::i18n::format!("选择自定义颜色"=>"Choose custom color")).size(px(20.)).rounded_full().cursor_pointer().on_click(cx.listener(|this,_,_,cx| this.open_sketch_color(cx))).child(gpui::img("icons/sketch-color.svg").size(px(20.))))
                                .children(PALETTE.into_iter().enumerate().map(|(i, color)| {
                                    let color = if i == 0 && self.mode == ThemeMode::Light { 0x000000 } else { color };
                                    div()
                                        .id(("sketch-color", i))
                                        .role(Role::Button)
                                        .aria_label(format!("#{color:06x}"))
                                        .size(px(20.))
                                        .rounded_full()
                                        .bg(rgb(color))
                                        .border_1()
                                        .border_color(
                                            if self.sketch.color.unwrap_or(0xffffff) == color {
                                                rgb(0x339cff)
                                            } else {
                                                theme.text.alpha(0.2)
                                            },
                                        )
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.sketch.checkpoint();
                                            this.sketch.color = Some(color);
                                            if let Some(i) = this.sketch.selected { this.sketch.marks[i].color=color; }
                                            cx.notify();
                                        }))
                                })),
                        )
                        .child(
                            div()
                                .id("sketch-save")
                                .role(Role::Button)
                                .aria_label(if self.sketch.background.is_some() { crate::i18n::format!("保存标注"=>"Save markup") } else { crate::i18n::format!("附加草图"=>"Attach sketch") })
                                .absolute()
                                .right(px(8.))
                                .bottom(px(8.))
                                .size(px(28.))
                                .rounded_full()
                                .bg(theme.button)
                                .text_color(theme.button_text)
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .opacity(if !self.sketch.can_save() {
                                    0.4
                                } else {
                                    1.
                                })
                                .on_click(cx.listener(|this, _, _, cx| this.save_sketch(cx)))
                                .child(super::super::icons::icon("sketch-save",theme.button_text.into()).size(px(16.))),
                        )
                        .when_some(self.sketch.text_position, |d, p| {
                            d.child(
                                div()
                                    .absolute()
                                    .left(px(canvas_left + p.0 * side / SIDE))
                                    .top(px(canvas_top + p.1 * side / SIDE))
                                    .w(px((SIDE-p.0).min(224.)*side/SIDE))
                                    .h(px(self.sketch.text_input.as_ref().map_or(30., |input| (input.read(cx).text().lines().count().max(1) as f32 * 30.).max(30.))))
                                    .border_1()
                                    .border_color(rgb(0x339cff))
                                    .child(self.sketch.text_input.as_ref().unwrap().clone()),
                            )
                        }),
                )
                .when(self.sketch.discard_confirm, |d| d.child(
                    div().absolute().inset_0().bg(gpui::rgba(0x00000066)).flex().items_center().justify_center()
                    .on_mouse_down(MouseButton::Left,|_,_,cx|cx.stop_propagation())
                    .child(div().id("sketch-discard-dialog").role(Role::Dialog).aria_label(crate::i18n::format!("放弃更改？"=>"Discard changes?"))
                        .w(px(420.)).max_w_full().h(px(262.)).rounded(px(25.)).bg(theme.elevated).text_color(theme.text).flex().flex_col()
                        .child(div().h(px(72.)).flex().items_center().justify_center().text_size(px(20.)).font_weight(gpui::FontWeight::SEMIBOLD).child(crate::i18n::format!("放弃更改？"=>"Discard changes?")))
                        .child(div().px(px(16.)).flex().flex_col().gap(px(12.))
                            .child(div().mb(px(14.)).text_size(px(16.)).line_height(px(24.)).text_center().text_color(theme.text_tertiary).child(crate::i18n::format!("你的更改不会被保存"=>"Your changes won’t be saved")))
                            .children([false,true].into_iter().enumerate().map(|(i,discard)| div().id(("sketch-discard-choice",i)).role(Role::Button).aria_label(if discard {crate::i18n::format!("放弃更改"=>"Discard changes")} else {crate::i18n::format!("继续编辑"=>"Keep editing")})
                                .h(px(44.)).rounded_full().bg(if discard {gpui::rgb(0xe55b58).alpha(0.1)} else {theme.button}).text_color(if discard {gpui::rgb(0xe55b58)} else {theme.button_text})
                                .when(self.sketch.discard_choice==i, |d|d.border_2().border_color(theme.text.alpha(0.25))).cursor_pointer().flex().items_center().justify_center()
                                .on_click(cx.listener(move |this,_,_,cx|this.choose_sketch_discard(discard,cx))).child(if discard {crate::i18n::format!("放弃更改"=>"Discard changes")} else {crate::i18n::format!("继续编辑"=>"Keep editing")})))
                        )
                    )
                ))
                .into_any_element(),
        )
    }
}
fn export_png(
    marks: &[Mark],
    background: u32,
    image: Option<&ImageBackground>,
) -> anyhow::Result<Vec<u8>> {
    let (width, height, logical_height) = image.map_or((760, 760, SIDE), |image| {
        (
            image.pixels.width(),
            image.pixels.height(),
            image.logical_height(),
        )
    });
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 760 {logical_height}\">"
    );
    if image.is_none() {
        svg.push_str(&format!(
            "<rect width=\"760\" height=\"760\" fill=\"#{background:06x}\"/>"
        ));
    }
    for mark in marks {
        if mark.tool == Tool::Text {
            let p = mark.points[0];
            let text = mark
                .text
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            for (row, line) in text.split('\n').enumerate() {
                svg.push_str(&format!("<text x=\"{}\" y=\"{}\" font-family=\"sans-serif\" font-size=\"{}\" fill=\"#{:06x}\">{line}</text>",p.0,p.1+mark.width+row as f32*mark.width*1.25,mark.width,mark.color));
            }
        } else {
            let points = mark
                .geometry()
                .iter()
                .map(|(x, y)| format!("{x},{y}"))
                .collect::<Vec<_>>()
                .join(" ");
            svg.push_str(&format!("<polyline points=\"{points}\" fill=\"none\" stroke=\"#{:06x}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",mark.color,mark.width));
        }
    }
    svg.push_str("</svg>");
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(&svg, &options)?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("Could not allocate sketch image"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    if let Some(image) = image {
        let mut combined = (*image.pixels).clone();
        combined.draw_pixmap(
            0,
            0,
            pixmap.as_ref(),
            &resvg::tiny_skia::PixmapPaint::default(),
            resvg::tiny_skia::Transform::identity(),
            None,
        );
        pixmap = combined;
    }
    Ok(pixmap.encode_png()?)
}

#[cfg(target_os = "macos")]
fn show_color_panel(color: u32) {
    use objc::{
        Message,
        runtime::{Class, Object, Sel},
    };
    // AppKit calls stay on GPUI's main thread; the shared panel owns its color.
    unsafe {
        let Some(class) = Class::get("NSColorPanel") else {
            return;
        };
        let Ok(panel): Result<*mut Object, _> =
            class.send_message(Sel::register("sharedColorPanel"), ())
        else {
            return;
        };
        let Some(panel) = panel.as_ref() else {
            return;
        };
        let Some(class) = Class::get("NSColor") else {
            return;
        };
        let Ok(value): Result<*mut Object, _> = class.send_message(
            Sel::register("colorWithSRGBRed:green:blue:alpha:"),
            (
                ((color >> 16) & 255) as f64 / 255.,
                ((color >> 8) & 255) as f64 / 255.,
                (color & 255) as f64 / 255.,
                1f64,
            ),
        ) else {
            return;
        };
        let _: Result<(), _> = panel.send_message(Sel::register("setColor:"), (value,));
        let _: Result<(), _> = panel.send_message(
            Sel::register("orderFront:"),
            (std::ptr::null_mut::<Object>(),),
        );
    }
}
#[cfg(target_os = "macos")]
fn color_panel_value() -> Option<u32> {
    use objc::{
        Message,
        runtime::{BOOL, Class, Object, Sel, YES},
    };
    unsafe {
        let panel: *mut Object = Class::get("NSColorPanel")?
            .send_message(Sel::register("sharedColorPanel"), ())
            .ok()?;
        let panel = panel.as_ref()?;
        let visible: BOOL = panel.send_message(Sel::register("isVisible"), ()).ok()?;
        if visible != YES {
            return None;
        }
        let color: *mut Object = panel.send_message(Sel::register("color"), ()).ok()?;
        let space: *mut Object = Class::get("NSColorSpace")?
            .send_message(Sel::register("sRGBColorSpace"), ())
            .ok()?;
        let color: *mut Object = color
            .as_ref()?
            .send_message(Sel::register("colorUsingColorSpace:"), (space,))
            .ok()?;
        let color = color.as_ref()?;
        let r: f64 = color.send_message(Sel::register("redComponent"), ()).ok()?;
        let g: f64 = color
            .send_message(Sel::register("greenComponent"), ())
            .ok()?;
        let b: f64 = color
            .send_message(Sel::register("blueComponent"), ())
            .ok()?;
        Some(
            ((r * 255.).round() as u32) << 16
                | ((g * 255.).round() as u32) << 8
                | (b * 255.).round() as u32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(width: u32, height: u32) -> Vec<u8> {
        let mut image = resvg::tiny_skia::Pixmap::new(width, height).unwrap();
        image.fill(resvg::tiny_skia::Color::from_rgba8(30, 70, 150, 255));
        image.encode_png().unwrap()
    }

    #[test]
    fn attachment_export_preserves_source_dimensions_pixels_and_markup_coordinates() {
        for (width, height) in [(1200, 600), (300, 900)] {
            let source = fixture(width, height);
            let path = std::env::temp_dir().join(format!(
                "echora-markup-export-{}-{width}.png",
                std::process::id()
            ));
            std::fs::write(&path, &source).unwrap();
            let background = ImageBackground::read(&path).unwrap();
            let mark = Mark {
                tool: Tool::Line,
                color: 0xff0000,
                width: 10.,
                points: vec![(100., 100.), (200., 100.)],
                text: String::new(),
            };
            let bytes = export_png(&[mark], 0xffffff, Some(&background)).unwrap();
            let image = resvg::tiny_skia::Pixmap::decode_png(&bytes).unwrap();
            assert_eq!((image.width(), image.height()), (width, height));
            assert_eq!(
                image.pixel(width - 10, height - 10),
                background.pixels.pixel(width - 10, height - 10)
            );
            assert!(
                image
                    .pixel(
                        (150. * width as f32 / SIDE) as u32,
                        (100. * width as f32 / SIDE) as u32
                    )
                    .unwrap()
                    .red()
                    > 240
            );
            assert_eq!(std::fs::read(&path).unwrap(), source);
            // Removing all vector marks restores the original raster, not a flattened prior edit.
            let erased = export_png(&[], 0, Some(&background)).unwrap();
            assert_eq!(
                resvg::tiny_skia::Pixmap::decode_png(&erased).unwrap(),
                *background.pixels
            );
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn attachment_save_reopen_cancel_and_replace_leave_other_context_and_prior_pngs_intact() {
        let source = fixture(640, 400);
        let path =
            std::env::temp_dir().join(format!("echora-markup-draft-{}.png", std::process::id()));
        std::fs::write(&path, &source).unwrap();
        let other = PathBuf::from("/tmp/keep-context.txt");
        let mut app = gpui::TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        app.update_entity(&composer, |composer, cx| {
            composer.attach_paths(vec![path.clone(), other.clone()], cx);
            assert!(composer.annotate_attachment(&path, cx));
            composer.sketch.down((100., 120.));
            composer.sketch.drag((200., 120.));
            composer.request_close_sketch(cx);
            assert!(composer.sketch.discard_confirm);
            composer.choose_sketch_discard(true, cx);
            assert_eq!(composer.prompt_context.files[0].path, path);
            assert_eq!(std::fs::read(&path).unwrap(), source);
            assert!(composer.annotate_attachment(&path, cx));
            composer.sketch.down((100., 120.));
            composer.sketch.drag((200., 120.));
            composer.save_sketch(cx);
            let first = composer.prompt_context.files[0].path.clone();
            assert_ne!(first, path);
            assert_eq!(composer.prompt_context.files[1].path, other);
            let prior_message = composer.prompt_context.files.clone();
            let first_bytes = std::fs::read(&first).unwrap();
            assert!(composer.reopen_sketch(&first, cx));
            assert_eq!(composer.sketch.marks.len(), 1);
            composer.sketch.marks.clear();
            composer.save_sketch(cx);
            let second = composer.prompt_context.files[0].path.clone();
            assert_ne!(second, first);
            assert_eq!(prior_message[0].path, first);
            assert_eq!(std::fs::read(&first).unwrap(), first_bytes);
            assert_eq!(
                resvg::tiny_skia::Pixmap::decode_png(&std::fs::read(&second).unwrap()).unwrap(),
                resvg::tiny_skia::Pixmap::decode_png(&source).unwrap()
            );
            assert_eq!(std::fs::read(&path).unwrap(), source);
            composer.prompt_context.files.clear();
            assert!(!composer.annotate_attachment(&second, cx));
            std::fs::remove_file(first).unwrap();
            std::fs::remove_file(second).unwrap();
        });
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn select_drag_erase_undo_and_png_export_preserve_the_document() {
        let mut state = SketchState {
            color: Some(0xff0000),
            width: Some(6.),
            ..Default::default()
        };
        state.down((50., 50.));
        state.drag((150., 100.));
        state.active = None;
        state.last = None;
        state.tool = Tool::Select;
        state.down((70., 60.));
        state.drag((80., 70.));
        state.last = None;
        assert_eq!(state.marks[0].points[0], (60., 60.));
        state.tool = Tool::Eraser;
        state.down((100., 80.));
        assert!(state.marks.is_empty());
        state.undo();
        assert_eq!(state.marks.len(), 1);
        let bytes = export_png(&state.marks, 0xffffff, None).unwrap();
        assert_eq!(
            crate::media::encoded_image_dimensions(&bytes),
            Some((760, 760))
        );
        let image = resvg::tiny_skia::Pixmap::decode_png(&bytes).unwrap();
        let p = image.pixel(60, 60).unwrap();
        assert!(
            p.red() > 240 && p.green() < 30,
            "export contains the moved red stroke"
        );
        state.redo();
        assert!(state.marks.is_empty());
    }
}
#[cfg(not(target_os = "macos"))]
fn show_color_panel(_: u32) {}
#[cfg(not(target_os = "macos"))]
fn color_panel_value() -> Option<u32> {
    None
}
