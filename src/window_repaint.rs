//! Restore native surface damage independently of Slint's retained UI changes.

use slint::winit_030::winit::event::WindowEvent;

#[cfg(target_os = "windows")]
pub fn prepare(window: &slint::Window, event: &WindowEvent) {
    use slint::private_unstable_api::re_exports::{LogicalRect, WindowInner};
    use slint::winit_030::WinitWindowAccessor;
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::GetUpdateRect;

    let mut region = LogicalRect::default();
    match event {
        WindowEvent::Focused(true) => {
            // Returning from another application may expose a lost surface even
            // when Windows no longer reports an update rectangle.
            let size = window.size().to_logical(window.scale_factor());
            region.size.width = size.width;
            region.size.height = size.height;
        }
        WindowEvent::RedrawRequested => {
            let invalid = window
                .with_winit_window(|native| {
                    let RawWindowHandle::Win32(handle) = native.window_handle().ok()?.as_raw()
                    else {
                        return None;
                    };
                    let mut rect = RECT::default();
                    // SAFETY: read the live window's client update region without
                    // validating it or changing Winit's normal WM_PAINT handling.
                    (unsafe { GetUpdateRect(handle.hwnd.get() as HWND, &mut rect, 0) } != 0)
                        .then_some(rect)
                })
                .flatten();
            let Some(rect) = invalid else {
                return;
            };
            let scale = window.scale_factor();
            region.origin.x = rect.left as f32 / scale;
            region.origin.y = rect.top as f32 / scale;
            region.size.width = (rect.right - rect.left) as f32 / scale;
            region.size.height = (rect.bottom - rect.top) as f32 / scale;
        }
        _ => return,
    }
    if region.size.width <= 0.0 || region.size.height <= 0.0 {
        return;
    }

    // Slint 1.18.1's software/Winit path presents only UI-dirty rectangles. A
    // system update must also mark the renderer dirty, otherwise a correct
    // retained bitmap is never copied back to the invalid native surface.
    // Keep this version-pinned internal bridge here; no public Window API marks
    // damage independently of changing UI properties.
    WindowInner::from_pub(window)
        .window_adapter()
        .renderer()
        .mark_dirty_region(region.into());
    if matches!(event, WindowEvent::Focused(true)) {
        log::debug!(target: "try::window_repaint", "event=background_recovery_requested reason=focus_return");
        window.request_redraw();
    }
}

#[cfg(not(target_os = "windows"))]
pub fn prepare(_window: &slint::Window, _event: &WindowEvent) {}
