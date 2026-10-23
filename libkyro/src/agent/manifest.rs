//! Agent manifest parsing, validation, and typed configuration.
//!
//! An agent profile is a Markdown file with YAML frontmatter. Manifests are the
//! authoritative source for configurable agent definitions; this module turns a
//! manifest document into a validated, immutable in-memory representation that
//! the runtime can execute against.
//!
//! Deliberate behavior:
//! - Unknown frontmatter fields are rejected (`deny_unknown_fields`), so a
//!   typo-ed config key can never be silently ignored.
//! - Malformed YAML, missing required fields, oversized documents, and empty
//!   instruction bodies produce explicit, field-scoped errors.
//! - Every resource limit (file size, instruction length, numeric ranges) is
//!   enforced here so a single manifest cannot exhaust the runtime.
//!
//! Security: manifest bodies are *instructions to the model*; they are never
//! executable code and never grant additional tool authority. Tool policy can
//! only restrict what the platform would otherwise allow.

use serde::{Deserialize, Serialize};
use std::path::Path as FSPath;

/// The manifest schema version this runtime understands.
pub const MANIFEST_SCHEMA_VERSION: u64 = 1;

/// Bounded resource limits for a single manifest document.
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
/// Maximum characters accepted for the instructions body.
pub const MAX_INSTRUCTIONS_CHARS: usize = 200 * 1024;

/// Memory scopes a profile may opt into. Mirrors the scopes that
/// [`crate::memory::MemoryStore`] actually maintains.
pub const ALLOWED_MEMORY_SCOPES: &[&str] = &["preference", "agent", "project"];

fn is_valid_profile_id(id: &str) -> bool {
    // Stable identifiers: lowercase alphanumerics with `-` or `_`, starting
    // with an alphanumeric. Identifiers are used verbatim as file names, so
    // anything else (including `/`, `..`, whitespace) is rejected here.
    id.len() >= 2
        && id.len() <= 48
        && id
            .chars()
            .next()
            .map(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            == Some(true)
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn is_valid_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 64
        && trimmed
            .chars()
            .all(|c| !c.is_control() && c != '@' && c != '/' && c != '\\')
        && trimmed == name
}

fn is_valid_tool_name(tool: &str) -> bool {
    !tool.is_empty()
        && tool.len() <= 64
        && tool.chars().next().map(|c| c.is_ascii_alphabetic()) == Some(true)
        && tool
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// Model selection configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    /// `inherit` keeps the parent run / application selection.
    /// `fixed` pins an explicit model identifier.
    #[serde(default)]
    pub mode: String,
    /// Explicit model identifier when `mode = "fixed"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            mode: "inherit".to_string(),
            model: None,
        }
    }
}

/// Tool policy: inherit the runtime toolset, or restrict the agent to an
/// explicit allowlist. A profile can restrict capabilities; it can never grant
/// privileges the platform would otherwise deny.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ToolPolicy {
    #[serde(default)]
    pub mode: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
}

impl Default for ToolPolicy {
    fn default() -> Self {
        Self {
            mode: "inherit".to_string(),
            allow: Vec::new(),
        }
    }
}

/// Context and memory policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContextPolicy {
    /// Which memory scopes are included in the composed prompt.
    #[serde(default = "default_memory_scopes")]
    pub memory_scopes: Vec<String>,
}

fn default_memory_scopes() -> Vec<String> {
    vec!["preference".to_string(), "agent".to_string()]
}

impl Default for ContextPolicy {
    fn default() -> Self {
        Self {
            memory_scopes: default_memory_scopes(),
        }
    }
}

/// Delegation policy: whether this agent may delegate, and the bounded depth
/// and fan-out it may create.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DelegationPolicy {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Maximum delegation depth this agent may create (chain of parents).
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Maximum concurrent or sequential children a single run of this agent
    /// may delegate.
    #[serde(default = "default_max_children")]
    pub max_children: u32,
}

fn default_true() -> bool {
    true
}
fn default_max_depth() -> u32 {
    3
}
fn default_max_children() -> u32 {
    4
}

