# 会话持久化 V1 剩余实现、完整切换与旧链路清除执行方案

## 0. 方案信息

- 状态：执行中；阶段 0-3 已完成，阶段 4/6 的 Core、Provider、输入准入与生命周期由主控单独收口，其余阶段未满足退出门。
- 日期：2026-08-27。
- 当前分支：`main`。
- 当前基线提交：`5c3c0a072ab2de8ca577376bbb91e61ec423942c`。
- 工具卡备份分支：`codex/persistence-card-wip-backup-20260826`。
- 工具卡备份提交：`a5d94fafcc399267491e65c6c0881a90615e60c6`。
- 本方案是后续实现、审核和验收的唯一主控方案；旧方案继续提供需求与设计证据，但不得再单独驱动实现。

### 0.1 关联方案与事实依据

1. [会话持久化架构审计与冻结需求](./20260826-01-SESSION_PERSISTENCE_ARCHITECTURE_AUDIT.md)
2. [会话持久化 V1 重构原方案](./20260826-02-SESSION_PERSISTENCE_V1_REARCHITECTURE.md)
3. [当前执行状态与历史验证记录](./20260826-02-SESSION_PERSISTENCE_V1_EXECUTION_STATUS.md)
4. [工具卡恢复台账](./20260826-03-TOOL_CARD_RECOVERY_LEDGER.md)
5. [V1 垂直切换与旧链路删除清单](./20260826-04-SWITCH_MANIFEST_V1.md)
6. 当前总执行方案：[本文](./20260827-01-SESSION_PERSISTENCE_V1_FINAL_CUTOVER.md)

### 0.2 本方案取代的错误执行方式

1. 不再围绕单条消息、单个工具状态或单个失败提示继续给旧母链路增加补丁。
2. 不再用更多拒绝写入、截断、容错解析或自动修复来掩盖写入模型不稳定。
3. 不再维护内存消息、运行时日志、展示记录、工具详情、状态快照等多套可互相覆盖的事实源。
4. 不再把“新模块单测通过”描述为“生产链路完成”。
5. 不再为了保留既有枚举数量、旧字段或旧测试而牺牲真实语义。
6. 不保留 V1 与旧链路的双写、双读、回退或兼容层。

## 1. 总体目标

本任务一次性完成以下结果：

1. 修复当前未提交 V1 代码中已经确认的结构性缺陷。
2. 补齐当前只存在于测试构件中的 V1 能力，使其具备完整生产语义。
3. 建立每个活动会话唯一的 `SessionDocument` 内存所有者和唯一 flush 协调器。
4. 让主事件流成为 Provider 请求、WebUI 顺序、工具卡、继续、重试、目标模式、统计和重启恢复的唯一事实源。
5. 完成 Agent 流、Provider 投影、服务 API、WebUI、Goal、Ask、上下文压缩、资源、分叉、编辑、索引和关闭流程的垂直接线。
6. 从备份分支按功能片段迁移工具卡实现，并改造成只消费 V1 事件，不恢复旧详情链路。
7. 在正式切换后物理删除旧持久化链路、旧数据库副本、旧详情接口、旧解析猜测和旧自动修复路径。
8. 完成真实长会话、故障注入、性能、SQLite、WebUI 和 Release 桌面验收。
9. 完成连续五轮全范围零问题审核；任一轮发现问题后修复并把连续计数归零。
10. 最终形成一个只有新会话格式、单一所有者、单一写入器、单一加载器、单一投影顺序和单一前端 reducer 的产品。

## 2. 最终验收定义

只有同时满足以下条件才允许宣告完成：

1. 新会话从创建、执行、工具调用、终止、继续、重试、目标自动继续、压缩、编辑、分叉、关闭到重启均只经过 V1。
2. 每个会话的持久化 `seq` 从 1 开始严格连续；加载、Provider 构造和前端显示均只按 `seq` 升序。
3. 同一个工具调用只占一个主事件条目，输入、输出、状态和 ChangeSet 在同一条目原位更新，不新增第二事实。
4. 每一次实际 Provider API 请求都有持久化开始事实和终态统计；崩溃在零输出阶段也能重建“请求已开始但未结束”。
5. 主事件文件中保存思考、assistant 正文、工具输入输出、ChangeSet、错误、中断、继续标记和显示层记录。
6. 运行中和重启后的工具卡状态、组合关系、展开内容、增删行数、编码和换行信息一致。
7. 上下文压缩阈值切换在下一次安全请求边界生效，绝不改变物理事件分片阈值。
8. Goal 在一次 assistant API 结束但目标未完成时自动调度后续执行，且 exactly-once。
9. 网络断开、余额不足、重试耗尽、用户中止后点击继续，继承未结束 Turn，不伪造用户消息，不丢失已产生内容和统计来源。
10. 关闭程序前所有必要 flush 完成；失败时应用保持存活并展示准确错误，不以丢弃数据换取退出。
11. 仓库搜索证明旧生产持久化入口、旧数据库事实副本、旧详情 RPC 和旧 WebUI 双模型全部消失。
12. Release 模式下 `shell_command` 不弹出窗口，服务断链恢复后事件顺序不发生跳动或重排。
13. 不读取、不迁移、不兼容旧持久化会话；损坏的新格式只报告准确损坏位置，不自动修复。
14. 所有自动化门禁通过，真实 Release 桌面矩阵通过，连续五轮全范围审核无新发现。

## 3. 当前代码的真实基线

### 3.1 已存在且可继续使用的构件

1. `crates/vulcan-session-core/src/v1/` 已具备事件、ID、状态机、统计、压缩节点、Provider 投影和初步服务 DTO。
2. `crates/vulcan-workspace-store/src/workspace_store/session_v1/` 已具备原子单文件写、分片序列化、加载、generation、故障注入和测试构件。
3. `crates/vulcan-change-set` 已具备持久化 ChangeSet 类型的初步实现。
4. `session_index_v1.rs`、`resource_catalog.rs`、`provider_api_trace.rs` 已有独立构件。
5. `conversation-event-window-store.ts` 已有以 `seq` 为键的窗口算法及单测。
6. 备份提交包含可复用的工具卡布局、文件差异、Markdown/native 结果展示、测试和样式。

### 3.2 方案创建时的局部基线（不得作为当前门禁）

1. `vulcan-session-core` 局部测试 136 项通过。
2. `vulcan-change-set` 局部测试 8 项通过。
3. `vulcan-workspace-store` 局部测试 348 项通过、1 项忽略。
4. WebUI 当前测试 98 个文件、462 项通过。
5. `cargo fmt --check`、`cargo check --workspace` 和 `git diff --check` 在最近一次核验中通过。

这些结果仅记录方案创建时的构件基线，不证明当前共享工作树仍通过，也不证明生产接线、旧链路删除或 Release 验收完成。当前门禁只接受本轮事实台账与各阶段退出前由主控重新执行的精确命令结果。

### 3.3 当前阻止切换的已确认缺陷

1. **尾段加载分配器回退**：`SessionDocument::from_parts` 只从驻留尾部事件恢复 `seq` 和本地 ID；滚段后空尾段会回退到 1，导致重复 ID 或追加失败。
2. **上下文阈值领域错位**：`CompactionWindowConfig { max_lines, max_bytes }` 被映射到 `FlushLimits`，把用户切换的上下文压缩节点误当成物理分片大小。
3. **编辑 generation 提交顺序错误**：当前 `publish_edit_generation` 先切换 `metadata.json`，后写 `compactions.json` 与 `runtime.json`，提交点后失败会暴露混合状态。
4. **API 开始事实未落盘**：`start_api()` 仅改变内存；请求在首个输出前崩溃时，重启后无法证明该物理 API 曾开始。
5. **工具载荷验证可绕过**：`ToolPayload` 的透明反序列化绕过 `try_new`；另有 panic 构造与错误码复用不当。
6. **服务 DTO 不完整**：当前 `ConversationEventSummary` 只有 envelope 和 `entity_id`，没有事件正文，不能驱动消息、工具卡、错误卡和统计显示。
7. **驻留窗口仍是构件**：分片目录和 loader 存在，但活动会话的精确驻留、懒加载、释放和 Provider 窗口尚未完整接线。
8. **生产调用图仍是旧链路**：Agent runtime、Provider bridge、hydration、工具详情和 WebUI 仍大量依赖 `ConversationSnapshot.messages/records/continuation`、`SessionRuntimeStateEntry` 及旧 segmented store。
9. **严格 Clippy 未通过**：最近一次三目标 crate 的 `-D warnings` 核验有 33 个错误和 1 个警告；执行状态文档中的“新模块零警告”不成立。
10. **长会话证据失真**：现有标为 100k 的测试只执行约 60 Turn，不是 100,000 条真实事件持久化、重启与分页。
11. **WebUI 新窗口未接线**：`conversation-event-window-store.ts` 只是独立文件和测试，hydration、实时 delta 和重连仍未统一进入它。
12. **工具卡台账未完成**：当前只有文件级差异，缺少 hunk 级取舍和 V1 DTO 映射。
13. **状态文档存在过度完成声明**：需要在执行前按本节事实修正。
14. **工作树存在临时脚本**：`.tmp_update_row2.py` 不属于产品或方案，必须在实现前移除。

## 4. 不可变设计原则

### 4.1 单一事实源

1. `SessionDocument` 是活动会话唯一内存权威。
2. 主事件流是会话语义唯一持久化权威。
3. `metadata` 只保存索引、配置、输入区、目录映射、当前 generation/context 指针和分配器前沿，不复制消息正文。
4. `runtime` 只保存当前被锁定的动态系统上下文来源快照和当前压缩节点选择。
5. `compactions` 只保存上下文压缩节点、实际压缩发生位置、保留起点和总结。
6. SQLite 只保存能精确拼接会话目录的必要索引、标题、项目、时间和归档状态，不保存事件或工具状态。
7. Provider 调试请求日志是独立 `temp` 支线，不参与会话重建、统计、继续或 Provider continuation。

### 4.2 顺序与标识

1. `seq` 是唯一显示顺序、持久化顺序和 Provider 语义顺序。
2. 会话内部引用使用 JavaScript 安全范围内的 `u64/int64` 本地 ID；跨系统 session/project/resource 标识保留既有稳定 ID。
3. 同一事件原位更新不得改变 `seq`、`created_at_ms`、事件 kind 或实体 ID，只允许更新载荷、状态和 `updated_at_ms`。
4. 不使用时间、状态、消息类型或工具成功失败进行二次排序。
5. 工具成功与失败不影响其所属 API 组和前端组合关系。

### 4.3 简单全量保存

1. 语义边界到达时，对活动尾段或对应小 JSON 文档执行完整原子替换。
2. 不逐事件建立复杂确认链，不为一条信息设计多层锁和补偿账本。
3. 流式 token 增量只更新内存和实时 UI；到工具边界、API 终态、中断、Turn 关闭、压缩提交或关闭屏障时统一 flush。
4. flush 失败保留内存最新状态和 dirty revision；下一次重试写最新完整候选，不丢弃、不回退、不截断。

### 4.4 不兼容、不修复、不回退

1. `schema_version` 固定为 1；当前未正式发布，不创建 V2。
2. 不兼容旧持久化会话，不提供迁移器。
3. 不扫描目录寻找丢失索引，不容忍缺失字段，不猜测旧格式。
4. 损坏时返回结构化错误，包含 session、文件、行号、字节偏移、期望和实际值。
5. 不自动删除或修复损坏数据；只允许删除明确的未发布候选目录或成功提交后已失去权威的旧 generation/context。
6. 不设置新旧链路 feature gate，不双写，不静默 fallback。

### 4.5 范围排除

1. 不讨论 Debug 与 Release 同时写同一存储根；正式产品验收只针对 Release。
2. 不清理项目级资源；资源默认长期保留，缺失时显示替代符。
3. 不把子代理目录、持久化终端、Lua session 和完整 API trace 分叉到新会话。
4. 不对会话正文中的敏感信息做额外脱敏；只对可选 Provider API trace 中的 token/密钥字段脱敏。

## 5. 最终物理与内存拓扑

### 5.1 会话目录

最终新会话目录仅允许包含以下权威内容：

```text
metadata.json
segments/
  g0000001/
    s0000001.jsonl
    s0000002.jsonl
context/
  c0000001/
    runtime.json
    compactions.json
vmm.json
terminals/
TASK/
temp/
  provider-api-trace/
```

说明：

1. `segments/gXXXXXXX` 是事件 generation；正常追加写当前 generation，编辑创建下一 generation。
2. `context/cXXXXXXX` 是一次不可变上下文提交单元；`runtime.json` 与 `compactions.json` 先在候选目录完整写入，最后由 `metadata.active_context_generation` 一次性发布。
3. context generation 不是旧 runtime 备份链，不用于保存完整事件；它只解决两个小 JSON 无法跨文件原子替换的问题。
4. 新 context 发布成功后，旧 context 可在非权威清理步骤删除；崩溃时 loader 只读取 metadata 指向的 context。
5. generation 编辑也遵循“候选 segments + 候选 context 全部完成，metadata 最后切换”的单提交点。
6. 普通活动尾段 flush 仍只完整替换当前尾段并更新 metadata；不会复制全部历史。

### 5.2 metadata 必须包含

1. 固定契约和 `schema_version = 1`。
2. `session_id`、项目精确寻址字段、标题、创建/更新时间、归档状态。
3. 当前事件 generation 和当前 context generation。
4. `split_list`：每段文件名、起止 seq、事件数、字节数、该段末端本地 ID 前沿。
5. 全局 `id_frontier`：下一个 seq 和下一个本地实体 ID。
6. 运行模式：Goal、Plan、授权范围及后续模式配置。
7. 会话配置：输出风格、知识库、当前模型与思考等级、下一请求模型与思考等级、上下文压缩节点 ID。
8. 输入区：草稿、按顺序 ResourceId、过程内待注入输入、未来 Turn 队列。
9. 不包含消息列表、工具详情、Provider 历史、终端内容或子代理内容副本。

### 5.3 runtime 必须锁定的来源

1. `runtime.json` 只锁定下列八种强类型来源在真实请求边界实际使用的精确正文与来源身份，不保存不可解释的完整最终系统提示词副本。
2. `MemoryProfile`：当前记忆画像基线；单 Turn VMM 召回、临时画像增量与短期注入不属于该来源。
3. `AgentsMd`：所有实际参与本次组合的层级 `AGENTS.md`，每层独立保留来源路径与正文。
4. `VulcanMd`：所有实际参与本次组合的项目/目录级 `Vulcan.md`，每层独立保留来源路径与正文。
5. `AgentPersona`：当前人格提示词的精确生效正文。
6. `OutputStyle`：当前输出风格的精确生效正文。
7. `PromptSkillInjection`：由提示词技能启用/关闭状态生成的固定系统注入正文；技能的单次调用内容不锁定。
8. `PluginInstructions`：明确声明为长生命周期的插件指令正文。
9. `BaselineHooks`：明确声明为上下文代次基线的 Hook 固定正文；请求级临时 Hook 不锁定。
10. Provider/Agent 基础提示词、Provider/模型/工具协议、工具 Schema/权限/本次可见集合、环境上下文、完整最终系统提示词、Goal/Plan、单 Turn VMM 召回与临时 Hook 均不得进入 runtime lock；它们继续按当前请求或主事件动态组合。
11. 上述八种来源只在首次物理 Provider 请求的可靠准备事务内初始化，并在上下文压缩发布新节点的同一母事务内一次性刷新；普通请求不得重新读取变化中的来源。
12. Goal/Plan 的过程语义以有序 `system_context` 事件进入主流，并在压缩总结中追补当前状态，不复制进 runtime lock。

### 5.4 活动会话内存所有权

统一建立：

```text
SessionRuntimeRegistry
  -> SessionHandle
     -> SessionDocument 写锁
     -> flush 串行锁
     -> 生命周期状态
     -> 已加载分片目录与驻留窗口
```

规则：

1. 一个 session 在一个正式进程内只能有一个 `SessionHandle`。
2. 所有 Agent、工具、Goal、Ask、服务控制和关闭逻辑都通过该 handle 修改文档。
3. flush 顺序固定为：取得 session flush 锁、在文档锁内生成最新候选、释放文档锁、写磁盘、重新取得文档锁确认 durable revision、释放 flush 锁。
4. 写磁盘期间后续内存修改可以继续增长 revision；确认旧候选后仍保留更新后的 dirty 状态。
5. 同 session flush 串行，不同 session 可并行。

### 5.5 分片与驻留

1. 物理滚段阈值固定为 3000 条事件或 8 MiB，不受上下文压缩窗口设置影响。
2. 活动尾段常驻内存；已封口分片按 Provider 上下文窗口或 WebUI 分页精确加载。
3. `split_list` 是唯一定位表；禁止遍历整个会话目录寻找分片。
4. 加载器根据分片目录中的末端本地 ID 前沿与 metadata 全局前沿交叉验证，空活动尾段也能恢复正确分配器。
5. Provider 请求结束后释放仅为该请求加载的封口分片；WebUI 已显式加载的历史窗口由前端保留，不因 Provider 释放而消失。

## 6. 主事件契约修订

### 6.1 新增 `api_started`

1. 不再维持当前 12 个 event kind 的人为数量限制，增加第 13 类 `api_started`。
2. 每次真实 Provider 请求在网络派发前先追加并持久化 `api_started`。
3. 该事件记录 `api_call_id`、所属 Turn、用途、Provider/连接/协议、实际模型、思考等级、请求配置快照和开始时间。
4. 该事件为 `host_only`，不回放给 Provider，但用于恢复零输出请求、分组、调试和统计。
5. 请求结束追加唯一 `api_stats`；二者共同界定一个物理 API。
6. 重启时发现 `api_started` 没有对应 `api_stats`，投影为持久化中断显示记录和未结束 API 状态，不伪造 Provider 输出。

### 6.2 Turn 规则

1. 一个 Turn 从根用户输入开始，到真正最终 assistant 完成并关闭 Turn 为止。
2. 同 Turn 内允许多个物理 API、工具批次、插队补充、中止、失败和继续。
3. 用户主动中止后直接点击继续，仍是原 Turn。
4. 用户主动中止后输入补充再发送，补充作为同 Turn 过程内 `user_input`，仍是原 Turn。
5. 网络、余额、重试耗尽后点击继续，不新增假的用户输入。
6. 只有已明确排队为未来任务的输入才在当前 Turn 真正闭合后提升为新 Turn。
7. `turn_stats` 只在真正闭合时产生一次；暂停期间统计由已持久化 API 事件动态计算。
8. 错误、中断、重试、继续和模型切换以永久 `display_note`/`execution_control` 事件显示，但不投影到 Provider。

### 6.3 工具事件

1. 工具请求参数确定后创建一条 `tool_call` 并在执行前 flush，使“正在调用”有真实持久化来源。
2. 工具流式进度可实时更新内存和 UI，同一 seq 原位更新；达到检查点或终态后 flush。
3. 成功、失败、停止均更新同一条事件；失败不是脱离工具组的新消息。
4. 常规工具在一条事件内保存 `input`、`output`、`status`、结构化错误和 `changeset`。
5. 持久化终端、子代理和 Lua session 只额外保存导航 ID；导航对象不存在时卡片仍正常显示历史输入输出。

### 6.4 Provider continuation

1. 每个实际 API 在对应终态记录中保存强类型 continuation/replay 状态及其来源路由。
2. 该状态只用于下次兼容的同 Provider、同协议、同模型路由继续；不从 temp trace 恢复。
3. Provider、协议、模型或路由不匹配时显式执行完整主事件投影，不探测备用 continuation。
4. 每次请求必须记录实际使用的模型和思考等级；下一请求切换时追加可见模型切换标记。

## 7. 执行依赖图

```text
阶段 0 事实纠偏
  -> 阶段 1 领域阻塞修复
  -> 阶段 2 存储原子性与驻留模型
  -> 阶段 3 唯一会话所有者与 flush
  -> 阶段 4 Provider/API 主链
  -> 阶段 5 runtime/压缩/Ask/动态配置
  -> 阶段 6 Turn/继续/Goal/队列/子代理
  -> 阶段 7 资源/编辑/分叉/索引/trace/关闭
  -> 阶段 8 服务 DTO/WebUI 事件窗口
  -> 阶段 9 工具卡选择性迁移
  -> 阶段 10 切换前全链路测试构件门
  -> 阶段 11 生产垂直接管
  -> 阶段 12 旧链路物理删除
  -> 阶段 13 全量自动化与 Release 验收
  -> 阶段 14 连续五轮审核
  -> 阶段 15 文档、提交与完成归档
```

阶段 11 与阶段 12 必须连续执行；二者之间不得生成 Release、不得用真实会话试运行、不得留下可交付的半切换状态。

## 8. 分阶段执行流程

### 阶段 0：冻结事实与清理执行环境

#### 实现内容

1. 保存当前 `git status --short`、当前 HEAD、备份分支提交和差异统计。
2. 删除非产品临时脚本 `.tmp_update_row2.py`。
3. 修正执行状态文档中的虚假或过度完成声明：Clippy、100k、生产接线、工具卡、真 SQLite、Release 验收均标为未完成。
4. 把本方案链接写入执行状态文档和切换清单。
5. 为当前未提交文件建立归属表：领域层、存储层、索引/资源/trace、WebUI 窗口、临时文件。
6. 确认不覆盖工作树中无法归属于本任务的并行修改；发现重叠时先记录并停止该文件修改。

#### 验证

1. `rtk git status --short` 无未知临时文件。
2. 状态文档中的完成数量与实际测试证据一致。
3. 本方案、审计台账、工具卡台账和切换清单相互有链接。

#### 退出门

只有事实基线无冲突、文件归属明确后进入阶段 1。

### 阶段 1：修复 V1 领域阻塞项

#### 1.1 分配器与分片前沿

1. 给 `SegmentInfo` 增加该段结束时的本地实体 ID 前沿，避免为恢复分配器加载全部历史。
2. `from_parts` 接收已验证的 catalog 前沿和活动尾段事件前沿。
3. 恢复规则使用 catalog、活动尾段和 metadata 三方一致性校验，而不是简单取最大值掩盖损坏。
4. 空活动尾段、单个封口段、多段、编辑 generation 和分叉均覆盖。

#### 1.2 API 开始事实

1. 增加 `ApiStartedEvent`、事件 kind、投影和序列化 golden。
2. `start_api` 改为创建事件并占用 seq，而不是仅更新内存字段。
3. 加入网络派发前的持久化门；持久化失败时不发请求。
4. 更新所有事件数量断言，不为保留旧数量拒绝真实语义。

#### 1.3 ToolPayload

1. 实现自定义 `Deserialize`，复用唯一对象校验入口。
2. 非对象输入返回专用 `InvalidToolPayload`，不得复用 `InvalidId`。
3. 移除生产 panic 构造；测试辅助构造也必须显式验证。
4. 对 input、output、null、数组、字符串、数字和未知字段建立反序列化测试。

#### 1.4 上下文压缩节点类型

1. 把 `ContextCompactionWindowNodeId` 移入无依赖环的低层权威位置，优先放入 `vulcan-session-core` 并让 `vulcan-context` 使用或临时重导出。
2. 在修改前用 Cargo 依赖图确认不会形成 `session-core -> context -> core -> session-core` 环。
3. metadata 只保存节点 ID，不保存 `max_lines/max_bytes`。
4. 每次请求由 `vulcan-context` 和当前模型配置解析真实 token 窗口。
5. `FlushLimits` 保持私有物理常量或测试注入，删除它与上下文配置的 `From` 映射。

#### 1.5 服务 DTO

1. 把当前只有 envelope 的 `ConversationEventSummary` 改为强类型可区分联合 DTO。
2. 每个事件正文一一映射，工具事件必须包含卡片所需完整持久化字段。
3. Rust 与 TypeScript 都不得在生产 DTO 中使用 `payload: unknown` 或再次按工具名猜结构。
4. hydration、分页和实时 delta 共用同一个 DTO 和同一个投影函数。

#### 1.6 代码质量

1. 修复本任务新增/修改模块的全部 `clippy -D warnings`。
2. 检查所有新增类、函数、接口和关键变量的中英文双语注释；修复乱码或错误编码。
3. 禁止用 `allow` 批量压制问题，除非逐项给出语言层必要性证据。

#### 定向验证

