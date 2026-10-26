//! Main-window creation, activation and teardown.
mod bindings;
mod view_model;
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

        bindings::connect(self, &window);

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
}