impl Default for DelegationPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_depth: 3,
            max_children: 4,
        }
    }
}

/// Runtime execution limits.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LimitsConfig {
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
}

fn default_max_steps() -> u32 {
    25
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self { max_steps: 25 }
    }
}

/// The typed frontmatter of an agent manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AgentManifestFrontmatter {
    #[serde(default = "default_schema_version")]
    pub schema_version: u64,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub avatar: String,
    pub role: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub tools: ToolPolicy,
    #[serde(default)]
    pub context: ContextPolicy,
    #[serde(default)]
    pub delegation: DelegationPolicy,
    #[serde(default)]
    pub limits: LimitsConfig,
}

fn default_schema_version() -> u64 {
    MANIFEST_SCHEMA_VERSION
}

/// A validated agent profile document: typed configuration plus the
/// Markdown instruction body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentManifest {
    pub frontmatter: AgentManifestFrontmatter,
    /// The Markdown body (instructions). Never empty.
    pub instructions: String,
    /// Original source location used in error reporting and snapshots.
    pub source_path: Option<String>,
    /// The complete original document bytes, used for content hashing.
    pub source_content: String,
}

impl AgentManifest {
    /// The full instructions text handed to the prompt composer.
    pub fn instructions(&self) -> &str {
        &self.instructions
    }
}

/// Detailed manifest validation failure.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestValidationError {
    /// Profile ID context, when known.
    pub profile_id: Option<String>,
    /// Field the failure applies to, when known (e.g. `name`, `tools.allow`).
    pub field: Option<String>,
    /// Human-readable explanation, suitable for surfacing in the UI.
    pub message: String,
}

