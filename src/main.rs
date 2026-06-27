use std::thread;
use std::time::Duration;

use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, SetMonitorBrightness, GetMonitorBrightness, PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    MonitorFromPoint, ReleaseDC, SelectObject, SetStretchBltMode, StretchBlt, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, COLORONCOLOR, DIB_RGB_COLORS, MONITOR_DEFAULTTOPRIMARY, RGBQUAD,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

use brightness_sys::Brightness;
use futures::{TryStreamExt, StreamExt};

fn get_primary_monitor_average_luminance() -> Option<f32> {
    unsafe {
        use windows::Win32::Foundation::HWND;
        let hwnd = HWND::default();
        let hdc_screen = GetDC(hwnd);
        if hdc_screen.is_invalid() {
            println!("Failed to get DC");
            return None;
        }

        let width = GetSystemMetrics(SM_CXSCREEN) as i32;
        let height = GetSystemMetrics(SM_CYSCREEN) as i32;

        if width == 0 || height == 0 {
            ReleaseDC(hwnd, hdc_screen);
            println!("Screen metrics returned 0");
            return None;
        }

        // We scale down the screen to 100x100 for extremely fast processing
        let target_w = 50;
        let target_h = 50;

        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let hbm_mem = CreateCompatibleBitmap(hdc_screen, target_w, target_h);

        let hbm_old = SelectObject(hdc_mem, hbm_mem);

        // Set stretch mode to COLORONCOLOR to prevent dropping pixels entirely
        SetStretchBltMode(hdc_mem, COLORONCOLOR);

        let blt_res = StretchBlt(
            hdc_mem, 0, 0, target_w, target_h,
            hdc_screen, 0, 0, width, height,
            SRCCOPY,
        );

        if blt_res.0 == 0 {
            SelectObject(hdc_mem, hbm_old);
            DeleteObject(hbm_mem);
            DeleteDC(hdc_mem);
            ReleaseDC(hwnd, hdc_screen);
            println!("StretchBlt failed");
            return None;
        }

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: target_w,
                biHeight: -target_h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD::default(); 1],
        };

        let num_pixels = (target_w * target_h) as usize;
        let mut pixels: Vec<u32> = vec![0; num_pixels];

        let scan_lines = GetDIBits(
            hdc_mem,
            hbm_mem,
            0,
            target_h as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // Cleanup
        SelectObject(hdc_mem, hbm_old);
        DeleteObject(hbm_mem);
        DeleteDC(hdc_mem);
        ReleaseDC(hwnd, hdc_screen);

        if scan_lines == 0 {
            println!("GetDIBits failed");
            return None;
        }

        let mut total_lum: u64 = 0;
        for &pixel in &pixels {
            let b = (pixel & 0xFF) as u64;
            let g = ((pixel >> 8) & 0xFF) as u64;
            let r = ((pixel >> 16) & 0xFF) as u64;
            
            // integer math for speed: 0.299 * 1000 = 299
            let lum = r * 299 + g * 587 + b * 114;
            total_lum += lum;
        }

        let avg_lum = (total_lum as f64) / (num_pixels as f64) / 1000.0;
        Some((avg_lum / 255.0) as f32)
    }
}

async fn set_monitor_brightness_wmi(target_brightness: u32) {
    let _ = brightness_sys::brightness_devices().try_for_each(|mut dev| async move {
        let _ = dev.set(target_brightness).await;
        Ok(())
    }).await;
}

