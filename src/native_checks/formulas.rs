//! Explicit native checks for formulas.
use super::*;

fn check_script_operators(checks: &AnswerChecks, card: &PracticeWindow, keypad: bool) {
    let ui = card.global::<crate::MathInputState>();
    ui.invoke_keypad("key:clear".into(), false);
    for token in ["P", if keypad { "key:power" } else { "^" }, "n", "+", "m"] {
        if keypad {
            ui.invoke_keypad(token.into(), false);
        } else {
            key(card.window(), token.into());
        }
    }
    assert_eq!(
        checks
            .app
            .structured_math
            .borrow()
            .as_ref()
            .unwrap()
            .1
            .complete()
            .unwrap(),
        "P^{n+m}",
        "native script input, keypad={keypad}"
    );
    ui.invoke_keypad("key:next".into(), false);
    key(card.window(), "+".into());
    key(card.window(), "q".into());
    assert_eq!(
        checks
            .app
            .structured_math
            .borrow()
            .as_ref()
            .unwrap()
            .1
            .complete()
            .unwrap(),
        "P^{n+m}+q"
    );
    ui.invoke_keypad("key:clear".into(), false);
}

fn scroll_math_catalog(card: &PracticeWindow, delta_y: f32) {
    let ui = card.global::<crate::MathInputState>();
    let position = slint::LogicalPosition::new(
        ui.get_keypad_left() + ui.get_keypad_width() / 2.0,
        ui.get_keypad_top() + 70.0,
    );
    card.window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    card.window().dispatch_event(WindowEvent::PointerScrolled {
        position,
        delta_x: 0.0,
        delta_y,
    });
}

fn practice_bottom(card: &PracticeWindow) -> i32 {
    let (_, y) = FloatingWindow::position(card.window()).unwrap();
    y + card.window().size().height as i32
}

