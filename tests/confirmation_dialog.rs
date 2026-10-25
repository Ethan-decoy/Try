//! Render and exercise the actual main UI without creating an OS window or input.
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{
    Key, Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
};
use slint::{ComponentHandle, LogicalPosition, PhysicalSize, Rgb8Pixel, SharedPixelBuffer};
use std::{cell::Cell, rc::Rc};

slint::include_modules!();

struct SoftwarePlatform(Rc<MinimalSoftwareWindow>);

impl Platform for SoftwarePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

fn paint(adapter: &MinimalSoftwareWindow) -> SharedPixelBuffer<Rgb8Pixel> {
    slint::platform::update_timers_and_animations();
    adapter.request_redraw();
    let size = adapter.size();
    let mut pixels = SharedPixelBuffer::new(size.width, size.height);
    adapter.draw_if_needed(|renderer| {
        renderer.render(pixels.make_mut_slice(), size.width as usize);
    });
    pixels
}

fn click(window: &MainWindow, adapter: &MinimalSoftwareWindow, x: f32, y: f32) {
    let position = LogicalPosition::new(x, y);
    for event in [
        WindowEvent::PointerMoved { position },
        WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        },
        WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        },
    ] {
        window.window().dispatch_event(event);
    }
    paint(adapter);
}

fn key(window: &MainWindow, adapter: &MinimalSoftwareWindow, key: Key) {
    let text: slint::SharedString = key.into();
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
    paint(adapter);
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
    let adapter = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(SoftwarePlatform(adapter.clone()))).unwrap();
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

    window.set_delete_description("删除“微积分”及其中的 4 道题？此操作无法撤销。".into());
    window.set_delete_pending(true);
    let deletion = paint(&adapter);
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
