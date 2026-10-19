//! Dedicated visual analysis subsystem for screenshots and UI audits.

use crate::agent::router::{
    ChatMessage, ContentPart, ImageUrlDetail, MessageContent, ModelRouter, DEFAULT_VISION_MODEL,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A detected visible defect in the page layout or rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualDefect {
    pub defect_type: String,
    pub description: String,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default = "default_severity")]
    pub severity: String,
}

fn default_severity() -> String {
    "medium".to_string()
}

/// A prominent region observed on the rendered page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualRegion {
    pub name: String,
    #[serde(default)]
    pub bounds: Option<String>,
    #[serde(default)]
    pub dom_ref: Option<String>,
    pub description: String,
}

/// A discrepancy between the visual appearance and DOM state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualDiscrepancy {
    #[serde(default)]
    pub dom_reference: Option<String>,
    pub description: String,
}

/// The structured outcome of visual analysis on a screenshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualAnalysisResult {
    pub summary: String,
    #[serde(default)]
    pub key_regions: Vec<VisualRegion>,
    #[serde(default)]
    pub visible_defects: Vec<VisualDefect>,
    #[serde(default)]
    pub discrepancies: Vec<VisualDiscrepancy>,
    pub confidence: String,
    pub supporting_evidence: String,
    pub uncertainty: String,
    #[serde(default)]
    pub analyzed_by_model: Option<String>,
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[serde(default)]
    pub vision_supported: bool,
}

/// Context passed along with a screenshot for visual analysis.
#[derive(Debug, Clone, Default)]
pub struct VisualAnalysisContext {
    pub route: String,
    pub preview_url: String,
    pub viewport: String,
    pub dom_summary: Option<String>,
    pub element_of_interest: Option<String>,
}

pub struct VisualAnalyzer {
    router: Arc<ModelRouter>,
    api_key: String,
    base_url: Option<String>,
    configured_vision_model: Option<String>,
}

impl VisualAnalyzer {
    pub fn new(router: Arc<ModelRouter>, api_key: String, base_url: Option<String>) -> Self {
        Self {
            router,
            api_key,
            base_url,
            configured_vision_model: None,
        }
    }

    pub fn with_configured_vision_model(mut self, model: Option<String>) -> Self {
        self.configured_vision_model = model;
        self
    }

    /// Resolves which vision model to use. If primary model is text-only,
    /// falls back to configured vision model or default vision model.
    pub fn resolve_vision_model(&self, primary_model: &str) -> Option<String> {
        if ModelRouter::is_vision_model(primary_model) {
            return Some(primary_model.to_string());
        }
        if let Some(ref m) = self.configured_vision_model {
            if ModelRouter::is_vision_model(m) {
                return Some(m.clone());
            }
        }
        if ModelRouter::is_vision_model(DEFAULT_VISION_MODEL) {
            return Some(DEFAULT_VISION_MODEL.to_string());
        }
        None
    }

