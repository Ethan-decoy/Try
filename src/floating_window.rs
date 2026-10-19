use slint::winit_030::{WinitWindowAccessor, winit};
use std::cell::{Cell, RefCell};
use r#try::knowledge::{Appearance, Settings};
use r#try::window_position::{PopupCorner, SavedPopupPosition, WorkArea};
use winit::dpi::PhysicalPosition;
use winit::window::{Theme, WindowAttributes, WindowLevel};

const PRACTICE_TITLE: &str = "Try · 练习";

/// Watches input ownership only while an unfocused reminder exists. It neither
/// activates the card nor keeps the native window alive after Slint hides it.
pub struct ReminderInputGuard {
    #[cfg(target_os = "windows")]
    _watch: windows::InputWatch,
}

impl ReminderInputGuard {
    pub fn install(native: &std::sync::Arc<winit::window::Window>) -> Result<Self, String> {
        #[cfg(target_os = "windows")]
        return windows::InputWatch::install(native).map(|watch| Self { _watch: watch });
        #[cfg(not(target_os = "windows"))]
        {
            let _ = native;
            Ok(Self {})
        }
    }
}

pub struct FloatingWindow {
    preferences: RefCell<(PopupCorner, Option<SavedPopupPosition>)>,
    placement: RefCell<Option<(winit::window::WindowId, Placement)>>,
    appearance: Cell<Appearance>,
    creating_practice: Cell<bool>,
    creating_passive: Cell<bool>,
}

/// Keep the user's anchor separate from positions used to fit a larger card.
struct Placement {
    corner: PopupCorner,
    preferred: Option<SavedPopupPosition>,
    placed: (i32, i32),
    user_moved: bool,
}

impl Placement {
    fn new(
        area: &WorkArea,
        size: (u32, u32),
        corner: PopupCorner,
        saved: Option<&SavedPopupPosition>,
    ) -> Self {
        let preferred = saved
            .filter(|saved| saved.monitor == area.monitor && saved.is_valid())
            .cloned();
        Self {
            corner,
            placed: area.place(size, corner, preferred.as_ref()),
            preferred,
            user_moved: false,
        }
    }

    fn observe_move(&mut self, area: &WorkArea, position: (i32, i32)) {
        if position != self.placed {
            self.preferred = Some(area.remember(position));
            self.placed = position;
            self.user_moved = true;
        }
    }

    fn resize(&mut self, area: &WorkArea, size: (u32, u32), current: (i32, i32)) -> (i32, i32) {
        self.observe_move(area, current);
        self.placed = area.place(size, self.corner, self.preferred.as_ref());
        self.placed
    }

    fn moved_position(
        &mut self,
        area: &WorkArea,
        current: (i32, i32),
    ) -> Option<SavedPopupPosition> {
        self.observe_move(area, current);
        self.preferred
            .as_ref()
            .filter(|saved| self.user_moved && saved.is_valid())
            .cloned()
    }
}

pub struct PracticeCreation<'a> {
    flag: &'a Cell<bool>,
    previous: bool,
    passive: &'a Cell<bool>,
    previous_passive: bool,
}
impl Drop for PracticeCreation<'_> {
    fn drop(&mut self) {
        self.flag.set(self.previous);
        self.passive.set(self.previous_passive);
    }
}

