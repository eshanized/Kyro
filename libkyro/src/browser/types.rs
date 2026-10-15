//! Structured types returned by the browser automation subsystem.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A semantic element observed on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageElement {
    /// Stable reference for the lifetime of one snapshot.
    #[serde(alias = "ref")]
    pub element_ref: String,
    /// The ARIA-style role of the element.
    pub role: String,
    /// The accessible name of the element.
    pub name: String,
    /// The HTML tag name.
    pub tag: String,
    /// The input type, when the element is an input.
    pub input_type: Option<String>,
    /// The current value, for editable elements.
    pub value: Option<String>,
    pub disabled: bool,
    pub checked: bool,
    /// The href of a link.
    pub href: Option<String>,
    pub placeholder: Option<String>,
    /// The visible text of the element.
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// `true` when the element is an interactive control.
    pub interactive: bool,
    /// The heading level, for headings.
    pub heading_level: Option<u8>,
    /// `true` when element is within current viewport.
    #[serde(default)]
    pub in_viewport: bool,
    /// `true` when element is partially or fully clipped by viewport bounds.
    #[serde(default)]
    pub clipped: bool,
    /// Associated labels.
    #[serde(default)]
    pub labels: Vec<String>,
}

/// A heading observed on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageHeading {
    #[serde(alias = "ref")]
    pub element_ref: String,
    pub level: u8,
    pub text: String,
}

/// A link observed on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLink {
    #[serde(alias = "ref")]
    pub element_ref: String,
    pub href: String,
    pub text: String,
}

/// A landmark region observed on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLandmark {
    #[serde(alias = "ref")]
    pub element_ref: String,
    pub role: String,
    pub tag: String,
    pub name: String,
}

/// A dialog or modal observed on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageDialog {
    #[serde(alias = "ref")]
    pub element_ref: String,
    pub role: String,
    pub name: String,
    pub open: bool,
}

/// A menu observed on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMenu {
    #[serde(alias = "ref")]
    pub element_ref: String,
    pub role: String,
    pub name: String,
}

/// Computed bounding box.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub in_viewport: bool,
    #[serde(default)]
    pub clipped: bool,
}

/// DOM context for element inspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomContext {
    pub tag: String,
    pub outer_html_snippet: String,
    pub parent_tag: Option<String>,
    pub parent_class: Option<String>,
    pub children_count: usize,
}

/// Computed styles for element inspection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComputedStyles {
    pub display: Option<String>,
    pub visibility: Option<String>,
    pub opacity: Option<String>,
    pub position: Option<String>,
    pub z_index: Option<String>,
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub font_size: Option<String>,
    pub box_sizing: Option<String>,
    pub width: Option<String>,
    pub height: Option<String>,
    pub overflow: Option<String>,
}

/// Detailed inspection outcome for a selected element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementInspectionResult {
    pub element_ref: Option<String>,
    pub tag: String,
    pub role: String,
    pub name: String,
    pub dom_context: DomContext,
    pub attributes: HashMap<String, String>,
    pub computed_styles: ComputedStyles,
    pub bounding_box: BoundingBox,
    pub interactable: bool,
    pub disabled: bool,
    pub cropped_screenshot: Option<String>,
}

/// A console message observed on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleEntry {
    pub level: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub text: String,
    pub stack: Option<serde_json::Value>,
    pub source: Option<String>,
    pub timestamp: i64,
}

/// A network request observed on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRequest {
    pub request_id: String,
    pub url: String,
    pub method: Option<String>,
    pub status: Option<u16>,
    pub mime_type: Option<String>,
    pub error: Option<String>,
    pub navigation: bool,
}

/// A failed network request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedRequest {
    pub request_id: String,
    pub url: String,
    pub method: Option<String>,
    pub error: String,
    pub navigation: bool,
    pub timestamp: i64,
}

/// A semantic locator for an element.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ElementTarget {
    /// Direct element reference from current snapshot (e.g. 'e1').
    #[serde(alias = "ref", alias = "element_ref")]
    pub ref_id: Option<String>,
    /// The ARIA role, e.g. `button` or `link`.
    pub role: Option<String>,
    /// A substring of the accessible name.
    pub name: Option<String>,
    /// A substring of the visible text.
    pub text: Option<String>,
    /// The HTML tag.
    pub tag: Option<String>,
    /// A `data-testid` value.
    #[serde(alias = "testId", alias = "data-testid")]
    pub test_id: Option<String>,
    /// A CSS selector (used only when nothing else matches).
    pub css: Option<String>,
    /// The index of the match when several elements match.
    pub nth: Option<usize>,
    /// Target X coordinate in viewport space.
    pub x: Option<i32>,
    /// Target Y coordinate in viewport space.
    pub y: Option<i32>,
    /// Snapshot token to guard against stale element actions.
    pub snapshot_id: Option<String>,
}

