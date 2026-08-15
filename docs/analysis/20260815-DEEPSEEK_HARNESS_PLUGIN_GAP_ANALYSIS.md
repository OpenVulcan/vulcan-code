# DeepSeek Harness 与 Vulcan Code 插件机制差距分析

> 状态：分析完成（只读源码对照）  
> 日期：2026-08-15  
> 对比仓库：`D:\dsh\deepseek-harness`、`D:\openvulcan\vulcan-code`

## 一、结论摘要

用户关于“差距很大”的判断方向正确，但差距不主要来自插件数量、TypeScript 与 Lua 的语言差异，也不只是缺少若干钩子。真正的差距是**宿主控制面的组织方式**：

- DeepSeek Harness 把模型适配、工具注册、会话日志、Agent Loop、系统提示词、UI、命令、任务、文件系统等都组织为 Cordis 插件图。注册即副作用，副作用隶属于插件 Fiber，插件卸载时可以统一撤销。
- Vulcan Code 已经有工程质量较高的插件包运行时：清单、内容指纹、权限、依赖拓扑、生命周期、Lua 沙箱、进程资源、MCP、工具、动作、UI、配置、密钥、存储与失效代际都比较完整；但插件仍是挂接到**固定宿主预留表面**上的能力包，不能组合或替换大多数宿主服务。
- 因此，只新增十几个事件名不能真正补齐差距。应先补齐作用域、注册租约、类型化点位、分发策略、能力授权、可回放审计和动态贡献注册表，再分批增加高价值钩子。

推荐目标不是照搬“everything is a plugin”，而是形成两层架构：

1. **特权 Rust Kernel**：保留协议路由、凭据、安全策略、会话持久化格式、Provider 物理传输、沙箱与授权等高风险事实边界。
2. **Extension Kernel**：把工具、提示词片段、上下文、命令、任务、UI 槽位、Agent/会话生命周期和受控 Provider 扩展组织为可作用域化、可卸载、可审计的贡献图。

## 二、事实边界

- `deepseek-harness` 当前工作树干净；分析依据是当前 `master` 源码与仓库文档。
- `vulcan-code` 当前存在与本分析无关的并行改动；本任务没有修改这些文件，也不将未提交状态视为稳定发布能力。
- 历史记录仅用于定位线索；结论以当前源码、类型定义、Schema 与调用链为准。
- 本报告严格区分 Vulcan 宿主插件、LuaSkills、MCP 工具与 Provider Manifest，它们不是同一扩展层。

## 三、DeepSeek Harness 的插件机制

### 3.1 运行时本体是插件图

DeepSeek Harness 的 `AGENTS.md` 明确规定“everything is a plugin”。这一点在源码中不是口号：模型适配器、工具注册表、会话日志、Agent Loop、系统提示词、UI 等都挂在 Cordis `Context` 上。运行实例由 Bundle/Profile/Patch 组合出一棵有顺序的插件树，配置行可以按 ID 整行替换。

Cordis 提供的关键原语包括：

- `ctx.plugin()`：创建插件 Fiber；
- `ctx.effect()`、`ctx.on()`：把资源和监听器登记为 Fiber 副作用；
- `dispose/restart/update/reload/unload`：统一管理副作用生命周期；
- `extend/isolate/intercept`：创建继承、隔离或拦截后的上下文；
- `emit/parallel/serial/bail/waterfall`：由点位选择分发语义；
- `ctx.inject()`：声明服务依赖并等待服务就绪。

这使插件不只是“收到事件后运行脚本”，而是能贡献服务、消费服务，并由运行时管理依赖、作用域和清理。

### 3.2 事件分为持久事实、实时过程和能力变化

DeepSeek Harness 把事件划分为三类：

- Session Events：追加到会话日志，是可回放事实，也是模型上下文的来源；
- Agent Events：仅描述当前执行过程；
- Capability Events：工具、技能、设置、子 Agent Provider 等能力集合发生变化。

当前生成的生产者/消费者矩阵列出了约 50 个事件，包括 Agent 请求、工具执行前后、会话创建/销毁/刷新、子 Agent 生命周期、系统提示词装配、文件写入意图、审批、设置、技能与工具目录变化等。其关键规则是：**任何对模型可见的输入都必须进入 Session Event**，不能只存在于瞬时回调中。

### 3.3 Turn 流程中存在可变换和可阻断的瀑布点

典型流程包含：

1. `agent/pre-step`：可拒绝当前步骤或改写步骤输入；
2. `agent/request`：请求装配；
3. `llm/stream`：流式响应；
4. `tools/pre-execute`、`tools/execute`、`tools/post-execute`：工具前置、执行与后置处理；
5. Session Event：将有效结果写入会话日志。

因此插件可以在一次 Turn 的关键控制点参与决策，而不仅是收到“已经完成”的通知。

### 3.4 UI 也是类型化注册表

`ui-slots` 不是三个固定挂载点，而是类型化槽位注册表：

- 槽位有 `single/list/keyed/chain` 等组合语义；
- 支持根级、可能有会话、确定有会话等作用域；
- 注册返回 disposer；
- 支持优先级、覆盖、子槽位递归清理、变更订阅和单个入口崩溃隔离。

这使 UI 的扩展方式与宿主服务、工具和事件的注册模式一致。

### 3.5 可通过工具动态定义并运行 Cordis 插件

`tool-cordis` 和 `cordis-host-runner` 允许在审批、不可变包版本、受限上下文、诊断和撤回机制下定义并运行宿主/客户端插件。这是比 Vulcan 当前 Lua 沙箱更强的自扩展能力，也意味着更大的攻击面和故障半径，不能直接照搬。