```text
rtk cargo test -p vulcan-session-core
rtk cargo clippy -p vulcan-session-core --lib -- -D warnings
rtk cargo fmt --all -- --check
```

#### 退出门

全部五个领域阻塞项有失败先行测试、实现和重启/反序列化验证；无领域层已知缺陷后进入阶段 2。

### 阶段 2：完成存储原子性、分片驻留与长会话模型

#### 2.1 统一提交点

1. 增加 `context/cXXXXXXX` 不可变候选目录与 metadata 当前 context 指针。
2. 新 context 写入顺序：创建候选目录、完整原子写 `compactions.json`、完整原子写 `runtime.json`、校验可加载、最后原子替换 metadata 指针。
3. 编辑写入顺序：创建候选事件 generation、构建候选 context、逐文件校验、最后一次 metadata 同时切换两者。
4. metadata 发布前失败时旧 generation/context 逐字节仍为权威；发布后失败不得再影响新权威。
5. 候选目录清理只允许在下一次明确发布同一候选编号前进行；loader 不自动修复或搜索。
6. 成功切换后旧非权威 generation/context 的删除是清理，不参与提交正确性。

#### 2.2 活动尾段

1. 活动尾段 `end_seq = null`，文件实际内容是权威；metadata 的事件数和字节数允许短暂落后但不得反向截断文件。
2. flush 对活动尾段做完整序列化和原子替换。
3. 达到 3000 条或 8 MiB 后封口当前段，再创建空或首事件新尾段，并按明确发布顺序更新 metadata。
4. 对滚段两个文件发布之间的每个崩溃点建立故障测试。

#### 2.3 精确加载

1. 启动只加载 metadata、当前 context、活动尾段和必要的最近完整 Turn 分片。
2. Provider 依据 runtime 保留起点与当前请求范围加载精确分片。
3. WebUI 最近 30 Turn 加载不得截断开放 Turn、API 组或工具事件。
4. `before_seq` 为排他游标，返回前一组完整 Turn，并给出下一游标。
5. 禁止目录遍历和全历史 JSON 解析作为正常加载路径。

#### 2.4 真正长会话

1. 生成至少 100,000 条主事件，跨越多个 8 MiB/3000 条分片。
2. 完成持久化、关闭、重启、最近 30 Turn hydration、向前分页、Provider 投影和继续追加。
3. 用文件读取计数器证明未读取无关封口分片。
4. 记录常驻内存、加载字节、文件数和耗时，不用 60 Turn 冒烟替代。

#### 定向验证

```text
rtk cargo test -p vulcan-workspace-store session_v1
rtk cargo clippy -p vulcan-workspace-store --lib -- -D warnings
```

#### 退出门

所有跨文件提交只存在 metadata 一个发布点；真实 100k 测试通过；活动尾段和空尾段重启可继续分配与追加。

### 阶段 3：建立唯一 Session owner 与 flush 协调器

#### 实现内容

1. 在服务内核建立 `SessionRuntimeRegistry` 与 `SessionHandle`。
2. 新会话创建顺序固定为：完整发布目录与 metadata，然后插入 SQLite 索引，最后注册内存 handle。
3. 已有活动会话只能从 registry 获取，禁止组件各自读取磁盘构建私有快照。
4. 所有修改方法返回 revision 和所需 flush 原因。
5. flush 只接受当前 document 产生的完整候选，不接受调用者拼装消息尾或状态片段。
6. 关闭屏障进入 `shutting_down`，拒绝新根输入，等待工具/API 安全收束和全部 dirty flush。
7. flush 失败时保持进程、会话和内存状态存活，向 UI 返回结构化错误与重试入口。
8. 崩溃恢复只依赖已发布文件，不依赖 Drop、后台队列 owner 或“最后再补一次”逻辑。

#### 语义 flush 矩阵

| 边界 | 是否必须持久化后再继续 |
| --- | --- |
| 根输入/过程内补充/队列提升 | 是 |
| `api_started` 后、Provider 派发前 | 是 |
| reasoning/text 的每个 token | 否，只更新内存与实时 UI |
| 工具开始、工具检查点、工具终态 | 是 |
| API 正常结束、错误、中断、重试耗尽 | 是 |
| 显示层永久错误/继续/模型切换标记 | 是 |
| Turn 真正关闭及唯一统计 | 是 |
| 上下文压缩提交 | 是，context 候选后 metadata 指针 |
| 输入草稿普通键入 | 防抖完整写 metadata，不逐键同步 |
| 正常关闭 | 是，必须等待 durable revision 追平 |

#### 验证

1. 并发工具完成顺序不改变 seq。
2. flush 写入期间追加新事件，旧候选确认后 dirty 仍存在并最终写入。
3. 连续两次 flush 失败后第三次成功，最终文件等于最新内存，不是第一次候选。
4. 关闭期间有活动 API/工具、失败 flush 和重复关闭请求均有确定结果。

#### 退出门

测试宿主中只有一个内存 owner 和一个 session writer；旧 writer 仍未接生产，但不得参与新测试构件。

### 阶段 4：接通 Provider、物理 API 与流式主事件

#### 实现内容

1. Agent stream 的 reasoning、assistant text、工具开始/更新/完成全部改为调用 `SessionHandle`。
2. Provider 派发必须在 `api_started` durable 后发生。
3. 每个 API 终态创建唯一 `api_stats`，包括实际模型、思考等级、token、缓存、耗时、状态、结束原因、错误分类和 continuation 状态。
4. Provider 请求只由 `project_provider_items` 从主事件 + runtime + compaction 选定窗口构建。
5. `visibility` 只过滤是否进入 Provider，不改变 seq 顺序。
6. reasoning、assistant、工具调用和工具结果严格保持真实发生顺序。
7. 断链重连根据最后已确认 seq 请求缺失增量；重复 seq 只能做同身份原位更新。
8. `ConversationSnapshot` 若暂时作为 Provider 传输 DTO 保留，只能由 V1 投影即时生成，禁止成为可修改或持久化事实源；最终可重命名为明确的 Provider 请求输入类型。

#### 验证

1. 零输出崩溃、首 token 后崩溃、工具开始后崩溃、工具结果后 stats 前崩溃。
2. 同 API 多次 reasoning/text 更新不新增乱序事件。
3. continuation 同路由继续、模型切换完整 replay、无 continuation 完整 replay。
4. Provider golden 与主事件选定窗口逐项一致。

#### 退出门

测试宿主中 Provider 构造不再读取旧 messages/records/continuation 权威。

### 阶段 5：runtime、压缩、Ask 与动态配置

#### 实现内容

1. 接通当前/下一请求模型和思考等级，在安全 API 边界完成切换并追加显示标记。
2. 上下文压缩窗口节点修改只写 metadata 配置；正在执行的请求不变，下一次压缩判定读取新节点。
3. 压缩时先生成总结与保留起点，写新 context 中的 compactions，再写 runtime 锁定快照，最后 metadata 发布。
4. 压缩节点同时保存实际压缩发生 seq、Provider 保留起点 seq、总结和统计来源。
5. 上一次未发布候选在下一次写相同候选编号前清理；已发布节点只由 metadata/context 指针选择。
6. Ask 状态进入主事件/metadata 的明确语义：提问、等待、回答、取消、重启恢复，不依赖旧 runtime log。
7. Goal/Plan 动态系统语义按 seq 写 `system_context`，支持 developer/system/隐藏 user 套壳投影，但持久化类型始终是系统语义。
8. runtime 锁定来源在压缩后刷新最新 AGENTS/Vulcan、记忆、人格、风格、技能固定注入和工具协议组合。

#### 验证

1. 执行中点击不同压缩窗口节点，UI 有响应，当前请求不变，下一判定使用新节点。
2. 压缩前、compactions 后、runtime 后、metadata 前后逐点故障。
3. 调整压缩阈值不会改变事件分片数量。
4. Ask 等待时关闭重启、回答后继续、取消后终止均保持同 Turn 规则。

#### 退出门

动态配置、压缩与 Ask 不再读写 `SessionRuntimeStateEntry` 的旧持久化副本。

### 阶段 6：Turn、继续、重试、Goal、队列和子代理

#### 6.1 输入与继续

1. 根输入创建新 Turn；过程内插入绑定开放 Turn；未来队列只在 Turn 关闭后提升。
2. 余额不足、网络错误、重试耗尽和用户中止均留下显示事件，但保持 Turn 开放。
3. 点击继续调用 `continue_turn`，不追加假的用户事件；有过程内补充时按 seq 纳入下一 API。
4. 继续前动态统计从已持久化 API 事件计算；真正关闭时生成唯一 turn stats。

#### 6.2 Goal

1. Goal 配置和状态进入 metadata，移除 Goal SQL 事实副本。
2. 一次 assistant API 返回后若目标未完成、未阻塞、未暂停，调度器自动创建下一次 Goal continuation。
3. 调度凭据由主事件/metadata 状态确定，使用原子状态变更保证 exactly-once。
4. 程序关闭重启后扫描活动会话的明确 Goal 状态，不扫描目录、不依赖旧 receipt；恢复一次且只恢复一次。
5. token 预算和时间统计从 `api_stats`/`turn_stats` 统一聚合。

#### 6.3 Todolist 与子代理

1. todolist 状态必须定义为主事件投影或 metadata 模式状态，不再独立持久化事实。
2. 子代理回流记录正文、来源 child session ID、来源 seq 范围和包容统计。
3. 分叉不复制 child TASK 目录；历史子代理工具卡仍显示调用参数和返回，导航失败不影响会话加载。

#### 验证

1. Goal 自动继续无需用户点击；关闭重启后 exactly-once。
2. 中止后直接继续、中止后补充继续、网络错误继续、余额错误继续。
3. 队列输入不会错误进入当前 Turn。
4. 子代理多次回流统计不重复计入父 Turn。

#### 退出门

Goal、继续、队列、重试和子代理语义只依赖 V1 事实。

### 阶段 7：资源、编辑、分叉、SQLite、Trace、终端与关闭

#### 7.1 项目资源

1. ResourceId 由项目级 catalog 管理，session 只保存有序引用。
2. 会话删除、归档、分叉不删除项目资源。
3. 丢失资源显示固定替代符，不阻止会话加载。

#### 7.2 编辑

1. 只允许编辑真实根用户输入。
2. 构建 `generation + 1`，封口前缀按字节复制，目标所在段重写，目标后事件删除。
3. 创建新的 context，保留仍位于目标边界内的有效压缩节点并重选指针，同时把 runtime 标记为强类型“基线待刷新”；generation 发布阶段不复制旧锁定来源，也不在 RPC/Store 层重复构造提示词。新 generation 的第一次物理 Provider prepare 必须使用唯一提示词组合器产出的最新强类型基线，在网络发送前与 `api_started` 同一可靠事务锁定。
4. 候选 generation/context 完整校验后，metadata 最后一次发布。
5. 对第 90,000 条附近事件编辑建立性能与故障测试。

#### 7.3 分叉

1. 新建独立 session，从分叉节点复制主事件历史；复制前缀已经按 seq 连续，Turn/API/Tool 等本地数字 ID 在不同 session 命名空间内保持原值，从而保留历史 Provider proof 与引用一致性，子会话后续分配器从完整复制事实重新构建。
2. runtime 不保留父会话锁定快照，而是发布强类型“基线待刷新”状态；仍在目标边界内的 compaction 节点可保留并重选指针。子会话第一次物理 Provider prepare 必须由唯一提示词组合器提供分叉时最新动态来源，并在网络发送前可靠锁定。
3. 不复制终端目录、TASK、Lua session、完整 API trace。
4. 工具历史正文和 ChangeSet 保留；外部导航对象不存在时只禁用导航。

#### 7.4 SQLite

1. 新建只服务 V1 的索引表，字段足以精确拼接项目与 session 目录。
2. 创建顺序：文件完整发布后插入 row。
3. 列表只读 row，不遍历目录；活动会话需要加载时直接拼接 metadata 路径。
4. 归档列表只显示 row；恢复时精确定位文件，不存在则报告并清理该 row。
5. 删除时目录存在则删除，不存在视为已删除，再清理 row。
6. 用真实 controller/SQLite 做创建、列表、归档、恢复、删除、锁冲突、连接失败和事务失败测试。

#### 7.5 Provider API trace

1. 开关关闭时完全忽略。
2. 开关开启时只保留最近 10 次请求，每次独立 input/output 文件，token/密钥替换为固定英文脱敏串。
3. 每次真实物理 HTTP attempt 使用独立递增的 `trace_sequence`，并同时记录 V1 `api_call_id` 与 `attempt_index`；不得复用 execution/context generation 充当轮换身份。
4. trace 写失败只记录诊断，不影响会话主事件提交。

#### 7.6 Release 终端与关闭

1. Release 启动 `shell_command`、shell session 辅助进程时统一使用隐藏窗口配置。
2. 正常关闭等待 session flush、trace 非阻塞收尾和终端安全停止；trace 失败不阻塞主数据，主数据失败阻止关闭。

#### 退出门

编辑/分叉原子性、真 SQLite、trace 轮换和 Release shell 启动均有自动化或可执行验收脚本。

### 阶段 8：服务 API、事件窗口与 WebUI 主链

#### 实现内容

1. 服务层新增 V1 hydration/page/live DTO，正文为强类型联合。
2. 最近 30 Turn、`before_seq`、开放 Turn 和工具组边界由后端统一计算。
3. 前端 `ConversationEventWindowStore` 正式接管 hydration、向前分页、实时 delta 和重连补洞。
4. 首次 hydration 建立连续窗口；向前分页必须与最早 seq 邻接；实时新增必须与最晚 seq 邻接。
5. 同 seq 更新前核对 kind 和实体 ID；冲突直接报结构错误。
6. UI 渲染严格按 seq，不对失败、工具、思考或系统消息二次排序。
7. 旧 transcript parser 仅可保留 assistant Markdown 正文解析职责，不再从文本猜工具、状态或 ChangeSet。
8. 重连请求携带最后确认 seq；服务只补缺失范围，重复范围幂等更新。
9. 移除 hydration 时先清空全部旧记录再重建的闪烁路径，使用同一窗口 store 合并。

#### 验证

1. hydration 与 live 对同一事件产生字节等价 DTO。
2. 重连重复、缺口、乱序、身份冲突、分页相邻和开放 Turn 场景。
3. 压缩期间旧内容不瞬间清空；压缩标记按 seq 出现，窗口保持已加载历史。
4. 重启后 reasoning、assistant、工具状态和显示错误均与关闭前一致。

#### 退出门

测试宿主 WebUI 不再依赖 messages/records 双模型和旧工具详情缓存。

### 阶段 9：从备份分支选择性迁移工具卡

#### 9.1 迁移纪律

1. 禁止 cherry-pick `a5d94faf`，禁止整文件无审查覆盖。
2. 先把 [工具卡恢复台账](./20260826-03-TOOL_CARD_RECOVERY_LEDGER.md) 补充为 hunk 级清单：来源文件、来源行块、保留目的、V1 输入、拒绝内容、对应测试。
3. 每个迁移片段同时对比 `main`、当前工作树和备份提交，避免恢复备份中的旧持久化补丁。
4. 工具卡迁移必须在强类型 V1 DTO 稳定后进行，不能为了复用卡片重新引入 detail RPC 或旧 record。

#### 9.2 可迁移范围

1. `conversation-tool-presentation-card.tsx` 的卡片容器、折叠行为、状态视觉和详情布局。
2. `file-change-presentation.ts` 的持久化 ChangeSet 渲染、增加/删除统计和 diff 行展示。
3. `markdown-output-presentation.ts` 的结构化 Markdown 工具结果展示。
4. `native-semantic-tool-presentation.ts` 的已明确工具解析和无图标目录列表行为。
5. `command-execution-presentation.ts` 的命令摘要与输出展示。
6. `conversation-tool-execution-model.ts` 中只与事件内工具状态有关的展示模型。
7. 对应测试与 `tool-presentations.css` 中纯视觉片段。

#### 9.3 明确拒绝迁移

1. `ConversationDetailRef`、`ToolDetailLoader` 和隐藏详情缓存。
2. message/record 查找、`legacy_partial`、工具批次 UUID 修复、重启后猜测状态。
3. 从当前文件系统重新读取文件来伪造历史 diff。
4. 按工具成功/失败拆分组合。
5. 展开卡片时额外发 RPC 获取历史正文。
6. 任何与旧持久化链路、旧 SQLite 或 state.json 相关的片段。

#### 9.4 卡片验收矩阵

1. `file` 读取：运行中、成功、失败、中止、零字节中间态、重启一致。
2. `file_edit`/`apply_patch`：卡片可展开，显示真实 `+/-` 行；删除行必须在正文中存在。
3. diff 头部的 `utf-8`、`LF/CRLF` 在展开前后来自同一持久化 ChangeSet，不因点击才出现。
4. 查询文件夹列表没有有效图标时隐藏占位点。
5. shell 成功与失败仍归属于同一 API 工具组合。
6. Markdown/native 结果、Ask、子代理、持久化终端导航在对象缺失时优雅禁用导航。
7. 关闭重启后卡片、思考、错误、工具状态、展开内容完全一致。

#### 退出门

工具卡只消费 V1 `tool_call` 事件；旧详情服务调用数为零；台账每个 hunk 有测试证据。

### 阶段 10：切换前全链路测试构件门

#### 必须完成

1. 新 owner、store、Provider projector、service DTO、WebUI reducer 和卡片在测试宿主中端到端运行。
2. P0/P1/P2 清单逐条有代码与测试证据，不允许“后续切换再补”。
3. 真 SQLite、真实 100k、90k 编辑、1/4/8 MiB 尾段、上下文窗口、Goal、Ask、continue、fork、trace 全部完成。
4. 更新切换清单为精确符号级接线和删除列表。
5. 生成生产调用图：旧入口、新入口、每个调用者、切换后的唯一目标。

#### 自动门禁

```text
rtk cargo fmt --all -- --check
rtk cargo check --workspace
rtk cargo clippy -p vulcan-session-core -p vulcan-change-set -p vulcan-workspace-store --lib -- -D warnings
rtk cargo test -p vulcan-session-core
rtk cargo test -p vulcan-change-set
rtk cargo test -p vulcan-workspace-store
rtk cargo test --workspace --doc
rtk npm run test
rtk npm run build
rtk git diff --check
```

涉及共享 Provider Manifest、服务内核注册表、端口或 controller 的测试按仓库要求使用单线程。

#### 退出门

测试宿主全绿、调用图完整、无未决架构问题后，才允许进入正式切换。

### 阶段 11：生产垂直接管

#### 切换原则

1. 生产接线在一个正式切换提交中完成；此前新链路可作为未接管构件存在，但不得双写正式会话。
2. 切换提交只做已验证构件的接线和旧生产入口移除，不在此阶段继续发明领域语义。
3. 切换前停止真实运行、关闭桌面程序并备份开发数据；切换后只创建 V1 新会话。

#### 同时切换的接线点

1. `vulcan-service-kernel`：session registry、flush、关闭屏障、hydration 入口。
2. `vulcan-agent-runtime`：submission、stream、工具、Ask、Goal、continue、retry、subagent。
3. `vulcan-core` 与 Provider bridge：从 V1 projector 构造请求。
4. `vulcan-workspace-store`：唯一 V1 writer、loader、index、resource、trace。
5. `vulcan-service-api`：V1 event window、控制和结构化错误。
6. WebUI：V1 window store、事件渲染与事件原生工具卡。
7. 桌面关闭、shell 进程和断链重连。

#### 切换后立即检查

1. 新建 session 的目录只包含 V1 文件。
2. 一次根输入只进入一个 owner、一个 writer 和一个事件流。
3. 一次工具调用前端只出现一个事件原生卡片。
4. 一次 Provider 请求只产生一个 `api_started` 和一个终态 `api_stats`。

#### 退出门

完成代码接线后立即进入阶段 12；不得在此处构建 Release 或进行真实会话验收。

### 阶段 12：旧链路物理删除

#### 12.1 后端删除范围

1. 删除 `workspace_session_persistence.rs` 中旧 writer、queue、pending owner、Drop flush、重试和 drain；如文件还有非持久化职责，仅保留无关职责并改名。
2. 删除 `segmented_session_store` 中 `message_events`、`message_mapping`、`runtime_io`、`runtime_log`、`runtime_backup`、`state_documents`、`session_reading` 等旧会话事实链。
3. 删除 `state.json`、runtime JSONL/index/backup 的常量、读写器、修复器和测试。
4. 删除 `provider_usage_migration`、Provider call ledger 及旧 usage 恢复。
5. 删除 `goal_store.rs` 的会话 Goal SQL 事实和 receipt。
6. 删除 `execution_retry_store.rs` 的会话重试事实副本。
7. 删除 session boundary 的独立持久化副本。
8. 删除消息/record 双写投影、UUID 工具批次关联、identity token rewrite 和旧 tolerant loader。
9. 删除所有旧格式兼容、自动修复、目录扫描和“缺字段兜底”。

#### 12.2 服务与前端删除范围

1. 删除 conversation tool detail RPC、缓存、loader 和 `ConversationDetailRef`。
2. 删除 `loadConversationMessages` 的旧消息/record hydration 路径。
3. 删除 transcript parser 中工具/状态/ChangeSet 猜测，只保留 assistant Markdown 正文解析。
4. 删除前端按成功失败重新分组、重启后补卡片和重新读文件构造 diff 的路径。
5. 删除对旧 `SessionRuntimeStateEntry` 展示副本的依赖。

#### 12.3 类型清理

1. `ConversationSnapshot.messages/records/continuation` 不再作为运行时权威或持久化结构。
2. 若 Provider 控制器仍需一次性请求快照，改成明确的临时 Provider 输入类型并只由 V1 projector 生成。
3. 删除不再有生产调用者的旧 ID、映射表和序列化类型。

#### 12.4 搜索门禁

对以下关键词逐一审核；结果只能是无关文档、历史测试删除记录或明确保留的非持久化传输类型：

```text
state_documents
SEGMENTED_STATE_FILE
runtime_io
runtime_log
runtime_backup
SEGMENTED_RUNTIME
append_segmented_session_message_tail
workspace_session_persistence
SessionProviderCallLedger
provider_usage_migration
goal_store
execution_retry_store
ConversationDetailRef
ToolDetailLoader
conversation-tool-detail-service
legacy_partial
message_mapping
rewrite_json_value_identity_tokens
SessionRuntimeStateEntry
ConversationSnapshot.messages
ConversationSnapshot.records
```

#### 退出门

1. 生产调用图只剩一个 owner、writer、loader、Provider projector 和 WebUI reducer。
2. 新 session 目录无旧文件。
3. 旧链路文件和符号物理删除或明确证明只剩非持久化无关职责。

### 阶段 13：全量自动化、故障、性能与 Release 桌面验收

#### 13.1 Rust 与 WebUI

1. 逐 crate check/test/clippy，再执行 workspace check 和必要的集成测试。
2. doctest 单独执行。
3. WebUI 全测试、类型检查和生产构建。
4. 所有新代码警告清零，不把旧基线警告混入完成声明；若发现仓库既有失败，先记录基线，再运行受影响精确测试。

#### 13.2 故障注入矩阵

至少覆盖：

1. 尾段临时文件写失败、flush 失败、rename 失败。
2. 滚段封口前、封口后、新尾段前、metadata 前后。
3. context 的 compactions 前后、runtime 前后、metadata 前后。
4. 编辑候选 generation 任一段失败、候选 context 失败、metadata 切换失败、切换后清理失败。
5. SQLite 连接、锁冲突、事务、row 插入、归档、删除失败。
6. Provider 零输出崩溃、流中断、工具运行中崩溃、stats 前崩溃。
7. UI 重连重复、缺口、乱序和身份冲突。
8. 关闭屏障中 flush 失败及重试成功。

#### 13.3 性能与规模

1. 活动尾段 1 MiB、4 MiB、8 MiB 完整写入的 p50/p95、吞吐和 CPU。
2. 100,000 条事件启动、最近 30 Turn hydration、向前分页、Provider 窗口和继续追加。
3. 第 90,000 条附近用户输入编辑的复制字节、耗时和峰值内存。
4. Provider 只加载保留窗口，不解析全部历史；记录实际读取分片数。
5. 与切换前同硬件基线比较，不允许出现由锁链、逐事件确认或全目录扫描造成的数量级退化。

