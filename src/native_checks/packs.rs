//! Explicit native checks for packs.
use super::*;

fn matrix_row_height(main: &MainWindow, row: usize) -> f32 {
    let card = main.get_cards().row_data(row).unwrap();
    let answer = if card.answer_mode == 2 {
        36.0
    } else {
        36.0_f32.max(card.answer_preview.formula_height + 6.0)
    };
    52.0_f32
        .max(card.preview.formula_height + 16.0)
        .max(answer + 16.0)
}

fn click_matrix(main: &MainWindow, row: Option<usize>, mode: usize) {
    let position = slint::LogicalPosition::new(
        main.get_matrix_area_left() + main.get_matrix_area_width()
            - (3 - mode) as f32 * main.get_matrix_mode_width()
            + main.get_matrix_mode_width() / 2.0,
        row.map_or(main.get_matrix_area_top() + 32.0 + 23.0, |row| {
            main.get_question_area_top()
                + (0..row).map(|i| matrix_row_height(main, i)).sum::<f32>()
                + matrix_row_height(main, row) / 2.0
        }),
    );
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
}

fn management_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(300), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    assert_eq!(main.get_cards().row_count(), 20);
                    main.invoke_set_card_answer_mode(0, 1);
                    main.invoke_set_card_answer_mode(1, 2);
                    assert_eq!(main.get_cards().row_data(0).unwrap().answer_mode, 1);
                    assert_eq!(main.get_cards().row_data(1).unwrap().answer_mode, 2);
                    assert_eq!(main.get_cards().row_data(2).unwrap().answer_mode, 0);
                    assert_eq!(main.get_uniform_answer_mode(), -1);
                    snapshot(main.window(), checks.output.join("question_list.png"));
                    assert!(
                        main.get_question_area_height() >= 144.0,
                        "The list must display at least three complete rows at the default size; actual height {}",
                        main.get_question_area_height()
                    );
                    main.invoke_toggle_all_cards(false);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 0);
                    let position = slint::LogicalPosition::new(
                        main.get_question_area_left() + 18.0,
                        main.get_question_area_top() + 2.0 * 58.0 + 29.0,
                    );
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
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 1);
                    assert!(main.get_selection_partial());
                    assert!(!main.get_selected_enabled());
                    click_matrix(&main, Some(2), 1);
                    assert_eq!(
                        main.get_cards().row_data(2).unwrap().answer_mode,
                        1,
                        "One click changes only this row"
                    );
                    assert_eq!(main.get_cards().row_data(1).unwrap().answer_mode, 2);
                    // Let model changes settle before the next keyboard event.
                    management_phase(checks.clone(), 16);
                    return;
                }
                16 => {
                    let main = main_window(app);
                    key(main.window(), Key::Tab.into());
                    snapshot(main.window(), checks.output.join("matrix_keyboard.png"));
                    management_phase(checks.clone(), 17);
                    return;
                }
                17 => {
                    let main = main_window(app);
                    key(main.window(), Key::Space.into());
                    assert_eq!(
                        main.get_cards().row_data(2).unwrap().answer_mode,
                        2,
                        "Tab and Space operate matrix cells"
                    );
                    click_matrix(&main, Some(2), 0);
                    assert_eq!(main.get_cards().row_data(2).unwrap().answer_mode, 0);
                    main.set_question_scroll_y(-144.0);
                    main.invoke_edit_card(2);
                    management_phase(checks.clone(), 1);
                    return;
                }
                1 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("question_editor.png"));
                    assert!(
                        main.get_question_area_height() >= 200.0,
                        "The editor must fit its basic fields; actual height {}",
                        main.get_question_area_height()
                    );
                    assert!(main.get_editor_visible());
                    assert!(main.get_editor_prompt_preview().formula_width > 0.0);
                    assert!(main.get_editor_answer_preview().formula_width > 0.0);
                    let before = app.store.load().unwrap();
                    main.set_editor_answer("".into());
                    main.invoke_save_editor();
                    assert!(!main.get_editor_error().is_empty());
                    assert!(main.get_editor_visible());
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_cancel_editor();
                    assert!(!main.get_editor_visible());
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_edit_card(2);
                    main.set_editor_prompt("√3/2 = ?（编辑）".into());
                    main.invoke_save_editor();
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[0].cards[2].prompt, "√3/2 = ?（编辑）");
                    assert_eq!(saved.packs[0].cards[2].id, before.packs[0].cards[2].id);
                    assert_eq!(saved.enabled_card_count(), 1);
                    main.invoke_toggle_all_cards(true);
                    main.invoke_add_card();
                }
                2 => {
                    let main = main_window(app);
                    assert_eq!(main.get_editor_title(), "新增题目");
                    main.set_editor_prompt("犹豫的英文动词？".into());
                    main.set_editor_answer("hesitate".into());
                    main.invoke_save_editor();
                    assert!(!main.get_editor_visible());
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[0].cards.len(), 21);
                    assert_eq!(saved.enabled_card_count(), 21);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(20, true);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 1);
                    app.tray.invoke_practice_now();
                }
                3 => {
                    let card = practice_window(app);
                    check_floating_window(card.window());
                    assert_eq!(card.get_prompt(), "犹豫的英文动词？");
                    assert_eq!(card.get_feedback(), 0);
                    assert_eq!(
                        app.practice.borrow().as_ref().unwrap().question.card.answer,
                        "hesitate"
                    );
                    snapshot(card.window(), checks.output.join("selected_question.png"));
                    for character in "hesitate".chars() {
                        key(card.window(), character.to_string().into());
                    }
                    assert_eq!(
                        card.get_answer_text(),
                        "hesitate",
                        "Native keyboard text must arrive before grading"
                    );
                    key(card.window(), Key::Return.into());
                    assert_eq!(card.get_feedback(), 1);
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(0, true);
                    app.tray.invoke_practice_now();
                }
                4 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_answer_mode(), 1);
                    assert_eq!(card.get_options().row_count(), 3);
                    snapshot(card.window(), checks.output.join("mixed_choice.png"));
                    let correct = (0..3)
                        .find(|index| {
                            app.practice
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .prepared
                                .accepts_choice(*index)
                                == Some(true)
                        })
                        .unwrap();
                    key(card.window(), (correct + 1).to_string().into());
                    assert_eq!(card.get_feedback(), 1);
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(1, true);
                    app.tray.invoke_practice_now();
                }
                5 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_answer_mode(), 2);
                    assert_eq!(card.get_options().row_count(), 2);
                    snapshot(card.window(), checks.output.join("mixed_judgment.png"));
                    let correct = (0..2)
                        .find(|index| {
                            app.practice
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .prepared
                                .accepts_choice(*index)
                                == Some(true)
                        })
                        .unwrap();
                    click_option(&card, correct);
                    assert_eq!(card.get_feedback(), 1);
                    app.close_practice();
                    app.show_main().unwrap();
                }
                6 => {
                    let main = main_window(app);
                    assert_eq!(main.get_uniform_answer_mode(), -1);
                    assert_eq!(main.get_cards().row_data(0).unwrap().answer_mode, 1);
                    assert_eq!(main.get_cards().row_data(1).unwrap().answer_mode, 2);
                    let participation = app.store.load().unwrap().settings.excluded_card_ids;
                    click_matrix(&main, None, 0);
                    assert_eq!(main.get_uniform_answer_mode(), 0);
                    assert!(
                        app.store
                            .load()
                            .unwrap()
                            .settings
                            .card_answer_modes
                            .is_empty()
                    );
                    assert_eq!(
                        app.store.load().unwrap().settings.excluded_card_ids,
                        participation
                    );
                    let before = app.store.load().unwrap();
                    main.invoke_set_all_answer_modes(1);
                    assert_eq!(
                        app.store.load().unwrap(),
                        before,
                        "Missing choice content rejects the entire batch"
                    );
                    main.invoke_set_all_answer_modes(2);
                    assert_eq!(app.store.load().unwrap(), before);
                    assert!(!main.get_editor_visible());
                    main.invoke_set_card_answer_mode(20, 2);
                    assert!(
                        !main.get_editor_visible(),
                        "Unavailable types do not unexpectedly open editing"
                    );
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_edit_card(20);
                    main.invoke_set_editor_answer_mode(2);
                    main.invoke_save_editor();
                    assert!(!main.get_editor_error().is_empty());
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_cancel_editor();
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_edit_card(20);
                    main.invoke_set_editor_answer_mode(2);
                }
                7 => {
                    let main = main_window(app);
                    main.set_editor_statement("hesitate 表示犹豫".into());
                    main.invoke_select_truth(true);
                    main.invoke_save_editor();
                    assert!(
                        !main.get_editor_visible(),
                        "Judgment saves with only its statement and truth choice"
                    );
                    assert_eq!(main.get_cards().row_data(20).unwrap().answer_mode, 2);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 1);
                    main.invoke_create_pack();
                }
                8 => {
                    let main = main_window(app);
                    assert!(main.get_editor_new_pack());
                    main.set_editor_pack_name("我的词汇".into());
                    main.set_editor_prompt("尝试的英文？".into());
                    main.set_editor_answer("try".into());
                    main.invoke_set_editor_answer_mode(2);
                    main.invoke_set_editor_answer_mode(0);
                    snapshot(main.window(), checks.output.join("new_pack_editor.png"));
                    main.invoke_save_editor();
                    let saved = app.store.load().unwrap();
                    assert_eq!(main.get_uniform_answer_mode(), 0);
                    assert_eq!(saved.packs.len(), 2);
                    assert_eq!(saved.packs[1].name, "我的词汇");
                    assert!(
                        saved.packs[1].cards[0].true_false.is_none(),
                        "An empty judgment draft does not persist after switching back to input"
                    );
                    let before = saved;
                    main.invoke_edit_card(0);
                    main.set_editor_prompt("未保存内容".into());
                    main.invoke_cancel_editor();
                    assert_eq!(app.store.load().unwrap(), before);
                    main.invoke_select_pack(0);
                    assert!(!main.get_description_expanded());
                    snapshot(main.window(), checks.output.join("pack_title.png"));
                    main.invoke_begin_rename();
                }
                9 => {
                    let main = main_window(app);
                    assert!(main.get_name_editing());
                    snapshot(main.window(), checks.output.join("pack_rename.png"));
                    main.set_name_draft("  ".into());
                    key(main.window(), Key::Return.into());
                    assert!(main.get_name_editing());
                    assert!(!main.get_metadata_error().is_empty());
                    let before = app.store.load().unwrap();
                    main.set_name_draft("常见角".into());
                    key(main.window(), Key::Return.into());
                    assert!(!main.get_name_editing());
                    let renamed = app.store.load().unwrap();
                    assert_eq!(renamed.packs[0].name, "常见角");
                    assert_eq!(renamed.packs[0].id, before.packs[0].id);
                    assert_eq!(renamed.packs[0].cards, before.packs[0].cards);
                    assert_eq!(renamed.settings, before.settings);
                    assert_eq!(main.get_packs().row_data(0).unwrap().name, "常见角");
                    main.invoke_begin_description_edit();
                }
                10 => {
                    let main = main_window(app);
                    main.set_description_draft(
                        "常见角的正弦、余弦与正切。\n偶尔练一道，记住就好。".into(),
                    );
                    main.invoke_save_pack_description();
                    assert!(!main.get_description_editing());
                    assert!(main.get_description_expanded());
                    let saved = app.store.load().unwrap();
                    assert!(saved.packs[0].description.contains("偶尔练一道"));
                    let exported = checks.output.join("authored_pack.json");
                    Store::export_pack(&exported, &saved.packs[0]).unwrap();
                    assert_eq!(Store::read_pack(&exported).unwrap(), saved.packs[0]);
                    main.invoke_toggle_card(0, true);
                    assert!(
                        main.get_description_expanded(),
                        "Row changes preserve disclosure state"
                    );
                    snapshot(main.window(), checks.output.join("pack_details.png"));
                    main.invoke_toggle_knowledge_mode();
                    assert!(main.get_knowledge_only());
                    assert_eq!(app.store.load().unwrap(), app.library.borrow().clone());
                    main.invoke_toggle_all_cards(false);
                    let before = app.store.load().unwrap();
                    main.invoke_select_pack(1);
                    assert!(!main.get_description_expanded());
                    assert!(main.get_knowledge_only(), "Mode is global across packs");
                    assert_eq!(app.store.load().unwrap(), before);
                    app.tray.invoke_practice_now();
                }
                11 => {
                    let card = practice_window(app);
                    check_floating_window(card.window());
                    check_no_caption_paint(card.window());
                    assert!(card.get_knowledge_only());
                    assert_eq!(card.get_prompt(), "尝试的英文？");
                    assert_eq!(card.get_standard_answer(), "try");
                    assert_eq!(card.get_options().row_count(), 0);
                    card.invoke_submit_answer("try".into());
                    card.invoke_choose_answer(0);
                    assert_eq!(card.get_feedback(), 0);
                    key(card.window(), "1".into());
                    assert_eq!(card.get_answer_text(), "");
                    snapshot(card.window(), checks.output.join("knowledge_text.png"));
                    key(card.window(), Key::Return.into());
                    assert!(app.practice.borrow().is_none());
                    app.show_main().unwrap();
                    let main = main_window(app);
                    assert!(
                        main.get_knowledge_only(),
                        "Closing windows keeps the temporary mode"
                    );
                    main.invoke_toggle_all_cards(false);
                    main.invoke_select_pack(0);
                    main.invoke_set_card_answer_mode(0, 1);
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(0, true);
                    snapshot(main.window(), checks.output.join("knowledge_matrix.png"));
                    // A timer-origin popup requires a closed main window and no pause.
                    main.invoke_set_pause(0);
                    main.invoke_go_background();
                    let probe = FocusProbe::new().unwrap();
                    checks.focus_probe.replace(Some(probe.clone_strong()));
                    let ready = checks.clone();
                    focus_probe_with_pointer(&probe, move || {
                        ready
                            .app
                            .show_practice_with_origin(PracticeOrigin::Reminder)
                            .unwrap();
                        management_phase(ready, 12);
                    });
                    return;
                }
                12 => {
                    let card = practice_window(app);
                    // Timer-origin knowledge cards preserve the other foreground window.
                    assert_eq!(
                        unsafe { GetForegroundWindow() },
                        native_handle(checks.focus_probe.borrow().as_ref().unwrap().window())
                    );
                    assert!(card.get_knowledge_only());
                    assert!(card.get_prompt_formula_width() > 0.0);
                    assert!(
                        card.get_standard_answer().is_empty(),
                        "A simple equation becomes a complete fact"
                    );
                    assert!(!card.get_prompt().contains('?'));
                    assert!(card.get_prompt().contains("1/2"));
                    assert_eq!(card.get_options().row_count(), 0);
                    snapshot(card.window(), checks.output.join("knowledge_math.png"));
                    checks
                        .focus_probe
                        .borrow_mut()
                        .take()
                        .unwrap()
                        .hide()
                        .unwrap();
                    let before = app.store.load().unwrap();
                    key(card.window(), Key::Escape.into());
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_knowledge_mode();
                    assert!(!main.get_knowledge_only());
                    assert_eq!(
                        app.store.load().unwrap(),
                        before,
                        "Global mode never writes quiz settings"
                    );
                    app.tray.invoke_practice_now();
                }
                13 => {
                    let card = practice_window(app);
                    assert!(!card.get_knowledge_only());
                    assert_eq!(
                        card.get_answer_mode(),
                        1,
                        "Quiz mode restores this question's actual type"
                    );
                    assert_eq!(card.get_options().row_count(), 3);
                    snapshot(card.window(), checks.output.join("restored_choice.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                }
                14 => {
                    let main = main_window(app);
                    assert!(main.get_question_area_height() >= 174.0);
                    snapshot(main.window(), checks.output.join("matrix_narrow.png"));
                    main.set_current_tab(1);
                }
                15 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("settings.png"));
                    main.invoke_toggle_knowledge_mode();
                    app.show_practice().unwrap();
                    assert!(practice_window(app).get_knowledge_only());
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        checks.finished.set(checks.app.practice.borrow().is_none());
                        if checks.finished.get() {
                            println!(
                                "Native one-click matrix, batch atomicity, keyboard controls, editing, metadata/export, global knowledge mode, nonactivating frameless facts, automatic close and quiz restoration passed."
                            );
                        }
                        checks.app.quit();
                    });
                    return;
                }
                _ => unreachable!(),
            }
            management_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens the native management window and exercises selection and question editing"]