### 3.6 DeepSeek Harness 自身仍有未闭环处

源码中的 Codex/Claude Code Hook 兼容桥仍存在明确 TODO：运行级停止、首次请求启动门、Stop 循环保护以及部分 hook 输出字段没有完整映射。因此 DeepSeek 的机制更开放、更一致，但并非所有桥接语义都已经完备。

## 四、Vulcan Code 当前插件机制

### 4.1 已具备的强项

Vulcan 的插件包与运行时已经覆盖：

- 清单版本、平台、必需文件、依赖和内容 SHA-256 指纹；
- Bundled/User 来源、同名遮蔽规则、不可变目录快照和失效代际拒绝；
- 权限声明：Lua 模块、HTTPS、二进制/Node/Python/会话进程、配置、密钥、UI 上下文、模型目录/调用、依赖网络、SQLite/LanceDB；
- 安装、更新、提交、回滚、启用、Drain、禁用、卸载与生命周期回调；
- Stateless/Session 工具、Actions、MCP Server、静态提示词、Skills、Agents、配置和本地化；
- 固定 Lua Bridge、声明函数校验、私有执行环境、进程资源与插件级存储；
- WebUI 动态插件 SDK 和已审批外部 URL；
- 插件快照代际和运行中陈旧授权拒绝。

这部分在供应链完整性、权限隔离、资源治理和失效一致性上比 DeepSeek 的通用 Cordis 模型更保守，也更适合承载不完全可信的第三方插件。

### 4.2 当前钩子模型

`vulcan-hooks` 已定义四种行为模式：`Observe`、`Transform`、`Decision`、`Effect`；四种作用域：`Global`、`Workspace`、`Project`、`Session`；三种失败策略：`Continue`、`Stop`、`Queue`。结果对象可以返回替换载荷、决策、注入、系统提示词块、诊断和元数据。

但宿主目前只发布 18 个点位：

1. `conversation.profile.prepare`
2. `conversation.turn.precheck`
3. `conversation.user.event`
4. 提问 asked/answered/rejected
5. 工具 started/finished
6. Turn completed/terminated/compacted
7. Provider started/dispatched/first-token/completed/retrying
8. `tool.shell_command.before_execute`
9. `git.commit_summary.context`

其中没有一个点位允许 `Decision`；通用工具 started/finished 只能 Observe/Effect，只有 `shell_command` 有专用 Transform 点。

### 4.3 类型设计与运行时执行仍有断层

源码核验得到以下确定差距：

- 注册表只按 `point` 取监听器，没有按 Workspace/Project/Session ID 做实际过滤；当前 `HookScope` 主要是元数据。
- `requested_capabilities` 会写入注册项，但没有找到将其与插件权限、点位允许能力和用户策略求交后写入 `granted_capabilities` 的生产实现；调用上下文目前使用空能力集。
- `payload_schema` 类型存在，但宿主描述符没有配置 Schema，分发前后也没有运行时验证。
- `apply_result` 会应用 payload、decision、injections 等字段，没有依据注册的 `HookMode` 限制结果能力；模式只限制“能不能注册到该点”。
- `Queue` 失败策略只将报告标记为 `has_queueable_failure`，没有找到真正的队列、Outbox、重试与恢复消费者。
- 分发器固定串行并在第一个非 Continue 决策停止，没有点位级的 `parallel/serial/waterfall/bail` 策略，也没有明确的多决策合并规则。
- 插件级持久 Lua Hook Runtime 使用一个 `AsyncMutex` 串行化该插件的全部 Hook 调用，安全但会造成跨会话头阻塞。

### 4.4 当前宿主仍是固定编排

服务启动流程按固定顺序准备插件运行时、预热 Hook、初始化 MCP/VMM 并同步插件 Hook。Lua Bridge 只暴露纯 Lua、`vulcan.host.call`、声明的进程入口、JSON 辅助和白名单模块，没有通用的服务定义、服务 Provider、上下文派生或贡献注册 API。

因此 Vulcan 当前是“固定宿主 + 受控插件表面”，不是“由插件图组装宿主”。

### 4.5 Lua 能主动调用宿主，但没有核心服务消费层

“Lua 只能承载宿主传入的信息，不能主动调用核心函数”接近事实，但需要区分两层：

- Lua 确实可以主动调用 `vulcan.host.call`，也可以通过 `vulcan.process.exec/session.open` 启动清单声明的进程；
- 目前 `dispatch_plugin_host_tool_request` 只列出 14 个固定宿主调用：进程规格解析、配置读取/更新、5 个 SQLite 操作、5 个 LanceDB 操作和 UI 失效通知；
- `VulcanPluginHostServices` 实际只安装 Storage 与 UI Event Publisher 两类宿主持有服务；
- 没有会话、Conversation、Agent/Subagent、模型调用、工具调用、审批、命令或后台任务等核心服务消费者接口；
- Manifest 虽已定义 `model_catalog_read`、`model_invoke` 权限名称，但当前源码没有与之对应的 Host Tool/Host Service 实现。

所以当前不是“零主动调用”，而是只有**基础设施窄 RPC**。Hooks、Tools、Actions 都主要由宿主进入 Lua；Lua 只能在这次回调内部处理载荷并返回 JSON，无法把核心服务作为受控依赖主动消费。

