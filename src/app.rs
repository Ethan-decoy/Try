#[cfg(not(target_os = "windows"))]
use crate::SystemAppearance;
use crate::floating_window::{FloatingWindow, ReminderInputGuard};
use crate::formula_presentation;
use crate::{
    AnswerOption, CardRow, FormulaNavigation, FormulaPosition, MainWindow, MathRegion, MathTool,
    PackRow, PracticeWindow, TryTray,
};
use slint::winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent};
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use r#try::autostart::Autostart;
use r#try::card_editor::CardDraft;
use r#try::formula;
use r#try::i18n::{self, InterfaceLanguage};
use r#try::knowledge::{AnswerContent, AnswerMode, AppResult, Appearance, KnowledgePack, Library};
use r#try::learning::{Learner, Origin as LearningOrigin, ReviewToken};
use r#try::math_answer;
use r#try::math_editor::Field;
use r#try::practice::{self, PreparedQuestion, Question, QuestionKey};
use r#try::reminder_schedule::{PracticePause, QuietHours, format_time};
use r#try::storage::Store;
use r#try::text_history::History as InputHistory;
use r#try::tr;
use r#try::update_check::{self, CheckError, CheckOutcome};
use r#try::update_schedule::{self, UpdateSchedule};
use r#try::updater::{self, PreparedUpdate};
use r#try::window_position::PopupCorner;

// Feature modules are private to the application coordinator. Domain logic
// remains in the library modules and the independent math-input dependency.
mod editor;
mod images;
mod main_window;
mod math;
mod packs;
mod practice_window;
mod settings;
mod updates;

type ImageTask = (
    u64,
    std::sync::mpsc::Receiver<AppResult<r#try::media::NormalizedImage>>,
);

type UpdateFetch = fn() -> Result<CheckOutcome, CheckError>;

pub struct App {
    store: Store,
    library: RefCell<Library>,
    learner: RefCell<Learner>,
    pending_learning_error: RefCell<Option<String>>,
    tray: TryTray,
    #[cfg(not(target_os = "windows"))]
    system_appearance: SystemAppearance,
    #[cfg(target_os = "windows")]
    system_dark: Cell<bool>,
    #[cfg(target_os = "windows")]
    system_theme_probe: Cell<fn() -> bool>,
    icon: slint::Image,
    main_window: RefCell<Option<MainWindow>>,
    main_tab: Cell<i32>,
    update_check: Arc<Mutex<UpdateCheck>>,
    update_fetch: Cell<UpdateFetch>,
    automatic_update_timer: Timer,
    update_transfer: Arc<Mutex<UpdateTransfer>>,
    update_transfer_timer: Timer,
    restart_update: RefCell<Option<PreparedUpdate>>,
    practice: RefCell<Option<PracticeSession>>,
    next_practice_id: Cell<u64>,
    pending_import: RefCell<Option<KnowledgePack>>,
    pending_delete: RefCell<Option<String>>,
    pending_card_delete: RefCell<Option<QuestionKey>>,
    editor: RefCell<Option<EditorTarget>>,
    editor_preview_timer: Timer,
    editor_image: RefCell<Option<String>>,
    image_task: RefCell<Option<ImageTask>>,
    image_revision: Cell<u64>,
    image_task_timer: Timer,
    image_preview: RefCell<Option<crate::ImagePreviewWindow>>,
    structured_math: RefCell<Option<(MathTarget, crate::structured_math::Session)>>,
    row_render_timer: Timer,
    pending_rows: RefCell<std::collections::BTreeSet<i32>>,
    math_fields: RefCell<Vec<Field>>,
    math_paths: RefCell<Vec<Vec<usize>>>,
    math_pending: Cell<bool>,
    selected_pack: Cell<usize>,
    knowledge_only: Cell<bool>,
    previous_question: RefCell<Option<QuestionKey>>,
    reminder_timer: Timer,
    reminder_environment: Cell<fn(bool) -> bool>,
    pause_timer: Timer,
    feedback_timer: Timer,
    autostart: RefCell<AppResult<Autostart>>,
    autostart_enabled: Cell<bool>,
    autostart_error: RefCell<String>,
    floating: Rc<FloatingWindow>,
}

#[derive(Clone, Default)]
enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    Finished(Result<CheckOutcome, CheckError>),
}