#### 13.4 Release 桌面矩阵

1. 新建会话、多轮文本、思考、并行/连续工具、文件读写、ChangeSet。
2. 工具运行中关闭/异常退出再打开；状态与卡片一致。
3. 用户中止、直接继续、补充继续、网络断链恢复、重试耗尽继续。
4. Goal 自动继续、关闭重启 exactly-once、完成/阻塞状态。
5. 上下文阈值运行中切换并在下一判定生效。
6. 压缩期间窗口不清空、完成后顺序不波动。
7. 分叉、编辑、归档、恢复、删除。
8. Provider trace 开关与 10 次轮换。
9. 每次 `shell_command` 都不出现控制台弹窗。
10. 关闭时主 flush 失败阻止退出，恢复后可重试；手动终止只显示一次简洁停止记录。

#### 退出门

自动化、故障、性能和 Release 矩阵全部有保存证据；任何未运行项目必须阻止完成。

### 阶段 14：连续五轮全范围代码审核

#### 规则

1. 每一轮都是全范围审核，不是只审某一专题；专题仅作为该轮重点。
2. 每轮必须检查源码、调用图、测试、错误路径、删除结果和 Release 证据。
3. 任一轮发现有效问题，立即记录、修复、补回归测试，并把连续零问题轮数重置为 0。
4. 只有连续五轮完整审核均无问题才可进入阶段 15。

#### 五轮重点

1. 第一轮：数据所有权、flush、崩溃一致性、跨文件提交、关闭屏障。
2. 第二轮：生产调用图、旧链路残留、双写/双读、SQLite 和目录寻址。
3. 第三轮：Turn/API/工具/Goal/Ask/continue/统计与 Provider continuation 语义。
4. 第四轮：WebUI seq 顺序、重连、分页、工具卡、ChangeSet、重启一致性。
5. 第五轮：故障注入、100k、性能、Release 行为、文档与测试真实性。

每轮结束输出：覆盖文件、调用链、运行命令、发现、修复提交、回归结果、当前连续零问题计数。

### 阶段 15：文档、提交与任务闭环

1. 更新架构审计台账，把最终源码位置和测试证据回填。
2. 更新执行状态文档，删除过时数量，记录最终真实命令和结果。
3. 完成工具卡 hunk 台账和切换清单。
4. 在本文末尾追加“执行变更总结”，包含概述、文件变更清单、关键代码调整、遗留问题与注意事项。
5. 检查 `git status`，只提交本任务拥有文件。
6. 正式切换提交和后续审核修复提交使用中文“标题 + 空行 + 分条正文”。
7. 全部完成后把本文移动为：`docs/completed/20260827/01-SESSION_PERSISTENCE_V1_FINAL_CUTOVER.md`。

## 9. 建议提交序列

以下提交用于降低实现阶段评审规模；阶段 11/12 的正式接线和删除仍保持一个原子切换提交：

1. `修正会话持久化 V1 执行基线与事实状态`
2. `修复会话事件领域契约与分配器恢复`
3. `完善会话分片驻留与原子上下文提交`
4. `建立会话唯一内存所有者与刷新协调器`
5. `接通 Provider 主事件投影与物理请求记录`
6. `接通上下文压缩 Ask Goal 与继续状态机`
7. `完善会话资源分叉编辑索引与调试追踪`
8. `接通会话事件窗口与事件原生工具卡`
9. `全面切换会话持久化 V1 并移除旧链路`
10. 后续审核若发现问题，每个修复按独立中文提交记录原因和回归测试。

不允许把备份提交整体 cherry-pick 到任一步骤。

## 10. 测试覆盖表

| 领域 | 最低覆盖 |
| --- | --- |
| ID/seq | 空尾段、滚段、多 generation、分叉、编辑、JS 安全上限、重复拒绝 |
| 事件 | 13 类事件序列化 golden、可见性、原位更新、未知字段拒绝 |
| ToolPayload | 对象成功，null/数组/字符串/数字失败，无 panic |
| API | 开始、零输出、reasoning、text、工具、成功、失败、中断、continuation |
| Turn | 根输入、插队、排队、中止、补充继续、无输入继续、唯一统计 |
| Goal | 自动继续、完成、阻塞、预算、重启 exactly-once |
| 压缩 | 两节点、动态窗口、候选失败、指针发布、最新锁定来源 |
| 存储 | 1/4/8 MiB、3000 条滚段、100k、原子替换、候选清理 |
| 编辑/分叉 | 90k 编辑、前缀复制、尾部删除、资源引用、导航对象缺失 |
| SQLite | 真库创建/列表/归档/恢复/删除/锁/事务失败 |
| WebUI | hydration/live 同 DTO、分页、重连、缺口、冲突、严格 seq |
| 卡片 | file/file_edit/shell/Markdown/native/Ask/subagent/terminal 与重启 |
| Release | shell 隐藏、断链恢复、关闭屏障、压缩切换、Goal 继续 |

## 11. 旧链路删除判定标准

“不再调用”不足以判定删除完成，必须同时满足：

1. 生产调用者为零。
2. 类型、常量、文件格式、数据库表、序列化入口和测试均已删除。
3. 新代码没有 legacy、fallback、repair、migration 或双写开关。
4. 新会话目录不会生成旧文件。
5. WebUI 网络面板不会出现工具详情补拉 RPC。
6. Provider 请求不再从旧 message/record/usage ledger 取历史。
7. Goal、retry、boundary、tool status 不再拥有第二份数据库或 runtime 副本。

## 12. 回滚策略

1. 阶段 0-10 均为未接管构件，可按提交回退，不影响正式旧链路运行。
2. 阶段 11/12 前必须保存代码提交和开发数据备份。
3. 阶段 11/12 失败只允许整体回退正式切换提交，不允许在产品中保留半切换并加兼容补丁。
4. 由于明确不兼容旧会话，切换后测试只使用新建 V1 session；回退时删除测试 V1 数据即可。
5. 不通过自动“修复”持久化文件实施回滚。

## 13. 执行过程报告格式

每个阶段完成时必须报告：

1. 本阶段实际修改的文件与符号。
2. 与本方案相比完成、偏差和未完成项。
3. 执行的测试命令、通过数量、失败数量和忽略数量。
4. 新发现问题及其优先级。
5. 是否满足退出门；不满足时禁止进入下一阶段。
6. 当前工作树中非本任务修改的保护情况。

## 14. 执行变更总结

本节在全部实现、验证和五轮审核闭环后填写；当前不得预先写“完成”。

### 14.0 执行中事实记录（2026-08-27）

