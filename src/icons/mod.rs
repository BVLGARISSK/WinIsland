pub mod brightness;
pub mod controls;
pub mod music;
pub mod settings;
pub mod volume;

use image::imageops::FilterType;
use winisland_render::{Image, Rgba};

pub(crate) fn load_glyph(bytes: &[u8]) -> Option<Image> {
    load_tinted_glyph(bytes, Rgba::WHITE).map(|(image, _)| image)
}

/// Loads a glyph cropped to its visible pixels, filled with `color`, plus its width-to-height
/// ratio.
pub(crate) fn load_tinted_glyph(bytes: &[u8], color: Rgba) -> Option<(Image, f32)> {
    let source = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = source.dimensions();
    let mut bounds = (width, height, 0, 0);
    for (x, y, pixel) in source.enumerate_pixels() {
        if pixel[3] > 8 {
            bounds.0 = bounds.0.min(x);
            bounds.1 = bounds.1.min(y);
            bounds.2 = bounds.2.max(x);
            bounds.3 = bounds.3.max(y);
        }
    }
    if bounds.0 > bounds.2 || bounds.1 > bounds.3 {
        return None;
    }
    let cropped = image::imageops::crop_imm(
        &source,
        bounds.0,
        bounds.1,
        bounds.2 - bounds.0 + 1,
        bounds.3 - bounds.1 + 1,
    )
    .to_image();
    let aspect = cropped.width() as f32 / cropped.height() as f32;
    let small = image::imageops::resize(&cropped, 64, 64, FilterType::Lanczos3);
    let mut pixels = small.into_raw();
    let tint = |channel: u8, alpha: u8| (u16::from(channel) * u16::from(alpha) / 255) as u8;
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[0] = tint(color.r(), pixel[3]);
        pixel[1] = tint(color.g(), pixel[3]);
        pixel[2] = tint(color.b(), pixel[3]);
    }
    Image::from_rgba8_premul(64, 64, &pixels).map(|image| (image, aspect))
}
