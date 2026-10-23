//! Structured process launching with controlled executable discovery.
//!
//! Every process Kyro starts on behalf of the model must be started from an
//! explicit executable + argument vector.  Model output, user input and file
//! paths are never concatenated into a shell command line, so shell
//! metacharacters cannot escape their argument position.
//!
//! The resolver deliberately does **not** trust `PATH` blindly: it searches a
//! controlled list of directories, refuses world-writable directories and
//! refuses relative or traversing paths.  This keeps "launch an application"
//! separate from "run a shell command".

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// Directory names that are never used to resolve an executable, because they
/// are conventionally world-writable and therefore attacker-controllable.
const NEVER_TRUSTED_DIR_NAMES: &[&str] = &["tmp", "var/tmp", "dev/shm"];

/// A program invocation expressed as an explicit executable and argument list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramSpec {
    /// Either a bare program name (resolved through the controlled search
    /// path) or an absolute path to an executable.
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

impl ProgramSpec {
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        }
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }
}

/// A program invocation that passed discovery and validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedProgram {
    /// Absolute path of the executable that will actually be run.
    pub path: PathBuf,
    /// Fully validated argument vector.
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchError {
    /// The requested executable could not be located.
    NotFound {
        executable: String,
        searched: Vec<String>,
    },
    /// The executable exists but is not usable (not a file, not executable, ...).
    NotExecutable { path: PathBuf, reason: String },
    /// The executable lives in a location Kyro refuses to trust.
    UntrustedLocation { path: PathBuf, reason: String },
    /// The argument vector was rejected before launch.
    InvalidArguments { reason: String },
    /// The working directory was rejected before launch.
    InvalidWorkingDirectory { path: PathBuf, reason: String },
    /// The process could not be spawned.
    SpawnFailed { reason: String },
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchError::NotFound {
                executable,
                searched,
            } => write!(
                f,
                "Executable '{}' was not found in the controlled search path (searched: {})",
                executable,
                searched.join(", ")
            ),
            LaunchError::NotExecutable { path, reason } => write!(
                f,
                "Executable '{}' is not usable: {}",
                path.display(),
                reason
            ),
            LaunchError::UntrustedLocation { path, reason } => write!(
                f,
                "Executable '{}' resides in an untrusted location: {}",
                path.display(),
                reason
            ),
            LaunchError::InvalidArguments { reason } => {
                write!(f, "Invalid launch arguments: {}", reason)
            }
            LaunchError::InvalidWorkingDirectory { path, reason } => write!(
                f,
                "Invalid working directory '{}': {}",
                path.display(),
                reason
            ),
            LaunchError::SpawnFailed { reason } => {
                write!(f, "Failed to spawn process: {}", reason)
            }
        }
    }
}

impl std::error::Error for LaunchError {}

/// Controlled executable discovery.
///
/// The search path is composed of well-known system locations plus the
/// directories the current user explicitly added to `PATH`, minus any
/// world-writable directory.  Absolute paths supplied by the caller are
/// accepted only when they point at a regular, executable, trusted file.
pub struct ExecutableResolver {
    search_dirs: Vec<PathBuf>,
}

