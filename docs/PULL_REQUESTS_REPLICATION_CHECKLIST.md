# Pull Requests 页面复刻清单

（只描述「可点击项 → 触发行为 → 进入什么页面」，不含任何样式描述。用 CDP 在 ChatGPT/Codex Electron App 上真实点击验证。）

## 复刻要求（验收标准）

> 本节为总体验收要求；下文 §0–§6 的清单即这些要求所必须覆盖的交互范围。

- **R1 样式来源与像素保真度**：UI 样式必须通过 **远程调试端口（CDP）打开 ChatGPT/Codex Electron App**，从运行中的真实界面提取（DOM 结构、计算样式、图标/资源等）作为唯一参考来源，禁止凭截图目测。
  - 与 App **完全 1:1 复刻**。
  - 量化验收：在 **Light / Dark 两套主题**下，逐个复刻组件与参考实现做像素级比对，**像素相似度 ≥ 99%**。
- **R2 UX 双端一致性验证**：在 UI/UX 复刻完成之后，用 **CDP 控制 ChatGPT App**（基准端）、**computer use 控制 GPUI**（被测端），在 Pull Requests 页面执行同一套操作序列。
  - 验收：两端在 Pull Requests 页面上的 **UX 交互完全一致** —— 每个可点击项触发的结果、页面跳转、状态变化，以及嵌套按钮的层级行为一一对应。

## 0. 入口

- 左侧边栏 `Pull requests` → 进入 **PR 列表页**（左=列表，右=详情占位「Select pull request to view」）

## 1. PR 列表页

### 1.1 列表顶部分类标签（互斥）

| 按钮 | 行为 |
| --- | --- |
| `All` | 显示全部 PR，按分组渲染：「Previously reviewed」「Authored」 |
| `Reviewing` | 只显示待我评审的 PR；无数据时页面显示空状态「You're all caught up」 |
| `Authored` | 只显示我发起的 PR；分组标题为「Authored」 |

### 1.2 搜索

- 搜索输入框（Search pull requests）：输入即过滤列表（跨标题/仓库/分支）
- 有内容时输入框右侧出现 `Clear search`：点击清空并恢复完整列表
- 无匹配时显示「No pull requests match this search」

### 1.3 过滤按钮（漏斗）

- `Filter pull requests` → 打开菜单，两个项各自带子菜单（需 hover 展开）：
  - `Status` → `All states` / `Open` / `Merged` / `Closed`
  - `Repository` → `All repositories` / `<org>/<repo>`
- 选中子项后：菜单关闭、列表重新加载（先显示「Loading pull requests」）、漏斗按钮出现激活标记

### 1.4 分组标题

- `Previously reviewed`、`Authored` 标题（含箭头）→ 点击折叠/展开该分组

### 1.5 PR 行

- 每一行整体是一个按钮（aria-label = PR 标题）→ 点击后右侧详情面板先加载（转圈），完成后进入 **PR 详情页**
- 行内没有嵌套按钮；hover 不会出现额外按钮

## 2. PR 详情页（默认 `Summary` 标签）

### 2.1 顶部通用按钮

| 按钮 | 触发 |
| --- | --- |
| `Summary` / `Code`（tab） | 在「概览」与「Diff」两个视图间切换 |
| `Open in browser` | 在系统默认浏览器打开该 PR 的 GitHub 页面 |
| `Chat` | 为该 PR 新建对话：跳到聊天页，输入框预填「Help me understand pull request #N: <标题> <PR 链接>」（未自动发送） |
| `Open chat` | 该 PR 已有对话时 `Chat` 变为 `Open chat`，点击跳到该对话 |
| `Merge` | 合并 PR；Draft 状态时禁用并提示「Merge unavailable: Mark as "Ready for review" to merge」 |
| `Enter full screen` / `Exit full screen` | 详情面板占满窗口（隐藏列表栏）；按钮文案互相切换 |

### 2.2 标题 / 元信息区（嵌套按钮）

- `Edit title`（标题右侧铅笔）→ 标题变编辑框，出现 `Cancel title editing` / `Save title`
- 变更统计按钮（`+x -y`，aria-label=`Review pull request changes`）→ 打开新的 **Review 标签页**（见 §4）
- `Request reviewers`（Reviewers 行内）→ 弹出对话框「Request approvals」，含输入框「Search by name or GitHub username」，输入后显示匹配用户（无结果显示 No users found），选中即发起评审请求，Esc 关闭
- `Change pull request status`（Status 行内，显示当前状态如 `Draft`）→ 菜单：`Draft` / `Ready for review` / `Closed`（当前状态项禁用）

### 2.3 Description 区块

- `Description` 标题 → 折叠/展开描述正文
- `Description actions`（…）→ 菜单：
  - `Edit description` → 正文变编辑框，出现 `Cancel` / `Save`
  - `Generate with Codex` → 由 Codex 生成/改写描述