fn native_question_management() {
    let _cursor = CursorGuard::capture();
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/question_management_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.feedback_seconds = 2;
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let window_hook = floating.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .with_winit_window_attributes_hook(move |attributes| window_hook.attributes(attributes))
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
    management_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Question management check failed or timed out"
    );
}

fn question_outline_pixels(main: &MainWindow) -> usize {
    let pixels = main.window().take_snapshot().unwrap();
    let scale = main.window().scale_factor();
    let left =
        ((main.get_question_area_left() + main.get_participation_width()) * scale).round() as u32;
    let top = (main.get_question_area_top() * scale).ceil() as u32 + 10;
    let bottom = ((main.get_question_area_top() + main.get_question_area_height()) * scale).floor()
        as u32
        - 10;
    (top..bottom)
        .flat_map(|y| (left..left + 3).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let pixel = pixels.as_slice()[(y * pixels.width() + x) as usize];
            pixel.r > 150 && pixel.g > 150 && pixel.b > 150
        })
        .count()
}

fn question_keyboard_pixels(main: &MainWindow) -> usize {
    let pixels = main.window().take_snapshot().unwrap();
    let scale = main.window().scale_factor();
    let left = ((main.get_question_area_left() + main.get_participation_width() + 10.0) * scale)
        .round() as u32;
    let right = left + (18.0 * scale).ceil() as u32;
    (0..3)
        .flat_map(|row| {
            let bottom = main.get_question_area_top()
                + (0..=row).map(|i| matrix_row_height(main, i)).sum::<f32>();
            let top = ((bottom - 12.0) * scale).round() as u32;
            let end = ((bottom + 4.0) * scale).round() as u32;
            (top..end).flat_map(move |y| (left..right).map(move |x| (x, y)))
        })
        .filter(|&(x, y)| {
            let pixel = pixels.as_slice()[(y * pixels.width() + x) as usize];
            pixel.r > 150 && pixel.g > 150 && pixel.b > 150
        })
        .count()
}

