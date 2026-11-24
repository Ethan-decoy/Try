//! Answer input, grading, feedback and frozen problem evidence.
use super::*;
use r#try::problem_report::AnswerAttempt;

impl App {
    pub(in crate::app) fn begin_practice_interaction(&self, id: u64, origin: LearningOrigin) {
        let mut practice = self.practice.borrow_mut();
        let Some(session) = practice
            .as_mut()
            .filter(|session| session.diagnostic_id == id && session.awaiting_interaction)
        else {
            return;
        };
        session.awaiting_interaction = false;
        session.window.set_awaiting_interaction(false);
        if !session.prepared.knowledge_only {
            session.review_token = self.learner.borrow_mut().presented(
                &session.question,
                session.prepared.mode,
                chrono::Utc::now().timestamp(),
                origin,
            );
        }
        session.window.invoke_focus_practice();
    }

    pub(in crate::app) fn edit_answer_input(&self, input: &str) {
        let practice = self.practice.borrow();
        let Some(session) = practice.as_ref().filter(|session| {
            !session.answered
                && session.prepared.mode == AnswerMode::Input
                && !session.prepared.knowledge_only
                && !session.prepared.allows_formula_input()
        }) else {
            return;
        };
        session.input_history.borrow_mut().record(input);
        session.window.set_input_error("".into());
        session.window.set_answer_text(input.into());
    }

    pub(in crate::app) fn inline_answer_command(&self, command: &str) {
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

    pub(in crate::app) fn refresh_answer_preview(&self) {
        let practice = self.practice.borrow();
        let Some(session) = practice.as_ref() else {
            return;
        };
        let Some(latex) = session
            .last_attempt
            .as_ref()
            .and_then(|attempt| attempt.math_latex.as_deref())
        else {
            return;
        };
        // A declared text alias may have an otherwise empty formula slot.
        // Its literal label is already on the card.
        if !r#try::math_editor::from_latex(latex).is_ok_and(|node| node.complete()) {
            return;
        }
        let window = &session.window;
        let preview = formula::render(latex, 22, window.get_pixel_scale(), (232, 232, 232))
            .map(|rendered| formula_presentation::option(window.get_answer_text(), rendered))
            .unwrap_or_default();
        window.set_input_preview(preview);
    }

    pub(in crate::app) fn submit_answer(self: &Rc<Self>, input: &str) {
        let correct = {
            let mut practice = self.practice.borrow_mut();
            let Some(session) = practice
                .as_mut()
                .filter(|session| !session.answered && !session.awaiting_interaction)
            else {
                return;
            };
            let attempt = if session.prepared.allows_formula_input() {
                let active = self.structured_math.borrow();
                let Some((MathTarget::Practice(id), edit)) = active.as_ref() else {
                    return;
                };
                if *id != session.diagnostic_id {
                    return;
                }
                let Some(attempt) = prepare_formula_attempt(session, edit) else {
                    return;
                };
                attempt
            } else {
                AnswerAttempt {
                    input: input.to_owned(),
                    math_latex: None,
                    keypad_expanded: false,
                }
            };
            let input_text = attempt.input.clone();
            let result = assess_input(session, attempt);
            if result.is_some() {
                session.window.set_answer_text(input_text.into());
            }
            result
        };
        if let Some(correct) = correct {
            self.close_structured_math_for(true);
            self.finish_answer(correct, None);
        }
    }

