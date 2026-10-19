//! Explicit native checks for lifecycle.
use super::*;

struct Checks {
    app: Rc<App>,
    output: PathBuf,
    finished: Cell<bool>,
    old_main: RefCell<Option<slint::Weak<MainWindow>>>,
    old_practice: RefCell<Option<slint::Weak<PracticeWindow>>>,
    focus_probe: RefCell<Option<FocusProbe>>,
    creation_foreground: Rc<Cell<Option<(HWND, HWND)>>>,
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
