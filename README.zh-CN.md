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

两张图片均由当前工作树打包的验收 bundle（`scripts/package_gpui_capture.sh`）实机截取，使用固定的对话与工具活动示例，不执行图中命令，也不发送模型请求。界面目前仍保留部分 Codex 字样。图片为实际应用截图，并非设计稿。

## 现在可以做什么

| 工作流 | 当前能力 |
|---|---|
| **开始对话** | 空白会话显示项目标题和输入框，不显示占位建议。 |
| **项目与会话** | 创建、恢复、搜索、重命名、归档、删除、移动和置顶会话；切换会话时保留后台轮次。 |
| **边运行边沟通** | 流式显示回复，向当前活动轮次追加输入；在时间线中查看计划、搜索、等待、工具活动与文件改动。 |
| **审批操作** | 通过原生卡片检查命令、文件和附加权限请求，查看自动复核结果，选择服务端支持的权限配置。 |
| **MCP 请求输入** | 以原生 form、url 卡片响应 `mcpServer/elicitation/request`：校验必填、类型、范围与选项，接受才提交结构化内容，跳过／取消分别对应协议动作，并等待 `serverRequest/resolved` 后收束。 |
| **处理文件** | 浏览本地文件树、筛选路径、多标签编辑、预览 Markdown 和图片，以及通过文件链接定位到行。 |
| **使用终端** | 在会话目录运行本机 shell，支持多标签、回看、文字选择和剪贴板。 |
| **审查与交付** | 查看 Git diff、逐行评论、暂存、还原、提交、创建分支、推送，并通过本机 `gh` 创建 PR。 |
| **浏览 Pull Request** | 打开侧边栏 `Pull requests` 页面：列表与过滤、Summary、Activity、提交与检查、带文件树的 diff、行内评论，以及从变更统计按钮打开的 Review 标签页。 |
| **侧边探索** | 从主会话派生临时对话，分别控制输入、模型、权限与停止操作。 |
| **配置 Codex** | 读取有效配置与来源，查看受管限制，编辑已支持的用户层设置，并通过后端回读核验保存结果。 |
| **选择语言** | 在设置 → 常规 → 语言中切换 English、简体中文或自动检测；切换立即生效并在本地保存。 |
| **账户** | 账户菜单与登录流程都由连接级账户快照驱动：缺失的套餐显示为未知而不是杜撰；登录保留服务端返回的 `loginId` 直到完成通知到达；退出登录先确认再发请求。只提供 Codex 管理的 ChatGPT 登录，API key、外部 token 与 Bedrock 变体返回明确错误。 |
| **管理账户** | 在账户菜单查看当前 ChatGPT 账户与套餐；通过 Codex 管理的 ChatGPT 登录、取消进行中的登录，并在确认后退出登录。 |
| **管理技能与 MCP** | 读取技能目录并按技能启用／禁用并核验回执；列出 MCP 服务器的状态、认证、工具与服务端扩展字段；重新加载服务器；完成 OAuth 登录并区分等待、成功、失败、取消与断连状态。 |
| **管理插件与应用** | 从后端读取插件目录与已安装子集，跨 marketplace 搜索，打开插件自身详情（描述、技能、MCP 服务器），经确认后安装／卸载，管理 marketplace（添加、更新、移除），读取已共享的插件与插件技能内容。目录行、分段徽标与计数一律使用服务端返回值；服务端没有提供图标或描述的插件就按原样呈现，不伪造占位内容。 |

具体协议覆盖与兼容规则以 [app-server 接入总表](docs/APP_SERVER_INTEGRATION.md) 为准。可见的界面入口不代表对应厂商能力已经完整接入。

## 快速开始

当前开发与验收平台为 **macOS**。需要 [rust-toolchain.toml](rust-toolchain.toml) 固定的 Rust 工具链、macOS 构建工具，以及已安装、已登录且位于 `PATH` 的 Codex CLI。当前接入基线为 `codex-cli 0.154.0`，更换 CLI 版本前请核对接入总表。

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

侧边栏导航显示新对话、拉取请求、已安排和插件。

