//! Saved preferences, appearance, pause scheduling and autostart.
use super::*;

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

    pub(super) fn sync_appearance(&self) {
        let appearance = self.library.borrow().settings.appearance;
        let dark = self.resolved_dark_theme(appearance);
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_dark_theme(dark);
            self.floating.apply_theme(window.window(), dark);
        }
        if let Some(session) = self.practice.borrow().as_ref() {
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
        let result = self.commit(|library| {
            library.settings.interval_minutes =
                u32::try_from(value).map_err(|_| tr!("出题间隔不能为负数。"))?;
            Ok(())
        });
        self.report(result, "");
    }

    pub(super) fn set_feedback(self: &Rc<Self>, value: i32) {
        let result = self.commit(|library| {
            library.settings.feedback_seconds =
                u32::try_from(value).map_err(|_| tr!("停留时间不能为负数。"))?;
            Ok(())
        });
        self.report(result, "");
    }

    pub(super) fn set_knowledge_duration(self: &Rc<Self>, value: i32) {
        let result = self.commit(|library| {
            library.settings.knowledge_seconds =
                u32::try_from(value).map_err(|_| tr!("停留时间不能为负数。"))?;
            Ok(())
        });
        self.report(result, "");
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
        if enabled {
            let draft = self
                .main_window
                .borrow()
                .as_ref()
                .map(|window| (window.get_quiet_start(), window.get_quiet_end()));
            if let Some((start, end)) = draft {
                match QuietHours::from_text(true, start.as_str(), end.as_str()) {
                    Ok(hours) => {
                        let result = self.commit(|library| {
                            library.settings.quiet_hours = hours;
                            Ok(())
                        });
                        self.report(result, "");
                    }
                    Err(error) => {
                        if let Some(window) = self.main_window.borrow().as_ref() {
                            window.set_quiet_time_error(error.into());
                        }
                    }
                }
                return;
            }
        }
        let result = self.commit(|library| {
            library.settings.quiet_hours.enabled = enabled;
            Ok(())
        });
        self.report(result, "");
    }

    pub(super) fn set_quiet_range(self: &Rc<Self>, start: &str, end: &str) {
        let enabled = self.library.borrow().settings.quiet_hours.enabled;
        match QuietHours::from_text(enabled, start, end) {
            Ok(hours) => {
                let result = self.commit(|library| {
                    library.settings.quiet_hours = hours;
                    Ok(())
                });
                self.report(result, "");
            }
            Err(error) => {
                if let Some(window) = self.main_window.borrow().as_ref() {
                    window.set_quiet_time_error(error.into());
                }
            }
        }
    }
}
