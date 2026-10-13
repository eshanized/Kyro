//! Navigation safety rules for browser automation.
//!
//! Browser automation executes untrusted web content, so the set of
//! destinations that may be reached is decided here, in one place, and is
//! enforced for every navigation.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UrlRejection {
    /// The URL could not be parsed at all.
    Malformed(String),
    /// The URL scheme is not permitted.
    SchemeNotAllowed {
        scheme: String,
        allowed: Vec<String>,
    },
    /// The host is on the blocked-domain list.
    BlockedDomain { host: String, domain: String },
    /// The destination is outside the origin the task is scoped to.
    OutsideOriginScope { current: String, requested: String },
    /// The URL is syntactically valid but has no usable host.
    MissingHost(String),
}

impl std::fmt::Display for UrlRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UrlRejection::Malformed(u) => write!(f, "'{}' is not a valid URL", u),
            UrlRejection::SchemeNotAllowed { scheme, allowed } => write!(
                f,
                "URL scheme '{}' is not permitted for browser automation (allowed: {})",
                scheme,
                allowed.join(", ")
            ),
            UrlRejection::BlockedDomain { host, domain } => {
                write!(f, "'{}' targets the blocked domain '{}'", host, domain)
            }
            UrlRejection::OutsideOriginScope { current, requested } => write!(
                f,
                "Navigation to '{}' is outside the origin scope '{}' of this browser session. \
Request explicit authorization before leaving the application origin.",
                requested, current
            ),
            UrlRejection::MissingHost(u) => {
                write!(f, "'{}' does not name a host that can be reached", u)
            }
        }
    }
}

impl std::error::Error for UrlRejection {}

/// The navigation rules applied to a browser session.
#[derive(Debug, Clone)]
pub struct NavigationPolicy {
    /// URL schemes that may be navigated to.
    pub allowed_schemes: Vec<String>,
    /// Hosts that may never be reached.
    pub blocked_domains: Vec<String>,
    /// When set, only this origin (scheme://host:port) may be navigated to
    /// unless an external check is explicitly authorised.
    pub origin_scope: Option<String>,
    /// Whether `file://` URLs are allowed.
    pub allow_file: bool,
    /// Whether `data:` URLs are allowed.
    pub allow_data: bool,
}

impl Default for NavigationPolicy {
    fn default() -> Self {
        Self {
            allowed_schemes: vec!["http".into(), "https".into()],
            blocked_domains: Vec::new(),
            origin_scope: None,
            allow_file: false,
            allow_data: false,
        }
    }
}

impl NavigationPolicy {
    pub fn with_blocked_domains(mut self, domains: &[String]) -> Self {
        self.blocked_domains = domains.to_vec();
        self
    }

    pub fn with_origin_scope(mut self, origin: Option<String>) -> Self {
        self.origin_scope = origin;
        self
    }

    /// Validates a URL against the policy.
    pub fn check(&self, url: &str) -> Result<ValidatedUrl, UrlRejection> {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(UrlRejection::Malformed(url.to_string()));
        }
        if trimmed.contains('\n') || trimmed.contains('\r') || trimmed.contains('\0') {
            return Err(UrlRejection::Malformed(url.to_string()));
        }

        // `data:` and `file:` are handled explicitly because their URL
        // parsing rules differ.
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("data:") {
            if !self.allow_data {
                return Err(UrlRejection::SchemeNotAllowed {
                    scheme: "data".into(),
                    allowed: self.allowed_schemes.clone(),
                });
            }
            return Ok(ValidatedUrl {
                url: trimmed.to_string(),
                scheme: "data".into(),
                host: None,
                origin: None,
            });
        }
        if lower.starts_with("file:") {
            if !self.allow_file {
                return Err(UrlRejection::SchemeNotAllowed {
                    scheme: "file".into(),
                    allowed: self.allowed_schemes.clone(),
                });
            }
            return Ok(ValidatedUrl {
                url: trimmed.to_string(),
                scheme: "file".into(),
                host: None,
                origin: None,
            });
        }

        // Reject schemes that are never permitted, including the ones that
        // would let page content run code in the automation context.
        for forbidden in [
            "javascript:",
            "blob:",
            "chrome:",
            "resource:",
            "moz-extension:",
            "view-source:",
            "jar:",
            "vbscript:",
            "about:",
        ] {
            if lower.starts_with(forbidden) {
                return Err(UrlRejection::SchemeNotAllowed {
                    scheme: forbidden.trim_end_matches(':').to_string(),
                    allowed: self.allowed_schemes.clone(),
                });
            }
        }

