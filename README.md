# Eyesaver

Eyesaver is an ultra-lightweight, high-performance Windows utility written in Rust that dynamically adjusts your monitors' brightness based on real-time on-screen content luminance and ambient room conditions.

---

## Key Features

* **Real-Time Content-Aware Dimming:** Continuously analyzes the primary monitor's content luminance using an optimized Win32 GDI pipeline (downsampled to 50x50 with integer Rec.601 math in <1ms) to eliminate blinding white backgrounds while keeping dark content comfortable.
* **Hardware-Safe Multi-Monitor Architecture:** Differentiates between internal laptop panels (instant IOCTL writes) and external monitors (DDC/CI over I2C). External monitor writes are strictly rate-limited (max 1 write/sec) to protect monitor EEPROM/flash memory from wear.
* **Zero Runtime Overhead:** Completely eliminated the asynchronous Tokio runtime in favor of a lean native standard thread with adaptive tick rates (50ms during transitions, 150ms when quiescent).
* **Automatic Pause Rules:**
  * Pauses automatically when the Windows Lock Screen (`LockApp.exe`) is displayed.
  * Pauses during full-screen games, videos, or presentations (`SHQueryUserNotificationState`).
  * Pauses when color-critical or media applications are in focus (e.g. Photoshop, Lightroom, DaVinci Resolve, VLC).
* **Ambient Light Sensor Integration:** Automatically detects laptop ambient light sensors (`Windows.Devices.Sensors.LightSensor`) and applies smooth logarithmic lux adjustments.
* **Night Mode Cap:** Automatically enforces maximum brightness limits during scheduled evening/night hours without requiring external third-party tools.
* **Intelligent Baseline Learning:** Nudges made via hotkeys or the native Windows brightness slider adjust the persistent offset, learning your preference across content changes.
* **Per-Monitor V2 DPI Aware:** Tao event loop initialization ensures accurate unscaled coordinates across mixed-DPI multi-monitor setups.
* **Single Instance Enforcement:** Named Windows Kernel Mutex prevents duplicate background instances from racing.
* **Global Hotkeys:**
  * `Ctrl + Alt + Up`: Nudge brightness up (+5%)
  * `Ctrl + Alt + Down`: Nudge brightness down (-5%)
  * `Ctrl + Alt + P`: Toggle pause / resume

---

## System Tray Controls

The tray icon dynamically renders a 32x32 sun icon (amber when active, grey when paused):
* **Status:** Live readout of screen luminance, target brightness %, night cap status, or auto-pause reason.
* **Brighter / Dimmer:** Quick 5% calibration shifts.
* **Strength Presets:** Radio-style selection between **Subtle**, **Normal**, and **Strong** curves (or Custom if hand-edited).
* **Pause / Resume:** Toggle indefinitely or select timed pauses (**30 minutes**, **1 hour**).
* **Reset Calibration:** Re-centers your learned offset back to 0.
* **Start with Windows:** Toggle automatic startup entry in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
* **Open Settings File / Reload Settings:** Edit `%LOCALAPPDATA%\Eyesaver\config.toml` in Notepad and reload without restarting.

---

## Configuration (`config.toml`)

Stored at `%LOCALAPPDATA%\Eyesaver\config.toml`:

```toml
preset = "normal"
min_brightness = 0.0
max_brightness = 100.0
offset = 0.0
external_min_write_interval_ms = 1000
pause_in_fullscreen = true
excluded_apps = ["Photoshop.exe", "Lightroom.exe", "Resolve.exe", "vlc.exe"]
hotkeys = true

[curve]
dark_luminance = 0.12
dark_brightness = 75.0
light_luminance = 0.86
light_brightness = 60.0

[night]
enabled = false
start_hour = 21
end_hour = 7
max_brightness = 50.0

[ambient]
enabled = false
strength = 15.0
```

---

## Build & Run

### Prerequisites
* Windows 10 / 11
* Rust toolchain (stable)

### Compile Release Binary
```bash
cargo build --release
```

The resulting binary (`target\release\brightness.exe`) is stripped, LTO-optimized, and has a sub-5MB footprint with zero external DLL requirements.

