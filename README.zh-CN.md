<h1 align="center">Echora</h1>

<p align="center"><strong>以原生 GPUI，重现 ChatGPT App 的交互体验。</strong></p>
<p align="center"><strong>GUI 完全由 GPT-6-Astra 制作。</strong><br>通过 Codex app-server 驱动，以完整复刻 ChatGPT App 交互体验为目标。<br>未来接入更多 Coding Agent，延续同一套熟悉的工作流。</p>
<p align="center"><a href="README.md">English</a> · <strong>简体中文</strong></p>

<p align="center">
  <img alt="GUI 完全由 GPT-6-Astra 制作" src="https://img.shields.io/badge/GUI_by-GPT--6--Astra-8b7cf8">
  <a href="https://github.com/rita152/Echora"><img alt="状态：早期开发" src="https://img.shields.io/badge/status-early_development-8b7cf8"></a>
  <a href="rust-toolchain.toml"><img alt="Rust 1.97.1" src="https://img.shields.io/badge/Rust-1.97.1-dea584"></a>
  <a href="Cargo.toml"><img alt="界面框架：GPUI" src="https://img.shields.io/badge/UI-GPUI-5ca9a2"></a>
  <img alt="开发平台：macOS" src="https://img.shields.io/badge/platform-macOS-999999">
</p>

![Echora 原生 GPUI 工作区：深色主题](docs/images/workspace-dark.png)

## Echora 的想法

**这个仓库中的 GUI 完全由 GPT-6-Astra 制作。** Echora 是产品名称，GPT-6-Astra 是制作它的模型。Echora 取自 *Echo* 的「回响」意象：使用 Rust 与 GPUI，在原生应用中重现 ChatGPT App 的交互体验。

项目的目标是**通过 Codex app-server，完整复刻 ChatGPT 桌面应用的交互体验**。复刻范围既包含外观，也包含工作流中的具体行为：创建和恢复对话、流式回复、运行中追加输入、审批操作、编辑文件、使用终端、审查改动与设置导航。

未来接入更多 Coding Agent 后，用户仍然可以沿用这套工作流，在同一个界面中使用不同厂商的 Agent 产品。Agent 的选择不断增加，熟悉的操作方式得以保留。

| 项目中的组成 | 角色 |
|---|---|
| **GPT-6-Astra** | 制作了这个仓库中的 GUI 实现。 |
| **Rust + GPUI** | 负责原生应用的界面绘制与交互。 |
| **Codex app-server** | 将 GUI 连接到 Codex 的后端能力。 |
| **ChatGPT App** | 完整交互体验的复刻参考。 |
| **其他 Coding Agent** | 未来通过各厂商对应的适配器接入。 |

**实现状态：** 完整交互复刻是项目目标。目前仅接入 Codex app-server 的部分能力，部分导航与设置仍为占位，其他 Coding Agent 尚未接入。实际覆盖范围以 [接入总表](docs/APP_SERVER_INTEGRATION.md) 为准。Echora 是独立应用，并非 OpenAI 官方产品，也不是运行在 ChatGPT 内部的扩展。

<details>
<summary>查看浅色主题</summary>

![Echora 原生 GPUI 工作区：浅色主题](docs/images/workspace-light.png)

</details>

两张图片均由当前工作树打包的验收 bundle（`scripts/package_gpui_capture.sh`）实机截取，使用固定的对话与工具活动示例（`--tool-group-state=completed`），并指向空的 `CODEX_HOME`，因此不会出现本机项目与会话；不执行图中命令，也不发送模型请求。界面目前仍保留部分 Codex 字样。图片为实际应用截图，并非设计稿。

## 现在可以做什么

| 工作流 | 当前能力 |
|---|---|
| **项目与会话** | 创建、恢复、搜索、重命名、归档、删除、移动和置顶会话；切换会话时保留后台轮次。 |
| **查看活动** | 侧边栏铃铛（或 `Option+Cmd+U`）把项目与最近列表换成活动视图：顶部“优先级”列出进行中、等待批准或回复、以及有未读轮次的聊天，下方按“今天”“昨天”和星期分组列出最近七天，分组标题在滚动时吸顶，每次显示十行。聊天读过后仍留在优先级中，直到点击“清除已读聊天”；新需要关注的聊天会随时追加。点击行会打开对应会话且不离开活动视图，第二行显示所属项目（或 Codex），悬停时提供置顶与归档。`…` 菜单可显示或隐藏优先级与置顶分区、把优先级全部标为已读，并在确认后归档优先级中的聊天。 |
| **边运行边沟通** | 流式显示回复，向当前活动轮次追加输入；在时间线中查看计划、搜索、等待、工具活动与文件改动。尚未完成的回复按 ChatGPT 应用的自适应节奏揭示（每 50 ms 放出一批字符，速率跟随积压），新出现的词、行内代码与链接以 0.7 s 淡入，列表项、表格行、引用与分割线以 0.15 s 淡入；未写完的语法不会闪现：最后一行未闭合的链接、图片或引用标记在闭合前保持隐藏，悬空的 `*`、`**` 会提前补齐。完成快照到达即显示全文，开启“减少动态效果”时逐条直接显示。 |
| **按 prompt 导航** | 通过会话左侧的提示轨在用户 prompt 之间跳转：标记会高亮当前屏幕内的轮次，指针悬停时立即向两侧递减，并以 ChatGPT 应用的 160 ms 弹性曲线过渡。指针进入提示轨 250 ms 后弹出预览（300 ms 内返回则立即弹出），预览跟随悬停的标记，指针移向预览途中保持打开；上方是 prompt，下方是该 prompt 在本轮中收到的最后一条回复，排版与 ChatGPT 的三行 Markdown 预览一致（代码块按纯文本显示，段落间距相同）。点击标记会平滑滚动到附近的轮次（较远的轮次直接跳转）并闪烁该气泡；按住沿提示轨拖动可快速浏览对话。在会话中任意位置（包括输入框）按 Alt+↑ / Alt+↓ 可跳到上一条或下一条 prompt。每轮的首条 prompt 停在距顶部 16 px 处，追加的 prompt 停在顶部，对话在透明的会话标题栏下方保持可见。标记较多时提示轨自身滚动、两端渐隐，并保持当前标记可见；在其上滚动滚轮不会滚动对话。收藏某个轮次后会保存本次运行的标记；Codex app-server 没有收藏方法，这些键由 Echora 自己维护。 |
| **审批操作** | 通过原生卡片检查命令、文件和附加权限请求，查看自动复核结果，选择服务端支持的权限配置。 |
| **MCP 请求输入** | 以原生 form、url 卡片响应 `mcpServer/elicitation/request`：校验必填、类型、范围与选项，接受才提交结构化内容，跳过／取消分别对应协议动作，并等待 `serverRequest/resolved` 后收束。 |
| **开始对话** | 空白会话显示项目标题和输入框，不显示占位建议。 |
| **处理文件** | 浏览本地文件树、筛选路径、多标签编辑、预览 Markdown 和图片，以及通过文件链接定位到行。 |
| **使用终端** | 在会话目录运行本机 shell，支持多标签、回看、文字选择和剪贴板。 |
| **审查与交付** | 查看 Git diff、逐行评论、暂存、还原、提交、创建分支、推送，并通过本机 `gh` 创建 PR。 |
| **浏览 Pull Request** | 打开侧边栏 `Pull requests` 页面：列表与过滤、Summary、Activity、提交与检查、带文件树的 diff、行内评论，以及从变更统计按钮打开的 Review 标签页。 |
| **侧边探索** | 从主会话派生临时对话，分别控制输入、模型、权限与停止操作。 |
| **配置 Codex** | 读取有效配置与来源，查看受管限制，编辑已支持的用户层设置，并通过后端回读核验保存结果。 |
| **钩子、实验性功能与记忆** | 「设置 → 钩子」按来源列出 `hooks/list` 返回的钩子，显示待审核与加载问题摘要，支持逐项信任与全部信任、启用开关（受管钩子始终开启）、详情、刷新，以及打开钩子所在的配置文件。「设置 → 配置」在「实验性质功能（测试版）」中列出服务端的 beta 功能，切换后提示需要重启。「设置 → 个性化」可启用 Codex 记忆、允许从使用工具的聊天中生成记忆，并在确认后删除全部记忆；斜杠菜单的「记忆」设置当前聊天是否使用与生成记忆。 |
| **代码审查、Shell 命令与分区** | 斜杠菜单的「代码审查」以 `review/start` 审查未提交的更改或当前分支相对基准分支的更改：默认在当前聊天进行；「设置 → Git → 审查结果呈现方式」为「单独」时，在同项目的新聊天中进行。该轮显示「审查模式」，审查面板打开对应 diff。以 `!` 开头的一行通过 `thread/shellCommand` 在聊天的 shell 中运行，不受沙盒限制。自定义侧栏分区位于置顶与项目之间，可新建、重命名、排序、折叠和移除；聊天与项目可通过菜单或拖放移入，也可以直接在分区中新建聊天。 |
| **选择语言** | 在设置 → 常规 → 语言中切换 English、简体中文或自动检测；切换立即生效并在本地保存。 |
| **管理账户** | 在账户菜单查看当前 ChatGPT 账户与套餐，未以 ChatGPT 登录时显示所配置模型提供方的名称；后端要求 OpenAI 认证时可通过 Codex 管理的 ChatGPT 登录，也可取消进行中的登录，并在确认后退出登录。 |
| **管理技能与 MCP** | 读取技能目录并按技能启用／禁用并核验回执；列出 MCP 服务器的状态、认证、工具与服务端扩展字段；重新加载服务器；完成 OAuth 登录并区分等待、成功、失败、取消与断连状态。 |
| **管理插件与应用** | 从后端读取插件目录与已安装子集，跨 marketplace 搜索，打开插件自身详情（描述、技能、MCP 服务器），经确认后安装／卸载，管理 marketplace（添加、更新、移除），读取已共享的插件与插件技能内容。目录行、分段徽标与计数一律使用服务端返回值；服务端没有提供图标或描述的插件就按原样呈现，不伪造占位内容。 |

