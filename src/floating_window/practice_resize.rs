//! Observe frameless resize gestures and enforce the native card lock on Windows.
use super::{card_region, windows::handle};
use slint::winit_030::winit::dpi::PhysicalSize;
use slint::winit_030::winit::window::Window;
use std::cell::Cell;
use r#try::window_position::PracticeCardSize;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetWindowRect, SC_MOVE, SC_SIZE, WM_EXITSIZEMOVE, WM_NCDESTROY, WM_SIZE,
    WM_SIZING, WM_SYSCOMMAND,
};

pub enum ResizeEvent {
    Started,
    Finished(Option<PracticeCardSize>),
}

struct Observer {
    is_locked: Box<dyn Fn() -> bool>,
    initial: Cell<Option<PracticeCardSize>>,
    clipped_frame: Cell<Option<(PhysicalSize<u32>, u32)>>,
    notify: Box<dyn Fn(ResizeEvent)>,
}

const SUBCLASS_ID: usize = 1;

pub fn install(
    window: &Window,
    is_locked: impl Fn() -> bool + 'static,
    notify: impl Fn(ResizeEvent) + 'static,
) -> bool {
    handle(window).is_some_and(|hwnd| install_on(hwnd, is_locked, notify))
}

fn install_on(
    hwnd: HWND,
    is_locked: impl Fn() -> bool + 'static,
    notify: impl Fn(ResizeEvent) + 'static,
) -> bool {
    let observer = Box::into_raw(Box::new(Observer {
        is_locked: Box::new(is_locked),
        initial: Cell::new(None),
        clipped_frame: Cell::new(None),
        notify: Box::new(notify),
    }));
    // SAFETY: the subclass runs on this UI thread and frees the state on destruction.
    unsafe {
        if SetWindowSubclass(hwnd, Some(messages), SUBCLASS_ID, observer as usize) == 0 {
            drop(Box::from_raw(observer));
            log::warn!(target: "try::floating_window",
                "event=practice_resize_subclass_failed reason={}", std::io::Error::last_os_error());
            return false;
        }
    }
    true
}

fn client_size(hwnd: HWND) -> Option<PracticeCardSize> {
    let mut rect = RECT::default();
    // SAFETY: the HWND belongs to the current subclass callback, and rect is writable.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 || unsafe { GetClientRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    let logical = |physical: i32| (f64::from(physical) * 96.0 / f64::from(dpi)).round() as u32;
    Some(PracticeCardSize {
        width: logical(rect.right - rect.left),
        height: logical(rect.bottom - rect.top),
    })
}

fn clip_resized_frame(hwnd: HWND, observer: &Observer) {
    let mut rect = RECT::default();
    // SAFETY: the native clip uses outer window coordinates from this HWND.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 || unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return;
    }
    let size = PhysicalSize::new(
        (rect.right - rect.left) as u32,
        (rect.bottom - rect.top) as u32,
    );
    // SetWindowRgn can emit nested position notifications for the same frame.
    // Record the geometry before applying the clip.
    if observer.clipped_frame.replace(Some((size, dpi))) != Some((size, dpi)) {
        card_region::apply(hwnd, size, f64::from(dpi) / 96.0, 1.0);
    }
}

unsafe extern "system" fn messages(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: install transfers the boxed observer to the subclass, which
    // owns it until WM_NCDESTROY. Notifications only update UI state or queue work.
    unsafe {
        let observer = data as *mut Observer;
        match message {
            WM_SYSCOMMAND
                if matches!(wparam as u32 & 0xfff0, SC_MOVE | SC_SIZE)
                    && ((*observer).is_locked)() =>
            {
                // The low bits describe the origin of the system command.
                return 0;
            }
            WM_SIZING if (*observer).initial.get().is_none() && !((*observer).is_locked)() => {
                if let Some(size) = client_size(hwnd) {
                    (*observer).initial.set(Some(size));
                    ((*observer).notify)(ResizeEvent::Started);
                }
            }
            WM_SIZE if lparam != 0 && (*observer).initial.get().is_some() => {
                // Windows can paint the resized surface before the next Slint
                // timer. Match its rounded clip within the same size message.
                clip_resized_frame(hwnd, &*observer);
            }
            WM_EXITSIZEMOVE => {
                if let Some(initial) = (*observer).initial.take() {
                    (*observer).clipped_frame.set(None);
                    let size = client_size(hwnd).filter(|size| *size != initial && size.is_valid());
                    ((*observer).notify)(ResizeEvent::Finished(size));
                }
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(messages), id);
                drop(Box::from_raw(observer));
            }
            _ => {}
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::GetWindowRgnBox;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SendMessageW,
        SetWindowPos, WMSZ_BOTTOMRIGHT, WS_POPUP,
    };

    #[test]
    fn rounded_clip_follows_each_size_message_before_the_drag_ends() {
        struct HiddenWindow(HWND);
        impl Drop for HiddenWindow {
            fn drop(&mut self) {
                // SAFETY: the test owns this HWND on its creating thread.
                unsafe { DestroyWindow(self.0) };
            }
        }

        // SAFETY: STATIC is a system class. The window is hidden and owned by this thread.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                windows_sys::w!("STATIC"),
                std::ptr::null(),
                WS_POPUP,
                0,
                0,
                376,
                220,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());
        let _window = HiddenWindow(hwnd);
        assert!(install_on(hwnd, || false, |_| {}));
        card_region::apply(hwnd, PhysicalSize::new(376, 220), 1.0, 1.0);

        for (width, height) in [(584, 220), (700, 330), (300, 200)] {
            let mut bounds = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            let mut clip = RECT::default();
            // SAFETY: only the hidden test window receives these messages. Bounds
            // is live for the synchronous WM_SIZING dispatch.
            unsafe {
                SendMessageW(
                    hwnd,
                    WM_SIZING,
                    WMSZ_BOTTOMRIGHT as usize,
                    (&mut bounds as *mut RECT) as isize,
                );
                assert_ne!(
                    SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        0,
                        0,
                        width,
                        height,
                        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
                    ),
                    0
                );
                assert!(GetWindowRgnBox(hwnd, &mut clip) > 1);
            }
            // No event loop or timer has run since Windows resized the window.
            assert!((clip.right - clip.left - width).abs() <= 1);
            assert!((clip.bottom - clip.top - height).abs() <= 1);
        }
    }
}
