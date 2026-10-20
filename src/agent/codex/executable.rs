//! Codex-specific entry point for desktop tool discovery.

use anyhow::{Context as _, Result};

pub(super) struct CodexExecutable;

impl CodexExecutable {
    pub(super) fn resolve() -> Result<crate::tool_executable::ToolExecutable> {
        crate::tool_executable::ToolExecutable::resolve("codex")
            .context("请安装 Codex CLI，并确认在用户 shell 中运行 codex --version 成功，然后重试")
    }
}

pub fn codex_executable_diagnostics() -> String {
    match CodexExecutable::resolve() {
        Ok(cli) => format!("{} ({})", cli.program.display(), cli.source),
        Err(error) => format!("unavailable: {error:#}"),
    }
}