具体协议覆盖与兼容规则以 [app-server 接入总表](docs/APP_SERVER_INTEGRATION.md) 为准。可见的界面入口不代表对应厂商能力已经完整接入。

## 快速开始

当前开发与验收平台为 **macOS**。需要 [rust-toolchain.toml](rust-toolchain.toml) 固定的 Rust 工具链、macOS 构建工具，以及已安装、已登录且位于 `PATH` 的 Codex CLI。当前接入基线为 `codex-cli 0.158.0`，更换 CLI 版本前请核对接入总表。

```bash
git clone https://github.com/rita152/Echora.git
cd Echora

rustc --version
codex --version
cargo run --release -- --theme=dark
```

浅色主题使用 `--theme=light`。本地开发可运行 `cargo run -- --theme=dark`，使用已优化的开发 profile。以下示例均从仓库根目录执行。

界面默认自动检测语言：中文系统区域使用简体中文，其余使用英语。设置 → 常规 → 语言可保存明确选择。启动时可用 `--language=en`、`--language=zh-CN` 或 `--language=auto` 临时覆盖，不修改已保存的偏好。此设置只改变应用界面文案，不翻译会话消息、项目名称、文件内容或后端提供的文本。

Echora 启动 `codex app-server --stdio`，使用本机 Codex 安装提供的后端配置与会话。Git 审查需要本机 Git，创建 PR 另需已登录的 `gh`。Python、Node.js 和 Electron 用于开发验证，不是运行原生界面的必要依赖。

Cargo 包和可执行文件目前仍名为 `gpui-chat-clone`，现有构建与截图命令继续沿用该名称。暂未提供预编译发布包，也尚未完成跨平台验证。

## 常用入口

侧边栏导航显示新对话、拉取请求、已安排和插件。标题栏与 ChatGPT 一致：侧栏展开时显示返回、前进和侧栏按钮，收起后只保留侧栏按钮；在会话中还会多出一个新对话按钮，与侧栏的新对话行走同一路径。

| 操作 | 入口 / 快捷键 |
|---|---|
| 打开会话 | 侧栏项目、最近、归档或搜索 |
| 查看项目信息 | 悬停侧栏项目行：卡片显示项目名、任务数、仓库、工作目录和“编辑项目” |
| 查看任务信息 | 悬停项目下的侧栏任务行：卡片显示任务标题、环境图标与距今时间，以及任务所属项目 |
| 重命名任务 | 双击侧栏任务行：第一次点击进入对应会话，第二次弹出与 ChatGPT 一致的居中重命名面板（标题输入框默认全选，可用 `取消`、`保存`、`Esc`、关闭按钮或蒙层退出） |
| 搜索聊天 | 侧边栏搜索按钮 → 历史会话搜索弹窗（`Enter` 打开、`⌘1`–`⌘9` 选择、`Esc` 关闭） |
| 查看活动 | 侧边栏铃铛（“查看活动”）；`Option+Cmd+U` 切换 |
| 切换终端 | 右侧面板 → 终端；`Ctrl+反引号` |
| 打开文件 | 右侧面板 → 文件；`Cmd+P` |
| 打开 Git 审查 | 右侧面板 → 审查；`Ctrl+Shift+G` |
| 打开侧边聊天 | 右侧面板菜单；`Option+Cmd+S` |
| 打开设置 | 账户菜单 → 设置；`Cmd+,` |
| 登录 / 退出登录 | 账户菜单 → 登录行（后端要求 OpenAI 认证时显示），或在应用内确认后 `退出登录`；`Esc` 关闭菜单 |
| 发送 / 向活动轮次追加输入 | `Enter`；`Shift+Enter` 换行；`Cmd+Enter` 对单条消息使用相反的跟进处理方式 |
| 斜杠命令 | 在行首或空格后输入 `/`；`Up`/`Down`（或 `Ctrl+N`/`Ctrl+P`）移动，`Enter` 执行，`Esc` 关闭 |
| 在聊天中查找 | 对话区域获得焦点时按 `Cmd+F`；`Cmd+G` / `Cmd+Shift+G` 或 `Enter` / `Shift+Enter` 切换结果，`Esc` 关闭 |
| 编辑最后一条排队消息 / 撤销或重做排队消息的删除或编辑 | 空输入框中按 `Up` / 输入框没有可撤销或重做的文本编辑时按 `Cmd+Z`、`Cmd+Shift+Z` |
| 立即保存文件 | `Cmd+S` |
| 终端清屏 | `Cmd+K` |
| 切换 / 关闭侧边聊天标签 | `Ctrl+Tab`、`Ctrl+Shift+Tab`；`Cmd+W` |

<details>
<summary>行为细节与当前边界</summary>