fn structured_math_phase(checks: Rc<AnswerChecks>, bottom: Rc<Cell<i32>>, phase: u8) {
    let delay = if phase >= 17 { 140 } else { 450 };
    Timer::single_shot(Duration::from_millis(delay), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    main.invoke_edit_card(0);
                    main.invoke_activate_content(1);
                    main.invoke_open_structured_math();
                    assert!(main.global::<crate::MathInputState>().get_open());
                }
                1 => {
                    let main = main_window(app);
                    let ui = main.global::<crate::MathInputState>();
                    ui.invoke_keypad("key:clear".into(), false);
                    key(main.window(), "2/".into());
                    assert!(!ui.get_complete());
                    ui.invoke_apply();
                    assert!(!ui.get_error().is_empty());
                    main.invoke_set_editor_answer_mode(2);
                    assert_eq!(main.get_editor_answer_mode(), 0);
                    assert!(ui.get_open());
                    assert_eq!(
                        app.math_fields.borrow()[1].content().unwrap().1,
                        r"\frac{1}{2}"
                    );
                    snapshot(
                        main.window(),
                        checks.output.join("authoring_incomplete.png"),
                    );
                }
                2 => {
                    let main = main_window(app);
                    let ui = main.global::<crate::MathInputState>();
                    key(main.window(), "3".into());
                    assert!(ui.get_complete());
                    ui.invoke_key("a".into(), true, false, false);
                    snapshot(main.window(), checks.output.join("authoring_inline.png"));
                    ui.invoke_apply();
                    assert!(!ui.get_open());
                    assert!(app.structured_math.borrow().is_none());
                    assert!(
                        app.math_fields.borrow()[1]
                            .content()
                            .unwrap()
                            .1
                            .contains(r"\frac{2}{3}")
                    );
                }
                3 => {
                    let main = main_window(app);
                    main.invoke_undo_math(false);
                    assert_eq!(app.math_fields.borrow()[1].root.plain(), "1/2");
                    main.invoke_undo_math(true);
                    assert_eq!(app.math_fields.borrow()[1].root.plain(), "2/3");
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    let saved = app.store.load().unwrap();
                    assert!(
                        saved.packs[0].cards[0]
                            .answer_latex
                            .as_ref()
                            .unwrap()
                            .contains(r"\frac{2}{3}")
                    );
                    main.invoke_edit_card(0);
                    main.invoke_activate_content(1);
                    main.invoke_open_structured_math();
                }
                4 => {
                    let main = main_window(app);
                    let ui = main.global::<crate::MathInputState>();
                    ui.invoke_keypad("key:clear".into(), false);
                    key(main.window(), "9".into());
                    key(main.window(), Key::Escape.into());
                    assert!(!ui.get_open());
                    assert_eq!(app.math_fields.borrow()[1].root.plain(), "2/3");
                    main.invoke_cancel_editor();
                    app.show_practice().unwrap();
                }
                5 => {
                    let card = practice_window(app);
                    bottom.set(practice_bottom(&card));
                    key(card.window(), "1/2".into());
                    let ui = card.global::<crate::MathInputState>();
                    assert!(
                        ui.get_open(),
                        "typing a formula before opening the keypad must use the structured editor"
                    );
                    assert!(!ui.get_keypad_expanded());
                    assert_eq!(
                        app.structured_math
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .1
                            .complete()
                            .unwrap(),
                        r"\frac{1}{2}"
                    );
                    // Whole-expression import leaves the caret after the
                    // fraction; enter its denominator before row navigation.
                    key(card.window(), Key::LeftArrow.into());
                    key(card.window(), Key::Home.into());
                    key(card.window(), "3".into());
                    assert_eq!(
                        app.structured_math
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .1
                            .complete()
                            .unwrap(),
                        r"\frac{1}{32}",
                        "the collapsed editor must move the caret inside the denominator"
                    );
                    key(card.window(), Key::Home.into());
                    modified_key(card.window(), Key::Shift, Key::RightArrow.into());
                    key(card.window(), "4".into());
                    assert_eq!(
                        app.structured_math
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .1
                            .complete()
                            .unwrap(),
                        r"\frac{1}{42}",
                        "Shift selection replaces only the selected denominator digit"
                    );
                    let caret = ui.get_formula();
                    key(card.window(), Key::End.into());
                    pointer_click(
                        card.window(),
                        slint::LogicalPosition::new(
                            card.get_input_canvas_left() + caret.caret_x,
                            card.get_input_canvas_top() + caret.caret_y + caret.caret_height / 2.0,
                        ),
                    );
                    key(card.window(), "5".into());
                    assert_eq!(
                        app.structured_math
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .1
                            .complete()
                            .unwrap(),
                        r"\frac{1}{452}",
                        "clicking a typeset caret position edits the matching structure"
                    );
                    ui.invoke_keypad("key:clear".into(), false);
                    assert!(card.global::<crate::MathInputState>().get_open());
                    check_script_operators(&checks, &card, false);
                    // Clearing changes the card height. Click after the native
                    // resize has updated hit testing and the button's position.
                    let app = checks.app.clone();
                    Timer::single_shot(Duration::from_millis(60), move || {
                        click_structured_formula_button(&practice_window(&app));
                    });
                }
                6 => {
                    let card = practice_window(app);
                    assert!(card.global::<crate::MathInputState>().get_keypad_expanded());
                    assert_eq!(
                        practice_bottom(&card),
                        bottom.get(),
                        "expansion keeps the bottom edge anchored"
                    );
                    let ui = card.global::<crate::MathInputState>();
                    let size = card
                        .window()
                        .size()
                        .to_logical(card.window().scale_factor());
                    assert!((size.width - 320.0).abs() < 2.0);
                    assert!(size.height > 400.0 && size.height <= 561.0, "{size:?}");
                    assert!(
                        ui.get_apply_center_y() + 16.0 <= ui.get_pad_bottom() + 0.5,
                        "inline buttons must fit their panel"
                    );
                    assert!(ui.get_keypad_width() > 200.0 && ui.get_keypad_width() <= 272.0);
                    check_script_operators(&checks, &card, true);
                    click_structured_formula_button(&card);
                    assert!(ui.get_open(), "collapsing must retain the active editor");
                    assert!(!ui.get_keypad_expanded());
                    assert!(!ui.get_complete());
                    assert!(
                        ui.get_error().is_empty(),
                        "collapse must not validate an empty draft"
                    );
                    assert_eq!(card.get_feedback(), 0);
                }
                7 => {
                    let card = practice_window(app);
                    assert_eq!(
                        practice_bottom(&card),
                        bottom.get(),
                        "collapse returns the card to the bottom edge"
                    );
                    assert!(
                        card.window()
                            .size()
                            .to_logical(card.window().scale_factor())
                            .height
                            < 320.0,
                        "collapsing an empty formula must shrink the card"
                    );
                    snapshot(
                        card.window(),
                        checks.output.join("practice_collapsed_empty.png"),
                    );
                    click_structured_formula_button(&card);
                    assert!(card.global::<crate::MathInputState>().get_keypad_expanded());
                }
                8 => {
                    let card = practice_window(app);
                    let ui = card.global::<crate::MathInputState>();
                    for (column, row) in [(1.0, 2.0), (3.0, 0.0)] {
                        pointer_click(
                            card.window(),
                            slint::LogicalPosition::new(
                                ui.get_keypad_left() + (column + 0.5) * ui.get_keypad_width() / 5.0,
                                ui.get_keypad_top() + row * 36.0 + 16.0,
                            ),
                        );
                    }
                    key(card.window(), Key::Return.into());
                    assert!(!ui.get_error().is_empty());
                    assert_eq!(card.get_feedback(), 0);
                    assert!(!app.practice.borrow().as_ref().unwrap().answered);
                    snapshot(card.window(), checks.output.join("practice_incomplete.png"));
                    let draft = ui.get_formula().accessible_text;
                    let error = ui.get_error();
                    click_structured_formula_button(&card);
                    assert!(ui.get_open());
                    assert!(!ui.get_keypad_expanded());
                    assert_eq!(ui.get_formula().accessible_text, draft);
                    assert_eq!(ui.get_error(), error, "collapse must not apply the draft");
                }
                9 => {
                    let card = practice_window(app);
                    assert_eq!(
                        practice_bottom(&card),
                        bottom.get(),
                        "an incomplete formula also returns to the bottom edge"
                    );
                    let ui = card.global::<crate::MathInputState>();
                    assert!(!ui.get_keypad_expanded());
                    assert!(
                        card.window()
                            .size()
                            .to_logical(card.window().scale_factor())
                            .height
                            < 320.0
                    );
                    snapshot(
                        card.window(),
                        checks.output.join("practice_collapsed_incomplete.png"),
                    );
                    // The same controller and caret remain live without the pad.
                    key(card.window(), "3".into());
                    ui.invoke_key("z".into(), true, false, false);
                    assert!(!ui.get_complete());
                    ui.invoke_key("y".into(), true, false, false);
                    assert!(ui.get_complete());
                    assert_eq!(
                        crate::structured_math::plain(
                            &app.structured_math
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .1
                                .complete()
                                .unwrap()
                        ),
                        "2/3"
                    );
                    assert_eq!(card.get_feedback(), 0);
                    assert_eq!(card.get_answer_text(), "2/3");
                    click_structured_formula_button(&card);
                    assert!(ui.get_keypad_expanded());
                }
                10 => {
                    let card = practice_window(app);
                    let ui = card.global::<crate::MathInputState>();
                    snapshot(
                        card.window(),
                        checks.output.join("practice_inline_dark.png"),
                    );
                    pointer_click(
                        card.window(),
                        slint::LogicalPosition::new(
                            ui.get_keypad_left() + ui.get_keypad_width() * 5.0 / 6.0,
                            ui.get_keypad_top() - 22.0,
                        ),
                    );
                }
                11 => {
                    let card = practice_window(app);
                    let ui = card.global::<crate::MathInputState>();
                    ui.invoke_choose_category(2);
                    assert!(ui.get_catalog().row_count() > 0);
                    snapshot(
                        card.window(),
                        checks.output.join("practice_inline_functions.png"),
                    );
                    key(card.window(), Key::Escape.into());
                    assert!(
                        ui.get_open(),
                        "Esc collapses the keypad without replacing the editor"
                    );
                    assert!(!ui.get_keypad_expanded());
                    assert_eq!(card.get_answer_text(), "2/3");
                    click_structured_formula_button(&card);
                    ui.invoke_keypad("key:clear".into(), false);
                    card.global::<crate::MathInputState>()
                        .invoke_committed("2/3".into());
                    app.library.borrow_mut().settings.appearance = Appearance::Light;
                    app.sync_appearance();
                }
                12 => {
                    let card = practice_window(app);
                    assert!(!card.get_dark_theme());
                    snapshot(
                        card.window(),
                        checks.output.join("practice_inline_light.png"),
                    );
                    click_structured_formula_button(&card);
                    let ui = card.global::<crate::MathInputState>();
                    assert!(ui.get_open());
                    assert!(!ui.get_keypad_expanded());
                    assert!(ui.get_complete());
                    assert_eq!(card.get_feedback(), 0, "collapsing does not grade");
                    assert_eq!(card.get_answer_text(), "2/3");
                }
                13 => {
                    let card = practice_window(app);
                    assert_eq!(
                        practice_bottom(&card),
                        bottom.get(),
                        "a complete formula also returns to the bottom edge"
                    );
                    assert!(
                        card.window()
                            .size()
                            .to_logical(card.window().scale_factor())
                            .height
                            < 320.0
                    );
                    snapshot(
                        card.window(),
                        checks.output.join("practice_collapsed_complete.png"),
                    );
                    key(card.window(), Key::Return.into());
                    let ui = card.global::<crate::MathInputState>();
                    assert!(!ui.get_open());
                    assert!(!ui.get_keypad_expanded());
                    assert_eq!(card.get_answer_text(), "2/3");
                    assert_eq!(
                        card.get_feedback(),
                        1,
                        "Enter grades directly in the collapsed editor"
                    );
                }
                14 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_feedback(), 1);
                    snapshot(card.window(), checks.output.join("practice_correct.png"));
                    app.close_practice();
                    app.show_practice().unwrap();
                    practice_window(app).invoke_open_structured_entry();
                }
                15 => {
                    let card = practice_window(app);
                    assert!(app.structured_math.borrow().is_some());
                    app.close_practice();
                    assert!(app.structured_math.borrow().is_none());
                    assert!(
                        app.store.load().unwrap().settings.popup_position.is_none(),
                        "automatic resize placement must not be saved as a manual position"
                    );
                    assert!(!card.global::<crate::MathInputState>().get_open());
                    assert!(!card.global::<crate::MathInputState>().get_keypad_expanded());
                    app.library.borrow_mut().settings.appearance = Appearance::Dark;
                    app.sync_appearance();
                    app.show_practice().unwrap();
                    let card = practice_window(app);
                    card.set_prompt("公式工具".into());
                    card.set_prompt_formula_width(0.0);
                    card.invoke_open_structured_entry();
                }
                16 => {
                    let card = practice_window(app);
                    let ui = card.global::<crate::MathInputState>();
                    pointer_click(
                        card.window(),
                        slint::LogicalPosition::new(
                            ui.get_keypad_left() + ui.get_keypad_width() * 5.0 / 6.0,
                            ui.get_keypad_top() - 22.0,
                        ),
                    );
                }
                17..=46 => {
                    let card = practice_window(app);
                    let ui = card.global::<crate::MathInputState>();
                    let group = i32::from((phase - 17) / 2);
                    if (phase - 17).is_multiple_of(2) {
                        ui.invoke_choose_category(group);
                        scroll_math_catalog(&card, 800.0);
                        for key in ui.get_catalog().iter() {
                            if key.label.chars().any(
                                |c| matches!(c, '\u{2e80}'..='\u{9fff}' | '\u{f900}'..='\u{faff}'),
                            ) {
                                assert_eq!(
                                    key.image.size().width,
                                    0,
                                    "{} must use native text",
                                    key.label
                                );
                            } else {
                                assert!(
                                    key.image.size().width > 0,
                                    "{} has no math preview",
                                    key.label
                                );
                            }
                        }
                        snapshot(
                            card.window(),
                            checks.output.join(format!("catalog_{group:02}_top.png")),
                        );
                        scroll_math_catalog(&card, -800.0);
                    } else {
                        snapshot(
                            card.window(),
                            checks.output.join(format!("catalog_{group:02}_bottom.png")),
                        );
                    }
                }
                47 => {
                    let card = practice_window(app);
                    app.library.borrow_mut().settings.appearance = Appearance::Light;
                    app.sync_appearance();
                    card.global::<crate::MathInputState>()
                        .invoke_choose_category(0);
                    scroll_math_catalog(&card, 800.0);
                }
                48 => {
                    let card = practice_window(app);
                    snapshot(
                        card.window(),
                        checks.output.join("catalog_light_basic_top.png"),
                    );
                    scroll_math_catalog(&card, -800.0);
                }
                49 => {
                    let card = practice_window(app);
                    let ui = card.global::<crate::MathInputState>();
                    snapshot(
                        card.window(),
                        checks.output.join("catalog_light_basic_bottom.png"),
                    );
                    // Basic catalog index 15 is the native "文字" caption, now
                    // on the second visible row after scrolling to the end.
                    pointer_click(
                        card.window(),
                        slint::LogicalPosition::new(
                            ui.get_keypad_left() + ui.get_keypad_width() / 6.0,
                            ui.get_keypad_top() + 36.0 + 16.0,
                        ),
                    );
                    assert_eq!(
                        app.structured_math
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .1
                            .complete()
                            .unwrap(),
                        r"\text{text}"
                    );
                    ui.invoke_dismiss();
                    assert_eq!(card.get_answer_text(), "text");
                    assert!(ui.get_open());
                    assert!(!ui.get_keypad_expanded());
                    assert_eq!(card.get_feedback(), 0);
                    assert_eq!(
                        app.structured_math
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .1
                            .complete()
                            .unwrap(),
                        r"\text{text}"
                    );
                    checks.finished.set(true);
                    println!(
                        "Structured math: inline editing, empty/incomplete/complete formula toggling, collapsed typing and undo/redo, persistence, height, corner anchoring without saving temporary moves, lifecycle and all 319 catalog captions in 15 groups passed; Chinese/native labels, structural scripts and both themes verified."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            structured_math_phase(checks.clone(), bottom, phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens isolated authoring and practice windows with inline math"]
fn native_structured_math() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/structured_math_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.feedback_seconds = 60;
    library.settings.popup_corner = PopupCorner::BottomRight;
    library.settings.remember_position = true;
    library.settings.popup_position = None;
    library.set_pack_enabled("test_pack", false).unwrap();
    let id = library.packs[0].cards[0].id.clone().unwrap();
    library.set_card_enabled("test_pack", &id, true).unwrap();
    library
        .set_card_answer_mode("test_pack", &id, AnswerMode::Input)
        .unwrap();
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
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    structured_math_phase(checks.clone(), Rc::new(Cell::new(0)), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(20), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Structured math integration failed or timed out"
    );
}

fn click_structured_formula_button(card: &PracticeWindow) {
    pointer_click(
        card.window(),
        slint::LogicalPosition::new(
            card.get_formula_button_center_x(),
            card.get_formula_button_center_y(),
        ),
    );
}

fn formula_editing_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(400), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    main.invoke_add_card();
                    main.invoke_activate_content(0);
                    main.invoke_math_tool("math".into());
                    type_content(app, &main, 0, "12+34");
                }
                1 => {
                    let main = main_window(app);
                    key(main.window(), Key::Home.into());
                    key(main.window(), Key::RightArrow.into());
                    key(main.window(), Key::RightArrow.into());
                    key(main.window(), "0".into());
                    assert_eq!(main.get_math_input(), "120+34");
                    modified_key(main.window(), Key::Control, "z".into());
                    assert_eq!(main.get_math_input(), "12+34");
                }
                2 => {
                    let main = main_window(app);
                    assert_eq!(
                        app.math_fields.borrow()[0].text_selection(),
                        TextSelection {
                            anchor: 2,
                            focus: 2
                        }
                    );
                    key(main.window(), Key::End.into());
                    main.window().dispatch_event(WindowEvent::KeyPressed {
                        text: Key::Shift.into(),
                    });
                    key(main.window(), Key::LeftArrow.into());
                    key(main.window(), Key::LeftArrow.into());
                    main.window().dispatch_event(WindowEvent::KeyReleased {
                        text: Key::Shift.into(),
                    });
                    let range = app.math_fields.borrow()[0].text_selection();
                    assert_eq!(
                        (range.anchor.min(range.focus), range.anchor.max(range.focus)),
                        (3, 5)
                    );
                    main.invoke_math_tool("root".into());
                    assert_eq!(main.get_math_preview().label, "12+sqrt(34)");
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                    main.set_editor_scroll_y(0.0);
                }
                3 => {
                    let main = main_window(app);
                    snapshot(
                        main.window(),
                        checks.output.join("range_template_compact.png"),
                    );
                    main.invoke_cancel_editor();
                    app.show_practice().unwrap();
                }
                4 => {
                    let card = practice_window(app);
                    key(card.window(), "12+34".into());
                    assert!(card.global::<crate::MathInputState>().get_open());
                    assert!(!card.global::<crate::MathInputState>().get_keypad_expanded());
                }
                5 => {
                    let card = practice_window(app);
                    key(card.window(), Key::Home.into());
                    key(card.window(), Key::RightArrow.into());
                    key(card.window(), Key::RightArrow.into());
                    key(card.window(), "0".into());
                    assert_eq!(card.get_answer_text(), "120+34");
                    modified_key(card.window(), Key::Control, "z".into());
                    assert_eq!(card.get_answer_text(), "12+34");
                }
                6 => {
                    let card = practice_window(app);
                    key(card.window(), Key::End.into());
                    card.window().dispatch_event(WindowEvent::KeyPressed {
                        text: Key::Shift.into(),
                    });
                    key(card.window(), Key::LeftArrow.into());
                    key(card.window(), Key::LeftArrow.into());
                    card.window().dispatch_event(WindowEvent::KeyReleased {
                        text: Key::Shift.into(),
                    });
                    key(card.window(), "5".into());
                    assert_eq!(card.get_answer_text(), "12+5");
                    modified_key(card.window(), Key::Control, "z".into());
                    assert_eq!(card.get_answer_text(), "12+34");
                }
                7 => {
                    let card = practice_window(app);
                    modified_key(card.window(), Key::Control, "a".into());
                    key(card.window(), "1/".into());
                    key(card.window(), Key::Return.into());
                    assert_eq!(card.get_feedback(), 0);
                    assert!(!card.global::<crate::MathInputState>().get_complete());
                    assert!(
                        !card
                            .global::<crate::MathInputState>()
                            .get_error()
                            .is_empty()
                    );
                    // Wait for the resized card before exercising the visible button.
                    let app = app.clone();
                    Timer::single_shot(Duration::from_millis(60), move || {
                        click_structured_formula_button(&practice_window(&app));
                    });
                }
                8 => {
                    let card = practice_window(app);
                    let ui = card.global::<crate::MathInputState>();
                    assert!(ui.get_keypad_expanded());
                    ui.invoke_keypad("key:clear".into(), false);
                    ui.invoke_committed(r"\sqrt{3}".into());
                    key(card.window(), "/".into());
                    key(card.window(), "2".into());
                    assert_eq!(card.get_answer_text(), "sqrt(3)/2");
                    click_structured_formula_button(&card);
                    assert!(ui.get_open());
                    assert!(!ui.get_keypad_expanded());
                }
                9 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_answer_text(), "sqrt(3)/2");
                    snapshot(
                        card.window(),
                        checks.output.join("caret_and_navigation.png"),
                    );
                    key(card.window(), Key::Escape.into());
                    assert!(app.practice.borrow().is_none());
                    assert_eq!(app.store.load().unwrap().packs[0].cards.len(), 20);
                    checks.finished.set(true);
                    println!(
                        "Native question text caret/ranges and practice structural caret, selection, undo, incomplete drafts, actual keypad toggle and root/fraction grouping passed."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            formula_editing_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens real windows to verify formula caret, range and navigation"]
fn native_formula_editing() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/formula_editing_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
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
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    formula_editing_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(20), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Formula editing check failed or timed out"
    );
}

