//! Practice-window creation, presentation and shutdown.
mod answer;
mod bindings;
mod timing;

pub(super) use answer::problem_context;
use answer::sync_answer_position;
pub(super) use timing::CloseCountdown;

use super::*;

impl App {
    pub fn show_practice(self: &Rc<Self>) -> AppResult<()> {
        self.show_practice_with_origin(PracticeOrigin::Manual)
    }

    pub(super) fn show_practice_with_origin(
        self: &Rc<Self>,
        origin: PracticeOrigin,
    ) -> AppResult<()> {
        let background = !self.library.borrow().settings.focus_on_reminder;
        if origin == PracticeOrigin::Reminder && !(self.reminder_environment.get())(background) {
            log::debug!(target: "try::app", "event=reminder_deferred reason=environment");
            self.arm_reminder();
            return Ok(());
        }
        if origin == PracticeOrigin::Reminder
            && (self.main_window.borrow().is_some() || {
                let library = self.library.borrow();
                let now = chrono::Local::now();
                library.settings.pause.active(now.timestamp())
                    || library.settings.quiet_hours.contains(&now)
            })
        {
            log::debug!(target:"try::app","event=reminder_deferred reason=paused");
            self.arm_reminder();
            return Ok(());
        }
        let existing = self
            .practice
            .borrow()
            .as_ref()
            .map(|session| session.diagnostic_id);
        if let Some(id) = existing {
            if origin == PracticeOrigin::Manual {
                self.begin_practice_interaction(id, LearningOrigin::Manual);
            }
            let practice = self.practice.borrow();
            let session = practice.as_ref().unwrap();
            log::debug!(target: "try::app",
                "event=practice_reopened card={} origin={origin:?}",
                session.diagnostic_id
            );
            if origin == PracticeOrigin::Manual {
                if session.input_guard.is_some() {
                    FloatingWindow::set_mouse_passthrough(session.window.window(), false)?;
                }
                session.window.window().set_minimized(false);
                FloatingWindow::activate(session.window.window());
                session.window.invoke_focus_answer();
            }
            return session.window.show().map_err(|error| error.to_string());
        }
        let question = if self.knowledge_only.get() {
            practice::select_question(
                &self.library.borrow(),
                self.previous_question.borrow().as_ref(),
                &mut rand::rng(),
            )
        } else {
            let selected = self.learner.borrow_mut().choose(
                &self.library.borrow(),
                self.previous_question.borrow().as_ref(),
                chrono::Utc::now().timestamp(),
                &mut rand::rng(),
            );
            match selected {
                Ok(question) => question,
                Err(error) => {
                    if origin == PracticeOrigin::Manual {
                        self.notify(
                            &tr!("练习记录暂不可用，将使用普通选题：{error}", error = error),
                            true,
                        );
                    } else {
                        log::warn!(target: "try::app", "event=reminder_learning_fallback reason={error}");
                    }
                    practice::select_question(
                        &self.library.borrow(),
                        self.previous_question.borrow().as_ref(),
                        &mut rand::rng(),
                    )
                }
            }
        }
        .ok_or(tr!("请先在主窗口开启要练习的题目。"))?;
        let mode = self
            .library
            .borrow()
            .card_answer_mode(&question.key.pack_id, &question.key.card_id);
        let prepared = if self.knowledge_only.get() {
            PreparedQuestion::knowledge(&question.card)
        } else {
            PreparedQuestion::new(&question.card, mode, &mut rand::rng())?
        };
        // Resolve media before opening a session or recording a review token.
        let question_image = prepared
            .prompt_image
            .as_deref()
            .map(|id| self.store.media().load(id))
            .transpose()?;
        if origin == PracticeOrigin::Manual {
            self.close_main();
        }
        let awaiting_interaction = origin == PracticeOrigin::Reminder && background;
        let _creation = if awaiting_interaction {
            self.floating.passive_creation()
        } else {
            self.floating.practice_creation()
        };
        let diagnostic_id = self.next_practice_id.get();
        self.next_practice_id.set(diagnostic_id.wrapping_add(1));
        let window = PracticeWindow::new().map_err(|error| error.to_string())?;
        window.set_dark_theme(self.resolved_dark_theme(self.library.borrow().settings.appearance));
        {
            let library = self.library.borrow();
            window.set_show_pack_tags(library.settings.show_pack_tags);
            window.set_pack_tag(
                library
                    .packs
                    .iter()
                    .find(|pack| pack.id == question.key.pack_id)
                    .map(|pack| pack.tag.trim())
                    .unwrap_or_default()
                    .into(),
            );
            window.set_tag_on_right(matches!(
                library.settings.popup_corner,
                PopupCorner::BottomLeft | PopupCorner::TopLeft
            ));
        }
        log::debug!(target: "try::app",
            "event=practice_created card={diagnostic_id} origin={origin:?} mode={:?} knowledge_only={}",
            prepared.mode,
            prepared.knowledge_only
        );
        window.set_app_icon(self.icon.clone());
        window.set_answer_mode(prepared.mode.index());
        window.set_knowledge_only(prepared.knowledge_only);
        window.set_has_prompt(prepared.has_prompt());
        window.set_prompt(prepared.prompt.text.clone().into());
        window.set_standard_answer(prepared.answer.text.clone().into());
        if let Some(image) = question_image {
            window.set_question_image(image.image);
            window.set_question_image_width(image.width as f32);
            window.set_question_image_height(image.height as f32);
        }
        formula_presentation::apply_practice(&window, &prepared, window.get_pixel_scale());
        let native_ready = self.bind_practice_window(&window, diagnostic_id, awaiting_interaction);
        self.reminder_timer.stop();
        self.previous_question.replace(Some(question.key.clone()));
        self.practice.replace(Some(PracticeSession {
            diagnostic_id,
            window: window.clone_strong(),
            question,
            prepared,
            answered: false,
            awaiting_interaction,
            input_guard: None,
            review_token: None,
            input_history: RefCell::default(),
            last_attempt: None,
            close_countdown: CloseCountdown::default(),
        }));
        if let Err(error) = window.show() {
            self.practice.borrow_mut().take();
            self.arm_reminder();
            return Err(error.to_string());
        }
        if let Some(session) = self.practice.borrow_mut().as_mut()
            && !session.prepared.knowledge_only
            && !awaiting_interaction
        {
            session.review_token = self.learner.borrow_mut().presented(
                &session.question,
                session.prepared.mode,
                chrono::Utc::now().timestamp(),
                match origin {
                    PracticeOrigin::Manual => LearningOrigin::Manual,
                    PracticeOrigin::Reminder => LearningOrigin::Reminder,
                },
            );
        }
        log::debug!(target: "try::app", "event=practice_shown card={diagnostic_id}");
        let weak = Rc::downgrade(self);
        let weak_window = window.as_weak();
        slint::spawn_local(async move {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let ready = window
                .window()
                .winit_window()
                .await
                .map_err(|e| e.to_string());
            let Some(app) = weak.upgrade() else {
                return;
            };
            let result = ready.and_then(|native| {
                let mut practice = app.practice.borrow_mut();
                let Some(session) = practice
                    .as_mut()
                    .filter(|session| session.diagnostic_id == diagnostic_id)
                else {
                    return Ok(());
                };
                if awaiting_interaction {
                    FloatingWindow::prepare_frame(session.window.window());
                    session.input_guard = Some(ReminderInputGuard::install(&native)?);
                }
                native_ready.set(true);
                app.place_practice(session);
                if !session.awaiting_interaction {
                    session.window.invoke_focus_answer();
                }
                if origin == PracticeOrigin::Manual
                    || (!session.prepared.knowledge_only
                        && app.library.borrow().settings.focus_on_reminder)
                {
                    FloatingWindow::activate(session.window.window());
                }
                Ok(())
            });
            if let Err(error) = result {
                let Some(session) = app
                    .practice
                    .borrow_mut()
                    .take_if(|session| session.diagnostic_id == diagnostic_id)
                else {
                    return;
                };
                let _ = session.window.hide();
                app.arm_reminder();
                if origin == PracticeOrigin::Manual {
                    app.notify(&error, true);
                } else {
                    log::warn!(target: "try::app", "event=reminder_failed reason={error}");
                }
            }
        })
        .map_err(|error| {
            let _ = window.hide();
            self.practice
                .borrow_mut()
                .take_if(|session| session.diagnostic_id == diagnostic_id);
            self.arm_reminder();
            error.to_string()
        })?;
        if window.get_knowledge_only() {
            self.arm_card_close();
        }
        Ok(())
    }

    pub(super) fn place_practice(&self, session: &PracticeSession) {
        self.floating.place(session.window.window());
        if let Some(on_right) = FloatingWindow::tag_on_right(session.window.window()) {
            session.window.set_tag_on_right(on_right);
        }
    }

    pub(super) fn close_practice(self: &Rc<Self>) {
        self.close_structured_math_for(true);
        self.close_image_preview();

        self.cancel_card_close();
        let session = self.practice.borrow_mut().take();
        if let Some(session) = session {
            log::debug!(target: "try::app",
                "event=practice_closed card={} answered={} position={:?}",
                session.diagnostic_id,
                session.answered,
                FloatingWindow::position(session.window.window())
            );
            if self.library.borrow().settings.remember_position
                && let Some(position) = self.floating.moved_position(session.window.window())
            {
                let result = self.commit(|library| {
                    library.settings.popup_position = Some(position);
                    Ok(())
                });
                self.report(result, "");
            }
            let _ = session.window.hide();
        }
        self.arm_reminder();
        self.resume_ready_update();
    }
}
