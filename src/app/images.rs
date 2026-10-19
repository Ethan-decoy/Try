//! Question image import, owned-media collection and image preview lifecycle.
use super::*;

impl App {
    pub(super) fn collect_media(&self) {
        let mut keep: std::collections::BTreeSet<String> = self
            .library
            .borrow()
            .packs
            .iter()
            .flat_map(|pack| {
                pack.cards
                    .iter()
                    .filter_map(|card| card.prompt_image.clone())
            })
            .collect();
        keep.extend(self.editor_image.borrow().clone());
        if let Some(pack) = self.pending_import.borrow().as_ref() {
            keep.extend(
                pack.cards
                    .iter()
                    .filter_map(|card| card.prompt_image.clone()),
            );
        }
        self.store.media().collect(&keep);
    }

    pub(super) fn set_editor_image(&self, id: Option<String>) -> AppResult<()> {
        self.editor_image.replace(id.clone());
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_editor_has_image(id.is_some());
            window.set_editor_picture(slint::Image::default());
            window.set_editor_picture_width(0.0);
            window.set_editor_picture_height(0.0);
            if let Some(id) = id {
                let loaded = self.store.media().load(&id)?;
                window.set_editor_picture(loaded.image);
                window.set_editor_picture_width(loaded.width as f32);
                window.set_editor_picture_height(loaded.height as f32);
            }
        }
        Ok(())
    }

    pub(super) fn choose_question_image(self: &Rc<Self>) {
        if self.editor.borrow().is_none() || self.image_task.borrow().is_some() {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!("选择题面图片"))
            .add_filter(tr!("图片"), &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        self.import_question_image(path);
    }

    pub(super) fn import_question_image(self: &Rc<Self>, path: std::path::PathBuf) {
        if self.editor.borrow().is_none() || self.image_task.borrow().is_some() {
            return;
        }
        let revision = self.image_revision.get();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.image_task.replace(Some((revision, receiver)));
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_editor_image_loading(true);
            window.set_editor_error("".into());
        }
        std::thread::spawn(move || {
            let _ = sender.send(r#try::media::normalize_file(&path));
        });
        let weak = Rc::downgrade(self);
        self.image_task_timer
            .start(TimerMode::Repeated, Duration::from_millis(40), move || {
                let Some(app) = weak.upgrade() else {
                    return;
                };
                let received = {
                    let task = app.image_task.borrow();
                    task.as_ref()
                        .and_then(|(revision, receiver)| match receiver.try_recv() {
                            Ok(result) => Some((*revision, result)),
                            Err(std::sync::mpsc::TryRecvError::Empty) => None,
                            Err(_) => {
                                Some((*revision, Err(tr!("图片处理未完成，请重新选择。").into())))
                            }
                        })
                };
                let Some((revision, result)) = received else {
                    return;
                };
                app.image_task_timer.stop();
                app.image_task.borrow_mut().take();
                if let Some(window) = app.main_window.borrow().as_ref() {
                    window.set_editor_image_loading(false);
                }
                if app.image_revision.get() != revision || app.editor.borrow().is_none() {
                    return;
                }
                let result = result.and_then(|image| {
                    app.store.media().install(&image.id, &image.png)?;
                    app.set_editor_image(Some(image.id))
                });
                if let Some(window) = app.main_window.borrow().as_ref() {
                    window.set_editor_error(result.err().unwrap_or_default().into());
                }
                app.collect_media();
            });
    }

    pub(super) fn close_image_preview(&self) {
        if let Some(window) = self.image_preview.borrow_mut().take() {
            let _ = window.hide();
        }
    }

    pub(super) fn preview_image(self: &Rc<Self>, image: slint::Image) {
        if image.size().width == 0 {
            return;
        }
        self.feedback_timer.stop();
        self.close_image_preview();
        let result = (|| -> AppResult<()> {
            let window = crate::ImagePreviewWindow::new().map_err(|e| e.to_string())?;
            window.set_picture(image);
            window.set_app_icon(self.icon.clone());
            window.set_dark_theme(
                self.resolved_dark_theme(self.library.borrow().settings.appearance),
            );
            let weak = Rc::downgrade(self);
            window.on_dismiss(move || {
                if let Some(app) = weak.upgrade() {
                    app.close_image_preview();
                }
            });
            let weak = Rc::downgrade(self);
            window.window().on_close_requested(move || {
                let weak = weak.clone();
                Timer::single_shot(Duration::ZERO, move || {
                    if let Some(app) = weak.upgrade() {
                        app.close_image_preview();
                    }
                });
                slint::CloseRequestResponse::HideWindow
            });
            window.show().map_err(|e| e.to_string())?;
            self.floating
                .apply_theme(window.window(), window.get_dark_theme());
            self.image_preview.replace(Some(window));
            Ok(())
        })();
        self.report(result, "");
    }
}
