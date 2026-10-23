//! Format-aware document chunking subsystem for deep local RAG.
//!
//! Provides intelligent splitting for Markdown, source code, and plain text documents
//! while preserving accurate 1-based line ranges, symbol headings, and content hashes.

use crate::storage::DocumentChunkRecord;
use sha2::{Digest, Sha256};

const MAX_CHUNK_CHARS: usize = 1200;
const MIN_CHUNK_CHARS: usize = 150;

/// Chunks a document into bounded, format-aware chunks with accurate line numbers.
pub fn chunk_document(path: &str, content: &str, format: &str) -> Vec<DocumentChunkRecord> {
    if content.trim().is_empty() {
        return Vec::new();
    }

    match format {
        "markdown" => chunk_markdown(path, content),
        "source_code" => chunk_source_code(path, content),
        _ => chunk_plain_text(path, content, format),
    }
}

/// Computes SHA-256 hex digest of a string.
pub fn compute_sha256(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let hash = hasher.finalize();
    hash.iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

/// Format-aware chunker for Markdown documents.
///
/// Preserves header breadcrumbs (e.g., "Overview > Architecture") and splits on
/// heading boundaries or paragraph boundaries when sections exceed budget.
pub fn chunk_markdown(path: &str, content: &str) -> Vec<DocumentChunkRecord> {
    let doc_hash = compute_sha256(content);
    let mut chunks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let now = chrono::Utc::now().timestamp_millis();

    let mut current_heading_stack: Vec<(usize, String)> = Vec::new();
    let mut current_chunk_lines: Vec<&str> = Vec::new();
    let mut chunk_start_line = 1;
    let mut current_char_count = 0;
    let mut chunk_index = 0;

    let flush_chunk = |chunks: &mut Vec<DocumentChunkRecord>,
                       current_chunk_lines: &mut Vec<&str>,
                       chunk_start_line: usize,
                       chunk_end_line: usize,
                       heading_stack: &[(usize, String)],
                       chunk_index: &mut usize| {
        let text = current_chunk_lines.join("\n").trim().to_string();
        if !text.is_empty() {
            let heading_str = if heading_stack.is_empty() {
                None
            } else {
                Some(
                    heading_stack
                        .iter()
                        .map(|(_, h)| h.as_str())
                        .collect::<Vec<&str>>()
                        .join(" > "),
                )
            };
            let content_hash = compute_sha256(&text);
            let id = format!("{}_{}", &doc_hash[..12], *chunk_index);

            chunks.push(DocumentChunkRecord {
                id,
                document_path: path.to_string(),
                chunk_index: *chunk_index,
                heading: heading_str,
                start_line: Some(chunk_start_line),
                end_line: Some(chunk_end_line),
                content: text,
                content_hash,
                format: "markdown".to_string(),
                chunk_version: 1,
                indexed_at: now,
                embedding_json: None,
            });
            *chunk_index += 1;
        }
        current_chunk_lines.clear();
    };

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        // Check if line is a Markdown header (#, ##, ###, etc.)
        if trimmed.starts_with('#') {
            let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
            if hash_count <= 6 && trimmed.chars().nth(hash_count) == Some(' ') {
                let header_title = trimmed[hash_count..].trim().to_string();

                if !current_chunk_lines.is_empty() {
                    let end_line = line_num.saturating_sub(1).max(chunk_start_line);
                    flush_chunk(
                        &mut chunks,
                        &mut current_chunk_lines,
                        chunk_start_line,
                        end_line,
                        &current_heading_stack,
                        &mut chunk_index,
                    );
                }

                while let Some(&(level, _)) = current_heading_stack.last() {
                    if level >= hash_count {
                        current_heading_stack.pop();
                    } else {
                        break;
                    }
                }
                current_heading_stack.push((hash_count, header_title));

                chunk_start_line = line_num;
                current_chunk_lines.push(line);
                current_char_count = line.len();
                continue;
            }
        }

        current_chunk_lines.push(line);
        current_char_count += line.len() + 1;

        // Split on section size budget
        if current_char_count >= MAX_CHUNK_CHARS
            && (trimmed.is_empty() || current_chunk_lines.len() >= 40)
        {
            flush_chunk(
                &mut chunks,
                &mut current_chunk_lines,
                chunk_start_line,
                line_num,
                &current_heading_stack,
                &mut chunk_index,
            );
            chunk_start_line = line_num + 1;
            current_char_count = 0;
        }
    }

    if !current_chunk_lines.is_empty() {
        let end_line = lines.len();
        flush_chunk(
            &mut chunks,
            &mut current_chunk_lines,
            chunk_start_line,
            end_line,
            &current_heading_stack,
            &mut chunk_index,
        );
    }

    chunks
}

