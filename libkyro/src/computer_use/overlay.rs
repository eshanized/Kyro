use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinateTransform {
    pub scale_factor_x: f64,
    pub scale_factor_y: f64,
}

impl CoordinateTransform {
    pub fn new(screen_width: u32, screen_height: u32, image_width: u32, image_height: u32) -> Self {
        Self {
            scale_factor_x: screen_width as f64 / image_width as f64,
            scale_factor_y: screen_height as f64 / image_height as f64,
        }
    }

    pub fn to_screen_coords(&self, img_x: i32, img_y: i32) -> (i32, i32) {
        (
            (img_x as f64 * self.scale_factor_x).round() as i32,
            (img_y as f64 * self.scale_factor_y).round() as i32,
        )
    }
}
