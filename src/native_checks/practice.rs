//! Explicit native checks for practice.
use super::*;

pub(super) fn starter_question(app: &Rc<App>, pack_id: &str, card_index: i32) {
    app.close_practice();
    app.show_main().unwrap();
    let main = main_window(app);
    let saved = app.store.load().unwrap();
    for pack in &saved.packs {
        main.invoke_toggle_pack(pack.id.clone().into(), pack.id == pack_id);
    }
    let index = saved.packs.iter().position(|p| p.id == pack_id).unwrap();
    main.invoke_select_pack(index as i32);
    main.invoke_toggle_all_cards(false);
    main.invoke_toggle_card(card_index, true);
    assert_eq!(app.store.load().unwrap().enabled_card_count(), 1);
    app.show_practice().unwrap();
}

fn starter_packs_phase(checks: Rc<AnswerChecks>, phase: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0..=4 => {
                    let index = usize::from(phase);
                    let main = main_window(app);
                    let saved = app.store.load().unwrap();
                    assert_eq!(main.get_packs().row_count(), 5);
                    assert_eq!(saved.enabled_card_count(), 20);
                    assert_eq!(main.get_selected_index(), i32::from(phase));
                    assert_eq!(main.get_selected_name(), saved.packs[index].name);
                    assert_eq!(main.get_cards().row_count(), 4);
                    snapshot(
                        main.window(),
                        checks.output.join(format!("{}.png", saved.packs[index].id)),
                    );
                    if phase < 4 {
                        main.invoke_select_pack(i32::from(phase) + 1);
                    } else {
                        for pack in &saved.packs {
                            main.invoke_toggle_pack(
                                pack.id.clone().into(),
                                pack.id == "english_recall",
                            );
                        }
                        main.window()
                            .set_size(slint::LogicalSize::new(720.0, 480.0));
                    }
                }
                5 => {
                    let main = main_window(app);
                    snapshot(main.window(), checks.output.join("minimum_size.png"));
                    assert_eq!(app.store.load().unwrap().enabled_card_count(), 4);
                    app.show_practice().unwrap();
                }
                6 | 8 | 10 | 12 | 14 => {
                    let window = practice_window(app);
                    let (pack_id, answer) = {
                        let practice = app.practice.borrow();
                        let session = practice.as_ref().unwrap();
                        (
                            session.question.key.pack_id.clone(),
                            session.question.card.answer.clone(),
                        )
                    };
                    let expected = match phase {
                        6 => "english_recall",
                        8 | 10 => "programming_recall",
                        12 => "math_recall",
                        14 => "chemistry_recall",
                        _ => unreachable!(),
                    };
                    assert_eq!(pack_id, expected);
                    snapshot(
                        window.window(),
                        checks.output.join(format!("question_{phase}.png")),
                    );
                    for character in answer.chars() {
                        key(window.window(), character.to_string().into());
                    }
                    key(window.window(), Key::Return.into());
                    assert_eq!(window.get_answer_text(), answer);
                    assert_eq!(window.get_feedback(), 1);
                    if phase < 12 {
                        assert_eq!(window.get_input_preview().formula_width, 0.0);
                    } else {
                        assert!(window.get_prompt_formula_width() > 0.0);
                        assert!(window.get_input_preview().formula_width > 0.0);
                    }
                }
                7 | 9 | 11 | 13 | 15 => {
                    snapshot(
                        practice_window(app).window(),
                        checks.output.join(format!("answer_{phase}.png")),
                    );
                    match phase {
                        7 => starter_question(app, "programming_recall", 1),
                        9 => starter_question(app, "programming_recall", 2),
                        11 => starter_question(app, "math_recall", 1),
                        13 => starter_question(app, "chemistry_recall", 3),
                        15 => {
                            std::fs::write(checks.output.join("result.txt"),
                                "Fresh five-domain profile, concise prompts, pack browsing, minimum size, English words, Rust/C++ code preserved as text, calculus and thermodynamics typed answers passed.").unwrap();
                            checks.finished.set(true);
                            app.quit();
                            return;
                        }
                        _ => unreachable!(),
                    }
                }
                _ => unreachable!(),
            }
            starter_packs_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens the native window with the fresh cross-domain starter packs"]
