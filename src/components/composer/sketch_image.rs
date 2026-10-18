//! Immutable raster backing for attachment markup; vector edits stay in `sketch`.
use std::{io::Cursor, path::Path, sync::Arc};

use resvg::tiny_skia::Pixmap;

#[derive(Clone)]
pub(super) struct ImageBackground {
    pub(super) pixels: Arc<Pixmap>,
    pub(super) preview: Arc<gpui::Image>,
}

impl ImageBackground {
    pub(super) fn read(path: &Path) -> anyhow::Result<Self> {
        Self::decode(&std::fs::read(path)?)
    }

    fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        let image = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()?
            .decode()?
            .into_rgba8();
        let (width, height) = image.dimensions();
        let mut rgba = image.into_raw();
        // tiny-skia stores premultiplied RGBA; PNG and image store straight RGBA.
        for p in rgba.chunks_exact_mut(4) {
            for i in 0..3 {
                p[i] = ((u16::from(p[i]) * u16::from(p[3]) + 127) / 255) as u8;
            }
        }
        let pixels = Pixmap::from_vec(
            rgba,
            resvg::tiny_skia::IntSize::from_wh(width, height)
                .ok_or_else(|| anyhow::anyhow!("Invalid image dimensions"))?,
        )
        .ok_or_else(|| anyhow::anyhow!("Could not decode attachment image"))?;
        let preview = Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            pixels.encode_png()?,
        ));
        Ok(Self {
            pixels: Arc::new(pixels),
            preview,
        })
    }

    pub(super) fn logical_height(&self) -> f32 {
        760. * self.pixels.height() as f32 / self.pixels.width() as f32
    }
}
