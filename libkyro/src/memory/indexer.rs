use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    pub name: String,
    pub exec: String,
    pub comment: String,
    pub categories: Vec<String>,
}

pub struct AppIndexer;

impl AppIndexer {
    /// Scans standard Linux desktop application directories for installed applications
    pub fn scan_installed_apps() -> Vec<InstalledApp> {
        let mut apps = Vec::new();
        let mut search_paths = vec![
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
        ];

        if let Some(home) = dirs::home_dir() {
            search_paths.push(home.join(".local/share/applications"));
        }

        for p in search_paths {
            if let Ok(entries) = fs::read_dir(p) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("desktop") {
                        if let Some(app) = Self::parse_desktop_file(&path) {
                            if !apps.iter().any(|a: &InstalledApp| a.name == app.name) {
                                apps.push(app);
                            }
                        }
                    }
                }
            }
        }

        apps.sort_by(|a, b| a.name.cmp(&b.name));
        apps
    }

    fn parse_desktop_file(path: &Path) -> Option<InstalledApp> {
        let content = fs::read_to_string(path).ok()?;
        let mut name = String::new();
        let mut exec = String::new();
        let mut comment = String::new();
        let mut categories = Vec::new();
        let mut is_desktop_entry = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "[Desktop Entry]" {
                is_desktop_entry = true;
                continue;
            }
            if !is_desktop_entry {
                continue;
            }
            if trimmed.starts_with('[') && trimmed != "[Desktop Entry]" {
                break; // Left main Desktop Entry section
            }

            if let Some((k, v)) = trimmed.split_once('=') {
                match k.trim() {
                    "Name" if name.is_empty() => name = v.trim().to_string(),
                    "Exec" if exec.is_empty() => {
                        // Strip field codes like %u, %F, %U
                        let clean_exec = v
                            .split_whitespace()
                            .filter(|w| !w.starts_with('%'))
                            .collect::<Vec<_>>()
                            .join(" ");
                        exec = clean_exec;
                    }
                    "Comment" if comment.is_empty() => comment = v.trim().to_string(),
                    "Categories" => {
                        categories = v
                            .split(';')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    _ => {}
                }
            }
        }

        if !name.is_empty() && !exec.is_empty() {
            Some(InstalledApp {
                name,
                exec,
                comment,
                categories,
            })
        } else {
            None
        }
    }
}
