//! Exercise practice settings and their card presentation with a software window.
mod support;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Key, Platform, PlatformError, WindowAdapter};
use slint::private_unstable_api::re_exports::{
    AccessibleRole, AccessibleStringProperty, ItemRc, WindowInner,
};
use slint::{ComponentHandle, ModelRc, PhysicalSize, Rgb8Pixel, SharedPixelBuffer, VecModel};
use std::ops::ControlFlow;
use std::time::Duration;
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

fn save_preview(name: &str, pixels: &SharedPixelBuffer<Rgb8Pixel>) {
    if let Some(directory) = std::env::var_os("TRY_UI_PREVIEW_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        image::save_buffer(
            directory.join(format!("{name}.png")),
            pixels.as_bytes(),
            pixels.width(),
            pixels.height(),
            image::ColorType::Rgb8,
        )
        .unwrap();
    }
}

fn footer_has_text(card: &PracticeWindow, pixels: &SharedPixelBuffer<Rgb8Pixel>) -> bool {
    let width = pixels.width() as usize;
    pixels
        .as_slice()
        .chunks_exact(width)
        .skip(card.get_answer_footer_top().ceil() as usize + 2)
        // Sample within the text column. The rounded frame also changes pixels near the corners.
        .any(|row| {
            row[width / 4..3 * width / 4]
                .windows(2)
                .any(|pair| pair[0] != pair[1])
        })
}

fn timed_window() -> (Rc<MinimalSoftwareWindow>, Rc<Cell<u64>>) {
    struct Clock {
        window: Rc<MinimalSoftwareWindow>,
        elapsed_ms: Rc<Cell<u64>>,
    }
    impl Platform for Clock {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(self.window.clone())
        }
        fn duration_since_start(&self) -> Duration {
            Duration::from_millis(self.elapsed_ms.get())
        }
    }

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    let elapsed_ms = Rc::new(Cell::new(1000));
    slint::platform::set_platform(Box::new(Clock {
        window: window.clone(),
        elapsed_ms: elapsed_ms.clone(),
    }))
    .unwrap();
    (window, elapsed_ms)
}

#[test]
fn editor_field_expands_on_pointer_input_and_keeps_history_tools_in_formula_mode() {
    use slint::platform::WindowEvent;

    let (adapter, elapsed_ms) = timed_window();
    let main = MainWindow::new().unwrap();
    let theme = main.global::<Theme>();
    theme.set_motion(MotionPreference::Full);
    main.set_editor_visible(true);
    main.set_editor_title("新增题目".into());
    main.set_editor_fields(ModelRc::new(VecModel::from(vec![
        AnswerOption::default();
        6
    ])));
    main.set_math_text_mode(true);
    main.set_math_can_undo(true);
    main.set_math_can_redo(true);
    let weak = main.as_weak();
    main.on_activate_content(move |index| weak.upgrade().unwrap().set_math_active_field(index));
    let weak = main.as_weak();
    main.on_set_content_formula(move |formula| {
        let main = weak.upgrade().unwrap();
        main.set_math_field_formula(formula);
        main.set_math_has_formula(formula);
        main.set_math_text_mode(!formula);
    });
    let actions = Rc::new(Cell::new(0));
    let count = actions.clone();
    main.on_math_tool(move |command| {
        assert_eq!(command, "clear");
        count.set(count.get() + 1);
    });
    let count = actions.clone();
    main.on_undo_math(move |_| count.set(count.get() + 1));
    main.show().unwrap();
    main.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    adapter.set_size(PhysicalSize::new(880, 640));
    paint(&adapter);

    // The padding below the image toolbar exposes the left edge of the field.
    let color = theme.get_soft().to_argb_u8();
    let x = main.get_question_area_left().round() as usize + 8;
    let first_field = || {
        let pixels = paint(&adapter);
        let stride = pixels.width() as usize;
        let limit = pixels.height() as usize;
        let is_field = |y: usize| {
            let p = pixels.as_slice()[y * stride + x];
            (p.r, p.g, p.b) == (color.red, color.green, color.blue)
        };
        let y = (main.get_question_area_top().ceil() as usize..limit - 40)
            .find(|&y| (y..y + 40).all(is_field))
            .unwrap();
        (y, (y..limit).take_while(|&y| is_field(y)).count())
    };
    let (top, compact) = first_field();
    click(&main, &adapter, x as f32 + 22.0, top as f32 + 20.0);
    assert_eq!(main.get_math_active_field(), 0);
    assert!(first_field().1.abs_diff(compact) <= 1);
    elapsed_ms.set(1045);
    let intermediate = first_field().1;
    elapsed_ms.set(1230);
    let (top, text_height) = first_field();
    assert!(compact < intermediate && intermediate < text_height);
    assert!(text_height < 100, "a plain input has room for its field");
    save_preview("editor_field_text", &paint(&adapter));

    let right = 880.0 - 24.0 - 12.0;
    let click_history = |top: usize| {
        for offset in [100.0, 64.0, 28.0] {
            click(&main, &adapter, right - offset, top as f32 + 28.0);
        }
    };
    click_history(top);
    assert_eq!(actions.get(), 0);

    click(&main, &adapter, right - 34.0, top as f32 - 20.0);
    assert!(main.get_math_field_formula());
    assert!(first_field().1.abs_diff(text_height) <= 1);
    elapsed_ms.set(1460);
    let (top, formula_height) = first_field();
    assert!(formula_height > text_height + 100);
    save_preview("editor_field_formula", &paint(&adapter));
    click_history(top);
    assert_eq!(actions.get(), 3);

    theme.set_motion(MotionPreference::Reduced);
    paint(&adapter);
    click(&main, &adapter, right - 101.0, top as f32 - 20.0);
    assert!(!main.get_math_field_formula());
    assert_eq!(first_field().1, text_height);
    theme.set_motion(MotionPreference::Full);
    main.set_math_active_field(-1);
    paint(&adapter);
    main.invoke_restore_draft_focus();
    paint(&adapter);
    key(&main, &adapter, Key::Return);
    assert_eq!(main.get_math_active_field(), 0);
    assert_eq!(first_field().1, text_height);
}

