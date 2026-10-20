//! Resolve the CLI for desktop launches without changing the application's
//! environment or importing shell credentials/configuration into the process.

use std::{path::PathBuf, process::Command};

use anyhow::Result;

pub(crate) struct ToolExecutable {
    pub(crate) program: PathBuf,
    pub(crate) path: Option<std::ffi::OsString>,
    pub(crate) source: &'static str,
}

impl ToolExecutable {
    pub(crate) fn resolve(name: &str) -> Result<Self> {
        #[cfg(unix)]
        {
            unix::resolve(name)
        }
        #[cfg(not(unix))]
        {
            Ok(Self {
                program: PathBuf::from(name),
                path: None,
                source: "inherited PATH",
            })
        }
    }

    pub(crate) fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        if let Some(path) = &self.path {
            command.env("PATH", path);
        }
        command
    }
}

#[cfg(unix)]
mod unix {
    use std::{
        env,
        ffi::{OsStr, OsString},
        io::Read,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt, process::CommandExt},
        path::{Path, PathBuf},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    use anyhow::{Context as _, Result, bail};

    use super::ToolExecutable;

    const SHELL_TIMEOUT: Duration = Duration::from_secs(5);
    const OUTPUT_LIMIT: usize = 64 * 1024;
    const PATH_MARKER: &[u8] = b"\0ECHORA_TOOL_PATH\0";
    const PATH_QUERY: &str = "printf '\\000ECHORA_TOOL_PATH\\000%s\\000' \"$PATH\"";

