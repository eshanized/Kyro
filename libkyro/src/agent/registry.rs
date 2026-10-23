//! Canonical agent registry.
//!
//! The registry resolves agent identity, instructions, metadata, runtime
//! configuration, and profile versions for the whole runtime. Markdown manifests
//! (see [`super::manifest`]) are the authoritative source for configurable agent
//! definitions; the SQLite `bots` table only remains a compatibility index that
//! legacy commands and tests can keep reading.
//!
//! Layout:
//!
//! ```text
//! <kyro-config>/kyro/agents/
//!   ├── overrides/<builtin-id>.md     user edits of built-in profiles
//!   ├── custom/<profile-id>.md        user-created profiles
//!   └── workflows/<workflow>.md       team workflow manifests
//! ```
//!
//! Shipped defaults live in `libkyro/resources/agents/` and are embedded via
//! `include_str!`, so the packaged application can always resolve them and
//! reset any user override back to the factory content.
//!
//! Resolution order for a built-in id: a valid user override replaces the
//! bundled default; an invalid override is never activated (the profile falls
//! back to the bundled default and the validation error is surfaced through the
//! registry listing).

use super::manifest::{self as manifest_mod, parse_manifest_with_source, AgentManifest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Stable identifier of the built-in default agent used when a caller does not
/// request a specific profile.
pub const DEFAULT_AGENT_ID: &str = "apex-orchestrator";

/// Bundled default manifests, embedded at compile time so the packaged
/// application never depends on a development working directory.
pub const BUILTIN_MANIFESTS: &[(&str, &str)] = &[
    (
        "apex-orchestrator",
        include_str!("../../resources/agents/apex-orchestrator.md"),
    ),
    (
        "forge-devops",
        include_str!("../../resources/agents/forge-devops.md"),
    ),
    (
        "scout-web",
        include_str!("../../resources/agents/scout-web.md"),
    ),
    (
        "kyro-routine-worker",
        include_str!("../../resources/agents/kyro-routine-worker.md"),
    ),
    (
        "kyro-remote-worker",
        include_str!("../../resources/agents/kyro-remote-worker.md"),
    ),
];

/// Origin of the active profile content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileOrigin {
    /// Active content equals or derives from the embedded bundled default.
    Builtin,
    /// Active content comes from a user override of a built-in profile.
    Override,
    /// Active content comes from a user-created custom profile.
    Custom,
}

/// Validation state of an entry in the registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "error", rename_all = "snake_case")]
pub enum ValidationState {
    Ok,
    /// The profile file failed validation; the profile is not active.
    Invalid(String),
}

/// A fully resolved, validated, agent profile plus its runtime configuration.
/// Snapshots of this struct are persisted with each run, so editing or deleting
/// a profile never changes an in-flight or historical run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileSnapshot {
    pub id: String,
    pub name: String,
    pub description: String,
    pub avatar: String,
    pub role: String,
    /// Effective runtime availability (manifest `enabled` AND not runtime-disabled).
    pub enabled: bool,
    pub origin: ProfileOrigin,
    pub schema_version: u64,
    /// Set of case-insensitive mention aliases (`@Name`, aliases, id).
    pub aliases: Vec<String>,
    pub model: manifest_mod::ModelConfig,
    pub tools: manifest_mod::ToolPolicy,
    pub context: manifest_mod::ContextPolicy,
    pub delegation: manifest_mod::DelegationPolicy,
    pub limits: manifest_mod::LimitsConfig,
    /// The Markdown instructions body.
    pub instructions: String,
    /// SHA-256 of the active manifest document (hex). Immutable per run.
    pub content_hash: String,
}

impl ProfileSnapshot {
    pub fn is_builtin(&self) -> bool {
        BUILTIN_MANIFESTS.iter().any(|(id, _)| *id == self.id)
    }

    /// True when delegation of tasks *to* this agent is permitted.
    pub fn accepts_delegation(&self) -> bool {
        self.enabled
    }

    /// Whether a tool with `tool_name` may be dispatched under this profile's
    /// tool policy (`inherit` → everything; `allowlist` → exact matches).
    pub fn tool_allowed(&self, tool_name: &str) -> bool {
        if self.tools.mode == "allowlist" {
            self.tools.allow.iter().any(|a| a == tool_name)
        } else {
            true
        }
    }

    /// Monotonic label used for provenance display; stable per content.
    pub fn version_label(&self) -> String {
        format!(
            "v{}:{}",
            self.schema_version,
            &self.content_hash[..12.min(self.content_hash.len())]
        )
    }
}

/// A registry entry: an active profile or a validation failure the user should
/// be able to see.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub source_path: Option<PathBuf>,
    pub origin: ProfileOrigin,
    /// Parsed document, present even when cross-profile uniqueness failed.
    pub doc: Option<AgentManifest>,
    pub validation: ValidationState,
    /// Present when the manifest parsed and passed all validation.
    pub snapshot: Option<ProfileSnapshot>,
    /// Entry-scoped disabled flag (mirrors the runtime `agent_disabled_ids` set).
    pub runtime_disabled: bool,
}

impl RegistryEntry {
    fn from_manifest(m: &AgentManifest, origin: ProfileOrigin, disabled: bool) -> Self {
        Self {
            source_path: m.source_path.clone().map(PathBuf::from),
            origin,
            doc: Some(m.clone()),
            validation: ValidationState::Ok,
            snapshot: Some(Self::snapshot_from(m, origin, disabled)),
            runtime_disabled: disabled,
        }
    }

    fn snapshot_from(m: &AgentManifest, origin: ProfileOrigin, disabled: bool) -> ProfileSnapshot {
        Self::snap_from_doc(m, origin, disabled)
    }

