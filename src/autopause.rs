//! Automatic pause rules: lock screen, excluded apps, full-screen apps.

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use windows::core::PWSTR;

use crate::config::Config;

/// Why adjustment should be paused right now, or None to keep running.
pub fn reason(cfg: &Config) -> Option<String> {
    if let Some(exe) = foreground_exe() {
        if exe.eq_ignore_ascii_case("LockApp.exe") {
            return Some("lock screen".into());
        }
        if cfg.excluded_apps.iter().any(|a| a.eq_ignore_ascii_case(&exe)) {
            return Some(exe);
        }
    }
    if cfg.pause_in_fullscreen {
        if let Ok(state) = unsafe { SHQueryUserNotificationState() } {
            if state == QUNS_BUSY
                || state == QUNS_RUNNING_D3D_FULL_SCREEN
                || state == QUNS_PRESENTATION_MODE
            {
                return Some("full-screen app".into());
            }
        }
    }
    None
}

/// File name (e.g. "chrome.exe") of the process owning the foreground window.
fn foreground_exe() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return None;
        }
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        full.rsplit('\\').next().map(str::to_string)
    }
}