impl Default for ExecutableResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutableResolver {
    pub fn new() -> Self {
        // Keep deterministic system-directory precedence, but do not trust a
        // well-known path blindly: hosted runners and container images can make
        // /usr/local/bin world-writable, just like a user-provided PATH entry.
        let mut candidates: Vec<PathBuf> = [
            "/usr/local/bin",
            "/usr/bin",
            "/bin",
            "/usr/local/sbin",
            "/usr/sbin",
            "/sbin",
            "/opt/bin",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();

        // The user's PATH is considered after system locations. The same trust
        // checks apply to every directory regardless of where it came from.
        if let Some(user_path) = std::env::var_os("PATH") {
            candidates.extend(std::env::split_paths(&user_path));
        }

        Self {
            search_dirs: Self::trusted_search_dirs(candidates),
        }
    }

    /// Removes duplicate, missing, and world-writable directories while
    /// preserving search order.
    fn trusted_search_dirs(candidates: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        for dir in candidates {
            if !dir.is_dir() || !Self::dir_is_trusted(&dir) || dirs.contains(&dir) {
                continue;
            }
            dirs.push(dir);
        }
        dirs
    }

    /// Builds a resolver with an explicit search path (used by tests).
    ///
    /// This deliberately preserves the provided paths so tests can exercise
    /// rejection behavior for an untrusted directory.
    pub fn with_search_dirs(dirs: Vec<PathBuf>) -> Self {
        Self { search_dirs: dirs }
    }

    pub fn search_dirs(&self) -> &[PathBuf] {
        &self.search_dirs
    }

    /// Resolves a program specification into a validated, absolute invocation.
    pub fn resolve(&self, spec: &ProgramSpec) -> Result<ResolvedProgram, LaunchError> {
        let executable = spec.executable.trim();
        if executable.is_empty() {
            return Err(LaunchError::NotFound {
                executable: spec.executable.clone(),
                searched: Vec::new(),
            });
        }
        if executable.contains('\0') {
            return Err(LaunchError::InvalidArguments {
                reason: "Executable name contains a null byte".into(),
            });
        }

        let args = validate_args(&spec.args)?;

        let path = if executable.contains('/') {
            self.resolve_explicit_path(Path::new(executable))?
        } else {
            self.resolve_in_search_path(executable)?
        };

        if let Some(cwd) = &spec.cwd {
            validate_working_directory(cwd)?;
        }

        Ok(ResolvedProgram { path, args })
    }

    fn resolve_in_search_path(&self, name: &str) -> Result<PathBuf, LaunchError> {
        let mut searched = Vec::new();
        for dir in &self.search_dirs {
            searched.push(dir.display().to_string());
            let candidate = dir.join(name);
            if !candidate.is_file() {
                continue;
            }
            if !Self::dir_is_trusted(dir) {
                return Err(LaunchError::UntrustedLocation {
                    path: candidate,
                    reason: "containing directory is world-writable".into(),
                });
            }
            if !is_executable_file(&candidate) {
                return Err(LaunchError::NotExecutable {
                    path: candidate,
                    reason: "file is not marked executable".into(),
                });
            }
            // Resolve symlinks so the executed binary is the one we inspected.
            let real = candidate.canonicalize().unwrap_or(candidate);
            if !is_executable_file(&real) {
                return Err(LaunchError::NotExecutable {
                    path: real,
                    reason: "resolved target is not marked executable".into(),
                });
            }
            return Ok(real);
        }

        Err(LaunchError::NotFound {
            executable: name.to_string(),
            searched,
        })
    }

    fn resolve_explicit_path(&self, raw: &Path) -> Result<PathBuf, LaunchError> {
        if !raw.is_absolute() {
            return Err(LaunchError::UntrustedLocation {
                path: raw.to_path_buf(),
                reason: "relative executable paths are not accepted".into(),
            });
        }
        for comp in raw.components() {
            if matches!(comp, Component::ParentDir) {
                return Err(LaunchError::UntrustedLocation {
                    path: raw.to_path_buf(),
                    reason: "executable path must not contain '..'".into(),
                });
            }
        }
        if !raw.is_file() {
            return Err(LaunchError::NotExecutable {
                path: raw.to_path_buf(),
                reason: "path does not exist or is not a regular file".into(),
            });
        }
        let real = raw.canonicalize().unwrap_or_else(|_| raw.to_path_buf());
        if !is_executable_file(&real) {
            return Err(LaunchError::NotExecutable {
                path: real,
                reason: "file is not marked executable".into(),
            });
        }
        if let Some(parent) = real.parent() {
            if !Self::dir_is_trusted(parent) {
                return Err(LaunchError::UntrustedLocation {
                    path: real,
                    reason: "containing directory is world-writable".into(),
                });
            }
        }
        Ok(real)
    }

    /// A directory is trusted when it is not world-writable and is not one of
    /// the conventionally shared scratch directories.
    pub fn dir_is_trusted(dir: &Path) -> bool {
        let text = dir.to_string_lossy().to_string();
        let normalized = text.trim_end_matches('/');
        for bad in NEVER_TRUSTED_DIR_NAMES {
            if normalized == *bad || normalized.ends_with(&format!("/{}", bad)) {
                return false;
            }
        }
        if let Ok(meta) = std::fs::metadata(dir) {
            use std::os::unix::fs::PermissionsExt;
            let mode = meta.permissions().mode();
            // world-writable bit
            if mode & 0o002 != 0 {
                return false;
            }
        }
        true
    }
}

fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) => {
            if !meta.is_file() {
                return false;
            }
            meta.permissions().mode() & 0o111 != 0
        }
        Err(_) => false,
    }
}

