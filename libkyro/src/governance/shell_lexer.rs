//! A small, explicit POSIX-shell lexer.
//!
//! The authorization engine and the execution engine must agree on *exactly*
//! which executable and argument vector will run.  The previous implementation
//! authorised a string with `shell-words` and then executed the same string
//! through `sh -c`, which re-parses the string with different rules (variable
//! expansion, command substitution, globbing, redirections) and therefore could
//! execute something the authorisation never inspected.
//!
//! This lexer produces the token vector *and* records whether the raw command
//! relied on any shell-only construct.  When it did, the caller must either
//! require approval or execute the command through a real shell; when it did
//! not, the token vector can be executed directly, and the two can never
//! disagree.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellConstruct {
    /// `|`, `||`, `&`, `&&`, `;`, `;;`
    Operator,
    /// `<`, `>`, `>>`, `<<`, `<<<`, `2>`, `&>`
    Redirection,
    /// `$(...)`, backticks, `${...}`
    Substitution,
    /// Process substitution `<(...)` / `>(...)`
    ProcessSubstitution,
    /// Brace group `{ ...; }`
    BraceGroup,
    /// `(` / `)` subshell
    Subshell,
    /// A newline separating commands
    Newline,
    /// Backslash escaping outside quotes
    Escape,
    /// A glob character (`*`, `?`, `[`)
    Glob,
    /// `~` at the start of a word
    Tilde,
    /// A variable reference (`$VAR`)
    Variable,
}

impl ShellConstruct {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShellConstruct::Operator => "operator",
            ShellConstruct::Redirection => "redirection",
            ShellConstruct::Substitution => "substitution",
            ShellConstruct::ProcessSubstitution => "process substitution",
            ShellConstruct::BraceGroup => "brace group",
            ShellConstruct::Subshell => "subshell",
            ShellConstruct::Newline => "newline",
            ShellConstruct::Escape => "escape",
            ShellConstruct::Glob => "glob",
            ShellConstruct::Tilde => "tilde expansion",
            ShellConstruct::Variable => "variable expansion",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LexedCommand {
    /// The argument vector, in order.  Empty when the command is empty.
    pub words: Vec<String>,
    /// Shell-only constructs that were present *outside* of quotes.
    pub constructs: Vec<ShellConstruct>,
    /// `true` when the command contains no shell-only construct and can be
    /// executed directly as an argument vector.
    pub is_simple: bool,
}

impl LexedCommand {
    pub fn executable(&self) -> Option<&str> {
        self.words.first().map(|s| s.as_str())
    }

