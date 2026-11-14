//! Card context actions exercised without native windows or input.
mod support;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, PhysicalSize, Rgb8Pixel, SharedPixelBuffer};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use support::{click, key, paint, software_window};

slint::include_modules!();

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

fn right_click(
    card: &PracticeWindow,
    adapter: &slint::platform::software_renderer::MinimalSoftwareWindow,
    x: f32,
    y: f32,
) {
    let position = LogicalPosition::new(x, y);
    for event in [
        WindowEvent::PointerMoved { position },
        WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Right,
        },
        WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Right,
        },
    ] {
        card.window().dispatch_event(event);
    }
    paint(adapter);
}

#[test]
fn right_click_menu_routes_commands_once_and_escape_keeps_the_card() {
    let adapter = software_window();
    let card = PracticeWindow::new().unwrap();
    card.set_prompt("Recall".into());
    card.set_standard_answer("Answer".into());
    card.set_answer_text("draft".into());
    let actions = Rc::new(RefCell::new(Vec::new()));
    let received = actions.clone();
    card.on_practice_action(move |action, minutes| received.borrow_mut().push((action, minutes)));
    let dismissals = Rc::new(Cell::new(0));
    let count = dismissals.clone();
    card.on_dismiss(move || count.set(count.get() + 1));
    let submissions = Rc::new(Cell::new(0));
    let count = submissions.clone();
    card.on_submit_answer(move |_| count.set(count.get() + 1));
    let menu_states = Rc::new(RefCell::new(Vec::new()));
    let received = menu_states.clone();
    card.on_context_menu_changed(move |opened| received.borrow_mut().push(opened));
    card.show().unwrap();
    adapter.set_size(PhysicalSize::new(320, 230));
    let before_menu = paint(&adapter);

    right_click(&card, &adapter, 8.0, 8.0);
    assert!(
        card.get_context_menu_open(),
        "A right click on the card opens its menu"
    );
    let main_menu = paint(&adapter);
    let border_x = (card.get_context_menu_left() + card.get_context_menu_width() / 2.0) as usize;
    let border_y = card.get_context_menu_top() as usize;
    let border_pixel = border_y * main_menu.width() as usize + border_x;
    assert_ne!(
        main_menu.as_slice()[border_pixel],
        before_menu.as_slice()[border_pixel],
        "The rendered popup starts at the reported anchor, including the window edge inset"
    );
    save_preview("practice_menu_dark", &main_menu);
    key(&card, &adapter, Key::Escape);
    assert!(!card.get_context_menu_open());
    assert_eq!(card.get_answer_text(), "draft");
    assert_eq!(dismissals.get(), 0);
    assert_eq!(submissions.get(), 0);
    assert!(menu_states.borrow().starts_with(&[true, false]));

    card.invoke_focus_answer();
    key(&card, &adapter, Key::Menu);
    assert!(card.get_context_menu_open());
    key(&card, &adapter, Key::Return);
    assert_eq!(&*actions.borrow(), &[(PracticeAction::ExcludeQuestion, 0)]);

    card.invoke_open_context_menu(312.0, 222.0);
    paint(&adapter);
    click(
        &card,
        &adapter,
        card.get_context_menu_left() + 20.0,
        card.get_context_menu_top() + 53.0,
    );
    save_preview("practice_pause_menu_dark", &paint(&adapter));
    assert!(card.get_context_menu_height() > 150.0);
    assert!(card.get_context_menu_left() >= 0.0 && card.get_context_menu_top() >= 0.0);
    assert!(card.get_context_menu_left() + card.get_context_menu_width() <= 320.0);
    assert!(
        card.get_context_menu_top() + card.get_context_menu_height()
            <= card.window().size().height as f32
    );
    key(&card, &adapter, Key::DownArrow);
    key(&card, &adapter, Key::Return);
    assert_eq!(
        &*actions.borrow(),
        &[
            (PracticeAction::ExcludeQuestion, 0),
            (PracticeAction::Pause, 60)
        ]
    );

    card.invoke_open_context_menu(312.0, 222.0);
    paint(&adapter);
    key(&card, &adapter, Key::DownArrow);
    key(&card, &adapter, Key::Return);
    key(&card, &adapter, Key::End);
    key(&card, &adapter, Key::Return);
    assert_eq!(
        actions.borrow().last(),
        Some(&(PracticeAction::CustomPause, 0))
    );

    card.set_feedback(2);
    card.invoke_open_context_menu(300.0, 220.0);
    paint(&adapter);
    key(&card, &adapter, Key::Escape);
    assert_eq!(
        dismissals.get(),
        0,
        "Closing the menu does not acknowledge a wrong answer"
    );
    card.invoke_open_context_menu(300.0, 220.0);
    paint(&adapter);
    click(
        &card,
        &adapter,
        card.get_context_menu_left() + 20.0,
        card.get_context_menu_top() + 85.0,
    );
    assert_eq!(
        &*actions.borrow(),
        &[
            (PracticeAction::ExcludeQuestion, 0),
            (PracticeAction::Pause, 60),
            (PracticeAction::CustomPause, 0),
            (PracticeAction::RecordProblem, 0)
        ]
    );
    assert!(
        card.get_context_menu_open(),
        "The save result remains visible in the menu"
    );
    card.set_problem_record_error("disk unavailable".into());
    card.set_problem_record_status(ProblemRecordStatus::Failed);
    key(&card, &adapter, Key::End);
    key(&card, &adapter, Key::Return);
    assert_eq!(
        actions.borrow().len(),
        5,
        "A failed local save can be retried"
    );
    card.set_problem_record_status(ProblemRecordStatus::Saved);
    save_preview("practice_problem_recorded_dark", &paint(&adapter));
    key(&card, &adapter, Key::Return);
    assert_eq!(
        actions.borrow().len(),
        5,
        "A completed save ignores repeated activation"
    );
    key(&card, &adapter, Key::Escape);
    assert_eq!(
        dismissals.get(),
        0,
        "Recording a problem does not acknowledge incorrect feedback"
    );
    assert_eq!(
        submissions.get(),
        0,
        "Management commands are not answer submissions"
    );

    card.set_knowledge_only(true);
    card.set_feedback(0);
    card.set_dark_theme(false);
    card.invoke_open_context_menu(300.0, 220.0);
    paint(&adapter);
    assert_eq!(card.get_problem_record_status(), ProblemRecordStatus::Idle);
    assert!(card.get_context_menu_open());
    save_preview("practice_menu_light", &paint(&adapter));
    key(&card, &adapter, Key::Escape);
    assert_eq!(dismissals.get(), 0);
    card.invoke_open_context_menu(300.0, 220.0);
    paint(&adapter);
    key(&card, &adapter, Key::Tab);
    key(&card, &adapter, Key::Return);
    assert!(card.get_context_menu_open());
    assert!(card.get_context_menu_height() > 150.0);
    key(&card, &adapter, Key::Escape);
    assert_eq!(actions.borrow().len(), 5);
    card.hide().unwrap();
}

