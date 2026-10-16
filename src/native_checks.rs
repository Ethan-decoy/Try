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
    KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput, VK_LBUTTON,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GWL_EXSTYLE, GWL_STYLE, GetClientRect, GetCursorPos, GetForegroundWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, HWND_MESSAGE, PostMessageW,
    SendMessageW, SetCursorPos, WM_APP, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_LBUTTONUP,
    WM_NCACTIVATE, WM_NCPAINT, WS_EX_TOPMOST, WindowFromPoint,
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

struct PracticeFocusChecks {
    app: Rc<App>,
    probe: FocusProbe,
    output: PathBuf,
    finished: Cell<bool>,
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

fn practice_focus_phase(checks: Rc<PracticeFocusChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => app
                    .show_practice_with_origin(PracticeOrigin::Reminder)
                    .unwrap(),
                1 => {
                    let window = practice_window(app);
                    assert_eq!(
                        unsafe { GetForegroundWindow() },
                        native_handle(window.window()),
                        "A scheduled answer card focuses automatically without a mouse click"
                    );
                    type_into_foreground(native_handle(window.window()), "37");
                }
                2 => {
                    let window = practice_window(app);
                    assert!(
                        window.get_answer_text() == "37",
                        "Direct keyboard input reaches the answer field"
                    );
                    snapshot(window.window(), checks.output.join("direct_input.png"));
                    app.close_practice();
                    app.show_settings(false).unwrap();
                    let main = main_window(app);
                    assert!(main.get_focus_on_reminder());
                    main.window().dispatch_event(WindowEvent::PointerScrolled {
                        position: slint::LogicalPosition::new(
                            main.get_settings_left() + 100.0,
                            main.get_settings_top() + 100.0,
                        ),
                        delta_x: 0.0,
                        delta_y: -1200.0,
                    });
                }
                3 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("focus_setting_on.png"));
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_reminder_focus_left() + 30.0,
                            main.get_reminder_focus_top() + 18.0,
                        ),
                    );
                }
                4 => {
                    assert!(!main_window(app).get_focus_on_reminder());
                    assert!(!app.store.load().unwrap().settings.focus_on_reminder);
                    app.close_main();
                    app.show_settings(false).unwrap();
                    assert!(
                        !main_window(app).get_focus_on_reminder(),
                        "Reopening the settings restores the saved switch"
                    );
                    snapshot(
                        main_window(app).window(),
                        checks.output.join("focus_setting_off.png"),
                    );
                    app.close_main();
                    checks.probe.show().unwrap();
                    checks.probe.set_input_text("probe".into());
                    checks.probe.invoke_focus_input();
                    FloatingWindow::activate(checks.probe.window());
                    app.show_practice_with_origin(PracticeOrigin::Reminder)
                        .unwrap();
                }
                5 => type_into_foreground(native_handle(checks.probe.window()), "X"),
                6 => {
                    assert!(
                        checks.probe.get_input_text().contains('X'),
                        "Disabling automatic focus retains input in the other window"
                    );
                    assert!(practice_window(app).get_answer_text().is_empty());
                    app.show_practice().unwrap();
                }
                7 => type_into_foreground(native_handle(practice_window(app).window()), "9"),
                8 => {
                    assert!(
                        practice_window(app).get_answer_text() == "9",
                        "Manual opening still focuses an existing card"
                    );
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_set_focus_on_reminder(true);
                    assert!(app.store.load().unwrap().settings.focus_on_reminder);
                    main.invoke_set_all_answer_modes(1);
                    app.close_main();
                    FloatingWindow::activate(checks.probe.window());
                    app.show_practice_with_origin(PracticeOrigin::Reminder)
                        .unwrap();
                }
                9 => type_into_foreground(native_handle(practice_window(app).window()), "1"),
                10 => {
                    assert!(
                        app.practice.borrow().as_ref().unwrap().answered,
                        "Number keys answer the focused multiple-choice card"
                    );
                    app.close_practice();
                    app.show_main().unwrap();
                    main_window(app).invoke_set_all_answer_modes(2);
                    app.close_main();
                    FloatingWindow::activate(checks.probe.window());
                    app.show_practice_with_origin(PracticeOrigin::Reminder)
                        .unwrap();
                }
                11 => type_into_foreground(native_handle(practice_window(app).window()), "1"),
                12 => {
                    assert!(
                        app.practice.borrow().as_ref().unwrap().answered,
                        "Number keys answer the focused true/false card"
                    );
                    app.close_practice();
                    app.show_main().unwrap();
                    main_window(app).invoke_toggle_knowledge_mode();
                    assert!(app.knowledge_only.get());
                    app.close_main();
                    FloatingWindow::activate(checks.probe.window());
                    app.show_practice_with_origin(PracticeOrigin::Reminder)
                        .unwrap();
                }
                13 => {
                    assert!(practice_window(app).get_knowledge_only());
                    assert_eq!(
                        unsafe { GetForegroundWindow() },
                        native_handle(checks.probe.window()),
                        "Fact viewing keeps the other window focused"
                    );
                    assert!(app.store.load().unwrap().settings.focus_on_reminder);
                    snapshot(
                        practice_window(app).window(),
                        checks.output.join("knowledge_without_activation.png"),
                    );
                    checks.finished.set(true);
                    checks.probe.hide().unwrap();
                    app.quit();
                }
                _ => unreachable!(),
            }
        }));
        if result.is_err() {
            let _ = checks.probe.hide();
            checks.app.quit();
        } else if phase < 13 {
            practice_focus_phase(checks, phase + 1);
        }
    });
}

#[test]
#[ignore = "opens isolated native windows and checks automatic keyboard focus"]
fn native_practice_focus() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/practice_focus_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let library = store.load().unwrap();
    assert!(library.settings.focus_on_reminder);
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let window_hook = floating.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .with_winit_window_attributes_hook(move |attributes| window_hook.attributes(attributes))
        .select()
        .unwrap();
    let app = App::new(store, library, floating).unwrap();
    app.start(true).unwrap();
    let checks = Rc::new(PracticeFocusChecks {
        app,
        probe: FocusProbe::new().unwrap(),
        output,
        finished: Cell::new(false),
    });
    practice_focus_phase(checks.clone(), 0);
    let timeout = checks.clone();
    Timer::single_shot(Duration::from_secs(20), move || {
        if !timeout.finished.get() {
            let _ = timeout.probe.hide();
            timeout.app.quit();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Automatic practice focus check did not complete"
    );
}

fn starter_question(app: &Rc<App>, pack_id: &str, card_index: i32) {
    app.close_practice();
    app.show_main().unwrap();
    let main = main_window(app);
    let saved = app.store.load().unwrap();
    for pack in &saved.packs {
        main.invoke_toggle_pack(pack.id.clone().into(), pack.id == pack_id);
    }
    let index = saved.packs.iter().position(|p| p.id == pack_id).unwrap();
    main.invoke_select_pack(index as i32);
    main.invoke_toggle_all_cards(false);
    main.invoke_toggle_card(card_index, true);
    assert_eq!(app.store.load().unwrap().enabled_card_count(), 1);
    app.show_practice().unwrap();
}

fn starter_packs_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0..=4 => {
                    let index = usize::from(phase);
                    let main = main_window(app);
                    let saved = app.store.load().unwrap();
                    assert_eq!(main.get_packs().row_count(), 5);
                    assert_eq!(saved.enabled_card_count(), 20);
                    assert_eq!(main.get_selected_index(), i32::from(phase));
                    assert_eq!(main.get_selected_name(), saved.packs[index].name);
                    assert_eq!(main.get_cards().row_count(), 4);
                    snapshot(
                        main.window(),
                        checks.output.join(format!("{}.png", saved.packs[index].id)),
                    );
                    if phase < 4 {
                        main.invoke_select_pack(i32::from(phase) + 1);
                    } else {
                        for pack in &saved.packs {
                            main.invoke_toggle_pack(
                                pack.id.clone().into(),
                                pack.id == "english_recall",
                            );
                        }
                        main.window()
                            .set_size(slint::LogicalSize::new(720.0, 480.0));
                    }
                }
                5 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("minimum_size.png"));
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 4);
                    app.show_practice().unwrap();
                }
                6 | 8 | 10 | 12 | 14 => {
                    let window = practice_window(app);
                    let (pack_id, answer) = {
                        let practice = app.practice.borrow();
                        let session = practice.as_ref().unwrap();
                        (
                            session.question.key.pack_id.clone(),
                            session.question.card.answer.clone(),
                        )
                    };
                    let expected = match phase {
                        6 => "english_recall",
                        8 | 10 => "programming_recall",
                        12 => "math_recall",
                        14 => "chemistry_recall",
                        _ => unreachable!(),
                    };
                    assert_eq!(pack_id, expected);
                    snapshot(
                        window.window(),
                        checks.output.join(format!("question_{phase}.png")),
                    );
                    for character in answer.chars() {
                        key(window.window(), character.to_string().into());
                    }
                    key(window.window(), Key::Return.into());
                    assert_eq!(window.get_answer_text(), answer);
                    assert_eq!(window.get_feedback(), 1);
                    if phase < 12 {
                        assert_eq!(window.get_input_preview().formula_width, 0.0);
                    } else {
                        assert!(window.get_prompt_formula_width() > 0.0);
                        assert!(window.get_input_preview().formula_width > 0.0);
                    }
                }
                7 | 9 | 11 | 13 | 15 => {
                    snapshot(
                        practice_window(app).window(),
                        checks.output.join(format!("answer_{phase}.png")),
                    );
                    match phase {
                        7 => starter_question(app, "programming_recall", 1),
                        9 => starter_question(app, "programming_recall", 2),
                        11 => starter_question(app, "math_recall", 1),
                        13 => starter_question(app, "chemistry_recall", 3),
                        15 => {
                            std::fs::write(checks.output.join("result.txt"),
                                "Fresh five-domain profile, concise prompts, pack browsing, minimum size, English words, Rust/C++ code preserved as text, calculus and thermodynamics typed answers passed.").unwrap();
                            checks.finished.set(true);
                            app.quit();
                            return;
                        }
                        _ => unreachable!(),
                    }
                }
                _ => unreachable!(),
            }
            starter_packs_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens the native window with the fresh cross-domain starter packs"]
fn native_starter_packs() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/starter_content_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = Store::open(profile.path().to_owned()).unwrap();
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.interface_language = InterfaceLanguage::Chinese;
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    starter_packs_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(20), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Starter pack check failed or timed out"
    );
}

fn practice_text_phase(checks: Rc<AnswerChecks>, case: u8, stage: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            if stage == 0 {
                let (prompt, answer, knowledge) = match case {
                    0 => ("hesitate".into(), "犹豫".into(), false),
                    1 => ("第一行提示\n第二行提示\n第三行提示".into(), "test".into(), false),
                    2 => ("下面这段提示包含比较多的文字，用来检查练习卡片能否保留正常边距并完整显示内容，而不是把题面的开头和结尾裁掉。".into(), "test".into(), false),
                    3 => ("标准答案显示检查".into(), "标准答案需要保留完整内容和正常边距，换行后不能只显示中间几行，也不能与作答区域挤在一起。".repeat(2), false),
                    4 => ("边界检查：很长的题面仍能滚动查看完整内容。".repeat(24), "终点".into(), false),
                    5 => ("知识展示中的短提示也可以自然换行，不应与答案重叠。".into(), "明确答案需要完整显示，保留内容之间的间距。".repeat(3), true),
                    6 => ("Identify the standard smart pointer that owns its object exclusively and releases the object automatically when the owner leaves its scope. Keep the whole cue visible.".into(), "std::unique_ptr".into(), false),
                    7 => ("abcdefghijklmnopqrstuvwxyz".repeat(4), "test".into(), false),
                    _ => unreachable!(),
                };
                app.close_practice();
                app.knowledge_only.set(knowledge);
                {
                    let mut library = app.library.borrow_mut();
                    let card = &mut library.packs[0].cards[0];
                    card.prompt = prompt;
                    card.answer = answer;
                    app.store.save(&library).unwrap();
                }
                app.show_practice().unwrap();
                practice_text_phase(checks.clone(), case, 1);
                return;
            }
            let window = practice_window(app);
            if stage == 1 {
                snapshot(
                    window.window(),
                    checks.output.join(format!("case_{case}.png")),
                );
                let size = window
                    .window()
                    .size()
                    .to_logical(window.window().scale_factor());
                println!(
                    "Text case {case}: height={} natural={} available={}",
                    size.height,
                    window.get_prompt_natural_height(),
                    window.get_prompt_area_height()
                );
                assert!(
                    window.get_prompt_area_height() + 0.5 >= window.get_prompt_natural_height(),
                    "Cue lines must fit without clipping their first or last row"
                );
                assert!(size.height >= 230.0 && size.height <= 561.0);
                if case == 0 {
                    assert!(
                        (window.get_formula_button_center_x() - 274.0).abs() <= 0.5,
                        "The answer area retains symmetric 24-pixel side padding"
                    );
                    assert!(
                        (size.height
                            - if window.get_pack_tag_visible() {
                                256.0
                            } else {
                                230.0
                            })
                        .abs()
                            <= 1.0,
                        "Short cues retain the compact card plus the optional tag header"
                    );
                }
                if case == 3 {
                    key(window.window(), "wrong".into());
                    key(window.window(), Key::Return.into());
                    assert_eq!(window.get_feedback(), 2);
                    practice_text_phase(checks.clone(), case, 2);
                    return;
                }
                if case == 4 {
                    assert!(window.get_content_height() > window.get_visible_content_height());
                    window
                        .window()
                        .dispatch_event(WindowEvent::PointerScrolled {
                            position: slint::LogicalPosition::new(160.0, 80.0),
                            delta_x: 0.0,
                            delta_y: -10000.0,
                        });
                    practice_text_phase(checks.clone(), case, 2);
                    return;
                }
            } else if case == 3 {
                snapshot(window.window(), checks.output.join("standard_answer.png"));
                println!(
                    "Standard answer: natural={} available={}",
                    window.get_answer_natural_height(),
                    window.get_answer_area_height()
                );
                assert!(
                    window.get_answer_area_height() + 0.5 >= window.get_answer_natural_height(),
                    "The full standard answer must fit after submitting a wrong answer"
                );
            } else {
                snapshot(window.window(), checks.output.join("scroll_end.png"));
                assert!(
                    window.get_content_scroll_y() < -100.0,
                    "Mouse wheel reveals the lower content"
                );
                assert!(
                    window.get_content_height() - window.get_visible_content_height()
                        + window.get_content_scroll_y()
                        <= 1.0,
                    "The last line remains reachable"
                );
            }
            if case == 7 {
                std::fs::write(checks.output.join("result.txt"),
                    "Short/multiline cues, long Chinese/English/identifiers, standard-answer feedback, knowledge display and bounded scrolling passed.").unwrap();
                checks.finished.set(true);
                app.quit();
            } else {
                practice_text_phase(checks.clone(), case + 1, 0);
            }
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens the native practice card to verify multiline content fits"]
fn native_practice_text_layout() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/practice_text_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = Store::open(profile.path().to_owned()).unwrap();
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.enabled_pack_ids = vec![library.packs[0].id.clone()];
    library.packs[0].cards.truncate(1);
    let card = &mut library.packs[0].cards[0];
    card.prompt_latex = None;
    card.answer_latex = None;
    card.aliases.clear();
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    practice_text_phase(checks.clone(), 0, 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.finished.get(), "Native practice text layout failed");
}

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

fn native_handle(window: &slint::Window) -> HWND {
    window
        .with_winit_window(|native| match native.window_handle().unwrap().as_raw() {
            RawWindowHandle::Win32(handle) => handle.hwnd.get() as HWND,
            _ => panic!("Expected a Windows native window"),
        })
        .unwrap()
}

fn check_floating_window(window: &slint::Window) {
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

fn header_colors(window: &slint::Window) -> (u32, u32) {
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

fn check_no_caption_paint(window: &slint::Window) {
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

fn mouse_button(flags: u32) {
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

struct Checks {
    app: Rc<App>,
    output: PathBuf,
    finished: Cell<bool>,
    old_main: RefCell<Option<slint::Weak<MainWindow>>>,
    old_practice: RefCell<Option<slint::Weak<PracticeWindow>>>,
    focus_probe: RefCell<Option<FocusProbe>>,
    creation_foreground: Rc<Cell<Option<(HWND, HWND)>>>,
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

fn answer_phase(checks: Rc<AnswerChecks>, phase: u8) {
    // Newly shown windows can still be in a DWM transition at 250 ms. Wait
    // before their first desktop comparison; keep repaint-message checks
    // synchronous so client redraws cannot conceal unwanted caption paint.
    let delay = if matches!(phase, 2 | 5) { 500 } else { 250 };
    Timer::single_shot(Duration::from_millis(delay), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    assert_eq!(main.get_answer_mode_labels().row_count(), 3);
                    main.invoke_set_all_answer_modes(1);
                    assert_eq!(
                        app.store.load().unwrap().uniform_answer_mode("test_pack"),
                        Some(AnswerMode::MultipleChoice)
                    );
                }
                1 => {
                    let main = main_window(app);
                    assert_eq!(
                        main.get_uniform_answer_mode(),
                        1,
                        "The visible selector must keep the saved mode after model updates"
                    );
                    snapshot(main.window(), checks.output.join("main_modes.png"));
                    app.tray.invoke_practice_now();
                }
                2 => {
                    let window = practice_window(app);
                    check_floating_window(window.window());
                    check_no_caption_paint(window.window());
                    assert_eq!(window.get_answer_mode(), 1);
                    assert_eq!(window.get_options().row_count(), 3);
                    assert!((0..3).all(|index| {
                        window.get_options().row_data(index).unwrap().formula_width > 0.0
                    }));
                    snapshot(window.window(), checks.output.join("multiple_choice.png"));
                    let wrong = (0..3)
                        .find(|index| {
                            app.practice
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .prepared
                                .accepts_choice(*index)
                                == Some(false)
                        })
                        .unwrap();
                    click_option(&window, wrong);
                    assert_eq!(
                        window.get_feedback(),
                        2,
                        "Clicking a tile immediately judges it"
                    );
                    assert_eq!(window.get_selected_option(), wrong as i32);
                    click_option(&window, (wrong + 1) % 3);
                    assert_eq!(
                        window.get_selected_option(),
                        wrong as i32,
                        "Feedback locks the first choice"
                    );
                    let before: Vec<_> = (0..3)
                        .map(|index| window.get_options().row_data(index).unwrap().label)
                        .collect();
                    window.invoke_rasterize_formulas(window.get_pixel_scale());
                    let after: Vec<_> = (0..3)
                        .map(|index| window.get_options().row_data(index).unwrap().label)
                        .collect();
                    assert_eq!(before, after, "DPI rerasterization keeps option order");
                }
                3 => {
                    let window = practice_window(app);
                    snapshot(
                        window.window(),
                        checks.output.join("multiple_choice_wrong.png"),
                    );
                    assert!(!app.feedback_timer.running());
                    key(window.window(), Key::Escape.into());
                    assert!(app.practice.borrow().is_some());
                    key(window.window(), Key::Return.into());
                    assert!(app.practice.borrow().is_none());
                    app.show_practice().unwrap();
                }
                4 => {
                    let window = practice_window(app);
                    let correct = (0..3)
                        .find(|index| {
                            app.practice
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .prepared
                                .accepts_choice(*index)
                                == Some(true)
                        })
                        .unwrap();
                    key(window.window(), (correct + 1).to_string().into());
                    assert_eq!(window.get_feedback(), 1, "Number keys select a choice");
                    snapshot(
                        window.window(),
                        checks.output.join("multiple_choice_correct.png"),
                    );
                    key(window.window(), Key::Return.into());
                    app.show_main().unwrap();
                    main_window(app).invoke_set_all_answer_modes(2);
                    assert_eq!(
                        app.store.load().unwrap().uniform_answer_mode("test_pack"),
                        Some(AnswerMode::TrueFalse)
                    );
                    app.tray.invoke_practice_now();
                }
                5 => {
                    let window = practice_window(app);
                    check_floating_window(window.window());
                    check_no_caption_paint(window.window());
                    assert_eq!(window.get_options().row_count(), 2);
                    assert_eq!(window.get_answer_mode(), 2);
                    let session = app.practice.borrow();
                    let statement = session
                        .as_ref()
                        .unwrap()
                        .question
                        .card
                        .true_false
                        .as_ref()
                        .unwrap();
                    assert_eq!(window.get_prompt().as_str(), statement.statement);
                    assert_eq!(
                        window.get_standard_answer().as_str(),
                        if statement.is_true { "✓" } else { "✕" }
                    );
                    let correct = usize::from(!statement.is_true);
                    drop(session);
                    snapshot(window.window(), checks.output.join("true_false.png"));
                    click_option(&window, correct);
                    assert_eq!(window.get_feedback(), 1);
                    snapshot(
                        window.window(),
                        checks.output.join("true_false_correct.png"),
                    );
                    key(window.window(), Key::Return.into());
                    app.show_practice().unwrap();
                }
                6 => {
                    let window = practice_window(app);
                    let wrong = (0..2)
                        .find(|index| {
                            app.practice
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .prepared
                                .accepts_choice(*index)
                                == Some(false)
                        })
                        .unwrap();
                    click_option(&window, wrong);
                    assert_eq!(window.get_feedback(), 2);
                    snapshot(window.window(), checks.output.join("true_false_wrong.png"));
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        answer_phase(checks, 7);
                    });
                    return;
                }
                7 => {
                    let window = practice_window(app);
                    assert_eq!(window.get_feedback(), 2);
                    assert!(!app.feedback_timer.running());
                    key(window.window(), Key::Escape.into());
                    assert!(app.practice.borrow().is_some());
                    key(window.window(), Key::Return.into());
                    assert!(app.practice.borrow().is_none());
                    app.show_practice().unwrap();
                    let window = practice_window(app);
                    let correct = (0..2)
                        .find(|index| {
                            app.practice
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .prepared
                                .accepts_choice(*index)
                                == Some(true)
                        })
                        .unwrap();
                    click_option(&window, correct);
                    assert_eq!(window.get_feedback(), 1);
                    assert!(app.feedback_timer.running());
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        answer_phase(checks, 8);
                    });
                    return;
                }
                8 => {
                    assert!(
                        app.practice.borrow().is_none(),
                        "Correct feedback times out"
                    );
                    app.show_main().unwrap();
                    main_window(app).invoke_toggle_knowledge_mode();
                    app.show_practice().unwrap();
                    assert!(practice_window(app).get_knowledge_only());
                    assert!(app.feedback_timer.running());
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        answer_phase(checks, 9);
                    });
                    return;
                }
                9 => {
                    assert!(app.practice.borrow().is_none(), "Facts still time out");
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_knowledge_mode();
                    main.invoke_set_all_answer_modes(0);
                    app.show_practice().unwrap();
                }
                10 => {
                    let window = practice_window(app);
                    for character in "wrong".chars() {
                        key(window.window(), character.to_string().into());
                    }
                    key(window.window(), Key::Return.into());
                    assert_eq!(window.get_feedback(), 2);
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        answer_phase(checks, 11);
                    });
                    return;
                }
                11 => {
                    let window = practice_window(app);
                    assert_eq!(window.get_feedback(), 2);
                    assert!(!app.feedback_timer.running());
                    snapshot(window.window(), checks.output.join("typed_wrong.png"));
                    key(window.window(), Key::Escape.into());
                    assert!(app.practice.borrow().is_some());
                    key(window.window(), Key::Return.into());
                    assert!(app.practice.borrow().is_none());
                    app.show_practice().unwrap();
                }
                12 => {
                    let window = practice_window(app);
                    let answer = app
                        .practice
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .question
                        .card
                        .answer
                        .clone();
                    for character in answer.chars() {
                        key(window.window(), character.to_string().into());
                    }
                    key(window.window(), Key::Return.into());
                    assert_eq!(window.get_feedback(), 1);
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        answer_phase(checks, 13);
                    });
                    return;
                }
                13 => {
                    assert!(
                        app.practice.borrow().is_none(),
                        "Correct typed answers time out"
                    );
                    std::fs::write(checks.output.join("result.txt"),
                        "Typed, multiple-choice and true/false wrong answers wait for Enter; Esc preserves wrong feedback; correct answers and facts retain automatic dismissal.").unwrap();
                    checks.finished.set(true);
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            answer_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens the native card to check selection, formulas, feedback and mode persistence"]
fn native_answer_modes() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/answer_modes_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.feedback_seconds = 2;
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let window_hook = floating.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .with_winit_window_attributes_hook(move |attributes| window_hook.attributes(attributes))
        .select()
        .unwrap();
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    answer_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Answer mode check failed or timed out"
    );
}

