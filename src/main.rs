use std::thread;
use std::time::Duration;
use xcap::Monitor;

use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, SetMonitorBrightness, PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTOPRIMARY};

fn get_primary_monitor_average_luminance() -> Option<f32> {
    let monitors = Monitor::all().ok()?;
    let primary = monitors.into_iter().find(|m| m.is_primary())?;
    
    // Capture the image
    let image = primary.capture_image().ok()?;
    
    // Calculate average luminance
    // To make it fast, we can sample every 10th pixel
    let mut total_lum: f64 = 0.0;
    let mut count = 0;
    
    let width = image.width();
    let height = image.height();
    let step = 10;
    
    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            let pixel = image.get_pixel(x, y);
            let r = pixel[0] as f64;
            let g = pixel[1] as f64;
            let b = pixel[2] as f64;
            
            // Perceived luminance formula
            let lum = 0.299 * r + 0.587 * g + 0.114 * b;
            total_lum += lum;
            count += 1;
        }
    }
    
    if count == 0 {
        return None;
    }
    
    let avg_lum = total_lum / (count as f64);
    // Normalize to 0.0 - 1.0 (since max pixel value is 255)
    Some((avg_lum / 255.0) as f32)
}

fn set_monitor_brightness(target_brightness: u32) {
    unsafe {
        let pt = POINT { x: 0, y: 0 };
        let h_monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY);
        
        let mut num_physical: u32 = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(h_monitor, &mut num_physical).is_ok() {
            if num_physical > 0 {
                let mut physical_monitors = vec![PHYSICAL_MONITOR::default(); num_physical as usize];
                if GetPhysicalMonitorsFromHMONITOR(h_monitor, &mut physical_monitors).is_ok() {
                    for pm in &physical_monitors {
                        let _ = SetMonitorBrightness(pm.hPhysicalMonitor, target_brightness);
                    }
                    let _ = DestroyPhysicalMonitors(&physical_monitors);
                }
            }
        }
    }
}

fn main() {
    println!("Starting Eyesaver...");
    println!("Press Ctrl+C to exit.");
    
    // Smoothing factor: higher = smoother but slower transitions
    let smoothing_factor = 0.8;
    let mut current_brightness: f32 = 50.0; // Assume starting at 50%
    
    loop {
        let start = std::time::Instant::now();
        
        if let Some(lum) = get_primary_monitor_average_luminance() {
            // User requested: darker screen -> higher brightness
            // lum is 0.0 (black) to 1.0 (white)
            // If screen is dark (lum = 0.0), brightness should be high (e.g. 100%)
            // If screen is light (lum = 1.0), brightness should be low (e.g. 20%)
            
            // Let's define bounds: 20% to 100%
            let min_brightness = 20.0;
            let max_brightness = 100.0;
            
            // Inverted relationship
            let target_b = max_brightness - (lum * (max_brightness - min_brightness));
            
            // Apply smoothing
            current_brightness = (current_brightness * smoothing_factor) + (target_b * (1.0 - smoothing_factor));
            
            // Set brightness
            set_monitor_brightness(current_brightness.round() as u32);
            
            let elapsed = start.elapsed();
            // We want to sample roughly 10 times a second (every 100ms)
            // Wait for the remaining time
            if elapsed < Duration::from_millis(100) {
                thread::sleep(Duration::from_millis(100) - elapsed);
            }
        } else {
            thread::sleep(Duration::from_millis(100));
        }
    }
}
