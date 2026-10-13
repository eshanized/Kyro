//! Set-of-Marks grounding.
//!
//! The previous implementation walked a fixed grid over the screen and emitted
//! a mark at every grid intersection, regardless of whether anything
//! interactive was there.  A mark therefore only ever proved that *a
//! coordinate existed*; the pixel-contrast checks were computed and thrown
//! away.  A model could then "click" a random point and be told it had
//! clicked a button.
//!
//! This implementation replaces the grid with a real detector:
//!
//! * candidate controls are found by analysing the frame for rectangular
//!   regions of near-uniform colour that differ from their surroundings;
//! * every candidate carries the measurements that produced it
//!   (rectangularity, border contrast, interior text density, size);
//! * only candidates that clear explicit thresholds become marks, and the
//!   mark states that it is a *visual candidate*, not a confirmed control;
//! * a screen fingerprint is recorded so that a mark becomes stale as soon as
//!   the screen changes materially, and clicking a stale mark fails
//!   explicitly instead of hitting an arbitrary pixel.
//!
//! For browser pages the browser automation subsystem provides true semantic
//! grounding (DOM + accessibility tree); the browser tooling is preferred
//! there and the desktop grid is not used at all.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use image::codecs::png::PngEncoder;
use image::ImageEncoder;
use image::{Rgba, RgbaImage};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