- 描述内 markdown 表格右上角 `Copy table` → 复制该表格（markdown）到剪贴板

### 2.4 Checks 区块

- `Checks` 标题 → 折叠/展开；无检查时内容为「No CI checks」，有检查时列出检查项

### 2.5 Activity 区块

- `Activity <n>` 标题 → 折叠/展开活动时间线
- 时间线内每个评论块的可点击项：
  - 作者名按钮（`Collapse comment by X` / `Expand comment by X`）→ 折叠/展开该评论
  - 时间戳旁的 permalink 图标（hover 出现）→ 在浏览器打开该评论的 GitHub 锚点链接
  - `Comment actions`（…）→ 菜单 `Edit` / `Quote reply` / `Delete`（不同评论可选项略有差异）
    - `Edit` → 评论变编辑框（Edit pull request comment）+ `Cancel` / `Save changes`
    - `Quote reply` → 打开回复框（Pull request reply），已预填被引用内容 + `Cancel` / `Post reply`
    - `Delete` → 删除该评论
  - `Open <文件名> in Code`（评审评论内）→ 切到 `Code` 标签并滚动定位到该文件
  - `Reply` → 打开空回复框（Pull request reply）+ `Cancel` / `Post reply`（无内容时 `Post reply` 禁用）
  - `Resolve` → 解决该讨论线程

### 2.6 提交区块

- `N commits` 标题 → 折叠/展开；展开后列出提交
- 每个提交的短 hash 是链接 → 在浏览器打开 `github.com/<repo>/commit/<sha>`

### 2.7 底部评论框

- 输入框 `Pull request comment` + `Post comment`（无内容时禁用，有内容时可用）

### 2.8 已合并 PR 的差异

- 顶部没有 `Merge` / `Edit title` / `Request reviewers` / `Change pull request status`；Status 显示 `Merged`；其余交互一致

## 3. Code 标签（Diff 视图）

### 3.1 工具栏

| 按钮 | 触发 |
| --- | --- |
| `Review options`（…） | 菜单（可勾选项，选中后文案变为 Disable …）：`Enable/Disable word wrap`、`Enable/Disable rich preview`、`Enable/Disable word diffs` |
| `Collapse all diffs` ↔ `Expand all diffs` | 全部折叠 / 展开 |
| `Switch to split diff` ↔ `Switch to unified diff` | 切换并排 / 统一 diff 视图 |
| `Show file tree` ↔ `Hide file tree` | 打开 / 关闭右侧文件树面板 |

- 分支行「base > head」为静态文本，不可点击

### 3.2 每个文件头（嵌套按钮，顺序：文件名、复制路径、折叠、打开）

- 文件名按钮 → 折叠/展开该文件的 diff
- `Copy path` → 复制文件路径到剪贴板
- `Toggle file diff` → 折叠/展开该文件的 diff
- `Open file`（tooltip「Open in editor」）→ 在编辑器中打开该文件

### 3.3 diff 内容

- 「N unmodified lines」折叠条（含箭头）→ 点击展开该段上下文
- hover 某一行时左侧出现 `+` 按钮 → 点击在该行下方打开行内评论框（占位符 Request change）+ `Cancel` / `Comment`（无内容时 `Comment` 禁用）

### 3.4 文件树面板

- `Filter files…` 输入框 → 输入即过滤树中文件（同时过滤 diff 文件列表）；输入后出现清空按钮
- 文件夹行（如 `docs`、`scripts`）→ 展开/折叠
- 文件行 → 选中该文件并把 diff 滚动定位过去；行尾显示 git 状态徽标（A/M）

## 4. Review 标签页（点变更统计按钮 `+x -y` 进入）

- 顶部出现标签：[图标] PR 标题 + `Close … tab`（X）+ `Enter full screen`
- `Close` → 关闭该标签，回到 Summary/Code 详情
- 下拉选择器 `All PR changes +x -y` → 菜单：
  - `All PR changes`（当前项带勾）
  - `Commits` →（子菜单列出各提交）选中后下拉文案变为 `Commit +x -y`，diff 只显示该提交的改动
- 其余与 Code 标签一致：`Review options`、`Collapse all diffs`、split/unified、`Show file tree`、文件头按钮、行内评论

## 5. 周边（进入该页后仍可见/可用）

- `Hide sidebar`：隐藏左侧边栏
- `Back` / `Forward`：应用历史前进后退（无前进历史时 `Forward` 禁用）

## 6. 未真实执行的写操作（避免改动真实仓库）

`Merge`、`Change pull request status`（Ready for review / Closed）、`Post comment`、`Post reply`、
`Save changes` / `Save title` / `Save`（描述）、`Delete`（评论）、`Resolve`、`Generate with Codex`、
`Request reviewers` 的最终提交。以上均已打开到「可提交状态」并确认按钮启用/禁用逻辑。
