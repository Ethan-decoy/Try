use crate::{PracticeWindow, Theme};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

impl PracticeWindow {
    pub(super) fn animate_visibility(&self, closing: bool) -> bool {
        if !self.global::<Theme>().get_spatial_motion() {
            return false;
        }
        let current = self.get_zoom_image();
        let frame = if current.size().width > 0 {
            current
        } else if let Ok(pixels) = self.window().take_snapshot() {
            zoom_image(pixels, self.window().scale_factor())
        } else {
            return false;
        };
        self.invoke_start_zoom(frame, closing);
        true
    }
}

fn zoom_image(pixels: SharedPixelBuffer<Rgba8Pixel>, dpi: f32) -> Image {
    if dpi >= 2.0 {
        return Image::from_rgba8(pixels);
    }
    // The software renderer samples scaled images at the nearest texel. Cache
    // half-pixel samples once so one-pixel input borders survive the zoom.
    let (w, h) = (pixels.width(), pixels.height());
    let mut smooth = SharedPixelBuffer::new(w * 2, h * 2);
    let width = w as usize;
    let source = pixels.as_slice();
    for (y, pair) in smooth
        .make_mut_slice()
        .chunks_exact_mut(width * 4)
        .enumerate()
    {
        let top = &source[y * width..][..width];
        let next_y = (y + 1).min(h as usize - 1);
        let bottom = &source[next_y * width..][..width];
        let (upper, lower) = pair.split_at_mut(width * 2);
        for x in 0..width {
            let next_x = (x + 1).min(width - 1);
            let top_mid = midpoint(top[x], top[next_x]);
            let bottom_mid = midpoint(bottom[x], bottom[next_x]);
            upper[x * 2] = top[x];
            upper[x * 2 + 1] = top_mid;
            lower[x * 2] = midpoint(top[x], bottom[x]);
            lower[x * 2 + 1] = midpoint(top_mid, bottom_mid);
        }
    }
    Image::from_rgba8(smooth)
}

fn midpoint(left: Rgba8Pixel, right: Rgba8Pixel) -> Rgba8Pixel {
    Rgba8Pixel::new(
        left.r.midpoint(right.r),
        left.g.midpoint(right.g),
        left.b.midpoint(right.b),
        left.a.midpoint(right.a),
    )
}
