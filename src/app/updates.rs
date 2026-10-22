//! Update checks, download progress and restart coordination for the UI.
use super::*;

impl App {
    pub(super) fn arm_automatic_update_check(self: &Rc<Self>, startup: bool) {
        self.automatic_update_timer.stop();
        if !self.library.borrow().settings.auto_check_updates {
            return;
        }
        let delay = self
            .update_check
            .lock()
            .unwrap()
            .schedule
            .delay(chrono::Utc::now().timestamp());
        let minimum = if startup {
            update_schedule::STARTUP_DELAY
        } else {
            Duration::from_secs(1)
        };
        let weak = Rc::downgrade(self);
        self.automatic_update_timer
            .start(TimerMode::SingleShot, delay.max(minimum), move || {
                if let Some(app) = weak.upgrade() {
                    app.begin_automatic_update_check(app.update_fetch.get());
                }
            });
    }

    pub(super) fn begin_automatic_update_check(
        self: &Rc<Self>,
        fetch: impl FnOnce() -> Result<CheckOutcome, CheckError> + Send + 'static,
    ) {
        if !self.library.borrow().settings.auto_check_updates {
            return;
        }
        let delay = self
            .update_check
            .lock()
            .unwrap()
            .schedule
            .delay(chrono::Utc::now().timestamp());
        if !delay.is_zero() {
            self.arm_automatic_update_check(false);
            return;
        }
        self.begin_update_check_with_origin(fetch, true);
    }

    pub(super) fn begin_update_check(
        self: &Rc<Self>,
        fetch: impl FnOnce() -> Result<CheckOutcome, CheckError> + Send + 'static,
    ) {
        self.begin_update_check_with_origin(fetch, false);
    }