    fn snap_from_doc(m: &AgentManifest, origin: ProfileOrigin, disabled: bool) -> ProfileSnapshot {
        let f = &m.frontmatter;
        let mut aliases: HashSet<String> = HashSet::new();
        aliases.insert(f.name.to_lowercase());
        aliases.insert(f.id.to_lowercase());
        for a in &f.aliases {
            aliases.insert(a.to_lowercase());
        }
        ProfileSnapshot {
            id: f.id.clone(),
            name: f.name.trim().to_string(),
            description: f.description.clone(),
            avatar: f.avatar.clone(),
            role: f.role.trim().to_string(),
            enabled: f.enabled && !disabled,
            origin,
            schema_version: f.schema_version,
            aliases: {
                let mut v: Vec<String> = aliases.into_iter().collect();
                v.sort();
                v
            },
            model: f.model.clone(),
            tools: f.tools.clone(),
            context: f.context.clone(),
            delegation: f.delegation.clone(),
            limits: f.limits,
            instructions: m.instructions.clone(),
            content_hash: Self::hash_content(&m.source_content),
        }
    }

    pub fn hash_content(content: &str) -> String {
        let digest = Sha256::digest(content.as_bytes());
        hex(&digest)
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

/// Failure to resolve a profile.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveError {
    UnknownProfile(String),
    DisabledProfile(String),
    InvalidProfile(String, String),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::UnknownProfile(id) => write!(
                f,
                "Unknown agent '{}'. Registered agents: see agent roster.",
                id
            ),
            ResolveError::DisabledProfile(id) => {
                write!(f, "Agent '{}' is disabled and cannot run.", id)
            }
            ResolveError::InvalidProfile(id, msg) => {
                write!(f, "Agent '{}' has an invalid profile: {}", id, msg)
            }
        }
    }
}

impl std::error::Error for ResolveError {}

fn check_enabled(snapshot: &ProfileSnapshot) -> Result<&ProfileSnapshot, ResolveError> {
    if snapshot.enabled {
        Ok(snapshot)
    } else {
        Err(ResolveError::DisabledProfile(snapshot.id.clone()))
    }
}

/// The agent registry. Cheap to clone a consistent view via `snapshot_map()`;
/// mutations go through the write methods which rebuild the entry map.
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    /// Root of the managed agents directory (`<config>/kyro/agents`).
    base_dir: Option<PathBuf>,
    entries: HashMap<String, RegistryEntry>,
    runtime_disabled: HashSet<String>,
    /// Directory signature captured at load time; compared against the live
    /// signature to detect external edits of manifest files.
    path_signature: u64,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::embedded_only()
    }
}

impl AgentRegistry {
    /// Build a registry from the embedded shipped defaults only. Used by tests
    /// and by callers without filesystem state.
    pub fn embedded_only() -> Self {
        let mut reg = Self {
            base_dir: None,
            entries: HashMap::new(),
            runtime_disabled: HashSet::new(),
            path_signature: 0,
        };
        reg.load_builtin_entries();
        reg
    }

    /// Load the registry from the user configuration directory, degrading to
    /// the embedded defaults when the directory is unusable so the application
    /// can always resolve agent profiles.
    pub fn load_or_embedded(config_dir: &Path) -> Self {
        match Self::load(config_dir) {
            Ok(reg) => reg,
            Err(e) => {
                log::warn!(
                    "[AgentRegistry] Falling back to embedded agent profiles: {}",
                    e
                );
                Self::embedded_only()
            }
        }
    }

    /// Root directory of the managed agents store.
    pub fn base_dir(config_dir: &Path) -> PathBuf {
        config_dir.join("agents")
    }

    /// Configuration directory root that owns this registry's manifests
    /// (`<config>/kyro`), when the registry is user-backed. Workspace state
    /// persists under it.
    pub fn config_root(&self) -> Option<&Path> {
        self.base_dir.as_deref().and_then(Path::parent)
    }

    pub fn overrides_dir(&self) -> Option<PathBuf> {
        self.base_dir.as_ref().map(|b| b.join("overrides"))
    }

    pub fn custom_dir(&self) -> Option<PathBuf> {
        self.base_dir.as_ref().map(|b| b.join("custom"))
    }

    pub fn workflows_dir(&self) -> Option<PathBuf> {
        self.base_dir.as_ref().map(|b| b.join("workflows"))
    }

    /// Ensure the managed directory structure exists (first launch / migration)
    /// without touching any existing user content.
    pub fn init_user_dirs(config_dir: &Path) -> Result<PathBuf, String> {
        let base = Self::base_dir(config_dir);
        for sub in ["overrides", "custom", "workflows"] {
            std::fs::create_dir_all(base.join(sub))
                .map_err(|e| format!("Failed to initialize agent config directory: {}", e))?;
        }
        Ok(base)
    }

    /// Load the registry from disk: embedded defaults + user overrides + custom
    /// profiles. Invalid files never replace an active profile.
    pub fn load(config_dir: &Path) -> Result<Self, String> {
        Self::load_with_disabled(config_dir, &[])
    }

    pub fn load_with_disabled(config_dir: &Path, disabled: &[String]) -> Result<Self, String> {
        let base = Self::init_user_dirs(config_dir)?;
        let mut reg = Self {
            base_dir: Some(base),
            entries: HashMap::new(),
            runtime_disabled: disabled.iter().cloned().collect(),
            path_signature: 0,
        };
        reg.load_builtin_entries();
        reg.load_overrides()?;
        reg.load_custom()?;
        reg.validate_cross_profile_uniqueness();
        reg.path_signature = reg.current_dir_signature().unwrap_or(0);
        Ok(reg)
    }