fn question_focus_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(250), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let main = main_window(&checks.app);
            if phase < 6 {
                if phase.is_multiple_of(2) {
                    let row = usize::from(phase / 2);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_question_area_left() + main.get_participation_width() + 30.0,
                            main.get_question_area_top()
                                + (0..row).map(|i| matrix_row_height(&main, i)).sum::<f32>()
                                + matrix_row_height(&main, row) / 2.0,
                        ),
                    );
                    assert!(main.get_editor_visible(), "The pointer opens this question");
                } else {
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_editor_cancel_center_x(),
                            main.get_editor_cancel_center_y(),
                        ),
                    );
                    assert!(
                        !main.get_editor_visible(),
                        "Pointer cancellation returns to the list"
                    );
                }
                question_focus_phase(checks.clone(), phase + 1);
            } else if phase == 6 {
                main.window().dispatch_event(WindowEvent::PointerMoved {
                    position: slint::LogicalPosition::new(8.0, 8.0),
                });
                snapshot(main.window(), checks.output.join("mouse_returns.png"));
                let outlines = question_outline_pixels(&main);
                println!("Question-outline pixels after three pointer returns: {outlines}");
                assert_eq!(
                    outlines, 0,
                    "Mouse editing and returning must leave no white question frames"
                );
                main.set_question_scroll_y(-30.0);
                question_focus_phase(checks.clone(), 7);
            } else if phase == 7 {
                snapshot(main.window(), checks.output.join("scrolled_returns.png"));
                assert_eq!(
                    question_outline_pixels(&main),
                    0,
                    "Scrolling retains no old mouse frames"
                );
                main.set_question_scroll_y(0.0);
                question_focus_phase(checks.clone(), 8);
            } else {
                let baseline = question_keyboard_pixels(&main);
                let mut reached_question = false;
                for _ in 0..80 {
                    key(main.window(), Key::Tab.into());
                    if question_keyboard_pixels(&main) > baseline + 20 {
                        reached_question = true;
                        break;
                    }
                }
                assert!(
                    reached_question,
                    "Tab still focuses a question with a short underline"
                );
                snapshot(main.window(), checks.output.join("keyboard_focus.png"));
                key(main.window(), Key::Return.into());
                assert!(
                    main.get_editor_visible(),
                    "Enter still opens the keyboard-focused question"
                );
                checks.finished.set(true);
                checks.app.quit();
            }
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens native windows and checks mouse editing does not leave focus frames"]
fn native_question_focus() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/question_focus_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
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
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    question_focus_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(15), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.finished.get(), "Question focus check failed");
}

