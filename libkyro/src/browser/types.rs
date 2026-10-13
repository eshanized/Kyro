//! Structured types returned by the browser automation subsystem.

use serde::{Deserialize, Serialize};

/// A semantic element observed on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageElement {
    /// Stable reference for the lifetime of one snapshot.
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
}

/// A heading observed on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageHeading {
    pub element_ref: String,
    pub level: u8,
    pub text: String,
}

/// A link observed on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLink {
    pub element_ref: String,
    pub href: String,
    pub text: String,
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
    /// The ARIA role, e.g. `button` or `link`.
    pub role: Option<String>,
    /// A substring of the accessible name.
    pub name: Option<String>,
    /// A substring of the visible text.
    pub text: Option<String>,
    /// The HTML tag.
    pub tag: Option<String>,
    /// A `data-testid` value.
    pub test_id: Option<String>,
    /// A CSS selector (used only when nothing else matches).
    pub css: Option<String>,
    /// The index of the match when several elements match.
    pub nth: Option<usize>,
}

impl ElementTarget {
    /// Validates the target and converts it to the worker payload.
    pub fn to_params(&self) -> Result<serde_json::Value, String> {
        if self.role.is_none()
            && self.name.is_none()
            && self.text.is_none()
            && self.tag.is_none()
            && self.test_id.is_none()
            && self.css.is_none()
        {
            return Err(
                "The element target must specify at least one of role, name, text, tag, testId or css"
                    .to_string(),
            );
        }
        let mut map = serde_json::Map::new();
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
}
