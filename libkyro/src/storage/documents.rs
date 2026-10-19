//! Local document analysis subsystem with text and PDF extraction,
//! deterministic date/action item parsing, document comparison,
//! privacy disclosure, and structured artifact generation.

use flate2::read::ZlibDecoder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::Path;

/// Metadata extracted from a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub file_name: String,
    pub file_path: String,
    pub format: String, // "markdown", "text", "pdf", "json", "csv", "source_code", "unsupported"
    pub size_bytes: u64,
    pub page_count: Option<usize>,
    pub word_count: usize,
    pub character_count: usize,
    pub is_local_only: bool,
}

/// An extracted action item with its source context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionItem {
    pub text: String,
    pub priority: String, // "high", "medium", "normal"
    pub line_number: Option<usize>,
}

/// A structured date mention found in the document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtractedDate {
    pub raw_date: String,
    pub context: String,
}

/// A section extracted from the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSection {
    pub heading: String,
    pub content: String,
}

/// The complete structured outcome of document analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentAnalysisResult {
    pub metadata: DocumentMetadata,
    pub summary: String,
    pub key_facts: Vec<String>,
    pub action_items: Vec<ActionItem>,
    pub extracted_dates: Vec<ExtractedDate>,
    pub missing_or_ambiguous: Vec<String>,
    pub sections: Vec<DocumentSection>,
    pub privacy_disclosure: String,
    pub raw_text: String,
}

/// The outcome of comparing two documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentComparisonResult {
    pub doc_a_path: String,
    pub doc_b_path: String,
    pub summary_differences: String,
    pub common_topics: Vec<String>,
    pub added_in_b: Vec<String>,
    pub removed_from_a: Vec<String>,
    pub action_items_comparison: Vec<String>,
    pub markdown_report: String,
}

pub struct DocumentAssistant;

