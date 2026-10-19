//! Safe file organization subsystem with preview, content-hash duplicate detection,
//! transaction manifests, path containment, and reversible undo.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A rule specifying how files are categorized and structured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationRule {
    pub name: String,
    pub custom_extensions: HashMap<String, String>,
    pub group_duplicates: bool,
    pub duplicates_folder_name: String,
    pub recursive: bool,
}

impl Default for OrganizationRule {
    fn default() -> Self {
        Self {
            name: "Default Category Rule".to_string(),
            custom_extensions: HashMap::new(),
            group_duplicates: true,
            duplicates_folder_name: "Duplicates".to_string(),
            recursive: false,
        }
    }
}

/// A planned single file operation within a proposed organization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedFileOp {
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub category: String,
    pub reason: String,
    pub file_size: u64,
    pub file_hash: Option<String>,
    pub is_duplicate: bool,
    pub duplicate_of: Option<PathBuf>,
    pub conflict: Option<String>,
}

/// A full preview plan ready for inspection by the user or agent before execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationPlan {
    pub plan_id: String,
    pub target_directory: PathBuf,
    pub total_scanned: usize,
    pub duplicates_count: usize,
    pub conflicts_count: usize,
    pub operations: Vec<PlannedFileOp>,
}

/// A recorded operation status in a transaction manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationOpRecord {
    pub source_path: String,
    pub destination_path: String,
    pub file_hash: String,
    pub file_size: u64,
    pub status: String, // "completed", "failed", "undone"
    pub error: Option<String>,
}

/// A durable transaction manifest containing everything needed to review or reverse moves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationTransaction {
    pub transaction_id: String,
    pub target_directory: String,
    pub created_at: i64,
    pub status: String, // "completed", "partially_failed", "undone"
    pub operations: Vec<OrganizationOpRecord>,
}

pub struct FileOrganizer;