| 操作 | 入口 / 快捷键 |
|---|---|
| 打开会话 | 侧栏项目、最近、归档或搜索 |
| 查看项目信息 | 悬停侧栏项目行：卡片显示项目名、任务数、仓库、工作目录和“编辑项目” |
| 查看任务信息 | 悬停项目下的侧栏任务行：卡片显示任务标题、环境图标与距今时间，以及任务所属项目 |
| 搜索聊天 | 侧边栏搜索按钮 → 历史会话搜索弹窗（`Enter` 打开、`⌘1`–`⌘9` 选择、`Esc` 关闭） |
| 切换终端 | 右侧面板 → 终端；`Ctrl+反引号` |
| 打开文件 | 右侧面板 → 文件；`Cmd+P` |
| 打开 Git 审查 | 右侧面板 → 审查；`Ctrl+Shift+G` |
| 打开侧边聊天 | 右侧面板菜单；`Option+Cmd+S` |
| 打开设置 | 账户菜单 → 设置；`Cmd+,` |
| 发送 / 向活动轮次追加输入 | `Enter`；`Shift+Enter` 换行 |
| 立即保存文件 | `Cmd+S` |
| 终端清屏 | `Cmd+K` |
| 切换 / 关闭面板标签 | `Ctrl+Tab`、`Ctrl+Shift+Tab`；`Cmd+W` |

<details>
<summary>行为细节与当前边界</summary>

