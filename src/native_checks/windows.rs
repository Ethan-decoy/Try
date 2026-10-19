//! Explicit native checks for windows.
use super::*;

enum RepaintTarget {
    Main(MainWindow),
    Minimal(FocusProbe),
}

impl RepaintTarget {
    fn window(&self) -> &slint::Window {
        match self {
            Self::Main(window) => window.window(),
            Self::Minimal(window) => window.window(),
        }
    }
}

struct RepaintChecks {
    target: RepaintTarget,
    app: Option<Rc<App>>,
    output: PathBuf,
    expected: Cell<u32>,
    cycles: Cell<u8>,
    completed: Cell<bool>,
}

fn lose_native_pixels(window: &slint::Window) {
    let hwnd = native_handle(window);
    // SAFETY: damage only this live test window; release both GDI resources.
    unsafe {
        let mut rect = RECT::default();
        assert_ne!(GetClientRect(hwnd, &mut rect), 0);
        let dc = GetDC(hwnd);
        let brush = CreateSolidBrush(0x00cc22dd);
        assert!(!dc.is_null() && !brush.is_null());
        assert_ne!(FillRect(dc, &rect, brush), 0);
        DeleteObject(brush);
        ReleaseDC(hwnd, dc);
    }
}

fn repaint_sample(window: &slint::Window) -> u32 {
    let hwnd = native_handle(window);
    // Sample a blank background point, on the composed desktop rather than a
    // renderer snapshot. A cached bitmap may be correct while presentation fails.
    unsafe {
        let mut rect = RECT::default();
        assert_ne!(GetClientRect(hwnd, &mut rect), 0);
        let mut point = POINT {
            x: 30,
            y: rect.bottom / 2,
        };
        assert_ne!(ClientToScreen(hwnd, &mut point), 0);
        assert_eq!(
            WindowFromPoint(point),
            hwnd,
            "Repaint sample is unobstructed"
        );
        let dc = GetDC(std::ptr::null_mut());
        assert!(!dc.is_null());
        let color = GetPixel(dc, point.x, point.y);
        ReleaseDC(std::ptr::null_mut(), dc);
        color
    }
}

fn repaint_phase(checks: Rc<RepaintChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let window = checks.target.window();
            let hwnd = native_handle(window);
            match phase {
                0 => {
                    snapshot(window, checks.output.join("renderer.png"));
                    // A snapshot refreshes the renderer's cache; allow another
                    // unchanged frame to settle before the screen-loss probe.
                    unsafe {
                        InvalidateRect(hwnd, std::ptr::null(), 0);
                    }
                }
                1 => {
                    if checks.cycles.get() == 0 {
                        checks.expected.set(repaint_sample(window));
                    }
                    println!("Native baseline background={:#08x}", checks.expected.get());
                    // Model an invalid native surface without touching Slint's
                    // valid retained bitmap or changing any UI property.
                    lose_native_pixels(window);
                    unsafe {
                        assert_ne!(InvalidateRect(hwnd, std::ptr::null(), 1), 0);
                    }
                }
                2 => {
                    let actual = repaint_sample(window);
                    println!(
                        "Invalidated native background={actual:#08x}; expected={:#08x}",
                        checks.expected.get()
                    );
                    assert_eq!(
                        actual,
                        checks.expected.get(),
                        "System repaint must restore unchanged background pixels"
                    );
                    checks.cycles.set(checks.cycles.get() + 1);
                    if checks.cycles.get() < 5 {
                        repaint_phase(checks.clone(), 1);
                        return;
                    }
                }
                3 => {
                    lose_native_pixels(window);
                    assert_ne!(
                        repaint_sample(window),
                        checks.expected.get(),
                        "Surface loss is observable before returning"
                    );
                    // Replay the exact focus hook without forcing desktop focus
                    // or mouse input from a background unit-test process.
                    crate::window_repaint::prepare(
                        window,
                        &slint::winit_030::winit::event::WindowEvent::Focused(true),
                    );
                }
                4 => {
                    assert_eq!(
                        repaint_sample(window),
                        checks.expected.get(),
                        "Focus-return recovery must restore the background without an explicit invalidation"
                    );
                    println!("Five native damage cycles and focus-return event replay passed.");
                    checks.completed.set(true);
                    window.hide().unwrap();
                    if let Some(app) = &checks.app {
                        app.quit();
                    } else {
                        slint::quit_event_loop().unwrap();
                    }
                    return;
                }
                _ => unreachable!(),
            }
            repaint_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            let _ = checks.target.window().hide();
            if let Some(app) = &checks.app {
                app.quit();
            } else {
                slint::quit_event_loop().unwrap();
            }
        }
    });
}

