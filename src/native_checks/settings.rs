//! Explicit native checks for settings.
use super::*;

fn settings_click(main: &MainWindow, right: f32, top: f32) {
    pointer_click(
        main.window(),
        slint::LogicalPosition::new(
            main.get_settings_left() + main.get_settings_width() - right,
            main.get_settings_content_top() + top,
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
                    main.set_settings_section(2);
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
                    settings_click(&main, 120.0, 24.0);
                    key(main.window(), Key::UpArrow.into());
                    key(main.window(), Key::Return.into());
                    assert_eq!(
                        main.get_interval_minutes(),
                        25,
                        "Actual dropdown changes interval"
                    );
                    settings_click(&main, 120.0, 72.0);
                    for _ in 0..3 {
                        key(main.window(), Key::UpArrow.into());
                    }
                    key(main.window(), Key::Return.into());
                    assert_eq!(main.get_feedback_seconds(), 1, "One-second preset works");
                    assert_eq!(app.store.load().unwrap().settings.feedback_seconds, 1);
                    settings_click(&main, 32.0, 24.0);
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
                    settings_click(&main, 32.0, 24.0);
                    key(main.window(), Key::UpArrow.into());
                    key(main.window(), Key::Escape.into());
                    assert_eq!(
                        main.get_interval_minutes(),
                        26,
                        "Esc cancels the popup draft"
                    );
                    settings_click(&main, 120.0, 165.0);
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
                    main.set_settings_section(2);
                    settings_click(&main, 30.0, 193.0);
                    assert!(main.get_autostart_enabled());
                    assert!(startup_file.is_file());
                    main.set_settings_section(0);
                    settings_click(&main, 30.0, 213.0);
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
