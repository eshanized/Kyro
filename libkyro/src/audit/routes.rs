//! Route discovery.
//!
//! Routes are discovered from *independent* sources so that a project is not
//! judged by a single mechanism:
//!
//! * framework route files (Next.js App Router and Pages Router, and the
//!   common `src/routes` / `src/pages` conventions),
//! * router configuration in the source,
//! * the links that the running application actually renders.
//!
//! Dynamic routes are recorded but never visited with invented identifiers,
//! and routes that require authentication or fixture data are reported as
//! blocked rather than silently skipped.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteKind {
    /// A concrete route.
    Static,
    /// A route with a parameter, e.g. `/blog/[slug]`.
    Dynamic,
    /// A catch-all route, e.g. `/docs/[...slug]`.
    CatchAll,
    /// An API route handler.
    Api,
}

impl RouteKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RouteKind::Static => "static",
            RouteKind::Dynamic => "dynamic",
            RouteKind::CatchAll => "catch-all",
            RouteKind::Api => "api",
        }
    }
}

/// The source a route was discovered from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteSource {
    RouteFile,
    RouterConfig,
    RenderedLink,
    Manifest,
}

impl RouteSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            RouteSource::RouteFile => "route_file",
            RouteSource::RouterConfig => "router_config",
            RouteSource::RenderedLink => "rendered_link",
            RouteSource::Manifest => "manifest",
        }
    }
}

/// A discovered route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredRoute {
    /// The normalised route path.
    pub path: String,
    /// The kind of route.
    pub kind: RouteKind,
    /// Where the route was found.
    pub sources: Vec<RouteSource>,
    /// The file the route was found in, when applicable.
    pub file: Option<PathBuf>,
    /// `true` when the route can be visited without inventing data.
    pub visitable: bool,
    /// Why the route cannot be visited, when it cannot.
    pub blocked_reason: Option<String>,
}

impl DiscoveredRoute {
    pub fn new(path: String, kind: RouteKind, source: RouteSource) -> Self {
        let (visitable, blocked_reason) = match kind {
            RouteKind::Static => (true, None),
            RouteKind::Api => (
                false,
                Some("API route handler, not a user interface".into()),
            ),
            RouteKind::Dynamic => (
                false,
                Some("dynamic route requires a concrete identifier that was not supplied".into()),
            ),
            RouteKind::CatchAll => (
                false,
                Some("catch-all route requires a concrete path that was not supplied".into()),
            ),
        };
        Self {
            path,
            kind,
            sources: vec![source],
            file: None,
            visitable,
            blocked_reason,
        }
    }
}

/// The outcome of route discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteInventory {
    /// Every route that was discovered.
    pub routes: Vec<DiscoveredRoute>,
    /// The number of routes that can be visited.
    pub visitable: usize,
    /// The number of routes that cannot be visited without more information.
    pub blocked: usize,
    /// A short explanation of what was inspected.
    pub summary: String,
}

/// Normalises a route path.
///
/// Ensures a leading slash, removes duplicated and trailing slashes and
/// removes the query string and fragment.
pub fn normalise_route(path: &str) -> String {
    let mut p = path.trim().to_string();
    if let Some(idx) = p.find('?') {
        p.truncate(idx);
    }
    if let Some(idx) = p.find('#') {
        p.truncate(idx);
    }
    if p.is_empty() {
        return "/".to_string();
    }
    if !p.starts_with('/') {
        p = format!("/{}", p);
    }
    while p.contains("//") {
        p = p.replace("//", "/");
    }
    if p.len() > 1 {
        while p.ends_with('/') {
            p.pop();
        }
    }
    p
}

