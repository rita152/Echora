use gpui::{Hsla, Svg, prelude::*, px, svg};

pub fn icon(name: &'static str, color: Hsla) -> Svg {
    svg()
        .path(format!("icons/{name}.svg"))
        .size(px(16.0))
        .text_color(color)
}
