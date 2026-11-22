//! Render and exercise the actual main UI without creating an OS window or input.
mod support;
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, PhysicalSize, Rgb8Pixel, SharedPixelBuffer};
use std::{cell::Cell, rc::Rc};
use support::{click, key, paint, software_window};

slint::include_modules!();

#[test]
fn footer_actions_remain_reachable_above_the_dialog_scrim_at_scaled_dpi() {
    let adapter = software_window();
    let window = MainWindow::new().unwrap();
    let scale = 1.5;
    window
        .window()
        .dispatch_event(WindowEvent::ScaleFactorChanged {
            scale_factor: scale,
        });
    adapter.set_size(PhysicalSize::new(1080, 720));
    window.set_delete_pending(true);
    window.show().unwrap();
    paint(&adapter);
    let bounds = || {
        [
            window.get_delete_dialog_left(),
            window.get_delete_dialog_top(),
            window.get_delete_dialog_width(),
            window.get_delete_dialog_height(),
        ]
    };
    let original = bounds();
    let frame = window.global::<WindowFrame>();
    frame.set_enabled(true);
    let height = frame.get_caption_height();
    adapter.set_size(PhysicalSize::new(
        1080,
        720 + (height * scale).round() as u32,
    ));
    paint(&adapter);
    let [left, top, width, dialog_height] = original;
    for (actual, expected) in bounds()
        .into_iter()
        .zip([left, top + height, width, dialog_height])
    {
        assert!((actual - expected).abs() < 1.0);
    }

    let tray = Rc::new(Cell::new(0));
    let quit = Rc::new(Cell::new(0));
    let count = tray.clone();
    window.on_go_background(move || count.set(count.get() + 1));
    let count = quit.clone();
    window.on_quit_app(move || count.set(count.get() + 1));
    let controls = window.get_window_controls();
    let button = controls.button_width;
    let center_y = controls.y + controls.height / 2.0;
    assert!((center_y - (480.0 + height - 30.0)).abs() < 1.0);
    click(&window, &adapter, 720.0 - 2.5 * button, height / 2.0);
    click(&window, &adapter, 720.0 - 0.5 * button, height / 2.0);
    assert_eq!((tray.get(), quit.get()), (0, 0));
    click(&window, &adapter, controls.x + 0.5 * button, center_y);
    assert_eq!((tray.get(), quit.get()), (1, 0));
    click(&window, &adapter, controls.x + 2.5 * button, center_y);
    assert_eq!((tray.get(), quit.get()), (1, 1));
    assert!(window.get_delete_pending());
}

#[test]
fn footer_actions_follow_window_width_and_remain_available_while_editing() {
    let adapter = software_window();
    let window = MainWindow::new().unwrap();
    let tray = Rc::new(Cell::new(false));
    let quit = Rc::new(Cell::new(false));
    let received = tray.clone();
    window.on_go_background(move || received.set(true));
    let received = quit.clone();
    window.on_quit_app(move || received.set(true));
    window.show().unwrap();

    let frame = window.global::<WindowFrame>();
    adapter.set_size(PhysicalSize::new(720, 480));
    paint(&adapter);
    // Native window decorations have separate system controls and two app actions here.
    click(&window, &adapter, 642.0, 450.0);
    click(&window, &adapter, 686.0, 450.0);
    assert!(tray.replace(false) && quit.replace(false));

    frame.set_enabled(true);
    for (width, height, margin, editing) in [
        (720, 514, 16.0, false),
        (880, 674, 24.0, true),
        (1200, 794, 32.0, false),
    ] {
        window.set_editor_visible(editing);
        adapter.set_size(PhysicalSize::new(width, height));
        paint(&adapter);
        let controls = window.get_window_controls();
        let button = controls.button_width;
        assert!((controls.x + 3.0 * button + margin - width as f32).abs() < 1.0);
        let center_y = height as f32 - 30.0;
        click(&window, &adapter, controls.x + 0.5 * button, center_y);
        click(&window, &adapter, controls.x + 2.5 * button, center_y);
        assert!(tray.replace(false) && quit.replace(false));
    }
}

fn centered(bounds: [f32; 4], size: PhysicalSize) {
    let [x, y, width, height] = bounds;
    assert!((x + width / 2.0 - size.width as f32 / 2.0).abs() <= 1.0);
    assert!((y + height / 2.0 - size.height as f32 / 2.0).abs() <= 1.0);
    assert!(x >= 24.0 && y >= 24.0);
    assert!(x + width <= size.width as f32 - 24.0);
    assert!(y + height <= size.height as f32 - 24.0);
}