fn run_native_repaint(minimal: bool) {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/main_repaint_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let hook = floating.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    let selector = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder);
    let selector = if minimal {
        selector
    } else {
        selector.with_winit_window_attributes_hook(move |attributes| hook.attributes(attributes))
    };
    selector.select().unwrap();
    let (target, app) = if minimal {
        let window = FocusProbe::new().unwrap();
        window.window().on_winit_window_event(|window, event| {
            crate::window_repaint::prepare(window, event);
            slint::winit_030::EventResult::Propagate
        });
        window.show().unwrap();
        FloatingWindow::activate(window.window());
        (RepaintTarget::Minimal(window), None)
    } else {
        let app = App::new(store, library, floating).unwrap();
        app.start(false).unwrap();
        (RepaintTarget::Main(main_window(&app)), Some(app))
    };
    target.window().with_winit_window(|native| {
        native.set_window_level(slint::winit_030::winit::window::WindowLevel::AlwaysOnTop);
    });
    let checks = Rc::new(RepaintChecks {
        target,
        app,
        output,
        expected: Cell::new(0),
        cycles: Cell::new(0),
        completed: Cell::new(false),
    });
    repaint_phase(checks.clone(), 0);
    Timer::single_shot(Duration::from_secs(15), || {
        let _ = slint::quit_event_loop();
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.completed.get(), "Native background recovery failed");
}

#[test]
#[ignore = "opens an isolated native main window and invalidates its screen pixels"]
fn native_main_repaint() {
    run_native_repaint(false);
}

#[test]
#[ignore = "checks the repaint bridge with a minimal window without floating-window hooks"]
fn native_repaint_baseline() {
    run_native_repaint(true);
}

pub(super) fn native_handle(window: &slint::Window) -> HWND {
    window
        .with_winit_window(|native| match native.window_handle().unwrap().as_raw() {
            RawWindowHandle::Win32(handle) => handle.hwnd.get() as HWND,
            _ => panic!("Expected a Windows native window"),
        })
        .unwrap()
}

pub(super) fn check_floating_window(window: &slint::Window) {
    let hwnd = native_handle(window);
    window.with_winit_window(|native| {
        assert!(!native.is_decorated(), "No system title bar");
        assert!(!native.is_resizable(), "No resizing border");
        assert_eq!(
            native.inner_size(),
            native.outer_size(),
            "No non-client frame occupies space"
        );
    });
    // SAFETY: all calls borrow the HWND of the live native test window.
    unsafe {
        let extended = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        assert_ne!(
            extended & WS_EX_TOPMOST,
            0,
            "Practice stays above ordinary windows"
        );
        let region = CreateRectRgn(0, 0, 0, 0);
        assert!(
            GetWindowRgn(hwnd, region) > 0,
            "Native round window region exists"
        );
        assert_eq!(
            PtInRegion(region, 0, 0),
            0,
            "Corner is clipped from native hit testing"
        );
        assert_ne!(PtInRegion(region, 50, 50), 0, "Card center is interactive");
        DeleteObject(region);
    }
    let saved = FloatingWindow::remember(window).unwrap();
    assert!(
        saved.x >= 23.0 && saved.y >= 23.0,
        "Card stays inside the work area"
    );
}

pub(super) fn header_colors(window: &slint::Window) -> (u32, u32) {
    let buffer = window.take_snapshot().unwrap();
    let expected_pixel = |x: usize, y: usize| {
        let offset = (y * buffer.width() as usize + x) * 4;
        let pixel = &buffer.as_bytes()[offset..offset + 3];
        u32::from(pixel[0]) | (u32::from(pixel[1]) << 8) | (u32::from(pixel[2]) << 16)
    };
    let hwnd = native_handle(window);
    let scale = window.scale_factor();
    // Capture the composed desktop once, then inspect its full header in memory.
    // Compare each point with the rendered header, including the pack tag.
    // A client DC omits the system caption; reading every pixel from the screen
    // separately synchronizes with desktop composition and slows dense checks.
    // SAFETY: hwnd is live; the bitmap, both DCs and selection are released below.
    unsafe {
        let mut rect = RECT::default();
        assert_ne!(GetWindowRect(hwnd, &mut rect), 0);
        let dc = GetDC(std::ptr::null_mut());
        assert!(!dc.is_null(), "Screen DC is available");
        let memory_dc = CreateCompatibleDC(dc);
        if memory_dc.is_null() {
            ReleaseDC(std::ptr::null_mut(), dc);
            panic!("Header capture DC is available");
        }
        let width = rect.right - rect.left;
        let height = (17.0 * scale).ceil() as i32;
        let bitmap = CreateCompatibleBitmap(dc, width, height);
        if bitmap.is_null() {
            DeleteDC(memory_dc);
            ReleaseDC(std::ptr::null_mut(), dc);
            panic!("Header capture bitmap is available");
        }
        let previous = SelectObject(memory_dc, bitmap);
        let copied = BitBlt(
            memory_dc, 0, 0, width, height, dc, rect.left, rect.top, SRCCOPY,
        );
        let margin = (16.0 * scale).round() as i32;
        let mut expected =
            expected_pixel(buffer.width() as usize / 2, (8.0 * scale).round() as usize);
        let mut color = expected;
        'header: for logical_y in [8.0, 12.0, 16.0] {
            let y = (logical_y * scale).round() as i32;
            for x in (margin..(width - margin)).step_by(4) {
                expected = expected_pixel(x as usize, y as usize);
                color = GetPixel(memory_dc, x, y);
                if color != expected {
                    break 'header;
                }
            }
        }
        SelectObject(memory_dc, previous);
        DeleteObject(bitmap);
        DeleteDC(memory_dc);
        ReleaseDC(std::ptr::null_mut(), dc);
        assert_ne!(copied, 0, "Actual desktop header capture succeeds");
        (color, expected)
    }
}

