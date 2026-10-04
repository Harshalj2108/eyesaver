//! Screen luminance sampling via GDI (primary monitor, downscaled to 50x50).

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, COLORONCOLOR, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, RGBQUAD, ReleaseDC, SRCCOPY,
    SelectObject, SetStretchBltMode, StretchBlt,
};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

const TARGET_W: i32 = 50;
const TARGET_H: i32 = 50;

/// Average luminance of the primary monitor in 0.0..=1.0, or None if capture failed
/// (e.g. on the secure desktop / lock screen).
pub fn primary_luminance() -> Option<f32> {
    unsafe {
        let hwnd = HWND::default();
        let hdc_screen = GetDC(hwnd);
        if hdc_screen.is_invalid() {
            return None;
        }

        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        if width == 0 || height == 0 {
            ReleaseDC(hwnd, hdc_screen);
            return None;
        }

        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let hbm_mem = CreateCompatibleBitmap(hdc_screen, TARGET_W, TARGET_H);
        let hbm_old = SelectObject(hdc_mem, hbm_mem);

        // COLORONCOLOR = nearest-neighbour sampling; much cheaper than HALFTONE.
        SetStretchBltMode(hdc_mem, COLORONCOLOR);
        let blt_ok = StretchBlt(
            hdc_mem, 0, 0, TARGET_W, TARGET_H, hdc_screen, 0, 0, width, height, SRCCOPY,
        )
        .as_bool();

        let mut pixels = vec![0u32; (TARGET_W * TARGET_H) as usize];
        let mut scan_lines = 0;
        if blt_ok {
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: TARGET_W,
                    biHeight: -TARGET_H, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                bmiColors: [RGBQUAD::default(); 1],
            };
            scan_lines = GetDIBits(
                hdc_mem,
                hbm_mem,
                0,
                TARGET_H as u32,
                Some(pixels.as_mut_ptr() as *mut _),
                &mut bmi,
                DIB_RGB_COLORS,
            );
        }

        // Cleanup (always)
        SelectObject(hdc_mem, hbm_old);
        let _ = DeleteObject(hbm_mem);
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(hwnd, hdc_screen);

        if scan_lines == 0 {
            return None;
        }

        // Rec.601 luma with integer math: 0.299 R + 0.587 G + 0.114 B (scaled by 1000)
        let total: u64 = pixels
            .iter()
            .map(|&p| {
                let b = (p & 0xFF) as u64;
                let g = ((p >> 8) & 0xFF) as u64;
                let r = ((p >> 16) & 0xFF) as u64;
                r * 299 + g * 587 + b * 114
            })
            .sum();
        let avg = total as f64 / pixels.len() as f64 / 1000.0;
        Some((avg / 255.0) as f32)
    }
}