fn save_preview(name: &str, pixels: &SharedPixelBuffer<Rgb8Pixel>) {
    let Some(directory) = std::env::var_os("TRY_UI_PREVIEW_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let bytes = pixels
        .as_slice()
        .iter()
        .flat_map(|pixel| [pixel.r, pixel.g, pixel.b])
        .collect();
    image::RgbImage::from_raw(pixels.width(), pixels.height(), bytes)
        .unwrap()
        .save(directory.join(format!("{name}.png")))
        .unwrap();
}

#[test]
fn centered_deletion_guards_and_available_update_badge() {
    let adapter = software_window();
    let window = MainWindow::new().unwrap();
    adapter.set_size(PhysicalSize::new(880, 640));
    window.set_app_version(env!("CARGO_PKG_VERSION").into());
    window.set_has_selection(true);
    window.set_selected_name("微积分".into());
    window.set_selected_tag("公式".into());
    window.set_show_pack_tags(true);
    window.set_selected_enabled(true);
    window.set_selected_card_count("4 道题 · 4 道参与".into());
    window.set_participation_summary("1 个包 · 4 道题".into());
    window.set_selected_description("微积分与三角函数的必要公式。".into());
    window.set_selected_description_summary("微积分与三角函数的必要公式。".into());
    window.set_packs(slint::ModelRc::new(slint::VecModel::from(vec![PackRow {
        id: "sample".into(),
        name: "微积分".into(),
        tag: "公式".into(),
        detail: "4 / 4 题".into(),
        participating: true,
        has_cards: true,
    }])));
    window.set_cards(slint::ModelRc::new(slint::VecModel::from(
        [
            ("(sin x)′ = ?", "cos(x)"),
            ("∫ e^x dx = ?", "e^x + C"),
            ("sin(2x) = ?", "2sin(x)cos(x)"),
            ("cos(2x) = ?", "cos²(x) − sin²(x)"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (prompt, answer))| CardRow {
            id: format!("card_{index}").into(),
            pack_id: "sample".into(),
            prompt: prompt.into(),
            answer: answer.into(),
            quick_answer: answer.into(),
            enabled: true,
            ..Default::default()
        })
        .collect::<Vec<_>>(),
    )));
    let cancelled = Rc::new(Cell::new(0));
    let confirmed = Rc::new(Cell::new(0));
    let count = cancelled.clone();
    let weak = window.as_weak();
    window.on_cancel_delete(move || {
        count.set(count.get() + 1);
        weak.upgrade().unwrap().set_delete_pending(false);
    });
    let count = confirmed.clone();
    let weak = window.as_weak();
    window.on_confirm_delete(move || {
        count.set(count.get() + 1);
        weak.upgrade().unwrap().set_delete_pending(false);
    });
    let weak = window.as_weak();
    window.on_cancel_card_delete(move || weak.upgrade().unwrap().set_card_delete_pending(false));
    window.show().unwrap();
    window
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    paint(&adapter);

    // The badge is rendered only for an available download, not just any check result.
    let badge_pixel = |pixels: &SharedPixelBuffer<Rgb8Pixel>| {
        let x = (window.get_about_icon_center_x() + 11.0).round() as usize;
        let y = (window.get_about_icon_center_y() - 9.0).round() as usize;
        let pixel = pixels.as_slice()[y * pixels.width() as usize + x];
        pixel.r > 200 && pixel.g < 130 && pixel.b < 140
    };
    window.set_update_state(2);
    window.set_update_download(false);
    assert!(!badge_pixel(&paint(&adapter)));
    window.set_update_download(true);
    let available = paint(&adapter);
    assert!(badge_pixel(&available));
    save_preview("update_notice_dark", &available);
    window.set_update_download(false);
    assert!(!badge_pixel(&paint(&adapter)));

    // The extracted About content keeps its popup geometry and action routing.
    let checks = Rc::new(Cell::new(0));
    let count = checks.clone();
    window.on_check_updates(move || count.set(count.get() + 1));
    let update_actions = Rc::new(Cell::new(0));
    let count = update_actions.clone();
    window.on_open_update_download(move || count.set(count.get() + 1));
    let count = update_actions.clone();
    window.on_cancel_update_download(move || count.set(count.get() + 10));
    let count = update_actions.clone();
    window.on_restart_update(move || count.set(count.get() + 100));
    let sample_x = (window.get_about_left() + 1.0) as usize;
    let sample_y = (window.get_about_top() + 100.0) as usize;
    let panel_color = |pixels: &SharedPixelBuffer<Rgb8Pixel>| {
        let pixel = pixels.as_slice()[sample_y * pixels.width() as usize + sample_x];
        (pixel.r, pixel.g, pixel.b)
    };
    let closed = panel_color(&paint(&adapter));
    window.invoke_show_about();
    let opened = panel_color(&paint(&adapter));
    assert_ne!(
        opened, closed,
        "About must render at its advertised position"
    );
    click(
        &window,
        &adapter,
        window.get_check_link_left() + 18.0,
        window.get_check_link_top() + 18.0,
    );
    assert_eq!(checks.get(), 1);
    window.set_update_state(1);
    window.set_update_status("正在检查…".into());
    assert_eq!(panel_color(&paint(&adapter)), opened);
    click(
        &window,
        &adapter,
        window.get_check_link_left() + 18.0,
        window.get_check_link_top() + 18.0,
    );
    assert_eq!(checks.get(), 1, "Checking disables repeated requests");
    window.set_update_state(2);
    window.set_update_status("新版本 0.6.1".into());
    window.set_update_download(true);
    window.set_update_action("下载并重启".into());
    for (kind, expected) in [(0, 1), (1, 11), (2, 111)] {
        window.set_update_action_kind(kind);
        assert_eq!(panel_color(&paint(&adapter)), opened);
        click(
            &window,
            &adapter,
            window.get_about_left() + 229.0,
            window.get_about_top() + 98.0,
        );
        assert_eq!(update_actions.get(), expected);
    }
    key(&window, &adapter, Key::Escape);
    assert_eq!(panel_color(&paint(&adapter)), closed);
    window.set_update_download(false);
    window.set_update_action_kind(0);

    window.set_delete_description("删除“微积分”及其中的 4 道题？此操作无法撤销。".into());
    let sidebar_pixel = |pixels: &SharedPixelBuffer<Rgb8Pixel>| {
        pixels.as_slice()[100 * pixels.width() as usize + 8].r
    };
    let undimmed = sidebar_pixel(&paint(&adapter));
    window.set_delete_pending(true);
    let deletion = paint(&adapter);
    let dimmed = sidebar_pixel(&deletion);
    assert!(
        dimmed > 0 && dimmed < undimmed,
        "the scrim keeps the library visible"
    );
    centered(
        [
            window.get_delete_dialog_left(),
            window.get_delete_dialog_top(),
            window.get_delete_dialog_width(),
            window.get_delete_dialog_height(),
        ],
        adapter.size(),
    );
    assert!(window.get_delete_cancel_focused());
    save_preview("delete_confirmation_dark", &deletion);
    // A backdrop click must not open About or steal the modal's focus.
    click(
        &window,
        &adapter,
        window.get_about_icon_center_x(),
        window.get_about_icon_center_y(),
    );
    assert!(window.get_delete_cancel_focused());
    key(&window, &adapter, Key::Return);
    assert_eq!((cancelled.get(), confirmed.get()), (1, 0));

    window.set_delete_pending(true);
    paint(&adapter);
    key(&window, &adapter, Key::Tab);
    assert!(window.get_delete_confirm_focused());
    key(&window, &adapter, Key::Tab);
    assert!(window.get_delete_cancel_focused());
    key(&window, &adapter, Key::Tab);
    assert!(window.get_delete_confirm_focused());
    key(&window, &adapter, Key::Escape);
    assert_eq!((cancelled.get(), confirmed.get()), (2, 0));
    window.set_delete_pending(true);
    paint(&adapter);
    click(
        &window,
        &adapter,
        window.get_delete_confirm_center_x(),
        window.get_delete_confirm_center_y(),
    );
    assert_eq!((cancelled.get(), confirmed.get()), (2, 1));

    // Small windows, long question prompts and save errors keep the actions on screen.
    adapter.set_size(PhysicalSize::new(720, 480));
    window.set_dark_theme(false);
    window.set_card_delete_prompt("较长的题目提示与必要条件。".repeat(100).into());
    window.set_message_is_error(true);
    window.set_message("无法保存，请重试。".into());
    window.set_card_delete_pending(true);
    let small = paint(&adapter);
    centered(
        [
            window.get_card_dialog_left(),
            window.get_card_dialog_top(),
            window.get_card_dialog_width(),
            window.get_card_dialog_height(),
        ],
        adapter.size(),
    );
    assert!(window.get_card_cancel_focused());
    save_preview("delete_confirmation_light_small", &small);
    key(&window, &adapter, Key::Escape);
    assert!(!window.get_card_delete_pending());
}