fn native_starter_packs() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/starter_content_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = Store::open(profile.path().to_owned()).unwrap();
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.interface_language = InterfaceLanguage::Chinese;
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
    starter_packs_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(20), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Starter pack check failed or timed out"
    );
}

fn practice_text_phase(checks: Rc<AnswerChecks>, case: u8, stage: u8) {
    Timer::single_shot(Duration::from_millis(350), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            if stage == 0 {
                let (prompt, answer, knowledge) = match case {
                    0 => ("hesitate".into(), "犹豫".into(), false),
                    1 => ("第一行提示\n第二行提示\n第三行提示".into(), "test".into(), false),
                    2 => ("下面这段提示包含比较多的文字，用来检查练习卡片能否保留正常边距并完整显示内容，而不是把题面的开头和结尾裁掉。".into(), "test".into(), false),
                    3 => ("标准答案显示检查".into(), "标准答案需要保留完整内容和正常边距，换行后不能只显示中间几行，也不能与作答区域挤在一起。".repeat(2), false),
                    4 => ("边界检查：很长的题面仍能滚动查看完整内容。".repeat(24), "终点".into(), false),
                    5 => ("知识展示中的短提示也可以自然换行，不应与答案重叠。".into(), "明确答案需要完整显示，保留内容之间的间距。".repeat(3), true),
                    6 => ("Identify the standard smart pointer that owns its object exclusively and releases the object automatically when the owner leaves its scope. Keep the whole cue visible.".into(), "std::unique_ptr".into(), false),
                    7 => ("abcdefghijklmnopqrstuvwxyz".repeat(4), "test".into(), false),
                    _ => unreachable!(),
                };
                app.close_practice();
                app.set_knowledge_mode(knowledge);
                {
                    let mut library = app.library.borrow_mut();
                    let card = &mut library.packs[0].cards[0];
                    card.prompt = prompt;
                    card.answer = answer;
                    app.store.save(&library).unwrap();
                }
                app.show_practice().unwrap();
                practice_text_phase(checks.clone(), case, 1);
                return;
            }
            let window = practice_window(app);
            if stage == 1 {
                snapshot(
                    window.window(),
                    checks.output.join(format!("case_{case}.png")),
                );
                let size = window
                    .window()
                    .size()
                    .to_logical(window.window().scale_factor());
                println!(
                    "Text case {case}: height={} natural={} available={}",
                    size.height,
                    window.get_prompt_natural_height(),
                    window.get_prompt_area_height()
                );
                assert!(
                    window.get_prompt_area_height() + 0.5 >= window.get_prompt_natural_height(),
                    "Cue lines must fit without clipping their first or last row"
                );
                assert!(size.height >= 230.0 && size.height <= 561.0);
                if case == 0 {
                    assert!(
                        (window.get_formula_button_center_x() - 274.0).abs() <= 0.5,
                        "The answer area retains symmetric 24-pixel side padding"
                    );
                    assert!(
                        (size.height
                            - if window.get_pack_tag_visible() {
                                256.0
                            } else {
                                230.0
                            })
                        .abs()
                            <= 1.0,
                        "Short cues retain the compact card plus the optional tag header"
                    );
                }
                if case == 3 {
                    key(window.window(), "wrong".into());
                    key(window.window(), Key::Return.into());
                    assert_eq!(window.get_feedback(), 2);
                    practice_text_phase(checks.clone(), case, 2);
                    return;
                }
                if case == 4 {
                    assert!(window.get_content_height() > window.get_visible_content_height());
                    window
                        .window()
                        .dispatch_event(WindowEvent::PointerScrolled {
                            position: slint::LogicalPosition::new(160.0, 80.0),
                            delta_x: 0.0,
                            delta_y: -10000.0,
                        });
                    practice_text_phase(checks.clone(), case, 2);
                    return;
                }
            } else if case == 3 {
                snapshot(window.window(), checks.output.join("standard_answer.png"));
                println!(
                    "Standard answer: natural={} available={}",
                    window.get_answer_natural_height(),
                    window.get_answer_area_height()
                );
                assert!(
                    window.get_answer_area_height() + 0.5 >= window.get_answer_natural_height(),
                    "The full standard answer must fit after submitting a wrong answer"
                );
            } else {
                snapshot(window.window(), checks.output.join("scroll_end.png"));
                assert!(
                    window.get_content_scroll_y() < -100.0,
                    "Mouse wheel reveals the lower content"
                );
                assert!(
                    window.get_content_height() - window.get_visible_content_height()
                        + window.get_content_scroll_y()
                        <= 1.0,
                    "The last line remains reachable"
                );
            }
            if case == 7 {
                std::fs::write(checks.output.join("result.txt"),
                    "Short/multiline cues, long Chinese/English/identifiers, standard-answer feedback, knowledge display and bounded scrolling passed.").unwrap();
                checks.finished.set(true);
                app.quit();
            } else {
                practice_text_phase(checks.clone(), case + 1, 0);
            }
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens the native practice card to verify multiline content fits"]
fn native_practice_text_layout() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/practice_text_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = Store::open(profile.path().to_owned()).unwrap();
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.enabled_pack_ids = vec![library.packs[0].id.clone()];
    library.packs[0].cards.truncate(1);
    let card = &mut library.packs[0].cards[0];
    card.prompt_latex = None;
    card.answer_latex = None;
    card.aliases.clear();
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
    practice_text_phase(checks.clone(), 0, 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.finished.get(), "Native practice text layout failed");
}

fn answer_phase(checks: Rc<AnswerChecks>, phase: u8) {
    // Newly shown windows can still be in a DWM transition at 250 ms. Wait
    // before their first desktop comparison; keep repaint-message checks
    // synchronous so client redraws cannot conceal unwanted caption paint.
    let delay = if matches!(phase, 2 | 5) { 500 } else { 250 };
    Timer::single_shot(Duration::from_millis(delay), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            match phase {
                0 => {
                    let main = main_window(app);
                    assert_eq!(main.get_answer_mode_labels().row_count(), 3);
                    main.invoke_set_all_answer_modes(1);
                    assert_eq!(
                        app.store.load().unwrap().uniform_answer_mode("test_pack"),
                        Some(AnswerMode::MultipleChoice)
                    );
                }
                1 => {
                    let main = main_window(app);
                    assert_eq!(
                        main.get_uniform_answer_mode(),
                        1,
                        "The visible selector must keep the saved mode after model updates"
                    );
                    snapshot(main.window(), checks.output.join("main_modes.png"));
                    app.tray.invoke_practice_now();
                }
                2 => {
                    let window = practice_window(app);
                    check_floating_window(window.window());
                    check_no_caption_paint(window.window());
                    assert_eq!(window.get_answer_mode(), 1);
                    assert_eq!(window.get_options().row_count(), 3);
                    assert!((0..3).all(|index| {
                        window.get_options().row_data(index).unwrap().formula_width > 0.0
                    }));
                    snapshot(window.window(), checks.output.join("multiple_choice.png"));
                    let wrong = (0..3)
                        .find(|index| {
                            app.practice
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .prepared
                                .accepts_choice(*index)
                                == Some(false)
                        })
                        .unwrap();
                    click_option(&window, wrong);
                    assert_eq!(
                        window.get_feedback(),
                        2,
                        "Clicking a tile immediately judges it"
                    );
                    assert_eq!(window.get_selected_option(), wrong as i32);
                    click_option(&window, (wrong + 1) % 3);
                    assert_eq!(
                        window.get_selected_option(),
                        wrong as i32,
                        "Feedback locks the first choice"
                    );
                    let before: Vec<_> = (0..3)
                        .map(|index| window.get_options().row_data(index).unwrap().label)
                        .collect();
                    window.invoke_rasterize_formulas(window.get_pixel_scale());
                    let after: Vec<_> = (0..3)
                        .map(|index| window.get_options().row_data(index).unwrap().label)
                        .collect();
                    assert_eq!(before, after, "DPI rerasterization keeps option order");
                }
                3 => {
                    let window = practice_window(app);
                    snapshot(
                        window.window(),
                        checks.output.join("multiple_choice_wrong.png"),
                    );
                    assert!(!app.feedback_timer.running());
                    key(window.window(), Key::Escape.into());
                    assert!(app.practice.borrow().is_some());
                    key(window.window(), Key::Return.into());
                    assert!(app.practice.borrow().is_none());
                    app.show_practice().unwrap();
                }
                4 => {
                    let window = practice_window(app);
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
                    key(window.window(), (correct + 1).to_string().into());
                    assert_eq!(window.get_feedback(), 1, "Number keys select a choice");
                    snapshot(
                        window.window(),
                        checks.output.join("multiple_choice_correct.png"),
                    );
                    key(window.window(), Key::Return.into());
                    app.show_main().unwrap();
                    main_window(app).invoke_set_all_answer_modes(2);
                    assert_eq!(
                        app.store.load().unwrap().uniform_answer_mode("test_pack"),
                        Some(AnswerMode::TrueFalse)
                    );
                    app.tray.invoke_practice_now();
                }
                5 => {
                    let window = practice_window(app);
                    check_floating_window(window.window());
                    check_no_caption_paint(window.window());
                    assert_eq!(window.get_options().row_count(), 2);
                    assert_eq!(window.get_answer_mode(), 2);
                    let session = app.practice.borrow();
                    let statement = session
                        .as_ref()
                        .unwrap()
                        .question
                        .card
                        .true_false
                        .as_ref()
                        .unwrap();
                    assert_eq!(window.get_prompt().as_str(), statement.statement);
                    assert_eq!(
                        window.get_standard_answer().as_str(),
                        if statement.is_true { "✓" } else { "✕" }
                    );
                    let correct = usize::from(!statement.is_true);
                    drop(session);
                    snapshot(window.window(), checks.output.join("true_false.png"));
                    click_option(&window, correct);
                    assert_eq!(window.get_feedback(), 1);
                    snapshot(
                        window.window(),
                        checks.output.join("true_false_correct.png"),
                    );
                    key(window.window(), Key::Return.into());
                    app.show_practice().unwrap();
                }
                6 => {
                    let window = practice_window(app);
                    let wrong = (0..2)
                        .find(|index| {
                            app.practice
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .prepared
                                .accepts_choice(*index)
                                == Some(false)
                        })
                        .unwrap();
                    click_option(&window, wrong);
                    assert_eq!(window.get_feedback(), 2);
                    snapshot(window.window(), checks.output.join("true_false_wrong.png"));
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        answer_phase(checks, 7);
                    });
                    return;
                }
                7 => {
                    let window = practice_window(app);
                    assert_eq!(window.get_feedback(), 2);
                    assert!(!app.feedback_timer.running());
                    key(window.window(), Key::Escape.into());
                    assert!(app.practice.borrow().is_some());
                    key(window.window(), Key::Return.into());
                    assert!(app.practice.borrow().is_none());
                    app.show_practice().unwrap();
                    let window = practice_window(app);
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
                    click_option(&window, correct);
                    assert_eq!(window.get_feedback(), 1);
                    assert!(app.feedback_timer.running());
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        answer_phase(checks, 8);
                    });
                    return;
                }
                8 => {
                    assert!(
                        app.practice.borrow().is_none(),
                        "Correct feedback times out"
                    );
                    app.show_main().unwrap();
                    main_window(app).invoke_toggle_knowledge_mode();
                    app.show_practice().unwrap();
                    assert!(practice_window(app).get_knowledge_only());
                    assert!(app.feedback_timer.running());
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        answer_phase(checks, 9);
                    });
                    return;
                }
                9 => {
                    assert!(app.practice.borrow().is_none(), "Facts still time out");
                    app.show_main().unwrap();
                    let main = main_window(app);
                    main.invoke_toggle_knowledge_mode();
                    main.invoke_set_all_answer_modes(0);
                    app.show_practice().unwrap();
                }
                10 => {
                    let window = practice_window(app);
                    for character in "wrong".chars() {
                        key(window.window(), character.to_string().into());
                    }
                    key(window.window(), Key::Return.into());
                    assert_eq!(window.get_feedback(), 2);
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        answer_phase(checks, 11);
                    });
                    return;
                }
                11 => {
                    let window = practice_window(app);
                    assert_eq!(window.get_feedback(), 2);
                    assert!(!app.feedback_timer.running());
                    snapshot(window.window(), checks.output.join("typed_wrong.png"));
                    key(window.window(), Key::Escape.into());
                    assert!(app.practice.borrow().is_some());
                    key(window.window(), Key::Return.into());
                    assert!(app.practice.borrow().is_none());
                    app.show_practice().unwrap();
                }
                12 => {
                    let window = practice_window(app);
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
                    let checks = checks.clone();
                    Timer::single_shot(Duration::from_millis(2300), move || {
                        answer_phase(checks, 13);
                    });
                    return;
                }
                13 => {
                    assert!(
                        app.practice.borrow().is_none(),
                        "Correct typed answers time out"
                    );
                    std::fs::write(checks.output.join("result.txt"),
                        "Typed, multiple-choice and true/false wrong answers wait for Enter; Esc preserves wrong feedback; correct answers and facts retain automatic dismissal.").unwrap();
                    checks.finished.set(true);
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            answer_phase(checks.clone(), phase + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens the native card to check selection, formulas, feedback and mode persistence"]
fn native_answer_modes() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/answer_modes_check");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.feedback_seconds = 2;
    library.settings.knowledge_seconds = 2;
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
    answer_phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        checks.finished.get(),
        "Answer mode check failed or timed out"
    );
}