struct UpdateCheck {
    status: UpdateStatus,
    schedule: UpdateSchedule,
    observer: Option<slint::Weak<MainWindow>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdatePhase {
    Checking,
    Downloading,
}

#[derive(Default)]
enum UpdateTransfer {
    #[default]
    Idle,
    Running {
        phase: UpdatePhase,
        percent: Option<u64>,
        cancel: Arc<AtomicBool>,
    },
    Ready(PreparedUpdate),
    Failed {
        phase: UpdatePhase,
        error: String,
    },
}

impl UpdateCheck {
    fn finish(&mut self, result: Result<CheckOutcome, CheckError>, automatic: bool) {
        // A transient background failure must not discard an already discovered release.
        self.status = if automatic && result.is_err() {
            match self.schedule.cached_outcome() {
                Some(outcome @ CheckOutcome::Available(_)) => UpdateStatus::Finished(Ok(outcome)),
                _ => UpdateStatus::Finished(result),
            }
        } else {
            UpdateStatus::Finished(result)
        };
    }
}

struct PracticeSession {
    diagnostic_id: u64,
    window: PracticeWindow,
    question: Question,
    prepared: PreparedQuestion,
    answered: bool,
    awaiting_interaction: bool,
    input_guard: Option<ReminderInputGuard>,
    review_token: Option<ReviewToken>,
    input_history: RefCell<InputHistory>,
}

enum MathTarget {
    Content {
        index: usize,
        original: r#try::math_editor::Node,
        input: String,
        path: Vec<usize>,
    },
    Practice(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PracticeOrigin {
    Manual,
    Reminder,
}

enum EditorTarget {
    NewPack {
        id: String,
    },
    Card {
        pack_id: String,
        card_id: Option<String>,
    },
}

enum PackMetadata {
    Name,
    Description,
    Tag,
}

impl App {
    pub fn new(
        store: Store,
        library: Library,
        floating: Rc<FloatingWindow>,
    ) -> AppResult<Rc<Self>> {
        i18n::activate(library.settings.interface_language);
        let knowledge_only = library.settings.knowledge_only;
        let icon = make_icon();
        let tray = TryTray::new().map_err(|error| error.to_string())?;
        #[cfg(not(target_os = "windows"))]
        let system_appearance = SystemAppearance::new().map_err(|error| error.to_string())?;
        #[cfg(not(target_os = "windows"))]
        system_appearance
            .global::<crate::Palette>()
            .set_color_scheme(slint::language::ColorScheme::Unknown);
        // AppKit opens an attached tray menu instead of sending a left-click callback.
        tray.set_practice_menu_visible(cfg!(target_os = "macos"));
        i18n::apply_to_slint().map_err(|error| error.to_string())?;
        tray.set_tray_icon(icon.clone());
        let autostart = std::env::current_exe()
            .map_err(|e| tr!("无法定位自启程序：{e}", e = e))
            .and_then(|exe| Autostart::for_current_user(&exe, store.directory()));
        let schedule = UpdateSchedule::open(
            store.directory(),
            env!("CARGO_PKG_VERSION"),
            chrono::Utc::now().timestamp(),
        );
        let status = schedule
            .cached_outcome()
            .map(|outcome| UpdateStatus::Finished(Ok(outcome)))
            .unwrap_or_default();
        let app = Rc::new(Self {
            learner: RefCell::new(Learner::new(&store)),
            pending_learning_error: RefCell::new(None),
            store,
            library: RefCell::new(library),
            tray,
            #[cfg(not(target_os = "windows"))]
            system_appearance,
            #[cfg(target_os = "windows")]
            system_dark: Cell::new(FloatingWindow::system_dark()),
            #[cfg(target_os = "windows")]
            system_theme_probe: Cell::new(FloatingWindow::system_dark),
            icon,
            main_window: RefCell::new(None),
            main_tab: Cell::new(0),
            update_check: Arc::new(Mutex::new(UpdateCheck {
                status,
                schedule,
                observer: None,
            })),
            update_fetch: Cell::new(update_check::check),
            automatic_update_timer: Timer::default(),
            update_transfer: Arc::new(Mutex::new(UpdateTransfer::Idle)),
            update_transfer_timer: Timer::default(),
            restart_update: RefCell::default(),
            practice: RefCell::new(None),
            next_practice_id: Cell::new(1),
            pending_import: RefCell::new(None),
            pending_delete: RefCell::new(None),
            pending_card_delete: RefCell::new(None),
            editor: RefCell::new(None),
            editor_preview_timer: Timer::default(),
            editor_image: RefCell::new(None),
            image_task: RefCell::new(None),
            image_revision: Cell::new(0),
            image_task_timer: Timer::default(),
            image_preview: RefCell::new(None),
            structured_math: RefCell::new(None),
            row_render_timer: Timer::default(),
            pending_rows: RefCell::default(),
            math_fields: RefCell::new(vec![]),
            math_paths: RefCell::new(vec![]),
            math_pending: Cell::new(false),
            selected_pack: Cell::new(0),
            knowledge_only: Cell::new(knowledge_only),
            previous_question: RefCell::new(None),
            reminder_timer: Timer::default(),
            reminder_environment: Cell::new(FloatingWindow::reminders_allowed),
            pause_timer: Timer::default(),
            feedback_timer: Timer::default(),
            autostart: RefCell::new(autostart),
            autostart_enabled: Cell::new(false),
            autostart_error: RefCell::new(String::new()),
            floating,
        });
        // A failed update may roll back JSON. Defer initial media collection
        // until the new process has acknowledged a usable library.
        if !std::env::args_os().any(|s| s == "--complete-update") {
            app.collect_media();
        }
        app.connect_tray();
        app.read_autostart_status();
        app.refresh();
        Ok(app)
    }

    pub fn start(self: &Rc<Self>, background: bool) -> AppResult<()> {
        self.tray.show().map_err(|error| error.to_string())?;
        log::info!(target: "try::app", "event=tray_started background={background}");
        self.arm_pause_expiry();
        if background {
            self.arm_reminder();
        } else {
            self.show_main()?;
        }
        self.arm_automatic_update_check(true);
        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn open_from_launcher(self: &Rc<Self>, practice: bool) {
        let result = if practice {
            self.show_practice()
        } else {
            self.show_main()
        };
        self.report(result, "");
    }

    fn connect_tray(self: &Rc<Self>) {
        #[cfg(not(target_os = "windows"))]
        {
            let weak = Rc::downgrade(self);
            self.system_appearance.on_system_theme_changed(move || {
                if let Some(app) = weak.upgrade() {
                    app.defer_tray_action(|app| app.sync_appearance());
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.tray.on_open_main(move || {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(|app| {
                    let result = app.show_main();
                    app.report(result, "");
                });
            }
        });
        let weak = Rc::downgrade(self);
        self.tray.on_open_settings(move |precise| {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(move |app| {
                    let result = app.show_settings(precise);
                    app.report(result, "");
                });
            }
        });
        let weak = Rc::downgrade(self);
        self.tray.on_open_about(move || {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(|app| {
                    let result = app.show_main();
                    app.report(result, "");
                    if let Some(window) = app.main_window.borrow().as_ref() {
                        window.invoke_show_about();
                    }
                });
            }
        });
        let weak = Rc::downgrade(self);
        self.tray.on_set_pause(move |minutes| {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(move |app| app.set_pause(minutes));
            }
        });
        let weak = Rc::downgrade(self);
        self.tray.on_set_knowledge_mode(move |enabled| {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(move |app| app.set_knowledge_mode(enabled));
            }
        });
        let weak = Rc::downgrade(self);
        self.tray.on_practice_now(move || {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(|app| {
                    let result = app.show_practice();
                    app.report(result, "");
                });
            }
        });
        let weak = Rc::downgrade(self);
        self.tray.on_quit_app(move || {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(|app| app.quit());
            }
        });
    }