#[test]
#[ignore = "opens native cards to verify binary history and the knowledge-only bypass"]
fn native_adaptive_practice() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local_artifacts/adaptive_practice_build/native");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let state_path = profile.path().join("state.json");
    let learning_path = profile.path().join("learning_state.json");
    let original = std::fs::read(&state_path).unwrap();
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.remember_position = false;
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let window_hook = floating.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .with_winit_window_attributes_hook(move |attributes| window_hook.attributes(attributes))
        .select()
        .unwrap();
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let finished = Rc::new(Cell::new(false));
    let completed = finished.clone();
    let active = app.clone();
    Timer::single_shot(Duration::from_millis(400), move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert!(!active.learner.borrow().is_loaded());
            main_window(&active).invoke_toggle_knowledge_mode();
            active.show_practice().unwrap();
            assert!(practice_window(&active).get_knowledge_only());
            assert!(!active.learner.borrow().is_loaded());
            assert!(!learning_path.exists());
            practice_window(&active).invoke_dismiss();
            assert!(!learning_path.exists());

            active.show_main().unwrap();
            main_window(&active).invoke_toggle_knowledge_mode();
            active.show_practice().unwrap();
            let window = practice_window(&active);
            assert!(!window.get_knowledge_only());
            assert!(active.learner.borrow().is_loaded());
            window.invoke_submit_answer("".into());
            window.invoke_submit_answer("sqrt(".into());
            assert!(!learning_path.exists());
            let answer = active
                .practice
                .borrow()
                .as_ref()
                .unwrap()
                .question
                .card
                .answer
                .clone();
            window.set_answer_text(answer.clone().into());
            window.invoke_submit_answer(answer.into());
            let saved = std::fs::read(&learning_path).unwrap();
            let document: serde_json::Value = serde_json::from_slice(&saved).unwrap();
            assert_eq!(document["reviews"].as_array().unwrap().len(), 1);
            assert_eq!(document["reviews"][0]["correct"], true);
            window.invoke_submit_answer("wrong".into());
            assert_eq!(saved, std::fs::read(&learning_path).unwrap());
            assert!(!active.reminder_timer.running());
            active.close_practice();

            active.show_practice().unwrap();
            practice_window(&active).invoke_dismiss();
            assert_eq!(saved, std::fs::read(&learning_path).unwrap());
            active.show_main().unwrap();
            main_window(&active).invoke_toggle_knowledge_mode();
            active.show_practice().unwrap();
            assert!(practice_window(&active).get_knowledge_only());
            practice_window(&active).invoke_dismiss();
            assert_eq!(saved, std::fs::read(&learning_path).unwrap());
            assert_eq!(original, std::fs::read(state_path).unwrap());
            completed.set(true);
            println!(
                "Knowledge-only bypass, binary history, incomplete input, duplicate submit and dismissal passed."
            );
        }));
        if result.is_err() {
            eprintln!("Native adaptive-practice assertions failed.");
        }
        active.quit();
    });
    let watchdog = app.clone();
    Timer::single_shot(Duration::from_secs(15), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        finished.get(),
        "Native adaptive-practice check failed or timed out"
    );
}

fn later(checks: Rc<Checks>, phase: u8, delay: Duration) {
    Timer::single_shot(delay, move || {
        // Keep a failed assertion from unwinding through the native event loop
        // and leaving the test windows running after the test has failed.
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_phase(checks.clone(), phase)
        }))
        .is_err()
        {
            if let Some(probe) = checks.focus_probe.borrow_mut().take() {
                let _ = probe.hide();
            }
            checks.app.quit();
        }
    });
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

fn run_phase(checks: Rc<Checks>, phase: u8) {
    let app = &checks.app;
    match phase {
        0 => {
            let window = main_window(app);
            assert!(window.window().is_visible());
            snapshot(window.window(), checks.output.join("main_window.png"));
            window.invoke_set_interval(1);
            window.invoke_set_feedback(2);
            assert_eq!(app.store.load().unwrap().settings.interval_minutes, 1);
            window.set_current_tab(1);
        }
        1 => {
            let window = main_window(app);
            snapshot(window.window(), checks.output.join("settings_window.png"));
            window.invoke_show_about();
        }
        2 => {
            let window = main_window(app);
            snapshot(window.window(), checks.output.join("about_window.png"));
            window.invoke_hide_about();
            window.set_current_tab(0);
            app.tray.invoke_practice_now();
        }
        3 => {
            // Native tray actions run after the menu dispatch returns.
            checks
                .old_practice
                .replace(Some(practice_window(app).as_weak()));
            let window = practice_window(app);
            assert!(
                app.main_window.borrow().is_none(),
                "Starting practice closes the management window"
            );
            check_floating_window(window.window());
            assert!(
                window.get_prompt_formula_width() > 0.0,
                "LaTeX question should have an image"
            );
            assert!(
                window.get_answer_formula_width() > 0.0,
                "LaTeX standard answer should have an image"
            );
            assert!(window.get_prompt_formula().size().width > 0);
            let scale = window.window().scale_factor().clamp(1.0, 3.0);
            assert!((window.get_pixel_scale() - window.window().scale_factor()).abs() < 0.001);
            assert_eq!(
                window.get_prompt_formula().size().width,
                (window.get_prompt_formula_width() * scale).ceil() as u32,
                "formula pixels must match the native monitor's scale"
            );
            snapshot(window.window(), checks.output.join("practice_question.png"));
            key(window.window(), Key::Return.into());
            assert_eq!(window.get_feedback(), 0, "empty Enter should not be judged");
            for character in "随便试试".chars() {
                key(window.window(), character.to_string().into());
            }
            assert_eq!(window.get_answer_text(), "随便试试");
            key(window.window(), Key::Return.into());
            assert_eq!(window.get_feedback(), 2);
        }
        4 => {
            let window = practice_window(app);
            snapshot(window.window(), checks.output.join("practice_wrong.png"));
            let snapshot = window.window().take_snapshot().unwrap();
            assert_eq!(snapshot.as_slice()[0].r, 80);
            assert_eq!(snapshot.as_slice()[0].g, 19);
            assert_eq!(snapshot.as_slice()[0].b, 30);
            later(checks, 5, Duration::from_millis(2300));
            return;
        }
        5 => {
            let window = practice_window(app);
            assert_eq!(window.get_feedback(), 2);
            assert!(!app.feedback_timer.running());
            key(window.window(), Key::Escape.into());
            assert!(
                app.practice.borrow().is_some(),
                "Wrong feedback needs Enter"
            );
            key(window.window(), Key::Return.into());
            drop(window);
            assert!(
                app.practice.borrow().is_none(),
                "Enter confirms and closes wrong feedback"
            );
            assert!(
                checks
                    .old_practice
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .upgrade()
                    .is_none()
            );
            native_tray_start();
        }
        6 => {
            let window = practice_window(app);
            check_floating_window(window.window());
            check_no_caption_paint(window.window());
            let answer = app
                .practice
                .borrow()
                .as_ref()
                .unwrap()
                .question
                .card
                .answer
                .clone();
            for character in answer.chars() {
                key(window.window(), character.to_string().into());
            }
            key(window.window(), Key::Return.into());
            assert_eq!(window.get_feedback(), 1);
            snapshot(window.window(), checks.output.join("practice_correct.png"));
            let snapshot = window.window().take_snapshot().unwrap();
            assert_eq!(snapshot.as_slice()[0].r, 18);
            assert_eq!(snapshot.as_slice()[0].g, 62);
            window.window().with_winit_window(|native| {
                let position = native.outer_position().unwrap();
                native.set_outer_position(slint::winit_030::winit::dpi::PhysicalPosition::new(
                    position.x - 150,
                    position.y - 100,
                ));
            });
            key(window.window(), Key::Escape.into());
            assert!(app.practice.borrow().is_none());
            assert!(
                app.store.load().unwrap().settings.popup_position.is_some(),
                "Native moved position is saved when closing"
            );
            app.show_main().unwrap();
            let main = main_window(app);
            checks.old_main.replace(Some(main.as_weak()));
            main.invoke_go_background();
        }
        7 => {
            assert!(
                checks
                    .old_main
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .upgrade()
                    .is_none()
            );
            assert!(app.main_window.borrow().is_none());
            app.tray.invoke_open_main();
            later(checks, 15, Duration::from_millis(400));
            return;
        }
        15 => {
            let main = main_window(app);
            main.invoke_toggle_all_cards(false);
            assert!(!main.get_selected_enabled());
            assert!(!app.tray.get_can_practice());
            let mut replacement = app.library.borrow().packs[0].clone();
            replacement.name = "导入替换检查".into();
            app.pending_import.replace(Some(replacement));
            app.refresh();
            assert!(main.get_import_pending());
            later(checks, 8, Duration::from_millis(400));
            return;
        }
        8 => {
            let main = main_window(app);
            snapshot(main.window(), checks.output.join("import_confirmation.png"));
            main.window()
                .set_size(slint::LogicalSize::new(720.0, 600.0));
        }
        9 => {
            let main = main_window(app);
            snapshot(main.window(), checks.output.join("main_minimum_size.png"));
            main.invoke_confirm_import();
            assert!(!main.get_import_pending());
            assert!(!main.get_selected_enabled());
            assert_eq!(app.store.load().unwrap().packs[0].name, "导入替换检查");
            main.invoke_toggle_all_cards(true);
            main.invoke_set_pause(-1);
            assert!(!app.reminder_timer.running());
            main.invoke_set_pause(0);
            assert_eq!(main.get_pause_minutes(), 0);
            main.invoke_go_background();
            assert!(app.reminder_timer.running());
            let probe = FocusProbe::new().unwrap();
            probe.show().unwrap();
            FloatingWindow::activate(probe.window());
            checks.focus_probe.replace(Some(probe));
            println!(
                "Native windows, Chinese text, Enter/Esc, feedback timeout, tray callbacks, replacement and component release passed; waiting one minute for the real reminder."
            );
            later(checks, 10, Duration::from_secs(61));
            return;
        }
        10 => {
            assert!(
                app.practice.borrow().is_some(),
                "one-minute reminder should open a card"
            );
            snapshot(
                practice_window(app).window(),
                checks.output.join("scheduled_question.png"),
            );
            let (before_popup, after_popup) = checks.creation_foreground.get().unwrap();
            assert!(
                !before_popup.is_null(),
                "Reminder has an existing OS foreground window"
            );
            assert_eq!(
                after_popup, before_popup,
                "Real scheduled popup must keep the OS foreground window from just before it opened"
            );
            let window = practice_window(app);
            check_floating_window(window.window());
            let expected = app.store.load().unwrap().settings.popup_position.unwrap();
            let actual = FloatingWindow::remember(window.window()).unwrap();
            assert_eq!(actual, expected, "Native drag position survives reopening");
            app.tray.invoke_practice_now();
            println!(
                "Real reminder kept the foreground window; checking the open card across focus changes."
            );
        }
        11 => {
            let window = practice_window(app);
            assert_eq!(
                unsafe { GetForegroundWindow() },
                native_handle(window.window()),
                "Manual opening activates an existing card"
            );
            check_no_caption_paint(window.window());
            FloatingWindow::activate(checks.focus_probe.borrow().as_ref().unwrap().window());
        }
        12 => {
            check_no_caption_paint(practice_window(app).window());
            app.tray.invoke_practice_now();
        }
        13 => {
            let window = practice_window(app);
            check_no_caption_paint(window.window());
            let hwnd = native_handle(window.window());
            // Exercise the same native paint path repeatedly before any client
            // redraw can hide a caption. Do not alter window styles here.
            for _ in 0..100 {
                unsafe {
                    SendMessageW(hwnd, WM_NCACTIVATE, 0, 0);
                    SendMessageW(hwnd, WM_NCACTIVATE, 1, 0);
                    SendMessageW(hwnd, WM_NCPAINT, 1, 0);
                }
                check_no_caption_paint(window.window());
            }
            later(checks, 14, Duration::from_secs(15));
            return;
        }
        14 => {
            check_no_caption_paint(practice_window(app).window());
            practice_window(app).invoke_dismiss();
            checks
                .focus_probe
                .borrow_mut()
                .take()
                .unwrap()
                .hide()
                .unwrap();
            checks.finished.set(true);
            app.tray.invoke_quit_app();
            println!(
                "Real scheduled reminder, open-card focus changes, 100 native repaint cycles, idle display and quit passed."
            );
            return;
        }
        _ => unreachable!(),
    }
    later(checks, phase + 1, Duration::from_millis(400));
}

#[test]
#[ignore = "opens native windows and waits for a real one-minute reminder"]
fn native_lifecycle() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/native_checks");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    // This lifecycle check covers the explicitly passive reminder preference.
    library.settings.focus_on_reminder = false;
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let window_hook = floating.clone();
    let creation_foreground = Rc::new(Cell::new(None));
    let foreground_sample = creation_foreground.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .with_winit_window_attributes_hook(move |attributes| {
            // Compare the actual foreground immediately before/after creation,
            // allowing the user to change applications during the minute wait.
            let before = unsafe { GetForegroundWindow() };
            let sample = foreground_sample.clone();
            Timer::single_shot(Duration::ZERO, move || {
                sample.set(Some((before, unsafe { GetForegroundWindow() })));
            });
            window_hook.attributes(attributes)
        })
        .select()
        .unwrap();
    let app = App::new(store, library, floating).unwrap();
    let icon = app.icon.to_rgba8().unwrap();
    image::save_buffer_with_format(
        output.join("logo.png"),
        icon.as_bytes(),
        icon.width(),
        icon.height(),
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(Checks {
        app,
        output,
        finished: Cell::new(false),
        old_main: RefCell::new(None),
        old_practice: RefCell::new(None),
        focus_probe: RefCell::new(None),
        creation_foreground,
    });
    later(checks.clone(), 0, Duration::from_millis(500));
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(95), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.finished.get(), "native lifecycle timed out");
}

fn matrix_row_height(main: &MainWindow, row: usize) -> f32 {
    let card = main.get_cards().row_data(row).unwrap();
    let answer = if card.answer_mode == 2 {
        36.0
    } else {
        36.0_f32.max(card.answer_preview.formula_height + 6.0)
    };
    52.0_f32
        .max(card.preview.formula_height + 16.0)
        .max(answer + 16.0)
}

