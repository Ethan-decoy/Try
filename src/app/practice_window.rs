//! Practice presentation, native input, answer feedback and reminder lifecycle.
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
        let weak = Rc::downgrade(self);
        window.on_preview_question_image(move || {
            if let Some(app) = weak.upgrade() {
                let image = app
                    .practice
                    .borrow()
                    .as_ref()
                    .map(|s| s.window.get_question_image());
                if let Some(image) = image {
                    app.preview_image(image);
                }
            }
        });
        formula_presentation::apply_practice(&window, &prepared, window.get_pixel_scale());
        let weak = Rc::downgrade(self);
        window.on_rasterize_formulas(move |scale| {
            if let Some(app) = weak.upgrade() {
                if let Some(session) = app.practice.borrow().as_ref() {
                    // This binding tracks asynchronous monitor DPI updates.
                    formula_presentation::apply_practice(&session.window, &session.prepared, scale);
                }
                app.refresh_answer_preview();
            }
        });
        let weak_window = window.as_weak();
        let weak_app = Rc::downgrade(self);
        let native_ready = Rc::new(Cell::new(!awaiting_interaction));
        let reshape_ready = native_ready.clone();
        let applied_theme = Cell::new(None);
        let floating = self.floating.clone();
        window.window().on_winit_window_event(move |window, event| {
            crate::window_repaint::prepare(window, event);
            if matches!(
                event,
                WindowEvent::Focused(true)
                    | WindowEvent::MouseInput {
                        state: slint::winit_030::winit::event::ElementState::Pressed,
                        ..
                    }
            ) && let Some(app) = weak_app.upgrade()
            {
                // Start before Slint dispatches the click, so the first click
                // can answer a choice card as well as activate the window.
                app.begin_practice_interaction(diagnostic_id, LearningOrigin::Reminder);
            }
            if matches!(
                event,
                WindowEvent::RedrawRequested | WindowEvent::ThemeChanged(_)
            ) && let Some(app) = weak_app.upgrade()
            {
                app.apply_native_theme(
                    window,
                    &applied_theme,
                    matches!(event, WindowEvent::ThemeChanged(_)),
                );
            }
            match event {
                WindowEvent::Resized(size) => log::debug!(target: "try::app",
                    "event=practice_resized card={diagnostic_id} width={} height={}",
                    size.width,
                    size.height
                ),
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    log::debug!(target: "try::app",
                        "event=practice_scale_changed card={diagnostic_id} scale={scale_factor}"
                    )
                }
                WindowEvent::Focused(focused) => log::debug!(target: "try::app",
                    "event=practice_focus_changed card={diagnostic_id} focused={focused}"
                ),
                WindowEvent::Moved(position) => log::trace!(target: "try::app",
                    "event=practice_moved card={diagnostic_id} x={} y={}",
                    position.x,
                    position.y
                ),
                _ => {}
            }
            if matches!(
                event,
                WindowEvent::Resized(_)
                    | WindowEvent::ScaleFactorChanged { .. }
                    | WindowEvent::Moved(_)
            ) {
                let reshape = !matches!(event, WindowEvent::Moved(_));
                let weak_window = weak_window.clone();
                let ready = reshape_ready.clone();
                let floating = floating.clone();
                Timer::single_shot(Duration::ZERO, move || {
                    if !ready.get() {
                        return;
                    }
                    if let Some(window) = weak_window.upgrade() {
                        if reshape {
                            floating.reshape_and_clamp(window.window());
                        }
                        if let Some(on_right) = FloatingWindow::tag_on_right(window.window()) {
                            window.set_tag_on_right(on_right);
                        }
                    }
                });
            }
            EventResult::Propagate
        });
        let weak = Rc::downgrade(self);
        window.on_answer_edited(move |input| {
            if let Some(app) = weak.upgrade() {
                app.edit_answer_input(input.as_str());
            }
        });
        let weak = Rc::downgrade(self);
        self.bind_structured_math(window.global::<crate::MathInputState>());
        window.on_open_structured_entry(move || {
            if let Some(app) = weak.upgrade() {
                app.open_practice_math();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_answer_position_changed(move |input, anchor, caret| {
            if let Some(app) = weak.upgrade()
                && let Some(session) = app.practice.borrow().as_ref()
                && session.window.get_answer_text() == input
            {
                sync_answer_position(session, anchor, caret, false);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_input_command(move |command| {
            if let Some(app) = weak.upgrade() {
                app.inline_answer_command(command.as_str());
            }
        });
        let weak = Rc::downgrade(self);
        window.on_submit_answer(move |input| {
            if let Some(app) = weak.upgrade() {
                app.submit_answer(input.as_str());
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_answer(move |index| {
            if let Some(app) = weak.upgrade() {
                app.choose_answer(index);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_dismiss(move || {
            if let Some(app) = weak.upgrade() {
                app.close_practice();
            }
        });
        let weak = Rc::downgrade(self);
        window.window().on_close_requested(move || {
            let weak = weak.clone();
            Timer::single_shot(Duration::ZERO, move || {
                if let Some(app) = weak.upgrade() {
                    app.close_practice();
                }
            });
            slint::CloseRequestResponse::HideWindow
        });
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

    pub(super) fn begin_practice_interaction(&self, id: u64, origin: LearningOrigin) {
        let mut practice = self.practice.borrow_mut();
        let Some(session) = practice
            .as_mut()
            .filter(|session| session.diagnostic_id == id && session.awaiting_interaction)
        else {
            return;
        };
        session.awaiting_interaction = false;
        if !session.prepared.knowledge_only {
            session.review_token = self.learner.borrow_mut().presented(
                &session.question,
                session.prepared.mode,
                chrono::Utc::now().timestamp(),
                origin,
            );
        }
        session.window.invoke_focus_answer();
    }

    pub(super) fn edit_answer_input(self: &Rc<Self>, input: &str) {
        self.close_structured_math_for(true);
        let math_hint = {
            let mut practice = self.practice.borrow_mut();
            let Some(session) = practice.as_mut().filter(|session| {
                !session.answered
                    && session.prepared.mode == AnswerMode::Input
                    && !session.prepared.knowledge_only
            }) else {
                return;
            };
            session.input_history.borrow_mut().record(input);
            session.window.set_input_error("".into());
            session.window.set_answer_text(input.into());
            session.window.set_input_preview(Default::default());
            session.question.card.answer_latex.is_some()
                || math_answer::recognize(&session.question.card.answer, false).is_some()
        };
        if math_answer::starts_structured_edit(input, math_hint) {
            self.start_practice_math(false);
        }
    }

    pub(super) fn inline_answer_command(&self, command: &str) {
        if !matches!(command, "undo" | "redo") {
            return;
        }
        let selection = {
            let mut practice = self.practice.borrow_mut();
            let Some(session) = practice.as_mut().filter(|session| !session.answered) else {
                return;
            };
            let Some((input, selection)) =
                session.input_history.borrow_mut().step(command == "redo")
            else {
                return;
            };
            session.window.set_answer_text(input.into());
            session.window.set_input_error("".into());

            selection
        };

        self.refresh_answer_preview();
        if let Some(session) = self.practice.borrow().as_ref() {
            sync_answer_position(
                session,
                selection.anchor as i32,
                selection.focus as i32,
                true,
            );
            session.window.invoke_focus_answer();
        }
    }

    pub(super) fn refresh_answer_preview(&self) {
        let practice = self.practice.borrow();
        let Some(session) = practice.as_ref().filter(|session| {
            session.prepared.mode == AnswerMode::Input && !session.prepared.knowledge_only
        }) else {
            return;
        };
        let window = &session.window;
        let preview = math_answer::recognize_inline(
            window.get_answer_text().as_str(),
            session.question.card.answer_latex.is_some(),
        )
        .and_then(|node| {
            formula::render_node(&node, 22, window.get_pixel_scale(), (232, 232, 232)).ok()
        })
        .map(|rendered| formula_presentation::option(window.get_answer_text(), rendered))
        .unwrap_or_default();
        window.set_input_preview(preview);
    }

    pub(super) fn submit_answer(self: &Rc<Self>, input: &str) {
        let structured = self
            .structured_math
            .borrow()
            .as_ref()
            .is_some_and(|(target, _)| matches!(target, MathTarget::Practice(_)));
        if structured {
            if self.apply_current_math() {
                let answer = self
                    .practice
                    .borrow()
                    .as_ref()
                    .map(|session| session.window.get_answer_text().to_string());
                if let Some(answer) = answer {
                    self.submit_answer(&answer);
                }
            }
            return;
        }
        if input.trim().is_empty() {
            return;
        }
        let correct = self
            .practice
            .borrow()
            .as_ref()
            .filter(|session| {
                !session.prepared.knowledge_only && session.prepared.mode == AnswerMode::Input
            })
            .map(|session| session.question.card.accepts(input));
        if let Some(correct) = correct {
            if !correct {
                let practice = self.practice.borrow();
                if let Some(session) = practice.as_ref() {
                    let math_hint = session.question.card.answer_latex.is_some()
                        || math_answer::recognize(&session.question.card.answer, false).is_some();
                    if let Some(error) = math_answer::incomplete_error(input, math_hint) {
                        session.window.set_input_error(error.into());
                        return;
                    }
                }
            }
            self.finish_answer(correct, None);
        }
    }

    pub(super) fn choose_answer(self: &Rc<Self>, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let correct = self
            .practice
            .borrow()
            .as_ref()
            .and_then(|session| session.prepared.accepts_choice(index));
        if let Some(correct) = correct {
            self.finish_answer(correct, Some(index));
        }
    }

    pub(super) fn finish_answer(self: &Rc<Self>, correct: bool, choice: Option<usize>) {
        if self
            .practice
            .borrow()
            .as_ref()
            .is_none_or(|session| session.awaiting_interaction)
        {
            return;
        }

        // Render static answer feedback only when it is needed.
        self.refresh_answer_preview();
        let review_token = {
            let mut session = self.practice.borrow_mut();
            let Some(session) = session.as_mut() else {
                return;
            };
            if session.answered {
                return;
            }
            session.answered = true;
            session
                .window
                .set_selected_option(choice.map_or(-1, |index| index as i32));
            session.window.set_feedback(if correct { 1 } else { 2 });
            // A disabled option must not keep the focus needed by Enter/Esc.
            session.window.invoke_focus_answer();
            session.review_token.take()
        };
        if correct {
            self.arm_card_close();
        } else {
            self.feedback_timer.stop();
        }
        if let Some(token) = review_token {
            let outcome = self.learner.borrow_mut().answer(
                &self.library.borrow(),
                &token,
                correct,
                chrono::Utc::now().timestamp(),
            );
            if let Err(error) = outcome {
                self.notify(&tr!("练习结果未能保存：{error}", error = error), true);
            }
        }
    }

    pub(super) fn arm_card_close(self: &Rc<Self>) {
        let seconds = {
            let practice = self.practice.borrow();
            let Some(session) = practice.as_ref() else {
                return;
            };
            let library = self.library.borrow();
            if session.prepared.knowledge_only {
                library.settings.knowledge_seconds
            } else {
                library.settings.feedback_seconds
            }
        };
        let duration = Duration::from_secs(seconds.into());
        let weak = Rc::downgrade(self);
        self.feedback_timer
            .start(TimerMode::SingleShot, duration, move || {
                if let Some(app) = weak.upgrade() {
                    app.close_practice();
                }
            });
    }

    pub(super) fn close_practice(self: &Rc<Self>) {
        self.close_structured_math_for(true);
        self.close_image_preview();

        self.feedback_timer.stop();
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

    pub(super) fn arm_reminder(self: &Rc<Self>) {
        self.reminder_timer.stop();
        let library = self.library.borrow();
        let now = chrono::Local::now();
        if library.settings.pause.active(now.timestamp())
            || library.enabled_card_count() == 0
            || self.main_window.borrow().is_some()
            || self.practice.borrow().is_some()
        {
            return;
        }
        let due = library
            .settings
            .quiet_hours
            .next_reminder(now, library.settings.interval_minutes);
        self.wait_for_reminder(due);
    }

    pub(super) fn wait_for_reminder(self: &Rc<Self>, due: chrono::DateTime<chrono::Local>) {
        // Slint stores timer durations as whole milliseconds. Round upwards
        // so a fractional deadline cannot fire just before quiet hours end.
        let duration =
            (due - chrono::Local::now()).to_std().unwrap_or_default() + Duration::from_millis(1);
        log::debug!(target: "try::app",
            "event=reminder_armed delay_seconds={}",
            duration.as_secs()
        );
        let weak = Rc::downgrade(self);
        self.reminder_timer
            .start(TimerMode::SingleShot, duration, move || {
                if let Some(app) = weak.upgrade() {
                    let active = !app
                        .library
                        .borrow()
                        .settings
                        .pause
                        .active(chrono::Utc::now().timestamp());
                    if !active
                        || app.main_window.borrow().is_some()
                        || app.practice.borrow().is_some()
                    {
                        app.arm_reminder();
                        return;
                    }
                    // Recheck wall time and quiet hours when firing, including
                    // after sleep or a local clock adjustment.
                    if chrono::Local::now() < due {
                        app.wait_for_reminder(due);
                        return;
                    }
                    if let Err(error) = app.show_practice_with_origin(PracticeOrigin::Reminder) {
                        // Automatic reminders must never replace a passive card
                        // with a modal error dialog over another application.
                        log::warn!(target: "try::app", "event=reminder_failed reason={error}");
                        app.arm_reminder();
                    }
                }
            });
    }
}

fn sync_answer_position(session: &PracticeSession, anchor: i32, caret: i32, restore: bool) {
    let input = session.window.get_answer_text();
    let bound = |offset: i32| {
        let mut offset = (offset.max(0) as usize).min(input.len());
        while !input.as_str().is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    };
    let anchor = bound(anchor);
    let caret = bound(caret);
    session
        .input_history
        .borrow_mut()
        .set_selection(input.as_str(), anchor, caret);
    let previous = session.window.get_answer_position();
    session.window.set_answer_position(FormulaPosition {
        anchor: anchor as i32,
        caret: caret as i32,
        bytes: input.len() as i32,
        revision: previous.revision.wrapping_add(i32::from(restore)),
    });
}
