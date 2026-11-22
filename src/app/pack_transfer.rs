//! Run portable pack I/O off the event loop. Only the App publishes library changes.
use super::*;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};

#[derive(Clone)]
pub(super) enum TransferRequest {
    Import {
        path: PathBuf,
        animated: bool,
    },
    Export {
        path: PathBuf,
        pack: Arc<KnowledgePack>,
    },
}

impl TransferRequest {
    fn run(&self, store: &Store) -> AppResult<Option<KnowledgePack>> {
        match self {
            Self::Import { path, .. } => store.import_pack(path).map(Some),
            Self::Export { path, pack } => store.export_portable_pack(path, pack).map(|()| None),
        }
    }

    fn progress(&self) -> &'static str {
        match self {
            Self::Import { .. } => tr!("正在导入知识包…"),
            Self::Export { .. } => tr!("正在导出知识包…"),
        }
    }

    fn failure(&self, error: &str) -> String {
        match self {
            Self::Import { .. } => tr!("导入失败。{error}", error = error),
            Self::Export { .. } => tr!("导出失败。{error}", error = error),
        }
    }
}

#[derive(Default)]
pub(super) enum TransferState {
    #[default]
    Idle,
    Running {
        request: TransferRequest,
        receiver: Receiver<AppResult<Option<KnowledgePack>>>,
    },
    Failed {
        request: TransferRequest,
        error: String,
    },
}

impl TransferState {
    pub(super) fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    pub(super) fn retry_pack(&self) -> Option<&KnowledgePack> {
        match self {
            Self::Failed {
                request: TransferRequest::Export { pack, .. },
                ..
            } => Some(pack),
            _ => None,
        }
    }
}

impl App {
    pub(super) fn import_pack(self: &Rc<Self>) {
        let Some(dialog) = self.pack_file_dialog(tr!("导入 Try 知识包")) else {
            return;
        };
        let animated = self
            .main_window
            .borrow()
            .as_ref()
            .is_some_and(MainWindow::get_import_from_pointer);
        if let Some(path) = dialog.pick_file() {
            self.start_pack_transfer(TransferRequest::Import { path, animated });
        }
    }

    pub(super) fn export_pack(self: &Rc<Self>) {
        let Some(dialog) = self.pack_file_dialog(tr!("导出 Try 知识包")) else {
            return;
        };
        let pack = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .cloned();
        let Some(pack) = pack else {
            return;
        };
        let Some(mut path) = dialog
            .set_file_name(format!("{}.json", pack.id))
            .save_file()
        else {
            return;
        };
        if path.extension().is_none() {
            path.set_extension("json");
        }
        self.start_pack_transfer(TransferRequest::Export {
            path,
            pack: Arc::new(pack),
        });
    }

    fn pack_file_dialog(&self, title: &str) -> Option<rfd::FileDialog> {
        let main = self.main_window.borrow();
        let window = main
            .as_ref()
            .filter(|window| window.get_tray_settings_available())?;
        let dialog = rfd::FileDialog::new()
            .set_title(title)
            .add_filter(tr!("Try 知识包"), &["json"]);
        Some(
            window
                .window()
                .with_winit_window(|native| dialog.clone().set_parent(native))
                .unwrap_or(dialog),
        )
    }

    pub(super) fn start_pack_transfer(self: &Rc<Self>, request: TransferRequest) {
        if self.pack_transfer.borrow().is_running()
            || !self
                .main_window
                .borrow()
                .as_ref()
                .is_some_and(MainWindow::get_tray_settings_available)
        {
            return;
        }
        let language = self.library.borrow().settings.interface_language;
        let store = self.store.clone();
        let job = request.clone();
        let (sender, receiver) = sync_channel(1);
        self.pack_transfer
            .replace(TransferState::Running { request, receiver });
        self.notify("", false);
        self.refresh_pack_transfer();
        if let Err(error) = std::thread::Builder::new()
            .name("try-pack-transfer".into())
            .spawn(move || {
                i18n::activate(language);
                let _ = sender.send(job.run(&store));
            })
        {
            self.finish_pack_transfer(Err(error.to_string()));
            return;
        }
        let weak = Rc::downgrade(self);
        self.pack_transfer_timer
            .start(TimerMode::Repeated, Duration::from_millis(40), move || {
                if let Some(app) = weak.upgrade() {
                    app.poll_pack_transfer();
                }
            });
    }

