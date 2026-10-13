use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use image::codecs::png::PngEncoder;
use image::ImageEncoder;
use serde::{Deserialize, Serialize};
use xcap::{Monitor, Window};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub app_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotResult {
    pub width: u32,
    pub height: u32,
    pub base64_png: String,
}

pub fn list_windows() -> Result<Vec<WindowInfo>, String> {
    let windows = Window::all().map_err(|e| format!("Failed to list windows: {}", e))?;
    let mut list = Vec::new();
    for w in windows {
        let is_minimized = w.is_minimized();
        if !is_minimized && w.width() > 10 && w.height() > 10 {
            list.push(WindowInfo {
                id: w.id(),
                title: w.title().to_string(),
                app_name: w.app_name().to_string(),
                x: w.x(),
                y: w.y(),
                width: w.width(),
                height: w.height(),
            });
        }
    }
    Ok(list)
}

pub fn capture_screen(monitor_index: usize) -> Result<ScreenshotResult, String> {
    let monitors = Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))?;
    let monitor = monitors
        .get(monitor_index)
        .or_else(|| monitors.first())
        .ok_or_else(|| "No monitors found".to_string())?;

    let img = monitor
        .capture_image()
        .map_err(|e| format!("Capture failed: {}", e))?;

    let width = img.width();
    let height = img.height();

    let mut png_bytes = Vec::new();
    let encoder = PngEncoder::new(&mut png_bytes);
    encoder
        .write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("PNG encoding failed: {}", e))?;

    let base64_png = BASE64.encode(&png_bytes);
    Ok(ScreenshotResult {
        width,
        height,
        base64_png,
    })
}

pub fn capture_window(window_id: u32) -> Result<ScreenshotResult, String> {
    let windows = Window::all().map_err(|e| format!("Failed to list windows: {}", e))?;
    let window = windows
        .into_iter()
        .find(|w| w.id() == window_id)
        .ok_or_else(|| format!("Window {} not found", window_id))?;

    let img = window
        .capture_image()
        .map_err(|e| format!("Window capture failed: {}", e))?;

    let width = img.width();
    let height = img.height();

    let mut png_bytes = Vec::new();
    let encoder = PngEncoder::new(&mut png_bytes);
    encoder
        .write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("PNG encoding failed: {}", e))?;

    let base64_png = BASE64.encode(&png_bytes);
    Ok(ScreenshotResult {
        width,
        height,
        base64_png,
    })
}

/// The bounds of a requested screenshot region, after validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Validates a requested capture region against the actual screen size.
///
/// The previous implementation clamped the width and the height independently
/// of the origin, so a selection that started near the right or bottom edge
/// produced a region that extended past the screen (or produced a silently
/// mis-sized crop).  The right and bottom edges are now clamped against the
/// screen size, and empty or out-of-bounds regions are rejected.
pub fn validate_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    screen_width: u32,
    screen_height: u32,
) -> Result<ValidatedRegion, String> {
    if width == 0 || height == 0 {
        return Err(format!(
            "Capture region must have a non-zero size (received {}x{})",
            width, height
        ));
    }

    if x < 0 || y < 0 {
        return Err(format!(
            "Capture region origin must be non-negative (received x={}, y={})",
            x, y
        ));
    }

    let origin_x = x as u32;
    let origin_y = y as u32;

    if origin_x >= screen_width || origin_y >= screen_height {
        return Err(format!(
            "Capture region origin ({}, {}) is outside the {}x{} screen",
            origin_x, origin_y, screen_width, screen_height
        ));
    }

    // Clamp the *edges*, not the size in isolation.
    let right = (origin_x as u64 + width as u64).min(screen_width as u64) as u32;
    let bottom = (origin_y as u64 + height as u64).min(screen_height as u64) as u32;

    let clamped_width = right.saturating_sub(origin_x);
    let clamped_height = bottom.saturating_sub(origin_y);

    if clamped_width == 0 || clamped_height == 0 {
        return Err(format!(
            "Capture region ({}, {}, {}, {}) does not overlap the {}x{} screen",
            x, y, width, height, screen_width, screen_height
        ));
    }

    Ok(ValidatedRegion {
        x: origin_x,
        y: origin_y,
        width: clamped_width,
        height: clamped_height,
    })
}

/// Captures a region of the first monitor.
///
/// The region is validated and clamped against the real capture size before
/// the crop is taken, so the returned image always matches the requested
/// (clamped) rectangle and never contains pixels from outside the screen.
pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<ScreenshotResult, String> {
    let full = capture_screen(0)?;
    let region = validate_region(x, y, width, height, full.width, full.height)?;

    let bytes = BASE64.decode(&full.base64_png).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;

    let sub =
        image::imageops::crop_imm(&img, region.x, region.y, region.width, region.height).to_image();

    let mut png_bytes = Vec::new();
    let encoder = PngEncoder::new(&mut png_bytes);
    encoder
        .write_image(
            sub.as_raw(),
            sub.width(),
            sub.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("Cropped PNG encoding failed: {}", e))?;

    Ok(ScreenshotResult {
        width: sub.width(),
        height: sub.height(),
        base64_png: BASE64.encode(&png_bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_sized_region() {
        let err = validate_region(0, 0, 0, 10, 1920, 1080).unwrap_err();
        assert!(err.contains("non-zero size"));
    }

    #[test]
    fn rejects_negative_origin() {
        let err = validate_region(-1, 0, 10, 10, 1920, 1080).unwrap_err();
        assert!(err.contains("non-negative"));
    }

    #[test]
    fn rejects_origin_outside_screen() {
        let err = validate_region(5000, 0, 10, 10, 1920, 1080).unwrap_err();
        assert!(err.contains("outside"));
    }

    #[test]
    fn clamps_right_and_bottom_edges() {
        let r = validate_region(1900, 1070, 100, 100, 1920, 1080).unwrap();
        assert_eq!(r.x, 1900);
        assert_eq!(r.y, 1070);
        assert_eq!(r.width, 20);
        assert_eq!(r.height, 10);
    }

    #[test]
    fn region_inside_screen_is_unchanged() {
        let r = validate_region(10, 20, 30, 40, 1920, 1080).unwrap();
        assert_eq!((r.x, r.y, r.width, r.height), (10, 20, 30, 40));
    }

    #[test]
    fn exact_screen_size_is_accepted() {
        let r = validate_region(0, 0, 1920, 1080, 1920, 1080).unwrap();
        assert_eq!(r.width, 1920);
        assert_eq!(r.height, 1080);
    }
}
