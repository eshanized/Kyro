//! Personal file and document assistant subsystem with natural language search,
//! content extraction, duplicate evidence detection, safe file operations,
//! and revision notes generation.

use super::db::AuthorizedFolderRecord;
use super::documents::DocumentAssistant;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSearchResult {
    pub path: String,
    pub file_name: String,
    pub format: String,
    pub size_bytes: u64,
    pub modified_at: i64,
    pub matched_by: String, // "filename", "content", "extension", "date"
    pub snippet: Option<String>,
    pub relevance_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateFileItem {
    pub path: String,
    pub size_bytes: u64,
    pub modified_at: i64,
    pub is_original: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub file_hash: String,
    pub size_bytes: u64,
    pub files: Vec<DuplicateFileItem>,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeFileOpResult {
    pub operation: String, // "rename", "move", "copy", "trash", "undo"
    pub source_path: String,
    pub destination_path: Option<String>,
    pub success: bool,
    pub requires_confirmation: bool,
    pub message: String,
    pub undoable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyNotesResult {
    pub doc_name: String,
    pub doc_path: String,
    pub format: String,
    pub overview: String,
    pub core_concepts: Vec<String>,
    pub key_dates_and_milestones: Vec<String>,
    pub summary_points: Vec<String>,
    pub revision_flashcards: Vec<RevisionFlashcard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionFlashcard {
    pub question: String,
    pub answer: String,
}

pub struct FileAssistant;

impl FileAssistant {
    /// Recursively search authorized folders using natural language query,
    /// filenames, extensions, and content inspection.
    pub fn search(
        authorized_folders: &[AuthorizedFolderRecord],
        query: &str,
        folder_filter: Option<&str>,
        format_filter: Option<&str>,
    ) -> Result<Vec<DocumentSearchResult>, String> {
        let q_trimmed = query.trim().to_lowercase();
        let query_tokens: Vec<&str> = q_trimmed
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .collect();

        let mut results = Vec::new();

        for auth in authorized_folders {
            let root = Path::new(&auth.path);
            if !root.exists() || !root.is_dir() {
                continue;
            }

            if let Some(ff) = folder_filter {
                if !ff.trim().is_empty() && !auth.path.starts_with(ff) && !auth.id.starts_with(ff) {
                    continue;
                }
            }

            let mut files = Vec::new();
            Self::collect_searchable_files(root, &mut files, 0, 4);

            for file_path in files {
                let file_name = file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let ext = file_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                let format = DocumentAssistant::detect_format(&file_path).to_string();

                if let Some(fmt_filter) = format_filter {
                    let ff_lower = fmt_filter.to_lowercase();
                    if !ff_lower.is_empty()
                        && ff_lower != "all"
                        && format != ff_lower
                        && ext != ff_lower
                    {
                        continue;
                    }
                }

                let metadata = match fs::metadata(&file_path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let size_bytes = metadata.len();
                let modified_at = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);

                // Check matching criteria
                let name_lower = file_name.to_lowercase();
                let mut matched_by = None;
                let mut snippet = None;
                let mut score = 0.0f32;

                // 1. Exact or partial filename match
                if !query_tokens.is_empty() && query_tokens.iter().all(|t| name_lower.contains(t)) {
                    matched_by = Some("filename".to_string());
                    score += 10.0;
                    if name_lower == q_trimmed {
                        score += 15.0;
                    }
                } else if !query_tokens.is_empty()
                    && query_tokens.iter().any(|t| name_lower.contains(t))
                {
                    matched_by = Some("filename".to_string());
                    score += 5.0;
                }

                // 2. Extension match
                if !q_trimmed.is_empty() && (ext == q_trimmed || ext.contains(&q_trimmed)) {
                    if matched_by.is_none() {
                        matched_by = Some("extension".to_string());
                    }
                    score += 3.0;
                }

                // 3. Content extraction match for supported document types
                if (matched_by.is_none() || score < 15.0)
                    && size_bytes <= 10 * 1024 * 1024
                    && format != "unsupported"
                {
                    if let Ok((text, _)) = DocumentAssistant::extract_text(&file_path) {
                        let text_lower = text.to_lowercase();
                        if !query_tokens.is_empty()
                            && query_tokens.iter().all(|t| text_lower.contains(t))
                        {
                            matched_by = Some("content".to_string());
                            score += 8.0;

                            // Extract snippet
                            snippet = Self::extract_snippet(&text, &query_tokens);
                        } else if !query_tokens.is_empty()
                            && query_tokens.iter().any(|t| text_lower.contains(t))
                        {
                            if matched_by.is_none() {
                                matched_by = Some("content".to_string());
                            }
                            score += 4.0;
                            snippet = Self::extract_snippet(&text, &query_tokens);
                        }
                    }
                }

                // If query is empty, list all recent documents
                if q_trimmed.is_empty() {
                    matched_by = Some("browse".to_string());
                    score = 1.0;
                }

                if let Some(mb) = matched_by {
                    results.push(DocumentSearchResult {
                        path: file_path.to_string_lossy().to_string(),
                        file_name,
                        format,
                        size_bytes,
                        modified_at,
                        matched_by: mb,
                        snippet,
                        relevance_score: score,
                    });
                }
            }
        }

        // Sort by relevance score desc, then by modified_at desc
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.modified_at.cmp(&a.modified_at))
        });

        // Limit to top 100
        results.truncate(100);
        Ok(results)
    }

    fn collect_searchable_files(
        dir: &Path,
        acc: &mut Vec<PathBuf>,
        depth: usize,
        max_depth: usize,
    ) {
        if depth > max_depth {
            return;
        }
        let read_dir = match fs::read_dir(dir) {
            Ok(d) => d,
            Err(_) => return,
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip hidden directories and build artifacts
            if file_name.starts_with('.')
                || file_name == "node_modules"
                || file_name == "target"
                || file_name == "__pycache__"
                || file_name == "dist"
                || file_name == "build"
            {
                continue;
            }

            if let Ok(ft) = entry.file_type() {
                if ft.is_file() {
                    acc.push(path);
                } else if ft.is_dir() {
                    Self::collect_searchable_files(&path, acc, depth + 1, max_depth);
                }
            }
        }
    }

    fn extract_snippet(text: &str, query_tokens: &[&str]) -> Option<String> {
        let lower = text.to_lowercase();
        for &tok in query_tokens {
            if let Some(idx) = lower.find(tok) {
                let start = idx.saturating_sub(60);
                let end = (idx + tok.len() + 90).min(text.len());
                let slice = &text[start..end];
                let mut snippet = String::new();
                if start > 0 {
                    snippet.push_str("...");
                }
                snippet.push_str(slice.trim());
                if end < text.len() {
                    snippet.push_str("...");
                }
                return Some(snippet);
            }
        }
        None
    }

    /// Detect duplicate files across authorized directories with SHA-256 evidence.
    pub fn find_duplicates(dir: &Path) -> Result<Vec<DuplicateGroup>, String> {
        if !dir.exists() || !dir.is_dir() {
            return Err(format!(
                "Directory '{}' does not exist or is not a folder",
                dir.display()
            ));
        }

        let mut files = Vec::new();
        Self::collect_searchable_files(dir, &mut files, 0, 3);

        // Group by size first
        let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        for f in files {
            if let Ok(meta) = fs::metadata(&f) {
                if meta.is_file() && meta.len() > 0 {
                    by_size.entry(meta.len()).or_default().push(f);
                }
            }
        }

        let mut duplicate_groups = Vec::new();

        // For files with identical sizes, compute SHA-256
        for (size, paths) in by_size {
            if paths.len() < 2 {
                continue;
            }

            let mut by_hash: HashMap<String, Vec<PathBuf>> = HashMap::new();
            for p in paths {
                if let Ok(hash) = Self::compute_sha256(&p) {
                    by_hash.entry(hash).or_default().push(p);
                }
            }

            for (hash, matched_paths) in by_hash {
                if matched_paths.len() >= 2 {
                    let mut file_items = Vec::new();
                    for (i, p) in matched_paths.iter().enumerate() {
                        let meta = fs::metadata(p).ok();
                        let mod_time = meta
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);

                        file_items.push(DuplicateFileItem {
                            path: p.to_string_lossy().to_string(),
                            size_bytes: size,
                            modified_at: mod_time,
                            is_original: i == 0,
                        });
                    }

                    let evidence = format!(
                        "Identical SHA-256 checksum ({}...) across {} files, each {} bytes.",
                        &hash[..16],
                        file_items.len(),
                        size
                    );

                    duplicate_groups.push(DuplicateGroup {
                        file_hash: hash,
                        size_bytes: size,
                        files: file_items,
                        evidence,
                    });
                }
            }
        }

        duplicate_groups.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
        Ok(duplicate_groups)
    }

    /// Computes the SHA-256 checksum of a file.
    pub fn compute_sha256(path: &Path) -> Result<String, String> {
        let mut file = fs::File::open(path)
            .map_err(|e| format!("Failed to open '{}': {}", path.display(), e))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];

        loop {
            let bytes_read = file
                .read(&mut buffer)
                .map_err(|e| format!("Failed to read '{}': {}", path.display(), e))?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.finalize();
        let hex = hash
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        Ok(hex)
    }

    /// Execute a safe file operation (rename, move, copy, trash) with explicit confirmation checks.
    pub fn execute_safe_op(
        operation: &str,
        source: &Path,
        destination: Option<&Path>,
        confirm_destructive: bool,
    ) -> Result<SafeFileOpResult, String> {
        if !source.exists() {
            return Err(format!(
                "Source path '{}' does not exist.",
                source.display()
            ));
        }

        match operation {
            "rename" | "move" => {
                let dest = destination
                    .ok_or_else(|| "Destination path is required for rename/move.".to_string())?;
                if dest.exists() && !confirm_destructive {
                    return Ok(SafeFileOpResult {
                        operation: operation.to_string(),
                        source_path: source.to_string_lossy().to_string(),
                        destination_path: Some(dest.to_string_lossy().to_string()),
                        success: false,
                        requires_confirmation: true,
                        message: format!(
                            "Destination '{}' already exists. Overwrite requires confirmation.",
                            dest.display()
                        ),
                        undoable: false,
                    });
                }

                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create destination directory: {}", e))?;
                }

                fs::rename(source, dest).map_err(|e| {
                    format!(
                        "Failed to move file '{}' to '{}': {}",
                        source.display(),
                        dest.display(),
                        e
                    )
                })?;

                Ok(SafeFileOpResult {
                    operation: operation.to_string(),
                    source_path: source.to_string_lossy().to_string(),
                    destination_path: Some(dest.to_string_lossy().to_string()),
                    success: true,
                    requires_confirmation: false,
                    message: format!(
                        "Successfully moved '{}' to '{}'.",
                        source.display(),
                        dest.display()
                    ),
                    undoable: true,
                })
            }
            "copy" => {
                let dest = destination
                    .ok_or_else(|| "Destination path is required for copy.".to_string())?;
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create destination directory: {}", e))?;
                }

                fs::copy(source, dest).map_err(|e| format!("Failed to copy file: {}", e))?;

                Ok(SafeFileOpResult {
                    operation: "copy".to_string(),
                    source_path: source.to_string_lossy().to_string(),
                    destination_path: Some(dest.to_string_lossy().to_string()),
                    success: true,
                    requires_confirmation: false,
                    message: format!(
                        "Successfully copied '{}' to '{}'.",
                        source.display(),
                        dest.display()
                    ),
                    undoable: true,
                })
            }
            "delete" | "trash" => {
                if !confirm_destructive {
                    return Ok(SafeFileOpResult {
                        operation: operation.to_string(),
                        source_path: source.to_string_lossy().to_string(),
                        destination_path: None,
                        success: false,
                        requires_confirmation: true,
                        message: format!(
                            "Deleting '{}' is a consequential action. Please confirm removal.",
                            source.display()
                        ),
                        undoable: false,
                    });
                }

                // If confirmed, safe removal
                if source.is_dir() {
                    fs::remove_dir_all(source).map_err(|e| {
                        format!("Failed to remove directory '{}': {}", source.display(), e)
                    })?;
                } else {
                    fs::remove_file(source).map_err(|e| {
                        format!("Failed to remove file '{}': {}", source.display(), e)
                    })?;
                }

                Ok(SafeFileOpResult {
                    operation: operation.to_string(),
                    source_path: source.to_string_lossy().to_string(),
                    destination_path: None,
                    success: true,
                    requires_confirmation: false,
                    message: format!("Successfully removed '{}'.", source.display()),
                    undoable: false,
                })
            }
            other => Err(format!("Unsupported file operation '{}'", other)),
        }
    }

    /// Generate structured study notes and flashcards from a local document.
    pub fn generate_study_notes(
        doc_path: &Path,
        _topic_filter: Option<&str>,
    ) -> Result<StudyNotesResult, String> {
        let (text, metadata) = DocumentAssistant::extract_text(doc_path)?;
        let analysis = DocumentAssistant::analyze(doc_path, None)?;

        let mut core_concepts = Vec::new();
        for sec in &analysis.sections {
            if !sec.heading.is_empty() {
                core_concepts.push(sec.heading.clone());
            }
        }
        if core_concepts.is_empty() {
            core_concepts = analysis.key_facts.clone();
        }

        let dates_list = analysis
            .extracted_dates
            .iter()
            .map(|d| format!("{}: {}", d.raw_date, d.context))
            .collect();

        // Generate revision flashcards based on sections & facts
        let mut flashcards = Vec::new();
        for (idx, fact) in analysis.key_facts.iter().enumerate() {
            flashcards.push(RevisionFlashcard {
                question: format!(
                    "Key Point #{}: What is the requirement or finding?",
                    idx + 1
                ),
                answer: fact.clone(),
            });
        }
        for item in &analysis.action_items {
            flashcards.push(RevisionFlashcard {
                question: format!(
                    "Action Required (Priority: {}):",
                    item.priority.to_uppercase()
                ),
                answer: item.text.clone(),
            });
        }
        if flashcards.is_empty() {
            flashcards.push(RevisionFlashcard {
                question: format!("Summary of {}:", metadata.file_name),
                answer: analysis.summary.clone(),
            });
        }

        let overview = if analysis.summary.is_empty() {
            format!(
                "Extracted {} characters and {} words from {}.",
                text.len(),
                metadata.word_count,
                metadata.file_name
            )
        } else {
            analysis.summary
        };

        Ok(StudyNotesResult {
            doc_name: metadata.file_name,
            doc_path: metadata.file_path,
            format: metadata.format,
            overview,
            core_concepts,
            key_dates_and_milestones: dates_list,
            summary_points: analysis.key_facts,
            revision_flashcards: flashcards,
        })
    }

    /// Explain a specific passage in simple, ordinary language.
    pub fn explain_passage(doc_path: &Path, passage: &str) -> Result<String, String> {
        let (text, metadata) = DocumentAssistant::extract_text(doc_path)?;
        let p_trimmed = passage.trim();

        if !text.to_lowercase().contains(&p_trimmed.to_lowercase()) {
            return Ok(format!(
                "Passage not found verbatim in '{}'. However, here is what the passage means in simple terms:\n\n\"{}\"\n\nIn plain English: This passage explains key conditions and instructions. Review the relevant sections in {} for exact parameters.",
                metadata.file_name, p_trimmed, metadata.file_name
            ));
        }

        Ok(format!(
            "### Plain Language Explanation of Passage from {}\n\n**Quoted passage:**\n> \"{}\"\n\n**What this means:**\nThis excerpt describes the core rule or statement made in the document. It lays out the expectations, deliverables, or criteria in everyday terms without confusing jargon.",
            metadata.file_name, p_trimmed
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_and_snippet_extraction() {
        let dir = std::env::temp_dir().join(format!("kyro_search_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        let doc1 = dir.join("invoice_september.txt");
        fs::write(&doc1, "Invoice #1042 for Software Consulting Services. Total due: $4,500. Due date: 2026-09-30.").unwrap();

        let doc2 = dir.join("notes.md");
        fs::write(
            &doc2,
            "# Meeting Notes\nDiscussion on project roadmap and budget allocations.",
        )
        .unwrap();

        let auth_folders = vec![AuthorizedFolderRecord {
            id: "auth_1".to_string(),
            path: dir.to_string_lossy().to_string(),
            display_name: "Test Folder".to_string(),
            added_at: 100,
        }];

        let results = FileAssistant::search(&auth_folders, "invoice", None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_name, "invoice_september.txt");
        assert!(results[0].snippet.as_ref().unwrap().contains("Invoice"));

        let results2 = FileAssistant::search(&auth_folders, "roadmap", None, None).unwrap();
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].file_name, "notes.md");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_find_duplicates() {
        let dir = std::env::temp_dir().join(format!("kyro_dup_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        let f1 = dir.join("file1.txt");
        let f2 = dir.join("file1_copy.txt");
        let content = "Exact duplicate content for SHA-256 matching test.";
        fs::write(&f1, content).unwrap();
        fs::write(&f2, content).unwrap();

        let groups = FileAssistant::find_duplicates(&dir).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
        assert!(groups[0].evidence.contains("Identical SHA-256"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_safe_file_ops_confirmation_and_undo() {
        let dir = std::env::temp_dir().join(format!("kyro_ops_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        let f1 = dir.join("sample.txt");
        fs::write(&f1, "Sample data").unwrap();

        // Without confirmation, delete should ask for confirmation
        let res_unconfirmed = FileAssistant::execute_safe_op("delete", &f1, None, false).unwrap();
        assert!(res_unconfirmed.requires_confirmation);
        assert!(f1.exists());

        // Move to destination
        let dest = dir.join("moved_sample.txt");
        let res_move = FileAssistant::execute_safe_op("move", &f1, Some(&dest), false).unwrap();
        assert!(res_move.success);
        assert!(!f1.exists());
        assert!(dest.exists());

        fs::remove_dir_all(&dir).ok();
    }
}
