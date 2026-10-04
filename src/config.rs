//! User settings, stored as TOML at %LOCALAPPDATA%\Eyesaver\config.toml.
//! Missing keys fall back to defaults, so old config files keep working.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Subtle,
    #[default]
    Normal,
    Strong,
    /// Set when the user hand-edits `[curve]`; the tray shows no preset ticked.
    Custom,
}

impl Preset {
    pub fn curve(self) -> Option<Curve> {
        let (dark_brightness, light_brightness) = match self {
            Preset::Subtle => (70.0, 62.0),
            Preset::Normal => (75.0, 60.0),
            Preset::Strong => (85.0, 45.0),
            Preset::Custom => return None,
        };
        Some(Curve {
            dark_luminance: 0.12,
            dark_brightness,
            light_luminance: 0.86,
            light_brightness,
        })
    }
}

/// Linear mapping through two points: (dark_luminance -> dark_brightness) and
/// (light_luminance -> light_brightness). Luminance is 0.0-1.0, brightness 0-100.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Curve {
    pub dark_luminance: f32,
    pub dark_brightness: f32,
    pub light_luminance: f32,
    pub light_brightness: f32,
}

impl Default for Curve {
    fn default() -> Self {
        Preset::Normal.curve().unwrap()
    }
}

impl Curve {
    pub fn eval(&self, lum: f32) -> f32 {
        let span = self.light_luminance - self.dark_luminance;
        if span.abs() < f32::EPSILON {
            return self.dark_brightness;
        }
        self.dark_brightness
            + (lum - self.dark_luminance) * (self.light_brightness - self.dark_brightness) / span
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Night {
    pub enabled: bool,
    /// Local hour (0-23) when the cap starts.
    pub start_hour: u8,
    /// Local hour (0-23) when the cap ends. May be smaller than start_hour (wraps midnight).
    pub end_hour: u8,
    pub max_brightness: f32,
}

impl Default for Night {
    fn default() -> Self {
        Self { enabled: false, start_hour: 21, end_hour: 7, max_brightness: 50.0 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Ambient {
    /// Use the laptop's ambient light sensor, if it has one.
    pub enabled: bool,
    /// Brightness points added per 10x change in room light (200 lux = no change).
    pub strength: f32,
}

impl Default for Ambient {
    fn default() -> Self {
        Self { enabled: false, strength: 15.0 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub preset: Preset,
    pub curve: Curve,
    pub min_brightness: f32,
    pub max_brightness: f32,
    /// Learned from your manual brightness changes. "Reset calibration" sets it to 0.
    pub offset: f32,
    /// Minimum milliseconds between DDC/CI writes to an external monitor.
    pub external_min_write_interval_ms: u64,
    /// Stop adjusting while a full-screen app (game, video, presentation) is in front.
    pub pause_in_fullscreen: bool,
    /// Executable names (e.g. "photoshop.exe") that pause adjustment while in front.
    pub excluded_apps: Vec<String>,
    /// Global hotkeys Ctrl+Alt+Up/Down/P. Needs an app restart to take effect.
    pub hotkeys: bool,
    pub night: Night,
    pub ambient: Ambient,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            preset: Preset::Normal,
            curve: Curve::default(),
            min_brightness: 0.0,
            max_brightness: 100.0,
            offset: 0.0,
            external_min_write_interval_ms: 1000,
            pause_in_fullscreen: true,
            excluded_apps: vec![
                "Photoshop.exe".into(),
                "Lightroom.exe".into(),
                "Resolve.exe".into(),
                "vlc.exe".into(),
            ],
            hotkeys: true,
            night: Night::default(),
            ambient: Ambient::default(),
        }
    }
}

pub fn path() -> PathBuf {
    crate::log::data_dir().join("config.toml")
}

/// Loads the config. Creates the file with defaults if missing. On a parse error the
/// broken file is left untouched (so the user can fix it) and defaults are used.
pub fn load() -> Config {
    let path = path();
    match fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(cfg) => cfg,
            Err(e) => {
                log!("config.toml is invalid, using defaults: {e}");
                Config::default()
            }
        },
        Err(_) => {
            let cfg = Config::default();
            save(&cfg);
            cfg
        }
    }
}

pub fn save(cfg: &Config) {
    match toml::to_string_pretty(cfg) {
        Ok(text) => {
            if let Err(e) = fs::write(path(), text) {
                log!("Failed to save config: {e}");
            }
        }
        Err(e) => log!("Failed to serialise config: {e}"),
    }
}
