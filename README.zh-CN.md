<h1 align="center">Echora</h1>

<p align="center"><strong>与 Codex 协作的原生桌面工作区。</strong></p>
<p align="center">Rust + GPUI · 由 Codex app-server 驱动</p>
<p align="center"><a href="README.md">English</a> · <strong>简体中文</strong></p>

Echora 将对话、本地项目、文件、终端、网页浏览和代码审查集中在一个桌面应用中。界面参考 ChatGPT App，以对话为中心，让需要的工具随时在旁边打开。

Echora 是一个处于早期开发阶段的独立应用，目前通过本机安装的 Codex CLI 接入 Codex。

![Echora 深色主题工作区](docs/images/workspace-dark.png)

<details>
<summary>浅色主题</summary>

![Echora 浅色主题工作区](docs/images/workspace-light.png)

</details>

*截图采集自当前原生构建，对话、文件和服务器列表均使用示例内容。部分界面标签仍沿用 Codex 名称。*

## 与 Agent 协作

在本地项目中开始对话，添加上下文，实时查看 Agent 的回复和工具活动。计划、命令、搜索与文件变更会显示在对话中，需要权限的操作则通过审批卡片交由你确认。

- 添加文件、文件夹、图片、技能、插件、本地聊天和应用内浏览器页面作为上下文。 可将文件或 macOS 截图浮窗拖入对话区域添加附件；未保存截图会在本地接收并显示图片预览。打开尚未发送的图片附件后，可选择**标注**，使用画笔、形状、文本、橡皮擦及撤销/重做。左侧笔画宽度滑块支持点击、拖动、滚轮及聚焦后的方向键调节；保存会以原始图片尺寸替换该草稿附件，不修改源文件。
- 选择 Codex 配置所提供的权限选项。新聊天沿用最近确认的选择；后端重连后自动刷新权限选项。
- 在 Agent 工作时继续补充指令，或将后续消息加入队列。
- 在同一项目或分组点击**新聊天**可返回尚未发送的草稿。切换聊天时，Prompt 与附件保留在内存中；标题栏的前进/后退也可返回原会话。
- 在后端支持时，通过斜杠菜单使用计划模式、设置目标或发起代码审查。
- 停止当前轮次、继续已中断的对话、查找聊天内容，并在各条提问之间跳转。
- 在右上角窗口预览中查看 Computer Use 的当前操作、进度和最近操作记录。macOS 实时预览需要屏幕录制权限；未授权时显示最近的工具截图并标明来源。「接管」会先停止 agent，等待确认后再切换到目标应用。收起预览或离开聊天会停止实时采集。
- 打开临时侧边聊天，在主对话旁探索另一个问题。

## 整理项目与对话

从已有文件夹创建本地项目，或新建一个 Git 工作区。在发送第一条消息前，可以选择项目，并决定使用本地检出目录还是新建工作树。

侧栏集中管理项目和对话，支持搜索、重命名、置顶、归档、删除、移动聊天，以及通过自定义分区整理内容。活动视图汇总进行中的聊天、等待你处理的请求和未读结果。切换聊天时，后台任务会继续运行。

## 在对话旁使用工具

右侧面板支持多个标签页，可以同时保留文件、终端、网页和差异视图。拖动标签调整顺序，或展开当前工具以获得更多空间。

| 工具 | 可以做什么 |
|---|---|
| **文件** | 浏览和搜索本地文件，编辑文本并自动保存，预览 Markdown 和图片，通过文件链接跳转到指定行。Markdown 预览支持 HTML 标题、对齐段落、强调、链接及可展开的折叠区域；相对链接和图片按文档目录解析，不执行脚本或嵌入应用。 |
| **终端** | 在对话的工作目录中运行本地 Shell，支持多个标签页、历史输出、文本选择和剪贴板操作。 |
| **浏览器** | 使用内置 WebKit 浏览器访问网页，通过地址与历史建议导航，页内查找、管理下载，并将页面内容附加到聊天。 |
| **变更** | 以统一或分栏视图查看 Git 差异，添加行评论，暂存或取消暂存变更，还原文件或代码块，提交、创建分支、推送和创建拉取请求。 |
| **侧边聊天** | 在临时对话中探索问题，独立选择权限，输入消息或停止生成。 |

环境面板提供工作位置、分支、Git 操作、已关联拉取请求、本地服务和后台终端的快捷入口。独立的「拉取请求」页面支持浏览和筛选 GitHub 拉取请求，阅读动态与检查结果，查看提交和差异，并对代码发表评论。

<details>
<summary>查看文件编辑与 Git 变更截图</summary>

**本地文件浏览与编辑**

![对话旁的本地文件树与 Rust 文件编辑器](docs/images/product-files.png)

**Git 差异审查**

![变更面板中展示的示例项目代码差异](docs/images/product-changes.png)

</details>

## 调整你的工作区

Echora 支持浅色和深色主题，以及英文、简体中文和自动语言识别。可以在 **设置 → 通用 → 语言** 中切换界面语言。

侧栏**插件**入口打开独立目录，提供公开、个人和已安装视图、即时搜索、分类条目、插件详情及确认后的安装／卸载操作。技能标签读取 agent 可用的技能。目录、图标、权限、加载状态和错误均来自 Codex app-server，不补造 ChatGPT 专属目录或私有服务内容；返回聊天会恢复该会话的草稿和面板。

