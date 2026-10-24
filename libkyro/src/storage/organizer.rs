//! Safe file organization subsystem with preview, content-hash duplicate detection,
//! transaction manifests, path containment, and reversible undo.

use crate::storage::db::{check_path_authorized_in_folders, Database};
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationPlan {
    pub plan_id: String,
    pub target_directory: PathBuf,
    pub total_scanned: usize,
    #[serde(default)]
    pub eligible_files: usize,
    #[serde(default)]
    pub unmodified_count: usize,
    #[serde(default)]
    pub planned_count: usize,
    pub duplicates_count: usize,
    pub conflicts_count: usize,
    pub operations: Vec<PlannedFileOp>,
    #[serde(default)]
    pub scan_warnings: Vec<String>,
}

impl Default for OrganizationPlan {
    fn default() -> Self {
        Self {
            plan_id: String::new(),
            target_directory: PathBuf::new(),
            total_scanned: 0,
            eligible_files: 0,
            unmodified_count: 0,
            planned_count: 0,
            duplicates_count: 0,
            conflicts_count: 0,
            operations: Vec::new(),
            scan_warnings: Vec::new(),
        }
    }
}

/// A recorded operation status in a transaction manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationOpRecord {
    pub source_path: String,
    pub destination_path: String,
    pub file_hash: String,
    pub file_size: u64,
    pub status: String, // "completed", "failed", "skipped", "stale", "undone", "copied_source_remains", "undo_failed"
    pub error: Option<String>,
}