此外，现有 Host Tool Callback 是同步函数。插件调用运行在 `spawn_blocking` 中，并持有 Lua Engine Mutex 和插件 Hook `operation_gate`。若简单地在回调中阻塞等待异步 Service Kernel，很容易出现长时间占锁、跨会话头阻塞或“核心调用再次触发同插件”的重入死锁。因此不能靠继续硬编码函数名解决。

完整扩展面应明确分成三个方向：

| 方向 | 机制 | 语义 |
| --- | --- | --- |
| Host → Plugin | Hook/Tool/Action/Lifecycle | 宿主在明确边界调用插件 |
| Plugin → Host | Host Capability Service | 插件主动查询或请求受控核心能力 |
| Plugin → Host Registry | Contribution Registration | 插件长期注册工具、上下文、命令、任务和 UI 槽位 |

## 五、逐项差距矩阵

| 维度 | DeepSeek Harness | Vulcan Code 当前 | 判断与补齐方向 |
| --- | --- | --- | --- |
| 组合根 | Bundle/Profile/Patch 生成插件树 | Service Bootstrap 固定编排 | 新增 Extension Kernel，不先改写 Rust Kernel |
| 服务扩展 | Definition/Provider/Consumer，可注入依赖 | 固定 Host Services | 增加受控贡献注册表和依赖图 |
| 插件主动调用 | 通过 Context 消费已注入服务 | 14 个同步、硬编码基础设施 Host Tool | 增加类型化 Host Capability Service 与异步 ABI |
| 生命周期 | 所有注册归属 Fiber，统一 Dispose | 各类贡献分别同步/清理 | 建立统一 Registration Lease/Effect Ledger |
| 作用域 | Context 派生、隔离和 Agent 子上下文 | Scope 枚举存在但未按实例过滤 | 先让作用域成为真实运行语义 |
| 事件与钩子 | 多种分发语义，覆盖 Turn/工具/会话/FS/UI 等 | 18 点，偏后置观测 | 增加通用控制点和点位级分发策略 |
| 类型契约 | TypeScript 声明合并和明确事件负载 | Rust 类型外层 + JSON payload | 为每个点位提供版本化输入/输出 Schema |
| 模型可见性 | 模型可见输入必须写 Session Event | Hook 注入和持久事件仍是两条链 | 增加有效贡献的持久审计事件 |
| UI | 通用类型化 Slot Tree | 3 个固定 Mount Point | 增加 Slot Registry，保留旧挂载点作别名 |
| 动态自扩展 | 可审批运行 Cordis 代码 | 固定 Lua Bridge/声明函数 | 不直接开放任意宿主代码；先开放声明式贡献 |
| 安全与完整性 | 运行时灵活，动态代码面更大 | 指纹、权限、代际、沙箱更强 | 保留 Vulcan 安全基线，不用开放性换覆盖率 |
| 并发 | 注册和事件语义可按能力组织 | 插件级 Hook 全串行 | 增加按会话串行与有界并行 Lane |
| 热更新 | Fiber 和 EntryTree 原生支持更新/卸载 | 快照同步、Drain、代际拒绝 | 用统一租约连接现有 Drain 与各贡献注册表 |

## 六、不能只加钩子：必须先补的十项内核能力

### 6.1 真实作用域目标

把作用域枚举改为带身份的注册目标，例如：

```text
Global
Workspace(workspace_id)
Project(project_id)
Session(session_id)
Agent(agent_id)
```

调用上下文必须稳定携带 `workspace_id/project_id/session_id/agent_id/turn_id/request_id`。注册表先按点位，再按作用域祖先链筛选；不能让插件脚本自行猜测归属。

### 6.2 点位级分发策略

`HookMode` 描述监听器能做什么，分发策略描述多个监听器如何组合，两者必须分离。建议支持：

- `Waterfall`：按顺序变换载荷；
- `SerialNotify`：顺序通知但不改变主流程；
- `ParallelNotify`：互不依赖的遥测并行；
- `Bail`：首个明确结果结束；
- `DecisionMerge`：显式选择首个非 Continue 或最严格决策。

每个点位固定策略，插件不得自行改变。

### 6.3 模式与结果能力强制对应

- Observe：只允许诊断和审计元数据；
- Transform：只允许声明的 payload 字段、注入或提示词块；
- Decision：只允许决策、原因和点位批准的补充信息；
- Effect：不允许改写 payload 或决策。

非法字段应作为插件契约错误显式失败，不能静默忽略。

### 6.4 版本化输入/输出 Schema

每个点位提供 Host-owned JSON Schema，输入分发前验证，Transform/Decision 返回后再次验证。宿主身份、协议路由、凭据句柄、权限判定、会话 ID 等字段必须标记为不可改写。工具参数被改写后必须重新经过该工具原 Schema 和权限校验。

### 6.5 能力授权求交

有效能力应按下式计算：

```text
granted = requested ∩ plugin_permissions ∩ point_allowed ∩ user_policy
```

只把 `granted` 写入调用上下文；宿主服务调用也按同一集合检查。不能把 Manifest 声明直接视为授权。

### 6.6 真正实现 Queue 或删除该语义

若保留 `Queue`，至少需要持久 Outbox、幂等键、重试次数/退避、死信、恢复扫描和可观测状态。否则应改名为明确的“记录待处理失败”，避免制造已具备可靠队列的错觉。

### 6.7 模型可见贡献的可回放审计

任何改变模型输入、工具参数/结果或停止决策的 Hook，都应写入持久事件，至少包含：

