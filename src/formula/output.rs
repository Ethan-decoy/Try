//! One coordinate system and resource limit for all display backends.
use super::{HitRegion, NativeText, RenderedFormula};
use crate::tr;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const MAX_BITMAP_PIXELS: u64 = 1024 * 512;

pub(super) fn scale(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(1.0, 3.0)
    } else {
        1.0
    }
}

pub(super) fn check_size(width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_BITMAP_PIXELS {
        return Err(tr!("这条公式的显示范围过大，请拆成更短的题目。").into());
    }
    Ok(())
}

/// Keep a vector backend's canvas (and baseline spacing), or crop a structured
/// image's editor padding. In both cases include native line boxes, translate
/// every layer together and provide a root hit region for an opaque formula.
pub(super) fn finish(
    source: SharedPixelBuffer<Rgba8Pixel>,
    scale: f32,
    canvas: (f32, f32),
    mut texts: Vec<NativeText>,
    mut regions: Vec<HitRegion>,
    crop_to_ink: bool,
) -> Result<RenderedFormula, String> {
    let (source_width, source_height) = (source.width(), source.height());
    let (mut left, mut top, mut right, mut bottom) = if crop_to_ink {
        (
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        )
    } else {
        (0.0, 0.0, canvas.0, canvas.1)
    };
    if crop_to_ink {
        for (index, pixel) in source.as_slice().iter().enumerate() {
            if pixel.a != 0 {
                let (x, y) = (index as u32 % source_width, index as u32 / source_width);
                left = left.min(x as f32 / scale);
                top = top.min(y as f32 / scale);
                right = right.max((x + 1) as f32 / scale);
                bottom = bottom.max((y + 1) as f32 / scale);
            }
        }
    }
    for text in &texts {
        left = left.min(text.x);
        top = top.min(text.y);
        right = right.max(text.x + text.width);
        bottom = bottom.max(text.y + text.height);
    }
    if ![left, top, right, bottom]
        .iter()
        .all(|value| value.is_finite())
        || left >= right
        || top >= bottom
    {
        return Err(tr!("无法建立公式图片。").into());
    }
    // Align the origin to physical pixels so the math image is copied exactly.
    // A cropped editor image keeps its existing one-pixel transparent border.
    left = (left * scale).floor() / scale;
    top = (top * scale).floor() / scale;
    if crop_to_ink {
        left -= 1.0 / scale;
        top -= 1.0 / scale;
        right = ((right * scale).ceil() + 1.0) / scale;
        bottom = ((bottom * scale).ceil() + 1.0) / scale;
    }
    let (width, height) = (right - left, bottom - top);
    let (pixel_width, pixel_height) = (
        (width * scale).ceil() as u32,
        (height * scale).ceil() as u32,
    );
    check_size(pixel_width, pixel_height)?;
    let (origin_x, origin_y) = ((left * scale).round() as i64, (top * scale).round() as i64);
    let pixels = if origin_x == 0
        && origin_y == 0
        && pixel_width == source_width
        && pixel_height == source_height
    {
        // Ordinary vector formulas already have the final canvas. Reuse their
        // buffer instead of allocating and copying a second image.
        source
    } else {
        let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(pixel_width, pixel_height);
        for (row, target) in pixels
            .make_mut_bytes()
            .chunks_exact_mut(pixel_width as usize * 4)
            .enumerate()
        {
            let y = origin_y + row as i64;
            if y < 0 || y >= i64::from(source_height) {
                continue;
            }
            let first = (-origin_x).max(0).min(i64::from(pixel_width));
            let last = (i64::from(source_width) - origin_x)
                .max(0)
                .min(i64::from(pixel_width));
            if first >= last {
                continue;
            }
            let start = (y as usize * source_width as usize + (origin_x + first) as usize) * 4;
            let len = (last - first) as usize * 4;
            target[first as usize * 4..last as usize * 4]
                .copy_from_slice(&source.as_bytes()[start..start + len]);
        }
        pixels
    };
    for text in &mut texts {
        text.x -= left;
        text.y -= top;
    }
    for region in &mut regions {
        region.x = (region.x - left).clamp(0.0, width);
        region.y = (region.y - top).clamp(0.0, height);
        region.width = region.width.min(width - region.x);
        region.height = region.height.min(height - region.y);
    }
    if regions.is_empty() {
        regions.push(HitRegion {
            path: vec![],
            x: 0.0,
            y: 0.0,
            width,
            height,
        });
    }
    Ok(RenderedFormula {
        image: Image::from_rgba8_premultiplied(pixels),
        width,
        height,
        texts,
        regions,
    })
}