impl FileOrganizer {
    /// Categorizes a filename extension into standard user-facing categories.
    pub fn categorize_extension(ext: &str, custom: &HashMap<String, String>) -> String {
        let ext_lower = ext.to_lowercase();
        if let Some(cat) = custom.get(&ext_lower) {
            return cat.clone();
        }

        match ext_lower.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tiff" | "ico" => {
                "Images".to_string()
            }
            "pdf" | "doc" | "docx" | "odt" | "rtf" | "tex" | "epub" => "Documents".to_string(),
            "txt" | "md" | "markdown" | "log" | "rst" => "Text".to_string(),
            "xls" | "xlsx" | "csv" | "tsv" | "ods" => "Spreadsheets".to_string(),
            "ppt" | "pptx" | "odp" | "key" => "Presentations".to_string(),
            "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => "Archives".to_string(),
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" => "Audio".to_string(),
            "mp4" | "mkv" | "mov" | "avi" | "webm" | "wmv" | "flv" => "Video".to_string(),
            "rs" | "ts" | "js" | "py" | "go" | "cpp" | "c" | "h" | "java" | "rb" | "php"
            | "html" | "css" | "json" | "yaml" | "yml" | "toml" | "xml" | "sh" => {
                "Code".to_string()
            }
            "iso" | "dmg" | "deb" | "rpm" | "exe" | "msi" | "apk" => "Installers".to_string(),
            _ => "Miscellaneous".to_string(),
        }
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

    /// Generates an inspection plan previewing all proposed moves and identifying conflicts.
    pub fn preview(
        root_dir: &Path,
        rule_opt: Option<OrganizationRule>,
    ) -> Result<OrganizationPlan, String> {
        if !root_dir.exists() {
            return Err(format!(
                "Target directory '{}' does not exist.",
                root_dir.display()
            ));
        }
        if !root_dir.is_dir() {
            return Err(format!(
                "Target path '{}' is not a directory.",
                root_dir.display()
            ));
        }

        let canonical_root = fs::canonicalize(root_dir)
            .map_err(|e| format!("Failed to canonicalize root directory: {}", e))?;

        let rule = rule_opt.unwrap_or_default();
        let mut entries = Vec::new();
        Self::collect_files(&canonical_root, &rule, &mut entries)?;
        entries.sort();

        let mut hash_map: HashMap<String, PathBuf> = HashMap::new();
        let mut planned_ops = Vec::new();
        let mut duplicates_count = 0;
        let mut conflicts_count = 0;

        for source_path in entries {
            let metadata = match fs::symlink_metadata(&source_path) {
                Ok(m) => m,
                Err(e) => {
                    conflicts_count += 1;
                    planned_ops.push(PlannedFileOp {
                        source_path: source_path.clone(),
                        destination_path: source_path.clone(),
                        category: "Error".to_string(),
                        reason: "Unreadable file metadata".to_string(),
                        file_size: 0,
                        file_hash: None,
                        is_duplicate: false,
                        duplicate_of: None,
                        conflict: Some(format!("Metadata error: {}", e)),
                    });
                    continue;
                }
            };

            // Symlink containment check
            if metadata.file_type().is_symlink() {
                let target = match fs::read_link(&source_path) {
                    Ok(t) => t,
                    Err(e) => {
                        conflicts_count += 1;
                        planned_ops.push(PlannedFileOp {
                            source_path: source_path.clone(),
                            destination_path: source_path.clone(),
                            category: "Symlink".to_string(),
                            reason: "Broken symbolic link".to_string(),
                            file_size: 0,
                            file_hash: None,
                            is_duplicate: false,
                            duplicate_of: None,
                            conflict: Some(format!("Broken link: {}", e)),
                        });
                        continue;
                    }
                };

                let resolved = if target.is_absolute() {
                    target
                } else {
                    source_path
                        .parent()
                        .unwrap_or(&canonical_root)
                        .join(&target)
                };

                if let Ok(canon_target) = fs::canonicalize(&resolved) {
                    if !canon_target.starts_with(&canonical_root) {
                        conflicts_count += 1;
                        planned_ops.push(PlannedFileOp {
                            source_path: source_path.clone(),
                            destination_path: source_path.clone(),
                            category: "Symlink".to_string(),
                            reason: "Symlink escapes target directory".to_string(),
                            file_size: 0,
                            file_hash: None,
                            is_duplicate: false,
                            duplicate_of: None,
                            conflict: Some(
                                "Security: Symlink target escapes directory boundary".to_string(),
                            ),
                        });
                        continue;
                    }
                }
            }

            let file_size = metadata.len();
            let file_hash = Self::compute_sha256(&source_path).ok();

            let mut is_duplicate = false;
            let mut duplicate_of = None;

            if let Some(ref hash) = file_hash {
                if let Some(orig_path) = hash_map.get(hash) {
                    is_duplicate = true;
                    duplicate_of = Some(orig_path.clone());
                    duplicates_count += 1;
                } else {
                    hash_map.insert(hash.clone(), source_path.clone());
                }
            }

            let file_name = match source_path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };

            let ext = source_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            let category = if is_duplicate && rule.group_duplicates {
                rule.duplicates_folder_name.clone()
            } else {
                Self::categorize_extension(ext, &rule.custom_extensions)
            };

            let target_subfolder = canonical_root.join(&category);
            let proposed_dest = target_subfolder.join(file_name);

            let mut conflict = None;
            // Prevent self-moves if file is already in correct category folder
            if proposed_dest == source_path {
                continue;
            }

            if proposed_dest.exists() {
                conflict = Some(format!(
                    "Destination already exists: '{}'",
                    proposed_dest.display()
                ));
                conflicts_count += 1;
            }

            let reason = if is_duplicate {
                format!(
                    "Duplicate file content identical to '{}'",
                    duplicate_of
                        .as_ref()
                        .map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or("original"))
                        .unwrap_or("original")
                )
            } else {
                format!("Categorized as '{}' by file extension '.{}'", category, ext)
            };

            planned_ops.push(PlannedFileOp {
                source_path,
                destination_path: proposed_dest,
                category,
                reason,
                file_size,
                file_hash,
                is_duplicate,
                duplicate_of,
                conflict,
            });
        }

        let plan_id = format!("org_plan_{}", uuid::Uuid::new_v4());
        Ok(OrganizationPlan {
            plan_id,
            target_directory: canonical_root,
            total_scanned: planned_ops.len(),
            duplicates_count,
            conflicts_count,
            operations: planned_ops,
        })
    }

    /// Helper to recursively or flatly collect eligible regular files.
    fn collect_files(
        dir: &Path,
        rule: &OrganizationRule,
        out: &mut Vec<PathBuf>,
    ) -> Result<(), String> {
        let read_dir =
            fs::read_dir(dir).map_err(|e| format!("Failed to read '{}': {}", dir.display(), e))?;

        for entry in read_dir.flatten() {
            let path = entry.path();
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    // Skip hidden dirs (e.g. .git, .kyro)
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with('.') {
                            continue;
                        }
                    }
                    if rule.recursive {
                        let _ = Self::collect_files(&path, rule, out);
                    }
                } else if file_type.is_file() || file_type.is_symlink() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if !name.starts_with('.') {
                            out.push(path);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Executes an inspected organization plan, enforcing path containment and recording a transaction manifest.
    pub fn execute(plan: &OrganizationPlan) -> Result<OrganizationTransaction, String> {
        let canonical_root = fs::canonicalize(&plan.target_directory)
            .map_err(|e| format!("Failed to canonicalize target directory: {}", e))?;

        let mut op_records = Vec::new();
        let mut has_partial_failure = false;

        for op in &plan.operations {
            // Skip operations with pre-existing conflicts
            if let Some(ref conflict_msg) = op.conflict {
                op_records.push(OrganizationOpRecord {
                    source_path: op.source_path.to_string_lossy().to_string(),
                    destination_path: op.destination_path.to_string_lossy().to_string(),
                    file_hash: op.file_hash.clone().unwrap_or_default(),
                    file_size: op.file_size,
                    status: "failed".to_string(),
                    error: Some(format!(
                        "Skipped due to pre-existing conflict: {}",
                        conflict_msg
                    )),
                });
                has_partial_failure = true;
                continue;
            }

            // Path containment enforcement
            if !op.source_path.starts_with(&canonical_root) {
                op_records.push(OrganizationOpRecord {
                    source_path: op.source_path.to_string_lossy().to_string(),
                    destination_path: op.destination_path.to_string_lossy().to_string(),
                    file_hash: op.file_hash.clone().unwrap_or_default(),
                    file_size: op.file_size,
                    status: "failed".to_string(),
                    error: Some("Security rejection: source path escapes target root".to_string()),
                });
                has_partial_failure = true;
                continue;
            }
            if !op.destination_path.starts_with(&canonical_root) {
                op_records.push(OrganizationOpRecord {
                    source_path: op.source_path.to_string_lossy().to_string(),
                    destination_path: op.destination_path.to_string_lossy().to_string(),
                    file_hash: op.file_hash.clone().unwrap_or_default(),
                    file_size: op.file_size,
                    status: "failed".to_string(),
                    error: Some(
                        "Security rejection: destination path escapes target root".to_string(),
                    ),
                });
                has_partial_failure = true;
                continue;
            }

            // Ensure destination directory exists
            if let Some(dest_dir) = op.destination_path.parent() {
                if let Err(e) = fs::create_dir_all(dest_dir) {
                    op_records.push(OrganizationOpRecord {
                        source_path: op.source_path.to_string_lossy().to_string(),
                        destination_path: op.destination_path.to_string_lossy().to_string(),
                        file_hash: op.file_hash.clone().unwrap_or_default(),
                        file_size: op.file_size,
                        status: "failed".to_string(),
                        error: Some(format!("Failed to create destination folder: {}", e)),
                    });
                    has_partial_failure = true;
                    continue;
                }
            }

            // Verify source still exists before moving
            if !op.source_path.exists() {
                op_records.push(OrganizationOpRecord {
                    source_path: op.source_path.to_string_lossy().to_string(),
                    destination_path: op.destination_path.to_string_lossy().to_string(),
                    file_hash: op.file_hash.clone().unwrap_or_default(),
                    file_size: op.file_size,
                    status: "failed".to_string(),
                    error: Some("Source file no longer exists at original path".to_string()),
                });
                has_partial_failure = true;
                continue;
            }

            // Prevent collision overwrite
            if op.destination_path.exists() {
                op_records.push(OrganizationOpRecord {
                    source_path: op.source_path.to_string_lossy().to_string(),
                    destination_path: op.destination_path.to_string_lossy().to_string(),
                    file_hash: op.file_hash.clone().unwrap_or_default(),
                    file_size: op.file_size,
                    status: "failed".to_string(),
                    error: Some(
                        "Destination file already exists (preventing overwrite)".to_string(),
                    ),
                });
                has_partial_failure = true;
                continue;
            }

            // Perform atomic or cross-filesystem move
            match fs::rename(&op.source_path, &op.destination_path) {
                Ok(_) => {
                    op_records.push(OrganizationOpRecord {
                        source_path: op.source_path.to_string_lossy().to_string(),
                        destination_path: op.destination_path.to_string_lossy().to_string(),
                        file_hash: op.file_hash.clone().unwrap_or_default(),
                        file_size: op.file_size,
                        status: "completed".to_string(),
                        error: None,
                    });
                }
                Err(e) => {
                    // Attempt copy + remove fallback for cross-filesystem moves
                    match fs::copy(&op.source_path, &op.destination_path) {
                        Ok(_) => {
                            let _ = fs::remove_file(&op.source_path);
                            op_records.push(OrganizationOpRecord {
                                source_path: op.source_path.to_string_lossy().to_string(),
                                destination_path: op.destination_path.to_string_lossy().to_string(),
                                file_hash: op.file_hash.clone().unwrap_or_default(),
                                file_size: op.file_size,
                                status: "completed".to_string(),
                                error: None,
                            });
                        }
                        Err(copy_err) => {
                            op_records.push(OrganizationOpRecord {
                                source_path: op.source_path.to_string_lossy().to_string(),
                                destination_path: op.destination_path.to_string_lossy().to_string(),
                                file_hash: op.file_hash.clone().unwrap_or_default(),
                                file_size: op.file_size,
                                status: "failed".to_string(),
                                error: Some(format!(
                                    "Move failed: {} (fallback copy failed: {})",
                                    e, copy_err
                                )),
                            });
                            has_partial_failure = true;
                        }
                    }
                }
            }
        }

        let status = if has_partial_failure {
            "partially_failed".to_string()
        } else {
            "completed".to_string()
        };

        Ok(OrganizationTransaction {
            transaction_id: format!("org_trx_{}", uuid::Uuid::new_v4()),
            target_directory: canonical_root.to_string_lossy().to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            status,
            operations: op_records,
        })
    }

    /// Reverses an executed organization transaction by safely moving files back to their original paths.
    pub fn undo(transaction: &mut OrganizationTransaction) -> Result<usize, String> {
        if transaction.status == "undone" {
            return Err("Transaction has already been undone.".to_string());
        }

        let mut reversed_count = 0;
        let mut dirs_to_prune: HashSet<PathBuf> = HashSet::new();

        // Process in reverse order
        for op in transaction.operations.iter_mut().rev() {
            if op.status != "completed" {
                continue;
            }

            let dest_path = PathBuf::from(&op.destination_path);
            let orig_path = PathBuf::from(&op.source_path);

            if !dest_path.exists() {
                op.error = Some(format!(
                    "Cannot undo: file no longer exists at destination '{}'",
                    dest_path.display()
                ));
                continue;
            }

            // Verify content hash before reversing to ensure file wasn't modified after move
            if !op.file_hash.is_empty() {
                if let Ok(current_hash) = Self::compute_sha256(&dest_path) {
                    if current_hash != op.file_hash {
                        op.error = Some(format!(
                            "Cannot undo: file at '{}' has been modified since organization (hash mismatch)",
                            dest_path.display()
                        ));
                        continue;
                    }
                }
            }

            // Verify original location is clear
            if orig_path.exists() {
                op.error = Some(format!(
                    "Cannot undo: original path '{}' is now occupied by another file",
                    orig_path.display()
                ));
                continue;
            }

            if let Some(parent) = orig_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            match fs::rename(&dest_path, &orig_path) {
                Ok(_) => {
                    op.status = "undone".to_string();
                    reversed_count += 1;
                    if let Some(dest_dir) = dest_path.parent() {
                        dirs_to_prune.insert(dest_dir.to_path_buf());
                    }
                }
                Err(e) => {
                    // Fallback copy + remove
                    if fs::copy(&dest_path, &orig_path).is_ok()
                        && fs::remove_file(&dest_path).is_ok()
                    {
                        op.status = "undone".to_string();
                        reversed_count += 1;
                        if let Some(dest_dir) = dest_path.parent() {
                            dirs_to_prune.insert(dest_dir.to_path_buf());
                        }
                    } else {
                        op.error = Some(format!("Failed to reverse move: {}", e));
                    }
                }
            }
        }

        // Clean up empty directories created by the organization
        for dir in dirs_to_prune {
            if dir.exists() {
                if let Ok(entries) = fs::read_dir(&dir) {
                    if entries.count() == 0 {
                        let _ = fs::remove_dir(&dir);
                    }
                }
            }
        }

        transaction.status = "undone".to_string();
        Ok(reversed_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_dir(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("kyro_org_test_{}_{}", name, uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn test_file_categorization_and_rules() {
        let custom = HashMap::from([("customext".to_string(), "CustomFolder".to_string())]);
        assert_eq!(
            FileOrganizer::categorize_extension("png", &custom),
            "Images"
        );
        assert_eq!(
            FileOrganizer::categorize_extension("pdf", &custom),
            "Documents"
        );
        assert_eq!(FileOrganizer::categorize_extension("txt", &custom), "Text");
        assert_eq!(FileOrganizer::categorize_extension("rs", &custom), "Code");
        assert_eq!(
            FileOrganizer::categorize_extension("customext", &custom),
            "CustomFolder"
        );
        assert_eq!(
            FileOrganizer::categorize_extension("unknownxyz", &custom),
            "Miscellaneous"
        );
    }

    #[test]
    fn test_full_organization_preview_execute_and_undo() {
        let temp = create_test_dir("lifecycle");
        let file1 = temp.join("report.pdf");
        let file2 = temp.join("photo.png");
        let file3 = temp.join("script.py");
        let dup = temp.join("photo_copy.png");

        fs::write(&file1, b"PDF document content").unwrap();
        fs::write(&file2, b"PNG image content identical").unwrap();
        fs::write(&file3, b"print('hello world')").unwrap();
        fs::write(&dup, b"PNG image content identical").unwrap(); // Exact same hash as file2

        // 1. Preview
        let plan = FileOrganizer::preview(&temp, None).expect("Preview should succeed");
        assert_eq!(plan.total_scanned, 4);
        assert_eq!(plan.duplicates_count, 1);
        assert_eq!(plan.conflicts_count, 0);

        // Verify operations in plan
        let dup_op = plan.operations.iter().find(|o| o.is_duplicate).unwrap();
        assert_eq!(dup_op.category, "Duplicates");

        // 2. Execute
        let mut trx = FileOrganizer::execute(&plan).expect("Execute should succeed");
        assert_eq!(trx.status, "completed");
        assert_eq!(
            trx.operations
                .iter()
                .filter(|o| o.status == "completed")
                .count(),
            4
        );

        // Verify files moved to correct subfolders
        assert!(!file1.exists());
        assert!(!file2.exists());
        assert!(temp.join("Documents").join("report.pdf").exists());
        assert!(temp.join("Images").join("photo.png").exists());
        assert!(temp.join("Code").join("script.py").exists());
        assert!(temp.join("Duplicates").join("photo_copy.png").exists());

        // 3. Undo
        let undone_count = FileOrganizer::undo(&mut trx).expect("Undo should succeed");
        assert_eq!(undone_count, 4);
        assert_eq!(trx.status, "undone");

        // Verify files returned to original location
        assert!(file1.exists());
        assert!(file2.exists());
        assert!(file3.exists());
        assert!(dup.exists());

        // Clean up
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_path_containment_and_conflict_detection() {
        let temp = create_test_dir("conflicts");
        let file_a = temp.join("item.txt");
        fs::write(&file_a, b"sample text").unwrap();

        // Create a pre-existing destination folder and file to force conflict
        let text_dir = temp.join("Text");
        fs::create_dir_all(&text_dir).unwrap();
        let conflict_dest = text_dir.join("item.txt");
        fs::write(&conflict_dest, b"already exists").unwrap();

        let plan = FileOrganizer::preview(&temp, None).expect("Preview should succeed");
        assert_eq!(plan.conflicts_count, 1);

        let conflict_op = plan
            .operations
            .iter()
            .find(|o| o.source_path == file_a)
            .unwrap();
        assert!(conflict_op.conflict.is_some());
        assert!(conflict_op
            .conflict
            .as_ref()
            .unwrap()
            .contains("Destination already exists"));

        // Execute must not overwrite the existing file
        let trx = FileOrganizer::execute(&plan).expect("Execute should handle conflict safely");
        assert_eq!(trx.status, "partially_failed");
        assert_eq!(fs::read(&conflict_dest).unwrap(), b"already exists");

        let _ = fs::remove_dir_all(&temp);
    }
}
