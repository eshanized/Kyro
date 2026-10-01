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

pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<ScreenshotResult, String> {
    let full = capture_screen(0)?;
    let bytes = BASE64.decode(&full.base64_png).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;

    let sub = image::imageops::crop_imm(
        &img,
        x.max(0) as u32,
        y.max(0) as u32,
        width.min(full.width),
        height.min(full.height),
    )
    .to_image();

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