#[test]
fn image_picker_keeps_its_vertical_bounds_across_editor_modes() {
    use slint::LogicalPosition;
    use slint::platform::{PointerEventButton, WindowEvent};

    let adapter = software_window();
    let main = MainWindow::new().unwrap();
    main.window()
        .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.25 });
    main.global::<WindowFrame>().set_enabled(true);
    main.set_editor_visible(true);
    main.set_editor_title("新增题目".into());
    main.set_editor_fields(ModelRc::new(VecModel::from(vec![
        AnswerOption::default();
        6
    ])));
    let picked = Rc::new(Cell::new(false));
    let signaled = picked.clone();
    main.on_choose_question_image(move || signaled.set(true));
    main.show().unwrap();
    adapter.set_size(PhysicalSize::new(1100, 842));

    let mut bounds = Vec::new();
    for (mode, name) in ["fill", "choice", "judgment"].into_iter().enumerate() {
        main.set_editor_answer_mode(mode as i32);
        save_preview(&format!("image_picker_{name}"), &paint(&adapter));
        let x = main.get_question_area_left() + 32.0;
        let top = main.get_question_area_top().ceil() as i32;
        let bottom = top + main.get_question_area_height().floor() as i32;
        // Measure the actual hit area in logical pixels.
        let active_rows = (top..bottom)
            .filter(|&y| {
                let position = LogicalPosition::new(x, y as f32);
                main.window().dispatch_event(WindowEvent::PointerPressed {
                    position,
                    button: PointerEventButton::Left,
                });
                main.window().dispatch_event(WindowEvent::PointerReleased {
                    position,
                    button: PointerEventButton::Left,
                });
                picked.replace(false)
            })
            .collect::<Vec<_>>();
        bounds.push((*active_rows.first().unwrap(), *active_rows.last().unwrap()));
    }
    assert!(bounds.iter().all(|&area| area == bounds[0]), "{bounds:?}");
    assert_eq!(bounds[0].1 - bounds[0].0 + 1, 32);
}

#[test]
fn editor_mode_indicator_slides_only_for_accepted_pointer_switches() {
    let (adapter, elapsed_ms) = timed_window();
    let main = MainWindow::new().unwrap();
    let theme = main.global::<Theme>();
    theme.set_motion(MotionPreference::Full);
    main.set_editor_visible(true);
    main.set_editor_new_pack(true);
    main.set_editor_title("新建知识包".into());
    main.set_editor_fields(ModelRc::new(VecModel::from(vec![
        AnswerOption::default();
        6
    ])));
    let accept = Rc::new(Cell::new(true));
    let allowed = accept.clone();
    let weak = main.as_weak();
    main.on_set_editor_answer_mode(move |mode| {
        if allowed.get() {
            weak.upgrade().unwrap().set_editor_answer_mode(mode);
        }
    });
    main.show().unwrap();
    adapter.set_size(PhysicalSize::new(880, 640));

    let initial = paint(&adapter);
    let primary = theme.get_primary().to_argb_u8();
    let is_primary = |pixel: &Rgb8Pixel| {
        (pixel.r, pixel.g, pixel.b) == (primary.red, primary.green, primary.blue)
    };
    let row_width = initial.width() as usize;
    let (sample_y, first, segment_width) = initial
        .as_slice()
        .chunks_exact(row_width)
        .take(main.get_question_area_top().ceil() as usize)
        .enumerate()
        .filter_map(|(y, row)| {
            let left = row.iter().position(is_primary)?;
            let right = row.iter().rposition(is_primary)?;
            Some((y, left, right - left + 1))
        })
        .max_by_key(|(_, _, width)| *width)
        .unwrap();
    assert!(
        segment_width > 150,
        "sample the indicator instead of the Save button"
    );
    let left_at = |index: usize| (first + index * (segment_width + 4)) as f32;
    let indicator_left = || {
        paint(&adapter).as_slice()[sample_y * row_width..(sample_y + 1) * row_width]
            .iter()
            .position(is_primary)
            .unwrap() as f32
    };
    let click_mode = |index| {
        click(
            &main,
            &adapter,
            left_at(index) + segment_width as f32 / 2.0,
            sample_y as f32,
        );
    };

    click_mode(1);
    assert_eq!(main.get_editor_answer_mode(), 1);
    assert!((indicator_left() - left_at(0)).abs() < 2.0);
    elapsed_ms.set(1060);
    let moving = indicator_left();
    save_preview("editor_mode_moving", &paint(&adapter));
    assert!(
        moving > left_at(0) + 10.0 && moving < left_at(1) - 10.0,
        "expected the indicator between {} and {}, got {moving} at row {sample_y}; active animations: {}",
        left_at(0),
        left_at(1),
        main.window().has_active_animations()
    );
    click_mode(2);
    assert_eq!(main.get_editor_answer_mode(), 2);
    assert!((indicator_left() - moving).abs() < 2.0);
    elapsed_ms.set(1260);
    assert!((indicator_left() - left_at(2)).abs() < 3.0);
    save_preview("editor_mode_selected", &paint(&adapter));

    key(&main, &adapter, Key::Home);
    assert_eq!(main.get_editor_answer_mode(), 0);
    assert!((indicator_left() - left_at(0)).abs() < 2.0);
    key(&main, &adapter, Key::RightArrow);
    assert_eq!(main.get_editor_answer_mode(), 1);
    assert!((indicator_left() - left_at(1)).abs() < 3.0);
    accept.set(false);
    click_mode(0);
    assert_eq!(main.get_editor_answer_mode(), 1);
    assert!((indicator_left() - left_at(1)).abs() < 3.0);

    accept.set(true);
    click_mode(0);
    theme.set_motion(MotionPreference::Reduced);
    assert!((indicator_left() - left_at(0)).abs() < 2.0);
    click_mode(2);
    assert!((indicator_left() - left_at(2)).abs() < 3.0);
    theme.set_motion(MotionPreference::Full);
    assert!((indicator_left() - left_at(2)).abs() < 3.0);
}