    fn load_builtin_entries(&mut self) {
        for (id, content) in BUILTIN_MANIFESTS {
            let manifest = match parse_manifest_with_source(
                content,
                Some(&format!("<resources>/agents/{}.md", id)),
            ) {
                Ok(m) => m,
                Err(e) => {
                    log::error!(
                        "[AgentRegistry] Bundled manifest for '{}' failed to parse: {}",
                        id,
                        e
                    );
                    self.entries.insert(
                        id.to_string(),
                        RegistryEntry {
                            source_path: None,
                            origin: ProfileOrigin::Builtin,
                            doc: None,
                            validation: ValidationState::Invalid(e.to_string()),
                            snapshot: None,
                            runtime_disabled: false,
                        },
                    );
                    continue;
                }
            };
            let disabled = self.runtime_disabled.contains(*id);
            self.entries.insert(
                id.to_string(),
                RegistryEntry::from_manifest(&manifest, ProfileOrigin::Builtin, disabled),
            );
        }
    }

    fn load_overrides(&mut self) -> Result<(), String> {
        let Some(dir) = self.overrides_dir() else {
            return Ok(());
        };
        self.load_dir(&dir, ProfileOrigin::Override)
    }

    fn load_custom(&mut self) -> Result<(), String> {
        let Some(dir) = self.custom_dir() else {
            return Ok(());
        };
        self.load_dir(&dir, ProfileOrigin::Custom)
    }

