use super::*;
use slint::Model;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::event_loop::EventLoop;
use slint::winit_030::winit::platform::windows::EventLoopBuilderExtWindows;
use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use r#try::math_editor::TextSelection;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, CreateRectRgn,
    CreateSolidBrush, DeleteDC, DeleteObject, FillRect, GetDC, GetPixel, GetWindowRgn,
    InvalidateRect, PtInRegion, ReleaseDC, SRCCOPY, SelectObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, ReleaseCapture,
    SendInput, SetCapture, VK_LBUTTON,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CURSOR_SHOWING, CURSORINFO, FindWindowExW, GWL_EXSTYLE, GWL_STYLE, GetClientRect,
    GetCursorInfo, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, HWND_MESSAGE, HWND_TOPMOST, PostMessageW, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SendMessageW, SetCursorPos, SetWindowPos, WM_APP, WM_ENTERSIZEMOVE,
    WM_EXITSIZEMOVE, WM_LBUTTONUP, WM_NCACTIVATE, WM_NCPAINT, WS_EX_TOPMOST, WindowFromPoint,
};

slint::slint! {
    export component FocusProbe inherits Window {
        title: "Try native input check";
        preferred-width: 360px;
        preferred-height: 160px;
        background: #202020;
        in-out property <string> input-text: "Keep typing here";
        public function focus-input() { input.focus(); }
        input := TextInput { text <=> root.input-text; color: #e5e5e5; x:24px; y:40px; width:310px; height:40px; }
    }

    export component FramelessProbe inherits Window {
        title: "Try frameless check";
        no-frame: true;
        width: 320px;
        height: 230px;
        background: #202020;
        WindowMoveArea { width: 100%; height: 100%; }
        Text { text: "Try"; color: #e5e5e5; x:24px; y:60px; }
    }
}

fn open_regression_profile(path: &std::path::Path) -> Store {
    let store = Store::open(path.to_owned()).unwrap();
    let mut library = r#try::test_fixtures::regression_library();
    library.settings.appearance = Appearance::Dark;
    // Unrelated desktop checks must never start a real background network request.
    library.settings.auto_check_updates = false;
    store.save(&library).unwrap();
    store
}

fn focus_probe_with_pointer(probe: &FocusProbe, ready: impl FnOnce() + 'static) {
    // The owning check restores the pointer when its entire scenario ends.
    // Restoring it during activation can change the foreground precondition.
    probe.show().unwrap();
    probe.window().with_winit_window(|native| {
        native.set_window_level(slint::winit_030::winit::window::WindowLevel::AlwaysOnTop);
    });
    let probe = probe.clone_strong();
    // Allow the newly shown native window to finish its initial transition.
    Timer::single_shot(Duration::from_millis(350), move || {
        let mut point = POINT { x: 24, y: 24 };
        // A background test process has no unconditional foreground right.
        // Click only our own verified client area to establish the precondition.
        unsafe {
            let hwnd = native_handle(probe.window());
            assert_ne!(ClientToScreen(hwnd, &mut point), 0);
            assert_eq!(WindowFromPoint(point), hwnd);
            assert_ne!(SetCursorPos(point.x, point.y), 0);
        }
        mouse_button(MOUSEEVENTF_LEFTDOWN);
        mouse_button(MOUSEEVENTF_LEFTUP);
        Timer::single_shot(Duration::from_millis(80), move || {
            unsafe {
                assert_eq!(GetForegroundWindow(), native_handle(probe.window()));
            }
            ready();
        });
    });
}

struct CursorGuard(POINT);

impl CursorGuard {
    fn capture() -> Self {
        let mut point = POINT::default();
        unsafe {
            assert_ne!(GetCursorPos(&mut point), 0);
        }
        Self(point)
    }
}

impl Drop for CursorGuard {
    fn drop(&mut self) {
        unsafe {
            SetCursorPos(self.0.x, self.0.y);
        }
    }
}

fn type_into_foreground(expected: HWND, text: &str) {
    assert_eq!(
        unsafe { GetForegroundWindow() },
        expected,
        "Only type into this test's focused window"
    );
    let inputs: Vec<_> = text
        .encode_utf16()
        .flat_map(|unit| {
            [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP].map(|flags| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: 0,
                        wScan: unit,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            })
        })
        .collect();
    // SAFETY: the focused target belongs to this test; inputs is a live INPUT array.
    assert_eq!(
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        },
        inputs.len() as u32
    );
}