#[test]
fn excluding_a_card_persists_and_keeps_content_and_other_participation() {
    use r#try::{
        knowledge::AnswerMode,
        learning::{Learner, Origin},
        learning_store::LearningStore,
        practice,
        storage::Store,
    };
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path().join("data")).unwrap();
    let mut library = store.load().unwrap();
    let mut learner = Learner::new(&store);
    let now = chrono::Utc::now().timestamp();
    let question = learner
        .choose(&library, None, now, &mut rand::rng())
        .unwrap()
        .unwrap();
    let target = question.key.card_id.clone();
    let pack_id = question.key.pack_id.clone();
    let token = learner
        .presented(&question, AnswerMode::Input, now, Origin::Manual)
        .unwrap();
    learner.answer(&library, &token, true, now + 1).unwrap();
    let learning_store = LearningStore::new(&store);
    let progress = learning_store.load().unwrap();
    let original_content = serde_json::to_value(&library.packs).unwrap();
    let count = library.enabled_card_count();
    library.set_card_enabled(&pack_id, &target, false).unwrap();
    store.save(&library).unwrap();
    learner.reconcile(&library).unwrap();
    assert_eq!(learning_store.load().unwrap(), progress);
    let mut restored = store.load().unwrap();
    assert_eq!(
        serde_json::to_value(&restored.packs).unwrap(),
        original_content
    );
    assert_eq!(restored.enabled_card_count(), count - 1);
    assert!(
        !restored
            .participating_cards()
            .any(|(pack, card)| pack.id == pack_id && card.id.as_deref() == Some(&target))
    );
    for _ in 0..30 {
        let question = practice::select_question(&restored, None, &mut rand::rng()).unwrap();
        assert!(question.key.pack_id != pack_id || question.key.card_id != target);
    }
    restored.set_card_enabled(&pack_id, &target, true).unwrap();
    store.save(&restored).unwrap();
    assert_eq!(store.load().unwrap().enabled_card_count(), count);
}