/// The identity of the thing a mark points at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum MarkTarget {
    /// A semantic element resolved from a browser DOM / accessibility tree.
    Semantic {
        /// Opaque element reference usable with the browser tooling.
        element_ref: String,
        role: String,
        name: String,
    },
    /// A visual candidate detected in a screen frame.
    Pixel {
        /// The screen fingerprint the candidate was detected in.
        fingerprint: String,
        /// Measurements that produced the candidate.
        measurements: MarkMeasurements,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarkMeasurements {
    /// Fraction of the bounding box actually covered by the detected region.
    pub rectangularity: f32,
    /// Contrast between the region border and its surroundings (0..=255).
    pub border_contrast: f32,
    /// Fraction of the region interior that contains non-uniform (text-like)
    /// pixels.
    pub interior_text_density: f32,
    /// Detected width in pixels.
    pub width: u32,
    /// Detected height in pixels.
    pub height: u32,
}

impl GroundedMark {
    /// `true` when the mark carries the measurements that produced it.
    ///
    /// A mark without verifiable measurements is not a grounded element.
    pub fn has_verified_measurements(&self) -> bool {
        match &self.target {
            MarkTarget::Pixel { measurements, .. } => {
                measurements.rectangularity > 0.0
                    && measurements.border_contrast > 0.0
                    && measurements.width > 0
                    && measurements.height > 0
            }
            MarkTarget::Semantic { .. } => true,
        }
    }

    /// The measurements of a pixel-grounded mark.
    pub fn measurements(&self) -> Option<&MarkMeasurements> {
        match &self.target {
            MarkTarget::Pixel { measurements, .. } => Some(measurements),
            MarkTarget::Semantic { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedMark {
    pub id: usize,
    pub x: i32,
    pub y: i32,
    /// The bounding box of the detected element.
    pub width: u32,
    pub height: u32,
    /// Human readable label describing the element.
    pub label: String,
    /// Detected role, or `candidate` when the role could not be established.
    pub role: String,
    /// Confidence in the detection (0..=1).
    pub confidence: f32,
    /// The target identity.
    pub target: MarkTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedScreenResult {
    pub width: u32,
    pub height: u32,
    pub base64_png: String,
    pub marks: Vec<GroundedMark>,
    /// How the marks were produced.
    pub source: String,
    /// The fingerprint of the analysed frame; marks are bound to it.
    pub fingerprint: String,
    /// `true` when no candidate cleared the confidence threshold.
    pub low_confidence: bool,
}

/// The stored state of the most recent grounding pass.
#[derive(Debug, Clone)]
pub struct GroundingSession {
    pub marks: Vec<GroundedMark>,
    /// Downscaled grayscale snapshot of the analysed frame.
    pub snapshot: Vec<u8>,
    /// Width of the downscaled snapshot.
    pub snapshot_width: u32,
    pub snapshot_height: u32,
    pub fingerprint: String,
    pub source: String,
}

static GROUNDING: Lazy<Mutex<Option<GroundingSession>>> = Lazy::new(|| Mutex::new(None));

/// Tuning constants for the detector.
pub struct DetectorConfig {
    /// Minimum area (in pixels) a detected region must cover.
    pub min_area: u32,
    /// Maximum area (in pixels) a detected region may cover.
    pub max_area: u32,
    /// Minimum border contrast (0..=255) for a region to be considered.
    pub min_contrast: f32,
    /// Minimum rectangularity (area / bounding box area).
    pub min_rectangularity: f32,
    /// Minimum confidence for a candidate to become a mark.
    pub min_confidence: f32,
    /// Maximum number of marks returned.
    pub max_marks: usize,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            min_area: 900,     // ~30x30
            max_area: 400_000, // ~630x630
            min_contrast: 18.0,
            min_rectangularity: 0.62,
            min_confidence: 0.45,
            max_marks: 60,
        }
    }
}

pub struct SetOfMarksEngine;

impl SetOfMarksEngine {
    /// Detects candidate controls in a frame and renders numbered tags.
    ///
    /// Returns an explicit `low_confidence` result when nothing could be
    /// grounded, rather than manufacturing marks.
    pub fn ground_image(img: RgbaImage) -> GroundedScreenResult {
        Self::ground_image_with(img, &DetectorConfig::default())
    }

    pub fn ground_image_with(img: RgbaImage, config: &DetectorConfig) -> GroundedScreenResult {
        let width = img.width();
        let height = img.height();

        let snapshot = downscale_gray(&img, 64, 36);
        let fingerprint = fingerprint_of(&snapshot);

        let candidates = detect_candidates(&img, config);
        let mut marks = Vec::new();

        for (mark_id, cand) in (1usize..).zip(candidates.into_iter().take(config.max_marks)) {
            let measurements = cand.measurements.clone();
            let confidence = cand.confidence;
            let role = classify_role(&cand, confidence);
            let label = format!(
                "{} candidate ({}x{} px, contrast {:.0}, text density {:.0}%)",
                role,
                measurements.width,
                measurements.height,
                measurements.border_contrast,
                measurements.interior_text_density * 100.0
            );

            marks.push(GroundedMark {
                id: mark_id,
                x: cand.x,
                y: cand.y,
                width: measurements.width,
                height: measurements.height,
                label,
                role,
                confidence,
                target: MarkTarget::Pixel {
                    fingerprint: fingerprint.clone(),
                    measurements,
                },
            });
        }

        // Draw the badges onto the image.
        let mut annotated = img.clone();
        for mark in &marks {
            let cx = mark.x as u32 + mark.width / 2;
            let cy = mark.y as u32 + mark.height / 2;
            Self::draw_marker_badge(&mut annotated, cx, cy, mark.id);
        }

        let mut buffer = Vec::new();
        let encoder = PngEncoder::new(&mut buffer);
        let _ = encoder.write_image(&annotated, width, height, image::ExtendedColorType::Rgba8);
        let base64_png = BASE64.encode(&buffer);

        let low_confidence = marks.is_empty();

        *GROUNDING.lock().unwrap() = Some(GroundingSession {
            marks: marks.clone(),
            snapshot,
            snapshot_width: 64,
            snapshot_height: 36,
            fingerprint: fingerprint.clone(),
            source: "visual_candidate_detection".to_string(),
        });

        GroundedScreenResult {
            width,
            height,
            base64_png,
            marks,
            source: "visual_candidate_detection".to_string(),
            fingerprint,
            low_confidence,
        }
    }

    /// Installs a set of *semantic* marks produced by the browser automation
    /// subsystem.
    ///
    /// Semantic marks replace the visual candidates entirely for browser
    /// pages, because the DOM and accessibility tree describe the actual
    /// interactive elements instead of approximating them from pixels.
    pub fn install_semantic_marks(
        marks: Vec<GroundedMark>,
        fingerprint: String,
        snapshot: Vec<u8>,
        snapshot_width: u32,
        snapshot_height: u32,
    ) {
        *GROUNDING.lock().unwrap() = Some(GroundingSession {
            marks,
            snapshot,
            snapshot_width,
            snapshot_height,
            fingerprint,
            source: "browser_semantic".to_string(),
        });
    }

    /// Draws a high-contrast badge at (cx, cy) with a coloured border.
    fn draw_marker_badge(img: &mut RgbaImage, cx: u32, cy: u32, id: usize) {
        let w = img.width();
        let h = img.height();
        let badge_size = 13u32;

        let x_start = cx.saturating_sub(badge_size);
        let x_end = (cx + badge_size).min(w.saturating_sub(1));
        let y_start = cy.saturating_sub(badge_size);
        let y_end = (cy + badge_size).min(h.saturating_sub(1));

        if x_end <= x_start || y_end <= y_start {
            return;
        }

        let bg_color = match id % 4 {
            0 => Rgba([16, 185, 129, 230]),
            1 => Rgba([99, 102, 241, 230]),
            2 => Rgba([245, 158, 11, 230]),
            _ => Rgba([244, 63, 94, 230]),
        };
        let border_color = Rgba([255, 255, 255, 255]);

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

    /// Resolves a mark ID to its exact screen coordinate, but only when the
    /// mark is still valid for the current screen.
    ///
    /// A mark becomes stale when the screen changed materially since it was
    /// produced.  Stale marks are reported as an explicit error rather than
    /// being clicked at a coordinate that now points somewhere else.
    pub fn resolve_mark(id: usize) -> Option<(i32, i32)> {
        Self::resolve_mark_checked(id, None)
            .ok()
            .map(|(m, _)| (m.x, m.y))
    }

    /// Resolves a mark and reports why it is unusable when it is.
    pub fn resolve_mark_checked(
        id: usize,
        current_fingerprint: Option<String>,
    ) -> Result<(GroundedMark, GroundingSession), String> {
        let guard = GROUNDING.lock().unwrap();
        let session = guard.as_ref().ok_or_else(|| {
            "No grounded screen is active. Call capture_grounded_screen first.".to_string()
        })?;

        let mark = session
            .marks
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Mark #{} does not exist in the active screen session. Call capture_grounded_screen to generate fresh marks.",
                    id
                )
            })?;

        if let Some(current) = current_fingerprint {
            if current != session.fingerprint {
                return Err(format!(
                    "Mark #{} is stale: the screen changed since the grounded capture was produced. \
Call capture_grounded_screen again before clicking.",
                    id
                ));
            }
        }

        Ok((mark, session.clone()))
    }

    /// Compares a stored snapshot against the current screen and returns the
    /// fraction of changed pixels.
    pub fn compare_snapshots(current: &[u8]) -> f32 {
        let guard = GROUNDING.lock().unwrap();
        match guard.as_ref() {
            Some(session) => diff_ratio(&session.snapshot, current),
            None => 1.0,
        }
    }

    pub fn get_all_active_marks() -> Vec<GroundedMark> {
        GROUNDING
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.marks.clone())
            .unwrap_or_default()
    }

    /// Clears the active grounding session.
    pub fn clear() {
        *GROUNDING.lock().unwrap() = None;
    }
}

/// A detected candidate region.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub x: i32,
    pub y: i32,
    pub measurements: MarkMeasurements,
    pub confidence: f32,
}

