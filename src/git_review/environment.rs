//! Read-only metadata for the Environment card; all subprocesses have deadlines.
use super::{git, process, workspace};
use anyhow::Result;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Clone, Debug, Default)]
pub struct Environment {
    pub branch: Option<String>,
    pub branches: Vec<String>,
    pub repository: Option<String>,
    pub changed: usize,
    pub additions: usize,
    pub deletions: usize,
    pub ahead: usize,
    pub upstream: bool,
    pub can_create_pr: bool,
    pub worktree: bool,
    pub servers: Vec<u16>,
}

pub fn read(cwd: &Path) -> Result<Environment> {
    let branches = workspace::branches(cwd)?;
    let changed = branches.dirty_files;
    let branch = branches.current.branch().map(str::to_owned);
    let repository = git(cwd, &["remote", "get-url", "origin"])
        .ok()
        .and_then(|url| github_repository(url.trim()));
    let upstream = git(cwd, &["rev-parse", "--abbrev-ref", "@{upstream}"]).is_ok();
    let ahead = git(cwd, &["rev-list", "--count", "@{upstream}..HEAD"])
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    let stat = git(cwd, &["diff", "--numstat", "HEAD", "--"])
        .or_else(|_| git(cwd, &["diff", "--cached", "--numstat", "--"]))
        .unwrap_or_default();
    let (mut additions, deletions) = numstat(&stat);
    for path in git(cwd, &["ls-files", "--others", "--exclude-standard", "-z"])
        .unwrap_or_default()
        .split('\0')
        .filter(|p| !p.is_empty())
    {
        if let Ok(path) = super::safe_path(path)
            && let Ok(metadata) = std::fs::symlink_metadata(cwd.join(path))
            && metadata.is_file()
            && metadata.len() <= 16 * 1024 * 1024
            && let Ok(bytes) = std::fs::read(cwd.join(path))
            && !bytes.contains(&0)
        {
            additions += bytes.iter().filter(|b| **b == b'\n').count()
                + usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
        }
    }
    let git_dir = git(cwd, &["rev-parse", "--absolute-git-dir"])?;
    let common = git(
        cwd,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let base = branches.default_remote.as_deref().unwrap_or("main");
    let base = base.strip_prefix("origin/").unwrap_or(base);
    let can_create_pr = repository.is_some() && branch.as_deref().is_some_and(|name| name != base);
    Ok(Environment {
        branch,
        branches: branches.names,
        repository,
        changed,
        additions,
        deletions,
        ahead,
        upstream,
        can_create_pr,
        worktree: git_dir.trim() != common.trim(),
        servers: servers(cwd),
    })
}

fn github_repository(remote: &str) -> Option<String> {
    let github = remote.starts_with("git@github.com:")
        || url::Url::parse(remote)
            .ok()
            .is_some_and(|url| url.host_str() == Some("github.com"));
    github.then(|| super::origin_owner_repo(remote)).flatten()
}

fn numstat(value: &str) -> (usize, usize) {
    value.lines().fold((0, 0), |(a, d), line| {
        let mut fields = line.split('\t');
        (
            a + fields
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0),
            d + fields
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0),
        )
    })
}

/// Only listeners whose process cwd is inside this checkout belong to this card.
fn servers(cwd: &Path) -> Vec<u16> {
    let Ok(cwd) = cwd.canonicalize() else {
        return Vec::new();
    };
    let Ok(output) = process::run(
        Command::new("lsof").args(["-nP", "-a", "-d", "cwd", "-Fpn"]),
        None,
        Duration::from_secs(3),
    ) else {
        return Vec::new();
    };
    let mut pid = None;
    let mut pids = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse::<u32>().ok();
        }
        if let Some(path) = line.strip_prefix('n')
            && PathBuf::from(path).starts_with(&cwd)
            && let Some(pid) = pid
        {
            pids.push(pid.to_string());
        }
    }
    if pids.is_empty() {
        return Vec::new();
    }
    let Ok(output) = process::run(
        Command::new("lsof").args([
            "-nP",
            "-a",
            "-p",
            &pids.join(","),
            "-iTCP",
            "-sTCP:LISTEN",
            "-Fn",
        ]),
        None,
        Duration::from_secs(3),
    ) else {
        return Vec::new();
    };
    parse_ports(&String::from_utf8_lossy(&output.stdout))
}

fn parse_ports(value: &str) -> Vec<u16> {
    value
        .lines()
        .filter_map(|line| line.strip_prefix('n')?.rsplit_once(':')?.1.parse().ok())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn pull(cwd: &Path) -> Result<()> {
    git(cwd, &["pull", "--ff-only"])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_stats_and_ipv6_listeners_are_not_misread() {
        assert_eq!(github_repository("/tmp/fixture/remote.git"), None);
        assert_eq!(github_repository("https://gitlab.com/owner/repo.git"), None);
        assert_eq!(
            github_repository("git@github.com:owner/repo.git").as_deref(),
            Some("owner/repo")
        );
        assert_eq!(numstat("4\t2\ta\n-\t-\tb\n1\t0\tc\n"), (5, 2));
        assert_eq!(
            parse_ports("p12\nn*:3000\nn[::1]:8080\nn127.0.0.1:3000\nf2\n"),
            vec![3000, 8080]
        );
    }
    #[test]
    fn environment_metadata_tracks_branch_dirty_state_and_worktrees() {
        let root = std::env::temp_dir().join(format!(
            "echora-environment-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let run = |args: &[&str]| {
            let o = Command::new("git")
                .current_dir(&root)
                .args(args)
                .output()
                .unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        };
        run(&["init", "-b", "main"]);
        run(&["config", "user.email", "test@example.invalid"]);
        run(&["config", "user.name", "Environment test"]);
        run(&[
            "remote",
            "add",
            "origin",
            "git@github.com:example/project.git",
        ]);
        std::fs::write(root.join("file"), "original\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "initial"]);
        let initial = read(&root).unwrap();
        assert_eq!(initial.branch.as_deref(), Some("main"));
        assert_eq!(initial.repository.as_deref(), Some("example/project"));
        assert_eq!(initial.changed, 0);
        assert!(!initial.worktree);
        assert!(!initial.can_create_pr);
        std::fs::write(root.join("file"), "changed\nnew\n").unwrap();
        run(&["switch", "-c", "codex/feature"]);
        let dirty = read(&root).unwrap();
        assert_eq!((dirty.changed, dirty.additions, dirty.deletions), (1, 2, 1));
        assert!(dirty.can_create_pr);
        let tree = root.join("tree");
        run(&[
            "worktree",
            "add",
            "--detach",
            tree.to_str().unwrap(),
            "HEAD",
        ]);
        let detached = read(&tree).unwrap();
        assert!(detached.worktree);
        assert!(detached.branch.is_none());
        assert!(!detached.can_create_pr);
        std::fs::remove_dir_all(root).unwrap();
    }
}