fn click_matrix(main: &MainWindow, row: Option<usize>, mode: usize) {
    let position = slint::LogicalPosition::new(
        main.get_matrix_area_left() + main.get_matrix_area_width()
            - (3 - mode) as f32 * main.get_matrix_mode_width()
            + main.get_matrix_mode_width() / 2.0,
        row.map_or(main.get_matrix_area_top() + 32.0 + 23.0, |row| {
            main.get_question_area_top()
                + (0..row).map(|i| matrix_row_height(main, i)).sum::<f32>()
                + matrix_row_height(main, row) / 2.0
        }),
    );
    main.window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    main.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    main.window().dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

fn management_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(300), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    assert_eq!(main.get_cards().row_count(), 20);
                    main.invoke_set_card_answer_mode(0, 1);
                    main.invoke_set_card_answer_mode(1, 2);
                    assert_eq!(main.get_cards().row_data(0).unwrap().answer_mode, 1);
                    assert_eq!(main.get_cards().row_data(1).unwrap().answer_mode, 2);
                    assert_eq!(main.get_cards().row_data(2).unwrap().answer_mode, 0);
                    assert_eq!(main.get_uniform_answer_mode(), -1);
                    snapshot(main.window(), checks.output.join("question_list.png"));
                    assert!(
                        main.get_question_area_height() >= 144.0,
                        "The list must display at least three complete rows at the default size; actual height {}",
                        main.get_question_area_height()
                    );
                    main.invoke_toggle_all_cards(false);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 0);
                    let position = slint::LogicalPosition::new(
                        main.get_question_area_left() + 18.0,
                        main.get_question_area_top() + 2.0 * 58.0 + 29.0,
                    );
                    main.window()
                        .dispatch_event(WindowEvent::PointerMoved { position });
                    main.window().dispatch_event(WindowEvent::PointerPressed {
                        position,
                        button: PointerEventButton::Left,
                    });
                    main.window().dispatch_event(WindowEvent::PointerReleased {
                        position,
                        button: PointerEventButton::Left,
                    });
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 1);
                    assert!(main.get_selection_partial());
                    assert!(!main.get_selected_enabled());
                    click_matrix(&main, Some(2), 1);
                    assert_eq!(
                        main.get_cards().row_data(2).unwrap().answer_mode,
                        1,
                        "One click changes only this row"
                    );
                    assert_eq!(main.get_cards().row_data(1).unwrap().answer_mode, 2);
                    // Let model changes settle before the next keyboard event.
                    management_phase(checks.clone(), 16);
                    return;
                }
                16 => {
                    let main = main_window(app);
                    key(main.window(), Key::Tab.into());
                    snapshot(main.window(), checks.output.join("matrix_keyboard.png"));
                    management_phase(checks.clone(), 17);
                    return;
                }
                17 => {
                    let main = main_window(app);
                    key(main.window(), Key::Space.into());
                    assert_eq!(
                        main.get_cards().row_data(2).unwrap().answer_mode,
                        2,
                        "Tab and Space operate matrix cells"
                    );
                    click_matrix(&main, Some(2), 0);
                    assert_eq!(main.get_cards().row_data(2).unwrap().answer_mode, 0);
                    main.set_question_scroll_y(-144.0);
                    main.invoke_edit_card(2);
                    management_phase(checks.clone(), 1);
                    return;
                }
                1 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("question_editor.png"));
                    assert!(
                        main.get_question_area_height() >= 200.0,
                        "The editor must fit its basic fields; actual height {}",
                        main.get_question_area_height()
                    );
                    assert!(main.get_editor_visible());
                    assert!(main.get_editor_prompt_preview().formula_width > 0.0);
                    assert!(main.get_editor_answer_preview().formula_width > 0.0);
                    let before = app.store.load().unwrap();
                    main.set_editor_answer("".into());
                    main.invoke_save_editor();
                    assert!(!main.get_editor_error().is_empty());
                    assert!(main.get_editor_visible());
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_cancel_editor();
                    assert!(!main.get_editor_visible());
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_edit_card(2);
                    main.set_editor_prompt("√3/2 = ?（编辑）".into());
                    main.invoke_save_editor();
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[0].cards[2].prompt, "√3/2 = ?（编辑）");
                    assert_eq!(saved.packs[0].cards[2].id, before.packs[0].cards[2].id);
                    assert_eq!(saved.enabled_card_count(), 1);
                    main.invoke_toggle_all_cards(true);
                    main.invoke_add_card();
                }
                2 => {
                    let main = main_window(app);
                    assert_eq!(main.get_editor_title(), "新增题目");
                    main.set_editor_prompt("犹豫的英文动词？".into());
                    main.set_editor_answer("hesitate".into());
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible());
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[0].cards.len(), 21);
                    assert_eq!(saved.enabled_card_count(), 21);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(20, true);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 1);
                    app.tray.invoke_practice_now();
                }
                3 => {
                    let card = practice_window(app);
                    check_floating_window(card.window());
                    assert_eq!(card.get_prompt(), "犹豫的英文动词？");
                    assert_eq!(card.get_feedback(), 0);
                    assert_eq!(
                        app.practice.borrow().as_ref().unwrap().question.card.answer,
                        "hesitate"
                    );
                    snapshot(card.window(), checks.output.join("selected_question.png"));
                    for character in "hesitate".chars() {
                        key(card.window(), character.to_string().into());
                    }
                    assert_eq!(
                        card.get_answer_text(),
                        "hesitate",
                        "Native keyboard text must arrive before grading"
                    );
                    key(card.window(), Key::Return.into());
                    assert_eq!(card.get_feedback(), 1);
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(0, true);
                    app.tray.invoke_practice_now();
                }
                4 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_answer_mode(), 1);
                    assert_eq!(card.get_options().row_count(), 3);
                    snapshot(card.window(), checks.output.join("mixed_choice.png"));
                    let correct = (0..3)
                        .find(|index| {
                            app.practice
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .prepared
                                .accepts_choice(*index)
                                == Some(true)
                        })
                        .unwrap();
                    key(card.window(), (correct + 1).to_string().into());
                    assert_eq!(card.get_feedback(), 1);
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(1, true);
                    app.tray.invoke_practice_now();
                }
                5 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_answer_mode(), 2);
                    assert_eq!(card.get_options().row_count(), 2);
                    snapshot(card.window(), checks.output.join("mixed_judgment.png"));
                    let correct = (0..2)
                        .find(|index| {
                            app.practice
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .prepared
                                .accepts_choice(*index)
                                == Some(true)
                        })
                        .unwrap();
                    click_option(&card, correct);
                    assert_eq!(card.get_feedback(), 1);
                    app.close_practice();
                    app.show_main().unwrap();
                }
                6 => {
                    let main = main_window(app);
                    assert_eq!(main.get_uniform_answer_mode(), -1);
                    assert_eq!(main.get_cards().row_data(0).unwrap().answer_mode, 1);
                    assert_eq!(main.get_cards().row_data(1).unwrap().answer_mode, 2);
                    let participation = app.store.load().unwrap().settings.excluded_card_ids;
                    click_matrix(&main, None, 0);
                    assert_eq!(main.get_uniform_answer_mode(), 0);
                    assert!(
                        app.store
                            .load()
                            .unwrap()
                            .settings
                            .card_answer_modes
                            .is_empty()
                    );
                    assert_eq!(
                        app.store.load().unwrap().settings.excluded_card_ids,
                        participation
                    );
                    let before = app.store.load().unwrap();
                    main.invoke_set_all_answer_modes(1);
                    assert_eq!(
                        app.store.load().unwrap(),
                        before,
                        "Missing choice content rejects the entire batch"
                    );
                    main.invoke_set_all_answer_modes(2);
                    assert_eq!(app.store.load().unwrap(), before);
                    assert!(!main.get_editor_visible());
                    main.invoke_set_card_answer_mode(20, 2);
                    assert!(
                        !main.get_editor_visible(),
                        "Unavailable types do not unexpectedly open editing"
                    );
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_edit_card(20);
                    main.invoke_set_editor_answer_mode(2);
                    main.invoke_save_editor();
                    assert!(!main.get_editor_error().is_empty());
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_cancel_editor();
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_edit_card(20);
                    main.invoke_set_editor_answer_mode(2);
                }
                7 => {
                    let main = main_window(app);
                    main.set_editor_statement("hesitate 表示犹豫".into());
                    main.invoke_select_truth(true);
                    main.invoke_save_editor();
                    assert!(
                        !main.get_editor_visible(),
                        "Judgment saves with only its statement and truth choice"
                    );
                    assert_eq!(main.get_cards().row_data(20).unwrap().answer_mode, 2);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 1);
                    main.invoke_create_pack();
                }
                8 => {
                    let main = main_window(app);
                    assert!(main.get_editor_new_pack());
                    main.set_editor_pack_name("我的词汇".into());
                    main.set_editor_prompt("尝试的英文？".into());
                    main.set_editor_answer("try".into());
                    main.invoke_set_editor_answer_mode(2);
                    main.invoke_set_editor_answer_mode(0);
                    snapshot(main.window(), checks.output.join("new_pack_editor.png"));
                    main.invoke_save_editor();
                    let saved = app.store.load().unwrap();
                    assert_eq!(main.get_uniform_answer_mode(), 0);
                    assert_eq!(saved.packs.len(), 2);
                    assert_eq!(saved.packs[1].name, "我的词汇");
                    assert!(
                        saved.packs[1].cards[0].true_false.is_none(),
                        "An empty judgment draft does not persist after switching back to input"
                    );
                    let before = saved;
                    main.invoke_edit_card(0);
                    main.set_editor_prompt("未保存内容".into());
                    main.invoke_cancel_editor();
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_select_pack(0);
                    assert!(!main.get_description_expanded());
                    snapshot(main.window(), checks.output.join("pack_title.png"));
                    main.invoke_begin_rename();
                }
                9 => {
                    let main = main_window(app);
                    assert!(main.get_name_editing());
                    snapshot(main.window(), checks.output.join("pack_rename.png"));
                    main.set_name_draft("  ".into());
                    key(main.window(), Key::Return.into());
                    assert!(main.get_name_editing());
                    assert!(!main.get_metadata_error().is_empty());
                    let before = app.store.load().unwrap();
                    main.set_name_draft("常见角".into());
                    key(main.window(), Key::Return.into());
                    assert!(!main.get_name_editing());
                    let renamed = app.store.load().unwrap();
                    assert_eq!(renamed.packs[0].name, "常见角");
                    assert_eq!(renamed.packs[0].id, before.packs[0].id);
                    assert_eq!(renamed.packs[0].cards, before.packs[0].cards);
                    assert_eq!(renamed.settings, before.settings);
                    assert_eq!(main.get_packs().row_data(0).unwrap().name, "常见角");
                    main.invoke_begin_description_edit();
                }
                10 => {
                    let main = main_window(app);
                    main.set_description_draft(
                        "常见角的正弦、余弦与正切。\n偶尔练一道，记住就好。".into(),
                    );
                    main.invoke_save_pack_description();
                    assert!(!main.get_description_editing());
                    assert!(main.get_description_expanded());
                    let saved = app.store.load().unwrap();
                    assert!(saved.packs[0].description.contains("偶尔练一道"));
                    let exported = checks.output.join("authored_pack.json");
                    Store::export_pack(&exported, &saved.packs[0]).unwrap();
                    assert_eq!(Store::read_pack(&exported).unwrap(), saved.packs[0]);
                    main.invoke_toggle_card(0, true);
                    assert!(
                        main.get_description_expanded(),
                        "Row changes preserve disclosure state"
                    );
                    snapshot(main.window(), checks.output.join("pack_details.png"));
                    main.invoke_toggle_knowledge_mode();
                    assert!(main.get_knowledge_only());
                    assert_eq!(app.store.load().unwrap(), app.library.borrow().clone());
                    main.invoke_toggle_all_cards(false);
                    let before = app.store.load().unwrap();
                    main.invoke_select_pack(1);
                    assert!(!main.get_description_expanded());
                    assert!(main.get_knowledge_only(), "Mode is global across packs");
                    assert_eq!(app.store.load().unwrap(), before);
                    app.tray.invoke_practice_now();
                }
                11 => {
                    let card = practice_window(app);
                    check_floating_window(card.window());
                    check_no_caption_paint(card.window());
                    assert!(card.get_knowledge_only());
                    assert_eq!(card.get_prompt(), "尝试的英文？");
                    assert_eq!(card.get_standard_answer(), "try");
                    assert_eq!(card.get_options().row_count(), 0);
                    card.invoke_submit_answer("try".into());
                    card.invoke_choose_answer(0);
                    assert_eq!(card.get_feedback(), 0);
                    key(card.window(), "1".into());
                    assert_eq!(card.get_answer_text(), "");
                    snapshot(card.window(), checks.output.join("knowledge_text.png"));
                    key(card.window(), Key::Return.into());
                    assert!(app.practice.borrow().is_none());
                    app.show_main().unwrap();
                    let main = main_window(app);
                    assert!(
                        main.get_knowledge_only(),
                        "Closing windows keeps the temporary mode"
                    );
                    main.invoke_toggle_all_cards(false);
                    main.invoke_select_pack(0);
                    main.invoke_set_card_answer_mode(0, 1);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(0, true);
                    snapshot(main.window(), checks.output.join("knowledge_matrix.png"));
                    // A timer-origin popup requires a closed main window and no pause.
                    main.invoke_set_pause(0);
                    main.invoke_go_background();
                    let probe = FocusProbe::new().unwrap();
                    probe.show().unwrap();
                    FloatingWindow::activate(probe.window());
                    checks.focus_probe.replace(Some(probe));
                    app.show_practice_with_origin(PracticeOrigin::Reminder)
                        .unwrap();
                }
                12 => {
                    let card = practice_window(app);
                    // Timer-origin knowledge cards preserve the other foreground window.
                    assert_eq!(
                        unsafe { GetForegroundWindow() },
                        native_handle(checks.focus_probe.borrow().as_ref().unwrap().window())
                    );
                    assert!(card.get_knowledge_only());
                    assert!(card.get_prompt_formula_width() > 0.0);
                    assert!(
                        card.get_standard_answer().is_empty(),
                        "A simple equation becomes a complete fact"
                    );
                    assert!(!card.get_prompt().contains('?'));
                    assert!(card.get_prompt().contains("1/2"));
                    assert_eq!(card.get_options().row_count(), 0);
                    snapshot(card.window(), checks.output.join("knowledge_math.png"));
                    checks
                        .focus_probe
                        .borrow_mut()
                        .take()
                        .unwrap()
                        .hide()
                        .unwrap();
                    let before = app.store.load().unwrap();
                    key(card.window(), Key::Escape.into());
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_knowledge_mode();
                    assert!(!main.get_knowledge_only());
                    assert_eq!(
                        app.store.load().unwrap(),
                        before,
                        "Global mode never writes quiz settings"
                    );
                    app.tray.invoke_practice_now();
                }
                13 => {
                    let card = practice_window(app);
                    assert!(!card.get_knowledge_only());
                    assert_eq!(
                        card.get_answer_mode(),
                        1,
                        "Quiz mode restores this question's actual type"
                    );
                    assert_eq!(card.get_options().row_count(), 3);
                    snapshot(card.window(), checks.output.join("restored_choice.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                }
                14 => {
                    let main = main_window(app);
                    assert!(main.get_question_area_height() >= 174.0);
                    snapshot(main.window(), checks.output.join("matrix_narrow.png"));
                    main.set_current_tab(1);
                }
                15 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("settings.png"));
                    main.invoke_toggle_knowledge_mode();
                    app.show_practice().unwrap();
                    assert!(practice_window(app).get_knowledge_only());
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        checks.finished.set(checks.app.practice.borrow().is_none());
                        if checks.finished.get() {
                            println!(
                                "Native one-click matrix, batch atomicity, keyboard controls, editing, metadata/export, global knowledge mode, nonactivating frameless facts, automatic close and quiz restoration passed."
                            );
                        }
                        checks.app.quit();
                    });
                    return;
                }
                _ => unreachable!(),
            }
            management_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens the native management window and exercises selection and question editing"]
fn native_question_management() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/question_management_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.feedback_seconds = 2;
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let window_hook = floating.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .with_winit_window_attributes_hook(move |attributes| window_hook.attributes(attributes))
        .select()
        .unwrap();
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    management_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Question management check failed or timed out"
    );
}

fn question_outline_pixels(main: &MainWindow) -> usize {
    let pixels = main.window().take_snapshot().unwrap();
    let scale = main.window().scale_factor();
    let left =
        ((main.get_question_area_left() + main.get_participation_width()) * scale).round() as u32;
    let top = (main.get_question_area_top() * scale).ceil() as u32 + 10;
    let bottom = ((main.get_question_area_top() + main.get_question_area_height()) * scale).floor()
        as u32
        - 10;
    (top..bottom)
        .flat_map(|y| (left..left + 3).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let pixel = pixels.as_slice()[(y * pixels.width() + x) as usize];
            pixel.r > 150 && pixel.g > 150 && pixel.b > 150
        })
        .count()
}

fn question_keyboard_pixels(main: &MainWindow) -> usize {
    let pixels = main.window().take_snapshot().unwrap();
    let scale = main.window().scale_factor();
    let left = ((main.get_question_area_left() + main.get_participation_width() + 10.0) * scale)
        .round() as u32;
    let right = left + (18.0 * scale).ceil() as u32;
    (0..3)
        .flat_map(|row| {
            let bottom = main.get_question_area_top()
                + (0..=row).map(|i| matrix_row_height(main, i)).sum::<f32>();
            let top = ((bottom - 12.0) * scale).round() as u32;
            let end = ((bottom + 4.0) * scale).round() as u32;
            (top..end).flat_map(move |y| (left..right).map(move |x| (x, y)))
        })
        .filter(|&(x, y)| {
            let pixel = pixels.as_slice()[(y * pixels.width() + x) as usize];
            pixel.r > 150 && pixel.g > 150 && pixel.b > 150
        })
        .count()
}

fn question_focus_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(250), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let main = main_window(&checks.app);
            if phase < 6 {
                if phase.is_multiple_of(2) {
                    let row = usize::from(phase / 2);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_question_area_left() + main.get_participation_width() + 30.0,
                            main.get_question_area_top()
                                + (0..row).map(|i| matrix_row_height(&main, i)).sum::<f32>()
                                + matrix_row_height(&main, row) / 2.0,
                        ),
                    );
                    assert!(main.get_editor_visible(), "The pointer opens this question");
                } else {
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_editor_cancel_center_x(),
                            main.get_editor_cancel_center_y(),
                        ),
                    );
                    assert!(
                        !main.get_editor_visible(),
                        "Pointer cancellation returns to the list"
                    );
                }
                question_focus_phase(checks.clone(), phase + 1);
            } else if phase == 6 {
                main.window().dispatch_event(WindowEvent::PointerMoved {
                    position: slint::LogicalPosition::new(8.0, 8.0),
                });
                snapshot(main.window(), checks.output.join("mouse_returns.png"));
                let outlines = question_outline_pixels(&main);
                println!("Question-outline pixels after three pointer returns: {outlines}");
                assert_eq!(
                    outlines, 0,
                    "Mouse editing and returning must leave no white question frames"
                );
                main.set_question_scroll_y(-30.0);
                question_focus_phase(checks.clone(), 7);
            } else if phase == 7 {
                snapshot(main.window(), checks.output.join("scrolled_returns.png"));
                assert_eq!(
                    question_outline_pixels(&main),
                    0,
                    "Scrolling retains no old mouse frames"
                );
                main.set_question_scroll_y(0.0);
                question_focus_phase(checks.clone(), 8);
            } else {
                let baseline = question_keyboard_pixels(&main);
                let mut reached_question = false;
                for _ in 0..80 {
                    key(main.window(), Key::Tab.into());
                    if question_keyboard_pixels(&main) > baseline + 20 {
                        reached_question = true;
                        break;
                    }
                }
                assert!(
                    reached_question,
                    "Tab still focuses a question with a short underline"
                );
                snapshot(main.window(), checks.output.join("keyboard_focus.png"));
                key(main.window(), Key::Return.into());
                assert!(
                    main.get_editor_visible(),
                    "Enter still opens the keyboard-focused question"
                );
                checks.finished.set(true);
                checks.app.quit();
            }
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens native windows and checks mouse editing does not leave focus frames"]
fn native_question_focus() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/question_focus_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    main_window(&app)
        .window()
        .set_size(slint::LogicalSize::new(820.0, 560.0));
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    question_focus_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(15), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.finished.get(), "Question focus check failed");
}

fn click_card_delete(main: &MainWindow, confirm: bool) {
    pointer_click(
        main.window(),
        slint::LogicalPosition::new(
            if confirm {
                main.get_card_confirm_center_x()
            } else {
                main.get_card_delete_center_x()
            },
            if confirm {
                main.get_card_confirm_center_y()
            } else {
                main.get_card_delete_center_y()
            },
        ),
    );
}