impl DocumentAssistant {
    /// Detects format and checks if Kyro can extract text from the file.
    pub fn detect_format(path: &Path) -> &'static str {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "pdf" => "pdf",
            "md" | "markdown" => "markdown",
            "txt" | "text" | "log" | "rst" => "text",
            "json" => "json",
            "csv" | "tsv" => "csv",
            "yaml" | "yml" | "toml" | "xml" => "structured_data",
            "rs" | "ts" | "js" | "py" | "go" | "c" | "cpp" | "java" | "sh" | "html" | "css" => {
                "source_code"
            }
            _ => "unsupported",
        }
    }

    /// Extracts text from a supported local document file (plain text or searchable PDF).
    pub fn extract_text(path: &Path) -> Result<(String, DocumentMetadata), String> {
        if !path.exists() {
            return Err(format!("Document '{}' does not exist.", path.display()));
        }

        let metadata_fs = fs::metadata(path)
            .map_err(|e| format!("Failed to inspect '{}': {}", path.display(), e))?;
        let size_bytes = metadata_fs.len();
        let format_name = Self::detect_format(path);

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document")
            .to_string();
        let file_path = path.to_string_lossy().to_string();

        if format_name == "unsupported" {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("unknown");
            return Err(format!(
                "Unsupported document format '.{}'. Kyro Document Assistant processes text documents (.txt, .md, .json, .csv, code) and searchable PDFs.",
                ext
            ));
        }

        let (text, page_count) = if format_name == "pdf" {
            let bytes = fs::read(path)
                .map_err(|e| format!("Failed to read PDF '{}': {}", path.display(), e))?;
            Self::extract_pdf_text(&bytes)?
        } else {
            let s = fs::read_to_string(path)
                .map_err(|e| format!("Failed to read text file '{}': {}", path.display(), e))?;
            (s, None)
        };

        let words: Vec<&str> = text.split_whitespace().collect();
        let char_count = text.chars().count();

        let doc_metadata = DocumentMetadata {
            file_name,
            file_path,
            format: format_name.to_string(),
            size_bytes,
            page_count,
            word_count: words.len(),
            character_count: char_count,
            is_local_only: true,
        };

        Ok((text, doc_metadata))
    }

    /// Robust PDF stream text extractor with FlateDecode decompression.
    pub fn extract_pdf_text(bytes: &[u8]) -> Result<(String, Option<usize>), String> {
        if bytes.len() < 5 || &bytes[0..5] != b"%PDF-" {
            return Err("Not a valid PDF file (missing %PDF- header).".to_string());
        }

        // Count approximate pages by searching for /Type /Page (excluding /Pages)
        let page_regex = Regex::new(r"/Type\s*/Page\b").unwrap();
        let bytes_as_lossy = String::from_utf8_lossy(bytes);
        let page_count = page_regex.find_iter(&bytes_as_lossy).count();
        let page_count_opt = if page_count > 0 {
            Some(page_count)
        } else {
            None
        };

        if bytes_as_lossy.contains("/Encrypt") {
            return Err(
                "PDF is encrypted or password-protected. Kyro cannot extract text from encrypted files without authorization."
                    .to_string(),
            );
        }

        let mut extracted_text = String::new();

        // Find streams in the PDF
        let stream_start_marker = b"stream\r\n";
        let stream_start_marker_lf = b"stream\n";
        let stream_end_marker = b"endstream";

        let mut pos = 0;
        while pos < bytes.len() {
            // Find "stream"
            let start_idx = if let Some(idx) = bytes[pos..]
                .windows(stream_start_marker.len())
                .position(|w| w == stream_start_marker)
            {
                pos + idx + stream_start_marker.len()
            } else if let Some(idx) = bytes[pos..]
                .windows(stream_start_marker_lf.len())
                .position(|w| w == stream_start_marker_lf)
            {
                pos + idx + stream_start_marker_lf.len()
            } else {
                break;
            };

            // Find "endstream"
            let end_idx = match bytes[start_idx..]
                .windows(stream_end_marker.len())
                .position(|w| w == stream_end_marker)
            {
                Some(idx) => start_idx + idx,
                None => break,
            };

            let stream_data = &bytes[start_idx..end_idx];

            // Inspect preceding dictionary to see if FlateDecode is used
            let dict_start = pos;
            let dict_slice = &bytes[dict_start..start_idx];
            let dict_str = String::from_utf8_lossy(dict_slice);

            let decompressed = if dict_str.contains("/FlateDecode") {
                let mut decoder = ZlibDecoder::new(stream_data);
                let mut decompressed_buf = Vec::new();
                if decoder.read_to_end(&mut decompressed_buf).is_ok() {
                    decompressed_buf
                } else {
                    // Try raw deflate fallback
                    let mut raw_decoder = flate2::read::DeflateDecoder::new(stream_data);
                    let mut raw_buf = Vec::new();
                    if raw_decoder.read_to_end(&mut raw_buf).is_ok() {
                        raw_buf
                    } else {
                        stream_data.to_vec()
                    }
                }
            } else {
                stream_data.to_vec()
            };

            // Extract text from text blocks (BT ... ET)
            let stream_text = Self::extract_text_operators(&decompressed);
            if !stream_text.trim().is_empty() {
                if !extracted_text.is_empty() {
                    extracted_text.push('\n');
                }
                extracted_text.push_str(&stream_text);
            }

            pos = end_idx + stream_end_marker.len();
        }

        if extracted_text.trim().is_empty() {
            return Err(
                "PDF contains scanned raster images without a readable text stream. Kyro cannot extract text from image-only PDFs."
                    .to_string(),
            );
        }

        Ok((extracted_text, page_count_opt))
    }

    /// Parses PDF text operators inside BT ... ET blocks: Tj, TJ, ', "
    fn extract_text_operators(decompressed: &[u8]) -> String {
        let content = String::from_utf8_lossy(decompressed);
        let mut result = String::new();

        // Extract blocks between BT and ET
        let mut in_bt = false;
        let mut bt_buf = String::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "BT" || trimmed.ends_with(" BT") {
                in_bt = true;
                bt_buf.clear();
                continue;
            }
            if trimmed == "ET" || trimmed.ends_with(" ET") {
                in_bt = false;
                Self::parse_bt_block(&bt_buf, &mut result);
                bt_buf.clear();
                continue;
            }
            if in_bt {
                bt_buf.push_str(line);
                bt_buf.push('\n');
            }
        }

        if in_bt && !bt_buf.is_empty() {
            Self::parse_bt_block(&bt_buf, &mut result);
        }

        result
    }

    fn parse_bt_block(block: &str, out: &mut String) {
        // Regex for (text) Tj
        let re_tj = Regex::new(r"\((.*?)\)\s*Tj").unwrap();
        for cap in re_tj.captures_iter(block) {
            if let Some(m) = cap.get(1) {
                let decoded = Self::decode_pdf_string(m.as_str());
                if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
                    out.push(' ');
                }
                out.push_str(&decoded);
            }
        }

        // Regex for [(t1) 120 (t2)] TJ
        let re_array_tj = Regex::new(r"\[(.*?)\]\s*TJ").unwrap();
        let re_str_inside_array = Regex::new(r"\((.*?)\)").unwrap();
        for cap in re_array_tj.captures_iter(block) {
            if let Some(array_content) = cap.get(1) {
                for inner_cap in re_str_inside_array.captures_iter(array_content.as_str()) {
                    if let Some(m) = inner_cap.get(1) {
                        let decoded = Self::decode_pdf_string(m.as_str());
                        out.push_str(&decoded);
                    }
                }
                out.push(' ');
            }
        }
    }

    /// Decodes standard PDF escape sequences.
    fn decode_pdf_string(s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next) = chars.next() {
                    match next {
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        '(' => result.push('('),
                        ')' => result.push(')'),
                        '\\' => result.push('\\'),
                        '0'..='7' => {
                            let mut octal = String::new();
                            octal.push(next);
                            for _ in 0..2 {
                                if let Some(&peeked) = chars.peek() {
                                    if ('0'..='7').contains(&peeked) {
                                        octal.push(chars.next().unwrap());
                                    } else {
                                        break;
                                    }
                                }
                            }
                            if let Ok(val) = u8::from_str_radix(&octal, 8) {
                                result.push(val as char);
                            }
                        }
                        other => result.push(other),
                    }
                }
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Deterministically extracts date occurrences from plain text.
    pub fn extract_dates(text: &str) -> Vec<ExtractedDate> {
        let mut dates = Vec::new();
        let mut seen = HashSet::new();

        // 1. ISO 8601 (YYYY-MM-DD)
        let re_iso = Regex::new(r"\b(\d{4}-\d{2}-\d{2})\b").unwrap();
        for cap in re_iso.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                let date_str = m.as_str().to_string();
                if seen.insert(date_str.clone()) {
                    let start = m.start().saturating_sub(25);
                    let end = (m.end() + 25).min(text.len());
                    dates.push(ExtractedDate {
                        raw_date: date_str,
                        context: text[start..end].trim().to_string(),
                    });
                }
            }
        }

        // 2. Formats like Month DD, YYYY or DD Month YYYY
        let re_verbal = Regex::new(
            r"\b((?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]* \d{1,2},? \d{4})\b",
        )
        .unwrap();
        for cap in re_verbal.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                let date_str = m.as_str().to_string();
                if seen.insert(date_str.clone()) {
                    let start = m.start().saturating_sub(25);
                    let end = (m.end() + 25).min(text.len());
                    dates.push(ExtractedDate {
                        raw_date: date_str,
                        context: text[start..end].trim().to_string(),
                    });
                }
            }
        }

        dates
    }

    /// Deterministically extracts action items and TODOs from plain text.
    pub fn extract_action_items(text: &str) -> Vec<ActionItem> {
        let mut items = Vec::new();

        let re_todo = Regex::new(r"(?i)\b(?:TODO|FIXME|ACTION|NEXT STEP):\s*(.+)").unwrap();
        let re_checkbox = Regex::new(r"-\s*\[\s*\]\s*(.+)").unwrap();
        let re_must_should =
            Regex::new(r"(?i)\b(?:we must|user must|should ensure|action required:)\s*(.+)")
                .unwrap();

        for (idx, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if let Some(cap) = re_todo.captures(trimmed) {
                items.push(ActionItem {
                    text: cap[1].trim().to_string(),
                    priority: if trimmed.contains("URGENT") || trimmed.contains("CRITICAL") {
                        "high".to_string()
                    } else {
                        "medium".to_string()
                    },
                    line_number: Some(idx + 1),
                });
            } else if let Some(cap) = re_checkbox.captures(trimmed) {
                items.push(ActionItem {
                    text: cap[1].trim().to_string(),
                    priority: "normal".to_string(),
                    line_number: Some(idx + 1),
                });
            } else if let Some(cap) = re_must_should.captures(trimmed) {
                items.push(ActionItem {
                    text: cap[1].trim().to_string(),
                    priority: "medium".to_string(),
                    line_number: Some(idx + 1),
                });
            }
        }

        items
    }

    /// Analyzes a document completely, distinguishing facts from interpretations,
    /// and generating a structured report.
    pub fn analyze(
        path: &Path,
        model_name_opt: Option<&str>,
    ) -> Result<DocumentAnalysisResult, String> {
        let (raw_text, metadata) = Self::extract_text(path)?;

        let extracted_dates = Self::extract_dates(&raw_text);
        let action_items = Self::extract_action_items(&raw_text);

        // Extract key sections (Markdown headers or paragraphs)
        let mut sections = Vec::new();
        let mut current_heading = "Overview".to_string();
        let mut current_buf = String::new();

        for line in raw_text.lines() {
            if line.starts_with('#') {
                if !current_buf.trim().is_empty() {
                    sections.push(DocumentSection {
                        heading: current_heading.clone(),
                        content: current_buf.trim().to_string(),
                    });
                    current_buf.clear();
                }
                current_heading = line.trim_start_matches('#').trim().to_string();
            } else {
                current_buf.push_str(line);
                current_buf.push('\n');
            }
        }
        if !current_buf.trim().is_empty() {
            sections.push(DocumentSection {
                heading: current_heading,
                content: current_buf.trim().to_string(),
            });
        }

        // Extracted facts: deterministic lines containing numbers, dates, or key labels
        let mut key_facts = Vec::new();
        for line in raw_text.lines().take(150) {
            let t = line.trim();
            if (t.contains(':')
                || t.contains('$')
                || t.contains('%')
                || t.contains("v1.")
                || t.contains("202"))
                && t.len() > 10
                && t.len() < 120
                && !t.starts_with('#')
            {
                key_facts.push(t.to_string());
                if key_facts.len() >= 8 {
                    break;
                }
            }
        }

        let privacy_disclosure = match model_name_opt {
            Some(model) => format!(
                "Local-first notice: Document parsed locally on-device. Analysis summary augmented via configured model '{}'.",
                model
            ),
            None => "100% Local-first: Text, dates, sections, and action items extracted strictly on-device without remote transmission.".to_string(),
        };

        let summary = format!(
            "Document '{}' ({} format, {} words, {} characters) parsed successfully. Contains {} structured sections, {} action items, and {} date mentions.",
            metadata.file_name,
            metadata.format,
            metadata.word_count,
            metadata.character_count,
            sections.len(),
            action_items.len(),
            extracted_dates.len(),
        );

        let mut missing = Vec::new();
        if action_items.is_empty() {
            missing
                .push("No explicit action items or TODO tasks detected in document.".to_string());
        }
        if extracted_dates.is_empty() {
            missing.push("No deadlines or explicit calendar dates found.".to_string());
        }

        Ok(DocumentAnalysisResult {
            metadata,
            summary,
            key_facts,
            action_items,
            extracted_dates,
            missing_or_ambiguous: missing,
            sections,
            privacy_disclosure,
            raw_text,
        })
    }

    /// Compares two documents, extracting common topics, additions, removals, and action differences.
    pub fn compare(path_a: &Path, path_b: &Path) -> Result<DocumentComparisonResult, String> {
        let (text_a, meta_a) = Self::extract_text(path_a)?;
        let (text_b, meta_b) = Self::extract_text(path_b)?;

        let lines_a: HashSet<&str> = text_a
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        let lines_b: HashSet<&str> = text_b
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        let added: Vec<String> = lines_b
            .difference(&lines_a)
            .take(10)
            .map(|s| s.to_string())
            .collect();

        let removed: Vec<String> = lines_a
            .difference(&lines_b)
            .take(10)
            .map(|s| s.to_string())
            .collect();

        let common: Vec<String> = lines_a
            .intersection(&lines_b)
            .filter(|l| l.len() > 15)
            .take(5)
            .map(|s| s.to_string())
            .collect();

        let actions_a = Self::extract_action_items(&text_a);
        let actions_b = Self::extract_action_items(&text_b);

        let mut action_diffs = Vec::new();
        for b_act in &actions_b {
            if !actions_a.iter().any(|a| a.text == b_act.text) {
                action_diffs.push(format!("New action item: {}", b_act.text));
            }
        }
        for a_act in &actions_a {
            if !actions_b.iter().any(|b| b.text == a_act.text) {
                action_diffs.push(format!("Completed/removed action: {}", a_act.text));
            }
        }

        let summary = format!(
            "Compared '{}' ({} bytes) with '{}' ({} bytes). Found {} new distinct lines, {} removed lines, and {} action item differences.",
            meta_a.file_name, meta_a.size_bytes, meta_b.file_name, meta_b.size_bytes, added.len(), removed.len(), action_diffs.len()
        );

        let report = format!(
            "# Document Comparison Report\n\n\
**Document A:** `{}`\n\
**Document B:** `{}`\n\n\
## Summary\n{}\n\n\
## Added Content (Document B)\n{}\n\n\
## Removed Content (Document A)\n{}\n\n\
## Action Items Delta\n{}\n",
            meta_a.file_path,
            meta_b.file_path,
            summary,
            if added.is_empty() {
                "No new lines detected.".to_string()
            } else {
                added
                    .iter()
                    .map(|s| format!("+ {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
            if removed.is_empty() {
                "No lines removed.".to_string()
            } else {
                removed
                    .iter()
                    .map(|s| format!("- {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
            if action_diffs.is_empty() {
                "Action items identical across documents.".to_string()
            } else {
                action_diffs
                    .iter()
                    .map(|s| format!("* {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        );

        Ok(DocumentComparisonResult {
            doc_a_path: meta_a.file_path,
            doc_b_path: meta_b.file_path,
            summary_differences: summary,
            common_topics: common,
            added_in_b: added,
            removed_from_a: removed,
            action_items_comparison: action_diffs,
            markdown_report: report,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date_and_action_item_extraction() {
        let text = "\
# Project Kickoff
Meeting date: 2026-03-15
Deadline is October 21, 2026 for phase 1.

## Tasks
TODO: Finalize security review
- [ ] Prepare deployment pipeline
ACTION: Review database migrations
We must update API keys before release.
";

        let dates = DocumentAssistant::extract_dates(text);
        assert_eq!(dates.len(), 2);
        assert_eq!(dates[0].raw_date, "2026-03-15");
        assert_eq!(dates[1].raw_date, "October 21, 2026");

        let actions = DocumentAssistant::extract_action_items(text);
        assert_eq!(actions.len(), 4);
        assert_eq!(actions[0].text, "Finalize security review");
        assert_eq!(actions[1].text, "Prepare deployment pipeline");
        assert_eq!(actions[2].text, "Review database migrations");
        assert!(actions[3].text.contains("update API keys"));
    }

    #[test]
    fn test_unsupported_format_reporting_honesty() {
        let temp =
            std::env::temp_dir().join(format!("kyro_unsupported_{}.bin", uuid::Uuid::new_v4()));
        fs::write(&temp, b"\x00\x01\x02\x03\x04").unwrap();

        let res = DocumentAssistant::extract_text(&temp);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.contains("Unsupported document format '.bin'"));

        let _ = fs::remove_file(&temp);
    }

    #[test]
    fn test_pdf_extraction_and_header_validation() {
        // Corrupt non-PDF bytes
        let invalid_pdf = b"NOT A PDF";
        let res = DocumentAssistant::extract_pdf_text(invalid_pdf);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("missing %PDF- header"));

        // Valid minimal PDF text stream
        let mut pdf_bytes = Vec::new();
        pdf_bytes.extend_from_slice(b"%PDF-1.4\n1 0 obj\n<< /Type /Page >>\nendobj\n");
        pdf_bytes.extend_from_slice(b"2 0 obj\n<< /Length 40 >>\nstream\nBT\n(Hello Kyro Document Assistant) Tj\nET\nendstream\nendobj\n");
        pdf_bytes.extend_from_slice(b"%%EOF\n");

        let (extracted, pages) =
            DocumentAssistant::extract_pdf_text(&pdf_bytes).expect("Valid PDF text");
        assert!(extracted.contains("Hello Kyro Document Assistant"));
        assert_eq!(pages, Some(1));
    }

    #[test]
    fn test_document_comparison() {
        let temp_a = std::env::temp_dir().join(format!("doc_a_{}.md", uuid::Uuid::new_v4()));
        let temp_b = std::env::temp_dir().join(format!("doc_b_{}.md", uuid::Uuid::new_v4()));

        fs::write(
            &temp_a,
            "# Project Spec\nCommon section line here\n- [ ] Old task\n",
        )
        .unwrap();
        fs::write(
            &temp_b,
            "# Project Spec\nCommon section line here\n- [ ] New updated task\nFeature XYZ added\n",
        )
        .unwrap();

        let cmp = DocumentAssistant::compare(&temp_a, &temp_b).expect("Comparison succeeds");
        assert!(cmp.summary_differences.contains("Compared"));
        assert!(cmp
            .action_items_comparison
            .iter()
            .any(|a| a.contains("New updated task")));
        assert!(cmp
            .action_items_comparison
            .iter()
            .any(|a| a.contains("Old task")));
        assert!(cmp.markdown_report.contains("# Document Comparison Report"));

        let _ = fs::remove_file(&temp_a);
        let _ = fs::remove_file(&temp_b);
    }
}
