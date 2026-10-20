//! Discover GitHub CLI for desktop launches without changing global PATH.

use anyhow::{Context as _, Result};
use std::process::Command;

pub(crate) fn command() -> Result<Command> {
    crate::tool_executable::ToolExecutable::resolve("gh")
        .map(|tool| tool.command())
        .context(crate::i18n::text(
            "找不到 GitHub CLI（gh）。请安装 gh，在终端运行 gh --version 和 gh auth login，然后点击重试；已检查应用 PATH、用户登录 shell 及常用安装目录",
        ))
}