fn arrange_input_windows(probe: &FocusProbe, card: &PracticeWindow) {
    // Another desktop topmost window can otherwise sit between the card and
    // the probe. Arrange only our two test windows, preserving input ownership.
    probe.window().with_winit_window(|native| {
        native.set_window_level(slint::winit_030::winit::window::WindowLevel::AlwaysOnTop);
    });
    let foreground = unsafe { GetForegroundWindow() };
    unsafe {
        assert_ne!(
            SetWindowPos(
                native_handle(card.window()),
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE
            ),
            0
        );
        assert_eq!(GetForegroundWindow(), foreground);
    }
}

fn native_tray_start() {
    let class = "SlintSystemTrayWindow\0".encode_utf16().collect::<Vec<_>>();
    let mut after = std::ptr::null_mut();
    // SAFETY: locate only this process's message-only tray window, then dispatch
    // the same notification message used by the Windows shell for a left click.
    unsafe {
        loop {
            after = FindWindowExW(HWND_MESSAGE, after, class.as_ptr(), std::ptr::null());
            assert!(
                !after.is_null(),
                "This test has a native tray callback window"
            );
            let mut process_id = 0;
            GetWindowThreadProcessId(after, &mut process_id);
            if process_id == std::process::id() {
                assert_ne!(PostMessageW(after, WM_APP + 1, 1, WM_LBUTTONUP as isize), 0);
                break;
            }
        }
    }
}

pub(super) fn snapshot(window: &slint::Window, path: PathBuf) {
    let buffer = window.take_snapshot().expect("native software snapshot");
    image::save_buffer_with_format(
        path,
        buffer.as_bytes(),
        buffer.width(),
        buffer.height(),
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .unwrap();
}

fn key(window: &slint::Window, text: slint::SharedString) {
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

fn click_option(window: &PracticeWindow, index: usize) {
    let size = window
        .window()
        .size()
        .to_logical(window.window().scale_factor());
    let count = window.get_options().row_count() as f32;
    let width = (size.width - 48.0 - 8.0 * (count - 1.0)) / count;
    let row_height = (window.get_option_formula_height() + 16.0).max(48.0);
    let footer_height = (window.get_answer_formula_height() + 13.0).max(64.0);
    let position = slint::LogicalPosition::new(
        24.0 + width / 2.0 + index as f32 * (width + 8.0),
        size.height - 18.0 - footer_height - row_height / 2.0,
    );
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
}

struct AnswerChecks {
    app: Rc<App>,
    output: PathBuf,
    finished: Cell<bool>,
    focus_probe: RefCell<Option<FocusProbe>>,
}

pub(super) fn main_window(app: &App) -> MainWindow {
    app.main_window.borrow().as_ref().unwrap().clone_strong()
}

pub(super) fn practice_window(app: &App) -> PracticeWindow {
    app.practice
        .borrow()
        .as_ref()
        .unwrap()
        .window
        .clone_strong()
}

fn pointer_click(window: &slint::Window, position: slint::LogicalPosition) {
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

fn modified_key(window: &slint::Window, modifier: Key, text: slint::SharedString) {
    window.dispatch_event(WindowEvent::KeyPressed {
        text: modifier.into(),
    });
    key(window, text);
    window.dispatch_event(WindowEvent::KeyReleased {
        text: modifier.into(),
    });
}

fn type_content(app: &App, main: &MainWindow, field: i32, input: &str) {
    main.invoke_activate_content(field);
    main.set_math_input(input.into());
    main.invoke_edit_math_input(input.into());
    assert!(app.flush_math(), "{}", main.get_math_error());
    app.refresh_math(false);
}

fn scroll_settings_to_bottom(main: &MainWindow) {
    main.window().dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(
            main.get_settings_left() + 100.0,
            main.get_settings_top() + 100.0,
        ),
        delta_x: 0.0,
        delta_y: -1200.0,
    });
}

mod appearance;
mod focus;
mod formulas;
#[cfg(feature = "distribution")]
mod licenses;
mod lifecycle;
mod packs;
mod practice;
mod settings;
mod updates;
mod windows;

use windows::{
    check_floating_window, check_no_caption_paint, header_colors, mouse_button, native_handle,
};