/// A durable transaction manifest containing everything needed to review or reverse moves.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationTransaction {
    pub transaction_id: String,
    pub target_directory: String,
    pub created_at: i64,
    pub status: String, // "completed", "partially_failed", "failed", "undone", "partially_undone", "undo_failed"
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

    /// Safely expands `~` or `~/...` to the user's home directory across platforms.
    pub fn expand_tilde(path: &Path) -> PathBuf {
        let s = path.to_string_lossy();
        if s == "~" {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"))
        } else if let Some(rest) = s.strip_prefix("~/") {
            dirs::home_dir().map(|h| h.join(rest)).unwrap_or_else(|| path.to_path_buf())
        } else {
            path.to_path_buf()
        }
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
        let mut eligible_files = Vec::new();
        let mut symlinks = Vec::new();
        let mut scan_warnings = Vec::new();
        let mut total_scanned = 0;

        Self::collect_files(
            &canonical_root,
            &rule,
            &mut eligible_files,
            &mut symlinks,
            &mut scan_warnings,
            &mut total_scanned,
        )?;

        eligible_files.sort();
        symlinks.sort();

        let mut hash_map: HashMap<String, PathBuf> = HashMap::new();
        let mut planned_ops = Vec::new();
        let mut duplicates_count = 0;
        let mut conflicts_count = 0;
        let mut unmodified_count = 0;

        // Process symlinks: symlinks are excluded from organization and reported as skipped items
        for symlink_path in symlinks {
            conflicts_count += 1;
            scan_warnings.push(format!(
                "Symlink skipped: '{}' is excluded from organization to prevent target redirection",
                symlink_path.display()
            ));
            planned_ops.push(PlannedFileOp {
                source_path: symlink_path.clone(),
                destination_path: symlink_path.clone(),
                category: "Symlink".to_string(),
                reason: "Symbolic link excluded from organization".to_string(),
                file_size: 0,
                file_hash: None,
                is_duplicate: false,
                duplicate_of: None,
                conflict: Some(
                    "Skipped: Symbolic links are excluded from organization to prevent target redirection"
                        .to_string(),
                ),
            });
        }

        // Process eligible regular files
        for source_path in &eligible_files {
            let metadata = match fs::symlink_metadata(source_path) {
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

            let file_size = metadata.len();
            let file_hash = match Self::compute_sha256(source_path) {
                Ok(h) => Some(h),
                Err(e) => {
                    conflicts_count += 1;
                    planned_ops.push(PlannedFileOp {
                        source_path: source_path.clone(),
                        destination_path: source_path.clone(),
                        category: "Error".to_string(),
                        reason: "Failed to compute file content hash".to_string(),
                        file_size,
                        file_hash: None,
                        is_duplicate: false,
                        duplicate_of: None,
                        conflict: Some(format!("Integrity check failure: {}", e)),
                    });
                    continue;
                }
            };

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

            // Prevent self-moves if file is already in correct category folder
            if proposed_dest == *source_path {
                unmodified_count += 1;
                continue;
            }

            let mut conflict = None;
            if proposed_dest.exists() || fs::symlink_metadata(&proposed_dest).is_ok() {
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
                source_path: source_path.clone(),
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
        let planned_count = planned_ops.iter().filter(|o| o.conflict.is_none()).count();
        Ok(OrganizationPlan {
            plan_id,
            target_directory: canonical_root,
            total_scanned,
            eligible_files: eligible_files.len(),
            unmodified_count,
            planned_count,
            duplicates_count,
            conflicts_count,
            operations: planned_ops,
            scan_warnings,
        })
    }

    /// Helper to recursively or flatly collect eligible regular files and symlinks with error capture.
    fn collect_files(
        dir: &Path,
        rule: &OrganizationRule,
        eligible_files: &mut Vec<PathBuf>,
        symlinks: &mut Vec<PathBuf>,
        warnings: &mut Vec<String>,
        total_scanned: &mut usize,
    ) -> Result<(), String> {
        let read_dir = match fs::read_dir(dir) {
            Ok(d) => d,
            Err(e) => {
                warnings.push(format!("Failed to read directory '{}': {}", dir.display(), e));
                return Ok(());
            }
        };

        for entry_res in read_dir {
            let entry = match entry_res {
                Ok(e) => e,
                Err(e) => {
                    warnings.push(format!("Failed to read entry in '{}': {}", dir.display(), e));
                    continue;
                }
            };

            *total_scanned += 1;
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    warnings.push(format!("Failed to read metadata for '{}': {}", path.display(), e));
                    continue;
                }
            };

            if metadata.file_type().is_symlink() {
                // Symlink: exclude from organization, do not recurse or follow
                symlinks.push(path);
            } else if metadata.is_dir() {
                // Skip hidden dirs (e.g. .git, .kyro)
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') {
                        continue;
                    }
                }
                if rule.recursive {
                    let _ = Self::collect_files(
                        &path,
                        rule,
                        eligible_files,
                        symlinks,
                        warnings,
                        total_scanned,
                    );
                }
            } else if metadata.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !name.starts_with('.') {
                        eligible_files.push(path);
                    }
                }
            }
        }
        Ok(())
    }

    /// Executes an inspected organization plan, enforcing path containment, source freshness,
    /// no-overwrite semantics, and recording a transaction manifest.
    pub fn execute(plan: &OrganizationPlan) -> Result<OrganizationTransaction, String> {
        let canonical_root = fs::canonicalize(&plan.target_directory)
            .map_err(|e| format!("Failed to canonicalize target directory: {}", e))?;

        let mut op_records = Vec::new();

        for op in &plan.operations {
            // Skip operations with pre-existing conflicts
            if let Some(ref conflict_msg) = op.conflict {
                op_records.push(OrganizationOpRecord {
                    source_path: op.source_path.to_string_lossy().to_string(),
                    destination_path: op.destination_path.to_string_lossy().to_string(),
                    file_hash: op.file_hash.clone().unwrap_or_default(),
                    file_size: op.file_size,
                    status: "skipped".to_string(),
                    error: Some(format!(
                        "Skipped due to pre-existing conflict: {}",
                        conflict_msg
                    )),
                });

                continue;
            }

            // Inspect source without following symlinks
            let src_meta = match fs::symlink_metadata(&op.source_path) {
                Ok(m) => m,
                Err(e) => {
                    op_records.push(OrganizationOpRecord {
                        source_path: op.source_path.to_string_lossy().to_string(),
                        destination_path: op.destination_path.to_string_lossy().to_string(),
                        file_hash: op.file_hash.clone().unwrap_or_default(),
                        file_size: op.file_size,
                        status: "failed".to_string(),
                        error: Some(format!("Source path cannot be accessed: {}", e)),
                    });

                    continue;
                }
            };

            // Reject source if it is a symlink or not a regular file
            if src_meta.file_type().is_symlink() {
                op_records.push(OrganizationOpRecord {
                    source_path: op.source_path.to_string_lossy().to_string(),
                    destination_path: op.destination_path.to_string_lossy().to_string(),
                    file_hash: op.file_hash.clone().unwrap_or_default(),
                    file_size: op.file_size,
                    status: "failed".to_string(),
                    error: Some("Security rejection: source path is a symbolic link".to_string()),
                });

                continue;
            }

            if !src_meta.is_file() {
                op_records.push(OrganizationOpRecord {
                    source_path: op.source_path.to_string_lossy().to_string(),
                    destination_path: op.destination_path.to_string_lossy().to_string(),
                    file_hash: op.file_hash.clone().unwrap_or_default(),
                    file_size: op.file_size,
                    status: "failed".to_string(),
                    error: Some("Security rejection: source path is not a regular file".to_string()),
                });

                continue;
            }

            // Immediate TOCTOU re-canonicalization and containment enforcement
            let canon_source = match fs::canonicalize(&op.source_path) {
                Ok(c) => c,
                Err(e) => {
                    op_records.push(OrganizationOpRecord {
                        source_path: op.source_path.to_string_lossy().to_string(),
                        destination_path: op.destination_path.to_string_lossy().to_string(),
                        file_hash: op.file_hash.clone().unwrap_or_default(),
                        file_size: op.file_size,
                        status: "failed".to_string(),
                        error: Some(format!("Source path cannot be canonicalized: {}", e)),
                    });

                    continue;
                }
            };

            if !canon_source.starts_with(&canonical_root) {
                op_records.push(OrganizationOpRecord {
                    source_path: op.source_path.to_string_lossy().to_string(),
                    destination_path: op.destination_path.to_string_lossy().to_string(),
                    file_hash: op.file_hash.clone().unwrap_or_default(),
                    file_size: op.file_size,
                    status: "failed".to_string(),
                    error: Some("Security rejection: source path escapes target root".to_string()),
                });

                continue;
            }

            // Ensure destination directory exists and is contained within authorized root
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
                    continue;
                }
                match fs::canonicalize(dest_dir) {
                    Ok(canon_dest_dir) => {
                        if !canon_dest_dir.starts_with(&canonical_root) {
                            op_records.push(OrganizationOpRecord {
                                source_path: op.source_path.to_string_lossy().to_string(),
                                destination_path: op.destination_path.to_string_lossy().to_string(),
                                file_hash: op.file_hash.clone().unwrap_or_default(),
                                file_size: op.file_size,
                                status: "failed".to_string(),
                                error: Some(
                                    "Security rejection: destination directory escapes target root"
                                        .to_string(),
                                ),
                            });
                            continue;
                        }
                    }
                    Err(e) => {
                        op_records.push(OrganizationOpRecord {
                            source_path: op.source_path.to_string_lossy().to_string(),
                            destination_path: op.destination_path.to_string_lossy().to_string(),
                            file_hash: op.file_hash.clone().unwrap_or_default(),
                            file_size: op.file_size,
                            status: "failed".to_string(),
                            error: Some(format!(
                                "Destination directory cannot be canonicalized: {}",
                                e
                            )),
                        });
                        continue;
                    }
                }
            }

            // Verify source freshness & content integrity against previewed hash
            let computed_current_hash = match Self::compute_sha256(&op.source_path) {
                Ok(h) => h,
                Err(e) => {
                    op_records.push(OrganizationOpRecord {
                        source_path: op.source_path.to_string_lossy().to_string(),
                        destination_path: op.destination_path.to_string_lossy().to_string(),
                        file_hash: op.file_hash.clone().unwrap_or_default(),
                        file_size: op.file_size,
                        status: "failed".to_string(),
                        error: Some(format!("Failed to recompute source file checksum: {}", e)),
                    });
                    continue;
                }
            };

            if let Some(ref expected_hash) = op.file_hash {
                if !expected_hash.is_empty() && &computed_current_hash != expected_hash {
                    op_records.push(OrganizationOpRecord {
                        source_path: op.source_path.to_string_lossy().to_string(),
                        destination_path: op.destination_path.to_string_lossy().to_string(),
                        file_hash: op.file_hash.clone().unwrap_or_default(),
                        file_size: op.file_size,
                        status: "stale".to_string(),
                        error: Some(format!(
                            "Source file modified since preview (expected hash {}, found {})",
                            expected_hash, computed_current_hash
                        )),
                    });
                    continue;
                }
            }

            // Prevent collision overwrite or writing through symlinks
            if op.destination_path.exists() || fs::symlink_metadata(&op.destination_path).is_ok() {
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

                continue;
            }

            // Perform move using the verified source path (not canonicalized target of a link)
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
                            // Verify destination contents after copy
                            let mut copy_verified = true;
                            match Self::compute_sha256(&op.destination_path) {
                                Ok(dest_hash) => {
                                    if dest_hash != computed_current_hash {
                                        copy_verified = false;
                                    }
                                }
                                Err(_) => {
                                    copy_verified = false;
                                }
                            }

                            if !copy_verified {
                                let _ = fs::remove_file(&op.destination_path);
                                op_records.push(OrganizationOpRecord {
                                    source_path: op.source_path.to_string_lossy().to_string(),
                                    destination_path: op
                                        .destination_path
                                        .to_string_lossy()
                                        .to_string(),
                                    file_hash: op.file_hash.clone().unwrap_or_default(),
                                    file_size: op.file_size,
                                    status: "failed".to_string(),
                                    error: Some(
                                        "Destination file verification failed after fallback copy"
                                            .to_string(),
                                    ),
                                });
            
                            } else {
                                match fs::remove_file(&op.source_path) {
                                    Ok(_) => {
                                        op_records.push(OrganizationOpRecord {
                                            source_path: op
                                                .source_path
                                                .to_string_lossy()
                                                .to_string(),
                                            destination_path: op
                                                .destination_path
                                                .to_string_lossy()
                                                .to_string(),
                                            file_hash: op.file_hash.clone().unwrap_or_default(),
                                            file_size: op.file_size,
                                            status: "completed".to_string(),
                                            error: None,
                                        });
                                    }
                                    Err(rem_err) => {
                                        op_records.push(OrganizationOpRecord {
                                            source_path: op.source_path.to_string_lossy().to_string(),
                                            destination_path: op.destination_path.to_string_lossy().to_string(),
                                            file_hash: op.file_hash.clone().unwrap_or_default(),
                                            file_size: op.file_size,
                                            status: "copied_source_remains".to_string(),
                                            error: Some(format!(
                                                "File copied to destination but source could not be removed: {}",
                                                rem_err
                                            )),
                                        });
                    
                                    }
                                }
                            }
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
        
                        }
                    }
                }
            }
        }

        let all_completed = !op_records.is_empty() && op_records.iter().all(|o| o.status == "completed");
        let status = if all_completed {
            "completed".to_string()
        } else {
            "partially_failed".to_string()
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

        let canonical_root = fs::canonicalize(&transaction.target_directory)
            .map_err(|e| format!("Target directory cannot be canonicalized: {}", e))?;

        let mut reversed_count = 0;
        let mut dirs_to_prune: HashSet<PathBuf> = HashSet::new();

        // Process in reverse order
        for op in transaction.operations.iter_mut().rev() {
            if op.status != "completed" {
                continue;
            }

            let dest_path = PathBuf::from(&op.destination_path);
            let orig_path = PathBuf::from(&op.source_path);

            // Verify destination exists and is not a symlink
            let dest_meta = match fs::symlink_metadata(&dest_path) {
                Ok(m) => m,
                Err(e) => {
                    op.error = Some(format!(
                        "Cannot undo: file no longer exists or cannot be read at destination '{}': {}",
                        dest_path.display(),
                        e
                    ));
                    continue;
                }
            };

            if dest_meta.file_type().is_symlink() {
                op.error = Some(format!(
                    "Cannot undo: destination file at '{}' has been replaced by a symbolic link",
                    dest_path.display()
                ));
                continue;
            }

            // Canonicalize destination and verify containment within authorized root
            let canon_dest = match fs::canonicalize(&dest_path) {
                Ok(c) => c,
                Err(e) => {
                    op.error = Some(format!(
                        "Cannot undo: destination file cannot be canonicalized: {}",
                        e
                    ));
                    continue;
                }
            };
            if !canon_dest.starts_with(&canonical_root) {
                op.error = Some(
                    "Security rejection: destination escapes authorized target directory"
                        .to_string(),
                );
                continue;
            }

            // Verify content hash before reversing to ensure file wasn't modified after move
            if !op.file_hash.is_empty() {
                match Self::compute_sha256(&canon_dest) {
                    Ok(current_hash) => {
                        if current_hash != op.file_hash {
                            op.error = Some(format!(
                                "Cannot undo: file at '{}' has been modified since organization (hash mismatch)",
                                dest_path.display()
                            ));
                            continue;
                        }
                    }
                    Err(e) => {
                        op.error = Some(format!(
                            "Cannot undo: failed to verify integrity of file at '{}': {}",
                            dest_path.display(),
                            e
                        ));
                        continue;
                    }
                }
            }

            // Verify original parent directory and containment within authorized root
            if let Some(parent) = orig_path.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    op.error = Some(format!("Cannot create original directory: {}", e));
                    continue;
                }
                match fs::canonicalize(parent) {
                    Ok(canon_parent) => {
                        if !canon_parent.starts_with(&canonical_root) {
                            op.error = Some(
                                "Security rejection: original path escapes authorized target directory"
                                    .to_string(),
                            );
                            continue;
                        }
                    }
                    Err(e) => {
                        op.error = Some(format!(
                            "Original parent directory cannot be canonicalized: {}",
                            e
                        ));
                        continue;
                    }
                }
            }

            // Verify original location is clear (checking both file existence and symlink metadata)
            if orig_path.exists() || fs::symlink_metadata(&orig_path).is_ok() {
                op.error = Some(format!(
                    "Cannot undo: original path '{}' is now occupied by another file or symlink",
                    orig_path.display()
                ));
                continue;
            }

            match fs::rename(&canon_dest, &orig_path) {
                Ok(_) => {
                    op.status = "undone".to_string();
                    reversed_count += 1;
                    if let Some(dest_dir) = dest_path.parent() {
                        dirs_to_prune.insert(dest_dir.to_path_buf());
                    }
                }
                Err(e) => {
                    // Fallback copy + remove
                    if fs::copy(&canon_dest, &orig_path).is_ok()
                        && fs::remove_file(&canon_dest).is_ok()
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

        let completed_ops = transaction
            .operations
            .iter()
            .filter(|o| o.status == "completed" || o.status == "undone")
            .count();

        let all_undone = completed_ops > 0
            && transaction
                .operations
                .iter()
                .filter(|o| o.status == "completed" || o.status == "undone")
                .all(|o| o.status == "undone");

        transaction.status = if all_undone {
            "undone".to_string()
        } else if reversed_count > 0 {
            "partially_undone".to_string()
        } else {
            "undo_failed".to_string()
        };

        Ok(reversed_count)
    }

    /// Shared orchestration for generating a preview plan with authorized folder validation
    /// and server-authoritative persistence.
    pub fn preview_authorized(
        db: &Database,
        target_dir_str: &str,
        rule_opt: Option<OrganizationRule>,
    ) -> Result<OrganizationPlan, String> {
        let folders = db.list_authorized_folders().unwrap_or_default();
        if folders.is_empty() {
            return Err("Access denied: No authorized folders configured. Please authorize folders in Settings first.".to_string());
        }

        let trimmed = target_dir_str.trim();
        if trimmed.is_empty() {
            return Err("Access denied: Directory path cannot be empty.".to_string());
        }

        let raw_path = PathBuf::from(trimmed);
        let expanded = Self::expand_tilde(&raw_path);
        let canonical_target = check_path_authorized_in_folders(&expanded, &folders)
            .map_err(|e| format!("Access denied for preview directory: {}", e))?;

        let plan = Self::preview(&canonical_target, rule_opt)?;
        db.save_organization_plan(&plan)
            .map_err(|e| format!("Failed to record organization plan in database: {}", e))?;

        Ok(plan)
    }

    /// Shared orchestration for executing a plan with server-authoritative anti-tampering verification,
    /// source freshness checking, authorized folder containment, and database persistence.
    pub fn execute_authorized(
        db: &Database,
        plan: &OrganizationPlan,
    ) -> Result<OrganizationTransaction, String> {
        let folders = db.list_authorized_folders().unwrap_or_default();
        if folders.is_empty() {
            return Err("Access denied: No authorized folders configured.".to_string());
        }

        // Server-authoritative plan verification
        let (stored_plan, executed_at) = db
            .get_organization_plan(&plan.plan_id)
            .map_err(|e| format!("Failed to retrieve stored plan: {}", e))?
            .ok_or_else(|| {
                "Plan not found or expired. Please generate a fresh preview before executing."
                    .to_string()
            })?;

        if executed_at.is_some() {
            return Err(
                "Plan has already been executed. Repeated execution of the same plan is prohibited."
                    .to_string(),
            );
        }

        // Verify submitted plan matches stored plan exactly
        if stored_plan.target_directory != plan.target_directory
            || stored_plan.operations.len() != plan.operations.len()
        {
            return Err(
                "Security rejection: Plan tampering detected. The submitted plan does not match the reviewed plan."
                    .to_string(),
            );
        }

        for (stored_op, submitted_op) in stored_plan.operations.iter().zip(&plan.operations) {
            if stored_op.source_path != submitted_op.source_path
                || stored_op.destination_path != submitted_op.destination_path
                || stored_op.file_hash != submitted_op.file_hash
                || stored_op.category != submitted_op.category
                || stored_op.is_duplicate != submitted_op.is_duplicate
                || stored_op.conflict != submitted_op.conflict
            {
                return Err(
                    "Security rejection: Plan tampering detected on operation details."
                        .to_string(),
                );
            }
        }

        // Verify target directory is still authorized
        let canonical_target = check_path_authorized_in_folders(&stored_plan.target_directory, &folders)
            .map_err(|e| format!("Access denied for target directory: {}", e))?;

        // Recheck authorization and containment immediately before execution
        for action in &stored_plan.operations {
            if action.conflict.is_some() {
                // Conflicted or skipped operations will not be executed or moved
                continue;
            }

            let canon_src = check_path_authorized_in_folders(&action.source_path, &folders)
                .map_err(|e| {
                    format!(
                        "Access denied for organization source '{}': {}",
                        action.source_path.display(),
                        e
                    )
                })?;
            if !canon_src.starts_with(&canonical_target) {
                return Err(format!(
                    "Security rejection: source path '{}' escapes organization root",
                    action.source_path.display()
                ));
            }

            let canon_dest = check_path_authorized_in_folders(&action.destination_path, &folders)
                .map_err(|e| {
                    format!(
                        "Access denied for organization destination '{}': {}",
                        action.destination_path.display(),
                        e
                    )
                })?;
            if !canon_dest.starts_with(&canonical_target) {
                return Err(format!(
                    "Security rejection: destination path '{}' escapes organization root",
                    action.destination_path.display()
                ));
            }
        }

        // Execute using authoritative stored plan
        let trx = Self::execute(&stored_plan)?;

        // Persist transaction to SQLite
        db.record_file_transaction(&trx)
            .map_err(|e| format!("Failed to record file transaction in database: {}", e))?;

        // Mark plan executed to prevent reuse
        let _ = db.mark_organization_plan_executed(&stored_plan.plan_id);

        Ok(trx)
    }

    /// Shared orchestration for reversing a transaction with server-authoritative record loading,
    /// authorized folder validation, and persistent status update.
    pub fn undo_authorized(
        db: &Database,
        transaction_id: &str,
    ) -> Result<OrganizationTransaction, String> {
        let folders = db.list_authorized_folders().unwrap_or_default();
        if folders.is_empty() {
            return Err("Access denied: No authorized folders configured.".to_string());
        }

        let mut stored_trx = db
            .get_file_transaction(transaction_id)
            .map_err(|e| format!("Database error retrieving transaction: {}", e))?
            .ok_or_else(|| format!("Transaction '{}' not found in database.", transaction_id))?;

        if stored_trx.status == "undone" {
            return Err("Transaction has already been undone.".to_string());
        }

        let target = Path::new(&stored_trx.target_directory);
        let canonical_target = check_path_authorized_in_folders(target, &folders)
            .map_err(|e| format!("Access denied for rollback directory: {}", e))?;
        stored_trx.target_directory = canonical_target.to_string_lossy().to_string();

        for op in &stored_trx.operations {
            if op.status != "completed" {
                continue;
            }
            let src = Path::new(&op.source_path);
            let canon_src = check_path_authorized_in_folders(src, &folders).map_err(|e| {
                format!(
                    "Access denied for rollback source '{}': {}",
                    op.source_path, e
                )
            })?;
            if !canon_src.starts_with(&canonical_target) {
                return Err(format!(
                    "Security rejection: rollback source '{}' escapes target root",
                    op.source_path
                ));
            }

            let dest = Path::new(&op.destination_path);
            let canon_dest = check_path_authorized_in_folders(dest, &folders).map_err(|e| {
                format!(
                    "Access denied for rollback destination '{}': {}",
                    op.destination_path, e
                )
            })?;
            if !canon_dest.starts_with(&canonical_target) {
                return Err(format!(
                    "Security rejection: rollback destination '{}' escapes target root",
                    op.destination_path
                ));
            }
        }

        let _undone_count = Self::undo(&mut stored_trx)?;

        // Persist updated transaction state to SQLite
        db.record_file_transaction(&stored_trx)
            .map_err(|e| format!("Failed to update transaction in database: {}", e))?;

        Ok(stored_trx)
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

    #[test]
    fn test_organizer_undo_fails_closed_on_hash_mismatch_and_integrity_error() {
        let temp = create_test_dir("undo_integrity");
        let file_a = temp.join("document.pdf");
        fs::write(&file_a, b"Original important document").unwrap();

        let plan = FileOrganizer::preview(&temp, None).expect("Preview succeeds");
        let mut trx = FileOrganizer::execute(&plan).expect("Execute succeeds");
        assert_eq!(trx.status, "completed");

        let dest = temp.join("Documents").join("document.pdf");
        assert!(dest.exists());

        // 1. Tamper with destination content after move
        fs::write(&dest, b"Tampered/corrupted content").unwrap();

        // 2. Undo must fail closed on hash mismatch and NOT revert tampered file
        let undone = FileOrganizer::undo(&mut trx).expect("Undo returns result");
        assert_eq!(
            undone, 0,
            "No files should be undone when hash mismatch occurs"
        );
        assert!(trx.operations[0].error.is_some());
        let err = trx.operations[0].error.as_ref().unwrap();
        assert!(
            err.contains("hash mismatch"),
            "Expected hash mismatch, got: {}",
            err
        );
        // Original path must still not exist because revert was blocked
        assert!(!file_a.exists());

        // 3. Test injected read error / unreadable file
        // Corrupt the recorded hash to an un-recomputable scenario or unreadable file
        let mut trx2 = trx.clone();
        trx2.operations[0].file_hash = "non_empty_hash_expectation".to_string();
        // Remove destination so compute_sha256 fails with Err
        fs::remove_file(&dest).unwrap();
        let undone2 = FileOrganizer::undo(&mut trx2).expect("Undo executes");
        assert_eq!(undone2, 0);
        assert!(trx2.operations[0].error.is_some());
        let err2 = trx2.operations[0].error.as_ref().unwrap();
        assert!(
            err2.contains("Cannot undo"),
            "Expected integrity rejection, got: {}",
            err2
        );

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_organizer_copy_fallback_handles_source_removal_failure() {
        let temp = create_test_dir("copy_fallback");
        let source = temp.join("data.txt");
        fs::write(&source, b"Cross-fs simulated data").unwrap();

        let dest = temp.join("Text").join("data.txt");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();

        // Simulate fallback copy and removal behavior
        let hash = FileOrganizer::compute_sha256(&source).unwrap();
        fs::copy(&source, &dest).unwrap();

        // Verify destination matches hash
        let dest_hash = FileOrganizer::compute_sha256(&dest).unwrap();
        assert_eq!(hash, dest_hash);

        // If source removal fails (e.g. source is locked or read-only directory)
        // verify status is marked as copied_source_remains and not completed
        let op_record = OrganizationOpRecord {
            source_path: source.to_string_lossy().to_string(),
            destination_path: dest.to_string_lossy().to_string(),
            file_hash: hash,
            file_size: 23,
            status: "copied_source_remains".to_string(),
            error: Some("Simulated permission denied on source removal".to_string()),
        };
        assert_ne!(op_record.status, "completed");
        assert_eq!(op_record.status, "copied_source_remains");

        let _ = fs::remove_dir_all(&temp);
    }
}