impl ManifestValidationError {
    pub fn new(profile_id: Option<&str>, field: Option<&str>, message: impl Into<String>) -> Self {
        Self {
            profile_id: profile_id.map(|s| s.to_string()),
            field: field.map(|s| s.to_string()),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ManifestValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid agent manifest")?;
        if let Some(id) = &self.profile_id {
            write!(f, " [profile: {}]", id)?;
        }
        if let Some(field) = &self.field {
            write!(f, " (field: {})", field)?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for ManifestValidationError {}

/// Split a Redis-style frontmatter document into `(frontmatter, body)`.
/// Returns `Ok(None)` for plain Markdown with no frontmatter block.
pub fn split_frontmatter(document: &str) -> Result<Option<(&str, &str)>, ManifestValidationError> {
    let trimmed_start = document.trim_start_matches('\u{feff}');
    if !trimmed_start.starts_with("---") {
        return Ok(None);
    }

    let mut offset = 0usize;
    let mut first_line_len: Option<usize> = None;
    let mut close_line_offset: Option<usize> = None;
    for (idx, line) in trimmed_start.split_inclusive('\n').enumerate() {
        if idx == 0 {
            let first = line.trim_end();
            if first != "---" {
                return Err(ManifestValidationError::new(
                    None,
                    Some("frontmatter"),
                    "Leading `---` is not a valid frontmatter delimiter".to_string(),
                ));
            }
            first_line_len = Some(line.len());
        } else if line.trim_end() == "---" {
            close_line_offset = Some(offset);
            offset += line.len();
            break;
        }
        offset += line.len();
    }

    let (Some(first_len), Some(close_offset)) = (first_line_len, close_line_offset) else {
        return Err(ManifestValidationError::new(
            None,
            Some("frontmatter"),
            "Unterminated frontmatter block: missing closing `---`".to_string(),
        ));
    };

    let frontmatter = &trimmed_start[first_len..close_offset];
    let body = &trimmed_start[offset..]; // offset has advanced past the closing delimiter
    Ok(Some((frontmatter, body)))
}

/// Parse and validate a manifest document without touching the filesystem.
pub fn parse_manifest(source_content: &str) -> Result<AgentManifest, ManifestValidationError> {
    parse_manifest_with_source(source_content, None)
}

pub fn parse_manifest_with_source(
    source_content: &str,
    source_path: Option<&str>,
) -> Result<AgentManifest, ManifestValidationError> {
    if source_content.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestValidationError::new(
            None,
            Some("document"),
            format!(
                "Manifest exceeds the maximum size limit of {} bytes",
                MAX_MANIFEST_BYTES
            ),
        ));
    }

    let Some((frontmatter_text, body)) =
        split_frontmatter(source_content).map_err(|e| ManifestValidationError {
            profile_id: e.profile_id,
            field: e.field.clone(),
            message: e.message,
        })?
    else {
        return Err(ManifestValidationError::new(
            None,
            Some("frontmatter"),
            "Manifest must start with a YAML frontmatter block (`---` ... `---`)",
        ));
    };

    let frontmatter: AgentManifestFrontmatter =
        serde_yaml_ng::from_str(frontmatter_text).map_err(|e| {
            ManifestValidationError::new(
                None,
                Some("frontmatter"),
                format!("YAML parsing failed: {}", e),
            )
        })?;

    validate_frontmatter(&frontmatter)?;

    let instructions = body.trim().to_string();
    if instructions.is_empty() {
        return Err(ManifestValidationError::new(
            Some(&frontmatter.id),
            Some("body"),
            "Instruction body must not be empty: write the agent's operating instructions below the frontmatter",
        ));
    }
    if instructions.chars().count() > MAX_INSTRUCTIONS_CHARS {
        return Err(ManifestValidationError::new(
            Some(&frontmatter.id),
            Some("body"),
            format!(
                "Instruction body exceeds {} characters",
                MAX_INSTRUCTIONS_CHARS
            ),
        ));
    }

    Ok(AgentManifest {
        source_path: source_path.map(|s| s.to_string()),
        source_content: source_content.to_string(),
        frontmatter,
        instructions,
    })
}

/// Validate typed frontmatter, enforcing every resource limit and invariant.
pub fn validate_frontmatter(f: &AgentManifestFrontmatter) -> Result<(), ManifestValidationError> {
    if f.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(ManifestValidationError::new(
            Some(&f.id),
            Some("schema_version"),
            format!(
                "Unsupported schema_version {} (this build supports version {})",
                f.schema_version, MANIFEST_SCHEMA_VERSION
            ),
        ));
    }

    if !is_valid_profile_id(&f.id) {
        return Err(ManifestValidationError::new(
            Some(&f.id),
            Some("id"),
            "Profile id must be 2-48 characters of lowercase a-z, 0-9, '-' or '_', starting with a letter or digit (used verbatim as a file name)",
        ));
    }

    if !is_valid_name(&f.name) {
        return Err(ManifestValidationError::new(
            Some(&f.id),
            Some("name"),
            "name must be 1-64 characters with no leading/trailing whitespace, control characters, '@', '/' or '\\'",
        ));
    }

    if f.description.chars().count() > 256 {
        return Err(ManifestValidationError::new(
            Some(&f.id),
            Some("description"),
            "description must not exceed 256 characters",
        ));
    }

    if f.role.trim().is_empty() || f.role.chars().count() > 128 {
        return Err(ManifestValidationError::new(
            Some(&f.id),
            Some("role"),
            "role must be 1-128 characters and not empty",
        ));
    }

    // Aliases share the profile-id character set; uniqueness across profiles is
    // enforced at the registry level.
    let mut seen = std::collections::HashSet::new();
    for alias in &f.aliases {
        if !is_valid_profile_id(alias) {
            return Err(ManifestValidationError::new(
                Some(&f.id),
                Some("aliases"),
                format!(
                    "Alias '{}' must use the same character rules as a profile id",
                    alias
                ),
            ));
        }
        if !seen.insert(alias.to_lowercase()) {
            return Err(ManifestValidationError::new(
                Some(&f.id),
                Some("aliases"),
                format!("Duplicate alias '{}'", alias),
            ));
        }
    }

    match f.model.mode.as_str() {
        "inherit" => {
            if f.model.model.is_some() {
                return Err(ManifestValidationError::new(
                    Some(&f.id),
                    Some("model"),
                    "model.model must be omitted when model.mode is 'inherit'",
                ));
            }
        }
        "fixed" => {
            let model = f.model.model.as_deref().unwrap_or_default();
            if model.is_empty() || model.len() > 160 {
                return Err(ManifestValidationError::new(
                    Some(&f.id),
                    Some("model"),
                    "model.model must be a non-empty model identifier of at most 160 characters when mode is 'fixed'",
                ));
            }
        }
        other => {
            return Err(ManifestValidationError::new(
                Some(&f.id),
                Some("model"),
                format!(
                    "Unknown model.mode '{}' (expected 'inherit' or 'fixed')",
                    other
                ),
            ));
        }
    }

    let allowlist_specs: [(&ToolPolicy, &str); 1] = [(&f.tools, "tools")];
    for (policy, field) in &allowlist_specs {
        match policy.mode.as_str() {
            "inherit" | "allowlist" => {}
            other => {
                return Err(ManifestValidationError::new(
                    Some(&f.id),
                    Some(*field),
                    format!(
                        "Unknown {}.mode '{}' (expected 'inherit' or 'allowlist')",
                        field, other
                    ),
                ));
            }
        }
        if policy.mode == "inherit" && !policy.allow.is_empty() {
            return Err(ManifestValidationError::new(
                Some(&f.id),
                Some(second_part(field)),
                "tools.allow must be empty when tools.mode is 'inherit'",
            ));
        }
        for tool in &policy.allow {
            if !is_valid_tool_name(tool) {
                return Err(ManifestValidationError::new(
                    Some(&f.id),
                    Some(second_part(field)),
                    format!(
                        "Tool name '{}' must be 1-64 characters of a-z, A-Z, 0-9, '_', '-' or '.', starting with a letter",
                        tool
                    ),
                ));
            }
        }
    }

    for scope in &f.context.memory_scopes {
        if !ALLOWED_MEMORY_SCOPES.contains(&scope.as_str()) {
            return Err(ManifestValidationError::new(
                Some(&f.id),
                Some("context.memory_scopes"),
                format!(
                    "Unknown memory scope '{}' (allowed: {})",
                    scope,
                    ALLOWED_MEMORY_SCOPES.join(", ")
                ),
            ));
        }
    }

    if f.delegation.max_depth > 8 {
        return Err(ManifestValidationError::new(
            Some(&f.id),
            Some("delegation.max_depth"),
            "delegation.max_depth must be between 0 and 8",
        ));
    }
    if f.delegation.max_children > 16 {
        return Err(ManifestValidationError::new(
            Some(&f.id),
            Some("delegation.max_children"),
            "delegation.max_children must be between 0 and 16",
        ));
    }
    if f.limits.max_steps == 0 || f.limits.max_steps > 200 {
        return Err(ManifestValidationError::new(
            Some(&f.id),
            Some("limits.max_steps"),
            "limits.max_steps must be between 1 and 200",
        ));
    }

    Ok(())
}

fn second_part(field: &str) -> &str {
    if field == "tools" {
        "tools.allow"
    } else {
        field
    }
}

/// Serialize a validated manifest back to canonical Markdown text.
pub fn render_manifest(m: &AgentManifest) -> String {
    render_manifest_with_instructions(m, &m.instructions)
}

/// Serialize with a replacement instruction body (used by the editor when only
/// instructions change).
pub fn render_manifest_with_instructions(m: &AgentManifest, instructions: &str) -> String {
    let yaml = serde_yaml_ng::to_string(&m.frontmatter).unwrap_or_default();
    format!("---\n{}---\n\n{}\n", yaml, instructions.trim_end())
}

/// Immutability-safe write path: refuse to read or write through symlinks and
/// keep every managed file inside the managed directory.
pub fn ensure_managed_path(
    dir: &FSPath,
    file_name: &str,
) -> Result<std::path::PathBuf, ManifestValidationError> {
    if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
        return Err(ManifestValidationError::new(
            None,
            Some("path"),
            "Managed file names must not contain path separators",
        ));
    }
    let dir_meta = std::fs::symlink_metadata(dir).map_err(|e| {
        ManifestValidationError::new(
            None,
            Some("path"),
            format!("Managed directory unreadable: {}", e),
        )
    })?;
    if dir_meta.file_type().is_symlink() {
        return Err(ManifestValidationError::new(
            None,
            Some("path"),
            "Managed agent directory is a symlink; refusing to follow it",
        ));
    }
    let path = dir.join(file_name);
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_symlink() {
            return Err(ManifestValidationError::new(
                None,
                Some("path"),
                "Managed agent file is a symlink; refusing to follow it",
            ));
        }
    }
    Ok(path)
}