fn set_monitor_brightness_ddcci(target_brightness: u32) {
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

async fn get_initial_brightness() -> f32 {
    // 1. Try WMI (Laptops)
    let mut devices = brightness_sys::brightness_devices();
    if let Some(Ok(dev)) = devices.next().await {
        if let Ok(val) = dev.get().await {
            return val as f32;
        }
    }
    
    // 2. Try DDC/CI (External monitors)
    unsafe {
        let pt = POINT { x: 0, y: 0 };
        let h_monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY);
        
        let mut num_physical: u32 = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(h_monitor, &mut num_physical).is_ok() && num_physical > 0 {
            let mut physical_monitors = vec![PHYSICAL_MONITOR::default(); num_physical as usize];
            if GetPhysicalMonitorsFromHMONITOR(h_monitor, &mut physical_monitors).is_ok() {
                let pm = &physical_monitors[0];
                let mut min_b: u32 = 0;
                let mut cur_b: u32 = 0;
                let mut max_b: u32 = 0;
                let _ = GetMonitorBrightness(pm.hPhysicalMonitor, &mut min_b, &mut cur_b, &mut max_b);
                let _ = DestroyPhysicalMonitors(&physical_monitors);
                if cur_b > 0 {
                    return cur_b as f32;
                }
            }
        }
    }
    
    // 3. Fallback
    50.0
}

#[tokio::main]
async fn main() {
    println!("Starting Eyesaver...");
    println!("Press Ctrl+C to exit.");
    
    // Smoothing factor: lower = faster transitions (0.65 means it fades quickly over ~150ms)
    // Dragging this out too long makes the discrete hardware steps (1-100) visible to the eye.
    let smoothing_factor = 0.65;
    
    // Initialize current brightness to the monitor's actual brightness
    let mut current_brightness: f32 = get_initial_brightness().await;
    let mut last_set_brightness: u32 = current_brightness.round() as u32;
    let mut active_target_b: f32 = current_brightness;
    
    println!("Initial brightness detected as: {}%", last_set_brightness);
    
    let mut baseline_offset: f32 = 0.0;
    let mut tick_counter: u32 = 0;
    
    loop {
        tick_counter += 1;
        // Check for manual overrides every 500ms (10 ticks)
        if tick_counter % 10 == 0 {
            let actual = get_initial_brightness().await;
            if (actual as i32 - last_set_brightness as i32).abs() > 2 {
                println!("Manual brightness override detected ({} -> {})! Recalibrating baseline...", last_set_brightness, actual);
                baseline_offset += actual - current_brightness;
                current_brightness = actual;
                active_target_b = actual;
                last_set_brightness = actual.round() as u32;
            }
        }
        let start = std::time::Instant::now();
        
        if let Some(lum) = get_primary_monitor_average_luminance() {
            // Calibration points from user:
            // lum = 0.12 -> brightness = 75
            // lum = 0.86 -> brightness = 60
            let lum1: f32 = 0.12;
            let bright1: f32 = 75.0;
            let lum2: f32 = 0.86;
            let bright2: f32 = 60.0;
            
            // Linear interpolation (map range)
            let mut calculated_target_b = bright1 + (lum - lum1) * (bright2 - bright1) / (lum2 - lum1);
            
            // Apply the manual baseline offset
            calculated_target_b += baseline_offset;
            
            calculated_target_b = calculated_target_b.clamp(0.0, 100.0);
            
            // Hysteresis (Deadzone): Only change the active target if the screen changed significantly.
            // This prevents tiny fluctuations (like a blinking cursor or small video) from causing flickering.
            if (calculated_target_b - active_target_b).abs() > 2.0 {
                active_target_b = calculated_target_b;
            }
            
            // Apply smoothing
            current_brightness = (current_brightness * smoothing_factor) + (active_target_b * (1.0 - smoothing_factor));
            let final_b = current_brightness.round() as u32;
            
            if final_b != last_set_brightness {
                println!("Luminance: {:.2} -> Target: {}, Smoothed: {}", lum, active_target_b.round(), final_b);
                
                // Set brightness using both APIs to cover external monitors + laptop displays
                set_monitor_brightness_ddcci(final_b);
                set_monitor_brightness_wmi(final_b).await;
                
                last_set_brightness = final_b;
            }
            
            // Run at ~20 FPS for much smoother and less stuttery transitions
            tokio::time::sleep(Duration::from_millis(50)).await;
        } else {
            let mut error_printed = false;
            if !error_printed {
                println!("Failed to get monitor luminance, retrying... (Ensure you are not running this in a headless terminal)");
                error_printed = true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