1. 阶段 1 至阶段 3 的 V1 领域文档、分片存储、唯一会话所有者与串行 flush 基础已经形成；当前只代表局部构件成立，不代表生产链路已经切换。
2. workspace-store 的 V1 定向测试曾达到 62 项通过、全量测试 371 项通过；crate 级 `cargo check` 与严格 Clippy 已通过。生命周期测试仍需等待并行 Goal 迁移恢复 service-kernel 编译后重新执行，不沿用被阻塞前的结果冒充最终门禁。
3. WebUI V1 事件窗口、显示投影和工具卡迁移已有 47 项定向测试通过，前端构建通过；旧 transcript/card/detail 生产链尚未删除，因此不得宣称工具卡已经完成最终接管。
4. Provider 工具批次的根提交点固定为 `BatchPrepared`：必须在同一 durable mutation 中创建全部工具事件，并把强类型 Provider continuation/source 绑定到最后一条 Provider 可见事件；Ask 或长工具开始前不得只把 source 留在 sink 内存。
5. 工具调用上下文必须从 `CoreToolInvocationRequest` 强类型传递准确的 session 与 Provider tool-call identity；禁止通过“最后一个 Ask”、工具名或文本匹配恢复身份。
6. Ask 的等待、回答和取消必须先修改并 flush V1 工具事件，再发布 WebUI 状态或恢复阻塞 future；进程内 oneshot/HashMap 只允许作为实时等待器，不得成为持久化事实源。
7. 当前 `LockedSourceKind` 已覆盖冻结方案要求的八种可变基线来源，但生产提示词组合器尚未把记忆画像、实际参与组合的所有层级 AGENTS/Vulcan、当前人格、当前输出风格、提示词技能固定注入、长期插件指令与基线 Hook 固定正文提取并接入 `runtime.json`。阶段 5 退出前必须建立唯一的强类型基线提取入口和重启/压缩刷新测试，不能以空 `locked_sources` 或测试夹具替代生产接线；Provider/Agent 基础提示词、完整最终系统提示词、工具 Schema/权限/本次可见性、环境上下文、Goal/Plan、单 Turn VMM 召回与临时 Hook 必须继续由当前请求或主事件动态组合，不得误塞进 runtime lock。
8. 当前仍处于新链路构件收口期，旧持久化链仍是生产路径；阶段 10 全链路构件门通过前不允许生产切换，阶段 11 接管完成前不允许执行阶段 12 的旧链物理删除。
9. 同一 `seq` 的流式文本、工具状态和子代理统计会原位演进，不能使用 `updated_at_ms` 充当并发版本；毫秒可能碰撞或回退。主事件必须保存 JS-safe 的单调 `event_revision`，每次原位语义变更严格递增，前端按该字段判断 stale/same/evolution，时间字段只用于展示。
10. 重连不能只从 `last_confirmed_seq` 读取：开放 Turn 内较早的并行工具或流式事件可能在客户端确认更晚 seq 后继续原位演进。存在开放 Turn 时，服务必须从 `min(last_confirmed_seq, open_turn_start_seq)` 重发完整开放 Turn；关闭 Turn 已全部终态，才可只补确认边界及其后的范围。
11. 阶段 12 切换前基线搜索仍显示大量旧链残留：`runtime_log` 30 个文件、`SessionRuntimeStateEntry` 99 个文件、`ConversationDetailRef` 20 个文件、`SessionProviderCallLedger` 12 个文件；`runtime_io`、`runtime_backup`、`provider_usage_migration`、`goal_store`、`execution_retry_store`、`message_mapping` 与 identity rewrite 也仍有生产命中。该结果仅作为删除基线，阶段 11 接管前不得提前移除；阶段 12 必须逐项区分会话旧事实与无关任务会话职责，并用搜索门禁归零或给出明确保留证明。
12. `create_and_register_root_session_v1` 已接入唯一生产 materialize 边界：旧会话身份与项目定位完成后、任何 Goal/Plan/config durable mutation 之前，新会话先按项目目录注册有序附件 ResourceId，再一次性发布 Fresh V1 文件、SQLite 精确定位行与 registry owner；现有会话只按 SQLite 精确地址加载。root 明确保持 `FreshUninitialized`，不再以空 `locked_sources` 冒充已初始化。该进展只证明 root owner 接线完成，首次真实 Provider 请求的 baseline 初始化与完整请求持久化仍属于阶段 4 阻塞项。
13. 当前 `provider_visible_fingerprint` 会复制覆盖范围内全部事件并以 `retained_from_seq = None`、空 system backfill 重新投影；工具批次绑定来源前还会额外复制整个常驻事件集合。该摘要既不是实际派发请求所使用的压缩窗口、活动系统回追与 runtime 锁定前缀，也形成随历史增长的重复全量复制。阶段 4/5 必须让 continuation 指纹来源于唯一真实 Provider 请求投影或其等价强类型覆盖证明；Core 只负责校验与落点，禁止独立重建另一份上下文摘要。
14. Goal/Plan 仍存在生产旧链残留：原生 Goal 工具、初始提交 Goal、Plan 互斥校验、旧 agent-stream host-result 投影、重启后的 Goal continuation 调度以及压缩/模型 UI 投影。迁移必须以 V1 `SessionHandle` 与主事件为唯一事实源：服务调用 Core 原子转换，错误不得留下半提交 metadata；生产加载后只依据已持久化开放 Goal Turn 恢复一次调度；旧 host-result/SQLite cache 投影在阶段 11 接管时直接删除，不建立适配层。
15. `system_context` 的 `remove` 既是状态失效操作，也可能承载进入/退出模式的真实 Provider 系统语义。完整 replay 必须按原 seq 投影明确的退出正文；压缩边界前的 active-system 回追则必须在 `remove` 后删除该 key，不再回填旧 `upsert`。禁止丢弃 remove 导致模型只看见进入指令，也禁止由服务层伪造另一个 upsert 规避状态机。
16. Release `shell_command` 弹窗的独立根因已收口到 `vulcan-host-tools::process_creation` 的共享构造工厂：std/Tokio 后台子进程在创建时统一获得 Windows `CREATE_NO_WINDOW`，四个真实入口不再各自后补 flag；PTY 内部 shell 仍由 `portable_pty` 创建，交互语义未改。已通过 host-tools Debug/Release 契约测试、103 项全量测试、三类命令工具定向测试、check 与严格 Clippy；agent-runtime 全 crate check 仍需在并行 Ask 迁移恢复编译后重跑，当前不得把该局部证据扩张为最终 Release 桌面验收。
17. runtime lock 不能直接复制当前 `PromptAssemblyOutput` 的全部片段：现有 composer 把 Provider/Agent/人格/风格/项目规则/环境/工具目录等基线，与请求用途、Plan、prompt skill 等 custom developer 片段混在一次输出中，hooks 又在 composer 后追加。实现必须先按唯一来源把“跨请求锁定基线”与“按 seq 动态 system_context/单请求控制”分层，再把最终锁定片段按稳定 node id、目标层、顺序与正文持久化；首次请求前与压缩刷新时使用同一组合入口，普通后续请求不得重新读取变化中的 AGENTS、画像、风格或 hook 来覆盖当前代次。
18. 当前 `RuntimeLock` 仅以空 `locked_sources` 表示状态，无法区分“新建后尚未锁定基线”与“基线已初始化但合法为空”；落盘重启后该歧义会使首次请求错误跳过初始化，或让普通请求重新读取最新来源。V1 Schema 必须持久化强类型初始化状态：root 新建为 `FreshUninitialized`，唯一基线初始化领域方法转为 `Initialized`，物理 `start_api` 拒绝 fresh，initialized-empty 合法；workspace-store 必须原样往返该字段，生命周期只能显式调用 durable 初始化入口，禁止用空数组猜测。
19. 当前 `ProviderV1StreamSink::start_request` 接收在外部先行构造的请求事实，再独立追加并 flush `api_started`；若真实 V1 请求投影发生在前，两步之间仍可能插入同会话修改，使记录的请求快照、指纹与实际派发上下文失配。阶段 4 必须建立唯一串行请求准备事务：先把既有脏状态可靠发布，在同一 owner 冻结边界内确定 retained/through/runtime/system 状态并读取所需封口分片，使用同一投影产生物理 Provider 请求和指纹，同时追加 `api_started`，可靠落盘后才允许网络派发。禁止在 sink 外先构造另一份 facts，再由 sink 补记起始事件。
20. Core 的失败原子性不能只覆盖 Goal：当前 `record_compaction` 在分配新 ID 前先裁剪候选并改 runtime 指针，ID 分配失败会留下无 revision 的部分修改；`truncate_after` 先原地截断再执行可失败重建，后续编辑/分叉接线后同样可能污染文档。全部 durable 领域方法必须遵循“先验证与暂存，后一次发布”，并用错误前后完整文档相等测试证明；同值 runtime/config 设置必须显式返回 unchanged，不能靠伪造 revision 让 registry 误判为成功修改。
21. 项目级 `resource_catalog` 当前没有生产调用，submission 的附件仍是 UUID/路径语义；root V1 创建若传空 `ResourceId` 列表会从第一条输入永久丢失附件事实。唯一会话物化边界必须在 root 发布前，依据附件权威解析结果和项目定位按原顺序 resolve-or-register 项目级资源，再把精确 `Vec<ResourceId>` 写入 root input。附件解析或注册失败必须阻止会话发布；允许已成功注册的项目资源成为未引用永久资源，但禁止留下半发布 session，也禁止猜测 UUID 到 int64 的映射。
22. registry 的幂等 durable 入口已能区分 clean-same、dirty-same 与真实 changed，但错误契约也必须闭环：如果 closure 已推进 revision 却错误报告 `Unchanged`，不得只返回 mismatch 并把新脏状态静默留在内存。生产调用必须直接映射 Core 强类型 `changed` 结果而非服务层自行判断；registry 检测到“已变化但报告未变化”时必须可靠处理该修订或显式封锁会话继续，测试同时覆盖虚假 Changed、虚假 Unchanged 与 Core 返回 Err 后文档完全不变。
23. Goal 自动继续已开始用 V1 活动 Goal 与开放 `GoalContinuation` TurnId 作为可靠权限，并在正式提交准入、Hook 与 kernel 注册前重复校验；但当前仍把适配条目写入旧 `SessionRuntimeState` PendingPrompt 队列，且 stale 清理只比较 goal_id、不比较 turn_id。最终接管时该队列不得成为第二份持久化事实：应在单次准入内从 V1 authority 局部构造适配对象，或证明该 UI 队列完全不落盘；同一 goal 的旧 turn 条目必须按强类型 turn_id 清除，测试覆盖重启、同 goal 不同 turn、并发唤醒和 exactly-once。
24. `add_project_resource` 的单次实体/目录原子替换不能替代项目级并发所有权：调用方若各自加载 catalog 后分配 `next_resource_id`，两个会话并发提交会分配同一 ID，后发布者覆盖先发布者。生产资源 admission 必须通过按项目定位的唯一内存 owner/Mutex 串行执行 load、resolve-or-add 与 publish；正式环境只考虑单 Release 进程，但必须测试同进程多会话并发注册、同内容实体规则和失败后连续分配。
25. 当前 Provider 投影对每个 `ToolCall` 都生成结果，并在 `output` 缺失时以空对象代替；中断、崩溃恢复、取消或失败工具因此会在下一请求中丢失终态语义。领域层必须保证每个终态工具都持久化明确的模型可见 output：成功为真实结果，失败/取消/中断为固定强类型错误结果；投影只允许读取已持久化结果，终态缺失 output 必须显式拒绝请求准备，禁止把空对象当兼容兜底。测试覆盖失败、手动中止、崩溃恢复、Ask 取消和下一 API 的 call/result 配对。
26. V1 `SystemContextEvent.ttl_turns` 目前只有 Schema 与 golden，Provider 投影和 active-system backfill 完全不消费它，短期记忆/Turn 作用域注入会永久留在模型上下文。迁移必须以现有 context-injection 权威语义为基准：transient 初始化 remaining=ttl，每个唯一逻辑 turn 只消费一次，归零删除；V1 投影需从 seq/turn 推导同等结果，覆盖 Upsert 重置、Remove 提前终止、Turn scope、同 turn 多 API、跨压缩回追与重启。若当前事件字段不足以唯一推导，应直接修正新 V1 Schema，禁止保留无执行语义的 TTL 字段。
27. 旧 Core 测试曾要求崩溃后的 `Prepared` 工具保持可执行，但当前生产 hydration 没有普通工具 exactly-once 恢复调度器，应用进程重启后原执行上下文也已消失；保留 Prepared 会让 Continue 的 Provider 投影面对无 output 的历史 tool call。正式契约应把崩溃时 Prepared/Running/WaitingUser 全部原位终结为 Interrupted 并写明确模型可见结果，已可靠完成的工具保持原终态；不为不存在的恢复链保留“以后也许执行”状态。测试必须验证重启显示、下一 API 配对和已完成工具不被误改。
28. 物理全尾文件保存与 WebUI 逻辑增量不能混为一谈：当前每次 durable flush 都把整个活动分片 `tail_events` 当 live batch 发出，而首次 hydration 只加载最近 30 Turns；当同一活动分片含更早 Turn 时，前端会收到窗口外未知旧 seq 并判定 gap。最终协议应由 Core 记录本修订真实变化的 event seq，并发送原位更新与新追加的最小强类型 patch；若暂时保留整尾发布，前端也必须明确只忽略 `seq < earliest` 的窗口外前缀，窗口内部缺口/冲突仍严格失败。测试覆盖单分片超过 30 Turns、较早并行工具原位更新、分页后 revision 最新性与重连。
29. `segment_generation` 只在编辑 generation 切换时递增，普通 flush/滚段保持不变；前端当前对更高 generation 的 push 直接走 exact-owner `applyLive`，必然 generation mismatch，且编辑可能已截断历史，本来也不能按普通 patch 合并。收到更高 generation 时必须进入显式 rebase：暂停旧 owner 增量、重新 hydration 新窗口、处理期间 delta 与并发多次 generation 跳跃，成功后只接受新 owner；低 generation 继续安全忽略。测试覆盖开放 Turn 编辑、已加载尾部被截断、hydration 期间新代次 push 与旧响应回写竞态。
30. 调试 Provider trace 的固定 10 槽目前以 caller `generation` 取模并拒绝同 generation 的不同 api_call_id，但生产尚未接线，若复用 execution/context generation 会原样重现“一个 generation 内 group identity 变化”错误。该支线必须使用每次真实 HTTP request/response 严格递增的独立 `trace_sequence`，identity 同时记录 V1 api_call_id 与 transport attempt_index；同一 API 的重试各占一槽，主事件统计不依赖 trace 成败。开关关闭不触盘，开启时只保留最近 10 个完整脱敏请求对，测试覆盖同 execution 多 API、同 API 多 attempt、重启继续、幂等重复与并发完成。
31. V1 lifecycle facade 当前 archive/rename/restore 主要只修改 SQLite，而冻结的 `metadata.json` 同时拥有 title、archived_at 与 updated_at，继续下去会形成重启前后两套值。完整会话 metadata 必须是权威，SQLite 只是精确地址与列表投影：rename/archive/restore 先做幂等 durable metadata mutation，再更新 SQLite；索引失败显式返回且允许同操作重试补齐。启动时读取 active row 对应 metadata 可完成已提交 metadata 的列表投影，不等于损坏自动修复；删除仍先删精确目录再删 row。列表排序增加稳定次键，测试覆盖每个文件/SQLite失败窗口和重启一致性。
32. `metadata.updated_at_ms` 按方案是会话更新时间，但当前普通输入、API、流式语义、工具、Turn、压缩与 runtime lock 修改均未触碰它，仅少数 Goal/Plan 入口赋值，侧栏与重启排序会长期停在旧时间。每个真实推进 memory revision 的领域边界必须以调用方提供的 `now_ms` 将 metadata 时间更新为 `max(current, now_ms)` 并标记 Metadata dirty；同值 no-op 不更新时间，event_revision 仍是并发版本，毫秒只用于展示/排序。流式过程继续只在既定 flush 边界落盘，不因此逐 token I/O。
33. 当前 V1 live emitter 只在 durable flush 后发布整尾，而 reasoning、assistant text 和 running tool output 的流式更新仅修改内存，正式切换后仍会复现“0 B/中间不更新/结束瞬间跳变”。每个成功的内存语义 mutation 必须立即发布本 revision 的精确 event patch，磁盘仍按既定语义边界全量保存尾文件，绝不逐 token I/O；Core mutation receipt 提供真实 changed seq，前端以 seq+event_revision 接受 volatile 演进，后续 durable 同 revision 重复幂等。正常关闭 flush 全部 dirty；崩溃重启以最后 durable hydration 替换 volatile 状态。
34. 工具参数形成阶段已经有独立事实链：Provider 先产生 `Detected`，随后产生一个或多个 `ArgumentsDelta`，最后产生 `Completed`；协调器还明确让首个非空参数增量绕过节流。因此“先显示 0 B、随后中间不更新”不是 Provider 缺少增量，而是实时投影或显示接续丢失。V1 主事件不应为每个参数碎片分配 seq 或逐片落盘，但必须保留 `toolRequestProgressChanged` 这条 live-only 高速旁路：按 `request_stream_id + request_revision` 单调覆盖，首个非空增量立即显示，Ready/Invalid/Aborted 后清理；`BatchPrepared` 持久化 ToolCall 时以同一 request/tool identity 接管卡片。删除旧持久化链时不得误删这条实时协议，测试必须覆盖 0 B→非空→多次更新→Ready→V1 ToolCall 接管、乱序/重复和重连不复活。
35. Plan 模式开关不能由 service 先改 metadata、再补一条系统事件：开启必须在 Core 同一 staged mutation 中完成 `metadata.mode.plan.enabled=true` 与明确非空 `plan_mode` SystemContext Upsert，关闭必须同次完成 metadata=false 与同一稳定 identity 的 Remove。输入、Goal/Plan 互斥、活动 API 和上下文身份须在任何修改前完成校验；错误前后完整文档相等，同值操作不重复写事件、不推进 revision，`updated_at_ms` 只在真实变化时单调更新。service 只能调用该单一领域入口，禁止给母事务打补丁。
36. 用户手动停止不是错误通知：V1 `interrupt_active_api` 应只保留 HostOnly 的执行控制事实、Interrupted 工具/语义、UiOnly 统计和过程组“你在 {duration} 后停止了”的状态展示；`ApiEndReason::UserInterrupted` 不得生成额外 `DisplayNotice` 或 SYSTEM 错误正文。Continue 仍继承原开放 Turn。切换验收必须覆盖停止前后即时界面、重启水化与继续后无重复停止提示，防止旧停止投影重新混入新链。
37. 会话配置 setter 的幂等结果必须贯穿 service：例如 Core `set_context_compaction_window_node_id` 同值返回 false 且不 bump，service 若仍使用要求 revision 前进的普通 `mutate_durable`，重复点击就会以 `MutationProducedNoRevision` 失败并表现为“RPC 有请求、界面无反应”。全部返回 changed/no-op 的配置入口必须使用 registry 条件 mutation，并由 Core bool 直接映射，禁止 service 预读猜测；同值请求返回当前投影且不触盘，真实变更持久化后立即更新 UI，并只在下一安全 Provider 请求边界生效。需横向扫描输出风格、当前/下一模型、压缩阈值及模式开关的同类错误。
38. Core 的 allocator 也属于事务状态：当前 `submit_root_input`、排队/插队输入、`start_api` 等入口仍可能先在真实 allocator 上分配 int64 ID，再由事件容量、队列容量或后续校验返回错误，造成“失败但 ID 被消耗”的隐性半提交。所有分配 ID 的领域入口都必须在 staged allocator 与 staged 文档片段上完成全部可失败步骤，最后一次发布；错误前后整个 SessionDocument（含 allocator）完全相等。被新原子 Goal 流取代的旧 continuation helper 应在调用归零后删除，不为死链补丁。
39. Hydration 窗口与实时补丁是两种不同协议：分页/重连窗口必须连续，实时演进则是由单次 revision 的 `changed_event_seqs` 定义的稀疏 patch。Core 读取实时 patch 不得调用会生成 flush candidate 副作用的持久化准备方法；registry 在文档锁内捕获精确 revision 对应事件、锁外立即发布 volatile patch，flush 仍全尾写盘，成功后用相同 changed seq 发布 durable 确认。前端保存全部 visibility 以维护 seq 连续性，但只渲染可见事件；同代次稀疏更新、窗口外旧 seq、连续新 append 与 generation rebase 必须分别处理，不能继续把 sparse patch 伪装成 WindowBatch。
40. Provider 原生 replay/continuation 必须绑定到该物理 API 最后一条 Provider 可见的 reasoning、assistant text 或 tool-call 事件；工具批次在 `BatchPrepared` 时绑定到最后一个 ToolCall，终结时只校验同一终态 identity 并写唯一统计，不能重复绑定。Provider 若以成功状态结束却完全没有任何 Provider 可见语义，必须在传输/投影边界显式判定为空响应错误，不能等到持久化闸门以通用引用错误失败，也不能制造虚假 assistant 事件来承载 continuation。
41. 输出风格与下一请求模型选择仍是配置权威迁移的一部分：`SelectionConfig.output_style` 必须由 Core 幂等原子 setter 修改；下一模型、思考级别与连接必须写入并由物理请求准备事务消费 `PendingRequestConfig`。同值配置不推进 revision、不触盘；真实变化只在下一安全 Provider 请求边界生效。禁止继续让旧 `SessionAddon` 或 `pending_model_selection` 成为生产权威，也禁止 V1 只验证在旧链外部已经构造完成的请求。
42. Goal 自动继续的权威触发点必须位于最终 assistant 完成的可靠领域边界：同一 Core 原子转换关闭当前 Turn、写入唯一 Turn 终结事实，并在 Goal 仍活动时创建下一条开放的 `GoalContinuation` Turn；该转换 flush 成功后才允许唤醒下一次派发。仅在 WebUI finished hook 中检查开放 Goal 不足以产生下一 Turn，也会重现“一轮结束后目标不继续”；生产调用归零不得以测试覆盖冒充接线完成。
43. 系统将 Goal 转为错误/阻塞状态同样必须 failure-atomic。旧 `update_goal_state` 若先修改 metadata 和 revision、再追加展示事件，会在容量或序列分配失败后留下半提交文档；应改为完整 staged 转换或以专用 `block_goal` 强类型入口替代。服务层 accounting 只能调用该单一领域入口，不能克隆整文档回滚或分步补记通知，测试必须验证所有失败点前后完整文档相等。
44. root 会话目录创建与首次运行时基线锁定不是同一时刻：生产 materialize 必须把 `runtime.json` 可靠保存为 `FreshUninitialized`，不能因创建请求尚无最终来源而用空 `locked_sources` 提前初始化。第一次物理 Provider 请求的唯一串行准备事务在完成 Provider/Agent、人格、输出风格、AGENTS/Vulcan、提示词技能固定注入与稳定 Hook 快照组合后，才以真实 `Vec<LockedSource>` 原子执行 baseline 初始化、请求投影与 `api_started`，flush 成功后派发网络；任一步失败完整文档不变。重启后的 Fresh 仍为 Fresh，Initialized 后普通请求不得重读变化中的来源。
45. API 终结与中断本身是一项母事务，不能逐条原位终结 reasoning/text/tool 后再追加统计：`complete_api_group` 与 `interrupt_active_api_state` 必须先在 staged 事件、allocator、active API 和来源绑定上完成全部校验与变换，再一次发布所有原位 revision、ExecutionControl、唯一 ApiStats、可选失败展示以及 active API 清理。第二条事件变换失败、provider source 绑定后统计追加失败、统计后展示追加失败、event_revision 上限或 seq 不变量失败时，完整 SessionDocument 必须与调用前相等；禁止以预估容量替代完整 failure-atomic 证明。
46. Continue 与普通 Turn 关闭也必须是最小原子语义组：`continue_turn` 的 `TurnContinueRequested` 与 `TurnResumed` 两条 ExecutionControl 只能同时出现；`close_turn` 的 TurnBoundary 与唯一 TurnStats 只能同时出现并在同次提交后更新 open-turn metadata。第二条事件分配或校验失败时不得留下单边记录。普通关闭与 Goal 完成应复用同一 staged closure 构造逻辑，避免两套实现再次分叉。
47. 工具请求草稿与 V1 ToolCall 的交接必须有同一持久身份：`RuntimeEventKind::ToolRequestStarted` 必须立即投影 Detected 的 0 B 快照，ArgumentsDelta 按 request revision 演进，Ready/Invalid/Aborted 发布明确终态清理；`ToolPreparation` 与 `ToolCallEvent` 必须持久化准确的 `request_stream_id`，BatchPrepared 从原 Provider recovery request 原样传入。前端只能以该 ID 让 V1 卡片接管实时草稿，不得用工具名、出现顺序或另一个 tool-call ID 猜测；测试覆盖 0 B 到非空、多次更新、终态 tombstone、V1 接管、乱序与重复。
48. 上下文压缩节点发布与新代次 locked sources 刷新必须属于同一领域母事务：压缩 API 成功后，单一强类型提交同时暂存 CompactionNode、`current_compaction_id`、最新八种基线来源和 allocator/revision；不得先 `record_compaction` 再独立 `set_locked_sources`。磁盘仍按“节点候选先写、runtime 指针与来源后发布”实现崩溃一致性，runtime 写失败时旧 pointer 与旧 sources 继续权威，未发布候选仅在下一次正式节点写入时按已发布前沿清理；重试不得制造重复节点。
49. 实时事件的内存可见与可靠落盘必须由 owner/generation 级 revision 前沿完整表达：初始 hydration/rebase 必须返回同一文档的精确 `memory_revision` 与 `durable_revision`，且满足 `durable_revision <= memory_revision`；前端在原子换窗时同步建立两条前沿。`volatile` 稀疏 patch 只能推进内存前沿并应用事件，空事件 `durable` ack 只推进可靠前沿；陈旧重复必须幂等，revision 回退、越过内存前沿或同 revision 内容冲突必须显式失败。历史分页不得覆盖当前前沿，generation rebase 必须以新 hydration 前沿整体替换。禁止仅在 DTO 声明 `revision/durability` 却不消费，也禁止 hydration 合并未 flush 的 resident 事件后把它们伪装成已可靠落盘。
50. Core API 终结/中断、Continue 与普通 Turn 关闭的母事务暂存实现已落地：事件副本、allocator、Provider source、stats/notice 与 active 状态均在验证完成后一次发布，普通 Turn 与 Goal Turn 复用同一 closure 构造；当前 `vulcan-session-core` 177/177 通过。该证据尚不替代故障注入、首次 Provider prepare、压缩刷新和全工作区门禁；执行状态继续保持阶段 4 进行中。
51. 切换前旧链残留基线已按源码文件命中数固化：`workspace_session_persistence` 4、`SessionProviderCallLedger` 12、`provider_usage_migration` 5、`VulcanGoalStore` 9、`execution_retry_store` 2、`ConversationDetailRef` 20、`ToolDetailLoader` 1、`SessionRuntimeStateEntry` 99、`append_segmented_session_message_tail` 5、`runtime_backup` 5、`state_documents` 3；`legacy_partial` 已为 0。该统计只用于阶段 12 的符号级清除对照，不能在新链尚未完成生产接管时机械删除。
52. WebUI 已实现 hydration/rebase 的 `memory_revision`/`durable_revision` 原子前沿、volatile 稀疏 patch、空 durable ack、分页隔离和 generation rebase，定向测试达到 101 项通过，TypeScript 检查与生产构建通过；主控复核同时发现 DTO 尚无 predecessor/base revision，接收端可从 revision 3 直接接受 revision 5 的稀疏更新并永久遗漏 revision 4 的独立 changed seq；registry 还会跳过没有 changed event 的 metadata-only revision，使下一条事件 patch 的基线天然断裂。该 P0 已退回 Rust/TypeScript 协议母层补充连续性证明，并要求每个真实 revision 都发送可为空事件的 volatile edge 与 durable ack；修复前不得把阶段 8 标记完成。
53. 首次物理 Provider 请求的跨层边界已经冻结：`vulcan-session-core` 只接收 provider-agnostic `ProviderProjectionProof`、`PrepareAndStartApiInput` 与八种强类型 locked baseline，不反向依赖 transport 请求；唯一 `SessionHandle` owner 使用精确 locator 与同一 `segment_generation` 读取连续事件，由 Agent Runtime 从同一 immutable projection 一次生成网络请求与 fingerprint，Core staged 初始化 Fresh baseline 并追加 `ApiStarted`，flush 成功后才把同一个请求对象交给 transport。sink 不再二次构造或补记事实。
54. Goal 保存已新增 failure-atomic `save_goal` 单入口，覆盖 disabled 创建、活动目标更新与 Completed 后创建新 Active generation；但 native `update_goal(status=complete)` 暴露出 Core 尚无 `complete_goal` 原子终态转换，生产 `VulcanGoalStore` 仍有三个入口。阶段 6 只有在 `complete_goal`、service/native V1 迁移和生产旧 store 零命中后才能退出。
55. registry 的领域契约破坏处理仍不合格：`mutate_in_memory` 遇到 Err 后 revision 已推进会留下污染内存，`mutate_durable_if_changed` 甚至会把 Err/虚假 Unchanged 泄漏出的 revision 主动 flush 后再报错。该行为会把已知半提交永久化，违背“Core 失败原子、磁盘保留最后可靠状态”的根原则。统一修复要求是正常 Domain Err 前后 revision/文档完全相等；任何 Err+revision、成功无 revision 或 changed 声明不一致都把唯一 SessionHandle 置为显式 quarantine，拒绝后续 mutation、flush 与网络派发，且绝不发布 live 或写入泄漏候选。
56. revision 前沿协议必须同时证明相邻实时边、可靠确认边与重连恢复边：稀疏 push 携带 `base_memory_revision -> revision` 且只能严格推进一步，事件允许为空；durable ack 携带 `base_durable_revision -> revision`，允许一次 flush 确认聚合区间但事件必须为空；reconnect 请求同时携带 `last_confirmed_seq` 与 `last_confirmed_memory_revision`，响应使用独立 DTO 返回基线、当前 memory/durable 前沿及恢复事件区间。live gap 只能触发单飞 reconnect，客户端游标/修订超前、generation 回退或服务进程 epoch 变化必须显式要求 hydration/rebase，并原子丢弃旧 volatile 状态，禁止把 reconnect 伪装成普通稀疏 patch。
57. root Session 的请求配置不能继续停留在旧运行态：当前 `CreateRootSessionV1Request` 不含 Provider/连接/模型/思考级别，`MetadataDocument::new` 令 `active_request` 全空，而首次 `prepare_and_start_api` 仅把实际配置写入 `ApiStarted`。冻结语义为 Fresh 根允许暂时没有 effective request；第一次真实 Provider prepare 必须从同一个已验证且将要派发的 `ApiRequestSnapshot` 在 Core 母事务内无展示地初始化 `metadata.active_request`，并与 runtime baseline、ApiStarted 一次发布。Initialized 后每个实际请求必须与 active 精确相等，pending 由唯一 owner 在投影前先原子提升；禁止由旧 `SessionRuntimeState` 二次构造或在 service 层补写 active。
58. 新 Schema 的严格性仍需在阶段 4/8 完整闭环：`ApiCompletion`/`ApiStatsEvent` 的 status、end reason、error 与 timing 必须共享提交期和加载期校验；`ToolCallEvent.request_stream_id` 必须改为非空强类型必填；`PersistentChangeSet` 必须在 `vulcan-change-set` 根层校验 change kind、行号顺序、hunk 实际行和 Full 内容统计，WebUI 加减行只能从持久化实际行派生；空 pending input 必须在分配 ID 前拒绝，空 promotion 必须真正 no-op；请求配置切换必须保存 previous/current 的强类型 UiOnly 事实，不能继续用泛化 `ModelSwitched` 掩盖 Provider、连接、思考级别或温度变化。
59. 现有 Provider durability barrier 能阻止网络提前分派，但其 unit ack 不能完成 V1 请求接管：`VulcanPublicContractAdapter` 在进入屏障前已经从旧 `ConversationSnapshot` 构造并持有 `AiExecutionRequest`，屏障后仍发送该原对象。唯一方案已经冻结为强类型 V1 preparation permit：屏障返回最终 `AiExecutionRequest`、后续工具结果回合所需 canonical controller config/snapshot、budget、同源 fingerprint/proof 与 V1 api_call_id；adapter 显式丢弃屏障前 candidate，Started telemetry、trace、wire dispatch 和 continuation 全部只消费 permit 对象。permit 在唯一 SessionHandle writer 边界内完成旧 dirty flush、pending 提升或 Fresh active 初始化、同 generation exact range、八类 baseline、V1 投影、Core staged start 与 durable flush；禁止把旧请求原样回传并只补一份 V1 证明。
60. 新 Session 的“完整目录先发布”必须是物理原子目录发布，而不是依次写最终目录：当前 writer 先在 final session directory 写 tail、runtime/context、metadata，任一步失败都会留下后续永远拒绝重试的半目录；SQLite 插入失败保留的完整未索引目录也无法由同一请求补写索引。修复必须在同父目录的唯一 staging directory 完整写入并验证全部文件，最后原子 rename 到 deterministic final directory；本次写失败只清理明确 staging，崩溃遗留 staging 不参与寻址且不扫描。final 已存在时仅允许同一个显式 create 请求精确加载并验证完整等价文件后幂等补 SQLite row；损坏或不等价直接报告具体路径，绝不修复、覆盖或兼容。测试覆盖每个文件失败点、rename 前后、SQLite 失败后显式重试及同 ID 并发。
61. native `update_goal(status=complete)` 发生在当前 Provider API 的工具执行阶段，不能套用“模式切换要求无 active API”的校验。Core 单一 failure-atomic `complete_goal` 必须允许在 UserInput 或 GoalContinuation 的活动 API 内完成目标：同次 staged 修改 Goal 为 Completed、写稳定 SystemContext Remove 与 UiOnly 完成事实，保持当前 API、当前 ToolCall 和开放 Turn 正常继续到 tool result、API terminal 与最终 assistant；仅取消尚未派发的未来 Goal continuation 权限。最终 assistant 关闭后 Completed 状态不得再产生下一 continuation，service 不得延迟排队形成第二事实源。
62. `RuntimeInitializationState` 当前加载与 generation 切换已透传，但主 writer 的 create/flush 两处通过 `..RuntimeDocument::new` 把任意内存状态重新写成 `FreshUninitialized`。所有 runtime.json 构造必须显式复制 `RuntimeLock.initialization_state`，并用 Fresh root、首次 prepare 后 Initialized、后续 runtime-only flush 与重启加载的端到端测试证明状态和八类 locked sources 精确往返；否则首次请求后重启会重复初始化或错误拒绝既有 baseline。
63. 对 Fresh Provider prepare 的脏文件报告已按当前源码否证：`prepare_and_start_api` 的局部 dirty 列表虽只显式列出 ActiveTail/Runtime，但统一 `bump_revision` 会无条件追加 `DirtyFile::Metadata`，且 Core 已有断言证明该事实。不得再次在子调用中重复添加 Metadata 或据此修改 writer；真正验收点是按真实 revision receipt 执行 flush/reload，证明 `metadata.active_request` 与 `runtime.initialization_state` 同一 durable revision 往返。
64. Core 的 `complete_goal` 已按活动 Provider 工具调用语义完成原子实现：Goal 在当前 API 内可转 Completed 并写稳定 Remove/UiOnly 事实，同时保持 active API、当前 ToolCall 与开放 Turn；最终 assistant 正常闭合后不得创建下一 GoalContinuation，尚未派发的 continuation 才可在无 active API 时同事务关闭。Core 定向测试当前 183/183 通过；阶段 6 仍需完成 native Goal 生产接线与 `VulcanGoalStore` 零命中，不能以领域测试替代生产切换。
65. 新建的 `ProviderApiTraceStore` 曾复用错误的 generation 身份：槽位、identity 和冲突规则以 generation 绑定 api_call_id，同 generation 的不同物理 API 会触发 `GroupIdentityChanged`，即使降为非致命 diagnostic 也没有修复身份模型。最终必须由每个真实 HTTP attempt 独立递增的 `trace_sequence` 定位，identity 同时包含 V1 api_call_id、attempt_index 与脱敏后 input/output 内容哈希，最近 10 槽按 sequence 轮转；相同 identity 只有在两份内容完全一致时才幂等。为防止“已预留并派发、但响应槽尚未发布”崩溃后复用 sequence，允许一个精确 `sequence.json` 在派发前可靠推进 frontier；重启只读取该固定 frontier 文件与固定 10 槽恢复最大 sequence，不使用 execution/context generation，也不扫描目录。当前实现的 sequence/api/attempt、双 bank pointer-last 与非致命方向已成立，尚需补齐 committed identity 的 input/output SHA-256 并完成生产接线。
66. V1 事件 envelope 与 metadata 配置已大范围拒绝未知字段，但 `runtime.json`/`compactions.json` 的 `LockedSource`、`RuntimeLock`、`CompactionNode`、顶层文档，以及 token/Turn/Subagent 嵌套统计结构仍会由 Serde 默认静默忽略未知字段。固定 schema_version=1 且不兼容旧开发格式时，这会把格式漂移伪装成成功加载；必须在 durable 类型定义母层统一 `deny_unknown_fields`，并用顶层与嵌套未知字段负向反序列化测试证明，禁止在 workspace loader 维护第二份字段白名单。
67. V1 主事件已经在 `UserInputEvent` 中保存有序 `ResourceId`，但当前唯一 Provider 投影把用户输入降为仅含 text 的 `ProjectedItem::UserText`，资源引用被完全丢弃。正式 permit 若沿用该结构将无法构造图片/文件输入，只能错误回读旧 `ConversationSnapshot`。投影必须保留 text + 有序 ResourceId，fingerprint 与 golden 同源包含资源引用；唯一 owner 再通过项目 catalog 解析为实际 Provider 输入。纯附件输入、过程内附件、排队附件、缺失资源替代与故意冲突的旧 snapshot 必须证明网络只消费 V1 投影。
68. 八类 runtime locked baseline 目前缺少可接生产的上游事实：`PromptSegment` 只有字符串 node_id/cache_key/title/source_label，没有强类型来源类别或跨请求稳定性分类，全库 `LockedSourceKind` 仍仅在测试出现。Session owner 若按名字或标签映射将违反零猜测原则。提示词组合器必须在每个片段的唯一生成源头输出穷尽 typed provenance/lifecycle，明确八类锁定来源与 Provider/Agent 基础、Goal/Plan、VMM 召回、环境、工具协议、请求级 Hook 等动态来源；permit 只做 enum 穷尽映射并保留精确 target/order/body/source identity，禁止字符串启发式分类。
69. 当前分叉辅助 `renumbered_fork_events` 只重排 seq、Provider source coverage 与 TurnStats 范围，没有重新分配父会话的 TurnId/PendingInputId/ApiCallId/ToolCallId，也没有修订 ApiStarted projection 的 retained/through/generation；它不能满足“新 session 连续本地 ID/seq 引用”。分叉必须由 Core 单一强类型 builder 按事实首次出现顺序穷尽重分配所有 session-local int64 并重映全部引用，项目 ResourceId 与 Provider/终端/子代理/request_stream 外部身份保持不变；service/store 只消费完整结果，禁止维护第二套字段映射。
70. 崩溃恢复的 API 终态写入与严格加载仍存在自相矛盾：`project_persisted_interruptions` 当前产生 `ApiCallStatus::Interrupted + ApiEndReason::Unknown + no error`，而 `validate_api_terminal_facts` 只接受 `Interrupted + UserInterrupted + no error`，导致真实崩溃状态成功写入后在重启严格加载时失败。必须在 Core 强类型终态模型统一“用户中止”“进程/崩溃中断”“错误中断”的 status/reason/error 组合，同一校验同时用于提交与加载；Store 不得改写，loader 不得放宽。测试必须覆盖 crash persist→flush→reload、用户中止、结构化错误中断和失败原子性。
71. 阶段 7 的 V1 编辑/分叉当前仍只有测试构件，没有生产调用：`publish_edit_generation` 与 `create_fork` 全库生产命中均为零，`service_submit_user_prompt_edit` 仍调用旧 `rollback_persisted_session_before_user_prompt`，`service_fork_conversation` 仍调用旧 `fork_persisted_session`。此外现有 V1 编辑由调用者直接传入 `runtime_lock`，分叉直接 clone 源锁定快照，均不能自行证明使用操作时的最新八类动态来源。生产接管必须由唯一 SessionHandle/lifecycle 在同一准入边界取得强类型最新来源：编辑发布新的 event/context generation，分叉消费 Core 完整重映射结果并以最新来源建立子会话；禁止在旧服务函数内双写或复制旧 runtime 快照。
72. Goal 旧事实链已完成一次物理清除：`goal_store.rs` 与 workspace-store 导出、service `summary_parse.rs`、runtime host-result→Goal cache 投影及旧 parser 测试已删除；生产静态搜索 `VulcanGoalStore`、`ThreadGoal*`、`GoalAccountingOutcome`、`goal_summary_from_tool_finished_event` 和旧 host-result 语义为零命中。新增 native/service 强类型 V1 get/create/block/complete、Completed 后新 generation，以及重启开放 GoalContinuation 并发只获一次 reservation/注册的测试。该证据完成 Goal 子链，不等于阶段 6 全部完成：pending request 的唯一物理提升和最终 assistant→下一 GoalContinuation 母事务仍依赖阶段 4/11 的 Provider/完成边界接管。
73. 当前 Store 层编辑/分叉辅助不仅尚未生产接线，还越过领域层直接制造事实：`publish_edit_generation` clone 后原位修改 `UserInputEvent`，保留旧 `event_revision`，并未推进 metadata 的 `document_revision/updated_at_ms`；`create_fork` 直接 clone 父 metadata，仅替换 session_id/title/generation，可能继承父 created/updated/archived/mode/config/goal 等与分叉目标不一致的状态。修复不能是在 Store 补几个字段，而必须由 Core 单一 edit/fork builder 从已验证源文档构建完整目标文档，重建 allocator、开放 Turn、模式/目标/配置、压缩与全部引用，定义新 generation 的事件版本语义和新 session 时间/归档语义，错误前后源文档不变；Store 只发布 Core 输出，不直接改事件或 clone 母 metadata。
74. 上下文压缩母事务目前仍是测试构件而非生产事实：Core `SessionDocument::record_compaction` 全库生产调用为零，生产 `maybe_auto_compact_webui_session_with_mode` 仍从旧 `ExecutionPreparationRequest.conversation_state`、旧 `session_runtime_state.context_start_message_id` 与 segmented session directory 构建和提交压缩。阶段 5/11 必须让摘要请求与正常请求一样经过 V1 projection permit；摘要成功后由唯一 SessionHandle 使用强类型 `CompactionCommitInput + 最新八类 locked_sources` 原子记录节点/runtime 指针/来源并可靠 flush，再生成后续请求。旧 runtime/segmented commit 必须删除，禁止并行双写。
75. Core 的请求配置与崩溃中断契约已闭环：统一 validator 同时覆盖 prepare/start、ApiStarted 重建和 metadata active/pending 加载；provider/model 必填且无首尾空白，optional Some 同样严格，temperature 必须 finite，非法输入在分配/修改前失败。崩溃投影新增强类型 `ApiEndReason::ProcessInterrupted` 并持久化完成时间，用户主动中止仍为 `UserInterrupted`，两者均通过 `from_parts` 严格重载。Core 当前 193/193、check 与生产 lib `clippy -D warnings` 通过；`--all-targets` 的既有测试 lint 基线留待阶段 13 统一收口，不得用它否定生产契约证据，也不得在 loader 放宽。
76. V1 SQLite/文件生命周期 facade 尚未接管生产：`archive_session_v1`、`restore_session_v1`、`delete_session_v1`、`rename_session_v1` 没有 service 调用；重命名仍进入 `rename_segmented_session_by_created_date`，归档仍进入 `archive_segmented_session_directory_for_locator`，恢复/删除仍使用旧 `restore_archived_session/delete_archived_session`。阶段 7/11 必须让 service lifecycle 在 registry owner 与精确 V1 locator 边界内调用新 facade，按文件与 SQLite 的冻结提交顺序实现 rename/archive/restore/delete 及“不存在”语义；不得保留旧 persist-version/global segmented 锁作为第二事实源，也不得在切换前双写。
77. V1 lifecycle facade 自身尚未维护 metadata 与 SQLite 的一致性：当前 rename/archive/restore 只更新索引 row，不更新 `metadata.json` 的 title、archived_at、updated_at 与 document revision；直接生产接线会让完整会话事实与侧栏目录永久分裂。实现前必须冻结最小跨资源提交协议和幂等重试语义，明确 metadata 是完整会话权威、SQLite 是精确定位/列表投影，并覆盖 metadata 写失败、SQLite 更新失败、两者之间进程崩溃及同请求重试；禁止 service 自行双写，也禁止因难以跨资源原子提交而删除其中一份冻结字段或静默容忍不一致。
78. 阶段 9 已在不提前切换生产的前提下完成：工具卡台账固化 `main..a5d94faf` 的 17 文件、+2939/-619 基线及逐 hunk 的 V1 输入/拒绝内容/目标测试；新增 V1 卡片与 resolver 只消费持久化 ToolCallEvent 的 input/output/ChangeSet/request_stream_id/external_refs，覆盖 file/file_edit/apply_patch/shell/markdown/Ask/动态子代理/terminal，不读取当前文件、不调用 detail RPC。ChangeSet 母层 validator 与全嵌套 `deny_unknown_fields` 已落地，真实增删行、UTF-8/LF、失败命令同组和目录无占位图标均有测试。当前 ChangeSet 12/12、service-api 7/7、Web V1 95/95、tsc、UI 架构检查与生产 build 通过；生产 transcript/detail 切换和旧消费者物理删除严格留到阶段 11/12 连续执行。
79. “继续”生产路径仍完全依赖旧重试旁路：Core/SessionHandle 的 `continue_turn` 生产调用为零，`service_continue_conversation_with_submission` 仍读取 `SessionRuntimeState.execution_retry_snapshot`、加载 `execution_retry_store` sidecar、校验 generation/ledger claim，并依赖旧 segmented directory 后调用 `service_retry_frozen_execution`。阶段 6/11 必须改为从 V1 handle 精确验证同一未结束 Turn、已中断 API 与无活动物理请求，用户可选补充仍写入该 Turn，再执行唯一 durable `continue_turn`；下一物理请求只由 V1 permit 投影。旧 retry snapshot/claim/ledger/sidecar 必须生产零命中并删除，测试覆盖用户中止、余额/网络/多次重试失败、无输入继续、补充输入、重复点击并发与重启。
80. 现有 Core 继续接口本身不足以替代旧 claim：`continue_turn` 只要求开放 Turn，不拒绝 active API、不验证最后物理 API 为允许继续的中断终态，重复调用还会重复追加控制事件；带补充输入时又需要组合三个各自提交的方法。必须新增单一 failure-atomic continuation transition，输入包含强类型外部 client action id 与可选 text/有序 ResourceId，在同一暂存提交内验证可继续终态、写入同 Turn 补充 UserInput 和 ContinueRequested/Resumed，并按 action id 实现同正文幂等、异正文冲突；容量/校验失败全文档相等。外部 action id 仅用于协议重试，不替代内部 int64；同时审计 root/queued submission 的 client_submission_id 是否需要同一最小持久化幂等契约。
81. 项目资源 admission 目前只覆盖新建根会话：`register_project_attachments_v1` 的唯一生产调用在 Created materialization，Existing 分支只加载 handle；现有会话新输入、queued、pending-in-turn 与 Continue 补充附件都未转换成项目 ResourceId，也未写入 V1 UserInput 资源事实。阶段 6/7/11 必须让每一种用户输入准入先按 metadata/index 的精确项目定位注册有序附件，再把 `Vec<ResourceId>` 交给对应 Core 单一领域入口；注册失败阻止事件发布，注册成功但会话事务失败允许留下永久未引用项目资源，不得回读旧 ConversationAttachment 或为每种输入复制 catalog 规则。
82. Core 的 `SystemContextEvent.ttl_turns` 已完成根层实现：TTL 仅由持久化 TurnBoundary 的唯一 TurnId 消费，同 Turn 多物理 API 不重复计数；`derive_active_system_state` 按 seq 重建 source/scope/remaining/last consumed，支持 Upsert 重置、Remove、压缩 backfill 与重启等价。TTL-bound Upsert 只通过当前活动前缀投影，过期不再注入；ttl=None 的 Goal/Plan 与显式 Remove 仍按 seq 重放。Turn scope 强制 ttl=1，Remove 禁止 ttl，非法 key/content/ttl 在提交与加载共用校验；旧 append_event 已收敛到统一 stage_event。Core 当前 196/196，生产 lib strict Clippy 通过。
83. 子代理回流仍未接入父 V1 主流：Core `append_subagent_return` 与 `append_subagent_return_stats` 没有生产调用，仅测试使用；因此真实父 TurnStats/Goal 预算、Provider 回流正文和 seq 顺序仍缺子代理事实。阶段 6/11 必须在子会话最终正文与统计已可靠落盘后，通过父 SessionHandle 原子写入 Provider 可见的 SubagentReturn 和 UiOnly SubagentReturnStats，再允许父物理请求继续；若两个现有方法无法保证一次提交，Core 必须提供组合入口。外部 child session/task identity 与 source seq 范围保持，重复同内容幂等、身份/正文冲突失败，并覆盖并发子代理、重启、统计聚合与父 Turn closure。
84. edit/fork 的模式与配置不能从截断后的 SystemContext 正文反解析，也不能直接 clone 当前 metadata。待用户最终确认的建议语义为：Edit 属于同 session，继承当前普通 selection/output/compaction 配置与当前 Goal/Plan，并在新尾部以强类型输入重新发布必要模式语义；Fork 继承当前普通配置和最新 runtime baseline，但清 pending/draft/queue，默认禁用 Goal/Plan，并在复制前缀仍有活动模式时追加明确 Remove，避免子会话自动续跑。该点确认前 Core 只冻结 builder 结构，不实现 mode/config 取舍；不得以“历史事件不足”在 Store 反解析正文或静默选择。
85. Continuation Core 接口已冻结为统一外部幂等契约：`ClientActionId` 强类型、非空、规范化、最大 256 bytes，只用于协议重试；`ContinuationRequest` 携带 action id、可选 text 与有序 ResourceId，`ContinueTurnReceipt` 返回原 Turn/可选输入 seq 与两个控制 seq。`continue_open_turn` 先按同一开放 Turn+action id重建，完全相同返回原 receipt，任一正文/资源漂移冲突；首次调用暂存可选 PendingInTurn UserInput 与 ContinueRequested/Resumed 后一次发布。资格按最近物理 ApiStats 的强类型 end reason 判断：允许用户/进程中断、网络、余额、重试耗尽及经现有策略确认可再次发起的 ProviderError，成功/取消/Unknown 禁止；重复同 action id 的幂等判定必须先于当前资格变化。根/排队输入后续复用同一 ClientActionId 持久化策略。
86. root 会话首次物理发布已按目录级母事务收口：`writer::create_session` 先在同一项目分区下创建唯一隐藏 staging 目录，完整写入 metadata/runtime/compactions/事件树并通过同一严格 loader 自检，随后一次 `fs::rename` 发布最终目录；任一步失败只清理本次精确 staging，不暴露半成品。SQLite 行严格在目录发布后写入；若索引写入失败，重试只能按精确 locator 调 `verify_existing_session_creation` 校验完整目录与原请求一致后补写行，冲突内容显式拒绝且不覆盖。该项已由当前源码和失败/重试测试结构核实，不再作为阶段 7 未完成项；最终仍需纳入全量故障矩阵与 Release 验收。
87. 子代理父级回流的前置事实尚未建立：`TaskRuntime` 与 `detached_subagent` 当前没有 child `SessionDocument`/`SessionHandle`/V1 registry 的可靠落盘 receipt，`store_task_outcome` 仍保存旧 task record/metadata，终态 payload 的 `source_turn_stats_record_id` 也是旧字符串记录 ID，无法诚实提供 child V1 的 `source_from_seq`、`source_to_seq` 与 `child_turn_stats_seq`。必须先让 child TaskSession 的 Provider/Turn 终态经 V1 owner flush，并让 `DelegateFinished` 携带强类型 V1 seq receipt；随后父 `append_subagent_return_with_stats` 才能一次 durable mutation 接入。禁止从旧 record id、文本或当前内存顺序反推 seq。
88. Continue 的外部 DTO/WebUI 仍暴露旧旁路权威：`ConversationContinuationSummary`、RPC 和前端 `activeContinuationKey` 继续使用 generation 与 `expected_ledger_revision` 进行可见性和成功对账。V1 原子 Continue 接入后，请求只能携带 conversation id、`ClientActionId` 与可选补充输入；可见状态必须投影为开放 Turn、最近中断终态与 continuation eligibility（如需要稳定展示键，应由 V1 事实产生），不得继续用旧 generation/ledger 字段校验、展示或伪装新链权威。阶段 11 必须同步删除前后端旧 token 分支与测试。
89. 当前 WebUI V1 构件尚无生产 React 接线：`createConversationEventWindowClient`、`ConversationEventWindowController`、`projectConversationEventDisplay` 与 `ConversationV1ToolCard` 均无生产调用；controller 只有 `snapshot()`，没有 subscribe/listener/`useSyncExternalStore`，live patch 即使进入 store 也不会触发界面更新。阶段 8/9 的测试通过只证明协议构件，不代表生产接管。阶段 11 前必须先完成可独立测试的 controller registry、外部 store 订阅桥与完整 V1 事件 renderer，再在单一切换点替换 `workbench.messages` 主链。
90. 旧前端生产主链仍为 `workbench`/`workbench.delta` → `useConversationHydration` → `service_session_messages`/`service_session_history_page` → `projectConversationRenderItems` → `parseConversationBlocks`/tool merge/group → `ToolExecutionBlock` → 旧 resolver/card；详情则经 `loadCachedConversationToolDetail` 与多层 props 调用 `service_session_tool_detail`，Rust 端仍做 message/record 双查询。阶段 11/12 必须按“V1 controller+renderer 接管 → 删除 detail props/RPC/cache/parser/旧 card”连续完成，不能只替换最末端卡片组件。
91. 子代理与审阅面板仍各自依赖旧消息事实：`WorkspaceContextPanel` 过滤 `workbench.messages` 并走旧 projection，`service_open_subagent_panel` 仍返回 `ConversationMessage[]` 回灌缓存；`WorkspaceReviewPanelHost::latestConversationDiffSource` 倒序扫描旧消息并以候选 metadata 路径猜 ChangeSet。生产切换时每个可见 child session 必须拥有独立 V1 controller，导航 RPC 不再携带消息；审阅来源只按 V1 tool_call 的 seq 选择最新 typed ChangeSet，禁止保留 parser 或路径猜测旁路。
92. 生成媒体是删除旧 detail 链前的阻塞契约：当前仍由 `useToolDetailMediaAttachments` 从 detail RPC 提取，V1 `external_refs` 的 Rust producer 尚未形成覆盖媒体/资源的统一 typed 契约，V1 card 也尚无媒体 renderer。必须先冻结并实现持久化 tool event 的 typed media/resource reference 及渲染测试；不允许因删除 detail RPC 静默丢失媒体能力。
93. 乐观用户输入、编辑和分叉 UI 仍以旧 `ConversationMessage/messageId` 为身份。阶段 11 必须改为 V1 `user_input` 的 seq/input_id，pending submission 仅作为 UI-only overlay，durable ACK 后由权威事件原子替换；不得保留旧 message cache 作为编辑/分叉第二事实源。
94. `ConversationLiveStreamStore` 不能整体删除：旧 appendMessageBody/message rebase/reconcile 部分应在 V1 接管后物理移除，但 `toolRequestProgressChanged` 的 0B 草稿、settled 防复活和 transient reasoning 的 live-only 通路仍是独立实时协议，应保留并通过 request_stream_id 与 V1 ToolCall 交接。切换清单必须按职责删除，不能按文件名机械清空。
95. 子代理统计的现有结构不足以满足父 Turn 账本：`SubagentInclusiveTotals` 只有 `api_call_count + AggregatedUsage`，旧 `state_cache` 因而把 subagent 的 `unknown_usage_api_call_count` 与 `tool_call_count` 固定为 0，会静默丢失真实 child 未知用量与工具数。V1 组合回流事件必须携带完整 `StatBucket`（或严格等价的全部字段），并从 child 已可靠落盘的最终 `TurnStats.buckets.total` 原样复制；父 direct/subagent/total 聚合只消费该强类型事实，禁止服务层从 usage segments 重算或补零。
96. 任何可幂等的 Core 母事务都必须显式返回 Changed/Idempotent，而不是让 service 通过 revision 差值猜测。`continue_open_turn` 与 `append_subagent_return_with_stats` 首次提交返回 Committed+receipt，完全相同的外部 action/child 事实返回 Idempotent+原 receipt 且不 bump、不 flush，内容或统计漂移返回显式冲突；service 据此映射 `SessionMutationChange`。否则幂等重试会被 `mutate_durable_if_changed` 误报 `MutationProducedNoRevision`，重现“请求已处理但 UI 无响应”的同类边界错误。
97. Continue 资格已形成强类型主事件契约：`ApiStatsEvent` 持久化 `ContinuationEligibility::{Allowed, Forbidden}`，Core 只校验并保存终态输入；agent-runtime 的 `provider_failure_continuation_eligibility` 在终态边界一次冻结策略。UserInterrupted、ProcessInterrupted、NetworkError、BalanceInsufficient、RetryExhausted 允许继续；Completed、Cancelled、Unknown 禁止；ProviderError 只有在没有语义输出、没有副作用且派发结果不是 Unknown 时允许。DTO 与 selector只读该持久事实，不从 reason/error 重算。该构件仍需随 Provider V1 permit、Continue RPC 与旧 sidecar 删除一起做生产验收，防止旧 `conversation_continuation` 继续成为第二资格源。
98. `ClientActionId` 的幂等返回要求不只适用于 Continue：当前 Core `submit_root_input`、`enqueue_input`、`add_pending_in_turn_input` 已能识别同 action id+同正文并直接返回原 TurnId/PendingInputId，但返回类型没有 Changed/Idempotent，现有 SessionHandle 又使用 `mutate_durable`，因此重复 RPC 会发生“无 revision 的成功”错误。三种输入入口必须与 Continue 统一返回显式 outcome，registry 通过 `mutate_durable_if_changed` 映射；异正文/资源冲突继续显式失败，测试覆盖首次、幂等重试、冲突与并发。
99. 父级子代理回流输入不得由 service 从 `DelegateFinished` JSON 手拼：Core 应提供纯领域 builder，从严格 child `SessionDocument`、最终 child TurnId、明确的回流正文来源以及可靠落盘前沿构造 `SubagentReturnInput`，验证唯一终态 TurnStats、source seq 范围全部属于该 child Turn、正文来源已 durable，并直接复制 `TurnStats.buckets.total`。如果 Core 文档本身不携带 durable frontier，调用方必须提供来自 child handle 的强类型 durable receipt/through-seq 证明；禁止默认全部内存事件可靠。
100. 子代理回流与 Provider 消费必须作为同一次生产切换：当前 `pending_dispatch` 仍把 `webui_session_mailbox_agent_messages_from_kernel` 注入下一请求；若父主流先追加 V1 SubagentReturn 而请求投影同时保留 mailbox，正文会进入 Provider 两次。接管顺序固定为 child V1 durable receipt → parent 组合回流 durable → exact Provider request 只读 V1 projection → 删除/停用 mailbox Provider 注入与旧 parent feedback → 允许父请求继续；端到端测试必须断言同一 child 回流在 wire request 中恰好一次。
101. 空 Provider 成功已在精确根点 `PublicTurnAccumulator::finish` 闭环：规范化 `replay_content()` 形成后、任何 replay/continuation 构造前会拒绝空集合；transient reasoning summary、孤立 provider message/context items 都不算持久化语义，reasoning、assistant text 或完整 tool call 任一真实存在才允许成功。当前测试覆盖完全空、transient-only、context-only，以及仅 reasoning、仅正文、仅工具三种有效最小输出；该项仍需在 Provider V1 permit 正式接管后执行端到端回归，确认 sink 不制造占位事件绕过根校验。
102. Provider trace 的并发正确性依赖唯一 session owner，而不只是 store 内部 Mutex：`ProviderApiTraceStore` 的 frontier/slot 锁仅在同一实例及其 clone 间共享；若生产每次请求或 adapter 独立 `new` 到相同 root，多个实例仍可读到同一 frontier 并预留重复 sequence。生产接线必须由 `SessionHandle` 或等价 per-session registry 持有唯一 trace owner，所有物理 attempt 复用该实例；正式环境只验证单进程 Release，不扩展 Debug/Release 共根问题。测试覆盖同 owner 并发 attempt 唯一连续，并以构造边界阻止同进程两个 owner 指向同一 session trace root。
103. 主控发现普通活动尾段 flush 仍存在跨文件半提交：writer 先原子替换 active tail，最后才写 metadata；若尾段成功而 metadata 写失败/进程崩溃，新根 UserInput 会与旧 `open_turn_start_seq=None` 冲突，pending/queued 晋升事件会与旧 metadata 中同一 ClientActionId 冲突，尾段新语义也没有可靠的 document revision 前沿。当前 loader 会分别在 `load_open_turn_resident_events` 和 `validate_client_action_history` 显式拒绝，说明“文件写成功后重启验证失败”仍可由正常崩溃窗口复现；现有故障测试未覆盖普通 tail→metadata。不得在 loader 放宽或猜测 Goal/Plan/config 状态。待用户确认的母层修复是：活动尾段每次仍只全量写一个几 MB 级不可变候选文件，`split_list` 的精确 file_name 由 metadata 最后一次原子切换；失败时旧 metadata 继续指向旧尾段，候选按确定命名在下一次重试覆盖/清理，成功后旧尾段仅作非权威清理。该方案替换“活动尾文件内容天然权威且 metadata 可语义落后”的旧假设，使 metadata 恢复为唯一发布点。