/// Derives the route path of a framework route file.
pub fn route_from_file(path: &Path, project_root: &Path) -> Option<DiscoveredRoute> {
    let file_path = path.to_path_buf();
    let relative = path.strip_prefix(project_root).ok()?;
    let components: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();

    let file_name = components.last()?.clone();
    let stem = file_name
        .rsplit_once('.')
        .map(|(s, _)| s.to_string())
        .unwrap_or(file_name.clone());

    let mut segments: Vec<String> = Vec::new();
    let mut kind = RouteKind::Static;
    let mut is_api = false;

    // Determine the framework convention from the directory layout.
    let dir_components = &components[..components.len().saturating_sub(1)];

    let is_app_router = dir_components.iter().any(|c| c == "app");
    let is_pages_router = dir_components.iter().any(|c| c == "pages");

    if is_app_router {
        // `app/blog/[slug]/page.tsx` -> /blog/[slug]
        for comp in dir_components.iter() {
            if comp == "app" {
                continue;
            }
            if comp.starts_with('(') && comp.ends_with(')') {
                // Route groups do not appear in the URL.
                continue;
            }
            if comp.starts_with('@') {
                // Parallel routes.
                continue;
            }
            segments.push(interpret_segment(comp, &mut kind));
        }
        if stem == "page" {
            // The page file itself carries no segment.
        } else if stem == "route" {
            is_api = true;
        } else {
            segments.push(stem.clone());
        }
    } else if is_pages_router {
        for (i, comp) in dir_components.iter().enumerate() {
            if comp == "pages" {
                continue;
            }
            if comp.starts_with('[') && comp.ends_with(']') {
                // Grouped dynamic routes.
                continue;
            }
            let _ = i;
            segments.push(interpret_segment(comp, &mut kind));
        }
        let name = if stem == "index" {
            String::new()
        } else {
            stem.clone()
        };
        if !name.is_empty() {
            segments.push(interpret_segment(&name, &mut kind));
        }
    } else {
        // `src/routes/about.tsx` style conventions.
        for comp in dir_components.iter() {
            if comp == "src" || comp == "routes" || comp == "app" || comp == "pages" {
                continue;
            }
            segments.push(interpret_segment(comp, &mut kind));
        }
        if stem != "index" {
            segments.push(interpret_segment(&stem, &mut kind));
        }
    }

    let path = if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    };

    let mut route = DiscoveredRoute::new(
        path,
        if is_api { RouteKind::Api } else { kind },
        RouteSource::RouteFile,
    );
    route.file = Some(file_path.to_path_buf());
    Some(route)
}

/// Interprets a single path segment, recognising dynamic segments.
fn interpret_segment(segment: &str, kind: &mut RouteKind) -> String {
    if segment.starts_with("[...") && segment.ends_with(']') {
        *kind = RouteKind::CatchAll;
        return segment.to_string();
    }
    if segment.starts_with('[') && segment.ends_with(']') {
        if *kind == RouteKind::Static {
            *kind = RouteKind::Dynamic;
        }
        return segment.to_string();
    }
    segment.to_string()
}

/// The file extensions that can hold a route definition.
const ROUTE_EXTENSIONS: &[&str] = &["tsx", "ts", "jsx", "js", "mjs", "mts"];

/// Directories that are never scanned for routes.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".next",
    "dist",
    "build",
    "out",
    "coverage",
    ".turbo",
    ".cache",
    "vendor",
    "target",
];

/// Scans a project for framework route files.
pub fn discover_route_files(project_root: &Path, max_entries: usize) -> Vec<DiscoveredRoute> {
    let mut routes: Vec<DiscoveredRoute> = Vec::new();
    let mut stack = vec![project_root.to_path_buf()];
    let mut visited = 0usize;

    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > max_entries {
                return routes;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            if !ROUTE_EXTENSIONS.contains(&ext.as_str()) {
                continue;
            }
            let file_name = name.to_lowercase();
            let is_page = file_name == "page.tsx"
                || file_name == "page.ts"
                || file_name == "page.jsx"
                || file_name == "page.js";
            let is_api_route = file_name == "route.tsx"
                || file_name == "route.ts"
                || file_name == "route.jsx"
                || file_name == "route.js";
            let looks_like_page = is_page
                || is_api_route
                || (dir_is_route_dir(&path)
                    && file_name != "index.js"
                    && !file_name.ends_with("test.ts"));

            if !looks_like_page {
                continue;
            }
            if let Some(route) = route_from_file(&path, project_root) {
                routes.push(route);
            }
        }
    }
    routes
}

fn dir_is_route_dir(path: &Path) -> bool {
    path.components().any(|c| {
        let n = c.as_os_str().to_string_lossy().to_string();
        n == "routes" || n == "app" || n == "pages"
    })
}

