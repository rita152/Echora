//! Live reference 26.1013: UI tokens round, prose uses ems, fenced code has
//! a 20px minimum line box, and files/diffs use 1.8em. Font families are separate.

use crate::workspace::FontSizes;
use std::cell::Cell;

// Rendering and text measurement run on the UI thread. Keep policy here so
// helper-only renderers share it without domain entities or GPUI contexts.
thread_local! { static SIZES: Cell<FontSizes> = Cell::new(FontSizes::default()); }

pub fn sizes() -> FontSizes {
    SIZES.get()
}
pub fn set_sizes(sizes: FontSizes) {
    SIZES.set(sizes);
}

fn token(sizes: FontSizes, baseline: f32) -> f32 {
    if sizes.ui() == 14.0 {
        return baseline;
    }
    (baseline * sizes.ui() / 14.0).round()
}

/// The reference's --text-* hierarchy (14/13/12 etc.), not a single size.
pub fn ui_size(baseline: f32) -> f32 {
    token(sizes(), baseline)
}

/// Relative UI line heights; fixed CSS leading (18px/20px etc.) stays fixed
/// at its rendering site instead of passing through this function.
pub fn ui_leading(baseline_size: f32, baseline_height: f32) -> f32 {
    baseline_height * (ui_size(baseline_size) / baseline_size)
}

pub fn prose_size(baseline: f32) -> f32 {
    baseline * (sizes().ui() / 14.0)
}
pub fn code_size() -> f32 {
    sizes().code()
}
pub fn code_line_height() -> f32 {
    (code_size() * 1.5).max(20.0)
}
pub fn diff_line_height() -> f32 {
    code_size() * 1.8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_tokens_match_live_reference_rounding_and_keep_their_hierarchy() {
        for (value, expected) in [
            (11.0, [11., 10., 9., 19.]),
            (14.0, [14., 13., 12., 24.]),
            (14.5, [15., 13., 12., 25.]),
            (16.0, [16., 15., 14., 27.]),
        ] {
            let sizes = FontSizes::default().with_value(false, value).unwrap();
            assert_eq!(
                [14., 13., 12., 24.].map(|baseline| token(sizes, baseline)),
                expected
            );
        }
    }
}