- 插件 ID、版本、内容指纹和运行代际；
- Hook 点位、调用 ID、输入/输出摘要；
- 原始值和最终有效值的安全摘要；
- 决策原因与真正进入模型上下文的贡献。

要求从会话事件重放时能重建“模型实际看到了什么”。

### 6.8 统一注册租约与 Drain

Hook、Tool、Action、Prompt、UI Slot、Command、Job、订阅、定时器都返回统一 `RegistrationLease`，归属插件运行代际。禁用/更新时先停止新租约，再等待在途调用，最后反序注销。这样才能获得 Cordis Fiber 的核心收益，而不必复制 Cordis。

### 6.9 并发 Lane 和重入规则

建议提供：

- `serial_per_plugin`
- `serial_per_session`
- `parallel_bounded(n)`

默认变换/决策按会话串行，纯观测遥测可有界并行。必须禁止同一 Hook 同步重入，并定义插件调用 Host Service 再触发同点位时的环路保护。

### 6.10 类型化 Host Capability Service

保留 `vulcan.host.call` 作为 Lua ABI，但把当前硬编码分发器升级为宿主持有的类型化能力注册表。不能开放 `call_core(function_name, args)`，也不能把 Rust Service 对象或函数指针直接交给 Lua。

每项能力至少声明：

```text
id + api_version
query | command | task
input_schema + output_schema
required_permissions
allowed_scopes + allowed_callers
timeout + cancellation + idempotency
reentrancy_policy + audit_policy
```

调用时由宿主注入插件 ID、内容指纹、运行代际、Lease Scope、Workspace/Session/Agent/Turn ID，并重新核对权限与作用域。插件只提交业务参数，不能自行提供身份或绑定目标。

同步与异步必须分流：

- 快速只读 Query 可保留同步调用，但要有严格超时，且禁止回调期间再次进入同一插件；
- 长耗时 Command/Task 应立即返回稳定 Task Handle，由核心异步执行，并提供 status/cancel/result 和完成事件；
- 必须在同一次 Lua 调用中取得结果的异步能力，需要真正的 Lua Coroutine `yield → await → resume` ABI；在此能力完成前，禁止在 Lua Engine Mutex 内直接 `block_on` Service Kernel Future。

第一批建议开放的能力：

1. `workspace.context.read@v1`
2. `session.snapshot.read@v1`
3. `model.catalog.read@v1`
4. `approval.request@v1`
5. `notification.publish@v1`
6. `job.submit/status/cancel@v1`
7. `session.plugin_event.append@v1`，只能写插件命名空间下的版本化事件

第二批在异步 ABI、预算和审计闭环后再开放：

- `model.invoke@v1`
- `subagent.spawn@v1`
- `tool.invoke@v1`
- `conversation.context.contribute@v1`

这些接口必须调用 Service Kernel 的业务用例入口，不能绕过会话事实、审批、Provider 精确路由、工具权限或持久化约束。

## 七、建议新增的钩子

### 7.1 P0：第一阶段必须补齐

| 点位 | 模式 | 精确时机 | 关键约束 |
| --- | --- | --- | --- |
| `conversation.input.before_accept` | Transform/Decision | 输入完成规范化后、进入普通提交或恢复专属流程前 | 负载显式带 `submission_kind=normal/followup/steer/continue/resume`；普通提交不得读取或验证旧恢复快照 |
| `tool.execution.before_execute` | Transform/Decision | 工具已解析、参数初验与基础权限完成后，实际执行前 | 适用于所有工具；改写参数后重新做 Schema 与权限校验 |
| `tool.execution.after_execute` | Transform/Effect | 得到原始结果后，写入会话和投影给模型前 | 同时保留 original/effective 摘要，禁止伪造工具身份 |
| `tool.execution.failed` | Observe/Effect | 工具最终失败后 | 不允许把失败偷偷改成成功 |
| `conversation.turn.before_stop` | Decision | 模型准备结束 Turn、最终提交结束状态前 | 支持 Continue/Stop；限制连续强制 Continue 次数，防止无限循环 |
| `provider.request.before_dispatch` | Transform/Decision | 精确协议请求组装、预算和权限确认后，物理网络调用前 | 路由、协议、凭据和模型绑定为宿主只读；禁止协议探测与降级 |
| `provider.request.failed` | Observe/Effect | 一次物理请求失败且宿主完成分类后 | 首期不交给普通插件决定跨协议重试；只记录明确错误类别 |
| `subagent.before_spawn` | Transform/Decision | 确定父会话和任务后、创建子 Agent 前 | 可限制任务、工具/技能集合和预算，不能改变父会话归属 |
| `subagent.started/completed/failed/disposed` | Observe/Effect | 各真实生命周期边界 | 使用稳定 subagent ID，区分完成、失败与清理 |
| `session.created/resumed/forked/before_dispose/disposed` | Observe/Effect | 会话存储事实边界 | `before_dispose` 只允许有界 Drain，不允许无限阻止销毁 |
| `approval.requested/resolved` | Observe/Effect | 审批服务请求与结果落定时 | 插件不能静默代替用户批准高风险操作 |

`conversation.turn.precheck` 可继续承担画像、记忆和上下文注入，但不要把普通提交与 Continue/Resume 快照验证混在一起。

### 7.2 P1：第二阶段补齐生态能力

