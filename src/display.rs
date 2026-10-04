//! Display backends. The `brightness` crate already handles both internal panels (IOCTL)
//! and external monitors (DDC/CI, normalised to 0-100). Devices are enumerated ONCE and
//! cached; re-enumerate only on resume or monitor hot-plug (see engine.rs).

use std::time::{Duration, Instant};

use brightness_sys::blocking::windows::BrightnessExt;
use brightness_sys::blocking::{Brightness, BrightnessDevice, brightness_devices};
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes,
    QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::ERROR_SUCCESS;

struct Display {
    dev: BrightnessDevice,
    name: String,
    /// Laptop panel (cheap IOCTL writes) vs external monitor (slow DDC/CI writes that may
    /// wear the monitor's NVRAM, so they are rate-limited).
    internal: bool,
    /// Last value we asked for (used to skip redundant writes).
    last_requested: Option<u32>,
    /// Last value the hardware actually has (used for manual-change detection).
    last_actual: Option<u32>,
    last_write_at: Option<Instant>,
    errors: u32,
}

pub struct Displays {
    list: Vec<Display>,
}

impl Displays {
    pub fn enumerate() -> Self {
        let internal_paths = internal_device_paths();
        let mut list = Vec::new();
        for dev in brightness_devices() {
            let dev = match dev {
                Ok(dev) => dev,
                Err(e) => {
                    log!("Skipping display (enumeration error): {e}");
                    continue;
                }
            };
            let name = dev.device_description().unwrap_or_default();
            let path = dev.device_path().unwrap_or_default();
            let internal = internal_paths.iter().any(|p| p.eq_ignore_ascii_case(&path));
            match dev.get() {
                Ok(current) => {
                    log!("Found display '{name}' internal={internal} brightness={current}%");
                    list.push(Display {
                        dev,
                        name,
                        internal,
                        last_requested: Some(current),
                        last_actual: Some(current),
                        last_write_at: None,
                        errors: 0,
                    });
                }
                // Monitor without DDC/CI (or DDC/CI disabled in its OSD): don't keep poking it.
                Err(e) => log!("Skipping display '{name}' (brightness not readable): {e}"),
            }
        }
        Self { list }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// The display used for manual-change detection: first internal panel, else the first one.
    fn reference_index(&self) -> Option<usize> {
        self.list
            .iter()
            .position(|d| d.internal)
            .or(if self.list.is_empty() { None } else { Some(0) })
    }

    pub fn reference_is_internal(&self) -> bool {
        self.reference_index().is_some_and(|i| self.list[i].internal)
    }

    pub fn initial_brightness(&self) -> Option<u32> {
        self.reference_index().and_then(|i| self.list[i].last_actual)
    }

    /// Returns `Some(actual)` if the user changed brightness outside this app
    /// (Windows slider, Fn keys, monitor OSD buttons).
    pub fn detect_manual_change(&mut self) -> Option<u32> {
        let i = self.reference_index()?;
        let d = &mut self.list[i];
        // External monitors can take a moment to report a freshly written value.
        if !d.internal && d.last_write_at.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
            return None;
        }
        let actual = d.dev.get().ok()?;
        let expected = d.last_actual?;
        if actual.abs_diff(expected) > 2 {
            d.last_actual = Some(actual);
            d.last_requested = Some(actual);
            Some(actual)
        } else {
            None
        }
    }

    /// Write `value` (0-100) to every display. Internal panels are written immediately
    /// (smooth fades); external monitors at most once per `external_min_interval`.
    pub fn apply(&mut self, value: u32, external_min_interval: Duration) {
        let now = Instant::now();
        for d in &mut self.list {
            if d.last_requested == Some(value) {
                continue;
            }
            let min_interval = if d.internal {
                Duration::ZERO
            } else {
                external_min_interval
            };
            if d.last_write_at.is_some_and(|t| now - t < min_interval) {
                continue;
            }
            d.last_write_at = Some(now);
            match d.dev.set(value) {
                Ok(()) => {
                    d.last_requested = Some(value);
                    // Internal panels snap to the nearest supported level; read it back
                    // (cheap IOCTL) so manual-change detection compares against reality.
                    d.last_actual = Some(if d.internal {
                        d.dev.get().unwrap_or(value)
                    } else {
                        value
                    });
                    d.errors = 0;
                }
                Err(e) => {
                    d.errors += 1;
                    if d.errors == 1 {
                        log!("Failed to set brightness on '{}': {e}", d.name);
                    }
                }
            }
        }
    }
}

/// Device paths of laptop panels, from the Windows display configuration.
/// Matches `BrightnessExt::device_path()` of the corresponding brightness device.
fn internal_device_paths() -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut num_paths = 0u32;
        let mut num_modes = 0u32;
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut num_paths, &mut num_modes)
            != ERROR_SUCCESS
        {
            return out;
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); num_paths as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); num_modes as usize];
        if QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut num_paths,
            paths.as_mut_ptr(),
            &mut num_modes,
            modes.as_mut_ptr(),
            None,
        ) != ERROR_SUCCESS
        {
            return out;
        }
        paths.truncate(num_paths as usize);

        for path in &paths {
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
            target.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
            target.header.adapterId = path.targetInfo.adapterId;
            target.header.id = path.targetInfo.id;
            target.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
            if DisplayConfigGetDeviceInfo(&mut target.header) != 0 {
                continue;
            }
            let tech = target.outputTechnology;
            if tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
                || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
                || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED
            {
                let raw = &target.monitorDevicePath;
                let len = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
                out.push(String::from_utf16_lossy(&raw[..len]));
            }
        }
    }
    out
}