fn click_card_delete(main: &MainWindow, confirm: bool) {
    pointer_click(
        main.window(),
        slint::LogicalPosition::new(
            if confirm {
                main.get_card_confirm_center_x()
            } else {
                main.get_card_delete_center_x()
            },
            if confirm {
                main.get_card_confirm_center_y()
            } else {
                main.get_card_delete_center_y()
            },
        ),
    );
}

fn question_deletion_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(300), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let main = main_window(app);
            match phase {
                0 => {
                    assert_eq!(main.get_cards().row_count(), 3);
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_question_area_left() + main.get_participation_width() + 30.0,
                            main.get_question_area_top() + matrix_row_height(&main, 0) / 2.0,
                        ),
                    );
                    assert!(main.get_editor_can_delete());
                    type_content(app, &main, 0, "7/3");
                    std::fs::write(
                        checks.output.join("draft.txt"),
                        main.get_editor_prompt().as_bytes(),
                    )
                    .unwrap();
                }
                1 => {
                    click_card_delete(&main, false);
                    assert!(main.get_card_delete_pending());
                    main.invoke_save_editor();
                    main.invoke_set_editor_answer_mode(2);
                    assert_eq!(main.get_editor_answer_mode(), 1);
                    assert_eq!(app.store.load().unwrap().packs[0].cards.len(), 3);
                }
                2 => {
                    snapshot(main.window(), checks.output.join("confirmation_zh.png"));
                    pointer_click(
                        main.window(),
                        slint::LogicalPosition::new(
                            main.get_card_cancel_center_x(),
                            main.get_card_cancel_center_y(),
                        ),
                    );
                    assert!(!main.get_card_delete_pending());
                    assert!(main.get_editor_visible());
                    assert_eq!(
                        main.get_editor_prompt(),
                        std::fs::read_to_string(checks.output.join("draft.txt")).unwrap()
                    );
                    click_card_delete(&main, false);
                }
                3 => {
                    key(main.window(), Key::Return.into());
                    assert!(
                        !main.get_card_delete_pending(),
                        "Default Enter cancels deletion"
                    );
                    click_card_delete(&main, false);
                }
                4 => {
                    key(main.window(), Key::Escape.into());
                    assert!(!main.get_card_delete_pending());
                    click_card_delete(&main, false);
                }
                5 => {
                    use std::os::windows::fs::OpenOptionsExt;
                    let before = app.library.borrow().clone();
                    let locked = std::fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(app.store.directory().join("state.json"))
                        .unwrap();
                    click_card_delete(&main, true);
                    assert_eq!(*app.library.borrow(), before);
                    assert!(main.get_card_delete_pending());
                    assert!(main.get_message_is_error());
                    snapshot(main.window(), checks.output.join("save_failure.png"));
                    drop(locked);
                }
                6 => {
                    click_card_delete(&main, true);
                    assert!(!main.get_editor_visible());
                    assert_eq!(main.get_cards().row_count(), 2);
                    assert!(app.previous_question.borrow().is_none());
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[1].cards.len(), 3);
                    assert_eq!(
                        saved.settings.card_answer_modes[&saved.packs[0].id].len(),
                        1
                    );
                    main.invoke_choose_language(2);
                    main.invoke_edit_card(0);
                }
                7 => click_card_delete(&main, false),
                8 => main
                    .window()
                    .set_size(slint::LogicalSize::new(720.0, 480.0)),
                9 => {
                    snapshot(
                        main.window(),
                        checks.output.join("confirmation_en_minimum.png"),
                    );
                    click_card_delete(&main, true);
                    assert_eq!(main.get_cards().row_count(), 1);
                    main.invoke_edit_card(0);
                }
                10 => click_card_delete(&main, false),
                11 => {
                    click_card_delete(&main, true);
                    assert!(main.get_has_selection());
                    assert_eq!(main.get_cards().row_count(), 0);
                    assert!(!main.get_selected_enabled());
                    assert!(!main.get_packs().row_data(0).unwrap().has_cards);
                    let saved = app.store.load().unwrap();
                    assert!(saved.packs[0].cards.is_empty());
                    assert_eq!(saved.packs[1].cards.len(), 3);
                    assert!(saved.settings.card_answer_modes.is_empty());
                    let exported = app.store.directory().join("empty_export.json");
                    Store::export_pack(&exported, &saved.packs[0]).unwrap();
                    assert_eq!(Store::read_pack(&exported).unwrap(), saved.packs[0]);
                    assert_eq!(
                        Store::read_pack(&app.store.directory().join("original.json"))
                            .unwrap()
                            .cards
                            .len(),
                        3
                    );
                    app.close_main();
                    app.show_main().unwrap();
                }
                12 => {
                    assert_eq!(main.get_cards().row_count(), 0);
                    snapshot(main.window(), checks.output.join("empty_pack.png"));
                    main.invoke_add_card();
                    assert!(!main.get_editor_can_delete());
                    main.invoke_delete_card();
                    assert!(!main.get_card_delete_pending());
                    type_content(app, &main, 0, "重新添加的问题");
                    type_content(app, &main, 1, "答案");
                    main.invoke_save_editor();
                    assert_eq!(main.get_cards().row_count(), 1);
                    assert_eq!(app.store.load().unwrap().packs[0].cards.len(), 1);
                    main.invoke_edit_card(0);
                    click_card_delete(&main, false);
                    app.close_main();
                    assert!(app.pending_card_delete.borrow().is_none());
                    app.show_main().unwrap();
                    assert!(!main_window(app).get_card_delete_pending());
                    checks.finished.set(true);
                    println!(
                        "Native question deletion, draft preservation, Cancel/Enter/Esc, failed-save retry, empty-pack export/reopen/add and original-file preservation passed."
                    );
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            question_deletion_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens isolated native windows to verify question deletion and empty packs"]
fn native_question_deletion() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/question_deletion_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.packs[0].cards.truncate(3);
    let original = library.packs[0].clone();
    let first = original.cards[0].id.clone().unwrap();
    library
        .set_card_answer_mode(&original.id, &first, AnswerMode::MultipleChoice)
        .unwrap();
    library
        .set_card_answer_mode(
            &original.id,
            original.cards[1].id.as_deref().unwrap(),
            AnswerMode::TrueFalse,
        )
        .unwrap();
    let mut retained = original.clone();
    retained.id = "retained_questions".into();
    library.import(retained, false).unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    Store::export_pack(&profile.path().join("original.json"), &original).unwrap();
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
    app.previous_question.replace(Some(QuestionKey {
        pack_id: original.id,
        card_id: first,
    }));
    let checks = Rc::new(AnswerChecks {
        app,
        output,
        finished: Cell::new(false),
        focus_probe: RefCell::new(None),
    });
    question_deletion_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.finished.get(), "Question deletion check failed");
}

