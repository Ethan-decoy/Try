//! Main-window creation, activation and teardown.
mod bindings;
#[cfg(all(test, target_os = "windows"))]
mod tests;
mod view_model;
use super::*;

impl App {
    pub(super) fn show_main(self: &Rc<Self>) -> AppResult<()> {
        self.hide_outgoing_practice();
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
        #[cfg(target_os = "windows")]
        crate::floating_window::desktop_frame::configure(&window, MainWindow::get_window_controls);
        window
            .global::<crate::Theme>()
            .set_motion(self.interface_motion());
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

        bindings::connect(self, &window);

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

    pub(super) fn request_leave(self: &Rc<Self>, action: LeaveAction) {
        if self.pack_transfer.borrow().is_running() {
            return;
        }
        let window = self
            .main_window
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong);
        if let Some(window) = window
            && (self.editor_has_changes(&window) || changed_metadata(&window).is_some())
        {
            if self.pending_leave.get().is_none() {
                self.pending_leave.set(Some(action));
                window.set_animate_leave_confirmation(window.get_leave_from_pointer());
                window.set_leave_description(
                    match action {
                        LeaveAction::Background => tr!("返回托盘前，要保存当前修改吗？"),
                        LeaveAction::Quit => tr!("退出前，要保存当前修改吗？"),
                        LeaveAction::Practice => tr!("开始练习前，要保存当前修改吗？"),
                    }
                    .into(),
                );
                window.set_leave_pending(true);
            }
            window.window().set_minimized(false);
            FloatingWindow::activate(window.window());
            return;
        }
        self.finish_leave(action);
    }

    fn finish_leave(self: &Rc<Self>, action: LeaveAction) {
        match action {
            LeaveAction::Background => self.close_main(),
            LeaveAction::Quit => self.quit(),
            LeaveAction::Practice => self.report(self.show_practice(), ""),
        }
    }

    fn cancel_leave(&self) {
        self.pending_leave.take();
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_leave_pending(false);
            window.invoke_restore_draft_focus();
        }
    }

    fn resolve_leave(self: &Rc<Self>, save: bool) {
        let Some(action) = self.pending_leave.get() else {
            return;
        };
        let Some(window) = self
            .main_window
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
        else {
            return;
        };
        if save && self.editor.borrow().is_some() {
            if window.get_editor_image_loading() {
                return;
            }
            if self.pending_card_delete.borrow().is_some() {
                self.cancel_delete_card();
            }
            self.save_editor();
            if self.editor.borrow().is_some() {
                self.cancel_leave();
                return;
            }
        } else if save && let Some(field) = changed_metadata(&window) {
            self.save_pack_metadata(field);
            if !window.get_metadata_error().is_empty() {
                self.cancel_leave();
                return;
            }
        }
        self.pending_leave.take();
        // If opening a practice card fails, the original editing surface remains available.
        window.set_leave_pending(false);
        self.finish_leave(action);
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
        self.editor_initial.borrow_mut().take();
        self.pending_leave.take();
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
        self.pack_transfer.take();
        self.pending_delete.borrow_mut().take();
        self.pending_card_delete.borrow_mut().take();
        self.refresh_tray_navigation();
        self.collect_media();
        self.arm_reminder();
        self.resume_ready_update();
    }
}

fn changed_metadata(window: &MainWindow) -> Option<PackMetadata> {
    if window.get_name_editing() && window.get_name_draft() != window.get_selected_name() {
        Some(PackMetadata::Name)
    } else if window.get_tag_editing() && window.get_tag_draft() != window.get_selected_tag() {
        Some(PackMetadata::Tag)
    } else if window.get_description_editing()
        && window.get_description_draft() != window.get_selected_description()
    {
        Some(PackMetadata::Description)
    } else {
        None
    }
}