    pub fn has_construct(&self, c: &ShellConstruct) -> bool {
        self.constructs.contains(c)
    }
}

/// Lexes a shell command line.
///
/// Quoted content is returned literally and never contributes a construct, so
/// `ls "a|b"` is a simple command with the literal argument `a|b`, while
/// `ls a|b` is not simple.
pub fn lex(input: &str) -> LexedCommand {
    let chars: Vec<char> = input.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut has_word = false;
    let mut constructs: Vec<ShellConstruct> = Vec::new();
    let mut i = 0usize;

    let push_word = |current: &mut String, has_word: &mut bool, words: &mut Vec<String>| {
        if *has_word {
            words.push(std::mem::take(current));
            *has_word = false;
        }
    };

    while i < chars.len() {
        let c = chars[i];

        match c {
            '\n' => {
                push_word(&mut current, &mut has_word, &mut words);
                constructs.push(ShellConstruct::Newline);
                i += 1;
            }
            ' ' | '\t' | '\r' => {
                push_word(&mut current, &mut has_word, &mut words);
                i += 1;
            }
            '\'' => {
                // Single quotes: everything literal until the next quote.
                has_word = true;
                i += 1;
                let mut closed = false;
                while i < chars.len() {
                    if chars[i] == '\'' {
                        closed = true;
                        i += 1;
                        break;
                    }
                    current.push(chars[i]);
                    i += 1;
                }
                if !closed {
                    constructs.push(ShellConstruct::Operator); // unterminated quote
                }
            }
            '"' => {
                has_word = true;
                i += 1;
                let mut closed = false;
                while i < chars.len() {
                    let ch = chars[i];
                    if ch == '"' {
                        closed = true;
                        i += 1;
                        break;
                    }
                    if ch == '\\' {
                        // Inside double quotes only these are escapable.
                        if i + 1 < chars.len() {
                            let nxt = chars[i + 1];
                            if matches!(nxt, '$' | '`' | '"' | '\\') {
                                current.push(nxt);
                                i += 2;
                                continue;
                            }
                        }
                        current.push(ch);
                        i += 1;
                        continue;
                    }
                    if ch == '$' {
                        if i + 1 < chars.len() && (chars[i + 1] == '(' || chars[i + 1] == '{') {
                            constructs.push(ShellConstruct::Substitution);
                        } else {
                            constructs.push(ShellConstruct::Variable);
                        }
                    }
                    if ch == '`' {
                        constructs.push(ShellConstruct::Substitution);
                    }
                    current.push(ch);
                    i += 1;
                }
                if !closed {
                    constructs.push(ShellConstruct::Operator);
                }
            }
            '\\' => {
                has_word = true;
                constructs.push(ShellConstruct::Escape);
                if i + 1 < chars.len() {
                    if chars[i + 1] == '\n' {
                        i += 2;
                        continue;
                    }
                    current.push(chars[i + 1]);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            '$' => {
                has_word = true;
                if i + 1 < chars.len() {
                    match chars[i + 1] {
                        '(' => {
                            constructs.push(ShellConstruct::Substitution);
                            // Skip the balanced parenthesised body.
                            let mut depth = 0usize;
                            let mut j = i;
                            while j < chars.len() {
                                if chars[j] == '(' {
                                    depth += 1;
                                } else if chars[j] == ')' {
                                    depth = depth.saturating_sub(1);
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                j += 1;
                            }
                            i = (j + 1).min(chars.len());
                            continue;
                        }
                        '{' => {
                            constructs.push(ShellConstruct::Substitution);
                            let mut j = i + 2;
                            while j < chars.len() && chars[j] != '}' {
                                j += 1;
                            }
                            i = (j + 1).min(chars.len());
                            continue;
                        }
                        _ => {
                            constructs.push(ShellConstruct::Variable);
                            current.push('$');
                            i += 1;
                            continue;
                        }
                    }
                }
                current.push('$');
                i += 1;
            }
            '`' => {
                has_word = true;
                constructs.push(ShellConstruct::Substitution);
                i += 1;
                let mut closed = false;
                while i < chars.len() {
                    if chars[i] == '`' {
                        closed = true;
                        i += 1;
                        break;
                    }
                    current.push(chars[i]);
                    i += 1;
                }
                if !closed {
                    constructs.push(ShellConstruct::Operator);
                }
            }
            '<' => {
                push_word(&mut current, &mut has_word, &mut words);
                if i + 1 < chars.len() && chars[i + 1] == '(' {
                    constructs.push(ShellConstruct::ProcessSubstitution);
                } else {
                    constructs.push(ShellConstruct::Redirection);
                }
                i += 1;
            }
            '>' => {
                push_word(&mut current, &mut has_word, &mut words);
                if i + 1 < chars.len() && chars[i + 1] == '(' {
                    constructs.push(ShellConstruct::ProcessSubstitution);
                } else {
                    constructs.push(ShellConstruct::Redirection);
                }
                i += 1;
            }
            '|' | '&' | ';' => {
                push_word(&mut current, &mut has_word, &mut words);
                constructs.push(ShellConstruct::Operator);
                i += 1;
            }
            '(' | ')' => {
                push_word(&mut current, &mut has_word, &mut words);
                constructs.push(ShellConstruct::Subshell);
                i += 1;
            }
            '{' => {
                // Only a brace *group* when followed by whitespace and closed by
                // `}`.  A lone `{` is a normal character in POSIX sh.
                if i + 1 < chars.len() && chars[i + 1].is_whitespace() {
                    constructs.push(ShellConstruct::BraceGroup);
                }
                has_word = true;
                current.push(c);
                i += 1;
            }
            '*' | '?' => {
                has_word = true;
                constructs.push(ShellConstruct::Glob);
                current.push(c);
                i += 1;
            }
            '[' => {
                has_word = true;
                constructs.push(ShellConstruct::Glob);
                current.push(c);
                i += 1;
            }
            '~' => {
                if !has_word && current.is_empty() {
                    constructs.push(ShellConstruct::Tilde);
                }
                has_word = true;
                current.push(c);
                i += 1;
            }
            '\0' => {
                // NUL cannot appear in a POSIX command line; treat as fatal.
                constructs.push(ShellConstruct::Operator);
                i += 1;
            }
            _ => {
                has_word = true;
                current.push(c);
                i += 1;
            }
        }
    }

    push_word(&mut current, &mut has_word, &mut words);

    // De-duplicate constructs while preserving order.
    let mut unique: Vec<ShellConstruct> = Vec::new();
    for c in constructs {
        if !unique.contains(&c) {
            unique.push(c);
        }
    }

    let is_simple = unique.is_empty() && !words.is_empty();

    LexedCommand {
        words,
        constructs: unique,
        is_simple,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_command_lexes_to_argv() {
        let l = lex("ls -la /tmp");
        assert!(l.is_simple);
        assert_eq!(l.words, vec!["ls", "-la", "/tmp"]);
    }

    #[test]
    fn pipeline_is_not_simple() {
        let l = lex("ls | grep foo");
        assert!(!l.is_simple);
        assert!(l.has_construct(&ShellConstruct::Operator));
    }

    #[test]
    fn command_substitution_is_not_simple() {
        let l = lex("ls $(whoami)");
        assert!(!l.is_simple);
        assert!(l.has_construct(&ShellConstruct::Substitution));
    }

    #[test]
    fn backtick_substitution_is_not_simple() {
        let l = lex("echo `id`");
        assert!(!l.is_simple);
        assert!(l.has_construct(&ShellConstruct::Substitution));
    }

    #[test]
    fn redirection_is_not_simple() {
        let l = lex("echo hi > /etc/passwd");
        assert!(!l.is_simple);
        assert!(l.has_construct(&ShellConstruct::Redirection));
    }

    #[test]
    fn quoted_operator_stays_literal() {
        let l = lex("grep \"a|b\" file");
        assert!(l.is_simple, "quoted pipe must not mark the command complex");
        assert_eq!(l.words, vec!["grep", "a|b", "file"]);
    }

    #[test]
    fn variable_expansion_is_not_simple() {
        let l = lex("cat $HOME/.bashrc");
        assert!(!l.is_simple);
        assert!(l.has_construct(&ShellConstruct::Variable));
    }

    #[test]
    fn env_assignment_prefix_is_lexed_as_a_word() {
        let l = lex("FOO=bar ls");
        assert!(l.is_simple);
        assert_eq!(l.words[0], "FOO=bar");
    }

    #[test]
    fn semicolon_chain_is_not_simple() {
        let l = lex("ls; rm -rf /");
        assert!(!l.is_simple);
        assert!(l.has_construct(&ShellConstruct::Operator));
    }

    #[test]
    fn empty_command_yields_no_words() {
        let l = lex("   ");
        assert!(l.words.is_empty());
        assert!(!l.is_simple);
    }

    #[test]
    fn single_quotes_are_literal() {
        let l = lex("echo '$(whoami)'");
        assert!(l.is_simple);
        assert_eq!(l.words[1], "$(whoami)");
    }
}