fn click_pack(main: &MainWindow, row: usize, toggle: bool) {
    pointer_click(
        main.window(),
        slint::LogicalPosition::new(
            main.get_pack_toggle_center_x() - if toggle { 0.0 } else { 70.0 },
            main.get_pack_list_top()
                + row as f32 * main.get_pack_row_stride()
                + main.get_pack_row_height() / 2.0,
        ),
    );
}

fn click_delete_pack(main: &MainWindow, confirm: bool) {
    let (x, y) = if confirm {
        (
            main.get_delete_confirm_center_x(),
            main.get_delete_confirm_center_y(),
        )
    } else {
        (
            main.get_delete_button_center_x(),
            main.get_delete_button_center_y(),
        )
    };
    pointer_click(main.window(), slint::LogicalPosition::new(x, y));
}

fn deletion_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(300), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let main = main_window(app);
            match phase {
                0 => {
                    click_pack(&main, 1, false);
                    main.invoke_edit_card(0);
                    click_delete_pack(&main, false);
                    main.invoke_delete_pack();
                    assert!(main.get_editor_visible());
                    assert!(!main.get_delete_pending());
                    main.invoke_cancel_editor();
                    click_delete_pack(&main, false);
                    assert!(main.get_delete_pending());
                    assert!(!main.get_tray_settings_available());
                }
                1 => {
                    snapshot(main.window(), checks.output.join("confirmation_zh.png"));
                    let before = app.store.load().unwrap();
                    click_pack(&main, 2, false);
                    click_pack(&main, 0, true);
                    click_matrix(&main, Some(0), 1);
                    assert_eq!(main.get_selected_index(), 1);
                    assert_eq!(app.store.load().unwrap(), before);
                    // The initial keyboard focus is Cancel, so Enter is safe.
                    key(main.window(), Key::Return.into());
                    assert!(!main.get_delete_pending());
                    assert_eq!(app.store.load().unwrap(), before);
                }
                2 => click_delete_pack(&main, false),
                3 => {
                    key(main.window(), Key::Escape.into());
                    assert!(!main.get_delete_pending());
                    assert_eq!(main.get_packs().row_count(), 3);
                    click_delete_pack(&main, false);
                    main.invoke_choose_language(2);
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                }
                4 => {
                    use std::os::windows::fs::OpenOptionsExt;
                    snapshot(
                        main.window(),
                        checks.output.join("confirmation_en_minimum.png"),
                    );
                    assert!(main.get_delete_description().contains("cannot be undone"));
                    let size = main
                        .window()
                        .size()
                        .to_logical(main.window().scale_factor());
                    assert!(main.get_delete_confirm_center_x() < size.width - 24.0);
                    assert!(main.get_delete_confirm_center_y() < size.height - 60.0);
                    let before = app.store.load().unwrap();
                    let state = app.store.directory().join("state.json");
                    let bytes = std::fs::read(&state).unwrap();
                    // Allow reading but deny replacement of this isolated state file.
                    let locked = std::fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(&state)
                        .unwrap();
                    click_delete_pack(&main, true);
                    assert!(main.get_message_is_error());
                    assert!(main.get_delete_pending());
                    assert_eq!(main.get_selected_index(), 1);
                    assert_eq!(*app.library.borrow(), before);
                    assert_eq!(std::fs::read(&state).unwrap(), bytes);
                    snapshot(main.window(), checks.output.join("save_failure.png"));
                    drop(locked);
                    app.previous_question.replace(Some(QuestionKey {
                        pack_id: "delete_me".into(),
                        card_id: before.packs[1].cards[0].id.clone().unwrap(),
                    }));
                    click_delete_pack(&main, true);
                    assert!(!main.get_delete_pending());
                    assert!(!main.get_message_is_error());
                    assert!(app.previous_question.borrow().is_none());
                    let saved = app.store.load().unwrap();
                    assert_eq!(
                        saved.packs,
                        [before.packs[0].clone(), before.packs[2].clone()]
                    );
                    assert_eq!(main.get_selected_index(), 1);
                    assert_eq!(main.get_selected_name(), saved.packs[1].name);
                    assert!(app.store.directory().join("original_import.json").is_file());
                }
                5 => {
                    snapshot(main.window(), checks.output.join("remaining_packs.png"));
                    click_delete_pack(&main, false);
                    app.close_main();
                    assert!(app.pending_delete.borrow().is_none());
                    app.show_main().unwrap();
                    assert!(!main_window(app).get_delete_pending());
                    assert_eq!(app.store.load().unwrap().packs.len(), 2);
                }
                6 => click_delete_pack(&main, false),
                7 => {
                    click_delete_pack(&main, true);
                    assert_eq!(main.get_packs().row_count(), 1);
                    assert_eq!(main.get_selected_index(), 0);
                    assert_eq!(main.get_selected_name(), "回归测试");
                    assert!(main.get_cards().row_count() > 0);
                    click_delete_pack(&main, false);
                }
                8 => {
                    click_delete_pack(&main, true);
                    assert!(!main.get_has_selection());
                    assert_eq!(main.get_packs().row_count(), 0);
                    assert_eq!(main.get_cards().row_count(), 0);
                    assert!(main.get_selected_name().is_empty());
                    assert!(main.get_selected_description().is_empty());
                    assert!(!app.tray.get_can_practice());
                    assert!(app.store.load().unwrap().packs.is_empty());
                    app.close_main();
                    app.show_main().unwrap();
                }
                9 => {
                    assert!(!main.get_has_selection());
                    snapshot(main.window(), checks.output.join("empty_library.png"));
                    main.invoke_delete_pack();
                    assert!(!main.get_delete_pending());
                    main.invoke_create_pack();
                    assert!(main.get_editor_visible());
                    assert!(main.get_editor_new_pack());
                    main.invoke_cancel_editor();
                    let source = app.store.directory().join("original_import.json");
                    app.commit(|library| library.import(Store::read_pack(&source)?, false))
                        .unwrap();
                    assert!(main.get_has_selection());
                    assert!(main.get_cards().row_count() > 0);
                    assert!(app.tray.get_can_practice());
                    assert_eq!(app.store.load().unwrap().packs.len(), 1);
                    checks.finished.set(true);
                    std::fs::write(checks.output.join("result.txt"),
                        "Native deletion, Cancel/Enter/Esc, draft and selection guards, failed-save recovery, adjacent selection, empty-state reopening, new/import recovery and original-file preservation passed.").unwrap();
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            deletion_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens isolated native windows to verify pack deletion and persistence"]
fn native_pack_deletion() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/pack_deletion_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    for (id, name) in [
        (
            "delete_me",
            "待删除的知识包 · A saved pack with questions and local preferences",
        ),
        ("retained_pack", "保留的知识包"),
    ] {
        let mut pack = library.packs[0].clone();
        pack.id = id.into();
        pack.name = name.into();
        library.import(pack, false).unwrap();
    }
    Store::export_pack(
        &profile.path().join("original_import.json"),
        &library.packs[1],
    )
    .unwrap();
    library
        .set_all_answer_modes("delete_me", AnswerMode::TrueFalse)
        .unwrap();
    let card_id = library.packs[1].cards[0].id.clone().unwrap();
    library
        .set_card_enabled("delete_me", &card_id, false)
        .unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.interface_language = InterfaceLanguage::Chinese;
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let window_hook = floating.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .with_winit_window_attributes_hook(move |attributes| window_hook.attributes(attributes))
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
    deletion_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(20), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Pack deletion check failed or timed out"
    );
}