- **运行中追加输入：** 设置 → 常规 →「跟进处理方式」在引导（`turn/steer`，默认）与排队（`thread/queue/add`）之间选择，`Cmd+Enter` 对单条消息取反，选择保存在 UI 偏好中。排队的消息显示在输入框上方的托盘里，每行可以立即发送（运行中引导进当前轮次并移出队列，空闲时直接开始）、编辑、在新的侧边聊天中打开、删除或拖动排序。与 ChatGPT 一致，编辑会先把消息移出队列，重新提交时排回原位置（没有运行中的轮次也没有其他排队消息时直接发送）；`Cmd+Z` 可在一分钟内恢复删除的消息、30 分钟内恢复编辑中的消息，`Cmd+Shift+Z` 重做。轮次结束后由服务端自行开始下一条；用户停止轮次后队列暂停，直到继续，此时发送新消息会先询问是否清空队列。侧边聊天始终引导。失败输入可连同附件与审查评论恢复，不覆盖后续新草稿，也不自动改发为新轮次。
- **目标：** 斜杠菜单的「目标」（或输入 `/goal`）打开输入框的「目标」标记，`/goal <目标>` 直接设置。之后服务端会在聊天空闲时自行推进，这些轮次与普通轮次一样流式显示，请求下方显示「设为目标」。托盘显示目标状态与已用时间，提供清除、暂停／恢复和编辑；编辑在右侧面板打开「编辑目标」标签，提供「还原」和「保存」（保存会同时恢复已暂停的目标）。替换已保存的目标前会先确认。停止轮次时先暂停进行中的目标再中断；目标达成后立即离开托盘并自动清除，达成目标的轮次显示「已在 … 内达成目标」，直到开始新的目标。超过 4000 个字符的目标与 ChatGPT 一样保存到 `$CODEX_HOME/attachments` 下的文件，以指针发送。
- **斜杠菜单：** 在行首或空格后输入 `/` 会在输入框上方打开菜单，提供「代码审查」（Git 项目且输入框没有其他内容时）、「目标」「压缩」「计划模式」（服务端提供时）、「批准」（存在可批准的自动复核拒绝时）和「记忆」（记忆功能开启时）；输入的查询按模糊匹配排序，标题中匹配的部分保持高亮，ChatGPT 的其他命令尚未实现。
- **协作模式：** 计划与默认模式来自连接的 `collaborationMode/list` 预设，每个连接读取一次；服务端列出计划模式时才提供该选项，模型与推理强度始终使用你的选择。
- **实时输出与历史：** 实时与恢复后的已完成轮次共用最终答复选择、工作过程折叠、答复操作与文件汇总。文本增量保留 item 身份，完成消息快照校正显示内容；相邻增量以 8 ms 窗口批处理，代码高亮复用已完成行，仅重解析未结束行。已完成轮次折叠最终答复之前的过程消息，追加的用户消息保留原有位置与附件。文件变更按路径汇总并保留原始 patch。历史由 app-server 提供，不伪造缺失的时间、计划步骤快照或自动复核历史。
- **审批：** 并发请求依次显示，提交后等待服务端释放。响应失败可见且不可重复提交。可查看原始请求补丁，展开和复制长命令。使用 Tab / 方向键导航、Enter 激活、Esc 关闭或拒绝；文件审批的 `Shift+Esc` 拒绝并停止轮次。
- **权限：** 读取服务端 profile 全部页面，展示禁用选项及原因。菜单隐藏内置 `:read-only` profile，不显示后续轮次提示和手动重新读取入口。已有线程等待 RPC 成功与匹配的设置通知后才显示生效，更新影响后续轮次；复核者变化还会经 `turn/settings/update` 同步到进行中的轮次。完整访问权限需要应用内确认，侧边聊天独立管理权限。线程被另一个 app-server 占用时，警告卡片固定在输入框上方，不随会话滚动。
- **账户：** 账户菜单与登录流程都由连接级账户快照驱动。缺失的套餐显示为未知而不是杜撰，登录保留服务端返回的 `loginId` 直到完成通知到达，退出登录先确认再发请求。只提供 Codex 管理的 ChatGPT 登录，API key、外部 token 与 Bedrock 变体返回明确错误。具体协议覆盖见接入总表。
- **配置：** 使用带版本的 `config/batchWrite` 保存并回读，项目层与受管层只读。冲突保留草稿，结果未知时不自动重试。保存的模型、推理强度、服务等级与个性默认值用于后续线程，不热更新已打开的线程。
- **活动：** 计划支持流式更新、步骤进度、复制、显式下载和只读文件标签；搜索保留查询与结果，等待保留时长与状态。Hook 反馈只读。被拒绝的自动复核会显示「被拒绝的原因」和「批准后允许的操作」，并提供文字链接「批准」，通过 `thread/approveGuardianDeniedAction` 记录一次重试授权，不会执行该操作；斜杠菜单的「批准」列出最新 10 条可批准的拒绝。自动复核详情支持键盘操作和文字选择，遵循减少动态效果设置。认证恢复与弃用提示的展示边界见接入总表。
- **文件编辑：** 停止输入约 400 ms 后自动保存，撤销 / 重做也写回磁盘。保留 UTF-8 BOM、CRLF 和权限，保存前检查外部修改。文本上限 2 MiB，单行上限 64 KiB；仅访问本机文件。
- **Git 审查：** 范围包括上一轮、未提交、未暂存、已暂存、已提交和分支，分支使用 merge-base。支持统一 / 拆分差异、文字差异、上下文展开和逐行评论。写入前校验 worktree 与 index；还原新增文件时，在 worktree Git 目录的 `gpui-discarded/` 下保留备份。
- **Pull requests：** 页面通过已认证的 `gh` 读写 GitHub。筛选、审阅者搜索、摘要、活动、检查与提交范围均使用 GitHub 数据。差异在统一、分栏与自动（仅对同时有增删的文件分栏）布局间切换，支持词级高亮、Markdown 预览、浮层文件树及行内评论；文件链接打开 GitHub 上所选提交的内容。Code 标签只标注 hunk 之间未改动的行数；从变更统计打开的 Review 标签会读取所显示文件的全文，每次展开 100 行，统计最后一个 hunk 之后的行数，并可在 `Commits` 菜单中把差异范围切换为全部改动或单个提交。写入失败保留草稿，提交中防止重复请求，成功后从 GitHub 刷新。“Draft description in chat” 打开预填的会话，发送前可检查内容。窄窗口在列表与详情之间切换，提供返回按钮。收起侧栏后，列表头部以及占满页面的详情或 Review 标签从红绿灯与侧栏按钮之后开始，与 ChatGPT 一致。列表与详情正文只在经典滚动条下预留滚动条槽位，macOS 叠加滚动条下不预留。
- **面板生命周期：** 收起面板或切换会话保留状态，shell 与临时侧边聊天不跨应用退出恢复。标签可拖动排序，有消息的侧边聊天关闭前需要确认。连接失效后仍可查看和复制消息。
- **活动视图：** 进行中与待处理状态来自 Echora 自身 app-server 连接上的 `thread/status/changed`，因此在其他客户端（例如 ChatGPT 应用）中运行的聊天不会在这里显示为进行中。app-server 没有已读状态：聊天不在主区域显示时，若轮次结束或请求批准／输入，由 Echora 自己记为未读并随 UI 偏好保存；打开该聊天或使用“全部标为已读”后恢复为已读。“定时任务”选项会保存，但目前没有线程来源能识别定时任务运行，因此不影响列表；参考实现的一次性引导气泡与 `⌘1`–`⌘9` 行快捷键尚未实现。
- **聊天搜索：** 弹窗先列出置顶聊天，再按最近顺序补足，最多九行；输入后经 app-server `thread/search` 检索。参考实现还会通过自身的检索服务合并 ChatGPT 云端会话，app-server 不提供该数据，因此命中较多时结果集合与排序可能不同。`Search files`（或 `⌘P`）把同一弹窗切到文件搜索：为当前会话工作目录打开一个 `fuzzyFileSearch` 会话，边输入边接收 `sessionUpdated` 结果，按服务端返回的下标高亮命中，选中后在文件面板打开。服务端不支持会话时回退到一次性 `fuzzyFileSearch` 请求。
- **改写消息：** 最新一条用户消息的悬停操作里提供编辑入口。提交改写后的文本会以该轮作为 `beforeTurnId` 调用 `thread/revert`，把持久化历史替换为该轮之前的前缀，随后发起新的 `turn/start`。只改会话历史，不动本地文件；轮次仍按既有分页路径重载。
- **在聊天中查找：** 在对话中按 `Cmd+F` 打开查找栏，由 `thread/searchOccurrences` 提供结果（先取前 250 个，还有更多时显示「+」，越过已读结果时继续读取下一页）。匹配在渲染后的消息中高亮，当前结果为橙色；结果所在轮次尚未加载时会重新读取一次历史。服务端无法搜索的临时侧边聊天改为在已加载的消息中本地查找。文件编辑器、终端与 PR 视图保留各自的 `Cmd+F`。
- **钩子、功能与记忆：** 钩子信任与启用、实验性功能开关和记忆设置都以带版本的用户层 `config/batchWrite` 立即保存并回读；冲突或失败时恢复服务端值并显示错误。运行中的 app-server 保留启动时的功能开关，因此功能更改在新的 Codex 连接上生效。`/memories` 让新聊天随 `thread/start` 发送所选设置；聊天开始后只能更改是否生成记忆（`thread/memoryMode/set`，失败时回滚）。
- **压缩上下文：** 斜杠菜单的「压缩」或输入 `/compact` 会执行 `thread/compact/start`；轮次运行中会提示无法压缩。压缩按不可 steer 的轮次运行，期间追加输入会如实展示服务端结论，压缩结果沿用既有 contextCompaction 条目展示。
- **代码审查：** 「代码审查」子菜单列出「审查未提交的更改」，并在「与基准分支比较」下列出默认目标分支和最多 100 个最近的本地分支（不含当前分支）；在保留的 `/` 后继续输入可以过滤，分支加载失败时提供「重试」。审查始终以 inline 方式发送：选「单独」时先建新聊天（`thread/start`，带 `threadSource: code_review` 与当前权限），因为 0.158 拒绝对分页线程使用 detached。轮次运行中不能开始审查。服务端会用另一个永不完成的 turn id 宣告审查轮次，Echora 会丢弃它，不接管这个虚假轮次。恢复的审查轮次与实时一样显示请求文本和「审查模式」标记。
- **Shell 命令：** 输入框以 `!` 开头时（侧边聊天、编辑队列项和目标草稿除外），底部显示警告标签「Shell · 在沙盒外运行」；按 Enter 发送这一行的其余部分，新聊天会先建立线程。服务端的 user-shell 轮次显示为默认展开的命令卡片，状态有运行中、完成、失败、超时和中断。命令被拒绝时文字放回输入框并提示。ChatGPT 没有这个入口。
- **自定义分区：** 分区及其中的聊天保存在服务端（`threadSection/*`、`thread/section/move`）；分区顺序、分区内的项目和折叠状态是本地 UI 偏好。移除分区后，其中的聊天回到「最近」。分区头菜单提供「在{分区}中新建聊天」「编辑」「归档聊天」「全部标为已读」「移除分区」；聊天与项目菜单提供「移至分区」和「新建分区…」。
- **提供方能力与记忆状态：** `modelProvider/capabilities/read` 门控「设置 → 配置」的网页搜索（提供方不支持时只能选「已禁用」，并说明原因）以及图片生成失败后的重试；`memory/status` 在「设置 → 个性化」增加「记忆整合」行，并在「聊天记忆」对话框显示状态。ChatGPT 不展示这两项，均为 Echora 新增。`thread/loaded/list` 在打开线程时核对本地的已加载记录，服务端不再持有该线程时会重新 resume。