fn top_pixel(window: &slint::Window) -> u32 {
    header_colors(window).0
}

pub(super) fn check_no_caption_paint(window: &slint::Window) {
    let (actual, expected) = header_colors(window);
    assert_eq!(
        actual, expected,
        "Windows must not paint a title bar over the practice card"
    );
}

// These focused native checks run individually because Winit owns one event
// loop per process. The baseline deliberately avoids our window controller.
fn run_frameless_probe(with_controller: bool) {
    let floating = Rc::new(FloatingWindow::new(&r#try::knowledge::Settings::default()));
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    let selector = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder);
    let selector = if with_controller {
        let hook = floating.clone();
        selector.with_winit_window_attributes_hook(move |attributes| hook.attributes(attributes))
    } else {
        selector
    };
    selector.select().unwrap();
    let _creation = with_controller.then(|| floating.practice_creation());
    let probe = FramelessProbe::new().unwrap();
    probe.show().unwrap();
    if with_controller {
        floating.place(probe.window());
        FloatingWindow::activate(probe.window());
    }
    drop(_creation);
    let passed = Rc::new(Cell::new(false));
    let result = passed.clone();
    Timer::single_shot(Duration::from_millis(500), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if with_controller {
                floating.place(probe.window());
            }
            let hwnd = native_handle(probe.window());
            assert!(
                !probe
                    .window()
                    .with_winit_window(|native| native.is_decorated())
                    .unwrap()
            );
            println!(
                "Controller={with_controller}; native style={:#x}; initial top={:#x}",
                unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) },
                top_pixel(probe.window())
            );
            if with_controller {
                probe.window().with_winit_window(|native| {
                    native.set_title("Try title refresh");
                    native.set_window_icon(None);
                });
                check_no_caption_paint(probe.window());
            }
            // A lost-focus non-client repaint is the user's delayed failure.
            // Send synchronously and sample before a client redraw can mask it.
            for (message, active) in [(WM_NCACTIVATE, 0), (WM_NCACTIVATE, 1), (WM_NCPAINT, 1)] {
                unsafe {
                    SendMessageW(hwnd, message, active, 0);
                }
                let color = top_pixel(probe.window());
                println!("message={message:#x}; wparam={active}; top={color:#x}");
                check_no_caption_paint(probe.window());
            }
        }));
        result.set(outcome.is_ok());
        probe.hide().unwrap();
        slint::quit_event_loop().unwrap();
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(passed.get(), "native frameless repaint check failed");
}

#[test]
#[ignore = "opens a minimal native no-frame window for comparison"]
fn native_no_frame_baseline() {
    run_frameless_probe(false);
}

#[test]
#[ignore = "opens a native card and exercises non-client repaint messages"]
fn native_frameless_repaint() {
    run_frameless_probe(true);
}

#[derive(Default)]
struct DragMessages {
    entered: AtomicUsize,
    exited: AtomicUsize,
    theme_caption: AtomicUsize,
    theme_frame: AtomicUsize,
}

unsafe extern "system" fn record_drag_message(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: the test keeps this Arc allocation alive until the callback is removed.
    unsafe {
        let messages = &*(data as *const DragMessages);
        let counter = match message {
            WM_ENTERSIZEMOVE => Some(&messages.entered),
            WM_EXITSIZEMOVE => Some(&messages.exited),
            0x00ae => Some(&messages.theme_caption),
            0x00af => Some(&messages.theme_frame),
            _ => None,
        };
        if let Some(counter) = counter {
            counter.fetch_add(1, Ordering::SeqCst);
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

pub(super) fn mouse_button(flags: u32) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    // SAFETY: input is a correctly sized native INPUT, targeting the owned test card.
    assert_eq!(
        unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) },
        1
    );
}