#[test]
fn pointer_switch_follows_settings_tab_order() {
    let (adapter, elapsed_ms) = timed_window();
    let main = MainWindow::new().unwrap();
    slint::select_bundled_translation("en").unwrap();
    let theme = main.global::<Theme>();
    theme.set_motion(MotionPreference::Full);
    main.set_current_tab(1);
    main.set_settings_section(1);
    main.show().unwrap();
    adapter.set_size(PhysicalSize::new(880, 640));
    paint(&adapter);

    let content_top = main.get_settings_content_top();
    let settled_left = main.get_reminder_focus_left();
    let settled_top = main.get_reminder_focus_top();
    let click_control = |role, label: &str| {
        let root = ItemRc::new_root(WindowInner::from_pub(main.window()).component());
        let center = root
            .visit_descendants(|item| {
                if item.accessible_role() == role
                    && item
                        .accessible_string_property(AccessibleStringProperty::Label)
                        .is_some_and(|text| text.as_str() == label)
                {
                    return ControlFlow::Break(item.map_to_window(item.geometry().center()));
                }
                ControlFlow::Continue(())
            })
            .unwrap_or_else(|| panic!("missing settings control {label}"));
        click(&main, &adapter, center.x, center.y);
    };
    let select_tab = |label| click_control(AccessibleRole::Tab, label);
    main.set_settings_section(0);
    paint(&adapter);
    select_tab("Card");
    assert_eq!(main.get_settings_section(), 1);
    assert!(main.window().has_active_animations());
    let from_right = main.get_reminder_focus_left();
    assert!(from_right - settled_left >= 5.0);
    assert_eq!(main.get_reminder_focus_top(), settled_top);
    elapsed_ms.set(1080);
    paint(&adapter);
    let during = main.get_reminder_focus_left();
    assert!(from_right > during && during - settled_left >= 1.0);
    select_tab("Card");
    assert_eq!(main.get_reminder_focus_left(), during);
    elapsed_ms.set(1230);
    paint(&adapter);
    assert_eq!(main.get_reminder_focus_left(), settled_left);
    assert_eq!(main.get_settings_content_top(), content_top);

    select_tab("General");
    assert_eq!(main.get_settings_section(), 2);
    select_tab("Card");
    assert_eq!(main.get_settings_section(), 1);
    assert!(settled_left - main.get_reminder_focus_left() >= 5.0);
    assert_eq!(main.get_reminder_focus_top(), settled_top);
    theme.set_motion(MotionPreference::Reduced);
    paint(&adapter);
    assert_eq!(main.get_reminder_focus_left(), settled_left);

    theme.set_motion(MotionPreference::Full);
    main.set_current_tab(0);
    paint(&adapter);
    click_control(AccessibleRole::Button, "Settings");
    assert_eq!(main.get_current_tab(), 1);
    assert_eq!(main.get_reminder_focus_left(), settled_left);
    assert_eq!(main.get_reminder_focus_top(), settled_top);

    select_tab("Card");
    key(&main, &adapter, Key::End);
    assert_eq!(main.get_settings_section(), 2);
    key(&main, &adapter, Key::LeftArrow);
    assert_eq!(main.get_settings_section(), 1);
    assert_eq!(main.get_reminder_focus_left(), settled_left);
    key(&main, &adapter, Key::Home);
    assert_eq!(main.get_settings_section(), 0);
    key(&main, &adapter, Key::LeftArrow);
    assert_eq!(main.get_settings_section(), 2);
    key(&main, &adapter, Key::RightArrow);
    assert_eq!(main.get_settings_section(), 0);
}