</details>

## 架构

```text
GPUI 应用 · 项目 · 会话 · 原生面板
                 │
       通用 AgentBackend 契约
                 │
        当前的 Codex 适配器
                 │
       codex app-server --stdio
                 │
        本机 Codex 配置与会话
```

`ChatApp` 装配共享服务，通过 `AgentBackend` 注入视图。项目和会话数据由后端管理；Echora 只在本地持久化 UI 偏好，不维护自己的会话数据库。未来适配器将依据真实协议与产品需求实现通用契约。

| 位置 | 职责 |
|---|---|
| [src/agent/](src/agent/) | 后端契约与模型、消息、事件、审批、活动、配置和历史领域类型；领域模块不依赖 GPUI。 |
| [src/agent/codex/](src/agent/codex/) | Codex 编解码、方法校验、通信、请求与派发；连接生命周期和路由位于 `manager.rs` 与 `manager/`。 |
| [src/workspace.rs](src/workspace.rs)、[src/workspace/](src/workspace/) | 工作区状态与通知合并；`loaders.rs` 负责分页，`preferences.rs` 原子保存 UI 偏好，`activity.rs` 负责侧栏活动视图。 |
| [src/conversation/](src/conversation/) | 会话状态、事件归约、流式批处理与历史恢复，不持有 GPUI Entity 或 Context。 |
| [src/configuration.rs](src/configuration.rs) | 配置草稿、保存回执与回读核验，使用 `src/agent/config.rs` 的领域类型。 |
| [src/i18n.rs](src/i18n.rs)、[src/i18n/](src/i18n/) | UI 语言选择、系统区域检测及应用文案翻译；不依赖 GPUI 或具体适配器。 |
| [src/components/](src/components/) | 侧栏、Composer、时间线、审批、MCP 请求、Markdown、文件、终端、审查、Pull Requests 页面、聊天搜索、账户菜单与侧边聊天的渲染和交互。 |
| [src/git_review.rs](src/git_review.rs)、[src/git_review/](src/git_review/) | Git/gh 操作、diff、版本校验、进程回收与评论，不依赖 GPUI 或具体 Agent 适配器。 |
| [src/pull_requests.rs](src/pull_requests.rs)、[src/pull_requests/](src/pull_requests/) | 通过本机 `gh` 读写 Pull Request 的模型、头像与分支关联，不依赖 GPUI。 |
| [src/skills.rs](src/skills.rs)、[src/mcp.rs](src/mcp.rs)、[src/plugins.rs](src/plugins.rs)、[src/apps.rs](src/apps.rs) | 技能、MCP 服务器、插件与应用的管理状态：快照、刷新周期与待确认的写入意图，不依赖 GPUI。 |
| [src/app.rs](src/app.rs)、[src/app/](src/app/) | 服务装配、会话 host、面板挂载、项目创建与图片预览。 |
| [src/settings/](src/settings/)、[src/media.rs](src/media.rs)、[src/typography.rs](src/typography.rs) | 设置页面、通用媒体工具、字体与字体验收。 |
| [src/theme.rs](src/theme.rs)、[src/assets.rs](src/assets.rs)、[src/stream_capture.rs](src/stream_capture.rs) | 浅色与深色主题 token、运行时资源解析与实时轮次采集。 |

macOS 的 UI 偏好默认保存到 `~/Library/Application Support/GPUI/ui-preferences.json`，可用 `GPUI_UI_PREFERENCES_PATH` 指定验收专用文件。后端配置不写入 UI 偏好。

GPUI 依赖固定在 [Cargo.toml](Cargo.toml) 的同一 Zed revision。`vendor/gpui`、`vendor/gpui_macos` 和 `vendor/gpui_apple` 保留文本、选择、虚拟列表与 Metal 合成修正，升级时需一并复核。`vendor/block` 修正 `block` crate 的 Objective-C 运行时符号声明，避免 Rust 的 future-incompatibility 诊断。OpenAI Sans 从本机已有 ChatGPT 安装读取，缺失时回退系统字体；仓库不分发该字体。

## 开发与验证

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features --no-deps -p gpui-chat-clone -- -D warnings
cargo check --all-targets
cargo check --all-targets --features screenshot
cargo build --features screenshot
git diff --check
```

Clippy 零告警要求限于第一方包。真实模型请求测试与手动滚动基准默认忽略。

配置与权限的集成检查：

```bash
python3 scripts/verify_config_permissions.py --output artifacts/config-permissions-smoke
```

该入口使用本机 CLI，以及输出目录内的独立配置 home 和 Git 项目，验证真实读取、写入、覆盖、版本冲突、非法值、null 删除、profile 分页、线程设置回执与进程重启。不发送模型请求，也不修改用户现有配置。

接入总表本身也由脚本核对：方法集合、默认／实验归属、状态统计与 `src/agent/codex/runtime.rs` 的退订名单都从 CLI schema 重新推导，`artifacts/` 为空时会先生成临时 schema 副本。

```bash
node scripts/verify_integration_table.mjs
```

### 截取原生界面

遵循 [AGENTS.md](AGENTS.md) 的专用实例约定：用本工作树自己的名称与标识打包验收 bundle。

```bash
scripts/package_gpui_capture.sh
```

脚本以 `--features screenshot` 构建，产出 `target/GPUI Capture (<worktree>-<digest>).app`，把 `assets/` 复制进 `Contents/Resources/assets`，并打印 bundle 路径、名称与标识。随后在仓库根目录用绝对路径启动该 bundle 内的 `Contents/MacOS/gpui-chat-clone`：

```bash
GPUI_UI_PREFERENCES_PATH="$PWD/artifacts/capture-preferences.json" \
GPUI_CAPTURE_OUTPUT="$PWD/artifacts/frame.png" \
  'target/GPUI Capture (gpui-<digest>).app/Contents/MacOS/gpui-chat-clone' \
  --theme=light --window-width=1440 --window-height=900
