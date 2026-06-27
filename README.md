# Eyesaver

Eyesaver is a lightweight, high-performance Windows utility written in Rust that dynamically adjusts your monitor's brightness based on the current on-screen content. By analyzing screen luminance in real-time, the application ensures optimal viewing comfort without requiring manual intervention.

## Key Features

* **Dynamic Brightness Scaling:** Continuously calculates the average luminance of the primary display and smoothly maps it to a target brightness level.
* **Universal Monitor Support:** Interacts directly with both internal laptop displays (via Windows Management Instrumentation) and external monitors (via DDC/CI).
* **Zero-Overhead Performance:** Achieves a near-zero CPU footprint (~0.00%) by utilizing highly optimized GDI hardware captures scaled to a 50x50 resolution, combined with efficient integer mathematics. Hardware API calls are strictly cached to prevent unnecessary driver wake-ups.
* **Native Windows Integration:** Operates entirely in the background as a native Windows subsystem process. It includes a System Tray interface for quick access and automatically registers itself to run on system startup.
* **Intelligent Manual Overrides:** Continuously monitors for user-initiated hardware brightness changes. If a manual adjustment is detected, the application calculates the differential and shifts its internal calibration curve to respect the user's new baseline.
* **Seamless Transitions:** Implements a 50ms (20 FPS) smoothing loop with hysteresis to provide fluid, flicker-free hardware fades.

## System Tray Interface

The application resides in the Windows System Tray and provides the following interactive features:
* **Live Status:** Displays real-time metrics for current screen luminance and target brightness percentage.
* **Pause/Resume:** Temporarily suspends automatic brightness adjustments.
* **Reset Calibration:** Clears any user-induced baseline offsets and restores the default luminance-to-brightness calibration curve.
* **Quit:** Gracefully terminates the application and background threads.

## Installation and Execution

### Prerequisites
* Windows operating system.
* Rust toolchain (cargo).

### Build Instructions
To compile the application, run the following command in the project root:

```bash
cargo build --release
```

### Usage
Run the compiled executable located in the release directory:

```bash
target\release\brightness.exe
```

Upon execution, the application will initialize, register itself in the Windows startup registry (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`), and place an icon in the System Tray.

## Architecture

* **Language:** Rust
* **Asynchronous Runtime:** `tokio` (dedicated thread for continuous hardware polling and transition loops).
* **UI Event Loop:** `tao` and `tray-icon` (running on the main thread for system tray integration).
* **Hardware Interfacing:** `windows-rs` for native Win32 GDI screen captures and DDC/CI commands, supplemented by the `brightness` crate for WMI interfacing.