#[test]
fn correction_outline_contracts_and_stays_off_when_motion_is_reenabled() {
    let (window, elapsed_ms) = timed_window();
    let card = PracticeWindow::new().unwrap();
    let theme = card.global::<Theme>();
    theme.set_motion(MotionPreference::Full);
    let frame = Rc::new(Cell::new((0.0, slint::Color::default())));
    let captured = frame.clone();
    card.on_card_ripple_frame(move |outset, ink| captured.set((outset, ink)));
    card.show().unwrap();
    window.set_size(PhysicalSize::new(320, 200));
    let advance = |ms| {
        elapsed_ms.set(elapsed_ms.get() + ms);
        paint(&window);
        frame.get()
    };
    advance(0);
    card.set_feedback(2);
    advance(0);
    let (first_outset, ink) = advance(40);
    let (later_outset, later_ink) = advance(60);
    assert!(first_outset > later_outset && later_outset > 0.0);
    assert!(ink.alpha() > later_ink.alpha() && later_ink.alpha() > 0);
    assert_eq!(
        slint::Color::from_rgb_u8(ink.red(), ink.green(), ink.blue()),
        theme.get_wrong_ink()
    );
    theme.set_motion(MotionPreference::Reduced);
    assert_eq!(advance(1).1.alpha(), 0);
    theme.set_motion(MotionPreference::Full);
    assert_eq!(advance(20).1.alpha(), 0);
    card.hide().unwrap();
}

#[test]
fn card_hint_control_is_reversible() {
    let adapter = software_window();
    let main = MainWindow::new().unwrap();
    main.set_current_tab(1);
    main.set_settings_section(1);
    let weak = main.as_weak();
    main.on_set_show_practice_hints(move |enabled| weak.unwrap().set_show_practice_hints(enabled));
    main.show().unwrap();
    adapter.set_size(PhysicalSize::new(760, 530));
    paint(&adapter);
    let x = main.get_settings_left() + main.get_settings_width() - 32.0;
    let y = main.get_settings_content_top() + 168.0;
    assert!(main.get_show_practice_hints());
    click(&main, &adapter, x, y);
    assert!(!main.get_show_practice_hints());
    save_preview("card_settings", &paint(&adapter));
    click(&main, &adapter, x, y);
    assert!(main.get_show_practice_hints());
    main.hide().unwrap();
}

#[test]
fn custom_card_position_can_return_to_a_corner_while_locked() {
    use slint::platform::WindowEvent;

    let adapter = software_window();
    let main = MainWindow::new().unwrap();
    main.set_current_tab(1);
    main.set_settings_section(1);
    main.set_has_custom_position(true);
    main.global::<Theme>().set_motion(MotionPreference::Reduced);
    let weak = main.as_weak();
    main.on_set_card_locked(move |locked| weak.unwrap().set_card_locked(locked));
    let corners = Rc::new(Cell::new(None));
    let chosen = corners.clone();
    let weak = main.as_weak();
    main.on_set_corner(move |corner| {
        chosen.set(Some(corner));
        let main = weak.unwrap();
        main.set_popup_corner(corner);
        main.set_has_custom_position(false);
    });
    main.show().unwrap();
    main.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    adapter.set_size(PhysicalSize::new(880, 640));
    paint(&adapter);

    let x = main.get_settings_left() + main.get_settings_width() - 36.0;
    let position_y = main.get_settings_content_top() + 24.0;
    click(&main, &adapter, x, position_y + 48.0);
    assert!(main.get_card_locked());
    click(&main, &adapter, x, position_y);
    key(&main, &adapter, Key::Return);
    assert_eq!(corners.get(), None);
    assert!(main.get_has_custom_position());

    click(&main, &adapter, x, position_y);
    key(&main, &adapter, Key::UpArrow);
    key(&main, &adapter, Key::Return);
    assert_eq!(corners.get(), Some(3));
    assert!(!main.get_has_custom_position());
    assert!(main.get_card_locked());
    main.hide().unwrap();
}

#[test]
fn practice_card_reflows_to_a_manual_size_and_locks_moving_and_resizing() {
    use slint::private_unstable_api::re_exports::{ItemTreeRc, Orientation, WindowInner};

    let adapter = software_window();
    let card = PracticeWindow::new().unwrap();
    card.set_resize_available(true);
    card.set_prompt(
        "A long prompt with words that wrap into multiple lines. "
            .repeat(20)
            .into(),
    );
    card.show().unwrap();
    let window = WindowInner::from_pub(card.window());
    let component = window.component();
    let layout = ItemTreeRc::borrow_pin(&component);
    let horizontal = || layout.as_ref().layout_info(Orientation::Horizontal);
    let vertical = || layout.as_ref().layout_info(Orientation::Vertical);
    let item = window.window_item().unwrap();
    let border = || item.as_pin_ref().resize_border_width().get();
    assert_eq!(horizontal().preferred_bounded(), 320.0);
    assert_eq!((horizontal().min, vertical().min), (320.0, 288.0));
    assert!(horizontal().min < horizontal().max && vertical().min < vertical().max);
    assert!(border() > 0.0);
    assert!(card.get_move_enabled());

    card.set_resizing(true);
    adapter.set_size(PhysicalSize::new(480, 320));
    card.set_pack_tag("Subject".into());
    paint(&adapter);
    assert_eq!(item.as_pin_ref().height().get(), 320.0);

    card.set_card_width(480.0);
    card.set_card_height(320.0);
    card.set_resizing(false);
    card.set_feedback(2);
    card.set_knowledge_only(true);
    paint(&adapter);
    assert_eq!(item.as_pin_ref().height().get(), 320.0);
    assert!(card.get_content_height() > card.get_visible_content_height());
    assert_eq!(horizontal().preferred_bounded(), 480.0);
    assert_eq!(vertical().preferred_bounded(), 320.0);

    card.set_card_locked(true);
    assert_eq!(border(), 0.0);
    assert!(!card.get_move_enabled());
    assert_eq!((horizontal().min, horizontal().max), (480.0, 480.0));
    assert_eq!((vertical().min, vertical().max), (320.0, 320.0));
    card.set_card_locked(false);
    assert!(border() > 0.0);
    assert!(card.get_move_enabled());
    assert!(horizontal().min < horizontal().max && vertical().min < vertical().max);

    card.set_card_width(280.0);
    card.set_card_height(190.0);
    assert_eq!(horizontal().preferred_bounded(), 320.0);
    assert_eq!(vertical().preferred_bounded(), 288.0);
    card.set_card_locked(true);
    assert_eq!((horizontal().min, horizontal().max), (320.0, 320.0));
    assert_eq!((vertical().min, vertical().max), (288.0, 288.0));
    card.hide().unwrap();
}

