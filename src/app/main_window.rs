//! Main-window bindings and view models for the local library.
use super::editor::editing_state;
use super::*;

impl App {
    pub(super) fn show_main(self: &Rc<Self>) -> AppResult<()> {
        if self.practice.borrow().is_some() {
            self.close_practice();
        }
        self.reminder_timer.stop();
        self.read_autostart_status();
        self.refresh();
        if let Some(window) = self.main_window.borrow().as_ref() {
            log::debug!(target: "try::app", "event=main_reopened");
            window.window().set_minimized(false);
            FloatingWindow::activate(window.window());
            return window.show().map_err(|error| error.to_string());
        }
        let window = MainWindow::new().map_err(|error| error.to_string())?;
        let weak = Rc::downgrade(self);
        let applied_theme = Cell::new(None);
        window.window().on_winit_window_event(move |window, event| {
            crate::window_repaint::prepare(window, event);
            if let WindowEvent::DroppedFile(path) = event
                && let Some(app) = weak.upgrade()
            {
                app.import_question_image(path.clone());
            }
            if matches!(
                event,
                WindowEvent::RedrawRequested | WindowEvent::ThemeChanged(_)
            ) && let Some(app) = weak.upgrade()
            {
                app.apply_native_theme(
                    window,
                    &applied_theme,
                    matches!(event, WindowEvent::ThemeChanged(_)),
                );
            }
            EventResult::Propagate
        });
        window.set_math_functions(Self::palette_model(r#try::math_palette::FUNCTIONS));
        window.set_math_symbols(Self::palette_model(r#try::math_palette::SYMBOLS));
        log::debug!(target: "try::app", "event=main_created");
        window.set_app_icon(self.icon.clone());
        window.set_app_version(env!("CARGO_PKG_VERSION").into());
        window.set_current_tab(self.main_tab.get());

        let weak = Rc::downgrade(self);
        window.on_tray_navigation_changed(move || {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(|app| app.refresh_tray_navigation());
            }
        });

        let weak = Rc::downgrade(self);
        window.on_check_updates(move || {
            if let Some(app) = weak.upgrade() {
                app.begin_update_check(app.update_fetch.get());
            }
        });
        let weak = Rc::downgrade(self);
        window.on_update_check_finished(move || {
            if let Some(app) = weak.upgrade() {
                app.refresh_update_status();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_open_update_download(move || {
            if let Some(app) = weak.upgrade() {
                app.open_update_download();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_cancel_update_download(move || {
            if let Some(app) = weak.upgrade() {
                app.cancel_update_download();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_restart_update(move || {
            if let Some(app) = weak.upgrade() {
                app.restart_after_update();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_update_editing_changed(move || {
            if let Some(app) = weak.upgrade() {
                app.resume_ready_update();
            }
        });

        let weak = Rc::downgrade(self);
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

        let weak = Rc::downgrade(self);
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

        let weak = Rc::downgrade(self);
        window.on_return_to_about(move || {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(|app| {
                    if let Some(window) = app.main_window.borrow().as_ref() {
                        window.invoke_show_about();
                    }
                });
            }
        });

        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        window.on_toggle_pack(move |id, enabled| {
            if let Some(app) = weak.upgrade() {
                app.set_pack_enabled(id.as_str(), enabled);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_toggle_all_cards(move |enabled| {
            if let Some(app) = weak.upgrade() {
                app.set_all_cards_enabled(enabled);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_toggle_card(move |index, enabled| {
            if let Some(app) = weak.upgrade() {
                app.set_card_enabled(index, enabled);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_edit_card(move |index| {
            if let Some(app) = weak.upgrade() {
                app.open_editor(Some(index), false, None);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_add_card(move || {
            if let Some(app) = weak.upgrade() {
                app.open_editor(None, false, None);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_create_pack(move || {
            if let Some(app) = weak.upgrade() {
                app.open_editor(None, true, None);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_save_editor(move || {
            if let Some(app) = weak.upgrade() {
                app.save_editor();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_cancel_editor(move || {
            if let Some(app) = weak.upgrade() {
                app.close_editor();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_delete_card(move || {
            if let Some(app) = weak.upgrade() {
                app.request_delete_card();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_delete_row(move |index| {
            if let Some(app) = weak.upgrade() {
                app.request_delete_row(index);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_confirm_card_delete(move || {
            if let Some(app) = weak.upgrade() {
                app.confirm_delete_card();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_cancel_card_delete(move || {
            if let Some(app) = weak.upgrade() {
                app.cancel_delete_card();
            }
        });
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        window.on_request_row_formula(move |index| {
            if let Some(app) = weak.upgrade() {
                app.queue_row_render(index);
            }
        });
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        window.on_prepare_quick_options(move |pack_id, card_id| {
            if let Some(app) = weak.upgrade() {
                app.prepare_quick_options(pack_id.as_str(), card_id.as_str());
            }
        });
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        window.on_activate_content(move |field| {
            if let Some(app) = weak.upgrade() {
                app.activate_content(field);
            }
        });
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        self.bind_structured_math(window.global::<crate::MathInputState>());
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
        let weak = Rc::downgrade(self);
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
                        if ["parent", "previous", "next", "exit", "up", "down"]
                            .contains(&name.as_str())
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
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        window.on_resize_math_grid(move |rows, columns| {
            if let Some(app) = weak.upgrade()
                && app.flush_math()
            {
                app.modify_math(|field| {
                    field.resize_grid(rows.max(1) as usize, columns.max(1) as usize)
                });
            }
        });
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        window.on_toggle_knowledge_mode(move || {
            if let Some(app) = weak.upgrade() {
                app.knowledge_only.set(!app.knowledge_only.get());
                app.refresh();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_save_pack_name(move || {
            if let Some(app) = weak.upgrade() {
                app.save_pack_metadata(PackMetadata::Name);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_save_pack_description(move || {
            if let Some(app) = weak.upgrade() {
                app.save_pack_metadata(PackMetadata::Description);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_save_pack_tag(move || {
            if let Some(app) = weak.upgrade() {
                app.save_pack_metadata(PackMetadata::Tag);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_card_answer_mode(move |row, mode| {
            if let Some(app) = weak.upgrade() {
                app.set_card_answer_mode(row, mode);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_editor_answer_mode(move |index| {
            if let Some(app) = weak.upgrade() {
                app.set_editor_answer_mode(index);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_question_image(move || {
            if let Some(app) = weak.upgrade() {
                app.choose_question_image();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_remove_question_image(move || {
            if let Some(app) = weak.upgrade() {
                let _ = app.set_editor_image(None);
                app.close_image_preview();
                app.collect_media();
            }
        });
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        window.on_import_pack(move || {
            if let Some(app) = weak.upgrade() {
                app.import_pack();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_export_pack(move || {
            if let Some(app) = weak.upgrade() {
                app.export_pack();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_delete_pack(move || {
            if let Some(app) = weak.upgrade() {
                app.request_delete_pack();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_confirm_delete(move || {
            if let Some(app) = weak.upgrade() {
                app.confirm_delete_pack();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_cancel_delete(move || {
            if let Some(app) = weak.upgrade() {
                app.pending_delete.borrow_mut().take();
                app.refresh();
                app.notify("", false);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_confirm_import(move || {
            if let Some(app) = weak.upgrade() {
                app.confirm_import();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_cancel_import(move || {
            if let Some(app) = weak.upgrade() {
                app.pending_import.borrow_mut().take();
                app.collect_media();
                app.refresh();
                app.notify("", false);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_pause(move |minutes| {
            if let Some(app) = weak.upgrade() {
                app.set_pause(minutes);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_interval(move |index| {
            if let Some(app) = weak.upgrade() {
                let current = app.library.borrow().settings.interval_minutes as i32;
                if let Some(value) = preset_value(index, &[15, 25, 30, 45], current) {
                    app.set_interval(value);
                }
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_feedback(move |index| {
            if let Some(app) = weak.upgrade() {
                let current = app.library.borrow().settings.feedback_seconds as i32;
                if let Some(value) = preset_value(index, &[1, 2, 3], current) {
                    app.set_feedback(value);
                }
            }
        });
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        window.on_set_interval(move |value| {
            if let Some(app) = weak.upgrade() {
                app.set_interval(value);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_feedback(move |value| {
            if let Some(app) = weak.upgrade() {
                app.set_feedback(value);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_autostart(move |enabled| {
            if let Some(app) = weak.upgrade() {
                app.set_autostart(enabled);
            }
        });
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        window.on_set_quiet_enabled(move |enabled| {
            if let Some(app) = weak.upgrade() {
                app.set_quiet_enabled(enabled);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_quiet_range(move |start, end| {
            if let Some(app) = weak.upgrade() {
                app.set_quiet_range(start.as_str(), end.as_str());
            }
        });
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        window.on_set_remember_position(move |enabled| {
            if let Some(app) = weak.upgrade() {
                let result = app.commit(|library| {
                    library.settings.remember_position = enabled;
                    Ok(())
                });
                app.report(result, "");
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_show_pack_tags(move |enabled| {
            if let Some(app) = weak.upgrade() {
                let result = app.commit(|library| {
                    library.settings.show_pack_tags = enabled;
                    Ok(())
                });
                app.report(result, "");
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_focus_on_reminder(move |enabled| {
            if let Some(app) = weak.upgrade() {
                let result = app.commit(|library| {
                    library.settings.focus_on_reminder = enabled;
                    Ok(())
                });
                app.report(result, "");
            }
        });
        let weak = Rc::downgrade(self);
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
        let weak = Rc::downgrade(self);
        window.on_go_background(move || {
            if let Some(app) = weak.upgrade() {
                app.close_main();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_quit_app(move || {
            if let Some(app) = weak.upgrade() {
                app.quit();
            }
        });
        let weak = Rc::downgrade(self);
        window.window().on_close_requested(move || {
            let weak = weak.clone();
            Timer::single_shot(Duration::ZERO, move || {
                if let Some(app) = weak.upgrade() {
                    app.close_main();
                }
            });
            slint::CloseRequestResponse::HideWindow
        });

        self.main_window.replace(Some(window.clone_strong()));
        self.update_check.lock().unwrap().observer = Some(window.as_weak());
        self.refresh();
        if let Err(error) = window.show() {
            self.main_window.borrow_mut().take();
            self.update_check.lock().unwrap().observer = None;
            self.arm_reminder();
            return Err(error.to_string());
        }
        log::debug!(target: "try::app", "event=main_shown");
        Ok(())
    }

    pub(super) fn close_main(self: &Rc<Self>) {
        self.close_structured_math_for(false);
        self.close_image_preview();
        self.editor_image.borrow_mut().take();
        self.image_revision
            .set(self.image_revision.get().wrapping_add(1));
        self.row_render_timer.stop();
        self.pending_rows.borrow_mut().clear();
        self.editor_preview_timer.stop();
        self.editor.borrow_mut().take();
        self.math_fields.borrow_mut().clear();
        self.math_paths.borrow_mut().clear();
        self.math_pending.set(false);
        self.update_check.lock().unwrap().observer = None;
        let window = self.main_window.borrow_mut().take();
        if let Some(window) = window {
            self.main_tab.set(window.get_current_tab());
            let _ = window.hide();
            log::debug!(target: "try::app", "event=main_closed");
        }
        self.pending_import.borrow_mut().take();
        self.pending_delete.borrow_mut().take();
        self.pending_card_delete.borrow_mut().take();
        self.refresh_tray_navigation();
        self.collect_media();
        self.arm_reminder();
        self.resume_ready_update();
    }

    pub(super) fn refresh(&self) {
        self.refresh_update_status();
        self.refresh_tray_navigation();
        let library = self.library.borrow();
        if let Some(session) = self.practice.borrow().as_ref() {
            let dark = self.resolved_dark_theme(library.settings.appearance);
            if session.window.get_dark_theme() != dark {
                session.window.set_dark_theme(dark);
                self.floating.apply_theme(session.window.window(), dark);
            }
        }
        let count = library.enabled_card_count();
        let now = chrono::Local::now();
        let pause_minutes = library.settings.pause.minutes(now.timestamp());
        let pause_end = match &library.settings.pause {
            PracticePause::Timed { until, .. } if pause_minutes > 0 => {
                let end = chrono::DateTime::from_timestamp(*until, 0)
                    .expect("validated pause")
                    .with_timezone(&chrono::Local);
                let pattern = if end.date_naive() == now.date_naive() {
                    "%H:%M"
                } else {
                    "%m-%d %H:%M"
                };
                Some(end.format(pattern).to_string())
            }
            _ => None,
        };
        self.tray.set_reminders_enabled(pause_minutes == 0);
        self.tray.set_can_practice(count > 0);
        self.tray.set_pause_minutes(pause_minutes);
        self.tray.set_pause_title(match &pause_end {
            Some(end) => tr!("暂离至 {p0}", p0 = end).into(),
            _ => tr!("暂离").into(),
        });
        let window = self.main_window.borrow();
        let Some(window) = window.as_ref() else {
            return;
        };
        let rows = library
            .packs
            .iter()
            .map(|pack| {
                let participating = library.pack_enabled_card_count(pack);
                PackRow {
                    id: pack.id.clone().into(),
                    name: pack.name.clone().into(),
                    tag: pack.tag.trim().into(),
                    participating: library.is_enabled(&pack.id),
                    has_cards: !pack.cards.is_empty(),
                    detail: if participating > 0 {
                        tr!("{p0} / {p1} 题", p0 = participating, p1 = pack.cards.len())
                    } else {
                        tr!("{p0} 题 · 未参与", p0 = pack.cards.len())
                    }
                    .into(),
                }
            })
            .collect::<Vec<_>>();
        let packs = window.get_packs();
        if let Some(model) = packs.as_any().downcast_ref::<VecModel<PackRow>>()
            && model.row_count() == rows.len()
        {
            for (index, row) in rows.into_iter().enumerate() {
                if model.row_data(index).as_ref() != Some(&row) {
                    model.set_row_data(index, row);
                }
            }
        } else {
            window.set_packs(ModelRc::new(VecModel::from(rows)));
        }
        window.set_participation_summary(
            tr!(
                "{p0} 个包 · {p1} 道题",
                p0 = library
                    .packs
                    .iter()
                    .filter(|pack| library.pack_enabled_card_count(pack) > 0)
                    .count(),
                p1 = library.enabled_card_count()
            )
            .into(),
        );
        let index = self
            .selected_pack
            .get()
            .min(library.packs.len().saturating_sub(1));
        self.selected_pack.set(index);
        window.set_selected_index(index as i32);
        window.set_has_selection(!library.packs.is_empty());
        if let Some(pack) = library.packs.get(index) {
            window.set_selected_name(pack.name.clone().into());
            window.set_selected_description(pack.description.clone().into());
            window.set_selected_description_summary(
                pack.description
                    .lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .unwrap_or_default()
                    .into(),
            );
            window.set_selected_tag(pack.tag.trim().into());
            let enabled = library.pack_enabled_card_count(pack);
            window.set_selected_enabled(!pack.cards.is_empty() && enabled == pack.cards.len());
            window.set_selection_partial(enabled > 0 && enabled < pack.cards.len());
            window.set_selected_card_count(
                tr!(
                    "{total} 道题 · {enabled} 道参与",
                    total = pack.cards.len(),
                    enabled = enabled
                )
                .into(),
            );
            let existing = window.get_cards();
            let card_rows = pack
                .cards
                .iter()
                .enumerate()
                .map(|(index, card)| {
                    let mode =
                        library.card_answer_mode(&pack.id, card.id.as_deref().unwrap_or_default());
                    let display = if self.knowledge_only.get() {
                        PreparedQuestion::knowledge(card)
                    } else {
                        let display_mode = if mode == AnswerMode::TrueFalse {
                            mode
                        } else {
                            AnswerMode::Input
                        };
                        PreparedQuestion::new(card, display_mode, &mut rand::rng())
                            .expect("the selected mode has validated content")
                    };
                    let prompt = if display.has_prompt() {
                        display.prompt.text
                    } else {
                        tr!("图片题 {p0}", p0 = index + 1)
                    };
                    let source = display.prompt.latex.unwrap_or_default();
                    let answer = if mode == AnswerMode::TrueFalse {
                        if card
                            .true_false
                            .as_ref()
                            .is_some_and(|statement| statement.is_true)
                        {
                            "✓".into()
                        } else {
                            "✕".into()
                        }
                    } else {
                        card.answer.clone()
                    };
                    let answer_source = if mode == AnswerMode::TrueFalse {
                        String::new()
                    } else {
                        card.answer_latex.clone().unwrap_or_default()
                    };
                    let cached = existing.row_data(index).filter(|row| {
                        row.prompt.as_str() == prompt
                            && row.formula_source.as_str() == source
                            && row.answer.as_str() == answer
                            && row.answer_formula_source.as_str() == answer_source
                    });
                    CardRow {
                        id: card.id.clone().unwrap_or_default().into(),
                        pack_id: pack.id.clone().into(),
                        prompt: prompt.into(),
                        quick_answer: card.answer.clone().into(),
                        truth: card
                            .true_false
                            .as_ref()
                            .is_some_and(|statement| statement.is_true),
                        answer: answer.into(),
                        answer_formula_source: answer_source.into(),
                        answer_preview: cached
                            .as_ref()
                            .map(|row| row.answer_preview.clone())
                            .unwrap_or_default(),
                        choice_available: card.supports(AnswerMode::MultipleChoice),
                        judgment_available: card.supports(AnswerMode::TrueFalse),
                        enabled: library
                            .is_card_enabled(&pack.id, card.id.as_deref().unwrap_or_default()),
                        answer_mode: library
                            .card_answer_mode(&pack.id, card.id.as_deref().unwrap_or_default())
                            .index(),
                        formula_source: source.into(),
                        rendered: cached.as_ref().is_some_and(|row| row.rendered),
                        formula_scale: cached.as_ref().map_or(0.0, |row| row.formula_scale),
                        preview: cached.map(|row| row.preview).unwrap_or_default(),
                    }
                })
                .collect::<Vec<_>>();
            let model = window.get_cards();
            if let Some(model) = model.as_any().downcast_ref::<VecModel<CardRow>>()
                && model.row_count() == card_rows.len()
            {
                // Keep the same row components and keyboard focus when a box changes.
                for (index, row) in card_rows.into_iter().enumerate() {
                    if model.row_data(index).as_ref() != Some(&row) {
                        model.set_row_data(index, row);
                    }
                }
            } else {
                window.set_cards(ModelRc::new(VecModel::from(card_rows)));
            }
            window.set_uniform_answer_mode(
                library
                    .uniform_answer_mode(&pack.id)
                    .map_or(-1, AnswerMode::index),
            );
            window.set_all_choice_available(pack.supports(AnswerMode::MultipleChoice));
            window.set_all_judgment_available(pack.supports(AnswerMode::TrueFalse));
        } else {
            window.set_selected_name("".into());
            window.set_selected_description("".into());
            window.set_selected_description_summary("".into());
            window.set_selected_tag("".into());
            window.set_selected_card_count("".into());
            window.set_selected_enabled(false);
            window.set_selection_partial(false);
            window.set_cards(ModelRc::default());
            window.set_quick_options(ModelRc::default());
            window.set_uniform_answer_mode(-1);
            window.set_all_choice_available(false);
            window.set_all_judgment_available(false);
        }
        window.set_knowledge_only(self.knowledge_only.get());
        let dark = self.resolved_dark_theme(library.settings.appearance);
        if window.get_dark_theme() != dark {
            window.set_dark_theme(dark);
            self.floating.apply_theme(window.window(), dark);
        }
        window.set_appearance(library.settings.appearance.index());
        window.set_interface_language(library.settings.interface_language.index());
        window.set_interval_minutes(library.settings.interval_minutes as i32);
        let (options, index) = preset_options(
            library.settings.interval_minutes as i32,
            &[15, 25, 30, 45],
            |n| tr!("{n} 分钟", n = n),
        );
        window.set_interval_options(options);
        window.set_interval_selection(index);
        window.set_feedback_seconds(library.settings.feedback_seconds as i32);
        let (options, index) =
            preset_options(library.settings.feedback_seconds as i32, &[1, 2, 3], |n| {
                tr!("{n} 秒", n = n)
            });
        window.set_feedback_options(options);
        window.set_feedback_selection(index);
        let (options, index) = preset_options(pause_minutes, &[0, 60, 120, 180, -1], |n| match n {
            0 => tr!("不暂停").into(),
            -1 => tr!("直到恢复").into(),
            60 | 120 | 180 => tr!("{p0} 小时", p0 = n / 60),
            _ => tr!("{n} 分钟", n = n),
        });
        window.set_pause_options(options);
        window.set_pause_selection(index);
        window.set_pause_minutes(pause_minutes);
        window.set_pause_detail(match &pause_end {
            Some(end) => tr!("至 {p0}", p0 = end).into(),
            None if pause_minutes == -1 => tr!("等待恢复").into(),
            _ => "".into(),
        });
        window.set_autostart_available(self.autostart.borrow().is_ok());
        window.set_autostart_enabled(self.autostart_enabled.get());
        window.set_autostart_error(self.autostart_error.borrow().clone().into());
        let quiet = &library.settings.quiet_hours;
        window.set_quiet_hours_enabled(quiet.enabled);
        window.set_quiet_start(format_time(quiet.start_minute).into());
        window.set_quiet_end(format_time(quiet.end_minute).into());
        window.set_quiet_summary(
            format!(
                "{}–{}{}",
                format_time(quiet.start_minute),
                if quiet.start_minute > quiet.end_minute {
                    tr!("次日 ")
                } else {
                    ""
                },
                format_time(quiet.end_minute)
            )
            .into(),
        );
        window.set_quiet_time_error("".into());
        window.set_popup_corner(library.settings.popup_corner.index());
        window.set_remember_position(library.settings.remember_position);
        window.set_show_pack_tags(library.settings.show_pack_tags);
        window.set_focus_on_reminder(library.settings.focus_on_reminder);
        window.set_auto_check_updates(library.settings.auto_check_updates);
        window.set_has_saved_position(library.settings.popup_position.is_some());
        window.set_practice_summary(if count == 0 {
            tr!("0 题").into()
        } else if pause_minutes == 0 {
            tr!(
                "{count} 题 · {p0} 分钟",
                p0 = library.settings.interval_minutes,
                count = count
            )
            .into()
        } else {
            tr!("{count} 题 · 暂停", count = count).into()
        });
        let pending = self.pending_import.borrow();
        window.set_import_pending(pending.is_some());
        if let Some(pack) = pending.as_ref() {
            let original = library.packs.iter().find(|item| item.id == pack.id);
            let original_description = original
                .map(|item| tr!("“{p0}”（{p1} 道题）", p0 = item.name, p1 = item.cards.len()))
                .unwrap_or_else(|| pack.id.clone());
            window.set_import_description(
                tr!(
                    "已存在{original_description}。将用导入的“{p0}”（{p1} 道题）替换原内容。",
                    p0 = pack.name,
                    p1 = pack.cards.len(),
                    original_description = original_description
                )
                .into(),
            );
        }
        let pending_delete = self.pending_delete.borrow();
        window.set_delete_pending(pending_delete.is_some());
        window.set_delete_description(
            pending_delete
                .as_ref()
                .and_then(|id| library.packs.iter().find(|pack| &pack.id == id))
                .map(|pack| {
                    tr!(
                        "删除“{name}”及其中的 {count} 道题？此操作无法撤销。",
                        name = pack.name,
                        count = pack.cards.len()
                    )
                })
                .unwrap_or_default()
                .into(),
        );
    }
}
