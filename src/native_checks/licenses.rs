//! Explicit native checks for licenses.
use super::*;

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
    // Sample above the shorter About popup and over the sidebar: its background
    // differs from the notice panel, unlike the same-colored workspace at x+280.
    let x = ((main.get_about_left() + 20.0) * scale) as usize;
    let y = ((main.get_notices_top() + 60.0) * scale) as usize;
    let offset = (y * buffer.width() as usize + x) * 4;
    let pixel = &buffer.as_bytes()[offset..offset + 3];
    // Regression profiles use the dark Theme.panel (#1c1c1c). Keep an exact
    // pixel check; the former #292929 expectation also fails on older builds.
    if (pixel == [28, 28, 28]) != visible {
        snapshot(
            main.window(),
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("local_artifacts/licenses_check/notice_visibility_failure.png"),
        );
    }
    assert_eq!(
        pixel == [28, 28, 28],
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
                        [28, 28, 28],
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
