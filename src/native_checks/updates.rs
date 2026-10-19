//! Explicit native checks for updates.
use super::*;

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
