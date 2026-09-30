//! Git actions an agent command performed, recognised the way the reference
//! does after `item/completed` (`fFn`/`pFn`): a `git push`, or a `git
//! checkout`/`git switch` naming a branch, inside a shell command line that
//! may `cd` first, pass `git -C <dir>`, or start with `env` and `VAR=value`
//! prefixes, possibly wrapped in `bash -c "..."`.
//!
//! Recognising them never writes an attachment. The caller only refreshes
//! what may have changed: pull-request lookups after a push, the checkout's
//! Git metadata, and the thread's branch.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitAction {
    Push {
        cwd: PathBuf,
    },
    /// `git checkout <branch>` / `git switch <branch>`, with or without
    /// `-b`/`-c`: the checkout now has this branch.
    CreateBranch {
        cwd: PathBuf,
        branch: String,
    },
}

impl GitAction {
    pub fn cwd(&self) -> &Path {
        match self {
            Self::Push { cwd } | Self::CreateBranch { cwd, .. } => cwd,
        }
    }
}

const SHELLS: &[&str] = &[
    "bash",
    "sh",
    "zsh",
    "dash",
    "ksh",
    "fish",
    "pwsh",
    "powershell",
    "cmd",
];

fn is_separator(token: &str) -> bool {
    matches!(token, ";" | "&" | "|" | "(" | ")" | "&&" | "||")
}

fn is_assignment(token: &str) -> bool {
    let Some((name, value)) = token.split_once('=') else {
        return false;
    };
    !value.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A POSIX-ish tokenizer: quotes group, backslashes escape, and the
/// separators `; & | ( )` stand alone.
fn tokenize(script: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut chars = script.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_token = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    current.push(c);
                }
            }
            '"' => {
                in_token = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => {
                            if let Some(next) = chars.next() {
                                current.push(next);
                            }
                        }
                        _ => current.push(c),
                    }
                }
            }
            '\\' => {
                in_token = true;
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ';' | '&' | '|' | '(' | ')' | '\n' => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
                tokens.push(if c == '\n' {
                    ";".to_owned()
                } else {
                    c.to_string()
                });
            }
            c if c.is_whitespace() => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            _ => {
                in_token = true;
                current.push(c);
            }
        }
    }
    if in_token {
        tokens.push(current);
    }
    tokens
}

/// `bash -lc "script"` (after any `env`/`VAR=` prefix) is the script itself.
fn unwrap_shell(command: &str) -> String {
    let tokens = tokenize(command);
    let mut index = 0;
    while index < tokens.len() && (tokens[index] == "env" || is_assignment(&tokens[index])) {
        index += 1;
    }
    let Some(program) = tokens.get(index) else {
        return command.to_owned();
    };
    let name = program.rsplit('/').next().unwrap_or(program);
    let name = name.strip_suffix(".exe").unwrap_or(name);
    if !SHELLS.contains(&name) {
        return command.to_owned();
    }
    let mut rest = index + 1;
    while let Some(flag) = tokens.get(rest) {
        if flag.starts_with('-') || flag.starts_with('/') {
            let takes_script =
                flag.contains('c') || flag.eq_ignore_ascii_case("-command") || flag == "/c";
            rest += 1;
            if takes_script {
                return tokens
                    .get(rest)
                    .map(|script| unwrap_shell(script))
                    .unwrap_or_default();
            }
        } else {
            break;
        }
    }
    command.to_owned()
}

fn join(cwd: &Path, target: &str) -> PathBuf {
    let path = Path::new(target);
    let mut joined = if path.is_absolute() {
        PathBuf::new()
    } else {
        cwd.to_owned()
    };
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                joined.pop();
            }
            std::path::Component::CurDir => {}
            other => joined.push(other.as_os_str()),
        }
    }
    joined
}

fn branch_argument(tokens: &[String]) -> Option<String> {
    for token in tokens {
        if is_separator(token) || token == "--" || token == "--detach" || token == "-d" {
            return None;
        }
        if !token.starts_with('-') {
            return Some(token.clone());
        }
    }
    None
}