fn formula_answers_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(400), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    main.invoke_add_card();
                    main.invoke_set_editor_answer_mode(1);
                    type_content(app, &main, 0, "1+1=?");
                    type_content(app, &main, 1, "3");
                    type_content(app, &main, 2, "2");
                    type_content(app, &main, 3, "4");
                    main.set_editor_correct_choice(1);
                    main.set_math_active_field(-1);
                }
                1 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("choice_editor.png"));
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[0].cards[20].answer, "2");
                    assert_eq!(
                        saved.card_answer_mode(
                            "test_pack",
                            saved.packs[0].cards[20].id.as_deref().unwrap()
                        ),
                        AnswerMode::MultipleChoice
                    );
                    main.invoke_add_card();
                    main.invoke_set_editor_answer_mode(2);
                    type_content(app, &main, 4, "1+1=3");
                    main.invoke_select_truth(false);
                    main.set_math_active_field(-1);
                }
                2 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("judgment_editor.png"));
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    let saved = app.store.load().unwrap();
                    let card = &saved.packs[0].cards[21];
                    assert_eq!(card.prompt, "1+1=3");
                    assert_eq!(card.answer, "✕");
                    assert!(!card.true_false.as_ref().unwrap().is_true);
                    assert_eq!(main.get_cards().row_data(21).unwrap().prompt, "1+1=3");
                    let exported = checks.output.join("simplified_pack.json");
                    Store::export_pack(&exported, &saved.packs[0]).unwrap();
                    assert_eq!(Store::read_pack(&exported).unwrap(), saved.packs[0]);
                    app.show_practice().unwrap();
                }
                3 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_prompt(), "√3/2 = ?");
                    key(card.window(), "sqrt(3)/2".into());
                }
                4 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_answer_text(), "sqrt(3)/2");
                    assert!(card.global::<crate::MathInputState>().get_open());
                    assert!(
                        card.global::<crate::MathInputState>()
                            .get_formula()
                            .content_width
                            > 0.0
                    );
                    snapshot(card.window(), checks.output.join("typed_formula.png"));
                    key(card.window(), Key::Return.into());
                    assert_eq!(card.get_feedback(), 1);
                    app.close_practice();
                    app.show_practice().unwrap();
                }
                5 => {
                    let card = practice_window(app);
                    click_structured_formula_button(&card);
                    assert!(card.global::<crate::MathInputState>().get_keypad_expanded());
                }
                6 => {
                    let card = practice_window(app);
                    check_floating_window(card.window());
                    check_no_caption_paint(card.window());
                    key(card.window(), "1/".into());
                    key(card.window(), Key::Return.into());
                    assert_eq!(card.get_feedback(), 0);
                    assert!(!card.global::<crate::MathInputState>().get_complete());
                }
                7 => {
                    let card = practice_window(app);
                    key(card.window(), "2".into());
                    let ui = card.global::<crate::MathInputState>();
                    assert!(ui.get_complete());
                    assert_eq!(card.get_answer_text(), "1/2");
                    ui.invoke_keypad("key:clear".into(), false);
                    assert!(!ui.get_complete());
                    key(card.window(), Key::Return.into());
                    assert_eq!(card.get_feedback(), 0, "clearing never grades");
                    modified_key(card.window(), Key::Control, "z".into());
                    assert_eq!(card.get_answer_text(), "1/2");
                }
                8 => {
                    let card = practice_window(app);
                    modified_key(card.window(), Key::Control, "a".into());
                    key(card.window(), "sqrt(3)/2".into());
                    snapshot(card.window(), checks.output.join("structured_formula.png"));
                    key(card.window(), Key::Return.into());
                    assert_eq!(card.get_feedback(), 1);
                    assert!(!card.global::<crate::MathInputState>().get_open());
                    app.close_practice();
                    app.show_practice().unwrap();
                }
                9 => {
                    let card = practice_window(app);
                    key(card.window(), "sqrt(3)/2".into());
                    click_structured_formula_button(&card);
                    key(card.window(), Key::Escape.into());
                    let ui = card.global::<crate::MathInputState>();
                    assert!(ui.get_open());
                    assert!(!ui.get_keypad_expanded());
                    assert_eq!(card.get_answer_text(), "sqrt(3)/2");
                    assert_eq!(card.get_feedback(), 0);
                    key(card.window(), Key::Escape.into());
                    assert!(app.practice.borrow().is_none());
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(21, true);
                    app.show_practice().unwrap();
                }
                10 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_answer_mode(), 2);
                    assert_eq!(card.get_standard_answer(), "✕");
                    assert_eq!(card.get_options().row_count(), 2);
                    click_option(&card, 0);
                    assert_eq!(card.get_feedback(), 2);
                    snapshot(card.window(), checks.output.join("judgment_symbol.png"));
                    checks.finished.set(true);
                    println!(
                        "Native simplified editors/export, structured answers, actual keypad toggle, clear/undo, incomplete protection, direct Enter, collapse/dismiss and symbol-only judgment passed."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            formula_answers_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens real editor and practice windows and uses the formula keypad"]
fn native_formula_answers() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/formula_answers_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.feedback_seconds = 2;
    library.set_pack_enabled("test_pack", false).unwrap();
    let card_id = library.packs[0].cards[2].id.clone().unwrap();
    library
        .set_card_enabled("test_pack", &card_id, true)
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
    formula_answers_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(25), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Formula answers check failed or timed out"
    );
}

