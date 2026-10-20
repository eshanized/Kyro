//! Visual regression baseline comparison subsystem.
//!
//! Provides trusted visual baseline storage and pixel-level comparison
//! to detect visual regressions across responsive viewports.

use crate::timeline::TimelineEngine;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// A stored visual baseline for an approved state of a route and viewport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualBaseline {
    pub project_path: String,
    pub route: String,
    pub viewport: String,
    pub base64_png: String,
    pub created_at: i64,
    pub artifact_id: Option<String>,
}

/// The result of comparing a current screenshot against a trusted baseline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualRegressionResult {
    pub has_regression: bool,
    pub diff_percent: f32,
    pub route: String,
    pub viewport: String,
    pub description: String,
    pub baseline_artifact: Option<String>,
    pub current_artifact: Option<String>,
}

pub struct VisualRegressionEngine;

impl VisualRegressionEngine {
    /// Sanitizes a string for safe filesystem storage
    pub fn sanitize_component(s: &str) -> String {
        s.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
    }

    /// Resolves the storage path for a project's baselines
    pub fn baselines_dir(project_path: &Path) -> PathBuf {
        let base_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kyro")
            .join("baselines");
        let hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(project_path.to_string_lossy().as_bytes());
            let out = hasher.finalize();
            out.iter().map(|b| format!("{:02x}", b)).collect::<String>()
        };
        base_dir.join(hash)
    }

    /// Stores an approved baseline for a specific route and viewport.
    pub fn save_baseline(
        project_path: &Path,
        route: &str,
        viewport: &str,
        base64_png: &str,
        artifact_id: Option<&str>,
    ) -> Result<VisualBaseline, String> {
        let dir = Self::baselines_dir(project_path);
        fs::create_dir_all(&dir).map_err(|e| {
            format!(
                "Failed to create baselines folder '{}': {}",
                dir.display(),
                e
            )
        })?;

        let filename = format!(
            "baseline_{}_{}.json",
            Self::sanitize_component(route),
            Self::sanitize_component(viewport)
        );
        let path = dir.join(filename);

        let baseline = VisualBaseline {
            project_path: project_path.to_string_lossy().to_string(),
            route: route.to_string(),
            viewport: viewport.to_string(),
            base64_png: base64_png.to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            artifact_id: artifact_id.map(|s| s.to_string()),
        };

        let json = serde_json::to_string_pretty(&baseline)
            .map_err(|e| format!("Failed to serialize baseline: {}", e))?;
        fs::write(&path, json)
            .map_err(|e| format!("Failed to write baseline file '{}': {}", path.display(), e))?;

        Ok(baseline)
    }

    /// Retrieves a stored baseline if one exists for the given route and viewport.
    pub fn load_baseline(
        project_path: &Path,
        route: &str,
        viewport: &str,
    ) -> Option<VisualBaseline> {
        let dir = Self::baselines_dir(project_path);
        let filename = format!(
            "baseline_{}_{}.json",
            Self::sanitize_component(route),
            Self::sanitize_component(viewport)
        );
        let path = dir.join(filename);

        if !path.exists() {
            return None;
        }

        let json = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&json).ok()
    }

    /// Compares a current screenshot against a baseline, returning regression metrics.
    pub fn compare(
        baseline_b64: &str,
        current_b64: &str,
        route: &str,
        viewport: &str,
        threshold_percent: f32,
    ) -> Result<VisualRegressionResult, String> {
        fn clean_payload(s: &str) -> &str {
            if let Some(idx) = s.find(";base64,") {
                &s[idx + 8..]
            } else {
                s.trim()
            }
        }

        let base_bytes = BASE64
            .decode(clean_payload(baseline_b64))
            .map_err(|e| format!("Failed to decode baseline base64: {}", e))?;
        let curr_bytes = BASE64
            .decode(clean_payload(current_b64))
            .map_err(|e| format!("Failed to decode current base64: {}", e))?;

        let base_img = image::load_from_memory(&base_bytes)
            .map_err(|e| format!("Failed to parse baseline image: {}", e))?
            .to_rgba8();
        let curr_img = image::load_from_memory(&curr_bytes)
            .map_err(|e| format!("Failed to parse current image: {}", e))?
            .to_rgba8();

        let diff_percent = TimelineEngine::compute_diff_percent(&base_img, &curr_img);
        let has_regression = diff_percent > threshold_percent;

        let description = if has_regression {
            format!(
                "Visual regression observed on {} ({}): {:.2}% pixel divergence from approved baseline (threshold: {:.1}%).",
                route, viewport, diff_percent, threshold_percent
            )
        } else {
            format!(
                "Visual appearance on {} ({}) matches approved baseline ({:.2}% difference, within {:.1}% tolerance).",
                route, viewport, diff_percent, threshold_percent
            )
        };

        Ok(VisualRegressionResult {
            has_regression,
            diff_percent,
            route: route.to_string(),
            viewport: viewport.to_string(),
            description,
            baseline_artifact: None,
            current_artifact: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    fn create_test_image_b64(color: [u8; 4], w: u32, h: u32) -> String {
        let mut img = image::RgbaImage::new(w, h);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba(color);
        }
        let mut buf = Vec::new();
        image::codecs::png::PngEncoder::new(&mut buf)
            .write_image(&img, w, h, image::ExtendedColorType::Rgba8)
            .unwrap();
        BASE64.encode(buf)
    }

    #[test]
    fn test_baseline_storage_and_comparison() {
        let temp =
            std::env::temp_dir().join(format!("kyro_baseline_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp).unwrap();

        let img_white = create_test_image_b64([255, 255, 255, 255], 100, 100);
        let img_white_dup = create_test_image_b64([255, 255, 255, 255], 100, 100);
        let img_black = create_test_image_b64([0, 0, 0, 255], 100, 100);

        // 1. Save baseline
        let saved = VisualRegressionEngine::save_baseline(
            &temp,
            "/settings",
            "1440x900",
            &img_white,
            Some("art_base_1"),
        )
        .expect("Save baseline succeeds");
        assert_eq!(saved.route, "/settings");
        assert_eq!(saved.viewport, "1440x900");

        // 2. Load baseline
        let loaded = VisualRegressionEngine::load_baseline(&temp, "/settings", "1440x900")
            .expect("Must load existing baseline");
        assert_eq!(loaded.artifact_id, Some("art_base_1".to_string()));

        // 3. Compare identical: 0% diff
        let res_identical = VisualRegressionEngine::compare(
            &loaded.base64_png,
            &img_white_dup,
            "/settings",
            "1440x900",
            1.0,
        )
        .expect("Compare succeeds");
        assert!(!res_identical.has_regression);
        assert_eq!(res_identical.diff_percent, 0.0);

        // 4. Compare inverted black image: 100% diff -> triggers regression
        let res_diverged = VisualRegressionEngine::compare(
            &loaded.base64_png,
            &img_black,
            "/settings",
            "1440x900",
            1.0,
        )
        .expect("Compare succeeds");
        assert!(res_diverged.has_regression);
        assert_eq!(res_diverged.diff_percent, 100.0);
        assert!(res_diverged
            .description
            .contains("Visual regression observed"));

        let _ = fs::remove_dir_all(&temp);
    }
}