/// Rejects argument vectors that cannot be represented safely.
pub fn validate_args(args: &[String]) -> Result<Vec<String>, LaunchError> {
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        if a.contains('\0') {
            return Err(LaunchError::InvalidArguments {
                reason: "argument contains a null byte".into(),
            });
        }
        if a.len() > 128 * 1024 {
            return Err(LaunchError::InvalidArguments {
                reason: "argument exceeds the 128 KiB limit".into(),
            });
        }
        out.push(a.clone());
    }
    if out.len() > 512 {
        return Err(LaunchError::InvalidArguments {
            reason: "argument vector exceeds 512 entries".into(),
        });
    }
    Ok(out)
}

/// Validates a working directory before it is used for a launch.
pub fn validate_working_directory(dir: &Path) -> Result<PathBuf, LaunchError> {
    if !dir.is_absolute() {
        return Err(LaunchError::InvalidWorkingDirectory {
            path: dir.to_path_buf(),
            reason: "working directory must be an absolute path".into(),
        });
    }
    for comp in dir.components() {
        if matches!(comp, Component::ParentDir) {
            return Err(LaunchError::InvalidWorkingDirectory {
                path: dir.to_path_buf(),
                reason: "working directory must not contain '..'".into(),
            });
        }
    }
    if !dir.is_dir() {
        return Err(LaunchError::InvalidWorkingDirectory {
            path: dir.to_path_buf(),
            reason: "working directory does not exist or is not a directory".into(),
        });
    }
    Ok(dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()))
}

/// Environment variables that Kyro always removes from a child process
/// because they can redirect a launched program at attacker-controlled code.
pub const STRIPPED_ENV_VARS: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "LD_AUDIT",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "BASH_ENV",
    "ENV",
    "CDPATH",
    "IFS",
    "PYTHONSTARTUP",
    "PYTHONPATH",
    "NODE_OPTIONS",
    "PERL5OPT",
    "RUBYOPT",
];