#[test]
#[ignore = "opens a real practice card and edits formulas inside its answer box"]
fn native_inline_answers() {
    check_inline_answers(0);
}

#[test]
#[ignore = "opens a real practice card and types an exponent without a formula hint"]
fn native_inline_script_typing() {
    check_inline_answers(20);
}

fn check_inline_answers(start_phase: u8) {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/inline_answers_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.feedback_seconds = 2;
    if start_phase == 20 {
        library.packs[0].cards[2].answer = "Paris".into();
        library.packs[0].cards[2].answer_latex = None;
    }
    library.set_pack_enabled("test_pack", false).unwrap();
    let card_id = library.packs[0].cards[2].id.clone().unwrap();
    library
        .set_card_enabled("test_pack", &card_id, true)
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
    checks.app.show_practice().unwrap();
    inline_answers_phase(checks.clone(), start_phase);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(25), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Inline answers check failed or timed out"
    );
}

fn inline_answers_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let card = practice_window(app);
            let ui = card.global::<crate::MathInputState>();
            match phase {
                0 => key(card.window(), "Paris".into()),
                1 => {
                    assert_eq!(card.get_answer_text(), "Paris");
                    assert!(!ui.get_open(), "ordinary words use native text input");
                    modified_key(card.window(), Key::Control, "a".into());
                    key(card.window(), "犹豫".into());
                }
                2 => {
                    assert_eq!(card.get_answer_text(), "犹豫");
                    assert!(!ui.get_open());
                    snapshot(card.window(), checks.output.join("plain_cjk.png"));
                    modified_key(card.window(), Key::Control, "z".into());
                    assert_eq!(card.get_answer_text(), "Paris");
                    modified_key(card.window(), Key::Control, "y".into());
                    assert_eq!(card.get_answer_text(), "犹豫");
                    modified_key(card.window(), Key::Control, "a".into());
                    key(card.window(), "sqrt(3)/2".into());
                }
                3 => {
                    assert!(ui.get_open());
                    assert!(!ui.get_keypad_expanded());
                    assert!(ui.get_complete());
                    assert_eq!(
                        app.structured_math.borrow().as_ref().unwrap().1.source(),
                        "sqrt(3)/2"
                    );
                    snapshot(card.window(), checks.output.join("inline_fraction.png"));
                    key(card.window(), Key::LeftArrow.into());
                    key(card.window(), Key::Home.into());
                    key(card.window(), "3".into());
                    assert_eq!(card.get_answer_text(), "sqrt(3)/32");
                    modified_key(card.window(), Key::Control, "z".into());
                    assert_eq!(card.get_answer_text(), "sqrt(3)/2");
                    click_structured_formula_button(&card);
                }
                4 => {
                    assert!(ui.get_keypad_expanded());
                    let source = app.structured_math.borrow().as_ref().unwrap().1.source();
                    snapshot(card.window(), checks.output.join("inline_expanded.png"));
                    click_structured_formula_button(&card);
                    assert!(ui.get_open());
                    assert!(!ui.get_keypad_expanded());
                    assert_eq!(
                        app.structured_math.borrow().as_ref().unwrap().1.source(),
                        source
                    );
                    modified_key(card.window(), Key::Control, "a".into());
                    key(card.window(), "1/".into());
                }
                5 => {
                    assert!(ui.get_open());
                    assert!(!ui.get_complete());
                    key(card.window(), Key::Return.into());
                    assert_eq!(
                        card.get_feedback(),
                        0,
                        "unfinished structures are not graded"
                    );
                    assert!(!ui.get_error().is_empty());
                    key(card.window(), "2".into());
                    assert!(ui.get_complete());
                    modified_key(card.window(), Key::Control, "a".into());
                    key(card.window(), "sqrt(3)/2".into());
                }
                6 => {
                    assert_eq!(card.get_answer_text(), "sqrt(3)/2");
                    key(card.window(), Key::Return.into());
                    assert_eq!(
                        card.get_feedback(),
                        1,
                        "one Enter checks the complete formula"
                    );
                    assert!(!ui.get_open());
                    assert!(card.get_input_preview().formula_width > 0.0);
                    snapshot(card.window(), checks.output.join("inline_correct.png"));
                    app.close_practice();
                    {
                        let mut library = app.library.borrow_mut();
                        let question = &mut library.packs[0].cards[2];
                        question.answer = "C++".into();
                        question.answer_latex = None;
                        question.aliases.clear();
                    }
                    app.show_practice().unwrap();
                }
                7 => key(card.window(), "C++".into()),
                8 => {
                    assert_eq!(card.get_answer_text(), "C++");
                    assert!(!ui.get_open(), "C++ must remain a source-code answer");
                    key(card.window(), Key::Return.into());
                    assert_eq!(card.get_feedback(), 1);
                    snapshot(card.window(), checks.output.join("plain_cpp.png"));
                    app.close_practice();
                    {
                        let mut library = app.library.borrow_mut();
                        library.packs[0].cards[2].answer = "&mut T".into();
                        library.settings.appearance = Appearance::Light;
                    }
                    app.show_practice().unwrap();
                }
                9 => key(card.window(), "&mut T".into()),
                10 => {
                    assert_eq!(card.get_answer_text(), "&mut T");
                    assert!(!ui.get_open());
                    snapshot(card.window(), checks.output.join("plain_rust_light.png"));
                    key(card.window(), Key::Return.into());
                    assert_eq!(card.get_feedback(), 1);
                    app.close_practice();
                    {
                        let mut library = app.library.borrow_mut();
                        let question = &mut library.packs[0].cards[2];
                        question.answer = "p^n*q^n".into();
                        question.answer_latex = Some(r"p^{n}\cdot q^{n}".into());
                    }
                    app.show_practice().unwrap();
                }
                11 => {
                    key(card.window(), r"p^{n}\text{⋅}q^{n}".into());
                    assert!(ui.get_open());
                    assert!(ui.get_complete());
                    key(card.window(), Key::Return.into());
                    assert_eq!(
                        card.get_feedback(),
                        1,
                        "complete dot product submits with one Enter"
                    );
                    assert!(card.get_input_error().is_empty());
                    assert!(!ui.get_open());
                    snapshot(
                        card.window(),
                        checks.output.join("multiplication_correct.png"),
                    );
                    app.close_practice();
                    app.show_practice().unwrap();
                }
                12 => {
                    key(card.window(), r"p^{m}\text{⋅}q^{n}".into());
                    key(card.window(), Key::Return.into());
                    assert_eq!(
                        card.get_feedback(),
                        2,
                        "a wrong exponent reaches answer feedback"
                    );
                    assert!(card.get_input_error().is_empty());
                    snapshot(
                        card.window(),
                        checks.output.join("multiplication_wrong.png"),
                    );
                    app.close_practice();
                    app.show_practice().unwrap();
                }
                13..=17 | 20..=24 => {
                    let start = if phase < 20 { 13 } else { 20 };
                    assert!(!ui.get_keypad_expanded());
                    key(
                        card.window(),
                        ["p", "^", "n", "+", "m"][(phase - start) as usize].into(),
                    );
                }
                18 | 25 => {
                    assert!(ui.get_open());
                    assert!(!ui.get_keypad_expanded());
                    assert_eq!(
                        app.structured_math
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .1
                            .complete()
                            .unwrap(),
                        "p^{n+m}",
                        "typing from the empty native answer must retain exponent scope; phase={phase}"
                    );
                    if phase == 18 {
                        app.close_practice();
                        {
                            let mut library = app.library.borrow_mut();
                            library.packs[0].cards[2].answer = "Paris".into();
                            library.packs[0].cards[2].answer_latex = None;
                        }
                        app.show_practice().unwrap();
                    } else {
                        checks.finished.set(true);
                        app.quit();
                        return;
                    }
                }
                19 => {
                    assert!(card.get_answer_text().is_empty());
                    assert!(!ui.get_open());
                }
                _ => unreachable!(),
            }
            inline_answers_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

