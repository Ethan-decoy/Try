use super::*;
use chrono::Timelike;
use slint::winit_030::winit::event_loop::EventLoop;
use slint::winit_030::winit::platform::windows::EventLoopBuilderExtWindows;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EndMenu, FindWindowExW, GetMenuItemCount, GetMenuState, GetMenuStringW, GetSubMenu,
    GetWindowThreadProcessId, HMENU, HWND_MESSAGE, MF_BYPOSITION, MF_DISABLED, MF_GRAYED,
    SendMessageW, WM_APP, WM_ENTERIDLE, WM_INITMENUPOPUP, WM_RBUTTONUP,
};

#[derive(Debug, serde::Serialize)]
struct MenuNode {
    title: String,
    enabled: bool,
    children: Vec<MenuNode>,
}

fn tray_window() -> HWND {
    let class = "SlintSystemTrayWindow\0".encode_utf16().collect::<Vec<_>>();
    let mut previous = std::ptr::null_mut();
    loop {
        // SAFETY: inspect message-only windows, selecting only this process.
        previous =
            unsafe { FindWindowExW(HWND_MESSAGE, previous, class.as_ptr(), std::ptr::null()) };
        assert!(!previous.is_null(), "The tray message window exists");
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(previous, &mut owner) };
        if owner == std::process::id() {
            return previous;
        }
    }
}

fn read_menu(menu: HMENU) -> Vec<MenuNode> {
    // SAFETY: called synchronously while Windows owns the live popup menu.
    let count = unsafe { GetMenuItemCount(menu) };
    assert!(count > 0);
    (0..count as u32)
        .map(|index| {
            let mut text = [0u16; 256];
            let length = unsafe {
                GetMenuStringW(
                    menu,
                    index,
                    text.as_mut_ptr(),
                    text.len() as i32,
                    MF_BYPOSITION,
                )
            };
            let state = unsafe { GetMenuState(menu, index, MF_BYPOSITION) };
            let child = unsafe { GetSubMenu(menu, index as i32) };
            MenuNode {
                title: String::from_utf16_lossy(&text[..length as usize]),
                enabled: state & (MF_DISABLED | MF_GRAYED) == 0,
                children: if child.is_null() {
                    vec![]
                } else {
                    read_menu(child)
                },
            }
        })
        .collect()
}