fn drag_mouse(hwnd_value: usize) {
    let hwnd = hwnd_value as HWND;
    let mut before = POINT::default();
    let mut rect = RECT::default();
    unsafe {
        assert_ne!(GetCursorPos(&mut before), 0);
        assert_ne!(GetWindowRect(hwnd, &mut rect), 0);
        assert!(
            GetAsyncKeyState(VK_LBUTTON as i32) >= 0,
            "Mouse must start released"
        );
        assert_ne!(SetCursorPos(rect.left + 100, rect.top + 60), 0);
        assert_eq!(
            WindowFromPoint(POINT {
                x: rect.left + 100,
                y: rect.top + 60
            }),
            hwnd,
            "Input only targets this test's native card"
        );
    }
    mouse_button(MOUSEEVENTF_LEFTDOWN);
    // Always release and restore the pointer if a test assertion fails.
    struct RestoreMouse(POINT);
    impl Drop for RestoreMouse {
        fn drop(&mut self) {
            mouse_button(MOUSEEVENTF_LEFTUP);
            // Let the system finish its move loop before restoring the pointer.
            std::thread::sleep(Duration::from_millis(120));
            unsafe {
                SetCursorPos(self.0.x, self.0.y);
            }
        }
    }
    let _restore = RestoreMouse(before);
    std::thread::sleep(Duration::from_millis(150));
    for step in 1..=8 {
        unsafe {
            assert_ne!(
                SetCursorPos(rect.left + 100 - step * 12, rect.top + 60 - step * 8),
                0
            );
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

#[test]
#[ignore = "moves the pointer briefly to drag only this test's native window"]
fn native_drag_repaint() {
    let floating = Rc::new(FloatingWindow::new(&r#try::knowledge::Settings::default()));
    let hook = floating.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .with_winit_window_attributes_hook(move |attributes| hook.attributes(attributes))
        .select()
        .unwrap();
    let _creation = floating.practice_creation();
    let probe = FramelessProbe::new().unwrap();
    probe.show().unwrap();
    drop(_creation);
    let passed = Rc::new(Cell::new(false));
    let result = passed.clone();
    Timer::single_shot(Duration::from_millis(500), move || {
        floating.place(probe.window());
        FloatingWindow::activate(probe.window());
        probe.window().with_winit_window(|native| {
            let position = native.outer_position().unwrap();
            native.set_outer_position(slint::winit_030::winit::dpi::PhysicalPosition::new(
                position.x - 600,
                position.y - 350,
            ));
        });
        let hwnd_value = native_handle(probe.window()) as usize;
        let messages = Arc::new(DragMessages::default());
        unsafe {
            assert_ne!(
                SetWindowSubclass(
                    hwnd_value as HWND,
                    Some(record_drag_message),
                    1,
                    Arc::as_ptr(&messages) as usize
                ),
                0
            );
        }
        let original = FloatingWindow::position(probe.window()).unwrap();
        let driver_messages = messages.clone();
        let driver = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            for drag in 1..=10 {
                drag_mouse(hwnd_value);
                for _ in 0..50 {
                    if driver_messages.exited.load(Ordering::SeqCst) == drag {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                assert_eq!(
                    driver_messages.entered.load(Ordering::SeqCst),
                    drag,
                    "Mouse drag {drag} must enter the native move loop"
                );
                assert_eq!(
                    driver_messages.exited.load(Ordering::SeqCst),
                    drag,
                    "Mouse drag {drag} must finish before the next press"
                );
                std::thread::sleep(Duration::from_millis(150));
            }
        });
        Timer::single_shot(Duration::from_secs(15), move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                assert!(driver.is_finished(), "Native drag driver must finish");
                driver.join().unwrap();
                println!(
                    "Native drag: enter={}, exit={}, theme caption={}, theme frame={}",
                    messages.entered.load(Ordering::SeqCst),
                    messages.exited.load(Ordering::SeqCst),
                    messages.theme_caption.load(Ordering::SeqCst),
                    messages.theme_frame.load(Ordering::SeqCst)
                );
                assert_eq!(
                    messages.entered.load(Ordering::SeqCst),
                    10,
                    "Every pointer drag must enter the real system move loop"
                );
                assert_eq!(
                    messages.entered.load(Ordering::SeqCst),
                    messages.exited.load(Ordering::SeqCst)
                );
                assert_ne!(
                    FloatingWindow::position(probe.window()).unwrap(),
                    original,
                    "The pointer drag must move the card"
                );
                check_no_caption_paint(probe.window());
            }));
            result.set(outcome.is_ok());
            unsafe {
                RemoveWindowSubclass(hwnd_value as HWND, Some(record_drag_message), 1);
            }
            probe.hide().unwrap();
            slint::quit_event_loop().unwrap();
        });
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(passed.get(), "native drag repaint check failed");
}
