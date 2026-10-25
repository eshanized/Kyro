//! Project discovery.
//!
//! The agent is asked to "find my project Runstead on the Desktop".  This
//! module answers that question by searching a *user authorised* directory
//! for directories that look like a project root, using the evidence that is
//! actually on disk (directory name, Git metadata, package manifests,
//! lockfiles) rather than guessing a framework.
//!
//! The search is deliberately bounded: a maximum depth, a maximum number of
//! visited entries, a wall-clock budget, and a hard exclusion list for
//! directories that cannot contain a project root.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Directories that are never descended into during discovery and recursive file organization.
pub const EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".hg",
    ".svn",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".cache",
    ".parcel-cache",
    ".yarn",
    ".pnpm-store",
    "bower_components",
    ".venv",
    "venv",
    "env",
    "__pycache__",
    "target",
    "build",
    "dist",
    "out",
    "coverage",
    ".gradle",
    ".idea",
    ".vscode",
    "vendor",
    "Pods",
    "DerivedData",
    ".terraform",
];

/// Returns whether a directory name matches canonical exclusion rules
/// (dependency directories, build artifacts, virtual environments, cache, and VCS metadata).
/// Matching is ASCII case-insensitive and robust to platform path separators.
pub fn is_excluded_directory(dir_name: &str) -> bool {
    let trimmed = dir_name.trim_matches(|c| c == '/' || c == '\\');
    let name = match Path::new(trimmed).file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => trimmed,
    };
    EXCLUDED_DIRS
        .iter()
        .any(|excluded| excluded.eq_ignore_ascii_case(name))
}

/// Files that identify a directory as a project root.
pub const PROJECT_MARKER_FILES: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "setup.py",
    "requirements.txt",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "composer.json",
    "Gemfile",
    "mix.exs",
    "deno.json",
    "deno.jsonc",
];

/// Lockfiles that identify the package manager of a Node.js project.
pub const LOCKFILES: &[(&str, &str)] = &[
    ("pnpm-lock.yaml", "pnpm"),
    ("yarn.lock", "yarn"),
    ("bun.lockb", "bun"),
    ("bun.lock", "bun"),
    ("package-lock.json", "npm"),
    ("npm-shrinkwrap.json", "npm"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    pub fn as_str(&self) -> &'static str {
        match self {
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Bun => "bun",
        }
    }

    /// The executable used to run a package script.
    pub fn run_command(&self) -> (&'static str, Vec<String>) {
        match self {
            PackageManager::Npm => ("npm", vec!["run".into()]),
            PackageManager::Pnpm => ("pnpm", vec!["run".into()]),
            PackageManager::Yarn => ("yarn", vec!["run".into()]),
            PackageManager::Bun => ("bun", vec!["run".into()]),
        }
    }

    /// The command used to install dependencies.
    pub fn install_command(&self) -> (&'static str, Vec<String>) {
        match self {
            PackageManager::Npm => ("npm", vec!["install".into()]),
            PackageManager::Pnpm => ("pnpm", vec!["install".into()]),
            PackageManager::Yarn => ("yarn", vec!["install".into()]),
            PackageManager::Bun => ("bun", vec!["install".into()]),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectKind {
    NodeJs,
    Rust,
    Python,
    Go,
    Java,
    DotNet,
    Ruby,
    Php,
    Elixir,
    Deno,
    Unknown,
}

impl ProjectKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectKind::NodeJs => "node",
            ProjectKind::Rust => "rust",
            ProjectKind::Python => "python",
            ProjectKind::Go => "go",
            ProjectKind::Java => "java",
            ProjectKind::DotNet => "dotnet",
            ProjectKind::Ruby => "ruby",
            ProjectKind::Php => "php",
            ProjectKind::Elixir => "elixir",
            ProjectKind::Deno => "deno",
            ProjectKind::Unknown => "unknown",
        }
    }
}

