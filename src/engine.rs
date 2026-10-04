//! Background brightness loop (plain std thread; no async runtime needed).

use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CMONITORS};

use crate::ambient::AmbientLight;
use crate::config::{self, Config, Night};
use crate::display::Displays;
use crate::{autopause, capture};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Pause {
    #[default]
    Running,
    Until(Instant),
    Indefinite,
}

/// State shared between the tray (main thread) and the engine thread.
#[derive(Default)]
pub struct Shared {
    /// Source of truth for settings, including the learned `offset`.
    pub config: Config,
    pub pause: Pause,
    // --- written by the engine, read by the tray ---
    pub luminance: f32,
    pub brightness: u32,
    pub display_count: usize,
    pub auto_pause: Option<String>,
    pub night_cap: bool,
}

impl Shared {
    pub fn is_paused(&self) -> bool {
        self.pause != Pause::Running
    }

    pub fn toggle_pause(&mut self) {
        self.pause = if self.is_paused() { Pause::Running } else { Pause::Indefinite };
    }

    /// Shift the learned offset (tray Brighter/Dimmer, hotkeys) and persist it.
    pub fn nudge(&mut self, delta: f32) {
        self.config.offset = (self.config.offset + delta).clamp(-100.0, 100.0);
        config::save(&self.config);
    }

    pub fn reset_calibration(&mut self) {
        self.config.offset = 0.0;
        config::save(&self.config);
        log!("Calibration reset");
    }
}

/// Lock that survives a poisoned mutex (a panic elsewhere must not kill the tray).
pub fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// Tick while a fade is in progress.
const ACTIVE_TICK: Duration = Duration::from_millis(50);
/// Tick when brightness is stable (saves CPU/GPU readbacks).
const IDLE_TICK: Duration = Duration::from_millis(150);
/// Weight of the old value per tick when getting darker (lower = faster).
/// Dimming is fast so a white page does not blind you.
const SMOOTHING_DIM: f32 = 0.55;
/// Weight of the old value per tick when getting brighter (slower, less noticeable).
const SMOOTHING_BRIGHTEN: f32 = 0.75;
/// Ignore target changes smaller than this (percentage points) to avoid flicker.
const HYSTERESIS: f32 = 2.0;

fn night_active(night: &Night) -> bool {
    if !night.enabled || night.start_hour == night.end_hour {
        return false;
    }
    let hour = unsafe { GetLocalTime() }.wHour as u8;
    if night.start_hour < night.end_hour {
        hour >= night.start_hour && hour < night.end_hour
    } else {
        hour >= night.start_hour || hour < night.end_hour
    }
}

pub fn run(shared: Arc<Mutex<Shared>>) {
    let mut displays = Displays::enumerate();
    lock(&shared).display_count = displays.len();
    let mut monitor_count = unsafe { GetSystemMetrics(SM_CMONITORS) };

    let mut current = displays.initial_brightness().unwrap_or(50) as f32;
    let mut target = current;
    let mut ambient: Option<AmbientLight> = None;
    let mut auto_pause: Option<String> = None;
    let mut last_auto_check: Option<Instant> = None;
    let mut last_override_check = Instant::now();
    let mut last_tick = Instant::now();

    loop {
        let now = Instant::now();
        let gap = now - last_tick;
        last_tick = now;

        let (paused, cfg) = {
            let mut s = lock(&shared);
            if let Pause::Until(until) = s.pause {
                if now >= until {
                    s.pause = Pause::Running;
                    log!("Timed pause ended");
                }
            }
            (s.is_paused(), s.config.clone())
        };
        if paused {
            thread::sleep(Duration::from_millis(500));
            continue;
        }

        // Automatic pause rules (cheap, but no need to check every tick).
        if last_auto_check.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
            last_auto_check = Some(now);
            let reason = autopause::reason(&cfg);
            if reason != auto_pause {
                log!("Auto-pause: {}", reason.as_deref().unwrap_or("off"));
                auto_pause = reason.clone();
                lock(&shared).auto_pause = reason;
            }
        }
        if auto_pause.is_some() {
            thread::sleep(Duration::from_millis(500));
            continue;
        }

        // A long gap between ticks means the PC was asleep: cached monitor handles may be
        // stale. Also re-enumerate when a monitor is plugged in or removed.
        let count = unsafe { GetSystemMetrics(SM_CMONITORS) };
        if gap > Duration::from_secs(5) || count != monitor_count {
            log!("Resume or display change detected (gap {gap:?}, monitors {monitor_count} -> {count})");
            displays = Displays::enumerate();
            monitor_count = count;
            lock(&shared).display_count = displays.len();
            if let Some(b) = displays.initial_brightness() {
                current = b as f32;
                target = current;
            }
        }

        // Manual override: DDC/CI reads are slow (40-100 ms), so check external monitors less often.
        let mut offset = cfg.offset;
        let check_every = if displays.reference_is_internal() {
            Duration::from_millis(500)
        } else {
            Duration::from_secs(3)
        };
        if last_override_check.elapsed() >= check_every {
            last_override_check = Instant::now();
            if let Some(actual) = displays.detect_manual_change() {
                offset += actual as f32 - current;
                current = actual as f32;
                target = current;
                let mut s = lock(&shared);
                s.config.offset = offset;
                config::save(&s.config);
                log!("Manual change to {actual}% detected; offset now {offset:+.1}");
            }
        }

        let mut interval = IDLE_TICK;
        if let Some(lum) = capture::primary_luminance() {
            let shift = if cfg.ambient.enabled {
                ambient
                    .get_or_insert_with(AmbientLight::new)
                    .shift(cfg.ambient.strength)
            } else {
                0.0
            };
            let night = night_active(&cfg.night);
            let mut max = cfg.max_brightness.clamp(0.0, 100.0);
            if night {
                max = max.min(cfg.night.max_brightness.max(0.0));
            }
            let min = cfg.min_brightness.clamp(0.0, max);

            let desired = (cfg.curve.eval(lum) + offset + shift).clamp(min, max);
            if (desired - target).abs() > HYSTERESIS {
                target = desired;
            }
            let smoothing = if target < current { SMOOTHING_DIM } else { SMOOTHING_BRIGHTEN };
            current = current * smoothing + target * (1.0 - smoothing);
            if (current - target).abs() < 0.5 {
                current = target;
            } else {
                interval = ACTIVE_TICK;
            }
            let out = current.round() as u32;
            displays.apply(out, Duration::from_millis(cfg.external_min_write_interval_ms));

            let mut s = lock(&shared);
            s.luminance = lum;
            s.brightness = out;
            s.night_cap = night;
        }
        thread::sleep(interval);
    }
}
