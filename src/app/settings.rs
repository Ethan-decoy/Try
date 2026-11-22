//! Saved preferences, appearance, pause scheduling and autostart.
use super::*;

pub(super) const INTERVAL_MINUTE_PRESETS: &[i32] = &[15, 25, 30, 45];
pub(super) const FEEDBACK_SECOND_PRESETS: &[i32] = &[1, 2, 3];
pub(super) const KNOWLEDGE_SECOND_PRESETS: &[i32] = &[5, 8, 15, 30];
pub(super) const PAUSE_MINUTE_PRESETS: &[i32] = &[0, 60, 120, 180, -1];

pub(super) fn preset_value(index: i32, presets: &[i32], current: i32) -> Option<i32> {
    let index = usize::try_from(index).ok()?;
    presets
        .get(index)
        .copied()
        .or_else(|| (index == presets.len() && !presets.contains(&current)).then_some(current))
}

pub(super) fn preset_options(
    current: i32,
    presets: &[i32],
    label: impl Fn(i32) -> String,
) -> (ModelRc<slint::SharedString>, i32) {
    let mut options = presets
        .iter()
        .map(|n| label(*n).into())
        .collect::<Vec<slint::SharedString>>();
    let index = presets
        .iter()
        .position(|n| *n == current)
        .unwrap_or_else(|| {
            options.push(tr!("{p0}（自定）", p0 = label(current)).into());
            presets.len()
        });
    (ModelRc::new(VecModel::from(options)), index as i32)
}

impl App {
    pub(super) fn resolved_dark_theme(&self, appearance: Appearance) -> bool {
        match appearance {
            Appearance::Light => false,
            Appearance::Dark => true,
            Appearance::System => {
                #[cfg(target_os = "windows")]
                {
                    self.system_dark.get()
                }
                #[cfg(not(target_os = "windows"))]
                {
                    self.system_appearance.get_dark()
                }
            }
        }
    }

    pub(super) fn interface_motion(&self) -> crate::MotionPreference {
        if !self.library.borrow().settings.interface_animations {
            return crate::MotionPreference::Reduced;
        }
        #[cfg(target_os = "windows")]
        if FloatingWindow::client_animations_enabled() {
            crate::MotionPreference::Full
        } else {
            crate::MotionPreference::Reduced
        }
        #[cfg(not(target_os = "windows"))]
        {
            crate::MotionPreference::ColorOnly
        }
    }

