use super::skill::SkillStep;
use std::collections::HashMap;

pub struct SelfHealingEngine;

impl SelfHealingEngine {
    /// Injects dynamic parameter values into a skill's text fields (e.g. {{date}} -> "2026-10-20")
    pub fn parameterize_step(step: &SkillStep, params: &HashMap<String, String>) -> SkillStep {
        let mut modified = step.clone();
        if let Some(ref text) = step.text {
            let mut resolved = text.clone();
            for (key, val) in params {
                let placeholder = format!("{{{{{}}}}}", key);
                resolved = resolved.replace(&placeholder, val);
            }
            modified.text = Some(resolved);
        }
        modified
    }

    /// Recalibrates click coordinates if window scale or DPR shifted between recording and replay
    pub fn recalibrate_coords(
        orig_x: i32,
        orig_y: i32,
        orig_w: u32,
        orig_h: u32,
        cur_w: u32,
        cur_h: u32,
    ) -> (i32, i32) {
        if orig_w == 0 || orig_h == 0 || (orig_w == cur_w && orig_h == cur_h) {
            return (orig_x, orig_y);
        }

        let scale_x = cur_w as f64 / orig_w as f64;
        let scale_y = cur_h as f64 / orig_h as f64;

        let new_x = (orig_x as f64 * scale_x).round() as i32;
        let new_y = (orig_y as f64 * scale_y).round() as i32;

        (new_x, new_y)
    }
}