fn math_toolbar(main: &MainWindow) -> slint::LogicalPosition {
    let preview = main.get_math_preview();
    let height = (preview.formula_height + 20.0).max(54.0);
    // Each visible tool row is 32 px with a 10 px gap. The picture
    // reserves 10 px horizontally and vertically centres the actual formula.
    slint::LogicalPosition::new(
        main.get_math_canvas_left() - 10.0,
        main.get_math_canvas_top()
            - (height - preview.formula_height * main.get_math_canvas_scale()) / 2.0
            - 42.0
                * (1 + i32::from(main.get_math_has_formula())
                    + i32::from(!main.get_math_text_mode())) as f32,
    )
}

fn click_math_slot(app: &App, main: &MainWindow, path: &[usize]) {
    let index = app
        .math_paths
        .borrow()
        .iter()
        .position(|p| p == path)
        .unwrap();
    let region = main.get_math_regions().row_data(index).unwrap();
    let scale = main.get_math_canvas_scale();
    pointer_click(
        main.window(),
        slint::LogicalPosition::new(
            main.get_math_canvas_left() + (region.x + region.width / 2.0) * scale,
            main.get_math_canvas_top() + (region.y + region.height / 2.0) * scale,
        ),
    );
}

fn click_math_structure_tool(main: &MainWindow, index: usize) {
    let toolbar = math_toolbar(main);
    let canvas_height = (main.get_math_preview().formula_height + 20.0).max(54.0);
    pointer_click(
        main.window(),
        slint::LogicalPosition::new(
            toolbar.x + 30.0 + index as f32 * 64.0,
            toolbar.y + 126.0 + canvas_height + 26.0,
        ),
    );
}