fn question_deletion_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(300), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let main = main_window(app);
            match phase {
                0 => {
                    assert_eq!(main.get_cards().row_count(), 3);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_question_area_left() + main.get_participation_width() + 30.0,
                            main.get_question_area_top() + matrix_row_height(&main, 0) / 2.0,
                        ),
                    );
                    assert!(main.get_editor_can_delete());
                    type_content(app, &main, 0, "7/3");
                    std::fs::write(
                        checks.output.join("draft.txt"),
                        main.get_editor_prompt().as_bytes(),
                    )
                    .unwrap();
                }
                1 => {
                    click_card_delete(&main, false);
                    assert!(main.get_card_delete_pending());
                    main.invoke_save_editor();
                    main.invoke_set_editor_answer_mode(2);
                    assert_eq!(main.get_editor_answer_mode(), 1);
                    assert_eq!(app.store.load().unwrap().packs[0].cards.len(), 3);
                }
                2 => {
                    snapshot(main.window(), checks.output.join("confirmation_zh.png"));
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_card_cancel_center_x(),
                            main.get_card_cancel_center_y(),
                        ),
                    );
                    assert!(!main.get_card_delete_pending());
                    assert!(main.get_editor_visible());
                    assert_eq!(
                        main.get_editor_prompt(),
                        std::fs::read_to_string(checks.output.join("draft.txt")).unwrap()
                    );
                    click_card_delete(&main, false);
                }
                3 => {
                    key(main.window(), Key::Return.into());
                    assert!(
                        !main.get_card_delete_pending(),
                        "Default Enter cancels deletion"
                    );
                    click_card_delete(&main, false);
                }
                4 => {
                    key(main.window(), Key::Escape.into());
                    assert!(!main.get_card_delete_pending());
                    click_card_delete(&main, false);
                }
                5 => {
                    use std::os::windows::fs::OpenOptionsExt;
                    let before = app.library.borrow().clone();
                    let locked = std::fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(app.store.directory().join("state.json"))
                        .unwrap();
                    click_card_delete(&main, true);
                    assert_eq!(*app.library.borrow(), before);
                    assert!(main.get_card_delete_pending());
                    assert!(main.get_message_is_error());
                    snapshot(main.window(), checks.output.join("save_failure.png"));
                    drop(locked);
                }
                6 => {
                    click_card_delete(&main, true);
                    assert!(!main.get_editor_visible());
                    assert_eq!(main.get_cards().row_count(), 2);
                    assert!(app.previous_question.borrow().is_none());
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[1].cards.len(), 3);
                    assert_eq!(
                        saved.settings.card_answer_modes[&saved.packs[0].id].len(),
                        1
                    );
                    main.invoke_choose_language(2);
                    main.invoke_edit_card(0);
                }
                7 => click_card_delete(&main, false),
                8 => main
                    .window()
                    .set_size(slint::LogicalSize::new(720.0, 480.0)),
                9 => {
                    snapshot(
                        main.window(),
                        checks.output.join("confirmation_en_minimum.png"),
                    );
                    click_card_delete(&main, true);
                    assert_eq!(main.get_cards().row_count(), 1);
                    main.invoke_edit_card(0);
                }
                10 => click_card_delete(&main, false),
                11 => {
                    click_card_delete(&main, true);
                    assert!(main.get_has_selection());
                    assert_eq!(main.get_cards().row_count(), 0);
                    assert!(!main.get_selected_enabled());
                    assert!(!main.get_packs().row_data(0).unwrap().has_cards);
                    let saved = app.store.load().unwrap();
                    assert!(saved.packs[0].cards.is_empty());
                    assert_eq!(saved.packs[1].cards.len(), 3);
                    assert!(saved.settings.card_answer_modes.is_empty());
                    let exported = app.store.directory().join("empty_export.json");
                    Store::export_pack(&exported, &saved.packs[0]).unwrap();
                    assert_eq!(Store::read_pack(&exported).unwrap(), saved.packs[0]);
                    assert_eq!(
                        Store::read_pack(&app.store.directory().join("original.json"))
                            .unwrap()
                            .cards
                            .len(),
                        3
                    );
                    app.close_main();
                    app.show_main().unwrap();
                }
                12 => {
                    assert_eq!(main.get_cards().row_count(), 0);
                    snapshot(main.window(), checks.output.join("empty_pack.png"));
                    main.invoke_add_card();
                    assert!(!main.get_editor_can_delete());
                    main.invoke_delete_card();
                    assert!(!main.get_card_delete_pending());
                    type_content(app, &main, 0, "重新添加的问题");
                    type_content(app, &main, 1, "答案");
                    main.invoke_save_editor();
                    assert_eq!(main.get_cards().row_count(), 1);
                    assert_eq!(app.store.load().unwrap().packs[0].cards.len(), 1);
                    main.invoke_edit_card(0);
                    click_card_delete(&main, false);
                    app.close_main();
                    assert!(app.pending_card_delete.borrow().is_none());
                    app.show_main().unwrap();
                    assert!(!main_window(app).get_card_delete_pending());
                    checks.finished.set(true);
                    println!(
                        "Native question deletion, draft preservation, Cancel/Enter/Esc, failed-save retry, empty-pack export/reopen/add and original-file preservation passed."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            question_deletion_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens isolated native windows to verify question deletion and empty packs"]
fn native_question_deletion() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/question_deletion_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.packs[0].cards.truncate(3);
    let original = library.packs[0].clone();
    let first = original.cards[0].id.clone().unwrap();
    library
        .set_card_answer_mode(&original.id, &first, AnswerMode::MultipleChoice)
        .unwrap();
    library
        .set_card_answer_mode(
            &original.id,
            original.cards[1].id.as_deref().unwrap(),
            AnswerMode::TrueFalse,
        )
        .unwrap();
    let mut retained = original.clone();
    retained.id = "retained_questions".into();
    library.import(retained, false).unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    Store::export_pack(&profile.path().join("original.json"), &original).unwrap();
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    app.previous_question.replace(Some(QuestionKey {
        pack_id: original.id,
        card_id: first,
    }));
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    question_deletion_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.finished.get(), "Question deletion check failed");
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

fn click_pack(main: &MainWindow, row: usize, toggle: bool) {
    pointer_click(
        main.window(),
        slint::LogicalPosition::new(
            main.get_pack_toggle_center_x() - if toggle { 0.0 } else { 70.0 },
            main.get_pack_list_top()
                + row as f32 * main.get_pack_row_stride()
                + main.get_pack_row_height() / 2.0,
        ),
    );
}

fn click_delete_pack(main: &MainWindow, confirm: bool) {
    let (x, y) = if confirm {
        (
            main.get_delete_confirm_center_x(),
            main.get_delete_confirm_center_y(),
        )
    } else {
        (
            main.get_delete_button_center_x(),
            main.get_delete_button_center_y(),
        )
    };
    pointer_click(main.window(), slint::LogicalPosition::new(x, y));
}

fn deletion_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(300), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let main = main_window(app);
            match phase {
                0 => {
                    click_pack(&main, 1, false);
                    main.invoke_edit_card(0);
                    click_delete_pack(&main, false);
                    main.invoke_delete_pack();
                    assert!(main.get_editor_visible());
                    assert!(!main.get_delete_pending());
                    main.invoke_cancel_editor();
                    click_delete_pack(&main, false);
                    assert!(main.get_delete_pending());
                    assert!(!main.get_tray_settings_available());
                }
                1 => {
                    snapshot(main.window(), checks.output.join("confirmation_zh.png"));
                    let before = app.store.load().unwrap();
                    click_pack(&main, 2, false);
                    click_pack(&main, 0, true);
                    click_matrix(&main, Some(0), 1);
                    assert_eq!(main.get_selected_index(), 1);
                    assert_eq!(app.store.load().unwrap(), before);
                    // The initial keyboard focus is Cancel, so Enter is safe.
                    key(main.window(), Key::Return.into());
                    assert!(!main.get_delete_pending());
                    assert_eq!(app.store.load().unwrap(), before);
                }
                2 => click_delete_pack(&main, false),
                3 => {
                    key(main.window(), Key::Escape.into());
                    assert!(!main.get_delete_pending());
                    assert_eq!(main.get_packs().row_count(), 3);
                    click_delete_pack(&main, false);
                    main.invoke_choose_language(2);
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                }
                4 => {
                    use std::os::windows::fs::OpenOptionsExt;
                    snapshot(
                        main.window(),
                        checks.output.join("confirmation_en_minimum.png"),
                    );
                    assert!(main.get_delete_description().contains("cannot be undone"));
                    let size = main
                        .window()
                        .size()
                        .to_logical(main.window().scale_factor());
                    assert!(main.get_delete_confirm_center_x() < size.width - 24.0);
                    assert!(main.get_delete_confirm_center_y() < size.height - 60.0);
                    let before = app.store.load().unwrap();
                    let state = app.store.directory().join("state.json");
                    let bytes = std::fs::read(&state).unwrap();
                    // Allow reading but deny replacement of this isolated state file.
                    let locked = std::fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(&state)
                        .unwrap();
                    click_delete_pack(&main, true);
                    assert!(main.get_message_is_error());
                    assert!(main.get_delete_pending());
                    assert_eq!(main.get_selected_index(), 1);
                    assert_eq!(*app.library.borrow(), before);
                    assert_eq!(std::fs::read(&state).unwrap(), bytes);
                    snapshot(main.window(), checks.output.join("save_failure.png"));
                    drop(locked);
                    app.previous_question.replace(Some(QuestionKey {
                        pack_id: "delete_me".into(),
                        card_id: before.packs[1].cards[0].id.clone().unwrap(),
                    }));
                    click_delete_pack(&main, true);
                    assert!(!main.get_delete_pending());
                    assert!(!main.get_message_is_error());
                    assert!(app.previous_question.borrow().is_none());
                    let saved = app.store.load().unwrap();
                    assert_eq!(
                        saved.packs,
                        [before.packs[0].clone(), before.packs[2].clone()]
                    );
                    assert_eq!(main.get_selected_index(), 1);
                    assert_eq!(main.get_selected_name(), saved.packs[1].name);
                    assert!(app.store.directory().join("original_import.json").is_file());
                }
                5 => {
                    snapshot(main.window(), checks.output.join("remaining_packs.png"));
                    click_delete_pack(&main, false);
                    app.close_main();
                    assert!(app.pending_delete.borrow().is_none());
                    app.show_main().unwrap();
                    assert!(!main_window(app).get_delete_pending());
                    assert_eq!(app.store.load().unwrap().packs.len(), 2);
                }
                6 => click_delete_pack(&main, false),
                7 => {
                    click_delete_pack(&main, true);
                    assert_eq!(main.get_packs().row_count(), 1);
                    assert_eq!(main.get_selected_index(), 0);
                    assert_eq!(main.get_selected_name(), "回归测试");
                    assert!(main.get_cards().row_count() > 0);
                    click_delete_pack(&main, false);
                }
                8 => {
                    click_delete_pack(&main, true);
                    assert!(!main.get_has_selection());
                    assert_eq!(main.get_packs().row_count(), 0);
                    assert_eq!(main.get_cards().row_count(), 0);
                    assert!(main.get_selected_name().is_empty());
                    assert!(main.get_selected_description().is_empty());
                    assert!(!app.tray.get_can_practice());
                    assert!(app.store.load().unwrap().packs.is_empty());
                    app.close_main();
                    app.show_main().unwrap();
                }
                9 => {
                    assert!(!main.get_has_selection());
                    snapshot(main.window(), checks.output.join("empty_library.png"));
                    main.invoke_delete_pack();
                    assert!(!main.get_delete_pending());
                    main.invoke_create_pack();
                    assert!(main.get_editor_visible());
                    assert!(main.get_editor_new_pack());
                    main.invoke_cancel_editor();
                    let source = app.store.directory().join("original_import.json");
                    app.commit(|library| library.import(Store::read_pack(&source)?, false))
                        .unwrap();
                    assert!(main.get_has_selection());
                    assert!(main.get_cards().row_count() > 0);
                    assert!(app.tray.get_can_practice());
                    assert_eq!(app.store.load().unwrap().packs.len(), 1);
                    checks.finished.set(true);
                    std::fs::write(checks.output.join("result.txt"),
                        "Native deletion, Cancel/Enter/Esc, draft and selection guards, failed-save recovery, adjacent selection, empty-state reopening, new/import recovery and original-file preservation passed.").unwrap();
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            deletion_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens isolated native windows to verify pack deletion and persistence"]
fn native_pack_deletion() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/pack_deletion_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    for (id, name) in [
        (
            "delete_me",
            "待删除的知识包 · A saved pack with questions and local preferences",
        ),
        ("retained_pack", "保留的知识包"),
    ] {
        let mut pack = library.packs[0].clone();
        pack.id = id.into();
        pack.name = name.into();
        library.import(pack, false).unwrap();
    }
    Store::export_pack(
        &profile.path().join("original_import.json"),
        &library.packs[1],
    )
    .unwrap();
    library
        .set_all_answer_modes("delete_me", AnswerMode::TrueFalse)
        .unwrap();
    let card_id = library.packs[1].cards[0].id.clone().unwrap();
    library
        .set_card_enabled("delete_me", &card_id, false)
        .unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.interface_language = InterfaceLanguage::Chinese;
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let window_hook = floating.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .with_winit_window_attributes_hook(move |attributes| window_hook.attributes(attributes))
        .select()
        .unwrap();
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    deletion_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(20), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Pack deletion check failed or timed out"
    );
}

fn multi_pack_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(300), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    assert_eq!(main.get_packs().row_count(), 2);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 21);
                    main.set_current_tab(1);
                    click_pack(&main, 1, true);
                    assert_eq!(main.get_current_tab(), 1);
                    assert_eq!(main.get_selected_index(), 0);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 19);
                }
                1 => {
                    let main = main_window(app);
                    click_pack(&main, 1, true);
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.pack_enabled_card_count(&saved.packs[1]), 2);
                    assert_eq!(main.get_current_tab(), 1);
                    assert_eq!(main.get_selected_index(), 0);
                    click_pack(&main, 1, false);
                    assert_eq!(main.get_current_tab(), 0);
                    assert_eq!(main.get_selected_index(), 1);
                    assert_eq!(main.get_cards().row_count(), 3);
                    for (index, mode) in [0, 1, 2].into_iter().enumerate() {
                        assert_eq!(main.get_cards().row_data(index).unwrap().answer_mode, mode);
                    }
                }
                2 => {
                    let main = main_window(app);
                    main.invoke_edit_card(0);
                    assert!(main.get_editor_visible());
                    click_pack(&main, 0, true);
                    assert!(app.store.load().unwrap().is_enabled("test_pack"));
                    assert!(main.get_editor_visible());
                    main.invoke_cancel_editor();
                }
                3 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("multiple_packs.png"));
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                }
                4 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("minimum_size.png"));
                    click_pack(&main, 0, true);
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.enabled_card_count(), 2);
                    assert!(
                        saved
                            .participating_cards()
                            .all(|(pack, _)| pack.id == "english_basics")
                    );
                    app.show_practice().unwrap();
                }
                5 => {
                    let card = practice_window(app);
                    check_floating_window(card.window());
                    let saved = app.store.load().unwrap();
                    {
                        let practice = app.practice.borrow();
                        let session = practice.as_ref().unwrap();
                        assert_eq!(session.question.key.pack_id, "english_basics");
                        let original = saved.packs[1]
                            .cards
                            .iter()
                            .find(|original| {
                                original.id.as_deref() == Some(&session.question.key.card_id)
                            })
                            .unwrap();
                        assert_eq!(session.question.card, *original);
                        assert_eq!(
                            card.get_answer_mode(),
                            saved
                                .card_answer_mode(
                                    &session.question.key.pack_id,
                                    &session.question.key.card_id,
                                )
                                .index()
                        );
                    }
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_pack("test_pack".into(), true);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 21);
                    checks.finished.set(true);
                    std::fs::write(checks.output.join("result.txt"),
                        "Native sidebar switches, browsing separation, saved ranges, mixed question modes, draft protection, multiple-pack sampling and minimum-size layout passed.").unwrap();
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            multi_pack_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens the native window to check independent knowledge-pack participation"]
fn native_multi_pack_participation() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/multi_pack_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    let english = KnowledgePack::from_json(
        r#"{
        "format":"try","schema_version":1,"id":"english_basics","name":"英语 · 常用词",
        "cards":[
            {"id":"hesitate","prompt":"“犹豫”的英文动词？","answer":"hesitate"},
            {"id":"accurate","prompt":"“准确的”的英文？","answer":"accurate",
             "distractors":[{"text":"approximate"},{"text":"vague"}]},
            {"id":"sensible","prompt":"“明智的”的英文？","answer":"sensible",
             "true_false":{"statement":"sensible 表示“敏感的”","is_true":false,
                           "correction":"sensible 表示“明智的”"}}
        ]
    }"#
        .as_bytes(),
    )
    .unwrap();
    library.import(english, false).unwrap();
    library
        .set_card_enabled("english_basics", "hesitate", false)
        .unwrap();
    library
        .set_card_answer_mode("english_basics", "accurate", AnswerMode::MultipleChoice)
        .unwrap();
    library
        .set_card_answer_mode("english_basics", "sensible", AnswerMode::TrueFalse)
        .unwrap();
    let first = library.packs[0].cards[0].id.clone().unwrap();
    library
        .set_card_enabled("test_pack", &first, false)
        .unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let window_hook = floating.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .with_winit_window_attributes_hook(move |attributes| window_hook.attributes(attributes))
        .select()
        .unwrap();
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    multi_pack_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(10), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Multi-pack check failed or timed out"
    );
}

fn click_formula_button(card: &PracticeWindow) {
    pointer_click(
        card.window(),
        slint::LogicalPosition::new(
            card.get_formula_button_center_x(),
            card.get_formula_button_center_y(),
        ),
    );
}

fn click_formula_key(card: &PracticeWindow, index: usize) {
    let canvas_height = (card.get_input_preview().formula_height + 16.0).clamp(54.0, 96.0);
    let key_width = (card.get_answer_footer_width() - 16.0 - 20.0) / 6.0;
    pointer_click(
        card.window(),
        slint::LogicalPosition::new(
            card.get_answer_footer_left()
                + 8.0
                + (index % 6) as f32 * (key_width + 4.0)
                + key_width / 2.0,
            card.get_answer_footer_top()
                + 13.0
                + 8.0
                + canvas_height
                + 8.0
                + (index / 6) as f32 * 36.0
                + 16.0,
        ),
    );
}

fn click_answer_structure_tool(card: &PracticeWindow, index: usize) {
    let canvas_height = (card.get_input_preview().formula_height + 16.0).clamp(54.0, 96.0);
    let x = match index {
        0 | 1 => 8.0 + 30.0 + index as f32 * 64.0,
        2 => card.get_answer_footer_width() - 60.0,
        3 => card.get_answer_footer_width() - 24.0,
        4 => card.get_answer_footer_width() - 96.0,
        _ => unreachable!(),
    };
    pointer_click(
        card.window(),
        slint::LogicalPosition::new(
            card.get_answer_footer_left() + x,
            card.get_answer_footer_top() + canvas_height + 233.0,
        ),
    );
}

fn click_answer_slot(app: &App, card: &PracticeWindow, path: &[usize]) {
    let index = app
        .practice
        .borrow()
        .as_ref()
        .unwrap()
        .entry
        .as_ref()
        .unwrap()
        .paths
        .iter()
        .position(|candidate| candidate == path)
        .unwrap();
    let region = card.get_entry_regions().row_data(index).unwrap();
    let preview = card.get_input_preview();
    let canvas_height = (preview.formula_height + 16.0).clamp(54.0, 96.0);
    let scale =
        (card.get_answer_footer_width() - 32.0).min(preview.formula_width) / preview.formula_width;
    pointer_click(
        card.window(),
        slint::LogicalPosition::new(
            card.get_answer_footer_left()
                + 16.0
                + (card.get_answer_footer_width() - 32.0 - preview.formula_width * scale) / 2.0
                + (region.x + region.width / 2.0) * scale,
            card.get_answer_footer_top()
                + 29.0
                + (canvas_height - 16.0 - preview.formula_height * scale) / 2.0
                + (region.y + region.height / 2.0) * scale,
        ),
    );
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

fn formula_editing_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(400), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    main.invoke_add_card();
                    main.invoke_activate_content(0);
                    main.invoke_math_tool("math".into());
                    type_content(app, &main, 0, "12+34");
                }
                1 => {
                    let main = main_window(app);
                    key(main.window(), Key::Home.into());
                    key(main.window(), Key::RightArrow.into());
                    key(main.window(), Key::RightArrow.into());
                    key(main.window(), "0".into());
                    assert_eq!(main.get_math_input(), "120+34");
                    modified_key(main.window(), Key::Control, "z".into());
                    assert_eq!(main.get_math_input(), "12+34");
                }
                2 => {
                    let main = main_window(app);
                    assert_eq!(
                        app.math_fields.borrow()[0].text_selection(),
                        TextSelection {
                            anchor: 2,
                            focus: 2
                        }
                    );
                    key(main.window(), Key::End.into());
                    main.window().dispatch_event(WindowEvent::KeyPressed {
                        text: Key::Shift.into(),
                    });
                    key(main.window(), Key::LeftArrow.into());
                    key(main.window(), Key::LeftArrow.into());
                    main.window().dispatch_event(WindowEvent::KeyReleased {
                        text: Key::Shift.into(),
                    });
                    let range = app.math_fields.borrow()[0].text_selection();
                    assert_eq!(
                        (range.anchor.min(range.focus), range.anchor.max(range.focus)),
                        (3, 5)
                    );
                    main.invoke_math_tool("root".into());
                    assert_eq!(main.get_math_preview().label, "12+sqrt(34)");
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                    main.set_editor_scroll_y(0.0);
                }
                3 => {
                    let main = main_window(app);
                    snapshot(
                        main.window(),
                        checks.output.join("range_template_compact.png"),
                    );
                    main.invoke_cancel_editor();
                    app.show_practice().unwrap();
                }
                4 => {
                    let card = practice_window(app);
                    key(card.window(), "12+34".into());
                    click_formula_button(&card);
                    assert!(card.get_entry_open());
                }
                5 => {
                    let card = practice_window(app);
                    key(card.window(), Key::Home.into());
                    key(card.window(), Key::RightArrow.into());
                    key(card.window(), Key::RightArrow.into());
                    click_formula_key(&card, 22); // mouse 0 at the real caret
                    assert_eq!(card.get_entry_input(), "120+34");
                    modified_key(card.window(), Key::Control, "z".into());
                    assert_eq!(card.get_entry_input(), "12+34");
                }
                6 => {
                    let card = practice_window(app);
                    key(card.window(), Key::End.into());
                    card.window().dispatch_event(WindowEvent::KeyPressed {
                        text: Key::Shift.into(),
                    });
                    key(card.window(), Key::LeftArrow.into());
                    key(card.window(), Key::LeftArrow.into());
                    card.window().dispatch_event(WindowEvent::KeyReleased {
                        text: Key::Shift.into(),
                    });
                    click_formula_key(&card, 13); // replace the selected 34 with 5
                    assert_eq!(card.get_entry_input(), "12+5");
                    modified_key(card.window(), Key::Control, "z".into());
                    assert_eq!(card.get_entry_input(), "12+34");
                }
                7 => {
                    let card = practice_window(app);
                    let caret = app
                        .practice
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .entry
                        .as_ref()
                        .unwrap()
                        .field
                        .text_selection();
                    assert_eq!(caret.anchor.min(caret.focus), 3);
                    assert_eq!(caret.anchor.max(caret.focus), 5);
                    modified_key(card.window(), Key::Control, "a".into());
                    key(card.window(), "sqrt(".into());
                    assert!(!card.get_entry_error().is_empty());
                    click_answer_structure_tool(&card, 4);
                    assert_eq!(card.get_entry_input(), "");
                    click_answer_structure_tool(&card, 2);
                    assert_eq!(card.get_entry_input(), "sqrt(");
                    assert!(!card.get_entry_error().is_empty());
                    click_formula_key(&card, 23);
                    assert_eq!(card.get_feedback(), 0);
                    click_answer_structure_tool(&card, 4);
                    click_formula_key(&card, 1); // an empty root
                    key(card.window(), Key::RightArrow.into()); // leave it
                    assert_eq!(card.get_entry_input(), "sqrt(□)");
                }
                8 => {
                    let card = practice_window(app);
                    key(card.window(), Key::LeftArrow.into()); // return to its empty slot
                    assert_eq!(card.get_entry_input(), "");
                    click_formula_key(&card, 20);
                    key(card.window(), Key::RightArrow.into());
                    assert_eq!(card.get_entry_input(), "sqrt(3)");
                    key(card.window(), Key::Backspace.into()); // enter; preserve the root
                    assert_eq!(card.get_entry_input(), "3");
                    assert_eq!(card.get_input_preview().label, "sqrt(3)");
                    click_formula_key(&card, 0);
                    click_formula_key(&card, 19);
                    key(card.window(), Key::RightArrow.into());
                    assert_eq!(card.get_entry_input(), "3/2");
                    key(card.window(), Key::RightArrow.into());
                    assert_eq!(card.get_entry_input(), "sqrt(3/2)");
                    key(card.window(), Key::LeftArrow.into());
                    assert_eq!(card.get_entry_input(), "2");
                    key(card.window(), Key::UpArrow.into());
                    assert_eq!(card.get_entry_input(), "3");
                    key(card.window(), Key::DownArrow.into());
                    assert_eq!(card.get_entry_input(), "2");
                    key(card.window(), Key::UpArrow.into());
                    key(card.window(), "1234".into());
                    key(card.window(), Key::Home.into());
                    key(card.window(), Key::RightArrow.into());
                    key(card.window(), Key::RightArrow.into());
                    key(card.window(), "0".into());
                    key(card.window(), Key::RightArrow.into());
                    key(card.window(), Key::RightArrow.into());
                    assert_eq!(
                        card.get_entry_input(),
                        "12034",
                        "Navigation uses the new byte length before the preview timer fires"
                    );
                    key(card.window(), Key::RightArrow.into());
                    assert_eq!(card.get_entry_input(), "2");
                }
                9 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_input_preview().label, "sqrt(12034/2)");
                    let canvas_height =
                        (card.get_input_preview().formula_height + 16.0).clamp(54.0, 96.0);
                    let size = card
                        .window()
                        .size()
                        .to_logical(card.window().scale_factor());
                    let buttons_bottom = card.get_answer_footer_top() + canvas_height + 249.0;
                    assert!(
                        buttons_bottom <= size.height - 8.0,
                        "The bottom buttons fit after the formula grows: bottom={buttons_bottom}, window={}",
                        size.height
                    );
                    snapshot(
                        card.window(),
                        checks.output.join("caret_and_navigation.png"),
                    );
                    key(card.window(), Key::Escape.into());
                    assert!(!card.get_entry_open());
                    assert_eq!(card.get_answer_text(), "12+34");
                    key(card.window(), Key::Escape.into());
                    assert!(app.practice.borrow().is_none());
                    assert_eq!(app.store.load().unwrap().packs[0].cards.len(), 20);
                    checks.finished.set(true);
                    println!(
                        "Native caret/range synchronization, mouse insertion, shared undo, invalid-draft recovery, empty-root navigation, structural backspace, cancellation and compact layout passed."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            formula_editing_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens real windows to verify formula caret, range and navigation"]
fn native_formula_editing() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/formula_editing_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    formula_editing_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(20), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Formula editing check failed or timed out"
    );
}

fn formula_answers_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(400), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    main.invoke_add_card();
                    main.invoke_set_editor_answer_mode(1);
                    type_content(app, &main, 0, "1+1=?");
                    type_content(app, &main, 1, "3");
                    type_content(app, &main, 2, "2");
                    type_content(app, &main, 3, "4");
                    main.set_editor_correct_choice(1);
                    main.set_math_active_field(-1);
                }
                1 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("choice_editor.png"));
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[0].cards[20].answer, "2");
                    assert_eq!(
                        saved.card_answer_mode(
                            "test_pack",
                            saved.packs[0].cards[20].id.as_deref().unwrap()
                        ),
                        AnswerMode::MultipleChoice
                    );
                    main.invoke_add_card();
                    main.invoke_set_editor_answer_mode(2);
                    type_content(app, &main, 4, "1+1=3");
                    main.invoke_select_truth(false);
                    main.set_math_active_field(-1);
                }
                2 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("judgment_editor.png"));
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    let saved = app.store.load().unwrap();
                    let card = &saved.packs[0].cards[21];
                    assert_eq!(card.prompt, "1+1=3");
                    assert_eq!(card.answer, "✕");
                    assert!(!card.true_false.as_ref().unwrap().is_true);
                    assert_eq!(main.get_cards().row_data(21).unwrap().prompt, "1+1=3");
                    let exported = checks.output.join("simplified_pack.json");
                    Store::export_pack(&exported, &saved.packs[0]).unwrap();
                    assert_eq!(Store::read_pack(&exported).unwrap(), saved.packs[0]);
                    app.show_practice().unwrap();
                }
                3 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_prompt(), "√3/2 = ?");
                    key(card.window(), "sqrt(3)/2".into());
                }
                4 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_answer_text(), "sqrt(3)/2");
                    assert!(card.get_input_preview().formula_width > 0.0);
                    snapshot(card.window(), checks.output.join("typed_formula.png"));
                    key(card.window(), Key::Return.into());
                    assert_eq!(card.get_feedback(), 1);
                    app.close_practice();
                    app.show_practice().unwrap();
                }
                5 => {
                    let card = practice_window(app);
                    click_formula_button(&card);
                    assert!(card.get_entry_open());
                    assert!(card.get_entry_input().is_empty());
                }
                6 => {
                    let card = practice_window(app);
                    check_floating_window(card.window());
                    check_no_caption_paint(card.window());
                    click_formula_key(&card, 1); // root first
                    click_formula_key(&card, 0); // first empty slot: numerator
                }
                7 => {
                    let card = practice_window(app);
                    click_formula_key(&card, 20); // 3
                    assert_eq!(card.get_entry_input(), "3");
                    click_formula_key(&card, 23);
                    assert_eq!(
                        card.get_feedback(),
                        0,
                        "An unfilled denominator stays editable"
                    );
                    assert!(card.get_entry_open());
                    click_answer_slot(app, &card, &[0, 1]);
                    assert_eq!(card.get_entry_input(), "");
                }
                8 => {
                    let card = practice_window(app);
                    click_formula_key(&card, 19); // 2
                    assert_eq!(
                        card.get_input_preview().label,
                        "sqrt(3/2)",
                        "Reproduce the mistaken nesting before repairing it"
                    );
                    assert!(card.get_entry_can_move_out());
                    snapshot(
                        card.window(),
                        checks.output.join("mouse_formula_nested.png"),
                    );
                    click_answer_structure_tool(&card, 0); // move fraction outside root
                    assert_eq!(card.get_input_preview().label, "sqrt(3)/2");
                    assert_eq!(card.get_entry_input(), "2");
                    click_answer_structure_tool(&card, 2); // undo repair
                    assert_eq!(card.get_input_preview().label, "sqrt(3/2)");
                    click_answer_structure_tool(&card, 3); // redo repair
                    assert_eq!(card.get_input_preview().label, "sqrt(3)/2");
                    assert!(
                        !card.get_entry_can_unwrap(),
                        "The denominator is outside the root after moving the fraction out"
                    );
                    click_answer_slot(app, &card, &[0, 0]);
                    assert!(card.get_entry_can_unwrap());
                    click_answer_structure_tool(&card, 1); // remove root, keep both numbers
                    assert_eq!(card.get_input_preview().label, "3/2");
                    click_answer_structure_tool(&card, 2); // undo removal
                    assert_eq!(card.get_input_preview().label, "sqrt(3)/2");
                    snapshot(card.window(), checks.output.join("mouse_formula.png"));
                    click_answer_structure_tool(&card, 4); // C clears the entire field
                    assert_eq!(card.get_entry_input(), "");
                    click_formula_key(&card, 23);
                    assert_eq!(
                        card.get_feedback(),
                        0,
                        "A cleared answer cannot be submitted"
                    );
                    click_answer_structure_tool(&card, 2);
                    assert_eq!(card.get_input_preview().label, "sqrt(3)/2");
                    assert_eq!(card.get_entry_input(), "3");
                    click_formula_key(&card, 23);
                    assert_eq!(card.get_feedback(), 1);
                    assert!(!card.get_entry_open());
                    app.close_practice();
                    app.show_practice().unwrap();
                }
                9 => {
                    let card = practice_window(app);
                    key(card.window(), "sqrt(3)/2".into());
                    click_formula_button(&card);
                    click_formula_key(&card, 0);
                    key(card.window(), Key::Escape.into());
                    assert!(!card.get_entry_open());
                    assert_eq!(card.get_answer_text(), "sqrt(3)/2");
                    assert_eq!(card.get_feedback(), 0);
                    key(card.window(), Key::Escape.into());
                    assert!(app.practice.borrow().is_none());
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(21, true);
                    app.show_practice().unwrap();
                }
                10 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_answer_mode(), 2);
                    assert_eq!(card.get_standard_answer(), "✕");
                    assert_eq!(card.get_options().row_count(), 2);
                    click_option(&card, 0);
                    assert_eq!(card.get_feedback(), 2);
                    snapshot(card.window(), checks.output.join("judgment_symbol.png"));
                    checks.finished.set(true);
                    println!(
                        "Native simplified editors/export, typed formula rendering, mouse-only root-first fraction repair, unwrap/undo/redo, incomplete-slot protection, cancellation, frameless expansion and symbol-only judgment passed."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            formula_answers_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens real editor and practice windows and uses the formula keypad"]
fn native_formula_answers() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/formula_answers_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.feedback_seconds = 2;
    library.set_pack_enabled("test_pack", false).unwrap();
    let card_id = library.packs[0].cards[2].id.clone().unwrap();
    library
        .set_card_enabled("test_pack", &card_id, true)
        .unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    formula_answers_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(25), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Formula answers check failed or timed out"
    );
}