    /// Analyzes a screenshot, returning structured visual intelligence.
    pub async fn analyze_screenshot(
        &self,
        base64_png: &str,
        primary_model: &str,
        context: &VisualAnalysisContext,
    ) -> VisualAnalysisResult {
        let vision_model = self.resolve_vision_model(primary_model);

        // If no vision model is available or no API key, report honestly without guessing
        let vision_model = match vision_model {
            Some(m) if !self.api_key.is_empty() => m,
            _ => {
                return VisualAnalysisResult {
                    summary: "Screenshot captured and stored as evidence. Visual analysis was not performed because no vision-capable model is configured or active.".to_string(),
                    key_regions: vec![],
                    visible_defects: vec![],
                    discrepancies: vec![],
                    confidence: "unverified".to_string(),
                    supporting_evidence: "Visual evidence saved to artifact registry.".to_string(),
                    uncertainty: "Screenshot was not analyzed by a vision model.".to_string(),
                    analyzed_by_model: None,
                    artifact_id: None,
                    vision_supported: false,
                };
            }
        };

        let system_prompt = "You are Kyro's Visual Browser Intelligence analyzer. \
Inspect the rendered page screenshot in the given viewport. \
Return ONLY a valid JSON object matching this schema:\n\
{\n\
  \"summary\": \"Brief overview of rendered visual state, layout, and visual completeness\",\n\
  \"key_regions\": [{\"name\": \"header/nav/etc\", \"bounds\": \"top\", \"dom_ref\": \"e1\", \"description\": \"...\"}],\n\
  \"visible_defects\": [{\"defect_type\": \"clipping|overlap|spacing|missing_asset|unreadable_text|blank_area\", \"description\": \"...\", \"location\": \"...\", \"severity\": \"high|medium|low\"}],\n\
  \"discrepancies\": [{\"dom_reference\": \"e1\", \"description\": \"...\"}],\n\
  \"confidence\": \"reproduced\" | \"strong_evidence\" | \"unverified\",\n\
  \"supporting_evidence\": \"Specific visual features observed in the image\",\n\
  \"uncertainty\": \"Any visual details obscured, ambiguous, or needing verification\"\n\
}";

        let user_prompt = format!(
            "Route: {}\nURL: {}\nViewport: {}\nDOM Summary: {}\nElement of interest: {}\n\
Analyze the rendered image for visual defects, layout consistency, and semantic alignment.",
            context.route,
            context.preview_url,
            context.viewport,
            context.dom_summary.as_deref().unwrap_or("none"),
            context.element_of_interest.as_deref().unwrap_or("none"),
        );

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Parts(vec![
                ContentPart::Text { text: user_prompt },
                ContentPart::ImageUrl {
                    image_url: ImageUrlDetail {
                        url: format!("data:image/png;base64,{}", base64_png),
                    },
                },
            ]),
            tool_calls: None,
            tool_call_id: None,
        }];

        match self
            .router
            .complete(
                &vision_model,
                &self.api_key,
                self.base_url.as_deref(),
                system_prompt,
                &messages,
                &[],
                None,
            )
            .await
        {
            Ok(resp) => {
                let content = resp.content;
                if !content.trim().is_empty() {
                    let clean_json = extract_json_block(&content);
                    if let Ok(mut parsed) =
                        serde_json::from_str::<VisualAnalysisResult>(&clean_json)
                    {
                        parsed.analyzed_by_model = Some(vision_model);
                        parsed.vision_supported = true;
                        return parsed;
                    }
                    return VisualAnalysisResult {
                        summary: content.clone(),
                        key_regions: vec![],
                        visible_defects: vec![],
                        discrepancies: vec![],
                        confidence: "strong_evidence".to_string(),
                        supporting_evidence: "Visual analysis completed by vision model."
                            .to_string(),
                        uncertainty: "Model output was unstructured text.".to_string(),
                        analyzed_by_model: Some(vision_model),
                        artifact_id: None,
                        vision_supported: true,
                    };
                }
                VisualAnalysisResult {
                    summary: "Vision model returned an empty response.".to_string(),
                    key_regions: vec![],
                    visible_defects: vec![],
                    discrepancies: vec![],
                    confidence: "unverified".to_string(),
                    supporting_evidence: "".to_string(),
                    uncertainty: "Empty response from vision endpoint.".to_string(),
                    analyzed_by_model: Some(vision_model),
                    artifact_id: None,
                    vision_supported: true,
                }
            }
            Err(e) => VisualAnalysisResult {
                summary: format!("Visual analysis failed: {}", e),
                key_regions: vec![],
                visible_defects: vec![],
                discrepancies: vec![],
                confidence: "unverified".to_string(),
                supporting_evidence: "".to_string(),
                uncertainty: format!("Inference error: {}", e),
                analyzed_by_model: Some(vision_model),
                artifact_id: None,
                vision_supported: false,
            },
        }
    }
}