- Workspace：`workspace.opened`、`workspace.before_close`、`workspace.closed`；
- Agent：`agent.created`、`agent.status_changed`、`agent.before_dispose`、`agent.disposed`；
- 文件系统：`fs.write.intent`、`fs.edit.intent`、`fs.operation.observed`；
- 能力目录：`settings.updated`、`skills.catalog.changed`、`agents.catalog.changed`、`tools.catalog.changed`、`mcp.catalog.changed`；
- 命令：`command.before_execute`、`command.after_execute`、`command.failed`；
- 后台任务：`job.started`、`job.completed`、`job.failed`、`job.cancelled`；
- 消息提交：`assistant.message.before_commit`（仅可信插件 Transform）、`assistant.message.committed`（Observe/Effect）。

文件写入 Intent 应在精确目标和操作已解析、真正写盘之前发生；它不能接收候选路径，也不能通过多路 fallback 猜目标。

### 7.3 P2：高频流式点位最后开放

- `provider.stream.delta`
- `tool.stream.delta`
- `terminal.output`
- `file.watch.changed`

这些点位默认只能 Observe，且必须支持批处理、采样、背压和自动熔断，不能让普通插件阻塞 Token 流、终端输出或文件监听线程。

## 八、还应新增哪些“贡献注册表”

以下能力不应伪装成 Hook，因为它们是长期注册的能力，不是一次性事件：

1. **Context Contribution Registry**：类型化系统提示词区段、上下文材料和变量，声明优先级、可见条件、Token 预算和持久性。
2. **Command Registry**：命令定义、参数 Schema、权限和展示元数据。
3. **Job Registry**：后台任务类型、状态查询、取消和恢复契约。
4. **Backend Registry**：受控的搜索、文件、终端或外部服务后端；采用 Definition/Provider/Consumer 模式。
5. **UI Slot Registry**：支持 `single/list/keyed/chain`、作用域、优先级、子槽位、递归释放和崩溃隔离；现有三个 Mount Point 可作为兼容别名。
6. **Provider Contribution Registry**：第一阶段只允许声明式 Provider Manifest、模型目录和宿主批准的适配器，不开放插件任意替换物理传输或跨协议回退。

## 九、建议落地顺序

### 阶段 A：先建立双向 Host Capability Bridge

把现有 14 个固定 Host Tool 迁入类型化能力注册表，形成统一的 Definition、授权、调用上下文、Schema、超时、审计和注销契约。先实现同步只读 Query 与异步 Task Handle；需要同次返回异步结果的模型/工具调用，等待 Coroutine `yield/await/resume` ABI 完成后再开放。

### 阶段 B：修正 Hook Kernel 契约

先实现真实作用域、Schema、Mode/Result 强制、点位级分发策略、能力授权、Queue 语义和审计事件。没有这些基础，新增控制型 Hook 会把不确定性和安全风险放大。

### 阶段 C：打通 P0 通用控制点

建议优先落在现有唯一事实拥有者：

- 通用工具调用：`StreamingRuntimeToolExecutor::execute_tool` 与 `VulcanManagedToolRuntimeBridge::invoke_definition` 所在链路；
- Provider 请求：`build_provider_transport_execution_request`、`execute_host_controlled_stream` 与显式 Retry Policy 所在链路；
- Turn 启停：Service Kernel 的 `start_webui_session_kernel_turn_with_admission`、`finish_webui_session_kernel_turn`、`stop_webui_session_kernel_generation`；
- 子 Agent：Session Core 的 subagent registry 与 Service Kernel 的 workspace mutation 链路；
- 会话：Session Runtime Kernel 的创建、恢复、Fork 与 Dispose 边界。

钩子必须落在唯一事实拥有者，而不是在 WebUI、投影层或多个调用点重复补丁。

### 阶段 D：建立 Extension Scope 与统一租约

把插件代际、Workspace、Project、Session、Agent 组成作用域树；各贡献注册表接入同一个 Lease/Drain 模型。完成后再开放 Command、Job、Backend 和 UI Slot。

### 阶段 E：逐步插件化非关键宿主能力

先迁移风险较低且边界清晰的系统提示词区段、命令、任务、目录 Provider、UI 槽位和只读上下文 Provider。协议路由、凭据、会话存储格式、安全策略和物理 Provider Transport 暂时保留在特权 Kernel。

### 阶段 F：再评估可信原生插件层

只有当第三方生态确实需要替换高性能服务时，才设计“签名 + 明确信任级别 + 原生 ABI/WASM + 进程隔离”的可信扩展层。不要直接把 `tool-cordis` 式任意宿主代码执行带入普通插件权限域。

## 十、风险、非目标与验收标准

### 10.1 主要风险

- Hook 排序或决策合并不确定，导致相同会话不可复现；
- 高频同步 Hook 增加首 Token 和工具执行延迟；
- 插件改写 Provider 路由、协议或凭据，破坏声明后直接执行原则；
- 插件注入已影响模型，但没有落入会话事实，导致重放与实际不一致；
- 插件卸载后残留定时器、订阅、UI、进程或后台任务；
- 跨会话共用锁造成头阻塞；
- Hook 调用 Host Service 再触发自身造成死循环；
- UI 插件崩溃或越权读取会话上下文。

### 10.2 明确非目标

- 不把整个 Rust Service Bootstrap 一次性改成插件树；
- 不允许普通插件任意执行宿主原生代码；
- 不允许插件探测、轮询或回退模型协议；
- 不把 LuaSkills、MCP、Provider Manifest 和宿主插件合并成一个模糊概念；
- 不用新增 Hook 掩盖尚未查清的字段归属或调用链。

### 10.3 必须通过的验收