#[test]
#[ignore = "opens a real practice card and edits formulas inside its answer box"]
fn native_inline_answers() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/inline_answers_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.feedback_seconds = 2;
    library.set_pack_enabled("test_pack", false).unwrap();
    let card_id = library.packs[0].cards[2].id.clone().unwrap();
    library
        .set_card_enabled("test_pack", &card_id, true)
        .unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    checks.app.show_practice().unwrap();
    inline_answers_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(25), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Inline answers check failed or timed out"
    );
}

fn click_inline_offset(app: &App, card: &PracticeWindow, offset: usize) {
    let rect = app
        .practice
        .borrow()
        .as_ref()
        .unwrap()
        .inline_input
        .as_ref()
        .unwrap()
        .caret(offset);
    pointer_click(
        card.window(),
        slint::LogicalPosition::new(
            card.get_input_canvas_left() + (rect.x + 0.1) * card.get_input_canvas_scale(),
            card.get_input_canvas_top()
                + (rect.y + rect.height / 2.0) * card.get_input_canvas_scale(),
        ),
    );
}

fn inline_answers_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            if phase == 0 {
                key(practice_window(app).window(), "sqrt(3)/2".into());
                modified_key(practice_window(app).window(), Key::Control, "y".into());
            } else {
                let card = practice_window(app);
                match phase {
                    1 => {
                        assert_eq!(
                            card.get_answer_text(),
                            "sqrt(3)/2",
                            "Native typing reaches the focused answer box"
                        );
                        assert!(
                            card.get_input_math_mode(),
                            "source={:?}, preview={:?}, width={}",
                            card.get_answer_text(),
                            card.get_input_preview().label,
                            card.get_input_preview().formula_width
                        );
                        assert!(!card.get_entry_open());
                        assert!(
                            card.get_input_canvas_top() < card.get_answer_footer_top(),
                            "Formula is inside the input row"
                        );
                        assert_eq!(card.get_answer_position().caret, 9);
                        snapshot(card.window(), checks.output.join("inline_fraction.png"));
                        click_inline_offset(app, &card, 5);
                    }
                    2 => {
                        assert_eq!(card.get_answer_position().caret, 5);
                        key(card.window(), Key::RightArrow.into());
                        assert_eq!(card.get_answer_position().caret, 6);
                        modified_key(card.window(), Key::Shift, Key::LeftArrow.into());
                        assert_eq!(card.get_answer_position().anchor, 6);
                        assert_eq!(card.get_answer_position().caret, 5);
                        assert!(card.get_input_selection().row_count() > 0);
                        key(card.window(), Key::RightArrow.into());
                        key(card.window(), "0".into());
                    }
                    3 => {
                        assert_eq!(card.get_answer_text(), "sqrt(30)/2");
                        assert!(card.get_input_math_mode());
                        click_inline_offset(app, &card, 9);
                        key(card.window(), "3".into());
                    }
                    4 => {
                        assert_eq!(card.get_answer_text(), "sqrt(30)/32");
                        assert!(card.get_input_math_mode());
                        let (start, end) = {
                            let practice = app.practice.borrow();
                            let layout = practice.as_ref().unwrap().inline_input.as_ref().unwrap();
                            (layout.caret(5), layout.caret(11))
                        };
                        let point = |rect: InputRect| {
                            slint::LogicalPosition::new(
                                card.get_input_canvas_left()
                                    + (rect.x + 0.1) * card.get_input_canvas_scale(),
                                card.get_input_canvas_top()
                                    + (rect.y + rect.height / 2.0) * card.get_input_canvas_scale(),
                            )
                        };
                        card.window().dispatch_event(WindowEvent::PointerPressed {
                            position: point(start),
                            button: PointerEventButton::Left,
                        });
                        card.window().dispatch_event(WindowEvent::PointerMoved {
                            position: point(end),
                        });
                        card.window().dispatch_event(WindowEvent::PointerReleased {
                            position: point(end),
                            button: PointerEventButton::Left,
                        });
                        assert_eq!(card.get_answer_position().anchor, 5);
                        assert_eq!(card.get_answer_position().caret, 11);
                        assert!(card.get_input_selection().row_count() >= 2);
                        modified_key(card.window(), Key::Control, "a".into());
                        assert_eq!(card.get_answer_position().anchor, 0);
                        assert_eq!(card.get_answer_position().caret, 11);
                        assert_eq!(card.get_input_selection().row_count(), 1);
                        snapshot(card.window(), checks.output.join("inline_selection.png"));
                        key(card.window(), r"\frac{\sqrt{3}}{2}".into());
                    }
                    5 => {
                        assert_eq!(card.get_answer_text(), r"\frac{\sqrt{3}}{2}");
                        assert!(card.get_input_math_mode());
                        snapshot(card.window(), checks.output.join("inline_latex.png"));
                        modified_key(card.window(), Key::Control, "a".into());
                        key(card.window(), "Paris".into());
                        assert!(
                            !card.get_input_math_mode(),
                            "Old formula must disappear immediately after an edit"
                        );
                    }
                    6 => {
                        assert_eq!(card.get_answer_text(), "Paris");
                        assert!(!card.get_input_math_mode());
                        snapshot(card.window(), checks.output.join("plain_text.png"));
                        modified_key(card.window(), Key::Control, "a".into());
                        key(card.window(), "sqrt(".into());
                    }
                    7 => {
                        assert_eq!(card.get_answer_text(), "sqrt(");
                        assert!(!card.get_input_math_mode());
                        assert_eq!(card.get_feedback(), 0);
                        key(card.window(), Key::Return.into());
                        assert_eq!(
                            card.get_feedback(),
                            0,
                            "An unfinished formula is not graded"
                        );
                        assert!(!card.get_entry_error().is_empty());
                        modified_key(card.window(), Key::Control, "z".into());
                    }
                    8 => {
                        assert_eq!(
                            card.get_answer_text(),
                            "Paris",
                            "Undo retains the previous input draft"
                        );
                        modified_key(card.window(), Key::Control, "a".into());
                        key(card.window(), "sqrt(3)/2".into());
                    }
                    9 => {
                        assert!(card.get_input_math_mode());
                        key(card.window(), Key::Return.into());
                        assert_eq!(card.get_feedback(), 1);
                        assert!(
                            card.get_input_math_mode(),
                            "Feedback retains inline typesetting"
                        );
                        snapshot(card.window(), checks.output.join("inline_correct.png"));
                        app.close_practice();
                        app.show_practice().unwrap();
                        click_formula_button(&practice_window(app));
                    }
                    10 => {
                        assert!(card.get_entry_open());
                        key(card.window(), "sqrt(3)/2".into());
                    }
                    11 => {
                        assert_eq!(card.get_entry_input(), "sqrt(3)/2");
                        click_formula_button(&card);
                    }
                    12 => {
                        assert!(!card.get_entry_open());
                        assert!(card.get_input_math_mode());
                        assert_eq!(card.get_answer_text(), "sqrt(3)/2");
                        assert_eq!(card.get_answer_position().caret, 9);
                        snapshot(card.window(), checks.output.join("inline_from_keypad.png"));
                        app.close_practice();
                        app.show_practice().unwrap();
                    }
                    13 => {
                        key(card.window(), "sqrt(3)/2".into());
                        assert!(!card.get_input_math_mode());
                        key(card.window(), Key::Return.into());
                        assert_eq!(card.get_feedback(), 1);
                        assert!(
                            card.get_input_math_mode(),
                            "Immediate submission formats current input"
                        );
                        snapshot(card.window(), checks.output.join("inline_fast_correct.png"));
                        checks.finished.set(true);
                        app.quit();
                        return;
                    }
                    _ => unreachable!(),
                }
            }
            inline_answers_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

fn type_content(app: &App, main: &MainWindow, field: i32, input: &str) {
    main.invoke_activate_content(field);
    main.set_math_input(input.into());
    main.invoke_edit_math_input(input.into());
    assert!(app.flush_math(), "{}", main.get_math_error());
    app.refresh_math(false);
}

fn math_toolbar(main: &MainWindow) -> slint::LogicalPosition {
    let preview = main.get_math_preview();
    let height = (preview.formula_height + 20.0).max(54.0);
    // Each visible tool row is 32 px with a 10 px gap. The picture
    // reserves 10 px horizontally and vertically centres the actual formula.
    slint::LogicalPosition::new(
        main.get_math_canvas_left() - 10.0,
        main.get_math_canvas_top()
            - (height - preview.formula_height * main.get_math_canvas_scale()) / 2.0
            - 42.0
                * (1 + i32::from(main.get_math_has_formula())
                    + i32::from(!main.get_math_text_mode())) as f32,
    )
}

fn click_math_slot(app: &App, main: &MainWindow, path: &[usize]) {
    let index = app
        .math_paths
        .borrow()
        .iter()
        .position(|p| p == path)
        .unwrap();
    let region = main.get_math_regions().row_data(index).unwrap();
    let scale = main.get_math_canvas_scale();
    pointer_click(
        main.window(),
        slint::LogicalPosition::new(
            main.get_math_canvas_left() + (region.x + region.width / 2.0) * scale,
            main.get_math_canvas_top() + (region.y + region.height / 2.0) * scale,
        ),
    );
}

fn click_math_structure_tool(main: &MainWindow, index: usize) {
    let toolbar = math_toolbar(main);
    let canvas_height = (main.get_math_preview().formula_height + 20.0).max(54.0);
    pointer_click(
        main.window(),
        slint::LogicalPosition::new(
            toolbar.x + 30.0 + index as f32 * 64.0,
            toolbar.y + 126.0 + canvas_height + 26.0,
        ),
    );
}

fn authoring_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let main = main_window(app);
            match phase {
                0 => {
                    use windows_sys::Win32::UI::WindowsAndMessaging::{
                        GetWindowTextW, ICON_BIG, ICON_SMALL, WM_GETICON,
                    };
                    let hwnd = native_handle(main.window());
                    let mut title = [0u16; 64];
                    // SAFETY: the handles come from the live Winit window and
                    // the writable title buffer covers its declared length.
                    unsafe {
                        let len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
                        assert_eq!(String::from_utf16_lossy(&title[..len as usize]), "Try");
                        assert_ne!(
                            SendMessageW(hwnd, WM_GETICON, ICON_SMALL as usize, 0),
                            0,
                            "Small titlebar icon is assigned"
                        );
                        assert_ne!(
                            SendMessageW(hwnd, WM_GETICON, ICON_BIG as usize, 0),
                            0,
                            "Large window icon is assigned"
                        );
                    }
                    snapshot(main.window(), checks.output.join("knowledge.png"));
                    assert!(
                        (0..main.get_cards().row_count())
                            .filter(|i| main.get_cards().row_data(*i).unwrap().rendered)
                            .count()
                            < 12,
                        "Only visible list formulas should be rasterized"
                    );
                    let selected = main.get_selected_index();
                    main.set_current_tab(1);
                    snapshot(main.window(), checks.output.join("settings.png"));
                    assert_eq!(main.get_selected_index(), selected);
                    main.invoke_select_pack(0);
                    assert_eq!(main.get_current_tab(), 0);
                    main.invoke_edit_card(0);
                    main.invoke_activate_content(0);
                }
                1 => {
                    snapshot(main.window(), checks.output.join("formula_editor.png"));
                    let paths = app.math_paths.borrow();
                    let fields = app.math_fields.borrow();
                    let index = paths
                        .iter()
                        .position(|path| {
                            fields[0]
                                .root
                                .at(path)
                                .is_some_and(|node| node.value == "6")
                        })
                        .unwrap();
                    let region = main.get_math_regions().row_data(index).unwrap();
                    let scale = main.get_math_canvas_scale();
                    let position = slint::LogicalPosition::new(
                        main.get_math_canvas_left() + (region.x + region.width / 2.0) * scale,
                        main.get_math_canvas_top() + (region.y + region.height / 2.0) * scale,
                    );
                    drop(fields);
                    drop(paths);
                    main.window()
                        .dispatch_event(WindowEvent::PointerMoved { position });
                    main.window().dispatch_event(WindowEvent::PointerPressed {
                        position,
                        button: PointerEventButton::Left,
                    });
                    main.window().dispatch_event(WindowEvent::PointerReleased {
                        position,
                        button: PointerEventButton::Left,
                    });
                    assert_eq!(
                        main.get_math_input(),
                        "6",
                        "Clicking the rendered denominator selects it"
                    );
                    main.set_math_input("3".into());
                    main.invoke_edit_math_input("3".into());
                }
                2 => {
                    assert!(main.get_math_error().is_empty());
                    let fields = app.math_fields.borrow();
                    assert_eq!(fields[0].selected().value, "3");
                    drop(fields);
                    main.invoke_undo_math(false);
                    assert_eq!(main.get_math_input(), "6");
                    main.invoke_undo_math(true);
                    assert_eq!(main.get_math_input(), "3");
                    let before = app.store.load().unwrap();
                    main.invoke_math_tool("clear".into());
                    assert_eq!(main.get_math_input(), "");
                    main.invoke_save_editor();
                    assert!(
                        main.get_editor_visible(),
                        "A cleared question stays in the draft"
                    );
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_undo_math(false);
                    assert_eq!(
                        main.get_math_input(),
                        "3",
                        "Undo also restores the selected denominator"
                    );
                    main.set_math_input("sqrt(".into());
                    main.invoke_edit_math_input("sqrt(".into());
                    main.invoke_math_tool("clear".into());
                    assert_eq!(main.get_math_input(), "");
                    assert!(
                        main.get_math_error().is_empty(),
                        "C also clears malformed input"
                    );
                    main.invoke_undo_math(false);
                    assert_eq!(main.get_math_input(), "sqrt(");
                    assert!(!main.get_math_error().is_empty());
                    main.invoke_save_editor();
                    assert!(main.get_editor_visible());
                    main.invoke_undo_math(false);
                    assert_eq!(main.get_math_input(), "3");
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[0].cards[0].id, before.packs[0].cards[0].id);
                    assert_eq!(
                        saved.packs[0].cards[0].answer,
                        before.packs[0].cards[0].answer
                    );
                    assert_eq!(saved.settings, before.settings);
                    main.invoke_add_card();
                    type_content(app, &main, 0, "计算 sin(pi/3) 的值");
                    main.invoke_activate_content(1);
                    main.invoke_math_tool("math".into());
                    main.invoke_math_tool("root".into());
                    main.invoke_math_tool("fraction".into());
                    app.modify_math(|field| {
                        field.select(vec![0, 1]);
                        Ok(())
                    });
                    main.set_math_input("2".into());
                    main.invoke_edit_math_input("2".into());
                    assert!(app.flush_math());
                    app.modify_math(|field| {
                        field.select(vec![0, 0]);
                        Ok(())
                    });
                    main.set_math_input("3".into());
                    main.invoke_edit_math_input("3".into());
                    assert!(app.flush_math());
                    app.refresh_math(false);
                    // Reveal the structure row below the answer preview in the desktop layout.
                    main.set_editor_scroll_y(-140.0);
                }
                3 => {
                    assert_eq!(main.get_math_preview().label, "sqrt(3/2)");
                    assert!(main.get_math_can_move_out());
                    snapshot(
                        main.window(),
                        checks.output.join("nested_formula_editor.png"),
                    );
                    click_math_structure_tool(&main, 0);
                    assert_eq!(main.get_math_preview().label, "sqrt(3)/2");
                    assert_eq!(main.get_math_input(), "3");
                    main.invoke_undo_math(false);
                    assert_eq!(main.get_math_preview().label, "sqrt(3/2)");
                    main.invoke_undo_math(true);
                    click_math_structure_tool(&main, 1);
                    assert_eq!(main.get_math_preview().label, "3/2");
                    main.invoke_undo_math(false);
                    assert_eq!(main.get_math_preview().label, "sqrt(3)/2");
                    snapshot(
                        main.window(),
                        checks.output.join("repaired_formula_editor.png"),
                    );
                    main.invoke_activate_content(0);
                    snapshot(main.window(), checks.output.join("mixed_editor.png"));
                    assert!(
                        main.get_math_preview().texts.row_count() >= 2,
                        "Mixed Chinese text uses native glyphs"
                    );
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    let saved = app.store.load().unwrap();
                    assert!(saved.packs[0].cards.last().unwrap().prompt.contains("计算"));
                    assert_eq!(saved.packs[0].cards.last().unwrap().answer, "sqrt(3)/2");
                    let path = checks.output.join("authored_pack.json");
                    Store::export_pack(&path, &saved.packs[0]).unwrap();
                    assert_eq!(Store::read_pack(&path).unwrap(), saved.packs[0]);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(20, true);
                    app.show_practice().unwrap();
                    let practice = practice_window(app);
                    assert!(practice.get_prompt_texts().row_count() >= 2);
                    snapshot(practice.window(), checks.output.join("mixed_practice.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_add_card();
                    type_content(app, &main, 0, "[[1,2],[3,4]]");
                    main.invoke_resize_math_grid(3, 3);
                    main.invoke_save_editor();
                    assert!(main.get_editor_visible());
                    assert!(!main.get_editor_error().is_empty());
                    for cell in [2usize, 5, 6, 7, 8] {
                        let region = app
                            .math_paths
                            .borrow()
                            .iter()
                            .position(|p| p == &vec![cell])
                            .unwrap();
                        main.invoke_select_math_region(region as i32);
                        let value = (cell + 1).to_string();
                        main.set_math_input(value.clone().into());
                        main.invoke_edit_math_input(value.into());
                        assert!(app.flush_math());
                        app.refresh_math(false);
                    }
                    type_content(app, &main, 1, "1");
                    main.invoke_activate_content(0);
                    main.set_editor_scroll_y(0.0);
                }
                4 => {
                    snapshot(main.window(), checks.output.join("matrix_editor.png"));
                    assert_eq!(main.get_math_grid_rows(), 3);
                    assert_eq!(main.get_math_grid_columns(), 3);
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    main.invoke_add_card();
                    type_content(app, &main, 0, "cases(x,x>=0,-x,x<0)");
                    type_content(app, &main, 1, "x");
                    main.invoke_activate_content(0);
                    main.set_editor_scroll_y(0.0);
                }
                5 => {
                    snapshot(main.window(), checks.output.join("cases_editor.png"));
                    let before = app.store.load().unwrap();
                    main.invoke_cancel_editor();
                    assert_eq!(
                        app.store.load().unwrap(),
                        before,
                        "Cancel leaves persisted content untouched"
                    );
                    main.invoke_add_card();
                    type_content(
                        app,
                        &main,
                        0,
                        "int(0,1,x^2)+sum(k=1,n,k)+lim(x->0,sin(x)/x)",
                    );
                    type_content(app, &main, 1, "1/3");
                    main.invoke_activate_content(0);
                    main.set_editor_scroll_y(0.0);
                }
                6 => {
                    snapshot(main.window(), checks.output.join("advanced_editor.png"));
                    assert!(main.get_math_preview().formula_width > 0.0);
                    assert!(main.get_math_error().is_empty());
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    main.invoke_add_card();
                    type_content(app, &main, 0, "求 ");
                    type_content(app, &main, 1, "3");
                    main.invoke_activate_content(0);
                    main.set_editor_scroll_y(0.0);
                }
                7 => {
                    assert!(!main.get_math_has_formula());
                    assert_eq!(main.get_math_preview().formula_width, 0.0);
                    snapshot(main.window(), checks.output.join("plain_editor.png"));
                    main.invoke_math_tool("math".into());
                }
                8 => {
                    assert!(main.get_math_has_formula());
                    assert!(!main.get_math_text_mode());
                    let top = math_toolbar(&main);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(top.x + 108.0, top.y + 16.0),
                    );
                }
                9 => {
                    snapshot(main.window(), checks.output.join("function_palette.png"));
                    let top = math_toolbar(&main);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(top.x + 113.0, top.y + 60.0),
                    );
                    assert_eq!(app.math_fields.borrow()[0].selection, vec![1, 0, 1]);
                    key(main.window(), "2".into());
                }
                10 => {
                    assert!(app.flush_math());
                    assert_eq!(
                        app.math_fields.borrow()[0].selected().value,
                        "2",
                        "Template selection returns keyboard focus to its slot"
                    );
                    click_math_slot(app, &main, &[1, 1, 0]);
                    key(main.window(), "8".into());
                }
                11 => {
                    assert!(app.flush_math());
                    assert_eq!(app.math_fields.borrow()[0].selected().value, "8");
                    snapshot(main.window(), checks.output.join("log_generator.png"));
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    let library = app.store.load().unwrap();
                    let card = library.packs[0].cards.last().unwrap();
                    assert!(card.prompt_latex.as_ref().unwrap().contains(r"\log"));
                    assert_eq!(card.answer, "3");
                    let path = checks.output.join("generated_functions_pack.json");
                    Store::export_pack(&path, &library.packs[0]).unwrap();
                    assert_eq!(Store::read_pack(&path).unwrap(), library.packs[0]);
                    main.invoke_add_card();
                    type_content(app, &main, 0, "ε = ?");
                    main.invoke_activate_content(1);
                    main.invoke_math_tool("math".into());
                    main.set_editor_scroll_y(0.0);
                }
                12 => {
                    let top = math_toolbar(&main);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(top.x + 184.0, top.y + 16.0),
                    );
                }
                13 => {
                    snapshot(main.window(), checks.output.join("symbol_palette.png"));
                    key(main.window(), Key::Escape.into());
                    assert_eq!(
                        app.math_fields.borrow()[1].selected(),
                        &r#try::math_editor::Node::blank()
                    );
                    let top = math_toolbar(&main);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(top.x + 184.0, top.y + 16.0),
                    );
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(top.x + 176.0, top.y + 96.0),
                    );
                    assert_eq!(main.get_math_input(), "ε");
                    key(main.window(), "+1".into());
                }
                14 => {
                    assert!(app.flush_math());
                    assert_eq!(
                        main.get_math_input(),
                        "ε+1",
                        "Typing after a symbol continues at its end"
                    );
                    snapshot(main.window(), checks.output.join("symbol_editor.png"));
                    main.invoke_cancel_editor();
                    let last = app.library.borrow().packs[0].cards.len() - 1;
                    main.invoke_edit_card(last as i32);
                    main.invoke_activate_content(0);
                    let path = app
                        .math_paths
                        .borrow()
                        .iter()
                        .find(|path| {
                            app.math_fields.borrow()[0]
                                .root
                                .at(path)
                                .is_some_and(|node| node.value == "8")
                        })
                        .unwrap()
                        .clone();
                    click_math_slot(app, &main, &path);
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                    main.set_editor_scroll_y(0.0);
                }
                15 => {
                    snapshot(
                        main.window(),
                        checks.output.join("generator_compact_before.png"),
                    );
                    let path = app
                        .math_paths
                        .borrow()
                        .iter()
                        .find(|path| {
                            app.math_fields.borrow()[0]
                                .root
                                .at(path)
                                .is_some_and(|node| node.value == "8")
                        })
                        .unwrap()
                        .clone();
                    click_math_slot(app, &main, &path);
                    assert!(
                        !main.get_math_text_mode(),
                        "Clicking slot {path:?} selected {:?}; canvas = ({}, {}), scale = {}",
                        app.math_fields.borrow()[0].selection,
                        main.get_math_canvas_left(),
                        main.get_math_canvas_top(),
                        main.get_math_canvas_scale()
                    );
                    assert!(main.get_math_has_formula());
                    assert!(main.get_math_error().is_empty());
                    snapshot(main.window(), checks.output.join("generator_compact.png"));
                    main.invoke_cancel_editor();
                    app.close_main();
                    assert!(app.math_fields.borrow().is_empty());
                    assert!(app.math_paths.borrow().is_empty());
                    checks.finished.set(true);
                    println!(
                        "Native math authoring, root-first fraction repair/unwrap clicks, plain text, function/symbol palette clicks, independent log slots, keyboard focus, export/reload, compact layout and cleanup checks passed."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            authoring_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens real windows and checks visual math authoring"]
fn native_math_authoring() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/math_authoring_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    authoring_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Visual math authoring check failed or timed out"
    );
}