/// A directory that looks like a project root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCandidate {
    /// The directory name.
    pub name: String,
    /// The absolute path of the project root.
    pub path: PathBuf,
    /// The kind of project, inferred from the manifest.
    pub kind: ProjectKind,
    /// The manifest that identified the project.
    pub manifest: Option<PathBuf>,
    /// The lockfile that determines the package manager.
    pub lockfile: Option<PathBuf>,
    /// The package manager, for Node.js projects.
    pub package_manager: Option<PackageManager>,
    /// `true` when the directory contains Git metadata.
    pub is_git_repo: bool,
    /// The script names declared in `package.json`.
    pub scripts: Vec<String>,
    /// `true` when the directory name matches the requested name.
    pub name_matched: bool,
}

/// The outcome of a discovery run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResult {
    /// The directory that was searched.
    pub searched_root: PathBuf,
    /// The name that was searched for.
    pub query: String,
    /// The candidates that were found, strongest match first.
    pub candidates: Vec<ProjectCandidate>,
    /// `true` when the search stopped early because a budget was reached.
    pub truncated: bool,
    /// How many directory entries were inspected.
    pub visited_entries: usize,
    /// How long the search took.
    pub duration_ms: u64,
    /// A human readable explanation of what happened.
    pub summary: String,
}

/// The budgets that bound a discovery run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryLimits {
    pub max_depth: usize,
    pub max_entries: usize,
    pub max_results: usize,
    pub timeout_ms: u64,
}

impl Default for DiscoveryLimits {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_entries: 4000,
            max_results: 12,
            timeout_ms: 15000,
        }
    }
}

/// Resolves the user's Desktop directory from the operating system.
pub fn desktop_directory() -> Option<PathBuf> {
    dirs::desktop_dir().or_else(|| dirs::home_dir().map(|h| h.join("Desktop")))
}

/// Resolves the user's home directory from the operating system.
pub fn home_directory() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Searches `root` for a project whose directory name matches `query`.
///
/// The match is case-insensitive.  Directories that are clearly not project
/// roots are skipped, and the search never follows symlinks that leave the
/// search root.
pub fn discover_projects(
    root: &Path,
    query: &str,
    limits: &DiscoveryLimits,
) -> Result<DiscoveryResult, String> {
    let started = Instant::now();
    if !root.exists() {
        return Err(format!(
            "The search location '{}' does not exist. Provide an existing directory to search.",
            root.display()
        ));
    }
    if !root.is_dir() {
        return Err(format!(
            "The search location '{}' is not a directory.",
            root.display()
        ));
    }

    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let query_lower = query.trim().to_lowercase();

    let mut candidates: Vec<ProjectCandidate> = Vec::new();
    let mut visited = 0usize;
    let mut truncated = false;
    let mut queue: Vec<(PathBuf, usize)> = vec![(canonical_root.clone(), 0)];

    while let Some((dir, depth)) = queue.pop() {
        if started.elapsed().as_millis() as u64 > limits.timeout_ms {
            truncated = true;
            break;
        }
        if visited >= limits.max_entries {
            truncated = true;
            break;
        }

        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            visited += 1;
            if visited > limits.max_entries {
                truncated = true;
                break;
            }
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            let meta = match std::fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_symlink() {
                // Only follow symlinks that stay inside the search root.
                let resolved = match path.canonicalize() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                if !resolved.starts_with(&canonical_root) {
                    continue;
                }
            }
            if !meta.is_dir() {
                continue;
            }
            if is_excluded_directory(&file_name) {
                continue;
            }
            if file_name.starts_with('.') && file_name != ".github" {
                continue;
            }

            if depth < limits.max_depth {
                queue.push((path.clone(), depth + 1));
            }

            let file_name_lower = file_name.to_lowercase();
            let name_exact = file_name_lower == query_lower;
            let name_approx = !query_lower.is_empty()
                && (file_name_lower.contains(&query_lower)
                    || query_lower.contains(&file_name_lower));

            if let Some(mut candidate) = inspect_project_root(&path, name_exact) {
                let pkg_name = candidate
                    .manifest
                    .as_deref()
                    .and_then(|m| manifest_package_name(Some(m)));
                let pkg_name_lower = pkg_name.as_deref().map(|s| s.to_lowercase());

                let pkg_exact = pkg_name_lower.as_deref() == Some(query_lower.as_str());
                let pkg_approx = pkg_name_lower
                    .as_deref()
                    .map(|s| s.contains(&query_lower))
                    .unwrap_or(false);

                let is_match =
                    query_lower.is_empty() || name_exact || name_approx || pkg_exact || pkg_approx;

                if is_match {
                    if pkg_exact {
                        candidate.name_matched = true;
                    }
                    candidates.push(candidate);
                    if candidates.len() >= limits.max_results {
                        truncated = true;
                        break;
                    }
                }
            }
        }
        if truncated {
            break;
        }
    }

    // Strongest matches first: exact name, then a project with a manifest,
    // then shallower paths.
    candidates.sort_by(|a, b| {
        b.name_matched
            .cmp(&a.name_matched)
            .then(b.is_git_repo.cmp(&a.is_git_repo))
            .then(
                a.path
                    .components()
                    .count()
                    .cmp(&b.path.components().count()),
            )
            .then(a.name.cmp(&b.name))
    });

    let summary = if candidates.is_empty() {
        format!(
            "No project named '{}' was found under '{}'. {} directories were inspected{}.",
            query,
            canonical_root.display(),
            visited,
            if truncated {
                " before the search budget was reached"
            } else {
                ""
            }
        )
    } else {
        format!(
            "Found {} candidate project(s) named '{}' under '{}' ({} directories inspected{}).",
            candidates.len(),
            query,
            canonical_root.display(),
            visited,
            if truncated {
                ", search budget reached"
            } else {
                ""
            }
        )
    };

    Ok(DiscoveryResult {
        searched_root: canonical_root,
        query: query.to_string(),
        candidates,
        truncated,
        visited_entries: visited,
        duration_ms: started.elapsed().as_millis() as u64,
        summary,
    })
}

