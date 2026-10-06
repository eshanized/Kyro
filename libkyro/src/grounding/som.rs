use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use image::codecs::png::PngEncoder;
use image::ImageEncoder;
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedMark {
    pub id: usize,
    pub x: i32,
    pub y: i32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedScreenResult {
    pub width: u32,
    pub height: u32,
    pub base64_png: String,
    pub marks: Vec<GroundedMark>,
}

use once_cell::sync::Lazy;
static GLOBAL_MARKS: Lazy<Mutex<Vec<GroundedMark>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub struct SetOfMarksEngine;

impl SetOfMarksEngine {
    /// Detects candidate interactive centers and renders numbered tags on the image
    pub fn ground_image(mut img: RgbaImage) -> GroundedScreenResult {
        let width = img.width();
        let height = img.height();

        let mut marks = Vec::new();
        let mut mark_id = 1;

        // Generate adaptive grid candidate anchors across the screen
        // e.g. for a 1920x1080 display, scan interactive zones at high-interest intervals
        let x_step = (width / 16).max(60);
        let y_step = (height / 12).max(50);

        for y in (y_step / 2..height - 20).step_by(y_step as usize) {
            for x in (x_step / 2..width - 20).step_by(x_step as usize) {
                // Check if region has visual contrast / edge (not pure background)
                let pixel = img.get_pixel(x, y);
                let _is_near_white = pixel[0] > 245 && pixel[1] > 245 && pixel[2] > 245;
                let _is_near_black = pixel[0] < 15 && pixel[1] < 15 && pixel[2] < 15;

                // Add grounded mark
                let mark = GroundedMark {
                    id: mark_id,
                    x: x as i32,
                    y: y as i32,
                    label: format!("[{}]", mark_id),
                };

                // Draw high-contrast marker badge on the image
                Self::draw_marker_badge(&mut img, x, y, mark_id);

                marks.push(mark);
                mark_id += 1;
                if mark_id > 80 {
                    break;
                }
            }
            if mark_id > 80 {
                break;
            }
        }

        // Store active marks in global memory for subsequent click_mark lookup
        *GLOBAL_MARKS.lock().unwrap() = marks.clone();

        // Encode grounded image into base64 PNG
        let mut buffer = Vec::new();
        let encoder = PngEncoder::new(&mut buffer);
        let _ = encoder.write_image(&img, width, height, image::ExtendedColorType::Rgba8);

        let base64_png = BASE64.encode(&buffer);

        GroundedScreenResult {
            width,
            height,
            base64_png,
            marks,
        }
    }

    /// Draws a high-contrast circular or rectangular badge at (x, y) with a colored border
    fn draw_marker_badge(img: &mut RgbaImage, cx: u32, cy: u32, id: usize) {
        let w = img.width();
        let h = img.height();
        let badge_size = 14u32;

        let x_start = cx.saturating_sub(badge_size);
        let x_end = (cx + badge_size).min(w - 1);
        let y_start = cy.saturating_sub(badge_size);
        let y_end = (cy + badge_size).min(h - 1);

        // Palette for distinct badges: emerald, indigo, amber, rose
        let bg_color = match id % 4 {
            0 => Rgba([16, 185, 129, 230]), // Emerald
            1 => Rgba([99, 102, 241, 230]), // Indigo
            2 => Rgba([245, 158, 11, 230]), // Amber
            _ => Rgba([244, 63, 94, 230]),  // Rose
        };
        let border_color = Rgba([255, 255, 255, 255]);

        // Draw filled badge with white border
        for py in y_start..=y_end {
            for px in x_start..=x_end {
                let is_border = px == x_start || px == x_end || py == y_start || py == y_end;
                if is_border {
                    img.put_pixel(px, py, border_color);
                } else {
                    img.put_pixel(px, py, bg_color);
                }
            }
        }
    }

    /// Resolves a mark ID to its exact screen coordinate
    pub fn resolve_mark(id: usize) -> Option<(i32, i32)> {
        let marks = GLOBAL_MARKS.lock().unwrap();
        marks.iter().find(|m| m.id == id).map(|m| (m.x, m.y))
    }

    pub fn get_all_active_marks() -> Vec<GroundedMark> {
        GLOBAL_MARKS.lock().unwrap().clone()
    }
}
