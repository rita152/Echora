# 仓库约定

适用于整个仓库；不设置子目录级 `AGENTS.md` 或 `AGENTS.override.md`。

## 项目背景

Echora 是基于 Rust 与 GPUI 的独立原生桌面 Agent 应用，GUI 完全由 GPT-6-Astra 制作。项目以通过 Codex app-server 完整复刻 ChatGPT App 的交互体验为目标，后续接入其他 coding agent，让用户在同一套熟悉的工作流中使用不同厂商的产品，并统一管理本机 agent 的发现、配置、启动、会话与运行状态。当前只接入 Codex app-server 的部分能力，完整交互复刻尚未完成，其他 agent 尚未接入；扩展以产品需求和实际协议为依据。

## 文档职责

仓库只维护以下四份 Markdown，中英文 README 保持功能、边界和命令一致。记录当前行为、边界和验证入口；完成过程、历史测试数量及逐次验收记录留在本机产物中。

| 文档 | 内容 | 更新时机 |
|---|---|---|
| [README.md](README.md) | 英文项目介绍、运行、功能入口、架构与验证命令 | 依赖、入口、行为、架构或常用命令变化 |
| [README.zh-CN.md](README.zh-CN.md) | 与英文 README 对应的中文说明 | 与英文 README 同步更新 |
| [AGENTS.md](AGENTS.md) | 项目背景、工作约定与文档职责 | 项目定位或仓库约定变化 |
| [docs/APP_SERVER_INTEGRATION.md](docs/APP_SERVER_INTEGRATION.md) | Codex app-server 全量方法及接入状态的唯一总表 | CLI schema、运行时接入或兼容处理变化 |

## 代码边界

- `src/agent/` 领域模块不得依赖 GPUI、界面组件或具体适配器；`mod.rs` 只维护模块与导出。
- Codex 编解码留在 `src/agent/codex/`，与连接生命周期分开维护；通用媒体、文件工具不经适配器导出给 UI。
- 工作区状态、分页加载、偏好持久化分别由 `src/workspace.rs`、`src/workspace/loaders.rs`、`src/workspace/preferences.rs` 负责。
- 会话状态与事件归约在 `src/conversation/`；GPUI Entity、Context 和交互驱动留在视图层。
- 实现与测试按职责维护。删除代码前核对调用方和协议覆盖；不得扩大 `allow(dead_code)` 或 Clippy 抑制范围来消除告警。

## 界面验收

- 修改渲染或交互后，先用 `scripts/package_gpui_capture.sh` 打包当前工作树的验收 bundle：它构建最新可执行文件、把 `assets/` 放进 `Contents/Resources/assets`，并让 bundle 名称与标识带上本工作树 slug。
- 验收实例必须来自当前工作树：在仓库根目录用绝对路径启动该 bundle 内的可执行文件，不要用 `open -n "target/GPUI Capture.app"`（会按 LaunchServices 解析到其他构建目录的同名旧副本）。`--print-diagnostics` 会打印可执行文件、bundle 身份与每个资源候选的判定，启动前先核对；缺少 `assets/icons` 时应用会向 stderr 告警并在窗口顶部显示红色条，不得把这种截图当作验收结果。`scripts/launch_project_hover_instance.sh` 是带上述约束的启动示例。
- Computer Use 先枚举应用并连接 `GPUI Capture`，再通过可访问性树和截图定位，复现修复前行为并验证修复结果。交互问题须检查完整命中区域及相关点击、滚动、拖动、键盘、悬停或文本选择路径。
- 前后使用相同主题、窗口尺寸、DPR、线程、内容及滚动位置；不得缩放或平移截图提高对比分数。
- ChatGPT 参考采集使用专用调试实例及新建端口；不占用其他任务的调试端口，不操作用户正在运行的 ChatGPT 或 GPUI 实例。
- 结束后只关闭本次专用实例；运行与改动相称的格式、测试和编译检查，交付时说明实际验证范围与结果。原始截图、日志和对比数据保存在 `artifacts/`；README 展示图片保存在 `docs/images/`。
