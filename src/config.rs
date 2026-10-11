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
    /// Set on load when `[curve]` differs from the preset's; the tray shows no preset ticked.
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
    /// Minimum milliseconds between DDC/CI writes to an external monitor (at least 500).
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
/// broken file is kept as config.toml.bad (so a later save cannot destroy it) and
/// defaults are used.
pub fn load() -> Config {
    let path = path();
    match fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(mut cfg) => {
                repair(&mut cfg);
                cfg
            }
            Err(e) => {
                let _ = fs::rename(&path, path.with_extension("toml.bad"));
                log!("config.toml is invalid (kept as config.toml.bad), using defaults: {e}");
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

/// Fixes what a hand edit can break: NaN bounds crash the engine, hours past 23 silently
/// disable night mode, a tiny interval removes the monitor-wear protection.
fn repair(cfg: &mut Config) {
    let d = Config::default();
    for (v, default) in [
        (&mut cfg.min_brightness, d.min_brightness),
        (&mut cfg.max_brightness, d.max_brightness),
        (&mut cfg.offset, d.offset),
        (&mut cfg.night.max_brightness, d.night.max_brightness),
        (&mut cfg.ambient.strength, d.ambient.strength),
        (&mut cfg.curve.dark_luminance, d.curve.dark_luminance),
        (&mut cfg.curve.dark_brightness, d.curve.dark_brightness),
        (&mut cfg.curve.light_luminance, d.curve.light_luminance),
        (&mut cfg.curve.light_brightness, d.curve.light_brightness),
    ] {
        if !v.is_finite() {
            *v = default;
        }
    }
    cfg.night.start_hour %= 24;
    cfg.night.end_hour %= 24;
    cfg.external_min_write_interval_ms = cfg.external_min_write_interval_ms.max(500);
    if cfg.preset.curve().is_some_and(|c| c != cfg.curve) {
        cfg.preset = Preset::Custom;
    }
}

/// Re-reads the file, applies `change` and saves, so hand edits not yet reloaded survive.
pub fn update(change: impl FnOnce(&mut Config)) -> Config {
    let mut cfg = load();
    change(&mut cfg);
    save(&cfg);
    cfg
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_passes_through_both_points() {
        let c = Curve::default();
        assert_eq!(c.eval(c.dark_luminance), c.dark_brightness);
        assert!((c.eval(c.light_luminance) - c.light_brightness).abs() < 1e-4);
    }

    #[test]
    fn partial_hand_edited_file_is_repaired() {
        let mut cfg: Config = toml::from_str(
            "max_brightness = nan\nexternal_min_write_interval_ms = 0\n\
             [night]\nstart_hour = 25\n[curve]\ndark_brightness = 90.0\n",
        )
        .unwrap();
        repair(&mut cfg);
        assert_eq!(cfg.max_brightness, 100.0);
        assert_eq!(cfg.external_min_write_interval_ms, 500);
        assert_eq!(cfg.night.start_hour, 1);
        assert_eq!(cfg.night.end_hour, 7); // missing keys keep their defaults
        assert_eq!(cfg.excluded_apps, Config::default().excluded_apps);
        assert_eq!(cfg.preset, Preset::Custom);
    }

    #[test]
    fn preset_curve_saved_and_reloaded_stays_that_preset() {
        let mut cfg: Config = toml::from_str(&toml::to_string(&Config::default()).unwrap()).unwrap();
        repair(&mut cfg);
        assert_eq!(cfg.preset, Preset::Normal);
    }
}