    pub(in crate::app) fn choose_answer(self: &Rc<Self>, index: i32) {
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

    pub(in crate::app) fn finish_answer(self: &Rc<Self>, correct: bool, choice: Option<usize>) {
        if self
            .practice
            .borrow()
            .as_ref()
            .is_none_or(|session| session.awaiting_interaction)
        {
            return;
        }

        self.refresh_answer_preview();
        let review_token = {
            let mut session = self.practice.borrow_mut();
            let Some(session) = session.as_mut() else {
                return;
            };
            if !session.finish_answer_feedback(correct, choice) {
                return;
            }
            session.review_token.take()
        };
        if correct {
            self.arm_card_close();
        } else {
            self.cancel_card_close();
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
}

fn prepare_formula_attempt(
    session: &PracticeSession,
    edit: &crate::structured_math::Session,
) -> Option<AnswerAttempt> {
    let draft = edit.latex_source();
    if draft.trim().is_empty() {
        return None;
    }
    let ui = session.window.global::<crate::MathInputState>();
    let (latex, input) = match edit.complete() {
        Ok(latex) => {
            let input = crate::structured_math::answer_input(&latex);
            (latex, input)
        }
        Err(error) => {
            let Some(spelling) = session.question.card.edited_literal_spelling(&draft) else {
                ui.set_error(error.into());
                return None;
            };
            (draft, spelling.to_owned())
        }
    };
    Some(AnswerAttempt {
        input,
        math_latex: Some(latex),
        keypad_expanded: ui.get_keypad_expanded(),
    })
}

pub(super) fn sync_answer_position(
    session: &PracticeSession,
    anchor: i32,
    caret: i32,
    restore: bool,
) {
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

// Assessment is shared by the application and offscreen record regressions.
fn assess_input(session: &mut PracticeSession, attempt: AnswerAttempt) -> Option<bool> {
    if attempt.input.trim().is_empty()
        || session.answered
        || session.prepared.knowledge_only
        || session.prepared.mode != AnswerMode::Input
    {
        return None;
    }
    let card = &session.question.card;
    let correct = if session.prepared.allows_formula_input() {
        card.accepts_edited_formula(attempt.math_latex.as_deref()?)
    } else {
        card.accepts(&attempt.input)
    };
    session.last_attempt = Some(attempt);
    Some(correct)
}

impl PracticeSession {
    fn finish_answer_feedback(&mut self, correct: bool, choice: Option<usize>) -> bool {
        if self.answered || self.awaiting_interaction {
            return false;
        }
        self.answered = true;
        self.window
            .set_selected_option(choice.map_or(-1, |index| index as i32));
        self.window.set_correct_option(
            self.prepared
                .correct_index()
                .map_or(-1, |index| index as i32),
        );
        self.window.set_feedback(if correct { 1 } else { 2 });
        // A disabled option must not keep the focus needed by Enter/Esc.
        self.window.invoke_focus_answer();
        true
    }
}

pub(in crate::app) fn problem_context(
    session: &PracticeSession,
    math: Option<&crate::structured_math::Session>,
) -> r#try::problem_report::ProblemContext {
    use r#try::problem_report::{Outcome, ProblemContext};
    let window = &session.window;
    let math_error = window.global::<crate::MathInputState>().get_error();
    ProblemContext {
        card: session.question.card.clone(),
        mode: session.prepared.mode,
        knowledge_only: session.prepared.knowledge_only,
        displayed_prompt: session.prepared.prompt.clone(),
        displayed_answer: session.prepared.answer.clone(),
        options: session.prepared.options.clone(),
        selected_option: usize::try_from(window.get_selected_option()).ok(),
        outcome: if session.prepared.knowledge_only {
            Outcome::Knowledge
        } else {
            match window.get_feedback() {
                1 => Outcome::Correct,
                2 => Outcome::Incorrect,
                _ => Outcome::Unanswered,
            }
        },
        current_input: math.map_or_else(
            || window.get_answer_text().to_string(),
            |edit| edit.source(),
        ),
        current_math_latex: math.map(|edit| edit.latex_source()),
        keypad_expanded: window
            .global::<crate::MathInputState>()
            .get_keypad_expanded(),
        last_attempt: session.last_attempt.clone(),
        input_error: if math.is_some() && !math_error.is_empty() {
            math_error.to_string()
        } else {
            window.get_input_error().to_string()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{click, key, paint, software_window};
    use crate::{PracticeAction, ProblemRecordStatus};
    use slint::PhysicalSize;
    use slint::platform::Key;
    #[cfg(target_os = "windows")]
    use r#try::math_answer;
    use r#try::problem_report::{self, Outcome};
    use r#try::window_position::PracticeCardSize;

    const MIN_CARD_SIZE: PhysicalSize =
        PhysicalSize::new(PracticeCardSize::MIN.width, PracticeCardSize::MIN.height);

    fn session(window: &PracticeWindow) -> PracticeSession {
        let card = serde_json::from_str(
            r#"{
            "id":"card-1", "prompt":"ln(x^y) = ?", "answer":"yln(x)",
            "answer_latex":"y\\ln(x)"
        }"#,
        )
        .unwrap();
        let prepared = PreparedQuestion::new(&card, AnswerMode::Input, &mut rand::rng()).unwrap();
        window.set_prompt(prepared.prompt.text.clone().into());
        window.set_standard_answer(prepared.answer.text.clone().into());
        PracticeSession {
            diagnostic_id: 7,
            window: window.clone_strong(),
            question: Question {
                key: QuestionKey {
                    pack_id: "pack-1".into(),
                    card_id: "card-1".into(),
                },
                card,
            },
            prepared,
            answered: false,
            awaiting_interaction: false,
            input_guard: None,
            review_token: None,
            input_history: RefCell::default(),
            last_attempt: None,
            close_countdown: CloseCountdown::default(),
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn dismiss_retires_the_question_immediately_and_only_finishes_its_own_window() {
        let adapter = software_window();
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let library = Library::default();
        let floating = Rc::new(FloatingWindow::new(&library.settings));
        let app = App::new(store, library, floating).unwrap();
        let open = |id, motion| {
            let card = PracticeWindow::new().unwrap();
            let mut scene = session(&card);
            scene.diagnostic_id = id;
            app.bind_practice_window(&card, id, false);
            app.practice.replace(Some(scene));
            card.global::<crate::Theme>().set_motion(motion);
            card.show().unwrap();
            adapter.set_size(MIN_CARD_SIZE);
            paint(&adapter);
            card
        };
        let first = open(7, crate::MotionPreference::Full);
        assert!(first.animate_visibility(false));
        let opening_scale = first.get_frame_scale();
        first.invoke_dismiss();
        assert!(app.practice.borrow().is_none() && app.reminder_timer.running());
        assert_eq!(first.get_frame_scale(), opening_scale);
        app.hide_outgoing_practice();
        let second = open(8, crate::MotionPreference::Full);
        second.invoke_dismiss();
        first.invoke_dismiss_finished();
        assert_eq!(app.outgoing_practice.borrow().as_ref().unwrap().0, 8);
        second
            .global::<crate::Theme>()
            .set_motion(crate::MotionPreference::Reduced);
        paint(&adapter);
        assert!(app.outgoing_practice.borrow().is_none());

        let instant = open(9, crate::MotionPreference::Reduced);
        assert!(!instant.animate_visibility(false));
        instant.invoke_dismiss();
        assert!(app.practice.borrow().is_none() && app.outgoing_practice.borrow().is_none());
    }

    // AppKit requires a native tray on the process main thread. The card-only
    // menu test and preference round trip still run on every platform.
    #[test]
    #[cfg(not(target_os = "macos"))]
    fn practice_menu_locks_and_unlocks_the_open_card_and_saves_the_setting() {
        let adapter = software_window();
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let library = store.load().unwrap();
        let floating = Rc::new(FloatingWindow::new(&library.settings));
        let app = App::new(store, library, floating).unwrap();
        let card = PracticeWindow::new().unwrap();
        let practice = session(&card);
        app.bind_practice_menu(&card, practice.diagnostic_id);
        app.practice.replace(Some(practice));
        card.global::<crate::Theme>()
            .set_motion(crate::MotionPreference::Reduced);
        card.set_answer_text("draft".into());
        card.show().unwrap();
        adapter.set_size(MIN_CARD_SIZE);
        paint(&adapter);

        let anchor = (
            MIN_CARD_SIZE.width as f32 - 10.0,
            MIN_CARD_SIZE.height as f32 - 10.0,
        );
        card.invoke_open_context_menu(anchor.0, anchor.1);
        paint(&adapter);
        click(
            &card,
            &adapter,
            card.get_context_menu_left() + 24.0,
            card.get_context_menu_top() + 85.0,
        );
        assert!(!card.get_context_menu_open());
        assert!(card.get_card_locked());
        assert!(!card.get_move_enabled());
        assert!(app.store.load().unwrap().settings.practice_card_locked);

        card.invoke_open_context_menu(anchor.0, anchor.1);
        paint(&adapter);
        key(&card, &adapter, Key::End);
        key(&card, &adapter, Key::UpArrow);
        key(&card, &adapter, Key::Return);
        assert!(!card.get_context_menu_open());
        assert!(!card.get_card_locked());
        assert!(card.get_move_enabled());
        assert!(!app.store.load().unwrap().settings.practice_card_locked);
        assert_eq!(card.get_answer_text(), "draft");
        assert!(app.practice.borrow().is_some());
        card.hide().unwrap();
    }

    #[test]
    fn a_correct_stacked_choice_keeps_the_card_height_and_answer_position() {
        let adapter = software_window();
        let window = PracticeWindow::new().unwrap();
        let mut scene = session(&window);
        let card = serde_json::from_str(
            r#"{"prompt":"Density?","answer":"mass divided by volume","distractors":[{"text":"length divided by time"},{"text":"force divided by area"}]}"#,
        )
        .unwrap();
        scene.prepared =
            PreparedQuestion::new(&card, AnswerMode::MultipleChoice, &mut rand::rng()).unwrap();
        window.set_answer_mode(scene.prepared.mode.index());
        formula_presentation::apply_practice(&window, &scene.prepared, 1.0);
        window.show().unwrap();
        adapter.set_size(MIN_CARD_SIZE);
        paint(&adapter);
        assert!(window.get_stacked_options());
        let content_height = window.get_content_height();
        let answer_bottom = window.get_answer_footer_top();

        let selected = scene.prepared.correct_index().unwrap();
        assert!(scene.finish_answer_feedback(true, Some(selected)));
        paint(&adapter);
        assert_eq!(window.get_content_height(), content_height);
        assert_eq!(window.get_answer_footer_top(), answer_bottom);
        window.hide().unwrap();
    }

    #[test]
    fn a_missed_choice_reveals_the_prepared_option_in_its_original_position() {
        let adapter = software_window();
        let window = PracticeWindow::new().unwrap();
        let mut scene = session(&window);
        let card = serde_json::from_str(
            r#"{"prompt":"hesitate","answer":"pause","distractors":[{"text":"hurry"},{"text":"agree"}]}"#,
        )
        .unwrap();
        scene.prepared =
            PreparedQuestion::new(&card, AnswerMode::MultipleChoice, &mut rand::rng()).unwrap();
        window.set_answer_mode(scene.prepared.mode.index());
        formula_presentation::apply_practice(&window, &scene.prepared, 1.0);
        window
            .global::<crate::Theme>()
            .set_motion(crate::MotionPreference::Reduced);
        window.show().unwrap();
        adapter.set_size(MIN_CARD_SIZE);
        paint(&adapter);
        let correct = scene
            .prepared
            .options
            .iter()
            .position(|option| option.text == "pause")
            .unwrap();
        let chosen = (correct + 1) % 3;
        assert!(scene.finish_answer_feedback(false, Some(chosen)));
        assert_eq!(window.get_selected_option(), chosen as i32);
        assert_eq!(window.get_correct_option(), correct as i32);
        let pixels = paint(&adapter);
        let width = (window.get_answer_footer_width() - 16.0) / 3.0;
        let y = (window.get_answer_footer_top() - window.get_answer_row_height() + 6.0) as usize;
        let tile_fill = |index| {
            let x = (window.get_answer_footer_left() + (width + 8.0) * index as f32 + width / 2.0)
                as usize;
            let pixel = pixels.as_slice()[y * pixels.width() as usize + x];
            slint::Color::from_rgb_u8(pixel.r, pixel.g, pixel.b)
        };
        assert_eq!(
            tile_fill(correct),
            window.global::<crate::Theme>().get_correct_soft()
        );
        assert_eq!(
            tile_fill(chosen),
            window.global::<crate::Theme>().get_wrong_soft()
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn practice_input_uses_the_saved_format_when_typing_or_pasting() {
        use slint::platform::WindowEvent;

        let adapter = software_window();
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let library = Library::default();
        let code = library
            .packs
            .iter()
            .flat_map(|pack| &pack.cards)
            .find(|card| card.id.as_deref() == Some("cpp_unique_pointer"))
            .unwrap()
            .clone();
        let template = r#try::knowledge::KnowledgePack::from_json(include_bytes!(
            "../../../knowledge_packs/rust_template.json"
        ))
        .unwrap();
        let transpose = template
            .cards
            .iter()
            .find(|card| card.id.as_deref() == Some("option_transpose"))
            .unwrap();
        let hybridization = library
            .packs
            .iter()
            .flat_map(|pack| &pack.cards)
            .find(|card| card.id.as_deref() == Some("methane_hybridization"))
            .unwrap()
            .clone();
        let calculus = r#try::knowledge::KnowledgePack::from_json(include_bytes!(
            "../../../knowledge_packs/authoring_template.json"
        ))
        .unwrap();
        let reciprocal = calculus
            .cards
            .iter()
            .find(|card| card.id.as_deref() == Some("reciprocal_antiderivative"))
            .unwrap();
        let product_rule = calculus
            .cards
            .iter()
            .find(|card| card.id.as_deref() == Some("product_rule"))
            .unwrap();
        let mut literal = code.clone();
        literal.answer = "x_i".into();
        literal.aliases.clear();
        let mut formula = literal.clone();
        formula.answer_latex = Some("x_{i}".into());
        formula.aliases = vec!["transpose()".into()];
        let mut power = formula.clone();
        power.answer = "(u)^(n+m)".into();
        power.answer_latex = Some(r"\left(u\right)^{n+m}".into());
        power.aliases.clear();
        let mut subscript = power.clone();
        subscript.answer = "(u)_(i+1)".into();
        subscript.answer_latex = Some(r"\left(u\right)_{i+1}".into());
        let mut path = literal.clone();
        path.answer = r"src\main.rs".into();
        let mut path_with_formula_prompt = literal.clone();
        path_with_formula_prompt.prompt_latex = Some(r"f(x)=?".into());
        path_with_formula_prompt.answer = "src/main.rs".into();
        let mut literal_ratio = path_with_formula_prompt.clone();
        literal_ratio.answer = "m/s".into();
        let floating = Rc::new(FloatingWindow::new(&library.settings));
        let app = App::new(store, library, floating).unwrap();

        for (card, input, paste, structured, feedback) in [
            (&code, "std::unique_ptr", false, false, 1),
            (&code, "unique_ptr", false, false, 1),
            (&code, "unique_ptr", true, false, 1),
            (&code, "unique_{ptr}", true, false, 2),
            (transpose, "transpose()", false, false, 1),
            (&literal, "x_i", false, false, 1),
            (&literal, "x_{i}", true, false, 2),
            (&literal, "sqrt(", false, false, 2),
            (&path, r"src\main.rs", false, false, 1),
            (&path_with_formula_prompt, "src/main.rs", false, false, 1),
            (&literal_ratio, "m/s", false, false, 1),
            (&formula, "x_i", false, true, 1),
            (&formula, "x_{i}", true, true, 1),
            (&formula, "transpose()", false, true, 1),
            (&formula, "transpose()", true, true, 1),
            (&power, "(u)^n+m", false, true, 1),
            (&subscript, "(u)_i+1", false, true, 1),
            (&hybridization, "sp^3", false, true, 1),
            (&hybridization, "sp^3", true, true, 1),
            (reciprocal, "ln(abs(x))+C", false, true, 1),
            (reciprocal, "ln(abs(x))+C", true, true, 1),
            (product_rule, "u'v+uv'", false, true, 1),
        ] {
            let window = PracticeWindow::new().unwrap();
            let mut scene = session(&window);
            scene.prepared =
                PreparedQuestion::new(card, AnswerMode::Input, &mut rand::rng()).unwrap();
            scene.question.card = card.clone();
            window.set_formula_input_allowed(scene.prepared.allows_formula_input());
            window.set_prompt(scene.prepared.prompt.text.clone().into());
            window.set_standard_answer(card.answer.clone().into());
            app.bind_practice_window(&window, scene.diagnostic_id, false);
            app.practice.replace(Some(scene));
            app.prepare_practice_math();
            window.show().unwrap();
            adapter.set_size(MIN_CARD_SIZE);
            window.invoke_focus_answer();
            assert_eq!(
                window.get_input_math_mode(),
                structured,
                "before typing {input}"
            );
            let enter_text = |text: slint::SharedString| {
                window
                    .window()
                    .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
                window
                    .window()
                    .dispatch_event(WindowEvent::KeyReleased { text });
                paint(&adapter);
            };
            if paste {
                enter_text(input.into());
            } else {
                for ch in input.chars() {
                    enter_text(ch.to_string().into());
                }
            }
            assert_eq!(window.get_input_math_mode(), structured, "editing {input}");
            key(&window, &adapter, Key::Return);
            assert_eq!(window.get_feedback(), feedback, "submitting {input}");
            if !structured {
                assert_eq!(window.get_answer_text(), input, "submitted text: {input}");
                assert_eq!(window.get_input_preview().formula_width, 0.0);
            }
            app.cancel_card_close();
            window.hide().unwrap();
            app.practice.borrow_mut().take();
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn practice_slash_opens_a_fraction_before_typing_the_denominator() {
        use slint::platform::WindowEvent;

        let adapter = software_window();
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let library = Library::default();
        let floating = Rc::new(FloatingWindow::new(&library.settings));
        let app = App::new(store, library, floating).unwrap();

        let card = serde_json::from_value::<r#try::knowledge::Card>(serde_json::json!({
            "prompt": "sin(pi/6) = ?",
            "prompt_latex": r"\sin(\pi/6)=?",
            "answer": "1/2",
            "answer_latex": r"\frac{1}{2}",
        }))
        .unwrap();
        let mut with_alias = card.clone();
        with_alias.prompt = "Half of u?".into();
        with_alias.prompt_latex = None;
        with_alias.answer = "0.5u".into();
        with_alias.answer_latex = Some(r"\frac{1}{2}u".into());
        with_alias.aliases = vec!["u/2".into(), "u/二".into()];
        for (card, numerator, expected) in [
            (&card, "1", r"\frac{1}{2}"),
            (&with_alias, "u ", r"\frac{u}{2}"),
            (&with_alias, "u", r"\frac{u}{2}"),
            (&card, "(1)", r"\frac{1}{2}"),
        ] {
            let window = PracticeWindow::new().unwrap();
            let mut scene = session(&window);
            scene.prepared =
                PreparedQuestion::new(card, AnswerMode::Input, &mut rand::rng()).unwrap();
            scene.question.card = card.clone();
            window.set_formula_input_allowed(scene.prepared.allows_formula_input());
            formula_presentation::apply_practice(&window, &scene.prepared, 1.0);
            app.bind_practice_window(&window, scene.diagnostic_id, false);
            app.practice.replace(Some(scene));
            app.prepare_practice_math();
            window.show().unwrap();
            adapter.set_size(MIN_CARD_SIZE);
            window.invoke_focus_answer();
            let type_key = |text: &str| {
                window
                    .window()
                    .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
                window
                    .window()
                    .dispatch_event(WindowEvent::KeyReleased { text: text.into() });
                paint(&adapter);
            };

            for character in numerator.chars() {
                type_key(&character.to_string());
            }
            type_key("/");
            assert!(
                window.get_input_math_mode(),
                "the first slash after {numerator} stayed in the text field: {}",
                window.get_answer_text()
            );
            let math = window.global::<crate::MathInputState>();
            assert!(!math.get_keypad_expanded());
            {
                let active = app.structured_math.borrow();
                let (_, edit) = active.as_ref().unwrap();
                let draft = edit.latex_source();
                assert!(draft.starts_with(r"\frac"), "{draft}");
                assert!(edit.complete().is_err(), "the denominator is still blank");
            }
            key(&window, &adapter, Key::Return);
            assert_eq!(window.get_feedback(), 0);
            assert!(!math.get_error().is_empty());
            type_key("2");
            let completed = app
                .structured_math
                .borrow()
                .as_ref()
                .unwrap()
                .1
                .complete()
                .unwrap();
            assert!(
                math_answer::matches_formula(&completed, expected, &[]),
                "{numerator}/2: {completed}"
            );
            key(&window, &adapter, Key::Return);
            assert_eq!(window.get_feedback(), 1);
            app.cancel_card_close();
            window.hide().unwrap();
            app.practice.borrow_mut().take();
        }
    }

    #[test]
    fn submitted_math_is_captured_and_saved_through_the_record_menu() {
        let adapter = software_window();
        let directory = tempfile::tempdir().unwrap();
        for (index, (expression, correct, expanded)) in
            [("yln(x)", true, false), ("ln(x)", false, true)]
                .into_iter()
                .enumerate()
        {
            let store =
                Rc::new(Store::open(directory.path().join(format!("case_{index}"))).unwrap());
            let edit = crate::structured_math::Session::new(crate::structured_math::Seed::Typing(
                expression.into(),
            ))
            .unwrap();
            // The same editor export and Try conversion used before assessment.
            let latex = edit.latex_source();
            let input = crate::structured_math::answer_input(&edit.complete().unwrap());
            let window = PracticeWindow::new().unwrap();
            let scene = Rc::new(RefCell::new(session(&window)));
            window.set_answer_text(input.clone().into());
            let weak = Rc::downgrade(&scene);
            let attempt_latex = latex.clone();
            window.on_submit_answer(move |text| {
                let scene = weak.upgrade().unwrap();
                let mut scene = scene.borrow_mut();
                let result = assess_input(
                    &mut scene,
                    AnswerAttempt {
                        input: text.to_string(),
                        math_latex: Some(attempt_latex.clone()),
                        keypad_expanded: expanded,
                    },
                )
                .unwrap();
                assert!(scene.finish_answer_feedback(result, None));
            });
            let saved = Rc::new(RefCell::new(None));
            let received = saved.clone();
            let weak = Rc::downgrade(&scene);
            let destination = store.clone();
            window.on_practice_action(move |action, _| {
                assert_eq!(action, PracticeAction::RecordProblem);
                let scene = weak.upgrade().unwrap();
                let evidence = problem_context(&scene.borrow(), None);
                let observer = scene.borrow().window.as_weak();
                // A later UI update must not change evidence already captured.
                scene
                    .borrow()
                    .window
                    .set_answer_text("changed after capture".into());
                let destination = destination.clone();
                let received = received.clone();
                Timer::single_shot(Duration::ZERO, move || {
                    received.replace(Some(problem_report::save(&destination, &evidence).unwrap()));
                    observer
                        .upgrade()
                        .unwrap()
                        .set_problem_record_status(ProblemRecordStatus::Saved);
                });
            });
            window.show().unwrap();
            adapter.set_size(MIN_CARD_SIZE);
            window.invoke_focus_answer();
            paint(&adapter);
            key(&window, &adapter, Key::Return);
            assert!(scene.borrow().answered);
            assert_eq!(window.get_feedback(), if correct { 1 } else { 2 });
            window.invoke_open_context_menu(
                MIN_CARD_SIZE.width as f32 - 16.0,
                MIN_CARD_SIZE.height as f32 - 16.0,
            );
            paint(&adapter);
            click(
                &window,
                &adapter,
                window.get_context_menu_left() + 24.0,
                window.get_context_menu_top() + 126.0,
            );
            paint(&adapter);
            let path = saved
                .borrow()
                .clone()
                .expect("The menu command saves a real local record");
            let report: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(report["context"]["card"]["id"], "card-1");
            assert_eq!(
                report["context"]["outcome"],
                if correct { "correct" } else { "incorrect" }
            );
            assert_eq!(report["context"]["current_input"], input);
            assert_eq!(report["context"]["last_attempt"]["input"], input);
            assert_eq!(report["context"]["last_attempt"]["math_latex"], latex);
            assert_eq!(
                report["context"]["last_attempt"]["keypad_expanded"],
                expanded
            );
            assert_eq!(
                window.get_problem_record_status(),
                ProblemRecordStatus::Saved
            );
            assert!(!store.directory().join("state.json").exists());
            assert!(!store.directory().join("learning_state.json").exists());
            key(&window, &adapter, Key::Escape);
            assert!(!window.get_context_menu_open());
            assert_eq!(window.get_feedback(), if correct { 1 } else { 2 });
            window.hide().unwrap();
        }
    }

    #[test]
    fn unfinished_math_is_recordable_without_a_grade_or_submission() {
        let adapter = software_window();
        let window = PracticeWindow::new().unwrap();
        let scene = session(&window);
        let edit = crate::structured_math::Session::new(crate::structured_math::Seed::Latex(
            r"\frac{1}{}".into(),
        ))
        .unwrap();
        let error = edit.complete().unwrap_err();
        let ui = window.global::<crate::MathInputState>();
        ui.set_open(true);
        ui.set_keypad_expanded(true);
        ui.set_error(error.clone().into());
        window.show().unwrap();
        adapter.set_size(MIN_CARD_SIZE);
        paint(&adapter);
        let evidence = problem_context(&scene, Some(&edit));
        assert_eq!(evidence.outcome, Outcome::Unanswered);
        assert_eq!(evidence.input_error, error);
        assert_eq!(evidence.current_math_latex, Some(edit.latex_source()));
        assert!(evidence.keypad_expanded);
        assert!(evidence.last_attempt.is_none());
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().join("data")).unwrap();
        let path = problem_report::save(&store, &evidence).unwrap();
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(report["context"], serde_json::to_value(evidence).unwrap());
        assert!(!scene.answered);
        assert!(!store.directory().join("learning_state.json").exists());
        window.hide().unwrap();
    }
}
