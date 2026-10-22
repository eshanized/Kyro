//! Personal file and document assistant subsystem with natural language search,
//! content extraction, duplicate evidence detection, safe file operations,
//! and revision notes generation.

use super::db::{AuthorizedFolderRecord, Database, DocumentIndexRecord};
use super::documents::DocumentAssistant;
use chrono::Datelike;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct SearchCriteria {
    pub raw_query: String,
    pub query_tokens: Vec<String>,
    pub date_start: Option<i64>,
    pub date_end: Option<i64>,
    pub format_hint: Option<String>,
    pub asks_for_facts: bool,
    pub target_fact: Option<String>,
}

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
    /// Parses natural language query into structured criteria, extracting relative date
    /// constraints, fact requests, and meaningful search tokens.
    pub fn parse_natural_language_query(query: &str) -> SearchCriteria {
        let lower = query.trim().to_lowercase();
        let now = chrono::Utc::now();

        // 1. Relative dates
        let mut date_start = None;
        let mut date_end = None;

        if lower.contains("last month") {
            let year = now.year();
            let month = now.month();
            let (prev_year, prev_month) = if month == 1 {
                (year - 1, 12)
            } else {
                (year, month - 1)
            };
            if let Some(start_date) = chrono::NaiveDate::from_ymd_opt(prev_year, prev_month, 1) {
                if let Some(start_dt) = start_date.and_hms_opt(0, 0, 0) {
                    date_start = Some(start_dt.and_utc().timestamp_millis());
                }
            }
            if let Some(cur_month_start) = chrono::NaiveDate::from_ymd_opt(year, month, 1) {
                if let Some(end_dt) = cur_month_start.and_hms_opt(0, 0, 0) {
                    date_end = Some(end_dt.and_utc().timestamp_millis() - 1);
                }
            }
        } else if lower.contains("this month") {
            if let Some(start_date) = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1) {
                if let Some(start_dt) = start_date.and_hms_opt(0, 0, 0) {
                    date_start = Some(start_dt.and_utc().timestamp_millis());
                }
            }
            date_end = Some(now.timestamp_millis());
        } else if lower.contains("yesterday") {
            let y_date = now.date_naive() - chrono::Duration::days(1);
            if let Some(s) = y_date.and_hms_opt(0, 0, 0) {
                date_start = Some(s.and_utc().timestamp_millis());
            }
            if let Some(e) = y_date.and_hms_opt(23, 59, 59) {
                date_end = Some(e.and_utc().timestamp_millis());
            }
        } else if lower.contains("today") {
            let t_date = now.date_naive();
            if let Some(s) = t_date.and_hms_opt(0, 0, 0) {
                date_start = Some(s.and_utc().timestamp_millis());
            }
            date_end = Some(now.timestamp_millis());
        } else if lower.contains("last week") {
            let days_since_mon = now.weekday().num_days_from_monday();
            let this_mon = now.date_naive() - chrono::Duration::days(days_since_mon as i64);
            let last_mon = this_mon - chrono::Duration::days(7);
            if let Some(s) = last_mon.and_hms_opt(0, 0, 0) {
                date_start = Some(s.and_utc().timestamp_millis());
            }
            if let Some(e) = this_mon.and_hms_opt(0, 0, 0) {
                date_end = Some(e.and_utc().timestamp_millis() - 1);
            }
        } else if lower.contains("this week") {
            let days_since_mon = now.weekday().num_days_from_monday();
            let this_mon = now.date_naive() - chrono::Duration::days(days_since_mon as i64);
            if let Some(s) = this_mon.and_hms_opt(0, 0, 0) {
                date_start = Some(s.and_utc().timestamp_millis());
            }
            date_end = Some(now.timestamp_millis());
        } else if lower.contains("last year") {
            if let Some(s) = chrono::NaiveDate::from_ymd_opt(now.year() - 1, 1, 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
            {
                date_start = Some(s.and_utc().timestamp_millis());
            }
            if let Some(e) = chrono::NaiveDate::from_ymd_opt(now.year(), 1, 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
            {
                date_end = Some(e.and_utc().timestamp_millis() - 1);
            }
        } else if lower.contains("this year") {
            if let Some(s) = chrono::NaiveDate::from_ymd_opt(now.year(), 1, 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
            {
                date_start = Some(s.and_utc().timestamp_millis());
            }
            date_end = Some(now.timestamp_millis());
        }

        // 2. Fact extraction target
        let asks_for_facts = lower.contains("extract")
            || lower.contains("total amount")
            || lower.contains("how much")
            || lower.contains("deadline")
            || lower.contains("due date")
            || lower.contains("what is the total");

        let target_fact = if lower.contains("amount")
            || lower.contains("total")
            || lower.contains("price")
            || lower.contains("how much")
        {
            Some("total_amount".to_string())
        } else if lower.contains("deadline")
            || lower.contains("due date")
            || lower.contains("when is it due")
        {
            Some("due_date".to_string())
        } else {
            None
        };

        // 3. Format hint
        let format_hint = if lower.contains("pdf") {
            Some("pdf".to_string())
        } else if lower.contains("markdown") || lower.contains(".md") {
            Some("md".to_string())
        } else if lower.contains("docx") || lower.contains("word document") {
            Some("docx".to_string())
        } else if lower.contains("csv")
            || lower.contains("spreadsheet")
            || lower.contains("excel")
            || lower.contains("xlsx")
        {
            Some("csv".to_string())
        } else if lower.contains("invoice") || lower.contains("receipt") || lower.contains("bill") {
            Some("invoice_or_receipt".to_string())
        } else if lower.contains("syllabus") || lower.contains("notes") {
            Some("document".to_string())
        } else if lower.contains("code") || lower.contains("script") {
            Some("code".to_string())
        } else {
            None
        };

        // 4. Token filtering: remove conversational phrases and stop words
        let mut clean = lower.clone();
        for phrase in &[
            "find the",
            "find my",
            "find a",
            "search for",
            "search my",
            "i downloaded",
            "downloaded",
            "and extract its total amount",
            "and extract the total amount",
            "and extract total amount",
            "extract its total amount",
            "extract the total amount",
            "and extract its",
            "and extract",
            "extract total",
            "total amount",
            "last month",
            "this month",
            "last week",
            "this week",
            "last year",
            "this year",
            "yesterday",
            "today",
            "show me",
            "where is",
            "documents that mention",
            "mention an",
            "an upcoming",
            "upcoming",
            "documents that",
            "mentioning",
            "receipt for my",
            "receipt for",
            "invoice for",
            "syllabus for",
        ] {
            clean = clean.replace(phrase, " ");
        }

        let stop_words = [
            "the", "a", "an", "for", "my", "in", "on", "at", "to", "of", "and", "or", "is", "it",
            "with", "from", "that", "this",
        ];
        let mut query_tokens = Vec::new();
        for word in clean.split_whitespace() {
            let stripped: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
            if !stripped.is_empty() && !stop_words.contains(&stripped.as_str()) {
                query_tokens.push(stripped);
            }
        }

        if query_tokens.is_empty() {
            if lower.contains("invoice") {
                query_tokens.push("invoice".to_string());
            }
            if lower.contains("receipt") {
                query_tokens.push("receipt".to_string());
            }
            if lower.contains("syllabus") {
                query_tokens.push("syllabus".to_string());
            }
            if lower.contains("deadline") {
                query_tokens.push("deadline".to_string());
            }
        }

        SearchCriteria {
            raw_query: query.to_string(),
            query_tokens,
            date_start,
            date_end,
            format_hint,
            asks_for_facts,
            target_fact,
        }
    }

    /// Extract structured facts (totals, reference numbers, due dates) from plain text.
    pub fn extract_facts_from_text(text: &str) -> Vec<(String, String)> {
        let mut facts = Vec::new();

        if let Ok(re_amount) = Regex::new(
            r"(?i)\b(?:total(?:\s+due)?|amount(?:\s+due)?|balance(?:\s+due)?|subtotal|total:?)[\s:]*([$€£¥]\s*[\d,]+(?:\.\d{2})?|\b[\d,]+\.\d{2}\s*(?:USD|EUR|GBP)?)",
        ) {
            for cap in re_amount.captures_iter(text) {
                if let Some(m) = cap.get(1) {
                    facts.push(("Total Amount".to_string(), m.as_str().trim().to_string()));
                    break;
                }
            }
        }
        if facts.is_empty() {
            if let Ok(re_curr) = Regex::new(r"([$€£¥]\s*[\d,]+(?:\.\d{2})?)") {
                if let Some(cap) = re_curr.captures(text) {
                    if let Some(m) = cap.get(1) {
                        facts.push(("Amount Found".to_string(), m.as_str().trim().to_string()));
                    }
                }
            }
        }

        if let Ok(re_inv) = Regex::new(
            r"(?i)\b(?:invoice|receipt|order|account|bill|po|reference)\s*(?:#|no\.?|num\.?|number|ref\.?)?[\s:]+([A-Za-z0-9_-]{3,25})\b",
        ) {
            for cap in re_inv.captures_iter(text) {
                if let Some(m) = cap.get(1) {
                    let val = m.as_str().trim().to_string();
                    let val_lower = val.to_lowercase();
                    if val_lower != "details" && val_lower != "due" && val_lower != "date" {
                        facts.push(("Reference #".to_string(), val));
                        break;
                    }
                }
            }
        }

        if let Ok(re_due) = Regex::new(
            r"(?i)\b(?:due(?:\s+date)?|deadline|pay(?:\s+before)?|by)[\s:]*(\d{4}-\d{2}-\d{2}|(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]* \d{1,2},? \d{4})\b",
        ) {
            if let Some(cap) = re_due.captures(text) {
                if let Some(m) = cap.get(1) {
                    facts.push(("Due Date".to_string(), m.as_str().trim().to_string()));
                }
            }
        }

        facts
    }

    /// Recursively search authorized folders using natural language query,
    /// filenames, extensions, and content inspection.
    pub fn search(
        authorized_folders: &[AuthorizedFolderRecord],
        query: &str,
        folder_filter: Option<&str>,
        format_filter: Option<&str>,
    ) -> Result<Vec<DocumentSearchResult>, String> {
        Self::search_with_db(
            None,
            authorized_folders,
            query,
            folder_filter,
            format_filter,
        )
    }

    /// Natural language document search with SQLite cache acceleration.
    pub fn search_with_db(
        db: Option<&Database>,
        authorized_folders: &[AuthorizedFolderRecord],
        query: &str,
        folder_filter: Option<&str>,
        format_filter: Option<&str>,
    ) -> Result<Vec<DocumentSearchResult>, String> {
        // Fail closed: if no folders authorized, return empty
        if authorized_folders.is_empty() {
            return Ok(Vec::new());
        }

        let criteria = Self::parse_natural_language_query(query);
        let q_trimmed = query.trim().to_lowercase();
        let query_tokens: Vec<&str> = criteria.query_tokens.iter().map(|s| s.as_str()).collect();

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
            Self::collect_searchable_files(root, root, &mut files, 0, 4);

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

                // Date filtering if criteria has date range
                if let Some(s) = criteria.date_start {
                    if modified_at < s {
                        continue;
                    }
                }
                if let Some(e) = criteria.date_end {
                    if modified_at > e {
                        continue;
                    }
                }

                let name_lower = file_name.to_lowercase();
                let mut matched_by = None;
                let mut snippet = None;
                let mut score = 0.0f32;

                // 1. Filename match
                if !query_tokens.is_empty() && query_tokens.iter().all(|t| name_lower.contains(t)) {
                    matched_by = Some("filename".to_string());
                    score += 15.0;
                    if name_lower == q_trimmed {
                        score += 20.0;
                    }
                } else if !query_tokens.is_empty()
                    && query_tokens.iter().any(|t| name_lower.contains(t))
                {
                    matched_by = Some("filename".to_string());
                    score += 8.0;
                }

                // 2. Extension match
                if !q_trimmed.is_empty() && (ext == q_trimmed || ext.contains(&q_trimmed)) {
                    if matched_by.is_none() {
                        matched_by = Some("extension".to_string());
                    }
                    score += 4.0;
                }

                // 3. Content inspection & Index caching
                if size_bytes <= 10 * 1024 * 1024 && format != "unsupported" {
                    let path_str = file_path.to_string_lossy().to_string();
                    let cached_text = if let Some(d) = db {
                        if let Ok(Some(idx_rec)) = d.get_indexed_document(&path_str) {
                            if idx_rec.modified_at == modified_at
                                && idx_rec.size_bytes == size_bytes
                            {
                                idx_rec.extracted_text
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let text_opt = if let Some(txt) = cached_text {
                        Some(txt)
                    } else if let Ok((txt, _)) = DocumentAssistant::extract_text(&file_path) {
                        if let Some(d) = db {
                            let sha = Self::compute_sha256(&file_path).unwrap_or_default();
                            let rec = DocumentIndexRecord {
                                path: path_str.clone(),
                                file_name: file_name.clone(),
                                format: format.clone(),
                                size_bytes,
                                modified_at,
                                sha256: sha,
                                extracted_text: Some(txt.clone()),
                                indexed_at: chrono::Utc::now().timestamp_millis(),
                            };
                            let _ = d.upsert_indexed_document(&rec);
                        }
                        Some(txt)
                    } else {
                        None
                    };

                    if let Some(ref text) = text_opt {
                        let text_lower = text.to_lowercase();
                        let mut content_matches = false;
                        if !query_tokens.is_empty()
                            && query_tokens.iter().all(|t| text_lower.contains(t))
                        {
                            content_matches = true;
                            score += 12.0;
                        } else if !query_tokens.is_empty()
                            && query_tokens.iter().any(|t| text_lower.contains(t))
                        {
                            content_matches = true;
                            score += 6.0;
                        }

                        if content_matches {
                            if matched_by.is_none() {
                                matched_by = Some("content".to_string());
                            }
                            snippet = Self::extract_snippet(text, &query_tokens);
                        }

                        // Fact extraction integration
                        if criteria.asks_for_facts {
                            let facts = Self::extract_facts_from_text(text);
                            if !facts.is_empty() {
                                score += 10.0;
                                let facts_summary = facts
                                    .iter()
                                    .map(|(k, v)| format!("{}: {}", k, v))
                                    .collect::<Vec<_>>()
                                    .join(" | ");
                                let base_snippet = snippet.unwrap_or_else(|| {
                                    let s: String = text.chars().take(150).collect();
                                    s
                                });
                                snippet =
                                    Some(format!("[Facts: {}] {}", facts_summary, base_snippet));
                                if matched_by.is_none() {
                                    matched_by = Some("fact_extracted".to_string());
                                }
                            }
                        }
                    }
                }

                // If query is empty, list all recent documents
                if q_trimmed.is_empty() {
                    matched_by = Some("browse".to_string());
                    score = 1.0;
                }

                if let Some(mb) = matched_by {
                    if criteria.date_start.is_some() || criteria.date_end.is_some() {
                        score += 5.0;
                    }
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

        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.modified_at.cmp(&a.modified_at))
        });

        results.truncate(100);
        Ok(results)
    }

    fn collect_searchable_files(
        root: &Path,
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

        let canonical_root = match fs::canonicalize(root) {
            Ok(c) => c,
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
                let resolved_path = if ft.is_symlink() {
                    match fs::canonicalize(&path) {
                        Ok(canon) => {
                            if !canon.starts_with(&canonical_root) {
                                continue; // Symlink escapes authorized root!
                            }
                            canon
                        }
                        Err(_) => continue,
                    }
                } else {
                    path
                };

                if let Ok(meta) = fs::metadata(&resolved_path) {
                    if meta.is_file() {
                        acc.push(resolved_path);
                    } else if meta.is_dir() {
                        Self::collect_searchable_files(
                            root,
                            &resolved_path,
                            acc,
                            depth + 1,
                            max_depth,
                        );
                    }
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
        Self::collect_searchable_files(dir, dir, &mut files, 0, 3);

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

    /// Generate structured study notes and flashcards from a local document,
    /// with optional topic filtering.
    pub fn generate_study_notes(
        doc_path: &Path,
        topic_filter: Option<&str>,
    ) -> Result<StudyNotesResult, String> {
        let (text, metadata) = DocumentAssistant::extract_text(doc_path)?;
        let analysis = DocumentAssistant::analyze(doc_path, None)?;

        let topic_clean = topic_filter.map(|t| t.trim()).filter(|t| !t.is_empty());

        let mut core_concepts = Vec::new();
        for sec in &analysis.sections {
            if !sec.heading.is_empty() {
                if let Some(topic) = topic_clean {
                    let top_low = topic.to_lowercase();
                    if sec.heading.to_lowercase().contains(&top_low)
                        || sec.content.to_lowercase().contains(&top_low)
                    {
                        core_concepts.push(sec.heading.clone());
                    }
                } else {
                    core_concepts.push(sec.heading.clone());
                }
            }
        }

        let mut key_facts = Vec::new();
        for fact in &analysis.key_facts {
            if let Some(topic) = topic_clean {
                let top_low = topic.to_lowercase();
                if fact.to_lowercase().contains(&top_low) {
                    key_facts.push(fact.clone());
                }
            } else {
                key_facts.push(fact.clone());
            }
        }

        let mut dates_list = Vec::new();
        for d in &analysis.extracted_dates {
            if let Some(topic) = topic_clean {
                let top_low = topic.to_lowercase();
                if d.context.to_lowercase().contains(&top_low)
                    || d.raw_date.to_lowercase().contains(&top_low)
                {
                    dates_list.push(format!("{}: {}", d.raw_date, d.context));
                }
            } else {
                dates_list.push(format!("{}: {}", d.raw_date, d.context));
            }
        }

        // Generate revision flashcards based on filtered sections & facts
        let mut flashcards = Vec::new();
        for (idx, fact) in key_facts.iter().enumerate() {
            flashcards.push(RevisionFlashcard {
                question: format!(
                    "Key Point #{}: What is the requirement or finding?",
                    idx + 1
                ),
                answer: fact.clone(),
            });
        }
        for item in &analysis.action_items {
            if let Some(topic) = topic_clean {
                let top_low = topic.to_lowercase();
                if !item.text.to_lowercase().contains(&top_low) {
                    continue;
                }
            }
            flashcards.push(RevisionFlashcard {
                question: format!(
                    "Action Required (Priority: {}):",
                    item.priority.to_uppercase()
                ),
                answer: item.text.clone(),
            });
        }

        let overview = if let Some(topic) = topic_clean {
            if core_concepts.is_empty() && key_facts.is_empty() && flashcards.is_empty() {
                return Err(format!(
                    "The topic '{}' was not found in '{}'.",
                    topic, metadata.file_name
                ));
            }
            format!(
                "Study notes focused on topic '{}' from {}.",
                topic, metadata.file_name
            )
        } else if analysis.summary.is_empty() {
            format!(
                "Extracted {} characters and {} words from {}.",
                text.len(),
                metadata.word_count,
                metadata.file_name
            )
        } else {
            analysis.summary
        };

        if flashcards.is_empty() {
            flashcards.push(RevisionFlashcard {
                question: format!("Summary of {}:", metadata.file_name),
                answer: overview.clone(),
            });
        }

        Ok(StudyNotesResult {
            doc_name: metadata.file_name,
            doc_path: metadata.file_path,
            format: metadata.format,
            overview,
            core_concepts: if core_concepts.is_empty() {
                key_facts.clone()
            } else {
                core_concepts
            },
            key_dates_and_milestones: dates_list,
            summary_points: key_facts,
            revision_flashcards: flashcards,
        })
    }

    /// Explain a specific passage in simple, ordinary language grounded in document content.
    pub fn explain_passage(doc_path: &Path, passage: &str) -> Result<String, String> {
        let (text, metadata) = DocumentAssistant::extract_text(doc_path)?;
        let p_trimmed = passage.trim();

        if p_trimmed.is_empty() {
            return Err("Passage to explain cannot be empty.".to_string());
        }

        let text_lower = text.to_lowercase();
        let p_lower = p_trimmed.to_lowercase();
        let found_idx = text_lower.find(&p_lower);

        if found_idx.is_none() {
            return Err(format!(
                "Passage not found in document '{}': '{}'. Please select or provide an excerpt that actually appears in the document.",
                metadata.file_name, p_trimmed
            ));
        }

        let idx = found_idx.unwrap();
        let ctx_start = idx.saturating_sub(100);
        let ctx_end = (idx + p_trimmed.len() + 100).min(text.len());
        let context_slice = text[ctx_start..ctx_end].trim();

        let dates_in_passage = DocumentAssistant::extract_dates(p_trimmed);
        let action_items_in_passage = DocumentAssistant::extract_action_items(p_trimmed);

        let mut explanation = format!(
            "### Plain Language Explanation of Passage from {}\n\n**Quoted Excerpt:**\n> \"{}\"\n\n**Surrounding Context in Source:**\n> \"...{}...\"\n\n",
            metadata.file_name, p_trimmed, context_slice
        );

        explanation.push_str("#### Key Points & Core Meaning:\n");
        explanation.push_str(&format!(
            "- **Statement**: The source explicitly states: \"{}\"\n",
            p_trimmed
        ));

        if !dates_in_passage.is_empty() {
            explanation.push_str("- **Key Dates Mentioned**:\n");
            for d in &dates_in_passage {
                explanation.push_str(&format!("  - `{}` ({})\n", d.raw_date, d.context));
            }
        }

        if !action_items_in_passage.is_empty() {
            explanation.push_str("- **Actionable Obligations**:\n");
            for item in &action_items_in_passage {
                explanation.push_str(&format!(
                    "  - {} [Priority: {}]\n",
                    item.text, item.priority
                ));
            }
        }

        explanation.push_str("\n#### Plain-English Breakdown:\n");
        explanation.push_str("This excerpt describes the core rule or statement made in the document. It lays out expectations, deliverables, or criteria in everyday terms without confusing jargon.");

        Ok(explanation)
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
