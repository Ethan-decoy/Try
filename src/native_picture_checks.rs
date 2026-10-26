//! Explicit desktop checks use only generated pictures and a temporary profile.
use super::native_checks::{main_window, practice_window, snapshot};
use super::*;
use slint::winit_030::winit::event_loop::EventLoop;
use slint::winit_030::winit::platform::windows::EventLoopBuilderExtWindows;
use std::path::PathBuf;

struct PictureChecks {
    app: Rc<App>,
    output: PathBuf,
    source: PathBuf,
    original: String,
    imported: r#try::media::NormalizedImage,
    captioned_height: Cell<f32>,
    captioned_knowledge_height: Cell<f32>,
    finished: Cell<bool>,
}

fn phase(checks: Rc<PictureChecks>, step: u8) {
    Timer::single_shot(Duration::from_millis(220), move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let app = &checks.app;
            if (step == 1 || step == 2) && app.image_task.borrow().is_some() {
                phase(checks.clone(), step);
                return;
            }
            match step {
                0 => {
                    app.open_editor(Some(0), false, None);
                    app.import_question_image(checks.source.clone());
                }
                1 => {
                    assert_eq!(
                        app.editor_image.borrow().as_deref(),
                        Some(checks.imported.id.as_str())
                    );
                    assert!(main_window(app).get_editor_picture_width() > 0.0);
                    app.close_editor();
                    assert_eq!(
                        app.library.borrow().packs[0].cards[0]
                            .prompt_image
                            .as_deref(),
                        Some(checks.original.as_str())
                    );
                    assert!(
                        app.store.media().read(&checks.imported.id).is_err(),
                        "Discarded draft image is collected"
                    );
                    app.open_editor(Some(0), false, None);
                    app.import_question_image(checks.source.clone());
                }
                2 => {
                    assert!(
                        main_window(app).get_editor_picture_view_height() > 1.0,
                        "Scroll layout retains a visible image preview"
                    );
                    snapshot(main_window(app).window(), checks.output.join("editor.png"));
                    app.save_editor();
                    assert!(
                        app.editor.borrow().is_none(),
                        "Image draft saves successfully"
                    );
                    assert_eq!(
                        app.library.borrow().packs[0].cards[0]
                            .prompt_image
                            .as_deref(),
                        Some(checks.imported.id.as_str())
                    );
                    assert!(
                        app.store.media().read(&checks.original).is_err(),
                        "Replaced image is collected"
                    );
                    std::fs::remove_file(&checks.source).unwrap();
                    assert_eq!(app.store.load().unwrap().packs, app.library.borrow().packs);
                    app.show_practice().unwrap();
                    let window = practice_window(app);
                    assert_eq!(
                        window.window().size().width,
                        (320.0 * window.window().scale_factor()).round() as u32
                    );
                    assert!(window.get_question_image_width() > 0.0);
                    assert!(
                        window.get_visible_content_height() + 1.0 >= window.get_content_height(),
                        "Compact image card fits its content"
                    );
                }
                3 => {
                    let window = practice_window(app);
                    checks
                        .captioned_height
                        .set(window.window().size().height as f32 / window.window().scale_factor());
                    snapshot(
                        practice_window(app).window(),
                        checks.output.join("practice_dark.png"),
                    );
                    let answer = app
                        .practice
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .question
                        .card
                        .answer
                        .clone();
                    practice_window(app).set_answer_text(answer.clone().into());
                    app.submit_answer(&answer);
                    assert_eq!(practice_window(app).get_feedback(), 1);
                    practice_window(app).invoke_preview_question_image();
                    assert!(app.image_preview.borrow().is_some());
                }
                4 => {
                    snapshot(
                        app.image_preview.borrow().as_ref().unwrap().window(),
                        checks.output.join("zoom.png"),
                    );
                    app.close_image_preview();
                    app.close_practice();
                    app.commit(|library| {
                        library.settings.appearance = Appearance::Light;
                        Ok(())
                    })
                    .unwrap();
                    app.knowledge_only.set(true);
                    app.show_practice().unwrap();
                    assert!(practice_window(app).get_knowledge_only());
                    assert!(practice_window(app).get_question_image_width() > 0.0);
                }
                5 => {
                    let window = practice_window(app);
                    checks
                        .captioned_knowledge_height
                        .set(window.window().size().height as f32 / window.window().scale_factor());
                    snapshot(
                        practice_window(app).window(),
                        checks.output.join("knowledge_light.png"),
                    );
                    app.close_practice();
                    app.knowledge_only.set(false);
                    app.commit(|library| {
                        let pack = &library.packs[0];
                        let (pack_id, card_id) =
                            (pack.id.clone(), pack.cards[0].id.clone().unwrap());
                        library.set_card_answer_mode(&pack_id, &card_id, AnswerMode::MultipleChoice)
                    })
                    .unwrap();
                    app.show_practice().unwrap();
                }
                6 => {
                    let window = practice_window(app);
                    assert_eq!(window.get_answer_mode(), AnswerMode::MultipleChoice.index());
                    assert!(window.get_question_image_width() > 0.0);
                    assert!(
                        window.get_visible_content_height() + 1.0 >= window.get_content_height()
                    );
                    snapshot(window.window(), checks.output.join("choice_light.png"));
                    let correct = (0..3)
                        .find(|&i| {
                            app.practice
                                .borrow()
                                .as_ref()
                                .unwrap()
                                .prepared
                                .accepts_choice(i)
                                == Some(true)
                        })
                        .unwrap();
                    window.invoke_choose_answer(correct as i32);
                    assert_eq!(window.get_feedback(), 1);
                    app.close_practice();
                    app.commit(|library| {
                        let pack = &library.packs[0];
                        let (pack_id, card_id) =
                            (pack.id.clone(), pack.cards[0].id.clone().unwrap());
                        library.set_card_answer_mode(&pack_id, &card_id, AnswerMode::TrueFalse)
                    })
                    .unwrap();
                    app.show_practice().unwrap();
                }
                7 => {
                    let window = practice_window(app);
                    assert_eq!(window.get_answer_mode(), AnswerMode::TrueFalse.index());
                    assert!(window.get_question_image_width() > 0.0);
                    snapshot(window.window(), checks.output.join("judgment_light.png"));
                    window.invoke_choose_answer(0);
                    assert_eq!(window.get_feedback(), 1);
                    app.close_practice();
                    app.show_main().unwrap();
                    app.commit(|library| {
                        let pack = &library.packs[0];
                        let (pack_id, card_id) =
                            (pack.id.clone(), pack.cards[0].id.clone().unwrap());
                        library.set_card_answer_mode(&pack_id, &card_id, AnswerMode::Input)
                    })
                    .unwrap();
                    app.open_editor(Some(0), false, None);
                    let main = main_window(app);
                    main.invoke_activate_content(0);
                    main.set_math_input("".into());
                    main.invoke_edit_math_input("".into());
                    assert!(app.flush_math(), "{}", main.get_math_error());
                    app.save_editor();
                    assert!(
                        app.editor.borrow().is_none(),
                        "Cleared image prompt saves through the real field editor"
                    );
                    assert!(app.library.borrow().packs[0].cards[0].prompt.is_empty());
                    assert_eq!(app.store.load().unwrap().packs, app.library.borrow().packs);
                    assert_eq!(
                        main_window(app).get_cards().row_data(0).unwrap().prompt,
                        tr!("图片题 {p0}", p0 = 1)
                    );
                    snapshot(
                        main_window(app).window(),
                        checks.output.join("image_only_list.png"),
                    );
                    app.show_practice().unwrap();
                }
                8 => {
                    let window = practice_window(app);
                    assert!(!window.get_has_prompt());
                    assert!(window.get_prompt_area_height() <= 0.5);
                    assert_eq!(window.get_prompt_natural_height(), 0.0);
                    let height =
                        window.window().size().height as f32 / window.window().scale_factor();
                    assert!(
                        height + 50.0 < checks.captioned_height.get(),
                        "Pure image card removes the caption's height and gap"
                    );
                    assert!(
                        window.get_visible_content_height() + 1.0 >= window.get_content_height()
                    );
                    snapshot(
                        window.window(),
                        checks.output.join("image_only_input_light.png"),
                    );
                    window.set_answer_text("wrong".into());
                    app.submit_answer("wrong");
                    assert_eq!(window.get_feedback(), 2);
                }
                9 => {
                    snapshot(
                        practice_window(app).window(),
                        checks.output.join("image_only_wrong_answer.png"),
                    );
                    app.close_practice();
                    app.knowledge_only.set(true);
                    app.show_practice().unwrap();
                }
                10 => {
                    let window = practice_window(app);
                    assert!(!window.get_has_prompt());
                    assert!(window.get_prompt_area_height() <= 0.5);
                    let height =
                        window.window().size().height as f32 / window.window().scale_factor();
                    assert!(height + 50.0 < checks.captioned_knowledge_height.get());
                    assert!(
                        window.get_visible_content_height() + 1.0 >= window.get_content_height()
                    );
                    snapshot(
                        window.window(),
                        checks.output.join("image_only_knowledge_light.png"),
                    );
                    app.close_practice();
                    app.knowledge_only.set(false);
                    app.commit(|library| {
                        let pack = &library.packs[0];
                        let (pack_id, card_id) =
                            (pack.id.clone(), pack.cards[0].id.clone().unwrap());
                        library.set_card_answer_mode(&pack_id, &card_id, AnswerMode::MultipleChoice)
                    })
                    .unwrap();
                    app.show_practice().unwrap();
                }
                11 => {
                    let window = practice_window(app);
                    assert!(!window.get_has_prompt());
                    assert!(window.get_prompt_area_height() <= 0.5);
                    snapshot(
                        window.window(),
                        checks.output.join("image_only_choice_light.png"),
                    );
                    app.close_practice();
                    app.show_main().unwrap();
                }
                12 => {
                    app.open_editor(Some(0), false, None);
                    app.set_editor_image(None).unwrap();
                    app.close_editor();
                    assert!(
                        app.store.media().read(&checks.imported.id).is_ok(),
                        "Cancel removal preserves committed image"
                    );
                    app.open_editor(Some(0), false, None);
                    app.set_editor_image(None).unwrap();
                    app.save_editor();
                    assert!(
                        app.editor.borrow().is_some(),
                        "Removing the only cue fails validation"
                    );
                    assert!(
                        app.library.borrow().packs[0].cards[0]
                            .prompt_image
                            .is_some()
                    );
                    assert!(!main_window(app).get_editor_error().is_empty());
                    main_window(app).invoke_activate_content(0);
                    main_window(app).set_math_input("图中是哪个函数？".into());
                    main_window(app).invoke_edit_math_input("图中是哪个函数？".into());
                    assert!(app.flush_math());
                    app.save_editor();
                    assert!(
                        app.library.borrow().packs[0].cards[0]
                            .prompt_image
                            .is_none()
                    );
                    assert!(app.store.media().read(&checks.imported.id).is_err());
                    app.store
                        .media()
                        .install(&checks.imported.id, &checks.imported.png)
                        .unwrap();
                    app.commit(|library| {
                        library.packs[0].cards[0].prompt_image = Some(checks.imported.id.clone());
                        Ok(())
                    })
                    .unwrap();
                    let before =
                        std::fs::read(app.store.directory().join("learning_state.json")).ok();
                    std::fs::remove_file(
                        app.store
                            .directory()
                            .join("media")
                            .join(format!("{}.png", checks.imported.id)),
                    )
                    .unwrap();
                    app.knowledge_only.set(false);
                    assert!(
                        app.show_practice().is_err(),
                        "Missing image does not create a normal answer session"
                    );
                    assert!(app.practice.borrow().is_none());
                    assert_eq!(
                        std::fs::read(app.store.directory().join("learning_state.json")).ok(),
                        before
                    );
                    std::fs::write(checks.output.join("passed.txt"), "native_picture_questions: image-only and captioned editor, practice, knowledge, choice, judgment and media lifecycle checks passed\n").unwrap();
                    checks.finished.set(true);
                    app.quit();
                    return;
                }
                _ => unreachable!(),
            }
            phase(checks.clone(), step + 1);
        }));
        if outcome.is_err() {
            checks.app.quit();
        }
    });
}