- **运行中追加输入：** 使用 `turn/steer` 立即发送到活动轮次，不提供服务端消息队列。失败输入可连同附件与审查评论恢复，不覆盖后续新草稿，也不自动改发为新轮次。
- **实时输出与历史：** 实时与恢复后的已完成轮次共用最终答复选择、工作过程折叠、答复操作与文件汇总。文本增量保留 item 身份，完成消息快照校正显示内容；相邻增量以 8 ms 窗口批处理，代码高亮复用已完成行，仅重解析未结束行。已完成轮次折叠最终答复之前的过程消息，追加的用户消息保留原有位置与附件。文件变更按路径汇总并保留原始 patch。历史由 app-server 提供，不伪造缺失的时间、计划步骤快照或自动复核历史。
- **审批：** 并发请求依次显示，提交后等待服务端释放。响应失败可见且不可重复提交。可查看原始请求补丁，展开、选择和复制长命令。使用 Tab / 方向键导航、Enter 激活、Esc 关闭或拒绝；文件审批的 `Shift+Esc` 拒绝并停止轮次。
- **权限：** 读取服务端 profile 全部页面，展示禁用选项及原因。菜单隐藏内置 `:read-only` profile，不显示后续轮次提示和手动重新读取入口。已有线程等待 RPC 成功与匹配的设置通知后才显示生效，更新影响后续轮次。完整访问权限需要应用内确认，侧边聊天独立管理权限。线程被另一个 app-server 占用时，警告卡片固定在输入框上方，不随会话滚动。
- **配置：** 使用带版本的 `config/batchWrite` 保存并回读，项目层与受管层只读。冲突保留草稿，结果未知时不自动重试。保存的模型、推理强度、服务等级与个性默认值用于后续线程，不热更新已打开的线程。
- **活动：** 计划支持流式更新、步骤进度、复制、显式下载和只读文件标签；搜索保留查询与结果，等待保留时长与状态。Hook 反馈只读。自动复核详情支持键盘操作和文字选择，遵循减少动态效果设置。认证恢复与弃用提示的展示边界见接入总表。
- **文件编辑：** 停止输入约 400 ms 后自动保存，撤销 / 重做也写回磁盘。保留 UTF-8 BOM、CRLF 和权限，保存前检查外部修改。文本上限 2 MiB，单行上限 64 KiB；仅访问本机文件。
- **Git 审查：** 范围包括上一轮、未提交、未暂存、已暂存、已提交和分支，分支使用 merge-base。支持统一 / 拆分差异、文字差异、上下文展开和逐行评论。写入前校验 worktree 与 index；还原新增文件时，在 worktree Git 目录的 `gpui-discarded/` 下保留备份。
- **Pull requests：** 页面通过已认证的 `gh` 读写 GitHub。筛选、审阅者搜索、摘要、活动、检查与提交范围均使用 GitHub 数据。支持分栏/统一差异、词级高亮、Markdown 预览、文件树导航及行内评论；文件链接打开 GitHub 上所选提交的内容。写入失败保留草稿，提交中防止重复请求，成功后从 GitHub 刷新。“Draft description in chat” 打开预填的会话，发送前可检查内容。窄窗口在列表与详情之间切换，提供返回按钮。
- **面板生命周期：** 收起面板或切换会话保留状态，shell 与临时侧边聊天不跨应用退出恢复。标签可拖动排序，有消息的侧边聊天关闭前需要确认。连接失效后仍可查看和复制消息。
- **聊天搜索：** 弹窗先列出置顶聊天，再按最近顺序补足，最多九行；输入后经 app-server `thread/search` 检索。参考实现还会通过自身的检索服务合并 ChatGPT 云端会话，app-server 不提供该数据，因此命中较多时结果集合与排序可能不同。`Search files`（或 `⌘P`）把同一弹窗切到文件搜索：为当前会话工作目录打开一个 `fuzzyFileSearch` 会话，边输入边接收 `sessionUpdated` 结果，按服务端返回的下标高亮命中，选中后在文件面板打开。服务端不支持会话时回退到一次性 `fuzzyFileSearch` 请求。
- **改写消息：** 最新一条用户消息的悬停操作里提供编辑入口。提交改写后的文本会以该轮作为 `beforeTurnId` 调用 `thread/revert`，把持久化历史替换为该轮之前的前缀，随后发起新的 `turn/start`。只改会话历史，不动本地文件；轮次仍按既有分页路径重载。
- **压缩上下文：** 在 composer 输入 `/compact` 会执行 `thread/compact/start`。压缩按不可 steer 的轮次运行，期间追加输入会如实展示服务端结论，压缩结果沿用既有 contextCompaction 条目展示。

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
| [src/workspace.rs](src/workspace.rs)、[src/workspace/](src/workspace/) | 工作区状态与通知合并；`loaders.rs` 负责分页，`preferences.rs` 原子保存 UI 偏好。 |
| [src/conversation/](src/conversation/) | 会话状态、事件归约、流式批处理与历史恢复，不持有 GPUI Entity 或 Context。 |
| [src/configuration.rs](src/configuration.rs) | 配置草稿、保存回执与回读核验，使用 `src/agent/config.rs` 的领域类型。 |
| [src/i18n.rs](src/i18n.rs)、[src/i18n/](src/i18n/) | UI 语言选择、系统区域检测及应用文案翻译；不依赖 GPUI 或具体适配器。 |
| [src/components/](src/components/) | Composer、时间线、审批、文件、终端、审查与侧边聊天的渲染和交互。 |
| [src/git_review.rs](src/git_review.rs)、[src/git_review/](src/git_review/) | Git/gh 操作、diff、版本校验、进程回收与评论，不依赖 GPUI 或具体 Agent 适配器。 |
| [src/app.rs](src/app.rs)、[src/app/](src/app/) | 服务装配、会话 host、面板挂载、项目创建与图片预览。 |
| [src/settings/](src/settings/)、[src/media.rs](src/media.rs)、[src/typography.rs](src/typography.rs) | 设置页面、通用媒体工具、字体与字体验收。 |

macOS 的 UI 偏好默认保存到 `~/Library/Application Support/GPUI/ui-preferences.json`，可用 `GPUI_UI_PREFERENCES_PATH` 指定验收专用文件。后端配置不写入 UI 偏好。

GPUI 依赖固定在 [Cargo.toml](Cargo.toml) 的同一 Zed revision。`vendor/gpui`、`vendor/gpui_macos` 和 `vendor/gpui_apple` 保留文本、选择、虚拟列表与 Metal 合成修正，升级时需一并复核。OpenAI Sans 从本机已有 ChatGPT 安装读取，缺失时回退系统字体；仓库不分发该字体。

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

