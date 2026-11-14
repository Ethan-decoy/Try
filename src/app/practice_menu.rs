//! Typed current-card commands; lifecycle and evidence capture stay with practice.
use super::*;
use crate::{PracticeAction, ProblemRecordStatus};

impl App {
    pub(super) fn bind_practice_menu(self: &Rc<Self>, window: &PracticeWindow, id: u64) {
        let weak = Rc::downgrade(self);
        window.on_context_menu_changed(move |opened| {
            if let Some(app) = weak.upgrade() {
                app.hold_card_close_for_menu(id, opened);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_practice_action(move |action, minutes| {
            let Some(app) = weak.upgrade() else { return };
            // Resolve identity before closing the popup can release its owner
            // or an expired feedback timer can dismiss this particular card.
            let key = app
                .practice
                .borrow()
                .as_ref()
                .filter(|session| session.diagnostic_id == id)
                .map(|session| session.question.key.clone());
            let Some(key) = key else { return };
            let problem = (action == PracticeAction::RecordProblem)
                .then(|| app.practice_problem(id))
                .flatten();
            app.defer_tray_action(move |app| match action {
                PracticeAction::ExcludeQuestion => {
                    let result = app.commit(|library| {
                        library.set_card_enabled(&key.pack_id, &key.card_id, false)
                    });
                    if result.is_ok() {
                        let current = app
                            .practice
                            .borrow()
                            .as_ref()
                            .is_some_and(|session| session.diagnostic_id == id);
                        if current {
                            app.close_practice();
                        }
                    }
                    app.report(result, "");
                }
                PracticeAction::Pause => {
                    if [60, 120, 180].contains(&minutes) {
                        app.set_pause(minutes);
                    }
                }
                PracticeAction::CustomPause => {
                    let result = app.show_settings(true);
                    app.report(result, "");
                }
                PracticeAction::RecordProblem => {
                    if let Some(context) = problem {
                        app.record_practice_problem(id, &context);
                    }
                }
            });
        });
    }

    fn practice_problem(&self, id: u64) -> Option<r#try::problem_report::ProblemContext> {
        let practice = self.practice.borrow();
        let session = practice
            .as_ref()
            .filter(|session| session.diagnostic_id == id)?;
        let active = self.structured_math.borrow();
        let math = active.as_ref().and_then(|(target, edit)| match target {
            MathTarget::Practice(active_id) if *active_id == id => Some(edit),
            _ => None,
        });
        Some(practice_window::problem_context(session, math))
    }

    fn record_practice_problem(&self, id: u64, context: &r#try::problem_report::ProblemContext) {
        let result = r#try::problem_report::save(&self.store, context);
        if let Err(error) = &result {
            log::warn!(target: "try::app", "event=problem_record_failed reason={error}");
        }
        let practice = self.practice.borrow();
        if let Some(session) = practice
            .as_ref()
            .filter(|session| session.diagnostic_id == id)
        {
            match result {
                Ok(_) => session
                    .window
                    .set_problem_record_status(ProblemRecordStatus::Saved),
                Err(error) => {
                    session.window.set_problem_record_error(error.into());
                    session
                        .window
                        .set_problem_record_status(ProblemRecordStatus::Failed);
                }
            }
        }
    }
}