unsafe extern "system" fn capture_menu(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    data: usize,
) -> LRESULT {
    if message == WM_INITMENUPOPUP {
        // SAFETY: inspect_menu keeps this allocation alive until detachment.
        let result = unsafe { &*(data as *const RefCell<Option<Vec<MenuNode>>>) };
        let read = std::panic::catch_unwind(|| read_menu(wparam as HMENU));
        if let Ok(nodes) = read {
            result.replace(Some(nodes));
        }
        // Finish only the test thread's native menu, without moving the pointer
        // or injecting keyboard input into other applications.
        unsafe { EndMenu() };
    } else if message == WM_ENTERIDLE {
        unsafe { EndMenu() };
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

fn inspect_menu(output: &std::path::Path) -> Vec<MenuNode> {
    slint::platform::update_timers_and_animations();
    let window = tray_window();
    let result: RefCell<Option<Vec<MenuNode>>> = RefCell::new(None);
    // SAFETY: attachment, menu dispatch and removal all run synchronously on
    // the same UI thread; result remains alive throughout that interval.
    unsafe {
        assert_ne!(
            SetWindowSubclass(
                window,
                Some(capture_menu),
                0x7472796d,
                &result as *const _ as usize
            ),
            0
        );
        SendMessageW(window, WM_APP + 1, 1, WM_RBUTTONUP as isize);
        RemoveWindowSubclass(window, Some(capture_menu), 0x7472796d);
    }
    let nodes = result.into_inner().expect("A real Windows menu was opened");
    std::fs::write(output, serde_json::to_vec_pretty(&nodes).unwrap()).unwrap();
    nodes
}

fn main_window(app: &App) -> MainWindow {
    app.main_window.borrow().as_ref().unwrap().clone_strong()
}

fn snapshot(window: &slint::Window, path: PathBuf) {
    let pixels = window.take_snapshot().unwrap();
    image::save_buffer_with_format(
        path,
        pixels.as_bytes(),
        pixels.width(),
        pixels.height(),
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .unwrap();
}

struct TrayChecks {
    app: Rc<App>,
    output: PathBuf,
    finished: Cell<bool>,
    saved: RefCell<Option<Library>>,
    old_main: RefCell<Option<slint::Weak<MainWindow>>>,
    old_practice: RefCell<Option<slint::Weak<PracticeWindow>>>,
}

fn phase(checks: Rc<TrayChecks>, step: u8) {
    Timer::single_shot(Duration::from_millis(300), move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match step {
                0 => {
                    let nodes = inspect_menu(&checks.output.join("menu_zh.json"));
                    assert_eq!(
                        nodes.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(),
                        ["打开", "设置", "", "暂离", "暂停", "", "关于", "退出"]
                    );
                    assert_eq!(
                        nodes[3]
                            .children
                            .iter()
                            .map(|n| n.title.as_str())
                            .collect::<Vec<_>>(),
                        ["1 小时", "2 小时", "3 小时", "自定…"]
                    );
                    assert!(
                        nodes
                            .iter()
                            .filter(|n| !n.title.is_empty())
                            .all(|n| n.enabled)
                    );
                    app.tray.invoke_open_settings(false);
                }
                1 => {
                    assert_eq!(main_window(app).get_current_tab(), 1);
                    app.close_main();
                    app.tray.invoke_open_about();
                }
                2 => {
                    assert!(
                        main_window(app).get_about_open(),
                        "Tray About must open its panel after recreating the main window"
                    );
                    assert_eq!(
                        main_window(app).get_current_tab(),
                        1,
                        "Open preserves the previous page"
                    );
                    snapshot(
                        main_window(app).window(),
                        checks.output.join("about_from_tray.png"),
                    );
                    main_window(app).invoke_hide_about();
                    app.tray.invoke_open_settings(true);
                }
                3 => {
                    let main = main_window(app);
                    assert_eq!(main.get_current_tab(), 1);
                    snapshot(main.window(), checks.output.join("precise_from_tray.png"));
                    main.window()
                        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                            text: slint::platform::Key::Escape.into(),
                        });
                    main.window()
                        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
                            text: slint::platform::Key::Escape.into(),
                        });
                    assert_eq!(
                        app.store.load().unwrap().settings.pause,
                        PracticePause::None
                    );
                    app.tray.invoke_set_pause(180);
                }
                4 => {
                    assert_eq!(app.tray.get_pause_minutes(), 180);
                    assert!(app.tray.get_pause_title().starts_with("暂离至 "));
                    assert!(app.pause_timer.running());
                    assert!(!app.reminder_timer.running());
                    let nodes = inspect_menu(&checks.output.join("menu_paused_zh.json"));
                    assert_eq!(nodes[4].title, "恢复");
                    assert_eq!(nodes[3].children[2].title, "✓ 3 小时");
                    main_window(app).invoke_choose_language(2);
                }
                5 => {
                    let nodes = inspect_menu(&checks.output.join("menu_paused_en.json"));
                    assert_eq!(nodes[0].title, "Open");
                    assert_eq!(nodes[1].title, "Settings");
                    assert!(nodes[3].title.starts_with("Paused until "));
                    assert_eq!(nodes[3].children[2].title, "✓ 3 hours");
                    assert_eq!(nodes[4].title, "Resume");
                    assert_eq!(nodes[6].title, "About");
                    assert_eq!(nodes[7].title, "Quit");
                    checks.saved.replace(Some(app.store.load().unwrap()));
                    checks.old_main.replace(Some(main_window(app).as_weak()));
                    app.close_main();
                    app.tray.invoke_set_pause(0);
                }
                6 => {
                    assert!(
                        checks
                            .old_main
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .upgrade()
                            .is_none()
                    );
                    assert!(app.reminder_timer.running());
                    assert_eq!(app.library.borrow().settings.pause, PracticePause::None);
                    app.show_practice().unwrap();
                    checks.old_practice.replace(Some(
                        app.practice.borrow().as_ref().unwrap().window.as_weak(),
                    ));
                    app.tray.invoke_set_pause(-1);
                }
                7 => {
                    assert!(app.main_window.borrow().is_none());
                    assert!(
                        app.practice.borrow().is_none(),
                        "Pausing closes the current card"
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
                    assert!(!app.reminder_timer.running());
                    assert!(
                        !app.pause_timer.running(),
                        "An indefinite pause has no deadline timer"
                    );
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.settings.pause, PracticePause::UntilResumed);
                    assert!(
                        saved
                            .settings
                            .pause
                            .active(chrono::Utc::now().timestamp() + 365 * 86400)
                    );
                    checks.saved.replace(Some(saved));
                    let nodes = inspect_menu(&checks.output.join("menu_until_resumed_en.json"));
                    assert_eq!(nodes[3].title, "Pause for");
                    assert_eq!(nodes[3].children.len(), 4);
                    assert_eq!(nodes[4].title, "Resume");
                    app.show_practice_with_origin(PracticeOrigin::Reminder)
                        .unwrap();
                    app.arm_reminder();
                    assert!(app.practice.borrow().is_none());
                    assert!(!app.reminder_timer.running());
                    assert!(
                        crate::single_instance::Instance::forward(app.store.directory(), false)
                            .unwrap()
                    );
                }
                8 => {
                    assert!(app.main_window.borrow().is_some());
                    assert!(
                        app.practice.borrow().is_none(),
                        "Reopening only shows the main window"
                    );
                    assert_eq!(main_window(app).get_pause_minutes(), -1);
                    assert_eq!(
                        app.store.load().unwrap(),
                        checks.saved.borrow().as_ref().unwrap().clone()
                    );
                    app.close_main();
                    assert!(
                        !app.reminder_timer.running(),
                        "Opening and closing never resume practice"
                    );
                    app.tray.invoke_practice_now();
                }
                9 => {
                    assert!(
                        app.practice.borrow().is_some(),
                        "Explicit manual practice remains available"
                    );
                    assert_eq!(
                        app.library.borrow().settings.pause,
                        PracticePause::UntilResumed
                    );
                    app.close_practice();
                    assert!(
                        !app.reminder_timer.running(),
                        "Manual practice does not resume the schedule"
                    );
                    assert!(
                        crate::single_instance::Instance::forward(app.store.directory(), true)
                            .unwrap()
                    );
                }
                10 => {
                    assert!(app.practice.borrow().is_some());
                    assert_eq!(
                        app.library.borrow().settings.pause,
                        PracticePause::UntilResumed
                    );
                    app.close_practice();
                    app.tray.invoke_set_pause(0);
                }
                11 => {
                    assert!(app.reminder_timer.running());
                    assert_eq!(
                        app.store.load().unwrap().settings.pause,
                        PracticePause::None
                    );
                    assert_eq!(
                        app.library.borrow().settings.quiet_hours,
                        checks.saved.borrow().as_ref().unwrap().settings.quiet_hours
                    );
                    let nodes = inspect_menu(&checks.output.join("menu_resumed_en.json"));
                    assert_eq!(nodes[4].title, "Pause");
                    // A timed break still expires automatically; only the
                    // isolated deadline is shortened below the UI's minimum.
                    app.commit(|library| {
                        library.settings.pause = PracticePause::Timed {
                            until: chrono::Utc::now().timestamp() + 2,
                            minutes: 1,
                        };
                        Ok(())
                    })
                    .unwrap();
                }
                12 => {
                    if app.pause_timer.running() {
                        phase(checks.clone(), step);
                        return;
                    }
                    assert_eq!(
                        app.store.load().unwrap().settings.pause,
                        PracticePause::None
                    );
                    assert_eq!(app.tray.get_pause_minutes(), 0);
                    assert!(app.reminder_timer.running());
                    assert!(app.main_window.borrow().is_none());
                    assert!(app.practice.borrow().is_none());
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.set_current_tab(0);
                    main.invoke_begin_rename();
                    main.set_name_draft("保留的名称草稿".into());
                }
                13 => {
                    let nodes = inspect_menu(&checks.output.join("menu_editing_en.json"));
                    assert!(!nodes[1].enabled, "Settings respects active editing");
                    assert!(
                        !nodes[3].children[3].enabled,
                        "Custom pause respects active editing"
                    );
                    app.tray.invoke_open_settings(true);
                }
                14 => {
                    let main = main_window(app);
                    assert_eq!(main.get_current_tab(), 0);
                    assert!(main.get_name_editing());
                    assert_eq!(main.get_name_draft(), "保留的名称草稿");
                    checks.finished.set(true);
                    app.tray.invoke_quit_app();
                    println!(
                        "Native menu labels/submenu/state, settings/about navigation, precise pause, bilingual refresh, indefinite pause persistence, released card, launcher reopening without resume, retained quiet hours, timed expiry and editing protection passed."
                    );
                    return;
                }
                _ => unreachable!(),
            }
            phase(checks.clone(), step + 1);
        }));
        if result.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens native tray menus and isolated windows"]