    pub(super) fn begin_update_check_with_origin(
        self: &Rc<Self>,
        fetch: impl FnOnce() -> Result<CheckOutcome, CheckError> + Send + 'static,
        automatic: bool,
    ) {
        {
            let mut transfer = self.update_transfer.lock().unwrap();
            match &*transfer {
                UpdateTransfer::Running { .. } | UpdateTransfer::Ready(_) => return,
                UpdateTransfer::Failed { .. } => *transfer = UpdateTransfer::Idle,
                UpdateTransfer::Idle => {}
            }
        }
        {
            let mut check = self.update_check.lock().unwrap();
            if matches!(check.status, UpdateStatus::Checking) {
                return;
            }
            if let Err(error) = check
                .schedule
                .record_attempt(chrono::Utc::now().timestamp())
            {
                log::warn!(target: "try::updates", "event=cache_attempt_save_failed reason={error}");
                if automatic {
                    drop(check);
                    self.arm_automatic_update_check(false);
                    return;
                }
            }
            check.status = UpdateStatus::Checking;
        }
        self.refresh_update_status();
        let state = self.update_check.clone();
        let worker = std::thread::Builder::new()
            .name("try_update_check".into())
            .spawn(move || {
                let origin = if automatic { "automatic" } else { "manual" };
                log::info!(target: "try::updates", "event=check_started origin={origin}");
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(fetch))
                    .unwrap_or(Err(CheckError::Network));
                match &result {
                    Ok(outcome) => log::info!(target: "try::updates", "event=check_finished origin={origin} outcome={outcome:?}"),
                    Err(error) => log::warn!(target: "try::updates", "event=check_failed origin={origin} reason={error}"),
                }
                let observer = {
                    let mut check = state.lock().unwrap();
                    if let Ok(outcome) = &result
                        && let Err(error) = check.schedule.record_result(outcome)
                    {
                        log::warn!(target: "try::updates", "event=cache_result_save_failed reason={error}");
                    }
                    check.finish(result, automatic);
                    check.observer.clone()
                };
                if let Some(observer) = observer {
                    // This holds no UI component alive. A recreated main window reads the
                    // retained result on creation, even if this observer was just dropped.
                    let _ = observer.upgrade_in_event_loop(|window| {
                        window.invoke_update_check_finished();
                    });
                }
            });
        if let Err(error) = worker {
            log::warn!(target: "try::updates", "event=worker_start_failed reason={error}");
            self.update_check
                .lock()
                .unwrap()
                .finish(Err(CheckError::Network), automatic);
            self.refresh_update_status();
        }
        self.arm_automatic_update_check(false);
    }

    pub(super) fn refresh_update_status(&self) {
        let window = self.main_window.borrow();
        let Some(window) = window.as_ref() else {
            return;
        };
        let check = self.update_check.lock().unwrap();
        let (mut state, mut text, mut download) = match &check.status {
            UpdateStatus::Idle => (0, String::new(), false),
            UpdateStatus::Checking => (1, tr!("正在检查…").to_owned(), false),
            UpdateStatus::Finished(Ok(CheckOutcome::Available(release))) => {
                (2, tr!("新版本 {version}", version = release.version), true)
            }
            UpdateStatus::Finished(Ok(CheckOutcome::UpToDate)) => {
                (2, tr!("已是最新").to_owned(), false)
            }
            UpdateStatus::Finished(Ok(CheckOutcome::NoPublicRelease)) => {
                (2, tr!("尚无公开版本").to_owned(), false)
            }
            UpdateStatus::Finished(Err(CheckError::RateLimited)) => {
                (2, tr!("检查受限，请稍后再试").to_owned(), false)
            }
            UpdateStatus::Finished(Err(_)) => (2, tr!("暂时无法检查").to_owned(), false),
        };
        let transfer = self.update_transfer.lock().unwrap();
        let mut action = tr!("下载并重启").to_owned();
        let mut busy = false;
        let mut action_kind = 0;
        match &*transfer {
            UpdateTransfer::Running {
                phase,
                percent,
                cancel,
            } => {
                action_kind = 1;
                if *phase == UpdatePhase::Checking {
                    state = 1;
                }
                text = if cancel.load(Ordering::Relaxed) {
                    tr!("正在取消…").into()
                } else if *phase == UpdatePhase::Checking {
                    tr!("正在检查…").into()
                } else if let Some(percent) = percent {
                    tr!("正在下载 {percent}%", percent = percent)
                } else {
                    tr!("正在下载…").into()
                };
                action = tr!("取消").into();
                download = true;
                busy = true;
            }
            UpdateTransfer::Ready(_) => {
                if self.has_pending_update_edits() {
                    action_kind = 1;
                    text = tr!("等待编辑完成").into();
                    action = tr!("取消").into();
                } else {
                    action_kind = 2;
                    text = tr!("更新已就绪").into();
                    action = tr!("重试更新").into();
                }
                download = true;
                busy = true;
            }
            UpdateTransfer::Failed { phase, .. } => {
                text = match phase {
                    UpdatePhase::Checking => tr!("检查失败，请重试"),
                    UpdatePhase::Downloading => tr!("下载失败，请重试"),
                }
                .into();
                action = tr!("重试").into();
            }
            UpdateTransfer::Idle => {}
        }
        window.set_update_state(state);
        window.set_update_status(text.into());
        window.set_update_download(download);
        window.set_update_action(action.into());
        window.set_update_busy(busy);
        window.set_update_action_kind(action_kind);
    }

    pub(super) fn open_update_download(self: &Rc<Self>) {
        self.open_update_download_with(self.update_fetch.get(), updater::prepare);
    }

    pub(super) fn open_update_download_with(
        self: &Rc<Self>,
        fetch: impl FnOnce() -> Result<CheckOutcome, CheckError> + Send + 'static,
        prepare: impl FnOnce(
            update_check::Release,
            &AtomicBool,
            &mut dyn FnMut(u64, Option<u64>),
        ) -> AppResult<PreparedUpdate>
        + Send
        + 'static,
    ) {
        {
            let transfer = self.update_transfer.lock().unwrap();
            if matches!(
                *transfer,
                UpdateTransfer::Running { .. } | UpdateTransfer::Ready(_)
            ) {
                return;
            }
        }
        let hinted_release = {
            let check = self.update_check.lock().unwrap();
            match &check.status {
                UpdateStatus::Finished(Ok(CheckOutcome::Available(release))) => release.clone(),
                _ => return,
            }
        };
        let state = self.update_check.clone();
        let transfer = self.update_transfer.clone();
        self.begin_update_download(UpdatePhase::Checking, move |cancel, progress| {
            if cancel.load(Ordering::Relaxed) {
                return Err(tr!("已取消更新下载。").into());
            }
            log::info!(target: "try::updates", "event=download_requested from={} cached_target={}",
                env!("CARGO_PKG_VERSION"), hinted_release.version);
            {
                let mut check = state.lock().unwrap();
                if let Err(error) = check
                    .schedule
                    .record_attempt(chrono::Utc::now().timestamp())
                {
                    log::warn!(target: "try::updates", "event=cache_attempt_save_failed reason={error}");
                }
            }
            // The daily cache is a hint. Resolve the public target again at the
            // user's download request, before fetching any package or checksum.
            log::info!(target: "try::updates", "event=check_started origin=download");
            let outcome = fetch().map_err(|error| {
                log::warn!(target: "try::updates", "event=check_failed origin=download reason={error}");
                if error == CheckError::RateLimited {
                    tr!("检查受限，请稍后再试").to_owned()
                } else {
                    tr!("暂时无法检查").to_owned()
                }
            })?;
            log::info!(target: "try::updates", "event=check_finished origin=download outcome={outcome:?}");
            {
                let mut check = state.lock().unwrap();
                if let Err(error) = check.schedule.record_result(&outcome) {
                    log::warn!(target: "try::updates", "event=cache_result_save_failed reason={error}");
                }
                check.finish(Ok(outcome.clone()), false);
            }
            if cancel.load(Ordering::Relaxed) {
                return Err(tr!("已取消更新下载。").into());
            }
            let CheckOutcome::Available(release) = outcome else {
                return Ok(None);
            };
            if let UpdateTransfer::Running { phase, .. } = &mut *transfer.lock().unwrap() {
                *phase = UpdatePhase::Downloading;
            }
            log::info!(target: "try::updates", "event=download_started from={} target={}",
                env!("CARGO_PKG_VERSION"), release.version);
            prepare(release, cancel, progress).map(Some)
        });
    }

    pub(super) fn cancel_update_download(self: &Rc<Self>) {
        let idle = {
            let mut transfer = self.update_transfer.lock().unwrap();
            match &*transfer {
                UpdateTransfer::Running { phase, cancel, .. } => {
                    if !cancel.swap(true, Ordering::Relaxed) {
                        log::info!(target: "try::updates", "event=update_cancel_requested phase={phase:?}");
                    }
                    false
                }
                UpdateTransfer::Ready(prepared) => {
                    log::info!(target: "try::updates", "event=update_cancelled phase=Ready target={}", prepared.release.version);
                    *transfer = UpdateTransfer::Idle;
                    true
                }
                _ => true,
            }
        };
        self.refresh_update_status();
        if idle {
            self.update_transfer_timer.stop();
            self.arm_automatic_update_check(false);
        }
    }

    pub(super) fn begin_update_download(
        self: &Rc<Self>,
        phase: UpdatePhase,
        fetch: impl FnOnce(
            &AtomicBool,
            &mut dyn FnMut(u64, Option<u64>),
        ) -> AppResult<Option<PreparedUpdate>>
        + Send
        + 'static,
    ) {
        if matches!(
            *self.update_transfer.lock().unwrap(),
            UpdateTransfer::Running { .. } | UpdateTransfer::Ready(_)
        ) {
            return;
        }
        if self.has_pending_update_edits() {
            log::debug!(target: "try::updates", "event=update_deferred reason=unsaved_edits");
            self.show_message(tr!("请先保存或取消编辑，再更新。"), true);
            return;
        }
        let language = self.library.borrow().settings.interface_language;
        let cancel = Arc::new(AtomicBool::new(false));
        *self.update_transfer.lock().unwrap() = UpdateTransfer::Running {
            phase,
            percent: None,
            cancel: cancel.clone(),
        };
        self.automatic_update_timer.stop();
        self.refresh_update_status();
        let state = self.update_transfer.clone();
        let worker = std::thread::Builder::new()
            .name("try_update_download".into())
            .spawn(move || {
                i18n::activate(language);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    fetch(&cancel, &mut |received, total| {
                        if let UpdateTransfer::Running { percent, .. } =
                            &mut *state.lock().unwrap()
                        {
                            *percent = total.filter(|n| *n > 0).map(|n| {
                                received
                                    .saturating_mul(100)
                                    .checked_div(n)
                                    .unwrap_or(0)
                                    .min(99)
                            });
                        }
                    })
                }))
                .unwrap_or_else(|_| Err(tr!("更新下载失败，请重试。").into()));
                // Serialize completion with cancellation so a late cancel cannot
                // be overwritten by a verified update awaiting installation.
                let mut transfer = state.lock().unwrap();
                let completed_phase = match &*transfer {
                    UpdateTransfer::Running { phase, .. } => *phase,
                    _ => phase,
                };
                *transfer = if cancel.load(Ordering::Relaxed) {
                    log::info!(target: "try::updates", "event=update_cancelled phase={completed_phase:?}");
                    UpdateTransfer::Idle
                } else {
                    match result {
                        Ok(Some(prepared)) => {
                            log::info!(target: "try::updates", "event=download_ready target={}", prepared.release.version);
                            UpdateTransfer::Ready(prepared)
                        }
                        Ok(None) => UpdateTransfer::Idle,
                        Err(error) => {
                            if completed_phase == UpdatePhase::Checking {
                                // The check already recorded its specific failure.
                                log::info!(target: "try::updates", "event=update_finished outcome=failed phase={completed_phase:?}");
                            } else {
                                log::warn!(target: "try::updates", "event=update_failed phase={completed_phase:?} reason={error}");
                            }
                            UpdateTransfer::Failed {
                                phase: completed_phase,
                                error,
                            }
                        }
                    }
                };
            });
        if let Err(error) = worker {
            log::warn!(target: "try::updates", "event=worker_start_failed origin=update phase={phase:?} reason={error}");
            *self.update_transfer.lock().unwrap() = UpdateTransfer::Failed {
                phase,
                error: tr!("无法启动更新，请重试。").into(),
            };
        }
        let weak = Rc::downgrade(self);
        self.update_transfer_timer.start(
            TimerMode::Repeated,
            Duration::from_millis(150),
            move || {
                if let Some(app) = weak.upgrade() {
                    app.refresh_update_status();
                    let state = app.update_transfer.lock().unwrap();
                    if !matches!(*state, UpdateTransfer::Running { .. }) {
                        let error = if let UpdateTransfer::Failed { error, .. } = &*state {
                            Some(error.clone())
                        } else {
                            None
                        };
                        let ready = matches!(*state, UpdateTransfer::Ready(_));
                        drop(state);
                        app.update_transfer_timer.stop();
                        if let Some(error) = error {
                            app.show_message(&error, true);
                        }
                        if ready {
                            app.restart_after_update();
                        } else {
                            app.arm_automatic_update_check(false);
                        }
                    }
                }
            },
        );
    }

    fn has_pending_update_edits(&self) -> bool {
        self.editor.borrow().is_some()
            || self.pending_import.borrow().is_some()
            || self.structured_math.borrow().is_some()
            || self.main_window.borrow().as_ref().is_some_and(|window| {
                window.get_name_editing()
                    || window.get_description_editing()
                    || window.get_tag_editing()
            })
    }

    pub(super) fn resume_ready_update(self: &Rc<Self>) {
        if !matches!(
            *self.update_transfer.lock().unwrap(),
            UpdateTransfer::Ready(_)
        ) {
            return;
        }
        // Finish the editing callback and release its window/data borrows first.
        // Recheck the state on delivery: cancellation or a new draft wins.
        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(app) = weak.upgrade() {
                app.restart_after_update();
            }
        });
    }

    pub(super) fn restart_after_update(self: &Rc<Self>) {
        if !matches!(
            *self.update_transfer.lock().unwrap(),
            UpdateTransfer::Ready(_)
        ) {
            return;
        }
        if self.has_pending_update_edits() {
            self.refresh_update_status();
            return;
        }
        if let Err(error) = self.store.save(&self.library.borrow()) {
            self.refresh_update_status();
            self.notify(&error, true);
            return;
        }
        let previous = std::mem::take(&mut *self.update_transfer.lock().unwrap());
        if let UpdateTransfer::Ready(prepared) = previous {
            log::info!(target: "try::updates", "event=install_requested from={} target={}",
                env!("CARGO_PKG_VERSION"), prepared.release.version);
            self.restart_update.replace(Some(prepared));
            self.quit();
        }
    }

    pub fn take_restart_update(&self) -> Option<PreparedUpdate> {
        self.restart_update.borrow_mut().take()
    }

    pub fn show_update_failure(&self) {
        self.show_message(tr!("更新未完成，原有数据已保留。"), true);
    }

    pub fn reject_update_startup(self: &Rc<Self>) {
        self.quit();
    }
}
