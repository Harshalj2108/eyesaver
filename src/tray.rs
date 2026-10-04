//! Tray icon artwork and menu items.

use tray_icon::Icon;
use tray_icon::menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};

use crate::config::Preset;

pub struct TrayMenu {
    pub menu: Menu,
    pub status: MenuItem,
    pub brighter: MenuItem,
    pub dimmer: MenuItem,
    pub subtle: CheckMenuItem,
    pub normal: CheckMenuItem,
    pub strong: CheckMenuItem,
    pub pause_toggle: MenuItem,
    pub pause_30: MenuItem,
    pub pause_60: MenuItem,
    pub reset: MenuItem,
    pub startup: CheckMenuItem,
    pub open_config: MenuItem,
    pub reload_config: MenuItem,
    pub quit: MenuItem,
}

impl TrayMenu {
    pub fn new(startup_enabled: bool, preset: Preset) -> Self {
        let menu = Menu::new();
        let status = MenuItem::new("Starting...", false, None);
        let brighter = MenuItem::new("Brighter  (Ctrl+Alt+Up)", true, None);
        let dimmer = MenuItem::new("Dimmer  (Ctrl+Alt+Down)", true, None);

        let strength = Submenu::new("Strength", true);
        let subtle = CheckMenuItem::new("Subtle", true, preset == Preset::Subtle, None);
        let normal = CheckMenuItem::new("Normal", true, preset == Preset::Normal, None);
        let strong = CheckMenuItem::new("Strong", true, preset == Preset::Strong, None);
        let _ = strength.append(&subtle);
        let _ = strength.append(&normal);
        let _ = strength.append(&strong);

        let pause_toggle = MenuItem::new("Pause  (Ctrl+Alt+P)", true, None);
        let pause_for = Submenu::new("Pause for", true);
        let pause_30 = MenuItem::new("30 minutes", true, None);
        let pause_60 = MenuItem::new("1 hour", true, None);
        let _ = pause_for.append(&pause_30);
        let _ = pause_for.append(&pause_60);

        let reset = MenuItem::new("Reset calibration", true, None);
        let startup = CheckMenuItem::new("Start with Windows", true, startup_enabled, None);
        let open_config = MenuItem::new("Open settings file", true, None);
        let reload_config = MenuItem::new("Reload settings", true, None);
        let quit = MenuItem::new("Quit", true, None);

        let _ = menu.append(&status);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&brighter);
        let _ = menu.append(&dimmer);
        let _ = menu.append(&strength);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&pause_toggle);
        let _ = menu.append(&pause_for);
        let _ = menu.append(&reset);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&startup);
        let _ = menu.append(&open_config);
        let _ = menu.append(&reload_config);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit);

        Self {
            menu,
            status,
            brighter,
            dimmer,
            subtle,
            normal,
            strong,
            pause_toggle,
            pause_30,
            pause_60,
            reset,
            startup,
            open_config,
            reload_config,
            quit,
        }
    }

    pub fn preset_for(&self, id: &MenuId) -> Option<Preset> {
        if id == self.subtle.id() {
            Some(Preset::Subtle)
        } else if id == self.normal.id() {
            Some(Preset::Normal)
        } else if id == self.strong.id() {
            Some(Preset::Strong)
        } else {
            None
        }
    }

    /// Radio-button behaviour: tick exactly the active preset (none for Custom).
    pub fn show_preset(&self, preset: Preset) {
        self.subtle.set_checked(preset == Preset::Subtle);
        self.normal.set_checked(preset == Preset::Normal);
        self.strong.set_checked(preset == Preset::Strong);
    }
}

/// 32x32 sun icon drawn in code (no asset files). Amber when active, grey when paused.
pub fn sun_icon(active: bool) -> Icon {
    const SIZE: usize = 32;
    let color: [u8; 4] = if active {
        [0xFF, 0xB3, 0x00, 0xFF]
    } else {
        [0x9E, 0x9E, 0x9E, 0xFF]
    };
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - 16.0;
            let dy = y as f32 + 0.5 - 16.0;
            let dist = (dx * dx + dy * dy).sqrt();
            // 8 rays: fractional position around the circle, centred on each ray.
            let turn = (dy.atan2(dx) / std::f32::consts::TAU * 8.0 + 0.15).rem_euclid(1.0);
            let on = dist <= 8.0 || ((11.0..=15.0).contains(&dist) && turn < 0.3);
            if on {
                let i = (y * SIZE + x) * 4;
                rgba[i..i + 4].copy_from_slice(&color);
            }
        }
    }
    Icon::from_rgba(rgba, SIZE as u32, SIZE as u32).expect("valid icon buffer")
}