#[test]
fn minimum_card_size_leaves_the_first_formula_keys_reachable() {
    use slint::platform::WindowEvent;

    let adapter = software_window();
    let card = PracticeWindow::new().unwrap();
    card.set_resize_available(true);
    card.set_card_height(288.0);
    card.set_pack_tag("数学".into());
    card.set_prompt("sin(2x) = ?".into());
    card.global::<Theme>().set_motion(MotionPreference::Reduced);
    let weak = card.as_weak();
    card.on_open_structured_entry(move || {
        let card = weak.unwrap();
        let math = card.global::<MathInputState>();
        math.set_practice(true);
        math.set_common_keys(ModelRc::new(VecModel::from(vec![MathKey {
            label: "分式".into(),
            token: "frac".into(),
            ..Default::default()
        }])));
        math.set_keypad_expanded(true);
        math.set_open(true);
        card.invoke_focus_answer();
    });
    let pressed = Rc::new(Cell::new(false));
    let pressed_on_keypad = pressed.clone();
    card.global::<MathInputState>()
        .on_keypad(move |token, _| pressed_on_keypad.set(token == "frac"));
    card.show().unwrap();
    adapter.set_size(PhysicalSize::new(320, 288));
    card.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    card.invoke_focus_practice();
    paint(&adapter);
    click(
        &card,
        &adapter,
        card.get_formula_button_center_x(),
        card.get_formula_button_center_y(),
    );
    let math = card.global::<MathInputState>();
    assert!(math.get_open() && math.get_keypad_expanded());
    assert_eq!(card.get_content_scroll_y(), 0.0);
    let viewport_bottom = 50.0 + card.get_visible_content_height();
    assert!(math.get_keypad_top() + 36.0 <= viewport_bottom);
    click(
        &card,
        &adapter,
        math.get_keypad_left() + 16.0,
        math.get_keypad_top() + 16.0,
    );
    assert!(pressed.get());
    card.hide().unwrap();
}

#[test]
fn a_minimum_knowledge_card_can_scroll_to_its_answer_by_keyboard() {
    use slint::platform::WindowEvent;

    let adapter = software_window();
    let card = PracticeWindow::new().unwrap();
    card.set_knowledge_only(true);
    card.set_card_height(288.0);
    card.set_prompt(
        "A long explanation that needs several lines of the practice card. "
            .repeat(8)
            .into(),
    );
    card.set_standard_answer("The answer follows the explanation.".into());
    card.global::<Theme>().set_motion(MotionPreference::Reduced);
    card.show().unwrap();
    adapter.set_size(PhysicalSize::new(320, 288));
    card.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    card.invoke_focus_practice();
    paint(&adapter);

    let scroll_range = card.get_content_height() - card.get_visible_content_height();
    assert!(scroll_range > 0.0);
    assert_eq!(card.get_content_scroll_y(), 0.0);
    key(&card, &adapter, Key::PageDown);
    assert!(card.get_content_scroll_y() < 0.0);
    key(&card, &adapter, Key::End);
    assert_eq!(card.get_content_scroll_y(), -scroll_range);
    key(&card, &adapter, Key::UpArrow);
    assert!(card.get_content_scroll_y() > -scroll_range);
    key(&card, &adapter, Key::Home);
    assert_eq!(card.get_content_scroll_y(), 0.0);
    card.hide().unwrap();
}