fn multi_pack_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(300), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    assert_eq!(main.get_packs().row_count(), 2);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 21);
                    main.set_current_tab(1);
                    click_pack(&main, 1, true);
                    assert_eq!(main.get_current_tab(), 1);
                    assert_eq!(main.get_selected_index(), 0);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 19);
                }
                1 => {
                    let main = main_window(app);
                    click_pack(&main, 1, true);
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.pack_enabled_card_count(&saved.packs[1]), 2);
                    assert_eq!(main.get_current_tab(), 1);
                    assert_eq!(main.get_selected_index(), 0);
                    click_pack(&main, 1, false);
                    assert_eq!(main.get_current_tab(), 0);
                    assert_eq!(main.get_selected_index(), 1);
                    assert_eq!(main.get_cards().row_count(), 3);
                    for (index, mode) in [0, 1, 2].into_iter().enumerate() {
                        assert_eq!(main.get_cards().row_data(index).unwrap().answer_mode, mode);
                    }
                }
                2 => {
                    let main = main_window(app);
                    main.invoke_edit_card(0);
                    assert!(main.get_editor_visible());
                    click_pack(&main, 0, true);
                    assert!(app.store.load().unwrap().is_enabled("test_pack"));
                    assert!(main.get_editor_visible());
                    main.invoke_cancel_editor();
                }
                3 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("multiple_packs.png"));
                    main.window()
                        .set_size(slint::LogicalSize::new(720.0, 480.0));
                }
                4 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("minimum_size.png"));
                    click_pack(&main, 0, true);
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.enabled_card_count(), 2);
                    assert!(
                        saved
                            .participating_cards()
                            .all(|(pack, _)| pack.id == "english_basics")
                    );
                    app.show_practice().unwrap();
                }
                5 => {
                    let card = practice_window(app);
                    check_floating_window(card.window());
                    let saved = app.store.load().unwrap();
                    {
                        let practice = app.practice.borrow();
                        let session = practice.as_ref().unwrap();
                        assert_eq!(session.question.key.pack_id, "english_basics");
                        let original = saved.packs[1]
                            .cards
                            .iter()
                            .find(|original| {
                                original.id.as_deref() == Some(&session.question.key.card_id)
                            })
                            .unwrap();
                        assert_eq!(session.question.card, *original);
                        assert_eq!(
                            card.get_answer_mode(),
                            saved
                                .card_answer_mode(
                                    &session.question.key.pack_id,
                                    &session.question.key.card_id,
                                )
                                .index()
                        );
                    }
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_pack("test_pack".into(), true);
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 21);
                    checks.finished.set(true);
                    std::fs::write(checks.output.join("result.txt"),
                        "Native sidebar switches, browsing separation, saved ranges, mixed question modes, draft protection, multiple-pack sampling and minimum-size layout passed.").unwrap();
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            multi_pack_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens the native window to check independent knowledge-pack participation"]
fn native_multi_pack_participation() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/multi_pack_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    let english = KnowledgePack::from_json(
        r#"{
        "format":"try","schema_version":1,"id":"english_basics","name":"英语 · 常用词",
        "cards":[
            {"id":"hesitate","prompt":"“犹豫”的英文动词？","answer":"hesitate"},
            {"id":"accurate","prompt":"“准确的”的英文？","answer":"accurate",
             "distractors":[{"text":"approximate"},{"text":"vague"}]},
            {"id":"sensible","prompt":"“明智的”的英文？","answer":"sensible",
             "true_false":{"statement":"sensible 表示“敏感的”","is_true":false,
                           "correction":"sensible 表示“明智的”"}}
        ]
    }"#
        .as_bytes(),
    )
    .unwrap();
    library.import(english, false).unwrap();
    library
        .set_card_enabled("english_basics", "hesitate", false)
        .unwrap();
    library
        .set_card_answer_mode("english_basics", "accurate", AnswerMode::MultipleChoice)
        .unwrap();
    library
        .set_card_answer_mode("english_basics", "sensible", AnswerMode::TrueFalse)
        .unwrap();
    let first = library.packs[0].cards[0].id.clone().unwrap();
    library
        .set_card_enabled("test_pack", &first, false)
        .unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let window_hook = floating.clone();
    let mut builder = EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    builder.with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder(builder)
        .with_winit_window_attributes_hook(move |attributes| window_hook.attributes(attributes))
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
    multi_pack_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(10), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Multi-pack check failed or timed out"
    );
}

fn quick_answer_point(
    main: &MainWindow,
    row: usize,
    offset: Option<f32>,
) -> slint::LogicalPosition {
    slint::LogicalPosition::new(
        main.get_matrix_area_left() + main.get_matrix_area_width()
            - 3.0 * main.get_matrix_mode_width()
            - main.get_matrix_answer_width()
            + offset.unwrap_or(main.get_matrix_answer_width() / 2.0),
        main.get_question_area_top()
            + (0..row).map(|i| matrix_row_height(main, i)).sum::<f32>()
            + matrix_row_height(main, row) / 2.0,
    )
}

fn click_quick_answer(main: &MainWindow, row: usize, offset: Option<f32>) {
    let position = quick_answer_point(main, row, offset);
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
}

fn replace_quick_input(main: &MainWindow, input: &str) {
    main.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    key(main.window(), "a".into());
    main.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    key(main.window(), input.into());
}