1. 加载、更新、禁用插件后，所有注册副作用严格创建、Drain、反序释放；
2. 多 Workspace/Session/Agent 并行时，作用域不会串扰；
3. 非法 Mode 输出、Schema 不匹配和宿主字段改写被显式拒绝；
4. 普通提交与 Continue/Resume 恢复契约保持隔离；
5. 工具参数/结果变换后二次校验，Provider 路由和协议不可被插件降级；
6. 会话事件重放能够还原模型实际看到的上下文和工具结果；
7. Continue/Stop/Queue 三种失败语义都有端到端测试和可观测状态；
8. 插件并发、超时、取消、重入、禁用 Drain 和陈旧代际均有压力测试；
9. UI Slot 子树能递归释放，单个插件崩溃不会拖垮宿主；
10. 内容指纹、权限、点位能力和用户策略四重授权在每次调用生效。

## 十一、达到 DSH 级别需要多少注册能力

### 11.1 不能用 Rust 函数数量一一对应

DeepSeek Harness 并不是把全部内部函数逐个注册给插件。它把能力组织为 Service Definition、Provider、Consumer，再通过 Context、作用域、事件和可撤销注册组合。其架构文档直接点名的基础能力已经包括 Session、System Prompt、Tools、Agents、Agent Loop、Scope、LLM、Shell、Subprocess、Terminal、Commands、Jobs、FS、Sandbox、Session Title、Goals；Base Bundle 还组合 Persistence、Approval、Settings、Credentials 和 Telemetry。

因此，DSH 级别的核心标准不是宿主预置一个固定的最终函数表，而是：

1. 宿主提供一组足以构建插件生态的内核服务；
2. 插件可以定义和提供新的类型化服务；
3. 其他插件可以按作用域发现并消费这些服务；
4. 全部注册随插件代际统一卸载。

如果 Vulcan 只把硬编码 Host Tool 从 14 个扩充到 100 个，但第三方仍不能注册新的 Service Provider，它仍然只是“大型固定 RPC 表”，还不是 DSH 式插件系统。

### 11.2 当前事实与建议目标

当前源码能确认的数字：

- 14 个同步、硬编码 Lua Host Tool；
- 2 类安装到 `VulcanPluginHostServices` 的外部宿主服务：Storage、UI Event Publisher；
- 18 个 Hook Point，且当前没有允许 Decision 的宿主点位；
- 3 个 UI Mount Point；
- UI SDK 提供 14 个宿主持有组件；
- Workspace 主视图仍是 6 个硬编码枚举值。

以下目标是基于当前模块边界给出的架构规模估算，不是源码现状：

| 成熟度 | 宿主服务族 | Lua 可调用操作总数 | Hook Point | UI 语义槽位 | 长期贡献注册表 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 当前 | 约 4 个硬编码域 | 14 | 18 | 3 | 现有分散清单贡献 |
| AI Workflow 插件可用 | 10–12 | 35–45 | 30–35 | 15–20 | 8–10 |
| DSH 可比基础 | 18–22 | 65–80 | 45–55 | 30–40 | 12–15 |
| 开放插件生态 | 数量可增长 | 不设固定上限 | 按声明扩展 | 子槽位可递归声明 | 插件可提供新 Service Definition |

所以以当前 14 个调用为基线，达到 DSH 可比基础预计还需新增约 **50–65 个稳定业务操作**。更重要的是增加约 **18–22 个服务族**以及允许插件定义 Provider/Consumer 的注册机制。

建议内核服务族至少包括：Runtime/Scope、Workspace、Project、Session、Conversation/Inbox、Prompt/Context、Agent、Subagent、LLM/Model、Provider Catalog、Tools、Approval、Filesystem、Subprocess/Sandbox、Terminal、Commands、Jobs、Settings、Credentials/Secrets、Storage、Notifications、Event/Audit。

VMM、Codebase Memory、Knowledge Base 不应继续增加到宿主固定函数表中；它们应通过上述注册机制提供自己的服务定义，例如 `memory.context`、`codebase.search/index`、`knowledge.library/search`。

## 十二、WebUI 是否需要为了插件扩展而重构

### 12.1 结论：需要重构组合架构，不需要更换 React

React 不是限制来源。当前动态插件已经能加载经过认证的 ESM，并由 Factory 返回任意 React Component。限制来自宿主布局与 Plugin UI ABI：

- Mount Point 只有 `skills.plugin.detail`、`workspace.main`、`conversation.right_panel`；
- `workspace.main` 统一追加在宿主主内容之后；
- `conversation.right_panel` 统一追加在内置右栏内容底部；
- 插件组件没有槽位 Props，只能从 SDK 使用 Action、资产、语言、14 个组件、失效修订和 Toast；
- `invokePluginAction` 只额外绑定 Workspace Path，没有向组件交付 Session/Agent/Turn 等上下文；
- Mount Manifest 没有槽位种类、优先级、条件、作用域、替换策略、子槽位或样式契约；
- `WorkbenchLayout` 和 `WorkspaceShell` 仍是大型固定组合组件，新页面和新入口需要宿主源码接线。

仓库内部已经有 `WorkspacePageDescriptor`，能描述标题、Header Actions、右侧 Actions、Tool Panels 和主内容。这证明当前 React 架构本身可以承载注册式组合，应把这些内部描述符提升为 Extension Slot API，而不是替换前端框架。

### 12.2 不允许任意 DOM 挂载，应提供语义槽位树

“几乎任何位置可挂载”应通过大量稳定、类型化的语义槽位实现，不能允许插件按 CSS Selector 查找 DOM 后插入节点。建议支持与 DSH 相同的四类组合语义：