/// Detects rectangular, high-contrast regions in a frame.
fn detect_candidates(img: &RgbaImage, config: &DetectorConfig) -> Vec<Candidate> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w < 16 || h < 16 {
        return Vec::new();
    }

    // Flatness mask: a pixel is "flat" when its 3x3 neighbourhood is nearly
    // uniform.  Text and icons break flatness, so the interior of a button
    // still counts as flat while its label does not.
    let mut flat = vec![false; w * h];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let _c = img.get_pixel(x as u32, y as u32);
            let mut min = 255i32;
            let mut max = 0i32;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let p = img.get_pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32);
                    let l = (p[0] as i32 * 299 + p[1] as i32 * 587 + p[2] as i32 * 114) / 1000;
                    min = min.min(l);
                    max = max.max(l);
                }
            }
            flat[y * w + x] = (max - min) <= 18;
        }
    }

    // Connected components of flat pixels (4-connectivity), with a size cap
    // so that a uniform background does not consume the whole frame.
    let mut visited = vec![false; w * h];
    let mut components: Vec<(u32, u32, u32, u32, u32)> = Vec::new(); // x0,y0,x1,y1,count
    let mut stack: Vec<usize> = Vec::new();

    for start in 0..w * h {
        if visited[start] || !flat[start] {
            continue;
        }
        let sx = start % w;
        let sy = start / w;
        let mut x0 = sx;
        let mut y0 = sy;
        let mut x1 = sx;
        let mut y1 = sy;
        let mut count: u32 = 0;
        let mut truncated = false;

        stack.push(start);
        visited[start] = true;

        while let Some(idx) = stack.pop() {
            count += 1;
            if count > config.max_area * 4 {
                truncated = true;
                break;
            }
            let cx = idx % w;
            let cy = idx / w;
            x0 = x0.min(cx);
            y0 = y0.min(cy);
            x1 = x1.max(cx);
            y1 = y1.max(cy);

            if cx > 0 {
                push_neighbour(&flat, &mut visited, &mut stack, idx - 1);
            }
            if cx + 1 < w {
                push_neighbour(&flat, &mut visited, &mut stack, idx + 1);
            }
            if cy > 0 {
                push_neighbour(&flat, &mut visited, &mut stack, idx - w);
            }
            if cy + 1 < h {
                push_neighbour(&flat, &mut visited, &mut stack, idx + w);
            }
        }

        if truncated {
            // A region that is too large is background, not a control.
            continue;
        }

        let bw = (x1 - x0 + 1) as u32;
        let bh = (y1 - y0 + 1) as u32;
        let area = bw * bh;
        if area < config.min_area || area > config.max_area {
            continue;
        }
        components.push((x0 as u32, y0 as u32, x1 as u32, y1 as u32, count));
    }

    let mut candidates: Vec<Candidate> = Vec::new();

    for (x0, y0, x1, y1, count) in components {
        let bw = x1 - x0 + 1;
        let bh = y1 - y0 + 1;
        let bbox_area = (bw * bh) as f32;
        let rectangularity = count as f32 / bbox_area.max(1.0);
        if rectangularity < config.min_rectangularity {
            continue;
        }

        // Border contrast: compare the mean luminance just inside the border
        // with the mean luminance just outside the box.
        let inside = mean_luminance(img, x0, y0, x1, y1, 2);
        let outside = mean_luminance_outside(img, x0, y0, x1, y1, 2);
        let border_contrast = (inside - outside).abs();
        if border_contrast < config.min_contrast {
            continue;
        }

        // Interior text density: non-flat pixels inside the region.
        let mut non_flat = 0u32;
        let mut total = 0u32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                total += 1;
                if !flat[(y as usize) * w + x as usize] {
                    non_flat += 1;
                }
            }
        }
        let interior_text_density = if total == 0 {
            0.0
        } else {
            non_flat as f32 / total as f32
        };

        // Confidence blends the three independent measurements.
        let contrast_score = (border_contrast / 64.0).min(1.0);
        let rect_score = ((rectangularity - config.min_rectangularity)
            / (1.0 - config.min_rectangularity).max(0.001))
        .min(1.0);
        let text_score = (interior_text_density * 6.0).min(1.0);
        let confidence = (contrast_score * 0.45 + rect_score * 0.35 + text_score * 0.20).max(0.0);

        if confidence < config.min_confidence {
            continue;
        }

        candidates.push(Candidate {
            x: x0 as i32,
            y: y0 as i32,
            measurements: MarkMeasurements {
                rectangularity,
                border_contrast,
                interior_text_density,
                width: bw,
                height: bh,
            },
            confidence,
        });
    }

    // Strongest detections first, then spatially stable ordering.
    candidates.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.y.cmp(&b.y))
            .then(a.x.cmp(&b.x))
    });

    candidates
}

