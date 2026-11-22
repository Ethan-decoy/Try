use super::*;
use slint::platform::software_renderer::MinimalSoftwareWindow;

// App constructs a native tray. These callback tests run with a software window
// and a temporary profile on Windows, without opening or activating a window.
fn open_profile(
    library: Library,
) -> (
    tempfile::TempDir,
    Rc<App>,
    MainWindow,
    Rc<MinimalSoftwareWindow>,
) {
    let adapter = crate::test_support::software_window();
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path().to_owned()).unwrap();
    store.save(&library).unwrap();
    let floating = Rc::new(FloatingWindow::new(&library.settings));
    let app = App::new(store, library, floating).unwrap();
    let window = MainWindow::new().unwrap();
    app.main_window.replace(Some(window.clone_strong()));
    bindings::connect(&app, &window);
    app.refresh();
    (directory, app, window, adapter)
}

#[test]
fn leaving_protects_the_draft_and_waits_for_a_successful_save() {
    use crate::test_support::{key, paint};
    use slint::platform::Key;

    let (_directory, app, window, adapter) = open_profile(Library::default());
    adapter.set_size(slint::PhysicalSize::new(880, 640));
    window.show().unwrap();
    let before = app.store.load().unwrap();
    window.invoke_create_pack();
    window.set_editor_pack_name("New draft".into());
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    paint(&adapter);
    assert!(window.get_leave_pending());

    // Enter on the focused default action keeps the complete draft open.
    key(&window, &adapter, Key::Return);
    assert!(!window.get_leave_pending());
    assert!(window.get_editor_visible());
    assert_eq!(window.get_editor_pack_name(), "New draft");
    key(&window, &adapter, Key::Return);
    assert!(window.get_editor_visible());
    assert_eq!(app.store.load().unwrap(), before);

    window.invoke_quit_app();
    assert!(matches!(app.pending_leave.get(), Some(LeaveAction::Quit)));
    key(&window, &adapter, Key::Escape);
    assert!(app.pending_leave.get().is_none());
    window.invoke_go_background();
    window.invoke_save_and_leave();
    assert!(app.main_window.borrow().is_some());
    assert!(window.get_editor_visible());
    assert!(!window.get_editor_error().is_empty());
    assert_eq!(app.store.load().unwrap(), before);

    for (index, text) in [(0, "Name the colour."), (1, "blue")] {
        window.invoke_activate_content(index);
        window.set_math_input(text.into());
        window.invoke_edit_math_input(text.into());
    }
    window.invoke_go_background();
    window.invoke_save_and_leave();
    assert!(app.main_window.borrow().is_none());
    let saved = app.store.load().unwrap();
    let pack = saved.packs.last().unwrap();
    assert_eq!(pack.name, "New draft");
    assert_eq!(pack.cards[0].answer, "blue");
}

#[test]
fn leaving_distinguishes_unchanged_content_from_metadata_drafts() {
    let (_directory, app, window, _) = open_profile(Library::default());
    let before = app.store.load().unwrap();
    window.invoke_begin_rename();
    window.set_name_draft("Unsaved pack name".into());
    window.invoke_go_background();
    assert!(window.get_leave_pending());
    window.invoke_discard_and_leave();
    assert!(app.main_window.borrow().is_none());
    assert_eq!(app.store.load().unwrap(), before);

    app.main_window.replace(Some(window.clone_strong()));
    window.invoke_reset_overview();
    window.invoke_edit_card(0);
    window.invoke_go_background();
    assert!(app.main_window.borrow().is_none());
    assert!(!window.get_leave_pending());
    assert_eq!(app.store.load().unwrap(), before);
}

