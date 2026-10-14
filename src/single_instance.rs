use slint::Timer;
use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;
use r#try::knowledge::AppResult;
use r#try::tr;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowExW, GetPropW, GetWindowThreadProcessId, HWND_MESSAGE,
    PostMessageW, RemovePropW, SetPropW, WM_APP, WM_NCDESTROY,
};

const OPEN_REQUEST: u32 = WM_APP + 0x547;
const SUBCLASS_ID: usize = 0x74727973;

struct Handler {
    callback: Rc<dyn Fn(bool)>,
    profile: Vec<u16>,
    attached: Cell<bool>,
}

/// Reuse Slint's existing tray message window: no extra window, thread or polling.
pub struct Instance {
    window: HWND,
    handler: Box<Handler>,
}

fn profile_key(directory: &Path) -> Vec<u16> {
    let directory = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_owned());
    let mut hash = DefaultHasher::new();
    directory.hash(&mut hash);
    format!("Try.Instance.{:016x}\0", hash.finish())
        .encode_utf16()
        .collect()
}

fn tray_windows() -> impl Iterator<Item = HWND> {
    let class: Vec<u16> = "SlintSystemTrayWindow\0".encode_utf16().collect();
    let mut previous = std::ptr::null_mut();
    std::iter::from_fn(move || {
        // SAFETY: enumerate message-only windows of the pinned Slint tray class.
        previous =
            unsafe { FindWindowExW(HWND_MESSAGE, previous, class.as_ptr(), std::ptr::null()) };
        (!previous.is_null()).then_some(previous)
    })
}

impl Instance {
    pub fn forward(directory: &Path, practice: bool) -> AppResult<bool> {
        let profile = profile_key(directory);
        for window in tray_windows() {
            // SAFETY: the profile property advertises our live, registered handler.
            unsafe {
                if GetPropW(window, profile.as_ptr()).is_null() {
                    continue;
                }
                let mut owner = 0;
                GetWindowThreadProcessId(window, &mut owner);
                // Hand activation permission from this user-launched process to the
                // existing process, so opening from Explorer brings Try to the front.
                AllowSetForegroundWindow(owner);
                if PostMessageW(window, OPEN_REQUEST, usize::from(practice), 0) == 0 {
                    return Err(tr!(
                        "无法打开正在运行的 Try：{p0}",
                        p0 = std::io::Error::last_os_error()
                    ));
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn listen(directory: &Path, callback: Rc<dyn Fn(bool)>) -> AppResult<Self> {
        let window = tray_windows()
            .find(|window| {
                let mut owner = 0;
                // SAFETY: only select this process's existing tray window.
                unsafe { GetWindowThreadProcessId(*window, &mut owner) };
                owner == std::process::id()
            })
            .ok_or(tr!("无法连接 Try 的托盘启动入口。"))?;
        let handler = Box::new(Handler {
            callback,
            profile: profile_key(directory),
            attached: Cell::new(false),
        });
        let instance = Self { window, handler };
        // SAFETY: Box keeps Handler stable. The guard detaches before freeing it;
        // all callbacks and teardown run on the same Slint UI thread.
        unsafe {
            if SetWindowSubclass(
                window,
                Some(request),
                SUBCLASS_ID,
                instance.handler.as_ref() as *const Handler as usize,
            ) == 0
            {
                return Err(tr!("无法注册 Try 的启动入口。").into());
            }
            instance.handler.attached.set(true);
            if SetPropW(window, instance.handler.profile.as_ptr(), 1usize as _) == 0 {
                return Err(tr!("无法登记 Try 的本地数据实例。").into());
            }
        }
        Ok(instance)
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        if self.handler.attached.replace(false) {
            // SAFETY: still attached to the live tray window; remove our callback
            // and readiness property before Box<Handler> is released.
            unsafe {
                RemovePropW(self.window, self.handler.profile.as_ptr());
                RemoveWindowSubclass(self.window, Some(request), SUBCLASS_ID);
            }
        }
    }
}

unsafe extern "system" fn request(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass_id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: the Instance guard owns this pointer throughout attachment.
    let handler = unsafe { &*(data as *const Handler) };
    if message == OPEN_REQUEST && wparam <= 1 {
        let callback = handler.callback.clone();
        // Defer until native message dispatch has returned and UI borrows end.
        Timer::single_shot(Duration::ZERO, move || callback(wparam == 1));
        return 0;
    }
    if message == WM_NCDESTROY {
        handler.attached.set(false);
        // SAFETY: stop borrowing the handler if Slint destroys its tray first.
        unsafe {
            RemovePropW(window, handler.profile.as_ptr());
            RemoveWindowSubclass(window, Some(request), subclass_id);
        }
    }
    // SAFETY: preserve all ordinary Slint tray messages and native teardown.
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}
