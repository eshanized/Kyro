//! Argument-aware executable policies for the shell authorisation engine.
//!
//! The previous implementation classified a command as safe based on the
//! *name* of the executable alone.  `find` was allow-listed, so `find . -delete`
//! was auto-approved, and `cat /etc/shadow` was auto-approved because `cat`
//! was allow-listed.  Each executable below therefore declares an explicit
//! argument policy: which flags are permitted, which flags are forbidden
//! outright, and how path-like arguments are constrained.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// How an executable may treat path-like arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathArgPolicy {
    /// The executable does not accept positional arguments at all.
    None,
    /// Positional arguments are literal text, not filesystem paths.
    Text,
    /// Path arguments must resolve inside the configured workspace root.
    WorkspaceOnly,
    /// Path arguments are accepted, but may not point at a protected system
    /// location (see `PROTECTED_READ_PATHS`).
    NoProtectedPaths,
}

/// The complete policy for a single executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutablePolicy {
    /// The program name as it appears in the token vector.
    pub name: &'static str,
    /// Permitted sub-commands (for tools such as `git`).  Empty means the
    /// executable has no sub-command concept.
    pub allowed_subcommands: &'static [&'static str],
    /// Flags that are always rejected, matched exactly (`-x`) or as a prefix
    /// (`-x:` means "any flag starting with `-x`").
    pub denied_flags: &'static [&'static str],
    /// When non-empty, only these flags are permitted.
    pub allowed_flags: &'static [&'static str],
    /// Constraint applied to path-like arguments.
    pub path_args: PathArgPolicy,
    /// `true` when the executable is expected to mutate state.
    pub mutating: bool,
    /// Human readable explanation used in approval prompts.
    pub description: &'static str,
}

impl ExecutablePolicy {
    pub fn permits_subcommand(&self, sub: &str) -> bool {
        if self.allowed_subcommands.is_empty() {
            return true;
        }
        self.allowed_subcommands.contains(&sub)
    }

    pub fn flag_is_denied(&self, flag: &str) -> bool {
        for denied in self.denied_flags {
            if let Some(prefix) = denied.strip_suffix(':') {
                if flag.starts_with(prefix) {
                    return true;
                }
            } else if flag == *denied {
                return true;
            }
        }
        false
    }

    pub fn flag_is_allowed(&self, flag: &str) -> bool {
        if self.allowed_flags.is_empty() {
            return true;
        }
        self.allowed_flags.contains(&flag)
    }
}

/// Directories and files that read-oriented utilities may not touch.
///
/// These are matched on path *components*, not as substrings, so a project
/// directory named `etcetera` is not affected.
pub const PROTECTED_READ_PATHS: &[&str] = &[
    "/etc",
    "/boot",
    "/sys",
    "/proc",
    "/dev",
    "/var/run",
    "/var/log",
    "/root",
    "/run",
    "/.ssh",
    "/.gnupg",
    "/.aws",
    "/.kube",
    "/.config/gcloud",
    "/etc/shadow",
    "/etc/passwd",
    "/etc/sudoers",
];

/// Additional protected file names, matched on the final component.
pub const PROTECTED_READ_FILES: &[&str] = &[
    "shadow",
    "passwd",
    "sudoers",
    "id_rsa",
    "id_ed25519",
    "credentials",
    "credentials.json",
    ".bashrc",
    ".zshrc",
    ".profile",
    ".netrc",
    ".npmrc",
    ".pypirc",
    "authorized_keys",
    "known_hosts",
];

/// `true` when `path` points at a protected system location.
///
/// The check is component-wise: a path is protected when it *starts with* one
/// of the protected roots, when it is inside a home directory and touches a
/// protected dot-directory, or when its final component is a protected file.
pub fn is_protected_path(path: &Path) -> bool {
    let raw = path.to_string_lossy().to_string();
    let normalized = raw.trim_end_matches('/');
    if normalized.is_empty() {
        return false;
    }

    // Component-wise prefix match against the protected roots.
    for prot in PROTECTED_READ_PATHS {
        if normalized == *prot || normalized.starts_with(&format!("{}/", prot)) {
            return true;
        }
    }

    // Home-relative protected directories (e.g. ~/.ssh, ~/.aws).
    let components: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    for comp in &components {
        if matches!(
            *comp,
            ".ssh" | ".gnupg" | ".aws" | ".kube" | ".netrc" | ".npmrc" | ".pypirc"
        ) {
            return true;
        }
    }

    // Final-component file names.
    if let Some(last) = components.last() {
        if PROTECTED_READ_FILES.contains(last) {
            return true;
        }
    }

    false
}

/// The policy table.
///
/// Every executable listed here is read-only by construction.  Anything not
/// listed is treated as unknown and therefore requires explicit approval.
pub fn policy_for(executable: &str) -> Option<ExecutablePolicy> {
    let base = executable.rsplit('/').next().unwrap_or(executable);
    POLICY_TABLE.iter().copied().find(|p| p.name == base)
}

