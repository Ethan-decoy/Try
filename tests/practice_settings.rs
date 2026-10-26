//! Exercise both duration controls without constructing the application or tray.
mod support;
use slint::platform::Key;
use slint::{ComponentHandle, ModelRc, PhysicalSize, VecModel};
use std::{cell::Cell, rc::Rc};
use support::{click, key, paint, software_window};

slint::include_modules!();

fn options(values: &[&str]) -> ModelRc<slint::SharedString> {
    ModelRc::new(VecModel::from(
        values
            .iter()
            .map(|value| (*value).into())
            .collect::<Vec<_>>(),
    ))
}

#[test]
fn correct_and_knowledge_controls_dispatch_independently() {
    let adapter = software_window();
    let main = MainWindow::new().unwrap();
    adapter.set_size(PhysicalSize::new(720, 560));
    main.set_current_tab(1);
    main.set_feedback_seconds(8);
    main.set_feedback_options(options(&["1 秒", "2 秒", "3 秒", "8 秒（自定）"]));
    main.set_feedback_selection(3);
    main.set_knowledge_seconds(8);
    main.set_knowledge_options(options(&["5 秒", "8 秒", "15 秒", "30 秒"]));
    main.set_knowledge_selection(1);

    let weak = main.as_weak();
    main.on_choose_feedback(move |index| {
        assert_eq!(index, 0);
        let main = weak.unwrap();
        main.set_feedback_seconds(1);
        main.set_feedback_selection(index);
    });
    let weak = main.as_weak();
    main.on_choose_knowledge_duration(move |index| {
        assert_eq!(index, 3);
        let main = weak.unwrap();
        main.set_knowledge_seconds(30);
        main.set_knowledge_selection(index);
    });
    let precise_knowledge = Rc::new(Cell::new(0));
    let captured = precise_knowledge.clone();
    let weak = main.as_weak();
    main.on_set_knowledge_duration(move |value| {
        captured.set(value);
        let main = weak.unwrap();
        main.set_knowledge_seconds(value);
        main.set_knowledge_options(options(&[
            "5 秒",
            "8 秒",
            "15 秒",
            "30 秒",
            "31 秒（自定）",
        ]));
        main.set_knowledge_selection(4);
    });
    let precise_feedback = Rc::new(Cell::new(0));
    let captured = precise_feedback.clone();
    let weak = main.as_weak();
    main.on_set_feedback(move |value| {
        captured.set(value);
        let main = weak.unwrap();
        main.set_feedback_seconds(value);
        main.set_feedback_selection(1);
    });

    main.show().unwrap();
    paint(&adapter);
    let menu_x = main.get_settings_left() + main.get_settings_width() - 120.0;
    let precise_x = main.get_settings_left() + main.get_settings_width() - 32.0;
    let content_top = main.get_settings_content_top();

    click(&main, &adapter, menu_x, content_top + 72.0);
    for _ in 0..3 {
        key(&main, &adapter, Key::UpArrow);
    }
    key(&main, &adapter, Key::Return);
    assert_eq!(main.get_feedback_seconds(), 1);
    assert_eq!(main.get_knowledge_seconds(), 8);
    assert_eq!(main.get_feedback_display(), "1 秒");

    click(&main, &adapter, menu_x, content_top + 120.0);
    for _ in 0..2 {
        key(&main, &adapter, Key::DownArrow);
    }
    key(&main, &adapter, Key::Return);
    assert_eq!(main.get_knowledge_seconds(), 30);
    assert_eq!(main.get_feedback_seconds(), 1);

    click(&main, &adapter, precise_x, content_top + 120.0);
    key(&main, &adapter, Key::UpArrow);
    key(&main, &adapter, Key::Return);
    assert_eq!(precise_knowledge.get(), 31);
    assert_eq!(main.get_knowledge_display(), "31 秒（自定）");
    assert_eq!(precise_feedback.get(), 0);
    click(&main, &adapter, precise_x, content_top + 120.0);
    key(&main, &adapter, Key::UpArrow);
    key(&main, &adapter, Key::Escape);
    assert_eq!(
        precise_knowledge.get(),
        31,
        "Escape cancels the custom draft"
    );

    click(&main, &adapter, precise_x, content_top + 72.0);
    key(&main, &adapter, Key::UpArrow);
    key(&main, &adapter, Key::Return);
    assert_eq!(precise_feedback.get(), 2);
    assert_eq!(main.get_feedback_display(), "2 秒");
    assert_eq!(main.get_knowledge_seconds(), 31);

    if let Some(directory) = std::env::var_os("TRY_UI_PREVIEW_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        for (name, dark) in [("settings_dark", true), ("settings_light", false)] {
            main.set_dark_theme(dark);
            let pixels = paint(&adapter);
            let bytes = pixels
                .as_slice()
                .iter()
                .flat_map(|p| [p.r, p.g, p.b])
                .collect();
            image::RgbImage::from_raw(pixels.width(), pixels.height(), bytes)
                .unwrap()
                .save(directory.join(format!("{name}.png")))
                .unwrap();
        }
    }
}