fn settings_click(main: &MainWindow, right: f32, top: f32) {
    pointer_click(
        main.window(),
        slint::LogicalPosition::new(
            main.get_settings_left() + main.get_settings_width() - right,
            main.get_settings_top() + top,
        ),
    );
}

fn localization_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(450), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            if phase == 7 {
                snapshot(
                    practice_window(app).window(),
                    checks.output.join("practice_en.png"),
                );
                app.close_practice();
                checks.finished.set(true);
                println!(
                    "Native language menu, live English/Chinese bindings, persisted preferences, unchanged pack content, formula tools, window recreation and minimum layout checks passed."
                );
                app.quit();
                return;
            }
            let main = main_window(app);
            match phase {
                0 => {
                    assert_eq!(main.get_interval_display(), "30 分钟");
                    snapshot(main.window(), checks.output.join("settings_zh.png"));
                    main.window().dispatch_event(WindowEvent::PointerScrolled {
                        position: slint::LogicalPosition::new(
                            main.get_settings_left() + 100.0,
                            main.get_settings_top() + 100.0,
                        ),
                        delta_x: 0.0,
                        delta_y: -600.0,
                    });
                    let weak = main.as_weak();
                    Timer::single_shot(Duration::from_millis(100), move || {
                        if let Some(main) = weak.upgrade() {
                            pointer_click(
                                main.window(),
                                slint::LogicalPosition::new(
                                    main.get_language_left() + 40.0,
                                    main.get_language_top() + 18.0,
                                ),
                            );
                        }
                    });
                }
                1 => {
                    key(main.window(), Key::DownArrow.into());
                    key(main.window(), Key::Return.into());
                    assert_eq!(
                        main.get_interface_language(),
                        2,
                        "Actual language menu switches to English"
                    );
                    assert_eq!(main.get_interval_display(), "30 min");
                    assert_eq!(main.get_feedback_display(), "8 s (custom)");
                    assert_eq!(main.get_pause_display(), "Off");
                    assert_eq!(main.get_answer_mode_labels().row_data(1).unwrap(), "Choice");
                    assert_eq!(
                        main.get_math_functions().row_data(0).unwrap().name,
                        "Logarithm with base"
                    );
                    assert_eq!(
                        app.store.load().unwrap().settings.interface_language,
                        InterfaceLanguage::English
                    );
                    snapshot(main.window(), checks.output.join("settings_en.png"));
                    main.set_current_tab(0);
                }
                2 => {
                    snapshot(main.window(), checks.output.join("matrix_en.png"));
                    app.open_editor(Some(0), false, None);
                    assert_eq!(main.get_editor_title(), "Edit card");
                }
                3 => {
                    snapshot(main.window(), checks.output.join("editor_en.png"));
                    main.invoke_cancel_editor();
                    main.invoke_choose_language(1);
                    assert_eq!(main.get_interval_display(), "30 分钟");
                    assert_eq!(main.get_answer_mode_labels().row_data(1).unwrap(), "选择");
                    main.invoke_choose_language(2);
                    app.close_main();
                    app.show_main().unwrap();
                    let reopened = main_window(app);
                    assert_eq!(reopened.get_interface_language(), 2);
                    assert_eq!(reopened.get_interval_display(), "30 min");
                    reopened
                        .window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                    reopened.set_current_tab(1);
                }
                4 => {
                    snapshot(
                        main.window(),
                        checks.output.join("settings_en_minimum_top.png"),
                    );
                    main.window().dispatch_event(WindowEvent::PointerScrolled {
                        position: slint::LogicalPosition::new(
                            main.get_settings_left() + 100.0,
                            main.get_settings_top() + 100.0,
                        ),
                        delta_x: 0.0,
                        delta_y: -300.0,
                    });
                }
                5 => {
                    snapshot(main.window(), checks.output.join("settings_en_minimum.png"));
                    main.set_current_tab(0);
                }
                6 => {
                    snapshot(main.window(), checks.output.join("matrix_en_minimum.png"));
                    let original: Vec<KnowledgePack> = serde_json::from_slice(
                        &std::fs::read(checks.output.join("original_packs.json")).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(
                        app.store.load().unwrap().packs,
                        original,
                        "Language does not rewrite authored content"
                    );
                    let pack = &app.library.borrow().packs[0];
                    let question = PreparedQuestion::new(
                        &pack.cards[0],
                        AnswerMode::TrueFalse,
                        &mut rand::rng(),
                    )
                    .unwrap();
                    assert_eq!(question.options[0].text, "True");
                    app.close_main();
                    app.show_practice().unwrap();
                }
                _ => unreachable!(),
            }
            localization_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens real bilingual windows using an isolated profile"]
fn native_localization() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/localization_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.interface_language = InterfaceLanguage::Chinese;
    std::fs::write(
        output.join("original_packs.json"),
        serde_json::to_vec(&library.packs).unwrap(),
    )
    .unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let main = main_window(&app);
    main.window()
        .set_size(slint::LogicalSize::new(920.0, 640.0));
    main.set_current_tab(1);
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    localization_phase(checks.clone(), 0);
    let watchdog = Rc::downgrade(&checks.app);
    Timer::single_shot(Duration::from_secs(15), move || {
        if let Some(app) = watchdog.upgrade() {
            app.quit();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Localization check failed or timed out"
    );
}

static AUTOMATIC_UPDATE_FETCHES: AtomicUsize = AtomicUsize::new(0);

fn automatic_update_fixture() -> Result<CheckOutcome, CheckError> {
    AUTOMATIC_UPDATE_FETCHES.fetch_add(1, Ordering::SeqCst);
    Ok(CheckOutcome::Available(
        update_check::Release::from_tag("v0.2.0").unwrap(),
    ))
}

struct AutomaticUpdateChecks {
    app: Rc<App>,
    output: PathBuf,
    finished: Cell<bool>,
    original_packs: Vec<KnowledgePack>,
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

fn automatic_update_phase(checks: Rc<AutomaticUpdateChecks>, phase: u8) {
    let delay = if phase == 0 {
        update_schedule::STARTUP_DELAY + Duration::from_secs(2)
    } else {
        Duration::from_millis(450)
    };
    Timer::single_shot(delay, move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    assert_eq!(AUTOMATIC_UPDATE_FETCHES.load(Ordering::SeqCst), 1);
                    assert!(
                        app.main_window.borrow().is_none(),
                        "Background checks stay quiet"
                    );
                    assert!(app.practice.borrow().is_none());
                    assert!(app.automatic_update_timer.running());
                    let mut restarted = UpdateSchedule::open(
                        app.store.directory(),
                        env!("CARGO_PKG_VERSION"),
                        chrono::Utc::now().timestamp(),
                    );
                    assert!(matches!(
                        restarted.cached_outcome(),
                        Some(CheckOutcome::Available(_))
                    ));
                    assert!(
                        restarted.delay(chrono::Utc::now().timestamp())
                            > Duration::from_secs(86300)
                    );
                    app.begin_automatic_update_check(automatic_update_fixture);
                    app.show_settings(false).unwrap();
                    let main = main_window(app);
                    main.window()
                        .set_size(slint::LogicalSize::new(820.0, 560.0));
                    scroll_settings_to_bottom(&main);
                }
                1 => {
                    assert_eq!(
                        AUTOMATIC_UPDATE_FETCHES.load(Ordering::SeqCst),
                        1,
                        "Daily budget suppresses a second automatic request"
                    );
                    let main = main_window(app);
                    assert!(main.get_auto_check_updates());
                    snapshot(
                        main.window(),
                        checks.output.join("automatic_setting_on.png"),
                    );
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_automatic_update_left() + 30.0,
                            main.get_automatic_update_top() + 18.0,
                        ),
                    );
                    assert!(
                        !main.get_auto_check_updates(),
                        "The actual checkbox disables automatic checks"
                    );
                }
                2 => {
                    assert!(!app.store.load().unwrap().settings.auto_check_updates);
                    assert!(!app.automatic_update_timer.running());
                    app.close_main();
                    app.show_settings(false).unwrap();
                    let main = main_window(app);
                    assert!(
                        !main.get_auto_check_updates(),
                        "The saved switch survives window recreation"
                    );
                    app.begin_automatic_update_check(automatic_update_fixture);
                    scroll_settings_to_bottom(&main);
                }
                3 => {
                    assert_eq!(
                        AUTOMATIC_UPDATE_FETCHES.load(Ordering::SeqCst),
                        1,
                        "Disabled automatic checks never invoke the transport"
                    );
                    let main = main_window(app);
                    snapshot(
                        main.window(),
                        checks.output.join("automatic_setting_off.png"),
                    );
                    main.invoke_show_about();
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_check_link_left() + 18.0,
                            main.get_check_link_top() + 18.0,
                        ),
                    );
                    assert_eq!(main.get_update_state(), 1);
                }
                4 => {
                    assert_eq!(
                        AUTOMATIC_UPDATE_FETCHES.load(Ordering::SeqCst),
                        2,
                        "Manual checks work while automatic checking is off and within the daily budget"
                    );
                    let main = main_window(app);
                    assert_eq!(main.get_update_status(), "新版本 0.2.0");
                    assert!(main.get_update_download());
                    snapshot(
                        main.window(),
                        checks.output.join("automatic_about_available.png"),
                    );
                    main.invoke_hide_about();
                    main.invoke_set_auto_check_updates(true);
                    assert!(app.store.load().unwrap().settings.auto_check_updates);
                    assert!(app.automatic_update_timer.running());
                    // Advance only the isolated budget to exercise a failed next-day check.
                    app.update_check
                        .lock()
                        .unwrap()
                        .schedule
                        .record_attempt(chrono::Utc::now().timestamp() - 86400)
                        .unwrap();
                    app.begin_automatic_update_check(|| {
                        AUTOMATIC_UPDATE_FETCHES.fetch_add(1, Ordering::SeqCst);
                        Err(CheckError::Network)
                    });
                }
                5 => {
                    assert_eq!(AUTOMATIC_UPDATE_FETCHES.load(Ordering::SeqCst), 3);
                    let main = main_window(app);
                    assert_eq!(
                        main.get_update_status(),
                        "新版本 0.2.0",
                        "A background failure retains an already discovered version"
                    );
                    assert!(main.get_update_download());
                    main.invoke_choose_language(2);
                    assert_eq!(main.get_update_status(), "Version 0.2.0 available");
                    scroll_settings_to_bottom(&main);
                }
                6 => {
                    let main = main_window(app);
                    snapshot(
                        main.window(),
                        checks.output.join("automatic_setting_en.png"),
                    );
                    assert_eq!(app.store.load().unwrap().packs, checks.original_packs);
                    assert!(
                        std::fs::metadata(app.store.directory().join("update_check_state.json"))
                            .unwrap()
                            .len()
                            <= 4096
                    );
                    app.close_main();
                    app.show_practice().unwrap();
                    app.close_practice();
                    assert!(
                        app.automatic_update_timer.running(),
                        "Closing practice does not cancel the daily update timer"
                    );
                    checks.finished.set(true);
                    println!(
                        "Native automatic checks passed: real 30-second delay, quiet background completion, daily/restart budget, actual persisted switch, manual override, retained hint after failure, bilingual layout and timer cleanup."
                    );
                    app.quit();
                    assert!(!app.automatic_update_timer.running());
                    return;
                }
                _ => unreachable!(),
            }
            automatic_update_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "waits for the real automatic-check delay using an isolated profile and transport fixture"]