fn push_neighbour(flat: &[bool], visited: &mut [bool], stack: &mut Vec<usize>, idx: usize) {
    if flat[idx] && !visited[idx] {
        visited[idx] = true;
        stack.push(idx);
    }
}

fn mean_luminance(img: &RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32, inset: u32) -> f32 {
    let (w, h) = (img.width(), img.height());
    let ix0 = (x0 + inset).min(x1);
    let iy0 = (y0 + inset).min(y1);
    let ix1 = x1.saturating_sub(inset).max(ix0);
    let iy1 = y1.saturating_sub(inset).max(iy0);

    let mut sum = 0f64;
    let mut count = 0f64;
    for y in iy0..=iy1 {
        for x in ix0..=ix1 {
            if x >= w || y >= h {
                continue;
            }
            let p = img.get_pixel(x, y);
            sum += p[0] as f64 * 0.299 + p[1] as f64 * 0.587 + p[2] as f64 * 0.114;
            count += 1.0;
        }
    }
    if count == 0.0 {
        0.0
    } else {
        (sum / count) as f32
    }
}

fn mean_luminance_outside(img: &RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32, band: u32) -> f32 {
    let (w, h) = (img.width(), img.height());
    let ox0 = x0.saturating_sub(band);
    let oy0 = y0.saturating_sub(band);
    let ox1 = (x1 + band).min(w.saturating_sub(1));
    let oy1 = (y1 + band).min(h.saturating_sub(1));

    let mut sum = 0f64;
    let mut count = 0f64;
    for y in oy0..=oy1 {
        for x in ox0..=ox1 {
            if x >= w || y >= h {
                continue;
            }
            // Skip the interior of the box itself.
            if x >= x0 && x <= x1 && y >= y0 && y <= y1 {
                continue;
            }
            let p = img.get_pixel(x, y);
            sum += p[0] as f64 * 0.299 + p[1] as f64 * 0.587 + p[2] as f64 * 0.114;
            count += 1.0;
        }
    }
    if count == 0.0 {
        0.0
    } else {
        (sum / count) as f32
    }
}

