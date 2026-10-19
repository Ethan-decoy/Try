//! Host policy for structured formula sessions shared by authoring and practice.
use super::*;

impl App {
    pub(super) fn with_math_ui(
        &self,
        practice: bool,
        update: impl FnOnce(crate::MathInputState<'_>, &slint::Window),
    ) {
        if practice {
            if let Some(session) = self.practice.borrow().as_ref() {
                update(
                    session.window.global::<crate::MathInputState>(),
                    session.window.window(),
                );
            }
        } else if let Some(window) = self.main_window.borrow().as_ref() {
            update(window.global::<crate::MathInputState>(), window.window());
        }
    }

    pub(super) fn close_structured_math_for(&self, practice: bool) {
        let owns = self
            .structured_math
            .borrow()
            .as_ref()
            .is_some_and(|(target, _)| matches!(target, MathTarget::Practice(_)) == practice);
        if owns {
            self.close_structured_math();
        }
    }

    pub(super) fn close_structured_math(&self) {
        let active = self.structured_math.borrow_mut().take();
        if let Some((target, _)) = active {
            self.with_math_ui(matches!(target, MathTarget::Practice(_)), |ui, _| {
                ui.set_open(false);
                ui.set_practice(false);
                ui.set_keypad_expanded(false);
                ui.set_formula(crate::MathInputView::default());
                ui.set_catalog(ModelRc::default());
                ui.set_error("".into());
            });
        }
    }

    pub(super) fn dismiss_structured_math(self: &Rc<Self>) {
        let practice = self
            .structured_math
            .borrow()
            .as_ref()
            .is_some_and(|(target, _)| matches!(target, MathTarget::Practice(_)));
        if practice {
            let mut expanded = false;
            self.with_math_ui(true, |ui, _| {
                expanded = ui.get_keypad_expanded();
                ui.set_keypad_expanded(false);
                ui.set_focus_request(ui.get_focus_request() + 1);
            });
            if !expanded {
                self.close_practice();
            }
        } else {
            self.close_structured_math();
        }
    }

    pub(super) fn bind_structured_math(self: &Rc<Self>, ui: crate::MathInputState<'_>) {
        let weak = Rc::downgrade(self);
        ui.on_committed(move |text| {
            if let Some(app) = weak.upgrade() {
                app.edit_structured_math(|edit| {
                    edit.insert(text.as_str());
                    false
                });
            }
        });
        let weak = Rc::downgrade(self);
        ui.on_key(move |text, control, shift, alt| {
            weak.upgrade().is_some_and(|app| {
                app.edit_structured_math(|edit| edit.key(text.as_str(), control, shift, alt))
            })
        });
        let weak = Rc::downgrade(self);
        ui.on_seek(move |x, y, extend| {
            if let Some(app) = weak.upgrade() {
                app.edit_structured_math(|edit| {
                    edit.pointer(x, y, extend);
                    false
                });
            }
        });
        let weak = Rc::downgrade(self);
        ui.on_keypad(move |token, extend| {
            if let Some(app) = weak.upgrade() {
                app.edit_structured_math(|edit| {
                    edit.keypad(token.as_str(), extend);
                    false
                });
            }
        });
        let weak = Rc::downgrade(self);
        ui.on_choose_category(move |index| {
            if let Some(app) = weak.upgrade() {
                let active = app.structured_math.borrow();
                if let Some((target, _)) = active.as_ref() {
                    app.with_math_ui(matches!(target, MathTarget::Practice(_)), |ui, _| {
                        ui.set_category(index);
                        ui.set_catalog(crate::structured_math::catalog(index));
                    });
                }
            }
        });
        let weak = Rc::downgrade(self);
        ui.on_refresh_view(move || {
            if let Some(app) = weak.upgrade() {
                app.refresh_structured_math();
            }
        });
        let weak = Rc::downgrade(self);
        ui.on_apply(move || {
            if let Some(app) = weak.upgrade() {
                let practice = app
                    .structured_math
                    .borrow()
                    .as_ref()
                    .is_some_and(|(target, _)| matches!(target, MathTarget::Practice(_)));
                if practice {
                    app.with_math_ui(true, |ui, _| {
                        ui.set_keypad_expanded(false);
                        ui.set_focus_request(ui.get_focus_request() + 1);
                    });
                } else {
                    app.apply_current_math();
                }
            }
        });
        let weak = Rc::downgrade(self);
        ui.on_dismiss(move || {
            if let Some(app) = weak.upgrade() {
                app.dismiss_structured_math();
            }
        });
    }

    pub(super) fn edit_structured_math(
        &self,
        edit: impl FnOnce(&mut crate::structured_math::Session) -> bool,
    ) -> bool {
        let consumed = self
            .structured_math
            .borrow_mut()
            .as_mut()
            .is_some_and(|(_, session)| edit(session));
        if let Some((MathTarget::Practice(id), edit)) = self.structured_math.borrow().as_ref()
            && let Some(practice) = self
                .practice
                .borrow()
                .as_ref()
                .filter(|session| session.diagnostic_id == *id)
        {
            practice.window.set_answer_text(edit.source().into());
        }
        self.refresh_structured_math();
        consumed
    }

    pub(super) fn refresh_structured_math(&self) {
        let active = self.structured_math.borrow();
        let Some((target, session)) = active.as_ref() else {
            return;
        };
        let dark = self.resolved_dark_theme(self.library.borrow().settings.appearance);
        self.with_math_ui(
            matches!(target, MathTarget::Practice(_)),
            |ui, window| match session.view(window.scale_factor(), dark) {
                Ok(view) => {
                    ui.set_formula(view);
                    ui.set_complete(session.complete().is_ok());
                    ui.set_error(session.error().into());
                }
                Err(error) => ui.set_error(error.into()),
            },
        );
    }

    pub(super) fn apply_content_math(&self) -> bool {
        if self
            .structured_math
            .borrow()
            .as_ref()
            .is_some_and(|(target, _)| matches!(target, MathTarget::Practice(_)))
        {
            return true;
        }
        self.apply_current_math()
    }

    pub(super) fn apply_current_math(&self) -> bool {
        let result = self
            .structured_math
            .borrow()
            .as_ref()
            .map(|(_, session)| session.complete());
        let Some(result) = result else {
            return true;
        };
        match result.and_then(|latex| self.apply_structured_math(&latex)) {
            Ok(()) => true,
            Err(error) => {
                if let Some((target, _)) = self.structured_math.borrow().as_ref() {
                    self.with_math_ui(matches!(target, MathTarget::Practice(_)), |ui, _| {
                        ui.set_error(error.into())
                    });
                }
                false
            }
        }
    }

    pub(super) fn open_content_math(self: &Rc<Self>) {
        self.flush_math();
        let target = (|| {
            let main = self.main_window.borrow();
            let index = usize::try_from(main.as_ref()?.get_math_active_field()).ok()?;
            let fields = self.math_fields.borrow();
            let field = fields.get(index)?;
            let node = field.selected();
            // Keep mixed CJK prose on the existing native text surface.
            if node.plain().chars().any(|c| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}')) {
                return None;
            }
            let seed = if field.error().is_some() || node.is_text() {
                crate::structured_math::Seed::Typing(field.input())
            } else {
                crate::structured_math::Seed::Latex(node.tex())
            };
            Some((
                MathTarget::Content {
                    index,
                    original: field.root.clone(),
                    input: field.input(),
                    path: field.selection.clone(),
                },
                seed,
            ))
        })();
        if let Some((target, seed)) = target {
            self.open_structured_math(target, seed, true);
        } else if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_math_error(tr!("请先选中其中的纯公式，再打开公式输入。").into());
        }
    }

    pub(super) fn open_practice_math(self: &Rc<Self>) {
        self.start_practice_math(true);
    }

    pub(super) fn start_practice_math(&self, expanded: bool) {
        let target = self
            .practice
            .borrow()
            .as_ref()
            .filter(|session| {
                !session.answered
                    && !session.prepared.knowledge_only
                    && session.prepared.mode == AnswerMode::Input
            })
            .map(|session| {
                let input = session.window.get_answer_text().to_string();
                let seed = if input.contains('\\') {
                    crate::structured_math::Seed::Latex(input)
                } else {
                    crate::structured_math::Seed::Typing(input)
                };
                (MathTarget::Practice(session.diagnostic_id), seed)
            });
        if let Some((target, seed)) = target {
            self.open_structured_math(target, seed, expanded);
        }
    }

    pub(super) fn open_structured_math(
        &self,
        target: MathTarget,
        seed: crate::structured_math::Seed,
        expanded: bool,
    ) {
        if let Some((target, _)) = self.structured_math.borrow().as_ref() {
            self.with_math_ui(matches!(target, MathTarget::Practice(_)), |ui, _| {
                ui.set_keypad_expanded(true);
                ui.set_focus_request(ui.get_focus_request() + 1);
            });
            return;
        }

        match crate::structured_math::Session::new(seed) {
            Ok(session) => {
                let practice = matches!(target, MathTarget::Practice(_));
                self.structured_math.replace(Some((target, session)));
                self.with_math_ui(practice, |ui, _| {
                    ui.set_categories(crate::structured_math::categories());
                    ui.set_category(0);
                    ui.set_catalog(crate::structured_math::catalog(0));
                });
                self.refresh_structured_math();
                self.with_math_ui(practice, |ui, _| {
                    ui.set_practice(practice);
                    ui.set_keypad_expanded(expanded);
                    ui.set_open(true);
                    ui.set_focus_request(ui.get_focus_request() + 1);
                });
            }
            Err(error) => {
                if matches!(target, MathTarget::Practice(_)) {
                    if let Some(session) = self.practice.borrow().as_ref() {
                        session.window.set_input_error(error.into());
                    }
                } else if let Some(window) = self.main_window.borrow().as_ref() {
                    window.set_math_error(error.into());
                }
            }
        }
    }

    pub(super) fn apply_structured_math(&self, latex: &str) -> AppResult<()> {
        let active = self.structured_math.borrow();
        let Some((target, _)) = active.as_ref() else {
            return Err(tr!("窗口已关闭。").into());
        };
        let practice_target = matches!(target, MathTarget::Practice(_));
        match target {
            MathTarget::Content {
                index,
                original,
                input,
                path,
            } => {
                let mut fields = self.math_fields.borrow_mut();
                let field = fields
                    .get_mut(*index)
                    .filter(|field| {
                        field.root == *original
                            && field.input() == *input
                            && field.selection == *path
                    })
                    .ok_or_else(|| tr!("原内容已改变，请关闭公式输入后重新打开。").to_owned())?;
                field.select(path.clone());
                field.replace_formula(latex)?;
                self.math_pending.set(false);
            }
            MathTarget::Practice(id) => {
                let mut practice = self.practice.borrow_mut();
                let session = practice
                    .as_mut()
                    .filter(|session| session.diagnostic_id == *id && !session.answered)
                    .ok_or_else(|| tr!("原内容已改变，请关闭公式输入后重新打开。").to_owned())?;
                let input = crate::structured_math::plain(latex);
                session.input_history.borrow_mut().record(&input);
                session.window.set_answer_text(input.into());
                session.window.set_input_error("".into());
            }
        }
        drop(active);
        self.close_structured_math();
        self.refresh_math(true);
        self.refresh_answer_preview();
        if practice_target && let Some(session) = self.practice.borrow().as_ref() {
            session.window.invoke_focus_answer();
        }
        Ok(())
    }
}