- `single`：单一页面或唯一 Provider；
- `list`：Toolbar、Menu、Status Items 等有序列表；
- `keyed`：按 Tool ID、Message Kind、File Kind 选择 Renderer；
- `chain`：Message Decorator、Context Wrapper 等可组合链。

第一批建议定义 36 个语义槽位：

| 界面域 | 数量 | 代表槽位 |
| --- | ---: | --- |
| App Shell | 5 | Titlebar Left/Right、Activity Bar、Status Bar、Global Overlay |
| Workspace/Navigation | 7 | Navigation Items、Workspace Pages、Header Actions、Main Before/After、Sidebar Tabs |
| Conversation | 10 | Header、Empty State、Message Before/After、Message Decorator、Tool Renderer、Assistant Renderer、Run Status、Right Tabs |
| Composer | 6 | Before/After、Primary/Secondary Toolbar、Attachment Provider、Submit Decorator |
| Files/Git/Terminal/Panel | 5 | Context Tabs、Bottom Panel、Terminal Toolbar、File Renderer、Git Renderer |
| Settings/Management | 3 | Settings Navigation、Settings Pages、Plugin Detail Tabs |

槽位调用上下文应由宿主提供稳定的 Workspace/Project/Session/Agent/Turn 身份和受控 Action Proxy，不能把完整 Redux/React State 或内部 Store 对象交给插件。

### 12.3 组件扩展需要两类注册表

仅有“挂一个 React Node”仍不足以支持深度集成，UI Extension Kernel 应同时提供：

1. **Placement Slot Registry**：页面、面板、Toolbar、菜单、状态栏和 Overlay；
2. **Renderer/Behavior Registry**：Tool Result Renderer、Message Renderer、File Preview Provider、Editor Provider、Composer Attachment Provider、Context Tab Provider。

宿主组件 SDK 也应版本化扩展，至少覆盖 Layout、Tabs、Menu、Form、Dialog、Table、Tree、Virtual List、Markdown、Code Editor、Chart、Error Boundary 和 Design Tokens。插件可组合这些稳定组件，也可以在自己的槽位内实现组件树。

### 12.4 必须引入 UI 信任级别

当前插件 ESM 在主 Renderer 内执行。若允许插件进入标题栏、会话消息、Composer 和全局 Overlay，其风险远大于只显示一个详情页。建议分层：

- Built-in/First-party Signed：允许主 Renderer React ESM、Keyed Renderer 和 Chain；
- Approved Local/Trusted：用户明确批准后开放受限高级槽位；
- Community/Untrusted：使用 Sandbox Iframe/WebView 或声明式 UI，只开放加法型槽位。

无论信任级别，都需要插件代际绑定、递归 Disposer、Error Boundary、CSS/Design Token 隔离、作用域过滤和性能熔断。

## 十三、VMM、Codebase、知识库和 AI 能力是否应插件化

结论是可以，而且从当前文件边界看，这些能力已经具有较强的独立模块形态；但应迁移成**不同信任等级的第一方插件**，不是全部作为普通 Lua 插件直接搬迁。

| 能力 | 适合迁入插件的部分 | 必须继续由微内核持有的部分 | 建议顺序 |
| --- | --- | --- | --- |
| 普通工具 | Tool Definition、Executor、Prompt、Renderer | 权限、审批、调度、结果持久化 | 第一批 |
| AI Workflow | Prompt、模型调用编排、命令、会话 UI、结果处理 | Provider Transport、凭据、Session Log、预算与审批 | 第一批验证样板 |
| Knowledge Base | Library 管理、检索 Provider、索引 Job、管理 UI | 通用 Storage、Job Runtime、Workspace 身份与路径安全 | 第二批 |
| Codebase Memory | Managed MCP Provider、索引策略、项目触发器、状态 UI | MCP/Process Supervisor、Sandbox、项目身份、分发验签 | 第二批 |
| Git/Terminal 扩展 | 工具、命令、面板、Renderer | 文件路径安全、PTY/进程所有权、用户审批 | 第二批 |
| VMM | Context Provider、记忆策略、Hook、管理 UI、配置 | 会话事实、Managed Process Supervisor、版本契约、凭据和故障隔离 | 最后一批 |
| Provider Adapter | Manifest、模型目录、协议适配贡献 | 精确协议绑定、凭据、物理传输、安全日志 | 可信插件阶段 |

VMM 当前同时接入启动协调、Managed Runtime、Supervisor、Session Controller、Conversation Hook、Context Compaction、Execution Report 和 Workbench Projection，是这些候选中耦合最深的。它适合作为最终验证 Extension Kernel 完整性的第一方特权插件，不适合作为第一个迁移目标。

Codebase Memory 已经通过独立 Controller、Managed MCP 身份、项目路由和恢复监控形成较清晰的边界；Knowledge Base 已经有独立 Runtime、Job Recovery、Service API 和 UI Area。它们比 VMM 更适合作为早期迁移样板。

## 十四、推荐的总体架构

```text
Vulcan Microkernel
  ├─ Session/Event Store
  ├─ Provider Transport/Credentials/Protocol Binding
  ├─ Permission/Approval/Sandbox/Process Ownership
  ├─ Plugin Runtime + Capability/Service Registry
  ├─ Scope/Lease/Task/Event/Audit
  └─ React Shell + Typed Slot Renderer

First-party Trusted Plugins
  ├─ AI Workflows
  ├─ Knowledge Base
  ├─ Codebase Memory
  ├─ VMM
  ├─ Git/Terminal Experiences
  └─ Built-in Tools and Renderers

Third-party Plugins
  ├─ Lua Logic + Host Capability Calls
  ├─ Tools/Commands/Jobs/Context Providers
  └─ Approved or Sandboxed UI Contributions
```