impl FloatingWindow {
    #[cfg(target_os = "windows")]
    pub fn system_dark() -> bool {
        use windows_sys::Win32::System::Registry::{
            HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW,
        };
        let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
            .encode_utf16()
            .collect();
        let name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
        let mut light = 1u32;
        let mut bytes = std::mem::size_of_val(&light) as u32;
        // SAFETY: both strings are terminated and light is a writable DWORD.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&raw mut light).cast(),
                &mut bytes,
            )
        };
        result == 0 && light == 0
    }

    #[cfg(target_os = "windows")]
    pub fn watch_system_theme(window: &slint::Window, notify: impl Fn() + 'static) {
        use windows_sys::Win32::UI::Shell::SetWindowSubclass;
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        window.with_winit_window(move |native| {
            let Ok(handle) = native.window_handle() else {
                return;
            };
            let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                return;
            };
            let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
            let watcher = Box::into_raw(Box::new(ThemeWatch {
                notify: Box::new(notify),
            }));
            // SAFETY: the boxed callback remains live until WM_NCDESTROY removes the subclass.
            if unsafe {
                SetWindowSubclass(
                    hwnd,
                    Some(theme_watch_proc),
                    THEME_WATCH_ID,
                    watcher as usize,
                )
            } == 0
            {
                // SAFETY: installation failed, so ownership never left this function.
                unsafe {
                    drop(Box::from_raw(watcher));
                }
            }
        });
    }

    pub fn new(settings: &Settings) -> Self {
        let controller = Self {
            preferences: RefCell::default(),
            placement: RefCell::default(),
            appearance: Cell::new(Appearance::default()),
            creating_practice: Cell::new(false),
            creating_passive: Cell::new(false),
        };
        controller.configure(settings);
        controller
    }

    pub fn configure(&self, settings: &Settings) {
        self.appearance.set(settings.appearance);
        self.preferences.replace((
            settings.popup_corner,
            settings
                .remember_position
                .then(|| settings.popup_position.clone())
                .flatten(),
        ));
    }

    pub fn attributes(&self, attributes: WindowAttributes) -> WindowAttributes {
        let icon = winit::window::Icon::from_rgba(
            include_bytes!(concat!(env!("OUT_DIR"), "/logo_rgba.bin")).to_vec(),
            32,
            32,
        )
        .ok();
        let theme = match self.appearance.get() {
            Appearance::Light => Some(Theme::Light),
            Appearance::Dark => Some(Theme::Dark),
            Appearance::System => None,
        };
        let attributes = attributes.with_theme(theme).with_window_icon(icon.clone());
        #[cfg(target_os = "windows")]
        let attributes = {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes.with_taskbar_icon(icon)
        };
        // Slint calls this hook before applying the component's title and
        // no-frame properties, so identify the explicit creation scope.
        if !self.creating_practice.get() {
            log::debug!(target: "try::floating_window", "event=main_native_attributes");
            return attributes;
        }
        let attributes = attributes
            .with_title(PRACTICE_TITLE)
            .with_active(false)
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop);
        log::debug!(target: "try::floating_window",
            "event=practice_native_attributes decorations={} active={} resizable={}",
            attributes.decorations,
            attributes.active,
            attributes.resizable
        );
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            let mut attributes = attributes.with_skip_taskbar(true);
            if self.creating_passive.get() {
                // Configure input ownership before moving the asynchronously
                // created window into a visible work area.
                return attributes.with_position(PhysicalPosition::new(-32_000, -32_000));
            }
            let (corner, saved) = &*self.preferences.borrow();
            let areas = windows::work_areas();
            if let Some(area) = areas
                .iter()
                .find(|area| {
                    saved
                        .as_ref()
                        .is_some_and(|saved| saved.monitor == area.monitor)
                })
                .or(areas.first())
            {
                let size = attributes
                    .inner_size
                    .unwrap_or(winit::dpi::LogicalSize::new(320.0, 230.0).into())
                    .to_physical::<u32>(area.scale);
                let (x, y) = area.place((size.width, size.height), *corner, saved.as_ref());
                attributes.position = Some(PhysicalPosition::new(x, y).into());
            }
            attributes
        }
        #[cfg(not(target_os = "windows"))]
        {
            if self.creating_passive.get() {
                attributes.with_position(PhysicalPosition::new(-32_000, -32_000))
            } else {
                attributes
            }
        }
    }

    pub fn apply_theme(&self, window: &slint::Window, dark: bool) {
        let theme = Some(if dark { Theme::Dark } else { Theme::Light });
        #[cfg(target_os = "macos")]
        let theme = if self.appearance.get() == Appearance::System {
            None
        } else {
            theme
        };
        window.with_winit_window(|native| {
            // Winit 0.30 on Windows does not update theme() after this override.
            native.set_theme(theme);
        });
    }

    pub fn place(&self, window: &slint::Window) {
        window.with_winit_window(|native| {
            #[cfg(target_os = "windows")]
            {
                use winit::platform::windows::WindowExtWindows;
                native.set_skip_taskbar(true);
            }
            #[cfg(target_os = "windows")]
            let areas = windows::work_areas();
            #[cfg(not(target_os = "windows"))]
            let areas: Vec<WorkArea> = native
                .available_monitors()
                .map(|monitor| WorkArea {
                    monitor: monitor.name().unwrap_or_else(|| "display".into()),
                    x: monitor.position().x,
                    y: monitor.position().y,
                    width: monitor.size().width,
                    height: monitor.size().height,
                    scale: monitor.scale_factor(),
                })
                .collect();
            let (corner, saved) = &*self.preferences.borrow();
            if let Some(area) = areas
                .iter()
                .find(|area| {
                    saved
                        .as_ref()
                        .is_some_and(|saved| saved.monitor == area.monitor)
                })
                .or(areas.first())
            {
                let size = native.outer_size();
                let placement =
                    Placement::new(area, (size.width, size.height), *corner, saved.as_ref());
                let (x, y) = placement.placed;
                self.placement.replace(Some((native.id(), placement)));
                native.set_outer_position(PhysicalPosition::new(x, y));
            }
            shape(native);
        });
    }

    pub fn practice_creation(&self) -> PracticeCreation<'_> {
        self.creation(false)
    }

    pub fn passive_creation(&self) -> PracticeCreation<'_> {
        self.creation(true)
    }

    fn creation(&self, passive: bool) -> PracticeCreation<'_> {
        PracticeCreation {
            flag: &self.creating_practice,
            previous: self.creating_practice.replace(true),
            passive: &self.creating_passive,
            previous_passive: self.creating_passive.replace(passive),
        }
    }

    pub fn prepare_frame(window: &slint::Window) {
        window.with_winit_window(shape);
    }

    pub fn reshape_and_clamp(&self, window: &slint::Window) {
        window.with_winit_window(|native| {
            shape(native);
            if let (Some(area), Ok(position)) = (current_area(native), native.outer_position()) {
                let size = native.outer_size();
                let (x, y) = {
                    let mut placement = self.placement.borrow_mut();
                    // Initial placement establishes the anchor. Late callbacks
                    // from a closed card must not replace the next card's state.
                    let Some((_, placement)) =
                        placement.as_mut().filter(|(id, _)| *id == native.id())
                    else {
                        return;
                    };
                    placement.resize(&area, (size.width, size.height), (position.x, position.y))
                };
                if x != position.x || y != position.y {
                    native.set_outer_position(PhysicalPosition::new(x, y));
                }
            }
        });
    }

    pub fn moved_position(&self, window: &slint::Window) -> Option<SavedPopupPosition> {
        window
            .with_winit_window(|native| {
                let area = current_area(native)?;
                let position = native.outer_position().ok()?;
                let mut placement = self.placement.borrow_mut();
                let (_, placement) = placement.as_mut().filter(|(id, _)| *id == native.id())?;
                placement.moved_position(&area, (position.x, position.y))
            })
            .flatten()
    }

    pub fn activate(window: &slint::Window) {
        window.with_winit_window(|native| {
            log::debug!(target: "try::floating_window", "event=window_activation_requested window={:?}", native.id());
            #[cfg(target_os = "windows")]
            windows::activate(native);
            #[cfg(not(target_os = "windows"))]
            native.focus_window();
        });
    }

    pub fn reminders_allowed(background: bool) -> bool {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::UI::Shell::SHQueryUserNotificationState;
            let mut state = 0;
            // SAFETY: state is a live output value for this synchronous Shell query.
            let result = unsafe { SHQueryUserNotificationState(&mut state) };
            // A background card may coexist with borderless full-screen apps.
            // Exclusive Direct3D, presentation, locked and unknown states defer.
            result >= 0 && windows::allows_reminder(state, background)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = background;
            true
        }
    }

    pub fn set_mouse_passthrough(window: &slint::Window, passive: bool) -> Result<(), String> {
        window
            .with_winit_window(|native| {
                native
                    .set_cursor_hittest(!passive)
                    .map_err(|error| r#try::tr!("无法设置提醒交互：{error}", error = error))?;
                #[cfg(target_os = "windows")]
                if passive {
                    windows::opaque_layer(native)?;
                }
                native.request_redraw();
                Ok(())
            })
            .unwrap_or_else(|| Err(r#try::tr!("提醒窗口尚未就绪。").to_owned()))
    }

    pub fn position(window: &slint::Window) -> Option<(i32, i32)> {
        window
            .with_winit_window(|native| {
                native
                    .outer_position()
                    .ok()
                    .map(|position| (position.x, position.y))
            })
            .flatten()
    }

    pub fn tag_on_right(window: &slint::Window) -> Option<bool> {
        window
            .with_winit_window(|native| {
                let position = native.outer_position().ok()?;
                let area = current_area(native)?;
                let size = native.outer_size();
                Some(area.tag_on_right((position.x, position.y), (size.width, size.height)))
            })
            .flatten()
    }

    #[cfg(all(test, feature = "native_checks", target_os = "windows"))]
    pub fn remember(window: &slint::Window) -> Option<SavedPopupPosition> {
        window
            .with_winit_window(|native| {
                let position = native.outer_position().ok()?;
                let area = current_area(native)?;
                let saved = area.remember((position.x, position.y));
                saved.is_valid().then_some(saved)
            })
            .flatten()
    }
}

fn current_area(native: &winit::window::Window) -> Option<WorkArea> {
    #[cfg(target_os = "windows")]
    return windows::current_area(native);
    #[cfg(not(target_os = "windows"))]
    {
        let monitor = native.current_monitor()?;
        Some(WorkArea {
            monitor: monitor.name().unwrap_or_else(|| "display".into()),
            x: monitor.position().x,
            y: monitor.position().y,
            width: monitor.size().width,
            height: monitor.size().height,
            scale: monitor.scale_factor(),
        })
    }
}

#[cfg(target_os = "windows")]
const THEME_WATCH_ID: usize = 0x54525954;

#[cfg(target_os = "windows")]
struct ThemeWatch {
    notify: Box<dyn Fn()>,
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn theme_watch_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    _id: usize,
    data: usize,
) -> isize {
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_NCDESTROY, WM_SETTINGCHANGE};
    let watcher = data as *mut ThemeWatch;
    if message == WM_SETTINGCHANGE {
        // SAFETY: SetWindowSubclass owns this allocation while the subclass is installed.
        unsafe {
            ((*watcher).notify)();
        }
    } else if message == WM_NCDESTROY {
        // SAFETY: this is the final callback for this window; remove before freeing its data.
        unsafe {
            RemoveWindowSubclass(hwnd, Some(theme_watch_proc), THEME_WATCH_ID);
            drop(Box::from_raw(watcher));
        }
    }
    // SAFETY: preserve the normal message chain for every message.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn shape(native: &winit::window::Window) {
    log::debug!(target: "try::floating_window",
        "event=window_shape_requested window={:?} decorations={} scale={} size={:?} position={:?}",
        native.id(),
        native.is_decorated(),
        native.scale_factor(),
        native.outer_size(),
        native.outer_position()
    );
    #[cfg(target_os = "windows")]
    windows::round_corners(native);
    #[cfg(not(target_os = "windows"))]
    let _ = native;
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateRoundRectRgn, DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR,
        MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromWindow, SetWindowRgn,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_STYLE, GetWindowLongPtrW, IsIconic, SetWindowLongPtrW, WM_NCACTIVATE, WM_NCDESTROY,
        WM_NCPAINT, WM_SETICON, WM_SETTEXT, WS_VISIBLE,
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    fn handle(native: &winit::window::Window) -> Option<HWND> {
        match native.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
            _ => None,
        }
    }

    pub fn activate(native: &winit::window::Window) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, SetForegroundWindow,
        };
        let Some(hwnd) = handle(native) else {
            return;
        };
        // Winit 0.30's focus_window injects an extended Alt press/release to
        // bypass foreground restrictions. Global hotkeys observe those events.
        // Request activation directly and respect the system's focus decision.
        // SAFETY: hwnd is borrowed from this live window on its owning thread.
        if unsafe { GetForegroundWindow() != hwnd && SetForegroundWindow(hwnd) == 0 } {
            native.request_user_attention(Some(winit::window::UserAttentionType::Informational));
            log::debug!(target: "try::floating_window", "event=window_activation_deferred");
        }
    }

    pub fn allows_reminder(state: i32, background: bool) -> bool {
        use windows_sys::Win32::UI::Shell::{QUNS_ACCEPTS_NOTIFICATIONS, QUNS_BUSY};
        state == QUNS_ACCEPTS_NOTIFICATIONS || (background && state == QUNS_BUSY)
    }

    #[test]
    fn reminder_notification_policy() {
        use windows_sys::Win32::UI::Shell::{
            QUNS_ACCEPTS_NOTIFICATIONS, QUNS_APP, QUNS_BUSY, QUNS_NOT_PRESENT,
            QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME, QUNS_RUNNING_D3D_FULL_SCREEN,
        };
        assert!(allows_reminder(QUNS_ACCEPTS_NOTIFICATIONS, false));
        assert!(allows_reminder(QUNS_ACCEPTS_NOTIFICATIONS, true));
        assert!(allows_reminder(QUNS_BUSY, true));
        assert!(!allows_reminder(QUNS_BUSY, false));
        for state in [
            QUNS_NOT_PRESENT,
            QUNS_PRESENTATION_MODE,
            QUNS_QUIET_TIME,
            QUNS_RUNNING_D3D_FULL_SCREEN,
            QUNS_APP,
            0,
            -1,
            999,
        ] {
            assert!(!allows_reminder(state, false));
            assert!(!allows_reminder(state, true));
        }
    }

    thread_local! {
        static INPUT_WATCHES: RefCell<std::collections::BTreeMap<usize, std::rc::Weak<InputState>>> = RefCell::default();
    }

    pub struct InputWatch {
        _state: std::rc::Rc<InputState>,
        hooks: Vec<windows_sys::Win32::UI::Accessibility::HWINEVENTHOOK>,
    }

    struct InputState {
        native: std::sync::Weak<winit::window::Window>,
        foreground: Cell<HWND>,
        passthrough: Cell<Option<bool>>,
        pending: Cell<bool>,
    }

    impl InputState {
        fn refresh(&self) -> Result<(), String> {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                CURSOR_SHOWING, CURSORINFO, GUITHREADINFO, GetCursorInfo, GetForegroundWindow,
                GetGUIThreadInfo, GetWindowThreadProcessId, HWND_TOPMOST, SWP_NOACTIVATE,
                SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
            };
            let Some(native) = self.native.upgrade() else {
                return Ok(());
            };
            let hwnd =
                handle(&native).ok_or_else(|| r#try::tr!("提醒窗口尚未就绪。").to_owned())?;
            let mut cursor = CURSORINFO {
                cbSize: size_of::<CURSORINFO>() as u32,
                ..Default::default()
            };
            let mut foreground = GUITHREADINFO {
                cbSize: size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            // SAFETY: read-only query of the active desktop's foreground handle.
            let active = unsafe { GetForegroundWindow() };
            // SAFETY: synchronous read-only desktop queries with correctly sized
            // output structures. Unknown ownership keeps clicks with the game.
            let passthrough = unsafe {
                active != hwnd
                    && (GetCursorInfo(&mut cursor) == 0
                        || cursor.flags & CURSOR_SHOWING == 0
                        || GetGUIThreadInfo(0, &mut foreground) == 0
                        || (!foreground.hwndCapture.is_null() && foreground.hwndCapture != hwnd))
            };
            if self.passthrough.get() != Some(passthrough) {
                native
                    .set_cursor_hittest(!passthrough)
                    .map_err(|error| r#try::tr!("无法设置提醒交互：{error}", error = error))?;
                if passthrough {
                    opaque_layer(&native)?;
                }
                self.passthrough.set(Some(passthrough));
                native.request_redraw();
            }
            if self.foreground.replace(active) != active && !active.is_null() && active != hwnd {
                let mut process = 0;
                // A newly activated topmost game can cover another topmost
                // window. Reorder only our card, without activating or moving it.
                // Leave our own previews/dialogs above the card.
                // SAFETY: hwnd is our live UI window; process is a live output.
                unsafe {
                    GetWindowThreadProcessId(active, &mut process);
                    if process != 0
                        && process != windows_sys::Win32::System::Threading::GetCurrentProcessId()
                        && SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                        ) == 0
                    {
                        return Err(r#try::tr!(
                            "无法设置提醒交互：{error}",
                            error = std::io::Error::last_os_error()
                        ));
                    }
                }
            }
            Ok(())
        }

        fn schedule(self: &std::rc::Rc<Self>) {
            if self.pending.replace(true) {
                return;
            }
            let weak = std::rc::Rc::downgrade(self);
            // Out-of-context events can reenter while native styles change.
            // Coalesce them on the UI loop without periodic polling.
            slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                if let Some(state) = weak.upgrade() {
                    state.pending.set(false);
                    if let Err(error) = state.refresh() {
                        // Fail closed rather than risk an interactive game overlay.
                        if let Some(native) = state.native.upgrade() {
                            native.set_visible(false);
                        }
                        log::warn!(target: "try::floating_window", "event=reminder_input_failed reason={error}");
                    }
                }
            });
        }
    }

    unsafe extern "system" fn input_event(
        hook: windows_sys::Win32::UI::Accessibility::HWINEVENTHOOK,
        event: u32,
        _hwnd: HWND,
        object: i32,
        _child: i32,
        _thread: u32,
        _time: u32,
    ) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            EVENT_OBJECT_HIDE, EVENT_OBJECT_SHOW, OBJID_CURSOR,
        };
        if matches!(event, EVENT_OBJECT_SHOW | EVENT_OBJECT_HIDE) && object != OBJID_CURSOR {
            return;
        }
        let state = INPUT_WATCHES.with(|watches| {
            watches
                .borrow()
                .get(&(hook as usize))
                .and_then(std::rc::Weak::upgrade)
        });
        if let Some(state) = state {
            state.schedule();
        }
    }

    impl InputWatch {
        pub fn install(native: &std::sync::Arc<winit::window::Window>) -> Result<Self, String> {
            use windows_sys::Win32::UI::Accessibility::SetWinEventHook;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                EVENT_OBJECT_HIDE, EVENT_OBJECT_SHOW, EVENT_SYSTEM_CAPTUREEND,
                EVENT_SYSTEM_CAPTURESTART, EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
            };
            let state = std::rc::Rc::new(InputState {
                native: std::sync::Arc::downgrade(native),
                foreground: Cell::new(std::ptr::null_mut()),
                passthrough: Cell::new(None),
                pending: Cell::new(false),
            });
            let mut watch = Self {
                _state: state.clone(),
                hooks: Vec::new(),
            };
            for (first, last) in [
                (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
                (EVENT_SYSTEM_CAPTURESTART, EVENT_SYSTEM_CAPTUREEND),
                (EVENT_OBJECT_SHOW, EVENT_OBJECT_HIDE),
            ] {
                // SAFETY: events are queued to this UI thread; no code is
                // injected into another process. Drop removes every subscription.
                let hook = unsafe {
                    SetWinEventHook(
                        first,
                        last,
                        std::ptr::null_mut(),
                        Some(input_event),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT,
                    )
                };
                if hook.is_null() {
                    return Err(r#try::tr!(
                        "无法设置提醒交互：{error}",
                        error = std::io::Error::last_os_error()
                    ));
                }
                INPUT_WATCHES.with(|watches| {
                    watches
                        .borrow_mut()
                        .insert(hook as usize, std::rc::Rc::downgrade(&state));
                });
                watch.hooks.push(hook);
            }
            state.refresh()?;
            Ok(watch)
        }
    }

    impl Drop for InputWatch {
        fn drop(&mut self) {
            for hook in self.hooks.drain(..) {
                INPUT_WATCHES.with(|watches| {
                    watches.borrow_mut().remove(&(hook as usize));
                });
                // SAFETY: each handle was installed on this same UI thread.
                unsafe {
                    windows_sys::Win32::UI::Accessibility::UnhookWinEvent(hook);
                }
            }
        }
    }

    pub fn opaque_layer(native: &winit::window::Window) -> Result<(), String> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{LWA_ALPHA, SetLayeredWindowAttributes};
        let hwnd = handle(native).ok_or_else(|| r#try::tr!("提醒窗口尚未就绪。").to_owned())?;
        // Winit enables WS_EX_LAYERED for passthrough. Our GDI software surface
        // needs explicit opaque alpha before the layered window can be visible.
        // SAFETY: hwnd belongs to this live Winit window on its owning UI thread.
        if unsafe { SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA) } == 0 {
            return Err(r#try::tr!(
                "无法设置提醒交互：{error}",
                error = std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    fn area(monitor: HMONITOR) -> Option<(WorkArea, bool)> {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        // SAFETY: info is a correctly sized, live MONITORINFOEXW output buffer.
        if unsafe { GetMonitorInfoW(monitor, &mut info as *mut _ as *mut MONITORINFO) } == 0 {
            return None;
        }
        let mut dpi_x = 96;
        let mut dpi_y = 96;
        // SAFETY: EnumDisplayMonitors/MonitorFromWindow provided a valid monitor;
        // both DPI outputs remain live for the duration of this call.
        unsafe {
            GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        }
        let end = info
            .szDevice
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(info.szDevice.len());
        let rect = info.monitorInfo.rcWork;
        Some((
            WorkArea {
                monitor: String::from_utf16_lossy(&info.szDevice[..end]),
                x: rect.left,
                y: rect.top,
                width: (rect.right - rect.left).max(0) as u32,
                height: (rect.bottom - rect.top).max(0) as u32,
                scale: f64::from(dpi_x.max(96)) / 96.0,
            },
            info.monitorInfo.dwFlags & 1 != 0,
        ))
    }

    pub fn work_areas() -> Vec<WorkArea> {
        unsafe extern "system" fn collect(
            monitor: HMONITOR,
            _dc: HDC,
            _rect: *mut RECT,
            data: LPARAM,
        ) -> i32 {
            // SAFETY: the synchronous enumeration owns this Vec for its full duration.
            let areas = unsafe { &mut *(data as *mut Vec<(WorkArea, bool)>) };
            if let Some(area) = area(monitor) {
                areas.push(area);
            }
            1
        }
        let mut areas: Vec<(WorkArea, bool)> = Vec::new();
        // SAFETY: callback and context pointer are valid until enumeration returns.
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(collect),
                &mut areas as *mut _ as LPARAM,
            );
        }
        areas.sort_by_key(|(_, primary)| !*primary);
        areas.into_iter().map(|(area, _)| area).collect()
    }

    pub fn current_area(native: &winit::window::Window) -> Option<WorkArea> {
        // SAFETY: hwnd is borrowed from the live Winit window.
        let monitor = unsafe { MonitorFromWindow(handle(native)?, MONITOR_DEFAULTTONEAREST) };
        area(monitor).map(|(area, _)| area)
    }

    const FRAMELESS_SUBCLASS: usize = 0x747279;
    // Undocumented theme messages also handled by Chromium's custom frames.
    // The native move loop sends these instead of WM_NCPAINT for caption buttons.
    const WM_NCUAHDRAWCAPTION: u32 = 0x00ae;
    const WM_NCUAHDRAWFRAME: u32 = 0x00af;

    unsafe extern "system" fn frameless_messages(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        id: usize,
        _data: usize,
    ) -> LRESULT {
        // SAFETY: Windows invokes this callback on the owning UI thread. It
        // stores no Rust references and removes itself with the native window.
        unsafe {
            match message {
                WM_NCUAHDRAWCAPTION | WM_NCUAHDRAWFRAME => 0,
                WM_NCPAINT if IsIconic(hwnd) == 0 => 0,
                WM_NCACTIVATE if IsIconic(hwnd) == 0 => {
                    // Winit still needs activation messages for keyboard focus.
                    // -1 asks DefWindowProc to skip only its frame paint.
                    DefSubclassProc(hwnd, message, wparam, -1)
                }
                WM_SETTEXT | WM_SETICON => {
                    // Default title/icon updates can paint a caption directly,
                    // bypassing WM_NCPAINT. Keep their native metadata update
                    // while inhibiting paint for this synchronous call only.
                    // No ShowWindow or frame refresh is performed here.
                    let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                    if style as u32 & WS_VISIBLE == 0 {
                        return DefSubclassProc(hwnd, message, wparam, lparam);
                    }
                    SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(WS_VISIBLE as isize));
                    let result = DefSubclassProc(hwnd, message, wparam, lparam);
                    SetWindowLongPtrW(hwnd, GWL_STYLE, style);
                    result
                }
                WM_NCDESTROY => {
                    RemoveWindowSubclass(hwnd, Some(frameless_messages), id);
                    DefSubclassProc(hwnd, message, wparam, lparam)
                }
                _ => DefSubclassProc(hwnd, message, wparam, lparam),
            }
        }
    }

    pub fn round_corners(native: &winit::window::Window) {
        let Some(hwnd) = handle(native) else {
            return;
        };
        // Native clipping changes Windows' frame painting path. Keep Winit's
        // own styles intact and prevent default caption paint for this card's
        // lifetime. Reinstalling the same callback/id simply updates it.
        // SAFETY: this live practice HWND is borrowed on its owning UI thread.
        if unsafe { SetWindowSubclass(hwnd, Some(frameless_messages), FRAMELESS_SUBCLASS, 0) } == 0
        {
            log::warn!(target: "try::floating_window",
                "event=window_subclass_failed reason={}",
                std::io::Error::last_os_error()
            );
            return;
        }
        let size = native.outer_size();
        let diameter = (32.0 * native.scale_factor()).round() as i32;
        // The software surface is opaque on Windows. A native window region cuts
        // the corners out of both rendering and mouse hit testing.
        // SAFETY: scalar dimensions are bounded by the live window; HWND is valid.
        let region = unsafe {
            CreateRoundRectRgn(
                0,
                0,
                size.width as i32 + 1,
                size.height as i32 + 1,
                diameter,
                diameter,
            )
        };
        if region.is_null() {
            log::warn!(target: "try::floating_window",
                "event=window_region_create_failed reason={}",
                std::io::Error::last_os_error()
            );
            return;
        }
        // Windows owns the region after a successful SetWindowRgn; release only on failure.
        if unsafe { SetWindowRgn(hwnd, region, 1) } == 0 {
            log::warn!(target: "try::floating_window",
                "event=window_region_apply_failed reason={}",
                std::io::Error::last_os_error()
            );
            unsafe {
                DeleteObject(region);
            }
        }
    }
}