104. Core 的外部输入幂等契约已经形成并由主控独立复核全量 `196/196`：`ClientActionId`、`ClientInputSubmission`、`ContinuationRequest`、`IdempotentMutation::{Committed, Idempotent}` 已覆盖 root、queued、pending-in-turn 与 Continue；API 终态持久化 `ContinuationEligibility`，重复相同 action id 在资格漂移后仍优先返回原 receipt，正文或资源漂移显式冲突。该结果只证明领域层成立；service、RPC、WebUI 和物理 Provider 续发尚未完成生产接管。
105. root V1 生命周期窄接口已经改为接收 `ClientInputSubmission`，`CreateRootSessionV1Request` 不再只携带正文和资源；registry 也已提供从 `IdempotentMutation` 到 Changed/Unchanged 的映射。生产提交入口仍必须把真实 `client_submission_id` 构造成 `ClientActionId` 原样传入，禁止测试式派生 ID、随机替代 ID 或 revision 猜测；root/queued/pending/Continue 的所有调用点与夹具需在阶段 6/7 统一清零旧签名。
106. Continue 生产接管不能分成“V1 admission 成功”和“旧 retry sidecar 物理续发”两套权威：当前 service 仍依赖 generation、expected ledger、claim/sidecar 和 `service_retry_frozen_execution`，而 V1 尚无完整的 durable Continue receipt→唯一 Provider permit 调用时序。正式接管必须固定为附件资源准入→同一 SessionHandle 幂等 Continue durable mutation→唯一 reservation/prepare-and-start permit→transport；只有能够保证不会出现“ACK 成功但没有续发”且同 action id 不二次派发后，才能删除旧 retry 链并切换 RPC。
107. Provider permit 的 Core 强类型事实已经落地，但生产装配仍未接通：`LockedBaselineProjection::project(Vec<LockedBaselineSegmentInput>)` 是唯一公开 composer 边界，按显式 kind/target/node/source/content 生成有序锁定来源；`PrepareAndStartApiInput` 与 compaction 都只接该 wrapper。`ProjectedToolCall/ToolResult` 也已保留 session-local `ToolCallId` 作为内部配对权威，Provider 原生 ID 只作可选关联。剩余阻塞是唯一 Prompt 生产装配器必须真正产出八类 provenance 并让 transport 只消费同一 V1 projection；不得回退旧 `ConversationSnapshot`、从标题/node 名猜分类，或按 Provider ID/顺序/文本配对工具。
108. Continue 的 action→API→wire observation Core 链已经落地：`PrepareAndStartApiInput.continuation_action_id` 与 `ApiStartedEvent` 记录 Continue 身份，同一开放 Turn 的同一 action 幂等返回原 `PreparedApiStart`；`mark_api_transport_observed` 追加 HostOnly `ApiTransportObserved` 并幂等返回 receipt。未观察的 prepared Continue 重启后不伪装进程中断，已观察的 in-flight Continue 才按正常崩溃中断处理。剩余生产要求不变：Hubble 的唯一 permit owner必须把 wire send 后的 observation 可靠落盘，Continue RPC 成功 ACK 至少等待该 receipt；普通新请求不伪造 action id，transport 类型不进入 Core，重复 action 不二次物理派发。
109. queued/pending 路径当前仍会丢失浏览器幂等身份：`queue_existing_submission` 虽持有 `client_submission_id`，旧 `enqueue_webui_pending_prompt_from_submission` 却不接收该字段并自生成 UI entry id；Core 虽已有 `enqueue_input/add_pending_in_turn_input(ClientInputSubmission)`，registry 尚缺对应 durable wrapper。阶段 6/7 必须补两个 `mutate_durable_if_changed` 窄入口，把 Committed/Idempotent 精确映射为 Changed/Unchanged，再让生产 queue/pending 服务原样传入浏览器 action id；旧 UI 队列只可保留未 ACK 的瞬态 overlay，不得继续持久化第二份输入事实。
110. 删除旧 tool-detail 媒体提取前必须补 V1 生成资源契约，且不能把导航专用 `ExternalResourceRef` 混作文件资源。ToolCall 主事件应持有有序 `GeneratedResourceRef { resource_id, kind, preview_resource_id }`：session 只保存项目级数字 ResourceId 与显示类别，不复制文件名、MIME、大小或路径；精确 `generated_attachments` producer Schema 在 ToolCompletion 前把主文件及可选预览文件按顺序注册进项目 ResourceCatalog，再一次 durable mutation 写入引用。项目资源解析 API 只按项目精确目录和 ResourceId 读取 catalog，不扫描；存在时返回目录中的名称/MIME/大小/规范实体路径，缺失时返回固定 placeholder。禁止解析 model output 字符串、复用旧 detail metadata fallback 或静默丢失媒体。
111. WebUI V1 独立 React 构件已形成并由主控复核定向 `5 files / 48 tests` 全绿：controller 具有稳定 snapshot cache 与 subscribe/unsubscribe，只在 hydration、live/reconnect/rebase 完整边界提交后通知；`useSyncExternalStore` 桥复用同一权威快照；独立 renderer 先验证完整 seq 再过滤 HostOnly，工具只走 `ConversationV1ToolCard`，没有导入旧 message/parser/detail/resolver。该进展仍不是生产切换：WorkbenchApp/ChatArea/子代理面板/审阅面板尚未建立 controller owner registry，typed generated resource 也未接入，因此阶段 11 前禁止删除旧渲染链。
112. Ask 的 Core 本地身份 seam 已落地，生产接管仍未完成：ToolBatchPrepared 返回幂等 `PreparedToolBatchReceipt`，其中每个有序 `PreparedToolReceipt` 携带精确 `api_call_id + tool_call_id + request_stream_id + tool_seq`；registry 已有 mark WaitingUser、answer、cancel durable wrappers，`VulcanToolInvocationContext` 类型也开始加入本地 api/tool 身份。但 agent-runtime 执行器仍需等待 batch ACK 后按 request_stream_id 注入这些字段，QuestionRuntime 仍需改为注册 waiter 后先 durable WaitingUser 再发布 UI，reply/reject 先 durable answer/cancel 再唤醒 future。禁止 Provider ID/工具名/顺序反查；旧 pending_questions 只能在生产 V1 投影接管后删除。
113. queued 输入的 V1 准入顺序必须整体前移，不能在旧 enqueue 旁补写：当前 `service_start_conversation_execution` 在遇到阻塞系统工作时先调用 `queue_existing_submission` 并立即返回，而 existing-session 的 `materialize_submission_session_v1`/精确 V1 handle 获取发生在其后，因此旧 queued 路径连 V1 owner 都尚未取得。根因修复顺序固定为：解析 existing session → 通过 SQLite 精确 locator 加载并注册唯一 `SessionHandle` → 执行阻塞判定 → 单次 `enqueue_input(ClientInputSubmission)` durable mutation → 旧 Hook 队列仅保留必要的进程内通知/调度适配且禁止再持久化 `runtime_state`/`pending_prompts`。Provider/Turn 完成后只由 V1 `schedule_next_queued_input` 晋升下一输入；禁止旧队列与 V1 同时成为持久化权威。
114. Hubble 已把当前 kernel lib 的旧 `start_api` 编译错误清零，并在 registry 增加 `prepare_and_start_api_durable`、`enqueue_input`、`add_pending_in_turn_input`、`answer_waiting_tool`、`cancel_waiting_tool` 窄入口；agent-runtime 与 service-kernel lib check 一度零错误。但这不代表 Provider 已接管：`provider_v1_sink` 在缺少 exact projection proof 与 typed locked baseline 时会结构化返回 `RequestPreparationNotCommitted`，真实物理请求仍会被阻断；Ask 的 `VulcanToolInvocationContext` 也仍缺本地 `ApiCallId/ToolCallId`。这些入口只有在定向幂等/失败原子测试、同源 Provider permit、调用上下文本地身份和生产调用图全部接通后才能从“构件完成”升级为“生产完成”。
115. Core 门禁必须按共享树当前状态判定，不能继续引用早先的 `196/196` 或中途 `199/199` 作为最终证据：continuation action → `ApiStarted` → transport observation 契约正在并行落地，当前曾出现缺失 `validate_api_continuation_binding_shape`、`validate_api_dispatch_history`、测试夹具漏 `continuation_action_id`、`record_api_transport_observed` 借用冲突以及 strict Clippy `collapsible_if`。这些属于同一上游暂态，不允许 service/store/UI 越界补桩；Einstein 报告全量 test 与 strict lib Clippy 同时通过后，主控必须独立复跑，才可更新阶段 4/6 完成状态。
116. Provider prepare 的 registry wrapper 必须跟随 Core 的 action 幂等语义升级：当前共享树中 `SessionDocument::prepare_and_start_api` 已返回 `IdempotentMutation<PreparedApiStart>`，但 `SessionHandle::prepare_and_start_api_durable` 仍声明普通 `SessionMutationReceipt<PreparedApiStart>` 并调用 `mutate_durable`。最终接口必须使用 `mutate_durable_if_changed` 与 Core outcome 映射为 `SessionConditionalMutationReceipt<PreparedApiStart>`；Committed 只发布一次 volatile edge 并可靠 flush，Idempotent 返回原 action→API binding、不 bump、不重新发 edge，但仍保证既有 dirty revision 已 durable。重复相同 Continue action 不得二次创建 ApiStarted、二次 reservation 或二次物理派发；请求/投影/Turn 漂移必须显式冲突，禁止把 Core 降回非幂等以适配旧 wrapper。
117. Ask 的本地工具身份通过 ToolBatchPrepared 屏障回执贯穿，而不是新增查询：Core 在同一次 durable batch mutation 中返回每个非空 `request_stream_id` 对应的本地 `ApiCallId + ToolCallId`；同一 batch 内 stream id 必须唯一，精确重试返回原映射，参数或映射漂移显式失败。service-kernel 只把该强类型批次 receipt 作为现有准备屏障 ACK 载荷返回；agent-runtime 的 `StreamingRuntimeToolExecutor` 等待 ACK 后按 `request_stream_id` 把本地身份注入每个工具请求，并原样构造 `VulcanToolInvocationContext`。普通 ToolStarted/Finished 屏障不承担反查；QuestionRuntime、service 和 WebUI 均不得通过 Provider tool-call id、工具名或顺序查找本地工具。
118. Continue selector 的独立 V1 构件已按 `open_turn_id + 最后物理 ApiStarted + 对应终态 ApiStats.continuation_eligibility` 实现，UI identity 仅是 `[session_id, window_generation, open_turn_id, api_stats_seq, event_revision]` 的本地元组，不进入 RPC。相邻 ApiStats 枚举漂移也已闭环：WebUI `end_reason` 精确加入 Core 的 `process_interrupted`，Rust→JSON→TS golden 使用真实 `ApiEndReason::ProcessInterrupted`；相同 reason 的 allowed/forbidden 两组测试证明 selector 只读持久化 eligibility，不从原因重算，也不把未知值兜底成 `unknown`。当前验证为 Web V1 定向 5 文件 87 项、service-api golden 9 项、tsc/UI architecture/production build 与 service-api check 通过；这些仍只证明切换前构件，不代表 ChatArea/RPC/Continue 生产接管。
119. Turn 完成与 queued/Goal 后继的 Core 母事务已落地：`complete_turn_and_select_successor` 在一次 staged revision 内按 queued user input → Goal continuation → none 选择唯一后继，queued 分支复用原 ClientActionId/PendingInputId、按 FIFO 追加 `QueuedScheduled` UserInput 并建立 UserRoot Turn；`TurnCompletionTransition` 强类型返回 `TurnSuccessor::{None, QueuedUser, GoalContinuation}`。主控复跑后的 Core 为 203/203 且严格 Clippy 通过。生产切换仍未完成：最终 assistant/API 可靠落盘的唯一 Hubble 边界必须调用 `complete_successful_turn` 并把该 transition receipt 交给 chaining；旧 mailbox/PendingPrompt/Goal 二次选择及独立 `schedule_next_queued_input` 生产入口必须在接管后物理清除，禁止在旧 terminal persistence 或 finished UI hook 里补第二次调度。
120. Core 当前即时门禁已由主控在 queued-successor 母事务进入共享树后再次独立复跑并全部通过：`cargo check -p vulcan-session-core --lib` 通过，`cargo test -p vulcan-session-core -- --test-threads=1` 为 203/203，`cargo clippy -p vulcan-session-core --lib -- -D warnings` 返回 `No issues found`。这只证明当前 Core 生产库与领域测试基线已稳定，阶段 4/6 仍不得标记完成；ToolBatch identity、Provider permit 与完成 transition 的生产调用方尚未收口，相关接线完成后必须再次执行全量 test/check/clippy。
121. Provider locked baseline 的 Core 输入已强类型化，但生产 Prompt 装配仍没有可用 provenance：全库 `LockedBaselineSegmentInput`/`LockedBaselineProjection` 的非测试调用目前只出现在 Core 与 service-api fixture；`PromptSegment` 只有通用 `PromptNodeKind`、target、node_id、cache_key、title、body、source_label，既不能区分 MemoryProfile/AgentsMd/VulcanMd/AgentPersona/OutputStyle/PromptSkillInjection/PluginInstructions/BaselineHooks 八类，也没有独立 source_id。阶段 4 必须由唯一 Prompt 生产装配器在来源形成处补齐强类型 baseline category 与 stable source identity，再直接投影为 `LockedBaselineProjection`；禁止在 Provider sink 根据 title、node_id、source_label 或 cache_key 推断，禁止把旧 `ConversationSnapshot` 当 fallback。
122. Ask 的 V1 主身份已在 runtime 层改为精确 `session_id + ApiCallId + ToolCallId`，Provider 原生 id 仅作可选关联；但 durable answer/cancel 后，当前 `provider_v1_sink::finish_tool` 仍会按旧 Running-only 路径二次终结同一工具。唯一 ToolFinished 边界必须读取同一本地 identity：Running 时写终态；WaitingUser 已由 durable user action 以相同结果完成/取消时只做幂等确认并推进 API；正文、状态或身份漂移显式冲突。不得用 Provider id 回查，不得吞掉二次终结错误。
123. 子代理父级回流仍有两个根层缺口：正文 `append_subagent_return` 与统计 `append_subagent_return_stats` 是两次独立 mutation，任一中间失败都会形成半回流；`SubagentInclusiveTotals` 还只保存 `api_call_count + usage`，会丢失 unknown-usage count、tool count 与下层 return count。必须由严格 child V1 文档构造正文范围与唯一终态 TurnStats receipt，并在 parent 一次 staged、可幂等 mutation 中追加正文和完整 `StatBucket` 统计；service 不得从旧 record id/usage_segments 猜 child seq 或重算统计。
124. workspace-store 当前生产库 `cargo check -p vulcan-workspace-store --lib` 已通过，但全量 lib test 在最新 Core 强类型契约下仍有 44 个夹具编译错误：旧 `submit_root_input(text, resources, now)`、裸 `Vec<LockedSource>`、直接读取 `IdempotentMutation` 内值、缺少 continuation action/eligibility 等。它们必须机械迁移到 `ClientInputSubmission`、`LockedBaselineProjection` 与显式 Committed/Idempotent 结果；禁止为使旧测试通过而恢复任何生产兼容重载。阶段 7 门禁只能使用迁移后全量测试与严格 Clippy 的新结果。
125. 动态压缩窗口配置已经进入 V1 metadata，但压缩节点发布尚未接生产：当前 `SessionDocument::commit_compaction`/`CompactionCommitInput` 仅 Core/Store 测试使用，service-kernel 与 agent-runtime 没有生产调用。阶段 5 必须在唯一 Compaction Provider API 成功、总结正文与 ApiStats 已可靠落盘后，用同一请求的 summary seq、retained_from_seq、完整 typed locked baseline 调用该母事务并 flush context generation；旧 compaction runtime/state 只能在接管后删除，不能把“窗口节点可切换”误报为压缩持久化已完成。
126. V1 SQLite facade 当前仍把索引误当权威：`archive_session_v1` 与 `rename_session_v1` 只更新 SQLite；`restore_session_v1` 只验证 metadata 文件存在后清 SQLite archived 状态，均未先修改/flush `MetadataDocument.title/archived_at_ms`。这违反“metadata 权威、SQLite 只投影”的冻结契约，也会在重启后出现行与完整会话事实分裂。阶段 7 必须通过每 session 生命周期互斥门执行 Core metadata 幂等 mutation → V1 flush → SQLite 精确投影；metadata 成功而 row 失败只返回 ProjectionPending 并允许按精确 locator 重试补投影，绝不反向回滚可靠 metadata、绝不目录扫描。delete 保持目录先删、row 后删的可重试顺序。
127. Compaction 的 Core 母事务已经具备足够严格的 `CompactionCommitInput`：它会校验 `api_call_id` 对应 Compaction purpose、终态 ApiStats 为 Completed、`occurred_at_seq` 是该 API 最后一条已完成 AssistantText/Reasoning、summary 正文完全一致，并在一次 revision 中发布 compactions 与 runtime locked baseline。真实缺口位于上游事实保留：`ProviderV1ApiTerminalFacts` 没有 summary seq、retained_from_seq 和 typed locked baseline；sink 的 active 状态只保留 summary 字符串，没有保存 `update_assistant_text` 返回的精确 `Seq`。阶段 4/5 必须由唯一 Provider 流边界产出强类型 `CompactionTerminalCommitFacts { summary_seq, summary, retained_from_seq, locked_baseline }`，Compaction success 先可靠终结物理 API，再以该 receipt 调用 Core compaction 母事务；普通 purpose 才进入 Turn completion。禁止按正文搜索 summary seq、从旧 runtime 推断 retained boundary 或重读最新提示词拼 baseline。
128. WebUI V1 目前只完成 selected-session controller owner、外部 store、连接恢复和独立 V1 renderer/card 的构件与测试，生产 transcript 尚未切换：`ChatArea`、`ConversationWorkspace`、`WorkspaceContextPanel` 仍以 `workbench.messages`、旧 renderItems/parser/grouping/detail RPC 为权威。阶段 11 必须在项目级 ResourceId 展示解析、Continue 新 DTO/receipt 和稳定 `prependOlder` 命令面就绪后一次切换主/子会话渲染，随后物理删除旧 transcript/detail 链；在此之前不得并排挂载两套 renderer，也不得把 controller 已接 owner 误报为生产显示接管。
129. Turn completion 的 Core 已原子选择唯一 successor，但生产完成链尚未消费该 receipt：`finished.rs` 仍在旧 `terminal_state_persisted` 后调用 `maybe_start_webui_turn_chain_after_finished`，后者继续按 mailbox → 旧 PendingPrompt → Goal 三次探测。阶段 4/6 的唯一完成母边界必须在 final assistant/普通 Provider success 后调用 `complete_successful_turn`、可靠 flush，并把完整 `SessionMutationReceipt<TurnCompletionTransition>` 传给 chaining。GoalContinuation 只能按 receipt 的 turn_id/context_seq 构造一次进程内 admission；QueuedUser 必须按 input_seq 从同一 V1 handle 读取已调度 UserInput，并逐项核验 turn_id/input_id/client_action_id。不得回查 pending_prompts、mailbox 或 Goal 再选后继，旧 ui_state 只可保留进程内排他而不得成为 durable authority。
130. 当前 writer 的普通尾段 flush 仍存在发布协议级崩溃窗口：`flush_session_with_limits` 在 metadata 指向 `sNNNNNNN.jsonl` 时，先原子覆盖该同名活动尾段，只有 metadata dirty 时才随后更新 metadata；进程若在两者之间退出，会留下“新尾段 + 旧 metadata”。严格 loader 又要求所有 split entry 都使用该 segment number 的 canonical 文件名，因此不能通过现有 `file_name` 临时指针回退。根修复候选是每个 generation 使用两个固定 active bank（如 `active-a.jsonl`/`active-b.jsonl`）：每次写未被 metadata 引用的 bank，校验后仅原子切换最后 open entry 的 file_name；封口时把完整尾段写到从未发布过的 canonical `sNNNNNNN.jsonl`，创建下一 active bank，再一次发布 metadata。该方案会修改新格式的 split-map/loader/writer 契约，属于待用户确认的重大架构决定；确认前只保留证据和设计，不得在 Store 子层补日志、回滚或自动修复。
131. Provider API 调试 trace 仍存在两套生产/新实现并存但未接管的事实：agent-runtime 的 `persist_provider_api_trace_snapshot` 仍直接调用 workspace-store 旧 `persist_provider_api_trace_attempt`，使用 group_sequence/group_id/attempt 目录和全局写锁；新 `ProviderApiTraceStore` 已实现每 session owner、连续 `trace_sequence`、十槽轮转、A/B payload/pointer、延迟完成防覆盖和非致命诊断，但当前只有自身测试与导出，没有生产调用。阶段 7 必须在唯一物理 API attempt 边界把 reserve→publish 接到新 store，trace failure 只记录诊断、不影响主会话；随后物理删除旧 group identity、旧 free function、旧目录准备/测试与调用。禁止双写，也禁止用执行 generation/group id 代替单调 trace_sequence。
132. 最新共享树的生产库已重新达到可编译状态：`cargo check -p vulcan-service-kernel --lib` 与 `cargo check -p vulcan-agent-runtime --lib` 均为 0 error；但 kernel 的 dead-code 输出同时证明 `prepare_and_start_api_durable`、`complete_successful_turn`、enqueue/pending/Ask wrappers 等仍无生产调用。该结果只能作为“接口迁移未继续破坏编译”的证据，不能作为阶段 4/6 接管完成证据。Core 子代理组合回流迁移期间全量测试仍有旧拆分 API 夹具编译错误，必须在新单一原子接口测试全绿后重新执行跨层门禁。
133. `cargo check -p vulcan-service-kernel --tests` 的最新真实基线为 54 errors/2 warnings，均是测试目标仍构造旧契约：旧三参数 `submit_root_input`、裸 `Vec` runtime baseline、已删除 `start_api`、可选 String request_stream_id、旧 `QuestionRuntime::ask` Provider-id 主键、旧 prepare_tool 参数与已删除 sink 错误变体。生产 lib 已 0 error，因此这些不能解释成生产实现失败，也不能通过恢复兼容重载消除；必须按最终 ClientInputSubmission、LockedBaselineProjection、prepare-and-start receipt、本地 ApiCallId/ToolCallId/RequestStreamId 机械迁移所有夹具，再把 `--tests` 作为阶段 4/6 的真实门禁。当前完整日志为 `~/AppData/Local/rtk/tee/1787789631_cargo_check.log`。
134. Beauvoir 已完成其所有权内的 Ask、Goal、Plan 与 kernel 测试夹具机械迁移；主控在相同共享树重新执行 `cargo check -p vulcan-service-kernel --tests` 后得到 48 errors/0 warnings，且诊断全部集中在 Hubble 已拥有的 `provider_v1_sink.rs`、`conversation_checkpoint.rs`、`conversation_events_v1.rs`、`session_runtime_registry_v1.rs`、`session_v1_lifecycle.rs` 与 `execution_retry_tests.rs`。剩余错误只包含最终强类型接口后的旧测试构造：`ClientInputSubmission`、`LockedBaselineProjection`、已删除 `start_api`、`RequestStreamId`、`prepare_tool` 参数和已删除 sink 错误变体；不得恢复旧生产重载或占位 proof。最新完整日志为 `C:\Users\lingeasy\AppData\Local\rtk\tee\1787789884_cargo_check.log`。
135. WebUI selected-session owner 的稳定分页命令面已经完成：`prependOlder()` 在 controller 母层单飞，失败不修改 snapshot 且可精确重试，dispose 后迟到响应不得提交；registry command object 固定绑定创建时的 selection token 与 controller，session 切换、draft/null、dispose 后旧闭包只会显式失效，绝不重定向到新 owner。React external store 已暴露同一 owner-bound command；扩展 V1 定向 6 文件 97 项、TypeScript 检查、UI architecture 与 production build 均通过。该结果只完成 Stage11-A2 构件，不代表 ChatArea 已切换。
136. 子代理正文与完整统计的 Core 母事务已最终收口并由主控/所有者双重验证：`source_assistant_text_seq`、强类型 `SubagentTaskId`、child source range、`child_turn_stats_seq` 与完整 `StatBucket` 都是 durable facts；child builder 只接受 memory/durable revision 相等、唯一终态 TurnStats、显式最新 Completed API 的 Completed AssistantText 且无 ToolCall；parent 以一个 staged mutation 相邻写入 Provider 正文与 UiOnly Final 统计，并返回显式 Committed/Idempotent。loader 拒绝缺身份、错误 visibility、非 Final、范围或统计漂移；旧两个分步 public append 入口已删除。最新 Core 全量门禁为 206/206、`cargo check` 通过、strict lib Clippy `No issues found`。生产 child lifecycle、parent DelegateFinished 接线与 mailbox 注入删除仍未完成，因此阶段 4/6 不得据此提前结束。
137. 主控复跑 `cargo check -p vulcan-agent-runtime --tests` 已达到 0 error，仅余 `tool_runtime_bridge/tests.rs::install_test_invocation_identities` 一个未使用测试 helper 警告；这证明 agent-runtime 的最终 Core/Ask 强类型夹具已基本完成迁移，但严格门禁仍要求删除或真实复用该 helper，禁止使用 `allow(dead_code)`。与之相对，`workspace-store --tests` 仍为 44 errors/0 warnings，且全部集中在 `session_v1/tests.rs` 的旧 `submit_root_input`、裸 baseline、旧 continue 与未解包 `IdempotentMutation` 测试构造；最新日志为 `C:\Users\lingeasy\AppData\Local\rtk\tee\1787790406_cargo_check.log`。
138. Workspace Review 的 V1 纯选择器已经完成但尚未接生产：它只从连续 seq 的 `ConversationEventWindowControllerSnapshot.events` 读取最新 typed `ToolCallEvent.changeset`，返回 owner generation、seq、event_revision、turn/tool 身份与原始 PersistentChangeSet；无 ChangeSet 返回空，乱序或非连续窗口显式失败，禁止排序、倒序扫描 `workbench.messages`、解析旧 block、猜路径或调用 detail RPC。定向 7 项、TypeScript、UI architecture、production build 与 whitespace 检查均通过；`WorkspaceReviewPanelHost` 仍在调用旧 `latestConversationDiffSource`，因此阶段 11 接管状态保持未完成。
139. Ask durable answer/cancel 后的二次 ToolFinished 已从 Core 母层消除：`SessionDocument::finish_tool` 现在返回 `IdempotentMutation<ToolCompletionReceipt>`；Running 首次调用在同一 seq 原位提交，已为 Completed/Failed/Cancelled/Stopped/Interrupted 时仅在 status、canonical output、ChangeSet 与 external refs 完全一致时返回原 receipt 且不 bump，调用方未来确认时间不会覆盖可靠 `finished_at_ms`；WaitingUser/Prepared 仍拒绝，任一语义漂移显式失败。Core 最新全量门禁为 209/209、strict lib Clippy `No issues found`。service-kernel 仍需把 `SessionHandle::finish_tool` 改为 conditional mutation 并由 sink 消费显式 changed/idempotent outcome；接线前不得在服务层读 revision、比对事件或吞掉重复终态错误。
140. Hubble 已把 `SessionHandle::finish_tool` 与 Provider sink 改为消费 Core 的显式 `IdempotentMutation<ToolCompletionReceipt>`，并完成其所有权范围内 48 个 service-kernel 测试夹具迁移；Beauvoir 随后收口 Queue V1 准入字段漂移。主控独立执行 `cargo check -p vulcan-service-kernel --tests` 已达到 0 error，执行 `cargo test -p vulcan-service-kernel queue --lib -- --test-threads=1` 为 18/18。当前仍有 9 组 dead-code warning，集中在尚未成为生产唯一入口的新 V1 wrapper 与待阶段 11/12 删除的旧显示、队列和 Goal guidance 辅助函数；这些 warning 只能由正式接管或物理删除消除，禁止 `allow` 压制，因此阶段 4/6 仍未满足退出门。
141. workspace-store 的 44 个旧 Core 测试夹具已完成第一轮最终强类型迁移，`session_v1/tests.rs` 不再产生旧 `submit_root_input`、裸 baseline、旧 continuation 或未解包 `IdempotentMutation` 错误；最新 `cargo check -p vulcan-workspace-store --tests` 仅被 Core 并行审计期间 `document.rs` 两处暂缺 `is_terminal_tool_status` 阻断。该阻塞必须由 Core 母层收口后再复跑 Store 全量 test/check/strict Clippy，不允许恢复 Store 兼容逻辑，也不得沿用迁移前 371 项结果冒充最终门禁。
142. WebUI 的 V1 user-input optimistic overlay 纯构件已经完成：它以强类型 `BrowserClientActionId` 和精确 `session_id + window_generation` 为 owner，只在同 owner、同 action、同正文的权威 V1 `user_input` 到达时结算；RPC ACK 仅标记 acknowledged，RPC 失败移除，事件先于 ACK、重复提交、代际切换、旧闭包与 dispose 均有显式幂等/冲突/失效语义。定向 10 项、TypeScript、UI architecture、production build 与 whitespace 检查均通过。生产接线前必须把共享 TS `ConversationEventBody.user_input.client_action_id` 补为必填并更新 Rust→JSON→TS golden；禁止 optional/default/fallback。该构件尚未接 ChatArea 或旧 `pendingSubmission`，因此阶段 11 状态保持未完成。
143. Core 公共 durable mutation 的本轮受限审计已经收口，主控独立复跑 `cargo test -p vulcan-session-core -- --test-threads=1` 为 211/211，`cargo check -p vulcan-session-core --lib` 通过，strict lib Clippy 返回 `No issues found`。本轮新增的 `is_terminal_tool_status` 母层 helper 统一了 WaitingUser answer/cancel 与普通 terminal confirmation 的幂等路径；审计同时发现 service-kernel `provider_v1_sink::start_prepared_tool` 仍把 Core 的 `false/no revision` 结果交给普通 `mutate_durable`，精确 BatchStarted/ACK 重放会触发 `MutationProducedNoRevision`。该直接调用方必须改成 conditional receipt 并补 replay 测试，不能据此把阶段 4 标记完成。
144. Queue V1 生产接线的主控审核发现一条第二权威泄漏：虽然 `queue_existing_submission` 已先可靠提交 `metadata.input_state.queued_inputs` 且不再在该函数内直接启动旧持久化任务，但兼容显示仍调用 `enqueue_webui_pending_prompt_from_submission`，把同一 action 写入可序列化的 `WorkspaceSnapshot.session_runtime_state.ui_state.pending_prompt_panel_queue`；任何后续旧 `spawn_latest_session_runtime_state_persist_task` 都会投影并写回整条 `SessionRuntimeStateEntry`，因此“process-local only”注释不成立。修复必须把兼容 UI/Hook 镜像移入不可序列化的 kernel 进程内 owner或直接改由 V1 投影驱动，禁止在 Store serializer/projector 增加 skip/filter 补丁；必须用“V1 提交后触发任一旧 runtime-state persist，state.json 不含新 action”的回归测试证明没有双 durable authority。
145. 共享 TS `user_input.client_action_id` 已完成强类型闭环：`conversation-event-window-store.ts` 现在导出唯一 branded `ClientActionId` 与严格 factory，按 Core 规则校验非空、无首尾空白及最多 256 UTF-8 bytes；`ConversationEventBody.user_input.client_action_id` 为必填，optimistic overlay 删除了平行品牌与局部补字段，所有 controller/store/renderer/overlay fixture 均显式构造。4 组精确 Vitest 共 85 项、TypeScript、UI architecture、production build 与 Rust→JSON→TS golden 均通过，`BrowserClientActionId`、optional/default/fallback 搜索归零。该闭环仍属于阶段 11 前置构件，尚未接入 ChatArea 或替换旧 `pendingSubmission`。
146. Queue 的进程内兼容镜像已经开始从可序列化 `SessionRuntimeStateEntry` 迁入 `ServiceBridgeState.pending_prompt_process_mirrors`，但主控沿 V1 重启语义继续审核后发现更深的母事务缺口：`SessionDocument::enqueue_input` 当前只把 submission 追加到 `metadata.input_state.queued_inputs`。如果此时已存在尚未产生 `ApiStarted` 的开放 `GoalContinuation` Turn，新 queued user 既不会关闭该空 Goal Turn，也不会建立优先的 UserRoot Turn；进程重启后旧 mirror 为空，Goal reconcile 仍可能先派发 Goal，而 queued input 又没有新的 Turn completion 边界可触发调度。Core 必须把“普通运行中 Turn 仅排队”与“未派发 GoalContinuation 被 queued user 原子抢占”冻结为一个 failure-atomic、可幂等的 enqueue 领域转换，返回强类型 successor receipt；服务层不得用 enqueue→close→schedule 多步拼接，Store/WebUI 也不得补偿。
147. Queue 兼容镜像迁移还暴露了 queued CRUD 的事实分裂：`service_edit_pending_prompt`、`service_remove_pending_prompt` 与 `service_reorder_pending_prompts` 当前都只修改 `pending_prompt_process_mirrors`，Core 又尚无 move-to-draft/remove/reorder queued input 的领域入口，因此 UI 显示的编辑、删除和顺序不会改变 `metadata.input_state.queued_inputs`，最终 completion 母事务仍会发送旧正文、旧资源或旧 FIFO。必须由 Core 提供精确 `ClientActionId/PendingInputId` 驱动的 failure-atomic queued CRUD 与显式 Committed/Idempotent receipt，Edit 原子移入 metadata draft，Remove 原子删除，Reorder 原子发布完整稳定顺序；服务成功 flush 后才更新 process mirror，镜像不得成为 CRUD 权威。
148. 最新只读残差矩阵再次证明阶段 4-12 尚未进入最终切换：V1 hydration/page/live RPC、稀疏 patch publisher 与 selected-session controller owner 已生产接线，但 `provider_v1_sink::start_request` 仍固定拒绝未提交 preparation，主/子 ChatArea 仍渲染 `workbench.messages`，Continue 仍传 generation/ledger，edit/fork、metadata 生命周期、Provider trace、compaction commit、子代理回流与 generated media 均没有唯一生产 caller；V1 card/review/optimistic overlay 仍以纯构件或测试宿主存在。当前可以独立继续的工作是 Provider permit、completion receipt-only chaining、compaction commit、Continue V1 service、Queue 第二权威与 CRUD、Provider trace、metadata-first 生命周期、Workspace Review V1 selector 生产切换和各输入路径 ResourceId admission；active-tail A/B 发布、edit/fork 继承语义及 generated resource 的受控内容路由仍等待明确架构确认后才能完成最终 durability/detail 删除。
149. `metadata.input_state.draft` 当前也是未接生产的空壳：Core 没有 draft setter/clear/move 领域入口，仓库生产代码对 `input_state.draft` 为零写入；WebUI 仍通过 `composer-panel/helpers.ts` 的浏览器本地 `vulcan-code.composer-draft.v1:*` 保存已存在会话的正文和附件草稿。最终切换必须让已存在会话的 draft 以 V1 metadata 为唯一持久化权威，并为正文与有序项目 ResourceId 提供幂等、可节流但 flush 边界明确的 setter/clear；浏览器内存仅可作为未确认输入和即时编辑缓存，服务 ACK 后必须以 V1 投影收敛，旧 localStorage 会话草稿键随后删除。新会话尚未 materialize 前的项目级本地 draft 可继续独立存在，不能与已存在 session 的 metadata draft 混为同一 owner。
150. workspace-store 在 Core 本轮公共契约稳定后完成串行最终门禁：`cargo test -p vulcan-workspace-store --lib -- --test-threads=1` 为 378 passed、2 ignored，耗时 1201.17 秒；`cargo check -p vulcan-workspace-store --tests` 通过；`cargo clippy -p vulcan-workspace-store --lib --no-deps -- -D warnings` 返回 `No issues found`。此前被并行 Core 暂态或输出截断影响的结果全部作废，以本轮串行证据为准。该结果证明 Store 当前 writer/loader/resource/trace/V1 夹具在既有契约下自洽，但不证明 Provider、生命周期、active-tail 发布点、WebUI 与旧链已完成生产切换，阶段 7/11/12 状态仍保持未完成。
151. Workspace Review 已完成独立生产切换并经主控源码复核：`WorkspaceReviewPanelHost` 只读取 WorkbenchApp 持有的 selected-session V1 external-store snapshot，先精确核对 owner session，再用连续 seq 选择最新携带 typed `PersistentChangeSet` 的 ToolCall；失败 ToolCall 不被过滤，generation/event_revision/prepend 均由原子 snapshot 决定。`workspaceReviewSnapshotFromChangeSetV1` 直接按持久化文件、hunk、增删行号和 full/truncated/omitted 生命周期构造 review model，`rawDiff` 仅由 typed model 单向序列化用于复制，不再作为解析输入；Host 的旧 `latestConversationDiffSource`、异步 raw-diff parser 状态和 Review 专用 `parseConversationBlocks` prop 链已删除。真实 Host+external-store 测试与 selector 共 12 项通过，TypeScript、UI architecture、production build 通过，生产 `latestConversationDiffSource` 搜索归零。该子链可记为阶段 11 的局部完成，但主/子 transcript、Continue、tool detail 与旧 hydration 仍未切换。
152. Core InputState 的 queued CRUD 与未派发 Goal 抢占语义已经完成母层闭环并由主控独立复核：新增 `queued_input_resolution` UiOnly 主事件，持久化原 `ClientActionId`、`PendingInputId`、正文、有序 `ResourceId` 与 `MovedToDraft/Removed` 终结类型；move/remove 在同一 staged revision 内提交 Metadata+Tail，精确重试在重启、后续队列变化和草稿再次修改后仍返回原 Idempotent receipt，部分身份漂移显式冲突、完全未知显式 NotFound。`enqueue_input` 在无活动 API 的未派发 `GoalContinuation` 上原子写 Stopped TurnBoundary/TurnStats、创建 `QueuedScheduled` UserInput 并打开 UserRoot，活动 Goal API 时仍只入队。加载器拒绝重复 resolution 身份、仍处于 pending/queue、已经调度和错误 draft 绑定等矛盾。后续流式幂等修复完成后，主控重新独立运行 `cargo test -p vulcan-session-core -- --test-threads=1` 为 218/218，`cargo check -p vulcan-session-core --lib` 通过，strict lib Clippy 为 `No issues found`。此事实只证明 Core 领域层成立；service Queue 必须继续区分 admission receipt 与当前显示状态，`SupersededGoalAndScheduled` 的唯一物理准入消费者尚未完成，因此阶段 4/6 仍未满足退出门。
153. Queue 服务投影已按完整 `EnqueueInputReceipt` 收口并经主控复核，但物理准入仍单独阻塞：`commit_existing_submission_v1_queue` 对新 `Queued` 精确核对 V1 metadata，对已 move/remove 的历史 admission 幂等重放允许当前 queue 行不存在且禁止重建 mirror/Hook；`SupersededGoalAndScheduled` 不再假装 queued，也不要求 queue 行，ACK 携带精确已创建 Turn。进程显示镜像仍只在 V1 flush 成功后更新且不进入 `state.json`。主控独立运行 `cargo check -p vulcan-service-kernel --tests` 为 0 error（9 组未接线/待删除 dead-code warning），`cargo test -p vulcan-service-kernel conversation_submission_queue_tests --lib -- --test-threads=1` 为 13/13。尚未闭环的是：Superseded 的 changed=true 与幂等 replay=false 都必须先交唯一 admission/permit owner 收敛 physical dispatch；当前代码对 no-change receipt 仍会提前 ACK，因此 Hubble 强类型入口接入前不得把 Queue、阶段 4 或阶段 6 标记完成。
154. Core 新事件的跨层严格 DTO 审核曾发现 WebUI V1 联合类型落后：TS 缺少 `queued_input_resolution`，子代理回流缺精确任务/正文/统计 seq，统计仍允许已删除的 `in_progress` 且 totals 不完整，执行控制也缺少 `api_transport_observed` 与 `client_action_id`。主控现已独立完成严格同步：共享 TS 联合类型、event identity、non-rendered/display 语义均按 Core 枚举穷尽；子代理统计只允许 `final` 并保留完整 `StatBucket`；不存在 default/fallback。该修复只代表跨层 DTO 已闭环，主 transcript 尚未生产切换，阶段 8/11 仍不得宣称完成。
155. 流式累计快照的伪 revision 已从 Core 与 Registry 母层协同修复，不在 WebUI 子层去重：`append_reasoning/append_assistant_text` 现返回强类型 `IdempotentMutation<Seq>`；完全相同正文与状态返回 Idempotent，不推进 event/document revision，正文只允许前缀式累计增长，Streaming 可一次转为 Completed/Interrupted，终态只允许精确重放，正文回退、终态改写、来源漂移和跨 API 更新显式失败且文档不变。Registry 新增纯内存 conditional mutation owner，Changed 才发布稀疏 volatile patch，Unchanged 不发布、不 flush、不触盘；durable conditional 路径复用同一校验母层后再 flush，避免两套变化判定。主控验证 Core 218/218、check、strict Clippy 全绿；service-kernel `--tests` 编译 0 error，reasoning 与 assistant-text 两条精确 live 回归各 1/1，通过断言重复快照不增加 patch 或 flush。现有 9 组 dead-code warning 仍指向未切换的生产旧链，不能据此提前完成阶段 4。
156. Rust→JSON→TypeScript 共享事件窗口 golden 已由主控按真实 Core 生命周期扩展为六条连续事件：根输入、`queued_input_resolution(Removed)`、请求配置切换、`api_started`、HostOnly `api_transport_observed`、终态 `api_stats(ProcessInterrupted/Allowed)`；`memory_revision=8` 与真实文档一致。Rust 服务契约新增队列处置缺失身份和未知字段负测，前端 controller golden 明确验证队列身份/处置正文以及传输已观察事件的 visibility/API 身份。`cargo test -p vulcan-service-api conversation_events_v1::tests` 为 11/11，前端 store+controller Vitest 为 71/71，`tsc -b` 通过；Core 已有子代理缺字段与非 Final 重载负测，因此未在 service 子层复制第二套 validator。
157. 提示词锁定来源的生产装配已从“拼成一个字符串再猜来源”改为强类型逐段保真：`ExecutionPreparation` 不再暴露单一 `prompt_skill_developer_segment`，而是保存有序 `additional_developer_segments`；启用的提示词技能以 `PromptSkillInjection/enabled-prompt-skills` 形成一条锁定来源，每个插件贡献以 `PluginInstructions/{plugin_id}/{contribution_id}` 独立形成锁定来源，代码库工具路由提示保持请求动态段，不再误入 runtime lock。`provider_runtime_context` 只透传装配器给出的 typed provenance，不按标题或正文重新分类。`cargo check -p vulcan-agent-runtime --lib`、`--tests`、两组定向测试及 strict lib Clippy 均通过；该修复闭环了已识别的来源合并错误，但记忆画像、各层 AGENTS/Vulcan、人格、输出风格与 Hook 的全部生产来源仍需在唯一 Provider permit 投影中逐项证明，阶段 4/5 尚未完成。
158. Provider Permit 的遥测边界已从闸门前 Started 改为强类型两阶段状态机：`RequestPrepared` 只预留 request identity 与 attempt 候选，并把 unpublished Started-shaped lifecycle facts 交给唯一 V1 preparation owner，禁止写遥测、递增 API 计数或产生 UI RequestStarted；Permit 返回的 `RequestStartDurabilityConfirmed` 携带最终 V1 request snapshot/context budget，使用同一 request id 提升为 active 并发布唯一 Started。拒绝候选会释放 pending identity、不生成 Started/Completed、也不消耗物理 attempt；main/detached/compaction preparation handler 均不再在 Permit 前写遥测。主控验证 agent-runtime 两组新边界测试、21 项遥测测试、strict lib Clippy、service-kernel `--tests` 编译及 V1 请求重建回归均通过。该结果闭环了“闸门前候选被当成事实”的根因，但阶段 4 仍需继续接通普通成功 Turn completion receipt、Compaction 分流和 Superseded queued Turn 的唯一物理 admission。
159. 普通 API 终态、逻辑 Turn 完成与后继分发的生产母边界已进一步收口：Core 不再接收调用方按流任务起点计算的 `duration_ms`，而是从权威 Turn 起始事件到完成时刻计算完整逻辑回合时长；`ProviderV1StreamSink` 不再把 `complete_api_group` 的可靠回执丢成 `()`，而是一次性向 finished 边界转移 `{ApiStats terminal receipt, optional TurnCompletionTransition receipt}`。无工具、成功、非 Compaction 的最终物理 API 才会原子关闭 Turn 并生成唯一后继；工具型 API 只关闭物理 API，不错误关闭逻辑 Turn。finished admission 现在以 V1 ApiStats 为权威终态可靠性事实，旧 workspace 快照失败不能继续卡住准入，也不能否决已经可靠生成的 queued/Goal/none 后继；旧链只保留尚未删除的过渡副作用。后继分发已经改成 receipt-only：queued 分支按 `input_seq + turn_id + input_id + ClientActionId` 精确读取 `QueuedScheduled` 主事件并按项目 ResourceId 目录恢复附件，Goal 分支核对精确 `turn_id + context_seq`，不再从 mailbox、旧 PendingPrompt 或 Goal 状态重新选择。`SupersededGoalAndScheduled` 的 changed 与幂等 replay 路径也在生产提交入口按浏览器 ClientActionId 解析同一开放 UserRoot 权威并调用同一精确分发函数；若无法取得物理准入则显式要求使用相同 submission id 重试，不再成功 ACK 后静默遗留未分发 Turn。主控验证：Core 全量 `220/220`；provider V1 sink `7/7`；finished V1/legacy 独立可靠性 `1/1`；Queue `13/13`；service-kernel lib check 为 0 error。现有 12 组 dead-code/unused warning 仍对应待切换或待删除旧链，因此阶段 4/6 尚未整体完成。