    pub(super) fn sync_appearance(&self) {
        let dark = self.resolved_dark_theme(self.library.borrow().settings.appearance);
        let motion = self.interface_motion();
        if motion == crate::MotionPreference::Reduced {
            self.hide_outgoing_practice();
        }
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.global::<crate::Theme>().set_motion(motion);
            window.set_dark_theme(dark);
            self.floating.apply_theme(window.window(), dark);
        }
        if let Some(session) = self.practice.borrow().as_ref() {
            if session.window.get_dark_theme() != dark {
                session.window.invoke_interrupt_entrance();
            }
            session.window.global::<crate::Theme>().set_motion(motion);
            session.window.set_dark_theme(dark);
            self.floating.apply_theme(session.window.window(), dark);
        }
        if let Some(window) = self.image_preview.borrow().as_ref() {
            window.set_dark_theme(dark);
            self.floating.apply_theme(window.window(), dark);
        }
        self.refresh_structured_math();
    }

    pub(super) fn apply_native_theme(
        self: &Rc<Self>,
        window: &slint::Window,
        applied: &Cell<Option<(Appearance, bool)>>,
        force: bool,
    ) {
        #[cfg(target_os = "windows")]
        if applied.get().is_none() {
            self.system_dark.set((self.system_theme_probe.get())());
        }
        let appearance = self.library.borrow().settings.appearance;
        let dark = self.resolved_dark_theme(appearance);
        let key = (appearance, dark);
        if force || applied.get() != Some(key) {
            #[cfg(target_os = "windows")]
            if applied.get().is_none() {
                let weak = Rc::downgrade(self);
                FloatingWindow::watch_system_theme(window, move || {
                    if let Some(app) = weak.upgrade() {
                        app.defer_tray_action(|app| app.refresh_system_appearance());
                    }
                });
            }
            applied.set(Some(key));
            self.sync_appearance();
            self.floating.apply_theme(window, dark);
        }
    }

    #[cfg(target_os = "windows")]
    pub(super) fn refresh_system_appearance(&self) {
        self.system_dark.set((self.system_theme_probe.get())());
        self.sync_appearance();
    }

    pub(super) fn set_practice_card_locked(self: &Rc<Self>, locked: bool) {
        let card = self
            .practice
            .borrow()
            .as_ref()
            .map(|session| session.window.clone_strong());
        // A move made before locking is part of the saved placement, even
        // when this card is still open.
        let position = card
            .as_ref()
            .filter(|card| locked && !card.get_card_locked())
            .and_then(|card| self.floating.moved_position(card.window()));
        let result = self.commit(|library| {
            library.settings.practice_card_locked = locked;
            if let Some(position) = position {
                library.settings.set_custom_popup_position(position);
            }
            Ok(())
        });
        if result.is_ok()
            && let Some(card) = card
        {
            card.set_card_locked(locked);
        }
        self.report(result, "");
    }

    pub(super) fn set_pause(self: &Rc<Self>, minutes: i32) {
        let result = self.commit(|library| {
            library.settings.pause =
                PracticePause::from_minutes(minutes, chrono::Utc::now().timestamp())?;
            Ok(())
        });
        if result.is_ok() {
            if minutes != 0 {
                self.close_practice();
            }
            log::info!(target: "try::app", "event=pause_changed minutes={minutes}");
        }
        self.report(result, "");
    }

    pub(super) fn set_interval(self: &Rc<Self>, value: i32) {
        let previous = self.library.borrow().settings.interval_minutes;
        self.set_practice_time(
            value,
            tr!("出题间隔不能为负数。"),
            |settings, minutes| {
                settings.interval_minutes = minutes;
            },
        );
        if self.library.borrow().settings.interval_minutes != previous {
            self.arm_reminder();
        }
    }

    pub(super) fn set_feedback(&self, value: i32) {
        self.set_practice_time(
            value,
            tr!("停留时间不能为负数。"),
            |settings, seconds| {
                settings.feedback_seconds = seconds;
            },
        );
    }

    pub(super) fn set_knowledge_duration(&self, value: i32) {
        self.set_practice_time(
            value,
            tr!("停留时间不能为负数。"),
            |settings, seconds| {
                settings.knowledge_seconds = seconds;
            },
        );
    }

    fn set_practice_time(
        &self,
        value: i32,
        negative_error: &str,
        update: impl FnOnce(&mut Settings, u32),
    ) {
        let result = self.save_library_edit(|library| {
            update(
                &mut library.settings,
                u32::try_from(value).map_err(|_| negative_error)?,
            );
            Ok(())
        });
        if let Some(window) = self.main_window.borrow().as_ref() {
            Self::refresh_practice_times(window, &self.library.borrow().settings);
        }
        match result {
            Ok(()) => self.report(Ok(()), ""),
            Err(error) => self.notify(&error, true),
        }
    }

    pub(super) fn set_knowledge_mode(self: &Rc<Self>, enabled: bool) {
        if self.knowledge_only.get() == enabled {
            return;
        }
        let result = self.commit(|library| {
            library.settings.knowledge_only = enabled;
            Ok(())
        });
        self.report(result, "");
    }

    pub(super) fn arm_pause_expiry(self: &Rc<Self>) {
        self.pause_timer.stop();
        let pause = self.library.borrow().settings.pause.clone();
        if let PracticePause::Timed { until, .. } = pause {
            let end = chrono::DateTime::from_timestamp(until, 0).expect("validated pause");
            let duration =
                (end - chrono::Utc::now()).to_std().unwrap_or_default() + Duration::from_millis(1);
            let weak = Rc::downgrade(self);
            self.pause_timer
                .start(TimerMode::SingleShot, duration, move || {
                    if let Some(app) = weak.upgrade() {
                        if app
                            .library
                            .borrow()
                            .settings
                            .pause
                            .expired(chrono::Utc::now().timestamp())
                        {
                            let result = app.commit(|library| {
                                library.settings.pause = PracticePause::None;
                                Ok(())
                            });
                            if result.is_ok() {
                                log::info!(target: "try::app", "event=pause_expired");
                            }
                            // A failed cleanup write does not make an expired pause permanent.
                            app.arm_reminder();
                            app.report(result, "");
                        } else {
                            // Recheck the deadline after a clock adjustment without polling.
                            app.arm_pause_expiry();
                        }
                    }
                });
        }
    }

    pub(super) fn read_autostart_status(&self) {
        let result = self
            .autostart
            .borrow()
            .as_ref()
            .map_err(Clone::clone)
            .and_then(Autostart::is_enabled);
        match result {
            Ok(enabled) => {
                self.autostart_enabled.set(enabled);
                self.autostart_error.borrow_mut().clear();
            }
            Err(error) => {
                self.autostart_enabled.set(false);
                self.autostart_error.replace(error);
            }
        }
    }

    pub(super) fn set_autostart(&self, enabled: bool) {
        let result = self
            .autostart
            .borrow()
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|entry| entry.set_enabled(enabled));
        self.read_autostart_status();
        if let Err(error) = &result {
            self.autostart_error.replace(error.clone());
            log::warn!(target: "try::app", "event=autostart_rejected reason={error}");
        } else {
            log::info!(target:"try::app","event=autostart_changed enabled={enabled}");
            self.notify("", false);
        }
        self.refresh();
    }

    pub(super) fn set_quiet_enabled(self: &Rc<Self>, enabled: bool) {
        let mut hours = self.library.borrow().settings.quiet_hours.clone();
        hours.enabled = enabled;
        self.save_quiet_hours(hours);
    }

    pub(super) fn set_quiet_range(self: &Rc<Self>, start: &str, end: &str) {
        self.update_quiet_range(start, end, true);
    }

    pub(super) fn update_quiet_range(self: &Rc<Self>, start: &str, end: &str, validate: bool) {
        let enabled = self.library.borrow().settings.quiet_hours.enabled;
        match QuietHours::from_text(enabled, start, end) {
            Ok(hours) => self.save_quiet_hours(hours),
            Err(error) => {
                if let Some(window) = self.main_window.borrow().as_ref() {
                    window.set_quiet_time_dirty(true);
                    window.set_quiet_time_error(if validate { error.into() } else { "".into() });
                }
            }
        }
    }

    fn save_quiet_hours(self: &Rc<Self>, hours: QuietHours) {
        let window = self
            .main_window
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong);
        let same = hours == self.library.borrow().settings.quiet_hours;
        if let Some(window) = &window {
            window.set_quiet_time_dirty(false);
            window.set_quiet_time_error("".into());
        }
        if same {
            return;
        }
        let result = self.commit(|library| {
            library.settings.quiet_hours = hours;
            Ok(())
        });
        if let Err(error) = &result
            && let Some(window) = window
        {
            window.set_quiet_time_dirty(true);
            window.set_quiet_time_error(error.as_str().into());
        }
        self.report(result, "");
    }
}