        let parsed =
            url::Url::parse(trimmed).map_err(|_| UrlRejection::Malformed(url.to_string()))?;
        let scheme = parsed.scheme().to_ascii_lowercase();
        if !self.allowed_schemes.contains(&scheme) {
            return Err(UrlRejection::SchemeNotAllowed {
                scheme,
                allowed: self.allowed_schemes.clone(),
            });
        }

        let host = parsed.host_str().map(|h| h.to_ascii_lowercase());
        let host = match host {
            Some(h) if !h.is_empty() => h,
            _ => return Err(UrlRejection::MissingHost(url.to_string())),
        };

        for domain in &self.blocked_domains {
            let d = domain
                .trim()
                .trim_start_matches('*')
                .trim_start_matches('.')
                .to_ascii_lowercase();
            if d.is_empty() {
                continue;
            }
            if host == d || host.ends_with(&format!(".{}", d)) {
                return Err(UrlRejection::BlockedDomain {
                    host: host.clone(),
                    domain: d,
                });
            }
        }

        let origin = parsed.origin().ascii_serialization();

        if let Some(scope) = &self.origin_scope {
            if !scope.is_empty() && !origins_match(scope, &origin) {
                return Err(UrlRejection::OutsideOriginScope {
                    current: scope.clone(),
                    requested: origin,
                });
            }
        }

        Ok(ValidatedUrl {
            url: trimmed.to_string(),
            scheme,
            host: Some(host),
            origin: Some(origin),
        })
    }
}

/// A URL that passed validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatedUrl {
    pub url: String,
    pub scheme: String,
    pub host: Option<String>,
    pub origin: Option<String>,
}

/// Compares two origins, ignoring a default port difference.
pub fn origins_match(a: &str, b: &str) -> bool {
    let norm = |o: &str| {
        let mut s = o.trim_end_matches('/').to_ascii_lowercase();
        if s.ends_with(":80") && s.starts_with("http://") {
            s.truncate(s.len() - 3);
        } else if s.ends_with(":443") && s.starts_with("https://") {
            s.truncate(s.len() - 4);
        }
        s
    };
    norm(a) == norm(b)
}

/// Returns the origin of a URL, when it has one.
pub fn origin_of(url: &str) -> Option<String> {
    url::Url::parse(url.trim())
        .ok()
        .map(|p| p.origin().ascii_serialization())
        .filter(|o| !o.is_empty() && o != "null")
}

/// Extracts the set of hosts that appear in a list of URLs.
pub fn hosts_of(urls: &[String]) -> HashSet<String> {
    urls.iter()
        .filter_map(|u| url::Url::parse(u).ok())
        .filter_map(|p| p.host_str().map(|h| h.to_ascii_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> NavigationPolicy {
        NavigationPolicy::default().with_blocked_domains(&["paypal.com".to_string()])
    }

    #[test]
    fn allows_http_and_https() {
        assert!(policy().check("http://localhost:3000").is_ok());
        assert!(policy().check("https://example.com/page").is_ok());
    }

    #[test]
    fn rejects_javascript_scheme() {
        let err = policy().check("javascript:alert(1)").unwrap_err();
        assert!(matches!(err, UrlRejection::SchemeNotAllowed { .. }));
    }

    #[test]
    fn rejects_data_and_file_by_default() {
        assert!(policy().check("data:text/html,<h1>x</h1>").is_err());
        assert!(policy().check("file:///etc/passwd").is_err());
    }

    #[test]
    fn rejects_blocked_domain_and_subdomains() {
        let p = policy();
        assert!(matches!(
            p.check("https://paypal.com/checkout").unwrap_err(),
            UrlRejection::BlockedDomain { .. }
        ));
        assert!(matches!(
            p.check("https://www.paypal.com/").unwrap_err(),
            UrlRejection::BlockedDomain { .. }
        ));
    }

    #[test]
    fn allows_similar_but_different_domain() {
        assert!(policy().check("https://paypal-not-really.com/").is_ok());
    }

    #[test]
    fn enforces_origin_scope() {
        let p = NavigationPolicy::default().with_origin_scope(Some("http://localhost:3000".into()));
        assert!(p.check("http://localhost:3000/dashboard").is_ok());
        let err = p.check("https://example.com/").unwrap_err();
        assert!(matches!(err, UrlRejection::OutsideOriginScope { .. }));
    }

    #[test]
    fn origin_scope_ignores_default_port() {
        let p = NavigationPolicy::default().with_origin_scope(Some("http://localhost:80".into()));
        assert!(p.check("http://localhost/").is_ok());
    }

    #[test]
    fn rejects_malformed_url() {
        assert!(matches!(
            policy().check("not a url").unwrap_err(),
            UrlRejection::Malformed(_)
        ));
    }

    #[test]
    fn rejects_newline_injection() {
        assert!(policy().check("http://example.com/\nrm -rf /").is_err());
    }
}
