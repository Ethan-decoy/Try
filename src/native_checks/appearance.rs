//! Explicit native checks for appearance.
use super::practice::starter_question;
use super::*;

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
                    snapshot(main.window(), checks.output.join("dark_main.png"));
                    let size = main
                        .window()
                        .size()
                        .to_logical(main.window().scale_factor());
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(46.0, size.height - 28.0),
                    );
                    assert_eq!(
                        main.get_current_tab(),
                        1,
                        "Settings opens from the lower-left navigation"
                    );
                }
                1 => {
                    let main = main_window(app);
                    main.set_settings_section(2);
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
                            - main.get_matrix_mode_width()
                            - 32.0
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
                        main.get_question_area_height() > 120.0,
                        "Switching packs retains a usable list viewport"
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
                    show_general_settings(&main);
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
                    main.set_current_tab(1);
                    main.set_settings_section(1);
                }
                25 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("card_settings.png"));
                    let size = main
                        .window()
                        .size()
                        .to_logical(main.window().scale_factor());
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(68.0, size.height - 28.0),
                    );
                }
                26 => {
                    snapshot(
                        main_window(app).window(),
                        checks.output.join("about_lower_left.png"),
                    );
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