/// Extracts route paths from router configuration in the source.
pub fn discover_router_config(project_root: &Path, max_files: usize) -> Vec<DiscoveredRoute> {
    let mut routes: Vec<DiscoveredRoute> = Vec::new();
    let mut stack = vec![project_root.to_path_buf()];
    let mut visited = 0usize;

    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > max_files {
                return routes;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            if !ROUTE_EXTENSIONS.contains(&ext.as_str()) {
                continue;
            }
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(_) => continue,
            };
            for route in extract_routes_from_source(&text, &path) {
                routes.push(route);
            }
        }
    }
    routes
}

/// Extracts route paths from a source file.
///
/// Only string literals that appear in a route-like position are considered, so
/// that an arbitrary string in the code is not mistaken for a route.
pub fn extract_routes_from_source(text: &str, file: &Path) -> Vec<DiscoveredRoute> {
    let mut routes: Vec<DiscoveredRoute> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    // `path: "/about"` / `path='/about'` / `path="/about"`
    let patterns = [
        "path: \"", "path: '", "path=\"", "path='", "to=\"", "to='", "href: \"", "href: '",
    ];

    for pattern in patterns {
        let mut idx = 0usize;
        while let Some(rel) = text[idx..].find(pattern) {
            let start = idx + rel + pattern.len();
            let quote = if pattern.ends_with('"') { '"' } else { '\'' };
            let rest = &text[start..];
            let end = match rest.find(quote) {
                Some(e) => e,
                None => break,
            };
            let literal = &rest[..end];
            idx = start + end;

            if literal.is_empty() || literal.contains(' ') {
                continue;
            }
            if !literal.starts_with('/') && !literal.contains('/') {
                // A single word is not a route.
                continue;
            }
            if literal.contains("${") || literal.contains("`") {
                continue;
            }
            let normalised = normalise_route(literal);
            if normalised == "/" && !seen.insert(normalised.clone()) {
                continue;
            }
            if !seen.insert(normalised.clone()) {
                continue;
            }
            let mut route =
                DiscoveredRoute::new(normalised, RouteKind::Static, RouteSource::RouterConfig);
            route.file = Some(file.to_path_buf());
            routes.push(route);
        }
    }

    routes
}