#[cfg(test)]
mod placement_tests {
    use super::*;

    fn area() -> WorkArea {
        WorkArea {
            monitor: "test".into(),
            x: -1920,
            y: -200,
            width: 1920,
            height: 1320,
            scale: 1.5,
        }
    }

    #[test]
    fn resizing_keeps_all_four_corners_without_saving_temporary_positions() {
        let area = area();
        for corner in [
            PopupCorner::BottomRight,
            PopupCorner::BottomLeft,
            PopupCorner::TopRight,
            PopupCorner::TopLeft,
        ] {
            let small = (480, 345);
            let large = (480, 795);
            let mut placement = Placement::new(&area, small, corner, None);
            let original = placement.placed;
            for _ in 0..5 {
                let expanded = placement.resize(&area, large, placement.placed);
                assert_eq!(expanded, area.place(large, corner, None));
                assert!(placement.moved_position(&area, expanded).is_none());
                let collapsed = placement.resize(&area, small, expanded);
                assert_eq!(collapsed, original);
                assert!(placement.moved_position(&area, collapsed).is_none());
            }
        }
    }

    #[test]
    fn resizing_restores_saved_position_after_temporary_clamping() {
        let area = area();
        let preferred = (-700, 700);
        let saved = area.remember(preferred);
        let mut placement =
            Placement::new(&area, (480, 345), PopupCorner::BottomRight, Some(&saved));
        assert_eq!(placement.placed, preferred);
        let expanded = placement.resize(&area, (480, 795), preferred);
        assert!(expanded.1 < preferred.1);
        assert!(placement.moved_position(&area, expanded).is_none());
        assert_eq!(placement.resize(&area, (480, 345), expanded), preferred);
    }

    #[test]
    fn dragging_an_expanded_card_replaces_the_anchor_and_is_remembered() {
        let area = area();
        let mut placement = Placement::new(&area, (480, 345), PopupCorner::BottomRight, None);
        let original = placement.placed;
        placement.resize(&area, (480, 795), original);
        let dragged = (-800, 180);
        assert_eq!(placement.resize(&area, (480, 345), dragged), dragged);
        assert_eq!(
            placement.moved_position(&area, dragged),
            Some(area.remember(dragged))
        );
        let expanded = placement.resize(&area, (480, 795), dragged);
        assert_eq!(placement.resize(&area, (480, 345), expanded), dragged);
    }
}