#[test]
fn a_minimum_formula_card_shows_the_validation_error_above_the_open_keypad() {
    use slint::platform::WindowEvent;

    let adapter = software_window();
    let card = PracticeWindow::new().unwrap();
    card.set_card_height(288.0);
    card.set_prompt("Evaluate".into());
    card.set_show_practice_hints(false);
    card.global::<Theme>().set_motion(MotionPreference::Reduced);
    let math = card.global::<MathInputState>();
    math.set_practice(true);
    math.set_open(true);
    math.set_keypad_expanded(true);
    let submissions = Rc::new(Cell::new(0));
    let answer_submitted = submissions.clone();
    let weak = card.as_weak();
    card.on_submit_answer(move |_| {
        answer_submitted.set(answer_submitted.get() + 1);
        weak.unwrap()
            .global::<MathInputState>()
            .set_error("Complete the denominator".into());
    });
    card.show().unwrap();
    adapter.set_size(PhysicalSize::new(320, 288));
    card.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    card.invoke_focus_practice();
    let before = paint(&adapter);
    let previous_keypad_top = math.get_keypad_top() - card.get_answer_footer_top();

    key(&card, &adapter, Key::Return);
    let after = paint(&adapter);
    assert_eq!(math.get_error(), "Complete the denominator");
    assert!(math.get_keypad_expanded());
    let keypad_displacement =
        math.get_keypad_top() - card.get_answer_footer_top() - previous_keypad_top;
    assert!(keypad_displacement > 8.0);
    let message_bottom = || card.get_answer_footer_top() + 13.0 + keypad_displacement - 8.0;
    let viewport_bottom = 24.0 + card.get_visible_content_height();
    assert!(message_bottom() <= viewport_bottom);
    assert_ne!(after.as_bytes(), before.as_bytes());

    card.set_prompt(
        "Consider the following setup before completing the missing part of the formula. "
            .repeat(6)
            .into(),
    );
    paint(&adapter);
    card.invoke_resume_answer_input();
    paint(&adapter);
    let before_error_scroll = card.get_content_scroll_y();
    assert!(before_error_scroll < 0.0);
    assert!(message_bottom() > viewport_bottom);
    key(&card, &adapter, Key::Return);
    assert_eq!(submissions.get(), 2);
    let error_scroll = card.get_content_scroll_y();
    assert!(error_scroll < before_error_scroll);
    assert!(card.get_answer_footer_top() >= 24.0);
    assert!(message_bottom() <= viewport_bottom);
    key(&card, &adapter, Key::PageUp);
    assert!(card.get_content_scroll_y() > error_scroll);
    key(&card, &adapter, Key::Return);
    assert_eq!(submissions.get(), 3);
    assert_eq!(card.get_content_scroll_y(), error_scroll);
    card.hide().unwrap();
}

#[test]
fn long_prompt_keeps_its_opening_visible_and_resumes_typing_at_the_caret() {
    use slint::LogicalPosition;
    use slint::platform::WindowEvent;

    let adapter = software_window();
    let card = PracticeWindow::new().unwrap();
    card.set_resize_available(true);
    card.set_card_height(288.0);
    card.set_formula_input_allowed(false);
    card.set_pack_tag("常识".into());
    card.set_prompt(
        "Coordinated Universal Time is the reference for standard clocks around the world. "
            .repeat(4)
            .into(),
    );
    card.global::<Theme>().set_motion(MotionPreference::Reduced);
    let weak = card.as_weak();
    card.on_answer_edited(move |text| weak.unwrap().set_answer_text(text));
    card.show().unwrap();
    adapter.set_size(PhysicalSize::new(320, 288));
    card.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    paint(&adapter);

    card.invoke_focus_practice();
    paint(&adapter);
    let viewport_bottom = || 50.0 + card.get_visible_content_height();
    assert_eq!(card.get_content_scroll_y(), 0.0);
    assert!(card.get_answer_footer_top() - card.get_answer_row_height() > viewport_bottom());

    card.invoke_prepare_card_shortcut();
    key(&card, &adapter, Key::Menu);
    assert!(card.get_context_menu_open());
    card.invoke_resume_answer_input();
    assert_eq!(card.get_content_scroll_y(), 0.0);
    key(&card, &adapter, Key::Escape);
    assert!(!card.get_context_menu_open());

    let type_text = |text: &str| {
        card.invoke_resume_answer_input();
        card.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
        card.window()
            .dispatch_event(WindowEvent::KeyReleased { text: text.into() });
        paint(&adapter);
    };
    type_text("a");
    type_text("b");
    assert_eq!(card.get_answer_text(), "ab");
    assert!(card.get_answer_footer_top() <= viewport_bottom() + 0.5);
    key(&card, &adapter, Key::LeftArrow);

    card.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(160.0, 96.0),
        delta_x: 0.0,
        delta_y: 1000.0,
    });
    paint(&adapter);
    assert_eq!(card.get_content_scroll_y(), 0.0);
    assert!(card.get_answer_footer_top() - card.get_answer_row_height() > viewport_bottom());

    type_text("界");
    assert_eq!(card.get_answer_text(), "a界b");
    assert!(card.get_answer_footer_top() <= viewport_bottom() + 0.5);
    card.hide().unwrap();
}