推荐顺序：

1. 先完成 Host Capability Service、插件可定义的 Service Provider/Consumer 与异步 Task ABI；
2. 再完成真实 Scope、Lease、Hook Kernel 和持久审计；
3. 将 WebUI 固定布局拆成 Typed Slot Tree，并用内部页面描述符做第一个兼容层；
4. 选择一个“AI API + Model Invoke + Session UI”的内部功能作为端到端第一方插件样板；
5. 迁移普通工具、Knowledge Base、Codebase Memory；
6. 最后迁移 VMM，并以其完整生命周期作为架构验收。

## 十五、源码证据索引

### 15.1 DeepSeek Harness

- `D:\dsh\deepseek-harness\AGENTS.md`
- `D:\dsh\deepseek-harness\docs\architecture.md:9`
- `D:\dsh\deepseek-harness\docs\architecture.md:15`
- `D:\dsh\deepseek-harness\docs\architecture.md:53`
- `D:\dsh\deepseek-harness\docs\architecture.md:63`
- `D:\dsh\deepseek-harness\docs\architecture.md:92`
- `D:\dsh\deepseek-harness\docs\architecture.md:104`
- `D:\dsh\deepseek-harness\docs\event-producer-consumer.md:10`
- `D:\dsh\deepseek-harness\vendor\cordis\src\context.ts`
- `D:\dsh\deepseek-harness\vendor\cordis\src\events.ts`
- `D:\dsh\deepseek-harness\vendor\cordis\src\fiber.ts`
- `D:\dsh\deepseek-harness\vendor\cordis\src\registry.ts`
- `D:\dsh\deepseek-harness\vendor\loader\src\entry.ts`
- `D:\dsh\deepseek-harness\packages\core\agent\src\runtime-types.ts`
- `D:\dsh\deepseek-harness\packages\core\tools\src\index.ts`
- `D:\dsh\deepseek-harness\packages\core\system-prompt\src\index.ts`
- `D:\dsh\deepseek-harness\packages\core\session\src\index.ts`
- `D:\dsh\deepseek-harness\packages\client\ui-slots\src\index.ts`
- `D:\dsh\deepseek-harness\packages\client\ui-slots\src\renderer.ts`
- `D:\dsh\deepseek-harness\packages\extensions\hooks-codex\src\index.ts`
- `D:\dsh\deepseek-harness\packages\extensions\hooks-claude-code\src\index.ts`
- `D:\dsh\deepseek-harness\packages\extensions\tool-cordis\src\index.ts`
- `D:\dsh\deepseek-harness\packages\extensions\cordis-host-runner\src\index.ts`

### 15.2 Vulcan Code

- `D:\openvulcan\vulcan-code\crates\vulcan-plugin-runtime\src\manifest.rs:195`
- `D:\openvulcan\vulcan-code\crates\vulcan-plugin-runtime\src\catalog.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-plugin-runtime\src\runtime.rs:3482`
- `D:\openvulcan\vulcan-code\crates\vulcan-plugin-runtime\src\runtime.rs:4798`
- `D:\openvulcan\vulcan-code\crates\vulcan-plugin-runtime\src\runtime.rs:7281`
- `D:\openvulcan\vulcan-code\crates\vulcan-plugin-runtime\src\host_services.rs:133`
- `D:\openvulcan\vulcan-code\crates\vulcan-hooks\src\types.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-hooks\src\registry.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-hooks\src\dispatch.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-agent-runtime\src\vulcan_plugin_hooks.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-agent-runtime\src\agent_runtime\provider_transport_request.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-agent-runtime\src\agent_runtime\provider_transport_stream_executor.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-agent-runtime\src\agent_runtime\provider_transport_stream_runtime.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-agent-runtime\src\agent_runtime\provider_transport_retry.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-agent-runtime\src\agent_runtime\tool_runtime_bridge.rs`
- `D:\openvulcan\vulcan-code\plugin-bridge\vulcan-plugin-bridge\runtime\bridge.lua:111`
- `D:\openvulcan\vulcan-code\crates\vulcan-session-core\src\runtime_kernel.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-session-core\src\subagent_registry.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-service-kernel\src\service_bootstrap.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-service-kernel\src\session_runtime_kernel.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-service-kernel\src\managed_memory_mesh_runtime.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-service-kernel\src\vmm_hooks.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-service-kernel\src\vmm_session_controller.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-service-kernel\src\codebase_memory_controller.rs`
- `D:\openvulcan\vulcan-code\crates\vulcan-service-kernel\src\knowledge_base_services.rs`
- `D:\openvulcan\vulcan-code\apps\vulcan-code-web\src\components\plugins\dynamic-plugin-mount-panel.tsx`
- `D:\openvulcan\vulcan-code\apps\vulcan-code-web\src\service\plugin-ui-module-runtime.ts`
- `D:\openvulcan\vulcan-code\apps\vulcan-code-web\src\components\workspace\workspace-resizable-body.tsx`
- `D:\openvulcan\vulcan-code\apps\vulcan-code-web\src\components\workspace\workspace-page-descriptor.ts`
- `D:\openvulcan\vulcan-code\apps\vulcan-code-web\src\components\workspace\workspace-shell\types.ts`