```

Computer Use 先枚举应用，连接打包脚本打印的那个名字，读取可访问性树和截图，再定位操作。按 `Cmd+Shift+F12` 保存未缩放 PNG 与 `.render.json`（含视口、DPR、可执行文件、bundle 标识与已解析的资源目录），应用继续运行。结束时只关闭本次启动的专用实例。

每个打包产物的名称与标识都以本工作树 slug 结尾，资源随 bundle 一起分发，因此 LaunchServices 不可能换成其他目录里的构建。采集脚本通过 `scripts/gpui_capture_binary.sh` 解析该 bundle，它每次调用都会先重新打包当前工作树；`GPUI_CAPTURE_SKIP_BUILD=1` 沿用已有产物，`GPUI_CAPTURE_BUNDLE` 指定一个原样使用的 bundle。`--print-diagnostics` 会打印可执行文件、bundle 身份以及每个资源候选路径与判定，验收运行可以自证在驱动哪一个构建。需要让 bundle 从别处读取资源时，用 `GPUI_ASSETS_DIR` 硬覆盖搜索。若所有候选都不可用，应用继续运行、向 stderr 告警，并在窗口顶部显示写明已尝试路径的红色条，截图不可能再悄悄呈现无图标的界面。`scripts/launch_project_hover_instance.sh` 使用同一套打包身份，会在本工作树的悬停验收实例已在运行时拒绝重复启动，并打印二进制路径、pid 与日志。

Computer Use 只能驱动 macOS 视作用户应用的 bundle，而且驱动过程不能触发隐私弹窗。`scripts/launch_verify_instance.sh` 会打包、把 bundle 复制到 `~/Applications`，再用 `launchctl submit` 加上本会话的 `PATH` 与真实 `HOME` 启动：GUI 进程因此不会打开本仓库所在外置卷上的任何文件，启动也不经过 LaunchServices，它的 `codex app-server --stdio` 子进程读到的正是 ChatGPT 应用同一份 `~/.codex` 项目、会话与登录态。脚本转发 `GPUI_UI_PREFERENCES_PATH`、`GPUI_ASSETS_DIR` 与 `CODEX_HOME`；验证任何会写入 Codex 配置或删除数据的操作（钩子信任、实验性功能、记忆开关、删除记忆）前，先把 `CODEX_HOME` 指向 `~/.codex` 的副本（例如 `cp -Rc ~/.codex/. "$HOME/Library/Application Support/echora-verify/codex-home/"`）。它的 `--stop` 只停止从本工作树副本运行的实例。打成 agent 应用（`GPUI_CAPTURE_AGENT_APP=1`）虽然不显示 Dock 图标，却也会让 Computer Use 的应用清单看不到它。对外置卷上的 bundle 使用 `open -n` 会弹出「想要访问可移除宗卷上的文件」，`launchctl submit` 与会话内的子进程都不会，因此采集脚本与 ChatGPT 参考启动器都不使用 `open -n`。

真实实时轮次采集：启动时传入 `--capture-live-turn="$PWD/artifacts/live.png"`，再在验收实例输入提示词。完成后保存尚未重新加载历史的实时画面，以及包含线程身份、活动数据的 `.json` 侧文件，然后退出。中断、失败或超过五分钟返回失败。

自动恢复会话截图可追加 `--resume-thread=THREAD_ID --screenshot="$PWD/artifacts/resumed-thread.png"`。ID 接受原始 UUID 或 `local:<uuid>`。可选 `--resume-scroll-from-bottom=3200` 指定距底部的滚动距离，省略则停在底部。应用等待历史、侧栏、模型目录与三个稳定绘制帧后截图退出，失败或超时返回非零状态。请选择未被其他活动写入进程占用的线程。

窗口尺寸为逻辑像素，PNG 分辨率取决于显示器 DPR。对照使用相同主题、内容、窗口尺寸、DPR 与滚动位置，不得缩放或平移图片。原始截图、日志和对比数据留在 `artifacts/`，README 展示图片放在 `docs/images/`。

<details>
<summary>更多截图与专项验证入口</summary>

完整启动参数见 [src/main.rs](src/main.rs)。

| 参数 | 用途 |
|---|---|
| `--markdown-file=/absolute/path/to/sample.txt` | 独立 Markdown 窗口，无需 app-server；可搭配 `--window-width=480` 检查窄窗。 |
| `--file-panel-root=/absolute/workspace --open-file=/absolute/file` | 真实文件编辑、保存与冲突检查；使用专用测试文件。 |
| `--review-root=/absolute/repository --review-filter=src/example.rs [--review-menu=scope\|options\|branch]` | 真实 Git 审查；截图等待 diff 就绪。加上 menu 参数会打开其中一个审查弹层，便于固定状态截图。 |
| `--settings-page=appearance` | 直接打开设置页，slug 即 `src/settings/catalog_*.rs` 中各页面的 `slug` 字段。 |
| `--language=en\|zh-CN\|auto` | 指定本次启动的 UI 语言；截图验收时明确指定，以保证可复现。 |
| `--chat-search-state=initial\|selected\|hover\|scroll\|files-empty\|files-result\|files-loading` | 以固定状态打开历史会话搜索弹窗用于截图。`--chat-search-query=` 会先执行一次真实检索（无命中时显示空状态）；`--chat-search-index=` 指定选中行，`scroll` 状态下则是以像素计的滚动偏移。`files-*` 状态显示带固定结果的文件搜索形态。 |
| `--image-generation-ui-state=running/completed/failed/load-error` | 固定图像生成状态；完成态另传 `--image-generation-path=/absolute/image.png`。 |
| `--auto-approval-ui-state=inProgress/approved/denied/timedOut/aborted/strict/warning` | 自动复核，支持 `--auto-approval-expanded`、`--auto-approval-details-expanded` 与 `--reduce-motion`；长说明和动态采样使用 `--auto-approval-rationale-file`、`--auto-approval-motion-output`。 |
| `--runtime-ui-state=completed/running/turnless/auth-started/auth-completed/interrupted/disconnected/history/long/deprecation` | 确定性 Hook、hookPrompt、认证与应用提示，不执行 Hook 或模型请求；`GPUI_RUNTIME_AUDIT_OUTPUT` 输出原始状态与本地收束原因。 |
| `--queue-ui-state=queued/paused/confirm/menu/failed/sending/editing/restored` | 跟进队列托盘：运行中的排队行、停止后的暂停横幅、暂停时发送的确认框、行菜单、发送失败或发送中的行、移出队列正在编辑的消息，以及恢复提示；不发送队列或模型请求。 |
| `--goal-ui-state=active/paused/blocked/usage-limited/budget-limited/chip/replace/complete/edit-tab` | 各状态的目标摘要、输入框的「目标」标记、替换确认、完成目标的轮次（「设为目标」「已在 3s 内达成目标」）以及「编辑目标」标签；不发送目标或模型请求。 |
| `--hooks-settings-state=overview/dialog/trusted/expanded/issues/overridden/refreshed/empty/loading/error` | 以固定数据显示钩子设置：来源总览、带「全部信任」横幅的来源对话框、信任之后、展开详情、加载问题、被覆盖的写入、刷新提示，以及空、加载中与错误状态。配合 `--settings-page=hooks-settings` 使用，不写入任何配置。 |
| `--experimental-features-state=list/restart/empty/loading/error` | 「配置」页（`--settings-page=agent`）的「实验性质功能（测试版）」：固定行、更改后（重启提示），以及空、加载中与错误状态。 |
| `--memories-state=settings-on/settings-off/settings-unavailable/delete-confirm/deleted/delete-failed/slash/dialog-new/dialog-started/dialog-generate-off/rollback` | Codex 记忆设置（`--settings-page=personalization`）及其删除确认与提示、含「记忆」的斜杠菜单，以及新聊天、已开始聊天、关闭生成、更改回滚后的「聊天记忆」对话框。不发送记忆请求。 |
| `--find-bar-state=open/results/second/capped/none` | 固定的「hello」聊天上的查找栏：空查询、两个结果中的第一个、第二个、首页截断（`+`）与无结果。本地匹配，不发送搜索请求。 |
| `--slash-menu-state=menu/query/approve/compact-busy` | 列出全部可用命令的斜杠菜单、输入 `/go` 后的查询结果、含两条拒绝的「批准」子菜单，以及轮次运行中选择「压缩」时的危险提示。 |
| `--review-menu-state=slash/submenu/submenu-branch/loading/failed/escaped`、`--review-turn-state=running/finished`、`--review-delivery-state=inline/detached` | 「代码审查」命令及子菜单（列出本仓库自己的分支）、带「审查模式」标记的审查轮次，以及「设置 → Git → 审查结果呈现方式」（`--settings-page=git-settings`）。不会真正开始审查。 |
| `--shell-mode-state=typing/running/completed/failed/timeout/interrupted` | `!` Shell 标签与各状态的 user-shell 命令卡片。 |
| `--capabilities-state=unsupported/supported`、`--memory-status-state=settings-pending/settings-ready/pending/ready` | 受提供方能力门控的网页搜索（`--settings-page=agent`），以及「设置 → 个性化」（`--settings-page=personalization`）或「聊天记忆」对话框中的记忆整合状态。 |
| `--sections-state=sidebar/hover/menu/thread-menu/dialog-new/dialog-edit` | 两个预置的自定义分区（一个含两个聊天和一个项目，一个为空）、分区头悬停与菜单、聊天菜单中的「移至分区」，以及新建和编辑分区对话框。 |
| `--auto-review-denial-state=denied/approving/approved/failed` | 被拒绝的自动复核及其批准区域的各个状态；不发送批准请求。 |
| `--progress-ui-state=running/streaming/completed/interrupted` | 计划、搜索与等待归约；streaming 定时产生更新和完成，running 可中断。 |
| `--streaming-reply-ui-state=streaming/completed` | 通过归约器按定时 token 突发喂入一段固定回复，配合 `--screenshot-delay-ms=`（开始计帧前按真实时间等待）可截到流式中途的节奏揭示与逐词淡入；completed 会补上完成快照，`--reduce-motion` 则按原样逐条显示、不做节奏与淡入。 |
| `--typography-specimen --typography-display=N` | 字体样本与显示器选择，见 `src/typography.rs`。 |
| `--pull-requests [--pull-requests-select=N \| --pull-requests-title=TEXT] [--pull-requests-tab=code\|review] [--pull-requests-list-tab=all\|reviewing\|authored] [--pull-requests-status=open\|merged\|closed\|all] [--pull-requests-search=TEXT] [--pull-requests-file-tree] [--pull-requests-scroll=px] [--pull-requests-action=...] [--pull-requests-comment-menu]` | Pull Requests 页面确定性状态：列表、标签、搜索、过滤、分组、详情分节、diff、文件树、Review 标签，以及 `scripts/capture_pull_requests_gpui.sh` 使用的交互状态。动作：`filter-menu`、`filter-status`、`filter-repository`、`title-edit`、`reviewers`、`status-menu`、`description-menu`、`comment-menu`、`expand-commits`、`fullscreen`、`split`、`auto-layout`、`collapse-all`、`review-options`、`inline-comment[=N]`（在第一个文件的新文件第 N 行打开评论草稿），Review 标签中另有 `scope-menu`、`scope-commits`、`scope-commit`、`scope-commit-menu`、`expand-first-gap`。 |
| `--pointer=X,Y` | 与 `--screenshot` 一起使用：页面就绪后把指针移到窗口坐标 `X,Y`，截图即包含该悬停状态及其打开的提示。 |
| `--project-hover-card=NAME` | 不依赖指针直接打开指定项目（按名称或稳定 id）的侧栏悬停卡片，用于悬停卡片截图的静态部分。 |
| `--new-chat-project=NAME` | 待侧栏列出指定项目（按名称或稳定 id）后，走项目行“新对话”按钮的同一路径在该项目中新建对话，使首页标题与输入框上方的项目、分支控件显示该项目。 |
| `--thread-hover-card=TITLE` | 不依赖指针直接打开指定任务（按标题或稳定 id）的侧栏悬停卡片，用于悬停卡片截图的静态部分。 |
| `--thread-rename=TITLE` | 不依赖指针直接打开指定任务（按标题或稳定 id）的重命名面板，用于重命名面板截图的静态部分。 |
| `--sidebar-width=PX` | 以持久化宽度渲染侧栏（限制在 240–480）。本机侧栏默认以 240 px 最小宽度打开；与 ChatGPT 采集对比时传入参考端的持久化宽度（用户未拖动时为 275 px）。 |
| `--sidebar-collapsed` | 以收起侧栏且过渡已结束的状态启动：标题栏只保留侧栏按钮，Pull Requests 等页面的头部从它之后开始。 |
| `--display=N` | 在平台显示器列表中的第 `N` 块屏幕上打开窗口，便于分别采集 Retina 面板（DPR 2）与外接 1x 显示器。 |
| `--print-diagnostics` | 打印可执行文件路径、工作目录、编译工作树、bundle 名称与标识、已解析的资源目录及其来源，以及每个资源候选路径与判定，随后退出。用于证明验收运行驱动的是哪一个构建。 |

`GPUI_ASSETS_DIR` 直接把加载器指向某个资源目录，并成为唯一候选，因此填错会明确失败，而不会悄悄改用别处的资源。未设置时的搜索顺序为：bundle 内的 `Contents/Resources/assets`、可执行文件旁的 `assets`、编译工作树、工作目录；只有包含 `icons/` 的目录才算可用。

`--approval-replay=/absolute/fixture.json` 通过生产解析与响应路径回放离线 JSON-RPC。fixture 包含从 `turn/started` 到 item 和审批请求的 `events` 数组，可选 `cwd`、`userMessage`、`assistantMessage` 与 `failWrites`。响应写入相邻 `.responses.jsonl`；回放不执行命令，也不修改被审批文件。

Pull Requests 页面由 `scripts/capture_pull_requests_reference.sh`（参考端，逐主题切换应用外观）和 `scripts/capture_pull_requests_gpui.sh both full`（本机端）采集，再用 `scripts/compare_pull_requests_suite.py` 逐组件打分；`scripts/verify_pull_requests_ux_reference.mjs` 驱动参考端执行验收序列，`scripts/extract_pull_request_icons.mjs` 从参考端 DOM 重新提取页面的内联 SVG 图标到 `assets/icons/`。原始截图、日志与分数保留在 `artifacts/`。

视觉脚本需要 Python 3、Pillow、NumPy 和 websocket-client。CDP 脚本需要支持全局 WebSocket 的 Node.js。设置验证另需 Electron（`npm ci`）和 `jq`。ChatGPT 参考截图只使用专用调试实例，并分配新端口：

```bash
export CHATGPT_CDP_HTTP="http://127.0.0.1:${CAPTURE_CDP_PORT:?Set a dedicated debug port}"
```

所有参考实例都必须显示 Echora 复刻的旧版布局：侧栏直接铺在窗口背景上，最左侧没有图标栏，侧栏和内容区也不嵌在灰色圆角框里。ChatGPT 用每次启动都重新拉取的 Statsig gate `3085093835` 在旧版布局与导航图标栏之间选择，因此新启动的实例，甚至用户自己的 ChatGPT，都可能显示图标栏。图标栏不作为参考。`scripts/launch_chatgpt_reference.sh` 与 `scripts/p0/launch_reference_instance.sh` 在窗口出现后会在内存中固定旧版布局。侧栏没有以 `legacy` 渲染时，它们会停掉实例并以非零状态退出。页面重载会丢失固定，采集前要重新固定，并且只有输出为 `"renderedLayout": "legacy"` 时才能采集：

```bash
node scripts/cdp_pin_reference_layout.mjs --layout=legacy --wait=60
```

这一固定不写入 profile 或 `~/.codex`。`--wait=秒数` 会一直重试，直到窗口、其 Statsig 客户端与所需布局都就绪。`--layout=network` 恢复为拉取到的值。不带 `--layout` 时脚本只报告当前布局；侧栏尚未挂载或处于折叠状态时显示 `unknown`。

| 专项 | 入口 |
|---|---|
| 真实线程双主题 | `python3 scripts/capture_resume_reference.py --endpoint "$CHATGPT_CDP_HTTP" --manifest /path/to/manifest.json`；`python3 scripts/capture_resume_gpui.py --manifest /path/to/manifest.json --output artifacts/resume-alignment/actual` |
| 终端 / 文件 | `node scripts/cdp_capture_terminal.mjs artifacts/terminal`；`node scripts/cdp_capture_file_panel.mjs artifacts/file-panel` |
| 审查 / 侧边聊天 | `node scripts/cdp_capture_review.mjs artifacts/review-reference`；`node scripts/cdp_capture_side_chat.mjs artifacts/side-chat reference` |
| 审查弹层 | `node scripts/cdp_capture_review_menus.mjs --output=artifacts/review-menus` 采集比较范围、查看选项与分支选择三个弹层，双主题并附带计算样式；`python3 scripts/compare_review_menus.py --reference DIR --gpui DIR --output DIR --scale 2` 逐像素比对两侧截图，输出裁切图、差异图与报告 |
| 账户菜单、退出登录 | `node scripts/cdp_capture_account.mjs --output artifacts/account-phase/chatgpt-reference --theme=light`；`scripts/capture_account_gpui.sh`；`python3 scripts/compare_account_phase.py` |
| 设置矩阵 | `CHATGPT_CDP_HTTP="$CHATGPT_CDP_HTTP" node scripts/extract_chatgpt_settings.cjs` 把参考端 18 个设置页（简体中文界面）保存为 `chat-reference/settings/` 下的 HTML 快照，供后续步骤读取；`./node_modules/.bin/electron scripts/verify_chatgpt_settings.cjs`；`REFRESH_SETTINGS_REFERENCES=1 scripts/capture_settings_matrix.sh`；`python3 scripts/verify_settings_matrix.py` |
| 已合并 Phase 1–4 局部组件门禁 | `python3 scripts/stage4/compare_merge_gate.py`（需要 `artifacts/merge-four-worktrees/` 下的专用 ChatGPT/GPUI 截图；每个局部组件阈值为 99%） |
| 侧栏项目悬停卡片 | `CHATGPT_CDP_HTTP="$CHATGPT_CDP_HTTP" node scripts/cdp_capture_project_hover.mjs --output=artifacts/project-hover/reference` 悬停真实行后采集参考卡片（几何、计算样式、图标与截图）；`--project-hover-card=NAME --screenshot=artifacts/project-hover/gpui/light-card.png` 采集本机卡片；`python3 scripts/compare_project_hover.py --reference artifacts/project-hover/reference --gpui artifacts/project-hover/gpui --output artifacts/project-hover/compare` 逐主题打分。`cargo test project_hover` 走与真实悬停相同的指针路径（延迟出现、停留在卡片上保持打开、移开后关闭）。 |
| 侧栏任务悬停卡片 | `scripts/capture_thread_hover_gpui.sh both` 在设置了 `CHATGPT_CDP_HTTP` 时刷新参考采集，用 `--thread-hover-card=TITLE` 采集两个主题，再由 `scripts/compare_thread_hover.py` 逐主题打分：报告 `pixelConsistency`、`pixelsWithin2`、`pixelsWithin12`、仓库统一的 `toleranceAdjustedSimilarity` 以及卡片的纵向锚点偏差。卡片在指针进入项目任务行 240 ms 后出现，指针停留在卡片上时保持打开，与参考一致地对不属于任何项目的“最近”行不显示卡片；`cargo test thread_hover` 走同一指针路径。 |
| 侧栏布局 | `scripts/launch_chatgpt_reference.sh` 启动专用参考实例（独立端口与 profile 克隆，并清掉克隆里指向用户窗口的 `Singleton*` 链接，避免应用把新实例转发进用户自己的窗口；同时带上让被遮挡窗口继续渲染的 Chromium 开关，因为被遮挡的页面不再接收 CDP 输入）；随后 `CHATGPT_CDP_HTTP="$CHATGPT_CDP_HTTP" node scripts/cdp_capture_sidebar_layout.mjs --output artifacts/sidebar-layout/reference` 通过应用自身的 Appearance 控件切换主题，记录侧栏每个 landmark 的几何、计算样式（两个主题）以及整窗与侧栏截图；`--theme=dark --window-width=1440 --window-height=900 --screenshot=artifacts/sidebar-layout/gpui/dark.png` 以同一视口采集本机侧栏，`python3 scripts/compare_sidebar_layout.py --reference …/dark-window.png --gpui …/dark.png --spec …/dark-spec.json --scale 2` 逐个 landmark 与参考对齐并给出逐行偏移。`scripts/cdp_sidebar_row_tree.mjs` 打印参考端某一行元素的盒子树。启动脚本会固定旧版布局（见上文），页面重载后需重新固定。参考端把 `sidebar-width` 与 Appearance 选择保存在 `$CODEX_HOME` 下（`.codex-global-state.json`、`config.toml` 的 `[desktop] appearanceTheme`），因此请用 `CHATGPT_REFERENCE_CODEX_HOME=DIR` 启动，让实例使用自己的 `~/.codex` 克隆；否则采集时切换主题或拖动其侧栏都会同时改变用户自己的 ChatGPT。本机侧用 `--sidebar-width=PX` 对齐参考端的持久化宽度，给 `cdp_capture_sidebar_layout.mjs` 传同样的 `--window=WxH`（只采一个主题时再加 `--theme=`）；对比 DPR 1 与 DPR 2 时，本机窗口用 `--display=N` 打开，参考采集传 `--dpr=2`。 |
| 侧栏活动视图 | `CHATGPT_CDP_HTTP="$CHATGPT_CDP_HTTP" node scripts/cdp_capture_activity_view.mjs --output=artifacts/activity-view-26917/reference` 点击参考应用的铃铛、悬停任务行与铃铛、打开 `…` 菜单并用滚轮滚动（仅在专用实例上设置 `data-theme` 读取浅色与深色，视口模拟为 1470×924），记录截图与各地标几何；`scripts/capture_activity_view_gpui.sh` 以 `--activity-open`、`--activity-hover=TITLE`、`--activity-tooltip=bell`、`--activity-options-open` 与 `--activity-scroll=PX` 基于真实 app-server 数据采集相同状态，`python3 scripts/compare_activity_view.py --reference … --gpui … --output …` 逐地标报告偏移。`cargo test activity` 以真实点击与状态事件驱动铃铛、任务行、菜单与“清除已读聊天”，并经按键绑定验证 `Option+Cmd+U`。 |
| 任务重命名面板 | `scripts/capture_thread_rename_gpui.sh both` 先采集原生侧（参考实例会给打开过的任务留下写者），再按相同设备像素比通过 CDP 重采参考，最后由 `scripts/compare_thread_rename.py` 逐主题打分。脚本按参考期望发出两次点击，记录面板几何、计算样式、真实 DOM 与截图，并用真实任务驱动取消、关闭按钮、蒙层、Esc、Enter 与保存（含 59 字符加省略号的截断）；`cargo test rename_panel` 覆盖同一契约。 |
| 会话用户消息导航轨 | `scripts/capture_user_message_rail_gpui.sh both` 先采集原生侧的静止态与单条悬停态，再以同一视口通过 CDP 重采参考，最后用 `scripts/compare_user_message_rail.py` 逐主题打分。参考侧脚本（`scripts/cdp_capture_user_message_rail.mjs`）通过应用自身的 Appearance 控件切换主题，并用真实指针悬停与点击记录轨道几何、计算样式、各标记宽度、提示卡延迟、预览 DOM 与截图；对比报告给出轨道的 `pixelsWithin2`、悬停态的 `toleranceAdjustedSimilarity`、卡片表面相似度以及两个锚点偏差。`cargo test navigation` 固定标记递减宽度、当前标记规则与预览卡截断点，并用窗口事件与模拟时钟驱动提示轨，固定悬停、关闭宽限、拖动浏览与跳转的时序。动效方面，`scripts/cdp_probe_user_message_rail_motion.mjs` 以固定的指针脚本逐帧记录参考；采集构建用 `--resume-thread=… --user-message-navigation-jump=1 --user-message-rail-motion --screenshot=PATH` 回放同一脚本（写出 `PATH.motion.json`）；`scripts/compare_user_message_rail_motion.py` 对比预览的打开与关闭时间、跳过延迟后的重新打开、标记过渡完成时间、平滑滚动时长、气泡闪烁、拖动浏览时的 `aria-current` 序列、每个标记的预览高度、Alt+方向键的落点以及滚轮路由。 |
| 跟进队列、目标与自动复核批准 | `python3 scripts/batch1_app_server_probe.py --output artifacts/batch1-baseline-<日期>` 以隔离的 `CODEX_HOME` 和本地假 Responses 端点运行本机 `codex app-server`，记录基线下队列、目标、协作模式与批准的行为，不发模型请求；`--scenario guardian_live` 让假审核拒绝一次提权的 `echo` 再批准该拒绝，在真实的 0.158 服务端上观察批准路径。原生侧用 `--queue-ui-state`、`--goal-ui-state`、`--slash-menu-state`、`--auto-review-denial-state` 采集，`python3 scripts/compare_batch1_captures.py OUT ref.png:echora.png:x0,y0,x1,y1[:name] …` 以同一矩形裁剪两侧（不缩放、不平移）并给出相似度。`cargo test followup_tests`、`cargo test goal_tab` 与 `cargo test batch1` 用脚本化后端驱动输入框、「编辑目标」标签与 manager 流程。 |
| 轮次设置、聊天内查找、钩子、实验性功能与记忆 | `python3 scripts/batch2_app_server_probe.py --output artifacts/batch2-baseline-<日期>` 以隔离的 `CODEX_HOME` 和假 Responses 端点记录 0.158 基线下 `turn/settings/update`、`thread/searchOccurrences`（分页、游标、大小写与 UTF-16 范围）、用户层与项目层的 `hooks/list`、`experimentalFeature/list`、`thread/memoryMode/set` 与 `memory/reset` 的行为。专用参考实例运行时，`CHATGPT_CDP_HTTP=http://127.0.0.1:PORT node scripts/cdp_capture_batch2.mjs --output=artifacts/batch2` 把两种主题的参考状态采集到 `artifacts/batch2-<主题>-<日期>/reference/`；`ECHORA_CODEX_HOME=<~/.codex 的副本> scripts/capture_batch2_gpui.sh <日期>` 把对应的原生状态采集到 `…/echora/`，并拒绝使用真实的 `~/.codex`。`cargo test batch2`、`cargo test find_tests` 与 `cargo test memories_tests` 用脚本化后端驱动 manager、设置页、查找栏与 `/memories` 流程。 |
| 代码审查、Shell 命令、提供方能力、记忆状态、已加载线程与自定义分区 | `python3 scripts/batch3_app_server_probe.py --output artifacts/batch3-baseline-<日期>` 以隔离的 `CODEX_HOME` 和假 Responses 端点记录 0.158 基线：`review/start`（含第二个 turn id）、`thread/shellCommand`（失败、超时、中断，以及在运行中的轮次内执行）、各提供方的 `modelProvider/capabilities/read`、`memory/status`、`thread/loaded/list` 分页，以及 `threadSection/update|delete`。`CHATGPT_CDP_HTTP=http://127.0.0.1:端口 node scripts/cdp_capture_batch3.mjs --output=artifacts/batch3` 采集参考状态，不会选中任何审查；`ECHORA_CODEX_HOME=<~/.codex 的副本> scripts/capture_batch3_gpui.sh <日期>` 采集原生状态。`python3 scripts/compare_batch3_captures.py <日期>` 在两侧分别测量面板与文字行；两侧画面对齐时再按同一矩形计算相似度，全程不缩放、不平移。`cargo test batch3` 与 `cargo test sections_tests` 以脚本化后端驱动编解码、manager、输入框、设置、工作区存储与侧栏。 |
| 图像生成 | `python3 scripts/compare_image_generation_component.py --help`，传入实测等尺寸裁切范围和 DPR。 |
| 历史诊断 | `python3 scripts/audit_resume_rendering.py --help`；rollout 仅用于离线诊断。 |