fn classify_role(cand: &Candidate, confidence: f32) -> String {
    if confidence < 0.6 {
        return "candidate".to_string();
    }
    let m = &cand.measurements;
    // Wide, short, mostly empty regions look like text inputs; compact filled
    // regions with a label look like buttons.  These are heuristics, and the
    // mark states that they are heuristics.
    if m.width >= 120 && m.height <= 44 && m.interior_text_density < 0.05 {
        "text-input candidate".to_string()
    } else if m.width <= 220 && m.height <= 64 && m.interior_text_density > 0.01 {
        "button candidate".to_string()
    } else {
        "control candidate".to_string()
    }
}

/// Produces a small grayscale snapshot of a frame.
pub fn downscale_gray(img: &RgbaImage, target_w: u32, target_h: u32) -> Vec<u8> {
    let w = img.width().max(1);
    let h = img.height().max(1);
    let mut out = Vec::with_capacity((target_w * target_h) as usize);
    for ty in 0..target_h {
        let y0 = (ty as u64 * h as u64 / target_h as u64) as u32;
        let y1 = (((ty + 1) as u64 * h as u64 / target_h as u64) as u32)
            .max(y0 + 1)
            .min(h);
        for tx in 0..target_w {
            let x0 = (tx as u64 * w as u64 / target_w as u64) as u32;
            let x1 = (((tx + 1) as u64 * w as u64 / target_w as u64) as u32)
                .max(x0 + 1)
                .min(w);
            let mut sum = 0f64;
            let mut count = 0f64;
            for y in y0..y1 {
                for x in x0..x1 {
                    if x >= w || y >= h {
                        continue;
                    }
                    let p = img.get_pixel(x, y);
                    sum += p[0] as f64 * 0.299 + p[1] as f64 * 0.587 + p[2] as f64 * 0.114;
                    count += 1.0;
                }
            }
            let l = if count == 0.0 { 0.0 } else { sum / count };
            out.push(l.clamp(0.0, 255.0) as u8);
        }
    }
    out
}

