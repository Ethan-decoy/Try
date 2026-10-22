//! Explicit native checks for updates.
use super::*;

static AUTOMATIC_UPDATE_FETCHES: AtomicUsize = AtomicUsize::new(0);

fn automatic_update_release() -> update_check::Release {
    let mut version = semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
    version.patch += 1;
    update_check::Release::from_tag(&format!("v{version}")).unwrap()
}

fn automatic_update_fixture() -> Result<CheckOutcome, CheckError> {
    AUTOMATIC_UPDATE_FETCHES.fetch_add(1, Ordering::SeqCst);
    Ok(CheckOutcome::Available(automatic_update_release()))
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
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Idle
                    ));
                    assert!(
                        app.restart_update.borrow().is_none(),
                        "a background check never downloads or restarts"
                    );
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
                    assert_eq!(
                        main.get_update_status(),
                        format!("新版本 {}", automatic_update_release().version)
                    );
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
                        format!("新版本 {}", automatic_update_release().version),
                        "A background failure retains an already discovered version"
                    );
                    assert!(main.get_update_download());
                    main.invoke_choose_language(2);
                    assert_eq!(
                        main.get_update_status(),
                        format!("Version {} available", automatic_update_release().version)
                    );
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
                    assert_eq!(main.get_update_action(), "Update & restart");
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
                    *app.update_transfer.lock().unwrap() = UpdateTransfer::Failed {
                        phase: UpdatePhase::Downloading,
                        error: "fixture".into(),
                    };
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
                    let prepared = updater::ui_fixture(app.store.directory()).unwrap();
                    app.begin_update_download(UpdatePhase::Downloading, move |cancel, progress| {
                        progress(42, Some(100));
                        let deadline = std::time::Instant::now() + Duration::from_secs(10);
                        while !cancel.load(Ordering::Relaxed)
                            && std::time::Instant::now() < deadline
                        {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Ok(Some(prepared))
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
                    assert!(app.restart_update.borrow().is_none());
                    assert!(
                        !std::fs::read_dir(app.store.directory())
                            .unwrap()
                            .any(|entry| {
                                entry
                                    .unwrap()
                                    .file_name()
                                    .to_string_lossy()
                                    .starts_with(".try_update_")
                            }),
                        "cancellation discards even a successfully verified download"
                    );
                    app.begin_update_download(UpdatePhase::Downloading, |_, _| {
                        Err("download failure fixture".into())
                    });
                }
                13 => {
                    assert_eq!(main.get_update_status(), "Download failed; retry");
                    assert_eq!(main.get_update_action(), "Retry");
                    assert!(
                        app.restart_update.borrow().is_none(),
                        "failed downloads never restart"
                    );
                    *app.update_transfer.lock().unwrap() =
                        UpdateTransfer::Ready(updater::ui_fixture(&checks.output).unwrap());
                    app.refresh_update_status();
                }
                14 => {
                    assert_eq!(main.get_update_action(), "Retry update");
                    assert_eq!(main.get_update_status(), "Update ready");
                    app.resume_ready_update();
                    main.invoke_cancel_update_download();
                    assert!(
                        app.restart_update.borrow().is_none(),
                        "late cancellation never restarts"
                    );
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Idle
                    ));
                    main.invoke_create_pack();
                    let fetches = checks.fetches.clone();
                    app.begin_update_download(UpdatePhase::Downloading, move |_, _| {
                        fetches.fetch_add(1, Ordering::SeqCst);
                        Err("must not download with an unsaved draft".into())
                    });
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Idle
                    ));
                    *app.update_transfer.lock().unwrap() =
                        UpdateTransfer::Ready(updater::ui_fixture(app.store.directory()).unwrap());
                    main.invoke_restart_update();
                    assert!(
                        app.restart_update.borrow().is_none(),
                        "unsaved editor blocks restart"
                    );
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Ready(_)
                    ));
                    main.invoke_choose_language(1);
                }
                15 => {
                    assert_eq!(
                        checks.fetches.load(Ordering::SeqCst),
                        1,
                        "an open draft blocks download before starting a worker"
                    );
                    assert_eq!(main.get_update_action(), "取消");
                    assert_eq!(main.get_update_status(), "等待编辑完成");
                    assert_eq!(app.store.load().unwrap().packs, checks.original_packs);
                    main.invoke_cancel_update_download();
                    main.invoke_cancel_editor();
                    let prepared = updater::ui_fixture(app.store.directory()).unwrap();
                    app.begin_update_download(UpdatePhase::Downloading, move |_, _| {
                        Ok(Some(prepared))
                    });
                    // Completion must install without any second button, even if
                    // the About window was closed while the worker finished.
                    app.close_main();
                    checks.finished.set(true);
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

fn update_edit_phase(checks: Rc<UpdateChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(450), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let main = main_window(app);
            match phase {
                0 => {
                    let prepared = updater::ui_fixture(app.store.directory()).unwrap();
                    let (send, receive) = std::sync::mpsc::channel();
                    app.begin_update_download(UpdatePhase::Downloading, move |_, _| {
                        receive.recv_timeout(Duration::from_secs(6)).unwrap();
                        Ok(Some(prepared))
                    });
                    main.invoke_create_pack();
                    app.pending_import
                        .replace(Some(checks.original_packs[0].clone()));
                    main.set_tag_draft(main.get_selected_tag());
                    main.set_tag_editing(true);
                    app.refresh();
                    send.send(()).unwrap();
                }
                1 => {
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Ready(_)
                    ));
                    assert!(
                        !app.update_transfer_timer.running(),
                        "waiting for edits does not poll"
                    );
                    assert!(app.restart_update.borrow().is_none());
                    assert_eq!(main.get_update_status(), "等待编辑完成");
                    assert_eq!(main.get_update_action(), "取消");
                    main.invoke_cancel_editor();
                }
                2 => {
                    assert!(
                        app.restart_update.borrow().is_none(),
                        "pending import still blocks installation"
                    );
                    assert!(app.editor.borrow().is_none());
                    main.invoke_cancel_import();
                    main.invoke_choose_language(2);
                    assert_eq!(main.get_update_status(), "Waiting for edits");
                    assert_eq!(main.get_update_action(), "Cancel");
                }
                3 => {
                    assert!(app.pending_import.borrow().is_none());
                    assert!(
                        app.restart_update.borrow().is_none(),
                        "inline metadata draft is also protected"
                    );
                    checks.finished.set(true);
                    main.invoke_save_pack_tag();
                    // Saving the final draft resumes the original download's
                    // restart automatically; no restart/update confirmation.
                    return;
                }
                _ => unreachable!(),
            }
            update_edit_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