/// Inspects a directory and reports it as a project root when the evidence
/// on disk supports that conclusion.
pub fn inspect_project_root(path: &Path, name_matched: bool) -> Option<ProjectCandidate> {
    let mut manifest: Option<PathBuf> = None;
    let mut kind = ProjectKind::Unknown;

    for marker in PROJECT_MARKER_FILES {
        let candidate = path.join(marker);
        if candidate.is_file() {
            manifest = Some(candidate.clone());
            kind = kind_for_manifest(marker);
            break;
        }
    }

    let mut lockfile: Option<PathBuf> = None;
    let mut package_manager: Option<PackageManager> = None;
    for (file, pm) in LOCKFILES {
        let candidate = path.join(file);
        if candidate.is_file() {
            lockfile = Some(candidate.clone());
            package_manager = Some(match *pm {
                "pnpm" => PackageManager::Pnpm,
                "yarn" => PackageManager::Yarn,
                "bun" => PackageManager::Bun,
                _ => PackageManager::Npm,
            });
            break;
        }
    }

    let is_git_repo = path.join(".git").exists();
    let scripts = manifest
        .as_ref()
        .and_then(|m| read_package_scripts(m))
        .unwrap_or_default();

    // A directory is a project root when it carries a manifest, a lockfile,
    // Git metadata, or when its name matches the requested project.
    let has_evidence = manifest.is_some() || lockfile.is_some() || is_git_repo;
    if !has_evidence && !name_matched {
        return None;
    }
    if !has_evidence && name_matched && scripts.is_empty() {
        // The name matches but nothing on disk identifies it as a project.
        // Report it so the user can choose, but keep the kind unknown.
        kind = ProjectKind::Unknown;
    }

    Some(ProjectCandidate {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string()),
        path: path.to_path_buf(),
        kind,
        manifest,
        lockfile,
        package_manager,
        is_git_repo,
        scripts,
        name_matched,
    })
}