fn native_tray_menu() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/tray_menu_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = Store::open(profile.path().to_owned()).unwrap();
    let mut library = store.load().unwrap();
    library.settings.interface_language = InterfaceLanguage::Chinese;
    let now = chrono::Local::now();
    let minute = (now.hour() * 60 + now.minute()) as u16;
    library.settings.quiet_hours = QuietHours {
        enabled: true,
        start_minute: (minute + 1439) % 1440,
        end_minute: (minute + 60) % 1440,
    };
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
    slint::platform::update_timers_and_animations();
    let weak = Rc::downgrade(&app);
    let _instance = crate::single_instance::Instance::listen(
        app.store.directory(),
        Rc::new(move |practice| {
            if let Some(app) = weak.upgrade() {
                app.open_from_launcher(practice);
            }
        }),
    )
    .unwrap();
    let checks = Rc::new(TrayChecks {
        app,
        output,
        finished: Cell::new(false),
        saved: RefCell::new(None),
        old_main: RefCell::new(None),
        old_practice: RefCell::new(None),
    });
    phase(checks.clone(), 0);
    let watchdog = Rc::downgrade(&checks.app);
    Timer::single_shot(Duration::from_secs(18), move || {
        if let Some(app) = watchdog.upgrade() {
            app.quit();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.finished.get(), "Tray check failed or timed out");
}
