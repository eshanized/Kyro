use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use image::codecs::png::PngEncoder;
use image::{ImageEncoder, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineStep {
    pub step: usize,
    pub action_name: String,
    pub click_x: Option<i32>,
    pub click_y: Option<i32>,
    pub before_base64: String,
    pub after_base64: String,
    pub diff_percent: f32,
    pub timestamp: i64,
}

pub struct TimelineEngine;

impl TimelineEngine {
    /// Draws a pulsing illuminated red targeting ring at the click coordinate
    pub fn annotate_click_marker(mut img: RgbaImage, click_x: i32, click_y: i32) -> String {
        let w = img.width() as i32;
        let h = img.height() as i32;

        if click_x < 0 || click_x >= w || click_y < 0 || click_y >= h {
            return Self::encode_png(&img);
        }

        let radius = 16i32;
        let ring_color = Rgba([239, 68, 68, 240]); // Red-500
        let center_color = Rgba([255, 255, 255, 255]); // White dot

        // Draw outer ring
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let dist_sq = dx * dx + dy * dy;
                let is_ring = dist_sq >= (radius - 3) * (radius - 3) && dist_sq <= radius * radius;
                let is_center = dist_sq <= 9;

                let px = click_x + dx;
                let py = click_y + dy;

                if px >= 0 && px < w && py >= 0 && py < h {
                    if is_ring {
                        img.put_pixel(px as u32, py as u32, ring_color);
                    } else if is_center {
                        img.put_pixel(px as u32, py as u32, center_color);
                    }
                }
            }
        }

        Self::encode_png(&img)
    }

    /// Computes percentage of pixels modified between before and after frames
    pub fn compute_diff_percent(before: &RgbaImage, after: &RgbaImage) -> f32 {
        if before.dimensions() != after.dimensions() {
            return 100.0;
        }

        let total_pixels = before.width() * before.height();
        if total_pixels == 0 {
            return 0.0;
        }

        let mut changed_pixels = 0u64;
        for (p1, p2) in before.pixels().zip(after.pixels()) {
            let dr = (p1[0] as i32 - p2[0] as i32).abs();
            let dg = (p1[1] as i32 - p2[1] as i32).abs();
            let db = (p1[2] as i32 - p2[2] as i32).abs();
            if dr + dg + db > 25 {
                changed_pixels += 1;
            }
        }

        ((changed_pixels as f64 / total_pixels as f64) * 100.0) as f32
    }

    pub fn encode_png(img: &RgbaImage) -> String {
        let mut buf = Vec::new();
        let encoder = PngEncoder::new(&mut buf);
        let _ = encoder.write_image(
            img,
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        );
        BASE64.encode(buf)
    }
}
