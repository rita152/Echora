//! English translations for app-owned source labels. Never apply to user content.

pub(super) fn english(source: &str) -> Option<&'static str> {
    Some(match source {
        "、" => ", ",
        "？" => "?",
        " · 写入结果未知，请重新读取并核对。" => {
            " · Write result unknown. Reload and verify."
        }
        " · 被覆盖" => " · Overridden",
        " · 计划" => " · Plan",
        " 和 " => " and ",
        " 文件夹" => " folder",
        " 模型、推理强度、Plan 推理强度、服务等级和个性默认值仅用于新会话；当前轮次权限不随配置保存改变。" => {
            " Model, reasoning effort, Plan effort, service tier, and personality defaults apply only to new conversations. Saving configuration does not change current turn permissions."
        }
        " 的内容" => "",
        " 的内容吗？" => "?",
        " 集成" => " integrations",
        "%m月%d日 %H:%M" => "%b %d, %H:%M",
        "1 完成" => "1 completed",
        "Codex turn 失败" => "Codex turn failed",
        "Codex 事件流意外断开" => "The Codex event stream disconnected unexpectedly",
        "Codex 将能够在未经您许可的情况下，在这台计算机上的任何位置运行命令、使用互联网，以及创建和编辑文件。这包括但不限于：" => {
            "Codex will be able to run commands, use the internet, and create or edit files anywhere on this computer without asking for permission. This includes:"
        }
        "Codex 未请求额外文件或网络权限，是否继续？" => {
            "Codex requested no additional file or network permissions. Continue?"
        }
        "Codex 未返回可用模型" => "Codex returned no available models",
        "Codex 模型目录连接在返回结果前关闭" => {
            "The Codex model catalog connection closed before returning a response"
        }
        "Codex 警告" => "Codex warning",
        "Codex 配置警告" => "Codex configuration warning",
        "Codex 错误" => "Codex error",
        "GPUI 正在加载" => "GPUI is loading",
        "Git 差异缺少文件名" => "Git diff is missing a file name",
        "Git 输出不是 UTF-8 文本" => "Git output is not UTF-8 text",
        "Git 重命名差异缺少目标文件名" => {
            "Git rename diff is missing the target file name"
        }
        "GitHub CLI 未返回 PR 地址" => "GitHub CLI did not return a PR URL",
        "GitHub 查询失败" => "GitHub query failed",
        "MCP 列表连接已关闭" => "The MCP list connection closed",
        "MCP 服务器重新加载失败" => "MCP server reload failed",
        "MCP 请求失败" => "MCP request failed",
        "Markdown 文件预览" => "Markdown preview",
        "OAuth 提供方返回 access_denied" => "The OAuth provider returned access_denied",
        "PR 标题" => "PR title",
        "PR 说明" => "PR description",
        "Plan 推理强度" => "Plan reasoning effort",
        "UI 偏好写入锁已损坏" => "UI preference write lock poisoned",
        "UI 偏好路径缺少父目录" => "UI preference path has no parent directory",
        "WorkspaceStore 状态锁已损坏" => "WorkspaceStore state lock poisoned",
        "`config/mcpServer/reload` 等待响应超时" => {
            "Timed out waiting for a response to `config/mcpServer/reload`"
        }
        "git URL 或本地路径" => "Git URL or local path",
        "marketplace 来源" => "Marketplace source",
        "pinned section 锁已损坏" => "Pinned section lock poisoned",
        "server request 清理在 Composer 中没有对应 pending request" => {
            "Server request cleanup has no matching pending request in the composer"
        }
        "server request 清理标识与 Composer pending request 不一致" => {
            "Server request cleanup ID does not match the composer pending request"
        }
        "serverRequest/resolved 在 Composer 中没有对应 pending request" => {
            "serverRequest/resolved has no matching pending request in the composer"
        }
        "serverRequest/resolved 标识与 Composer pending request 不一致" => {
            "serverRequest/resolved ID does not match the composer pending request"
        }
        "⌕  筛选文件…" => "⌕  Filter files…",
        "☐ 包含未暂存的更改" => "☐ Include unstaged changes",
        "☐ 提交并推送本地更改" => "☐ Commit and push local changes",
        "☑ 包含未暂存的更改" => "☑ Include unstaged changes",
        "☑ 提交并推送本地更改" => "☑ Commit and push local changes",
        "✓ 已查看" => "✓ Viewed",
        "上一轮" => "Last Turn",
        "上一题" => "Previous question",
        "上下文已自动压缩" => "Context automatically compacted",
        "上移" => "Move up",
        "下一步" => "Next",
        "下一题" => "Next question",
        "下移" => "Move down",
        "下载图片" => "Download image",
        "下载计划" => "Download plan",
        "不允许网页搜索" => "Do not allow web search",
        "不再询问" => "Do not ask again",
        "不加载完整文件" => "Don't load full files",
        "不受信任" => "Untrusted",
        "不可用" => "Unavailable",
        "不支持非 UTF-8 文件名" => "Non-UTF-8 file names are not supported",
        "与 coding agent 的连接已断开" => "Disconnected from the coding agent",
        "严重" => "Critical",
        "个人" => "Personal",
        "个性" => "Personality",
        "个性化" => "Personalization",
        "个性默认值" => "Default personality",
        "中" => "Medium",
        "临时聊天连接已结束" => "The temporary chat connection ended",
        "主题" => "Theme",
        "了解更多" => "Learn more",
        "二进制文件内容已更改" => "Binary file changed",
        "互联网和已连接的应用" => "Internet and connected apps",
        "互联网访问" => "Internet access",
        "仅可使用服务器允许的配置；新聊天继承此设置" => {
            "Only server-allowed profiles are available; new chats inherit this setting"
        }
        "仅对检测到的风险操作请求批准" => "Ask only for actions identified as risky",
        "仅支持普通文件" => "Only regular files are supported",
        "仅用于新会话" => "New chats only",
        "仅用于新会话的 Plan 默认值" => "Plan defaults for new chats only",
        "仅用于新会话；合法值由模型目录提供" => {
            "New chats only; valid values come from the model catalog"
        }
        "仅用于新会话；已有会话可在模型菜单中修改" => {
            "Applies to new chats; use the model menu to change existing chats"
        }
        "从不" => "Never",
        "从使用过 MCP 工具或网页搜索的聊天生成记忆" => {
            "Create memories from chats that used MCP tools or web search"
        }
        "从工作区目录树中选择文件" => "Choose a file from the workspace tree",
        "从此处分叉" => "Fork from here",
        "会话" => "Session",
        "会话历史" => "Conversation history",
        "会话已不可用" => "Conversation is no longer available",
        "会话进行中，无法压缩上下文" => {
            "Cannot compact context while the conversation is running"
        }
        "低" => "Low",
        "你" => "You",
        "你需要重新登录才能继续使用 ChatGPT" => {
            "You will need to sign in again to continue using ChatGPT"
        }
        "使用 ChatGPT 登录" => "Sign in with ChatGPT",
        "使用 OpenAI 维护的搜索索引" => "Use the search index maintained by OpenAI",
        "使用 config.toml 中定义的权限" => "Use permissions defined in config.toml",
        "使用模型目录提供的强度" => "Use efforts provided by the model catalog",
        "侧边聊天" => "Side chat",
        "侧边聊天是临时聊天，关闭应用后会消失。" => {
            "Side chats are temporary and disappear when you close the app."
        }
        "侧边聊天标签页" => "Side chat tabs",
        "侧边聊天的连接已结束。你仍可查看和复制消息，或新建侧边聊天继续。" => {
            "This side chat disconnected. You can still read and copy messages, or start a new side chat to continue."
        }
        "侧边聊天的连接已结束，请新建侧边聊天继续。" => {
            "This side chat disconnected. Start a new side chat to continue."
        }
        "侧边聊天输入框" => "Side chat input",
        "保存" => "Save",
        "保持简短且易于识别" => "Keep it short and recognizable",
        "保存中…" => "Saving…",
        "保存后会回读有效配置。配置默认值与当前线程权限分别管理；线程权限变更从下一轮起生效，复核者会同步到进行中的轮次。" => {
            "Effective configuration is read back after saving. Defaults and current thread permissions are managed separately; thread permission changes apply from the next turn, and the reviewer also switches for a running turn."
        }
        "保存连接已关闭" => "The save connection closed",
        "保存连接已关闭；写入结果未知，请重新读取后核对" => {
            "The save connection closed; the result is unknown. Reload and verify."
        }
        "保留编辑" => "Keep edits",
        "停止当前轮次" => "Stop current turn",
        "停止生成" => "Stop generating",
        "停用" => "Disable",
        "允许 ChatGPT " => "Allow ChatGPT to ",
        "允许 ChatGPT 与" => "Allow ChatGPT to connect to",
        "允许 ChatGPT 查看 " => "Allow ChatGPT to read ",
        "允许 ChatGPT 查看和编辑 " => "Allow ChatGPT to read and edit ",
        "允许 ChatGPT 编辑 " => "Allow ChatGPT to edit ",
        "允许 ChatGPT 连接互联网？" => "Allow ChatGPT to access the internet?",
        "允许一次" => "Allow once",
        "允许不受限制地访问当前网页" => "Allow unrestricted access to live web pages",
        "允许所有修改" => "Allow all changes",
        "允许此对话" => "Allow for this conversation",
        "允许类似命令" => "Allow similar commands",
        "允许访问已索引的外部网页" => "Allow access to indexed external web pages",
        "先设置远程主机。然后可在此处选择主机和文件夹。" => {
            "Set up a remote host first, then choose a host and folder here."
        }
        "全部标为已读" => "Mark all as read",
        "共享" => "Share",
        "共享请求连接在返回结果前关闭" => {
            "The sharing connection closed before returning a response"
        }
        "关闭" => "Close",
        "关闭侧边聊天" => "Close side chat",
        "关闭侧边聊天？" => "Close side chat?",
        "关闭图片预览" => "Close image preview",
        "关闭子智能体面板" => "Close subagent panel",
        "关闭审查标签页" => "Close review tab",
        "关闭对话框" => "Close dialog",
        "关闭面板信息下拉框" => "Close panel information menu",
        "其他" => "Other",
        "内置终端" => "Built-in terminal",
        "内联" => "Inline",
        "再次点击以删除" => "Click again to delete",
        "再次点击以移除" => "Click again to remove",
        "写入" => "Write",
        "写入命令输入失败" => "Could not write command input",
        "分支" => "Branch",
        "分支上暂无提交记录" => "No commits on this branch",
        "分支名称" => "Branch name",
        "分支已推送，但创建 PR 失败" => "Branch pushed, but PR creation failed",
        "切换到拆分差异视图" => "Switch to split diff",
        "切换到统一差异视图" => "Switch to unified diff",
        "切换文件差异对比" => "Toggle file diff",
        "切换文件树" => "Toggle file tree",
        "列出文件失败" => "Failed to list files",
        "创建 PR" => "Create PR",
        "创建 PR 需要已登录的 GitHub CLI（gh）" => {
            "Creating a PR requires an authenticated GitHub CLI (gh)"
        }
        "创建 Pull Request" => "Create pull request",
        "创建 Pull Request   ⌘⏎" => "Create pull request   ⌘⏎",
        "创建侧边聊天的响应通道已关闭" => {
            "The side chat creation response connection closed"
        }
        "创建置顶分区" => "Create pinned section",
        "创建草稿 PR" => "Create draft PR",
        "创建项目" => "Create project",
        "删除" => "Delete",
        "删除会话" => "Delete conversation",
        "删除聊天" => "Delete chat",
        "删除项目" => "Delete project",
        "刷新" => "Refresh",
        "刷新中…" => "Refreshing…",
        "刷新已归档聊天" => "Refresh archived chats",
        "刷新最近聊天" => "Refresh recent chats",
        "刷新置顶聊天" => "Refresh pinned chats",
        "刷新项目" => "Refresh projects",
        "排队" => "Queue",
        "加载 workspace 分页" => "Load workspace page",
        "加载会话分区" => "Load conversation sections",
        "加载完整文件" => "Load full files",
        "加载已归档聊天" => "Load archived chats",
        "加载最近聊天" => "Load recent chats",
        "加载置顶聊天" => "Load pinned chats",
        "加载项目" => "Load projects",
        "单独" => "Separate",
        "单行不能超过 64 KB，请拆分长行" => {
            "A line cannot exceed 64 KB. Split long lines."
        }
        "单行超过 64 KB，请在外部编辑器中打开" => {
            "A line exceeds 64 KB. Open it in an external editor."
        }
        "卸载" => "Uninstall",
        "卸载连接在返回结果前关闭" => {
            "The uninstall connection closed before returning a response"
        }
        "历史差异不能撤销当前文件" => "Historical diffs cannot restore current files",
        "压缩请求的响应通道提前关闭" => {
            "The compaction response connection closed early"
        }
        "发送" => "Send",
        "发送中…" => "Sending…",
        "发送失败 · 恢复输入" => "Send failed · Restore input",
        "发送消息" => "Send message",
        "取消" => "Cancel",
        "取消共享" => "Unshare",
        "取消归档" => "Unarchive",
        "取消暂存" => "Unstage",
        "取消暂存差异块" => "Unstage hunk",
        "取消登录" => "Cancel sign-in",
        "取消登录连接在返回结果前关闭" => {
            "The cancel sign-in connection closed before returning a response"
        }
        "取消置顶" => "Unpin",
        "受管配置" => "Managed configuration",
        "变更" => "Changes",
        "只读" => "Read only",
        "可不受限制地访问互联网和你电脑上的任何文件" => {
            "Unrestricted access to the internet and files on your computer"
        }
        "名称" => "Name",
        "否，并告诉 ChatGPT 应该如何做得不同" => {
            "No, and tell ChatGPT what to do differently"
        }
        "启动参数" => "Launch arguments",
        "启用" => "Enable",
        "启用富文本预览" => "Enable rich preview",
        "启用文字差异" => "Enable word diffs",
        "启用自动换行" => "Enable word wrap",
        "命令" => "Command",
        "命令审批 responder 不存在" => "The command approval responder is unavailable",
        "命令审批响应连接不存在" => {
            "The command approval response connection is unavailable"
        }
        "命令输入不可用" => "Command input unavailable",
        "命令输出不可用" => "Command output unavailable",
        "命令错误输出不可用" => "Command error output unavailable",
        "命令预览，只读" => "Command preview, read only",
        "和" => " and ",
        "回退会话历史" => "Revert conversation history",
        "回退请求的响应通道提前关闭，历史状态未知" => {
            "The revert response connection closed early; history state is unknown"
        }
        "因多次被驳回，自动审查已停止本轮操作。请添加更多上下文或选择其他权限模式以继续。" => {
            "Automatic review stopped this turn after repeated denials. Add more context or choose another permission mode to continue."
        }
        "图像生成失败，请重试。" => "Image generation failed. Please try again.",
        "图像生成额度已用完" => "Image generation limit reached",
        "图片不可用" => "Image unavailable",
        "图片预览" => "Image preview",
        "圆形" => "Circle",
        "在 ChatGPT 运行时将后续消息加入队列，或引导当前运行。按 ⌘⏎ 可对单条消息执行相反操作" => {
            "Queue follow-ups while ChatGPT runs or steer the current run. Press ⌘⏎ to do the opposite for one message"
        }
        "在 Finder 中显示" => "Reveal in Finder",
        "在你的电脑上编辑、运行和测试文件" => {
            "Edit, run, and test files on your computer"
        }
        "在侧边面板中打开计划" => "Open plan in side panel",
        "在浏览器中打开 PR" => "Open PR in browser",
        "在访达中显示" => "Reveal in Finder",
        "在默认应用中打开" => "Open in default app",
        "复制" => "Copy",
        "复制 git apply 命令" => "Copy git apply command",
        "复制当前内容" => "Copy current contents",
        "复制消息" => "Copy message",
        "复制绝对路径" => "Copy absolute path",
        "复制计划" => "Copy plan",
        "复制路径" => "Copy path",
        "复制配置来源与诊断" => "Copy configuration sources and diagnostics",
        "复制钩子反馈" => "Copy hook feedback",
        "外观" => "Appearance",
        "失败" => "Failed",
        "套餐" => "Plan",
        "始终允许" => "Always allow",
        "始终允许此网站" => "Always allow this site",
        "始终拒绝此网站" => "Always deny this site",
        "子智能体" => "Subagents",
        "安全检查中" => "Running safety checks",
        "安装" => "Install",
        "安装包默认值" => "Package defaults",
        "安装连接在返回结果前关闭" => {
            "The install connection closed before returning a response"
        }
        "完全访问" => "Full access",
        "完全访问权限" => "Full access",
        "完成思考" => "Finished thinking",
        "完整访问权限" => "Full access",
        "实时" => "Live",
        "审批选项" => "Approval options",
        "审查" => "Review",
        "审查命令超时，已停止相关进程，请刷新后重试" => {
            "Review command timed out and related processes were stopped. Refresh and try again."
        }
        "审查已更改的文件" => "Review changed files",
        "审查文件" => "Review files",
        "审查文件更改" => "Review file changes",
        "审查结果呈现方式" => "Review delivery",
        "审核" => "Review",
        "对全部取消暂存" => "Unstage all",
        "对文件取消暂存" => "Unstage file",
        "将删除这个插件的共享记录。" => "Delete this plugin’s sharing record.",
        "将把这个插件发布到账号的插件服务，并保留服务端返回的共享链接。" => {
            "Publish this plugin to your account’s plugin service and keep the sharing link returned by the server."
        }
        "将重新拉取全部插件目录。" => "Refresh all plugin catalogs.",
        "尚无文件更改" => "No file changes yet",
        "尚未登录" => "Not signed in",
        "尚未读取配置" => "Configuration not loaded",
        "尚未连接任何应用" => "No apps connected yet",
        "尽可能在当前聊天中启动 /review，或启动单独的审查聊天" => {
            "Start /review in the current chat when possible or launch a separate review chat"
        }
        "展开" => "Expand",
        "展开上下文" => "Expand context",
        "展开全部差异" => "Expand all diffs",
        "展开命令预览" => "Expand command preview",
        "展开显示" => "Show more",
        "工作区写入" => "Workspace write",
        "工作区目录树" => "Workspace file tree",
        "工作过程" => "Work details",
        "工具" => "Tools",
        "工具发现" => "Tool discovery",
        "差异过大，请选择较小的审查范围" => {
            "Diff too large. Choose a smaller review scope."
        }
        "已" => "Completed",
        "已中断" => "Interrupted",
        "已停用" => "Disabled",
        "已允许" => "Allowed",
        "已共享插件" => "Plugin shared",
        "已关闭" => "Closed",
        "已写入文件，但有效配置回读失败。草稿已保留，请重新读取后核对。" => {
            "File written, but effective configuration could not be read back. Your draft was kept. Reload and verify."
        }
        "已列出文件" => "Listed files",
        "已创建" => "Created",
        "已创建 Pull Request" => "Pull request created",
        "已删除" => "Deleted",
        "已卸载插件" => "Uninstalled plugin",
        "已取消" => "Canceled",
        "已取消共享" => "Sharing revoked",
        "已取消登录" => "Sign-in canceled",
        "已复制" => "Copied",
        "已复制 git apply 命令" => "Copied git apply command",
        "已安排" => "Scheduled",
        "已安装插件" => "Installed plugin",
        "已完成" => "Completed",
        "已工作" => "Worked",
        "已开启" => "On",
        "已归档" => "Archived",
        "已归档聊天" => "Archived chats",
        "已打开网页" => "Opened web page",
        "已打开链接。请在浏览器中完成操作后再选择“继续”。" => {
            "Link opened. Complete the action in your browser, then choose Continue."
        }
        "已拒绝" => "Denied",
        "已接受" => "Accepted",
        "已接受 · 恢复副本" => "Accepted · Restore copy",
        "已接受 · 确认异常" => "Accepted · Confirmation issue",
        "已提交" => "Committed",
        "已搜索文件" => "Searched files",
        "已搜索网页" => "Searched the web",
        "已暂存" => "Staged",
        "已更新" => "Updated",
        "已更新共享范围" => "Sharing scope updated",
        "已更新插件目录" => "Plugin catalog updated",
        "已更新更改" => "Changes updated",
        "已查找网页" => "Searched web page",
        "已检查的图像" => "Inspected images",
        "已添加 marketplace" => "Marketplace added",
        "已登录" => "Signed in",
        "已登录 ChatGPT 账户" => "Signed in to ChatGPT",
        "已禁用" => "Disabled",
        "已移除 marketplace" => "Marketplace removed",
        "已等待" => "Waited",
        "已索引" => "Indexed",
        "已缓存" => "Cached",
        "已编辑" => "Edited",
        "已编辑 0 个文件" => "Edited 0 files",
        "已编辑的文件" => "Edited files",
        "已编辑的文件，展开文件更改" => "Edited files, expand changes",
        "已编辑的文件，折叠文件更改" => "Edited files, collapse changes",
        "已请求登录，正在等待服务端返回授权信息。" => {
            "Sign-in requested. Waiting for authorization details from the server."
        }
        "已读取文件" => "Read files",
        "已读取文件运行了命令" => "Read files, ran commands",
        "已读取最新配置，草稿已保留。请核对来源与有效值，然后确认草稿。" => {
            "Latest configuration loaded and draft kept. Verify sources and effective values, then confirm the draft."
        }
        "已连接" => "Connected",
        "已配置令牌" => "Token configured",
        "已配置的钩子将显示在此处" => "Configured hooks will appear here",
        "已重新加载 MCP 服务器" => "MCP servers reloaded",
        "帮我批准" => "Auto review",
        "常规" => "General",
        "应如何批准 ChatGPT 操作？" => "How should ChatGPT actions be approved?",
        "应用" => "Apps",
        "应用 UI 语言" => "App interface language",
        "应用到草稿" => "Apply to draft",
        "应用目录" => "App catalog",
        "应用目录已在后端更新" => "App catalog updated on the server",
        "应用目录请求失败" => "App catalog request failed",
        "应用请求连接在返回结果前关闭" => {
            "The app request connection closed before returning a response"
        }
        "建立连接？" => "?",
        "开始工作" => "Started working",
        "归档" => "Archive",
        "归档会话" => "Archive conversation",
        "归档聊天" => "Archive chat",
        "当前 Codex turn 没有可用的中断连接" => {
            "No interrupt connection is available for the current Codex turn"
        }
        "当前 coding agent 不支持 MCP 登录" => {
            "This coding agent does not support MCP sign-in"
        }
        "当前 coding agent 不支持 MCP 管理" => {
            "This coding agent does not support MCP management"
        }
        "当前 coding agent 不支持 marketplace 管理" => {
            "This coding agent does not support marketplace management"
        }
        "当前 coding agent 不支持共享插件" => {
            "This coding agent does not support plugin sharing"
        }
        "当前 coding agent 不支持卸载插件" => {
            "This coding agent does not support uninstalling plugins"
        }
        "当前 coding agent 不支持安装插件" => {
            "This coding agent does not support installing plugins"
        }
        "当前 coding agent 不支持应用目录" => {
            "This coding agent does not support app catalogs"
        }
        "当前 coding agent 不支持技能管理" => {
            "This coding agent does not support skill management"
        }
        "当前 coding agent 不支持插件管理" => {
            "This coding agent does not support plugin management"
        }
        "当前 coding agent 不支持运行中追加输入。" => {
            "This coding agent does not support adding input while running."
        }
        "当前会话没有可搜索的工作区目录" => {
            "This conversation has no workspace directory to search"
        }
        "当前后端不支持配置" => "This backend does not support configuration",
        "当前方案不可用" => "Current plan unavailable",
        "当前未保存的编辑将被磁盘内容替换。" => {
            "Unsaved edits will be replaced by the contents on disk."
        }
        "当前没有可压缩的会话" => "No conversation to compact",
        "当前没有可编辑的会话" => "No conversation to edit",
        "当前轮次尚未就绪。输入已保留，请稍后发送。" => {
            "The current turn is not ready. Your input was kept. Send it again shortly."
        }
        "当前轮次已结束。输入已保留，请手动发送。" => {
            "The current turn ended. Your input was kept. Send it manually."
        }
        "当前轮次正在启动，尚未取得可用轮次标识。输入已保留，请稍后发送。" => {
            "The current turn is starting and has no usable ID yet. Your input was kept. Send it again shortly."
        }
        "待审批" => "Awaiting approval",
        "待开始" => "Pending",
        "忽略" => "Dismiss",
        "成功" => "Succeeded",
        "我们要构建什么？" => "What should we build?",
        "所有项目" => "All projects",
        "所需应用不可用" => "Required app unavailable",
        "手动上下文压缩" => "Manual context compaction",
        "打开位置" => "Open in",
        "打开侧边聊天" => "Open side chat",
        "打开侧边面板标签页" => "Open side panel tab",
        "打开授权地址" => "Open authorization URL",
        "打开文件" => "Open file",
        "打开浏览器" => "Open browser",
        "打开计划" => "Open plan",
        "打开链接" => "Open link",
        "打开面板信息下拉框" => "Open panel information menu",
        "打开面板选择器" => "Open panel selector",
        "批准方式" => "Approval method",
        "批准策略" => "Approval policy",
        "找不到差异块" => "Diff hunk not found",
        "找不到文件" => "File not found",
        "找不到该 Pull Request" => "Pull request not found",
        "技能" => "Skills",
        "技能列表连接已关闭" => "The skills list connection closed",
        "技能请求失败" => "Skills request failed",
        "折叠" => "Collapse",
        "折叠全部差异" => "Collapse all diffs",
        "拉取请求" => "Pull requests",
        "拒绝" => "Deny",
        "拒绝并停止" => "Deny and stop",
        "拒绝文件修改" => "Reject file changes",
        "拒绝此操作并停止当前轮次" => "Deny this action and stop the current turn",
        "拒绝此操作，继续当前轮次" => "Deny this action and continue the current turn",
        "按请求" => "On request",
        "授权已完成，服务器状态正在刷新。" => {
            "Authorization completed. Refreshing server status."
        }
        "推理强度" => "Reasoning effort",
        "推理摘要" => "Reasoning summary",
        "推荐" => "Recommended",
        "推送" => "Push",
        "推送分支失败；已完成的本地提交会保留" => {
            "Branch push failed; completed local commits were kept"
        }
        "推送完成" => "Push completed",
        "提交" => "Commit",
        "提交信息" => "Commit message",
        "提交信息（留空将自动生成）…" => "Commit message (leave blank to generate)…",
        "提交失败" => "Submission failed",
        "提交完成" => "Commit completed",
        "提交并推送" => "Commit and push",
        "提交或推送" => "Commit or push",
        "提交未被接受。输入快照已保留。" => {
            "Submission was not accepted. A copy of your input was kept."
        }
        "插件" => "Plugins",
        "插件共享" => "Plugin sharing",
        "插件技能" => "Plugin skills",
        "插件搜索" => "Plugin search",
        "插件核对" => "Plugin check",
        "插件目录" => "Plugin catalog",
        "插件目录中没有任何条目" => "The plugin catalog is empty",
        "插件目录已在后端更新" => "Plugin catalog updated on the server",
        "插件请求失败" => "Plugin request failed",
        "插件请求连接在返回结果前关闭" => {
            "The plugin request connection closed before returning a response"
        }
        "搜索插件" => "Search plugins",
        "搜索文件失败" => "File search failed",
        "搜索聊天" => "Search chats",
        "搜索设置…" => "Search settings…",
        "撤销" => "Undo",
        "撤销更改…" => "Discard changes…",
        "操作结果未知；请重新读取目录确认当前状态" => {
            "Operation result unknown. Reload the catalog to confirm the current state."
        }
        "操作超时，结果未确认；请重新读取后再试" => {
            "Operation timed out; result unconfirmed. Reload before trying again."
        }
        "收起" => "Collapse",
        "收起上下文" => "Collapse context",
        "收起命令预览" => "Collapse command preview",
        "收起文件列表" => "Collapse file list",
        "放大图片" => "Zoom in",
        "放弃修改" => "Discard changes",
        "放弃更改并关闭" => "Discard changes and close",
        "放弃未保存的编辑并关闭" => "Discard unsaved edits and close",
        "文件" => "Files",
        "文件不在当前审查中" => "File is not in the current review",
        "文件为只读，无法保存" => "File is read only and cannot be saved",
        "文件仍有未保存的编辑" => "Files have unsaved edits",
        "文件内容不是有效的 base64" => "File contents are not valid base64",
        "文件内容，可直接编辑并自动保存" => "File contents, editable with autosave",
        "文件和文件夹" => "Files and folders",
        "文件审批 responder 不存在" => "The file approval responder is unavailable",
        "文件已在其他位置更改，请刷新差异后重试。" => {
            "The file changed elsewhere. Refresh the diff and try again."
        }
        "文件已被其他程序修改。请复制当前编辑内容后重新加载，以免覆盖外部更改。" => {
            "Another application changed this file. Copy your edits and reload to avoid overwriting external changes."
        }
        "文件已重命名，内容未更改" => "File renamed with no content changes",
        "文件搜索" => "File search",
        "文件正在更改，请稍后刷新。" => "File is changing. Refresh shortly.",
        "文件版本（写入后发生变化或配置层不可见）" => {
            "File version (changed after writing or configuration layer not visible)"
        }
        "文件状态已变化" => "File state changed",
        "文件超过 2 MB，请在外部编辑器中打开" => {
            "File exceeds 2 MB. Open it in an external editor."
        }
        "新分支" => "New branch",
        "新对话" => "New chat",
        "新建终端" => "New terminal",
        "新建远程项目" => "New remote project",
        "方形" => "Square",
        "无" => "None",
        "无可写配置" => "No writable configuration",
        "无效备份路径" => "Invalid backup path",
        "无效的 Git 引用" => "Invalid Git reference",
        "无效的仓库相对路径" => "Invalid repository-relative path",
        "无效路径" => "Invalid path",
        "无法写入审批响应，请停止此轮次后重试" => {
            "Could not send approval response. Stop this turn and try again."
        }
        "无法写入文件审批响应，请停止此轮次后重试" => {
            "Could not send file approval response. Stop this turn and try again."
        }
        "无法写入权限审批响应" => "Could not send permission approval response",
        "无法写入用户输入响应" => "Could not send user input response",
        "无法加载更改" => "Could not load changes",
        "无法加载聊天历史" => "Could not load chat history",
        "无法启动审查命令" => "Could not start review command",
        "无法回复命令审批" => "Could not respond to command approval",
        "无法回复文件审批" => "Could not respond to file approval",
        "无法回复权限审批" => "Could not respond to permission approval",
        "无法回复用户输入请求" => "Could not respond to user input request",
        "无法显示生成的图像" => "Could not display generated image",
        "无法解析 Git 文件状态" => "Could not parse Git file status",
        "无法解析 gh 输出" => "Could not parse gh output",
        "无法读取命令状态" => "Could not read command status",
        "无法运行 Git" => "Could not run Git",
        "无需认证" => "No authentication required",
        "星期一" => "Monday",
        "星期三" => "Wednesday",
        "星期二" => "Tuesday",
        "星期五" => "Friday",
        "星期六" => "Saturday",
        "星期四" => "Thursday",
        "星期日" => "Sunday",
        "是否允许 ChatGPT 向正在运行的终端发送此输入？" => {
            "Allow ChatGPT to send this input to the running terminal?"
        }
        "是否允许 ChatGPT 编辑以下文件？" => "Allow ChatGPT to edit these files?",
        "是否允许 ChatGPT 运行此命令？" => "Allow ChatGPT to run this command?",
        "显示前 500 个结果" => "Showing the first 500 results",
        "显示文件" => "Show files",
        "显示更多" => "Show more",
        "显示更少" => "Show less",
        "显示空白字符" => "Show white space",
        "智能体默认设置" => "Agent defaults",
        "暂不" => "Not now",
        "暂不可用" => "Temporarily unavailable",
        "暂存全部" => "Stage all",
        "暂存差异块" => "Stage hunk",
        "暂存文件" => "Stage file",
        "暂存更改" => "Stage changes",
        "暂无已归档聊天" => "No archived chats",
        "暂无最近聊天" => "No recent chats",
        "暂无项目" => "No projects yet",
        "更多 Git 操作" => "More Git actions",
        "更快消耗使用额度" => "Uses your allowance faster",
        "更改权限" => "Change permissions",
        "更新" => "Update",
        "更新共享范围" => "Update sharing scope",
        "更新插件目录" => "Update plugin catalog",
        "更新置顶状态" => "Update pinned state",
        "更新连接在返回结果前关闭" => {
            "The update connection closed before returning a response"
        }
        "更新项目" => "Update project",
        "最小" => "Minimal",
        "最近" => "Recents",
        "最高" => "Maximum",
        "服务" => "Service",
        "服务器" => "Server",
        "服务器没有提供可用的审批选项" => {
            "The server did not provide any available approval options"
        }
        "服务端扩展字段" => "Server extension fields",
        "服务端未允许使用此权限配置" => {
            "The server does not allow this permission profile"
        }
        "服务端未返回该技能内容" => "The server did not return contents for this skill",
        "服务端权限配置" => "Server permission profile",
        "服务等级" => "Service tier",
        "未保存" => "Unsaved",
        "未完成授权，服务器保持未登录状态。" => {
            "Authorization was not completed. The server remains signed out."
        }
        "未找到" => "Not found",
        "未找到钩子" => "No hooks found",
        "未指定来源／服务端默认" => "Unspecified source / server default",
        "未提交" => "Uncommitted",
        "未暂存" => "Unstaged",
        "未知" => "Unknown",
        "未知验证" => "Unknown verification",
        "未能保存文件，保留编辑或放弃更改？" => {
            "Could not save file. Keep edits or discard changes?"
        }
        "未记录" => "Not recorded",
        "未设置" => "Not set",
        "未设置（继承）" => "Not set (inherit)",
        "未读回复" => "Unread response",
        "未连接" => "Not connected",
        "未选择" => "Not selected",
        "未配置受管限制" => "No managed restrictions configured",
        "本地" => "Local",
        "本机服务端仅允许写入用户配置；此层只读" => {
            "The local server only allows writing user configuration; this layer is read only"
        }
        "本轮操作已被自动审查终止" => "Automatic review stopped this turn",
        "本轮更改" => "Changes this turn",
        "权限" => "Permissions",
        "权限与会话默认值" => "Permissions and conversation defaults",
        "权限列表连接已关闭" => "The permission list connection closed",
        "权限变更尚未确认，输入已保留。请等待确认后发送新轮次。" => {
            "Permission changes are unconfirmed. Your input was kept. Wait for confirmation before starting a turn."
        }
        "权限审批 responder 不存在" => "The permission approval responder is unavailable",
        "权限设置连接在返回结果前关闭" => {
            "The permissions connection closed before returning a response"
        }
        "权限请求" => "Permission request",
        "权限请求失败" => "Permission request failed",
        "权限请求已取消" => "Permission request canceled",
        "权限配置" => "Permission profile",
        "权限配置尚未读取" => "Permission profiles have not loaded",
        "权限配置连接已关闭" => "The permission profiles connection closed",
        "来自插件" => "From plugin",
        "极高" => "Extra high",
        "柔和连续" => "Soft and continuous",
        "查看 " => "read ",
        "查看和编辑 " => "read and edit ",
        "查看审查评论" => "View review comments",
        "查看源代码" => "View source",
        "查看选项" => "View options",
        "标准" => "Standard",
        "标记为已查看" => "Mark as viewed",
        "标题" => "Title",
        "模型" => "Model",
        "模型不可用" => "Model unavailable",
        "正在保存技能设置…" => "Saving skill settings…",
        "正在停止当前轮次。输入已保留，请等待停止完成后发送。" => {
            "Stopping the current turn. Your input was kept. Send it after the turn stops."
        }
        "正在列出文件" => "Listing files",
        "正在创建…" => "Creating…",
        "正在创建项目…" => "Creating project…",
        "正在加载…" => "Loading…",
        "正在加载待审批的文件更改…" => "Loading file changes for approval…",
        "正在加载更改…" => "Loading changes…",
        "正在加载预览…" => "Loading preview…",
        "正在压缩上下文" => "Compacting context",
        "正在启动" => "Starting",
        "正在处理插件操作…" => "Processing plugin operation…",
        "正在工作" => "Working",
        "正在思考" => "Thinking",
        "正在打开网页" => "Opening web page",
        "正在提交…" => "Submitting…",
        "正在搜索文件" => "Searching files",
        "正在搜索网页" => "Searching the web",
        "正在查找网页" => "Finding on web page",
        "正在生成图像..." => "Generating image...",
        "正在等待" => "Waiting",
        "正在编辑文件" => "Editing files",
        "正在读取 MCP 服务器…" => "Loading MCP servers…",
        "正在读取应用目录…" => "Loading app catalog…",
        "正在读取或保存配置" => "Reading or saving configuration",
        "正在读取技能…" => "Loading skills…",
        "正在读取插件目录…" => "Loading plugin catalog…",
        "正在读取文件…" => "Reading file…",
        "正在读取新会话配置，输入已保留。" => {
            "Loading new conversation settings. Your input was kept."
        }
        "正在读取权限配置" => "Loading permission profiles",
        "正在载入子智能体" => "Loading subagent",
        "正在载入子智能体…" => "Loading subagent…",
        "此会话正由另一个 app-server 使用，请释放后重试。" => {
            "Another app-server is using this conversation. Release it and try again."
        }
        "此侧边聊天将消失且无法恢复。确定要关闭吗？" => {
            "This side chat will disappear and cannot be restored. Close it?"
        }
        "此分支已存在 Pull Request" => "A pull request already exists for this branch",
        "此后端不支持读取线程设置" => {
            "This backend does not support reading thread settings"
        }
        "此操作被视为高风险，需要明确授权" => {
            "This action is considered high risk and needs explicit authorization"
        }
        "此文件不是 UTF-8 文本，请在外部编辑器中打开" => {
            "This file is not UTF-8 text. Open it in an external editor."
        }
        "此文件为二进制文件，无法作为文本编辑" => {
            "This binary file cannot be edited as text"
        }
        "此更改需要按文件暂存或取消暂存" => {
            "This change must be staged or unstaged as a whole file"
        }
        "此目录不是 Git 仓库" => "This directory is not a Git repository",
        "此请求需要额外的安全检查，可能需要更多时间。" => {
            "This request needs additional safety checks, which may take more time."
        }
        "此轮没有文件更改。" => "No file changes in this turn.",
        "此项目中的更改将显示在此处。" => "Changes in this project will appear here.",
        "水平滚动条" => "Horizontal scrollbar",
        "沙盒设置" => "Sandbox settings",
        "没有匹配的 MCP 服务器" => "No matching MCP servers",
        "没有匹配的技能" => "No matching skills",
        "没有匹配的插件" => "No matching plugins",
        "没有匹配的文件" => "No matching files",
        "没有可写配置文件" => "No writable configuration file",
        "没有可用模型" => "No models available",
        "没有可用模型，请选择模型后重试。输入已保留。" => {
            "No model available. Select a model and try again. Your input was kept."
        }
        "没有已连接的远程目标" => "No connected remote hosts",
        "没有待保存的修改" => "No changes to save",
        "没有文本差异" => "No text differences",
        "没有配置 MCP 服务器" => "No MCP servers configured",
        "注释" => "Comment",
        "浅色" => "Light",
        "浏览器" => "Browser",
        "浏览目录" => "Browse catalog",
        "深色" => "Dark",
        "添加" => "Add",
        "添加标题…" => "Add a title…",
        "添加文件或文件夹" => "Add files or folders",
        "添加文件等内容" => "Add files and more",
        "添加连接在返回结果前关闭" => {
            "The add connection closed before returning a response"
        }
        "添加项目" => "Add project",
        "清晰稳定" => "Clear and steady",
        "清除文件筛选" => "Clear file filter",
        "源文件夹" => "Source folder",
        "状态" => "Status",
        "环境信息" => "Environment information",
        "生成的图像文件已移动或删除。" => {
            "The generated image file was moved or deleted."
        }
        "用户" => "User",
        "用户批准" => "User approval",
        "用户批准或服务端自动复核" => "User approval or automatic server review",
        "用户输入 responder 不存在" => "The user input responder is unavailable",
        "用户输入请求" => "User input request",
        "用户输入请求事件标识不一致" => {
            "User input request event identifiers do not match"
        }
        "用户配置" => "User configuration",
        "登录" => "Sign in",
        "登录 ChatGPT" => "Sign in to ChatGPT",
        "登录 ChatGPT 账户" => "Sign in to ChatGPT",
        "登录中…" => "Signing in…",
        "登录失败" => "Sign-in failed",
        "登录失败，请重试" => "Sign-in failed. Please try again.",
        "登录成功" => "Signed in successfully",
        "登录连接在返回结果前关闭" => {
            "The sign-in connection closed before returning a response"
        }
        "登录连接已关闭" => "The sign-in connection closed",
        "目前没有连接任何远程主机。" => "No remote hosts are currently connected.",
        "确认" => "Confirm",
        "确认已核对草稿" => "Confirm draft reviewed",
        "禁止访问" => "Access denied",
        "禁用富文本预览" => "Disable rich preview",
        "禁用文字差异" => "Disable word diffs",
        "禁用自动换行" => "Disable word wrap",
        "移动会话" => "Move conversation",
        "移动聊天" => "Move chat",
        "移动项目" => "Move project",
        "移至“无项目”" => "Move to “No project”",
        "移除" => "Remove",
        "移除此文件中的值，使用继承配置或服务端默认值" => {
            "Remove this value from the file and use inherited configuration or the server default"
        }
        "移除连接在返回结果前关闭" => {
            "The removal connection closed before returning a response"
        }
        "移除项目" => "Remove project",
        "空目录" => "Empty directory",
        "等待失败" => "Wait failed",
        "等待审批" => "Awaiting approval",
        "等待已中断" => "Wait interrupted",
        "等待授权" => "Waiting for authorization",
        "等待输入" => "Waiting for input",
        "等待输出…" => "Waiting for output…",
        "筛选审查文件" => "Filter review files",
        "筛选文件" => "Filter files",
        "筛选文件…" => "Filter files…",
        "简洁" => "Concise",
        "管理员" => "Administrator",
        "管理员已停用" => "Disabled by administrator",
        "管理员或服务端不允许使用此权限配置" => {
            "This permission profile is not allowed by the administrator or server"
        }
        "管理插件、技能和 MCP" => "Manage plugins, skills, and MCP",
        "系统" => "System",
        "红色" => "Red",
        "纯文本" => "Plain text",
        "线程设置连接在返回前关闭" => {
            "The thread settings connection closed before returning a response"
        }
        "组织" => "Organization",
        "终端" => "Terminal",
        "终端命令" => "Terminal commands",
        "终端连接已关闭" => "Terminal connection closed",
        "经优化提示的审查智能体已审查此请求。" => {
            "A review agent with tailored instructions has reviewed this request."
        }
        "经过精心提示的审查智能体在 ChatGPT 运行此请求前已停止审查此请求" => {
            "A review agent with tailored instructions stopped reviewing this request before ChatGPT ran it"
        }
        "经过精心提示的审查智能体在 ChatGPT 运行此请求前已超时。" => {
            "A review agent with tailored instructions timed out before ChatGPT ran this request."
        }
        "经过精心提示的审查智能体正在审查此请求，随后 ChatGPT 才会运行它" => {
            "A review agent with tailored instructions is reviewing this request before ChatGPT runs it"
        }
        "继承 / 未设置" => "Inherit / Not set",
        "继续" => "Continue",
        "编写计划" => "Writing plan",
        "编码" => "Coding",
        "编辑" => "Edit",
        "编辑项目" => "Edit project",
        "编辑 " => "edit ",
        "编辑了多个文件" => "Edited multiple files",
        "编辑了文件" => "Edited files",
        "编辑了文件读取文件" => "Edited and read files",
        "编辑了文件读取文件运行了命令" => "Edited files, read files, ran commands",
        "编辑了文件运行了命令" => "Edited files, ran commands",
        "编辑器" => "Editor",
        "编辑外部文件和使用互联网时始终询问" => {
            "Always ask before editing external files or using the internet"
        }
        "编辑文件" => "Edit files",
        "编辑消息" => "Edit message",
        "编辑评论" => "Edit comment",
        "缩小图片" => "Zoom out",
        "网站" => "Website",
        "网页搜索" => "Web search",
        "网页搜索失败" => "Web search failed",
        "网页搜索已中断" => "Web search interrupted",
        "置顶" => "Pinned",
        "置顶会话" => "Pin conversation",
        "聊天标题" => "Chat title",
        "聊天正在启动，请在就绪后更改权限；更改不会影响当前轮次" => {
            "Chat is starting. Change permissions once it is ready; changes do not affect the current turn."
        }
        "聊天输入框" => "Chat input",
        "聊天连接不可用，输入已保留。请重新连接或新建侧边聊天。" => {
            "Chat connection unavailable. Your input was kept. Reconnect or start a new side chat."
        }
        "自动" => "Automatic",
        "自动保存未完成或遇到冲突。返回编辑以保留当前内容。" => {
            "Autosave is incomplete or has a conflict. Return to editing to keep your changes."
        }
        "自动复核" => "Automatic review",
        "自动复核（兼容旧值）" => "Automatic review (legacy)",
        "自动审查停止说明" => "Automatic review stop explanation",
        "自动审核中" => "Automatic review in progress",
        "自动审核已停止" => "Automatic review stopped",
        "自动审核已批准" => "Automatically approved",
        "自动审核超时" => "Automatic review timed out",
        "自动检测" => "Auto-detect",
        "自定义" => "Custom",
        "自定义 (config.toml)" => "Custom (config.toml)",
        "蓝色" => "Blue",
        "要开启完整访问权限吗？" => "Enable full access?",
        "要退出登录？" => "Log out?",
        "规整直接" => "Structured and direct",
        "计划模式" => "Plan mode",
        "计划步骤" => "Plan steps",
        "认证" => "Authentication",
        "认证状态未知" => "Authentication status unknown",
        "记忆" => "Memory",
        "设置" => "Settings",
        "设置在此电脑上如何收集、保留和整合本地记忆。" => {
            "Choose how local memories are collected, retained, and consolidated on this computer. "
        }
        "设置权限中…" => "Updating permissions…",
        "访问网站、发送数据并使用已启用的插件" => {
            "Access websites, send data, and use enabled plugins"
        }
        "评价回复" => "Rate response",
        "该 MCP elicitation 的 responder 已经失效" => {
            "The MCP elicitation responder is no longer valid"
        }
        "该插件没有本地路径，plugin/share/save 无法定位它" => {
            "This plugin has no local path; plugin/share/save cannot locate it"
        }
        "该插件没有远端 id，服务端不提供技能内容" => {
            "This plugin has no remote ID; the server does not provide skill contents"
        }
        "该服务器已不在列表中" => "This server is no longer in the list",
        "详细" => "Detailed",
        "语言" => "Language",
        "说明（留空将自动生成）" => "Description (leave blank to generate)",
        "请先为此次提交填写新分支名称" => {
            "Enter a new branch name for this commit first"
        }
        "请先保存或放弃当前文件的修改" => {
            "Save or discard changes to the current file first"
        }
        "请先保存或清空当前草稿，再恢复失败的输入。" => {
            "Save or clear the current draft before restoring failed input."
        }
        "请先创建分支再推送" => "Create a branch before pushing",
        "请先读取配置" => "Load configuration first",
        "请在浏览器中完成授权。完成后此窗口会自动更新。" => {
            "Complete authorization in your browser. This window will update automatically."
        }
        "请在浏览器中完成授权，然后返回这里。" => {
            "Complete authorization in your browser, then return here."
        }
        "请打开下面的地址，并输入一次性代码。完成后此窗口会自动更新。" => {
            "Open the address below and enter the one-time code. This window will update automatically."
        }
        "请查看附加文件。" => "Please review the attached files.",
        "请求已取消" => "Request canceled",
        "请求已失效" => "Request expired",
        "请求批准" => "Ask for approval",
        "请求更改" => "Request changes",
        "请等待配置就绪并核对草稿" => {
            "Wait for configuration to load and review the draft"
        }
        "请输入有效的分支名称" => "Enter a valid branch name",
        "请输入非空值，或在菜单中选择继承" => {
            "Enter a nonempty value, or choose Inherit from the menu"
        }
        "请选择一个选项。" => "Please choose an option.",
        "请选择可写配置文件" => "Choose a writable configuration file",
        "请选择图标形状。" => "Choose an icon shape.",
        "请选择界面主色。" => "Choose the primary interface color.",
        "请重新生成上一张图像，保持相同要求。" => {
            "Please regenerate the last image with the same requirements."
        }
        "请重新读取配置并核对草稿，保存不会自动重试" => {
            "Reload configuration and review the draft. Saving will not retry automatically."
        }
        "读取" => "Read",
        "读取、创建、修改、上传或删除此计算机上任意位置的文件" => {
            "Read, create, modify, upload, or delete files anywhere on this computer"
        }
        "读取中…" => "Loading…",
        "读取会话" => "Read conversation",
        "读取会话历史" => "Read conversation history",
        "读取命令输出失败" => "Could not read command output",
        "读取命令错误失败" => "Could not read command errors",
        "读取失败" => "Load failed",
        "读取失败后需要先成功重新读取配置，才能保存草稿" => {
            "After a read failure, reload configuration successfully before saving the draft"
        }
        "读取已共享的插件" => "Load shared plugins",
        "读取权限中…" => "Loading permissions…",
        "读取聊天历史" => "Read chat history",
        "读取聊天历史的响应通道提前关闭" => {
            "The chat history connection closed before returning a response"
        }
        "读取账户状态" => "Read account status",
        "读取配置的连接已关闭" => "The configuration read connection closed",
        "读取配额" => "Read usage limits",
        "引导" => "Steer",
        "账户" => "Account",
        "账户状态未知" => "Account status unknown",
        "账户连接在返回结果前关闭" => {
            "The account connection closed before returning a response"
        }
        "资源" => "Resources",
        "赞" => "Good response",
        "跟进处理方式" => "Follow-up behavior",
        "跳转到文件" => "Jump to file",
        "跳过" => "Skip",
        "踩" => "Bad response",
        "轻度" => "Light",
        "输入其他值…" => "Enter another value…",
        "输入后文件将超过 2 MB，请缩小粘贴内容" => {
            "This would exceed the 2 MB file limit. Paste less text."
        }
        "输入数字" => "Enter a number",
        "输入配置值" => "Enter configuration value",
        "输出详细程度" => "Response detail",
        "运行中" => "Running",
        "运行了命令" => "Ran commands",
        "运行命令、安装软件和更改系统设置" => {
            "Run commands, install software, and change system settings"
        }
        "返回" => "Back",
        "返回侧边聊天" => "Back to side chat",
        "返回子智能体列表" => "Back to subagents",
        "返回应用" => "Back to app",
        "返回编辑" => "Return to editing",
        "还原全部" => "Restore all",
        "还原文件" => "Restore file",
        "还原更改" => "Restore changes",
        "还原更改？" => "Restore changes?",
        "还没有可管理的技能" => "No skills to manage yet",
        "这会带来敏感数据丢失或泄露、提示注入等风险。你可以将其关闭。" => {
            "This carries risks including sensitive data loss or exposure and prompt injection. You can turn it off at any time."
        }
        "这将还原当前列表中的所有文件更改。" => {
            "This will restore all file changes in the current list."
        }
        "进入全屏" => "Enter full screen",
        "进入或退出全屏" => "Enter or exit full screen",
        "进行中" => "In progress",
        "远程" => "Remote",
        "远程主机" => "Remote host",
        "连接中" => "Connecting",
        "连接到互联网" => "access the internet",
        "连接失败" => "Connection failed",
        "连接已变化，请重新读取权限配置" => {
            "The connection changed. Reload permission profiles."
        }
        "连接已断开" => "Disconnected",
        "连接已重建" => "Connection reestablished",
        "连接已重建，权限变更结果未确认，请重新读取后核对。" => {
            "Connection reestablished; permission changes are unconfirmed. Reload and verify."
        }
        "追加输入" => "Add input",
        "追加输入响应连接已关闭，接受状态未知。输入快照已保留。" => {
            "The additional input connection closed; acceptance is unknown. A copy of your input was kept."
        }
        "退出全屏" => "Exit full screen",
        "退出登录" => "Log out",
        "退出登录连接在返回结果前关闭" => {
            "The sign-out connection closed before returning a response"
        }
        "选择 ChatGPT 何时请求批准" => "Choose when ChatGPT asks for approval",
        "选择 ChatGPT 回复包含细节的详细程度" => {
            "Choose how much detail ChatGPT includes in responses"
        }
        "选择 ChatGPT 回复的默认语气" => "Choose the default tone for ChatGPT responses",
        "选择 ChatGPT 总结其推理的方式" => "Choose how ChatGPT summarizes its reasoning",
        "选择 ChatGPT 访问网络的方式" => "Choose how ChatGPT accesses the web",
        "选择 ChatGPT 运行命令时的权限范围" => {
            "Choose the permission scope for running commands"
        }
        "选择已连接计算机上的文件夹" => "Choose a folder on a connected computer",
        "选择模型" => "Choose model",
        "选择模型和思考强度" => "Choose model and reasoning effort",
        "选择项目" => "Choose project",
        "通知" => "Notifications",
        "通过配置和已启用的插件管理生命周期钩子。" => {
            "Manage lifecycle hooks through configuration and enabled plugins. "
        }
        "速度" => "Speed",
        "邀请好友" => "Invite friends",
        "配置" => "Configuration",
        "配置值" => "Configuration value",
        "配置尚未就绪" => "Configuration is not ready",
        "配置层不可写" => "Configuration layer is not writable",
        "配置层缺少版本，无法安全保存" => {
            "Configuration layer has no version and cannot be saved safely"
        }
        "配置已在外部修改。草稿已保留，请先重新读取，再核对后保存。" => {
            "Configuration changed externally. Your draft was kept. Reload, review, and save again."
        }
        "配置文件不可写" => "Configuration file is not writable",
        "配置新聊天的权限、网页访问和智能体回复" => {
            "Configure permissions, web access, and agent responses for new chats"
        }
        "配置来源" => "Configuration source",
        "配置来源与受管限制" => "Configuration sources and managed restrictions",
        "配额连接在返回结果前关闭" => {
            "The usage connection closed before returning a response"
        }
        "醒目强调" => "Bold emphasis",
        "重做" => "Redo",
        "重命名" => "Rename",
        "重命名会话" => "Rename conversation",
        "重命名状态缺少原路径" => "Rename status is missing the original path",
        "重命名聊天" => "Rename chat",
        "重新加载" => "Reload",
        "重新加载中…" => "Reloading…",
        "重新加载文件？" => "Reload file?",
        "重新加载结果未知" => "Reload result unknown",
        "重新加载结果未知；请重新读取服务器列表" => {
            "Reload result unknown. Reload the server list."
        }
        "重新加载超时，结果未确认；连接已重置，请重试" => {
            "Reload timed out; result unconfirmed. The connection was reset. Please try again."
        }
        "重新加载连接已关闭，结果未确认" => {
            "The reload connection closed; the result is unconfirmed"
        }
        "重新打开应用页或刷新目录以读取最新状态" => {
            "Reopen the apps page or refresh the catalog to load the latest state"
        }
        "重新打开插件页或刷新目录以读取最新状态" => {
            "Reopen the plugins page or refresh the catalog to load the latest state"
        }
        "重新打开链接" => "Reopen link",
        "重新登录" => "Sign in again",
        "重新读取" => "Reload",
        "重置为默认设置" => "Reset to defaults",
        "重试" => "Retry",
        "重试保存" => "Retry save",
        "重试回复" => "Retry response",
        "重试图像生成" => "Retry image generation",
        "重试连接已关闭" => "The retry connection closed",
        "钩子" => "Hooks",
        "钩子反馈" => "Hook feedback",
        "钩子反馈，打开钩子设置" => "Hook feedback, open hook settings",
        "错误" => "Error",
        "随心输入" => "Ask anything",
        "隐藏文件" => "Hide files",
        "隐藏空白字符" => "Hide white space",
        "集成" => "Integrations",
        "需要明确授权" => "Explicit authorization required",
        "需要登录" => "Sign-in required",
        "需要账户验证" => "Account verification required",
        "需要采取行动" => "Action required",
        "项目" => "Projects",
        "项目名称" => "Project name",
        "项目类型" => "Project type",
        "项目配置" => "Project configuration",
        "预览" => "Preview",
        "高" => "High",
        "高级" => "Advanced",
        "默认推理强度" => "Default reasoning effort",
        "默认权限配置" => "Default permission profile",
        "默认模型" => "Default model",
        "默认速度" => "Default speed",
        "（无输出）" => "(no output)",
        "；会话静态默认值" => "; static session default",
        "；认证已失效，请重新连接该服务" => {
            "; authentication expired, please reconnect this service"
        }
        "查看活动" => "View activity",
        "查看活动，需要关注" => "View activity, needs attention",
        "关闭活动视图" => "Turn off activity view",
        "优先级" => "Priority",
        "暂无需要关注的任务" => "Nothing needs attention",
        "今天" => "Today",
        "昨天" => "Yesterday",
        "活动视图选项" => "Activity view options",
        "显示" => "Show",
        "优先事项部分" => "Priority section",
        "定时任务" => "Scheduled",
        "恢复默认设置" => "Restore defaults",
        "清除已读聊天" => "Clear read chats",
        "置顶聊天" => "Pin chat",
        "取消置顶聊天" => "Unpin chat",
        "归档会停止所有正在进行的工作。你可以稍后在设置中恢复该聊天。" => {
            "Archiving will stop any ongoing work. You can restore the chat later in settings."
        }
        "归档会停止所有正在进行的工作。你可以稍后在设置中恢复这些聊天。" => {
            "Archiving will stop any ongoing work. You can restore the chats later in settings."
        }
        "最近的对话串不会被归档" => "Recent threads won’t be archived",
        "正在归档…" => "Archiving…",
        "停止并归档" => "Stop and archive",
        "收藏该轮次" => "Bookmark turn",
        "取消收藏该轮次" => "Remove bookmark",
        _ => return None,
    })
}
