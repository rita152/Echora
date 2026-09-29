# Codex app-server 接入

## 基线与口径

核对基线：`codex-cli 0.154.0`（2026-09-18 本机复核）。方法与字段来自该 CLI 生成的 default/experimental schema，接入状态来自仓库实现；应用运行时启动的 `PATH` 上的 `codex app-server --stdio` 即该版本。schema 随 CLI 版本生成，见[官方协议说明](https://learn.chatgpt.com/docs/app-server#message-schema)；升级时重新导出并核对：

```bash
codex --version
codex app-server generate-json-schema --out artifacts/app-server-schema/default
codex app-server generate-json-schema --experimental --out artifacts/app-server-schema/experimental
```

参考客户端的方法封装面可用只读扫描核对，不启动参考应用、不触碰运行中的实例；脚本以本表基线的 experimental schema 为方法全集：

```bash
node scripts/scan_reference_rpc_methods.mjs /Applications/ChatGPT.app/Contents/Resources/app.asar artifacts/app-server-reference-methods-20260918
```

当前扫描：参考 bundle 内嵌 252 个方法名中的 238 个，未出现的 14 个均为客户端请求，明细见 `artifacts/app-server-reference-methods-20260918/reference-method-scan.json`。方法名出现只代表参考客户端包含对应协议封装，不等于该产品流程已启用；未出现也不排除运行时动态拼接方法名。

参考 bundle 自身已先行到 `codex-cli 0.155.0-alpha.2.6`，多出尚未进入本表基线的 `userVerification/cancel`、`memory/status`、`thread/attachment/add|list|remove` 与通知 `thread/attachment/updated`；这些方法本机基线不发送，也不在总表内，若服务端主动下发会按未知方法报致命协议错误。

共 **252** 个方法：159 个客户端请求、11 个服务端请求、1 个客户端通知、81 个服务端通知。表中“默认”表示方法出现在默认 schema，“实验”表示仅出现在 experimental schema；字段以 experimental schema 为准。运行时启用 `experimentalApi=true`。

| 状态 | 数量 | 判定 |
|---|---|---|
| 已接入 | 142 | 表中声明的产品行为已连通协议、领域数据和 UI／副作用；不表示消费全部可选字段 |
| 后端已接入 | 4 | 已实现读取或校验，尚无对应可见 UI 调用方或展示 |
| 部分接入 | 5 | 只支持部分类型、有效变体或限定生命周期窗口 |
| 兼容退订 | 2 | initialize 按完整方法名退订；不代表对应产品能力已接入 |
| 未接入 | 99 | 客户端不发送；服务端请求按原 id 回受控回执并保持 generation 与共享连接，同时登记连接级诊断；仅 EOF、崩溃、写失败或致命协议错误终止连接；服务端通知仍按严格协议校验处理 |

未接入行的“—”沿用上述规则；带受控回执的服务端请求会另外列出入口，见下段。`tool/requestUserInput` 是兼容别名，不计入本版本 schema 的 252 项。

服务端请求不再以断开连接暴露覆盖缺口：不会转成交互请求的服务端请求一律按原 id 回受控回执，并记录连接级诊断（方法、请求 id、thread／turn、处置，以及脱敏且截断的 params 形状），连接、共享 pending RPC 与活动轮次全部继续存活。旧版审批协议（`applyPatchApproval`、`execCommandApproval`）校验载荷后回显式拒绝结果，并说明本客户端不为旧版协议提供审批 UI；`currentTime/read` 回本机时钟的 Unix 整秒；本阶段不集成或未知的方法回 `-32601`；已知方法的载荷无法解码时回 `-32602`。动态工具调用见 `item/tool/call`。

## 连接与状态

- **连接**：`ChatApp` 持有一个共享 manager。每个 generation 启动一个 `codex app-server --stdio`，只握手一次；单 reader 读取 stdout，stdin 串行写入完整 JSONL。所有 RPC 共用递增 request id，响应可乱序；已消费的追加、配置写入和线程设置 RPC id 保留到 generation 结束，重复响应不再次更新结果。初始化按下文能力协商退订两项通知。
- **线程与轮次**：首次提示词执行 `thread/start → turn/start`；既有线程在当前 generation 未加载时先 resume，之后直接 start turn。同一线程最多一个活动 turn，不同线程可并行；start/resume/fork 共用串行生命周期注册表。服务端自行发起的轮次（活动目标在线程空闲时继续推进、轮次结束后推进服务端队列、`thread/queue/start`）以没有本地 owner 的 `turn/started` 到达：manager 接管为普通受管轮次（同样的路由、审批与清理），再经连接事件 `TurnStarted` 把事件流交给显示该线程的会话；只有 `turn/started` 能引入这样的轮次，其他未知轮次消息仍是协议错误。观察方丢弃句柄不会中断服务端工作，显式停止才发送 `turn/interrupt`。
- **归属与提前事件**：轮次事件按 `threadId + turnId` 路由；server request 按原始字符串／数字 id 记录所属轮次。`turn/start` 响应前的事件按 wire 顺序缓存，取得响应后验证并回放；错配 id、字段或枚举报错。
- **审批与输入**：保留数字／字符串 request id 的区别，按到达顺序显示一张请求卡；键盘只响应当前可见请求。响应写入最多尝试一次，提交后等待 `serverRequest/resolved` 释放 responder；写入失败显示错误并阻止重复提交。文件审批关联同轮次、同 item 的原始 changes／patch，不使用聚合 turn diff 或当前磁盘内容代替。会话或轮次切换使旧点击失效；终态清理自身请求、响应句柄及临时关联。连接仅保留有上限的已释放 id／thread 标记，忽略已知重复或迟到的 resolved。
- **自动复核**：复核记录与人工审批请求分开，所有复核通知统一通过线程订阅与单调快照分发，避免轮次通道关闭时的竞争。未绑定 turn 的提前通知按完整标识等待真实 turn/start 响应，复核通知不会抢占启动中的轮次；已结束轮次的迟到通知经线程订阅更新原活动或历史快照。中断／失败／完成后本地结束等待展示，保留服务端原始状态与时间，不伪造完成通知；后续真实结果仍可补全。重复开始、重复完成和较旧完成消息不会回退已有结果。视图复用完整复核键，有目标项时随对应工具展示（MCP 拒绝独立展示），无目标项时独立展示；通过态隐藏但保留数据。当前 schema 未提供复核历史 item，应用重启后仅恢复服务端实际返回的历史，不从 rollout 或本地数据库补造复核。
- **终止与恢复**：turn 完成、中断或业务失败不关闭共享进程。EOF、崩溃、写失败或致命协议错误使旧 generation 的 pending RPC 和活动轮次各失败一次；回收旧进程后，下一次显式操作可重建连接，不自动重放提示词。应用退出时幂等终止并 wait 子进程。
- **状态通知**：应用／线程状态通过 `AgentConnectionEvent` 快照订阅，轮次事件进入各自 `AgentRun`。工作区通知可先于 RPC 响应；内存覆盖层防止迟到列表撤销重命名、移动、归档或删除。
- **活动视图与未读**：侧边栏铃铛打开的活动视图只读取 `thread/list` 与 `thread/status/changed`：`active` 带 waitingOnApproval／waitingOnUserInput 为待处理，其余 `active` 为进行中。协议没有线程已读标记，因此未读由客户端维护：线程离开 `active` 进入 idle／systemError，或进入待处理时，若它不是主区域正在显示的会话，就记为未读；打开该会话或在活动视图中「全部标为已读」后清除。未读 id 保存在本地 UI 偏好（与参考应用自行保存 `unread-thread-ids-by-host` 一致），不写入 app-server。只有本应用自身连接上发生的轮次能被观察到，另一个客户端（例如 ChatGPT 应用）中运行的线程不会显示为进行中。「归档聊天」逐个调用 `thread/archive`。
- **工作区与历史**：以服务端稳定 id 管理项目和线程；置顶使用服务端 `Pinned` 分区，当前 schema 无 `isPinned`。历史先 `thread/read(includeTurns=false)`，再分页读取 `thread/turns/list(itemsView=full)`；实际非 full 的轮次由 `thread/items/list` 补全。不维护本地会话数据库。
- **临时侧边聊天**：`thread/fork → thread/inject_items` 完成后才允许发送。父历史仅供参考，侧边说明禁止延续父任务或调用子 agent；新消息明确要求的修改才属于侧边请求。关闭使用 `thread/unsubscribe`；临时 id 只在所属 generation 使用，失效后保留可读消息，禁止 resume。

### MCP elicitation

`mcpServer/elicitation/request` 是独立的 server-to-client 请求：卡片由 connection generation + 原始 request id 拥有，不进入 turn 会话的 pending registry，因此既不复用也不伪装成命令审批、文件审批、权限审批或 `item/tool/requestUserInput`。当前只支持标准 MCP `mode=form` 与 `mode=url`。

**参数与字段边界**：`params.serverName`、`params.threadId` 必须是字符串；`params.turnId` 区分缺省、null 与字符串三种状态并原样保留，不强制绑定活动 turn。`form` 解析标准 `requestedSchema`：string（含 `format`、`minLength`、`maxLength`）、number/integer（含 `minimum`、`maximum`）、boolean、单选（`enum`／`enumNames`／`oneOf{const,title}`）、多选（`items.enum`／`items.anyOf`、`minItems`、`maxItems`）以及 `title`、`description`、`default`、`required`；未知字段、重复选项、非法边界与不支持的 primitive 都按协议错误处理。`url` 保留 `elicitationId`、`message`、`url`，只接受 http/https 绝对 URL。`openai/form`、`openaiForm`、`openai/userVerification` 需要完整 schema/UI 支持，尚未实现：收到时按协议错误回 `-32602` 并终止当前连接，不静默降级。

**响应与 resolved**：响应严格为 `{action, content?}`，`accept` 才提交结构化 `content`，且写回前按 schema 校验必填、类型、枚举、数值范围、`minItems`／`maxItems` 与 `format`；校验失败不写任何响应，卡片保持可编辑。`decline`／`cancel` 永不伪造 `content`。写回成功只进入 Submitting，必须等匹配 generation 与 request id 的 `serverRequest/resolved` 才显示终态；重复与迟到 resolved 幂等，错 thread 或重复 request id 报错并终止连接。URL 模式打开链接只是本地动作，不产生协议 action。

**失效路径**：EOF、崩溃、致命协议错误与显式重启都会回收 generation，并向 UI 发布该 generation 的 elicitation 失效事件；`thread/closed` 与关闭 side conversation 同样使该线程的等待请求失效。旧 generation 的 responder 一律拒绝回复，且不会自动重试或重复写回。连接重建不会重放旧 elicitation。

**UI 与验收**：Pending／Submitting 卡片显示在 Composer 上方，终态（已接受、已拒绝、已取消、已失效）留在会话流中；elicitation 不结束、不重启、不覆盖所属 turn，也不改动会话 phase。字段保留明确的领域状态（文本、布尔、单选、多选）与逐字段校验错误。视觉基线来自 ChatGPT 桌面应用真实 CDP 采集（`artifacts/mcp-elicitation-cdp-20260913/`：form／url 卡片、校验错误、提交中、已解析、深色与浅色主题），可复现入口为 `scripts/cdp_mcp_elicitation_capture.mjs`、`scripts/mcp_elicitation_fixture.mjs` 与 `scripts/compare_mcp_elicitation_pixels.py`。

### 配置与权限

设置页按当前会话工作目录调用 `config/read(includeLayers=true,cwd)`，并在同一 generation 调用 `configRequirements/read`。`config`、`origins`、`layers`、每层版本、disabledReason 和原始来源元数据分别保留；缺省、null、用户层显式值与更高层的有效值不混为一谈。已知来源在适配器归一化为用户、项目、系统、受管、会话与默认层，未知来源保持只读；当前 CLI 实测仅允许写用户配置文件，因此项目层不因 schema 接受 `filePath` 就显示为可写。选中 profile 的配置层、没有版本或没有绝对路径的来源也不可写。

当前可编辑字段为 `approval_policy`、`sandbox_mode`、`web_search`、`model_verbosity`、`model_reasoning_summary`、`approvals_reviewer`、`default_permissions`、`model`、`model_reasoning_effort`、`plan_mode_reasoning_effort`、`service_tier`、`personality`，以及由开关直接保存的 `hooks.state."<key>".enabled`、`hooks.state."<key>".trusted_hash`、实验性功能 `features.<name>`（仅设置页列出的 beta 项）、`features.memories`、`memories.generate_memories`、`memories.use_memories`、`memories.disable_on_external_context`（同时删除旧键 `memories.no_memories_if_mcp_or_web_search`）；这些开关的规则见「钩子、实验性功能与记忆」。枚举来自本机 schema；模型、推理强度和服务档位结合实际模型目录。保留合法 granular approval policy 和可扩展字符串，不把旧 `on-failure` 列为当前 schema 的新选项；兼容已返回的值。`guardian_subagent` 与 `auto_review` 在比较时等价，granular 的两个缺省布尔值按 schema 补为 false。合法未知字段仍保留在快照与来源详情中，未提供通用 JSON 配置编辑器。

受管允许值映射到批准策略／复核者、沙盒、网页搜索及权限 profile 的选项和禁用原因；`defaultPermissions`、`models.newThread` 的强制模型／推理强度／serviceTier 等约束显示为受管值。其余 feature、登录 shell、存储和网络等实际要求保留在详情中；没有对应编辑控件时不新增可提交入口。存在命名权限定义且其他有效层没有默认 profile 时，禁止删除必需的 `default_permissions`。服务端校验始终作为最终约束，客户端不自行放宽权限。

所有保存统一使用 `config/batchWrite`。只提交用户明确修改的字段，带服务端用户层 `filePath`、`expectedVersion`，每个 edit 使用 `mergeStrategy=replace`；null 表示删除所选层字段以恢复继承。包括 granular 对象在内均整字段替换，避免合并残留旧权限开关。没有 `config/value/write` 旁路。响应保留 status、version、filePath、overriddenMetadata，随后在同一连接回读并核对用户层实际值、版本、有效值和来源；分别反馈写入成功、被覆盖或回读差异。

仅修改模型、推理强度、Plan 推理强度、serviceTier、personality 时发送 `reloadUserConfig=false`；含其他已支持字段的用户层保存才请求重载。本机 schema 明确这些会话静态默认值不会通过重载热更新已有线程。尚未创建且用户未手动选择模型的草稿按自己的工作目录刷新默认值；恢复继承或删除显式模型／推理强度后，新草稿恢复服务端模型目录的默认值；已经创建的线程维持实际线程设置。配置回读成功不代表正在运行的轮次切换权限。

读取、编辑、保存回执和线程有效权限分别建模。草稿按工作目录保存在内存；关闭设置再打开可以继续。冲突保留 edits，须重新读取并显式核对后再提交新版本；读取失败、连接变化和结果未知均阻止沿用旧版本保存。校验失败保留草稿；写入后回执缺失明确显示结果未知，不自动重试。配置 RPC 超时终止旧 generation，后续显式读取才重建。配置真源始终是 app-server，UI preferences 不持久化这些配置。

`permissionProfile/list` 按 cwd 遍历 nextCursor，拒绝循环游标与重复 id。当前列表 schema 有 `id`、`allowed`、nullable `description`，没有 `extends`；解码兼容服务端可选 extends 扩展，继承关系也从有效配置的 `permissions.<id>.extends` 和 `ActivePermissionProfile.extends` 读取，缺失时不伪造。菜单显示服务端 profile 和禁用原因，内置 `:read-only` 不作为额外选项展示；不显示后续轮次提示和手动重新读取入口。提交前再校验 allowed。固定入口的映射如下：

| 入口 | 请求权限 |
|---|---|
| 请求权限 | `:workspace`、on-request、user |
| 智能协助 | `:workspace`、on-request、auto_review |
| 完整访问权限 | `:danger-full-access`、never、user；先确认 |
| 自定义 | 继承当前 cwd 的服务端默认配置；既有线程通过无模型轮次的临时 thread/start 解析后 unsubscribe |
| 服务端命名 profile | 发送所选 id，其他未明确修改的线程设置由服务端决定 |

首次发送与既有线程更新复用同一权限编码。已有线程先从 start/resume/fork 响应及设置通知建立有效权限快照，不能拿配置文件值替代线程状态。每线程独立串行权限队列，不阻塞其他线程；操作绑定原 threadId、generation 和本地 operationId。waiter 在写 RPC 前注册，RPC 成功与匹配的 `thread/settings/updated` 缺一不可；通知先于响应时暂存，失败不发布成功。匹配校验本次明确发送的 policy、reviewer、profile／sandbox，已确认重复／已知迟到通知不回退状态。

本机通知没有 operationId 或服务端版本，无法从协议区分“与本次期望完全相同的外部修改”和本次操作回执；串行队列与字段匹配提供当前可实现的关联边界。通知等待超时关闭旧 generation，禁止其迟到回执满足新连接；连接 generation 更新同时清理尚未绑定线程的旧设置快照和待确认权限操作，旧读取回调不能覆盖新状态；切换会话、关闭侧边标签和线程关闭使旧视图操作失效。临时线程只使用原 generation，关闭取消 waiter 并 unsubscribe，不能自动 resume。权限更新对后续轮次生效；进行中的轮次保留原批准策略与沙盒，只有复核者随之切换：`thread/settings/update` 成功并收到匹配通知后，若该线程有唯一未结束的受管轮次且本次带 approvalsReviewer，在同一串行队列内发送 `turn/settings/update {threadId,turnId,approvalsReviewer}`（与参考相同的时机与字段）。applied 表示当前轮次已切换；targetUnavailable（轮次恰好结束）只记日志；RPC 失败不回退线程设置，只提示「将从下一轮起生效」。原生菜单、主／侧边选择、等待反馈、失败恢复及完整访问确认均经过这条路径。

配置领域位于 `src/agent/config.rs`，编解码位于 `config.rs`，连接操作位于 `manager/config.rs`、`manager/settings.rs`，草稿与回读判定位于 `src/configuration.rs`，交互位于设置和 Composer 视图。真实独立配置验证入口为 `python3 scripts/verify_config_permissions.py --output artifacts/config-permissions-smoke`。

### 账户、登录与配额

账户、登录与配额是**连接级**状态，不依附任何 thread 或 turn。领域模型位于 [src/agent/account.rs](../src/agent/account.rs)：账户快照保留 `account`（字段缺失、显式 null、已知账户三态）与 nullable 的 `authMode`/`planType`；登录状态含未登录、登录中、已登录、失败、已取消以及当前 `loginId`；配额按 `accountId` + `limitId` 建立桶，并保留 primary/secondary、credits、individualLimit、spendControlReached、normalModelSlug、重置额度与后端 upsell。编解码位于 [src/agent/codex/account.rs](../src/agent/codex/account.rs)，连接操作与归约位于 `manager/account`。

| 行为 | 规则 |
|---|---|
| account/read | 保留 `account=null` 与字段缺失的区别；缺失套餐、余额或额度不转为 0 或空串；无活动 thread/turn 时照常处理并把结果写入连接快照。 |
| account/rateLimits/read | 同时消费 `rateLimits` 与 `rateLimitsByLimitId`；同一账户的多个 `limitId` 各自成桶，B 桶的稀疏更新不会覆盖 A 桶；读取到不同 `accountId` 时丢弃上一账户的桶，避免跨账户混合。 |
| account/rateLimits/updated | 单桶稀疏补丁，只应用存在且非 null 的字段：nullable 表示该字段当前不可用，不清除已经确认的值；不结束任何活动轮次，也不进入会话状态。 |
| account/updated | 应用级通知：nullable `authMode`/`planType` 只表示当前不可用；写入连接事件快照，新订阅者会收到回放。 |
| account/login/start | 只发送 `type=chatgpt`；解码 `chatgpt`（authUrl+loginId）与 `chatgptDeviceCode`（loginId+userCode+verificationUrl）。第一版登录只承诺 Codex 管理的 ChatGPT 登录，`chatgptAuthTokens`、Bedrock、外部 token 刷新与 API key 登录不提供可见入口，遇到这些变体返回明确错误且不记录凭据。 |
| account/login/completed | 按 `loginId` 关联当前登录；支持 success/error 与 nullable `loginId`（仅在单个登录进行中时归属）；取消后的迟到完成、重复完成通知都不会把状态改回成功；成功后重新读取账户与配额。 |
| account/login/cancel | 只取消匹配的当前登录，不影响已完成的登录；RPC 失败、断连保留 pending 供重试，`notFound` 同样结束本地等待。 |
| account/logout | 确认后调用；成功即清理本机账户、登录与配额快照，随后以 account/read 与 account/rateLimits/read 确认服务端状态；确认失败单独报告，不把登出回执当成服务端最终状态。 |

generation 变化、账户切换与登出都会清理旧快照；`fail_generation` 会移除该 generation 的账户快照并向订阅者发布空状态，旧回调不能污染新连接。新订阅者收到的连接快照包含当前账户、登录状态与完整配额桶。

UI 由真实后端状态驱动：侧边栏账户菜单显示账户标签与套餐（打开菜单即 `account/read → account/rateLimits/read`），退出登录先显示确认对话框，登录入口、登录中、device code/授权 URL、失败重试都在同一套状态上渲染。未知、加载中与失败状态分别渲染，不显示硬编码的账户、套餐、Token、余额或连续天数；账户显示名取自后端返回的邮箱本地部分，协议不提供昵称时不会杜撰。

本阶段不接入 `account/usage/read`、`account/rateLimitResetCredit/consume`、`account/sendAddCreditsNudgeEmail`、`account/workspaceMessages/read`、`account/chatgptAuthTokens/refresh`、Amazon Bedrock 登录，以及 realtime／remoteControl 客户端请求与 environment 方法（`remoteControl/status/changed` 通知只保存连接快照，无 Composer UI）；这些入口不显示或明确标注不可用，设置页也不再提供「使用情况和计费」页。`mcpServer/elicitation/request` 与 Skills／MCP 管理已各自接入，边界见对应小节与总表。参考采集脚本为 `scripts/cdp_capture_account.mjs`，GPUI 采集脚本为 `scripts/capture_account_gpui.sh`，像素比较脚本为 `scripts/compare_account_phase.py`；原始截图、动作日志、CDP 脚本与相似度报告保存在 `artifacts/account-phase/`，覆盖账户菜单与退出确认两种主题下的对比分数（差异主要来自字形栅格化、半透明表面的底层内容不同，以及协议不提供的账户显示名）。该阶段的 Computer Use 无法附加到当时同名同标识的 `GPUI Capture.app`（多次 `timeoutReached`），交互验收改用应用自身的采集入口与真实事件驱动的 UI 测试，细节见 `artifacts/account-phase/ui-validation/computer-use-report.json`；现在打包产物的名称与标识带工作树 slug（`scripts/package_gpui_capture.sh`），Computer Use 可按名称唯一绑定，附件前的身份自证见 `--print-diagnostics`。

### 技能与 MCP 管理

设置页的「插件」页承载插件、应用、MCP 与技能四个分段：插件与应用沿用参考目录，MCP 与技能由 app-server 驱动。`skills/list` 按 cwd 读取，保留 scope、interface、dependencies、未知字段与每项加载错误；本版 schema 没有 cursor，若服务端返回 `nextCursor` 会带重复游标校验地跟随并在超过 32 页时中止。`skills/config/write` 只提交用户明确修改的 `enabled` 与单一选择器，本机技能用 `path`、插件技能用 `name`，以服务端 `effectiveEnabled` 回执为准并随后复查列表；保存中、成功与失败分别建模，失败保留用户意图以便显式重试，重试不会翻转成相反值。`skills/changed` 只作为失效信号：使缓存过期并重新读取，不覆盖较新的本地写入结果，也不清空正在保存或失败的本地操作；读取按 cycle 丢弃过期响应，切换工作目录会重建该目录的缓存。

`mcpServerStatus/list` 每次请求都显式选择 detail：列表与详情首读使用 `full`，重新加载或登录完成后的复查使用 `toolsAndAuthOnly`，此时保留已加载的工具／资源目录而不是用空目录覆盖。每个 server 保留稳定 name、pluginId、runtimeStatus（nullable 与 `notStarted` 区分）、authStatus、serverInfo、工具、资源、资源模板、`toolsError` 以及服务端未知字段；分页按 cursor 顺序合并，重复或循环 cursor 中止并显示错误。`config/mcpServer/reload` 无 params，绑定当前 cwd 与 generation，30 秒无响应即关闭旧 generation 并报告超时；成功、失败与结果未知使用不同文案，成功后重新读取列表。

`mcpServer/oauth/login` 只发送用户选择的 name、可选 threadId、scopes 与 clientRegistration，返回 `authorizationUrl` 后由客户端生成 loginId 并记录 pending 登录；协议没有服务端 loginId，也没有取消请求，因此取消是本地失效：过期通知按 (threadId, name) 找不到 pending 登录时被解码后忽略，断连或 generation 切换会把仍等待的登录报告为已中断。`mcpServer/startupStatus/updated` 按 generation、thread（无 threadId 时按应用层）与 server 分层保存，仅供 MCP 管理界面和所属会话使用，不结束任何轮次；应用层状态不会写入无关会话。启动状态与 OAuth 状态互补：列表 `runtimeStatus` 为 null 时用最近的生命周期通知展示连接中／失败，收到更细的列表数据后以服务端为准。

代码入口：领域类型在 `src/agent/skills.rs`、`src/agent/mcp.rs`，编解码在 `src/agent/codex/skills.rs`、`src/agent/codex/mcp.rs`，连接操作在 `src/agent/codex/manager/skills.rs`、`src/agent/codex/manager/mcp.rs`，界面状态在 `src/skills.rs`、`src/mcp.rs`，视图在 `src/settings/view/plugins.rs`、`plugins_mcp.rs`、`plugins_skills.rs`。未接入边界：`skills/extraRoots/set`、`plugin/share/checkout`、`mcpServer/event/stream/*`、`mcpServer/tool/call`、`mcpServer/resource/read`；`mcpServer/elicitation/request` 已在上一节接入；MCP 服务器配置的新增／编辑／卸载仍由 Codex 配置文件负责，本阶段只做状态、重新加载与 OAuth。参考采集与对比工具见 `scripts/stage4/`，产物在 `artifacts/skills-mcp-stage4/`。

### 运行中追加输入

`AgentBackend::steer_turn` 只操作已接受的活动轮次。`turn/start` 响应校验后发布包含 generation、threadId、turnId 的身份；追加调用捕获该身份和独立提交 id，再按原连接的 request id 接收响应。追加不创建 AgentRun，不等待新的 `turn/started`，不清空输出、活动或审批，也不修改轮次终态。同一轮次的请求按提交顺序在后台写出，响应独立等待；这是客户端写入次序，不是服务端消息队列。发送与中断意图同步；请求已写出后的结束竞态交由服务端 `expectedTurnId` 前置条件决定，不自动改发 `turn/start`。

| 提交时状态 | 行为 |
|---|---|
| 空闲／真实终态后再次手动发送 | 沿用 `turn/start` |
| 正在启动、尚未取得已校验的 turnId | 保留草稿与附件，反馈尚未就绪 |
| 有活动轮次 | 跟进方式为「引导」（默认）时即时 `turn/steer`，为「排队」时 `thread/queue/add`，⌘⏎ 对单条消息取反；模型、工作目录、权限与 plan/default 选择仅在下一次 `turn/start` 生效 |
| 正在停止 | 保留输入，提示等待真实轮次终态后手动发送 |
| RPC 拒绝／响应缺失或 turnId 不匹配 | 只更新该次提交的失败状态，保留文本、附件、审查评论快照；新草稿不被覆盖 |
| 原连接失效／重建／临时聊天关闭 | 旧身份不能绑定新 generation，不 resume、不重发追加输入 |
| 已接受后收到轮次终态或迟到 RPC | 结果只归属原提交；不恢复已经结束的轮次 |
| 恢复到仍运行但非本 manager 持有的历史轮次 | 没有可用活动身份，保留输入并反馈尚未就绪 |

每次发送有独立快照以及 Sending／Accepted／Failed 状态。服务端 `userMessage` 可先于响应到达；消息事件已证明接受后，即使确认响应丢失或校验失败，也保留 Accepted 并显示该提交的确认异常，不恢复或自动重发输入。`clientId` 优先关联本次 `clientUserMessageId`，缺省时按当前轮次尚未关联的内容与附件快照匹配，不将相同文本的多次合法提交合并。相同 item 内容的 started/completed 重复通知在协议层去重；会话层另按 item.id 和已关联的 clientId 防止重复气泡。消息更新保留原位置；计划卡按追加消息划分展示段，追加前的计划保留在该消息之前，不被后续计划覆盖。历史保留 clientId，并据此关联已接受但尚未收到实时消息的回执；按服务端 item 顺序恢复后续用户消息，完成折叠不会隐藏追加消息。

Composer 在运行中有草稿时显示“追加输入”，无草稿时显示停止；Enter 发送、Shift+Enter 换行。失败快照可显式恢复，自动恢复仅限草稿自发送后未修改且仍为空；已有新草稿时不覆盖。主会话与侧边聊天分别持有草稿、提交和事件流。提交快照只保留在当前应用会话中，不写入后端历史。RPC 已接受但尚未收到 userMessage 时显示接受回执；若轮次此时结束，仍保留回执和快照，并允许显式恢复副本，不把未收到的消息事件伪造到服务端历史，也不自动重发。

运行中是否改为排队由「跟进处理方式」决定，见下节；引导路径与本节行为不变。输入可选 `text_elements` 和图片 `detail` 未设置时按 schema 默认值省略；当前输入入口不增加音频、skill 或 mention 编辑能力。文件上下文仍使用文本包络；其中增加附件类型元数据供历史恢复，属于 input 文本，不增加 RPC 字段。旧混合包络若图片已转换为不透明 URL、无法判断路径类型，则保留原来的图片展示，不猜测额外文件卡。

代码入口：协议位于 [src/agent/codex/](../src/agent/codex/)，领域类型位于 [src/agent/](../src/agent/)，工作区合并位于 [src/workspace.rs](../src/workspace.rs)，会话归约位于 [src/conversation/](../src/conversation/)。方法表的“入口”相对于 `src/agent/codex/`，省略 `.rs`。

### 服务端排队

参考与本机基线的共同事实（`artifacts/batch1-queue-20260928/wire`、`artifacts/batch1-baseline-20260928`）：服务端拥有队列并在轮次正常结束后自行启动下一条（`turn/completed → thread/queue/changed → turn/started`，其 userMessage 的 `clientId` 即排队时的 `clientUserMessageId`）；用户中断后不再推进，空闲时新增的排队也不会自动开始；客户端只在「立即发送」空闲线程与「继续」时调用 `thread/queue/start`。`thread/queue/changed` 只是失效信号（0.158 参考还会周期性下发），客户端据此重新 list，列表在途时又有变化就再读一次，直到结果对应最新信号。本机 0.154 的队列只存在于加载该线程的 app-server 进程中：换一个进程后即使 resume 也读到空队列（`artifacts/batch1-baseline-20260928/unloaded-thread-reads.txt`），而 `thread/goal/get` 对未加载线程照常返回持久化的目标。

| 操作 | 协议 | Echora 行为 |
|---|---|---|
| 运行中发送（排队模式） | `thread/queue/add {threadId,input,clientUserMessageId}` | 草稿快照（文本、附件、审查评论及编码后的提示词）按 clientUserMessageId 保存在内存；失败时显示错误并把输入放回空草稿，不重试 |
| 打开线程 / 失效 | `thread/queue/list {threadId,cursor}` | 遍历 nextCursor，拒绝重复游标与重复 id；超过 64 页中止；旧 generation 或其他线程的结果丢弃 |
| 立即发送（运行中） | `turn/steer`（沿用该行的 clientUserMessageId）→ `thread/queue/delete` | `deleted=false` 视为错误显示在该行；行在途时忽略重复点击 |
| 立即发送（空闲）/ 继续 | 先 resume 未加载线程 → `thread/queue/start {threadId,queuedSubmissionId?}` | 开始的轮次作为服务端轮次接入会话 |
| 编辑（菜单或空输入框中按 ↑ 编辑最后一行） | `thread/queue/delete` → 提交时 `thread/queue/add`（新 clientUserMessageId）+ `thread/queue/reorder` | 与参考相同：先从服务端队列删除，再把输入载回输入框（要求草稿为空；删除在途期间有新输入则放回原行）。提交时若仍在运行、正在启动或还有排队消息，就按原位置排回：放在原来的下一条之前，否则原来的上一条之后，否则末尾；否则作为新轮次直接发送。若原行又出现在列表中（被其他客户端恢复），改用 `thread/queue/update` 原位替换 |
| 删除 / 清空队列 | `thread/queue/delete` | 逐条删除，不显示提示 |
| 撤销删除或编辑（⌘Z） | `thread/queue/add`（原 clientUserMessageId 与原输入）+ `thread/queue/reorder` | 输入框没有可撤销的文本编辑时，⌘Z 恢复最近一次删除（60 s 内）或编辑（30 min 内）的消息，按上面的位置规则排回，并显示成功提示「已恢复队列中的消息／已恢复排队的消息」；编辑中的草稿随之清空 |
| 重做（⌘⇧Z） | `thread/queue/delete` | 输入框没有可重做的文本编辑时，撤销恢复的消息再次删除（删除）或再次移出并载回输入框（编辑），计时重新开始，不显示提示；新的删除或编辑会丢弃待重做项 |
| 在侧边聊天中打开 | `thread/queue/delete` → 新侧边聊天的首轮 `turn/start` | 菜单项只在可以开侧边聊天时出现；删除成功后新建侧边聊天标签并立即发送该消息，无法创建时按原 id 放回队列；不可撤销 |
| 拖动排序 | `thread/queue/reorder {threadId,queuedSubmissionIds}` | 移动 6 px 后开始拖动，只在多于一行且没有行在途时启用；发送完整顺序（服务端拒绝部分列表），失败后重新 list |

跟进方式保存在本地 UI 偏好 `follow_up_mode`（默认「引导」，与 9334 参考实例读取到的 `steer` 一致），不写 app-server；参考把它写入 Codex 配置 `desktop.followUpQueueMode`，那是桌面应用自己的设置。用户停止轮次后若队列非空，托盘显示「由于你中断了当前响应，队列已暂停」与「继续」；此时发送新消息先弹出「发送消息？」：「清空队列」先 `turn/start` 再逐条删除，「发送消息」发送后让队列在新轮次结束时继续。侧边聊天是临时线程，参考不为其启用服务端队列，Echora 在侧边聊天中始终引导。

### 线程目标

`thread/goal/get` 在打开线程时回填（参考只在 resume 后回填），失败只记录日志、不显示错误。斜杠菜单的「目标」（计划模式开启时先关闭）或输入 `/goal` 打开 Goal 标记再输入目标，完整输入 `/goal <objective>` 也可直接提交，发送 `thread/goal/set {threadId,objective,status:"active"}`；已有目标时先弹出「替换当前目标吗？」。新聊天中目标以 `/goal <objective>` 作为首条提示词发送，首轮被接受后再 set，与参考一致。设置后服务端在线程空闲时自行发起推进轮次（基线探测里没有 userMessage），会话把目标文本显示为该轮请求。托盘摘要按状态显示「进行中的目标／已暂停的目标／目标已停滞／目标使用受限／目标受限」、目标文本、`timeUsedSeconds`（有预算时加 `tokensUsed / tokenBudget`），操作为清除、暂停／恢复（active→paused；paused、blocked、usageLimited→active；budgetLimited 与 complete 无切换）与编辑。编辑在右侧面板打开「编辑目标」标签（再次点击同一编辑按钮关闭它）：顶部显示「刚刚更新／# 分钟前更新」（按 `updatedAt` 取整分钟）、「还原」和「保存」，两者只在文本与已保存目标不同时可用，⌘S 也会保存；保存与参考一致地发送 `thread/goal/set {threadId,objective,status:"active"}`，因此会同时恢复已暂停的目标；失败显示「未成功保存目标」并保留编辑。目标被清除、完成或在别处改成其他内容时标签自动关闭。以目标发出的用户消息下方常驻「设为目标」标记；完成目标的轮次在回复操作行显示「已在 {totalTime} 内达成目标」，时间取目标自身的 `timeUsedSeconds`，按参考的英文单位格式（`0s`、`3m 12s`、`5m`、`1h 5m`）；标记挂在 `thread/goal/updated` 的 turnId 所指轮次（缺省为当前轮次），之后任何非 complete 的目标更新都会撤下它，与参考只保留最后一个已完成目标一致。回复操作行只在最新一轮常显，更早的轮次悬停时显示（GPUI 没有 focus-within，键盘聚焦不会单独显示）。超过 4000 个字符（去掉首尾空白后按码点计）的目标与参考相同地写入 `$CODEX_HOME/attachments/<uuid>/goal-objective.md`，`thread/goal/set` 发送「Read the Codex goal objective file at <path> before continuing.」；「编辑目标」标签只对指向该目录下同名文件的这句原文读回文件内容，读取失败提示「未成功加载目标」，写入失败显示「未能加载目标附件」。参考经 app-server `fs/*` 写入，本客户端未接入 `fs/*`，因此只支持本机线程，由客户端直接写本机的 Codex 目录。用户停止时若目标为 active，先 `goal/set {status:"paused"}`，得到响应或 500 ms 后再 `turn/interrupt`，与 wire 顺序一致（`artifacts/batch1-goal-20260928/wire/goal-stop-pause-interrupt.jsonl`）。`thread/goal/updated` 报告新的 complete 时自动 `thread/goal/clear`；与参考一致，目标一旦 complete 托盘行立即消失，不保留「已达成目标」行。快照按 `updatedAt` 单调更新；请求在途期间到达的更新通知优先于较旧的响应；每个线程同时只有一个目标操作。

### 协作模式与自动复核批准

`collaborationMode/list {}` 每个 generation 读取一次并缓存；按 mode 去重、只保留 plan 与 default 且按此顺序；mode 为 null 的预设丢弃，未知 mode、空 reasoning_effort 报错（参考把未知 mode 改写为 default）。`turn/start` 的 collaborationMode 只取预设的 mode：settings 使用用户当前的模型与推理强度，developer_instructions 为 null，同时顶层 model/effort 发送 null（参考 `Gqn` 与 wire 一致）。读取失败只记录日志：default 仍照常发送，plan 选项隐藏，已选 plan 的提交以错误结束并保留输入。

`thread/approveGuardianDeniedAction {threadId,event}` 的 `event` 是核心 GuardianAssessmentEvent。参考从 `item/autoApprovalReview/*` 的原始 params 逐字段派生 snake_case 对象（`artifacts/batch1-autoreview-20260928/reference/bundle-event-mapping.txt`），Echora 在领域层原样保存该 params，批准时按同一映射生成：缺省字段保持缺省、null 保持 null，status 与 action 枚举转为 snake_case。本机 0.154 服务端接受这些变体并拒绝 camelCase（`wire/baseline-0.154-event-deserialization.json`），但不校验 status，因此客户端只对 denied 复核提供入口。拒绝块按参考依次显示「被拒绝的原因」（rationale，缺省为「未提供拒绝原因」）和「批准后允许的操作」，说明右侧是文字链接样式的「批准」（请求在途时为「正在批准…」）；批准记录后整段隐藏。斜杠菜单的「批准」只在存在可批准的拒绝时出现，子菜单列出最新 10 条（标题为动作摘要，第二行为 rationale 或「自动审查未提供理由」，在途时为「正在记录批准操作…」且全部行禁用），记录成功后关闭。同一会话同时只允许一个请求在途，已批准的 reviewId 不再发送，成功提示「已记录批准」，失败提示「无法记录自动审核批准」；「批准」的点状下划线（0.5 px、下移 2 px）由逐点绘制实现；切换线程后迟到的结果丢弃，旧 generation 的请求在 manager 拒绝。参考拒绝块受 Statsig 3487373434 控制；参考中无法稳定引发真实拒绝，因此用本机基线实测：`python3 scripts/batch1_app_server_probe.py --scenario guardian_live` 以 `approvals_reviewer="auto_review"` 让假模型对 `echo` 请求 `require_escalated`，假审核返回 deny，得到真实的 `item/autoApprovalReview/completed`；按上述映射生成的 event 被 `thread/approveGuardianDeniedAction` 接受（`{}`），服务端随即在审核上下文写入一条「用户已手动批准此前被拒绝的操作」的 developer 消息，重试仍经过自动审核并在放行后执行（`artifacts/batch1-autoreview-20260928/live-probe`）。同一 event 再次批准也会被接受并重复写入该消息，客户端的单次发送因此必要。单元测试以这次实测的 params 断言 Echora 生成的 event 与服务端接受的完全一致。

### 斜杠菜单与提示

光标前是一个 `/query` 记号时（`/` 位于行首或空白之后，查询从 `/` 到光标，可含空格），输入框上方 8 px 处浮出菜单：与输入框同宽，16 px 圆角、4 px 内边距、最高 320 px；行 28 px、12 px 圆角，未高亮时 75% 不透明。命令按本地化标题字母序排列，输入查询后按标题与 id 的模糊匹配评分排序，未匹配的标题部分变暗，无结果时显示「无命令」；查询含空格且没有匹配时菜单自动关闭，以便按原文发送；↑↓（及 Ctrl+N/P）移动、Enter 选择、Esc 关闭直到查询改变，选择后只移除光标前的 `/query` 记号，其余文字保留。本客户端提供「目标」「压缩」（仅已有线程，且输入框只有一行 `/…` 时出现；说明带 `min(last.totalTokens, 上下文窗口) / 上下文窗口` 四舍五入后的百分比，用量未知时省略；运行中选择时显示危险提示「聊天期间无法使用 Compact」）、「计划模式」（仅有 plan 预设时）与「批准」（见上节）；参考的其他命令、`@` 触发与技能分组不在本批范围。成功与危险提示显示在对话区顶部居中，最多叠放 3 条、5 秒后消失，可手动关闭；布局与两种配色（危险提示为红色边框 15%／40%、`#fff0f0`／`#280b0a` 底色与感叹号图标）取自参考的 toast。

0.158 参考与本机基线的差异（只实现 0.154 schema 已有字段）：通知多出 `emittedAtMs`；`turn/completed` 携带 summary items；`turn/start` 多出 `environments`、`turnTrigger`、`multiAgentMode`、`responsesapiClientMetadata` 等字段；`turn/steer` 多出 `responsesapiClientMetadata`；`thread/queue/changed` 周期性重复下发。

### 钩子、实验性功能与记忆

三类开关都是用户层 `config/batchWrite`：`filePath`、`expectedVersion` 取自当前读取到的用户层，edits 只含所切换的字段、`mergeStrategy=replace`，写后回读核对；同一时刻只允许一个开关写入，写入期间显示意图值，冲突、失败或回执缺失时恢复服务端值并在该行提示，结果未知不自动重试。有未保存的配置草稿时两类写入互斥，互不覆盖对方的版本。参考对同样的键发送 `mergeStrategy=upsert`、`filePath=null`、`expectedVersion=null`（`artifacts/batch2-{hooks,features,memories}-20260928/wire/`）：服务端会写到默认用户层并跳过版本检查，并发编辑时后写覆盖前写。本机 0.154 两种写法都被接受，过期版本返回 `configVersionConflict`（`artifacts/batch2-baseline-20260928`），因此 Echora 沿用既有纪律，以显式层与版本换取冲突检测；布尔叶子上 replace 与 upsert 结果相同。

**钩子**（`hooks/list`）：打开「钩子」设置页即读取，cwds 为当前项目根、其后是其他已知项目根（排序、去重），空列表时退回当前 cwd。列表按参考分组：「来自配置」（用户、管理员等配置层）、「来自插件」、「来自项目」与「其他来源」；每个来源行显示钩子数与「N 个问题 · N 项待审核」摘要，点击打开 680 宽的来源对话框：需审核时顶部横幅提供「全部信任」，其下是可折叠的加载问题（`warnings` 与 `errors`，后者在 0.154 只来自插件）和按事件分组的钩子行。钩子行显示标题（`{序号} - {statusMessage}` 或「钩子 {序号}」）、打开所在配置文件、需要审核时的「信任」（提示区分 untrusted「新钩子」与 modified「钩子自上次标记为可信后已更改」），以及启用开关；受管钩子（`isManaged`）始终开启、不可切换，未信任的钩子在信任前不可启用；展开显示类型、handler、命令或 MCP 服务器／工具、matcher 与超时。信任写 `hooks.state."<key>".trusted_hash=<currentHash>`，启用写 `.enabled`；key 以 JSON 引号包裹整段，来源按去引号的点号路径匹配。写入回执 `okOverridden` 显示为被更高层覆盖；成功后重新读取列表。「刷新」重新读取并提示「钩子已刷新」。项目层 `hooks.state` 在 0.154 不生效（基线已验证），因此所有状态都写用户层。

**实验性功能**（`experimentalFeature/list`）：设置 →「配置」页的「实验性质功能（测试版）」只显示服务端返回的 beta 项，按服务端顺序；与参考一致排除 `memories`、`multi_agent`、`plugins`、`plugin`、`remote_control`、`chronicle`、`workspace_dependencies` 与 `realtime_*`。切换写 `features.<name>`，不请求 `reloadUserConfig`；列表随后显示新的 enabled，但运行中的 app-server 保留启动时的功能开关，因此显示「重启 Echora（新建 Codex 连接）以应用实验性功能更改」，直到读到新 generation 的列表。参考该分区受 Statsig 2106641128 控制、放在同一「配置」页；0.158 参考改用 `experimentalFeature/enablement/set`，不在本批范围。

**记忆**：个性化设置顶部的「Codex 记忆」以功能列表中的 `memories` 项判定：缺失时显示「这台电脑不支持 Codex 记忆功能」。「启用 Codex 记忆功能」一次写入 `features.memories`、`memories.generate_memories`、`memories.use_memories` 三项（参考拆成两次写入，中途失败会留下不一致的组合）；「允许从使用工具的聊天中生成记忆」写 `memories.disable_on_external_context=!值` 并删除旧键，功能关闭时不可操作。「删除」先确认，确认后发送无 params 的 `memory/reset`，成功提示「Codex 记忆已删除」、失败提示「无法删除 Codex 记忆」，不自动刷新。斜杠菜单的「记忆」仅在 `memories` 功能开启时出现，打开 400 宽的「聊天记忆」对话框：新聊天两项均可切换，默认值取有效配置，首条消息以 `thread/start.config {"memories.use_memories","memories.generate_memories"}` 发送；已开始的聊天「使用记忆」只读（「对话开始后无法更改」），「生成记忆」乐观切换并发送 `thread/memoryMode/set`，失败回滚并提示，迟到、旧 generation 或其他线程的回执丢弃。

### 聊天内查找

⌘F 只在对话区域（key context `ChatConversation`）获得焦点时打开查找栏，文件编辑器、终端与 PR 面板保留各自的 ⌘F；⌘G／⇧⌘G、Enter／⇧Enter 切换结果，Esc 关闭并把焦点还给输入框。查询停顿 150 ms 后发送 `thread/searchOccurrences {threadId,searchTerm,limit:250}`（searchTerm 去首尾空白，空白查询不发送）；与参考一致只读取第一页，计数显示「{当前} / {总数} 个结果」，还有 nextCursor 时加「+」。越过最后一个已读结果时带 cursor 读取下一页，游标重复即报错停止；读完后回绕到第一个。0.154 每个匹配各占一项（同一条消息重复出现），因此同一 item 的第 n 个结果对应渲染文本中的第 n 个匹配；高亮在 Markdown 文本 run 与用户消息上按不区分大小写的字面匹配绘制，当前结果为橙色、其余为黄色（取自参考）。snippetMatchRange 是 UTF-16 下标，转字节时拒绝越界、倒序和切开代理对的范围。结果所在轮次未加载时复用历史分页重新读取一次，之后仍找不到则标记不可达；临时侧边线程返回 `-32601`，改为在已加载的用户消息和最终回复中本地查找。请求绑定查询周期、threadId 与 generation，旧查询、其他线程或旧连接的回执不改变状态。0.154 实测：游标绑定检索词（换词复用旧游标返回 -32600），未加载但已持久化的线程无需 resume 也可搜索，`turnCursor` 对 `thread/turns/list` 是包含式游标（Echora 暂未直接使用，改为整段历史重读）。

0.158 参考与本机基线在本批方法上的差异：`hooks/list` 在参考中随设置页多次调用并包含更多来源；实验性功能切换改走 `experimentalFeature/enablement/set`；记忆设置另有 `memory/status` 与从 ChatGPT 导入，均不在 0.154 schema 或本批范围内。

## Item 与历史兼容

实时 `item/started`／`item/completed` 与历史恢复支持下表类型。未知实时类型报错，未知历史类型保留为 `ThreadHistoryItem::Unsupported`。本机 schema 共 19 个 ThreadItem 变体，全部已有实时／历史编解码与领域状态支持；相邻版本的两个别名（collabToolCall、image_generation）不计入这 19 项。

| 类型 | 数据与兼容处理 | 展示行为 |
|---|---|---|
| `userMessage` | 校验 text/image/localImage/audio/localAudio/skill/mention；文本统一换行、解码显示转义并移除附件包络；从本应用路径包络及类型元数据恢复文件上下文，保留图片顺序（localImage 历史转换为 data URL 时也不重复生成文件卡） | 按 item.id/clientId 关联提交并去重；后续用户消息保留在当前 turn 的原始事件位置，历史不再合并到首条气泡 |
| `hookPrompt` | 独立于 Hook 运行记录；fragments 逐项保留 hookRunId/text 及原始顺序。实时开始／完成按 thread/turn/item 原位更新；历史只使用服务端返回的 item，完成标记为未知 | 带“钩子反馈”链接的只读文本气泡，支持长文本展开、正文选择、整段复制和打开现有钩子设置；不创建用户提交、助手最终答复或审批 responder |
| `agentMessage` | 实时增量全程保留 item.id，按同一 item 合并，完成快照原位校正文本（含缺失 delta、无 started）；已完成 item 的重复完成或迟到 delta 不追加文本。实时与历史均保留 phase，共用最终答复规则：优先取最后一条 final_answer，旧数据回退到最后一条未标注消息。实时完成后使用相同的过程折叠、答复操作与文件汇总 | 仅已完成且可识别最终答复的轮次折叠过程前缀 |
| `reasoning` | 按 item.id 与 summaryIndex/contentIndex 保存稀疏增量，保留开始／完成时间；不跨 item/index 合并 | 展示 summary，缺省时展示 content；完成后显示耗时 |
| `commandExecution` | 保留 command、cwd、exitCode、commandActions；旧历史缺少 actions/cwd 时用空列表／线程目录 | 读取、搜索、列目录与 shell 分别显示，输出归属对应命令 |
| `fileChange` | 保留 path、kind、diff；实时接受 patchUpdated 和 turn 聚合 diff；历史按路径汇总 | 文件卡及固定历史差异使用原始 patch；本机 Git 面板另由 Git/gh 提供工作区数据 |
| `imageView` | id/path；同 id 原位更新 | 缩略图与全局原图预览 |
| `imageGeneration` | 当前 status 为 in_progress/completed/failed，result 必需；读取 nullable revisedPrompt/savedPath/transparentBackground/failure。唯一 typed failure 为 usageLimitExceeded{limitId,resetsAt}；旧命名仅在历史路径兼容 | 优先 savedPath，文件不可用时物化 base64；中断移除未完成 loader，不伪造 failed item |
| `contextCompaction` | 按 item.id 更新；历史恢复为已完成活动 | 独立压缩上下文活动 |
| `collabAgentToolCall`、`collabToolCall`、`subAgentActivity` | 当前、相邻版本与旧历史映射为 AgentCollaboration。按载荷中的工具／接收者状态更新；稳定 item.id 原位更新，旧离散事件按 agentThreadId 合并 | 每个接收者独立状态；子任务失败不结束父轮次，支持只读嵌套子会话面板 |
| `mcpToolCall` | 保留 server/tool/status/arguments/appContext/pluginId/result/error；兼容旧 metadata、mcpAppResourceUri 与字符串 error，连接器自定义 JSON 不丢字段 | 按稳定 item.id 更新；完成快照保留已收到的 progress，错误可见 |
| `plan` | `item/plan/delta` 按所属 turn 内的 item.id 累加；completed item.text 权威覆盖增量，迟到 started/delta 不撤销终态 | Markdown 计划卡；整卡及键盘打开只读文件标签，支持复制和显式导出，沿用本地评价 UI |
| `webSearch` | 共享实时／历史解析，保留 query、action、results 和额外 JSON 字段；校验 search/openPage/findInPage/other 及 nullable 字段，results 为数组或 null | 单条显示查询／页面／查找目标及状态，多项沿用活动分组 |
| `sleep` | durationMs 为 uint64；实时按 started/completed 更新，中断／失败只结束仍在运行的活动 | 等待时长及状态；中断明确标记“原定”时长，不把请求时长称为实际耗时 |
| `functionCallOutput` | 保留 name、nullable namespace 与 required output；output 为字符串或 responses API 内容项数组，逐项校验 input_text/input_image（含 nullable detail：auto/low/high/original）/input_audio/encrypted_content。历史项一律标记 completed，缺失的可选字段保持 null，不从 rollout 或磁盘补全 | 参考客户端把它并入 turn 活动而不给独立行，GPUI 保持一致；字段留在领域数据中 |
| `dynamicToolCall` | 保留 tool、nullable namespace、required arguments（schema 为 `true`，任意 JSON 合法，含显式 null）、status（inProgress/completed/failed）、nullable success、nullable contentItems（inputText/inputImage/inputAudio）与 nullable durationMs。历史项一律标记 completed | 按稳定 item.id 原位更新并只保留一行；namespace 为空且工具为 automation_update／load_workspace_dependencies 时不渲染，与参考客户端一致；行样式沿用 MCP 工具行，hover／focus 显示 chevron，Enter／Space 展开 arguments、耗时与内容项 |
| `enteredReviewMode`、`exitedReviewMode` | 保留 item.id 与 review；entered 由 item.type 推断，历史项一律标记 completed | 参考客户端把两种审查模式并入 turn 活动而不给独立行，GPUI 保持一致；字段留在领域数据中 |

协作枚举、字段校验与历史别名见 `items.rs`；历史解码见 `workspace_protocol.rs`。计划、搜索和等待共享 `progress.rs` 解码；样式、尺寸和交互入口见 README 与组件实现。

已完成的 item 一律保持终态：`item/completed` 之后的迟到或重复 `item/started` 不重新激活该活动，重复 completed 幂等；不同 item.id 与不同 turn 各自独立。item 完成不结束 turn，turn 终态仍只由 `turn/completed` 决定。

`turn/plan/updated` 的 explanation／步骤状态单独建模为 turn 进度，替换当前 turn 的上一份步骤快照，不覆盖 plan 文本，也不自动推断所有步骤完成。活动结束不替代 `turn/completed`；turn 终止时仅收束仍活动的 item，并把仍进行中的计划步骤恢复为 pending。相同生命周期事件只更新已有活动；跨 thread/turn 由 manager 路由隔离，已结束 turn 的迟到计划／搜索／等待通知及重复 turn 完成通知不会重新绑定新 turn。增量没有序号或偏移，合法重复字符必须保留，不能按字符串去重；最终 plan item 负责文本收敛。

历史 `ThreadItem` 没有逐项开始／完成标记，也不携带 `turn/plan/updated` 步骤快照。恢复保留最终 plan 文本和搜索 JSON；尾项状态参考 turn 状态，前序记录按完成展示，不能还原并行活动的逐项终止时刻或实际等待耗时。本机 CLI 的中断样例在 full 历史中仅返回用户消息，未持久化未完成的 sleep；这类完全缺失的 item 无法跨应用重启恢复。步骤快照不从计划文本伪造，也不另建本地会话数据库。当前参考 ChatGPT 隐藏 sleep；GPUI 按产品要求保留等待行。计划文本的原生选择目前限于普通正文段落（含粗体／斜体样式），跨 Markdown 块及链接／代码混排片段的连续选择尚未实现；整份计划可用复制按钮取得。

## 运行时观察与能力协商

初始化保持 `experimentalApi=true`、`requestAttestation=false`，发送精确的 `optOutNotificationMethods` 两项：`turn/moderationMetadata`、`thread/compacted`。目标与队列通知已接入，见上文；`skills/changed` 与 `app/list/updated` 不在退订名单中：设置页分别把它们当作技能目录与应用目录的失效信号。默认 schema 与 experimental schema 均包含这些通知方法；逐项理由见总表。退订只作用于通知，不能屏蔽请求、响应或错误；未实现或未知的服务端请求按原 id 回受控回执（方法特定结果、`-32601` 或 `-32602`）并保持 generation 与共享连接，处置登记在连接级诊断中；只有 EOF、崩溃、写失败或致命协议错误才进入连接失败处理。不会退订 item/started 或 item/completed，也不会忽略未知方法。

Hook、认证恢复和 hookPrompt 快照通过带 generation 的观察通道交付。Hook 以 threadId/optional turnId/run.id 区分身份；缺省与 null 共同使用独立的无轮次键，同一 run.id 可以跨轮次存在，不把无 turnId 的记录迁移到前台或已知轮次。认证恢复以 threadId/turnId/provider 区分身份。两者可以早于 turn/start 响应，也可以晚于 turn/completed；不会建立或结束 turn。重复事件原位更新；服务端完成、终态 status 和较新完成时间不会被迟到 started 回退。turn 完成／中断／失败只收束该 turn 的本地等待；无 turnId 的 Hook 继续独立存在，在线程关闭或连接失效时本地收束。原始 status、message、output、时间和是否实际收到 completed 始终保留。

连接快照先重放 generation，再重放已归约记录和本地收束状态。新 generation 清空连接级观察快照，旧 reader 的观察不能污染新连接；已有会话可在内存中保留旧 generation 的闭合记录，并只向相同 thread/turn 投影。应用级弃用提示独立保存并可供新订阅者读取，不进入会话历史。应用重启后不恢复这些通知：本机 Thread/Turn 历史没有定义 HookRunSummary 或认证恢复记录，ThreadExtra 也没有可依赖的已定义字段，禁止从提示词、配置、rollout 或本地数据库补造。

Hook 字段范围：eventName 支持 preToolUse、permissionRequest、postToolUse、preCompact、postCompact、sessionStart、sessionEnd、userPromptSubmit、subagentStart、subagentStop、stop、interrupt；executionMode 支持 sync/async；handlerType 支持 command/mcpTool/prompt/agent；scope 支持 thread/turn。source 支持 system、user、project、mdm、sessionFlags、plugin、cloudRequirements、cloudManagedConfig、legacyManagedConfigFile、legacyManagedConfigMdm、unknown，缺省为 unknown。entries 的 warning/stop/feedback/context/error 均保留，展示时隐藏 context。completedAt、durationMs、statusMessage 允许缺省或 null，时间按 schema 的 int64 原样保留。

当前 ChatGPT 参考中，认证恢复通知被退订，弃用提示被保存但未在已核查的首页／会话页显示，因此二者只标记为后端已接入。Hook 运行通知的 UI 目前限于有实际 turn 归属和回复操作栏的最终摘要；无 turnId 等未完成展示路径仍标为部分接入。组件参考通过专用 ChatGPT 实例与 CDP 采集；确定性数据驱动真实组件与自然后端触发是不同验收范围，不以模拟回放宣称真实认证恢复或 Hook 执行成功。

## 方法总表

### 客户端请求（159）

| 方法 | API | 状态 | 已实现行为与限制 | 入口 |
|---|---|---|---|---|
| `account/bedrock/discover` | 实验 | 未接入 | — | — |
| `account/bedrock/setup` | 实验 | 未接入 | — | — |
| `account/login/cancel` | 默认 | 已接入 | 只取消当前 loginId 对应的登录；canceled 与 notFound 都结束本地等待，RPC 失败保留 pending 以便重试或继续等待完成通知。 | `manager/account` |
| `account/login/start` | 默认 | 已接入 | 只发送 type=chatgpt，解码 chatgpt（authUrl+loginId）与 chatgptDeviceCode（loginId+userCode+verificationUrl）；其他变体返回明确错误，不作为成功状态。 | `manager/account` |
| `account/logout` | 默认 | 已接入 | 确认后调用；成功后清理账户、登录与配额快照，再以 account/read 与 account/rateLimits/read 确认服务端状态，回执缺失时报告未确认。 | `manager/account` |
| `account/rateLimitResetCredit/consume` | 默认 | 未接入 | — | — |
| `account/rateLimits/read` | 默认 | 已接入 | 同时消费 rateLimits 与 rateLimitsByLimitId，按 accountId + limitId 隔离；保留 primary/secondary/credits/individualLimit/spendControl/normalModelSlug、reset credit 与 upsell，缺失值不补 0。 | `manager/account` |
| `account/read` | 默认 | 已接入 | 读取初始账户状态；保留 account=null 与字段缺失的差别；支持 chatgpt/apiKey/amazonBedrock 变体与 nullable email；无活动 thread/turn 时照常处理并进入连接快照。 | `manager/account` |
| `account/sendAddCreditsNudgeEmail` | 默认 | 未接入 | — | — |
| `account/usage/read` | 默认 | 未接入 | — | — |
| `account/workspaceMessages/read` | 默认 | 未接入 | — | — |
| `app/installed` | 默认 | 已接入 | forceRefresh/threadId；保留 id、enabled、callable、runtimeName 与未知字段；与 `app/list` 分开缓存，两者不互相折算。 | `manager/apps`、`apps` |
| `app/list` | 默认 | 已接入 | cursor/limit/forceRefetch/threadId；遍历 nextCursor，拒绝重复与循环游标；保留图标资产、标签、branding、appMetadata 与全部未知字段；应用行、分段徽标计数与空态均来自该响应。 | `manager/apps`、`apps` |
| `app/read` | 默认 | 已接入 | appIds/includeTools/threadId；toolSummaries 缺失与空数组分别保留，missingAppIds 按服务端上报渲染。 | `manager/apps`、`apps` |
| `collaborationMode/list` | 实验 | 已接入 | params `{}`，每个 generation 读取一次并缓存；按 mode 去重、顺序 plan、default，null mode 丢弃，未知 mode 报错。turn/start 只取预设的 mode，模型与推理强度沿用用户选择；读取失败仍发 default，plan 不可用。 | `manager/collaboration`、`collaboration` |
| `command/exec` | 默认 | 未接入 | — | — |
| `command/exec/resize` | 默认 | 未接入 | — | — |
| `command/exec/terminate` | 默认 | 未接入 | — | — |
| `command/exec/write` | 默认 | 未接入 | — | — |
| `config/batchWrite` | 默认 | 已接入 | 单项／多项统一 edits+replace，用户层 filePath、expectedVersion、适用的 reloadUserConfig；消费完整回执并回读，冲突或失败保留草稿。 | `manager/config`、`config` |
| `config/mcpServer/reload` | 默认 | 已接入 | 无 params；绑定当前 cwd 与 generation，区分成功、失败、超时与结果未知；成功后再读 `mcpServerStatus/list`。 | `manager/mcp`、`mcp` |
| `config/read` | 默认 | 已接入 | 当前 cwd、includeLayers=true；有效配置、origins、layers、版本与覆盖关系驱动设置及新线程默认值；未以 ChatGPT 登录时，`model_provider` 及其 `model_providers.<id>.name` 作为侧栏底部账户入口的名称。 | `manager/config`、`config` |
| `config/value/write` | 默认 | 未接入 | — | — |
| `configRequirements/read` | 默认 | 已接入 | 同 generation 读取 nullable requirements，约束对应选项和强制值；其余要求在来源详情保留展示。 | `manager/config`、`config` |
| `environment/add` | 实验 | 未接入 | — | — |
| `environment/info` | 实验 | 未接入 | — | — |
| `environment/status` | 实验 | 未接入 | — | — |
| `experimentalFeature/enablement/set` | 默认 | 未接入 | — | — |
| `experimentalFeature/list` | 默认 | 已接入 | cursor/limit=100/threadId；遍历全部页（最多 32 页），拒绝重复游标与重复 name，严格校验 stage 与 name；设置页「实验性质功能（测试版）」只列服务端 beta 行（排除参考同样隐藏的项），切换写 `features.<name>`；记忆设置与 `/memories` 以 `memories` 项判定可用与开启。 | `manager/features`、`features` |
| `externalAgentConfig/detect` | 默认 | 未接入 | — | — |
| `externalAgentConfig/import` | 默认 | 未接入 | — | — |
| `externalAgentConfig/import/readHistories` | 默认 | 未接入 | — | — |
| `externalAgentConfig/import/recordHistory` | 默认 | 未接入 | — | — |
| `feedback/upload` | 默认 | 未接入 | — | — |
| `fs/copy` | 默认 | 未接入 | — | — |
| `fs/createDirectory` | 默认 | 未接入 | — | — |
| `fs/getMetadata` | 默认 | 未接入 | — | — |
| `fs/readDirectory` | 默认 | 未接入 | — | — |
| `fs/readFile` | 默认 | 未接入 | — | — |
| `fs/remove` | 默认 | 未接入 | — | — |
| `fs/unwatch` | 默认 | 未接入 | — | — |
| `fs/watch` | 默认 | 未接入 | — | — |
| `fs/writeFile` | 默认 | 未接入 | — | — |
| `fuzzyFileSearch` | 默认 | 已接入 | 搜索弹窗文件模式的回退路径：服务端不支持会话时改为一次性请求，cancellationToken 固定为 gpui-fuzzy-file-search。 | `manager` |
| `fuzzyFileSearch/sessionStart` | 实验 | 已接入 | 搜索弹窗进入文件模式时为当前会话 cwd 建一个会话；失败按 session not found 回退到一次性请求。 | `manager` |
| `fuzzyFileSearch/sessionStop` | 实验 | 已接入 | 关闭弹窗、切换模式或切换会话时结束会话；会话 id 保留有上限的退休标记，迟到通知保持惰性。 | `manager` |
| `fuzzyFileSearch/sessionUpdate` | 实验 | 已接入 | 每次输入变化更新查询；结果经 sessionUpdated 通知返回，按 cycle 丢弃过期响应。 | `manager` |
| `hooks/list` | 默认 | 已接入 | cwds 为当前项目根与其他已知项目根；按 generation 校验每项 cwd、事件／来源／handler／trustStatus 枚举、绝对 sourcePath 与非负超时；钩子设置页按参考分组显示来源、摘要、问题与逐项信任／启用，写入 `hooks.state."<key>".{trusted_hash,enabled}` 后重新读取。 | `manager/hooks`、`hooks` |
| `initialize` | 默认 | 已接入 | 每个连接 generation 一次；发送 clientInfo、experimentalApi=true、requestAttestation=false；按完整方法名退订两项通知，列表见运行时能力协商。 | `manager` |
| `marketplace/add` | 默认 | 已接入 | source（必填）与可选 refName/sparsePaths；来源文本只来自用户在“添加”面板的输入；成功/失败/超时/结果未知四态分离，结果未知不自动重试。 | `manager/plugins`、`plugins_catalog` |
| `marketplace/remove` | 默认 | 已接入 | marketplaceName；先确认再发送，失败保留意图可显式重试，成功后强制重读目录。 | `manager/plugins`、`plugins_catalog` |
| `marketplace/upgrade` | 默认 | 已接入 | marketplaceName 可空（空即由服务端选择全部）；逐条保留 selectedMarketplaces、upgradedRoots 与 errors。 | `manager/plugins`、`plugins_catalog` |
| `mcpServer/event/stream/start` | 实验 | 未接入 | — | — |
| `mcpServer/event/stream/stop` | 实验 | 未接入 | — | — |
| `mcpServer/oauth/login` | 默认 | 已接入 | 只提交 name/threadId/scopes/clientRegistration/timeoutSecs；返回 authorizationUrl 并记录客户端生成的 loginId；取消是本地失效，断连使 pending 登录失效。 | `manager/mcp`、`mcp` |
| `mcpServer/resource/read` | 默认 | 未接入 | — | — |
| `mcpServer/tool/call` | 默认 | 未接入 | — | — |
| `mcpServerStatus/list` | 默认 | 已接入 | cursor/limit/detail/threadId；保留 id/name、runtimeStatus、authStatus、错误、工具、资源、模板与全部未知字段；循环游标中止并报错。 | `manager/mcp`、`mcp` |
| `memory/reset` | 实验 | 已接入 | 与参考一致不带 params；个性化设置「删除」先确认，成功／失败分别提示，不伪造成功。 | `manager/features`、`settings/view/memories` |
| `mock/experimentalMethod` | 实验 | 未接入 | — | — |
| `model/list` | 默认 | 已接入 | limit=50、includeHidden=false；遍历 nextCursor，拒绝循环游标；返回模型、默认值、推理强度及服务档位。 | `manager/catalog` |
| `modelProvider/capabilities/read` | 默认 | 未接入 | — | — |
| `permissionProfile/list` | 默认 | 已接入 | cwd、limit=100、遍历 nextCursor，拒绝循环游标及重复 id；消费 id/allowed/description，兼容可选 extends；设置和权限菜单展示，提交前复核 allowed。 | `manager/catalog`、`catalog` |
| `plugin/install` | 默认 | 已接入 | pluginName + marketplacePath/remoteMarketplaceName（未用一侧显式 null），可选 installAttemptId；120s 操作窗口，成功回执保留 authPolicy 与 appsNeedingAuth；超时与不可读结果分别建模为 TimedOut/Unknown，不自动重试。 | `manager/plugins`、`plugins_catalog` |
| `plugin/installed` | 默认 | 已接入 | cwds 与 installSuggestionPluginNames；与 `plugin/list` 分别缓存，安装开关状态以该响应为准。 | `manager/plugins`、`plugins` |
| `plugin/list` | 默认 | 已接入 | cwds、forceRefetch、marketplaceKinds（浏览目录时请求全部类型）；保留 marketplaceLoadErrors、featuredPluginIds、interface、source 四种变体与未知字段；行内容与分段徽标计数均来自该响应。 | `manager/plugins`、`plugins` |
| `plugin/reconcile` | 默认 | 已接入 | reason；每个连接 generation 首次加载插件目录时执行一次，回执中的 changedPlugins 与失败 id 列表按服务端上报展示。 | `manager/plugins`、`plugins_catalog` |
| `plugin/read` | 默认 | 已接入 | pluginName + marketplacePath/remoteMarketplaceName；保留 description、apps、appTemplates、hooks、mcpServers、scheduledTasks、skills 与未知字段。 | `manager/plugins`、`plugins` |
| `plugin/search` | 实验 | 已接入 | searchTerm + cursor/limit/scope/cwds；scope=global 表示跨全部可见 marketplace；遍历 nextCursor 并拒绝重复游标；结果行保留其自身的 marketplace 身份。 | `manager/plugins`、`plugins` |
| `plugin/share/checkout` | 默认 | 未接入 | — | — |
| `plugin/share/delete` | 默认 | 已接入 | remotePluginId；经确认卡发送，结果未知不自动重试。 | `manager/plugins`、`plugins_catalog` |
| `plugin/share/list` | 默认 | 已接入 | 无参数；未登录 ChatGPT 时服务端返回 -32600，错误原文按服务端语义展示，不伪造空列表。 | `manager/plugins`、`plugins_catalog` |
| `plugin/share/save` | 默认 | 已接入 | pluginPath + 可选 remotePluginId/discoverability/shareTargets；无本地路径的插件在本地拒绝并说明原因，不发送注定失败的请求；成功保留 shareUrl。 | `manager/plugins`、`plugins_catalog` |
| `plugin/share/updateTargets` | 默认 | 已接入 | remotePluginId + discoverability + shareTargets；targets 直接来自服务端 sharePrincipals，遇到接口无法表达的 owner 角色时拒绝发送而不是静默缩权。 | `manager/plugins`、`plugins_catalog` |
| `plugin/skill/read` | 默认 | 已接入 | remoteMarketplaceName/remotePluginId/skillName；contents 为 null 与空串分开保留；无远端 id 的技能在本地说明原因。 | `manager/plugins`、`plugins_catalog` |
| `plugin/uninstall` | 默认 | 已接入 | pluginId；先确认再发送，成功/失败/超时/结果未知四态分离，失败保留意图可显式重试。 | `manager/plugins`、`plugins_catalog` |
| `process/kill` | 实验 | 未接入 | — | — |
| `process/resizePty` | 实验 | 未接入 | — | — |
| `process/spawn` | 实验 | 未接入 | — | — |
| `process/writeStdin` | 实验 | 未接入 | — | — |
| `project/create` | 实验 | 已接入 | 发送 idempotencyKey、name、roots[{path}]；以 result.project 更新工作区。 | `manager/workspace` |
| `project/delete` | 实验 | 已接入 | 按 projectId 删除；同步移除项目及关联列表状态。 | `manager/workspace` |
| `project/import` | 实验 | 未接入 | — | — |
| `project/list` | 实验 | 已接入 | 按 position 升序分页；提供侧栏项目数据。 | `manager/workspace` |
| `project/move` | 实验 | 已接入 | projectId、nullable beforeProjectId；使用服务端顺序。 | `manager/workspace` |
| `project/read` | 实验 | 未接入 | — | — |
| `project/update` | 实验 | 已接入 | 按需发送 name、roots；以 result.project 更新工作区。 | `manager/workspace` |
| `remoteControl/client/list` | 实验 | 未接入 | — | — |
| `remoteControl/client/revoke` | 实验 | 未接入 | — | — |
| `remoteControl/disable` | 实验 | 未接入 | — | — |
| `remoteControl/enable` | 实验 | 未接入 | — | — |
| `remoteControl/pairing/start` | 实验 | 未接入 | — | — |
| `remoteControl/pairing/status` | 实验 | 未接入 | — | — |
| `remoteControl/status/read` | 实验 | 未接入 | — | — |
| `review/start` | 默认 | 未接入 | 启动模型代码评审；本机 Git 审查面板使用 Git/gh 与已有 turn diff，不调用此方法。 | — |
| `server/diagnostics` | 实验 | 未接入 | — | — |
| `skills/config/write` | 默认 | 已接入 | 只提交用户明确修改的 enabled 与单一选择器（插件技能用 name，本机技能用 path）；以服务端 effectiveEnabled 回执为准并复查列表。 | `manager/skills`、`skills` |
| `skills/extraRoots/set` | 默认 | 未接入 | — | — |
| `skills/list` | 默认 | 已接入 | cwds/forceReload；保留 scope、interface、dependencies、errors 与未知字段；本版 schema 无 cursor，若服务端返回 nextCursor 则带重复游标校验地跟随。 | `manager/skills`、`skills` |
| `thread/approveGuardianDeniedAction` | 默认 | 已接入 | 仅 denied 复核；event 由原样保存的复核通知 params 按参考映射派生（snake_case，缺省与 null 区分）；入口为拒绝块的「批准」与斜杠菜单的「批准」子菜单；每会话一个在途请求，已批准的 reviewId 不重发，成功提示「已记录批准」，绑定 generation。 | `manager/auto_review`、`auto_approval` |
| `thread/archive` | 默认 | 已接入 | 按 threadId 归档；通知与列表合并规则见“连接与状态”。 | `manager/workspace` |
| `thread/backgroundTerminals/clean` | 实验 | 未接入 | — | — |
| `thread/backgroundTerminals/list` | 实验 | 未接入 | — | — |
| `thread/backgroundTerminals/terminate` | 实验 | 未接入 | — | — |
| `thread/compact/start` | 默认 | 已接入 | 手动压缩；composer 的 `/compact` 命令触发，只发送 threadId 并要求响应 result 为对象。压缩按不可 steer 的轮次运行，期间追加输入按 `activeTurnNotSteerable{turnKind:compact}` 如实展示为提交失败，完成经既有 turn／item 事件与 contextCompaction 活动收束。 | `manager/compact`、`components/composer` |
| `thread/decrement_elicitation` | 实验 | 未接入 | — | — |
| `thread/delete` | 默认 | 已接入 | 按 threadId 删除；从所有侧栏集合移除。 | `manager/workspace` |
| `thread/fork` | 默认 | 已接入 | 仅用于临时侧边聊天：ephemeral=true、excludeTurns=true、threadSource=user，携带 cwd、说明及可选 model/effort/serviceTier。验证新 id、ephemeral 和先到的 thread/started；无持久化分叉 UI。 | `manager/side_conversation` |
| `thread/goal/clear` | 默认 | 已接入 | 按 threadId 清除；用户点击清除或服务端报告新的 complete 后自动调用；返回 cleared 布尔值，缺失即报错。 | `manager/goal`、`goal` |
| `thread/goal/get` | 默认 | 已接入 | 打开线程时回填目标快照；goal 为 null 或缺省表示无目标；返回的 threadId 必须一致；失败只记录日志。 | `manager/goal`、`goal` |
| `thread/goal/set` | 默认 | 已接入 | 新目标与「编辑目标」保存都发送 objective + status=active（超过 4000 个字符时 objective 为指向附件文件的一句话）；暂停／恢复只发 status；按 generation 绑定，线程未加载时先 resume；返回的 goal 按 updatedAt 与通知合并。 | `manager/goal`、`goal` |
| `thread/increment_elicitation` | 实验 | 未接入 | — | — |
| `thread/inject_items` | 默认 | 已接入 | 向新侧边线程注入 user message，标记父历史仅供参考；不开始 turn。失败时释放临时 fork，不交付可发送的线程。 | `manager/side_conversation` |
| `thread/items/list` | 默认 | 已接入 | 按 threadId、nullable turnId 升序分页；补全非 full 的历史轮次。 | `manager/workspace` |
| `thread/list` | 默认 | 已接入 | 分页读取最近、归档、项目与分区列表；保留前后游标及 projectId/sectionId 的省略、null、值三态。与参考端一致发送 `useStateDbOnly=true`、空 `modelProviders`／`sourceKinds` 与 null `parentThreadId`：缺省时服务端改走 rollout 扫描，只返回最近 10 个且不带游标，侧栏项目与最近聊天会缺行。 | `manager/workspace` |
| `thread/loaded/list` | 默认 | 未接入 | — | — |
| `thread/memoryMode/set` | 实验 | 已接入 | threadId/mode（enabled/disabled）绑定 generation 与线程；`/memories` 对已开始的聊天乐观切换「生成记忆」，失败回滚并提示，迟到或旧线程回执丢弃。 | `manager/features`、`conversation/memory` |
| `thread/metadata/update` | 默认 | 已接入 | projectId 省略表示不变，空字符串表示移出项目，非空 id 表示分配；读取 result.thread。 | `manager/workspace` |
| `thread/name/set` | 默认 | 已接入 | threadId、name；重命名会话。侧栏双击任务行弹出与参考一致的居中面板：输入框默认全选，取消／关闭／Esc／蒙层不发送请求，保存与 Enter 复用同一提交路径，空或纯空白不发送，超过 60 个字符的名称与参考一致截为前 59 个字符加省略号。 | `manager/workspace`、`components/sidebar` |
| `thread/queue/add` | 实验 | 已接入 | 运行中且跟进方式为排队（或 ⌘⏎ 取反）时发送 input 与新的 clientUserMessageId，重新提交编辑时同样用新 id；⌘Z 恢复与侧边聊天失败放回时沿用原 id 与原 input；返回的 clientUserMessageId 必须一致；草稿快照按该 id 保留。 | `manager/queue`、`queue` |
| `thread/queue/delete` | 实验 | 已接入 | 删除、清空队列、开始编辑、在侧边聊天中打开与立即发送（引导成功后）使用；deleted=false 原样返回，立即发送路径视为错误，其余路径视为已不在队列、不再编辑或移动。 | `manager/queue`、`queue` |
| `thread/queue/list` | 实验 | 已接入 | 打开线程与每次 queue/changed 后读取；遍历 nextCursor，拒绝重复游标与重复 id；在途期间有新变化就再读。 | `manager/queue`、`queue` |
| `thread/queue/reorder` | 实验 | 已接入 | 拖动排序、编辑重新排队与撤销恢复后发送完整顺序；失败显示错误并重新读取。 | `manager/queue`、`queue` |
| `thread/queue/start` | 实验 | 已接入 | 空闲线程的立即发送与「继续」；未加载时先 resume；开始的轮次作为服务端轮次接入。 | `manager/queue`、`queue` |
| `thread/queue/update` | 实验 | 已接入 | 编辑中的原行重新出现在列表里时原位替换 input（参考 enqueue 的同一规则）；返回的 id 必须一致。 | `manager/queue`、`queue` |
| `thread/read` | 默认 | 已接入 | includeTurns=false；读取线程上下文，历史另行分页。 | `manager/workspace` |
| `thread/realtime/appendAudio` | 实验 | 未接入 | — | — |
| `thread/realtime/appendSpeech` | 实验 | 未接入 | — | — |
| `thread/realtime/appendText` | 实验 | 未接入 | — | — |
| `thread/realtime/listVoices` | 实验 | 未接入 | — | — |
| `thread/realtime/start` | 实验 | 未接入 | — | — |
| `thread/realtime/stop` | 实验 | 未接入 | — | — |
| `thread/resume` | 默认 | 已接入 | threadId、excludeTurns=true；当前 generation 未加载时执行一次，返回 id 必须匹配；失败不回退为新建。 | `manager/turn` |
| `thread/revert` | 默认 | 已接入 | 改写最新用户消息时按 beforeTurnId 传该轮 id：把持久化历史替换为该轮之前的前缀，随后仍走 thread/read 与分页路径重载；响应是历史权威来源，校验返回 thread 与请求 threadId 一致并保留可选 turnsBackwardsCursor／itemsBackwardsCursor，不回退本地文件改动。先到的 `thread/reverted` 记为该请求的确认。 | `manager/revert`、`components/composer` |
| `thread/rollback` | 默认 | 未接入 | — | — |
| `thread/search` | 实验 | 已接入 | 非空 searchTerm、archived、分页和排序；返回 thread 与 snippet。历史会话搜索弹窗用它检索会话；空查询改用 `threadSection/list` 的置顶分区与 `thread/list`（recency 倒序）拼出前九行。参考实现的弹窗还会合并 ChatGPT 云端会话，app-server 无对应数据。 | `manager/workspace` |
| `thread/searchOccurrences` | 实验 | 已接入 | threadId/searchTerm（去首尾空白）/limit=250/cursor；UTF-16 snippetMatchRange 转字节并拒绝越界、倒序或切开代理对；游标重复即停止；`-32601`（临时线程）回退本地已加载消息查找；聊天内 ⌘F 查找栏逐项定位与高亮。 | `manager/search`、`conversation/find`、`home/find` |
| `thread/section/move` | 默认 | 已接入 | threadId、nullable sectionId/beforeThreadId；用于置顶和取消置顶。 | `manager/workspace` |
| `thread/settings/update` | 实验 | 已接入 | 主／临时线程按线程队列更新 approvalPolicy、approvalsReviewer、permissions 或 sandboxPolicy；绑定 generation／操作，等待 RPC 成功和匹配的有效权限通知，影响后续轮次；复核者另经 `turn/settings/update` 同步到进行中的轮次。 | `manager/settings`、`permissions` |
| `thread/shellCommand` | 默认 | 未接入 | — | — |
| `thread/start` | 默认 | 已接入 | 首条提示词才新建；发送 cwd、projectId、historyMode=paginated、ephemeral=false、serviceName、model、serviceTier；采用 result.thread.id。 | `manager/turn` |
| `thread/timeline/list` | 实验 | 未接入 | — | — |
| `thread/turns/list` | 默认 | 已接入 | 按 threadId 升序分页，itemsView=full；保留实际 itemsView 和双向游标，按需补取 item。 | `manager/workspace` |
| `thread/unarchive` | 默认 | 已接入 | 按 threadId 取消归档；读取 result.thread 并刷新列表。 | `manager/workspace` |
| `thread/unsubscribe` | 默认 | 已接入 | 只关闭本应用创建的临时线程；先请求中断自身轮次，接受 unsubscribed/notSubscribed/notLoaded；不影响父线程。 | `manager/side_conversation` |
| `threadSection/create` | 默认 | 已接入 | name、可选 appearance{icon,color}；返回 section，当前用于建立 Pinned 分区。 | `manager/workspace` |
| `threadSection/delete` | 默认 | 未接入 | — | — |
| `threadSection/list` | 默认 | 已接入 | 遍历分区分页；以服务端 Pinned 的稳定 id 实现置顶。 | `manager/workspace` |
| `threadSection/update` | 默认 | 未接入 | — | — |
| `turn/interrupt` | 默认 | 已接入 | 定向 threadId/turnId，每轮最多发送一次；等待真实 interrupted 终态，保持共享连接。 | `manager/turn` |
| `turn/settings/update` | 实验 | 已接入 | 线程权限更新成功后，若该线程有唯一进行中的轮次且本次带 approvalsReviewer，在同一串行队列里发送 `{threadId,turnId,approvalsReviewer}`；applied 与 targetUnavailable 严格解码，失败只影响当前轮次并提示从下一轮生效。 | `manager/settings`、`turn_settings` |
| `turn/start` | 默认 | 已接入 | 文本及 localImage 输入、路径上下文、model/effort/serviceTier；选择 plan/default 时 collaborationMode 由本 generation 的预设生成、顶层 model/effort 为 null；新线程首轮附权限字段。可选 clientUserMessageId；以 result.turn.id 建立轮次归属。 | `manager/turn`、`input`、`collaboration` |
| `turn/steer` | 默认 | 已接入 | 主会话／临时侧边聊天运行中即时追加：threadId、expectedTurnId、input、clientUserMessageId；校验 result.turnId。复用文本、localImage、文件路径上下文及审查评论编码；不传 model/cwd/权限等轮次覆盖字段。 | `manager/steer`、`input` |
| `userVerification/delete` | 实验 | 未接入 | — | — |
| `userVerification/enroll` | 实验 | 未接入 | — | — |
| `userVerification/status` | 实验 | 未接入 | — | — |
| `userVerification/verify` | 实验 | 未接入 | — | — |
| `windowsSandbox/readiness` | 默认 | 未接入 | — | — |
| `windowsSandbox/setupStart` | 默认 | 未接入 | — | — |

### 客户端通知（1）

| 方法 | API | 状态 | 已实现行为与限制 | 入口 |
|---|---|---|---|---|
| `initialized` | 默认 | 已接入 | initialize 成功后发送一次 params={}，无 id。 | `manager` |

### 服务端请求（11）

| 方法 | API | 状态 | 已实现行为与限制 | 入口 |
|---|---|---|---|---|
| `account/chatgptAuthTokens/refresh` | 默认 | 未接入 | 只校验收到的 previousAccountId／reason（当前仅 unauthorized），随后按原 id 回 `-32601` 错误、保持连接并记录诊断：本客户端只做 Codex 托管登录，没有可刷新的外部令牌，不伪造 accessToken／chatgptAccountId。 | `server_requests`、`manager/dispatch` |
| `applyPatchApproval` | 默认 | 部分接入 | 旧版文件审批：校验 callId／conversationId／fileChanges（add／delete／update 变体）后立即回 `{decision:{denied:{rejection}}}`，文案说明本客户端不为旧版协议提供审批 UI；不进 v2 展示与审批队列、无卡片，诊断记录登记方法、id 与 conversationId。 | `approvals`、`server_requests`、`manager/dispatch` |
| `attestation/generate` | 默认 | 未接入 | initialize 一直发送 requestAttestation=false；若到达则校验 params 为对象后按原 id 回 `-32601`、保持连接并记录诊断，不伪造 attestation token。 | `server_requests`、`manager/dispatch` |
| `currentTime/read` | 实验 | 已接入 | 校验字符串 params.threadId 后回 `{currentTimeAt}`，取本机时钟的 Unix 整秒；schema 未要求线程已加载，因此不做加载校验，也不缓存或伪造时间。 | `server_requests`、`manager/dispatch` |
| `execCommandApproval` | 默认 | 部分接入 | 旧版命令审批：校验 callId／command[]／conversationId／cwd／parsedCmd[]（read／list_files／search／unknown）后立即回 denied 回执；不复用 v2 命令审批卡片或决策队列，诊断记录登记方法、id 与 conversationId。 | `approvals`、`server_requests`、`manager/dispatch` |
| `item/commandExecution/requestApproval` | 默认 | 已接入 | kind 缺省为 command，支持 writeStdin；保留 approvalId、startedAtMs、nullable environmentId/cwd/command/reason、网络 host/protocol 与 additionalPermissions。availableDecisions 缺省／null 使用历史决策及服务端建议；显式空列表显示错误。按有序决策及完整策略载荷校验 accept、acceptForSession、decline、cancel、execpolicy 和网络 allow/deny，拒绝未提供的决策；cancel 不改写为 decline。 | `approvals`、`requests`、`registry` |
| `item/fileChange/requestApproval` | 默认 | 已接入 | 校验 threadId/turnId/itemId/startedAtMs，保留 nullable reason/grantRoot。原始 item changes 到达前仅允许拒绝；支持 accept、acceptForSession、decline、cancel，原 id 回传并等待 resolved。文件行打开对应原始补丁；grantRoot 是 schema 标注的不稳定提示，不由客户端自行扩大写入权限。 | `approvals`、`requests`、`registry`、`dispatch` |
| `item/permissions/requestApproval` | 默认 | 已接入 | 校验 thread/turn/item、cwd、startedAtMs、nullable environmentId/reason；保留 read/write、entries、glob 深度、path/glob/special path 与 nullable network。允许只返回请求子集及 turn/session scope，拒绝返回空权限。 | `requests`、`permissions`、`registry` |
| `item/tool/call` | 默认 | 部分接入 | 通过 namespace + tool 注册表路由；内置工作区依赖与自动化工具在当前运行时诚实返回 success=false，未知工具同样返回协议合法失败，不伪造成功。 | `client_tools`、`server_requests`、`manager` |
| `item/tool/requestUserInput` | 默认 | 已接入 | 保留 question id/header/question/options/isOther/isSecret、isBlocking、nullable autoResolutionMs；返回 question id → 字符串数组的 answers，Debug 隐去答案；兼容 tool/requestUserInput 别名。 | `requests`、`registry` |
| `mcpServer/elicitation/request` | 默认 | 已接入 | 只支持标准 MCP `mode=form` 与 `mode=url`；请求由 connection generation + 原始 request id 拥有，不绑定 turn，缺省／null／活动／已完成 turn 与 side conversation 都可展示与回复；`openai/form`、`openaiForm`、`openai/userVerification` 按协议错误回 `-32602` 并终止连接。 | `elicitation`、`manager/dispatch`、`manager/connection`、`conversation/elicitation`、`mcp_elicitation` |

### 服务端通知（81）

| 方法 | API | 状态 | 已实现行为与限制 | 入口 |
|---|---|---|---|---|
| `account/login/completed` | 默认 | 已接入 | 按 loginId 关联当前登录；支持 nullable loginId（仅在单个登录进行中时归属）与 onboardingEntrypoint 校验；取消后的迟到完成、重复通知均被忽略；成功后重读账户与配额。 | `manager/dispatch`、`manager/account` |
| `account/rateLimits/updated` | 默认 | 已接入 | 应用级稀疏补丁：按 accountId + limitId 合并单桶，nullable/缺省字段不清除已确认值，不影响其他桶或其他会话，也不结束活动轮次；账户切换、登出与 generation 变化会清理旧快照。 | `manager/dispatch`、`manager/account` |
| `account/updated` | 默认 | 已接入 | 应用级通知，不绑定 thread/turn；nullable authMode/planType 只表示当前不可用；进入连接事件快照并支持新订阅者回放。 | `manager/dispatch`、`manager/account` |
| `app/list/updated` | 默认 | 已接入 | 只作缓存失效信号：载荷仍完整解码（形状变化照样报错），但不覆盖屏幕上已有目录、不清空进行中的操作；应用分段可见时才重新执行 `app/list`。 | `manager/dispatch`、`apps` |
| `autoApprovalReview/strictReviewRequired` | 默认 | 已接入 | 按 thread/turn/startedAtMs 保存独立复核提示，同一时间去重；只展示额外安全检查状态，无 request id 或人工审批 responder，不改变 turn 终态。 | `auto_approval`、`manager/dispatch` |
| `command/exec/outputDelta` | 默认 | 未接入 | — | — |
| `configWarning` | 默认 | 已接入 | 应用级 summary 及可选 details/path/range；无活动轮次仍显示配置警告。 | `manager/dispatch`、`notifications` |
| `deprecationNotice` | 默认 | 后端已接入 | 应用级 summary 与 optional/nullable details；无活动线程也接收、去重并向新订阅者重放。独立于会话内容保存。当前 ChatGPT 接收并保存该通知，未观察到首页／会话页可见提示；GPUI 不新增无参考的提示卡。 | `runtime`、`manager/events` |
| `error` | 默认 | 已接入 | 定向轮次的 error.message、details、willRetry；显示错误信息，终态仍等待 turn/completed。 | `notifications` |
| `externalAgentConfig/import/completed` | 默认 | 未接入 | — | — |
| `externalAgentConfig/import/progress` | 默认 | 未接入 | — | — |
| `fs/changed` | 默认 | 未接入 | — | — |
| `fuzzyFileSearch/sessionCompleted` | 默认 | 已接入 | 标记当前查询结果结束。 | `manager` |
| `fuzzyFileSearch/sessionUpdated` | 默认 | 已接入 | 按 sessionId 路由到拥有该会话的弹窗；未知会话报协议错误，已退休的会话忽略。 | `manager` |
| `guardianWarning` | 默认 | 已接入 | 线程级 message，允许无活动轮次；同一当前轮次去重，通用消息保留原文，反复拒绝提示显示状态分隔行。schema 无 turnId/reviewId，不推定归属或终态。 | `auto_approval`、`manager/dispatch` |
| `hook/completed` | 默认 | 部分接入 | 同一 run.id 原位收敛，保留 running/completed/failed/blocked/stopped 原始状态与实际收到 completed 的标记，不由方法名推断成功。匹配已结束轮次且存在回复操作栏时显示钩子图标和运行详情浮层；无 turnId 或没有对应回复操作栏的展示路径未完成。 | `agent/runtime/state`、`home/runtime` |
| `hook/started` | 默认 | 部分接入 | 按 generation/threadId/run.id 和实际提供的 optional/nullable turnId 建模，保留完整运行身份、来源、事件、执行模式、状态、输出及时间。支持无活动 turn；不抢占 pending turn/start。运行中的独立提示在 ChatGPT 参考中不可见；无 turnId 记录目前只有运行时状态。 | `runtime`、`manager/dispatch` |
| `item/agentMessage/delta` | 默认 | 已接入 | 按 thread/turn/item 追加 delta，进入所属消息；8 ms 批次只合并相邻同 item 的文本，保留生命周期边界。视图按 ChatGPT 的自适应节奏揭示尚未完成的消息：每 50 ms 按跟随积压的速率放出字符，新出现的词、行内代码与链接以 0.7 s 淡入，列表项、表格行、引用与分割线以 0.15 s 淡入；解析前按参考修复未写完的尾部：隐藏最后一行未闭合的链接、写到一半的图片和未结束的引用标记，并补上悬空的 `*`/`**`；代码围栏未闭合时不改动。完成快照到达即显示全文并停止动画，减少动态效果时直接显示。 | `notifications` |
| `item/autoApprovalReview/completed` | 默认 | 已接入 | 以 threadId/turnId/reviewId 原位更新；保留完整 action、nullable targetItemId/rationale/riskLevel/userAuthorization、startedAtMs/completedAtMs 与 decisionSource=agent，并原样保存 params 供批准拒绝使用。处理 approved/denied/timedOut/aborted，不替代 turn/completed。 | `auto_approval`、`notifications`、`manager/dispatch` |
| `item/autoApprovalReview/started` | 默认 | 已接入 | 支持 command/execve/writeStdin/applyPatch/networkAccess/mcpToolCall/requestPermissions 七类动作及共享的五种状态；targetItemId 可缺失或为 null。开始通知不得覆盖已完成结果。 | `auto_approval`、`notifications`、`manager/dispatch` |
| `item/commandExecution/outputDelta` | 默认 | 已接入 | 按 itemId 追加命令输出 delta。 | `dispatch` |
| `item/commandExecution/terminalInteraction` | 默认 | 已接入 | 保留 itemId、processId；stdin 仅转为“是否写入”的布尔值，正文不进入领域或 UI 状态；复用命令活动。 | `dispatch` |
| `item/completed` | 默认 | 已接入 | 全部 19 个 ThreadItem 类型见“Item 与历史兼容”；以 item 载荷状态更新，不以通知名称推定成功。已完成的 item 保持终态，重复完成幂等，不结束 turn。未知实时类型使连接失败。 | `dispatch`、`items` |
| `item/fileChange/outputDelta` | 默认 | 已接入 | deprecated；校验 thread/turn/item/delta，不再产生内容事件。 | `dispatch` |
| `item/fileChange/patchUpdated` | 默认 | 已接入 | 按 itemId 替换 changes[path/diff/kind]，刷新文件卡与差异统计。 | `dispatch`、`items` |
| `item/mcpToolCall/progress` | 默认 | 已接入 | 按 itemId 追加 message，完成快照保留进度；孤立 progress 不创建工具项。 | `dispatch` |
| `item/plan/delta` | 默认 | 已接入 | 按 thread/turn/item 归属累加计划文本；支持早到增量，最终 item 权威覆盖；终态后忽略迟到增量。 | `progress`、`dispatch` |
| `item/reasoning/summaryPartAdded` | 默认 | 已接入 | 按 itemId 和非负 summaryIndex 建立槽位；孤立增量不创建 reasoning 项。 | `dispatch` |
| `item/reasoning/summaryTextDelta` | 默认 | 已接入 | 按 itemId/summaryIndex 追加 delta；仅合并相邻且同 item/index 的事件。 | `dispatch` |
| `item/reasoning/textDelta` | 默认 | 已接入 | 按 itemId/contentIndex 追加 delta；summary 为空时以 content 展示正文。 | `dispatch` |
| `item/started` | 默认 | 已接入 | 全部 19 个 ThreadItem 类型见“Item 与历史兼容”；创建或原位更新活动，已完成 item 的迟到 started 不重新激活。userMessage 按 item.id/clientId 关联提交并原位去重；未知实时类型使连接失败。 | `dispatch`、`items` |
| `mcpServer/event/stream/notification` | 默认 | 未接入 | — | — |
| `mcpServer/oauthLogin/completed` | 默认 | 已接入 | 按 name/threadId 关联本客户端启动的 loginId；迟到、已取消或本客户端未启动的完成通知解码后不再影响状态；成功后只做轻量状态读。 | `manager/mcp`、`mcp` |
| `mcpServer/startupStatus/updated` | 默认 | 已接入 | 按 app 或 thread/server 保存 starting/ready/failed/cancelled；threadId/error/failureReason 可省略或 null，仅接受 reauthenticationRequired 原因；不结束 turn。 | `manager/dispatch`、`notifications` |
| `model/rerouted` | 默认 | 已接入 | 定向轮次的 fromModel/toModel/reason，更新实际模型与提示。 | `notifications` |
| `model/safetyBuffering/updated` | 默认 | 已接入 | 保留 model/useCases/reasons/showBufferingUi、nullable fasterModel；更新所属会话的安全检查状态。 | `notifications` |
| `model/verification` | 默认 | 已接入 | 读取 verifications[]；目标 Composer 显示账户验证要求并进入失败状态。 | `notifications` |
| `modelProvider/authRecoveryCompleted` | 默认 | 后端已接入 | 保留完成 message 与原 started message；只停止该身份的认证等待，不代表请求成功或 turn 完成。迟到 started 不撤销完成结果；本地收束原因与服务端结果分别保留。 | `runtime`、`agent/runtime/state` |
| `modelProvider/authRecoveryStarted` | 默认 | 后端已接入 | 按 generation/threadId/turnId/provider 保存恢复中的 message，不绑定 pending turn/start；早到与重复事件幂等归约。当前 ChatGPT 退订此通知，无对应可见 UI。 | `runtime`、`agent/runtime/state` |
| `process/exited` | 默认 | 未接入 | — | — |
| `process/outputDelta` | 默认 | 未接入 | — | — |
| `project/changed` | 默认 | 已接入 | projectId、created/updated/deleted；刷新或移除项目，可先于 RPC 响应。 | `manager/dispatch` |
| `remoteControl/status/changed` | 默认 | 后端已接入 | 校验 status/serverName/installationId、nullable environmentId，保存连接快照；无 Composer UI。 | `manager/dispatch`、`notifications` |
| `serverRequest/resolved` | 默认 | 已接入 | 按原类型 requestId 找到所属轮次，再核对 thread/item/kind，释放命令／文件／权限审批或输入 responder 与活动 owner。已知同线程的重复及终态后迟到通知幂等忽略；未知 id 或错配 thread 报错；过期 handle 始终不可回复。 | `manager/dispatch`、`manager/connection`、`requests`、`registry` |
| `skills/changed` | 默认 | 已接入 | 作为失效信号使技能缓存过期并重新执行 `skills/list`；不携带可消费载荷，不覆盖较新的本地写入结果，也不清空用户正在编辑的状态。 | `manager/dispatch` |
| `thread/archived` | 默认 | 已接入 | 按 threadId 移除最近、项目及置顶条目，刷新归档；覆盖迟到快照。 | `manager/dispatch` |
| `thread/closed` | 默认 | 已接入 | 从当前 generation 的已加载集合移除并发布关闭状态；侧边聊天保留消息，禁用发送。 | `manager/dispatch` |
| `thread/compacted` | 默认 | 兼容退订 | 按本机 schema 的 Deprecated: Use ContextCompaction item type instead 说明退订；继续通过 contextCompaction item 展示，避免双重活动。 | `runtime::OPT_OUT_NOTIFICATION_METHODS` |
| `thread/deleted` | 默认 | 已接入 | 按 threadId 从所有集合移除；迟到列表不得恢复已删除线程。 | `manager/dispatch` |
| `thread/environment/connected` | 默认 | 未接入 | — | — |
| `thread/environment/disconnected` | 默认 | 未接入 | — | — |
| `thread/goal/cleared` | 默认 | 已接入 | 校验字符串 threadId、不得带 id；作为连接事件发布，会话清除该线程的目标快照；已完成目标在回复操作行上的「已在 … 内达成目标」标记不受清除影响。任何时刻到达都正常处理，不再有 resume 特例窗口。 | `manager/dispatch`、`goal` |
| `thread/goal/updated` | 默认 | 已接入 | 校验完整 ThreadGoal、六种 status 与 nullable turnId，goal.threadId 必须与 params 一致；按 updatedAt 单调更新快照，新的 complete 触发自动清除。 | `manager/dispatch`、`goal` |
| `thread/name/updated` | 默认 | 已接入 | threadId、可省略或 null 的 threadName；即时更新名称并覆盖迟到快照。 | `manager/dispatch` |
| `thread/project/updated` | 默认 | 已接入 | threadId、必需但 nullable 的 projectId；移动或移出项目并覆盖迟到快照。 | `manager/dispatch` |
| `thread/queue/changed` | 默认 | 已接入 | 只含 threadId 的失效信号；显示该线程的会话重新 list，其他线程与旧 generation 忽略。 | `manager/dispatch`、`queue` |
| `thread/realtime/closed` | 默认 | 未接入 | — | — |
| `thread/realtime/error` | 默认 | 未接入 | — | — |
| `thread/realtime/item/completed` | 默认 | 未接入 | — | — |
| `thread/realtime/item/started` | 默认 | 未接入 | — | — |
| `thread/realtime/item/transcript/delta` | 默认 | 未接入 | — | — |
| `thread/realtime/itemAdded` | 默认 | 未接入 | — | — |
| `thread/realtime/outputAudio/delta` | 默认 | 未接入 | — | — |
| `thread/realtime/sdp` | 默认 | 未接入 | — | — |
| `thread/realtime/started` | 默认 | 未接入 | — | — |
| `thread/realtime/transcript/delta` | 默认 | 未接入 | — | — |
| `thread/realtime/transcript/done` | 默认 | 未接入 | — | — |
| `thread/reverted` | 默认 | 已接入 | 仅 threadId；可先于 thread/revert 响应到达，按“通知先于 RPC 响应”暂存为该请求的确认。非本客户端发起的回退作为连接事件发布，令所属会话把已归约历史标记为过期并按分页路径重载，不改变侧栏条目身份。 | `manager/dispatch`、`manager/revert`、`manager/connection` |
| `thread/settings/updated` | 默认 | 已接入 | 按原 threadId／generation 同步 model/effort/serviceTier/cwd 与有效权限；匹配本次期望值才满足 waiter，处理响应前通知、重复／已知迟到回执及关闭临时线程。 | `manager/dispatch`、`manager/settings`、`notifications` |
| `thread/started` | 默认 | 已接入 | 校验 params.thread.id，关联当前 start/resume/fork；RPC 响应是最终 id 来源。已加载线程的迟到通知不得绑定到下一次生命周期请求。 | `manager/dispatch` |
| `thread/status/changed` | 默认 | 已接入 | 按 threadId 保存 notLoaded/idle/systemError/active；active 仅接受 waitingOnApproval/waitingOnUserInput，不替代 turn 终态。活动视图据此区分进行中与待处理，并记录客户端维护的未读状态（见“连接与状态”）。 | `manager/dispatch`、`notifications` |
| `thread/tokenUsage/updated` | 默认 | 已接入 | 连接级分发，允许 resume 响应前上报已结束轮次的用量；按 threadId/turnId 保存 tokenUsage.total/last 与可选 context window，不绑定运行轮次、不创建活动或结束轮次。 | `notifications` |
| `thread/unarchived` | 默认 | 已接入 | 从归档移除，刷新最近及项目列表；覆盖迟到快照。 | `manager/dispatch` |
| `turn/completed` | 默认 | 已接入 | 接受 completed/interrupted/failed；失败读取 message/details。保留服务端可选 startedAt、completedAt、durationMs，实时完成沿用历史的时间标签与用时；缺失时不推算用时。每轮只发送一个终态并清理自身请求，其他轮次及共享连接继续存活。 | `dispatch`、`manager/connection` |
| `turn/diff/updated` | 默认 | 已接入 | 所属轮次最新聚合 unified diff；保留原始 patch，刷新文件卡与“上一轮”范围；空 diff 不清除已有 item changes。 | `dispatch` |
| `turn/moderationMetadata` | 默认 | 兼容退订 | 完整方法名退订；metadata 为任意 JSON，当前无消费路径。保留 error、model/safetyBuffering/updated、model/verification 等已接入状态，不用 metadata 推定成功或终态。 | `runtime::OPT_OUT_NOTIFICATION_METHODS` |
| `turn/plan/updated` | 默认 | 已接入 | 独立 turn 步骤快照与 explanation；输入框上方显示步骤进度，悬停／点击／键盘查看步骤。 | `progress`、`dispatch` |
| `turn/started` | 默认 | 已接入 | 要求 turn.status=inProgress；可早于 turn/start 响应，验证后使所属会话进入流式状态。没有本地 owner 时（目标推进、队列推进、queue/start）接管为服务端轮次并经 `TurnStarted` 交给会话；会话忙时排队到当前轮次结束再接上。 | `notifications`、`manager/turn`、`manager/external_turn` |
| `warning` | 默认 | 已接入 | message、可选 threadId；应用级警告无活动轮次仍可见，线程级只进入目标 Composer。 | `manager/dispatch`、`notifications` |
| `windows/worldWritableWarning` | 默认 | 未接入 | — | — |
| `windowsSandbox/setupCompleted` | 默认 | 未接入 | — | — |

## 维护与验证

修改方法、有效变体、兼容别名或失败处理时，同步更新本表与对应测试；升级 CLI 时核对四个 schema union（ClientRequest、ServerRequest、ClientNotification、ServerNotification），保持方法唯一、方向／API 分类和状态统计一致。只有形成表中声明的产品路径后才标记“已接入”。

批次一（协作模式、服务端排队、线程目标、自动复核批准）的本机基线行为可用 `python3 scripts/batch1_app_server_probe.py --output artifacts/batch1-baseline-<日期>` 复现：它以隔离的 CODEX_HOME 和本地假 Responses 端点运行 PATH 上的 `codex app-server`，不发真实模型请求；截图对比用 `python3 scripts/compare_batch1_captures.py`。批次二（轮次设置、查找、钩子、实验性功能、记忆）的基线用 `python3 scripts/batch2_app_server_probe.py --output artifacts/batch2-baseline-<日期>` 复现，同样不发真实模型请求；参考截图用 `node scripts/cdp_capture_batch2.mjs`（专用参考实例），Echora 截图用 `ECHORA_CODEX_HOME=<~/.codex 的副本> scripts/capture_batch2_gpui.sh <日期>`。

`scripts/verify_integration_table.mjs` 直接用 CLI schema 重新推导方法集合、默认／实验归属、方法唯一性、口径表统计、合计行与 `runtime::OPT_OUT_NOTIFICATION_METHODS`，发现任何结构性不一致都以非零状态退出；`artifacts/app-server-schema` 缺失时会先用本机 `codex` 生成临时副本，因此可在干净检出上直接运行。

```bash
node scripts/verify_integration_table.mjs
cargo test agent::codex
cargo test workspace::
cargo test conversation::
cargo test components::composer
cargo test side_ -- --test-threads=1
```

协议解析与反向请求回归在 `src/agent/codex/tests.rs`；共享进程、乱序响应、线程隔离、清理与退出回收在 `manager/tests.rs`，临时侧边线程在 `manager/tests/side_conversation.rs`。实际模型请求测试默认忽略；常规回归使用 scripted transport 或 fake backend。构建与界面验收入口见 [README.md](../README.md)。