fn extract_json_block(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(start) = trimmed.find("```json") {
        let rest = &trimmed[start + 7..];
        if let Some(end) = rest.find("```") {
            return rest[..end].trim().to_string();
        }
    } else if let Some(start) = trimmed.find("```") {
        let rest = &trimmed[start + 3..];
        if let Some(end) = rest.find("```") {
            return rest[..end].trim().to_string();
        }
    }
    if let (Some(s), Some(e)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if s <= e {
            return trimmed[s..=e].to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_from_markdown_blocks() {
        let block = "Here is the analysis:\n```json\n{\"summary\": \"Clean layout\", \"confidence\": \"reproduced\"}\n```\nDone.";
        assert_eq!(
            extract_json_block(block),
            "{\"summary\": \"Clean layout\", \"confidence\": \"reproduced\"}"
        );

        let raw = "Some prefix {\"summary\": \"Inline\"} suffix";
        assert_eq!(extract_json_block(raw), "{\"summary\": \"Inline\"}");
    }

    #[test]
    fn resolve_vision_model_honors_primary_and_fallback() {
        let router = Arc::new(ModelRouter::new());
        let analyzer = VisualAnalyzer::new(router, "test-key".into(), None);

        // Primary is already a vision model
        assert_eq!(
            analyzer.resolve_vision_model("gemini-1.5-pro"),
            Some("gemini-1.5-pro".to_string())
        );

        // Primary is text-only -> falls back to DEFAULT_VISION_MODEL
        assert_eq!(
            analyzer.resolve_vision_model("deepseek-coder"),
            Some(DEFAULT_VISION_MODEL.to_string())
        );

        // Configured vision model overrides default
        let analyzer_custom =
            VisualAnalyzer::new(Arc::new(ModelRouter::new()), "test-key".into(), None)
                .with_configured_vision_model(Some("gpt-4o-mini".to_string()));
        assert_eq!(
            analyzer_custom.resolve_vision_model("deepseek-coder"),
            Some("gpt-4o-mini".to_string())
        );
    }

    #[tokio::test]
    async fn honest_reporting_when_no_api_key_or_vision_available() {
        let router = Arc::new(ModelRouter::new());
        // Empty API key
        let analyzer = VisualAnalyzer::new(router, "".into(), None);
        let ctx = VisualAnalysisContext::default();

        let res = analyzer
            .analyze_screenshot("fake_base64", "deepseek-coder", &ctx)
            .await;
        assert!(!res.vision_supported);
        assert_eq!(res.confidence, "unverified");
        assert!(res.visible_defects.is_empty());
        assert!(res.uncertainty.contains("not analyzed by a vision model"));
    }

    #[test]
    fn visual_analysis_result_serialization() {
        let res = VisualAnalysisResult {
            summary: "Layout has overlapping button".to_string(),
            key_regions: vec![VisualRegion {
                name: "Header".to_string(),
                bounds: Some("top".to_string()),
                dom_ref: Some("e1".to_string()),
                description: "Navigation header".to_string(),
            }],
            visible_defects: vec![VisualDefect {
                defect_type: "overlap".to_string(),
                description: "Submit button overlaps footer".to_string(),
                location: Some("bottom-right".to_string()),
                severity: "high".to_string(),
            }],
            discrepancies: vec![VisualDiscrepancy {
                dom_reference: Some("e2".to_string()),
                description: "Button styled as disabled but DOM is enabled".to_string(),
            }],
            confidence: "strong_evidence".to_string(),
            supporting_evidence: "Observed bounding box overlap in screenshot".to_string(),
            uncertainty: "Font rendering may differ slightly on Retina displays".to_string(),
            analyzed_by_model: Some("gemini-1.5-pro".to_string()),
            artifact_id: Some("art_123".to_string()),
            vision_supported: true,
        };

        let json = serde_json::to_string(&res).expect("Must serialize");
        let deserialized: VisualAnalysisResult =
            serde_json::from_str(&json).expect("Must deserialize");
        assert_eq!(deserialized.visible_defects.len(), 1);
        assert_eq!(deserialized.visible_defects[0].defect_type, "overlap");
        assert_eq!(deserialized.discrepancies.len(), 1);
        assert!(deserialized.vision_supported);
    }
}