    fn load_dir(&mut self, dir: &Path, origin: ProfileOrigin) -> Result<(), String> {
        if !dir.exists() {
            return Ok(());
        }
        let read_dir = std::fs::read_dir(dir)
            .map_err(|e| format!("Failed to read agent directory {:?}: {}", dir, e))?;
        let mut paths: Vec<PathBuf> = read_dir
            .flatten()
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                name.ends_with(".md") && !name.starts_with(".")
            })
            .map(|e| e.path())
            .collect();
        paths.sort();
        for path in paths {
            // Skip symlinks outright: managed directories must not be followed
            // into user-controlled locations.
            if std::fs::symlink_metadata(&path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(true)
            {
                log::warn!("[AgentRegistry] Skipping symlinked manifest {:?}", path);
                continue;
            }
            let id_from_file = path.file_stem().map(|s| s.to_string_lossy().to_string());
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("[AgentRegistry] Unreadable manifest {:?}: {}", path, e);
                    if let Some(id) = id_from_file {
                        self.entries.entry(id).or_insert(RegistryEntry {
                            source_path: Some(path.clone()),
                            origin,
                            doc: None,
                            validation: ValidationState::Invalid(format!("Unreadable: {}", e)),
                            snapshot: None,
                            runtime_disabled: false,
                        });
                    }
                    continue;
                }
            };
            let src_str = path.to_string_lossy().to_string();
            match parse_manifest_with_source(&content, Some(&src_str)) {
                Ok(m) => {
                    let id = m.frontmatter.id.clone();
                    // A custom directory file that claims a built-in id is an
                    // override, not a separate profile.
                    let effective_origin = if origin == ProfileOrigin::Custom
                        && BUILTIN_MANIFESTS.iter().any(|(bid, _)| *bid == id)
                    {
                        ProfileOrigin::Override
                    } else {
                        origin
                    };
                    let disabled = self.runtime_disabled.contains(&id);
                    self.entries.insert(
                        id.clone(),
                        RegistryEntry::from_manifest(&m, effective_origin, disabled),
                    );
                }
                Err(e) => {
                    let id = e
                        .profile_id
                        .clone()
                        .or_else(|| id_from_file.clone())
                        .unwrap_or_else(|| {
                            path.file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default()
                        });
                    log::warn!("[AgentRegistry] Invalid manifest {:?}: {}", path, e);
                    // An entry that already exists (e.g. a bundled default)
                    // keeps its previous valid state (doc-only insertion, no
                    // overwrite); otherwise the invalid file gets a visible
                    // invalid entry.
                    self.entries.entry(id).or_insert(RegistryEntry {
                        source_path: Some(path.clone()),
                        origin,
                        doc: None,
                        validation: ValidationState::Invalid(e.to_string()),
                        snapshot: None,
                        runtime_disabled: false,
                    });
                }
            }
        }
        Ok(())
    }

    /// Cross-profile identity validation: names and aliases must be unique
    /// (case-insensitive) so `@AgentName` routing stays unambiguous. Priority
    /// order for name ownership: bundled built-in > user override > custom;
    /// within the same class, lexicographic id. A colliding entry is flagged
    /// invalid and stays inactive until its name/alias is fixed.
    fn validate_cross_profile_uniqueness(&mut self) {
        // Recompute error states from scratch so removing a colliding profile
        // also clears flags. Entries without a parsed doc keep their
        // parse-level failure state.
        for entry in self.entries.values_mut() {
            if let Some(doc) = entry.doc.clone() {
                entry.snapshot = Some(RegistryEntry::snap_from_doc(
                    &doc,
                    entry.origin,
                    entry.runtime_disabled,
                ));
                entry.validation = ValidationState::Ok;
            }
        }

        let mut own_ids: Vec<String> = self.entries.keys().cloned().collect();
        own_ids.sort_by(|a, b| {
            let class = |v: &str| match self.entries.get(v) {
                Some(e) if e.origin == ProfileOrigin::Builtin => 0u8,
                Some(e) if e.origin == ProfileOrigin::Override => 1u8,
                _ => 2u8,
            };
            class(a).cmp(&class(b)).then_with(|| a.cmp(b))
        });

        let mut seen_names: HashMap<String, String> = HashMap::new();
        for id in &own_ids {
            let Some(snapshot) = self.entries.get(id).and_then(|e| e.snapshot.clone()) else {
                continue;
            };
            for key in &snapshot.aliases {
                let k = key.to_lowercase();
                if let Some(owner) = seen_names.get(&k) {
                    if owner != id {
                        let msg = format!(
                            "Name/alias '{}' collides with agent '@{}'; rename the profile or remove the alias so @mention routing stays unambiguous",
                            key, owner
                        );
                        if let Some(entry) = self.entries.get_mut(id) {
                            entry.validation = ValidationState::Invalid(msg);
                            entry.snapshot = None; // colliding profiles are inactive
                        }
                        break;
                    }
                } else {
                    seen_names.insert(k, id.clone());
                }
            }
        }
    }

    // ---------------------------------------------------------------- lookups

    pub fn get(&self, id: &str) -> Option<&RegistryEntry> {
        self.entries.get(id)
    }

    /// Resolve an active, validated profile by id or alias.
    pub fn resolve(&self, id_or_alias: &str) -> Result<&ProfileSnapshot, ResolveError> {
        let q = id_or_alias.trim().to_lowercase();
        let by_alias = self.entries.values().find_map(|e| {
            let s = e.snapshot.as_ref()?;
            if e.validation == ValidationState::Ok
                && s.aliases.iter().any(|a| a.to_lowercase() == q)
            {
                Some(s)
            } else {
                None
            }
        });
        if let Some(snapshot) = by_alias {
            return check_enabled(snapshot);
        }
        match self.entries.get(id_or_alias) {
            None => Err(ResolveError::UnknownProfile(id_or_alias.to_string())),
            Some(entry) => match (&entry.validation, &entry.snapshot) {
                (ValidationState::Ok, Some(snapshot)) => check_enabled(snapshot),
                (ValidationState::Invalid(msg), _) => Err(ResolveError::InvalidProfile(
                    id_or_alias.to_string(),
                    msg.clone(),
                )),
                _ => Err(ResolveError::UnknownProfile(id_or_alias.to_string())),
            },
        }
    }

    /// Resolve the default profile for runs that launch without an explicit id.
    pub fn resolve_default(&self) -> Result<&ProfileSnapshot, ResolveError> {
        self.resolve(DEFAULT_AGENT_ID)
    }

    /// Resolve a profile like `resolve`, but degrade gracefully for ids that
    /// predate the registry: legacy/pseudo agent ids are mapped onto their
    /// canonical worker profiles, and anything still unknown falls back to the
    /// bundled default. Disabled and invalid profiles still surface as errors
    /// so an explicit non-runnable selection is never silently replaced.
    pub fn resolve_lenient(&self, id_or_alias: &str) -> Result<ProfileSnapshot, ResolveError> {
        match self.resolve(id_or_alias) {
            Ok(snapshot) => Ok(snapshot.clone()),
            Err(ResolveError::UnknownProfile(_)) => {
                if let Some(mapped) = legacy_profile_mapping(id_or_alias) {
                    if let Ok(snapshot) = self.resolve(mapped) {
                        return Ok(snapshot.clone());
                    }
                }
                match self.resolve_default() {
                    Ok(snapshot) => Ok(snapshot.clone()),
                    Err(_) => Err(ResolveError::UnknownProfile(id_or_alias.to_string())),
                }
            }
            Err(e) => Err(e),
        }
    }

    /// All entries (active + invalid) for roster/status surfaces.
    pub fn list(&self) -> Vec<&RegistryEntry> {
        let mut entries: Vec<&RegistryEntry> = self.entries.values().collect();
        entries.sort_by(|a, b| match (&a.snapshot, &b.snapshot) {
            (Some(x), Some(y)) => x.id.cmp(&y.id),
            (Some(_), None) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Greater,
        });
        entries
    }

    /// All active snapshots, sorted by id.
    pub fn active_profiles(&self) -> Vec<ProfileSnapshot> {
        let mut profiles: Vec<ProfileSnapshot> = self
            .entries
            .values()
            .filter_map(|e| {
                (e.validation == ValidationState::Ok)
                    .then(|| e.snapshot.clone())
                    .flatten()
            })
            .collect();
        profiles.sort_by(|a, b| a.id.cmp(&b.id));
        profiles
    }

    /// (id, entry) pairs for every known profile, sorted by id. Invalid
    /// manifests are included so users can see and fix them.
    pub fn list_entries(&self) -> Vec<(String, &RegistryEntry)> {
        let mut ids: Vec<&String> = self.entries.keys().collect();
        ids.sort();
        ids.into_iter()
            .map(|id| (id.clone(), self.entries.get(id).expect("key from map")))
            .collect()
    }

    pub fn is_builtin_id(id: &str) -> bool {
        BUILTIN_MANIFESTS.iter().any(|(bid, _)| *bid == id)
    }

    /// Bundled factory content for a built-in id (used by reset-to-default).
    pub fn builtin_default_content(id: &str) -> Option<&'static str> {
        BUILTIN_MANIFESTS
            .iter()
            .find(|(bid, _)| *bid == id)
            .map(|(_, c)| *c)
    }

    /// File path for a managed profile id.
    pub fn profile_path(&self, id: &str) -> Result<PathBuf, String> {
        if !path_component_ok(id) {
            return Err(format!("Invalid agent profile id '{}'", id));
        }
        let custom_names: HashSet<String> = BUILTIN_MANIFESTS
            .iter()
            .map(|(id, _)| id.to_string())
            .collect();
        match &self.base_dir {
            Some(base) => {
                if custom_names.contains(id) {
                    Ok(base.join("overrides").join(format!("{}.md", id)))
                } else {
                    Ok(base.join("custom").join(format!("{}.md", id)))
                }
            }
            None => {
                Err("Agent registry is not backed by a user configuration directory".to_string())
            }
        }
    }

    // ---------------------------------------------------------------- writes

    /// Persist a manifest document for `id` atomically and activate it.
    /// Validation happens before anything touches the active profile; an
    /// invalid save is rejected outright and the previously active profile
    /// remains in place.
    pub fn save_profile(&mut self, id: &str, document: &str) -> Result<ProfileSnapshot, String> {
        if !path_component_ok(id) {
            return Err(format!("Invalid agent profile id '{}'", id));
        }
        let manifest = parse_manifest_with_source(document, None).map_err(|e| e.to_string())?;
        if manifest.frontmatter.id != id {
            return Err(format!(
                "Manifest id '{}' does not match the requested profile id '{}'",
                manifest.frontmatter.id, id
            ));
        }
        let path = self.profile_path(id)?;
        manifest_mod::atomic_write(&path, document).map_err(|e| {
            // Nothing was activated; the previously active profile remains.
            format!("Save rejected; active profile unchanged. Error: {}", e)
        })?;
        // Activate by reloading the single profile we know we wrote.
        let origin = if BUILTIN_MANIFESTS.iter().any(|(bid, _)| *bid == id) {
            ProfileOrigin::Override
        } else {
            ProfileOrigin::Custom
        };
        let disabled = self.runtime_disabled.contains(id);
        let entry = RegistryEntry {
            source_path: Some(path),
            origin,
            doc: Some(manifest.clone()),
            validation: ValidationState::Ok,
            snapshot: Some(RegistryEntry::snapshot_from(&manifest, origin, disabled)),
            runtime_disabled: disabled,
        };
        self.entries.insert(id.to_string(), entry);
        self.validate_cross_profile_uniqueness();
        self.refresh_signature();
        // Cross-profile validation may have flagged the saved profile (e.g. a
        // name/alias collision); report the final state so callers never claim
        // a success the runtime would not honor.
        match self.entries.get(id).and_then(|e| e.snapshot.clone()) {
            Some(snapshot) => Ok(snapshot),
            None => Err(self
                .entries
                .get(id)
                .and_then(|e| match &e.validation {
                    ValidationState::Invalid(msg) => Some(msg.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    format!(
                        "Agent '{}' was saved but flagged invalid; see the agent roster",
                        id
                    )
                })),
        }
    }

    /// Delete a user-owned profile file. Built-in profiles are protected: an
    /// override is deleted (falling back to the bundled default), but a pure
    /// built-in can only be reset, never deleted.
    pub fn delete_profile(&mut self, id: &str) -> Result<ProfileOrigin, String> {
        if !path_component_ok(id) {
            return Err(format!("Invalid agent profile id '{}'", id));
        }
        let path = self.profile_path(id)?;
        let origin = if BUILTIN_MANIFESTS.iter().any(|(bid, _)| *bid == id) {
            if self.entries.get(id).map(|e| e.origin) == Some(ProfileOrigin::Override) {
                ProfileOrigin::Override
            } else {
                return Err(
                    "Built-in agents cannot be deleted. Reset them to defaults instead."
                        .to_string(),
                );
            }
        } else {
            ProfileOrigin::Custom
        };
        std::fs::remove_file(&path).map_err(|e| format!("Failed to delete profile file: {}", e))?;
        if origin == ProfileOrigin::Override {
            // Re-activate the bundled default for this id.
            if let Some(content) = Self::builtin_default_content(id) {
                if let Ok(m) = parse_manifest_with_source(content, Some("<resources>")) {
                    let disabled = self.runtime_disabled.contains(id);
                    self.entries.insert(
                        id.to_string(),
                        RegistryEntry::from_manifest(&m, ProfileOrigin::Builtin, disabled),
                    );
                    self.validate_cross_profile_uniqueness();
                    self.refresh_signature();
                    return Ok(origin);
                }
            }
        }
        self.entries.remove(id);
        self.validate_cross_profile_uniqueness();
        self.refresh_signature();
        Ok(origin)
    }

    /// Reset a built-in profile to its shipped default (removes the override).
    pub fn reset_builtin(&mut self, id: &str) -> Result<ProfileSnapshot, String> {
        let default = Self::builtin_default_content(id)
            .ok_or_else(|| format!("'{}' is not a built-in agent profile", id))?;
        let path = self.profile_path(id)?;
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to remove override file: {}", e))?;
        }
        let m =
            parse_manifest_with_source(default, Some("<resources>")).map_err(|e| e.to_string())?;
        let disabled = self.runtime_disabled.contains(id);
        self.entries.insert(
            id.to_string(),
            RegistryEntry::from_manifest(&m, ProfileOrigin::Builtin, disabled),
        );
        self.validate_cross_profile_uniqueness();
        self.refresh_signature();
        match self.entries.get(id).and_then(|e| e.snapshot.clone()) {
            Some(snapshot) => Ok(snapshot),
            None => Err(format!(
                "Reset left agent '{}' flagged invalid; see the agent roster",
                id
            )),
        }
    }

    /// Toggle runtime availability. Writes through to the settings-side
    /// disabled-id set owned by the caller.
    pub fn set_runtime_disabled(&mut self, id: &str, disabled: bool) -> Result<(), String> {
        let snapshot = self
            .entries
            .get(id)
            .and_then(|e| e.snapshot.clone())
            .ok_or_else(|| format!("Unknown agent profile '{}'", id))?;
        if snapshot.is_builtin() || Self::builtin_default_content(id).is_some() {
            // Built-ins may be disabled at runtime; their files stay untouched.
        } else {
            // Custom profiles: record the flag inside the manifest so the
            // source stays authoritative.
            let path = self.profile_path(id)?;
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read profile: {}", e))?;
            let m = parse_manifest_with_source(&content, None).map_err(|e| e.to_string())?;
            let mut fm = m.frontmatter.clone();
            fm.enabled = !disabled;
            let doc = manifest_mod::render_manifest(&AgentManifest {
                frontmatter: fm,
                instructions: m.instructions.clone(),
                source_path: None,
                source_content: String::new(),
            });
            manifest_mod::atomic_write(&path, &doc)
                .map_err(|e| format!("Save rejected; profile unchanged. Error: {}", e))?;
            let updated = parse_manifest_with_source(&doc, None).map_err(|e| e.to_string())?;
            if let Some(entry) = self.entries.get_mut(id) {
                let rebuilt =
                    RegistryEntry::from_manifest(&updated, entry.origin, entry.runtime_disabled);
                *entry = rebuilt;
                entry.source_path = Some(path.clone());
            }
            self.validate_cross_profile_uniqueness();
            self.refresh_signature();
            return Ok(());
        }
        if disabled {
            self.runtime_disabled.insert(id.to_string());
        } else {
            self.runtime_disabled.remove(id);
        }
        if let Some(entry) = self.entries.get_mut(id) {
            // The manifest's own `enabled` flag still applies; runtime
            // disabling is a mask on top of it.
            let manifest_enabled = entry
                .doc
                .as_ref()
                .map(|d| d.frontmatter.enabled)
                .unwrap_or(true);
            if let Some(s) = entry.snapshot.as_mut() {
                s.enabled = manifest_enabled && !disabled;
            }
            entry.runtime_disabled = disabled;
        }
        Ok(())
    }

    /// Disabled-id set for persistence by the caller.
    pub fn runtime_disabled_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.runtime_disabled.iter().cloned().collect();
        ids.sort();
        ids
    }

    /// Cheap external-change detection: compare (mtime, size) of every managed
    /// manifest against the signature captured at the last load. Any detected
    /// change triggers a rescan so external edits become active.
    pub fn has_external_changes(&self) -> bool {
        let Some(dir_signature) = self.current_dir_signature() else {
            return false;
        };
        dir_signature != self.path_signature
    }

    fn current_dir_signature(&self) -> Option<u64> {
        let mut hasher = Sha256::new();
        let mut kv: Vec<(String, String)> = Vec::new();
        for sub in ["overrides", "custom"] {
            let dir = self.base_dir.as_ref()?.join(sub);
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for entry in rd.flatten() {
                    let path = entry.path();
                    if !path.to_string_lossy().ends_with(".md") {
                        continue;
                    }
                    let meta = std::fs::metadata(&path).ok()?;
                    kv.push((
                        path.to_string_lossy().to_string(),
                        format!("{}:{}", meta.len(), file_mtime_millis(&path)),
                    ));
                }
            }
        }
        kv.sort();
        for (k, v) in kv {
            hasher.update(k.as_bytes());
            hasher.update(v.as_bytes());
        }
        let d = hasher.finalize();
        Some(u64::from_le_bytes(d[..8].try_into().unwrap_or([0; 8])))
    }

    /// Re-capture the directory signature after a registry-owned file write so
    /// `has_external_changes()` only reports edits the registry did not make.
    fn refresh_signature(&mut self) {
        self.path_signature = self.current_dir_signature().unwrap_or(0);
    }

    /// Rescan from disk when external edits are detected. Keeps in-memory
    /// snapshots pinned for callers holding clones.
    pub fn ensure_fresh(&mut self) -> bool {
        if !self.has_external_changes() {
            return false;
        }
        self.reload().ok();
        true
    }

    /// Full rescan from disk.
    pub fn reload(&mut self) -> Result<(), String> {
        let Some(base) = self.base_dir.clone() else {
            return Ok(());
        };
        let config_dir = base
            .parent()
            .ok_or_else(|| "Invalid agents directory".to_string())?;
        let disabled = self.runtime_disabled_ids();
        let mut reg = Self::load_with_disabled(config_dir, &disabled)?;
        std::mem::swap(self, &mut reg);
        Ok(())
    }

    /// Snapshot map for cheap consistent reads.
    pub fn snapshot_map(&self) -> HashMap<String, ProfileSnapshot> {
        self.entries
            .iter()
            .filter_map(|(k, e)| e.snapshot.clone().map(|s| (k.clone(), s)))
            .collect()
    }
}