设置中还可访问已接入的 Codex 偏好、技能、插件、MCP 服务器状态与 OAuth 登录、钩子、实验性功能和记忆选项。可用功能取决于所连接的账号、Codex 版本及配置。

<details>
<summary>查看 MCP 与技能管理截图</summary>

**MCP 服务器状态**

![MCP 管理页中的示例服务器连接状态](docs/images/product-mcp.png)

**技能列表**

![技能管理页中的示例技能与启用开关](docs/images/product-skills.png)

</details>

## 开始使用

当前开发与原生交互验证平台为 **macOS**。从源码构建和运行应用需要：

- [rust-toolchain.toml](rust-toolchain.toml) 指定的 Rust 工具链，当前为 **1.97.1**。
- Xcode 及其 **Metal Toolchain** 组件，并将 Xcode 设为当前开发者目录。
- 位于 `PATH` 中、已完成配置且可正常使用的 Codex CLI。当前接入基线为 **codex-cli 0.158.0**。
- 仓库功能需要 Git；拉取请求功能还需要已登录的 GitHub CLI（`gh`）。

```bash
git clone https://github.com/rita152/Echora.git
cd Echora
cargo run --locked --release -- --theme=dark
```

如果缺少 Metal 编译器，请在构建前安装并检查：

```bash
xcodebuild -downloadComponent MetalToolchain
xcrun --sdk macosx metal --version
```

使用 `--theme=light` 启动浅色主题。添加 `--language=en`、`--language=zh-CN` 或 `--language=auto` 可指定本次启动的界面语言；在设置中选择的语言会保留到后续启动。

从 [GitHub Releases](https://github.com/rita152/Echora/releases) 下载开发预览版：

| 平台 | 安装包 | 安装方式 |
| --- | --- | --- |
| macOS Apple Silicon（arm64） | `Echora-<version>-macos-arm64.dmg` | 打开磁盘映像，将 Echora 拖入“应用程序”。 |
| Windows x86_64 | `Echora-<version>-windows-x86_64-setup.exe` | 运行 Setup，安装到当前用户目录，并创建开始菜单入口和卸载程序。 |
| Linux x86_64，Ubuntu 24.04 或兼容的新发行版 | `Echora-<version>-linux-amd64.deb` | 使用软件安装器打开，或运行 `sudo apt install ./Echora-<version>-linux-amd64.deb`，然后从应用菜单启动。 |

安装包已包含应用资源，Windows 还包含应用私有的 MSVC 运行库。仍需另行安装并配置 Codex CLI。开发版安装包尚未经过受信任的代码签名或 macOS 公证，系统可能要求确认后才能打开。当前仅在 macOS 上进行原生交互验证；CI 在对应平台的 runner 上检查安装、资源发现和卸载，不代表完整的 Windows/Linux GUI 验收。

`.tar.gz` 是可执行文件与资源的便携压缩包，不是安装程序；使用时请让可执行文件与 `assets/` 目录保持相邻。可执行文件目前名为 `gpui-chat-clone`。`SHA256SUMS` 覆盖全部安装包和便携压缩包。GitHub 的 **Source code** 下载项为源码，不是安装后的应用。主分支和拉取请求的构建仍在 [Actions 页面](https://github.com/rita152/Echora/actions/workflows/release.yml)保留 14 天。

### 发布版本

版本号遵循 SemVer：从 `v0.1.0-alpha.1`、`v0.1.0-alpha.2` 开始，按需推进到 beta/rc 预览版，稳定后发布 `v0.1.0`。带预发布后缀的版本会自动标记为 GitHub **Pre-release**，不标记为 Latest。

更新根目录 `Cargo.toml` 的包版本及 `Cargo.lock` 中对应的条目，提交并推送，然后推送匹配的 `v` 标签（例如先执行 `git tag v0.1.0-alpha.2`，再执行 `git push origin v0.1.0-alpha.2`）。工作流会校验版本、编译三平台、打包并检查原生安装包，仅在全部任务成功后创建 GitHub Release，附上全部下载项和校验文件。在 GitHub 网页发布 Release 也会触发编译并附上相同产物；其标签必须与标签所指源码中的版本一致。构建失败后可以重新运行工作流。如需用更新的打包工具重建已有版本，从 `main` 手动触发工作流并将 `release_tag` 设为已有标签（例如 `gh workflow run release.yml --ref main -f release_tag=v0.1.0-alpha.1`）。应用源码和资源取自该标签，打包工具取自工作流提交，仅更新 Release 附件，不移动标签。

## 当前支持范围

- **目前只接入 Codex。** Echora 启动本机 Codex app-server，使用其配置和对话历史。
- **ChatGPT 的交互体验尚未完整实现。** 部分导航和设置入口仍为占位；定时任务、远程主机和 ChatGPT 云端工作流尚未接入。
- **浏览器用于浏览网页和分享页面上下文。** 暂不支持 Agent 操控内置浏览器，也不支持 Chrome 扩展。
- **部分状态仅在本次运行中保留。** 应用退出后，终端会话和侧边聊天随之结束。活动状态来自 Echora 自身连接所观察到的轮次，在其他客户端运行的任务可能不会在这里显示为进行中。

Echora 并非 OpenAI 官方产品。详细的后端接入范围与兼容说明见 [Codex app-server 接入文档](docs/APP_SERVER_INTEGRATION.md)。
