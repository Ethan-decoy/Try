use slint::winit_030::winit::dpi::PhysicalSize;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};

pub(super) const CORNER_RADIUS: f32 = 16.0;

/// Clip the opaque surface and its hit area together. The HWND and its saved
/// desktop position stay fixed while the displayed snapshot scales.
pub(super) fn apply(hwnd: HWND, size: PhysicalSize<u32>, dpi: f64, scale: f32) {
    let inset_x = (size.width as f32 * (1.0 - scale) / 2.0).round() as i32;
    let inset_y = (size.height as f32 * (1.0 - scale) / 2.0).round() as i32;
    let diameter = (2.0 * f64::from(CORNER_RADIUS) * dpi * f64::from(scale)).round() as i32;
    // SAFETY: scalar bounds come from the live window, on its owning UI thread.
    let region = unsafe {
        CreateRoundRectRgn(
            inset_x,
            inset_y,
            size.width as i32 - inset_x + 1,
            size.height as i32 - inset_y + 1,
            diameter,
            diameter,
        )
    };
    if region.is_null() {
        log::warn!(target: "try::floating_window", "event=window_region_create_failed reason={}", std::io::Error::last_os_error());
        return;
    }
    // Windows takes ownership on success. Only a rejected region is ours to free.
    if unsafe { SetWindowRgn(hwnd, region, 1) } == 0 {
        log::warn!(target: "try::floating_window", "event=window_region_apply_failed reason={}", std::io::Error::last_os_error());
        unsafe {
            DeleteObject(region);
        }
    }
}