线程 manifest 是包含 `id`、`title`、`slug` 的 JSON 数组。线程 / 设置对照在 1× 显示器使用 1440×900、DPR 1；设置矩阵覆盖 18 页的两种主题。已合并的 Phase 1–4 门禁在固定局部组件范围内，先执行一次有记录的 2×→1× BOX 归一化，再输出裁切图、差异图和分数到 `artifacts/merge-four-worktrees/visual/final-report/`。Git 写操作验收使用临时仓库与本机 bare remote，PR 命令链使用 `gh` 测试替身。

```bash
GPUI_MARKDOWN_BENCH_FILE=docs/APP_SERVER_INTEGRATION.md \
  cargo test markdown_preview_scroll_timings -- --ignored --nocapture
GPUI_DIFF_BENCH_PATCH=/absolute/path/to/long.diff \
GPUI_DIFF_BENCH_OUTPUT=/tmp/gpui-diff-timings.json \
  cargo test long_diff_scroll_timings -- --ignored --nocapture

cargo test -p gpui_macos --lib --features font-kit typography_
cargo test -p gpui_apple --lib compositing_tests
node scripts/cdp_capture_chatgpt_typography.mjs --artifact-dir=artifacts/typography/live
node scripts/cdp_capture_typography_specimen.mjs artifacts/typography 1
"$(scripts/gpui_capture_binary.sh)" \
  --typography-specimen --screenshot=artifacts/typography/gpui-1x.png
python3 scripts/compare_typography.py \
  artifacts/typography/electron-1x.png artifacts/typography/gpui-1x.png \
  --output=artifacts/typography/comparison-1x.json
```

