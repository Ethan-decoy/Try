//! Main-window actions grouped by the feature that owns their behavior.
use super::super::editor::editing_state;
use super::super::*;

pub(super) fn connect(app: &Rc<App>, window: &MainWindow) {
    connect_navigation(app, window);
    connect_about_updates(app, window);
    connect_library(app, window);
    connect_editor(app, window);
    connect_math(app, window);
    connect_settings(app, window);
}

fn connect_navigation(app: &Rc<App>, window: &MainWindow) {
    let weak = Rc::downgrade(app);
    window.on_tray_navigation_changed(move || {
        if let Some(app) = weak.upgrade() {
            app.defer_tray_action(|app| app.refresh_tray_navigation());
        }
    });
    let weak = Rc::downgrade(app);
    window.on_go_background(move || {
        if let Some(app) = weak.upgrade() {
            app.close_main();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_quit_app(move || {
        if let Some(app) = weak.upgrade() {
            app.quit();
        }
    });
}

fn connect_about_updates(app: &Rc<App>, window: &MainWindow) {
    let weak = Rc::downgrade(app);
    window.on_check_updates(move || {
        if let Some(app) = weak.upgrade() {
            app.begin_update_check(app.update_fetch.get());
        }
    });
    let weak = Rc::downgrade(app);
    window.on_update_check_finished(move || {
        if let Some(app) = weak.upgrade() {
            app.refresh_update_status();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_open_update_download(move || {
        if let Some(app) = weak.upgrade() {
            app.open_update_download();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_cancel_update_download(move || {
        if let Some(app) = weak.upgrade() {
            app.cancel_update_download();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_restart_update(move || {
        if let Some(app) = weak.upgrade() {
            app.restart_after_update();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_update_editing_changed(move || {
        if let Some(app) = weak.upgrade() {
            app.resume_ready_update();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_open_project_home(move || {
        if let Some(app) = weak.upgrade() {
            app.defer_tray_action(|app| {
                if let Err(error) = webbrowser::open("https://github.com/Ethan-decoy/Try") {
                    log::warn!(target: "try::app", "event=project_home_open_failed reason={error}");
                    app.notify(tr!("无法打开浏览器"), true);
                }
            });
        }
    });
    let weak = Rc::downgrade(app);
    window.on_open_notices(move || {
        if let Some(app) = weak.upgrade() {
            // Wait for the popup's pointer handler and close to finish.
            app.defer_tray_action(|app| {
                if let Some(window) = app.main_window.borrow().as_ref() {
                    window.invoke_show_notices();
                }
            });
        }
    });
    let weak = Rc::downgrade(app);
    window.on_return_to_about(move || {
        if let Some(app) = weak.upgrade() {
            app.defer_tray_action(|app| {
                if let Some(window) = app.main_window.borrow().as_ref() {
                    window.invoke_show_about();
                }
            });
        }
    });
}

fn connect_library(app: &Rc<App>, window: &MainWindow) {
    let weak = Rc::downgrade(app);
    window.on_select_pack(move |index| {
        if let Some(app) = weak.upgrade()
            && index >= 0
            && (index as usize) < app.library.borrow().packs.len()
        {
            app.selected_pack.set(index as usize);
            if let Some(window) = app.main_window.borrow().as_ref() {
                window.set_current_tab(0);
                window.invoke_reset_overview();
            }
            app.refresh();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_toggle_pack(move |id, enabled| {
        if let Some(app) = weak.upgrade() {
            app.set_pack_enabled(id.as_str(), enabled);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_toggle_all_cards(move |enabled| {
        if let Some(app) = weak.upgrade() {
            app.set_all_cards_enabled(enabled);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_toggle_card(move |index, enabled| {
        if let Some(app) = weak.upgrade() {
            app.set_card_enabled(index, enabled);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_create_pack(move || {
        if let Some(app) = weak.upgrade() {
            app.open_editor(None, true, None);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_all_answer_modes(move |index| {
        if let Some(app) = weak.upgrade() {
            let Some(mode) = usize::try_from(index)
                .ok()
                .and_then(|index| AnswerMode::ALL.get(index).copied())
            else {
                return;
            };
            let selection = app
                .library
                .borrow()
                .packs
                .get(app.selected_pack.get())
                .map(|pack| pack.id.clone());
            if let Some(id) = selection {
                let result = app.commit(|library| library.set_all_answer_modes(&id, mode));
                app.report(result, "");
            }
        }
    });
    let weak = Rc::downgrade(app);
    window.on_toggle_knowledge_mode(move || {
        if let Some(app) = weak.upgrade() {
            app.set_knowledge_mode(!app.knowledge_only.get());
        }
    });
    let weak = Rc::downgrade(app);
    window.on_save_pack_name(move || {
        if let Some(app) = weak.upgrade() {
            app.save_pack_metadata(PackMetadata::Name);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_save_pack_description(move || {
        if let Some(app) = weak.upgrade() {
            app.save_pack_metadata(PackMetadata::Description);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_save_pack_tag(move || {
        if let Some(app) = weak.upgrade() {
            app.save_pack_metadata(PackMetadata::Tag);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_card_answer_mode(move |row, mode| {
        if let Some(app) = weak.upgrade() {
            app.set_card_answer_mode(row, mode);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_import_pack(move || {
        if let Some(app) = weak.upgrade() {
            app.import_pack();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_export_pack(move || {
        if let Some(app) = weak.upgrade() {
            app.export_pack();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_delete_pack(move || {
        if let Some(app) = weak.upgrade() {
            app.request_delete_pack();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_confirm_delete(move || {
        if let Some(app) = weak.upgrade() {
            app.confirm_delete_pack();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_cancel_delete(move || {
        if let Some(app) = weak.upgrade() {
            app.pending_delete.borrow_mut().take();
            app.refresh();
            app.notify("", false);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_confirm_import(move || {
        if let Some(app) = weak.upgrade() {
            app.confirm_import();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_cancel_import(move || {
        if let Some(app) = weak.upgrade() {
            app.pending_import.borrow_mut().take();
            app.collect_media();
            app.refresh();
            app.notify("", false);
        }
    });
}

fn connect_editor(app: &Rc<App>, window: &MainWindow) {
    let weak = Rc::downgrade(app);
    window.on_edit_card(move |index| {
        if let Some(app) = weak.upgrade() {
            app.open_editor(Some(index), false, None);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_add_card(move || {
        if let Some(app) = weak.upgrade() {
            app.open_editor(None, false, None);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_save_editor(move || {
        if let Some(app) = weak.upgrade() {
            app.save_editor();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_cancel_editor(move || {
        if let Some(app) = weak.upgrade() {
            app.close_editor();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_delete_card(move || {
        if let Some(app) = weak.upgrade() {
            app.request_delete_card();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_delete_row(move |index| {
        if let Some(app) = weak.upgrade() {
            app.request_delete_row(index);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_confirm_card_delete(move || {
        if let Some(app) = weak.upgrade() {
            app.confirm_delete_card();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_cancel_card_delete(move || {
        if let Some(app) = weak.upgrade() {
            app.cancel_delete_card();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_preview_editor(move || {
        if let Some(app) = weak.upgrade() {
            let weak = Rc::downgrade(&app);
            app.editor_preview_timer.start(
                TimerMode::SingleShot,
                Duration::from_millis(250),
                move || {
                    if let Some(app) = weak.upgrade() {
                        app.render_editor_preview();
                    }
                },
            );
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_editor_answer_mode(move |index| {
        if let Some(app) = weak.upgrade() {
            app.set_editor_answer_mode(index);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_choose_question_image(move || {
        if let Some(app) = weak.upgrade() {
            app.choose_question_image();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_remove_question_image(move || {
        if let Some(app) = weak.upgrade() {
            let _ = app.set_editor_image(None);
            app.close_image_preview();
            app.collect_media();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_preview_question_image(move || {
        if let Some(app) = weak.upgrade() {
            let image = app
                .main_window
                .borrow()
                .as_ref()
                .map(|w| w.get_editor_picture());
            if let Some(image) = image {
                app.preview_image(image);
            }
        }
    });
}

fn connect_math(app: &Rc<App>, window: &MainWindow) {
    let weak = Rc::downgrade(app);
    window.on_request_row_formula(move |index| {
        if let Some(app) = weak.upgrade() {
            app.queue_row_render(index);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_save_quick_answer(move |pack_id, card_id, input| {
        weak.upgrade()
            .map_or_else(
                || tr!("窗口已关闭。").into(),
                |app| {
                    app.save_quick_answer(pack_id.as_str(), card_id.as_str(), input.as_str())
                        .err()
                        .unwrap_or_default()
                },
            )
            .into()
    });
    let weak = Rc::downgrade(app);
    window.on_prepare_quick_options(move |pack_id, card_id| {
        if let Some(app) = weak.upgrade() {
            app.prepare_quick_options(pack_id.as_str(), card_id.as_str());
        }
    });
    let weak = Rc::downgrade(app);
    window.on_choose_quick_answer(move |pack_id, card_id, mode, index| {
        weak.upgrade()
            .map_or_else(
                || tr!("窗口已关闭。").into(),
                |app| {
                    app.choose_quick_answer(pack_id.as_str(), card_id.as_str(), mode, index)
                        .err()
                        .unwrap_or_default()
                },
            )
            .into()
    });
    let weak = Rc::downgrade(app);
    window.on_activate_content(move |field| {
        if let Some(app) = weak.upgrade() {
            app.activate_content(field);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_edit_math_input(move |_| {
        if let Some(app) = weak.upgrade() {
            app.math_pending.set(true);
            // Record every draft immediately, including incomplete input.
            // Only the expensive preview waits for the debounce timer.
            app.flush_math();
            let weak = Rc::downgrade(&app);
            app.editor_preview_timer.start(
                TimerMode::SingleShot,
                Duration::from_millis(180),
                move || {
                    if let Some(app) = weak.upgrade() {
                        app.refresh_math(false);
                    }
                },
            );
        }
    });
    let weak = Rc::downgrade(app);
    window.on_math_position_changed(move |input, anchor, caret| {
        if let Some(app) = weak.upgrade()
            && let Some(window) = app.main_window.borrow().as_ref()
            && let Ok(index) = usize::try_from(window.get_math_active_field())
            && let Some(field) = app.math_fields.borrow_mut().get_mut(index)
            && field.input() == input.as_str()
        {
            field.set_text_selection(anchor.max(0) as usize, caret.max(0) as usize);
            let (position, navigation) =
                editing_state(field, window.get_math_position().revision, false);
            window.set_math_position(position);
            window.set_math_navigation(navigation);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_select_math_region(move |index| {
        if let Some(app) = weak.upgrade()
            && app.flush_math()
        {
            let path = usize::try_from(index)
                .ok()
                .and_then(|i| app.math_paths.borrow().get(i).cloned());
            if let Some(path) = path {
                app.modify_math(|field| {
                    field.select(path);
                    Ok(())
                });
            }
        }
    });
    let weak = Rc::downgrade(app);
    app.bind_structured_math(window.global::<crate::MathInputState>());
    let reveal_window = window.as_weak();
    window
        .global::<crate::MathInputState>()
        .on_reveal(move |top, height| {
            if let Some(window) = reveal_window.upgrade() {
                let viewport_top = window.get_math_viewport_top() + 4.0;
                let viewport_bottom = viewport_top + window.get_math_viewport_height() - 8.0;
                let delta = if top < viewport_top || height > viewport_bottom - viewport_top {
                    viewport_top - top
                } else if top + height > viewport_bottom {
                    viewport_bottom - top - height
                } else {
                    0.0
                };
                window.set_editor_scroll_y((window.get_editor_scroll_y() + delta).min(0.0));
            }
        });
    window.on_open_structured_math(move || {
        if let Some(app) = weak.upgrade() {
            app.open_content_math();
        }
    });
    let weak = Rc::downgrade(app);
    window.on_math_tool(move |name| {
        if let Some(app) = weak.upgrade() {
            if name == "undo" || name == "redo" {
                app.math_pending.set(false);
                app.editor_preview_timer.stop();
                app.modify_math(|field| {
                    field.undo(name == "redo");
                    Ok(())
                });
            } else if name == "clear" {
                app.math_pending.set(false);
                app.modify_math(|field| field.insert_at("clear"));
            } else if name == "backspace" {
                app.math_pending.set(false);
                app.modify_math(|field| {
                    // A deletion may leave an incomplete draft. Publish it
                    // and its own error so another deletion can repair it.
                    let _ = field.backspace();
                    Ok(())
                });
            } else if app.flush_math() {
                app.modify_math(|field| {
                    if ["parent", "previous", "next", "exit", "up", "down"].contains(&name.as_str())
                    {
                        field.navigate(name.as_str());
                        Ok(())
                    } else {
                        field.insert_at(name.as_str())
                    }
                });
            }
        }
    });
    let weak = Rc::downgrade(app);
    window.on_math_parent(move || {
        if let Some(app) = weak.upgrade()
            && app.flush_math()
        {
            app.modify_math(|field| {
                field.navigate("parent");
                Ok(())
            });
        }
    });
    let weak = Rc::downgrade(app);
    window.on_undo_math(move |redo| {
        if let Some(app) = weak.upgrade() {
            app.math_pending.set(false);
            app.editor_preview_timer.stop();
            app.modify_math(|field| {
                field.undo(redo);
                Ok(())
            });
        }
    });
    let weak = Rc::downgrade(app);
    window.on_resize_math_grid(move |rows, columns| {
        if let Some(app) = weak.upgrade()
            && app.flush_math()
        {
            app.modify_math(|field| {
                field.resize_grid(rows.max(1) as usize, columns.max(1) as usize)
            });
        }
    });
}

fn connect_settings(app: &Rc<App>, window: &MainWindow) {
    let weak = Rc::downgrade(app);
    window.on_set_pause(move |minutes| {
        if let Some(app) = weak.upgrade() {
            app.set_pause(minutes);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_choose_interval(move |index| {
        if let Some(app) = weak.upgrade() {
            let current = app.library.borrow().settings.interval_minutes as i32;
            if let Some(value) = preset_value(index, &[15, 25, 30, 45], current) {
                app.set_interval(value);
            }
        }
    });
    let weak = Rc::downgrade(app);
    window.on_choose_feedback(move |index| {
        if let Some(app) = weak.upgrade() {
            let current = app.library.borrow().settings.feedback_seconds as i32;
            if let Some(value) = preset_value(index, &[1, 2, 3], current) {
                app.set_feedback(value);
            }
        }
    });
    let weak = Rc::downgrade(app);
    window.on_choose_knowledge_duration(move |index| {
        if let Some(app) = weak.upgrade() {
            let current = app.library.borrow().settings.knowledge_seconds as i32;
            if let Some(value) = preset_value(index, &[5, 8, 15, 30], current) {
                app.set_knowledge_duration(value);
            }
        }
    });
    let weak = Rc::downgrade(app);
    window.on_choose_pause(move |index| {
        if let Some(app) = weak.upgrade() {
            let current = app
                .library
                .borrow()
                .settings
                .pause
                .minutes(chrono::Utc::now().timestamp());
            if let Some(value) = preset_value(index, &[0, 60, 120, 180, -1], current) {
                app.set_pause(value);
            }
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_interval(move |value| {
        if let Some(app) = weak.upgrade() {
            app.set_interval(value);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_feedback(move |value| {
        if let Some(app) = weak.upgrade() {
            app.set_feedback(value);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_knowledge_duration(move |value| {
        if let Some(app) = weak.upgrade() {
            app.set_knowledge_duration(value);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_autostart(move |enabled| {
        if let Some(app) = weak.upgrade() {
            app.set_autostart(enabled);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_choose_language(move |index| {
        if let Some(app) = weak.upgrade()
            && let Some(language) = InterfaceLanguage::from_index(index)
        {
            let result = app.commit(|library| {
                library.settings.interface_language = language;
                Ok(())
            });
            app.report(result, "");
        }
    });
    let weak = Rc::downgrade(app);
    window.on_choose_appearance(move |index| {
        if let Some(app) = weak.upgrade()
            && let Some(appearance) = Appearance::from_index(index)
        {
            let result = app.commit(|library| {
                library.settings.appearance = appearance;
                Ok(())
            });
            app.report(result, "");
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_quiet_enabled(move |enabled| {
        if let Some(app) = weak.upgrade() {
            app.set_quiet_enabled(enabled);
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_quiet_range(move |start, end| {
        if let Some(app) = weak.upgrade() {
            app.set_quiet_range(start.as_str(), end.as_str());
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_corner(move |index| {
        if let Some(app) = weak.upgrade()
            && let Some(corner) = PopupCorner::from_index(index)
        {
            let result = app.commit(|library| {
                library.settings.popup_corner = corner;
                library.settings.popup_position = None;
                Ok(())
            });
            app.report(result, "");
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_remember_position(move |enabled| {
        if let Some(app) = weak.upgrade() {
            let result = app.commit(|library| {
                library.settings.remember_position = enabled;
                Ok(())
            });
            app.report(result, "");
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_show_pack_tags(move |enabled| {
        if let Some(app) = weak.upgrade() {
            let result = app.commit(|library| {
                library.settings.show_pack_tags = enabled;
                Ok(())
            });
            app.report(result, "");
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_focus_on_reminder(move |enabled| {
        if let Some(app) = weak.upgrade() {
            let result = app.commit(|library| {
                library.settings.focus_on_reminder = enabled;
                Ok(())
            });
            app.report(result, "");
        }
    });
    let weak = Rc::downgrade(app);
    window.on_set_auto_check_updates(move |enabled| {
        if let Some(app) = weak.upgrade() {
            let result = app.commit(|library| {
                library.settings.auto_check_updates = enabled;
                Ok(())
            });
            if result.is_ok() {
                app.arm_automatic_update_check(true);
            }
            app.report(result, "");
        }
    });
}