#[test]
#[ignore = "opens isolated native editor, image cards and image preview"]
fn native_picture_questions() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local_artifacts/image_question_check");
    std::fs::create_dir_all(&output).unwrap();
    let _ = std::fs::remove_file(output.join("passed.txt"));
    let profile = tempfile::tempdir_in(&output).unwrap();
    let store = Store::open(profile.path().join("data")).unwrap();
    let source = profile.path().join("source.png");
    image::DynamicImage::new_rgb8(30, 20).save(&source).unwrap();
    let original = r#try::media::normalize_file(&source).unwrap();
    store.media().install(&original.id, &original.png).unwrap();
    let mut pixels = image::RgbaImage::from_pixel(800, 400, image::Rgba([255, 255, 255, 255]));
    for x in 24..776 {
        pixels.put_pixel(x, 200, image::Rgba([180, 180, 180, 255]));
        let y = (200.0 - 120.0 * ((x as f64 - 400.0) / 60.0).sin()).round() as u32;
        for dy in 0..3 {
            pixels.put_pixel(x, y + dy, image::Rgba([24, 24, 24, 255]));
        }
    }
    for y in 20..380 {
        pixels.put_pixel(400, y, image::Rgba([180, 180, 180, 255]));
    }
    pixels.save(&source).unwrap();
    let imported = r#try::media::normalize_file(&source).unwrap();
    let mut library = r#try::test_fixtures::regression_library();
    library.settings.appearance = Appearance::Dark;
    library.settings.auto_check_updates = false;
    library.settings.pause = PracticePause::UntilResumed;
    library.settings.feedback_seconds = 30;
    library.settings.knowledge_seconds = 30;
    library.settings.enabled_pack_ids = vec![library.packs[0].id.clone()];
    library.packs[0].cards.truncate(1);
    library.packs[0].cards[0].prompt = "图中是哪个函数？".into();
    library.packs[0].cards[0].prompt_latex = None;
    library.packs[0].cards[0].answer = "sin(x)".into();
    library.packs[0].cards[0].answer_latex = None;
    library.packs[0].cards[0].answer_latex = None;
    library.packs[0].cards[0].aliases = vec!["sinx".into()];
    library.packs[0].cards[0].distractors = vec![
        r#try::knowledge::AnswerContent {
            text: "cos(x)".into(),
            latex: None,
        },
        r#try::knowledge::AnswerContent {
            text: "x^2".into(),
            latex: None,
        },
    ];
    library.packs[0].cards[0].true_false = Some(r#try::knowledge::TrueFalse {
        statement: "图中是 sin(x)。".into(),
        statement_latex: None,
        is_true: true,
        correction: "✓".into(),
        correction_latex: None,
    });
    library.packs[0].cards[0].prompt_image = Some(original.id.clone());
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
    let checks = Rc::new(PictureChecks {
        app,
        output,
        source,
        original: original.id,
        imported,
        captioned_height: Cell::new(0.0),
        captioned_knowledge_height: Cell::new(0.0),
        finished: Cell::new(false),
    });
    phase(checks.clone(), 0);
    let watchdog = checks.app.clone();
    Timer::single_shot(Duration::from_secs(30), move || watchdog.quit());
    slint::run_event_loop_until_quit().unwrap();
    assert!(checks.finished.get(), "Native picture checks failed");
}