/// Format-aware chunker for source code files.
///
/// Detects top-level declarations (functions, structs, classes, modules, interfaces)
/// and preserves accurate 1-based start and end line ranges.
pub fn chunk_source_code(path: &str, content: &str) -> Vec<DocumentChunkRecord> {
    let doc_hash = compute_sha256(content);
    let mut chunks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let now = chrono::Utc::now().timestamp_millis();

    let mut current_symbol: Option<String> = None;
    let mut current_chunk_lines: Vec<&str> = Vec::new();
    let mut chunk_start_line = 1;
    let mut current_char_count = 0;
    let mut chunk_index = 0;

    let flush_chunk = |chunks: &mut Vec<DocumentChunkRecord>,
                       current_chunk_lines: &mut Vec<&str>,
                       chunk_start_line: usize,
                       chunk_end_line: usize,
                       symbol: &Option<String>,
                       chunk_index: &mut usize| {
        let text = current_chunk_lines.join("\n").trim().to_string();
        if !text.is_empty() {
            let content_hash = compute_sha256(&text);
            let id = format!("{}_{}", &doc_hash[..12], *chunk_index);

            chunks.push(DocumentChunkRecord {
                id,
                document_path: path.to_string(),
                chunk_index: *chunk_index,
                heading: symbol.clone(),
                start_line: Some(chunk_start_line),
                end_line: Some(chunk_end_line),
                content: text,
                content_hash,
                format: "source_code".to_string(),
                chunk_version: 1,
                indexed_at: now,
                embedding_json: None,
            });
            *chunk_index += 1;
        }
        current_chunk_lines.clear();
    };

    let is_symbol_boundary = |line: &str| -> Option<String> {
        let trimmed = line.trim();
        let prefixes = [
            "pub fn ",
            "fn ",
            "pub async fn ",
            "async fn ",
            "pub struct ",
            "struct ",
            "pub enum ",
            "enum ",
            "pub trait ",
            "trait ",
            "pub mod ",
            "mod ",
            "export function ",
            "function ",
            "export default function ",
            "export class ",
            "class ",
            "export interface ",
            "interface ",
            "export type ",
            "type ",
            "def ",
            "async def ",
        ];

        for prefix in &prefixes {
            if trimmed.starts_with(prefix) {
                let first_line = trimmed.split('{').next().unwrap_or(trimmed).trim();
                return Some(first_line.to_string());
            }
        }
        None
    };

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;

        if let Some(symbol_name) = is_symbol_boundary(line) {
            if !current_chunk_lines.is_empty()
                && (current_symbol.is_some() || current_char_count >= MIN_CHUNK_CHARS)
            {
                let end_line = line_num.saturating_sub(1).max(chunk_start_line);
                flush_chunk(
                    &mut chunks,
                    &mut current_chunk_lines,
                    chunk_start_line,
                    end_line,
                    &current_symbol,
                    &mut chunk_index,
                );
                chunk_start_line = line_num;
                current_char_count = 0;
            }
            current_symbol = Some(symbol_name);
        }

        current_chunk_lines.push(line);
        current_char_count += line.len() + 1;

        if current_char_count >= MAX_CHUNK_CHARS
            && (line.trim().is_empty() || current_chunk_lines.len() >= 50)
        {
            flush_chunk(
                &mut chunks,
                &mut current_chunk_lines,
                chunk_start_line,
                line_num,
                &current_symbol,
                &mut chunk_index,
            );
            chunk_start_line = line_num + 1;
            current_char_count = 0;
        }
    }

    if !current_chunk_lines.is_empty() {
        let end_line = lines.len();
        flush_chunk(
            &mut chunks,
            &mut current_chunk_lines,
            chunk_start_line,
            end_line,
            &current_symbol,
            &mut chunk_index,
        );
    }

    chunks
}