fn run_update_check(output_name: &str, drive: fn(Rc<UpdateChecks>, u8)) {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local_artifacts")
        .join(output_name);
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
    drive(checks.clone(), 0);
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
    assert!(
        checks.app.take_restart_update().is_some(),
        "verified download is automatically handed to shutdown"
    );
    assert!(matches!(
        *checks.app.update_transfer.lock().unwrap(),
        UpdateTransfer::Idle
    ));
    assert!(!checks.app.update_transfer_timer.running());
    assert_eq!(
        checks.app.store.load().unwrap().packs,
        checks.original_packs
    );
    println!(
        "Native update check passed: {output_name}; isolated profile preserved and verified download handed to shutdown."
    );
}

#[test]
#[ignore = "opens real update UI using fixtures and an isolated profile"]
fn native_update_check() {
    run_update_check("update_check_validation", update_phase);
}

#[test]
#[ignore = "opens real update UI using fixtures and an isolated profile"]
fn native_update_edit_guard() {
    run_update_check("update_edit_guard_validation", update_edit_phase);
}

fn latest_download_fixture() -> Result<CheckOutcome, CheckError> {
    Ok(CheckOutcome::Available(
        update_check::Release::from_tag("v99.1.0").unwrap(),
    ))
}

fn set_cached_download_hint(app: &App) {
    let cached = CheckOutcome::Available(update_check::Release::from_tag("v99.0.0").unwrap());
    {
        let mut check = app.update_check.lock().unwrap();
        check
            .schedule
            .record_attempt(chrono::Utc::now().timestamp())
            .unwrap();
        check.schedule.record_result(&cached).unwrap();
        check.finish(Ok(cached), false);
    }
    app.refresh_update_status();
}