fn manifest_package_name(manifest: Option<&Path>) -> Option<String> {
    let manifest_path = manifest?;
    if manifest_path.file_name()?.to_str()? == "package.json" {
        let pkg = read_package_json(manifest_path)?;
        return pkg
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());
    }
    None
}

fn kind_for_manifest(marker: &str) -> ProjectKind {
    match marker {
        "package.json" => ProjectKind::NodeJs,
        "Cargo.toml" => ProjectKind::Rust,
        "pyproject.toml" | "setup.py" | "requirements.txt" => ProjectKind::Python,
        "go.mod" => ProjectKind::Go,
        "pom.xml" | "build.gradle" | "build.gradle.kts" => ProjectKind::Java,
        "composer.json" => ProjectKind::Php,
        "Gemfile" => ProjectKind::Ruby,
        "mix.exs" => ProjectKind::Elixir,
        "deno.json" | "deno.jsonc" => ProjectKind::Deno,
        _ => ProjectKind::Unknown,
    }
}

/// Reads the script names declared in a `package.json`.
pub fn read_package_scripts(manifest: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(manifest).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let scripts = value.get("scripts")?.as_object()?;
    let mut names: Vec<String> = scripts.keys().cloned().collect();
    names.sort();
    Some(names)
}

/// Reads the whole `package.json` document.
pub fn read_package_json(manifest: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(manifest).ok()?;
    serde_json::from_str(&text).ok()
}

