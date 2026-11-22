//! A click-through outline on the desktop outside the opaque practice HWND.
use super::{card_region::CORNER_RADIUS, windows::handle};
use crate::PracticeWindow;
use resvg::tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};
use slint::{Color, ComponentHandle, winit_030::WinitWindowAccessor};
use std::{
    cell::RefCell,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT, SIZE},
    Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC,
        SelectObject,
    },
    UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GW_OWNER, GetWindow, GetWindowRect, IsIconic,
        IsWindowVisible, SW_SHOWNOACTIVATE, ShowWindow, ULW_ALPHA, UpdateLayeredWindow,
        WS_DISABLED, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
        WS_POPUP,
    },
};

const SPREAD: f32 = 7.0;

pub(crate) fn bind(card: &PracticeWindow) {
    let weak = card.as_weak();
    let ripple = RefCell::new(None);
    card.on_card_ripple_frame(move |outset_fraction, ink| {
        if ink.alpha() == 0 {
            ripple.take();
        } else if let Some(card) = weak.upgrade() {
            card.window().with_winit_window(|native| {
                if let Some(owner) = handle(native) {
                    frame(
                        &mut ripple.borrow_mut(),
                        owner,
                        native.scale_factor() as f32,
                        outset_fraction,
                        ink,
                    );
                }
            });
        }
    });
}

fn frame(slot: &mut Option<Surface>, owner: HWND, dpi: f32, outset_fraction: f32, ink: Color) {
    let mut bounds = RECT::default();
    // SAFETY: the binding borrows the live owner on its UI thread for this call.
    if unsafe {
        IsWindowVisible(owner) == 0
            || IsIconic(owner) != 0
            || GetWindowRect(owner, &mut bounds) == 0
    } {
        *slot = None;
        return;
    }
    let pad = ((SPREAD + 1.0) * dpi).ceil() as i32;
    let size = SIZE {
        cx: bounds.right - bounds.left + 2 * pad,
        cy: bounds.bottom - bounds.top + 2 * pad,
    };
    if slot
        .as_ref()
        .is_none_or(|s| s.size.cx != size.cx || s.size.cy != size.cy || s.owner != owner)
    {
        *slot = Surface::new(owner, size);
    }
    if let Some(surface) = slot {
        let origin = POINT {
            x: bounds.left - pad,
            y: bounds.top - pad,
        };
        let spread = SPREAD * outset_fraction * dpi;
        if !surface.present(origin, pad as f32 - spread, dpi, spread, ink) {
            *slot = None;
        }
    }
}

// One top-down, premultiplied BGRA bitmap is reused throughout the gesture.
// The owned tool window has no input or activation role and disappears with the card.
struct Surface {
    owner: HWND,
    window: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u8,
    size: SIZE,
}

impl Surface {
    fn new(owner: HWND, size: SIZE) -> Option<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.cx,
                biHeight: -size.cy,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut s = Self {
            owner,
            window: null_mut(),
            dc: null_mut(),
            bitmap: null_mut(),
            bits: null_mut(),
            size,
        };
        // SAFETY: all handles are local to this thread. Drop releases partial allocations too.
        unsafe {
            s.dc = CreateCompatibleDC(null_mut());
            if s.dc.is_null() {
                return None;
            }
            let mut bits = null_mut();
            s.bitmap = CreateDIBSection(s.dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if s.bitmap.is_null() || bits.is_null() || SelectObject(s.dc, s.bitmap).is_null() {
                return None;
            }
            s.bits = bits.cast();
            s.window = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                windows_sys::w!("STATIC"),
                null(),
                WS_POPUP | WS_DISABLED,
                0,
                0,
                0,
                0,
                owner,
                null_mut(),
                null_mut(),
                null(),
            );
        }
        (!s.window.is_null()).then_some(s)
    }

    fn present(&mut self, origin: POINT, inset: f32, dpi: f32, spread: f32, ink: Color) -> bool {
        let (w, h) = (self.size.cx as u32, self.size.cy as u32);
        // SAFETY: the selected 32-bit DIB owns exactly w*h four-byte pixels, with no row padding.
        let bytes =
            unsafe { std::slice::from_raw_parts_mut(self.bits, w as usize * h as usize * 4) };
        bytes.fill(0);
        let mut pixels = PixmapMut::from_bytes(bytes, w, h).expect("valid 32-bit DIB dimensions");
        let radius = CORNER_RADIUS * dpi + spread;
        let (right, bottom) = (w as f32 - inset, h as f32 - inset);
        let mut path = PathBuilder::new();
        // Cubic control offset from each corner of a circular quarter arc.
        let corner = radius * (1.0 - 0.552_284_8);
        path.move_to(inset + radius, inset);
        path.line_to(right - radius, inset);
        path.cubic_to(
            right - corner,
            inset,
            right,
            inset + corner,
            right,
            inset + radius,
        );
        path.line_to(right, bottom - radius);
        path.cubic_to(
            right,
            bottom - corner,
            right - corner,
            bottom,
            right - radius,
            bottom,
        );
        path.line_to(inset + radius, bottom);
        path.cubic_to(
            inset + corner,
            bottom,
            inset,
            bottom - corner,
            inset,
            bottom - radius,
        );
        path.line_to(inset, inset + radius);
        path.cubic_to(
            inset,
            inset + corner,
            inset + corner,
            inset,
            inset + radius,
            inset,
        );
        path.close();
        let mut paint = Paint::default();
        paint.set_color_rgba8(ink.blue(), ink.green(), ink.red(), ink.alpha());
        let stroke = Stroke {
            width: dpi,
            ..Default::default()
        };
        pixels.stroke_path(
            &path.finish().unwrap(),
            &paint,
            &stroke,
            Transform::identity(),
            None,
        );
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: UpdateLayeredWindow copies this frame synchronously. Show only after it succeeds.
        unsafe {
            if UpdateLayeredWindow(
                self.window,
                null_mut(),
                &origin,
                &self.size,
                self.dc,
                &POINT::default(),
                0,
                &blend,
                ULW_ALPHA,
            ) == 0
            {
                return false;
            }
            if IsWindowVisible(self.window) == 0 {
                ShowWindow(self.window, SW_SHOWNOACTIVATE);
            }
        }
        true
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // The owner may already have destroyed its owned windows. Releasing the
        // memory DC first deselects the bitmap before its storage is freed.
        unsafe {
            if !self.window.is_null() && GetWindow(self.window, GW_OWNER) == self.owner {
                DestroyWindow(self.window);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
        }
    }
}