fn quick_answers_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            if phase == 7 {
                let card = practice_window(app);
                assert_eq!(card.get_answer_mode(), 2);
                let correct = (0..2)
                    .find(|index| {
                        app.practice
                            .borrow()
                            .as_ref()
                            .unwrap()
                            .prepared
                            .accepts_choice(*index)
                            == Some(true)
                    })
                    .unwrap();
                click_option(&card, correct);
                assert_eq!(
                    card.get_feedback(),
                    1,
                    "Practice uses the newly configured judgment answer"
                );
                app.close_practice();
                app.show_main().unwrap();
                let main = main_window(app);
                main.invoke_toggle_knowledge_mode();
                main.window()
                    .set_size(slint::LogicalSize::new(720.0, 480.0));
                quick_answers_phase(checks.clone(), phase + 1);
                return;
            }
            let main = main_window(app);
            match phase {
                0 => {
                    assert!(
                        main.get_cards()
                            .row_data(0)
                            .unwrap()
                            .answer_preview
                            .formula_width
                            > 0.0
                    );
                    assert_eq!(main.get_cards().row_data(1).unwrap().answer_mode, 1);
                    assert_eq!(main.get_cards().row_data(2).unwrap().answer_mode, 2);
                    snapshot(main.window(), checks.output.join("quick_answers.png"));
                    click_quick_answer(&main, 0, None);
                    replace_quick_input(&main, "3/5");
                    key(main.window(), Key::Return.into());
                    assert!(app.store.load().unwrap().packs[0].cards[0].accepts("3/5"));
                    assert!(
                        !main.get_editor_visible(),
                        "Quick editing stays in the matrix"
                    );
                }
                1 => {
                    let saved = app.store.load().unwrap();
                    assert!(!saved.packs[0].cards[0].accepts("1/2"));
                    assert!(saved.packs[0].cards[0].aliases.is_empty());
                    replace_quick_input(&main, "sqrt(");
                    key(main.window(), Key::Return.into());
                    assert_eq!(
                        app.store.load().unwrap(),
                        saved,
                        "An invalid formula keeps the valid card"
                    );
                }
                2 => {
                    snapshot(main.window(), checks.output.join("quick_invalid.png"));
                    key(main.window(), Key::Escape.into());
                    replace_quick_input(&main, "7/9");
                    key(main.window(), Key::Tab.into());
                }
                3 => {
                    assert!(
                        app.store.load().unwrap().packs[0].cards[0].accepts("7/9"),
                        "Tab commits the answer before moving focus"
                    );
                    click_quick_answer(&main, 0, None);
                    replace_quick_input(&main, "5/7");
                    key(main.window(), Key::Escape.into());
                    assert!(
                        app.store.load().unwrap().packs[0].cards[0].accepts("7/9"),
                        "Esc cancels only the inline draft"
                    );
                    click_quick_answer(&main, 1, None);
                }
                4 => {
                    assert_eq!(main.get_quick_options().row_count(), 3);
                    assert!(main.get_quick_options().row_data(1).unwrap().formula_width > 0.0);
                    snapshot(main.window(), checks.output.join("quick_choices.png"));
                    key(main.window(), Key::DownArrow.into());
                    key(main.window(), Key::Return.into());
                }
                5 => {
                    let saved = app.store.load().unwrap();
                    let original = &r#try::test_fixtures::regression_library().packs[0].cards[1];
                    let card = &saved.packs[0].cards[1];
                    assert_eq!(card.answer, original.distractors[0].text);
                    assert_eq!(card.answer_latex, original.distractors[0].latex);
                    assert_eq!(card.distractors[0].text, original.answer);
                    assert_eq!(
                        saved.card_answer_mode("test_pack", card.id.as_deref().unwrap()),
                        AnswerMode::MultipleChoice
                    );
                    let truth = saved.packs[0].cards[2].true_false.as_ref().unwrap().is_true;
                    click_quick_answer(&main, 2, Some(if truth { 68.0 } else { 26.0 }));
                }
                6 => {
                    let saved = app.store.load().unwrap();
                    let original = r#try::test_fixtures::regression_library();
                    assert_ne!(
                        saved.packs[0].cards[2].true_false.as_ref().unwrap().is_true,
                        original.packs[0].cards[2]
                            .true_false
                            .as_ref()
                            .unwrap()
                            .is_true
                    );
                    let export = checks.output.join("quick_answers_export.json");
                    Store::export_pack(&export, &saved.packs[0]).unwrap();
                    assert_eq!(
                        KnowledgePack::from_json(&std::fs::read(export).unwrap()).unwrap(),
                        saved.packs[0]
                    );
                    snapshot(main.window(), checks.output.join("quick_saved.png"));
                    main.invoke_toggle_all_cards(false);
                    main.invoke_toggle_card(2, true);
                    app.tray.invoke_practice_now();
                }
                8 => {
                    assert!(main.get_matrix_answer_width() >= 84.0);
                    assert!(
                        main.get_matrix_area_width()
                            - 3.0 * main.get_matrix_mode_width()
                            - main.get_matrix_answer_width()
                            - main.get_participation_width()
                            > 100.0,
                        "Question and answer remain distinct at the minimum width"
                    );
                    snapshot(main.window(), checks.output.join("quick_compact.png"));
                    let before = app.store.load().unwrap();
                    assert!(
                        !main
                            .invoke_save_quick_answer(
                                "test_pack".into(),
                                "missing_card".into(),
                                "9/11".into()
                            )
                            .is_empty()
                    );
                    assert_eq!(
                        app.store.load().unwrap(),
                        before,
                        "A stale row identifier never edits another card"
                    );
                    main.invoke_toggle_knowledge_mode();
                    let id = main.get_cards().row_data(0).unwrap().id;
                    assert!(
                        main.invoke_save_quick_answer(
                            "test_pack".into(),
                            id,
                            "[[1,2],[3,4]]".into()
                        )
                        .is_empty()
                    );
                }
                9 => {
                    assert!(
                        main.get_cards()
                            .row_data(0)
                            .unwrap()
                            .answer_preview
                            .formula_height
                            > 30.0
                    );
                    snapshot(main.window(), checks.output.join("quick_tall_formula.png"));
                    checks.finished.set(true);
                    app.quit();
                    println!(
                        "Native inline formula editing, Enter/Tab saving, Esc cancellation, invalid draft protection, choice/truth selection, export, practice and minimum-width layout passed."
                    );
                    return;
                }
                _ => unreachable!(),
            }
            quick_answers_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens real matrix windows with isolated question data"]
