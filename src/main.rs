#![windows_subsystem = "windows"]

#[macro_use]
mod log; // must stay first so `log!` is visible in the modules below
mod ambient;
mod autopause;
mod capture;
mod config;
mod display;
mod engine;
mod hotkeys;
mod system;
mod tray;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::TrayIconBuilder;
use tray_icon::menu::MenuEvent;

use engine::{Pause, Shared, lock};
use hotkeys::Action;

/// Brightness points per Brighter/Dimmer step.
const NUDGE_STEP: f32 = 5.0;

fn main() {
    if !system::ensure_single_instance() {
        return;
    }
    log!("Eyesaver {} starting", env!("CARGO_PKG_VERSION"));
    system::init_startup();
    let config = config::load();
    let hotkeys_enabled = config.hotkeys;
    let initial_preset = config.preset;

    // Build the event loop BEFORE starting the engine: tao makes the process per-monitor
    // DPI aware here, and the screen capture needs real (unscaled) screen dimensions.
    let event_loop = EventLoopBuilder::new().build();

    let shared = Arc::new(Mutex::new(Shared { config, ..Default::default() }));
    {
        let shared = shared.clone();
        std::thread::spawn(move || engine::run(shared));
    }

    let menu = tray::TrayMenu::new(system::startup_enabled(), initial_preset);
    let mut tray_icon = Some(
        TrayIconBuilder::new()
            .with_menu(Box::new(menu.menu.clone()))
            .with_tooltip("Eyesaver")
            .with_icon(tray::sun_icon(true))
            .build()
            .expect("failed to create tray icon"),
    );
    let hotkeys = hotkeys::Hotkeys::new(hotkeys_enabled);

    let menu_rx = MenuEvent::receiver();
    let mut icon_active = true;

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(250));

        while let Some(action) = hotkeys.try_next() {
            let mut s = lock(&shared);
            match action {
                Action::Brighter => s.nudge(NUDGE_STEP),
                Action::Dimmer => s.nudge(-NUDGE_STEP),
                Action::TogglePause => s.toggle_pause(),
            }
        }

        while let Ok(event) = menu_rx.try_recv() {
            let id = event.id;
            let mut s = lock(&shared);
            if id == menu.quit.id() {
                log!("Quit requested");
                tray_icon.take();
                *control_flow = ControlFlow::Exit;
                return;
            } else if id == menu.brighter.id() {
                s.nudge(NUDGE_STEP);
            } else if id == menu.dimmer.id() {
                s.nudge(-NUDGE_STEP);
            } else if id == menu.pause_toggle.id() {
                s.toggle_pause();
            } else if id == menu.pause_30.id() {
                s.pause = Pause::Until(Instant::now() + Duration::from_secs(30 * 60));
            } else if id == menu.pause_60.id() {
                s.pause = Pause::Until(Instant::now() + Duration::from_secs(60 * 60));
            } else if id == menu.reset.id() {
                s.reset_calibration();
            } else if let Some(preset) = menu.preset_for(&id) {
                s.config.preset = preset;
                if let Some(curve) = preset.curve() {
                    s.config.curve = curve;
                }
                config::save(&s.config);
                menu.show_preset(preset);
                log!("Preset changed to {preset:?}");
            } else if id == menu.startup.id() {
                // Derive the new state from the registry, not from the menu's checkbox,
                // so it is correct whether or not the menu toggled itself.
                let enable = !system::startup_enabled();
                system::set_startup(enable);
                menu.startup.set_checked(enable);
            } else if id == menu.open_config.id() {
                let _ = std::process::Command::new("notepad.exe")
                    .arg(config::path())
                    .spawn();
            } else if id == menu.reload_config.id() {
                s.config = config::load();
                menu.show_preset(s.config.preset);
                log!("Settings reloaded");
            }
        }

        let s = lock(&shared);
        let text = match s.pause {
            Pause::Indefinite => "Paused".to_string(),
            Pause::Until(until) => {
                let left = until.saturating_duration_since(Instant::now()).as_secs();
                format!("Paused ({} min left)", left.div_ceil(60))
            }
            Pause::Running => {
                if let Some(reason) = &s.auto_pause {
                    format!("Auto-paused: {reason}")
                } else if s.display_count == 0 {
                    "No controllable display found".to_string()
                } else {
                    format!(
                        "Luminance {:.2} | Brightness {}%{}",
                        s.luminance,
                        s.brightness,
                        if s.night_cap { " | night cap" } else { "" }
                    )
                }
            }
        };
        menu.status.set_text(&text);
        menu.pause_toggle.set_text(if s.is_paused() {
            "Resume  (Ctrl+Alt+P)"
        } else {
            "Pause  (Ctrl+Alt+P)"
        });

        let active = !s.is_paused() && s.auto_pause.is_none();
        if let Some(icon) = &tray_icon {
            let _ = icon.set_tooltip(Some(format!("Eyesaver - {text}")));
            if icon_active != active {
                icon_active = active;
                let _ = icon.set_icon(Some(tray::sun_icon(active)));
            }
        }
    });
}
