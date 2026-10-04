//! Optional ambient light sensor (many laptops have one; desktops usually don't).

use std::time::{Duration, Instant};

use windows::Devices::Sensors::LightSensor;

pub struct AmbientLight {
    sensor: Option<LightSensor>,
    last_read: Option<Instant>,
    lux: Option<f32>,
}

impl AmbientLight {
    pub fn new() -> Self {
        let sensor = LightSensor::GetDefault().ok();
        log!("Ambient light sensor present: {}", sensor.is_some());
        Self { sensor, last_read: None, lux: None }
    }

    /// Brightness points to add for the current room light. 200 lux (typical office) = 0,
    /// each 10x darker/brighter = -/+ `strength`. Clamped to +/-25. Reads at most every 2 s.
    pub fn shift(&mut self, strength: f32) -> f32 {
        let Some(sensor) = &self.sensor else {
            return 0.0;
        };
        if self.last_read.is_none_or(|t| t.elapsed() >= Duration::from_secs(2)) {
            self.last_read = Some(Instant::now());
            self.lux = sensor
                .GetCurrentReading()
                .and_then(|r| r.IlluminanceInLux())
                .ok();
        }
        match self.lux {
            Some(lux) => ((lux.max(1.0).log10() - 200f32.log10()) * strength).clamp(-25.0, 25.0),
            None => 0.0,
        }
    }
}