/// Atomic file write: contents land via temp file + rename so a crash or a
/// partial write can never corrupt an active profile.
pub fn atomic_write(path: &FSPath, contents: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Managed path has no parent directory".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    let tmp = parent.join(format!(
        ".kyro-manifest-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&tmp, contents).map_err(|e| format!("Failed to write temporary file: {}", e))?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("Failed to commit profile file: {}", e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "---\nschema_version: 1\nid: forge-devops\nname: Forge\nrole: DevOps\n---\n\n# Forge\n\nInstructions here.\n";

    #[test]
    fn parses_valid_manifest() {
        let m = parse_manifest(VALID).expect("valid manifest parses");
        assert_eq!(m.frontmatter.id, "forge-devops");
        assert!(m.instructions.contains("# Forge"));
        assert_eq!(m.frontmatter.delegation.max_depth, 3);
        assert_eq!(m.frontmatter.limits.max_steps, 25);
        assert!(m.frontmatter.enabled);
    }

    #[test]
    fn rejects_documents_without_frontmatter() {
        assert!(parse_manifest("# Just markdown").is_err());
        assert!(parse_manifest("").is_err());
    }

    #[test]
    fn rejects_unterminated_frontmatter() {
        let doc = "---\nid: ok-x\nname: X\nrole: r\n";
        let err = parse_manifest(doc).unwrap_err();
        assert_eq!(err.field.as_deref(), Some("frontmatter"));
    }

    #[test]
    fn rejects_unknown_fields() {
        let doc = "---\nid: ok-x\nname: X\nrole: r\nscary_field: true\n---\nbody";
        let err = parse_manifest(doc).unwrap_err();
        assert!(
            err.message.contains("unknown field"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn rejects_invalid_schema_version() {
        let doc = "---\nschema_version: 99\nid: ok-x\nname: X\nrole: r\n---\nbody";
        let err = parse_manifest(doc).unwrap_err();
        assert!(err.message.contains("schema_version"));
    }

    #[test]
    fn rejects_bad_ids() {
        for bad in ["../evil", "UPPER", "a", "", "with space"] {
            let doc = format!("---\nid: {}\nname: X\nrole: r\n---\nbody", bad);
            let err = parse_manifest(&doc).unwrap_err();
            assert_eq!(err.field.as_deref(), Some("id"), "for {}", bad);
        }
    }

    #[test]
    fn rejects_missing_name_and_role_and_empty_body() {
        let err = parse_manifest("---\nid: ok-id\n---\nbody").unwrap_err();
        let msg = err.message;
        assert!(
            msg.contains("missing field") || msg.contains("name"),
            "got {}",
            msg
        );

        let err = parse_manifest("---\nid: ok-id\nname: X\nrole: r\n---\n   \n").unwrap_err();
        assert_eq!(err.field.as_deref(), Some("body"));
    }

    #[test]
    fn rejects_oversized_documents() {
        let big = format!(
            "---\nid: ok-x\nname: X\nrole: r\n---\n{}",
            "a".repeat(MAX_MANIFEST_BYTES + 1)
        );
        let err = parse_manifest(&big).unwrap_err();
        assert!(err.message.contains("maximum size"));
    }

    #[test]
    fn rejects_unknown_model_and_tool_modes_and_scopes() {
        let doc = "---\nid: ok-x\nname: X\nrole: r\nmodel:\n  mode: warp\n---\nbody";
        assert_eq!(
            parse_manifest(doc).unwrap_err().field.as_deref(),
            Some("model")
        );

        let doc = "---\nid: ok-x\nname: X\nrole: r\ntools:\n  mode: weird\n  allow: []\n---\nbody";
        assert_eq!(
            parse_manifest(doc).unwrap_err().field.as_deref(),
            Some("tools")
        );

        let doc = "---\nid: ok-x\nname: X\nrole: r\ncontext:\n  memory_scopes: [agent, galactic]\n---\nbody";
        assert_eq!(
            parse_manifest(doc).unwrap_err().field.as_deref(),
            Some("context.memory_scopes")
        );
    }

    #[test]
    fn validates_tool_names() {
        let doc = "---\nid: ok-x\nname: X\nrole: r\ntools:\n  mode: allowlist\n  allow: [read_file, \"1bad\"]\n---\nbody";
        assert_eq!(
            parse_manifest(doc).unwrap_err().field.as_deref(),
            Some("tools.allow")
        );

        let doc = "---\nid: ok-x\nname: X\nrole: r\ntools:\n  mode: allowlist\n  allow: [read_file]\n---\nbody";
        assert!(parse_manifest(doc).is_ok());
    }

    #[test]
    fn inherit_mode_rejects_allow_and_fixed_model() {
        let doc = "---\nid: ok-x\nname: X\nrole: r\ntools:\n  mode: inherit\n  allow: [read_file]\n---\nbody";
        assert_eq!(
            parse_manifest(doc).unwrap_err().field.as_deref(),
            Some("tools.allow")
        );

        let doc = "---\nid: ok-x\nname: X\nrole: r\nmodel:\n  mode: inherit\n  model: openai/gpt-4o\n---\nbody";
        assert!(parse_manifest(doc).is_err());
    }

    #[test]
    fn accepts_fixed_model() {
        let doc =
            "---\nid: ok-x\nname: X\nrole: r\nmodel:\n  mode: fixed\n  model: nvidia/nemotron-3-ultra-550b-a55b\n---\nbody";
        let m = parse_manifest(doc).expect("fixed model parses");
        assert_eq!(
            m.frontmatter.model.model.as_deref(),
            Some("nvidia/nemotron-3-ultra-550b-a55b")
        );
    }

    #[test]
    fn enforces_numeric_ranges() {
        for field in [
            "delegation:\n  enabled: true\n  max_depth: 9\n  max_children: 2",
            "limits:\n  max_steps: 0",
            "delegation:\n  enabled: true\n  max_children: 17",
        ] {
            let doc = format!("---\nid: ok-x\nname: X\nrole: r\n{}\n---\nbody", field);
            assert!(parse_manifest(&doc).is_err(), "should reject: {}", field);
        }
    }

    #[test]
    fn round_trip_render() {
        let m = parse_manifest(VALID).unwrap();
        let rendered = render_manifest(&m);
        let reparsed = parse_manifest(&rendered).expect("round trip parses");
        assert_eq!(reparsed.frontmatter, m.frontmatter);
        assert_eq!(reparsed.instructions, m.instructions);
    }

    #[test]
    fn handles_bom_and_crlf() {
        let doc = "---\r\nid: ok-id-1\r\nname: X\r\nrole: r\r\n---\r\n\r\nBody text\r\n";
        let m = parse_manifest(doc).expect("crlf manifest parses");
        assert_eq!(m.frontmatter.id, "ok-id-1");
        assert!(m.instructions.contains("Body text"));
    }

    #[test]
    fn duplicate_aliases_rejected() {
        let doc = "---\nid: ok-x\nname: X\nrole: r\naliases: [dup, dup]\n---\nbody";
        assert_eq!(
            parse_manifest(doc).unwrap_err().field.as_deref(),
            Some("aliases")
        );
    }

    #[test]
    fn managed_path_refuses_escape() {
        assert!(ensure_managed_path(FSPath::new("/tmp"), "ok.md").is_ok());
        assert!(ensure_managed_path(FSPath::new("/tmp"), "../escape.md").is_err());
        assert!(ensure_managed_path(FSPath::new("/tmp"), "sub/dir.md").is_err());
    }
}