fn native_automatic_updates() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/automatic_update_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    assert!(r#try::knowledge::Settings::default().auto_check_updates);
    library.settings.auto_check_updates = true;
    library.settings.interface_language = InterfaceLanguage::Chinese;
    store.save(&library).unwrap();
    let original_packs = library.packs.clone();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    AUTOMATIC_UPDATE_FETCHES.store(0, Ordering::SeqCst);
    app.update_fetch.set(automatic_update_fixture);
    app.start(true).unwrap();
    assert_eq!(
        AUTOMATIC_UPDATE_FETCHES.load(Ordering::SeqCst),
        0,
        "Startup does not immediately access the network"
    );
    assert!(app.automatic_update_timer.running());
    let checks = Rc::new(AutomaticUpdateChecks {
        app,
        output,
        finished: Cell::new(false),
        original_packs,
    });
    automatic_update_phase(checks.clone(), 0);
    let watchdog = Rc::downgrade(&checks.app);
    Timer::single_shot(Duration::from_secs(45), move || {
        if let Some(app) = watchdog.upgrade() {
            app.quit();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Native automatic update check failed or timed out"
    );
}

struct UpdateChecks {
    app: Rc<App>,
    output: PathBuf,
    finished: Cell<bool>,
    gate: RefCell<Option<std::sync::mpsc::Sender<()>>>,
    fetches: Arc<AtomicUsize>,
    original_packs: Vec<KnowledgePack>,
    about_geometry: Cell<Option<[f32; 6]>>,
}

fn update_phase(checks: Rc<UpdateChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(450), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let main = main_window(app);
            let size = main
                .window()
                .size()
                .to_logical(main.window().scale_factor());
            let geometry = [
                main.get_about_height(),
                main.get_about_left(),
                size.height - main.get_about_top(),
                main.get_check_link_top() - main.get_about_top(),
                main.get_home_link_top() - main.get_about_top(),
                main.get_notices_link_top() - main.get_about_top(),
            ];
            if let Some(original) = checks.about_geometry.get() {
                assert_eq!(
                    geometry, original,
                    "Version checking must preserve the about panel size, anchor and content positions"
                );
            } else {
                checks.about_geometry.set(Some(geometry));
            }
            match phase {
                0 => {
                    assert_eq!(main.get_update_state(), 0);
                    assert!(!main.get_update_download());
                    main.invoke_show_about();
                    snapshot(main.window(), checks.output.join("about_idle_zh.png"));
                    let (send, receive) = std::sync::mpsc::channel();
                    checks.gate.replace(Some(send));
                    let weak = Rc::downgrade(app);
                    let fetches = checks.fetches.clone();
                    let receive = RefCell::new(Some(receive));
                    main.on_check_updates(move || {
                        if let Some(app) = weak.upgrade()
                            && let Some(receive) = receive.borrow_mut().take()
                        {
                            let fetches = fetches.clone();
                            app.begin_update_check(move || {
                                fetches.fetch_add(1, Ordering::SeqCst);
                                receive.recv_timeout(Duration::from_secs(6)).unwrap();
                                Ok(CheckOutcome::Available(
                                    update_check::Release::from_tag("v0.2.0").unwrap(),
                                ))
                            });
                        }
                    });
                    // Operate the actual check icon next to the version label.
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_check_link_left() + 18.0,
                            main.get_check_link_top() + 18.0,
                        ),
                    );
                    assert_eq!(main.get_update_state(), 1, "Check icon starts the worker");
                    assert_eq!(main.get_update_status(), "正在检查…");
                    let fetches = checks.fetches.clone();
                    app.begin_update_check(move || {
                        fetches.fetch_add(1, Ordering::SeqCst);
                        Ok(CheckOutcome::UpToDate)
                    });
                }
                1 => {
                    assert_eq!(
                        checks.fetches.load(Ordering::SeqCst),
                        1,
                        "Repeated checks share one running worker"
                    );
                    snapshot(main.window(), checks.output.join("about_checking_zh.png"));
                    let old = main.as_weak();
                    main.invoke_hide_about();
                    app.close_main();
                    drop(main);
                    assert!(
                        old.upgrade().is_none(),
                        "Worker does not retain the main window"
                    );
                    app.show_main().unwrap();
                    let reopened = main_window(app);
                    reopened
                        .window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                    reopened.invoke_choose_language(2);
                    assert_eq!(reopened.get_update_status(), "Checking…");
                    reopened.invoke_show_about();
                    checks.gate.borrow_mut().take().unwrap().send(()).unwrap();
                }
                2 => {
                    assert_eq!(
                        main.get_update_state(),
                        2,
                        "Worker notifies the recreated window"
                    );
                    assert_eq!(main.get_update_status(), "Version 0.2.0 available");
                    assert!(main.get_update_download());
                    assert!(main.get_about_top() >= 0.0);
                    // Resize notifications dismiss popups; reopen after layout settles.
                    main.invoke_show_about();
                    snapshot(
                        main.window(),
                        checks.output.join("about_available_en_minimum.png"),
                    );
                    main.invoke_choose_language(1);
                    assert_eq!(main.get_update_status(), "新版本 0.2.0");
                }
                3 => {
                    snapshot(
                        main.window(),
                        checks.output.join("about_available_zh_minimum.png"),
                    );
                    key(main.window(), Key::Escape.into());
                    main.invoke_show_about();
                    app.begin_update_check(|| Err(CheckError::Network));
                }
                4 => {
                    assert_eq!(main.get_update_status(), "暂时无法检查");
                    assert!(!main.get_update_download());
                    snapshot(main.window(), checks.output.join("about_failure_zh.png"));
                    app.begin_update_check(|| Ok(CheckOutcome::UpToDate));
                }
                5 => {
                    assert_eq!(main.get_update_status(), "已是最新");
                    assert!(!main.get_update_download());
                    snapshot(main.window(), checks.output.join("about_current_zh.png"));
                    app.begin_update_check(|| Ok(CheckOutcome::NoPublicRelease));
                }
                6 => {
                    assert_eq!(main.get_update_status(), "尚无公开版本");
                    main.invoke_choose_language(2);
                    assert_eq!(main.get_update_status(), "No public releases yet");
                    snapshot(main.window(), checks.output.join("about_no_release_en.png"));
                    main.invoke_hide_about();
                    app.close_main();
                    drop(main);
                    app.show_main().unwrap();
                    let main = main_window(app);
                    assert_eq!(main.get_update_status(), "No public releases yet");
                    main.invoke_show_about();
                    app.begin_update_check(|| Err(CheckError::RateLimited));
                }
                7 => {
                    assert_eq!(main.get_update_status(), "Check limited; try later");
                    *app.update_transfer.lock().unwrap() = UpdateTransfer::Failed("fixture".into());
                    app.begin_update_check(|| Err(CheckError::RateLimited));
                    assert!(
                        matches!(*app.update_transfer.lock().unwrap(), UpdateTransfer::Idle),
                        "a fresh check clears a stale download error"
                    );
                    assert!(!main.get_update_download());
                    snapshot(
                        main.window(),
                        checks.output.join("about_rate_limited_en.png"),
                    );
                    main.invoke_choose_language(1);
                }
                8 => {
                    assert_eq!(main.get_update_status(), "检查受限，请稍后再试");
                    assert!(!main.get_update_download());
                    snapshot(
                        main.window(),
                        checks.output.join("about_rate_limited_zh.png"),
                    );
                    app.begin_update_download(|cancel, progress| {
                        progress(42, Some(100));
                        let deadline = std::time::Instant::now() + Duration::from_secs(10);
                        while !cancel.load(Ordering::Relaxed)
                            && std::time::Instant::now() < deadline
                        {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err("cancelled fixture".into())
                    });
                }
                9 => {
                    assert_eq!(main.get_update_status(), "正在下载 42%");
                    assert_eq!(main.get_update_action(), "取消");
                    assert!(main.get_update_busy());
                    main.invoke_choose_language(2);
                }
                10 => {
                    assert_eq!(main.get_update_status(), "Downloading 42%");
                    assert_eq!(main.get_update_action(), "Cancel");
                    app.close_main();
                    app.show_main().unwrap();
                }
                11 => {
                    assert_eq!(main.get_update_status(), "Downloading 42%");
                    main.invoke_show_about();
                    main.invoke_cancel_update_download();
                }
                12 => {
                    assert!(!main.get_update_busy());
                    assert_eq!(main.get_update_status(), "Check limited; try later");
                    *app.update_transfer.lock().unwrap() =
                        UpdateTransfer::Ready(updater::ui_fixture(&checks.output).unwrap());
                    app.refresh_update_status();
                }
                13 => {
                    assert_eq!(main.get_update_action(), "Restart & update");
                    assert_eq!(main.get_update_status(), "Update ready");
                    main.invoke_cancel_update_download();
                    assert!(
                        app.restart_update.borrow().is_none(),
                        "late cancellation never restarts"
                    );
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Idle
                    ));
                    *app.update_transfer.lock().unwrap() =
                        UpdateTransfer::Ready(updater::ui_fixture(&checks.output).unwrap());
                    app.refresh_update_status();
                    app.editor.replace(Some(EditorTarget::NewPack {
                        id: "update_fixture".into(),
                    }));
                    main.invoke_restart_update();
                    assert!(
                        app.restart_update.borrow().is_none(),
                        "unsaved editor blocks restart"
                    );
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Ready(_)
                    ));
                    app.editor.borrow_mut().take();
                    main.invoke_choose_language(1);
                }
                14 => {
                    assert_eq!(main.get_update_action(), "重启并更新");
                    assert_eq!(main.get_update_status(), "更新已就绪");
                    assert_eq!(app.store.load().unwrap().packs, checks.original_packs);
                    main.invoke_restart_update();
                    assert!(
                        app.restart_update.borrow().is_some(),
                        "ready update is handed to shutdown"
                    );
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Idle
                    ));
                    checks.finished.set(true);
                    println!(
                        "Native update checking, transfer progress, cancellation, ready state, unsaved-editor guard, bilingual labels and stable about geometry passed."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            update_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens real update UI using fixtures and an isolated profile"]
fn native_update_check() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/update_check_validation");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.interface_language = InterfaceLanguage::Chinese;
    let original_packs = library.packs.clone();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    main_window(&app)
        .window()
        .set_size(slint::LogicalSize::new(820.0, 560.0));
    let checks = Rc::new(UpdateChecks {
        app,
        output,
        finished: Cell::new(false),
        gate: RefCell::new(None),
        fetches: Arc::new(AtomicUsize::new(0)),
        original_packs,
        about_geometry: Cell::new(None),
    });
    update_phase(checks.clone(), 0);
    let watchdog = Rc::downgrade(&checks.app);
    Timer::single_shot(Duration::from_secs(20), move || {
        if let Some(app) = watchdog.upgrade() {
            app.quit();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Native update check failed or timed out"
    );
}

#[cfg(feature = "distribution")]
struct LicenseChecks {
    app: Rc<App>,
    output: PathBuf,
    finished: Cell<bool>,
}

#[cfg(feature = "distribution")]
fn assert_notice_panel(main: &MainWindow, visible: bool) {
    let buffer = main.window().take_snapshot().unwrap();
    let scale = main.window().scale_factor();
    let x = ((main.get_about_left() + 280.0) * scale) as usize;
    let y = ((main.get_notices_top() + 60.0) * scale) as usize;
    let offset = (y * buffer.width() as usize + x) * 4;
    let pixel = &buffer.as_bytes()[offset..offset + 3];
    if (pixel == [41, 41, 41]) != visible {
        snapshot(
            main.window(),
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("local_artifacts/licenses_check/notice_visibility_failure.png"),
        );
    }
    assert_eq!(
        pixel == [41, 41, 41],
        visible,
        "Notice popup visibility at ({x}, {y}), pixel {pixel:?}"
    );
}

#[cfg(feature = "distribution")]
fn click_notices(main: &MainWindow) {
    pointer_click(
        main.window(),
        slint::LogicalPosition::new(
            main.get_notices_link_left() + 40.0,
            main.get_notices_link_top() + 18.0,
        ),
    );
}

#[cfg(feature = "distribution")]
fn license_phase(checks: Rc<LicenseChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let main = main_window(app);
            match phase {
                0 => main.invoke_show_about(),
                1 => {
                    snapshot(main.window(), checks.output.join("about_minimal_zh.png"));
                    click_notices(&main);
                }
                2 => {
                    assert_notice_panel(&main, true);
                    snapshot(main.window(), checks.output.join("notices_minimal_zh.png"));
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_about_left() + 30.0,
                            main.get_notices_top() + 33.0,
                        ),
                    );
                }
                3 => {
                    assert_notice_panel(&main, false);
                    snapshot(main.window(), checks.output.join("about_after_back.png"));
                    click_notices(&main);
                }
                4 => {
                    assert_notice_panel(&main, true);
                    key(main.window(), Key::Escape.into());
                }
                5 => {
                    assert_notice_panel(&main, false);
                    main.invoke_choose_language(2);
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                }
                6 => {
                    let buffer = main.window().take_snapshot().unwrap();
                    let scale = main.window().scale_factor();
                    let size = main.window().size().to_logical(scale);
                    let x = ((main.get_about_left() + 36.0) * scale) as usize;
                    let y = ((size.height - 120.0) * scale) as usize;
                    let offset = (y * buffer.width() as usize + x) * 4;
                    assert_ne!(
                        &buffer.as_bytes()[offset..offset + 3],
                        [41, 41, 41],
                        "Resizing dismisses the old popup"
                    );
                    main.invoke_show_about();
                }
                7 => {
                    snapshot(main.window(), checks.output.join("about_minimal_en.png"));
                    click_notices(&main);
                }
                8 => {
                    assert_notice_panel(&main, true);
                    snapshot(main.window(), checks.output.join("notices_minimal_en.png"));
                    pointer_click(main.window(), slint::LogicalPosition::new(500.0, 100.0));
                }
                9 => {
                    assert_notice_panel(&main, false);
                    main.invoke_show_about();
                }
                10 => click_notices(&main),
                11 => {
                    assert_notice_panel(&main, true);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_about_left() + 262.0,
                            main.get_notices_top() + 33.0,
                        ),
                    );
                }
                12 => {
                    assert_notice_panel(&main, false);
                    main.invoke_show_notices();
                }
                13 => {
                    assert_notice_panel(&main, true);
                    let old_main = main.as_weak();
                    app.close_main();
                    drop(main);
                    assert!(
                        old_main.upgrade().is_none(),
                        "Tray return releases the main window and its popups"
                    );
                    checks.finished.set(true);
                    println!(
                        "Native compact MIT footer and third-party notices, bilingual minimum layout, real back/close/outside/Escape actions, embedded originals and popup release passed."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            license_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[cfg(feature = "distribution")]
#[test]
#[ignore = "opens compact third-party notices using an isolated profile"]
fn native_licenses() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/licenses_check");
    std::fs::create_dir_all(&output).unwrap();
    let mut embedded = Vec::new();
    r#try::distribution::write_notices(&mut embedded).unwrap();
    let text = std::str::from_utf8(&embedded).unwrap();
    assert!(text.contains("Permission is hereby granted"));
    assert!(text.contains("SIL OPEN FONT LICENSE"));
    assert!(text.contains("Copyright"));
    assert!(text.contains("Mozilla Public License"));
    assert!(text.contains("https://crates.io/api/v1/crates/option-ext/0.2.0/download"));
    std::fs::write(output.join("embedded_notices.txt"), &embedded).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.interface_language = InterfaceLanguage::Chinese;
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(LicenseChecks {
        app,
        output,
        finished: Cell::new(false),
    });
    license_phase(checks.clone(), 0);
    let watchdog = Rc::downgrade(&checks.app);
    Timer::single_shot(Duration::from_secs(12), move || {
        if let Some(app) = watchdog.upgrade() {
            app.quit();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.finished.get(), "License check failed or timed out");
}

fn settings_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(400), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let startup_file = app.store.directory().join("startup_entry");
            match phase {
                0 => {
                    let main = main_window(app);
                    assert_eq!(main.get_pause_minutes(), 0);
                    assert_eq!(main.get_feedback_seconds(), 8);
                    assert_eq!(
                        main.get_feedback_options().row_data(3).unwrap(),
                        "8 秒（自定）"
                    );
                    assert!(!main.get_autostart_enabled());
                    assert!(!main.get_quiet_hours_enabled());
                    snapshot(main.window(), checks.output.join("settings_default.png"));
                    settings_click(&main, 120.0, 51.0);
                    key(main.window(), Key::UpArrow.into());
                    key(main.window(), Key::Return.into());
                    assert_eq!(
                        main.get_interval_minutes(),
                        25,
                        "Actual dropdown changes interval"
                    );
                    settings_click(&main, 120.0, 105.0);
                    for _ in 0..3 {
                        key(main.window(), Key::UpArrow.into());
                    }
                    key(main.window(), Key::Return.into());
                    assert_eq!(main.get_feedback_seconds(), 1, "One-second preset works");
                    assert_eq!(app.store.load().unwrap().settings.feedback_seconds, 1);
                    settings_click(&main, 32.0, 51.0);
                }
                1 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("settings_precise.png"));
                    key(main.window(), Key::UpArrow.into());
                    key(main.window(), Key::Return.into());
                    assert_eq!(
                        main.get_interval_minutes(),
                        26,
                        "Precise popup applies on Enter"
                    );
                    assert_eq!(
                        main.get_interval_options().row_data(4).unwrap(),
                        "26 分钟（自定）"
                    );
                    assert_eq!(main.get_interval_display(), "26 分钟（自定）");
                    settings_click(&main, 32.0, 51.0);
                    key(main.window(), Key::UpArrow.into());
                    key(main.window(), Key::Escape.into());
                    assert_eq!(
                        main.get_interval_minutes(),
                        26,
                        "Esc cancels the popup draft"
                    );
                    settings_click(&main, 120.0, 180.0);
                    for _ in 0..3 {
                        key(main.window(), Key::DownArrow.into());
                    }
                    key(main.window(), Key::Return.into());
                    assert_eq!(
                        main.get_pause_minutes(),
                        180,
                        "Actual menu pauses for three hours"
                    );
                    assert!(main.get_pause_detail().starts_with("至 "));
                    assert!(app.pause_timer.running());
                    assert!(!app.reminder_timer.running());
                    assert_eq!(app.store.load().unwrap(), *app.library.borrow());
                    // Read the actual isolated system registration, rather than
                    // enabling this computer's login startup entry.
                    settings_click(&main, 30.0, 280.0);
                    assert!(main.get_autostart_enabled());
                    assert!(startup_file.is_file());
                    settings_click(&main, 30.0, 224.0);
                }
                2 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("settings_daily.png"));
                    main.invoke_set_quiet_enabled(true);
                    let original = app.store.load().unwrap().settings.quiet_hours;
                    main.set_quiet_start("25:00".into());
                    main.invoke_set_quiet_range("25:00".into(), "08:00".into());
                    assert!(!main.get_quiet_time_error().is_empty());
                    assert_eq!(main.get_quiet_start(), "25:00");
                    assert_eq!(app.store.load().unwrap().settings.quiet_hours, original);
                    main.invoke_set_quiet_range("23:00".into(), "07:30".into());
                    assert!(main.get_quiet_time_error().is_empty());
                    key(main.window(), Key::Escape.into());
                    main.invoke_set_pause(-1);
                    assert!(!app.pause_timer.running());
                    assert_eq!(main.get_pause_minutes(), -1);
                    assert_eq!(main.get_pause_display(), "直到恢复");
                    snapshot(main.window(), checks.output.join("settings_paused.png"));
                    std::fs::remove_file(&startup_file).unwrap();
                    app.read_autostart_status();
                    app.refresh();
                    assert!(!main.get_autostart_enabled());
                    main.invoke_set_autostart(true);
                    main.invoke_set_autostart(false);
                    assert!(!main.get_autostart_enabled());
                    assert!(!startup_file.exists());
                    let blocker = app.store.directory().join("startup_blocker");
                    std::fs::write(&blocker, b"preserve").unwrap();
                    let _ = app
                        .autostart
                        .replace(Ok(Autostart::isolated(blocker.join("entry"))));
                    main.invoke_set_autostart(true);
                    assert!(!main.get_autostart_enabled());
                    assert!(!main.get_autostart_error().is_empty());
                    assert!(main.get_message().is_empty());
                    assert_eq!(std::fs::read(blocker).unwrap(), b"preserve");
                    let _ = app.autostart.replace(Ok(Autostart::isolated(startup_file)));
                    app.read_autostart_status();
                    app.refresh();
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                }
                3 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("settings_compact.png"));
                    main.invoke_go_background();
                    assert!(app.main_window.borrow().is_none());
                    assert!(!app.reminder_timer.running());
                    app.show_practice_with_origin(PracticeOrigin::Reminder)
                        .unwrap();
                    assert!(app.practice.borrow().is_none());
                    app.show_practice().unwrap();
                    assert!(
                        app.practice.borrow().is_some(),
                        "Manual practice works while paused"
                    );
                }
                4 => {
                    assert!(!app.feedback_timer.running(), "Unanswered cards stay open");
                    snapshot(
                        practice_window(app).window(),
                        checks.output.join("manual_during_pause.png"),
                    );
                    app.close_practice();
                    assert!(!app.reminder_timer.running());
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.set_current_tab(1);
                    assert_eq!(main.get_pause_minutes(), -1);
                    main.invoke_set_pause(0);
                    main.invoke_set_quiet_enabled(false);
                    main.invoke_set_interval(45);
                    main.invoke_set_feedback(12);
                    // Shorten only the isolated test deadline. Production
                    // input retains the one-minute minimum.
                    app.commit(|library| {
                        library.settings.pause = PracticePause::Timed {
                            until: chrono::Utc::now().timestamp() + 1,
                            minutes: 180,
                        };
                        Ok(())
                    })
                    .unwrap();
                    assert!(app.pause_timer.running());
                    assert!(!app.reminder_timer.running());
                }
                5..=7 => {
                    if app.pause_timer.running() {
                        assert!(!app.reminder_timer.running());
                    }
                    if phase == 7 {
                        let main = main_window(app);
                        assert_eq!(
                            main.get_pause_minutes(),
                            0,
                            "Expiry updates the open settings page"
                        );
                        assert_eq!(
                            app.store.load().unwrap().settings.pause,
                            PracticePause::None
                        );
                        assert_eq!(main.get_pause_display(), "不暂停");
                        assert!(!app.pause_timer.running());
                        main.invoke_go_background();
                        assert!(
                            app.reminder_timer.running(),
                            "Expiry resumes a fresh full interval"
                        );
                        use chrono::Timelike;
                        let now = chrono::Local::now();
                        let minute = (now.hour() * 60 + now.minute()) as u16;
                        app.set_quiet_range(
                            &format_time((minute + 1380) % 1440),
                            &format_time((minute + 60) % 1440),
                        );
                        app.set_quiet_enabled(true);
                        app.wait_for_reminder(
                            chrono::Local::now() + chrono::Duration::milliseconds(100),
                        );
                    }
                }
                8 => {
                    assert!(
                        app.practice.borrow().is_none(),
                        "Daily quiet hours suppress the real timer callback"
                    );
                    assert!(app.reminder_timer.running());
                    app.show_practice().unwrap();
                    assert!(app.practice.borrow().is_some());
                    app.close_practice();
                    app.set_quiet_enabled(false);
                    app.wait_for_reminder(
                        chrono::Local::now() + chrono::Duration::milliseconds(100),
                    );
                }
                9 => {
                    assert!(
                        app.practice.borrow().is_some(),
                        "Automatic practice resumes outside quiet hours"
                    );
                    app.close_practice();
                    app.set_pause(-1);
                    assert!(!app.reminder_timer.running());
                    assert!(!app.pause_timer.running());
                    assert!(!startup_file.exists());
                    checks.finished.set(true);
                    println!(
                        "Native dropdown, precise apply/cancel, custom values, pause expiry, quiet hours, isolated autostart and cleanup checks passed."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            settings_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}
#[test]
#[ignore = "opens real settings windows with an isolated startup registration"]
fn native_settings() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/settings_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let library = store.load().unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    let _ = app.autostart.replace(Ok(Autostart::isolated(
        profile.path().join("startup_entry"),
    )));
    app.start(false).unwrap();
    main_window(&app).set_current_tab(1);
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    settings_phase(checks.clone(), 0);
    let watchdog = Rc::downgrade(&checks.app);
    Timer::single_shot(Duration::from_secs(20), move || {
        if let Some(app) = watchdog.upgrade() {
            app.quit();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.finished.get(), "Settings check failed or timed out");
}

fn quick_answer_point(
    main: &MainWindow,
    row: usize,
    offset: Option<f32>,
) -> slint::LogicalPosition {
    slint::LogicalPosition::new(
        main.get_matrix_area_left() + main.get_matrix_area_width()
            - 3.0 * main.get_matrix_mode_width()
            - main.get_matrix_answer_width()
            + offset.unwrap_or(main.get_matrix_answer_width() / 2.0),
        main.get_question_area_top()
            + (0..row).map(|i| matrix_row_height(main, i)).sum::<f32>()
            + matrix_row_height(main, row) / 2.0,
    )
}

fn click_quick_answer(main: &MainWindow, row: usize, offset: Option<f32>) {
    let position = quick_answer_point(main, row, offset);
    main.window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    main.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    main.window().dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

fn replace_quick_input(main: &MainWindow, input: &str) {
    main.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    key(main.window(), "a".into());
    main.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    key(main.window(), input.into());
}

fn quick_answers_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            if phase == 7 {
                let card = practice_window(app);
                assert_eq!(card.get_answer_mode(), 2);
                let correct = (0..2)
                    .find(|index| {
                        app.practice
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .prepared
                            .accepts_choice(*index)
                            == Some(true)
                    })
                    .unwrap();
                click_option(&card, correct);
                assert_eq!(
                    card.get_feedback(),
                    1,
                    "Practice uses the newly configured judgment answer"
                );
                app.close_practice();
                app.show_main().unwrap();
                let main = main_window(app);
                main.invoke_toggle_knowledge_mode();
                main.window()
                    .set_size(slint::LogicalSize::new(720.0, 480.0));
                quick_answers_phase(checks.clone(), phase + 1);
                return;
            }
            let main = main_window(app);
            match phase {
                0 => {
                    assert!(
                        main.get_cards()
                            .row_data(0)
                            .unwrap()
                            .answer_preview
                            .formula_width
                            > 0.0
                    );
                    assert_eq!(main.get_cards().row_data(1).unwrap().answer_mode, 1);
                    assert_eq!(main.get_cards().row_data(2).unwrap().answer_mode, 2);
                    snapshot(main.window(), checks.output.join("quick_answers.png"));
                    click_quick_answer(&main, 0, None);
                    replace_quick_input(&main, "3/5");
                    key(main.window(), Key::Return.into());
                    assert!(app.store.load().unwrap().packs[0].cards[0].accepts("3/5"));
                    assert!(
                        !main.get_editor_visible(),
                        "Quick editing stays in the matrix"
                    );
                }
                1 => {
                    let saved = app.store.load().unwrap();
                    assert!(!saved.packs[0].cards[0].accepts("1/2"));
                    assert!(saved.packs[0].cards[0].aliases.is_empty());
                    replace_quick_input(&main, "sqrt(");
                    key(main.window(), Key::Return.into());
                    assert_eq!(
                        app.store.load().unwrap(),
                        saved,
                        "An invalid formula keeps the valid card"
                    );
                }
                2 => {
                    snapshot(main.window(), checks.output.join("quick_invalid.png"));
                    key(main.window(), Key::Escape.into());
                    replace_quick_input(&main, "7/9");
                    key(main.window(), Key::Tab.into());
                }
                3 => {
                    assert!(
                        app.store.load().unwrap().packs[0].cards[0].accepts("7/9"),
                        "Tab commits the answer before moving focus"
                    );
                    click_quick_answer(&main, 0, None);
                    replace_quick_input(&main, "5/7");
                    key(main.window(), Key::Escape.into());
                    assert!(
                        app.store.load().unwrap().packs[0].cards[0].accepts("7/9"),
                        "Esc cancels only the inline draft"
                    );
                    click_quick_answer(&main, 1, None);
                }
                4 => {
                    assert_eq!(main.get_quick_options().row_count(), 3);
                    assert!(main.get_quick_options().row_data(1).unwrap().formula_width > 0.0);
                    snapshot(main.window(), checks.output.join("quick_choices.png"));
                    key(main.window(), Key::DownArrow.into());
                    key(main.window(), Key::Return.into());
                }
                5 => {
                    let saved = app.store.load().unwrap();
                    let original = &r#try::test_fixtures::regression_library().packs[0].cards[1];
                    let card = &saved.packs[0].cards[1];
                    assert_eq!(card.answer, original.distractors[0].text);
                    assert_eq!(card.answer_latex, original.distractors[0].latex);
                    assert_eq!(card.distractors[0].text, original.answer);
                    assert_eq!(
                        saved.card_answer_mode("test_pack", card.id.as_deref().unwrap()),
                        AnswerMode::MultipleChoice
                    );
                    let truth = saved.packs[0].cards[2].true_false.as_ref().unwrap().is_true;
                    click_quick_answer(&main, 2, Some(if truth { 68.0 } else { 26.0 }));
                }
                6 => {
                    let saved = app.store.load().unwrap();
                    let original = r#try::test_fixtures::regression_library();
                    assert_ne!(
                        saved.packs[0].cards[2].true_false.as_ref().unwrap().is_true,
                        original.packs[0].cards[2]
                            .true_false
                            .as_ref()
                            .unwrap()
                            .is_true
                    );
                    let export = checks.output.join("quick_answers_export.json");
                    Store::export_pack(&export, &saved.packs[0]).unwrap();
                    assert_eq!(
                        KnowledgePack::from_json(&std::fs::read(export).unwrap()).unwrap(),
                        saved.packs[0]
                    );
                    snapshot(main.window(), checks.output.join("quick_saved.png"));
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(2, true);
                    app.tray.invoke_practice_now();
                }
                8 => {
                    assert!(main.get_matrix_answer_width() >= 84.0);
                    assert!(
                        main.get_matrix_area_width()
                            - 3.0 * main.get_matrix_mode_width()
                            - main.get_matrix_answer_width()
                            - main.get_participation_width()
                            > 100.0,
                        "Question and answer remain distinct at the minimum width"
                    );
                    snapshot(main.window(), checks.output.join("quick_compact.png"));
                    let before = app.store.load().unwrap();
                    assert!(
                        !main
                            .invoke_save_quick_answer(
                                "test_pack".into(),
                                "missing_card".into(),
                                "9/11".into()
                            )
                            .is_empty()
                    );
                    assert_eq!(
                        app.store.load().unwrap(),
                        before,
                        "A stale row identifier never edits another card"
                    );
                    main.invoke_toggle_knowledge_mode();
                    let id = main.get_cards().row_data(0).unwrap().id;
                    assert!(
                        main.invoke_save_quick_answer(
                            "test_pack".into(),
                            id,
                            "[[1,2],[3,4]]".into()
                        )
                        .is_empty()
                    );
                }
                9 => {
                    assert!(
                        main.get_cards()
                            .row_data(0)
                            .unwrap()
                            .answer_preview
                            .formula_height
                            > 30.0
                    );
                    snapshot(main.window(), checks.output.join("quick_tall_formula.png"));
                    checks.finished.set(true);
                    app.quit();
                    println!(
                        "Native inline formula editing, Enter/Tab saving, Esc cancellation, invalid draft protection, choice/truth selection, export, practice and minimum-width layout passed."
                    );
                    return;
                }
                _ => unreachable!(),
            }
            quick_answers_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens real matrix windows with isolated question data"]
fn native_quick_answers() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/quick_answers_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    let pack_id = library.packs[0].id.clone();
    let choice_id = library.packs[0].cards[1].id.clone().unwrap();
    let truth_id = library.packs[0].cards[2].id.clone().unwrap();
    library
        .set_card_answer_mode(&pack_id, &choice_id, AnswerMode::MultipleChoice)
        .unwrap();
    library
        .set_card_answer_mode(&pack_id, &truth_id, AnswerMode::TrueFalse)
        .unwrap();
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    quick_answers_phase(checks.clone(), 0);
    let watchdog = Rc::downgrade(&checks.app);
    Timer::single_shot(Duration::from_secs(20), move || {
        if let Some(app) = watchdog.upgrade() {
            app.quit();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Quick answer check failed or timed out"
    );
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmGetWindowAttribute(
        window: HWND,
        attribute: u32,
        value: *mut std::ffi::c_void,
        size: u32,
    ) -> i32;
}

static SYSTEM_THEME_DARK: AtomicBool = AtomicBool::new(false);

fn system_theme_fixture() -> bool {
    SYSTEM_THEME_DARK.load(Ordering::SeqCst)
}

fn simulate_system_theme(app: &Rc<App>, dark: Option<bool>) {
    if let Some(dark) = dark {
        SYSTEM_THEME_DARK.store(dark, Ordering::SeqCst);
        app.system_theme_probe.set(system_theme_fixture);
    } else {
        app.system_theme_probe.set(FloatingWindow::system_dark);
    }
    let window = app
        .main_window
        .borrow()
        .as_ref()
        .map(|window| native_handle(window.window()))
        .or_else(|| {
            app.practice
                .borrow()
                .as_ref()
                .map(|session| native_handle(session.window.window()))
        })
        .expect("The test has a visible native window");
    // SAFETY: this sends a setting notification only to this test's owned window.
    unsafe {
        SendMessageW(
            window,
            windows_sys::Win32::UI::WindowsAndMessaging::WM_SETTINGCHANGE,
            0,
            0,
        );
    }
}

fn assert_titlebar_theme(window: &slint::Window, dark: bool) {
    let mut actual = 0i32;
    // SAFETY: this handle belongs to the live test window and actual is a writable BOOL.
    let result = unsafe {
        DwmGetWindowAttribute(
            native_handle(window),
            20, // DWMWA_USE_IMMERSIVE_DARK_MODE
            (&raw mut actual).cast(),
            std::mem::size_of_val(&actual) as u32,
        )
    };
    assert_eq!(result, 0, "The native titlebar theme must be readable");
    assert_eq!(
        actual != 0,
        dark,
        "Native titlebar follows the app appearance"
    );
}

fn desktop_theme_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(400), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    assert!(main.get_dark_theme());
                    assert_titlebar_theme(main.window(), true);
                    assert_eq!(main.get_cards().row_count(), 4);
                    assert!(main.get_question_area_height() > 190.0);
                    assert!(
                        main.get_question_area_height() < 300.0,
                        "Small packs leave space below the inset panel"
                    );
                    snapshot(main.window(), checks.output.join("dark_main.png"));
                    main.set_current_tab(1);
                }
                1 => {
                    let main = main_window(app);
                    main.window().dispatch_event(WindowEvent::PointerScrolled {
                        position: slint::LogicalPosition::new(
                            main.get_settings_left() + 100.0,
                            main.get_settings_top() + 100.0,
                        ),
                        delta_x: 0.0,
                        delta_y: -600.0,
                    });
                }
                2 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("dark_settings.png"));
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_appearance_left() + 40.0,
                            main.get_appearance_top() + 18.0,
                        ),
                    );
                }
                3 => {
                    let main = main_window(app);
                    key(main.window(), Key::UpArrow.into());
                    key(main.window(), Key::Return.into());
                    assert!(
                        !main.get_dark_theme(),
                        "The real appearance menu switches to light"
                    );
                    assert_eq!(
                        app.store.load().unwrap().settings.appearance,
                        Appearance::Light
                    );
                    let original: Vec<KnowledgePack> = serde_json::from_slice(
                        &std::fs::read(checks.output.join("original_packs.json")).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(
                        app.store.load().unwrap().packs,
                        original,
                        "Appearance never rewrites pack content"
                    );
                    snapshot(main.window(), checks.output.join("light_settings.png"));
                    main.set_current_tab(0);
                }
                4 => {
                    let main = main_window(app);
                    let buffer = main.window().take_snapshot().unwrap();
                    let offset = ((buffer.height() - 3) as usize * buffer.width() as usize
                        + buffer.width() as usize
                        - 3)
                        * 4;
                    let pixel = &buffer.as_bytes()[offset..offset + 3];
                    assert!(
                        pixel.iter().all(|channel| *channel > 230),
                        "The actual canvas repaints in light colors"
                    );
                    snapshot(main.window(), checks.output.join("light_main.png"));
                    assert_titlebar_theme(main.window(), false);
                    main.invoke_choose_language(2);
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                }
                5 => {
                    let main = main_window(app);
                    assert!(main.get_matrix_mode_width() >= 36.0);
                    assert!(main.get_matrix_answer_width() >= 84.0);
                    assert!(
                        main.get_matrix_area_width()
                            - 3.0 * main.get_matrix_mode_width()
                            - main.get_matrix_answer_width()
                            - main.get_participation_width()
                            > 150.0
                    );
                    snapshot(
                        main.window(),
                        checks.output.join("light_main_en_narrow.png"),
                    );
                    app.open_editor(Some(0), false, None);
                }
                6 => {
                    let main = main_window(app);
                    snapshot(
                        main.window(),
                        checks.output.join("light_formula_editor.png"),
                    );
                    main.invoke_cancel_editor();
                    app.close_main();
                    app.show_main().unwrap();
                    assert!(
                        !main_window(app).get_dark_theme(),
                        "Recreated windows keep the saved appearance"
                    );
                }
                7 => {
                    assert_titlebar_theme(main_window(app).window(), false);
                    starter_question(app, "math_recall", 0);
                }
                8 => {
                    let card = practice_window(app);
                    assert!(!card.get_dark_theme());
                    snapshot(card.window(), checks.output.join("light_practice.png"));
                    card.invoke_submit_answer("0".into());
                    assert_eq!(card.get_feedback(), 2);
                }
                9 => {
                    snapshot(
                        practice_window(app).window(),
                        checks.output.join("light_wrong.png"),
                    );
                    app.close_practice();
                    app.show_main().unwrap();
                    main_window(app).invoke_choose_appearance(1);
                    starter_question(app, "programming_recall", 0);
                }
                10 => {
                    let card = practice_window(app);
                    assert!(card.get_dark_theme());
                    snapshot(card.window(), checks.output.join("dark_practice.png"));
                    let answer = app
                        .practice
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .question
                        .card
                        .answer
                        .clone();
                    key(card.window(), answer.into());
                    key(card.window(), Key::Return.into());
                    assert_eq!(card.get_feedback(), 1);
                }
                11 => {
                    snapshot(
                        practice_window(app).window(),
                        checks.output.join("dark_correct.png"),
                    );
                    app.close_practice();
                    app.show_main().unwrap();
                    app.commit(|library| {
                        let source = library.packs[0].cards.clone();
                        library.packs[0].cards = (0..24)
                            .map(|index| {
                                let mut card = source[index % source.len()].clone();
                                card.id = Some(format!("layout_probe_{index}"));
                                card
                            })
                            .collect();
                        let id = library.packs[0].id.clone();
                        library.settings.excluded_card_ids.remove(&id);
                        library.settings.card_answer_modes.remove(&id);
                        Ok(())
                    })
                    .unwrap();
                    let main = main_window(app);
                    main.invoke_choose_appearance(0);
                    main.invoke_select_pack(0);
                    main.invoke_toggle_pack("math_recall".into(), true);
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                }
                12 => {
                    let main = main_window(app);
                    assert_titlebar_theme(main.window(), false);
                    snapshot(main.window(), checks.output.join("light_many_top.png"));
                    std::fs::write(
                        checks.output.join("matrix_top.json"),
                        serde_json::to_vec(&main.get_matrix_area_top()).unwrap(),
                    )
                    .unwrap();
                    main.window().dispatch_event(WindowEvent::PointerScrolled {
                        position: slint::LogicalPosition::new(
                            main.get_question_area_left() + 100.0,
                            main.get_question_area_top() + 100.0,
                        ),
                        delta_x: 0.0,
                        delta_y: -2000.0,
                    });
                }
                13 => {
                    let main = main_window(app);
                    assert!(main.get_question_scroll_y() < -300.0);
                    let top: f32 = serde_json::from_slice(
                        &std::fs::read(checks.output.join("matrix_top.json")).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(
                        main.get_matrix_area_top(),
                        top,
                        "Header and all-card controls remain fixed while rows scroll"
                    );
                    snapshot(main.window(), checks.output.join("light_many_bottom.png"));
                    main.invoke_select_pack(1);
                }
                14 => {
                    let main = main_window(app);
                    assert_eq!(main.get_cards().row_count(), 4);
                    assert!(
                        main.get_question_area_height() < 300.0,
                        "Switching to a small pack shrinks the panel again"
                    );
                    snapshot(main.window(), checks.output.join("light_small_again.png"));
                    let before = app.store.load().unwrap();
                    main.invoke_choose_appearance(99);
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_choose_appearance(Appearance::Dark.index());
                }
                15 => {
                    let main = main_window(app);
                    assert!(main.get_dark_theme());
                    assert_titlebar_theme(main.window(), true);
                    simulate_system_theme(app, Some(false));
                    main.set_current_tab(1);
                    scroll_settings_to_bottom(&main);
                }
                16 => {
                    let main = main_window(app);
                    assert!(
                        main.get_dark_theme(),
                        "A manual theme ignores a system color change"
                    );
                    assert_titlebar_theme(main.window(), true);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_appearance_left() + 40.0,
                            main.get_appearance_top() + 18.0,
                        ),
                    );
                }
                17 => {
                    let main = main_window(app);
                    key(main.window(), Key::DownArrow.into());
                    key(main.window(), Key::Return.into());
                    assert_eq!(
                        main.get_appearance(),
                        Appearance::System.index(),
                        "The actual menu includes System"
                    );
                    assert_eq!(
                        app.store.load().unwrap().settings.appearance,
                        Appearance::System
                    );
                }
                18 => {
                    let main = main_window(app);
                    assert!(!main.get_dark_theme());
                    assert_titlebar_theme(main.window(), false);
                    snapshot(main.window(), checks.output.join("system_light.png"));
                    std::fs::write(
                        checks.output.join("before_system_change.json"),
                        std::fs::read(app.store.directory().join("state.json")).unwrap(),
                    )
                    .unwrap();
                    // Change only this test's backend palette source, never the user's OS settings.
                    simulate_system_theme(app, Some(true));
                }
                19 => {
                    let main = main_window(app);
                    assert!(
                        main.get_dark_theme(),
                        "System updates the visible client area"
                    );
                    assert_titlebar_theme(main.window(), true);
                    snapshot(main.window(), checks.output.join("system_dark.png"));
                    assert_eq!(
                        std::fs::read(app.store.directory().join("state.json")).unwrap(),
                        std::fs::read(checks.output.join("before_system_change.json")).unwrap(),
                        "System changes do not rewrite settings or packs"
                    );
                    app.close_main();
                    app.show_main().unwrap();
                }
                20 => {
                    let main = main_window(app);
                    assert_eq!(main.get_appearance(), Appearance::System.index());
                    assert!(main.get_dark_theme());
                    assert_titlebar_theme(main.window(), true);
                    starter_question(app, "math_recall", 0);
                }
                21 => {
                    assert!(practice_window(app).get_dark_theme());
                    simulate_system_theme(app, Some(false));
                }
                22 => {
                    let card = practice_window(app);
                    assert!(
                        !card.get_dark_theme(),
                        "An already visible practice card follows System"
                    );
                    assert_titlebar_theme(card.window(), false);
                    snapshot(
                        card.window(),
                        checks.output.join("system_practice_light.png"),
                    );
                    app.close_practice();
                    app.show_main().unwrap();
                }
                23 => {
                    let main = main_window(app);
                    assert!(!main.get_dark_theme());
                    assert_titlebar_theme(main.window(), false);
                    simulate_system_theme(app, None);
                }
                24 => {
                    let main = main_window(app);
                    assert_eq!(
                        main.get_dark_theme(),
                        FloatingWindow::system_dark(),
                        "Resetting the fixture returns to the actual backend theme"
                    );
                    assert_titlebar_theme(main.window(), main.get_dark_theme());
                    std::fs::write(checks.output.join("result.txt"), "Native appearance menu, persisted System default, DWM titlebar attributes, both manual directions, simulated backend changes, manual override, unchanged data, window recreation, visible practice updates, bilingual/minimum layout and fixed-header scrolling passed.").unwrap();
                    checks.finished.set(true);
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            desktop_theme_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens isolated real windows to verify desktop layout and appearance"]
fn native_desktop_theme() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/desktop_theme_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = Store::open(profile.path().to_owned()).unwrap();
    let mut library = store.load().unwrap();
    library.settings.appearance = Appearance::Dark;
    library.settings.auto_check_updates = false;
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.interface_language = InterfaceLanguage::Chinese;
    store.save(&library).unwrap();
    std::fs::write(
        output.join("original_packs.json"),
        serde_json::to_vec(&library.packs).unwrap(),
    )
    .unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    desktop_theme_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Desktop appearance check failed or timed out"
    );
}