#[test]
fn resized_practice_card_centers_short_answers_and_keeps_long_content_reachable() {
    use slint::LogicalPosition;
    use slint::platform::WindowEvent;

    let (adapter, _elapsed_ms) = timed_window();
    let card = PracticeWindow::new().unwrap();
    card.set_resize_available(true);
    card.set_prompt("UTC".into());
    card.set_standard_answer("协调世界时".into());
    card.set_pack_tag("常识".into());
    card.set_formula_input_allowed(false);
    card.global::<WindowFrame>().set_enabled(true);
    card.global::<Theme>().set_motion(MotionPreference::Reduced);
    card.show().unwrap();
    card.window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    card.invoke_focus_answer();
    card.set_resizing(true);

    let resize = |width, height| {
        adapter.set_size(PhysicalSize::new(width, height));
        paint(&adapter)
    };
    let check_caret = |frame: &SharedPixelBuffer<Rgb8Pixel>| {
        let width = frame.width() as usize;
        let y = (card.get_answer_footer_top() - card.get_answer_row_height() / 2.0) as usize;
        let left = card.get_answer_footer_left() as usize;
        let right = left + card.get_answer_footer_width() as usize;
        let caret = (left + 3..right - 3)
            .find(|x| {
                let p = frame.as_slice()[y * width + x];
                p.r > 160 && p.g > 160 && p.b > 160
            })
            .expect("the empty answer field renders a bright caret");
        assert!(
            caret.abs_diff(width / 2) <= 2,
            "caret at {caret} in a {width}-pixel card"
        );
    };
    let compact = resize(320, 288);
    let compact_footer = card.get_answer_footer_top();
    assert_eq!(card.get_answer_footer_width(), 272.0);
    assert_eq!(card.get_answer_footer_left(), 24.0);
    check_caret(&compact);
    save_preview("practice_compact", &compact);

    let large = resize(834, 586);
    let wide_footer = card.get_answer_footer_top();
    let answer_width = card.get_answer_footer_width();
    assert_eq!(answer_width, 480.0);
    assert_eq!(card.get_answer_footer_left(), (834.0 - answer_width) / 2.0);
    let answer_center = wide_footer - card.get_answer_row_height() / 2.0;
    assert!((answer_center - 586.0 / 2.0).abs() < 32.0);
    check_caret(&large);
    save_preview("practice_wide", &large);

    let tall = resize(320, 586);
    assert_eq!(card.get_answer_footer_width(), 272.0);
    assert_eq!(card.get_answer_footer_top(), wide_footer);
    save_preview("practice_tall", &tall);
    resize(834, 288);
    assert_eq!(card.get_answer_footer_width(), answer_width);
    assert_eq!(card.get_answer_footer_top(), compact_footer);

    card.set_prompt(
        "Coordinated Universal Time is the common reference for clocks around the world. "
            .repeat(4)
            .into(),
    );
    resize(320, 288);
    assert!(card.get_content_height() > card.get_visible_content_height());
    card.window().dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(160.0, 100.0),
        delta_x: 0.0,
        delta_y: -120.0,
    });
    paint(&adapter);
    assert!(card.get_content_scroll_y() < 0.0);
    let expanded = resize(834, 586);
    assert_eq!(card.get_content_height(), card.get_visible_content_height());
    assert_eq!(card.get_content_scroll_y(), 0.0);
    save_preview("practice_wide_long_prompt", &expanded);

    card.set_prompt("UTC".into());
    card.set_knowledge_only(true);
    save_preview("practice_wide_knowledge", &paint(&adapter));
    card.set_knowledge_only(false);
    card.set_answer_text("格林尼治时间".into());
    card.set_feedback(2);
    save_preview("practice_wide_correction", &paint(&adapter));
    card.set_feedback(0);
    card.set_answer_mode(1);
    card.set_options(ModelRc::new(VecModel::from(vec![
        AnswerOption {
            label: "Universal".into(),
            ..Default::default()
        },
        AnswerOption {
            label: "Local time".into(),
            ..Default::default()
        },
        AnswerOption {
            label: "Time zone".into(),
            ..Default::default()
        },
    ])));
    let wide_choices = paint(&adapter);
    assert!(!card.get_stacked_options());
    save_preview("practice_wide_choice", &wide_choices);
    resize(320, 288);
    assert!(card.get_stacked_options());

    card.set_answer_mode(0);
    card.set_answer_text("".into());
    let mut picture = SharedPixelBuffer::<Rgb8Pixel>::new(80, 160);
    picture.make_mut_slice().fill(Rgb8Pixel {
        r: 83,
        g: 124,
        b: 154,
    });
    card.set_question_image(slint::Image::from_rgb8(picture));
    card.set_question_image_width(80.0);
    card.set_question_image_height(160.0);
    let previews = Rc::new(Cell::new(0));
    let opened = previews.clone();
    card.on_preview_question_image(move || opened.set(opened.get() + 1));
    let pictured = resize(834, 586);
    let check_portrait = |frame: &SharedPixelBuffer<Rgb8Pixel>| {
        let row = frame
            .as_slice()
            .chunks_exact(frame.width() as usize)
            .find(|row| {
                row.iter()
                    .filter(|p| p.r == 83 && p.g == 124 && p.b == 154)
                    .count()
                    == 110
            })
            .expect("the 1:2 portrait renders at 110 by 220");
        assert!(!row.iter().any(|p| p.r > 240 && p.g > 240 && p.b > 240));
    };
    check_portrait(&pictured);
    let picture_y = card.get_answer_footer_top() - card.get_answer_row_height() - 20.0 - 110.0;
    click(&card, &adapter, 417.0, picture_y);
    assert_eq!(previews.get(), 1);
    save_preview("practice_wide_picture", &pictured);
    card.set_knowledge_only(true);
    check_portrait(&paint(&adapter));
    card.set_knowledge_only(false);

    card.set_question_image_width(0.0);
    card.set_question_image_height(0.0);
    card.set_formula_input_allowed(true);
    let math = card.global::<MathInputState>();
    math.set_practice(true);
    math.set_open(true);
    math.set_keypad_expanded(true);
    resize(320, 288);
    assert!(card.get_content_height() > card.get_visible_content_height());
    let formula = resize(834, 586);
    assert_eq!(math.get_keypad_width(), answer_width);
    assert!(math.get_pad_bottom() < 586.0);
    save_preview("practice_wide_formula", &formula);
    card.hide().unwrap();
}