脚本以 `--features screenshot` 构建，产出 `target/GPUI Capture (<worktree>-<digest>).app`，把 `assets/` 复制进 `Contents/Resources/assets`，并打印 bundle 路径、名称与标识。随后在仓库根目录用绝对路径启动该 bundle 内的可执行文件：

```bash
GPUI_UI_PREFERENCES_PATH="$PWD/artifacts/capture-preferences.json" \
GPUI_CAPTURE_OUTPUT="$PWD/artifacts/frame.png" \
  'target/GPUI Capture (gpui-<digest>).app/Contents/MacOS/gpui-chat-clone' \
  --theme=light --window-width=1440 --window-height=900
```

Computer Use 先枚举应用，连接打包脚本打印的那个名字，读取可访问性树和截图，再定位操作。按 `Cmd+Shift+F12` 保存未缩放 PNG 与 `.render.json`（含视口、DPR、可执行文件、bundle 标识与已解析的资源目录），应用继续运行。结束时只关闭本次启动的专用实例。

过去两个 bundle 无法区分：各工作树共用同一个名称与 `com.openai.gpui-chat-clone.capture` 标识，`open -n "target/GPUI Capture.app"` 会按 LaunchServices 解析到已注册的那一份，可能是另一个构建目录里的旧副本——它的工作树没有 `assets/`，进程照常启动、文字照常渲染，但所有 SVG 都是空白。现在打包产物的名称与标识都带本工作树 slug，资源随 bundle 一起分发，`--print-diagnostics` 会打印可执行文件、bundle 身份以及每个资源候选路径与判定，验收运行可以自证在驱动哪一个构建。需要让 bundle 从别处读取资源时，用 `GPUI_ASSETS_DIR` 硬覆盖搜索。若所有候选都不可用，应用继续运行、向 stderr 告警，并在窗口顶部显示写明已尝试路径的红色条，截图不可能再悄悄呈现无图标的界面。`scripts/launch_project_hover_instance.sh` 使用同一套打包身份，会在本工作树的悬停验收实例已在运行时拒绝重复启动，并打印二进制路径、pid 与日志。

自动恢复会话截图可追加 `--resume-thread=THREAD_ID --screenshot="$PWD/artifacts/resumed-thread.png"`。ID 接受原始 UUID 或 `local:<uuid>`。可选 `--resume-scroll-from-bottom=3200` 指定距底部的滚动距离，省略则停在底部。应用等待历史、侧栏、模型目录与三个稳定绘制帧后截图退出，失败或超时返回非零状态。请选择未被其他活动写入进程占用的线程。

真实实时轮次采集：启动时传入 `--capture-live-turn="$PWD/artifacts/live.png"`，再在验收实例输入提示词。完成后保存尚未重新加载历史的实时画面，以及包含线程身份、活动数据的 `.json` 侧文件，然后退出。中断、失败或超过五分钟返回失败。

窗口尺寸为逻辑像素，PNG 分辨率取决于显示器 DPR。对照使用相同主题、内容、窗口尺寸、DPR 与滚动位置，不得缩放或平移图片。旧 `scripts/compare_all.sh` 会缩放截图，不作为像素验收入口。原始截图、日志和对比数据留在 `artifacts/`，README 展示图片放在 `docs/images/`。

<details>
<summary>更多截图与专项验证入口</summary>

完整启动参数见 [src/main.rs](src/main.rs)。

