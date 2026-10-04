//! Tiny file logger. The app runs without a console, so `println!` output is lost.
//! Log file: %LOCALAPPDATA%\Eyesaver\eyesaver.log (rotated to eyesaver.log.old at 1 MB).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// %LOCALAPPDATA%\Eyesaver, created on demand.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("Eyesaver");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn write(msg: &str) {
    let path = data_dir().join("eyesaver.log");
    if fs::metadata(&path).map(|m| m.len() > 1_000_000).unwrap_or(false) {
        let _ = fs::rename(&path, path.with_extension("log.old"));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        let _ = writeln!(
            file,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {}",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, msg
        );
    }
}

/// Usage: `log!("value is {}", x);` — available in every module declared after `mod log;`.
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}