#[test]
#[ignore = "opens native cards to verify binary history and the knowledge-only bypass"]
fn native_adaptive_practice() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("local_artifacts/adaptive_practice_build/native");
    std::fs::create_dir_all(&output).unwrap();
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = open_regression_profile(profile.path());
    let state_path = profile.path().join("state.json");
    let learning_path = profile.path().join("learning_state.json");
    let original = std::fs::read(&state_path).unwrap();
    let mut library = store.load().unwrap();
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.remember_position = false;
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
    let finished = Rc::new(Cell::new(false));
    let completed = finished.clone();
    let active = app.clone();
    Timer::single_shot(Duration::from_millis(400), move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert!(!active.learner.borrow().is_loaded());
            main_window(&active).invoke_toggle_knowledge_mode();
            active.show_practice().unwrap();
            assert!(practice_window(&active).get_knowledge_only());
            assert!(!active.learner.borrow().is_loaded());
            assert!(!learning_path.exists());
            practice_window(&active).invoke_dismiss();
            assert!(!learning_path.exists());

            active.show_main().unwrap();
            main_window(&active).invoke_toggle_knowledge_mode();
            active.show_practice().unwrap();
            let window = practice_window(&active);
            assert!(!window.get_knowledge_only());
            assert!(active.learner.borrow().is_loaded());
            window.invoke_submit_answer("".into());
            window.invoke_submit_answer("sqrt(".into());
            assert!(!learning_path.exists());
            let answer = active
                .practice
                .borrow()
                .as_ref()
                .unwrap()
                .question
                .card
                .answer
                .clone();
            window.set_answer_text(answer.clone().into());
            window.invoke_submit_answer(answer.into());
            let saved = std::fs::read(&learning_path).unwrap();
            let document: serde_json::Value = serde_json::from_slice(&saved).unwrap();
            assert_eq!(document["reviews"].as_array().unwrap().len(), 1);
            assert_eq!(document["reviews"][0]["correct"], true);
            window.invoke_submit_answer("wrong".into());
            assert_eq!(saved, std::fs::read(&learning_path).unwrap());
            assert!(!active.reminder_timer.running());
            active.close_practice();

            active.show_practice().unwrap();
            practice_window(&active).invoke_dismiss();
            assert_eq!(saved, std::fs::read(&learning_path).unwrap());
            active.show_main().unwrap();
            main_window(&active).invoke_toggle_knowledge_mode();
            active.show_practice().unwrap();
            assert!(practice_window(&active).get_knowledge_only());
            practice_window(&active).invoke_dismiss();
            assert_eq!(saved, std::fs::read(&learning_path).unwrap());
            assert_eq!(original, std::fs::read(state_path).unwrap());
            completed.set(true);
            println!(
                "Knowledge-only bypass, binary history, incomplete input, duplicate submit and dismissal passed."
            );
        }));
        if result.is_err() {
            eprintln!("Native adaptive-practice assertions failed.");
        }
        active.quit();
    });
    let watchdog = app.clone();
    Timer::single_shot(Duration::from_secs(15), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(
        finished.get(),
        "Native adaptive-practice check failed or timed out"
    );
}