    pub(crate) fn resolve(name: &str) -> Result<ToolExecutable> {
        let inherited = env::var_os("PATH").unwrap_or_default();
        let home = env::var_os("HOME").map(PathBuf::from);
        let shell = env::var_os("SHELL")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && executable(path))
            .unwrap_or_else(|| PathBuf::from("/bin/sh"));
        resolve_with(name, &inherited, home.as_deref(), &shell)
    }

    fn resolve_with(
        name: &str,
        inherited: &OsStr,
        home: Option<&Path>,
        shell: &Path,
    ) -> Result<ToolExecutable> {
        if let Some(cli) = find_in_path(inherited, name, "inherited PATH") {
            return Ok(cli);
        }
        let shell_result = shell_path(shell, home, inherited, SHELL_TIMEOUT);
        if let Ok(path) = &shell_result
            && let Some(cli) = find_in_path(path, name, "login shell PATH")
        {
            return Ok(cli);
        }
        // Homebrew/native/npm/Volta installations can remain usable even if a
        // user's startup script is broken or waits for terminal input.
        let mut directories = Vec::new();
        if let Some(home) = &home {
            directories.extend(
                [".local/bin", ".cargo/bin", ".npm-global/bin", ".volta/bin"]
                    .map(|suffix| home.join(suffix)),
            );
            directories.extend(nvm_directories(&home.join(".nvm")));
        }
        directories.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]);
        directories.extend(env::split_paths(&inherited));
        let fallback = env::join_paths(directories)?;
        if let Some(cli) = find_in_path(&fallback, name, "standard install directories") {
            return Ok(cli);
        }
        let detail = match shell_result {
            Ok(_) => format!("shell PATH 中也没有可执行的 {name}"),
            Err(error) => format!("{error:#}"),
        };
        bail!(
            "找不到工具 {name}：已检查应用 PATH、用户 shell {} 和常用安装目录；{detail}",
            shell.display()
        )
    }

    fn executable(path: &Path) -> bool {
        path.metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }

    fn find_in_path(path: &OsStr, name: &str, source: &'static str) -> Option<ToolExecutable> {
        // Never discover an executable in the opened project's cwd via an
        // empty or relative PATH entry. Keep symlink paths: npm's launcher and
        // its sibling node live in the version manager's bin directory.
        let directories: Vec<_> = env::split_paths(path)
            .filter(|dir| dir.is_absolute())
            .collect();
        let program = directories
            .iter()
            .map(|dir| dir.join(name))
            .find(|path| executable(path))?;
        let mut child_directories = vec![program.parent()?.to_path_buf()];
        child_directories.extend(directories);
        Some(ToolExecutable {
            program,
            path: Some(env::join_paths(child_directories).ok()?),
            source,
        })
    }

    fn nvm_directories(root: &Path) -> Vec<PathBuf> {
        let default = std::fs::read_to_string(root.join("alias/default")).unwrap_or_default();
        let default = default.trim().trim_start_matches('v');
        let mut versions: Vec<_> = std::fs::read_dir(root.join("versions/node"))
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let version = name.strip_prefix('v')?;
                let parts = version
                    .split('.')
                    .map(str::parse::<u64>)
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .ok()?;
                if parts.len() != 3 {
                    return None;
                }
                let is_default = !default.is_empty()
                    && (version == default || version.starts_with(&format!("{default}.")));
                Some((is_default, parts, entry.path().join("bin")))
            })
            .collect();
        versions.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        versions.into_iter().map(|(_, _, path)| path).collect()
    }

    fn parse_shell_path(output: &[u8]) -> Result<OsString> {
        let start = output
            .windows(PATH_MARKER.len())
            .rposition(|window| window == PATH_MARKER)
            .context("用户 shell 未返回工具 PATH")?
            + PATH_MARKER.len();
        let end = output[start..]
            .iter()
            .position(|byte| *byte == 0)
            .context("用户 shell 返回的工具 PATH 不完整")?;
        Ok(OsString::from_vec(output[start..start + end].to_vec()))
    }

    fn shell_path(
        shell: &Path,
        home: Option<&Path>,
        inherited: &OsStr,
        timeout: Duration,
    ) -> Result<OsString> {
        let mut command = Command::new(shell);
        command
            .args(["-ilc", PATH_QUERY])
            .env("PATH", inherited)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        if let Some(home) = home {
            command.current_dir(home);
        }
        let mut child = command.spawn().context("无法读取用户 shell 的工具 PATH")?;
        let result = (|| {
            let mut stdout = child.stdout.take().context("用户 shell stdout 不可用")?;
            use std::os::fd::AsRawFd;
            // Poll both stdout and process status: startup files can hang,
            // flood stdout, or leave descendants holding the pipe open.
            let fd = stdout.as_raw_fd();
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(std::io::Error::last_os_error()).context("无法读取用户 shell stdout");
            }
            let deadline = Instant::now() + timeout;
            let mut output = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                match stdout.read(&mut buffer) {
                    Ok(0) => {
                        if let Some(status) = child.try_wait()? {
                            if !status.success() {
                                bail!("用户 shell 退出失败：{status}");
                            }
                            return parse_shell_path(&output);
                        }
                    }
                    Ok(count) => {
                        output.extend_from_slice(&buffer[..count]);
                        if output.len() > OUTPUT_LIMIT {
                            bail!("用户 shell 启动输出过多，无法读取工具 PATH");
                        }
                        if Instant::now() < deadline {
                            continue;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error).context("无法读取用户 shell stdout"),
                }
                if Instant::now() >= deadline {
                    bail!(
                        "读取用户 shell 工具 PATH 超时（{} ms）",
                        timeout.as_millis()
                    );
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        })();
        if result.is_err() {
            // Also reap descendants such as a startup-script `sleep`; no
            // detached discovery worker should survive a failed attempt.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
        }
        let _ = child.wait();
        result
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::atomic::{AtomicU64, Ordering};

        struct Fixture(PathBuf);

        impl Fixture {
            fn new() -> Self {
                static NEXT: AtomicU64 = AtomicU64::new(0);
                let path = env::temp_dir().join(format!(
                    "echora-cli-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&path).unwrap();
                Self(path)
            }

            fn script(&self, name: &str, text: &str) -> PathBuf {
                let path = self.0.join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, text).unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
                path
            }

            fn shell(&self) -> PathBuf {
                // A version manager can print startup banners. Only the
                // delimited PATH should be read, never the rest of its output.
                self.script("shell", "#!/bin/sh\nprintf 'startup banner\\n'\nprintf '\\000ECHORA_TOOL_PATH\\000%s/node version/bin:/usr/bin:/bin\\000' \"$PWD\"\n")
            }
        }

        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        #[test]
        fn finder_path_discovers_version_manager_and_runs_shebang_with_sibling_runtime() {
            let fixture = Fixture::new();
            let shell = fixture.shell();
            let codex = fixture.script(
                "node version/bin/codex",
                "#!/usr/bin/env echora-fixture-node\n",
            );
            fixture.script(
                "node version/bin/echora-fixture-node",
                "#!/bin/sh\nprintf 'fixture runtime: %s\\n' \"$2\"\n",
            );
            let inherited = OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin");
            let cli = resolve_with("codex", inherited, Some(&fixture.0), &shell).unwrap();
            assert_eq!(
                cli.program.canonicalize().unwrap(),
                codex.canonicalize().unwrap()
            );
            assert_eq!(cli.source, "login shell PATH");
            let output = cli.command().arg("--version").output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"fixture runtime: --version\n");
        }

        #[test]
        fn finder_path_discovers_github_cli_in_a_shell_path_with_spaces() {
            let fixture = Fixture::new();
            let shell = fixture.shell();
            let gh = fixture.script("node version/bin/gh", "#!/bin/sh\nprintf 'gh fixture\n'\n");
            let cli = resolve_with(
                "gh",
                OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin"),
                Some(&fixture.0),
                &shell,
            )
            .unwrap();
            assert_eq!(
                cli.program.canonicalize().unwrap(),
                gh.canonicalize().unwrap()
            );
            assert_eq!(cli.source, "login shell PATH");
            let output = cli.command().arg("--version").output().unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"gh fixture\n");
        }

        #[test]
        fn different_tool_names_do_not_reuse_codex_or_relative_executables() {
            let fixture = Fixture::new();
            fixture.script("bin/codex", "#!/bin/sh\n");
            let gh = fixture.script("bin/gh", "#!/bin/sh\n");
            let path =
                env::join_paths([Path::new(""), Path::new("."), &fixture.0.join("bin")]).unwrap();
            assert_eq!(find_in_path(&path, "gh", "test").unwrap().program, gh);
            assert!(find_in_path(&path, "absent-tool", "test").is_none());
            let error = resolve_with(
                "echora-definitely-absent-tool",
                OsStr::new("/usr/bin:/bin"),
                Some(&fixture.0),
                Path::new("/does/not/exist"),
            )
            .err()
            .unwrap();
            assert!(error.to_string().contains("echora-definitely-absent-tool"));
            assert!(error.to_string().contains("常用安装目录"));
        }

        #[test]
        fn inherited_cli_wins_without_executing_shell() {
            let fixture = Fixture::new();
            let codex = fixture.script("bin/codex", "#!/bin/sh\nexit 0\n");
            let cli = resolve_with(
                "codex",
                codex.parent().unwrap().as_os_str(),
                None,
                Path::new("/does/not/exist"),
            )
            .unwrap();
            assert_eq!(cli.program, codex);
            assert_eq!(cli.source, "inherited PATH");
        }

        #[test]
        fn lookup_skips_broken_links_nonexecutables_and_relative_entries() {
            let fixture = Fixture::new();
            let broken = fixture.0.join("broken");
            std::fs::create_dir(&broken).unwrap();
            std::os::unix::fs::symlink("missing", broken.join("codex")).unwrap();
            let nonexec = fixture.script("nonexec/codex", "text");
            std::fs::set_permissions(&nonexec, std::fs::Permissions::from_mode(0o644)).unwrap();
            let good = fixture.script("good/codex", "#!/bin/sh\n");
            let path = env::join_paths([
                Path::new(""),
                Path::new("."),
                &broken,
                nonexec.parent().unwrap(),
                good.parent().unwrap(),
            ])
            .unwrap();
            assert_eq!(find_in_path(&path, "codex", "test").unwrap().program, good);
        }

        #[test]
        fn standard_install_is_found_when_shell_startup_fails() {
            let fixture = Fixture::new();
            let codex = fixture.script(".local/bin/codex", "#!/bin/sh\n");
            let shell = fixture.script("shell", "#!/bin/sh\nexit 1\n");
            let cli = resolve_with("codex", OsStr::new(""), Some(&fixture.0), &shell).unwrap();
            assert_eq!(cli.program, codex);
            assert_eq!(cli.source, "standard install directories");
        }

        #[test]
        fn shell_startup_is_bounded_even_with_descendants_holding_stdout() {
            let fixture = Fixture::new();
            let shell = fixture.script("shell", "#!/bin/sh\n/bin/sleep 20 &\nexit 0\n");
            let start = Instant::now();
            let error = shell_path(
                &shell,
                Some(&fixture.0),
                OsStr::new("/usr/bin:/bin"),
                Duration::from_millis(80),
            )
            .unwrap_err();
            assert!(error.to_string().contains("超时"));
            assert!(start.elapsed() < Duration::from_secs(2));
        }

        #[test]
        fn shell_output_is_bounded_and_not_included_in_errors() {
            let fixture = Fixture::new();
            let shell = fixture.script(
                "shell",
                "#!/bin/sh\nwhile :; do printf 'PRIVATE_STARTUP_OUTPUT'; done\n",
            );
            let error = shell_path(
                &shell,
                Some(&fixture.0),
                OsStr::new("/usr/bin:/bin"),
                Duration::from_secs(2),
            )
            .unwrap_err();
            assert!(error.to_string().contains("输出过多"));
            assert!(!error.to_string().contains("PRIVATE_STARTUP_OUTPUT"));
        }

        #[test]
        fn path_framing_rejects_missing_or_truncated_values() {
            assert!(parse_shell_path(b"arbitrary shell output").is_err());
            assert!(parse_shell_path(b"\0ECHORA_TOOL_PATH\0incomplete").is_err());
            assert_eq!(
                parse_shell_path(b"banner\0ECHORA_TOOL_PATH\0/a path/bin:/bin\0after").unwrap(),
                OsString::from("/a path/bin:/bin")
            );
        }

        #[test]
        fn nvm_fallback_respects_default_and_compares_versions_numerically() {
            let fixture = Fixture::new();
            for version in ["v9.0.0", "v24.12.0", "v24.9.0"] {
                fixture.script(
                    &format!(".nvm/versions/node/{version}/bin/codex"),
                    "#!/bin/sh\n",
                );
            }
            let root = fixture.0.join(".nvm");
            assert_eq!(
                nvm_directories(&root)[0],
                root.join("versions/node/v24.12.0/bin")
            );
            std::fs::create_dir_all(root.join("alias")).unwrap();
            std::fs::write(root.join("alias/default"), "24.9.0\n").unwrap();
            assert_eq!(
                nvm_directories(&root)[0],
                root.join("versions/node/v24.9.0/bin")
            );
            let shell = fixture.script("shell", "#!/bin/sh\nexit 1\n");
            let cli = resolve_with(
                "codex",
                OsStr::new("/usr/bin:/bin"),
                Some(&fixture.0),
                &shell,
            )
            .unwrap();
            assert_eq!(cli.program, root.join("versions/node/v24.9.0/bin/codex"));
        }
    }
}
