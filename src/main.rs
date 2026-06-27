use std::thread;
use std::time::Duration;

use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, SetMonitorBrightness, PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    MonitorFromPoint, ReleaseDC, SelectObject, SetStretchBltMode, StretchBlt, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, COLORONCOLOR, DIB_RGB_COLORS, MONITOR_DEFAULTTOPRIMARY, RGBQUAD,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetDesktopWindow, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

use brightness_sys::Brightness;
use futures::TryStreamExt;

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
        let target_w = 100;
        let target_h = 100;

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

        let mut total_lum: f64 = 0.0;
        for &pixel in &pixels {
            // format is BGRA for 32-bit DIB
            let b = (pixel & 0xFF) as f64;
            let g = ((pixel >> 8) & 0xFF) as f64;
            let r = ((pixel >> 16) & 0xFF) as f64;
            
            let lum = 0.299 * r + 0.587 * g + 0.114 * b;
            total_lum += lum;
        }

        let avg_lum = total_lum / (num_pixels as f64);
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

#[tokio::main]
async fn main() {
    println!("Starting Eyesaver...");
    println!("Press Ctrl+C to exit.");
    
    // Smoothing factor: higher = smoother but slower transitions (e.g. 0.95)
    let smoothing_factor = 0.95;
    let mut current_brightness: f32 = 50.0; // Assume starting at 50%
    
    loop {
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
            let mut target_b = bright1 + (lum - lum1) * (bright2 - bright1) / (lum2 - lum1);
            
            // Clamp to absolute bounds to ensure valid brightness values
            target_b = target_b.clamp(0.0, 100.0);
            
            // Apply smoothing
            current_brightness = (current_brightness * smoothing_factor) + (target_b * (1.0 - smoothing_factor));
            let final_b = current_brightness.round() as u32;
            
            println!("Luminance: {:.2} -> Target Brightness: {}, Final Smoothed: {}", lum, target_b.round(), final_b);
            
            // Set brightness using both APIs to cover external monitors + laptop displays
            set_monitor_brightness_ddcci(final_b);
            set_monitor_brightness_wmi(final_b).await;
            
            let elapsed = start.elapsed();
            if elapsed < Duration::from_millis(100) {
                tokio::time::sleep(Duration::from_millis(100) - elapsed).await;
            }
        } else {
            let mut error_printed = false;
            if !error_printed {
                println!("Failed to get monitor luminance, retrying... (Ensure you are not running this in a headless terminal)");
                error_printed = true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