// Millisecond mtime for freshness signatures (0 when unavailable).
fn file_mtime_millis(path: &Path) -> u128 {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn path_component_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Legacy/pseudo agent ids that predate the profile registry, mapped onto the
/// canonical worker profiles that replaced them. Runs recorded with these ids
/// (Telegram bridge, routine scheduler) resolve to the equivalent profile
/// instead of erroring, while keeping their historical semantics.
fn legacy_profile_mapping(id: &str) -> Option<&'static str> {
    match id {
        "telegram-bot" => Some("kyro-remote-worker"),
        "manual-routine-worker" | "scheduled-routine-worker" => Some("kyro-routine-worker"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Temp dir registry + shared serial lock (tests mutate env-independent
    // paths but each gets its own directory).
    static SERIAL: Mutex<()> = Mutex::new(());

    struct TempConfig {
        dir: PathBuf,
        config: PathBuf,
    }

    impl TempConfig {
        fn new() -> Self {
            let _g = SERIAL.lock().unwrap();
            let dir = std::env::temp_dir().join(format!("kyro_registry_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(dir.join("config")).unwrap();
            // Mutex is not needed here because each test owns its directory;
            // keeping the lock scope local avoids poisoning across awaits.
            Self {
                config: dir.join("config"),
                dir,
            }
        }
        fn config(&self) -> &Path {
            &self.config
        }
        fn base(&self) -> PathBuf {
            AgentRegistry::base_dir(self.config())
        }
        fn overrides(&self) -> PathBuf {
            self.base().join("overrides")
        }
        fn custom(&self) -> PathBuf {
            self.base().join("custom")
        }
    }

    impl Drop for TempConfig {
        fn drop(&mut self) {
            let _ = fs_cleanup(&self.dir);
        }
    }

    fn fs_cleanup(dir: &Path) -> std::io::Result<()> {
        std::fs::remove_dir_all(dir)
    }

    const CUSTOM_DOC: &str = "---\nid: test-eng\nname: Test Engineer\ndescription: writes tests\navatar: code\nrole: Test Engineer\naliases: [teng]\n---\n\nYou are the Test Engineer of the registry test.\n";

    #[test]
    fn loads_builtin_profiles() {
        let cfg = TempConfig::new();
        let reg = AgentRegistry::load(cfg.config()).expect("registry loads");
        let profiles = reg.active_profiles();
        let ids: Vec<&str> = profiles.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"apex-orchestrator"));
        assert!(ids.contains(&"forge-devops"));
        assert!(ids.contains(&"scout-web"));
        assert_eq!(
            profiles
                .iter()
                .filter(|p| p.id == "apex-orchestrator")
                .count(),
            1,
            "Overrides/custom dirs start empty, builtins appear once"
        );
        // Every builtin's instructions come from the manifest body, not a Rust literal.
        let apex = reg.resolve("apex-orchestrator").unwrap();
        assert!(apex.instructions.contains("# Identity"));
        let forge = reg.resolve("forge").unwrap(); // via alias
        assert_eq!(forge.id, "forge-devops");
    }

    #[test]
    fn resolves_alias_and_rejects_unknown_and_disabled() {
        let cfg = TempConfig::new();
        let mut reg = AgentRegistry::load(cfg.config()).unwrap();
        reg.runtime_disabled.insert("scout-web".into());
        reg.set_runtime_disabled("scout-web", true).unwrap();
        let err = reg.resolve("scout-web").unwrap_err();
        assert!(matches!(err, ResolveError::DisabledProfile(id) if id == "scout-web"));
        let err = reg.resolve("scout").unwrap_err(); // alias resolves, then disabled
        assert!(matches!(err, ResolveError::DisabledProfile(_)));
        assert!(matches!(
            reg.resolve("nope-agent"),
            Err(ResolveError::UnknownProfile(_))
        ));
    }

    #[test]
    fn override_replaces_builtin_and_reset_restores_default() {
        let cfg = TempConfig::new();
        let mut reg = AgentRegistry::load(cfg.config()).unwrap();

        let doc =
            "---\nid: forge-devops\nname: Forge\nrole: User Edited Role\n---\n\n# Custom forge body\n";
        reg.save_profile("forge-devops", doc)
            .expect("override saves");
        let forge = reg.resolve("forge-devops").unwrap();
        assert_eq!(forge.role, "User Edited Role");
        assert_eq!(forge.origin, ProfileOrigin::Override);
        assert!(forge.instructions.contains("Custom forge body"));

        reg.reset_builtin("forge-devops").expect("reset");
        let forge = reg.resolve("forge-devops").unwrap();
        assert_eq!(forge.origin, ProfileOrigin::Builtin);
        assert!(forge.instructions.contains("You are Forge"));
        assert!(!cfg.overrides().exists() || cfg.overrides().read_dir().unwrap().count() == 0);
    }

    #[test]
    fn custom_profiles_create_edit_disable_delete() {
        let cfg = TempConfig::new();
        let mut reg = AgentRegistry::load(cfg.config()).unwrap();
        let snap = reg
            .save_profile("test-eng", CUSTOM_DOC)
            .expect("custom saves");
        assert_eq!(snap.origin, ProfileOrigin::Custom);
        assert_eq!(snap.id, "test-eng");

        let resolved = reg.resolve("teng").unwrap();
        assert_eq!(resolved.id, "test-eng");

        // Disable through the tracked runtime flag, then re-enable.
        reg.set_runtime_disabled("test-eng", true).unwrap();
        let refetched = reg.snapshot_map().remove("test-eng").unwrap();
        assert!(!refetched.enabled);
        reg.set_runtime_disabled("test-eng", false).unwrap();
        assert!(reg.resolve("test-eng").is_ok());

        // Delete: after this the profile is gone and the alias no longer resolves.
        let origin = reg.delete_profile("test-eng").unwrap();
        assert_eq!(origin, ProfileOrigin::Custom);
        assert!(matches!(
            reg.resolve("test-eng"),
            Err(ResolveError::UnknownProfile(_))
        ));
    }

    #[test]
    fn deletions_and_renames_of_builtins_are_protected() {
        let cfg = TempConfig::new();
        let mut reg = AgentRegistry::load(cfg.config()).unwrap();
        assert!(reg.delete_profile("apex-orchestrator").is_err());
        // Custom dir file claiming a builtin id is treated as an override and reset-able.
    }

    #[test]
    fn invalid_manifest_never_replaces_active_profile() {
        let cfg = TempConfig::new();
        let mut reg = AgentRegistry::load(cfg.config()).unwrap();
        let before = reg.resolve("forge-devops").unwrap().clone();

        // Write an invalid override directly to disk (simulating a bad external edit).
        std::fs::write(
            cfg.overrides().join("forge-devops.md"),
            "---\nid: forged-devops\nname: Bad\nrole: r\n---\nbody",
        )
        .unwrap();
        reg.reload().unwrap();
        // The override file's id does not match the filename's builtin id, so it
        // registers as its own profile; forge still resolves to the builtin.
        let after = reg.resolve("forge-devops").unwrap();
        assert_eq!(after, &before);

        // An invalid file with the right id must also NOT break the builtin.
        std::fs::write(
            cfg.overrides().join("forge-devops.md"),
            "---\nid: forge-devops\nname: Forge\nrole: r\nschema_version: 42\n---\nbody",
        )
        .unwrap();
        reg.reload().unwrap();
        let after = reg.resolve("forge-devops").unwrap();
        assert_eq!(after.id, "forge-devops");
        assert!(after.instructions.contains("You are Forge"));
        // and the registry still lists an entry for the builtin (fallback).
    }

    #[test]
    fn duplicate_name_collision_flags_the_colliding_entry() {
        let cfg = TempConfig::new();
        let mut reg = AgentRegistry::load(cfg.config()).unwrap();
        // Second profile reusing the name "Forge" (case-insensitive).
        let _ = reg.save_profile(
            "dup-forge",
            "---\nid: dup-forge\nname: forge\nrole: r\n---\n\nBody.\n",
        );
        let err = reg.resolve("dup-forge").unwrap_err();
        assert!(matches!(err, ResolveError::InvalidProfile(_, msg) if msg.contains("collides")));
        // The original Forge remains valid and resolvable.
        assert!(reg.resolve("forge-devops").is_ok());
    }

    #[test]
    fn alias_colliding_with_other_profile_flagged() {
        let cfg = TempConfig::new();
        let mut reg = AgentRegistry::load(cfg.config()).unwrap();
        let _ = reg.save_profile(
            "alias-thief",
            "---\nid: alias-thief\nname: Thief\nrole: r\naliases: [forge]\n---\n\nBody.\n",
        );
        let err = reg.resolve("alias-thief").unwrap_err();
        assert!(
            matches!(err, ResolveError::InvalidProfile(_, msg) if msg.contains("collides") || msg.contains("forge"))
        );
        // Forge still resolves through its own alias first (first-scanned wins).
        assert_eq!(reg.resolve("forge").unwrap().id, "forge-devops");
    }

    #[test]
    fn external_edits_detected_and_reload_keeps_running_snapshots() {
        let cfg = TempConfig::new();
        let mut reg: AgentRegistry = AgentRegistry::load(cfg.config()).unwrap();
        reg.save_profile("test-eng", CUSTOM_DOC).unwrap();
        assert!(reg.resolve("test-eng").is_ok());

        // Pin a snapshot from before the edit.
        let pinned = reg.resolve("test-eng").unwrap().clone();

        // External edit (no registry write involved).
        let edited = format!(
            "---\nid: test-eng\nname: Test Engineer\nrole: Updated\n---\n\nUpdated body {}\n",
            uuid::Uuid::new_v4()
        );
        std::fs::write(cfg.custom().join("test-eng.md"), edited).unwrap();

        assert!(reg.has_external_changes());
        reg.ensure_fresh();
        let fresh = reg.resolve("test-eng").unwrap().clone();
        assert!(fresh.instructions.contains("Updated body"));
        // The pinned snapshot taken before the external edit is untouched.
        assert!(pinned.instructions.contains("registry test"));
        assert_ne!(fresh.content_hash, pinned.content_hash);
    }

    #[test]
    fn save_preserves_last_valid_profile_on_write_failure() {
        let cfg = TempConfig::new();
        let mut reg: AgentRegistry = AgentRegistry::load(cfg.config()).unwrap();
        reg.save_profile("test-eng", CUSTOM_DOC).unwrap();
        // Invalid document rejected without touching existing content.
        assert!(reg
            .save_profile("test-eng", "---\nid: test-eng\nname: Bad\n---\n")
            .is_err());
        let still = reg.resolve("test-eng").unwrap();
        assert!(still.instructions.contains("registry test"));
    }

    #[test]
    fn path_traversal_is_refused() {
        let cfg = TempConfig::new();
        let mut reg: AgentRegistry = AgentRegistry::load(cfg.config()).unwrap();
        assert!(reg.save_profile("../escape", CUSTOM_DOC).is_err());
        assert!(reg.profile_path("sub/thing").is_err());
    }

    #[test]
    fn snapshots_are_immutable_per_content() {
        let cfg = TempConfig::new();
        let mut reg = AgentRegistry::load(cfg.config()).unwrap();
        let pinned = reg.resolve("forge-devops").unwrap().clone();
        let doc_with_new_role =
            "---\nid: forge-devops\nname: Forge\nrole: Edited\n---\n\nEdited body.\n";
        reg.save_profile("forge-devops", doc_with_new_role).unwrap();
        // The pinned clone is unchanged.
        assert_eq!(pinned.role, "DevOps, Code & Terminal Specialist");
        assert!(pinned.instructions.contains("You are Forge"));
        // New resolution sees the edit.
        assert_eq!(reg.resolve("forge-devops").unwrap().role, "Edited");
        // Hashes differ between snapshots.
        assert_ne!(
            pinned.content_hash,
            reg.resolve("forge-devops").unwrap().content_hash
        );
    }

    #[test]
    fn symlinked_manifests_are_not_loaded() {
        let cfg = TempConfig::new();
        // The managed custom directory is created by the first registry load;
        // create it up front so the symlink can be planted before loading.
        std::fs::create_dir_all(cfg.custom()).unwrap();
        let target = cfg.dir.join("evil.md");
        std::fs::write(&target, "---\nid: sylink\nname: Sylink\nrole: r\n---\nbody").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, cfg.custom().join("sylink.md")).unwrap();
        let reg = AgentRegistry::load(cfg.config()).unwrap();
        assert!(matches!(
            reg.resolve("sylink"),
            Err(ResolveError::UnknownProfile(_))
        ));
    }

    #[test]
    fn version_label_is_stable_per_content() {
        let cfg = TempConfig::new();
        let mut reg = AgentRegistry::load(cfg.config()).unwrap();
        let a = reg.resolve("forge-devops").unwrap().clone();
        reg.save_profile(
            "forge-devops",
            "---\nid: forge-devops\nname: Forge\nrole: r\n---\n\nchanged\n",
        )
        .unwrap();
        let b = reg.resolve("forge-devops").unwrap().clone();
        assert_ne!(a.version_label(), b.version_label());
        assert_eq!(a.version_label(), a.version_label());
        assert_ne!(a.content_hash, b.content_hash);
    }
}