/// Returns the environment overrides that survive sanitisation.
pub fn sanitize_env(overrides: &[(String, String)]) -> Vec<(String, String)> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (k, v) in overrides {
        let upper = k.to_ascii_uppercase();
        if STRIPPED_ENV_VARS.contains(&upper.as_str()) {
            continue;
        }
        if k.contains('\0') || v.contains('\0') || k.contains('=') || k.is_empty() {
            continue;
        }
        if seen.insert(upper) {
            out.push((k.clone(), v.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "kyro_launcher_{}_{}_{}",
            tag,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn rejects_relative_explicit_paths() {
        let r = ExecutableResolver::new();
        let spec = ProgramSpec::new("./evil");
        let err = r.resolve(&spec).unwrap_err();
        assert!(matches!(err, LaunchError::UntrustedLocation { .. }));
    }

    #[test]
    fn rejects_parent_traversal_in_explicit_paths() {
        let r = ExecutableResolver::new();
        let spec = ProgramSpec::new("/usr/bin/../../tmp/evil");
        let err = r.resolve(&spec).unwrap_err();
        assert!(matches!(err, LaunchError::UntrustedLocation { .. }));
    }

    #[test]
    fn rejects_missing_executable() {
        let r = ExecutableResolver::new();
        let spec = ProgramSpec::new("definitely-not-a-real-binary-xyz");
        let err = r.resolve(&spec).unwrap_err();
        assert!(matches!(err, LaunchError::NotFound { .. }));
    }

    #[test]
    fn rejects_null_byte_in_arguments() {
        let r = ExecutableResolver::new();
        let spec = ProgramSpec::new("sh").with_args(vec!["a\0b"]);
        let err = r.resolve(&spec).unwrap_err();
        assert!(matches!(err, LaunchError::InvalidArguments { .. }));
    }

    #[test]
    fn shell_metacharacters_in_arguments_stay_literal() {
        let r = ExecutableResolver::new();
        let payload = "; rm -rf / #";
        let spec = ProgramSpec::new("echo").with_args(vec![payload]);
        let resolved = r.resolve(&spec).expect("echo should resolve");
        assert_eq!(resolved.args[0], payload);
    }

    #[test]
    fn resolves_absolute_executable() {
        let dir = tmpdir("abs");
        let bin = dir.join("hello");
        std::fs::write(&bin, "#!/bin/sh\necho hi\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let r = ExecutableResolver::new();
        let resolved = r
            .resolve(&ProgramSpec::new(bin.to_string_lossy().to_string()))
            .unwrap();
        assert!(resolved.path.ends_with("hello"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn skips_world_writable_search_dirs_and_uses_the_next_trusted_executable() {
        use std::os::unix::fs::PermissionsExt;

        let root = tmpdir("trusted_search");
        let untrusted_dir = root.join("untrusted");
        let trusted_dir = root.join("trusted");
        std::fs::create_dir_all(&untrusted_dir).unwrap();
        std::fs::create_dir_all(&trusted_dir).unwrap();
        std::fs::set_permissions(
            &untrusted_dir,
            std::fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        std::fs::set_permissions(
            &trusted_dir,
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();

        for dir in [&untrusted_dir, &trusted_dir] {
            let executable = dir.join("npm");
            std::fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(
                &executable,
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        let resolver = ExecutableResolver::with_search_dirs(
            ExecutableResolver::trusted_search_dirs(vec![
                untrusted_dir.clone(),
                trusted_dir.clone(),
            ]),
        );
        let resolved = resolver
            .resolve(&ProgramSpec::new("npm"))
            .expect("the trusted npm candidate should be selected");
        assert_eq!(resolved.path, trusted_dir.join("npm").canonicalize().unwrap());
        assert!(!resolver.search_dirs().contains(&untrusted_dir));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_non_executable_file() {
        let dir = tmpdir("nonexec");
        let bin = dir.join("plain.txt");
        std::fs::write(&bin, "not executable").unwrap();
        let r = ExecutableResolver::new();
        let err = r
            .resolve(&ProgramSpec::new(bin.to_string_lossy().to_string()))
            .unwrap_err();
        assert!(matches!(err, LaunchError::NotExecutable { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_missing_working_directory() {
        let r = ExecutableResolver::new();
        let spec = ProgramSpec::new("echo").with_cwd("/definitely/not/here/xyz");
        let err = r.resolve(&spec).unwrap_err();
        assert!(matches!(err, LaunchError::InvalidWorkingDirectory { .. }));
    }

    #[test]
    fn strips_dangerous_environment_variables() {
        let env = vec![
            ("LD_PRELOAD".to_string(), "/tmp/evil.so".to_string()),
            ("DISPLAY".to_string(), ":99".to_string()),
        ];
        let clean = sanitize_env(&env);
        assert_eq!(clean.len(), 1);
        assert_eq!(clean[0].0, "DISPLAY");
    }
}