#[test]
fn hiding_card_hints_keeps_shortcuts_and_corrections_available() {
    use slint::platform::WindowEvent;

    let adapter = software_window();
    let card = PracticeWindow::new().unwrap();
    // The software fixture also runs on images without CJK fonts.
    slint::select_bundled_translation("en").unwrap();
    card.global::<WindowFrame>().set_enabled(true);
    card.set_prompt("hesitate".into());
    card.set_standard_answer("pause".into());
    card.set_answer_text("pause".into());
    card.set_pack_tag("English".into());
    card.global::<Theme>().set_motion(MotionPreference::Reduced);
    card.show().unwrap();
    adapter.set_size(PhysicalSize::new(320, 226));
    let layout = || {
        (
            card.get_content_height(),
            card.get_visible_content_height(),
            card.get_content_scroll_y(),
            card.get_prompt_area_height(),
            card.get_answer_row_height(),
            card.get_answer_footer_top(),
        )
    };
    let check_hint_visibility = || {
        card.set_show_practice_hints(true);
        let shown = paint(&adapter);
        let original_layout = layout();
        assert!(
            footer_has_text(&card, &shown),
            "hint missing in mode {}, feedback {}, stacked {}, frame {}x{}, layout {:?}",
            card.get_answer_mode(),
            card.get_feedback(),
            card.get_stacked_options(),
            shown.width(),
            shown.height(),
            original_layout,
        );

        card.set_show_practice_hints(false);
        let hidden = paint(&adapter);
        assert_eq!(layout(), original_layout);
        assert!(!footer_has_text(&card, &hidden));
        (shown, hidden)
    };
    let (shown, hidden) = check_hint_visibility();
    let answering_height = card.get_visible_content_height();
    save_preview("fill_with_hints", &shown);
    save_preview("fill_without_hints", &hidden);
    card.set_awaiting_interaction(true);
    check_hint_visibility();
    card.set_awaiting_interaction(false);

    card.set_input_error("Complete the denominator".into());
    assert!(footer_has_text(&card, &paint(&adapter)));
    card.set_input_error("".into());
    let weak = card.as_weak();
    card.on_submit_answer(move |_| weak.unwrap().set_feedback(1));
    card.invoke_focus_answer();
    key(&card, &adapter, Key::Return);
    assert_eq!(card.get_feedback(), 1);
    assert_eq!(card.get_visible_content_height(), answering_height);

    card.set_feedback(0);
    let chosen = Rc::new(Cell::new(-1));
    let selected = chosen.clone();
    card.on_choose_answer(move |index| selected.set(index));
    for (mode, labels) in [
        (1, &["wait", "pause", "hurry"][..]),
        (
            1,
            &[
                "mass divided by volume",
                "length divided by time",
                "force divided by area",
            ][..],
        ),
        (2, &["True", "False"][..]),
    ] {
        card.set_options(ModelRc::new(VecModel::from(
            labels
                .iter()
                .map(|&label| AnswerOption {
                    label: label.into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        )));
        card.set_answer_mode(mode);
        card.invoke_focus_answer();
        paint(&adapter);
        let height = if card.get_stacked_options() { 332 } else { 226 };
        adapter.set_size(PhysicalSize::new(320, height));
        check_hint_visibility();
        chosen.set(-1);
        card.window()
            .dispatch_event(WindowEvent::KeyPressed { text: "2".into() });
        card.window()
            .dispatch_event(WindowEvent::KeyReleased { text: "2".into() });
        assert_eq!(chosen.get(), 1);
    }
    card.set_selected_option(1);
    card.set_correct_option(0);
    card.set_feedback(2);
    card.invoke_focus_answer();
    check_hint_visibility();
    let dismissed = Rc::new(Cell::new(false));
    let closed = dismissed.clone();
    card.on_dismiss(move || closed.set(true));
    key(&card, &adapter, Key::Escape);
    assert!(!dismissed.get());
    key(&card, &adapter, Key::Return);
    assert!(dismissed.get());
    dismissed.set(false);
    card.set_show_practice_hints(false);
    save_preview("wrong_answer_without_hints", &paint(&adapter));
    click(
        &card,
        &adapter,
        card.window().size().width as f32 - 26.0,
        card.window().size().height as f32 - 20.0,
    );
    assert!(
        dismissed.get(),
        "the wrong-answer card can close without hints"
    );
    card.set_answer_mode(0);
    card.set_answer_text("hurry".into());
    assert!(
        footer_has_text(&card, &paint(&adapter)),
        "the submitted answer stays visible below the correction"
    );
    card.hide().unwrap();
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

    if std::env::var_os("TRY_UI_PREVIEW_DIR").is_some() {
        for (name, dark) in [("settings_dark", true), ("settings_light", false)] {
            main.set_dark_theme(dark);
            save_preview(name, &paint(&adapter));
        }
    }
}