fn native_quick_answers() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/quick_answers_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    let pack_id = library.packs[0].id.clone();
    let choice_id = library.packs[0].cards[1].id.clone().unwrap();
    let truth_id = library.packs[0].cards[2].id.clone().unwrap();
    library
        .set_card_answer_mode(&pack_id, &choice_id, AnswerMode::MultipleChoice)
        .unwrap();
    library
        .set_card_answer_mode(&pack_id, &truth_id, AnswerMode::TrueFalse)
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
    quick_answers_phase(checks.clone(), 0);
    let watchdog = Rc::downgrade(&checks.app);
    Timer::single_shot(Duration::from_secs(20), move || {
        if let Some(app) = watchdog.upgrade() {
            app.quit();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Quick answer check failed or timed out"
    );
}

fn pack_tags_phase(checks: Rc<AnswerChecks>, phase: u8, hidden_height: f32) {
    Timer::single_shot(Duration::from_millis(250), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            let mut baseline = hidden_height;
            match phase {
                0 => {
                    let main = main_window(app);
                    main.invoke_begin_tag_edit();
                    assert!(!main.get_tray_settings_available());
                    main.set_tag_draft("  数学基础  ".into());
                    main.invoke_save_pack_tag();
                    assert!(!main.get_tag_editing());
                    assert_eq!(main.get_selected_tag(), "数学基础");
                    assert_eq!(main.get_packs().row_data(0).unwrap().tag, "数学基础");
                    let saved = app.store.load().unwrap();
                    assert_eq!(saved.packs[0].tag, "数学基础");
                    Store::export_pack(&checks.output.join("tagged.json"), &saved.packs[0])
                        .unwrap();
                    assert_eq!(
                        Store::read_pack(&checks.output.join("tagged.json")).unwrap(),
                        saved.packs[0]
                    );
                    snapshot(main.window(), checks.output.join("pack_metadata.png"));
                    main.invoke_begin_tag_edit();
                }
                1 => {
                    let main = main_window(app);
                    let before = app.store.load().unwrap();
                    main.set_tag_draft("字".repeat(25).into());
                    main.invoke_save_pack_tag();
                    assert!(main.get_tag_editing());
                    assert!(!main.get_metadata_error().is_empty());
                    assert_eq!(app.store.load().unwrap(), before);
                    key(main.window(), Key::Escape.into());
                }
                2 => {
                    let main = main_window(app);
                    assert!(!main.get_tag_editing(), "Esc cancels the invalid tag draft");
                    main.invoke_set_show_pack_tags(false);
                    assert!(!app.store.load().unwrap().settings.show_pack_tags);
                    app.close_main();
                    app.show_main().unwrap();
                    assert!(!main_window(app).get_show_pack_tags());
                    assert_eq!(
                        main_window(app).get_packs().row_data(0).unwrap().tag,
                        "数学基础"
                    );
                    snapshot(
                        main_window(app).window(),
                        checks.output.join("sidebar_hidden.png"),
                    );
                    app.show_practice().unwrap();
                }
                3 => {
                    let card = practice_window(app);
                    assert!(!card.get_pack_tag_visible());
                    assert_eq!(card.get_pack_tag(), "数学基础");
                    baseline = card
                        .window()
                        .size()
                        .to_logical(card.window().scale_factor())
                        .height;
                    snapshot(card.window(), checks.output.join("hidden_dark.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    main_window(app).invoke_set_show_pack_tags(true);
                    app.show_practice().unwrap();
                }
                4 => {
                    let card = practice_window(app);
                    assert!(card.get_pack_tag_visible());
                    assert!(
                        card.get_tag_on_right(),
                        "Left-hand card uses the right tag corner"
                    );
                    let height = card
                        .window()
                        .size()
                        .to_logical(card.window().scale_factor())
                        .height;
                    assert!((height - baseline - 26.0).abs() < 1.0);
                    assert!(card.get_pack_tag_left() > 160.0);
                    snapshot(card.window(), checks.output.join("dark_left.png"));
                    card.window().with_winit_window(|native| {
                        let monitor = native.current_monitor().unwrap();
                        let position = native.outer_position().unwrap();
                        native.set_outer_position(
                            slint::winit_030::winit::dpi::PhysicalPosition::new(
                                monitor.position().x + monitor.size().width as i32
                                    - native.outer_size().width as i32
                                    - 48,
                                position.y,
                            ),
                        );
                    });
                }
                5 => {
                    let card = practice_window(app);
                    assert!(
                        !card.get_tag_on_right(),
                        "Native move switches the tag to the left corner"
                    );
                    assert_eq!(card.get_pack_tag_left(), 24.0);
                    snapshot(card.window(), checks.output.join("dark_right.png"));
                    card.invoke_submit_answer("wrong".into());
                }
                6 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_feedback(), 2);
                    assert!(card.get_pack_tag_visible());
                    snapshot(card.window(), checks.output.join("dark_wrong.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_choose_appearance(Appearance::Light.index());
                    main.invoke_set_corner(PopupCorner::TopRight.index());
                    app.knowledge_only.set(true);
                    app.show_practice().unwrap();
                }
                7 => {
                    let card = practice_window(app);
                    assert!(!card.get_dark_theme());
                    assert!(card.get_knowledge_only());
                    assert!(!card.get_tag_on_right());
                    snapshot(card.window(), checks.output.join("light_knowledge.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_begin_tag_edit();
                    main.set_tag_draft("".into());
                    main.invoke_save_pack_tag();
                    assert!(app.store.load().unwrap().packs[0].tag.is_empty());
                    assert!(main.get_packs().row_data(0).unwrap().tag.is_empty());
                    app.show_practice().unwrap();
                }
                8 => {
                    let card = practice_window(app);
                    assert!(card.get_pack_tag().is_empty());
                    assert!(!card.get_pack_tag_visible());
                    snapshot(card.window(), checks.output.join("empty_tag.png"));
                    app.close_practice();
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_begin_tag_edit();
                    main.set_tag_draft("数学".repeat(12).into());
                    main.invoke_save_pack_tag();
                    main.invoke_choose_language(InterfaceLanguage::English.index());
                    assert_eq!(main.get_packs().row_data(0).unwrap().tag, "数学".repeat(12));
                    snapshot(main.window(), checks.output.join("english_metadata.png"));
                    app.show_practice().unwrap();
                }
                9 => {
                    let card = practice_window(app);
                    assert_eq!(card.get_pack_tag(), "数学".repeat(12));
                    assert!(card.get_pack_tag_width() <= 138.0);
                    assert!(card.get_pack_tag_visible());
                    snapshot(card.window(), checks.output.join("light_long_tag.png"));
                    std::fs::write(checks.output.join("result.txt"), "Native pack metadata editing, invalid-draft cancellation, persisted visibility, export, 26px header, moved-event inward alignment, dark feedback, light fact viewing, blank tags and long-tag width passed.").unwrap();
                    checks.finished.set(true);
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            pack_tags_phase(checks.clone(), phase + 1, baseline);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens isolated real windows to verify knowledge-pack tags"]
fn native_pack_tags() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/pack_tag_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = Store::open(profile.path().to_owned()).unwrap();
    let mut library = store.load().unwrap();
    let pack_id = library.packs[0].id.clone();
    let card_id = library.packs[0].cards[0].id.clone().unwrap();
    library.settings.enabled_pack_ids = vec![pack_id.clone()];
    library.set_all_cards_enabled(&pack_id, false).unwrap();
    library.set_card_enabled(&pack_id, &card_id, true).unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.interface_language = InterfaceLanguage::Chinese;
    library.settings.popup_corner = PopupCorner::BottomLeft;
    library.settings.remember_position = false;
    library.settings.feedback_seconds = 30;
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
    pack_tags_phase(checks.clone(), 0, 0.0);
    let watchdog = Rc::downgrade(&checks.app);
    Timer::single_shot(Duration::from_secs(20), move || {
        if let Some(app) = watchdog.upgrade() {
            app.quit();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Knowledge-pack tag check failed or timed out"
    );
}