| 参数 | 用途 |
|---|---|
| `--markdown-file=/absolute/path/to/sample.txt` | 独立 Markdown 窗口，无需 app-server；可搭配 `--window-width=480` 检查窄窗。 |
| `--file-panel-root=/absolute/workspace --open-file=/absolute/file` | 真实文件编辑、保存与冲突检查；使用专用测试文件。 |
| `--review-root=/absolute/repository --review-filter=src/example.rs [--review-menu=scope\|options\|branch]` | 真实 Git 审查；截图等待 diff 就绪。加上 menu 参数会打开其中一个审查弹层，便于固定状态截图。 |
| `--settings-page=appearance` | 直接打开设置页，页面列表见 `src/settings/mod.rs`。 |
| `--language=en\|zh-CN\|auto` | 指定本次启动的 UI 语言；截图验收时明确指定，以保证可复现。 |
| `--chat-search-state=initial\|selected\|hover\|query\|no-match` | 以固定状态打开历史会话搜索弹窗用于截图；配合 `--chat-search-query=` 与 `--chat-search-index=`。 |
| `--image-generation-ui-state=running/completed/failed/load-error` | 固定图像生成状态；完成态另传 `--image-generation-path=/absolute/image.png`。 |
| `--auto-approval-ui-state=inProgress/approved/denied/timedOut/aborted/strict/warning` | 自动复核，支持 `--auto-approval-expanded`、`--auto-approval-details-expanded` 与 `--reduce-motion`；长说明和动态采样使用 `--auto-approval-rationale-file`、`--auto-approval-motion-output`。 |
| `--runtime-ui-state=completed/running/turnless/auth-started/auth-completed/interrupted/disconnected/history/long/deprecation` | 确定性 Hook、hookPrompt、认证与应用提示，不执行 Hook 或模型请求；`GPUI_RUNTIME_AUDIT_OUTPUT` 输出原始状态与本地收束原因。 |
| `--progress-ui-state=running/streaming/completed/interrupted` | 计划、搜索与等待归约；streaming 定时产生更新和完成，running 可中断。 |
| `--typography-specimen --typography-display=N` | 字体样本与显示器选择，见 `src/typography.rs`。 |
| `--pull-requests [--pull-requests-select=N \| --pull-requests-title=TEXT] [--pull-requests-tab=code\|review] [--pull-requests-list-tab=all\|reviewing\|authored] [--pull-requests-status=open\|merged\|closed\|all] [--pull-requests-search=TEXT] [--pull-requests-file-tree] [--pull-requests-scroll=px] [--pull-requests-action=...] [--pull-requests-comment-menu]` | Pull Requests 页面确定性状态：列表、标签、搜索、过滤、分组、详情分节、diff、文件树、Review 标签，以及 `scripts/capture_pull_requests_gpui.sh` 使用的交互状态。 |
| `--project-hover-card=NAME` | 不依赖指针直接打开指定项目（按名称或稳定 id）的侧栏悬停卡片，用于悬停卡片截图的静态部分。 |
| `--thread-hover-card=TITLE` | 不依赖指针直接打开指定任务（按标题或稳定 id）的侧栏悬停卡片，用于悬停卡片截图的静态部分。 |
| `--print-diagnostics` | 打印可执行文件路径、工作目录、编译工作树、bundle 名称与标识、已解析的资源目录及其来源，以及每个资源候选路径与判定，随后退出。用于证明验收运行驱动的是哪一个构建。 |

`GPUI_ASSETS_DIR` 直接把加载器指向某个资源目录，并成为唯一候选，因此填错会明确失败，而不会悄悄改用别处的资源。未设置时的搜索顺序为：bundle 内的 `Contents/Resources/assets`、可执行文件旁的 `assets`、编译工作树、工作目录；只有包含 `icons/` 的目录才算可用。

`--approval-replay=/absolute/fixture.json` 通过生产解析与响应路径回放离线 JSON-RPC。fixture 包含从 `turn/started` 到 item 和审批请求的 `events` 数组，可选 `cwd`、`userMessage`、`assistantMessage` 与 `failWrites`。响应写入相邻 `.responses.jsonl`；回放不执行命令，也不修改被审批文件。

Pull Requests 页面由 `scripts/capture_pull_requests_reference.sh`（参考端，逐主题切换应用外观）和 `scripts/capture_pull_requests_gpui.sh both full`（本机端）采集，再用 `scripts/compare_pull_requests_suite.py` 逐组件打分；`scripts/verify_pull_requests_ux_reference.mjs` 驱动参考端执行验收序列。原始截图、日志与分数保留在 `artifacts/`。

视觉脚本需要 Python 3、Pillow、NumPy 和 websocket-client。CDP 脚本需要支持全局 WebSocket 的 Node.js。设置验证另需 Electron（`npm ci`）和 `jq`。ChatGPT 参考截图只使用专用调试实例，并分配新端口：

