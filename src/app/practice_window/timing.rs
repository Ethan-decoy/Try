//! Reminder scheduling and the single owner of card-close countdown state.
use super::*;
use std::time::Instant;

#[derive(Default)]
pub(in crate::app) struct CloseCountdown {
    deadline: Option<Instant>,
    remaining: Option<Duration>,
}

impl CloseCountdown {
    fn arm(&mut self, now: Instant, duration: Duration) {
        self.deadline = Some(now + duration);
        self.remaining = None;
    }

    fn pause(&mut self, now: Instant) {
        if let Some(deadline) = self.deadline.take() {
            self.remaining = Some(deadline.saturating_duration_since(now));
        }
    }

    fn resume(&mut self, now: Instant) -> Option<Duration> {
        let remaining = self.remaining.take()?;
        self.deadline = Some(now + remaining);
        Some(remaining)
    }

    fn cancel(&mut self) {
        self.deadline = None;
        self.remaining = None;
    }
}

impl App {
    pub(in crate::app) fn cancel_card_close(&self) {
        self.card_close_timer.stop();
        if let Some(session) = self.practice.borrow_mut().as_mut() {
            session.close_countdown.cancel();
        }
    }

    pub(in crate::app) fn arm_card_close(self: &Rc<Self>) {
        let (id, duration) = {
            let mut practice = self.practice.borrow_mut();
            let Some(session) = practice.as_mut() else {
                return;
            };
            let library = self.library.borrow();
            let seconds = if session.prepared.knowledge_only {
                library.settings.knowledge_seconds
            } else {
                library.settings.feedback_seconds
            };
            let duration = Duration::from_secs(seconds.into());
            session
                .close_countdown
                .arm(std::time::Instant::now(), duration);
            (session.diagnostic_id, duration)
        };
        self.schedule_card_close(id, duration);
    }

    pub(in crate::app) fn hold_card_close_for_menu(self: &Rc<Self>, id: u64, opened: bool) {
        let duration = {
            let mut practice = self.practice.borrow_mut();
            let Some(session) = practice
                .as_mut()
                .filter(|session| session.diagnostic_id == id)
            else {
                return;
            };
            if opened {
                session.close_countdown.pause(Instant::now());
                self.card_close_timer.stop();
                None
            } else {
                session.close_countdown.resume(Instant::now())
            }
        };
        if let Some(duration) = duration {
            self.schedule_card_close(id, duration);
        }
    }

    fn schedule_card_close(self: &Rc<Self>, id: u64, duration: Duration) {
        let opened = self
            .practice
            .borrow()
            .as_ref()
            .filter(|session| session.diagnostic_id == id)
            .is_some_and(|session| session.window.get_context_menu_open());
        if opened {
            self.hold_card_close_for_menu(id, true);
            return;
        }
        let weak = Rc::downgrade(self);
        self.card_close_timer.start(
            TimerMode::SingleShot,
            duration.max(Duration::from_millis(1)),
            move || {
                if let Some(app) = weak.upgrade() {
                    let state = app
                        .practice
                        .borrow()
                        .as_ref()
                        .filter(|session| session.diagnostic_id == id)
                        .map(|session| session.window.get_context_menu_open());
                    match state {
                        Some(true) => app.hold_card_close_for_menu(id, true),
                        Some(false) => app.close_practice(),
                        None => {}
                    }
                }
            },
        );
    }
    pub(in crate::app) fn arm_reminder(self: &Rc<Self>) {
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

    pub(in crate::app) fn wait_for_reminder(self: &Rc<Self>, due: chrono::DateTime<chrono::Local>) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_pause_keeps_remaining_time_instead_of_restarting_the_duration() {
        let now = Instant::now();
        let mut close = CloseCountdown::default();
        close.arm(now, Duration::from_secs(10));
        close.pause(now + Duration::from_secs(3));
        close.pause(now + Duration::from_secs(4));
        assert_eq!(
            close.resume(now + Duration::from_secs(60)),
            Some(Duration::from_secs(7))
        );
        assert_eq!(close.resume(now), None);
    }

    #[test]
    fn wrong_unanswered_and_preview_cards_do_not_acquire_a_close_timer() {
        let now = Instant::now();
        let mut close = CloseCountdown::default();
        close.pause(now);
        assert_eq!(close.resume(now), None);
        close.arm(now, Duration::from_secs(2));
        close.pause(now + Duration::from_secs(3));
        assert_eq!(close.resume(now), Some(Duration::ZERO));
        close.cancel();
        close.pause(now);
        assert_eq!(close.resume(now), None);
    }
}
