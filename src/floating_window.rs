use slint::winit_030::{WinitWindowAccessor, winit};
use std::cell::{Cell, RefCell};
use r#try::knowledge::{Appearance, Settings};
use r#try::window_position::{PopupCorner, SavedPopupPosition, WorkArea};
use winit::dpi::PhysicalPosition;
use winit::window::{Theme, WindowAttributes, WindowLevel};

const PRACTICE_TITLE: &str = "Try · 练习";

pub struct FloatingWindow {
    preferences: RefCell<(PopupCorner, Option<SavedPopupPosition>)>,
    appearance: Cell<Appearance>,
    creating_practice: Cell<bool>,
}

pub struct PracticeCreation<'a> {
    flag: &'a Cell<bool>,
    previous: bool,
}
impl Drop for PracticeCreation<'_> {
    fn drop(&mut self) {
        self.flag.set(self.previous);
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
            appearance: Cell::new(Appearance::default()),
            creating_practice: Cell::new(false),
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
            attributes
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
                let (x, y) = area.place((size.width, size.height), *corner, saved.as_ref());
                native.set_outer_position(PhysicalPosition::new(x, y));
            }
            shape(native);
        });
    }

    pub fn practice_creation(&self) -> PracticeCreation<'_> {
        PracticeCreation {
            flag: &self.creating_practice,
            previous: self.creating_practice.replace(true),
        }
    }

    pub fn reshape_and_clamp(window: &slint::Window) {
        window.with_winit_window(|native| {
            shape(native);
            #[cfg(target_os = "windows")]
            if let (Some(area), Ok(position)) =
                (windows::current_area(native), native.outer_position())
            {
                let size = native.outer_size();
                let saved = area.remember((position.x, position.y));
                let (x, y) = area.place(
                    (size.width, size.height),
                    PopupCorner::BottomRight,
                    Some(&saved),
                );
                if x != position.x || y != position.y {
                    native.set_outer_position(PhysicalPosition::new(x, y));
                }
            }
        });
    }

    pub fn activate(window: &slint::Window) {
        window.with_winit_window(|native| {
            log::debug!(target: "try::floating_window", "event=window_activation_requested window={:?}", native.id());
            native.focus_window();
        });
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
                #[cfg(target_os = "windows")]
                let area = windows::current_area(native)?;
                #[cfg(not(target_os = "windows"))]
                let area = {
                    let monitor = native.current_monitor()?;
                    WorkArea {
                        monitor: monitor.name().unwrap_or_else(|| "display".into()),
                        x: monitor.position().x,
                        y: monitor.position().y,
                        width: monitor.size().width,
                        height: monitor.size().height,
                        scale: monitor.scale_factor(),
                    }
                };
                let size = native.outer_size();
                Some(area.tag_on_right((position.x, position.y), (size.width, size.height)))
            })
            .flatten()
    }

    pub fn remember(window: &slint::Window) -> Option<SavedPopupPosition> {
        window
            .with_winit_window(|native| {
                let position = native.outer_position().ok()?;
                #[cfg(target_os = "windows")]
                let area = windows::current_area(native)?;
                #[cfg(not(target_os = "windows"))]
                let area = {
                    let monitor = native.current_monitor()?;
                    WorkArea {
                        monitor: monitor.name()?,
                        x: monitor.position().x,
                        y: monitor.position().y,
                        width: monitor.size().width,
                        height: monitor.size().height,
                        scale: monitor.scale_factor(),
                    }
                };
                let saved = area.remember((position.x, position.y));
                saved.is_valid().then_some(saved)
            })
            .flatten()
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