/// Detects the framework of a Node.js project from its dependencies.
pub fn detect_node_framework(manifest: &Path) -> Option<String> {
    let pkg = read_package_json(manifest)?;
    let deps = pkg
        .get("dependencies")
        .and_then(|d| d.as_object())
        .cloned()
        .unwrap_or_default();
    let dev_deps = pkg
        .get("devDependencies")
        .and_then(|d| d.as_object())
        .cloned()
        .unwrap_or_default();

    let mut frameworks: Vec<(&str, &str)> = Vec::new();
    for (name, label) in [
        ("next", "nextjs"),
        ("vite", "vite"),
        ("nuxt", "nuxt"),
        ("@remix-run/react", "remix"),
        ("@angular/core", "angular"),
        ("svelte", "svelte"),
        ("astro", "astro"),
        ("gatsby", "gatsby"),
        ("react-scripts", "create-react-app"),
        ("@remix-run/dev", "remix"),
        ("webpack-dev-server", "webpack"),
    ] {
        if deps.contains_key(name) || dev_deps.contains_key(name) {
            frameworks.push((label, name));
        }
    }
    frameworks.sort();
    frameworks.first().map(|(l, _)| l.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "kyro_discovery_{}_{}_{}",
            tag,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn finds_a_project_by_name() {
        let root = scratch("find");
        let project = root.join("Runstead");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("package.json"),
            r#"{"name":"runstead","scripts":{"dev":"next dev","build":"next build"},"devDependencies":{"next":"14.2.0"}}"#,
        )
        .unwrap();

        let result = discover_projects(&root, "runstead", &DiscoveryLimits::default()).unwrap();
        assert_eq!(result.candidates.len(), 1);
        let c = &result.candidates[0];
        assert_eq!(c.name, "Runstead");
        assert_eq!(c.kind, ProjectKind::NodeJs);
        assert!(c.name_matched);
        assert!(c.scripts.contains(&"dev".to_string()));
        assert_eq!(
            detect_node_framework(&project.join("package.json")).as_deref(),
            Some("nextjs")
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn name_matching_is_case_insensitive() {
        let root = scratch("case");
        let project = root.join("RUNSTEAD");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("package.json"), "{}").unwrap();
        let result = discover_projects(&root, "Runstead", &DiscoveryLimits::default()).unwrap();
        assert_eq!(result.candidates.len(), 1);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_project_reports_an_empty_result() {
        let root = scratch("missing");
        let result = discover_projects(&root, "Runstead", &DiscoveryLimits::default()).unwrap();
        assert!(result.candidates.is_empty());
        assert!(result.summary.contains("No project named"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn ambiguous_matches_are_all_reported() {
        let root = scratch("ambiguous");
        for dir in ["Runstead", "runstead", "Runstead-old"] {
            let p = root.join(dir);
            fs::create_dir_all(&p).unwrap();
            fs::write(p.join("package.json"), "{}").unwrap();
        }
        let result = discover_projects(&root, "runstead", &DiscoveryLimits::default()).unwrap();
        assert!(result.candidates.len() >= 2, "expected several candidates");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn node_modules_is_excluded() {
        let root = scratch("excluded");
        let nested = root.join("node_modules").join("Runstead");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("package.json"), "{}").unwrap();
        let result = discover_projects(&root, "runstead", &DiscoveryLimits::default()).unwrap();
        assert!(result.candidates.is_empty());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn lockfile_determines_the_package_manager() {
        let root = scratch("pm");
        let project = root.join("Runstead");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("package.json"),
            r#"{"scripts":{"dev":"vite"}}"#,
        )
        .unwrap();
        fs::write(project.join("pnpm-lock.yaml"), "lockfileVersion: 9\n").unwrap();
        let result = discover_projects(&root, "runstead", &DiscoveryLimits::default()).unwrap();
        assert_eq!(
            result.candidates[0].package_manager,
            Some(PackageManager::Pnpm)
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_root_is_reported() {
        let err = discover_projects(
            Path::new("/definitely/not/here"),
            "x",
            &DiscoveryLimits::default(),
        )
        .unwrap_err();
        assert!(err.contains("does not exist"));
    }

    #[test]
    fn symlink_outside_the_root_is_not_followed() {
        let root = scratch("symlink_root");
        let outside = scratch("symlink_outside");
        fs::write(outside.join("package.json"), "{}").unwrap();
        let link = root.join("Runstead");
        let _ = std::os::unix::fs::symlink(&outside, &link);
        let result = discover_projects(&root, "runstead", &DiscoveryLimits::default()).unwrap();
        assert!(
            result.candidates.is_empty(),
            "a symlink out of the search root must not be followed"
        );
        fs::remove_dir_all(&root).ok();
        fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn unrelated_projects_are_not_included_in_candidates() {
        let root = scratch("unrelated");
        let target = root.join("analytics-dashboard");
        fs::create_dir_all(&target).unwrap();
        fs::write(
            target.join("package.json"),
            r#"{"name":"analytics-dashboard"}"#,
        )
        .unwrap();

        let unrelated = root.join("billing-service");
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(
            unrelated.join("package.json"),
            r#"{"name":"billing-service"}"#,
        )
        .unwrap();

        let result =
            discover_projects(&root, "analytics-dashboard", &DiscoveryLimits::default()).unwrap();
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].name, "analytics-dashboard");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn exact_match_ranks_before_approximate() {
        let root = scratch("rank");
        let approx = root.join("app-preview-service");
        fs::create_dir_all(&approx).unwrap();
        fs::write(approx.join("package.json"), "{}").unwrap();

        let exact = root.join("app");
        fs::create_dir_all(&exact).unwrap();
        fs::write(exact.join("package.json"), "{}").unwrap();

        let result = discover_projects(&root, "app", &DiscoveryLimits::default()).unwrap();
        assert!(result.candidates.len() >= 2);
        assert_eq!(result.candidates[0].name, "app");
        assert!(result.candidates[0].name_matched);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn supports_paths_with_spaces_and_unicode() {
        let root = scratch("unicode");
        let project = root.join("Mon Projet Étoilé 🌟");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("package.json"), r#"{"name":"mon-projet"}"#).unwrap();

        let result = discover_projects(&root, "mon-projet", &DiscoveryLimits::default()).unwrap();
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].path, project);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn desktop_dir_resolution() {
        let desktop = desktop_directory().expect("desktop dir must be resolvable");
        assert!(desktop.is_absolute());
    }
}