/// The Git actions of one command line run in `cwd`.
pub fn git_actions(command: &str, cwd: &Path) -> Vec<GitAction> {
    let tokens = tokenize(&unwrap_shell(command));
    let mut actions = Vec::new();
    let mut directory = cwd.to_owned();
    let mut at_start = true;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if is_separator(token) {
            at_start = true;
            index += 1;
            continue;
        }
        if !at_start || token == "env" || is_assignment(token) {
            index += 1;
            continue;
        }
        at_start = false;
        if token == "cd" {
            if let Some(target) = tokens.get(index + 1)
                && !target
                    .chars()
                    .any(|c| matches!(c, '$' | '`' | '*' | '?' | '[' | ']' | '~'))
            {
                directory = join(&directory, target);
            }
            index += 1;
            continue;
        }
        if token != "git" {
            index += 1;
            continue;
        }
        let mut git_cwd = directory.clone();
        let mut position = index + 1;
        while tokens.get(position).map(String::as_str) == Some("-C") {
            if let Some(target) = tokens.get(position + 1) {
                git_cwd = join(&git_cwd, target);
            }
            position += 2;
        }
        match tokens.get(position).map(String::as_str) {
            Some("push") => actions.push(GitAction::Push { cwd: git_cwd }),
            Some("checkout" | "switch") => {
                if let Some(branch) = branch_argument(&tokens[position + 1..]) {
                    actions.push(GitAction::CreateBranch {
                        cwd: git_cwd,
                        branch,
                    });
                }
            }
            _ => {}
        }
        index = position + 1;
    }
    actions
}

/// The command of a dynamic `exec`/`exec_command` tool call: its `cmd`
/// argument, and the directory from `workdir`, `cwd` or `working_directory`.
pub fn dynamic_exec_command(arguments: &serde_json::Value) -> Option<(String, Option<String>)> {
    let arguments = match arguments {
        serde_json::Value::String(text) => serde_json::from_str(text).ok()?,
        other => other.clone(),
    };
    let command = arguments.get("cmd")?.as_str()?.to_owned();
    let directory = ["workdir", "cwd", "working_directory"]
        .iter()
        .find_map(|key| arguments.get(*key)?.as_str().map(str::to_owned));
    Some((command, directory))
}

/// The reference skips a dynamic exec whose output reads like a failure
/// (`command failed`, `exit code 1`, `exit_code: 2`, …).
pub fn output_looks_failed(output: &str) -> bool {
    let lower = output.to_lowercase();
    if lower.contains("command failed") {
        return true;
    }
    let bytes = lower.as_bytes();
    let nonzero_after = |start: usize| {
        let rest = lower[start..].trim_start_matches([' ', ':', '=', '"', '\'', '\\']);
        rest.chars()
            .next()
            .is_some_and(|c| ('1'..='9').contains(&c))
    };
    for pattern in [
        "exited with exit code",
        "exit with exit code",
        "exited with code",
        "exit with code",
        "exit_code",
        "exit-code",
        "exitcode",
    ] {
        let mut from = 0;
        while let Some(at) = lower[from..].find(pattern) {
            let end = from + at + pattern.len();
            if nonzero_after(end) {
                return true;
            }
            from = end;
            if from >= bytes.len() {
                break;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actions(command: &str) -> Vec<GitAction> {
        git_actions(command, Path::new("/repo"))
    }

    #[test]
    fn push_and_branch_commands_are_recognised_through_prefixes() {
        assert_eq!(
            actions("git push -u origin HEAD"),
            [GitAction::Push {
                cwd: "/repo".into()
            }]
        );
        assert_eq!(
            actions("/bin/zsh -lc 'cd sub && git checkout -b feat/x'"),
            [GitAction::CreateBranch {
                cwd: "/repo/sub".into(),
                branch: "feat/x".into()
            }]
        );
        assert_eq!(
            actions("env GIT_TRACE=1 FOO=bar git -C ../other switch -c topic"),
            [GitAction::CreateBranch {
                cwd: "/other".into(),
                branch: "topic".into()
            }]
        );
        // A plain checkout of a branch counts too, as in the reference.
        assert_eq!(
            actions("git checkout main; git push"),
            [
                GitAction::CreateBranch {
                    cwd: "/repo".into(),
                    branch: "main".into()
                },
                GitAction::Push {
                    cwd: "/repo".into()
                },
            ]
        );
    }

    #[test]
    fn detached_and_unrelated_commands_are_ignored() {
        assert!(actions("git checkout -- file.txt").is_empty());
        assert!(actions("git switch --detach HEAD~1").is_empty());
        assert!(actions("echo git push").is_empty());
        assert!(actions("git status").is_empty());
        // `cd` to a path with shell expansion leaves the directory alone.
        assert_eq!(
            actions("cd $HOME && git push"),
            [GitAction::Push {
                cwd: "/repo".into()
            }]
        );
    }

    #[test]
    fn dynamic_exec_arguments_and_failure_output() {
        let arguments = serde_json::json!({ "cmd": "git push", "workdir": "sub" });
        assert_eq!(
            dynamic_exec_command(&arguments),
            Some(("git push".to_owned(), Some("sub".to_owned())))
        );
        assert_eq!(
            dynamic_exec_command(&serde_json::Value::String(r#"{"cmd":"ls"}"#.into())),
            Some(("ls".to_owned(), None))
        );
        assert!(output_looks_failed("Process exited with exit code 1"));
        assert!(output_looks_failed("{\"exit_code\": 2}"));
        assert!(!output_looks_failed("Process exited with code 0"));
    }
}
