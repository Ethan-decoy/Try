#[cfg(not(target_os = "windows"))]
use crate::SystemAppearance;
use crate::floating_window::FloatingWindow;
use crate::formula_presentation;
use crate::{
    AnswerOption, CardRow, FormulaNavigation, FormulaPosition, MainWindow, MathRegion, MathTool,
    PackRow, PracticeWindow, TryTray,
};
use slint::winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent};
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
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
use r#try::math_input::{History as InputHistory, Layout as InputLayout, Rect as InputRect};
use r#try::practice::{self, PreparedQuestion, Question, QuestionKey};
use r#try::reminder_schedule::{PracticePause, QuietHours, format_time};
use r#try::storage::Store;
use r#try::tr;
use r#try::update_check::{self, CheckError, CheckOutcome};
use r#try::update_schedule::{self, UpdateSchedule};
use r#try::window_position::PopupCorner;

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
    answer_preview_timer: Timer,
    row_render_timer: Timer,
    pending_rows: RefCell<std::collections::BTreeSet<i32>>,
    math_fields: RefCell<Vec<Field>>,
    math_paths: RefCell<Vec<Vec<usize>>>,
    math_pending: Cell<bool>,
    selected_pack: Cell<usize>,
    knowledge_only: Cell<bool>,
    previous_question: RefCell<Option<QuestionKey>>,
    reminder_timer: Timer,
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
    browser_error: bool,
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
    review_token: Option<ReviewToken>,
    entry: Option<AnswerEditor>,
    inline_input: Option<InputLayout>,
    input_history: RefCell<InputHistory>,
    initial_position: Cell<Option<(i32, i32)>>,
}