160. Continue 的并发重复派发窗口已从根层闭环：Core 新增 `ContinuationDispatchResolution`，只按持久化 `ApiStarted`、`ApiTransportObserved` 与终态 `ApiStats` 重建同一 `ClientActionId` 的 `AwaitingTransport/TransportObserved/TerminalBeforeTransport` 状态；`SessionHandle` 增加每会话进程内单飞门，但该门只协调并发，不保存业务事实，进程重启后仍以 V1 主事件为唯一权威。Continue 服务在 V1 durable admission 前取得该门，已存在可靠 TransportObserved 时返回同身份幂等 ACK，尚未观察 transport 时由后台准备任务持门直到可靠观察或明确的传输前终止；浏览器超时、取消或 RPC 重放不会提前释放并启动第二次物理请求。`complete_api_group` 在可靠终态提交后唤醒等待者，等待循环采用先注册 `Notify` 再检查状态的方式避免丢失唤醒。主控独立验证 Core 全量 `221/221`、两项 continuation 状态重建测试、kernel 单飞测试与 service-kernel lib check 均通过；当前仍有 20 组旧 continuation/retry/queue 显示链 dead-code warning，下一批必须物理删除，阶段 4/6 尚未整体完成。

161. Ask 的 V1 durable-before-visible/resume 母边界已由主控单线程收口：`VulcanToolInvocationContext` 新增独立的 controller-owned tool-call identity，QuestionRuntime 以精确 `session_id + ApiCallId + ToolCallId` 作为权威身份，Provider 原生 tool-call id 仅保留可选关联；等待、回答、取消均先通过 `SessionHandle` 提交并 flush 同一 V1 ToolCall 事件，再发布 WebUI 状态或唤醒阻塞 future。可靠回答/取消后的通用 `ToolFinished` 只确认既有 Completed/Cancelled 终态，不再写第二个终态。审核同时定位并修复了一处真实锁重入死锁：reply/reject 原先持有 pending 写锁调用投影回调，而回调再次读取 pending；现写锁仅覆盖精确查找、可靠提交和移除，释放后才完成 process waiter 与 UI 广播，未削弱同请求串行性。验证：agent-runtime QuestionRuntime `15/15`，cancelled Ask terminal confirmation `1/1`，service projection broadcast `1/1`，`cargo check -p vulcan-service-kernel --tests` 0 error。Ask 的领域与服务母边界已经成立，但 detached child Provider owner、子代理父级回流和旧显示链尚未完成，因此阶段 4/5/6 仍保持未完成。