    pub(super) fn poll_pack_transfer(self: &Rc<Self>) {
        let result = match &*self.pack_transfer.borrow() {
            TransferState::Running { receiver, .. } => receiver.try_recv(),
            _ => return,
        };
        match result {
            Ok(result) => self.finish_pack_transfer(result),
            Err(TryRecvError::Disconnected) => {
                self.finish_pack_transfer(Err(tr!("文件处理未完成，请重试。").into()));
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    fn finish_pack_transfer(self: &Rc<Self>, result: AppResult<Option<KnowledgePack>>) {
        let TransferState::Running { request, .. } = self.pack_transfer.take() else {
            return;
        };
        self.pack_transfer_timer.stop();
        let result = match result {
            Ok(Some(pack)) => self.finish_import(
                pack,
                matches!(request, TransferRequest::Import { animated: true, .. }),
            ),
            Ok(None) => {
                self.notify(tr!("已导出。"), false);
                Ok(())
            }
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            let error = request.failure(&error);
            log::warn!(target: "try::app", "event=pack_transfer_failed reason={error}");
            self.pack_transfer
                .replace(TransferState::Failed { request, error });
        }
        self.collect_media();
        self.refresh_pack_transfer();
        self.resume_ready_update();
    }

    fn finish_import(self: &Rc<Self>, pack: KnowledgePack, animated: bool) -> AppResult<()> {
        let duplicate = self
            .library
            .borrow()
            .packs
            .iter()
            .any(|item| item.id == pack.id);
        if duplicate {
            self.pending_import.replace(Some(pack));
            if let Some(window) = self.main_window.borrow().as_ref() {
                window.set_animate_import_confirmation(animated);
            }
            self.refresh();
        } else {
            let id = pack.id.clone();
            self.commit(|library| library.import(pack, false))?;
            self.select_imported_pack(&id);
            self.notify(tr!("已导入。"), false);
        }
        Ok(())
    }

    pub(super) fn confirm_import(self: &Rc<Self>) {
        let Some(pack) = self.pending_import.take() else {
            return;
        };
        let replacement = pack.clone();
        let result = self.commit(|library| library.import(pack, true));
        if result.is_ok() {
            self.select_imported_pack(&replacement.id);
        } else {
            self.pending_import.replace(Some(replacement));
            self.refresh();
        }
        self.report(result, tr!("已替换。"));
    }

    fn select_imported_pack(&self, id: &str) {
        let index = self
            .library
            .borrow()
            .packs
            .iter()
            .position(|pack| pack.id == id);
        if let Some(index) = index {
            self.selected_pack.set(index);
            if let Some(window) = self.main_window.borrow().as_ref() {
                window.set_current_tab(0);
                window.invoke_reset_overview();
            }
        }
        self.refresh();
    }

    pub(super) fn retry_pack_transfer(self: &Rc<Self>) {
        let request = match &*self.pack_transfer.borrow() {
            TransferState::Failed { request, .. } => request.clone(),
            _ => return,
        };
        self.start_pack_transfer(request);
    }

    pub(super) fn dismiss_pack_transfer_error(&self) {
        if matches!(&*self.pack_transfer.borrow(), TransferState::Failed { .. }) {
            self.pack_transfer.take();
            self.collect_media();
            self.refresh_pack_transfer();
        }
    }

    fn refresh_pack_transfer(&self) {
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        let state = self.pack_transfer.borrow();
        let message = match &*state {
            TransferState::Idle => "",
            TransferState::Running { request, .. } => request.progress(),
            TransferState::Failed { error, .. } => error,
        };
        window.set_pack_transfer_busy(state.is_running());
        window.set_pack_transfer_failed(matches!(*state, TransferState::Failed { .. }));
        window.set_pack_transfer_message(message.into());
    }
}