fn pack_tags_phase(checks: Rc<AnswerChecks>, phase: u8, hidden_height: f32) {
    Timer::single_shot(Duration::from_millis(250), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let mut baseline = hidden_height;
            match phase {
                0 => {
                    let main = main_window(app);
                    main.invoke_begin_tag_edit();
                    assert!(!main.get_tray_settings_available());
                    main.set_tag_draft("  数学基础  ".into());
                    main.invoke_save_pack_tag();
                    assert!(!main.get_tag_editing());
                    assert_eq!(main.get_selected_tag(), "数学基础");
                    assert_eq!(main.get_packs().row_data(0).unwrap().tag, "数学基础");
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[0].tag, "数学基础");
                    Store::export_pack(&checks.output.join("tagged.json"), &saved.packs[0])
                        .unwrap();
                    assert_eq!(
                        Store::read_pack(&checks.output.join("tagged.json")).unwrap(),
                        saved.packs[0]
                    );
                    snapshot(main.window(), checks.output.join("pack_metadata.png"));
                    main.invoke_begin_tag_edit();
                }
                1 => {
                    let main = main_window(app);
                    let before = app.store.load().unwrap();
                    main.set_tag_draft("字".repeat(25).into());
                    main.invoke_save_pack_tag();
                    assert!(main.get_tag_editing());
                    assert!(!main.get_metadata_error().is_empty());
                    assert_eq!(app.store.load().unwrap(), before);
                    key(main.window(), Key::Escape.into());
                }
                2 => {
                    let main = main_window(app);
                    assert!(!main.get_tag_editing(), "Esc cancels the invalid tag draft");
                    main.invoke_set_show_pack_tags(false);
                    assert!(!app.store.load().unwrap().settings.show_pack_tags);
                    app.close_main();
                    app.show_main().unwrap();
                    assert!(!main_window(app).get_show_pack_tags());
                    assert_eq!(
                        main_window(app).get_packs().row_data(0).unwrap().tag,
                        "数学基础"
                    );
                    snapshot(
                        main_window(app).window(),
                        checks.output.join("sidebar_hidden.png"),
                    );
                    app.show_practice().unwrap();
                }
                3 => {
                    let card = practice_window(app);
                    assert!(!card.get_pack_tag_visible());
                    assert_eq!(card.get_pack_tag(), "数学基础");
                    baseline = card
                        .window()
                        .size()
                        .to_logical(card.window().scale_factor())
                        .height;
                    snapshot(card.window(), checks.output.join("hidden_dark.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    main_window(app).invoke_set_show_pack_tags(true);
                    app.show_practice().unwrap();
                }
                4 => {
                    let card = practice_window(app);
                    assert!(card.get_pack_tag_visible());
                    assert!(
                        card.get_tag_on_right(),
                        "Left-hand card uses the right tag corner"
                    );
                    let height = card
                        .window()
                        .size()
                        .to_logical(card.window().scale_factor())
                        .height;
                    assert!((height - baseline - 26.0).abs() < 1.0);
                    assert!(card.get_pack_tag_left() > 160.0);
                    snapshot(card.window(), checks.output.join("dark_left.png"));
                    card.window().with_winit_window(|native| {
                        let monitor = native.current_monitor().unwrap();
                        let position = native.outer_position().unwrap();
                        native.set_outer_position(
                            slint::winit_030::winit::dpi::PhysicalPosition::new(
                                monitor.position().x + monitor.size().width as i32
                                    - native.outer_size().width as i32
                                    - 48,
                                position.y,
                            ),
                        );
                    });
                }
                5 => {
                    let card = practice_window(app);
                    assert!(
                        !card.get_tag_on_right(),
                        "Native move switches the tag to the left corner"
                    );
                    assert_eq!(card.get_pack_tag_left(), 24.0);
                    snapshot(card.window(), checks.output.join("dark_right.png"));
                    card.invoke_submit_answer("wrong".into());
                }
                6 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_feedback(), 2);
                    assert!(card.get_pack_tag_visible());
                    snapshot(card.window(), checks.output.join("dark_wrong.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_choose_appearance(Appearance::Light.index());
                    main.invoke_set_corner(PopupCorner::TopRight.index());
                    app.knowledge_only.set(true);
                    app.show_practice().unwrap();
                }
                7 => {
                    let card = practice_window(app);
                    assert!(!card.get_dark_theme());
                    assert!(card.get_knowledge_only());
                    assert!(!card.get_tag_on_right());
                    snapshot(card.window(), checks.output.join("light_knowledge.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_begin_tag_edit();
                    main.set_tag_draft("".into());
                    main.invoke_save_pack_tag();
                    assert!(app.store.load().unwrap().packs[0].tag.is_empty());
                    assert!(main.get_packs().row_data(0).unwrap().tag.is_empty());
                    app.show_practice().unwrap();
                }
                8 => {
                    let card = practice_window(app);
                    assert!(card.get_pack_tag().is_empty());
                    assert!(!card.get_pack_tag_visible());
                    snapshot(card.window(), checks.output.join("empty_tag.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_begin_tag_edit();
                    main.set_tag_draft("数学".repeat(12).into());
                    main.invoke_save_pack_tag();
                    main.invoke_choose_language(InterfaceLanguage::English.index());
                    assert_eq!(main.get_packs().row_data(0).unwrap().tag, "数学".repeat(12));
                    snapshot(main.window(), checks.output.join("english_metadata.png"));
                    app.show_practice().unwrap();
                }
                9 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_pack_tag(), "数学".repeat(12));
                    assert!(card.get_pack_tag_width() <= 138.0);
                    assert!(card.get_pack_tag_visible());
                    snapshot(card.window(), checks.output.join("light_long_tag.png"));
                    std::fs::write(checks.output.join("result.txt"), "Native pack metadata editing, invalid-draft cancellation, persisted visibility, export, 26px header, moved-event inward alignment, dark feedback, light fact viewing, blank tags and long-tag width passed.").unwrap();
                    checks.finished.set(true);
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            pack_tags_phase(checks.clone(), phase + 1, baseline);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens isolated real windows to verify knowledge-pack tags"]
fn native_pack_tags() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/pack_tag_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = Store::open(profile.path().to_owned()).unwrap();
    let mut library = store.load().unwrap();
    let pack_id = library.packs[0].id.clone();
    let card_id = library.packs[0].cards[0].id.clone().unwrap();
    library.settings.enabled_pack_ids = vec![pack_id.clone()];
    library.set_all_cards_enabled(&pack_id, false).unwrap();
    library.set_card_enabled(&pack_id, &card_id, true).unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.interface_language = InterfaceLanguage::Chinese;
    library.settings.popup_corner = PopupCorner::BottomLeft;
    library.settings.remember_position = false;
    library.settings.feedback_seconds = 30;
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
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
    let app = App::new(store, library, floating).unwrap();
    app.start(false).unwrap();
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    pack_tags_phase(checks.clone(), 0, 0.0);
    let watchdog = Rc::downgrade(&checks.app);
    Timer::single_shot(Duration::from_secs(20), move || {
        if let Some(app) = watchdog.upgrade() {
            app.quit();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Knowledge-pack tag check failed or timed out"
    );
}
