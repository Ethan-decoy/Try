//! Explicit native checks for focus.
use super::*;

struct PracticeFocusChecks {
    app: Rc<App>,
    probe: FocusProbe,
    output: PathBuf,
    finished: Cell<bool>,
    pointer_events: Rc<Cell<u32>>,
    cursor_before: POINT,
}

fn practice_focus_phase(checks: Rc<PracticeFocusChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let ready = checks.clone();
                    focus_probe_with_pointer(&checks.probe, move || {
                        ready
                            .app
                            .show_practice_with_origin(PracticeOrigin::Reminder)
                            .unwrap();
                        practice_focus_phase(ready, 1);
                    });
                }
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
                    let answer = window.get_answer_text();
                    assert!(
                        answer == "37",
                        "Direct keyboard input reaches the answer field (chars={}, digit positions={:?}/{:?})",
                        answer.chars().count(),
                        answer.find('3'),
                        answer.find('7')
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
                    checks.pointer_events.set(0);
                    checks.probe.show().unwrap();
                    checks.probe.set_input_text("probe".into());
                    checks.probe.invoke_focus_input();
                    FloatingWindow::activate(checks.probe.window());
                    app.show_practice_with_origin(PracticeOrigin::Reminder)
                        .unwrap();
                    assert!(app.practice.borrow().as_ref().unwrap().awaiting_interaction);
                }
                5 => {
                    let window = practice_window(app);
                    let (x, y) = FloatingWindow::position(window.window()).unwrap();
                    checks.probe.window().with_winit_window(|native| {
                        native.set_outer_position(
                            slint::winit_030::winit::dpi::PhysicalPosition::new(x - 120, y - 80),
                        );
                        native.set_cursor_visible(false);
                    });
                    arrange_input_windows(&checks.probe, &window);
                    assert_eq!(
                        unsafe { GetForegroundWindow() },
                        native_handle(checks.probe.window())
                    );
                    // Cursor visibility is a client-area property. An outer
                    // window offset can land in the caption and leave it visible.
                    // Start inside our probe's client area, to the left of the card.
                    let mut point = POINT { x: 12, y: 12 };
                    unsafe {
                        assert_ne!(
                            ClientToScreen(native_handle(checks.probe.window()), &mut point),
                            0
                        );
                        assert!(point.x < x);
                        assert_eq!(WindowFromPoint(point), native_handle(checks.probe.window()));
                        assert_ne!(SetCursorPos(point.x, point.y), 0);
                    }
                }
                6 => {
                    type_into_foreground(native_handle(checks.probe.window()), "X");
                    let window = practice_window(app);
                    let (actual, expected) = header_colors(window.window());
                    assert_eq!(
                        actual, expected,
                        "A click-through reminder remains visibly rendered"
                    );
                    let mut rect = RECT::default();
                    // SAFETY: both handles belong to the isolated test. Check
                    // the actual OS hit target before sending any mouse input.
                    unsafe {
                        let mut cursor = CURSORINFO {
                            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
                            ..Default::default()
                        };
                        assert_ne!(GetCursorInfo(&mut cursor), 0);
                        assert_eq!(
                            cursor.flags & CURSOR_SHOWING,
                            0,
                            "The simulated game really hides the system cursor"
                        );
                        assert_ne!(GetWindowRect(native_handle(window.window()), &mut rect), 0);
                        let point = POINT {
                            x: (rect.left + rect.right) / 2,
                            y: (rect.top + rect.bottom) / 2,
                        };
                        assert_eq!(
                            WindowFromPoint(point),
                            native_handle(checks.probe.window()),
                            "Mouse input must pass through the scheduled card to the window below"
                        );
                        assert!(GetAsyncKeyState(VK_LBUTTON as i32) >= 0);
                        assert_ne!(SetCursorPos(point.x, point.y), 0);
                        let mut actual = POINT::default();
                        assert_ne!(GetCursorPos(&mut actual), 0);
                        assert_eq!((actual.x, actual.y), (point.x, point.y));
                        assert_eq!(
                            WindowFromPoint(actual),
                            native_handle(checks.probe.window())
                        );
                        assert_eq!(GetForegroundWindow(), native_handle(checks.probe.window()));
                    }
                    mouse_button(MOUSEEVENTF_LEFTDOWN);
                    mouse_button(MOUSEEVENTF_LEFTUP);
                    snapshot(
                        window.window(),
                        checks.output.join("background_reminder.png"),
                    );
                }
                7 => {
                    // SAFETY: restore the pointer after clicking only our test window.
                    unsafe {
                        SetCursorPos(checks.cursor_before.x, checks.cursor_before.y);
                    }
                    assert_eq!(
                        checks.pointer_events.get(),
                        1,
                        "The underlying window receives the actual click"
                    );
                    assert!(
                        checks.probe.get_input_text().contains('X'),
                        "Disabling automatic focus retains input in the other window"
                    );
                    assert!(practice_window(app).get_answer_text().is_empty());
                    let window = practice_window(app);
                    window.invoke_submit_answer("wrong".into());
                    assert_eq!(
                        window.get_feedback(),
                        0,
                        "Unactivated reminders do not record an answer"
                    );
                    assert!(
                        app.practice
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .review_token
                            .is_none()
                    );
                    checks
                        .probe
                        .window()
                        .with_winit_window(|native| native.set_cursor_visible(true));
                }
                8 => {
                    let window = practice_window(app);
                    let (actual, expected) = header_colors(window.window());
                    assert_eq!(
                        actual, expected,
                        "Releasing the cursor keeps the native frame clean"
                    );
                    let mut rect = RECT::default();
                    unsafe {
                        assert_ne!(GetWindowRect(native_handle(window.window()), &mut rect), 0);
                        assert_eq!(
                            WindowFromPoint(POINT {
                                x: (rect.left + rect.right) / 2,
                                y: (rect.top + rect.bottom) / 2,
                            }),
                            native_handle(window.window()),
                            "The visible free cursor can click the existing card"
                        );
                        assert_eq!(GetForegroundWindow(), native_handle(checks.probe.window()));
                        assert_ne!(
                            SetCursorPos(
                                (rect.left + rect.right) / 2,
                                (rect.top + rect.bottom) / 2
                            ),
                            0
                        );
                    }
                    type_into_foreground(native_handle(checks.probe.window()), "Y");
                }
                9 => {
                    assert_eq!(
                        unsafe { GetForegroundWindow() },
                        native_handle(checks.probe.window()),
                        "Hovering over the card must not activate it"
                    );
                    assert!(checks.probe.get_input_text().contains('Y'));
                    assert!(practice_window(app).get_answer_text().is_empty());
                    assert!(app.practice.borrow().as_ref().unwrap().awaiting_interaction);
                    assert!(
                        app.practice
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .review_token
                            .is_none()
                    );
                    let window = practice_window(app);
                    let mut rect = RECT::default();
                    unsafe {
                        assert_ne!(GetWindowRect(native_handle(window.window()), &mut rect), 0);
                        let x = (rect.left + rect.right) / 2;
                        let y = (rect.top + rect.bottom) / 2;
                        // Windows may hide the pointer while the preceding Y
                        // is typed. Moving it models releasing it again before
                        // hovering or clicking; let the input guard observe it.
                        assert_ne!(SetCursorPos(x + 1, y), 0);
                        assert_ne!(SetCursorPos(x, y), 0);
                    }
                    let ready = checks.clone();
                    Timer::single_shot(Duration::from_millis(80), move || {
                        let window = practice_window(&ready.app);
                        let mut point = POINT::default();
                        let mut cursor = CURSORINFO {
                            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
                            ..Default::default()
                        };
                        unsafe {
                            assert_ne!(GetCursorInfo(&mut cursor), 0);
                            assert_ne!(cursor.flags & CURSOR_SHOWING, 0);
                            assert_ne!(GetCursorPos(&mut point), 0);
                            assert_eq!(WindowFromPoint(point), native_handle(window.window()));
                            assert_eq!(GetForegroundWindow(), native_handle(ready.probe.window()));
                        }
                        mouse_button(MOUSEEVENTF_LEFTDOWN);
                        mouse_button(MOUSEEVENTF_LEFTUP);
                        practice_focus_phase(ready, 10);
                    });
                }
                10 => {
                    let window = practice_window(app);
                    assert!(!app.practice.borrow().as_ref().unwrap().awaiting_interaction);
                    assert!(
                        app.practice
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .review_token
                            .is_some()
                    );
                    type_into_foreground(native_handle(window.window()), "9");
                }
                11 => {
                    assert!(
                        practice_window(app).get_answer_text() == "9",
                        "Clicking the card directly restores keyboard input"
                    );
                    FloatingWindow::activate(checks.probe.window());
                    unsafe {
                        SetCapture(native_handle(checks.probe.window()));
                    }
                    arrange_input_windows(&checks.probe, &practice_window(app));
                }
                12 => {
                    let window = practice_window(app);
                    let mut rect = RECT::default();
                    unsafe {
                        assert_ne!(GetWindowRect(native_handle(window.window()), &mut rect), 0);
                        assert_eq!(
                            WindowFromPoint(POINT {
                                x: (rect.left + rect.right) / 2,
                                y: (rect.top + rect.bottom) / 2
                            }),
                            native_handle(checks.probe.window()),
                            "A foreground mouse capture makes the card pass through again"
                        );
                        assert_ne!(ReleaseCapture(), 0);
                    }
                }
                13 => {
                    let window = practice_window(app);
                    let mut rect = RECT::default();
                    unsafe {
                        assert_ne!(GetWindowRect(native_handle(window.window()), &mut rect), 0);
                        assert_eq!(
                            WindowFromPoint(POINT {
                                x: (rect.left + rect.right) / 2,
                                y: (rect.top + rect.bottom) / 2
                            }),
                            native_handle(window.window()),
                            "Releasing capture restores clickability without recreating the card"
                        );
                        assert_eq!(GetForegroundWindow(), native_handle(checks.probe.window()));
                    }
                    app.show_practice().unwrap();
                }
                14 => {
                    assert_eq!(
                        unsafe { GetForegroundWindow() },
                        native_handle(practice_window(app).window()),
                        "The system tray can still explicitly activate the card"
                    );
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_set_all_answer_modes(1);
                    app.close_main();
                    FloatingWindow::activate(checks.probe.window());
                    app.show_practice_with_origin(PracticeOrigin::Reminder)
                        .unwrap();
                }
                15 => {
                    let window = practice_window(app);
                    assert!(app.practice.borrow().as_ref().unwrap().awaiting_interaction);
                    assert_eq!(
                        unsafe { GetForegroundWindow() },
                        native_handle(checks.probe.window())
                    );
                    let scale = window.get_pixel_scale();
                    let mut point = POINT {
                        x: (72.0 * scale) as i32,
                        y: ((window.get_answer_footer_top() - 24.0) * scale) as i32,
                    };
                    unsafe {
                        assert_ne!(
                            ClientToScreen(native_handle(window.window()), &mut point),
                            0
                        );
                        assert_eq!(WindowFromPoint(point), native_handle(window.window()));
                        assert_ne!(SetCursorPos(point.x, point.y), 0);
                    }
                    mouse_button(MOUSEEVENTF_LEFTDOWN);
                    mouse_button(MOUSEEVENTF_LEFTUP);
                }
                16 => {
                    assert!(
                        app.practice.borrow().as_ref().unwrap().answered,
                        "The first click both activates and answers a multiple-choice card"
                    );
                    assert!(!app.practice.borrow().as_ref().unwrap().awaiting_interaction);
                    let pack_id = app
                        .practice
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .question
                        .key
                        .pack_id
                        .clone();
                    assert_eq!(
                        app.learner
                            .borrow()
                            .pack_summary(
                                &app.library.borrow(),
                                &pack_id,
                                chrono::Utc::now().timestamp()
                            )
                            .effective_reviews,
                        1,
                        "The first click records exactly one review"
                    );
                    assert_eq!(practice_window(app).get_selected_option(), 0);
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_set_focus_on_reminder(true);
                    assert!(app.store.load().unwrap().settings.focus_on_reminder);
                    app.close_main();
                    FloatingWindow::activate(checks.probe.window());
                    app.show_practice_with_origin(PracticeOrigin::Reminder)
                        .unwrap();
                }
                17 => type_into_foreground(native_handle(practice_window(app).window()), "1"),
                18 => {
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
                19 => type_into_foreground(native_handle(practice_window(app).window()), "1"),
                20 => {
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
                21 => {
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
                    app.close_practice();
                    app.reminder_environment.set(|_| false);
                    let previous = app.previous_question.borrow().clone();
                    let next_id = app.next_practice_id.get();
                    app.show_practice_with_origin(PracticeOrigin::Reminder)
                        .unwrap();
                    assert!(
                        app.practice.borrow().is_none(),
                        "Blocked environments defer reminders"
                    );
                    assert!(app.reminder_timer.running());
                    assert_eq!(*app.previous_question.borrow(), previous);
                    assert_eq!(
                        app.next_practice_id.get(),
                        next_id,
                        "A deferred reminder does not select or present a question"
                    );
                    app.show_practice().unwrap();
                }
                22 => {
                    let window = practice_window(app);
                    assert_eq!(
                        unsafe { GetForegroundWindow() },
                        native_handle(window.window()),
                        "Explicit manual practice is allowed even while reminders are blocked"
                    );
                    app.close_practice();
                    app.reminder_environment.set(|_| true);
                    FloatingWindow::activate(checks.probe.window());
                    // Exercise the real timer error path without eligible content.
                    // It must log the failure instead of opening a modal dialog.
                    app.library.borrow_mut().packs.clear();
                    app.wait_for_reminder(
                        chrono::Local::now() + chrono::Duration::milliseconds(30),
                    );
                }
                23 => {
                    assert!(app.practice.borrow().is_none());
                    assert!(!app.reminder_timer.running());
                    assert_eq!(
                        unsafe { GetForegroundWindow() },
                        native_handle(checks.probe.window()),
                        "Automatic reminder errors must not steal foreground focus"
                    );
                    std::fs::write(checks.output.join("background_result.txt"),
                        "Background cards stay visible without stealing input; hidden cursor and mouse capture pass clicks through; releasing the cursor restores clickability; hovering does not activate; clicking the card activates and records presentation; the tray remains available; blocked environments defer and timer errors stay quiet.").unwrap();
                    checks.finished.set(true);
                    checks.probe.hide().unwrap();
                    app.quit();
                }
                _ => unreachable!(),
            }
        }));
        if result.is_err() {
            checks
                .probe
                .window()
                .with_winit_window(|native| native.set_cursor_visible(true));
            unsafe {
                ReleaseCapture();
                SetCursorPos(checks.cursor_before.x, checks.cursor_before.y);
            }
            let _ = checks.probe.hide();
            checks.app.quit();
        } else if phase > 0 && phase != 9 && phase < 23 {
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
    app.reminder_environment.set(|_| true);
    app.start(true).unwrap();
    let mut cursor_before = POINT::default();
    unsafe {
        assert_ne!(GetCursorPos(&mut cursor_before), 0);
    }
    let probe = FocusProbe::new().unwrap();
    let pointer_events = Rc::new(Cell::new(0));
    let pointer_count = pointer_events.clone();
    probe.window().on_winit_window_event(move |_, event| {
        if matches!(
            event,
            slint::winit_030::winit::event::WindowEvent::MouseInput {
                state: slint::winit_030::winit::event::ElementState::Pressed,
                button: slint::winit_030::winit::event::MouseButton::Left,
                ..
            }
        ) {
            pointer_count.set(pointer_count.get() + 1);
        }
        slint::winit_030::EventResult::Propagate
    });
    let checks = Rc::new(PracticeFocusChecks {
        app,
        probe,
        output,
        finished: Cell::new(false),
        pointer_events,
        cursor_before,
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
    unsafe {
        SetCursorPos(cursor_before.x, cursor_before.y);
    }
    assert!(
        checks.finished.get(),
        "Automatic practice focus check did not complete"
    );
}