`cargo test live_conversation_stream_timings -- --ignored --nocapture` 测量 500 次实时会话更新与 GPUI 布局耗时，并验证向上滚动后继续输出不会移动阅读位置。

`cargo test streaming_reply` 与 `cargo test -- streaming::tests fade::tests repair::tests` 覆盖流式回复的节奏揭示（节拍、完成时补齐、减少动态效果）、淡入所依赖的分词、时间线与缓动曲线，以及未写完 Markdown 尾部的修复。

`cargo test streaming_highlight_timings -- --ignored --nocapture` 对比 500 次逐步增长的 Rust 代码更新在增量高亮与完整解析下的耗时，并校验高亮区间完全相等。

滚动基准测量 GPUI 测试窗口的事件与布局耗时，不代表屏幕 FPS。字体比较只统计字形像素。2× 验证将 CDP 参数改为 `2`，GPUI 使用真实 Retina 显示器，比较传 `--dpr=2`。半透明对照另存目录：CDP 和比较脚本加 `--translucent`，GPUI 加 `--typography-translucent`，比较工具将两端不同的 alpha 编码合成到同一底色。

</details>

## 接下来的方向

- 在原生 GPUI 中完整复刻 ChatGPT App 的交互体验，包括具体交互细节。
- 依据真实协议与能力，接入更多厂商的 Coding Agent。
- 统一管理本机 Agent 的发现、配置、启动、会话与运行状态。
- 随着 Agent 选择增多，继续保持熟悉的对话与审查流程。
- 按产品需要扩展 Codex 接入范围，并明确兼容边界。

以上是发展方向，并非已经交付的集成或发布日期承诺。参与开发前请阅读 [仓库约定](AGENTS.md) 与 [app-server 接入总表](docs/APP_SERVER_INTEGRATION.md)。行为变化时，请同步维护中英文 README。