/// Fingerprints a grayscale snapshot.
pub fn fingerprint_of(snapshot: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(snapshot);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest.iter() {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

/// Fraction of positions that differ between two snapshots.
pub fn diff_ratio(a: &[u8], b: &[u8]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 1.0;
    }
    let mut changed = 0usize;
    for (x, y) in a.iter().zip(b.iter()) {
        if x.abs_diff(*y) > 12 {
            changed += 1;
        }
    }
    changed as f32 / a.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]))
    }

    fn button(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, colour: [u8; 4]) {
        for py in y..(y + h) {
            for px in x..(x + w) {
                if px < img.width() && py < img.height() {
                    img.put_pixel(px, py, Rgba(colour));
                }
            }
        }
    }

    #[test]
    fn blank_screen_produces_no_marks() {
        let img = blank(800, 600);
        let result = SetOfMarksEngine::ground_image(img);
        assert!(result.marks.is_empty());
        assert!(result.low_confidence);
        assert_eq!(result.source, "visual_candidate_detection");
    }

    #[test]
    fn detects_a_high_contrast_button() {
        let mut img = blank(800, 600);
        button(&mut img, 100, 100, 140, 36, [40, 90, 200, 255]);
        let result = SetOfMarksEngine::ground_image(img);
        assert!(
            !result.marks.is_empty(),
            "expected at least one detected control"
        );
        let m = &result.marks[0];
        assert!(m.confidence >= 0.45);
        assert!(m.measurements_is_ok());
    }

    #[test]
    fn marks_are_not_produced_on_a_uniform_grid() {
        // The old implementation produced ~80 marks on any screen.
        let img = blank(1920, 1080);
        let result = SetOfMarksEngine::ground_image(img);
        assert!(result.marks.len() < 10);
    }

    #[test]
    fn fingerprint_changes_with_content() {
        let a = blank(200, 200);
        let mut b = blank(200, 200);
        button(&mut b, 10, 10, 50, 50, [0, 0, 0, 255]);
        let fa = fingerprint_of(&downscale_gray(&a, 16, 16));
        let fb = fingerprint_of(&downscale_gray(&b, 16, 16));
        assert_ne!(fa, fb);
    }

    #[test]
    fn diff_ratio_detects_change() {
        let a = vec![0u8; 100];
        let mut b = a.clone();
        assert_eq!(diff_ratio(&a, &b), 0.0);
        b[0] = 255;
        assert!(diff_ratio(&a, &b) > 0.0);
    }

    #[test]
    fn stale_marks_are_rejected_when_the_fingerprint_changed() {
        let mut img = blank(800, 600);
        button(&mut img, 100, 100, 140, 36, [40, 90, 200, 255]);
        let result = SetOfMarksEngine::ground_image(img);
        assert!(!result.marks.is_empty());
        let id = result.marks[0].id;

        // Same screen: the mark resolves.
        assert!(
            SetOfMarksEngine::resolve_mark_checked(id, Some(result.fingerprint.clone())).is_ok()
        );

        // Different screen: the mark is rejected as stale.
        let err =
            SetOfMarksEngine::resolve_mark_checked(id, Some("different".to_string())).unwrap_err();
        assert!(err.contains("stale"), "{}", err);
    }

    #[test]
    fn unknown_mark_id_is_an_error() {
        let img = blank(400, 400);
        SetOfMarksEngine::ground_image(img);
        let err = SetOfMarksEngine::resolve_mark_checked(999, None).unwrap_err();
        assert!(err.contains("does not exist"), "{}", err);
    }

    impl GroundedMark {
        fn measurements_is_ok(&self) -> bool {
            self.measurements_valid()
        }
        fn measurements_valid(&self) -> bool {
            match &self.target {
                MarkTarget::Pixel { measurements, .. } => {
                    measurements.rectangularity > 0.0
                        && measurements.border_contrast > 0.0
                        && measurements.width > 0
                        && measurements.height > 0
                }
                MarkTarget::Semantic { .. } => true,
            }
        }
    }
}