162. 会话 metadata 生命周期与 SQLite 列表投影已切到“文件权威、索引随后”的单向提交协议：每个 `SessionHandle` 的生命周期门串行覆盖强类型 metadata mutation、V1 flush 与精确 SQLite row 投影；rename/archive/restore 只在 metadata 可靠后更新 title/archived_at/updated_at，SQLite 失败返回可重试投影错误且不回滚已可靠事实，同一操作重试不会产生 revision 漂移。restore 先按 SQLite 精确 locator 预检 metadata，文件缺失时删除陈旧索引并显式报告，不扫描目录、不猜测；delete 先关闭准入并可靠 flush，再删除精确目录，目录缺失视为已删，最后移除 row。active/archived 列表改为只读 V1 session index，并以 `updated_at_ms + session_id` 稳定排序。验证：生命周期 `11/11`、workspace-store `--tests` 编译、service-kernel `--tests` 编译与 workspace-store strict lib Clippy 均通过；旧分段生命周期路径仍需随阶段 12 其他消费者一起物理删除。

163. Provider API 调试 trace 的旧 UUID group/进程 group sequence/四槽目录体系已从生产与测试中物理删除；原错误文案 `Provider API trace group identity changed within one generation` 与全部旧 free function 搜索归零。唯一新链由 `SessionHandle` 持有的 `ProviderApiTraceStore` 负责：物理派发前按 V1 `ApiCallId + attempt_index` 可靠预留连续 `trace_sequence`，响应结束后发布脱敏 input/output 完整对，保留最近十次；trace 失败只记录诊断，不影响 Provider 主链。打开、清理、预留和发布现在共用一个 session owner 锁，清理会精确重置序号前沿，并在创建/删除前拒绝 trace root 或直接 parent 的符号链接；配置推理的无会话 trace 保留为独立诊断分支并改用专属常量/锁。验证：新 store `10/10`、配置推理隔离 `1/1`、service-kernel `--tests` 编译及 workspace-store strict lib Clippy 通过。该条完成阶段 7 的 trace 子项，但 Provider Permit、Compaction 与主 WebUI 切换仍未整体完成。

164. 已存在会话的输入草稿现已切换为 V1 metadata 唯一可靠权威：新增严格的草稿读写 DTO、服务与 RPC，所有附件先解析或复用项目级 `ResourceId`，metadata 只保存正文与有序资源引用；现有会话不再读写浏览器 localStorage，新会话尚未 materialize 前仍保留独立的项目级本地草稿。前端按会话串行合并高频输入，提交前等待精确 V1 flush，失败不调用提交 API；提交成功后由 Core 根输入、排队或过程内输入母事务仅在正文与资源完全匹配时原子消费草稿，失败恢复原草稿。资源解析只下发受控内容 URL，不暴露物理路径。验证：service-api 严格 DTO 负测 `1/1`，前端草稿客户端与串行协调器 `7/7`，TypeScript、UI architecture、production build、service-kernel `--tests` 编译通过。该条闭环了执行事实 149 的既有会话草稿权威缺口；主 WebUI 与其他旧运行态投影仍需继续审核。

165. 活动尾段已经从原先“覆盖同一路径后再写 metadata”的崩溃窗口改为每个分片 A/B 双 bank 单提交点：写入器始终把完整候选写入当前 `SegmentInfo.file_name` 的对侧 bank，滚段时也先把完整封口段写入未发布 bank，最后只由 metadata 切换权威文件名；metadata 发布失败时旧 bank 仍逐字节权威，重试可安全覆盖尚未发布候选，metadata 成功后旧 bank 才成为可清理对象。实现以 `alternate_segment_file_name` 和严格 bank 文件名校验为唯一物理规则，不依赖 Debug/Release 双进程假设。故障测试 `active_tail_metadata_failure_keeps_old_bank_authoritative_and_retryable` 已存在，workspace-store 全量 `378 passed / 2 ignored`、`--tests` check 与 strict lib Clippy 均通过；原执行事实 103/130 所述“尚待选择 A/B 协议”已由当前源码事实取代。