impl ElementTarget {
    /// Validates the target and converts it to the worker payload.
    pub fn to_params(&self) -> Result<serde_json::Value, String> {
        if self.ref_id.is_none()
            && self.role.is_none()
            && self.name.is_none()
            && self.text.is_none()
            && self.tag.is_none()
            && self.test_id.is_none()
            && self.css.is_none()
            && (self.x.is_none() || self.y.is_none())
        {
            return Err(
                "The element target must specify at least one of ref, role, name, text, tag, testId, css, or coordinates (x, y)"
                    .to_string(),
            );
        }
        let mut map = serde_json::Map::new();
        if let Some(v) = &self.ref_id {
            map.insert("ref".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &self.role {
            map.insert("role".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &self.name {
            map.insert("name".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &self.text {
            map.insert("text".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &self.tag {
            map.insert("tag".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &self.test_id {
            map.insert("testId".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &self.css {
            map.insert("css".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = self.nth {
            map.insert("nth".into(), serde_json::Value::from(v));
        }
        if let Some(x) = self.x {
            map.insert("x".into(), serde_json::Value::from(x));
        }
        if let Some(y) = self.y {
            map.insert("y".into(), serde_json::Value::from(y));
        }
        if let Some(v) = &self.snapshot_id {
            map.insert("snapshot_id".into(), serde_json::Value::String(v.clone()));
        }
        Ok(serde_json::Value::Object(map))
    }
}

/// The result of a page snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSnapshot {
    pub url: String,
    pub title: String,
    pub ready_state: String,
    pub elements: Vec<PageElement>,
    pub headings: Vec<PageHeading>,
    pub links: Vec<PageLink>,
    pub forms: Vec<serde_json::Value>,
    pub anchors: Vec<serde_json::Value>,
    pub images: Vec<serde_json::Value>,
    pub scripts: Vec<String>,
    pub viewport: Viewport,
    pub scroll: serde_json::Value,
    pub overflow_x: bool,
    #[serde(default)]
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub timestamp: Option<i64>,
    #[serde(default)]
    pub landmarks: Vec<PageLandmark>,
    #[serde(default)]
    pub dialogs: Vec<PageDialog>,
    #[serde(default)]
    pub menus: Vec<PageMenu>,
    #[serde(default)]
    pub failed_images: Vec<serde_json::Value>,
    #[serde(default)]
    pub unnamed_interactive: Vec<serde_json::Value>,
    #[serde(default)]
    pub page_dimensions: Option<serde_json::Value>,
    #[serde(default)]
    pub has_closed_shadow_roots: bool,
    #[serde(default)]
    pub cross_origin_frames: Vec<String>,
    #[serde(default)]
    pub console_errors: Vec<ConsoleEntry>,
    #[serde(default)]
    pub failed_requests: Vec<FailedRequest>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

/// A tab in the browser session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub context_id: String,
    pub url: String,
    pub title: Option<String>,
    pub parent: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_target_is_rejected() {
        let target = ElementTarget::default();
        assert!(target.to_params().is_err());
    }

    #[test]
    fn a_target_with_a_role_is_accepted() {
        let target = ElementTarget {
            role: Some("button".into()),
            name: Some("Save".into()),
            ..Default::default()
        };
        let params = target.to_params().unwrap();
        assert_eq!(params["role"], "button");
        assert_eq!(params["name"], "Save");
    }

    #[test]
    fn a_target_with_only_a_css_selector_is_accepted() {
        let target = ElementTarget {
            css: Some("#submit".into()),
            ..Default::default()
        };
        assert!(target.to_params().is_ok());
    }

    #[test]
    fn a_target_with_ref_is_accepted() {
        let target = ElementTarget {
            ref_id: Some("e42".into()),
            ..Default::default()
        };
        let params = target.to_params().unwrap();
        assert_eq!(params["ref"], "e42");
    }

    #[test]
    fn a_target_with_coordinates_is_accepted() {
        let target = ElementTarget {
            x: Some(150),
            y: Some(300),
            ..Default::default()
        };
        let params = target.to_params().unwrap();
        assert_eq!(params["x"], 150);
        assert_eq!(params["y"], 300);
    }

    #[test]
    fn a_target_with_test_id_and_snapshot_is_accepted() {
        let target = ElementTarget {
            test_id: Some("login-button".into()),
            snapshot_id: Some("snap_12345".into()),
            ..Default::default()
        };
        let params = target.to_params().unwrap();
        assert_eq!(params["testId"], "login-button");
        assert_eq!(params["snapshot_id"], "snap_12345");
    }

    #[test]
    fn page_snapshot_deserialization_with_rich_structure() {
        let json_data = serde_json::json!({
            "url": "http://localhost:3000/dashboard",
            "title": "Kyro Dashboard",
            "ready_state": "complete",
            "snapshot_id": "snap_abc123",
            "timestamp": 1729500000000i64,
            "viewport": {
                "width": 1440,
                "height": 900
            },
            "scroll": { "x": 0, "y": 0 },
            "overflow_x": false,
            "elements": [
                {
                    "ref": "e1",
                    "role": "button",
                    "name": "Submit Form",
                    "tag": "button",
                    "text": "Submit",
                    "x": 100,
                    "y": 200,
                    "width": 120,
                    "height": 40,
                    "disabled": false,
                    "checked": false,
                    "interactive": true,
                    "in_viewport": true,
                    "clipped": false
                }
            ],
            "headings": [],
            "links": [],
            "forms": [],
            "anchors": [],
            "images": [],
            "scripts": [],
            "landmarks": [
                {
                    "ref": "e0",
                    "role": "navigation",
                    "name": "Main Menu",
                    "tag": "nav"
                }
            ],
            "dialogs": [],
            "menus": [],
            "failed_images": ["broken.png"],
            "unnamed_interactive": [],
            "has_closed_shadow_roots": true,
            "cross_origin_frames": ["https://checkout.example.com"],
            "console_errors": [
                {
                    "level": "error",
                    "type": "error",
                    "text": "ReferenceError: foo is not defined",
                    "timestamp": 1729500001000i64
                }
            ],
            "failed_requests": [
                {
                    "request_id": "r1",
                    "url": "http://localhost:3000/api/missing",
                    "error": "net::ERR_FAILED",
                    "navigation": false,
                    "timestamp": 1729500002000i64
                }
            ]
        });

        let snapshot: PageSnapshot = serde_json::from_value(json_data).expect("Must deserialize");
        assert_eq!(snapshot.snapshot_id, Some("snap_abc123".to_string()));
        assert_eq!(snapshot.viewport.width, 1440);
        assert_eq!(snapshot.elements.len(), 1);
        assert!(snapshot.elements[0].in_viewport);
        assert!(snapshot.elements[0].interactive);
        assert_eq!(snapshot.elements[0].x, 100);
        assert_eq!(snapshot.landmarks.len(), 1);
        assert_eq!(snapshot.landmarks[0].role, "navigation");
        assert!(snapshot.has_closed_shadow_roots);
        assert_eq!(snapshot.cross_origin_frames.len(), 1);
        assert_eq!(snapshot.console_errors.len(), 1);
        assert_eq!(snapshot.failed_requests.len(), 1);
    }

    #[test]
    fn element_inspection_result_deserialization() {
        let json_data = serde_json::json!({
            "element_ref": "e1",
            "tag": "button",
            "role": "button",
            "name": "Submit",
            "dom_context": {
                "tag": "button",
                "outer_html_snippet": "<button class=\"btn btn-primary\" id=\"submit-btn\">Submit</button>",
                "parent_tag": "div",
                "parent_class": "form-group",
                "children_count": 0
            },
            "attributes": {
                "type": "submit",
                "class": "btn btn-primary"
            },
            "computed_styles": {
                "display": "block",
                "visibility": "visible",
                "opacity": "1",
                "position": "relative",
                "z_index": "auto",
                "color": "rgb(255, 255, 255)",
                "background_color": "rgb(0, 102, 204)"
            },
            "bounding_box": {
                "x": 50,
                "y": 100,
                "width": 200,
                "height": 45,
                "in_viewport": true,
                "clipped": false
            },
            "interactable": true,
            "disabled": false,
            "cropped_screenshot": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
        });

        let inspection: ElementInspectionResult = serde_json::from_value(json_data).expect("Must deserialize");
        assert!(inspection.interactable);
        assert_eq!(inspection.dom_context.tag, "button");
        assert_eq!(inspection.computed_styles.display.as_deref(), Some("block"));
        assert_eq!(inspection.bounding_box.width, 200);
        assert!(inspection.cropped_screenshot.is_some());
    }
}
