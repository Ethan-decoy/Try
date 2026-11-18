use super::*;

// App constructs a native tray. These callback tests run with a software window
// and a temporary profile on Windows, without opening or activating a window.
fn open_profile(library: Library) -> (tempfile::TempDir, Rc<App>, MainWindow) {
    crate::test_support::software_window();
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path().to_owned()).unwrap();
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let app = App::new(store, library, floating).unwrap();
    let window = MainWindow::new().unwrap();
    app.main_window.replace(Some(window.clone_strong()));
    bindings::connect(&app, &window);
    app.refresh();
    (directory, app, window)
}

#[test]
fn choice_answer_aliases_after_a_failed_save_match_a_direct_save() {
    let mut library = Library::default();
    let cards = ["direct", "retry"].map(|id| {
        serde_json::json!({
            "id": id, "prompt": "Color?", "answer": "red", "aliases": ["crimson"],
            "distractors": [{"text": "green"}, {"text": "yellow"}]
        })
    });
    let pack = KnowledgePack::from_json(
        &serde_json::to_vec(&serde_json::json!({
            "format": "try", "schema_version": 1, "id": "colors", "name": "Colors", "cards": cards
        }))
        .unwrap(),
    )
    .unwrap();
    library.import(pack, false).unwrap();
    let pack_index = library.packs.len() - 1;
    let (_directory, app, window) = open_profile(library);
    window.invoke_select_pack(pack_index as i32);
    let edit_field = |index, text: &str| {
        window.invoke_activate_content(index);
        window.set_math_input(text.into());
        window.invoke_edit_math_input(text.into());
    };
    for row in [0, 1] {
        window.invoke_edit_card(row);
        window.invoke_set_editor_answer_mode(AnswerMode::MultipleChoice.index());
        edit_field(1, "blue");
        if row == 1 {
            edit_field(2, "yellow");
            window.invoke_save_editor();
            assert!(window.get_editor_visible());
            assert!(!window.get_editor_error().is_empty());
            let unchanged = &app.store.load().unwrap().packs[pack_index].cards[1];
            assert_eq!(unchanged.answer, "red");
            assert!(unchanged.accepts("crimson"));
            edit_field(2, "green");
        }
        window.invoke_save_editor();
        assert!(
            !window.get_editor_visible(),
            "{}",
            window.get_editor_error()
        );
        let saved = &app.store.load().unwrap().packs[pack_index].cards[row as usize];
        assert_eq!(saved.answer, "blue");
        assert!(saved.accepts("blue"));
        assert!(!saved.accepts("crimson"), "stale alias for row {row}");
    }
}

#[test]
fn practice_time_changes_preserve_quiet_hours_edits_and_reject_invalid_values() {
    let (_directory, app, window) = open_profile(Library::default());
    let original = app.store.load().unwrap();
    window.set_quiet_start("22:4".into());
    window.invoke_set_quiet_range(window.get_quiet_start(), window.get_quiet_end());
    assert!(!window.get_quiet_time_error().is_empty());
    window.invoke_set_feedback(4);
    window.invoke_set_knowledge_duration(17);
    window.invoke_set_interval(42);
    assert_eq!(window.get_interval_minutes(), 42);
    assert_eq!(window.get_feedback_seconds(), 4);
    assert_eq!(window.get_knowledge_seconds(), 17);
    assert_eq!(window.get_quiet_start(), "22:4");
    assert!(!window.get_quiet_time_error().is_empty());
    let mut expected = original;
    expected.settings.interval_minutes = 42;
    expected.settings.feedback_seconds = 4;
    expected.settings.knowledge_seconds = 17;
    assert_eq!(app.store.load().unwrap(), expected);

    window.invoke_set_feedback(61);
    assert!(window.get_message_is_error());
    assert_eq!(window.get_feedback_seconds(), 4);
    assert_eq!(window.get_quiet_start(), "22:4");
    assert_eq!(*app.library.borrow(), expected);
    assert_eq!(app.store.load().unwrap(), expected);

    app.close_main();
    app.reminder_timer.stop();
    app.set_feedback(5);
    app.set_knowledge_duration(18);
    assert!(!app.reminder_timer.running());
    app.set_interval(43);
    assert!(app.reminder_timer.running());
}