166. 工具生成媒体已经进入 V1 强类型主链，不再依赖旧 detail RPC 或本地路径猜测：Core `ToolCallEvent.generated_resources` 持久化有序 `GeneratedResourceRef { resource_id, kind, preview_resource_id }`；Provider 工具终态在可靠提交前把生成附件登记到项目级 Resource Catalog，再把得到的项目 `ResourceId` 写入同一工具事件。服务只按 `session_id + resource_id` 精确定位并校验目录、大小与 SHA-256 后返回受控内容响应；WebUI V1 DTO、工具卡和资源列表只消费这些 typed 引用，缺失实体显示固定不可用占位，不暴露 `entity_relative_path`、绝对路径或 provider binding。Rust→JSON→TypeScript golden、kernel 生成资源测试与前端卡片/renderer 测试已覆盖；原审计中把 GeneratedResourceRef 视为待决契约的结论已过期。

167. 后续自动化与性能验收的并发策略已按实际硬件冻结：当前主机为 AMD Ryzen 9 9950X（16 核 32 线程），纯 CPU、彼此独立且不共享进程级资源的测试默认最多使用 8 个线程或 worker，Release 编译使用 `-j 8`；涉及 SQLite、服务注册表、端口、Provider Manifest、同一 session 文件、故障注入顺序或其他共享权威状态的测试继续使用 `--test-threads=1`。任何时刻只运行一个 Cargo 命令，禁止通过并发 Cargo 任务争抢 target 锁和共享资源制造假失败；该策略用于加速而不改变测试语义。

168. 100k 性能初测暴露了一处真实 Core 复杂度缺陷：`SessionDocument::close_turn` 经 `aggregate_turn_buckets` 在每个 Turn 结束时从会话起点两次扫描全部事件，连续 Turn 因而形成 O(n²) 累积。修复位于领域母层而非 Store 或测试：`aggregate_turn_buckets` 现在接收权威 `start_seq`，先严格校验 resident suffix 的首事件 seq/turn identity，再只扫描该 Turn 起点后的常驻后缀，同时保留对 session-scoped 交错事件的过滤语义；Goal supersede、完成 Goal、普通 close 与 queued/Goal successor 四个调用点统一使用该入口。验证 `cargo test -j 8 -p vulcan-session-core --lib -- --test-threads=8` 为 `231 passed`。

169. 初始 100k Release 结果中 457,529 ms 的构建时间不能解释为持久化链本身：分阶段基准 `performance_completed_turn_phase_costs` 在 2,000 Turn/12,000 event 下显示 `submit_root=30 ms`、`prepare_api=4,274 ms`、`assistant=3 ms`、`complete_api=4 ms`、`close_turn=4 ms`。唯一主导项来自 Store 性能夹具的 `prepare_api` 每 Turn 都调用 `provider_visible_fingerprint(document.events(), through_seq)`，把从 seq 1 开始的全部历史重新 clone、投影和序列化；该夹具同时又从不创建压缩边界，因此测量的是“每次请求重发无限增长全部历史”，不是会话文档 mutation、flush、窗口读取或滚段性能。

170. 100k 存储性能夹具已按职责边界修正，而未修改生产 Provider 行为：存储规模测试仍调用公开且失败原子的 `prepare_and_start_api`，但只提供满足 Core proof shape/frontier 契约的固定 64 位小写指纹，使该测试专门测量主事件 mutation、分片、flush、窗口加载与 edit generation；真实 `provider_visible_fingerprint` 保留在小规模语义测试和独立分阶段基准中。修正后的 Release 结果为：100,200 events、34 segments，构建 1,838 ms（其中 root admission 1,256 ms、prepare 44 ms、assistant 12 ms、API completion 27 ms、Turn close 31 ms），活动尾段加载 2 ms，最近/更早 30 Turn 分页各 1 ms，Provider 精确窗口 1 ms 且只读 1 个分片，第 90k 附近 edit generation 构建 9,431 ms、发布 130 ms、复制前缀 49,037,857 bytes，编辑后 append flush 12 ms；测试总耗时 13.73 秒。该结果同时证明原 457 秒主要是错误组合基准，但不掩盖已经独立修复的 Core Turn 聚合 O(n²) 缺陷。

171. 修正后 100k Release 测试的进程采样证据为：wall 11,397 ms、CPU 10,719 ms、平均约 0.94 个核心、整机 32 逻辑线程占用约 2.94%，采样峰值 working set 约 326.5 MiB；峰值包含完整 100k resident `SessionDocument` 与 edit generation 副本，不能冒充普通启动常驻内存。现有 1/4/8 MiB 活动尾段基准分别为：1 MiB p50 33.373 ms/p95 35.716 ms/30.086 MiB/s，4 MiB p50 84.552 ms/p95 91.087 ms/47.356 MiB/s，8 MiB p50 161.523 ms/p95 190.792 ms/49.554 MiB/s。当前 root ClientActionId 全历史查重在 100k 总计约 1.256 秒，尚不足以证明需要新增持久化派生索引；除非后续完整审核得到真实产品瓶颈证据，否则不为节省约一秒引入第二事实源或额外落盘结构。

172. 阶段 12 旧会话链路的精确关键词门禁已再次执行：`state_documents`、`SEGMENTED_STATE_FILE`、`runtime_io`、`runtime_backup`、`SEGMENTED_RUNTIME`、`append_segmented_session_message_tail`、`workspace_session_persistence`、`SessionProviderCallLedger`、会话 `goal_store`、`execution_retry_store`、`ConversationDetailRef`、`ToolDetailLoader`、`conversation-tool-detail-service`、`legacy_partial`、`message_mapping` 与 `rewrite_json_value_identity_tokens` 均为零命中。仅剩的 `provider_usage_migration` 命中属于应用级 Provider 用量数据迁移，不读写 session V1 事实；`runtime_log` 命中属于 VMM 运行诊断变量和测试名称，不是已删除的分段会话 runtime log 模块。该分类以当前源码调用点为准，不为满足字符串归零误删无关职责。
173. 已物理删除无生产消费者的旧分叉投影副本：workspace-store 的 `SessionTaskMirrorSnapshot`、`SessionForkSourceSnapshot`，service-api/WebUI 的 `ConversationForkSourceSummary`、`ConversationSummary.fork_source`，以及所有固定 `fork_source: None` 构造和对应文案均已移除。V1 `MetadataDocument.fork_origin` 继续保存唯一可靠的父会话、父 generation 与目标 seq；前端不再收到从未被真实填充的旧 message-anchor 分叉 DTO。`SessionTaskMirrorSnapshot|SessionForkSourceSnapshot|ConversationForkSourceSummary|fork_source|forkSource|anchor_message_id` 搜索归零。
174. 主 WebUI 已不再声明或消费旧 `ConversationMessage`、`ConversationRenderItem`、`ConversationTimelineItem` 与 message-metadata thinking 过滤模型；无引用的 `conversation-metadata.ts` 已删除，service-api 的旧 `ConversationMessage` DTO 也已移除。生产 ChatArea 只从 selected-session V1 event-window snapshot 取得 `authoritativeEvents`，V1 renderer/tool card 是唯一 transcript 消费者；旧 `projectConversationRenderItems`、`parseConversationBlocks`、tool detail loader、旧卡片文件与 `workbench.messages` 生产读取均为零命中。Provider 侧仍存在的 `vulcan_core::types::ConversationMessage/ConversationSnapshot` 只由 V1 projection 在 permit 边界即时构造为传输适配输入，不能反向写入或覆盖 V1 事实。
175. 对 `EnqueueInputDisposition::SupersededGoalAndScheduled` 的完整调用链复核证明该分支没有静默遗留开放 Turn：`queue_existing_submission` 只提交并返回 durable admission receipt；其唯一生产调用者在看到 ACK `turn_id` 后，以同一 `ClientActionId` 从 V1 读取精确 UserRoot authority，并调用 `start_exact_v1_queued_turn`，失败则显式要求复用同一 submission id 重试。普通 Turn 完成也只消费 `TurnCompletionTransition.successor`；Goal/queued/none 不从旧 mailbox、PendingPrompt 或 Goal 状态二次选择。

176. 阶段 13 的全量自动化门禁已在同一工作树串行完成：`cargo fmt --all -- --check`、选定生产 crate 的 strict Clippy、`cargo check -j 8 --workspace --all-targets`、`cargo test -j 8 --workspace -- --test-threads=1`、`cargo test -j 8 --workspace --doc` 全部通过；加入启动隔离回归后的全工作区测试结果为 `3591 passed / 7 ignored / 82 suites`，doctest 为 38 个 suite 全绿。WebUI 以最多 8 worker 运行 92 个测试文件、497 项测试全部通过，随后 `pnpm run lint`、`check:ui-architecture`、production build 与 build budget 全部通过；生产构建仅保留仓库既有的大 chunk 提示，不含本任务新增错误或警告。

177. 全工作区测试首次发现唯一真实失败：`system_provider_registry_matches_golden_snapshot`。源码与专用测试证明权威 `minimax.json` 的 global/CN 路由都要求 `anthropic-messages-video.v1`，而已提交 golden 同时遗漏两条路由；问题是测试基准陈旧，不是生产 manifest 错误。已使用仓库规定的生成入口机械重建 `crates/vulcan-provider-adapters/resources/golden/system-providers-v2.json`，只补充对应的 106 行路由事实；关闭生成环境变量后定向 golden 测试通过，随后全工作区测试通过，没有放宽比较或删除有效路由。

178. 真实 Controller/SQLite 故障验收已从伪事务修正为真实跨进程锁：Controller RPC 不保留跨请求事务，原测试把 `BEGIN EXCLUSIVE` 与带参数 SQL 拼成一次调用既违反单语句协议，拆成两次调用又无法持锁。现测试从打包 runtime manifest 精确解析受管 Python，以 stdlib `sqlite3` 持有外部 `BEGIN EXCLUSIVE` 事务，并用 stdout readiness 与 Drop 守卫保证锁已生效且失败时不遗留进程；Controller 的 archive 写在锁期间返回结构化索引写失败，释放锁后同一生命周期继续成功。`real_controller_session_v1_lifecycle_and_failures` 与 `real_controller_markdown_bm25_end_to_end` 两条 ignored 真实运行用例均通过，随后 workspace-store `--tests` check 与 strict lib Clippy 通过。

179. Release 启动旧库缺少 `session_index_v1` 时的非必要阻断已经从启动母边界修复：根因是工作区快照加载先查询 V1 索引、而 Schema 只在创建 V1 会话时初始化，导致完全合法的旧 Release 配置库以 `no such table: session_index_v1` 阻断整个应用。当前启动先尝试建立空 V1 索引；该初始化、某项目活动/归档列表查询、单条非法 `session_id` 或归档项目下的活动行发生错误时，只输出带项目/记录身份的诊断并跳过对应非必要会话数据，不阻断工作台与核心服务启动。显式打开、恢复或修改具体 V1 会话时仍保持严格错误，不在加载器中兼容旧格式、扫描目录、猜测地址或自动修复损坏。真实 Controller 测试覆盖“仅有旧项目表、没有 V1 表”和“V1 索引含非法会话 ID”两种启动场景；Release 已重新构建并用正式二进制验证 discovery ready、带 token 的 `/rpc` OPTIONS 200、主窗口正常关闭且进程退出码 0。正式库启动后只创建空 `session_index_v1`，行数为 0，证明旧会话没有被导入；由于无需删除即可隔离，未执行不必要的破坏性清库。

180. 最终工作树完成连续五轮全范围零问题审核后，按仓库正式入口 `./make.ps1 release` 再次重建最终 Release：WebUI 共转换 4589 个模块，Tauri 桌面壳与 `vulcan-terminal-host` 以 `custom-protocol` 和 optimized release profile 构建成功，正式产物发布至 `output/bin`，总 Rust Release 构建耗时 3 分 06 秒。此次最终重建没有编译错误或新增警告，WebUI 仅保留仓库既有的大 chunk 提示；结合第 179 条同一实现版本的正式二进制启动、RPC 与正常退出验收，阶段 13、Release 验收和五轮审核退出门全部满足。

181. 第一次五轮审核后的暂存差异门禁发现 `conversation-model.ts` 与 `thinking-display.ts` 各有一个文件末尾多余空行；虽然不影响运行语义，仍按“发现任何问题即把连续审核计数归零”的规则处理。删除两处 EOF 空行后，主控以最终暂存树重新执行五轮审核：第一轮覆盖 Core 231、Store 故障保持 5、关闭 3、Release 隐藏进程 2 与旧链归零；第二轮覆盖索引 10、生命周期 13、Provider V1 10、服务事件 13、Web 窗口 71 与启动/创建顺序；第三轮覆盖 Goal 30、子代理 35、压缩 14、Continue 4、问题投影 3、Agent Ask 15、Agent Provider V1 10、前端 Continue 8 与 receipt-only 后继；第四轮覆盖服务事件 13、ChangeSet 12、WebUI 92 文件 497 项、TypeScript、UI architecture 和旧详情/排序扫描；第五轮覆盖 100,200 事件与 90k 编辑、1/4/8 MiB 尾段、真实 Controller/SQLite 与旧库启动隔离、故障保持、关闭、Release 隐藏进程及暂存差异。五轮均无新发现。随后再次运行正式 `./make.ps1 release`，最终树 WebUI 4589 模块、Tauri/terminal-host optimized release 增量构建 29.79 秒完成，build budget 全部通过；因此最终有效连续计数为 5/5。

### 14.1 核心修复与调整概述

本任务已经完成会话持久化 V1 的全链路切换，并把持久化、Provider、运行时语义与 WebUI 展示统一到同一条按 `seq` 排列的强类型主事件流：

1. 以 `SessionDocument`、`SessionHandle` 和 V1 分片目录建立唯一内存 owner、唯一可靠写入器、唯一严格加载器和唯一 Provider 投影，移除 messages/records/state.json/runtime log 等旧会话事实副本。
2. 主事件完整表达根输入、过程内输入、排队、Continue、reasoning、assistant、工具、Ask、Goal、Plan、子代理、压缩、请求配置、错误、中断、API 统计和 Turn 统计；同一事件的流式演进使用 `event_revision`，顺序只由 `seq` 决定。
3. Provider Permit 在同一 session owner 中完成精确事件范围读取、锁定基线投影、请求指纹、`api_started` 可靠提交与实际网络派发；Continue 额外以 `ClientActionId` 和 `api_transport_observed` 建立可重启的 exactly-once 边界。
4. 活动尾段采用 A/B bank 与 metadata 最后发布，metadata 生命周期采用“V1 文件权威、SQLite 精确索引随后”的单向提交；失败允许文件暂时领先索引，禁止索引领先可靠事实。
5. Goal、Plan、Ask、排队、Goal 自动继续、子代理回流、上下文压缩、编辑、分叉、草稿、资源和生成媒体均进入强类型 V1 事务，不再通过服务层多步拼接母事务。
6. WebUI 由 selected-session V1 event-window owner 接管 hydration、分页、稀疏 live patch、revision frontier、重连恢复、主/子会话 transcript、Continue、Workspace Review、工具卡和 ChangeSet；旧 transcript parser、详情 RPC、record lookup 和重启猜测逻辑已删除。
7. Release 下所有后台命令创建统一经过隐藏窗口工厂；正常关闭等待必要 session flush，Provider trace 等非主数据失败只记诊断，不阻断可靠主数据或应用启动。
8. 启动时主动保证空 V1 索引存在；旧库、缺表、某项目列表失败或单条非法索引记录均只跳过非必要会话数据，不阻断工作台启动。具体 V1 会话的显式打开、恢复和修改仍严格报告损坏，不做兼容、目录扫描或自动修复。

### 14.2 文件变更清单

最终提交前工作树共有 568 个状态项：315 个修改、185 个删除、68 个新增；已跟踪差异为 500 个文件、`+19335/-152958`。大量删除来自旧会话持久化与旧前端 transcript/detail 双模型的物理清除，而不是把旧代码保留为未调用兼容层。

#### 新增

1. `crates/vulcan-session-core/src/v1/`：V1 ID、事件、文档、统计、Provider 投影、上下文、配置、服务 DTO 和完整领域测试。
2. `crates/vulcan-workspace-store/src/workspace_store/session_v1/`、`session_v1_store.rs`、`session_index_v1.rs`、`resource_catalog.rs`、`provider_api_trace.rs`、`session_vmm_v1.rs`：V1 分片、A/B 发布、精确索引、项目资源、调试 trace 和 VMM 文件。
3. `crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs`、`session_v1_lifecycle.rs`、`session_request_config_v1.rs`、`conversation_events_v1.rs`、`conversation_generations_v1.rs`、`conversation_drafts_v1.rs`、`conversation_resources_v1.rs`、`agent_stream/provider_v1_sink.rs`：唯一 session owner、生命周期、Permit、事件窗口和控制服务。
4. `crates/vulcan-service-api/src/conversation_events_v1.rs`、`conversation_generations_v1.rs`、`session_drafts_v1.rs`、`session_resources_v1.rs` 及严格跨层 golden。
5. `crates/vulcan-change-set/src/persistent.rs`：持久化 ChangeSet 强类型结构与完整母层校验。
6. WebUI 的 `conversation-event-window-*`、selected-session registry、React external store、V1 renderer/workspace/tool card、Continue selector、资源列表、乐观输入 overlay、V1 generation dialog 与 Workspace Review selector。

#### 修改

1. `vulcan-agent-runtime`、`vulcan-service-kernel`、`vulcan-core`、`vulcan-prompt-composer`：接通 typed baseline、Provider Permit、工具本地身份、Ask、Goal、Continue、队列、子代理、压缩和终态统计。
2. `vulcan-workspace-store` 的项目加载、查询、序列化与数据平面：只从 SQLite 精确 locator 和 V1 metadata 构建活动/归档列表，启动错误按必要性隔离。
3. `vulcan-host-tools` 与 Tauri shell：统一 Release 后台进程创建和正常关闭屏障。
4. WebUI 的 `WorkbenchApp`、`ChatArea`、workspace shell、composer、review、连接状态与服务 client：切换到 V1 owner、严格 DTO 和受控 ResourceId 内容访问。
5. 根 `Cargo.toml`、`Cargo.lock` 及相关 crate manifest：移除旧 crate/依赖，接入 V1 与 ChangeSet 依赖边界。
6. Provider manifest golden：按权威生成入口同步 global/CN 两条 `anthropic-messages-video.v1` 路由，没有修改生产 manifest 语义。

#### 删除

1. `workspace_session_persistence.rs`、`segmented_session_store/`、`session_load/`、旧 persistence/edit-fork/runtime-tail/snapshot、`runtime_logs.rs`、`runtime_state_codec.rs`、`session_addon_store.rs`。
2. 会话 `goal_store.rs`、`execution_retry_store.rs`、boundary store、Provider call ledger 与旧 provider usage migration 会话链。
3. `vulcan-main-session` crate、旧 session-core/store/router/compactor、旧 task-session segmented store 与 task persistence mirror。
4. service-kernel 的旧 conversation hydration/history/detail/display/continuation/message projection、旧 mailbox/checkpoint/retry/recovery 和旧 Goal summary parser。
5. WebUI 的旧 ConversationMessage/RenderItem/Timeline、transcript parser、tool detail、旧工具卡/展示 resolver、旧 continuation reconciliation、旧 history hydration 和 message merge/projection。

### 14.3 关键代码调整详情

1. `SessionDocument` 的 durable mutation 统一采用预校验、staging、一次发布和强类型 `Committed/Idempotent` 结果；失败前后文档完全相等，服务层通过 `mutate_durable_if_changed` 映射变化，不比较 revision 猜测。
2. 新请求调用图固定为：浏览器 `ClientActionId`/输入准入 → `SessionHandle` 可靠 mutation → 精确 V1 event range 与 locked baseline → `prepare_and_start_api` → flush → `ApiTransportObserved` → transport；旧 `ConversationSnapshot` 只允许作为 Permit 返回的一次性传输适配值。
3. API 终态调用图固定为：工具/文本终态 → `complete_api_group` 唯一 `api_stats` → 最终 assistant 时 `complete_turn_and_select_successor` → `TurnSuccessor::{QueuedUser, GoalContinuation, None}`；服务只分发 receipt 已创建的开放 Turn，不重读旧队列或 Goal 二次选择。
4. Continue 由 `continue_open_turn`、`continuation_action_id`、`ContinuationEligibility`、`ApiStarted` 和 `ApiTransportObserved` 共同确定；重复 RPC 只返回原 receipt，未观察传输与已观察传输在重启后有不同且明确的恢复语义。
5. 工具调用以本地 `ApiCallId + ToolCallId + RequestStreamId` 为权威身份，Provider 原生 ID 仅作可选关联；ToolCall 的 input、output、ChangeSet、generated resources 和 external refs 在同一事件中持久化，工具卡不再二次读取文件或详情接口。
6. 子代理由 child durable 文档构建 `SubagentReturnInput`，精确验证 final assistant seq、TurnStats 来源范围和完整 `StatBucket`，父文档一次原子追加正文与统计；不从旧 `DelegateFinished` JSON 重算统计。
7. 压缩提交携带实际 summary seq、retained boundary 和最新 `LockedBaselineProjection`，一次更新 compaction 节点与 runtime lock；系统上下文 TTL 只由持久化 TurnBoundary 消耗。
8. 分片 writer 把候选完整写入非权威 bank，fsync 后由 metadata 一次切换 bank 名；加载器只认 metadata 指向文件。生命周期 mutation 先 flush metadata，再投影 SQLite row，创建则完整目录发布后才插入 row。
9. 启动加载先执行 `ensure_session_v1_storage_schema`；Schema 初始化或项目列表失败被记录为带身份诊断并返回空/跳过结果。该策略只适用于启动所需的非关键展示数据，不吞掉显式 V1 操作错误，也不导入旧 session。
10. WebUI store 以 `segment_generation + memory_revision + durable_revision + event_revision` 验证 hydration/live/reconnect，稀疏 patch 不伪装连续分页；React 只渲染按 seq 的 V1 事件，失败工具、reasoning、错误和配置切换不再被重排。
11. 长会话性能修复把 Turn 统计聚合从每 Turn 扫描全历史的 O(n²) 改为从权威 `start_seq` 扫描当前 Turn 后缀；100,200 事件、34 分片的 Release 测试总耗时约 13.73 秒，第 90k 附近 edit generation 约 9.43 秒。
12. 自动化最终结果：Rust 全工作区 `3591 passed / 7 ignored / 82 suites`，doctest 38 suite；WebUI 92 文件、497 项；fmt、全目标 check、选定生产 crate strict Clippy、TypeScript、UI architecture、production build、build budget、真实 Controller/SQLite、100k、1/4/8 MiB、关闭屏障和 Release hidden-process 测试全部通过。
13. 连续五轮零问题审核依次覆盖：数据权威/A-B/关闭/Release；SQLite/metadata/启动隔离/Provider/Web；Continue/Goal/Ask/子代理/压缩；WebUI 顺序/重连/卡片/ChangeSet/旧链归零；文档/故障/性能/真实 Controller/最终 Release。任一中途发现的问题均在计数归零后重新开始，最终连续五轮没有新发现。

### 14.4 遗留问题与注意事项

1. 没有阻止本方案验收或迁移到 `docs/completed/` 的遗留问题。
2. V1 格式明确不兼容旧会话：启动不扫描、不导入、不自动修复旧目录；缺表或不支持的旧记录作为非必要会话数据忽略。用户若不再需要旧数据可人工清理，但系统正常启动不依赖清理动作。
3. 新格式的显式打开、恢复、编辑、分叉或生命周期操作遇到真实损坏时仍返回准确结构化错误；“启动不中断”不能被解释为吞掉用户主动操作的可靠性错误。
4. Provider API trace 是可选调试支线，写失败只保留诊断；session 主事件、metadata、资源 catalog 或必要 flush 失败仍必须阻止对应可靠操作或正常退出。
5. WebUI production build 仍有仓库既有的大 chunk 提示，但 build budget 全部通过；该提示不影响本次功能和可靠性验收，也没有通过放宽预算隐藏。
6. 100k 测试中的约 326.5 MiB 峰值包含完整 resident 文档和 edit generation 副本，不代表普通启动常驻内存；后续只有在真实产品证据显示瓶颈时才考虑派生索引或进一步懒加载，禁止为理论优化重新制造第二事实源。
