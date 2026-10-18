//! Convert Metal's premultiplied readback to PNG's straight-alpha pixels.

pub(crate) fn straight_alpha(mut pixel: [u8; 4]) -> [u8; 4] {
    let alpha = u32::from(pixel[3]);
    if alpha == 0 {
        return [0; 4];
    }
    for channel in &mut pixel[..3] {
        *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
    }
    pixel
}

#[cfg(test)]
mod tests {
    use super::straight_alpha;

    #[test]
    fn translucent_sidebar_retains_reference_white_and_alpha() {
        // ChatGPT's CDP PNG stores white at 70% opacity as [255,255,255,178].
        // Metal readback stores the same paint with RGB multiplied by alpha.
        assert_eq!(straight_alpha([178, 178, 178, 178]), [255, 255, 255, 178]);
        assert_eq!(straight_alpha([28, 28, 28, 178]), [40, 40, 40, 178]);
    }

    #[test]
    fn transparent_opaque_and_colored_pixels_keep_valid_png_values() {
        assert_eq!(straight_alpha([0, 0, 0, 0]), [0, 0, 0, 0]);
        assert_eq!(straight_alpha([26, 28, 31, 255]), [26, 28, 31, 255]);
        assert_eq!(straight_alpha([64, 32, 16, 128]), [128, 64, 32, 128]);
        assert_eq!(straight_alpha([129, 0, 0, 128]), [255, 0, 0, 128]);
    }
}