```bash
export CHATGPT_CDP_HTTP="http://127.0.0.1:${CAPTURE_CDP_PORT:?Set a dedicated debug port}"
```

| 专项 | 入口 |
|---|---|
| 真实线程双主题 | `python3 scripts/capture_resume_reference.py --endpoint "$CHATGPT_CDP_HTTP" --manifest /path/to/manifest.json`；`python3 scripts/capture_resume_gpui.py --manifest /path/to/manifest.json --output artifacts/resume-alignment/actual` |
| 终端 / 文件 | `node scripts/cdp_capture_terminal.mjs artifacts/terminal`；`node scripts/cdp_capture_file_panel.mjs artifacts/file-panel` |
| 审查 / 侧边聊天 | `node scripts/cdp_capture_review.mjs artifacts/review-reference`；`node scripts/cdp_capture_side_chat.mjs artifacts/side-chat reference` |
| 审查弹层 | `node scripts/cdp_capture_review_menus.mjs --output=artifacts/review-menus` 采集比较范围、查看选项与分支选择三个弹层，双主题并附带计算样式；`python3 scripts/compare_review_menus.py --reference DIR --gpui DIR --scale 2` 逐像素比对两侧截图，输出裁切图、差异图与报告 |
| 账户菜单、退出登录 | `node scripts/cdp_capture_account.mjs --output artifacts/account-phase/chatgpt-reference --theme=light`；`scripts/capture_account_gpui.sh`；`python3 scripts/compare_account_phase.py` |
| 设置矩阵 | `./node_modules/.bin/electron scripts/verify_chatgpt_settings.cjs`；`REFRESH_SETTINGS_REFERENCES=1 scripts/capture_settings_matrix.sh`；`python3 scripts/verify_settings_matrix.py` |
| 已合并 Phase 1–4 局部组件门禁 | `python3 scripts/stage4/compare_merge_gate.py`（需要 `artifacts/merge-four-worktrees/` 下的专用 ChatGPT/GPUI 截图；每个局部组件阈值为 99%） |
| 侧栏项目悬停卡片 | `CHATGPT_CDP_HTTP="$CHATGPT_CDP_HTTP" node scripts/cdp_capture_project_hover.mjs --output artifacts/project-hover/reference` 悬停真实行后采集参考卡片（几何、计算样式、图标与截图）；`--project-hover-card=NAME --screenshot=artifacts/project-hover/gpui/light-card.png` 采集本机卡片；`python3 scripts/compare_project_hover.py --reference artifacts/project-hover/reference --gpui artifacts/project-hover/gpui --output artifacts/project-hover/compare` 逐主题打分。`cargo test project_hover` 走与真实悬停相同的指针路径（延迟出现、停留在卡片上保持打开、移开后关闭）。 |
| 侧栏任务悬停卡片 | `scripts/capture_thread_hover_gpui.sh both` 在设置了 `CHATGPT_CDP_HTTP` 时刷新参考采集，用 `--thread-hover-card=TITLE` 采集两个主题，再由 `scripts/compare_thread_hover.py` 逐主题打分：报告 `pixelConsistency`、`pixelsWithin2`、`pixelsWithin12`、仓库统一的 `toleranceAdjustedSimilarity` 以及卡片的纵向锚点偏差。卡片在指针进入项目任务的行动 240 ms 后出现，指针停留在卡片上时保持打开，与参考一致地对不属于任何项目的"最近"行不显示卡片；`cargo test thread_hover` 走同一指针路径。 |
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
'$(scripts/gpui_capture_binary.sh)' \
  --typography-specimen --screenshot=artifacts/typography/gpui-1x.png
python3 scripts/compare_typography.py \
  artifacts/typography/electron-1x.png artifacts/typography/gpui-1x.png \
  --output=artifacts/typography/comparison-1x.json
```

`cargo test live_conversation_stream_timings -- --ignored --nocapture` 测量 500 次实时会话更新与 GPUI 布局耗时，并验证向上滚动后继续输出不会移动阅读位置。

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