fn authoring_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let main = main_window(app);
            match phase {
                0 => {
                    use windows_sys::Win32::UI::WindowsAndMessaging::{
                        GetWindowTextW, ICON_BIG, ICON_SMALL, WM_GETICON,
                    };
                    let hwnd = native_handle(main.window());
                    let mut title = [0u16; 64];
                    // SAFETY: the handles come from the live Winit window and
                    // the writable title buffer covers its declared length.
                    unsafe {
                        let len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
                        assert_eq!(String::from_utf16_lossy(&title[..len as usize]), "Try");
                        assert_ne!(
                            SendMessageW(hwnd, WM_GETICON, ICON_SMALL as usize, 0),
                            0,
                            "Small titlebar icon is assigned"
                        );
                        assert_ne!(
                            SendMessageW(hwnd, WM_GETICON, ICON_BIG as usize, 0),
                            0,
                            "Large window icon is assigned"
                        );
                    }
                    snapshot(main.window(), checks.output.join("knowledge.png"));
                    assert!(
                        (0..main.get_cards().row_count())
                            .filter(|i| main.get_cards().row_data(*i).unwrap().rendered)
                            .count()
                            < 12,
                        "Only visible list formulas should be rasterized"
                    );
                    let selected = main.get_selected_index();
                    main.set_current_tab(1);
                    snapshot(main.window(), checks.output.join("settings.png"));
                    assert_eq!(main.get_selected_index(), selected);
                    main.invoke_select_pack(0);
                    assert_eq!(main.get_current_tab(), 0);
                    main.invoke_edit_card(0);
                    main.invoke_activate_content(0);
                }
                1 => {
                    snapshot(main.window(), checks.output.join("formula_editor.png"));
                    let paths = app.math_paths.borrow();
                    let fields = app.math_fields.borrow();
                    let index = paths
                        .iter()
                        .position(|path| {
                            fields[0]
                                .root
                                .at(path)
                                .is_some_and(|node| node.value == "6")
                        })
                        .unwrap();
                    let region = main.get_math_regions().row_data(index).unwrap();
                    let scale = main.get_math_canvas_scale();
                    let position = slint::LogicalPosition::new(
                        main.get_math_canvas_left() + (region.x + region.width / 2.0) * scale,
                        main.get_math_canvas_top() + (region.y + region.height / 2.0) * scale,
                    );
                    drop(fields);
                    drop(paths);
                    main.window()
                        .dispatch_event(WindowEvent::PointerMoved { position });
                    main.window().dispatch_event(WindowEvent::PointerPressed {
                        position,
                        button: PointerEventButton::Left,
                    });
                    main.window().dispatch_event(WindowEvent::PointerReleased {
                        position,
                        button: PointerEventButton::Left,
                    });
                    assert_eq!(
                        main.get_math_input(),
                        "6",
                        "Clicking the rendered denominator selects it"
                    );
                    main.set_math_input("3".into());
                    main.invoke_edit_math_input("3".into());
                }
                2 => {
                    assert!(main.get_math_error().is_empty());
                    let fields = app.math_fields.borrow();
                    assert_eq!(fields[0].selected().value, "3");
                    drop(fields);
                    main.invoke_undo_math(false);
                    assert_eq!(main.get_math_input(), "6");
                    main.invoke_undo_math(true);
                    assert_eq!(main.get_math_input(), "3");
                    let before = app.store.load().unwrap();
                    main.invoke_math_tool("clear".into());
                    assert_eq!(main.get_math_input(), "");
                    main.invoke_save_editor();
                    assert!(
                        main.get_editor_visible(),
                        "A cleared question stays in the draft"
                    );
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_undo_math(false);
                    assert_eq!(
                        main.get_math_input(),
                        "3",
                        "Undo also restores the selected denominator"
                    );
                    main.set_math_input("sqrt(".into());
                    main.invoke_edit_math_input("sqrt(".into());
                    main.invoke_math_tool("clear".into());
                    assert_eq!(main.get_math_input(), "");
                    assert!(
                        main.get_math_error().is_empty(),
                        "C also clears malformed input"
                    );
                    main.invoke_undo_math(false);
                    assert_eq!(main.get_math_input(), "sqrt(");
                    assert!(!main.get_math_error().is_empty());
                    main.invoke_save_editor();
                    assert!(main.get_editor_visible());
                    main.invoke_undo_math(false);
                    assert_eq!(main.get_math_input(), "3");
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[0].cards[0].id, before.packs[0].cards[0].id);
                    assert_eq!(
                        saved.packs[0].cards[0].answer,
                        before.packs[0].cards[0].answer
                    );
                    assert_eq!(saved.settings, before.settings);
                    main.invoke_add_card();
                    type_content(app, &main, 0, "计算 sin(pi/3) 的值");
                    main.invoke_activate_content(1);
                    main.invoke_math_tool("math".into());
                    main.invoke_math_tool("root".into());
                    main.invoke_math_tool("fraction".into());
                    app.modify_math(|field| {
                        field.select(vec![0, 1]);
                        Ok(())
                    });
                    main.set_math_input("2".into());
                    main.invoke_edit_math_input("2".into());
                    assert!(app.flush_math());
                    app.modify_math(|field| {
                        field.select(vec![0, 0]);
                        Ok(())
                    });
                    main.set_math_input("3".into());
                    main.invoke_edit_math_input("3".into());
                    assert!(app.flush_math());
                    app.refresh_math(false);
                    // Reveal the structure row below the answer preview in the desktop layout.
                    main.set_editor_scroll_y(-140.0);
                }
                3 => {
                    assert_eq!(main.get_math_preview().label, "sqrt(3/2)");
                    assert!(main.get_math_can_move_out());
                    snapshot(
                        main.window(),
                        checks.output.join("nested_formula_editor.png"),
                    );
                    click_math_structure_tool(&main, 0);
                    assert_eq!(main.get_math_preview().label, "sqrt(3)/2");
                    assert_eq!(main.get_math_input(), "3");
                    main.invoke_undo_math(false);
                    assert_eq!(main.get_math_preview().label, "sqrt(3/2)");
                    main.invoke_undo_math(true);
                    click_math_structure_tool(&main, 1);
                    assert_eq!(main.get_math_preview().label, "3/2");
                    main.invoke_undo_math(false);
                    assert_eq!(main.get_math_preview().label, "sqrt(3)/2");
                    snapshot(
                        main.window(),
                        checks.output.join("repaired_formula_editor.png"),
                    );
                    main.invoke_activate_content(0);
                    snapshot(main.window(), checks.output.join("mixed_editor.png"));
                    assert!(
                        main.get_math_preview().texts.row_count() >= 2,
                        "Mixed Chinese text uses native glyphs"
                    );
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    let saved = app.store.load().unwrap();
                    assert!(saved.packs[0].cards.last().unwrap().prompt.contains("计算"));
                    assert_eq!(saved.packs[0].cards.last().unwrap().answer, "sqrt(3)/2");
                    let path = checks.output.join("authored_pack.json");
                    Store::export_pack(&path, &saved.packs[0]).unwrap();
                    assert_eq!(Store::read_pack(&path).unwrap(), saved.packs[0]);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(20, true);
                    app.show_practice().unwrap();
                    let practice = practice_window(app);
                    assert!(practice.get_prompt_texts().row_count() >= 2);
                    snapshot(practice.window(), checks.output.join("mixed_practice.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_add_card();
                    type_content(app, &main, 0, "[[1,2],[3,4]]");
                    main.invoke_resize_math_grid(3, 3);
                    main.invoke_save_editor();
                    assert!(main.get_editor_visible());
                    assert!(!main.get_editor_error().is_empty());
                    for cell in [2usize, 5, 6, 7, 8] {
                        let region = app
                            .math_paths
                            .borrow()
                            .iter()
                            .position(|p| p == &vec![cell])
                            .unwrap();
                        main.invoke_select_math_region(region as i32);
                        let value = (cell + 1).to_string();
                        main.set_math_input(value.clone().into());
                        main.invoke_edit_math_input(value.into());
                        assert!(app.flush_math());
                        app.refresh_math(false);
                    }
                    type_content(app, &main, 1, "1");
                    main.invoke_activate_content(0);
                    main.set_editor_scroll_y(0.0);
                }
                4 => {
                    snapshot(main.window(), checks.output.join("matrix_editor.png"));
                    assert_eq!(main.get_math_grid_rows(), 3);
                    assert_eq!(main.get_math_grid_columns(), 3);
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    main.invoke_add_card();
                    type_content(app, &main, 0, "cases(x,x>=0,-x,x<0)");
                    type_content(app, &main, 1, "x");
                    main.invoke_activate_content(0);
                    main.set_editor_scroll_y(0.0);
                }
                5 => {
                    snapshot(main.window(), checks.output.join("cases_editor.png"));
                    let before = app.store.load().unwrap();
                    main.invoke_cancel_editor();
                    assert_eq!(
                        app.store.load().unwrap(),
                        before,
                        "Cancel leaves persisted content untouched"
                    );
                    main.invoke_add_card();
                    type_content(
                        app,
                        &main,
                        0,
                        "int(0,1,x^2)+sum(k=1,n,k)+lim(x->0,sin(x)/x)",
                    );
                    type_content(app, &main, 1, "1/3");
                    main.invoke_activate_content(0);
                    main.set_editor_scroll_y(0.0);
                }
                6 => {
                    snapshot(main.window(), checks.output.join("advanced_editor.png"));
                    assert!(main.get_math_preview().formula_width > 0.0);
                    assert!(main.get_math_error().is_empty());
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    main.invoke_add_card();
                    type_content(app, &main, 0, "求 ");
                    type_content(app, &main, 1, "3");
                    main.invoke_activate_content(0);
                    main.set_editor_scroll_y(0.0);
                }
                7 => {
                    assert!(!main.get_math_has_formula());
                    assert_eq!(main.get_math_preview().formula_width, 0.0);
                    snapshot(main.window(), checks.output.join("plain_editor.png"));
                    main.invoke_math_tool("math".into());
                }
                8 => {
                    assert!(main.get_math_has_formula());
                    assert!(!main.get_math_text_mode());
                    let top = math_toolbar(&main);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(top.x + 108.0, top.y + 16.0),
                    );
                }
                9 => {
                    snapshot(main.window(), checks.output.join("function_palette.png"));
                    let top = math_toolbar(&main);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(top.x + 113.0, top.y + 60.0),
                    );
                    assert_eq!(app.math_fields.borrow()[0].selection, vec![1, 0, 1]);
                    key(main.window(), "2".into());
                }
                10 => {
                    assert!(app.flush_math());
                    assert_eq!(
                        app.math_fields.borrow()[0].selected().value,
                        "2",
                        "Template selection returns keyboard focus to its slot"
                    );
                    click_math_slot(app, &main, &[1, 1, 0]);
                    key(main.window(), "8".into());
                }
                11 => {
                    assert!(app.flush_math());
                    assert_eq!(app.math_fields.borrow()[0].selected().value, "8");
                    snapshot(main.window(), checks.output.join("log_generator.png"));
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible(), "{}", main.get_editor_error());
                    let library = app.store.load().unwrap();
                    let card = library.packs[0].cards.last().unwrap();
                    assert!(card.prompt_latex.as_ref().unwrap().contains(r"\log"));
                    assert_eq!(card.answer, "3");
                    let path = checks.output.join("generated_functions_pack.json");
                    Store::export_pack(&path, &library.packs[0]).unwrap();
                    assert_eq!(Store::read_pack(&path).unwrap(), library.packs[0]);
                    main.invoke_add_card();
                    type_content(app, &main, 0, "ε = ?");
                    main.invoke_activate_content(1);
                    main.invoke_math_tool("math".into());
                    main.set_editor_scroll_y(0.0);
                }
                12 => {
                    let top = math_toolbar(&main);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(top.x + 184.0, top.y + 16.0),
                    );
                }
                13 => {
                    snapshot(main.window(), checks.output.join("symbol_palette.png"));
                    key(main.window(), Key::Escape.into());
                    assert_eq!(
                        app.math_fields.borrow()[1].selected(),
                        &r#try::math_editor::Node::blank()
                    );
                    let top = math_toolbar(&main);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(top.x + 184.0, top.y + 16.0),
                    );
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(top.x + 176.0, top.y + 96.0),
                    );
                    assert_eq!(main.get_math_input(), "ε");
                    key(main.window(), "+1".into());
                }
                14 => {
                    assert!(app.flush_math());
                    assert_eq!(
                        main.get_math_input(),
                        "ε+1",
                        "Typing after a symbol continues at its end"
                    );
                    snapshot(main.window(), checks.output.join("symbol_editor.png"));
                    main.invoke_cancel_editor();
                    let last = app.library.borrow().packs[0].cards.len() - 1;
                    main.invoke_edit_card(last as i32);
                    main.invoke_activate_content(0);
                    let path = app
                        .math_paths
                        .borrow()
                        .iter()
                        .find(|path| {
                            app.math_fields.borrow()[0]
                                .root
                                .at(path)
                                .is_some_and(|node| node.value == "8")
                        })
                        .unwrap()
                        .clone();
                    click_math_slot(app, &main, &path);
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                    main.set_editor_scroll_y(0.0);
                }
                15 => {
                    snapshot(
                        main.window(),
                        checks.output.join("generator_compact_before.png"),
                    );
                    let path = app
                        .math_paths
                        .borrow()
                        .iter()
                        .find(|path| {
                            app.math_fields.borrow()[0]
                                .root
                                .at(path)
                                .is_some_and(|node| node.value == "8")
                        })
                        .unwrap()
                        .clone();
                    click_math_slot(app, &main, &path);
                    assert!(
                        !main.get_math_text_mode(),
                        "Clicking slot {path:?} selected {:?}; canvas = ({}, {}), scale = {}",
                        app.math_fields.borrow()[0].selection,
                        main.get_math_canvas_left(),
                        main.get_math_canvas_top(),
                        main.get_math_canvas_scale()
                    );
                    assert!(main.get_math_has_formula());
                    assert!(main.get_math_error().is_empty());
                    snapshot(main.window(), checks.output.join("generator_compact.png"));
                    main.invoke_cancel_editor();
                    app.close_main();
                    assert!(app.math_fields.borrow().is_empty());
                    assert!(app.math_paths.borrow().is_empty());
                    checks.finished.set(true);
                    println!(
                        "Native math authoring, root-first fraction repair/unwrap clicks, plain text, function/symbol palette clicks, independent log slots, keyboard focus, export/reload, compact layout and cleanup checks passed."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            authoring_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens real windows and checks visual math authoring"]
fn native_math_authoring() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/math_authoring_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
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
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    authoring_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Visual math authoring check failed or timed out"
    );
}