    fn defer_tray_action(self: &Rc<Self>, action: impl FnOnce(Rc<Self>) + 'static) {
        let weak = Rc::downgrade(self);
        // Native menus still borrow Slint's menu tree while activating an item.
        // Finish that dispatch before changing menu properties or window owners.
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(app) = weak.upgrade() {
                action(app);
            }
        });
    }

    fn show_settings(self: &Rc<Self>, precise: bool) -> AppResult<()> {
        self.show_main()?;
        if let Some(window) = self.main_window.borrow().as_ref() {
            // A queued menu action can arrive after editing has begun. Respect
            // the same navigation guard as the main window without discarding it.
            if !window.get_tray_settings_available() {
                return Ok(());
            }
            window.set_current_tab(1);
            if precise {
                let weak = window.as_weak();
                Timer::single_shot(Duration::ZERO, move || {
                    if let Some(window) = weak.upgrade() {
                        window.invoke_show_pause_details();
                    }
                });
            }
        }
        Ok(())
    }

    fn refresh_tray_navigation(&self) {
        let available = self
            .main_window
            .borrow()
            .as_ref()
            .is_none_or(|window| window.get_tray_settings_available());
        self.tray.set_settings_available(available);
    }

    fn commit(self: &Rc<Self>, edit: impl FnOnce(&mut Library) -> AppResult<()>) -> AppResult<()> {
        let mut candidate = self.library.borrow().clone();
        edit(&mut candidate)?;
        let language_changed = candidate.settings.interface_language
            != self.library.borrow().settings.interface_language;
        self.store.save(&candidate)?;
        self.knowledge_only.set(candidate.settings.knowledge_only);
        self.floating.configure(&candidate.settings);
        self.library.replace(candidate);
        self.collect_media();
        if !self.knowledge_only.get()
            && let Err(error) = self.learner.borrow_mut().reconcile(&self.library.borrow())
        {
            self.pending_learning_error
                .replace(Some(tr!("练习记录未能保存：{error}", error = error)));
        }
        if language_changed {
            i18n::activate(self.library.borrow().settings.interface_language);
            i18n::apply_to_slint().map_err(|error| error.to_string())?;
            self.read_autostart_status();
            if let Some(window) = self.main_window.borrow().as_ref() {
                window.set_math_functions(Self::palette_model(r#try::math_palette::FUNCTIONS));
                window.set_math_symbols(Self::palette_model(r#try::math_palette::SYMBOLS));
                window.set_message("".into());
            }
            log::info!(target: "try::app", "event=interface_language_changed preference={:?}", self.library.borrow().settings.interface_language);
        }
        self.refresh();
        self.arm_pause_expiry();
        self.arm_reminder();
        Ok(())
    }

    fn notify(&self, message: &str, is_error: bool) {
        if is_error {
            log::warn!(target: "try::app", "event=operation_rejected reason={message}");
        }
        self.show_message(message, is_error);
    }

    fn show_message(&self, message: &str, is_error: bool) {
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_message(message.into());
            window.set_message_is_error(is_error);
        } else if is_error {
            rfd::MessageDialog::new()
                .set_title("Try")
                .set_description(message)
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
    }

    fn report(&self, result: AppResult<()>, success_message: &str) {
        match result {
            Ok(()) => {
                if let Some(error) = self.pending_learning_error.borrow_mut().take() {
                    self.notify(&error, true);
                } else {
                    self.notify(success_message, false);
                }
            }
            Err(error) => {
                self.refresh();
                self.notify(&error, true);
            }
        }
    }

    fn quit(self: &Rc<Self>) {
        self.close_structured_math();
        self.close_image_preview();
        log::info!(target: "try::app", "event=quit_requested");
        self.close_practice();
        self.reminder_timer.stop();
        self.pause_timer.stop();
        self.feedback_timer.stop();
        self.automatic_update_timer.stop();
        self.update_transfer_timer.stop();
        if let UpdateTransfer::Running { cancel, .. } = &*self.update_transfer.lock().unwrap() {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(window) = self.main_window.borrow_mut().take() {
            let _ = window.hide();
        }
        if let Some(session) = self.practice.borrow_mut().take() {
            let _ = session.window.hide();
        }
        let _ = self.tray.hide();
        let _ = slint::quit_event_loop();
    }
}

fn preset_value(index: i32, presets: &[i32], current: i32) -> Option<i32> {
    let index = usize::try_from(index).ok()?;
    presets
        .get(index)
        .copied()
        .or_else(|| (index == presets.len() && !presets.contains(&current)).then_some(current))
}

fn preset_options(
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

fn make_icon() -> slint::Image {
    // Build from the canonical SVG, then share 4 KiB of pixels across tray and
    // windows. No file access or SVG decoding is needed while the app runs.
    const SIZE: u32 = 32;
    const PIXELS: &[u8; 4096] = include_bytes!(concat!(env!("OUT_DIR"), "/logo_rgba.bin"));
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(SIZE, SIZE);
    buffer.make_mut_bytes().copy_from_slice(PIXELS);
    slint::Image::from_rgba8(buffer)
}

#[cfg(all(test, feature = "native_checks", target_os = "windows"))]
#[path = "native_checks/mod.rs"]
mod native_checks;

#[cfg(all(test, feature = "native_checks", target_os = "windows"))]
#[path = "native_picture_checks.rs"]
mod native_picture_checks;

#[cfg(all(test, feature = "native_checks", target_os = "windows"))]
#[path = "tray_checks.rs"]
mod tray_checks;