struct AnswerEditor {
    field: Field,
    paths: Vec<Vec<usize>>,
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
                browser_error: false,
            })),
            update_fetch: Cell::new(update_check::check),
            automatic_update_timer: Timer::default(),
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
            answer_preview_timer: Timer::default(),
            row_render_timer: Timer::default(),
            pending_rows: RefCell::default(),
            math_fields: RefCell::new(vec![]),
            math_paths: RefCell::new(vec![]),
            math_pending: Cell::new(false),
            selected_pack: Cell::new(0),
            knowledge_only: Cell::new(false),
            previous_question: RefCell::new(None),
            reminder_timer: Timer::default(),
            pause_timer: Timer::default(),
            feedback_timer: Timer::default(),
            autostart: RefCell::new(autostart),
            autostart_enabled: Cell::new(false),
            autostart_error: RefCell::new(String::new()),
            floating,
        });
        app.collect_media();
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

    fn show_main(self: &Rc<Self>) -> AppResult<()> {
        if self.practice.borrow().is_some() {
            self.close_practice();
        }
        self.reminder_timer.stop();
        self.read_autostart_status();
        self.refresh();
        if let Some(window) = self.main_window.borrow().as_ref() {
            log::debug!(target: "try::app", "event=main_reopened");
            window.window().set_minimized(false);
            FloatingWindow::activate(window.window());
            return window.show().map_err(|error| error.to_string());
        }
        let window = MainWindow::new().map_err(|error| error.to_string())?;
        let weak = Rc::downgrade(self);
        let applied_theme = Cell::new(None);
        window.window().on_winit_window_event(move |window, event| {
            crate::window_repaint::prepare(window, event);
            if let WindowEvent::DroppedFile(path) = event
                && let Some(app) = weak.upgrade()
            {
                app.import_question_image(path.clone());
            }
            if matches!(
                event,
                WindowEvent::RedrawRequested | WindowEvent::ThemeChanged(_)
            ) && let Some(app) = weak.upgrade()
            {
                app.apply_native_theme(
                    window,
                    &applied_theme,
                    matches!(event, WindowEvent::ThemeChanged(_)),
                );
            }
            EventResult::Propagate
        });
        window.set_math_functions(Self::palette_model(r#try::math_palette::FUNCTIONS));
        window.set_math_symbols(Self::palette_model(r#try::math_palette::SYMBOLS));
        log::debug!(target: "try::app", "event=main_created");
        window.set_app_icon(self.icon.clone());
        window.set_app_version(env!("CARGO_PKG_VERSION").into());
        window.set_current_tab(self.main_tab.get());

        let weak = Rc::downgrade(self);
        window.on_tray_navigation_changed(move || {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(|app| app.refresh_tray_navigation());
            }
        });

        let weak = Rc::downgrade(self);
        window.on_check_updates(move || {
            if let Some(app) = weak.upgrade() {
                app.begin_update_check(app.update_fetch.get());
            }
        });
        let weak = Rc::downgrade(self);
        window.on_update_check_finished(move || {
            if let Some(app) = weak.upgrade() {
                app.refresh_update_status();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_open_update_download(move || {
            if let Some(app) = weak.upgrade() {
                app.open_update_download();
            }
        });

        let weak = Rc::downgrade(self);
        window.on_open_project_home(move || {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(|app| {
                    if let Err(error) = webbrowser::open("https://github.com/Ethan-decoy/Try") {
                        log::warn!(target: "try::app", "event=project_home_open_failed reason={error}");
                        app.notify(tr!("无法打开浏览器"), true);
                    }
                });
            }
        });

        let weak = Rc::downgrade(self);
        window.on_open_notices(move || {
            if let Some(app) = weak.upgrade() {
                // Wait for the popup's pointer handler and close to finish.
                app.defer_tray_action(|app| {
                    if let Some(window) = app.main_window.borrow().as_ref() {
                        window.invoke_show_notices();
                    }
                });
            }
        });

        let weak = Rc::downgrade(self);
        window.on_return_to_about(move || {
            if let Some(app) = weak.upgrade() {
                app.defer_tray_action(|app| {
                    if let Some(window) = app.main_window.borrow().as_ref() {
                        window.invoke_show_about();
                    }
                });
            }
        });

        let weak = Rc::downgrade(self);
        window.on_select_pack(move |index| {
            if let Some(app) = weak.upgrade()
                && index >= 0
                && (index as usize) < app.library.borrow().packs.len()
            {
                app.selected_pack.set(index as usize);
                if let Some(window) = app.main_window.borrow().as_ref() {
                    window.set_current_tab(0);
                    window.invoke_reset_overview();
                }
                app.refresh();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_toggle_pack(move |id, enabled| {
            if let Some(app) = weak.upgrade() {
                app.set_pack_enabled(id.as_str(), enabled);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_toggle_all_cards(move |enabled| {
            if let Some(app) = weak.upgrade() {
                app.set_all_cards_enabled(enabled);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_toggle_card(move |index, enabled| {
            if let Some(app) = weak.upgrade() {
                app.set_card_enabled(index, enabled);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_edit_card(move |index| {
            if let Some(app) = weak.upgrade() {
                app.open_editor(Some(index), false, None);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_add_card(move || {
            if let Some(app) = weak.upgrade() {
                app.open_editor(None, false, None);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_create_pack(move || {
            if let Some(app) = weak.upgrade() {
                app.open_editor(None, true, None);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_save_editor(move || {
            if let Some(app) = weak.upgrade() {
                app.save_editor();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_cancel_editor(move || {
            if let Some(app) = weak.upgrade() {
                app.close_editor();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_delete_card(move || {
            if let Some(app) = weak.upgrade() {
                app.request_delete_card();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_confirm_card_delete(move || {
            if let Some(app) = weak.upgrade() {
                app.confirm_delete_card();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_cancel_card_delete(move || {
            if let Some(app) = weak.upgrade() {
                app.cancel_delete_card();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_preview_editor(move || {
            if let Some(app) = weak.upgrade() {
                let weak = Rc::downgrade(&app);
                app.editor_preview_timer.start(
                    TimerMode::SingleShot,
                    Duration::from_millis(250),
                    move || {
                        if let Some(app) = weak.upgrade() {
                            app.render_editor_preview();
                        }
                    },
                );
            }
        });
        let weak = Rc::downgrade(self);
        window.on_request_row_formula(move |index| {
            if let Some(app) = weak.upgrade() {
                app.queue_row_render(index);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_save_quick_answer(move |pack_id, card_id, input| {
            weak.upgrade()
                .map_or_else(
                    || tr!("窗口已关闭。").into(),
                    |app| {
                        app.save_quick_answer(pack_id.as_str(), card_id.as_str(), input.as_str())
                            .err()
                            .unwrap_or_default()
                    },
                )
                .into()
        });
        let weak = Rc::downgrade(self);
        window.on_prepare_quick_options(move |pack_id, card_id| {
            if let Some(app) = weak.upgrade() {
                app.prepare_quick_options(pack_id.as_str(), card_id.as_str());
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_quick_answer(move |pack_id, card_id, mode, index| {
            weak.upgrade()
                .map_or_else(
                    || tr!("窗口已关闭。").into(),
                    |app| {
                        app.choose_quick_answer(pack_id.as_str(), card_id.as_str(), mode, index)
                            .err()
                            .unwrap_or_default()
                    },
                )
                .into()
        });
        let weak = Rc::downgrade(self);
        window.on_activate_content(move |field| {
            if let Some(app) = weak.upgrade() {
                app.activate_content(field);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_edit_math_input(move |_| {
            if let Some(app) = weak.upgrade() {
                app.math_pending.set(true);
                // Record every draft immediately, including incomplete input.
                // Only the expensive preview waits for the debounce timer.
                app.flush_math();
                let weak = Rc::downgrade(&app);
                app.editor_preview_timer.start(
                    TimerMode::SingleShot,
                    Duration::from_millis(180),
                    move || {
                        if let Some(app) = weak.upgrade() {
                            app.refresh_math(false);
                        }
                    },
                );
            }
        });
        let weak = Rc::downgrade(self);
        window.on_math_position_changed(move |input, anchor, caret| {
            if let Some(app) = weak.upgrade()
                && let Some(window) = app.main_window.borrow().as_ref()
                && let Ok(index) = usize::try_from(window.get_math_active_field())
                && let Some(field) = app.math_fields.borrow_mut().get_mut(index)
                && field.input() == input.as_str()
            {
                field.set_text_selection(anchor.max(0) as usize, caret.max(0) as usize);
                let (position, navigation) =
                    editing_state(field, window.get_math_position().revision, false);
                window.set_math_position(position);
                window.set_math_navigation(navigation);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_select_math_region(move |index| {
            if let Some(app) = weak.upgrade()
                && app.flush_math()
            {
                let path = usize::try_from(index)
                    .ok()
                    .and_then(|i| app.math_paths.borrow().get(i).cloned());
                if let Some(path) = path {
                    app.modify_math(|field| {
                        field.select(path);
                        Ok(())
                    });
                }
            }
        });
        let weak = Rc::downgrade(self);
        window.on_math_tool(move |name| {
            if let Some(app) = weak.upgrade() {
                if name == "undo" || name == "redo" {
                    app.math_pending.set(false);
                    app.editor_preview_timer.stop();
                    app.modify_math(|field| {
                        field.undo(name == "redo");
                        Ok(())
                    });
                } else if name == "clear" {
                    app.math_pending.set(false);
                    app.modify_math(|field| field.insert_at("clear"));
                } else if name == "backspace" {
                    app.math_pending.set(false);
                    app.modify_math(|field| {
                        // A deletion may leave an incomplete draft. Publish it
                        // and its own error so another deletion can repair it.
                        let _ = field.backspace();
                        Ok(())
                    });
                } else if app.flush_math() {
                    app.modify_math(|field| {
                        if ["parent", "previous", "next", "exit", "up", "down"]
                            .contains(&name.as_str())
                        {
                            field.navigate(name.as_str());
                            Ok(())
                        } else {
                            field.insert_at(name.as_str())
                        }
                    });
                }
            }
        });
        let weak = Rc::downgrade(self);
        window.on_math_parent(move || {
            if let Some(app) = weak.upgrade()
                && app.flush_math()
            {
                app.modify_math(|field| {
                    field.navigate("parent");
                    Ok(())
                });
            }
        });
        let weak = Rc::downgrade(self);
        window.on_undo_math(move |redo| {
            if let Some(app) = weak.upgrade() {
                app.math_pending.set(false);
                app.editor_preview_timer.stop();
                app.modify_math(|field| {
                    field.undo(redo);
                    Ok(())
                });
            }
        });
        let weak = Rc::downgrade(self);
        window.on_resize_math_grid(move |rows, columns| {
            if let Some(app) = weak.upgrade()
                && app.flush_math()
            {
                app.modify_math(|field| {
                    field.resize_grid(rows.max(1) as usize, columns.max(1) as usize)
                });
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_all_answer_modes(move |index| {
            if let Some(app) = weak.upgrade() {
                let Some(mode) = usize::try_from(index)
                    .ok()
                    .and_then(|index| AnswerMode::ALL.get(index).copied())
                else {
                    return;
                };
                let selection = app
                    .library
                    .borrow()
                    .packs
                    .get(app.selected_pack.get())
                    .map(|pack| pack.id.clone());
                if let Some(id) = selection {
                    let result = app.commit(|library| library.set_all_answer_modes(&id, mode));
                    app.report(result, "");
                }
            }
        });
        let weak = Rc::downgrade(self);
        window.on_toggle_knowledge_mode(move || {
            if let Some(app) = weak.upgrade() {
                app.knowledge_only.set(!app.knowledge_only.get());
                app.refresh();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_save_pack_name(move || {
            if let Some(app) = weak.upgrade() {
                app.save_pack_metadata(PackMetadata::Name);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_save_pack_description(move || {
            if let Some(app) = weak.upgrade() {
                app.save_pack_metadata(PackMetadata::Description);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_save_pack_tag(move || {
            if let Some(app) = weak.upgrade() {
                app.save_pack_metadata(PackMetadata::Tag);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_card_answer_mode(move |row, mode| {
            if let Some(app) = weak.upgrade() {
                app.set_card_answer_mode(row, mode);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_editor_answer_mode(move |index| {
            if let Some(app) = weak.upgrade() {
                app.set_editor_answer_mode(index);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_question_image(move || {
            if let Some(app) = weak.upgrade() {
                app.choose_question_image();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_remove_question_image(move || {
            if let Some(app) = weak.upgrade() {
                let _ = app.set_editor_image(None);
                app.close_image_preview();
                app.collect_media();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_preview_question_image(move || {
            if let Some(app) = weak.upgrade() {
                let image = app
                    .main_window
                    .borrow()
                    .as_ref()
                    .map(|w| w.get_editor_picture());
                if let Some(image) = image {
                    app.preview_image(image);
                }
            }
        });
        let weak = Rc::downgrade(self);
        window.on_import_pack(move || {
            if let Some(app) = weak.upgrade() {
                app.import_pack();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_export_pack(move || {
            if let Some(app) = weak.upgrade() {
                app.export_pack();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_delete_pack(move || {
            if let Some(app) = weak.upgrade() {
                app.request_delete_pack();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_confirm_delete(move || {
            if let Some(app) = weak.upgrade() {
                app.confirm_delete_pack();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_cancel_delete(move || {
            if let Some(app) = weak.upgrade() {
                app.pending_delete.borrow_mut().take();
                app.refresh();
                app.notify("", false);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_confirm_import(move || {
            if let Some(app) = weak.upgrade() {
                app.confirm_import();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_cancel_import(move || {
            if let Some(app) = weak.upgrade() {
                app.pending_import.borrow_mut().take();
                app.collect_media();
                app.refresh();
                app.notify("", false);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_pause(move |minutes| {
            if let Some(app) = weak.upgrade() {
                app.set_pause(minutes);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_interval(move |index| {
            if let Some(app) = weak.upgrade() {
                let current = app.library.borrow().settings.interval_minutes as i32;
                if let Some(value) = preset_value(index, &[15, 25, 30, 45], current) {
                    app.set_interval(value);
                }
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_feedback(move |index| {
            if let Some(app) = weak.upgrade() {
                let current = app.library.borrow().settings.feedback_seconds as i32;
                if let Some(value) = preset_value(index, &[1, 2, 3], current) {
                    app.set_feedback(value);
                }
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_pause(move |index| {
            if let Some(app) = weak.upgrade() {
                let current = app
                    .library
                    .borrow()
                    .settings
                    .pause
                    .minutes(chrono::Utc::now().timestamp());
                if let Some(value) = preset_value(index, &[0, 60, 120, 180, -1], current) {
                    app.set_pause(value);
                }
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_interval(move |value| {
            if let Some(app) = weak.upgrade() {
                app.set_interval(value);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_feedback(move |value| {
            if let Some(app) = weak.upgrade() {
                app.set_feedback(value);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_autostart(move |enabled| {
            if let Some(app) = weak.upgrade() {
                app.set_autostart(enabled);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_language(move |index| {
            if let Some(app) = weak.upgrade()
                && let Some(language) = InterfaceLanguage::from_index(index)
            {
                let result = app.commit(|library| {
                    library.settings.interface_language = language;
                    Ok(())
                });
                app.report(result, "");
            }
        });
        let weak = Rc::downgrade(self);
        window.on_choose_appearance(move |index| {
            if let Some(app) = weak.upgrade()
                && let Some(appearance) = Appearance::from_index(index)
            {
                let result = app.commit(|library| {
                    library.settings.appearance = appearance;
                    Ok(())
                });
                app.report(result, "");
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_quiet_enabled(move |enabled| {
            if let Some(app) = weak.upgrade() {
                app.set_quiet_enabled(enabled);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_quiet_range(move |start, end| {
            if let Some(app) = weak.upgrade() {
                app.set_quiet_range(start.as_str(), end.as_str());
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_corner(move |index| {
            if let Some(app) = weak.upgrade()
                && let Some(corner) = PopupCorner::from_index(index)
            {
                let result = app.commit(|library| {
                    library.settings.popup_corner = corner;
                    library.settings.popup_position = None;
                    Ok(())
                });
                app.report(result, "");
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_remember_position(move |enabled| {
            if let Some(app) = weak.upgrade() {
                let result = app.commit(|library| {
                    library.settings.remember_position = enabled;
                    Ok(())
                });
                app.report(result, "");
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_show_pack_tags(move |enabled| {
            if let Some(app) = weak.upgrade() {
                let result = app.commit(|library| {
                    library.settings.show_pack_tags = enabled;
                    Ok(())
                });
                app.report(result, "");
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_focus_on_reminder(move |enabled| {
            if let Some(app) = weak.upgrade() {
                let result = app.commit(|library| {
                    library.settings.focus_on_reminder = enabled;
                    Ok(())
                });
                app.report(result, "");
            }
        });
        let weak = Rc::downgrade(self);
        window.on_set_auto_check_updates(move |enabled| {
            if let Some(app) = weak.upgrade() {
                let result = app.commit(|library| {
                    library.settings.auto_check_updates = enabled;
                    Ok(())
                });
                if result.is_ok() {
                    app.arm_automatic_update_check(true);
                }
                app.report(result, "");
            }
        });
        let weak = Rc::downgrade(self);
        window.on_go_background(move || {
            if let Some(app) = weak.upgrade() {
                app.close_main();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_quit_app(move || {
            if let Some(app) = weak.upgrade() {
                app.quit();
            }
        });
        let weak = Rc::downgrade(self);
        window.window().on_close_requested(move || {
            let weak = weak.clone();
            Timer::single_shot(Duration::ZERO, move || {
                if let Some(app) = weak.upgrade() {
                    app.close_main();
                }
            });
            slint::CloseRequestResponse::HideWindow
        });

        self.main_window.replace(Some(window.clone_strong()));
        self.update_check.lock().unwrap().observer = Some(window.as_weak());
        self.refresh();
        if let Err(error) = window.show() {
            self.main_window.borrow_mut().take();
            self.update_check.lock().unwrap().observer = None;
            self.arm_reminder();
            return Err(error.to_string());
        }
        log::debug!(target: "try::app", "event=main_shown");
        Ok(())
    }

    fn close_main(self: &Rc<Self>) {
        self.close_image_preview();
        self.editor_image.borrow_mut().take();
        self.image_revision
            .set(self.image_revision.get().wrapping_add(1));
        self.row_render_timer.stop();
        self.pending_rows.borrow_mut().clear();
        self.editor_preview_timer.stop();
        self.editor.borrow_mut().take();
        self.math_fields.borrow_mut().clear();
        self.math_paths.borrow_mut().clear();
        self.math_pending.set(false);
        self.update_check.lock().unwrap().observer = None;
        let window = self.main_window.borrow_mut().take();
        if let Some(window) = window {
            self.main_tab.set(window.get_current_tab());
            let _ = window.hide();
            log::debug!(target: "try::app", "event=main_closed");
        }
        self.pending_import.borrow_mut().take();
        self.pending_delete.borrow_mut().take();
        self.pending_card_delete.borrow_mut().take();
        self.refresh_tray_navigation();
        self.collect_media();
        self.arm_reminder();
    }

    fn arm_automatic_update_check(self: &Rc<Self>, startup: bool) {
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

    fn begin_automatic_update_check(
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

    fn begin_update_check(
        self: &Rc<Self>,
        fetch: impl FnOnce() -> Result<CheckOutcome, CheckError> + Send + 'static,
    ) {
        self.begin_update_check_with_origin(fetch, false);
    }

    fn begin_update_check_with_origin(
        self: &Rc<Self>,
        fetch: impl FnOnce() -> Result<CheckOutcome, CheckError> + Send + 'static,
        automatic: bool,
    ) {
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
            check.browser_error = false;
        }
        self.refresh_update_status();
        let state = self.update_check.clone();
        let worker = std::thread::Builder::new()
            .name("try_update_check".into())
            .spawn(move || {
                log::debug!(target: "try::updates", "event=check_started automatic={automatic}");
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(fetch))
                    .unwrap_or(Err(CheckError::Network));
                match &result {
                    Ok(outcome) => log::info!(target: "try::updates", "event=check_finished outcome={outcome:?}"),
                    Err(error) => log::warn!(target: "try::updates", "event=check_failed reason={error}"),
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

    fn refresh_update_status(&self) {
        let window = self.main_window.borrow();
        let Some(window) = window.as_ref() else {
            return;
        };
        let check = self.update_check.lock().unwrap();
        let (state, text, download) = match &check.status {
            UpdateStatus::Idle => (0, String::new(), false),
            UpdateStatus::Checking => (1, tr!("正在检查…").to_owned(), false),
            UpdateStatus::Finished(Ok(CheckOutcome::Available(release))) => (
                2,
                if check.browser_error {
                    tr!("无法打开浏览器").to_owned()
                } else {
                    tr!("新版本 {version}", version = release.version)
                },
                true,
            ),
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
        window.set_update_state(state);
        window.set_update_status(text.into());
        window.set_update_download(download);
    }

    fn open_update_download(&self) {
        let url = {
            let check = self.update_check.lock().unwrap();
            match &check.status {
                UpdateStatus::Finished(Ok(CheckOutcome::Available(release))) => release.page_url(),
                _ => return,
            }
        };
        let result = webbrowser::open(&url);
        if let Err(error) = &result {
            log::warn!(target: "try::updates", "event=browser_open_failed reason={error}");
        }
        self.update_check.lock().unwrap().browser_error = result.is_err();
        self.refresh_update_status();
    }

    fn resolved_dark_theme(&self, appearance: Appearance) -> bool {
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

    fn sync_appearance(&self) {
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
    }

    fn apply_native_theme(
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
    fn refresh_system_appearance(&self) {
        self.system_dark.set((self.system_theme_probe.get())());
        self.sync_appearance();
    }

    fn refresh(&self) {
        self.refresh_update_status();
        self.refresh_tray_navigation();
        let library = self.library.borrow();
        if let Some(session) = self.practice.borrow().as_ref() {
            let dark = self.resolved_dark_theme(library.settings.appearance);
            if session.window.get_dark_theme() != dark {
                session.window.set_dark_theme(dark);
                self.floating.apply_theme(session.window.window(), dark);
            }
        }
        let count = library.enabled_card_count();
        let now = chrono::Local::now();
        let pause_minutes = library.settings.pause.minutes(now.timestamp());
        let pause_end = match &library.settings.pause {
            PracticePause::Timed { until, .. } if pause_minutes > 0 => {
                let end = chrono::DateTime::from_timestamp(*until, 0)
                    .expect("validated pause")
                    .with_timezone(&chrono::Local);
                let pattern = if end.date_naive() == now.date_naive() {
                    "%H:%M"
                } else {
                    "%m-%d %H:%M"
                };
                Some(end.format(pattern).to_string())
            }
            _ => None,
        };
        self.tray.set_reminders_enabled(pause_minutes == 0);
        self.tray.set_can_practice(count > 0);
        self.tray.set_pause_minutes(pause_minutes);
        self.tray.set_pause_title(match &pause_end {
            Some(end) => tr!("暂离至 {p0}", p0 = end).into(),
            _ => tr!("暂离").into(),
        });
        let window = self.main_window.borrow();
        let Some(window) = window.as_ref() else {
            return;
        };
        let rows = library
            .packs
            .iter()
            .map(|pack| {
                let participating = library.pack_enabled_card_count(pack);
                PackRow {
                    id: pack.id.clone().into(),
                    name: pack.name.clone().into(),
                    tag: pack.tag.trim().into(),
                    participating: participating > 0,
                    has_cards: !pack.cards.is_empty(),
                    detail: if participating > 0 {
                        tr!("{p0} / {p1} 题", p0 = participating, p1 = pack.cards.len())
                    } else {
                        tr!("{p0} 题 · 未参与", p0 = pack.cards.len())
                    }
                    .into(),
                }
            })
            .collect::<Vec<_>>();
        let packs = window.get_packs();
        if let Some(model) = packs.as_any().downcast_ref::<VecModel<PackRow>>()
            && model.row_count() == rows.len()
        {
            for (index, row) in rows.into_iter().enumerate() {
                if model.row_data(index).as_ref() != Some(&row) {
                    model.set_row_data(index, row);
                }
            }
        } else {
            window.set_packs(ModelRc::new(VecModel::from(rows)));
        }
        window.set_participation_summary(
            tr!(
                "{p0} 个包 · {p1} 道题",
                p0 = library
                    .packs
                    .iter()
                    .filter(|pack| library.pack_enabled_card_count(pack) > 0)
                    .count(),
                p1 = library.enabled_card_count()
            )
            .into(),
        );
        let index = self
            .selected_pack
            .get()
            .min(library.packs.len().saturating_sub(1));
        self.selected_pack.set(index);
        window.set_selected_index(index as i32);
        window.set_has_selection(!library.packs.is_empty());
        if let Some(pack) = library.packs.get(index) {
            window.set_selected_name(pack.name.clone().into());
            window.set_selected_description(pack.description.clone().into());
            window.set_selected_tag(pack.tag.trim().into());
            let enabled = library.pack_enabled_card_count(pack);
            window.set_selected_enabled(!pack.cards.is_empty() && enabled == pack.cards.len());
            window.set_selection_partial(enabled > 0 && enabled < pack.cards.len());
            window.set_selected_card_count(format!("{enabled} / {}", pack.cards.len()).into());
            let existing = window.get_cards();
            let card_rows = pack
                .cards
                .iter()
                .enumerate()
                .map(|(index, card)| {
                    let mode =
                        library.card_answer_mode(&pack.id, card.id.as_deref().unwrap_or_default());
                    let display = if self.knowledge_only.get() {
                        PreparedQuestion::knowledge(card)
                    } else {
                        let display_mode = if mode == AnswerMode::TrueFalse {
                            mode
                        } else {
                            AnswerMode::Input
                        };
                        PreparedQuestion::new(card, display_mode, &mut rand::rng())
                            .expect("the selected mode has validated content")
                    };
                    let source = display.prompt.latex.unwrap_or_default();
                    let answer = if mode == AnswerMode::TrueFalse {
                        if card
                            .true_false
                            .as_ref()
                            .is_some_and(|statement| statement.is_true)
                        {
                            "✓".into()
                        } else {
                            "✕".into()
                        }
                    } else {
                        card.answer.clone()
                    };
                    let answer_source = if mode == AnswerMode::TrueFalse {
                        String::new()
                    } else {
                        card.answer_latex.clone().unwrap_or_default()
                    };
                    let cached = existing.row_data(index).filter(|row| {
                        row.prompt.as_str() == display.prompt.text
                            && row.formula_source.as_str() == source
                            && row.answer.as_str() == answer
                            && row.answer_formula_source.as_str() == answer_source
                    });
                    CardRow {
                        id: card.id.clone().unwrap_or_default().into(),
                        pack_id: pack.id.clone().into(),
                        prompt: display.prompt.text.into(),
                        quick_answer: card.answer.clone().into(),
                        truth: card
                            .true_false
                            .as_ref()
                            .is_some_and(|statement| statement.is_true),
                        answer: answer.into(),
                        answer_formula_source: answer_source.into(),
                        answer_preview: cached
                            .as_ref()
                            .map(|row| row.answer_preview.clone())
                            .unwrap_or_default(),
                        choice_available: card.supports(AnswerMode::MultipleChoice),
                        judgment_available: card.supports(AnswerMode::TrueFalse),
                        enabled: library
                            .is_card_enabled(&pack.id, card.id.as_deref().unwrap_or_default()),
                        answer_mode: library
                            .card_answer_mode(&pack.id, card.id.as_deref().unwrap_or_default())
                            .index(),
                        formula_source: source.into(),
                        rendered: cached.as_ref().is_some_and(|row| row.rendered),
                        formula_scale: cached.as_ref().map_or(0.0, |row| row.formula_scale),
                        preview: cached.map(|row| row.preview).unwrap_or_default(),
                    }
                })
                .collect::<Vec<_>>();
            let model = window.get_cards();
            if let Some(model) = model.as_any().downcast_ref::<VecModel<CardRow>>()
                && model.row_count() == card_rows.len()
            {
                // Keep the same row components and keyboard focus when a box changes.
                for (index, row) in card_rows.into_iter().enumerate() {
                    if model.row_data(index).as_ref() != Some(&row) {
                        model.set_row_data(index, row);
                    }
                }
            } else {
                window.set_cards(ModelRc::new(VecModel::from(card_rows)));
            }
            window.set_uniform_answer_mode(
                library
                    .uniform_answer_mode(&pack.id)
                    .map_or(-1, AnswerMode::index),
            );
            window.set_all_choice_available(pack.supports(AnswerMode::MultipleChoice));
            window.set_all_judgment_available(pack.supports(AnswerMode::TrueFalse));
        } else {
            window.set_selected_name("".into());
            window.set_selected_description("".into());
            window.set_selected_tag("".into());
            window.set_selected_card_count("".into());
            window.set_selected_enabled(false);
            window.set_selection_partial(false);
            window.set_cards(ModelRc::default());
            window.set_quick_options(ModelRc::default());
            window.set_uniform_answer_mode(-1);
            window.set_all_choice_available(false);
            window.set_all_judgment_available(false);
        }
        window.set_knowledge_only(self.knowledge_only.get());
        let dark = self.resolved_dark_theme(library.settings.appearance);
        if window.get_dark_theme() != dark {
            window.set_dark_theme(dark);
            self.floating.apply_theme(window.window(), dark);
        }
        window.set_appearance(library.settings.appearance.index());
        window.set_interface_language(library.settings.interface_language.index());
        window.set_interval_minutes(library.settings.interval_minutes as i32);
        let (options, index) = preset_options(
            library.settings.interval_minutes as i32,
            &[15, 25, 30, 45],
            |n| tr!("{n} 分钟", n = n),
        );
        window.set_interval_options(options);
        window.set_interval_selection(index);
        window.set_feedback_seconds(library.settings.feedback_seconds as i32);
        let (options, index) =
            preset_options(library.settings.feedback_seconds as i32, &[1, 2, 3], |n| {
                tr!("{n} 秒", n = n)
            });
        window.set_feedback_options(options);
        window.set_feedback_selection(index);
        let (options, index) = preset_options(pause_minutes, &[0, 60, 120, 180, -1], |n| match n {
            0 => tr!("不暂停").into(),
            -1 => tr!("直到恢复").into(),
            60 | 120 | 180 => tr!("{p0} 小时", p0 = n / 60),
            _ => tr!("{n} 分钟", n = n),
        });
        window.set_pause_options(options);
        window.set_pause_selection(index);
        window.set_pause_minutes(pause_minutes);
        window.set_pause_detail(match &pause_end {
            Some(end) => tr!("至 {p0}", p0 = end).into(),
            None if pause_minutes == -1 => tr!("等待恢复").into(),
            _ => "".into(),
        });
        window.set_autostart_available(self.autostart.borrow().is_ok());
        window.set_autostart_enabled(self.autostart_enabled.get());
        window.set_autostart_error(self.autostart_error.borrow().clone().into());
        let quiet = &library.settings.quiet_hours;
        window.set_quiet_hours_enabled(quiet.enabled);
        window.set_quiet_start(format_time(quiet.start_minute).into());
        window.set_quiet_end(format_time(quiet.end_minute).into());
        window.set_quiet_summary(
            format!(
                "{}–{}{}",
                format_time(quiet.start_minute),
                if quiet.start_minute > quiet.end_minute {
                    tr!("次日 ")
                } else {
                    ""
                },
                format_time(quiet.end_minute)
            )
            .into(),
        );
        window.set_quiet_time_error("".into());
        window.set_popup_corner(library.settings.popup_corner.index());
        window.set_remember_position(library.settings.remember_position);
        window.set_show_pack_tags(library.settings.show_pack_tags);
        window.set_focus_on_reminder(library.settings.focus_on_reminder);
        window.set_auto_check_updates(library.settings.auto_check_updates);
        window.set_has_saved_position(library.settings.popup_position.is_some());
        window.set_practice_summary(if count == 0 {
            tr!("0 题").into()
        } else if pause_minutes == 0 {
            tr!(
                "{count} 题 · {p0} 分钟",
                p0 = library.settings.interval_minutes,
                count = count
            )
            .into()
        } else {
            tr!("{count} 题 · 暂停", count = count).into()
        });
        let pending = self.pending_import.borrow();
        window.set_import_pending(pending.is_some());
        if let Some(pack) = pending.as_ref() {
            let original = library.packs.iter().find(|item| item.id == pack.id);
            let original_description = original
                .map(|item| tr!("“{p0}”（{p1} 道题）", p0 = item.name, p1 = item.cards.len()))
                .unwrap_or_else(|| pack.id.clone());
            window.set_import_description(
                tr!(
                    "已存在{original_description}。将用导入的“{p0}”（{p1} 道题）替换原内容。",
                    p0 = pack.name,
                    p1 = pack.cards.len(),
                    original_description = original_description
                )
                .into(),
            );
        }
        let pending_delete = self.pending_delete.borrow();
        window.set_delete_pending(pending_delete.is_some());
        window.set_delete_description(
            pending_delete
                .as_ref()
                .and_then(|id| library.packs.iter().find(|pack| &pack.id == id))
                .map(|pack| {
                    tr!(
                        "删除“{name}”及其中的 {count} 道题？此操作无法撤销。",
                        name = pack.name,
                        count = pack.cards.len()
                    )
                })
                .unwrap_or_default()
                .into(),
        );
    }

    fn commit(self: &Rc<Self>, edit: impl FnOnce(&mut Library) -> AppResult<()>) -> AppResult<()> {
        let mut candidate = self.library.borrow().clone();
        edit(&mut candidate)?;
        let language_changed = candidate.settings.interface_language
            != self.library.borrow().settings.interface_language;
        self.store.save(&candidate)?;
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

    fn set_pause(self: &Rc<Self>, minutes: i32) {
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

    fn set_interval(self: &Rc<Self>, value: i32) {
        let result = self.commit(|library| {
            library.settings.interval_minutes =
                u32::try_from(value).map_err(|_| tr!("出题间隔不能为负数。"))?;
            Ok(())
        });
        self.report(result, "");
    }

    fn set_feedback(self: &Rc<Self>, value: i32) {
        let result = self.commit(|library| {
            library.settings.feedback_seconds =
                u32::try_from(value).map_err(|_| tr!("卡片停留不能为负数。"))?;
            Ok(())
        });
        self.report(result, "");
    }

    fn arm_pause_expiry(self: &Rc<Self>) {
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

    fn read_autostart_status(&self) {
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

    fn set_autostart(&self, enabled: bool) {
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

    fn set_quiet_enabled(self: &Rc<Self>, enabled: bool) {
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

    fn set_quiet_range(self: &Rc<Self>, start: &str, end: &str) {
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

    fn save_pack_metadata(self: &Rc<Self>, field: PackMetadata) {
        let Some(window) = self
            .main_window
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
        else {
            return;
        };
        let id = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .map(|pack| pack.id.clone());
        let Some(id) = id else {
            return;
        };
        let result =
            match field {
                PackMetadata::Description => self.commit(|library| {
                    library.set_pack_description(&id, window.get_description_draft().as_str())
                }),
                PackMetadata::Name => self
                    .commit(|library| library.rename_pack(&id, window.get_name_draft().as_str())),
                PackMetadata::Tag => self
                    .commit(|library| library.set_pack_tag(&id, window.get_tag_draft().as_str())),
            };
        match result {
            Ok(()) => {
                window.set_metadata_error("".into());
                match field {
                    PackMetadata::Description => window.set_description_editing(false),
                    PackMetadata::Name => window.set_name_editing(false),
                    PackMetadata::Tag => window.set_tag_editing(false),
                }
            }
            Err(error) => window.set_metadata_error(error.into()),
        }
    }

    fn set_pack_enabled(self: &Rc<Self>, id: &str, enabled: bool) {
        let result = self.commit(|library| library.set_pack_enabled(id, enabled));
        self.report(result, "");
    }

    fn set_all_cards_enabled(self: &Rc<Self>, enabled: bool) {
        let id = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .map(|pack| pack.id.clone());
        if let Some(id) = id {
            let result = self.commit(|library| library.set_all_cards_enabled(&id, enabled));
            self.report(result, "");
        }
    }

    fn set_card_enabled(self: &Rc<Self>, index: i32, enabled: bool) {
        let target = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .and_then(|pack| {
                usize::try_from(index)
                    .ok()
                    .and_then(|index| pack.cards.get(index))
                    .and_then(|card| card.id.clone())
                    .map(|id| (pack.id.clone(), id))
            });
        if let Some((pack_id, card_id)) = target {
            let result =
                self.commit(|library| library.set_card_enabled(&pack_id, &card_id, enabled));
            self.report(result, "");
        }
    }

    fn set_card_answer_mode(self: &Rc<Self>, row: i32, index: i32) {
        let Some(mode) = usize::try_from(index)
            .ok()
            .and_then(|index| AnswerMode::ALL.get(index).copied())
        else {
            return;
        };
        let target = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .and_then(|pack| {
                usize::try_from(row)
                    .ok()
                    .and_then(|row| pack.cards.get(row))
                    .and_then(|card| card.id.clone().map(|id| (pack.id.clone(), id)))
            });
        if let Some((pack_id, card_id)) = target {
            let result =
                self.commit(|library| library.set_card_answer_mode(&pack_id, &card_id, mode));
            self.report(result, "");
        }
    }

    fn quick_card(
        &self,
        pack_id: &str,
        card_id: &str,
    ) -> AppResult<(r#try::knowledge::Card, AnswerMode)> {
        let library = self.library.borrow();
        let card = library
            .packs
            .iter()
            .find(|pack| pack.id == pack_id)
            .and_then(|pack| {
                pack.cards
                    .iter()
                    .find(|card| card.id.as_deref() == Some(card_id))
            })
            .ok_or(tr!("这道题已不存在，请重新选择。"))?;
        Ok((card.clone(), library.card_answer_mode(pack_id, card_id)))
    }

    fn save_quick_answer(
        self: &Rc<Self>,
        pack_id: &str,
        card_id: &str,
        input: &str,
    ) -> AppResult<()> {
        let (card, mode) = self.quick_card(pack_id, card_id)?;
        if mode != AnswerMode::Input {
            return Err(tr!("答题方式已改变，请重新选择答案。").into());
        }
        let updated = r#try::card_editor::with_answer(&card, input)?;
        if updated == card {
            return Ok(());
        }
        self.commit(|library| library.save_card_with_mode(pack_id, Some(card_id), updated, mode))
    }

    fn choose_quick_answer(
        self: &Rc<Self>,
        pack_id: &str,
        card_id: &str,
        mode_index: i32,
        index: i32,
    ) -> AppResult<()> {
        let (card, mode) = self.quick_card(pack_id, card_id)?;
        if mode.index() != mode_index {
            return Err(tr!("答题方式已改变，请重新选择答案。").into());
        }
        let mut draft = CardDraft::from(&card);
        let updated = match mode {
            AnswerMode::MultipleChoice if (0..3).contains(&index) => {
                if index == 0 {
                    return Ok(());
                }
                draft.into_card_for_mode(mode, index as usize)
            }
            AnswerMode::TrueFalse if (0..2).contains(&index) => {
                let truth = index == 0;
                if truth == draft.is_true {
                    return Ok(());
                }
                draft.is_true = truth;
                draft.into_card_for_mode(mode, 0)
            }
            _ => return Err(tr!("请选择一个有效答案。").into()),
        };
        self.commit(|library| library.save_card_with_mode(pack_id, Some(card_id), updated, mode))
    }

    fn prepare_quick_options(&self, pack_id: &str, card_id: &str) {
        let options = self
            .quick_card(pack_id, card_id)
            .ok()
            .filter(|(_, mode)| *mode == AnswerMode::MultipleChoice)
            .map(|(card, _)| {
                let scale = self
                    .main_window
                    .borrow()
                    .as_ref()
                    .map_or(1.0, MainWindow::get_pixel_scale);
                std::iter::once(AnswerContent {
                    text: card.answer,
                    latex: card.answer_latex,
                })
                .chain(card.distractors)
                .map(|option| formula_presentation::content(&option, 12, scale))
                .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_quick_options(ModelRc::new(VecModel::from(options)));
        }
    }

    fn set_editor_answer_mode(&self, index: i32) {
        if self.pending_card_delete.borrow().is_some() {
            return;
        }
        let Some(mode) = usize::try_from(index)
            .ok()
            .and_then(|index| AnswerMode::ALL.get(index).copied())
        else {
            return;
        };
        if !self.flush_math() {
            return;
        }
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_editor_answer_mode(index);
            window.set_editor_error("".into());
            window.set_math_active_field(-1);
        }
        self.seed_judgment(mode);
        self.render_editor_preview();
    }

    fn seed_judgment(&self, mode: AnswerMode) {
        if mode != AnswerMode::TrueFalse {
            return;
        }
        let mut fields = self.math_fields.borrow_mut();
        if fields.len() >= 5
            && fields[4].root.plain().trim().is_empty()
            && let Ok((text, latex)) = fields[0].content()
        {
            fields[4] = Field::new(&text, &latex);
        }
    }

    fn open_editor(&self, index: Option<i32>, new_pack: bool, requested_mode: Option<AnswerMode>) {
        if self.pending_delete.borrow().is_some()
            || self.pending_import.borrow().is_some()
            || self.pending_card_delete.borrow().is_some()
        {
            return;
        }
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        let library = self.library.borrow();
        let (target, draft, name, title, mode) = if new_pack {
            let id = loop {
                use rand::Rng;
                let id = format!("pack_{:016x}", rand::rng().random::<u64>());
                if !library.packs.iter().any(|p| p.id == id) {
                    break id;
                }
            };
            (
                EditorTarget::NewPack { id },
                CardDraft::default(),
                String::new(),
                tr!("新建知识包"),
                AnswerMode::Input,
            )
        } else {
            let Some(pack) = library.packs.get(self.selected_pack.get()) else {
                return;
            };
            let card = index
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| pack.cards.get(i));
            if index.is_some() && card.is_none() {
                return;
            }
            (
                EditorTarget::Card {
                    pack_id: pack.id.clone(),
                    card_id: card.and_then(|c| c.id.clone()),
                },
                card.map(CardDraft::from).unwrap_or_default(),
                pack.name.clone(),
                if card.is_some() {
                    tr!("编辑题目")
                } else {
                    tr!("新增题目")
                },
                card.map(|card| {
                    library.card_answer_mode(&pack.id, card.id.as_deref().unwrap_or_default())
                })
                .unwrap_or_default(),
            )
        };
        let mode = requested_mode.unwrap_or(mode);
        self.image_revision
            .set(self.image_revision.get().wrapping_add(1));
        let draft_image = draft.prompt_image.clone();
        window.set_editor_image_loading(self.image_task.borrow().is_some());
        self.math_fields.replace(vec![
            Field::new(&draft.prompt, &draft.prompt_latex),
            Field::new(&draft.answer, &draft.answer_latex),
            Field::new(&draft.choice_one, &draft.choice_one_latex),
            Field::new(&draft.choice_two, &draft.choice_two_latex),
            Field::new(&draft.statement, &draft.statement_latex),
            Field::new(&draft.correction, &draft.correction_latex),
        ]);
        self.math_pending.set(false);
        self.math_paths.borrow_mut().clear();
        window.set_math_active_field(-1);
        window.set_math_error("".into());
        window.set_current_tab(0);
        let show_aliases = !draft.aliases.is_empty();
        window.set_editor_new_pack(new_pack);
        window.set_editor_can_delete(matches!(
            &target,
            EditorTarget::Card {
                card_id: Some(_),
                ..
            }
        ));
        window.set_editor_title(title.into());
        window.set_editor_answer_mode(mode.index());
        window.set_editor_correct_choice(if draft.answer.trim().is_empty() {
            -1
        } else {
            0
        });
        window.set_editor_pack_name(name.into());
        window.set_editor_prompt(draft.prompt.into());
        window.set_editor_answer(draft.answer.into());
        window.set_editor_aliases(draft.aliases.into());
        window.set_editor_prompt_latex(draft.prompt_latex.into());
        window.set_editor_answer_latex(draft.answer_latex.into());
        window.set_editor_choice_one(draft.choice_one.into());
        window.set_editor_choice_two(draft.choice_two.into());
        window.set_editor_choice_one_latex(draft.choice_one_latex.into());
        window.set_editor_choice_two_latex(draft.choice_two_latex.into());
        window.set_editor_statement(draft.statement.into());
        window.set_editor_statement_latex(draft.statement_latex.into());
        window.set_editor_is_true(draft.is_true);
        window.set_editor_truth_choice(if draft.has_judgment {
            i32::from(!draft.is_true)
        } else {
            -1
        });
        window.set_editor_correction(draft.correction.into());
        window.set_editor_correction_latex(draft.correction_latex.into());
        window.set_editor_extra(show_aliases);
        window.set_editor_error("".into());
        window.set_message("".into());
        self.editor.replace(Some(target));
        window.set_editor_visible(true);
        window.invoke_reset_editor_scroll();
        self.seed_judgment(mode);
        self.render_editor_preview();
        if let Err(error) = self.set_editor_image(draft_image) {
            window.set_editor_error(error.into());
        }
    }

    fn render_editor_preview(&self) {
        if self.editor.borrow().is_none() {
            return;
        }
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        let fields = self.math_fields.borrow();
        let previews = fields
            .iter()
            .map(|field| {
                let mut preview = AnswerOption {
                    label: field.input().into(),
                    ..Default::default()
                };
                if !field.root.is_text()
                    && let Ok(rendered) = formula::render_node(
                        &field.root,
                        16,
                        window.get_pixel_scale(),
                        (232, 232, 232),
                    )
                {
                    preview = formula_presentation::option(preview.label, rendered);
                }
                preview
            })
            .collect::<Vec<_>>();
        if previews.len() >= 2 {
            window.set_editor_prompt_preview(previews[0].clone());
            window.set_editor_answer_preview(previews[1].clone());
        }
        window.set_editor_fields(ModelRc::new(VecModel::from(previews)));
        drop(fields);
        drop(main);
        self.refresh_math(false);
    }

    fn queue_row_render(self: &Rc<Self>, index: i32) {
        if let Some(window) = self.main_window.borrow().as_ref()
            && let Ok(index) = usize::try_from(index)
            && let Some(row) = window.get_cards().row_data(index)
            && ((row.formula_source.is_empty() && row.answer_formula_source.is_empty())
                || (row.rendered && (row.formula_scale - window.get_pixel_scale()).abs() < 0.001))
        {
            return;
        }
        self.pending_rows.borrow_mut().insert(index);
        if self.row_render_timer.running() {
            return;
        }
        let weak = Rc::downgrade(self);
        self.row_render_timer
            .start(TimerMode::SingleShot, Duration::ZERO, move || {
                if let Some(app) = weak.upgrade() {
                    let rows = std::mem::take(&mut *app.pending_rows.borrow_mut());
                    for index in rows {
                        app.render_row(index);
                    }
                }
            });
    }

    fn render_row(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        let model = window.get_cards();
        let Some(model) = model.as_any().downcast_ref::<VecModel<CardRow>>() else {
            return;
        };
        let Some(mut row) = model.row_data(index) else {
            return;
        };
        if (row.formula_source.is_empty() && row.answer_formula_source.is_empty())
            || (row.rendered && (row.formula_scale - window.get_pixel_scale()).abs() < 0.001)
        {
            return;
        }
        row.rendered = true;
        row.formula_scale = window.get_pixel_scale();
        if !row.formula_source.is_empty() {
            row.preview = formula_presentation::content(
                &AnswerContent {
                    text: row.prompt.to_string(),
                    latex: Some(row.formula_source.to_string()),
                },
                12,
                window.get_pixel_scale(),
            );
        }
        if !row.answer_formula_source.is_empty() {
            row.answer_preview = formula_presentation::content(
                &AnswerContent {
                    text: row.answer.to_string(),
                    latex: Some(row.answer_formula_source.to_string()),
                },
                12,
                window.get_pixel_scale(),
            );
        }
        model.set_row_data(index, row);
        // Bound retained row bitmaps even when a large pack is scrolled through.
        let radius = ((window.get_question_area_height() / 48.0).ceil() as usize + 8).max(16);
        for old_index in 0..model.row_count() {
            if old_index.abs_diff(index) > radius
                && let Some(mut row) = model.row_data(old_index)
                && row.rendered
            {
                row.preview = AnswerOption::default();
                row.answer_preview = AnswerOption::default();
                row.rendered = false;
                model.set_row_data(old_index, row);
            }
        }
    }

    fn activate_content(&self, index: i32) {
        if !self.flush_math() {
            return;
        }
        if usize::try_from(index)
            .ok()
            .is_none_or(|i| i >= self.math_fields.borrow().len())
        {
            return;
        }
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_math_active_field(index);
        }
        self.refresh_math(true);
    }

    fn flush_math(&self) -> bool {
        self.editor_preview_timer.stop();
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return false;
        };
        let Ok(index) = usize::try_from(window.get_math_active_field()) else {
            return true;
        };
        let mut fields = self.math_fields.borrow_mut();
        let Some(field) = fields.get_mut(index) else {
            return false;
        };
        if !self.math_pending.get() {
            return field.error().is_none();
        }
        match field.update(window.get_math_input().as_str()) {
            Ok(()) => {
                self.math_pending.set(false);
                window.set_math_error("".into());
                window.set_editor_error("".into());
                true
            }
            Err(error) => {
                window.set_math_error(error.into());
                false
            }
        }
    }

    fn modify_math(&self, change: impl FnOnce(&mut Field) -> AppResult<()>) {
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        let Ok(index) = usize::try_from(window.get_math_active_field()) else {
            return;
        };
        let mut fields = self.math_fields.borrow_mut();
        let Some(field) = fields.get_mut(index) else {
            return;
        };
        if let Err(error) = change(field) {
            window.set_math_error(error.into());
            return;
        }
        window.set_math_error("".into());
        drop(fields);
        drop(main);
        self.refresh_math(true);
    }

    fn refresh_math(&self, reset_input: bool) {
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        let Ok(index) = usize::try_from(window.get_math_active_field()) else {
            return;
        };
        let fields = self.math_fields.borrow();
        let Some(field) = fields.get(index) else {
            return;
        };
        if reset_input {
            window.set_math_input(field.input().into());
        }
        let (position, navigation) =
            editing_state(field, window.get_math_position().revision, reset_input);
        window.set_math_position(position);
        window.set_math_navigation(navigation);
        window.set_math_error(field.error().unwrap_or_default().into());
        window.set_math_text_mode(field.selected().accepts_prose());
        window.set_math_has_formula(!field.root.is_text());
        window.set_math_can_undo(field.can_undo());
        window.set_math_can_redo(field.can_redo());
        window.set_math_can_move_out(field.can_move_out());
        window.set_math_can_unwrap(field.can_unwrap());
        let (rows, cols) = field.grid_size().unwrap_or_default();
        window.set_math_grid_rows(rows as i32);
        window.set_math_grid_columns(cols as i32);
        if field.root.is_text() {
            window.set_math_regions(ModelRc::default());
            window.set_math_selection(MathRegion::default());
            self.math_paths.borrow_mut().clear();
            Self::publish_content_preview(
                window,
                index,
                AnswerOption {
                    label: field.root.value.clone().into(),
                    ..Default::default()
                },
            );
            return;
        }
        match formula::render_node(&field.root, 16, window.get_pixel_scale(), (232, 232, 232)) {
            Ok(rendered) => {
                let selected = rendered
                    .regions
                    .iter()
                    .filter(|region| region.path.starts_with(&field.selection))
                    .collect::<Vec<_>>();
                let selection = if field.selection.is_empty() {
                    MathRegion {
                        x: 0.0,
                        y: 0.0,
                        width: rendered.width,
                        height: rendered.height,
                    }
                } else if selected.is_empty() {
                    MathRegion::default()
                } else {
                    let left = selected.iter().map(|r| r.x).fold(f32::INFINITY, f32::min);
                    let top = selected.iter().map(|r| r.y).fold(f32::INFINITY, f32::min);
                    let right = selected.iter().map(|r| r.x + r.width).fold(0.0, f32::max);
                    let bottom = selected.iter().map(|r| r.y + r.height).fold(0.0, f32::max);
                    MathRegion {
                        x: left,
                        y: top,
                        width: right - left,
                        height: bottom - top,
                    }
                };
                window.set_math_selection(selection);
                window.set_math_regions(ModelRc::new(VecModel::from(
                    rendered
                        .regions
                        .iter()
                        .map(|region| MathRegion {
                            x: region.x,
                            y: region.y,
                            width: region.width,
                            height: region.height,
                        })
                        .collect::<Vec<_>>(),
                )));
                self.math_paths.replace(
                    rendered
                        .regions
                        .iter()
                        .map(|region| region.path.clone())
                        .collect(),
                );
                let preview = formula_presentation::option(field.root.plain().into(), rendered);
                Self::publish_content_preview(window, index, preview);
            }
            Err(error) => {
                window.set_math_preview(AnswerOption {
                    label: field.input().into(),
                    ..Default::default()
                });
                window.set_math_regions(ModelRc::default());
                window.set_math_selection(MathRegion::default());
                self.math_paths.borrow_mut().clear();
                window.set_math_error(error.into());
            }
        }
    }

    fn publish_content_preview(window: &MainWindow, index: usize, preview: AnswerOption) {
        window.set_math_preview(preview.clone());
        let model = window.get_editor_fields();
        if let Some(model) = model.as_any().downcast_ref::<VecModel<AnswerOption>>() {
            model.set_row_data(index, preview.clone());
        }
        if index == 0 {
            window.set_editor_prompt_preview(preview.clone());
        }
        if index == 1 {
            window.set_editor_answer_preview(preview);
        }
    }

    fn palette_model(tools: &[r#try::math_palette::Tool]) -> ModelRc<MathTool> {
        ModelRc::new(VecModel::from(
            tools
                .iter()
                .map(|tool| MathTool {
                    name: i18n::text(tool.name).into(),
                    command: tool.command.into(),
                })
                .collect::<Vec<_>>(),
        ))
    }

    fn save_editor(self: &Rc<Self>) {
        if self.image_task.borrow().is_some() {
            return;
        }
        if self.pending_card_delete.borrow().is_some() {
            return;
        }
        if !self.flush_math() {
            return;
        }
        let Some(window) = self
            .main_window
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
        else {
            return;
        };
        let Some(mode) = usize::try_from(window.get_editor_answer_mode())
            .ok()
            .and_then(|index| AnswerMode::ALL.get(index).copied())
        else {
            window.set_editor_error(tr!("请选择这道题的答题方式。").into());
            return;
        };
        if (mode == AnswerMode::MultipleChoice
            && !(0..3).contains(&window.get_editor_correct_choice()))
            || (mode == AnswerMode::TrueFalse
                && !(0..2).contains(&window.get_editor_truth_choice()))
        {
            window.set_editor_error(tr!("请选择正确答案。").into());
            return;
        }
        let old_answer = window.get_editor_answer();
        let old_latex = window.get_editor_answer_latex();
        for (index, field) in self.math_fields.borrow().iter().enumerate() {
            if !(field.is_dirty()
                || (mode == AnswerMode::TrueFalse
                    && index == 4
                    && window.get_editor_statement().trim().is_empty()))
            {
                continue;
            }
            let (text, tex) = match field.content() {
                Ok(content) => content,
                Err(error) => {
                    window.set_editor_error(error.into());
                    return;
                }
            };
            match index {
                0 => {
                    window.set_editor_prompt(text.into());
                    window.set_editor_prompt_latex(tex.into());
                }
                1 => {
                    window.set_editor_answer(text.into());
                    window.set_editor_answer_latex(tex.into());
                }
                2 => {
                    window.set_editor_choice_one(text.into());
                    window.set_editor_choice_one_latex(tex.into());
                }
                3 => {
                    window.set_editor_choice_two(text.into());
                    window.set_editor_choice_two_latex(tex.into());
                }
                4 => {
                    window.set_editor_statement(text.into());
                    window.set_editor_statement_latex(tex.into());
                }
                5 => {
                    window.set_editor_correction(text.into());
                    window.set_editor_correction_latex(tex.into());
                }
                _ => {}
            }
        }
        let mut draft = CardDraft {
            prompt_image: self.editor_image.borrow().clone(),
            prompt: window.get_editor_prompt().to_string(),
            answer: window.get_editor_answer().to_string(),
            aliases: window.get_editor_aliases().to_string(),
            prompt_latex: window.get_editor_prompt_latex().to_string(),
            answer_latex: window.get_editor_answer_latex().to_string(),
            choice_one: window.get_editor_choice_one().to_string(),
            choice_two: window.get_editor_choice_two().to_string(),
            choice_one_latex: window.get_editor_choice_one_latex().to_string(),
            choice_two_latex: window.get_editor_choice_two_latex().to_string(),
            has_judgment: window.get_editor_answer_mode() == AnswerMode::TrueFalse.index()
                || !window.get_editor_statement().trim().is_empty()
                || !window.get_editor_statement_latex().trim().is_empty()
                || !window.get_editor_correction().trim().is_empty()
                || !window.get_editor_correction_latex().trim().is_empty(),
            statement: window.get_editor_statement().to_string(),
            statement_latex: window.get_editor_statement_latex().to_string(),
            is_true: window.get_editor_is_true(),
            correction: window.get_editor_correction().to_string(),
            correction_latex: window.get_editor_correction_latex().to_string(),
        };
        if mode == AnswerMode::MultipleChoice
            && draft.answer.trim() != old_answer.trim()
            && !math_answer::matches(
                &draft.answer,
                old_answer.as_str(),
                (!old_latex.is_empty()).then_some(old_latex.as_str()),
                &[],
            )
        {
            draft.aliases.clear();
        }
        let card = draft.into_card_for_mode(
            mode,
            usize::try_from(window.get_editor_correct_choice()).unwrap_or_default(),
        );
        if let Some(id) = &card.prompt_image
            && let Err(error) = self.store.media().read(id)
        {
            window.set_editor_error(error.into());
            return;
        }
        let result = match self.editor.borrow().as_ref() {
            Some(EditorTarget::NewPack { id }) => {
                let pack = KnowledgePack {
                    format: "try".into(),
                    schema_version: 1,
                    id: id.clone(),
                    name: window.get_editor_pack_name().trim().to_owned(),
                    tag: String::new(),
                    description: String::new(),
                    cards: vec![card],
                };
                let result = self.commit(|library| {
                    library.import(pack, false)?;
                    let card_id = library.packs.last().unwrap().cards[0].id.clone().unwrap();
                    library.set_card_answer_mode(id, &card_id, mode)
                });
                if result.is_ok() {
                    self.selected_pack
                        .set(self.library.borrow().packs.len() - 1);
                    window.invoke_reset_overview();
                }
                result
            }
            Some(EditorTarget::Card { pack_id, card_id }) => self.commit(|library| {
                library.save_card_with_mode(pack_id, card_id.as_deref(), card, mode)
            }),
            None => return,
        };
        match result {
            Ok(()) => {
                self.close_editor();
                self.refresh();
            }
            Err(error) => {
                window.set_editor_error(error.into());
            }
        }
    }

    fn close_editor(&self) {
        self.image_revision
            .set(self.image_revision.get().wrapping_add(1));
        self.editor_image.borrow_mut().take();
        self.close_image_preview();
        self.editor_preview_timer.stop();
        self.pending_card_delete.borrow_mut().take();
        self.editor.borrow_mut().take();
        self.math_fields.borrow_mut().clear();
        self.math_paths.borrow_mut().clear();
        self.math_pending.set(false);
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_card_delete_pending(false);
            window.set_editor_can_delete(false);
            window.set_card_delete_prompt("".into());
            window.set_editor_visible(false);
            window.set_editor_picture(slint::Image::default());
            window.set_editor_picture_width(0.0);
            window.set_editor_picture_height(0.0);
            window.set_editor_has_image(false);
            window.set_editor_error("".into());
            window.set_editor_prompt_preview(AnswerOption::default());
            window.set_editor_answer_preview(AnswerOption::default());
            window.set_editor_fields(ModelRc::default());
            window.set_math_active_field(-1);
            window.set_math_preview(AnswerOption::default());
            window.set_math_regions(ModelRc::default());
        }
        self.collect_media();
    }

    fn collect_media(&self) {
        let mut keep: std::collections::BTreeSet<String> = self
            .library
            .borrow()
            .packs
            .iter()
            .flat_map(|pack| {
                pack.cards
                    .iter()
                    .filter_map(|card| card.prompt_image.clone())
            })
            .collect();
        keep.extend(self.editor_image.borrow().clone());
        if let Some(pack) = self.pending_import.borrow().as_ref() {
            keep.extend(
                pack.cards
                    .iter()
                    .filter_map(|card| card.prompt_image.clone()),
            );
        }
        self.store.media().collect(&keep);
    }

    fn set_editor_image(&self, id: Option<String>) -> AppResult<()> {
        self.editor_image.replace(id.clone());
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_editor_has_image(id.is_some());
            window.set_editor_picture(slint::Image::default());
            window.set_editor_picture_width(0.0);
            window.set_editor_picture_height(0.0);
            if let Some(id) = id {
                let loaded = self.store.media().load(&id)?;
                window.set_editor_picture(loaded.image);
                window.set_editor_picture_width(loaded.width as f32);
                window.set_editor_picture_height(loaded.height as f32);
            }
        }
        Ok(())
    }

    fn choose_question_image(self: &Rc<Self>) {
        if self.editor.borrow().is_none() || self.image_task.borrow().is_some() {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!("选择题面图片"))
            .add_filter(tr!("图片"), &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        self.import_question_image(path);
    }

    fn import_question_image(self: &Rc<Self>, path: std::path::PathBuf) {
        if self.editor.borrow().is_none() || self.image_task.borrow().is_some() {
            return;
        }
        let revision = self.image_revision.get();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.image_task.replace(Some((revision, receiver)));
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_editor_image_loading(true);
            window.set_editor_error("".into());
        }
        std::thread::spawn(move || {
            let _ = sender.send(r#try::media::normalize_file(&path));
        });
        let weak = Rc::downgrade(self);
        self.image_task_timer
            .start(TimerMode::Repeated, Duration::from_millis(40), move || {
                let Some(app) = weak.upgrade() else {
                    return;
                };
                let received = {
                    let task = app.image_task.borrow();
                    task.as_ref()
                        .and_then(|(revision, receiver)| match receiver.try_recv() {
                            Ok(result) => Some((*revision, result)),
                            Err(std::sync::mpsc::TryRecvError::Empty) => None,
                            Err(_) => {
                                Some((*revision, Err(tr!("图片处理未完成，请重新选择。").into())))
                            }
                        })
                };
                let Some((revision, result)) = received else {
                    return;
                };
                app.image_task_timer.stop();
                app.image_task.borrow_mut().take();
                if let Some(window) = app.main_window.borrow().as_ref() {
                    window.set_editor_image_loading(false);
                }
                if app.image_revision.get() != revision || app.editor.borrow().is_none() {
                    return;
                }
                let result = result.and_then(|image| {
                    app.store.media().install(&image.id, &image.png)?;
                    app.set_editor_image(Some(image.id))
                });
                if let Some(window) = app.main_window.borrow().as_ref() {
                    window.set_editor_error(result.err().unwrap_or_default().into());
                }
                app.collect_media();
            });
    }

    fn close_image_preview(&self) {
        if let Some(window) = self.image_preview.borrow_mut().take() {
            let _ = window.hide();
        }
    }

    fn preview_image(self: &Rc<Self>, image: slint::Image) {
        if image.size().width == 0 {
            return;
        }
        self.feedback_timer.stop();
        self.close_image_preview();
        let result = (|| -> AppResult<()> {
            let window = crate::ImagePreviewWindow::new().map_err(|e| e.to_string())?;
            window.set_picture(image);
            window.set_app_icon(self.icon.clone());
            window.set_dark_theme(
                self.resolved_dark_theme(self.library.borrow().settings.appearance),
            );
            let weak = Rc::downgrade(self);
            window.on_dismiss(move || {
                if let Some(app) = weak.upgrade() {
                    app.close_image_preview();
                }
            });
            let weak = Rc::downgrade(self);
            window.window().on_close_requested(move || {
                let weak = weak.clone();
                Timer::single_shot(Duration::ZERO, move || {
                    if let Some(app) = weak.upgrade() {
                        app.close_image_preview();
                    }
                });
                slint::CloseRequestResponse::HideWindow
            });
            window.show().map_err(|e| e.to_string())?;
            self.floating
                .apply_theme(window.window(), window.get_dark_theme());
            self.image_preview.replace(Some(window));
            Ok(())
        })();
        self.report(result, "");
    }

    fn import_pack(self: &Rc<Self>) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!("导入 Try 知识包"))
            .add_filter(tr!("Try 知识包"), &["json"])
            .pick_file()
        else {
            return;
        };
        let imported = self.store.import_pack(&path);
        if imported.is_err() {
            self.collect_media();
        }
        match imported {
            Ok(pack) => {
                let duplicate = self
                    .library
                    .borrow()
                    .packs
                    .iter()
                    .any(|item| item.id == pack.id);
                if duplicate {
                    self.pending_import.replace(Some(pack));
                    self.refresh();
                    self.notify("", false);
                } else {
                    let id = pack.id.clone();
                    let result = self.commit(|library| library.import(pack, false));
                    if result.is_ok() {
                        let index = self
                            .library
                            .borrow()
                            .packs
                            .iter()
                            .position(|pack| pack.id == id)
                            .unwrap_or(0);
                        self.selected_pack.set(index);
                        if let Some(window) = self.main_window.borrow().as_ref() {
                            window.invoke_reset_overview();
                        }
                        self.refresh();
                    }
                    self.report(result, tr!("已导入。"));
                }
            }
            Err(error) => self.notify(&error, true),
        }
    }

    fn confirm_import(self: &Rc<Self>) {
        let Some(pack) = self.pending_import.borrow_mut().take() else {
            return;
        };
        let replacement = pack.clone();
        let result = self.commit(|library| library.import(pack, true));
        if result.is_err() {
            self.pending_import.replace(Some(replacement));
        }
        self.refresh();
        self.report(result, tr!("已替换。"));
    }

    fn request_delete_card(&self) {
        let key = match self.editor.borrow().as_ref() {
            Some(EditorTarget::Card {
                pack_id,
                card_id: Some(card_id),
            }) => QuestionKey {
                pack_id: pack_id.clone(),
                card_id: card_id.clone(),
            },
            _ => return,
        };
        if self.pending_card_delete.borrow().is_some() {
            return;
        }
        self.editor_preview_timer.stop();
        let prompt = self
            .library
            .borrow()
            .packs
            .iter()
            .find(|pack| pack.id == key.pack_id)
            .and_then(|pack| {
                pack.cards
                    .iter()
                    .find(|card| card.id.as_deref() == Some(&key.card_id))
            })
            .map(|card| card.prompt.clone())
            .unwrap_or_default();
        self.pending_card_delete.replace(Some(key));
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_card_delete_prompt(prompt.into());
            window.set_card_delete_pending(true);
            window.set_editor_error("".into());
            window.set_message("".into());
        }
    }

    fn cancel_delete_card(&self) {
        self.pending_card_delete.borrow_mut().take();
        if let Some(window) = self.main_window.borrow().as_ref() {
            window.set_card_delete_pending(false);
            window.set_card_delete_prompt("".into());
            window.set_editor_error("".into());
            window.set_message("".into());
        }
        self.render_editor_preview();
    }

    fn confirm_delete_card(self: &Rc<Self>) {
        let Some(key) = self.pending_card_delete.borrow().clone() else {
            return;
        };
        let result = self.commit(|library| library.delete_card(&key.pack_id, &key.card_id));
        if result.is_ok() {
            if self.previous_question.borrow().as_ref() == Some(&key) {
                self.previous_question.borrow_mut().take();
            }
            self.close_editor();
            log::info!(target: "try::app", "event=card_deleted remaining_cards={}", self.library.borrow().packs.iter().map(|pack| pack.cards.len()).sum::<usize>());
        }
        self.refresh();
        self.report(result, "");
    }

    fn request_delete_pack(&self) {
        let main = self.main_window.borrow();
        let Some(window) = main.as_ref() else {
            return;
        };
        if !window.get_tray_settings_available() {
            return;
        }
        let id = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .map(|pack| pack.id.clone());
        let Some(id) = id else { return };
        window.set_current_tab(0);
        self.pending_delete.replace(Some(id));
        self.notify("", false);
        self.refresh();
    }

    fn confirm_delete_pack(self: &Rc<Self>) {
        // Keep the stable pack id and confirmation until the atomic save succeeds.
        let Some(id) = self.pending_delete.borrow().clone() else {
            return;
        };
        let result = self.commit(|library| library.delete_pack(&id));
        if result.is_ok() {
            self.pending_delete.borrow_mut().take();
            if self
                .previous_question
                .borrow()
                .as_ref()
                .is_some_and(|key| key.pack_id == id)
            {
                self.previous_question.borrow_mut().take();
            }
            if let Some(window) = self.main_window.borrow().as_ref() {
                window.invoke_reset_overview();
                window.set_question_scroll_y(0.0);
                window.set_quick_options(ModelRc::default());
            }
            log::info!(target: "try::app", "event=pack_deleted remaining_packs={}", self.library.borrow().packs.len());
        }
        self.refresh();
        self.report(result, "");
    }

    fn export_pack(&self) {
        let pack = self
            .library
            .borrow()
            .packs
            .get(self.selected_pack.get())
            .cloned();
        let Some(pack) = pack else { return };
        let Some(mut path) = rfd::FileDialog::new()
            .set_title(tr!("导出 Try 知识包"))
            .set_file_name(format!("{}.json", pack.id))
            .add_filter(tr!("Try 知识包"), &["json"])
            .save_file()
        else {
            return;
        };
        if path.extension().is_none() {
            path.set_extension("json");
        }
        self.report(
            self.store.export_portable_pack(&path, &pack),
            tr!("已导出。"),
        );
    }

    pub fn show_practice(self: &Rc<Self>) -> AppResult<()> {
        self.show_practice_with_origin(PracticeOrigin::Manual)
    }

    fn show_practice_with_origin(self: &Rc<Self>, origin: PracticeOrigin) -> AppResult<()> {
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
        if let Some(session) = self.practice.borrow().as_ref() {
            log::debug!(target: "try::app",
                "event=practice_reopened card={} origin={origin:?}",
                session.diagnostic_id
            );
            if origin == PracticeOrigin::Manual {
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
                    self.notify(
                        &tr!("练习记录暂不可用，将使用普通选题：{error}", error = error),
                        true,
                    );
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
        let _creation = self.floating.practice_creation();
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
                app.refresh_answer_entry(false);
            }
        });
        let weak_window = window.as_weak();
        let weak_app = Rc::downgrade(self);
        let applied_theme = Cell::new(None);
        window.window().on_winit_window_event(move |window, event| {
            crate::window_repaint::prepare(window, event);
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
                Timer::single_shot(Duration::ZERO, move || {
                    if let Some(window) = weak_window.upgrade() {
                        if reshape {
                            FloatingWindow::reshape_and_clamp(window.window());
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
        window.on_toggle_entry(move || {
            if let Some(app) = weak.upgrade() {
                app.toggle_answer_entry();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_cancel_entry(move || {
            if let Some(app) = weak.upgrade() {
                app.cancel_answer_entry();
            }
        });
        let weak = Rc::downgrade(self);
        window.on_entry_command(move |command| {
            if let Some(app) = weak.upgrade() {
                app.answer_entry_command(command.as_str());
            }
        });
        let weak = Rc::downgrade(self);
        window.on_select_entry_region(move |index| {
            if let Some(app) = weak.upgrade()
                && app.flush_answer_entry()
            {
                {
                    let mut practice = app.practice.borrow_mut();
                    if let Some(entry) =
                        practice.as_mut().and_then(|session| session.entry.as_mut())
                        && let Some(path) = usize::try_from(index)
                            .ok()
                            .and_then(|i| entry.paths.get(i))
                            .cloned()
                    {
                        entry.field.select(path);
                    }
                }
                app.refresh_answer_entry(true);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_entry_position_changed(move |input, anchor, caret| {
            if let Some(app) = weak.upgrade()
                && let Some(session) = app.practice.borrow_mut().as_mut()
            {
                if let Some(entry) = session.entry.as_mut() {
                    if entry.field.input() == input.as_str() {
                        entry
                            .field
                            .set_text_selection(anchor.max(0) as usize, caret.max(0) as usize);
                        let (position, navigation) = editing_state(
                            &entry.field,
                            session.window.get_entry_position().revision,
                            false,
                        );
                        session.window.set_entry_position(position);
                        session.window.set_entry_navigation(navigation);
                    }
                } else if session.window.get_answer_text() == input {
                    sync_answer_position(session, anchor, caret, false);
                }
            }
        });
        let weak = Rc::downgrade(self);
        window.on_input_pointer_select(move |x, y, extend| {
            if let Some(app) = weak.upgrade() {
                app.select_inline_answer(x, y, extend);
            }
        });
        let weak = Rc::downgrade(self);
        window.on_input_move_caret(move |direction, extend| {
            if let Some(app) = weak.upgrade() {
                app.move_inline_answer(direction, extend);
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
            review_token: None,
            entry: None,
            inline_input: None,
            input_history: RefCell::default(),
            initial_position: Cell::new(None),
        }));
        if let Err(error) = window.show() {
            self.practice.borrow_mut().take();
            self.arm_reminder();
            return Err(error.to_string());
        }
        if let Some(session) = self.practice.borrow_mut().as_mut()
            && !session.prepared.knowledge_only
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
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(app) = weak.upgrade()
                && let Some(session) = app.practice.borrow().as_ref()
            {
                app.floating.place(session.window.window());
                if let Some(on_right) = FloatingWindow::tag_on_right(session.window.window()) {
                    session.window.set_tag_on_right(on_right);
                }
                session
                    .initial_position
                    .set(FloatingWindow::position(session.window.window()));
                // Internal input focus and OS activation are separate. Display-only
                // reminders stay passive; answer cards follow the saved preference.
                session.window.invoke_focus_answer();
                if origin == PracticeOrigin::Manual
                    || (!session.prepared.knowledge_only
                        && app.library.borrow().settings.focus_on_reminder)
                {
                    FloatingWindow::activate(session.window.window());
                }
            }
        });
        if window.get_knowledge_only() {
            self.arm_card_close();
        }
        Ok(())
    }

    fn edit_answer_input(self: &Rc<Self>, input: &str) {
        {
            let mut practice = self.practice.borrow_mut();
            let Some(session) = practice.as_mut().filter(|session| {
                !session.answered
                    && session.prepared.mode == AnswerMode::Input
                    && !session.prepared.knowledge_only
            }) else {
                return;
            };
            if let Some(entry) = session.entry.as_mut() {
                session.window.set_entry_input(input.into());
                let error = entry.field.update(input).err().unwrap_or_default();
                session.window.set_entry_error(error.into());
            } else {
                session.input_history.borrow_mut().record(input);
                session.window.set_entry_error("".into());
                session.window.set_answer_text(input.into());
                session.inline_input = None;
            }
        }
        let weak = Rc::downgrade(self);
        self.answer_preview_timer.start(
            TimerMode::SingleShot,
            Duration::from_millis(150),
            move || {
                if let Some(app) = weak.upgrade() {
                    app.refresh_answer_entry(false);
                }
            },
        );
    }

    fn toggle_answer_entry(&self) {
        self.answer_preview_timer.stop();
        if self
            .practice
            .borrow()
            .as_ref()
            .is_some_and(|session| session.entry.is_some())
        {
            self.apply_answer_entry();
            return;
        }
        {
            let mut practice = self.practice.borrow_mut();
            let Some(session) = practice.as_mut().filter(|session| {
                !session.answered
                    && !session.prepared.knowledge_only
                    && session.prepared.mode == AnswerMode::Input
            }) else {
                return;
            };
            session.entry = Some(AnswerEditor {
                field: math_answer::editor(session.window.get_answer_text().as_str()),
                paths: vec![],
            });
            session.window.set_entry_open(true);
            session.window.set_entry_error("".into());
        }
        self.refresh_answer_entry(true);
        if let Some(session) = self.practice.borrow().as_ref() {
            session.window.invoke_focus_answer();
        }
    }

    fn select_inline_answer(&self, x: f32, y: f32, extend: bool) {
        let practice = self.practice.borrow();
        let Some(session) = practice
            .as_ref()
            .filter(|session| !session.answered && session.entry.is_none())
        else {
            return;
        };
        let Some(layout) = session
            .inline_input
            .as_ref()
            .filter(|layout| layout.source.as_str() == session.window.get_answer_text().as_str())
        else {
            return;
        };
        let caret = layout.hit(x, y) as i32;
        let anchor = if extend {
            session.window.get_answer_position().anchor
        } else {
            caret
        };
        sync_answer_position(session, anchor, caret, true);
    }

    fn move_inline_answer(&self, direction: i32, extend: bool) {
        let practice = self.practice.borrow();
        let Some(session) = practice
            .as_ref()
            .filter(|session| !session.answered && session.entry.is_none())
        else {
            return;
        };
        let Some(layout) = session
            .inline_input
            .as_ref()
            .filter(|layout| layout.source.as_str() == session.window.get_answer_text().as_str())
        else {
            return;
        };
        let position = session.window.get_answer_position();
        let caret = if !extend && position.anchor != position.caret && direction == 0 {
            position.anchor.min(position.caret)
        } else if !extend && position.anchor != position.caret && direction == 1 {
            position.anchor.max(position.caret)
        } else {
            layout.move_caret(position.caret.max(0) as usize, direction) as i32
        };
        sync_answer_position(
            session,
            if extend { position.anchor } else { caret },
            caret,
            true,
        );
    }

    fn inline_answer_command(&self, command: &str) {
        if !matches!(command, "undo" | "redo") {
            return;
        }
        let selection = {
            let mut practice = self.practice.borrow_mut();
            let Some(session) = practice
                .as_mut()
                .filter(|session| !session.answered && session.entry.is_none())
            else {
                return;
            };
            let Some((input, selection)) =
                session.input_history.borrow_mut().step(command == "redo")
            else {
                return;
            };
            session.window.set_answer_text(input.into());
            session.window.set_entry_error("".into());
            session.inline_input = None;
            selection
        };
        self.answer_preview_timer.stop();
        self.refresh_answer_entry(false);
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

    fn cancel_answer_entry(&self) {
        self.answer_preview_timer.stop();
        {
            let mut practice = self.practice.borrow_mut();
            if let Some(session) = practice.as_mut() {
                session.entry = None;
                session.window.set_entry_open(false);
                session.window.set_entry_error("".into());
                session.window.set_entry_regions(ModelRc::default());
            }
        }
        self.refresh_answer_entry(false);
        if let Some(session) = self.practice.borrow().as_ref() {
            let position = session.window.get_answer_position();
            sync_answer_position(session, position.anchor, position.caret, true);
            session.window.invoke_focus_answer();
        }
    }

    fn flush_answer_entry(&self) -> bool {
        self.answer_preview_timer.stop();
        let mut practice = self.practice.borrow_mut();
        let Some(session) = practice.as_mut() else {
            return false;
        };
        let Some(entry) = session.entry.as_mut() else {
            return true;
        };
        match entry
            .field
            .update(session.window.get_entry_input().as_str())
        {
            Ok(()) => {
                session.window.set_entry_error("".into());
                true
            }
            Err(error) => {
                session.window.set_entry_error(error.into());
                false
            }
        }
    }

    fn apply_answer_entry(&self) -> Option<String> {
        if !self.flush_answer_entry() {
            return None;
        }
        let input = {
            let mut practice = self.practice.borrow_mut();
            let session = practice.as_mut()?;
            let entry = session.entry.as_ref()?;
            match entry.field.content() {
                Ok((text, _)) => {
                    session.input_history.borrow_mut().record(&text);
                    session.window.set_answer_text(text.clone().into());
                    session.window.set_entry_open(false);
                    session.window.set_entry_regions(ModelRc::default());
                    session.entry = None;
                    text
                }
                Err(error) => {
                    session.window.set_entry_error(error.into());
                    return None;
                }
            }
        };
        self.refresh_answer_entry(true);
        if let Some(session) = self.practice.borrow().as_ref() {
            session.window.invoke_focus_answer();
        }
        Some(input)
    }

    fn answer_entry_command(self: &Rc<Self>, command: &str) {
        if self
            .practice
            .borrow()
            .as_ref()
            .is_none_or(|session| session.entry.is_none() || session.answered)
        {
            return;
        }
        if command == "submit" {
            self.submit_answer("");
            return;
        }
        if command == "backspace" || command.len() == 1 {
            self.answer_preview_timer.stop();
            if let Some(session) = self.practice.borrow_mut().as_mut() {
                let field = &mut session.entry.as_mut().unwrap().field;
                let result = if command == "backspace" {
                    field.backspace()
                } else {
                    field.type_text(command)
                };
                session
                    .window
                    .set_entry_error(result.err().unwrap_or_default().into());
            }
            self.refresh_answer_entry(true);
        } else if command == "clear" {
            // Clearing must also work when the current input cannot be parsed.
            self.flush_answer_entry();
            if let Some(session) = self.practice.borrow_mut().as_mut() {
                session.entry.as_mut().unwrap().field.clear();
                session.window.set_entry_error("".into());
            }
            self.refresh_answer_entry(true);
        } else if command == "undo" || command == "redo" {
            self.answer_preview_timer.stop();
            if let Some(session) = self.practice.borrow_mut().as_mut() {
                let entry = session.entry.as_mut().unwrap();
                entry.field.undo(command == "redo");
                session.window.set_entry_error("".into());
            }
            self.refresh_answer_entry(true);
        } else if self.flush_answer_entry() {
            let error = {
                let mut practice = self.practice.borrow_mut();
                let session = practice.as_mut().unwrap();
                let entry = session.entry.as_mut().unwrap();
                let result =
                    if ["parent", "previous", "next", "exit", "up", "down"].contains(&command) {
                        entry.field.navigate(command);
                        Ok(())
                    } else {
                        entry.field.insert_at(command)
                    };
                result.err()
            };
            self.refresh_answer_entry(true);
            if let Some(error) = error
                && let Some(session) = self.practice.borrow().as_ref()
            {
                session.window.set_entry_error(error.into());
            }
        }
        if let Some(session) = self.practice.borrow().as_ref() {
            session.window.invoke_focus_answer();
        }
    }

    fn refresh_answer_entry(&self, reset_input: bool) {
        let mut practice = self.practice.borrow_mut();
        let Some(session) = practice.as_mut().filter(|session| {
            !session.answered
                && session.prepared.mode == AnswerMode::Input
                && !session.prepared.knowledge_only
        }) else {
            return;
        };
        let window = &session.window;
        if let Some(entry) = session.entry.as_mut() {
            if reset_input {
                window.set_entry_input(entry.field.input().into());
            }
            let (position, navigation) = editing_state(
                &entry.field,
                window.get_entry_position().revision,
                reset_input,
            );
            window.set_entry_position(position);
            window.set_entry_navigation(navigation);
            window.set_entry_error(entry.field.error().unwrap_or_default().into());
            window.set_entry_can_move_out(entry.field.can_move_out());
            window.set_entry_can_unwrap(entry.field.can_unwrap());
            window.set_entry_can_undo(entry.field.can_undo());
            window.set_entry_can_redo(entry.field.can_redo());
            if let Ok(rendered) = formula::render_node(
                &entry.field.root,
                22,
                window.get_pixel_scale(),
                (232, 232, 232),
            ) {
                let selected: Vec<_> = rendered
                    .regions
                    .iter()
                    .filter(|region| region.path.starts_with(&entry.field.selection))
                    .collect();
                let selection = if entry.field.selection.is_empty() {
                    MathRegion {
                        x: 0.0,
                        y: 0.0,
                        width: rendered.width,
                        height: rendered.height,
                    }
                } else if selected.is_empty() {
                    MathRegion::default()
                } else {
                    let left = selected.iter().map(|r| r.x).fold(f32::INFINITY, f32::min);
                    let top = selected.iter().map(|r| r.y).fold(f32::INFINITY, f32::min);
                    let right = selected.iter().map(|r| r.x + r.width).fold(0.0, f32::max);
                    let bottom = selected.iter().map(|r| r.y + r.height).fold(0.0, f32::max);
                    MathRegion {
                        x: left,
                        y: top,
                        width: right - left,
                        height: bottom - top,
                    }
                };
                window.set_entry_selection(selection);
                window.set_entry_regions(ModelRc::new(VecModel::from(
                    rendered
                        .regions
                        .iter()
                        .map(|r| MathRegion {
                            x: r.x,
                            y: r.y,
                            width: r.width,
                            height: r.height,
                        })
                        .collect::<Vec<_>>(),
                )));
                entry.paths = rendered.regions.iter().map(|r| r.path.clone()).collect();
                window.set_input_preview(formula_presentation::option(
                    entry.field.root.plain().into(),
                    rendered,
                ));
            }
        } else {
            window.set_entry_navigation(FormulaNavigation::default());
            let input = window.get_answer_text();
            let preview = math_answer::recognize_inline(
                input.as_str(),
                session.question.card.answer_latex.is_some(),
            )
            .and_then(|node| {
                let rendered =
                    formula::render_node(&node, 22, window.get_pixel_scale(), (232, 232, 232))
                        .ok()?;
                let layout = InputLayout::new(
                    input.as_str(),
                    &node,
                    &rendered.regions,
                    rendered.width,
                    rendered.height,
                )?;
                Some((rendered, layout))
            })
            .map(|(rendered, layout)| {
                session.inline_input = Some(layout);
                formula_presentation::option(input.clone(), rendered)
            })
            .unwrap_or_default();
            if preview.formula_width <= 0.0 {
                session.inline_input = None;
            }
            window.set_input_preview(preview);
            let position = window.get_answer_position();
            let caret = if reset_input {
                input.len() as i32
            } else {
                position.caret
            };
            sync_answer_position(
                session,
                if reset_input { caret } else { position.anchor },
                caret,
                reset_input,
            );
        }
    }

    fn submit_answer(self: &Rc<Self>, input: &str) {
        if self
            .practice
            .borrow()
            .as_ref()
            .is_some_and(|session| session.entry.is_some())
        {
            if let Some(input) = self.apply_answer_entry() {
                self.submit_answer(&input);
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
                        session.window.set_entry_error(error.into());
                        return;
                    }
                }
            }
            self.finish_answer(correct, None);
        }
    }

    fn choose_answer(self: &Rc<Self>, index: i32) {
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

    fn finish_answer(self: &Rc<Self>, correct: bool, choice: Option<usize>) {
        self.answer_preview_timer.stop();
        // Enter can arrive before the preview debounce. Finish the current
        // layout and retain it while the same input receives result colours.
        self.refresh_answer_entry(false);
        let review_token = {
            let mut session = self.practice.borrow_mut();
            let Some(session) = session.as_mut() else {
                return;
            };
            if session.answered {
                return;
            }
            session.answered = true;
            session.entry = None;
            session.window.set_entry_open(false);
            session.window.set_entry_regions(ModelRc::default());
            session
                .window
                .set_selected_option(choice.map_or(-1, |index| index as i32));
            session.window.set_feedback(if correct { 1 } else { 2 });
            // A disabled option must not keep the focus needed by Enter/Esc.
            session.window.invoke_focus_answer();
            session.review_token.take()
        };
        self.arm_card_close();
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

    fn arm_card_close(self: &Rc<Self>) {
        let duration = Duration::from_secs(self.library.borrow().settings.feedback_seconds.into());
        let weak = Rc::downgrade(self);
        self.feedback_timer
            .start(TimerMode::SingleShot, duration, move || {
                if let Some(app) = weak.upgrade() {
                    app.close_practice();
                }
            });
    }

    fn close_practice(self: &Rc<Self>) {
        self.close_image_preview();
        self.answer_preview_timer.stop();
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
                && FloatingWindow::position(session.window.window())
                    != session.initial_position.get()
                && let Some(position) = FloatingWindow::remember(session.window.window())
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
    }

    fn arm_reminder(self: &Rc<Self>) {
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

    fn wait_for_reminder(self: &Rc<Self>, due: chrono::DateTime<chrono::Local>) {
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
                    let result = app.show_practice_with_origin(PracticeOrigin::Reminder);
                    app.report(result, "");
                }
            });
    }

    fn notify(&self, message: &str, is_error: bool) {
        if is_error {
            log::warn!(target: "try::app", "event=operation_rejected reason={message}");
        }
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
        self.close_image_preview();
        log::info!(target: "try::app", "event=quit_requested");
        self.close_practice();
        self.reminder_timer.stop();
        self.pause_timer.stop();
        self.feedback_timer.stop();
        self.automatic_update_timer.stop();
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
    if let Some(layout) = session
        .inline_input
        .as_ref()
        .filter(|layout| layout.source.as_str() == input.as_str())
    {
        session
            .window
            .set_input_caret(input_region(layout.caret(caret)));
        session
            .window
            .set_input_selection(ModelRc::new(VecModel::from(
                layout
                    .selection(anchor, caret)
                    .into_iter()
                    .map(input_region)
                    .collect::<Vec<_>>(),
            )));
    } else {
        session.window.set_input_caret(MathRegion::default());
        session.window.set_input_selection(ModelRc::default());
    }
}

fn input_region(rect: InputRect) -> MathRegion {
    MathRegion {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn editing_state(
    field: &Field,
    revision: i32,
    reset: bool,
) -> (FormulaPosition, FormulaNavigation) {
    let caret = field.text_selection();
    (
        FormulaPosition {
            anchor: caret.anchor as i32,
            caret: caret.focus as i32,
            bytes: field.input().len() as i32,
            revision: if reset {
                revision.wrapping_add(1)
            } else {
                revision
            },
        },
        FormulaNavigation {
            previous: field.can_navigate("previous"),
            next: field.can_navigate("next"),
            parent: field.can_navigate("parent"),
            exit: field.can_navigate("exit"),
            up: field.can_navigate("up"),
            down: field.can_navigate("down"),
            enter: field.has_trailing_structure(),
            container: !field.selected().children.is_empty() && !field.selected().accepts_prose(),
        },
    )
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
#[path = "native_checks.rs"]
mod native_checks;

#[cfg(all(test, feature = "native_checks", target_os = "windows"))]
#[path = "native_picture_checks.rs"]
mod native_picture_checks;

#[cfg(all(test, feature = "native_checks", target_os = "windows"))]
#[path = "tray_checks.rs"]
mod tray_checks;