fn await_pack_transfer(app: &Rc<App>, window: &MainWindow) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while window.get_pack_transfer_busy() {
        app.poll_pack_transfer();
        assert!(
            std::time::Instant::now() < deadline,
            "file transfer timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn pack_import_retries_and_keeps_a_failed_replacement_available() {
    use crate::app::pack_transfer::TransferRequest;
    use std::fs;

    let (directory, app, window, _) = open_profile(Library::default());
    let before = app.store.load().unwrap();
    let path = directory.path().join("import.json");
    app.start_pack_transfer(TransferRequest::Import {
        path: path.clone(),
        animated: false,
    });
    assert!(window.get_pack_transfer_busy());
    window.invoke_go_background();
    assert!(app.main_window.borrow().is_some());
    await_pack_transfer(&app, &window);
    assert!(window.get_pack_transfer_failed());
    assert_eq!(app.store.load().unwrap(), before);

    let mut replacement = before.packs[0].clone();
    replacement.name = "Replacement queued for review".to_owned();
    Store::export_pack(&path, &replacement).unwrap();
    window.invoke_retry_pack_transfer();
    await_pack_transfer(&app, &window);
    assert!(!window.get_pack_transfer_failed());
    assert!(window.get_import_pending());
    assert_eq!(app.store.load().unwrap(), before);

    let state = directory.path().join("state.json");
    let backup = directory.path().join("state_backup.json");
    fs::rename(&state, &backup).unwrap();
    fs::create_dir(&state).unwrap();
    window.invoke_confirm_import();
    assert!(window.get_import_pending());
    assert!(window.get_message_is_error());
    assert_eq!(*app.library.borrow(), before);
    fs::remove_dir(&state).unwrap();
    fs::rename(&backup, &state).unwrap();
    window.invoke_confirm_import();
    assert!(!window.get_import_pending());
    assert_eq!(app.store.load().unwrap().packs[0].name, replacement.name);
    assert_eq!(window.get_selected_name().as_str(), replacement.name);
}

#[test]
fn pack_export_retries_the_same_snapshot_at_the_selected_path() {
    use crate::app::pack_transfer::TransferRequest;

    let (directory, app, window, _) = open_profile(Library::default());
    let original = app.store.load().unwrap().packs[0].clone();
    let destination = directory.path().join("export");
    let path = destination.join("pack.json");
    app.start_pack_transfer(TransferRequest::Export {
        path: path.clone(),
        pack: Arc::new(original.clone()),
    });
    await_pack_transfer(&app, &window);
    assert!(window.get_pack_transfer_failed());
    window.invoke_select_pack(1);
    std::fs::create_dir(&destination).unwrap();
    window.invoke_retry_pack_transfer();
    await_pack_transfer(&app, &window);
    assert!(!window.get_pack_transfer_failed());
    assert_eq!(Store::read_pack(&path).unwrap(), original);
    assert_eq!(window.get_selected_index(), 1);
}

#[test]
fn quiet_hours_validate_after_editing_and_keep_the_last_saved_range() {
    let mut library = Library::default();
    library.settings.quiet_hours.enabled = true;
    let (_directory, app, window, _) = open_profile(library);
    let original = app.store.load().unwrap().settings.quiet_hours;
    window.set_quiet_start("2".into());
    window.invoke_edit_quiet_range(window.get_quiet_start(), window.get_quiet_end());
    assert!(window.get_quiet_time_error().is_empty());
    assert_eq!(app.store.load().unwrap().settings.quiet_hours, original);
    window.invoke_set_quiet_range(window.get_quiet_start(), window.get_quiet_end());
    assert!(!window.get_quiet_time_error().is_empty());
    app.refresh();
    assert_eq!(window.get_quiet_start(), "2");
    assert!(!window.get_quiet_time_error().is_empty());
    window.set_quiet_start("23:15".into());
    window.invoke_edit_quiet_range(window.get_quiet_start(), window.get_quiet_end());
    assert!(window.get_quiet_time_error().is_empty());
    assert_eq!(
        app.store.load().unwrap().settings.quiet_hours.start_minute,
        23 * 60 + 15
    );
}

#[test]
fn compact_library_browses_packs_and_saves_the_practice_mode_from_the_footer() {
    use crate::test_support::{click, key, paint};
    use slint::platform::Key;

    let mut library = Library::default();
    for index in 0..10 {
        let mut pack = library.packs[0].clone();
        pack.id = format!("navigation_{index}");
        pack.name = format!("Pack {index}");
        pack.cards.clear();
        library.packs.push(pack);
    }
    let (_directory, app, window, adapter) = open_profile(library);
    adapter.set_size(slint::PhysicalSize::new(720, 480));
    window.show().unwrap();
    paint(&adapter);
    let original = app.store.load().unwrap();
    let last = original.packs.len() as i32 - 1;
    click(
        &window,
        &adapter,
        window.get_pack_list_left() + 40.0,
        window.get_pack_list_top() + window.get_pack_row_height() / 2.0,
    );
    for (command, expected) in [
        (Key::DownArrow, 1),
        (Key::End, last),
        (Key::UpArrow, last - 1),
    ] {
        key(&window, &adapter, command);
        assert_eq!(window.get_selected_index(), expected);
        assert_eq!(
            window.get_selected_name().as_str(),
            original.packs[expected as usize].name
        );
    }
    // After jumping offscreen, the first visible row is also from the end of the list.
    click(
        &window,
        &adapter,
        window.get_pack_list_left() + 40.0,
        window.get_pack_list_top() + 20.0,
    );
    assert!(window.get_selected_index() > 0 && window.get_selected_index() < last - 1);
    key(&window, &adapter, Key::Home);
    assert_eq!(window.get_selected_index(), 0);
    assert_eq!(app.store.load().unwrap(), original);

    // At minimum width both mode choices remain exposed in a single control.
    click(
        &window,
        &adapter,
        window.get_question_area_left() + 130.0,
        450.0,
    );
    assert!(window.get_knowledge_only());
    assert!(app.store.load().unwrap().settings.knowledge_only);
    key(&window, &adapter, Key::LeftArrow);
    assert!(!window.get_knowledge_only());
    assert_eq!(app.store.load().unwrap(), original);
}

#[test]
fn selecting_a_pack_preserves_the_quiet_hours_draft() {
    use crate::test_support::{click, paint};

    let library = Library::default();
    let second_pack = library.packs[1].name.clone();
    let (_directory, app, window, adapter) = open_profile(library);
    let saved = app.store.load().unwrap();
    window.set_current_tab(1);
    window.set_quiet_start("22:4".into());
    window.invoke_set_quiet_range(window.get_quiet_start(), window.get_quiet_end());
    let error = window.get_quiet_time_error();
    assert!(!error.is_empty());
    adapter.set_size(slint::PhysicalSize::new(880, 640));
    window.show().unwrap();
    paint(&adapter);

    click(
        &window,
        &adapter,
        window.get_pack_list_left() + 40.0,
        window.get_pack_list_top() + window.get_pack_row_stride() + 20.0,
    );
    assert_eq!(window.get_current_tab(), 0);
    assert_eq!(window.get_selected_index(), 1);
    assert_eq!(window.get_selected_name().as_str(), second_pack);
    assert_eq!(window.get_quiet_start(), "22:4");
    assert_eq!(window.get_quiet_time_error(), error);
    assert_eq!(app.store.load().unwrap(), saved);
}

#[test]
fn card_hints_default_on_for_legacy_profiles_and_remember_both_choices() {
    let mut legacy = serde_json::to_value(Library::default()).unwrap();
    legacy["settings"]
        .as_object_mut()
        .unwrap()
        .remove("show_practice_hints");
    let (_directory, app, window, _) = open_profile(serde_json::from_value(legacy).unwrap());
    assert!(window.get_show_practice_hints());
    let original = app.store.load().unwrap();
    for enabled in [false, true] {
        window.invoke_set_show_practice_hints(enabled);
        let mut expected = original.clone();
        expected.settings.show_practice_hints = enabled;
        assert_eq!(app.store.load().unwrap(), expected);
        assert_eq!(window.get_show_practice_hints(), enabled);
    }
}

#[test]
fn interface_animations_default_on_for_old_profiles_and_save_immediately() {
    let mut legacy = serde_json::to_value(Library::default()).unwrap();
    legacy["settings"]
        .as_object_mut()
        .unwrap()
        .remove("interface_animations");
    let (_directory, app, window, _) = open_profile(serde_json::from_value(legacy).unwrap());
    assert!(window.get_interface_animations());
    window.invoke_set_interface_animations(false);
    assert_eq!(
        window.global::<crate::Theme>().get_motion(),
        crate::MotionPreference::Reduced
    );
    assert!(!window.get_interface_animations());
    assert!(!app.store.load().unwrap().settings.interface_animations);
    window.invoke_set_interface_animations(true);
    assert!(app.store.load().unwrap().settings.interface_animations);
    assert_eq!(
        window.global::<crate::Theme>().get_motion(),
        app.interface_motion()
    );
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
    let (_directory, app, window, _) = open_profile(library);
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
fn editor_persists_each_field_format_and_follows_the_correct_choice() {
    let (_directory, app, window, _) = open_profile(Library::default());
    let before = app.store.load().unwrap();
    let index = before.packs[0].cards.len();
    window.invoke_add_card();
    window.invoke_set_editor_answer_mode(AnswerMode::MultipleChoice.index());
    let edit = |index, value: &str, formula| {
        window.invoke_activate_content(index);
        assert!(!window.get_math_field_formula(), "new field {index}");
        window.invoke_set_content_formula(formula);
        assert_eq!(window.get_math_field_formula(), formula);
        window.set_math_input(value.into());
        window.invoke_edit_math_input(value.into());
    };
    edit(0, "x_i", false);
    edit(1, "std::unique_ptr", false);
    edit(2, "x_i", true);
    edit(3, r"C:\src\main.rs", false);
    window.set_editor_correct_choice(1);
    window.invoke_save_editor();
    assert!(
        !window.get_editor_visible(),
        "{}",
        window.get_editor_error()
    );
    let saved = app.store.load().unwrap();
    let card = &saved.packs[0].cards[index];
    assert_eq!(card.prompt, "x_i");
    assert!(card.prompt_latex.is_none());
    assert!(card.answer_latex.is_some());
    assert!(card.accepts("x_{i}"));
    assert_eq!(card.distractors[0].text, "std::unique_ptr");
    assert_eq!(card.distractors[1].text, r"C:\src\main.rs");
    assert!(card.distractors.iter().all(|choice| choice.latex.is_none()));

    window.invoke_edit_card(index as i32);
    window.invoke_activate_content(1);
    assert!(window.get_math_field_formula());
    window.invoke_set_content_formula(false);
    assert!(!window.get_math_field_formula());
    window.invoke_cancel_editor();
    assert_eq!(app.store.load().unwrap(), saved);
}

#[test]
fn practice_time_changes_preserve_quiet_hours_edits_and_reject_invalid_values() {
    let (_directory, app, window, _) = open_profile(Library::default());
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