/// Merges route sources, de-duplicating by path.
pub fn merge_routes(groups: Vec<Vec<DiscoveredRoute>>) -> RouteInventory {
    let mut merged: Vec<DiscoveredRoute> = Vec::new();

    for group in groups {
        for route in group {
            match merged.iter_mut().find(|r| r.path == route.path) {
                Some(existing) => {
                    for source in &route.sources {
                        if !existing.sources.contains(source) {
                            existing.sources.push(*source);
                        }
                    }
                    if existing.file.is_none() {
                        existing.file = route.file.clone();
                    }
                    // A route that is static in one place wins over a dynamic
                    // classification from another.
                    if existing.kind != RouteKind::Static && route.kind == RouteKind::Static {
                        existing.kind = RouteKind::Static;
                        existing.visitable = true;
                        existing.blocked_reason = None;
                    }
                }
                None => merged.push(route),
            }
        }
    }

    merged.sort_by(|a, b| {
        a.visitable
            .cmp(&b.visitable)
            .then(
                b.path
                    .matches('/')
                    .count()
                    .cmp(&a.path.matches('/').count()),
            )
            .then(a.path.cmp(&b.path))
    });

    let visitable = merged.iter().filter(|r| r.visitable).count();
    let blocked = merged.len() - visitable;

    RouteInventory {
        summary: format!(
            "Discovered {} route(s): {} visitable, {} blocked.",
            merged.len(),
            visitable,
            blocked
        ),
        routes: merged,
        visitable,
        blocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "kyro_routes_{}_{}_{}",
            tag,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn normalises_routes() {
        assert_eq!(normalise_route("about"), "/about");
        assert_eq!(normalise_route("/about/"), "/about");
        assert_eq!(normalise_route("//about//x//"), "/about/x");
        assert_eq!(normalise_route("/about?x=1"), "/about");
        assert_eq!(normalise_route(""), "/");
        assert_eq!(normalise_route("/"), "/");
    }

    #[test]
    fn discovers_next_app_router_routes() {
        let root = scratch("app");
        fs::create_dir_all(root.join("app/blog/[slug]")).unwrap();
        fs::create_dir_all(root.join("app/about")).unwrap();
        fs::write(root.join("app/page.tsx"), "export default function P(){}").unwrap();
        fs::write(
            root.join("app/about/page.tsx"),
            "export default function A(){}",
        )
        .unwrap();
        fs::write(
            root.join("app/blog/[slug]/page.tsx"),
            "export default function B(){}",
        )
        .unwrap();

        let routes = discover_route_files(&root, 1000);
        let paths: Vec<String> = routes.iter().map(|r| r.path.clone()).collect();
        assert!(paths.contains(&"/".to_string()));
        assert!(paths.contains(&"/about".to_string()));
        assert!(paths.contains(&"/blog/[slug]".to_string()));

        let dynamic = routes.iter().find(|r| r.path == "/blog/[slug]").unwrap();
        assert_eq!(dynamic.kind, RouteKind::Dynamic);
        assert!(!dynamic.visitable);
        assert!(dynamic.blocked_reason.is_some());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn discovers_next_pages_router_routes() {
        let root = scratch("pages");
        fs::create_dir_all(root.join("pages/blog")).unwrap();
        fs::write(
            root.join("pages/index.tsx"),
            "export default function I(){}",
        )
        .unwrap();
        fs::write(
            root.join("pages/about.tsx"),
            "export default function A(){}",
        )
        .unwrap();
        fs::write(
            root.join("pages/blog/index.tsx"),
            "export default function B(){}",
        )
        .unwrap();

        let routes = discover_route_files(&root, 1000);
        let paths: Vec<String> = routes.iter().map(|r| r.path.clone()).collect();
        assert!(paths.contains(&"/".to_string()));
        assert!(paths.contains(&"/about".to_string()));
        assert!(paths.contains(&"/blog".to_string()));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn route_groups_do_not_appear_in_the_url() {
        let root = scratch("groups");
        fs::create_dir_all(root.join("app/(marketing)/pricing")).unwrap();
        fs::write(
            root.join("app/(marketing)/pricing/page.tsx"),
            "export default function P(){}",
        )
        .unwrap();
        let routes = discover_route_files(&root, 1000);
        assert_eq!(routes[0].path, "/pricing");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn extracts_router_configuration() {
        let text = r#"
            const routes = [
              { path: '/dashboard', element: <D/> },
              { path: '/settings', element: <S/> },
            ];
            <Link to="/profile">Profile</Link>
        "#;
        let routes = extract_routes_from_source(text, Path::new("/tmp/app.tsx"));
        let paths: Vec<String> = routes.iter().map(|r| r.path.clone()).collect();
        assert!(paths.contains(&"/dashboard".to_string()));
        assert!(paths.contains(&"/settings".to_string()));
        assert!(paths.contains(&"/profile".to_string()));
    }

    #[test]
    fn ignores_non_route_strings() {
        let text = r#"const label = "Save changes"; const other = "hello";"#;
        let routes = extract_routes_from_source(text, Path::new("/tmp/app.tsx"));
        assert!(routes.is_empty());
    }

    #[test]
    fn merges_and_deduplicates_routes() {
        let a = vec![DiscoveredRoute::new(
            "/about".into(),
            RouteKind::Static,
            RouteSource::RouteFile,
        )];
        let b = vec![DiscoveredRoute::new(
            "/about".into(),
            RouteKind::Static,
            RouteSource::RenderedLink,
        )];
        let inventory = merge_routes(vec![a, b]);
        assert_eq!(inventory.routes.len(), 1);
        assert_eq!(inventory.routes[0].sources.len(), 2);
    }

    #[test]
    fn a_static_route_wins_over_a_dynamic_one() {
        let a = vec![DiscoveredRoute::new(
            "/blog/[slug]".into(),
            RouteKind::Dynamic,
            RouteSource::RouteFile,
        )];
        let b = vec![DiscoveredRoute::new(
            "/blog/[slug]".into(),
            RouteKind::Static,
            RouteSource::RenderedLink,
        )];
        let inventory = merge_routes(vec![a, b]);
        assert_eq!(inventory.routes[0].kind, RouteKind::Static);
        assert!(inventory.routes[0].visitable);
    }
}