fn update_target_phase(checks: Rc<UpdateChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(450), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let selected = checks.output.join("selected_release.txt");
            match phase {
                0 => {
                    set_cached_download_hint(app);
                    assert_eq!(main_window(app).get_update_status(), "新版本 99.0.0");
                    let (send, receive) = std::sync::mpsc::channel();
                    checks.gate.replace(Some(send));
                    let calls = checks.fetches.clone();
                    app.open_update_download_with(
                        move || {
                            receive.recv_timeout(Duration::from_secs(8)).unwrap();
                            latest_download_fixture()
                        },
                        move |release, _, _| {
                            calls.fetch_add(1, Ordering::SeqCst);
                            std::fs::write(selected, release.version.to_string()).unwrap();
                            Err("stop after recording the selected download".into())
                        },
                    );
                    assert_eq!(
                        main_window(app).get_update_status(),
                        "正在检查…",
                        "Resolving a fresh target must not be presented as downloading"
                    );
                    let repeated = checks.fetches.clone();
                    app.open_update_download_with(
                        move || {
                            repeated.fetch_add(100, Ordering::SeqCst);
                            latest_download_fixture()
                        },
                        |_, _, _| Err("must not start a second download".into()),
                    );
                }
                1 => {
                    let main = main_window(app);
                    assert_eq!(main.get_update_state(), 1);
                    assert!(main.get_update_busy());
                    assert_eq!(main.get_update_action(), "取消");
                    main.invoke_choose_language(2);
                    assert_eq!(main.get_update_status(), "Checking…");
                    assert_eq!(main.get_update_action(), "Cancel");
                    app.close_main();
                    drop(main);
                    app.show_main().unwrap();
                    let reopened = main_window(app);
                    assert_eq!(reopened.get_update_status(), "Checking…");
                    reopened.invoke_choose_language(1);
                    checks.gate.borrow_mut().take().unwrap().send(()).unwrap();
                }
                2 => {
                    assert_eq!(
                        std::fs::read_to_string(selected).unwrap(),
                        "99.1.0",
                        "Download must refresh a cached target even within the daily check budget"
                    );
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Failed {
                            phase: UpdatePhase::Downloading,
                            ..
                        }
                    ));
                    assert_eq!(main_window(app).get_update_status(), "下载失败，请重试");
                    assert_eq!(checks.fetches.load(Ordering::SeqCst), 1);
                    let persisted = UpdateSchedule::open(
                        app.store.directory(),
                        env!("CARGO_PKG_VERSION"),
                        chrono::Utc::now().timestamp(),
                    );
                    assert_eq!(
                        persisted.cached_outcome(),
                        Some(latest_download_fixture().unwrap())
                    );
                    let calls = checks.fetches.clone();
                    app.open_update_download_with(
                        || Err(CheckError::Network),
                        move |_, _, _| {
                            calls.fetch_add(1, Ordering::SeqCst);
                            Err("must not download a cached target after a failed refresh".into())
                        },
                    );
                }
                3 => {
                    assert_eq!(
                        checks.fetches.load(Ordering::SeqCst),
                        1,
                        "Failed refresh must not fall back to the cached package"
                    );
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Failed {
                            phase: UpdatePhase::Checking,
                            ..
                        }
                    ));
                    assert!(main_window(app).get_update_download());
                    assert_eq!(main_window(app).get_update_status(), "检查失败，请重试");
                    main_window(app).invoke_choose_language(2);
                    assert_eq!(main_window(app).get_update_status(), "Check failed; retry");
                    assert_eq!(main_window(app).get_update_action(), "Retry");
                    main_window(app).invoke_choose_language(1);
                    assert!(app.restart_update.borrow().is_none());
                    let calls = checks.fetches.clone();
                    app.open_update_download_with(
                        || Ok(CheckOutcome::UpToDate),
                        move |_, _, _| {
                            calls.fetch_add(1, Ordering::SeqCst);
                            Err("must not download when no newer release is available".into())
                        },
                    );
                }
                4 => {
                    assert_eq!(checks.fetches.load(Ordering::SeqCst), 1);
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Idle
                    ));
                    assert_eq!(main_window(app).get_update_status(), "已是最新");
                    assert!(!main_window(app).get_update_download());
                    set_cached_download_hint(app);
                    let calls = checks.fetches.clone();
                    app.open_update_download_with(
                        || Ok(CheckOutcome::NoPublicRelease),
                        move |_, _, _| {
                            calls.fetch_add(1, Ordering::SeqCst);
                            Err("must not download a withdrawn release".into())
                        },
                    );
                }
                5 => {
                    assert_eq!(checks.fetches.load(Ordering::SeqCst), 1);
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Idle
                    ));
                    assert_eq!(main_window(app).get_update_status(), "尚无公开版本");
                    assert!(!main_window(app).get_update_download());
                    set_cached_download_hint(app);
                    let (send, receive) = std::sync::mpsc::channel();
                    checks.gate.replace(Some(send));
                    let calls = checks.fetches.clone();
                    app.open_update_download_with(
                        move || {
                            std::fs::write(selected, "checking").unwrap();
                            receive.recv_timeout(Duration::from_secs(8)).unwrap();
                            latest_download_fixture()
                        },
                        move |_, _, _| {
                            calls.fetch_add(1, Ordering::SeqCst);
                            Err("must not prepare a package after cancelling the check".into())
                        },
                    );
                }
                6 => {
                    // Wait until the request really entered the blocked fetch.
                    // Cancelling sooner can validly finish before any request.
                    if !std::fs::read_to_string(selected).is_ok_and(|value| value == "checking") {
                        update_target_phase(checks.clone(), phase);
                        return;
                    }
                    app.cancel_update_download();
                    app.cancel_update_download();
                    assert_eq!(main_window(app).get_update_status(), "正在取消…");
                    app.close_main();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    assert_eq!(main.get_update_status(), "正在取消…");
                    main.invoke_choose_language(2);
                    assert_eq!(main.get_update_status(), "Cancelling…");
                    main.invoke_choose_language(1);
                    checks.gate.borrow_mut().take().unwrap().send(()).unwrap();
                }
                7 => {
                    if matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Running { .. }
                    ) {
                        update_target_phase(checks.clone(), phase);
                        return;
                    }
                    assert_eq!(checks.fetches.load(Ordering::SeqCst), 1);
                    assert!(matches!(
                        *app.update_transfer.lock().unwrap(),
                        UpdateTransfer::Idle
                    ));
                    assert!(!app.update_transfer_timer.running());
                    assert!(app.restart_update.borrow().is_none());
                    assert_eq!(main_window(app).get_update_status(), "新版本 99.1.0");
                    let mut prepared = updater::ui_fixture(app.store.directory()).unwrap();
                    app.open_update_download_with(latest_download_fixture, move |release, _, _| {
                        assert_eq!(release.version.to_string(), "99.1.0");
                        prepared.release = release;
                        Ok(prepared)
                    });
                    checks.finished.set(true);
                    return;
                }
                _ => unreachable!(),
            }
            update_target_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens the real download coordinator with a stale cached release and an isolated profile"]
fn native_update_latest_target() {
    run_update_check("update_latest_target_validation", update_target_phase);
}
