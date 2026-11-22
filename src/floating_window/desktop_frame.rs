//! Native window commands for the title bar drawn by Slint.
use super::windows::{handle, suppress_frame_paint};
use crate::{WindowControlBounds, WindowFrame};
use slint::{ComponentHandle, Global, winit_030::WinitWindowAccessor};
use std::cell::Cell;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClientRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTMAXBUTTON,
    HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, SendMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE,
    WM_NCMOUSEMOVE,
};

pub(crate) fn configure<T>(component: &T, control_bounds: fn(&T) -> WindowControlBounds)
where
    T: ComponentHandle + 'static,
    for<'a> WindowFrame<'a>: Global<'a, T>,
{
    let frame = component.global::<WindowFrame>();
    frame.set_enabled(true);
    let layout_owner = component.as_weak();
    let weak = component.as_weak();
    let state = Caption {
        bounds: Box::new(move || layout_owner.upgrade().map(|owner| control_bounds(&owner))),
        edge: frame.get_resize_width(),
        hovered: Cell::new(false),
        paint_hover: Box::new(move |hovered| {
            if let Some(component) = weak.upgrade() {
                component
                    .global::<WindowFrame>()
                    .set_maximize_hovered(hovered);
            }
        }),
    };
    let weak = component.as_weak();
    let _ = slint::spawn_local(async move {
        let Some(component) = weak.upgrade() else {
            return;
        };
        let Ok(native) = component.window().winit_window().await else {
            return;
        };
        let Some(hwnd) = handle(&native) else { return };
        suppress_frame_paint(hwnd);
        let data = Box::into_raw(Box::new(state));
        // SAFETY: the owning UI thread frees the state on WM_NCDESTROY.
        if unsafe { SetWindowSubclass(hwnd, Some(caption_proc), 1, data as usize) } == 0 {
            // SAFETY: Windows did not take the state.
            unsafe { drop(Box::from_raw(data)) };
            log::warn!("Unable to attach the native title-bar hit test");
        }
    });
}

struct Caption {
    bounds: Box<dyn Fn() -> Option<WindowControlBounds>>,
    edge: f32,
    hovered: Cell<bool>,
    paint_hover: Box<dyn Fn(bool)>,
}

impl Caption {
    fn hit(&self, x: f32, y: f32, width: f32, height: f32, maximized: bool) -> Option<u32> {
        if y < 0.0 || y >= height || x < 0.0 || x >= width {
            return None;
        }
        let bounds = (self.bounds)()?;
        if bounds.height == 0.0 {
            return Some(HTCLIENT);
        }
        if !maximized {
            let side = if x < self.edge {
                HTLEFT
            } else if x >= width - self.edge {
                HTRIGHT
            } else {
                HTCLIENT
            };
            if y < self.edge {
                return Some(match side {
                    HTLEFT => HTTOPLEFT,
                    HTRIGHT => HTTOPRIGHT,
                    _ => HTTOP,
                });
            }
            if y >= height - self.edge {
                return Some(match side {
                    HTLEFT => HTBOTTOMLEFT,
                    HTRIGHT => HTBOTTOMRIGHT,
                    _ => HTBOTTOM,
                });
            }
            if side != HTCLIENT {
                return Some(side);
            }
        }
        // Windows offers Snap Layouts on HTMAXBUTTON. Footer buttons use Slint's client input.
        if y >= bounds.height {
            return Some(HTCLIENT);
        }
        let from_left = x - bounds.x;
        if (bounds.y..bounds.y + bounds.height).contains(&y)
            && (0.0..3.0 * bounds.button_width).contains(&from_left)
        {
            return Some(
                if (bounds.button_width..2.0 * bounds.button_width).contains(&from_left) {
                    HTMAXBUTTON
                } else {
                    HTCLIENT
                },
            );
        }
        Some(HTCAPTION)
    }

    fn hover(&self, hwnd: HWND, active: bool) {
        if self.hovered.replace(active) == active {
            return;
        }
        (self.paint_hover)(active);
        if active {
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_NONCLIENT | TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            // SAFETY: the window and the structure belong to this UI thread.
            unsafe { TrackMouseEvent(&mut tracking) };
        }
    }
}

fn client_point(hwnd: HWND, screen: LPARAM) -> Option<POINT> {
    let mut point = POINT {
        x: screen as i16 as i32,
        y: (screen >> 16) as i16 as i32,
    };
    // SAFETY: point is writable and hwnd is the live recipient of this message.
    (unsafe { ScreenToClient(hwnd, &mut point) } != 0).then_some(point)
}

unsafe extern "system" fn caption_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: SetWindowSubclass supplies the boxed Caption until WM_NCDESTROY.
    let caption = unsafe { &*(data as *const Caption) };
    // SAFETY: all native calls below act synchronously on this message's HWND.
    unsafe {
        match msg {
            WM_NCHITTEST => {
                let mut rect = RECT::default();
                if let Some(p) = client_point(hwnd, lp)
                    && GetClientRect(hwnd, &mut rect) != 0
                {
                    let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
                    if let Some(hit) = caption.hit(
                        p.x as f32 / scale,
                        p.y as f32 / scale,
                        rect.right as f32 / scale,
                        rect.bottom as f32 / scale,
                        IsZoomed(hwnd) != 0,
                    ) {
                        return hit as LRESULT;
                    }
                }
            }
            WM_NCMOUSEMOVE => caption.hover(hwnd, wp == HTMAXBUTTON as usize),
            WM_NCMOUSELEAVE | WM_MOUSEMOVE => caption.hover(hwnd, false),
            WM_NCLBUTTONDOWN | WM_NCLBUTTONUP if wp == HTMAXBUTTON as usize => {
                // Windows handles Snap hover. Give the click to the same Slint
                // button used by the keyboard, including drag-out cancellation.
                if let Some(p) = client_point(hwnd, lp) {
                    let pos = (p.x as u16 as u32 | ((p.y as u16 as u32) << 16)) as LPARAM;
                    SendMessageW(hwnd, WM_MOUSEMOVE, 0, pos);
                    let (button, flags) = if msg == WM_NCLBUTTONDOWN {
                        (WM_LBUTTONDOWN, 1)
                    } else {
                        (WM_LBUTTONUP, 0)
                    };
                    return SendMessageW(hwnd, button, flags, pos);
                }
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(caption_proc), id);
                drop(Box::from_raw(data as *mut Caption));
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wp, lp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_title_bar_maximize_button_opts_into_snap_layouts() {
        for (top, expected_hit) in [(467.0, HTCLIENT), (0.0, HTMAXBUTTON)] {
            let controls = WindowControlBounds {
                x: 566.0,
                y: top,
                button_width: 46.0,
                height: 34.0,
            };
            let x = controls.x + 1.5 * controls.button_width;
            let y = controls.y + controls.height / 2.0;
            let caption = Caption {
                bounds: Box::new(move || Some(controls.clone())),
                edge: 5.0,
                hovered: Cell::new(false),
                paint_hover: Box::new(|_| {}),
            };
            for maximized in [false, true] {
                assert_eq!(
                    caption.hit(x, y, 720.0, 514.0, maximized),
                    Some(expected_hit)
                );
            }
        }
    }
}