/// Chunk plain text or other extracted documents on paragraph boundaries.
pub fn chunk_plain_text(path: &str, content: &str, format: &str) -> Vec<DocumentChunkRecord> {
    let doc_hash = compute_sha256(content);
    let mut chunks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let now = chrono::Utc::now().timestamp_millis();

    let mut current_chunk_lines: Vec<&str> = Vec::new();
    let mut chunk_start_line = 1;
    let mut current_char_count = 0;
    let mut chunk_index = 0;

    let flush_chunk = |chunks: &mut Vec<DocumentChunkRecord>,
                       current_chunk_lines: &mut Vec<&str>,
                       chunk_start_line: usize,
                       chunk_end_line: usize,
                       chunk_index: &mut usize| {
        let text = current_chunk_lines.join("\n").trim().to_string();
        if !text.is_empty() {
            let content_hash = compute_sha256(&text);
            let id = format!("{}_{}", &doc_hash[..12], *chunk_index);

            chunks.push(DocumentChunkRecord {
                id,
                document_path: path.to_string(),
                chunk_index: *chunk_index,
                heading: None,
                start_line: Some(chunk_start_line),
                end_line: Some(chunk_end_line),
                content: text,
                content_hash,
                format: format.to_string(),
                chunk_version: 1,
                indexed_at: now,
                embedding_json: None,
            });
            *chunk_index += 1;
        }
        current_chunk_lines.clear();
    };

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        current_chunk_lines.push(line);
        current_char_count += line.len() + 1;

        if current_char_count >= MAX_CHUNK_CHARS
            && (line.trim().is_empty() || current_chunk_lines.len() >= 35)
        {
            flush_chunk(
                &mut chunks,
                &mut current_chunk_lines,
                chunk_start_line,
                line_num,
                &mut chunk_index,
            );
            chunk_start_line = line_num + 1;
            current_char_count = 0;
        }
    }

    if !current_chunk_lines.is_empty() {
        let end_line = lines.len();
        flush_chunk(
            &mut chunks,
            &mut current_chunk_lines,
            chunk_start_line,
            end_line,
            &mut chunk_index,
        );
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_chunking_with_heading_hierarchy() {
        let md = "# System Architecture\n\nIntroduction to the design.\n\n## Database Tier\n\nSQLite is used for storage.\n\n### Migrations\n\nVersioned schema migrations.";
        let chunks = chunk_markdown("/docs/arch.md", md);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].heading.as_deref(), Some("System Architecture"));
        assert_eq!(chunks[0].start_line, Some(1));
        assert!(chunks[0].content.contains("Introduction to the design"));

        assert_eq!(
            chunks[1].heading.as_deref(),
            Some("System Architecture > Database Tier")
        );
        assert!(chunks[1].content.contains("SQLite is used for storage"));

        assert_eq!(
            chunks[2].heading.as_deref(),
            Some("System Architecture > Database Tier > Migrations")
        );
        assert!(chunks[2].content.contains("Versioned schema migrations"));
    }

    #[test]
    fn test_source_code_chunking_with_symbols() {
        let code = "// Header comments\npub fn calculate_total(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub struct ServerConfig {\n    pub port: u16,\n}\n";
        let chunks = chunk_source_code("/src/math.rs", code);

        assert!(chunks.len() >= 2);
        assert!(chunks[0]
            .heading
            .as_ref()
            .unwrap()
            .contains("pub fn calculate_total"));
        assert_eq!(chunks[0].start_line, Some(1));
        assert!(chunks[1]
            .heading
            .as_ref()
            .unwrap()
            .contains("pub struct ServerConfig"));
    }

    #[test]
    fn test_empty_content_returns_no_chunks() {
        let chunks = chunk_document("/empty.txt", "   \n\n  ", "text");
        assert!(chunks.is_empty());
    }
}