/// All policies, for tests and documentation.
pub const POLICY_TABLE: &[ExecutablePolicy] = &[
    ExecutablePolicy {
        name: "ls",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::NoProtectedPaths,
        mutating: false,
        description: "list directory contents",
    },
    ExecutablePolicy {
        name: "dir",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::NoProtectedPaths,
        mutating: false,
        description: "list directory contents",
    },
    ExecutablePolicy {
        name: "pwd",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::None,
        mutating: false,
        description: "print working directory",
    },
    ExecutablePolicy {
        name: "whoami",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::None,
        mutating: false,
        description: "print current user",
    },
    ExecutablePolicy {
        name: "uname",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::None,
        mutating: false,
        description: "print system information",
    },
    ExecutablePolicy {
        name: "date",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::None,
        mutating: false,
        description: "print the current date",
    },
    ExecutablePolicy {
        name: "uptime",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::None,
        mutating: false,
        description: "print uptime",
    },
    ExecutablePolicy {
        name: "df",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::None,
        mutating: false,
        description: "report filesystem usage",
    },
    ExecutablePolicy {
        name: "free",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::None,
        mutating: false,
        description: "report memory usage",
    },
    ExecutablePolicy {
        name: "ps",
        allowed_subcommands: &[],
        denied_flags: &["--sort=:", "-U:", "-p:"],
        allowed_flags: &[],
        path_args: PathArgPolicy::Text,
        mutating: false,
        description: "report process status",
    },
    ExecutablePolicy {
        name: "echo",
        allowed_subcommands: &[],
        denied_flags: &["-e:", "-n:", "-E:"],
        allowed_flags: &[],
        path_args: PathArgPolicy::Text,
        mutating: false,
        description: "print text",
    },
    ExecutablePolicy {
        name: "which",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::Text,
        mutating: false,
        description: "locate an executable",
    },
    ExecutablePolicy {
        name: "cat",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::NoProtectedPaths,
        mutating: false,
        description: "read file contents",
    },
    ExecutablePolicy {
        name: "head",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::NoProtectedPaths,
        mutating: false,
        description: "read the first lines of a file",
    },
    ExecutablePolicy {
        name: "tail",
        allowed_subcommands: &[],
        // `tail -f` follows a file forever, which stalls the agent.
        denied_flags: &["-f", "--follow"],
        allowed_flags: &[],
        path_args: PathArgPolicy::NoProtectedPaths,
        mutating: false,
        description: "read the last lines of a file",
    },
    ExecutablePolicy {
        name: "grep",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::NoProtectedPaths,
        mutating: false,
        description: "search file contents",
    },
    ExecutablePolicy {
        name: "find",
        allowed_subcommands: &[],
        // Every flag that mutates the filesystem, spawns a command, or writes
        // results to a file.
        denied_flags: &[
            "-delete",
            "-exec",
            "-execdir",
            "-ok",
            "-okdir",
            "-fprint",
            "-fprintf",
            "-fls",
            "-files0-from",
        ],
        allowed_flags: &[],
        path_args: PathArgPolicy::NoProtectedPaths,
        mutating: false,
        description: "search for files",
    },
    ExecutablePolicy {
        name: "wc",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::NoProtectedPaths,
        mutating: false,
        description: "count lines, words and bytes",
    },
    ExecutablePolicy {
        name: "stat",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::NoProtectedPaths,
        mutating: false,
        description: "display file metadata",
    },
    ExecutablePolicy {
        name: "file",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::NoProtectedPaths,
        mutating: false,
        description: "determine file type",
    },
    ExecutablePolicy {
        name: "git",
        allowed_subcommands: &[
            "status",
            "log",
            "diff",
            "branch",
            "show",
            "tag",
            "rev-parse",
            "ls-files",
            "ls-remote",
            "describe",
            "blame",
            "shortlog",
        ],
        denied_flags: &[],
        allowed_flags: &[],
        path_args: PathArgPolicy::NoProtectedPaths,
        mutating: false,
        description: "inspect repository state",
    },
    ExecutablePolicy {
        name: "cargo",
        allowed_subcommands: &[],
        denied_flags: &[],
        // Only version queries are auto-approved.
        allowed_flags: &["--version", "-V", "-v"],
        path_args: PathArgPolicy::None,
        mutating: true,
        description: "query the Rust toolchain version",
    },
    ExecutablePolicy {
        name: "node",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &["--version", "-v"],
        path_args: PathArgPolicy::None,
        mutating: true,
        description: "query the Node.js version",
    },
    ExecutablePolicy {
        name: "npm",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &["--version", "-v"],
        path_args: PathArgPolicy::None,
        mutating: true,
        description: "query the npm version",
    },
    ExecutablePolicy {
        name: "rustc",
        allowed_subcommands: &[],
        denied_flags: &[],
        allowed_flags: &["--version", "-V", "-v"],
        path_args: PathArgPolicy::None,
        mutating: true,
        description: "query the Rust compiler version",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_policy_denies_delete_and_exec() {
        let p = policy_for("find").unwrap();
        assert!(p.flag_is_denied("-delete"));
        assert!(p.flag_is_denied("-exec"));
        assert!(p.flag_is_denied("-execdir"));
        assert!(p.flag_is_denied("-ok"));
        assert!(p.flag_is_denied("-fprint"));
        assert!(!p.flag_is_denied("-name"));
    }

    #[test]
    fn git_policy_restricts_subcommands() {
        let p = policy_for("git").unwrap();
        assert!(p.permits_subcommand("status"));
        assert!(!p.permits_subcommand("push"));
        assert!(!p.permits_subcommand("config"));
        assert!(!p.permits_subcommand("remote"));
    }

    #[test]
    fn protected_paths_are_component_wise() {
        assert!(is_protected_path(Path::new("/etc/shadow")));
        assert!(is_protected_path(Path::new("/etc/passwd")));
        assert!(is_protected_path(Path::new("/home/u/.ssh/id_rsa")));
        assert!(is_protected_path(Path::new("/proc/1/environ")));
        // Not protected: the string merely *contains* "etc".
        assert!(!is_protected_path(Path::new("/home/u/Desktop/ethereum")));
        assert!(!is_protected_path(Path::new("/home/u/projects/etc-config")));
        assert!(!is_protected_path(Path::new("/home/u/Desktop/Runstead")));
    }

    #[test]
    fn version_only_tools_reject_other_flags() {
        let p = policy_for("node").unwrap();
        assert!(p.flag_is_allowed("--version"));
        assert!(!p.flag_is_allowed("script.js"));
    }
}
