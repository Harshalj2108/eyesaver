//! Windows integration: single-instance guard and "Start with Windows" registry entry.

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_OK, MessageBoxW};
use windows::core::w;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_KEY: &str = r"Software\Eyesaver";
const RUN_VALUE: &str = "Eyesaver";

/// Returns false if another Eyesaver is already running (after telling the user).
/// The mutex handle is intentionally never closed; Windows releases it at process exit.
pub fn ensure_single_instance() -> bool {
    unsafe {
        let created = CreateMutexW(None, false, w!("Local\\EyesaverSingleInstance"));
        if created.is_ok() && GetLastError() == ERROR_ALREADY_EXISTS {
            MessageBoxW(
                HWND::default(),
                w!("Eyesaver is already running. Look for its icon in the system tray."),
                w!("Eyesaver"),
                MB_OK | MB_ICONINFORMATION,
            );
            return false;
        }
    }
    true
}

pub fn startup_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(RUN_VALUE))
        .is_ok()
}

pub fn set_startup(enabled: bool) {
    let Ok(run) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_WRITE)
    else {
        return;
    };
    if enabled {
        if let Ok(exe) = std::env::current_exe() {
            // Quoted, because the path may contain spaces.
            let _ = run.set_value(RUN_VALUE, &format!("\"{}\"", exe.display()));
        }
    } else {
        let _ = run.delete_value(RUN_VALUE);
    }
}

/// First run ever: enable startup (keeps the old default). Later runs: if still enabled,
/// refresh the path in case the exe moved. Never re-enables after the user turned it off.
pub fn init_startup() {
    let Ok((app, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(APP_KEY) else {
        return;
    };
    let first_run = app.get_value::<u32, _>("Initialized").is_err();
    if first_run {
        let _ = app.set_value("Initialized", &1u32);
        set_startup(true);
    } else if startup_enabled() {
        set_startup(true);
    }
}
