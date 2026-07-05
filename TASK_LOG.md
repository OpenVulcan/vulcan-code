# 子智能体恢复链路长期优化日志

## 2026-07-04 第 1 轮：子智能体恢复元数据父级索引下沉

### 本轮目标

定位并修复一处会拖慢会话切换与子智能体列表恢复的结构性坏味道，要求从执行链路上消除重复全量扫描，而不是在 UI 层增加等待或兜底。

### 问题与执行流分析

会话水合和运行时恢复都会读取分段 TaskSession 元数据，用于恢复未关闭子智能体列表。修复前的关键路径是：

1. `SegmentedTaskSessionStore::load_all_task_session_metadata()` 一次性加载所有子任务元数据。
2. `TaskRuntime::restore_persisted_subagent_registry_for_parent_tree()` 针对每个待扫描父节点，在全量元数据上执行 `metadata.parent == parent` 过滤。
3. `load_webui_unclosed_subagent_metadata_preload()` 先把全量元数据转换成 WebUI 状态，再按 `parent_session_id` 二次分组。

这个结构在子智能体数量、嵌套层级、会话切换次数上升后，会把“恢复未关闭子智能体”变成重复扫描和重复转换。它也会放大用户观察到的“打开会话后子代理列表过一会儿才冒出来”的问题。

### 核心修复

1. 在 `ParentSessionRef` 上补齐 `PartialOrd`、`Ord`、`Hash` 派生，让持久化父级引用可以作为稳定索引键。
2. 在 `SegmentedTaskSessionStore` 新增 `load_task_session_metadata_by_parent()`，一次 metadata-only 扫描后直接返回 `BTreeMap<ParentSessionRef, Vec<TaskSessionMetadata>>`。
3. `list_child_task_sessions()` 改为复用父级索引 API，避免存储层保留两套过滤与排序逻辑。
4. `TaskRuntime::restore_persisted_subagent_registry_for_parent_tree()` 改为从父级索引中 `remove(parent)`，树遍历期间不再反复扫描全量元数据。
5. `load_webui_unclosed_subagent_metadata_preload()` 改为按 `ParentSessionRef` BFS 读取对应组，只转换当前父级下的未归档子智能体状态。
6. 增加存储层测试，覆盖直属子任务和嵌套子任务按父级分组加载。

### 文件变更

- 修改：`crates/vulcan-session-core/src/model.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/store_impl.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/tests.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`

### 验证记录

- `cargo test -p vulcan-task-session load_task_session_metadata_by_parent_groups_direct_and_nested_children -- --nocapture`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：16 个测试通过。
- `cargo test -p vulcan-agent-runtime subagent -- --nocapture`：11 个测试通过；仅保留既有测试 dead_code 警告。
- `cargo check -p vulcan-service-kernel`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮自动审核发现并修复了一个普通构建下的 `BTreeMap` unused import 警告，同时将旧的 `list_child_task_sessions()` 接入新索引 API，避免存储层逻辑分叉。当前未发现新的阻塞问题。

## 2026-07-04 第 2 轮：子智能体终态记录与 mailbox 发布顺序收紧

### 本轮目标

审计子智能体运行结束、失败、中断、排队消息续跑与父级通知之间的真实执行顺序，修复“状态已显示完成但父级仍等待”“mailbox 已收到但列表/右侧状态不同步”“落盘失败后留下幽灵智能体”的底层一致性风险。

### 问题与执行流分析

修复前的关键路径是：

1. `run_task_with_shared_runtime_context()` 后台 Join 分支拿到子智能体终态后调用 `store_task_outcome()` 或 `mark_task_failure()`。
2. 这些调用的错误被 `let _ = ...` 静默丢弃。
3. 后台分支继续释放 active guard、续跑排队消息、向父级发布完成或失败事件。
4. `store_task_outcome()` / `mark_task_failure()` 内部先更新内存记录或 registry，再发布 mailbox，最后才尝试写分段 TaskSession。
5. 如果分段持久化失败，父级已经可能收到终态消息，排队消息也可能被唤醒，但磁盘、registry、resident sessions 三者并没有同一事实来源。

这个顺序会放大子智能体面板和主线程看到不同状态的问题：主线程拿到消息继续推理，UI 可能还认为智能体没有结束；或者 list_agents 看到的 registry 与 resident session 不一致。

### 核心修复

1. 将终态写入顺序收紧为：分段 TaskSession 持久化成功 -> 提交 resident `sessions` -> 更新原生 `subagent_registry` -> 发布 mailbox。
2. `persist_task_record_outcome()`、`persist_task_record_failure()`、`persist_task_record_state()` 改为返回 `Result<TaskSessionStatus, String>`，不再吞掉持久化错误。
3. `store_task_outcome()`、`store_running_task_outcome_in_memory()`、`mark_task_failure()` 改为两阶段记录构造：先构造候选 `TaskSessionRecord`，持久化成功后才提交到内存 sessions。
4. 后台子智能体终态分支不再忽略记录失败：终态记录失败时向父级发布明确失败诊断，并且不续跑排队消息。
5. 运行中 prompt intervention 注入失败时，恢复 inbox 的合并错误不再被丢弃，至少输出明确诊断日志。
6. 新增失败测试，覆盖终态落盘失败不会发布 mailbox、不更新 registry、不污染 resident sessions；运行态启动落盘失败也不会留下幽灵 resident session。

### 文件变更

- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/agent_control.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`

### 验证记录

- `cargo test -p vulcan-agent-runtime persistence_store`：12 个测试通过；仅保留既有测试 dead_code 警告。
- `cargo test -p vulcan-agent-runtime subagent`：11 个测试通过；仅保留既有测试 dead_code 警告。
- `cargo test -p vulcan-agent-runtime runtime_flow`：16 个测试通过；仅保留既有测试 dead_code 警告。
- `cargo check -p vulcan-service-kernel`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。
- `cargo clippy -p vulcan-agent-runtime --tests --no-deps -- -D warnings`：未作为通过条件。当前包和其路径依赖存在大量既有 clippy 债务，包括大 enum、过多参数、旧 doc 空行、collapsible-if 等，和本轮子智能体终态一致性修复无直接因果关系。

### 审核结论

本轮自动审核先发现了一个二阶问题：虽然 mailbox 已经延后，但内存 `sessions` 仍可能在持久化失败前被写入，从而留下幽灵智能体。已进一步修复为四段提交顺序。随后又发现运行中注入失败恢复 inbox 的诊断被丢弃，已补充显式日志。当前聚焦范围内未发现新的阻塞问题。

## 2026-07-04 第 3 轮：子智能体导航投影隐藏 IO 与启动期恢复收束

### 本轮目标

继续排查会话切换和重启后未关闭子智能体列表出现延迟、数量不稳定的问题，要求把“磁盘恢复”和“界面投影”彻底分层，避免普通导航计算偷偷触发分段 TaskSession 扫描。

### 问题与执行流分析

修复前的关键路径是：

1. `subagent_navigation_snapshot_from_state()` 名义上是界面投影函数，但内部会调用 `load_webui_unclosed_subagent_metadata_preload()`。
2. `load_webui_unclosed_subagent_metadata_preload()` 会访问分段 TaskSession store，并读取未关闭子智能体 metadata。
3. 这个投影函数被 `emit_webui_subagent_navigation_changed_delta()`、workspace mutation 的 `Rebuild` 分支、workspace persistence 冷刷新路径等热路径复用。
4. 因此即使当前会话已经水合过，或者当前会话根本没有子智能体，某些刷新/切换/事件投影仍可能重新扫盘。
5. 启动路径只调用了 `hydrate_selected_session_runtime_if_needed()`，没有把未关闭子智能体 metadata preload 合入服务缓存，也没有把首个 `subagent_navigation` 放进启动完成后的初始 workbench。

这个结构解释了用户观察到的两类现象：一是切换会话后子智能体列表有时先显示 1 个、等一会儿才变成 3 个；二是短会话在“定位会话”阶段也会明显变慢，因为普通投影混入了恢复 IO。

### 核心修复

1. `subagent_navigation_snapshot_from_state()` 改为纯缓存投影，不再调用分段 TaskSession 读取逻辑。
2. 删除导航投影层私有的 `refresh_webui_subagent_tree_task_cache_from_segmented_store()`，避免后续把扫盘逻辑重新塞回投影路径。
3. `SubagentNavigationRefreshMode::Rebuild` 的说明改为“从已经水合完成的运行时缓存重建”，不再描述为从分段状态重建。
4. 启动期 workspace 阶段在 ready 前显式执行 `load_webui_unclosed_subagent_metadata_preload()`，并通过 `apply_webui_unclosed_subagent_metadata_preload()` 合并进服务缓存。
5. 启动期首个 `WorkbenchSnapshot` 现在会携带已恢复的 `subagent_navigation`，避免进入工作台后再等待某次异步刷新才出现未关闭智能体列表。
6. 启动文案与日志补充未关闭智能体索引语义，并记录启动期恢复数量。
7. 调整导航测试，明确断言：未执行显式 preload 前，缓存投影不能从磁盘“发现”持久化子智能体；显式 preload 后才可见。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/workbench_surface_projection.rs`
- 修改：`crates/vulcan-service-kernel/src/service_bootstrap.rs`
- 修改：`crates/vulcan-service-kernel/src/service_bootstrap/startup_updates.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`

### 验证记录

- `cargo fmt`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：16 个测试通过。
- `cargo test -p vulcan-service-kernel detached_subagent -- --nocapture`：21 个测试通过。
- `cargo test -p vulcan-service-kernel mailbox -- --nocapture`：18 个测试通过。
- `cargo test -p vulcan-service-kernel subagent_panel -- --nocapture`：3 个测试通过。
- `cargo test -p vulcan-agent-runtime subagent -- --nocapture`：11 个测试通过；仅保留既有 dead_code 警告。
- `cargo test -p vulcan-agent-runtime runtime_flow -- --nocapture`：16 个测试通过；仅保留既有 dead_code 警告。
- `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`：12 个测试通过；仅保留既有 dead_code 警告。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮自动审核发现两处二阶问题并已修复：第一，导航投影虽然去掉了隐藏 IO，但启动期尚未显式恢复未关闭子智能体 metadata，导致首次 workbench 仍可能缺少子智能体列表；第二，启动步骤 ready 文案仍停留在旧的“项目与会话索引”语义。当前已把恢复入口收束到启动期和显式会话水合，热路径投影只读缓存；聚焦范围内未发现新的阻塞问题。

## 2026-07-04 第 4 轮：直属父级 TaskSession 元数据加载去全量分组

### 本轮目标

继续审计子智能体面板、会话切换和未关闭智能体恢复链路中的存储访问成本，修复“只查一个父级，却构建全父级分组表”的热路径坏味道，并在审核阶段补齐暴露出来的子智能体 sidecar 策略解析回归。

### 问题与执行流分析

本轮定位到的主问题在 TaskSession 存储层：

1. `subagent_task_panel_states_from_segmented_task_session_store()` 只需要某一个父级会话的直属子智能体状态。
2. 它调用 `SegmentedTaskSessionStore::list_child_task_sessions(parent)`。
3. 旧实现为了拿一个父级的结果，会先调用 `load_task_session_metadata_by_parent()` 构建所有父级到子任务的 `BTreeMap`。
4. 构建完成后再从 map 中 `remove(&parent)`，其余父级数据全部被丢弃。
5. `find_task_session_by_parent_and_public_id()` 也直接全量扫描所有 TaskSession 文档，再在内存中过滤目标父级和公开任务名。

这个结构把两类不同需求混在一起：启动/树恢复需要“全父级分组”，面板与 public id 查找只需要“一个父级的直属子项”。长期上应该把这两个读取形态拆开，避免会话切换和子智能体面板恢复时产生不必要的全局分组开销。

审核阶段还发现一个独立但相关的 catalog 问题：

1. `SubagentToolPolicySidecar` 和 `SubagentPromptSkillPolicySidecar` 使用 `#[serde(untagged)]` 同时支持新版对象和旧版三态 map。
2. 新版对象没有 `deny_unknown_fields`，因此旧版 `toolPolicy: { shell_command: allow }` 会先被现代对象分支吞掉。
3. 未知字段被忽略后得到一个空的 `All` 策略，导致 sidecar 工具策略无声丢失。
4. 完整 `vulcan-task-session` 测试因此失败，说明这是实际解析回归，不是测试噪音。

### 核心修复

1. 新增 `load_task_session_metadata_for_parent(parent)`，专门用于单父级直属子任务元数据读取。
2. `list_child_task_sessions(parent)` 改为直接调用单父级 helper，不再构建全父级 `BTreeMap`。
3. `find_task_session_by_parent_and_public_id()` 改为复用单父级 helper，并保留“未关闭记录优先、更新时间倒序、创建时间倒序、task id 倒序”的选择语义。
4. 保留 `load_task_session_metadata_by_parent()` 作为全树/启动恢复专用读取形态，避免把全局索引需求和热路径查询再次耦合。
5. 为单父级读取新增测试，覆盖直属父级与嵌套父级互不串线。
6. 为全父级分组保留测试，确保嵌套子智能体仍按直接父级归组。
7. 为新版 sidecar policy 对象增加 `deny_unknown_fields`，让旧版 map 不会误入现代空对象分支。
8. 调整 sidecar 测试为符合长期“仅允许/仅禁止二选一”模型的单模式策略，并新增旧版 deny-only map 测试。

### 文件变更

- 修改：`crates/vulcan-task-session/src/segmented_store/store_impl.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/traits.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/tests.rs`
- 修改：`crates/vulcan-task-session/src/catalog.rs`

### 验证记录

- `cargo fmt`：通过。
- `cargo test -p vulcan-task-session load_task_session_metadata_for_parent_returns_only_direct_children -- --nocapture`：通过。
- `cargo test -p vulcan-task-session load_task_session_metadata_by_parent_groups_direct_and_nested_children -- --nocapture`：通过。
- `cargo test -p vulcan-task-session sidecar_overrides_markdown_metadata_for_categorized_definition -- --nocapture`：通过。
- `cargo test -p vulcan-task-session sidecar_legacy_deny_tool_policy_maps_to_deny_only -- --nocapture`：通过。
- `cargo test -p vulcan-task-session`：28 个测试通过。
- `cargo check -p vulcan-task-session`：通过。
- `cargo check -p vulcan-workbench-model`：通过。
- `cargo test -p vulcan-workbench-model subagent -- --nocapture`：26 个测试通过。
- `cargo test -p vulcan-agent-runtime invoke_spawn_agent_reuses_public_id_after_close_agent -- --nocapture`：通过；仅保留既有测试 dead_code 警告。
- `cargo test -p vulcan-agent-runtime invoke_spawn_agent_rejects_duplicate_public_id -- --nocapture`：通过；仅保留既有测试 dead_code 警告。
- `cargo test -p vulcan-agent-runtime resolve_task_session_id_reuses_existing_public_task_identifier -- --nocapture`：通过；仅保留既有测试 dead_code 警告。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮自动审核先发现 `vulcan-task-session` 完整测试失败，根因是 `serde(untagged)` 的现代 sidecar policy 对象过度宽松，导致旧版 map 被解析成空策略。已通过严格字段解析和补充 deny-only 用例修复。当前聚焦范围内，单父级查询与全父级分组的职责已经拆开，未发现新的阻塞问题。

## 2026-07-04 第 5 轮：子智能体生命周期注册表 panic 去除

### 本轮目标

继续审计子智能体运行时生命周期链路，重点处理生产路径中通过 `expect` 假定注册表锁永远可用的问题。目标是让子智能体调度、分离面板事件流、活动运行登记、后端桥接和执行上下文桥接在异常状态下返回明确诊断或降级日志，而不是直接让运行时 panic。

### 问题与执行流分析

本轮定位到的核心路径如下：

1. `run_task_with_shared_runtime_context()` 在派生子智能体前会先打开分离事件流，用于右侧子智能体面板接收实时事件。
2. 之后它发布 `DelegateStarted`，持久化 running 状态，创建子控制器任务，再把 abort handle 安装进 active-run 注册表。
3. 主 provider 流启动时会调用 `install_execution_context()` 注册父会话共享执行上下文，子智能体后端则依赖 `install_backend()` 把父会话和任务执行 handler 关联起来。
4. 旧实现中这些注册表大量使用 `.expect("... lock")`，包括 backend、execution context、active run、detached stream 等生命周期核心锁。
5. 一旦某个锁被污染，子智能体创建/续跑/面板监听会直接 panic；更糟的是，如果 panic 出现在子任务创建后、active-run guard 安装前，可能造成外层只看到调度卡死或状态不同步。

这些注册表是子智能体生命周期控制面，不应该把“锁污染”表现成宿主崩溃。长期语义应当是：关键安装路径返回 `Result` 并阻止后续运行；非关键通知/清理路径记录日志并安全返回。

### 核心修复

1. `open_detached_task_event_stream_for_run()` 改为返回 `Result<(), String>`，打开分离事件流失败时让 `spawn_agent` 调用链直接得到诊断错误。
2. `install_active_task_run()` 改为返回 `Result<ActiveTaskRunGuard, String>`，active-run 注册失败时不再 panic。
3. `run_task_with_shared_runtime_context()` 在 active-run 安装失败时通过 `?` 返回，已创建的 `AbortChildRunOnDrop` 会自动中止子控制器任务，避免后台泄漏。
4. `subscribe_detached_task_events()` 在分离事件流注册表锁污染时返回已关闭空流，并写入日志，避免 UI 监听路径 panic。
5. `publish_detached_task_event()` 与 `close_detached_task_event_stream()` 在锁污染时记录日志并返回，避免事件投递/关闭阶段崩溃。
6. `install_backend()` 改为返回 `Result<TaskBackendGuard, String>`，provider 流无法安装子智能体后端时直接把错误写回流并退出。
7. `install_execution_context()` 改为返回 `Result<TaskExecutionRuntimeContextGuard, String>`，主运行流无法安装共享执行上下文时走统一失败终态事件。
8. `backend_for_session()`、`remove_backend()`、`execution_context_for_session()`、`remove_execution_context()` 从 panic 改为日志降级。
9. 测试调用点全部显式 `expect` 安装成功，避免新 `Result` 被静默忽略。
10. 新增锁污染测试，覆盖 active-run、detached stream、backend、execution context 四类生命周期注册表。

### 文件变更

- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_state.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/provider_transport_stream_runtime.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_driver_loop.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/preparation_config.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`

### 验证记录

- `cargo fmt`：通过。
- `cargo check -p vulcan-agent-runtime`：通过。
- `cargo test -p vulcan-agent-runtime runtime_flow -- --nocapture`：19 个测试通过；锁污染用例会输出被 `catch_unwind` 捕获的测试 panic 文本，属于人为污染锁的测试噪声。
- `cargo test -p vulcan-agent-runtime preparation_config -- --nocapture`：18 个测试通过；同样包含一条被捕获的锁污染测试 panic 文本。
- `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`：12 个测试通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `rg -n "\.expect\(" crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/lifecycle_methods.rs crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_state.rs -S`：无结果，目标生产文件已无相关 `expect`。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮自动审核重点检查了新增错误路径是否会造成子控制器任务泄漏、provider 流无终态、UI 监听卡死或测试遗漏。当前结论是：关键注册失败会显式返回错误；active-run 安装失败时已创建的子控制器由 drop guard 自动中止；主运行流 context 安装失败会构建统一失败终态；非关键事件投递和清理路径只日志降级。聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 1 轮：停止态子智能体快照恢复错误显式化

### 本轮目标

继续审计子智能体停止、取消和持久化恢复链路，修复“持久化上下文缺失”和“持久化上下文读取失败”被压成同一个 `None` 的坏味道。目标是让停止态子智能体快照恢复能够区分正常缺失、存储读取失败、上下文 JSON 损坏，避免静默把子智能体历史降级为空快照。

### 问题与执行流分析

本轮定位到的关键路径在 `run_task_with_shared_runtime_context()` 的停止/取消分支：

1. 子智能体正常启动后会先持久化 running 状态，并在运行过程中通过分段 TaskSession store 保存上下文。
2. 当子智能体被用户停止、应用关闭或 Tokio 任务取消时，运行时会构造一条 synthetic stopped outcome。
3. 为了避免丢失子智能体已经产生的上下文，旧逻辑会调用 `current_task_snapshot_from_store()` 尝试从 TaskSession store 恢复最新快照。
4. 旧实现中 `task_session_store_for_root()`、`store.load_context()`、`serde_json::from_value()` 都通过 `.ok()?` 折叠成 `None`。
5. 因此“没有配置 store / 没有 context”与“store 读取失败 / context 损坏”完全不可区分，调用方只能静默回退到 `request.snapshot`。

这个结构会掩盖真正的数据损坏：停止态看似成功记录，但实际可能丢掉了子智能体运行中的上下文，而且日志和测试都看不到原因。

### 核心修复

1. `current_task_snapshot_from_store()` 返回类型从 `Option<CoreConversationSnapshot>` 改为 `Result<Option<CoreConversationSnapshot>, String>`。
2. 未配置 TaskSession store 或不存在 context 时返回 `Ok(None)`，表示允许的正常缺失。
3. store 根目录查询失败、context 读取失败、conversation_state 反序列化失败时返回 `Err(String)`，保留明确诊断。
4. 新增 `stopped_task_snapshot_from_store_or_request()`，停止/取消分支统一通过它读取快照。
5. 停止/取消分支遇到损坏持久化上下文时会写入明确日志，再回退到 request 初始快照，避免停止动作本身被损坏上下文阻塞。
6. 补充测试覆盖三种状态：成功读取持久化快照、未配置 store 返回 `Ok(None)`、损坏 conversation_state 返回 `Err`。

### 文件变更

- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/persistence_store.rs`

### 验证记录

- `cargo fmt`：通过。
- `cargo check -p vulcan-agent-runtime`：通过。
- `cargo test -p vulcan-agent-runtime persistence_store -- --nocapture`：14 个测试通过；新增 2 个快照恢复错误语义测试。
- `cargo test -p vulcan-agent-runtime runtime_flow -- --nocapture`：19 个测试通过；锁污染用例仍会输出被 `catch_unwind` 捕获的测试 panic 文本，属于既有测试噪声。
- `cargo check -p vulcan-service-kernel`：通过。
- `rg -n "current_task_snapshot_from_store\(|stopped_task_snapshot_from_store_or_request\(" crates/vulcan-agent-runtime/src -S`：确认生产调用点只剩停止/取消 helper，测试调用点已同步新返回类型。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮自动审核重点检查了新 `Result<Option<_>>` 语义是否被调用方重新吞掉。当前结论是：允许缺失仍是 `Ok(None)`，真实读取或反序列化错误会保留诊断；停止/取消路径在日志中暴露损坏上下文，同时仍能完成停止状态记录。聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 2 轮：会话运行内核入口 panic 去除

### 本轮目标

继续审计 WebUI 会话与子智能体恢复链路，重点处理会话运行内核首次访问入口中的“插入后再 `expect`”坏味道。目标是让会话内核按需恢复时不依赖 panic 断言，同时保护已有实时内核不会被持久化旧快照覆盖。

### 问题与执行流分析

本轮定位到的关键路径在 `session_runtime_kernel_mut()`：

1. WebUI 主会话、子智能体 mailbox、wait_agent 协调和父回合绑定都会通过这个 helper 获取 `SessionRuntimeKernel`。
2. 如果内存中没有对应内核，旧实现会从 `workspace_snapshot.session_runtime_state` 或 `archived_session_runtime_state` 恢复持久化内核。
3. 旧实现恢复后通过 `insert()` 写入 `session_runtime_kernels`，随后再调用 `get_mut().expect("session runtime kernel should exist after insertion")`。
4. 这个断言在正常 `BTreeMap` 语义下理论上成立，但它把一个局部 map 操作不变量变成了生产路径 panic；一旦未来改动触发异常，UI 会话/子智能体初始化链路会直接崩。
5. 直接改成 `Entry` 单分支返回会触发 Rust 借用检查限制，因为函数需要返回 `&mut SessionRuntimeKernel`，同时缺失分支还要借用 `state` 读取工作区快照。

长期语义应当是：先判断是否需要恢复，只有缺失时才构造恢复内核；随后由 `Entry` 完成“已有返回、缺失插入”。已有实时内核必须优先于持久化快照，避免切换会话或恢复后被旧状态覆盖。

### 核心修复

1. `session_runtime_kernel_mut()` 改为两阶段入口：
   - 先通过 `contains_key()` 判断内存内核是否存在。
   - 仅在缺失时调用 `restored_webui_session_runtime_kernel()` 构造恢复内核。
   - 再通过 `BTreeMap::entry()` 返回已有内核或插入恢复内核。
2. 删除生产路径中的 `get_mut().expect("session runtime kernel should exist after insertion")`。
3. 对理论上不应出现的“预检命中但 Entry 为空”分支写入 `session_kernel_entry_invariant_repaired` trace，并创建空内核保证 UI 初始化链路不崩。
4. 新增测试 `session_runtime_kernel_mut_preserves_existing_live_kernel()`，覆盖“内存已有实时内核、工作区也有旧持久化内核”时必须保留实时内核，不得重新恢复旧 mailbox。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/session_runtime_kernel.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/session_kernel_tests.rs`

### 验证记录

- `cargo fmt`：通过。
- `cargo test -p vulcan-service-kernel session_runtime_kernel_mut_preserves_existing_live_kernel -- --nocapture`：通过。
- `cargo test -p vulcan-service-kernel session_kernel -- --nocapture`：12 个测试通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。
- `rg -n "session_runtime_kernel should exist after insertion|session_runtime_kernel_mut_preserves_existing_live_kernel" crates/vulcan-service-kernel/src -S`：确认旧 panic 文案已移除，新保护性测试存在。

### 审核结论

本轮自动审核重点检查了三个方面：入口是否还可能 panic、是否会重复恢复已有实时内核、以及 mailbox 恢复测试是否仍通过。当前结论是：`session_runtime_kernel_mut()` 不再依赖插入后断言；已有实时内核会优先保留；持久化 mailbox 未消费/已消费两类恢复语义均保持通过。聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 3 轮：子智能体父卡片 metadata 合并失败显式化

### 本轮目标

继续审计子智能体事件回灌与父会话 task 卡片同步链路，处理父级工具卡片 metadata 合并失败被静默丢弃的问题。目标是让“子智能体已经完成，但父卡片状态/工具输出没有同步”的异常能在运行 trace 中明确暴露，同时用测试固定成功更新与缺失目标两类边界。

### 问题与执行流分析

本轮定位到的关键路径如下：

1. 子智能体运行时产生 `DelegateStarted` / `DelegateFinished` 后，会进入 `ingest_webui_subagent_runtime_event()`。
2. ingest 阶段会调用 `attach_webui_task_session_metadata_to_parent_tool_message()`，尝试把 task-session payload 反挂到父会话可见的 `spawn_agent` 工具卡片。
3. 该函数先从 metadata 中解析 task-session payload，再通过 `find_webui_task_session_tool_message_ids_for_payload()` 定位父会话卡片消息。
4. 旧实现中如果找不到父卡片，直接 `return`；如果找到了但 `merge_webui_task_session_tool_message_metadata()` 返回 `false`，调用点使用 `let _ = ...` 静默丢弃。
5. 停止投影路径 `mark_webui_subagent_task_stopped_in_workspace()` 也同样通过 `let _ = ...` 丢弃 merge 结果。

这条链路失败时不会影响子智能体自身完成，也不一定阻止 mailbox 注入，所以用户会看到“子代理结果后来到了，但父级状态、右侧过程或工具卡片不同步”的割裂现象。长期语义应当是：父卡片镜像失败不能中断事件 ingest，但必须留下带 parent/message/task/status 的诊断。

### 核心修复

1. `merge_webui_task_session_tool_message_metadata()` 返回类型从 `bool` 改为 `Result<(), String>`。
2. 合并结果为空时返回明确错误，包含父会话和消息标识。
3. 找不到需要更新的规范记录或可见消息时返回明确错误，不再用 `false` 表示内部不变量失败。
4. `attach_webui_task_session_metadata_to_parent_tool_message()` 在找不到父卡片时写入 `subagent_parent_tool_metadata_target_missing` trace。
5. `attach_webui_task_session_metadata_to_parent_tool_message()` 在 merge 失败时写入 `subagent_parent_tool_metadata_merge_failed` trace。
6. `mark_webui_subagent_task_stopped_in_workspace()` 在停止态父卡片 merge 失败时写入 `subagent_stop_parent_tool_metadata_merge_failed` trace。
7. 新增测试覆盖：
   - merge 成功时父级可见 task 卡片 `display_state` 与 payload `execution_status` 会更新。
   - 父卡片消息缺失时 merge 返回显式错误，错误中包含目标 message id。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/subagent_event_ingest.rs`
- 修改：`crates/vulcan-service-kernel/src/task_mirror_status_projection.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`

### 验证记录

- `cargo fmt`：通过。
- `cargo test -p vulcan-service-kernel task_session_tool_metadata_merge -- --nocapture`：2 个测试通过。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：18 个测试通过。
- `cargo test -p vulcan-service-kernel mailbox -- --nocapture`：18 个测试通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `rg -n "let _ = .*merge_webui_task_session_tool_message_metadata|merge_webui_task_session_tool_message_metadata\(" crates/vulcan-service-kernel/src --glob '!**/tests/**' -S`：确认生产调用点均显式处理 `Result`。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮自动审核重点检查了父卡片目标缺失、merge 失败、停止投影和 mailbox 回灌四条路径。当前结论是：父卡片镜像失败不再静默；失败不会中断子智能体事件 ingest；可见卡片更新和缺失目标错误均有测试覆盖。聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 4 轮：父级实时流关闭时禁止派生孤儿子智能体

### 本轮目标

继续审计子智能体启动链路，处理 `DelegateStarted` 投递到父级实时流失败时仍继续派生后台子任务的问题。目标是让父会话无法接收启动事件时不创建孤儿运行、不留下分离事件流、不占用公开子智能体名称。

### 问题与执行流分析

本轮定位到的关键路径在 `run_task_with_shared_runtime_context()`：

1. 子智能体启动前先构造 running payload，并调用 `open_detached_task_event_stream_for_run()` 打开分离事件流。
2. 随后构造 `DelegateStarted` runtime event，并附加 UI 投影确认屏障。
3. 旧实现通过 `let _ = parent_live_sender.send(...)` 静默丢弃父级实时流发送失败。
4. 如果父级 live receiver 已关闭，后续仍会继续等待/跳过投影屏障、写入 running 记录、派生后台 child run、安装 active run。
5. 这会产生“运行时里存在子智能体，但父会话没有可靠启动卡片和投影事件”的分裂状态，后续 UI 只能依赖轮询补救。
6. 进一步审计发现，`invoke_spawn_agent()` / `invoke_task()` 在 backend 返回错误时会调用 `mark_task_failure()`。而 `prepare_task_execution_request()` 对新建任务会先写入 `created` 占位；如果父流关闭错误被当成普通失败处理，就会把未启动的任务持久化成 failed，并阻塞同名重试。

长期语义应当是：父流无法接收启动事件时，子智能体没有真正启动；该场景属于启动前取消，不应派生后台运行，不应持久化 failed 子代理，也不应占用 public id。

### 核心修复

1. 新增 `cancel_runtime_event_projection_barrier()`，用于撤销永远不会被 UI 确认的 runtime-event 投影屏障。
2. `run_task_with_shared_runtime_context()` 显式检查 `parent_live_sender.send()` 结果：
   - 发送失败时撤销投影屏障。
   - 关闭刚打开的 detached task event stream。
   - 返回统一的启动前取消错误。
   - 不再继续写 running 记录、派生后台任务或安装 active run。
3. 新增启动前取消错误标记与辅助函数：
   - `task_start_parent_stream_closed_error()`
   - `task_execution_error_is_prestart_cancelled()`
   - `task_execution_visible_error()`
4. `invoke_task()` 与 `invoke_spawn_agent()` 统一通过 `handle_prestart_cancelled_backend_error()` 处理启动前取消：
   - 对外返回去除内部标记后的友好错误。
   - 调用 `discard_prestarted_task_record()` 清理尚未启动的 `created` 占位。
   - 跳过 `mark_task_failure()`，避免写入 failed 记录和 mailbox 失败事件。
5. 新增 `discard_prestarted_task_record()`，只在 task_id、public_task_id、root、parent 与 `created` 状态全部匹配时删除占位，避免误删已运行或终态子智能体。
6. 新增测试覆盖：
   - 父 live stream 已关闭时 handler 返回错误，不留下 active run、detached stream 或 synthetic session。
   - `spawn_agent` 启动前取消不会持久化 failed 记录，不注册 agent entry，且同名 public id 可立即重试成功。

### 文件变更

- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_execution.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/parent_events.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/invocation_methods.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/persistence_methods.rs`
- 修改：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs`

### 验证记录

- `cargo fmt`：通过。
- `cargo test -p vulcan-agent-runtime runtime_flow -- --nocapture`：21 个测试通过；锁污染用例仍会输出被 `catch_unwind` 捕获的测试 panic 文本，属于既有测试噪声。
- `cargo check -p vulcan-agent-runtime`：通过。
- `rg -n "let _ = parent_live_sender\.send|parent_live_sender\.send\(|handle_prestart_cancelled_backend_error|task_execution_error_is_prestart_cancelled|discard_prestarted_task_record" crates/vulcan-agent-runtime/src/agent_runtime/task_runtime crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs -S`：确认父流发送不再静默丢弃，启动前取消处理集中在统一 helper。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮自动审核重点检查了父流关闭、投影屏障清理、detached stream 清理、启动前占位清理、同名重试和普通失败落库边界。当前结论是：父级无法接收 `DelegateStarted` 时不会产生孤儿子智能体；启动前取消不会污染子智能体注册表或阻塞 public id 重试；普通 backend 失败仍按原语义落库为 failed。聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 5 轮：会话水合与未关闭智能体索引恢复对齐

### 本轮目标

继续审计子智能体列表恢复与会话水合链路，处理“切换会话后主会话先显示、未关闭智能体列表延迟出现或缺失”的问题。同时优化小会话仍在“正在定位会话”阶段等待数秒的路径，避免正文水合无条件重读完整 workspace index。

### 问题与执行流分析

本轮确认到的执行流如下：

1. 启动路径会对选中会话执行 `load_webui_unclosed_subagent_metadata_preload()`，因此冷启动初始会话理论上可以恢复未关闭智能体索引。
2. 完整正文水合路径 `load_hydrated_session_workbench()` 也会在构建 workbench 前预载未关闭智能体 metadata。
3. 但 `service_select_conversation()` 只切换 `selected_session`，随后直接用 `subagent_navigation_snapshot_from_cached_state()` 从内存缓存投影导航。若内存缓存没有目标会话的智能体索引，返回 workbench 时列表为空，后续只能靠别的路径补齐。
4. `refresh_workbench_snapshot()` 同样只做选中会话 runtime 水合和任务镜像状态应用，没有在构建 workbench 前同步未关闭智能体 metadata，导致刷新类操作可能把导航投影回空。
5. 前端 `useConversationHydration()` 看到 `selectedConversationHydrated` 为真时会跳过 `service_session_messages()`，所以“会话正文已保留但智能体索引未预载”的状态会长期留在 UI。
6. 深入审核后又发现 `retained_hydrated_session_messages_response()` 的已保留窗口直接返回分支也只读缓存，不会执行 metadata 预载。
7. `load_hydrated_session_workbench()` 还会无条件调用 `load_workspace_index_snapshot()`，即使服务内存中已经有包含目标会话的 workspace snapshot，也要重读完整工作区索引，放大了小会话定位阶段耗时。

长期语义应当是：任何会构建父会话 workbench 的入口，都必须在投影 `subagent_navigation` 前完成轻量未关闭智能体 metadata 预载；正文水合应优先复用保留 workspace snapshot，只在目标会话不存在时回退完整索引读取。

### 核心修复

1. 新增 `preload_webui_unclosed_subagent_metadata_into_state()`，统一执行“锁外读取 TaskSession metadata、锁内合并 WebUI 子智能体缓存”的预载流程。
2. `service_select_conversation()` 在构建 workbench 前调用共享预载入口，并记录 `select_conversation_subagent_preload` 慢 trace。
3. `refresh_workbench_snapshot()` 在选中会话 runtime 水合和任务镜像状态应用后调用共享预载入口，并记录 `refresh_workbench_subagent_preload` 慢 trace。
4. `retained_hydrated_session_messages_response()` 改为两段锁：第一段确认保留窗口并复制 workspace 用于锁外预载，第二段重新读取当前会话消息并写回 workbench，避免已水合直返分支漏掉智能体索引。
5. `load_hydrated_session_workbench()` 新增保留 workspace snapshot 复用逻辑。目标会话已存在于内存 snapshot 时，不再重读完整 workspace index。
6. 新增 `retained_workspace_snapshot_for_session_hydration()` 作为可测试 helper，明确“只有已知常规会话才能复用保留快照”的边界。
7. 新增测试覆盖共享状态预载恢复多个未关闭智能体、正文水合保留快照复用边界。

### 文件变更

- 新增并完成：`docs/completed/20260705/01-SUBAGENT_HYDRATION_RESTORE_ALIGNMENT.md`
- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/workbench_refresh.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/prompt_runtime_state_tests.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`

### 验证记录

- `cargo fmt`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：19 个测试通过。
- `cargo test -p vulcan-service-kernel mailbox -- --nocapture`：18 个测试通过。
- `cargo test -p vulcan-service-kernel session_kernel -- --nocapture`：12 个测试通过。
- `cargo test -p vulcan-service-kernel prompt_runtime_state -- --nocapture`：14 个测试通过。
- `cargo test -p vulcan-service-kernel retained_workspace_snapshot_for_hydration -- --nocapture`：1 个测试通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮循环审核先后发现并修复了两个遗漏：已保留窗口直返分支未预载智能体索引，以及两段锁之间可能复用旧会话消息。最终复查确认：会话选择、完整正文水合、保留窗口直返、普通 workbench refresh 均会在导航投影前补齐未关闭智能体 metadata；导航投影仍保持只读内存缓存；正文水合在可复用保留 workspace snapshot 时不再重读完整索引。聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 6 轮：会话选择刷新目标保持

### 本轮目标

继续审计会话切换与子智能体恢复链路，处理 stale 内存工作区快照下点击目标会话后，刷新回退可能选回默认会话或旧会话的问题。

### 问题与执行流分析

本轮确认到的执行流如下：

1. `service_select_conversation()` 会先读取内存中的 `workspace_snapshot.sessions` 判断目标会话是否存在。
2. 当内存快照过期且目标会话不在当前 `sessions` 中时，旧代码直接调用 `refresh_workbench_snapshot()`。
3. 普通 `refresh_workbench_snapshot()` 会重新加载 workspace index，但没有携带“用户刚刚点击的目标会话”这一语义。
4. `load_workspace_index_snapshot()` 生成的 `selected_session` 可能是默认最新会话，`preserve_active_session_run_state()` 只能保留旧选中会话，也不能表达新的选择目标。
5. 第一轮修复后，循环审核继续发现刷新函数后半段还有第二次 `preserve_active_session_run_state()`，它可能把已经设置好的目标会话再次覆盖成旧选中会话。

长期语义应当是：普通刷新保持无目标行为；由“选择会话”触发的刷新必须携带目标会话，并在每次运行态保留后重新应用目标选择规则，只要目标仍在可见会话列表中，就不能被默认索引或旧内存选中会话覆盖。

### 核心修复

1. 新增 `refresh_workbench_snapshot_for_selected_session()`，专门承载选择会话触发的目标刷新。
2. `refresh_workbench_snapshot()` 保持原签名，内部委托给无目标刷新逻辑。
3. 新增 `selected_session_id_after_refresh()`，统一以 `WorkspaceSnapshot.sessions` 判定目标是否仍可见。
4. `service_select_conversation()` 的缺失回退路径改为目标刷新入口。
5. 在刷新函数第二次运行态保留之后再次应用目标选择规则，防止旧选中会话覆盖用户请求。
6. 新增测试覆盖目标存在、目标不可见、二次运行态保留覆盖后恢复目标三个场景。

### 文件变更

- 新增并完成：`docs/completed/20260705/02-SELECT_CONVERSATION_REFRESH_TARGET.md`
- 修改：`crates/vulcan-service-kernel/src/workbench_refresh.rs`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/prompt_runtime_state_tests.rs`

### 验证记录

- `cargo fmt`：通过。
- `cargo test -p vulcan-service-kernel targeted_refresh_selection -- --nocapture`：3 个测试通过。
- `cargo test -p vulcan-service-kernel retained_workspace_snapshot_for_hydration_requires_known_session -- --nocapture`：1 个测试通过。
- `cargo test -p vulcan-service-kernel prompt_runtime_state -- --nocapture`：17 个测试通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮循环审核先发现选择会话回退缺失目标语义，随后又发现二次运行态保留会覆盖目标选择。两个问题均已按同一目标选择 helper 修复。最终复查确认：只有 `service_select_conversation()` 的缺失回退路径使用带目标刷新；普通项目、技能、记忆刷新仍走无目标入口；目标会话不可见时不会强行设置非法 `selected_session`。聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 7 轮：完整会话水合目标选择保持

### 本轮目标

继续审计会话切换、正文水合和未关闭智能体导航恢复链路，处理完整会话水合过程中目标会话可能被旧内存选中会话覆盖的问题。

### 问题与执行流分析

本轮确认到的执行流如下：

1. `load_hydrated_session_workbench()` 会读取或复用 `WorkspaceSnapshot`，随后用 `preserve_active_session_run_state()` 合并当前运行态。
2. 初次运行态保留后，函数会将 `refreshed_snapshot.selected_session` 设置为用户请求的 `parsed_session_id`。
3. 加载消息、水合运行态、预载未关闭智能体 metadata 之后，函数在构建 WebUI workbench 前还会再次从当前 `guard.workspace_snapshot` 捕获运行态，并第二次调用 `preserve_active_session_run_state()`。
4. 第二次运行态保留会按旧内存快照恢复 `selected_session`；如果旧选中会话仍可见，就可能覆盖刚请求的水合目标。
5. 覆盖发生在 `active_surface_messages_from_workspace_and_state()`、`subagent_navigation_snapshot_from_cached_state()` 和 `workbench_from_runtime()` 之前，因此会导致主会话正文或子智能体导航按旧会话投影。

长期语义应当是：由“加载具体会话”触发的完整水合必须在每个运行态保留步骤之后重新应用目标选择。普通无目标刷新可以保留旧选择；明确目标水合不能被 stale 内存选中会话反向覆盖。

### 核心修复

1. 在 `load_hydrated_session_workbench()` 第二次 `preserve_active_session_run_state()` 之后，重新调用 `selected_session_id_after_refresh(&refreshed_snapshot, Some(parsed_session_id))`。
2. 复用上一轮目标选择 helper，保持“目标可见则选目标，否则回落当前快照选择”的规则唯一。
3. 新增 `hydrated_session_selection_recovers_after_late_run_state_preservation` 单元测试，模拟旧会话覆盖目标后必须恢复目标的场景。

### 文件变更

- 新增并完成：`docs/completed/20260705/03-HYDRATED_SESSION_SELECTION_TARGET.md`
- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/prompt_runtime_state_tests.rs`

### 验证记录

- `cargo fmt`：通过。
- `cargo test -p vulcan-service-kernel hydrated_session_selection_recovers_after_late_run_state_preservation -- --nocapture`：1 个测试通过。
- `cargo test -p vulcan-service-kernel prompt_runtime_state -- --nocapture`：18 个测试通过。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：19 个测试通过。
- `cargo test -p vulcan-service-kernel session_kernel -- --nocapture`：12 个测试通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮循环审核确认新增目标选择恢复发生在最后一次运行态保留之后、导航投影和 workbench 构建之前，能覆盖 stale `guard.workspace_snapshot` 把完整水合目标改回旧会话的问题。相关子智能体导航和 session kernel 测试均通过，聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 8 轮：WebUI 会话选择合并策略显式化

### 本轮目标

继续审计会话切换后 WebUI 右侧子智能体面板与主会话选择不一致的问题，处理前端合并 service snapshot 时保留旧 `selectedConversationId` 导致顶层当前会话派生字段错位的路径。

### 问题与执行流分析

本轮确认到的执行流如下：

1. 用户点击侧栏会话后，`handleSelectConversation()` 会立即更新本地 React 选中会话，并调用 `selectConversation(conversationId)`。
2. `selectConversation()` 返回的 `WorkbenchSnapshot` 已经表达了服务端确认后的目标 `selectedConversationId`。
3. WebUI 旧的 `mergeWorkbenchSnapshot()` 只要旧 `current.selectedConversationId` 仍存在于 incoming 会话列表，就继续保留旧选择。
4. `deriveSelectedConversationFields()` 根据 `workbench.selectedConversationId` 派生顶层 `subagentNavigation`、pending prompts、上下文注入等字段。
5. 因此 React 本地选中会话可能已经是新会话，但 `workbenchState.selectedConversationId` 和顶层派生字段仍保持旧会话，表现为子智能体列表切换后延迟出现、错显旧会话或需要等待后续刷新。

长期语义应当是：后台刷新默认保留本地当前会话，防止服务快照抢焦点；但用户主动选择会话后的返回快照必须采用服务端明确选择，使 `workbenchState` 和顶层派生字段立即对齐目标会话。

### 核心修复

1. `WorkbenchMergeOptions` 新增 `selectedConversationSource?: "local" | "incoming"`，显式表达快照合并时的选择来源。
2. 新增 `visibleIncomingConversationId()`，只允许仍存在于 incoming 会话列表中的候选会话进入选择字段。
3. `mergeWorkbenchSnapshot()` 默认仍使用本地可见选择优先；传入 incoming 策略时优先采用服务端快照中的合法选择。
4. `mergeWorkbenchWithLocalCaches()` 增加 options 透传，保留子智能体界面缓存逻辑不变。
5. `handleSelectConversation()` 只对 `selectConversation()` 返回快照启用 incoming 策略，后续 `markConversationRead()` 与普通后台快照仍保持默认本地优先。

### 文件变更

- 新增并完成：`docs/completed/20260705/04-WEBUI_SELECTION_MERGE_POLICY.md`
- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`TASK_LOG.md`

### 验证记录

- `pnpm --dir apps/vulcan-code-web lint`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。
- `apps/vulcan-code-web/dist`：构建检查未产生变更。

### 审核结论

本轮审核确认：只有用户主动选择会话后的 `selectConversation()` 响应采用 incoming 选择策略；全局 service snapshot、后台刷新和 mark-read 快照仍走默认本地选择优先，不会引入抢焦点行为。前端类型检查通过，聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 9 轮：分叉会话打开目标保持

### 本轮目标

继续审计 WebUI 会话导航快照合并链路，处理点击 final assistant 下方 fork 按钮后，新分叉会话可能没有成为当前会话的问题。

### 问题与执行流分析

本轮确认到的执行流如下：

1. 前端 `forkConversation()` 注释明确为“复制一个持久化会话并打开”。
2. `handleForkConversation()` 注释同样写着“创建持久化副本并打开”。
3. 后端 `service_fork_conversation()` 创建 `target_session_id` 后调用 `load_hydrated_session_workbench(... target_session_id ...)`，返回目标新会话的 hydrated workbench。
4. 前端旧代码拿到 snapshot 后只调用 `mergeWorkbenchWithLocalCaches(current, snapshot)`。
5. 默认合并策略会保留本地可见 `selectedConversationId`。源会话在 fork 后仍然可见，所以新 fork 会话会被服务端选中语义覆盖掉，表现为 fork 成功但界面仍停留在源会话。

长期语义应当是：后台刷新继续本地选择优先；但用户主动触发且服务端响应明确携带导航目标的动作，必须显式采用 incoming 选择策略。

### 核心修复

1. `WorkbenchConversationActionsOptions.mergeWorkbenchWithLocalCaches` 类型扩展为支持 `WorkbenchMergeOptions`。
2. `handleForkConversation()` 在写入本地侧栏选择前，先校验 `snapshot.selectedConversationId` 是否存在于 incoming 会话列表。
3. fork snapshot 合并时传入 `{ selectedConversationSource: "incoming" }`，确保 workbench 顶层派生字段切到新会话。
4. 补齐 `handleForkConversation()` 的 hook 依赖，避免后续合并函数变化时闭包落后。

### 文件变更

- 新增并完成：`docs/completed/20260705/05-FORK_CONVERSATION_SELECTION_POLICY.md`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-actions.ts`
- 修改：`TASK_LOG.md`

### 验证记录

- `pnpm --dir apps/vulcan-code-web lint`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。
- `rg -n "selectedConversationSource|handleForkConversation|forkConversation\\(|mergeWorkbenchWithLocalCaches\\(" apps/vulcan-code-web/src/components/workspace/workbench-layout apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx -S`：确认 incoming 策略只用于点击会话和 fork 打开新会话两个导航入口。
- `apps/vulcan-code-web/dist`：未产生构建产物变更。

### 审核结论

本轮审核确认 fork 的服务端语义、前端注释与新合并策略已经一致：fork 成功后采用服务端目标会话，但仍校验目标会话可见性；普通后台刷新、模型切换、pending prompt、历史加载等非导航动作继续默认本地选择优先。聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 10 轮：WebUI 当前会话投影统一

### 本轮目标

继续审计 WebUI 本地选中会话与 workbench 顶层派生字段的双轨状态，处理主界面只同步 `contextInjection`、其它当前会话字段仍可能沿用旧会话的问题。

### 问题与执行流分析

本轮确认到的执行流如下：

1. `WorkbenchLayout` 同时维护 React 本地 `selectedConversationId` 和 `workbenchState.selectedConversationId`。
2. `mergeWorkbenchSnapshot()` 会按 `workbench.selectedConversationId` 调用 `deriveSelectedConversationFields()`，派生 `pendingPrompts`、`sessionGoal`、`subagentNavigation` 等顶层字段。
3. `consoleWorkbenchState` 在传给 `WorkspaceShell` 前只按本地选择重算了 `contextInjection`。
4. `WorkspaceShell` 读取 `props.selectedConversationId` 判断当前界面会话，同时读取 `props.workbench.subagentNavigation`、`props.workbench.sessionTodolist`、`props.workbench.sessionGoal` 等顶层字段。
5. 因此当本地选择先于服务快照更新，或进入草稿会话时，只有 `contextInjection` 被修正，其它顶层字段仍可能短暂来自旧会话。

长期语义应当是：传给主界面的 workbench 必须始终按 UI 本地选择统一重投影当前会话字段；草稿会话或不可见会话则清空这些字段，而不是保留旧会话状态。

### 核心修复

1. 在 `workbench-state.ts` 新增 `workbenchWithSelectedConversationProjection()`。
2. 该 helper 复用现有 `deriveSelectedConversationFields()`，统一重投影 pending prompts、session goal、session todolist、VMM 策略、子智能体导航和上下文注入。
3. 将原本误导的 `visibleIncomingConversationId()` 改为通用 `visibleConversationId()`。
4. `consoleWorkbenchState` 改为传入本地选中会话调用统一 helper；草稿会话传入 `null`，清空旧派生字段。

### 文件变更

- 新增并完成：`docs/completed/20260705/06-WEBUI_SELECTED_PROJECTION_SYNC.md`
- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`TASK_LOG.md`

### 验证记录

- `pnpm --dir apps/vulcan-code-web lint`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。
- `rg -n "visibleIncomingConversationId|visibleConversationId|workbenchWithSelectedConversationProjection|consoleWorkbenchState" apps/vulcan-code-web/src/state/workbench-state.ts apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx -S`：确认旧误导命名无残留，主界面改用统一投影 helper。

### 审核结论

本轮审核确认：当前会话顶层派生字段不再在 `WorkbenchLayout` 中手写单字段同步，而是复用状态层统一投影；草稿会话和不可见会话不会继续携带旧 `subagentNavigation`、pending prompts 或 session goal。前端类型检查通过，聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 11 轮：会话选择与子智能体水合边界收敛

### 本轮目标

继续审计会话切换卡顿与未关闭子智能体恢复延迟问题，明确“选择会话”和“初始化会话正文”的职责边界，并清理当前会话上下文注入投影中的旧字段回退。

### 问题与执行流分析

本轮确认到的执行流如下：

1. 前端点击侧栏会话时先调用 `service_select_conversation`，随后再由 `useConversationHydration` 判断是否需要加载正文。
2. 当前工作树中 `service_select_conversation` 曾承担未关闭子智能体元数据预载，这会把磁盘 TaskSession 元数据扫描放进轻量选择动作。
3. 子智能体导航投影函数 `subagent_navigation_snapshot_from_cached_state` 的注释明确写着 cache-only，磁盘 IO 应属于会话水合或启动恢复阶段。
4. `load_hydrated_session_workbench` 已在加载遮罩覆盖的正文水合路径中恢复未关闭子智能体索引。
5. `deriveSelectedConversationFields` 仍然对 `contextInjection` 使用顶层旧镜像回退，和按会话归一化缓存的长期语义不一致。

长期语义应当是：选择会话只做内存快照投影；未水合会话的消息、未关闭智能体索引等重 IO 统一在正文水合路径完成；当前会话顶层字段只由按会话缓存派生。

### 核心修复

1. 确认并保持 `service_select_conversation` 不再调用未关闭子智能体磁盘预载。
2. `contextInjection` 顶层字段改为只从 `contextInjectionByConversation` 的当前会话条目派生。
3. 删除 `contextInjectionForConversation` 及其无效传参，避免后续继续误读顶层镜像字段。
4. 保留 `workbench_refresh` 和 `load_hydrated_session_workbench` 中的子智能体预载，让启动和正文水合继续负责恢复未关闭智能体。

### 文件变更

- 新增并完成：`docs/completed/20260705/07-CONVERSATION_SELECTION_SUBAGENT_HYDRATION.md`
- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `pnpm --dir apps/vulcan-code-web lint`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation --lib`：通过，19 个相关测试全部通过。
- `cargo fmt --check`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。
- 聚焦检索确认：未关闭子智能体预载只保留在 workspace refresh 与会话正文水合路径；`service_select_conversation` 不再有该重 IO。

### 审核结论

本轮审核确认选择会话路径已经回到轻量内存投影，正文水合路径仍负责未关闭智能体恢复；当前会话 `contextInjection` 不再有旧顶层字段回退。聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 12 轮：WebUI 已水合会话窗口合并策略修复

### 本轮目标

继续排查会话切换后智能体列表延迟出现、已水合会话重复显示“定位会话”、无子智能体的小会话也可能卡顿数秒的问题，重点确认前端工作台快照合并是否会丢失已水合会话状态。

### 问题与执行流分析

本轮确认到的执行流如下：

1. `WorkbenchLayout` 使用 `workbenchState.hydratedConversationWindows` 建立 `hydratedConversationWindowById`。
2. `selectedConversationHydrated` 完全依赖当前会话是否存在对应 hydrated window。
3. 当该值为 `false` 时，`useConversationHydration` 会调用 `loadConversationMessages`，触发后端正文水合与未关闭智能体元数据恢复。
4. 后端正常 `loadConversationMessages` 与工作台刷新会返回真实 `hydratedConversationWindows`。
5. fallback 工作台投影以及部分轻量快照可能带空窗口列表。
6. 前端 `mergeWorkbenchSnapshot` 保留了消息、上下文注入、子智能体导航等按会话缓存，但没有合并 `hydratedConversationWindows`，导致 `...incoming` 可以把已知窗口整体覆盖为空。

长期语义应当是：已水合会话窗口属于按会话归一化缓存，局部快照缺席该字段时不能把已知窗口清空；只有会话摘要中已不存在的会话才应剪枝。

### 核心修复

1. 在 `workbench-state.ts` 新增 `mergeHydratedConversationWindows()`。
2. `mergeWorkbenchSnapshot` 显式合并 `hydratedConversationWindows`：
   - 以 `sessionId` 为唯一键。
   - incoming 窗口覆盖当前窗口。
   - incoming 缺失窗口保留当前缓存。
   - 最终只保留仍在 incoming 会话摘要中的窗口。
3. 保持后端 `retain_hydrated_conversation_windows_for_workspace` 作为真实生命周期裁剪来源，前端只做局部快照抗丢失合并。

### 文件变更

- 新增并完成：`docs/completed/20260705/08-WEBUI_HYDRATED_WINDOW_MERGE.md`
- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 修改：`TASK_LOG.md`

### 验证记录

- `pnpm --dir apps/vulcan-code-web lint`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation --lib`：通过，19 个相关测试全部通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。
- 聚焦源码审核确认：后端不存在“发送空窗口列表要求前端清空仍可见会话窗口”的语义；窗口生命周期通过 `workspace.hydrated_session_ids` 裁剪。

### 审核结论

本轮审核确认 `hydratedConversationWindows` 已被纳入前端统一快照合并策略，避免局部快照误清空已水合窗口并反复触发正文水合。聚焦范围内未发现新的类型、状态污染或后端生命周期冲突问题。

## 2026-07-05 第 13 轮：子智能体未关闭元数据预载幂等化

### 本轮目标

继续排查会话打开和切换时未关闭子智能体列表延迟出现的问题，重点审计启动、工作台刷新、会话正文水合是否会对同一父会话重复扫描 TaskSession metadata。

### 问题与执行流分析

本轮确认到的执行流如下：

1. `load_webui_unclosed_subagent_metadata_preload` 会读取父会话目录下的 TaskSession metadata，用于恢复未关闭子智能体导航列表。
2. 同一个 root 会话的恢复结果最终会写入 `ServiceBridgeState.session_subagent_tasks`，后续导航投影理论上应只读内存缓存。
3. 启动恢复、工作台刷新、会话正文水合和已水合快路径都可能触发未关闭子智能体 metadata 恢复。
4. 旧状态里没有记录“某个 root 会话已经完成磁盘 metadata 预载”，因此同一个会话切换或刷新时可能重复扫描同一批 TaskSession 文件。
5. 已水合快路径还缺少普通父会话判断，存在子智能体详情会话误触发 root 预载标记的风险。

长期语义应当是：未关闭子智能体 metadata 的磁盘恢复是 root 会话级一次性操作；完成后由内存缓存负责投影，运行时新建、关闭、完成事件继续增量更新缓存。

### 核心修复

1. 在 `ServiceBridgeState` 新增 `webui_preloaded_unclosed_subagent_roots`，记录已完成未关闭智能体 metadata 预载的 root 会话。
2. 新增 `load_webui_unclosed_subagent_metadata_preload_once()` 与 `apply_webui_unclosed_subagent_metadata_preload_once()`，集中处理预载门控与标记写入。
3. `service_bootstrap`、`workbench_refresh`、`conversation_history` 改为共享 once 语义，避免多路径旁路重复扫描。
4. 已水合会话快路径增加普通父会话检测，避免子智能体详情会话污染 root 级预载状态。
5. 子智能体导航测试补充重复预载断言：首次恢复 3 个未关闭智能体，第二次同 root 返回 0 且列表仍完整。

### 文件变更

- 新增并完成：`docs/completed/20260705/09-SUBAGENT_METADATA_PRELOAD_ONCE.md`
- 修改：`crates/vulcan-service-kernel/src/state.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/service_bootstrap.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/workbench_refresh.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation --lib`：通过，19 个测试。
- `cargo test -p vulcan-service-kernel detached_subagent --lib`：通过，21 个测试。
- `cargo test -p vulcan-service-kernel prompt_runtime_state --lib`：通过，18 个测试。
- `cargo test -p vulcan-service-kernel --lib`：通过，165 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。
- `cargo clippy -p vulcan-service-kernel --lib --tests -- -D warnings`：被依赖 crate 的既有 clippy lint 拦截，未进入本轮 service-kernel 代码；本轮未修改这些依赖 lint。

### 审核结论

本轮审核确认未关闭子智能体 metadata 恢复已经收敛到 root 会话级 once 入口，启动、刷新、水合和快路径不再各自直接扫描。聚焦范围内没有发现运行时新建/关闭智能体被 once 门控隐藏的问题。后续应继续单独审计“无子代理的小会话仍在定位会话阶段卡顿数秒”的完整工作区索引读取路径。

## 2026-07-05 第 14 轮：子智能体元数据镜像索引快路径

### 本轮目标

继续排查会话切换、正文水合和无子智能体短会话打开时的卡顿问题，重点确认未关闭子智能体预载是否仍然依赖 `task/` 子目录全量扫描。

### 问题与执行流分析

本轮确认到的执行流如下：

1. `service_session_messages` 在未命中水合缓存时进入 `load_hydrated_session_workbench`。
2. `load_hydrated_session_workbench` 会调用 `load_webui_unclosed_subagent_metadata_preload_once`。
3. 该函数此前最终走到 `SegmentedTaskSessionStore::load_task_session_metadata_by_parent()`。
4. `load_task_session_metadata_by_parent()` 会枚举根会话 `task/` 目录下所有子目录并读取每个 `metadata.json`。
5. 根会话 `metadata.json` 已有 `tasks` 镜像，但缺少 parent/root/public task id 等足以独立恢复导航的字段。

长期语义应当是：子智能体导航列表只需要根会话级摘要，不能为了显示未关闭列表扫描所有子智能体完整 metadata；完整转录应在用户打开具体智能体面板时再读取。

### 核心修复

1. 新增 `TaskSessionMirrorSummary`，作为根 `tasks` 镜像的稳定摘要类型。
2. 扩展根任务镜像写入字段：`root_session_id`、`public_task_id`、`parent`、`last_invocation_kind`。
3. 新增 `load_task_session_mirror_summaries()` 与 `load_task_session_mirror_summaries_by_parent()`，只读取根 `metadata.json`。
4. 旧镜像缺少 parent/public/root 时，只从根会话分段里的 `task_invocation_event` 回填，不读取子任务目录。
5. WebUI 未关闭智能体预载优先走根镜像快路径，Archived 被过滤，Completed 但未关闭继续显示。
6. 根 `tasks` 镜像字段缺失且存在 `task/` 目录时，才显式 trace 并回退旧扫描，避免半迁移会话丢失智能体列表。

### 文件变更

- 新增并完成：`docs/completed/20260705/10-SUBAGENT_METADATA_MIRROR_FAST_PATH.md`
- 修改：`crates/vulcan-task-session/src/model.rs`
- 修改：`crates/vulcan-task-session/src/lib.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/mirror.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/store_impl.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/tests.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo test -p vulcan-task-session segmented_store --lib`：通过，10 个测试。
- `cargo test -p vulcan-task-session --lib`：通过，30 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation --lib`：通过，20 个测试。
- `cargo test -p vulcan-service-kernel detached_subagent --lib`：通过，21 个测试。
- `cargo test -p vulcan-service-kernel prompt_runtime_state --lib`：通过，18 个测试。
- `cargo test -p vulcan-service-kernel session_kernel --lib`：通过，12 个测试。
- `cargo test -p vulcan-service-kernel --lib`：通过，166 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮审核确认未关闭子智能体导航预载已经改为根镜像快路径：新会话不会为了导航扫描子任务目录，已有半迁移会话在缺少根镜像字段时仍能显式回退恢复列表。聚焦范围内未发现新的状态投影、Archived 过滤或嵌套 parent 分组问题。

## 2026-07-05 第 15 轮：会话选择与正文水合快速路径

### 本轮目标

继续排查“无子智能体的小会话切换也会在定位会话阶段卡数秒”以及“已恢复子智能体列表切换后短暂消失、等待后才出现”的问题，重点审计会话点击后的真实调用链。

### 问题与执行流分析

本轮确认到的执行流如下：

1. 前端点击侧栏会话时会立即本地选中，然后调用 `service_select_conversation` 同步后端 selected marker。
2. 同时，`useConversationHydration` 会根据 `hydratedConversationWindows` 判断目标会话是否需要调用 `service_session_messages`。
3. 旧的 `service_select_conversation` 并不是轻量 marker 更新，而是会重新投影 surface、重建完整 `WorkbenchSnapshot`。
4. `service_session_messages` 又会读取正文窗口、应用 hydration、恢复未关闭子智能体索引。
5. 两条路径并发返回时，前端会先收到缺少完整子智能体导航的选择快照，再收到正文水合快照，于是出现“先没有/数量少，等待后才完整”的抖动。

长期语义应当是：会话选择只改变当前选中项；正文水合才负责读取磁盘、恢复未关闭智能体和生成完整导航。

### 核心修复

1. `service_select_conversation` 普通路径改为纯内存选择：
   - 不调用 `load_workspace_index_snapshot`；
   - 不调用 `load_webui_unclosed_subagent_metadata_preload_once`；
   - 不调用 `workbench_from_runtime`；
   - 不读取 segmented 会话正文。
2. 新增 `select_conversation_in_retained_state`，仅当 workspace 和 workbench 侧栏摘要都包含目标会话时走快速路径，否则降级目标会话 refresh。
3. 新增 `retained_selected_conversation_messages`，只投影已保留正文，同时保留其他已水合会话消息。
4. 新增 `sync_selected_conversation_fields_for_retained_workbench`，同步当前会话的 pending prompt、ask、goal、todolist、plan mode、VMM policy 和 context injection。
5. 前端 `selectMergedConversationHydration` 修复跨会话旧进度保留问题：incoming 没有进度时，只保留属于当前选中会话的旧进度。
6. 新增 `conversation_selection_tests`，覆盖未水合会话不被偷偷水合、缓存子智能体导航不触发磁盘预载、hydrated window 摘要不丢失。

### 文件变更

- 新增：`docs/completed/20260705/11-SESSION_SELECTION_HYDRATION_FAST_PATH.md`
- 新增：`crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`
- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`crates/vulcan-service-kernel/src/tests.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt`：通过。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，2 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，21 个测试。
- `cargo test -p vulcan-service-kernel session_kernel -- --nocapture`：通过，12 个测试。
- `cargo test -p vulcan-service-kernel`：通过，168 个测试。
- `pnpm lint`（在 `apps/vulcan-code-web` 下执行）：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：被非本轮既有 clippy lint 拦截，未作为本轮通过项。

### 审核结论

本轮审核确认普通会话点击已经从“完整 workbench 重建”收敛为“纯内存选中投影”。子智能体恢复只留在正文水合、启动初始选中会话和显式刷新路径，避免选择快照与正文水合快照竞争导致列表抖动。聚焦范围内未发现新的正文重复读取、子智能体预载重复触发或 hydration window 丢失问题。

## 2026-07-05 第 16 轮：严格 Clippy 阻塞清理

### 本轮目标

清理 `vulcan-service-kernel` 严格 Clippy 链路暴露出的跨 crate 阻塞项，让本轮子代理/会话水合相关测试可以继续把 `-D warnings` 作为可靠闸门。

### 核心修复

1. Host tools：将 pipe/pty supervisor 与 AI 读取参数收敛为请求对象，顺手清理命令会话输出解码和模板渲染的机械 lint。
2. Workspace store：将 snapshot 持久化与 runtime injection 日志记录改成请求对象，清理会话水合分页、分段消息映射和测试中的局部 lint。
3. MCP runtime：将 SSE listener 启动参数改为请求对象，清理 bearer token 嵌套判断。
4. Agent runtime：清理 provider prompt、stream driver、task runtime、tool rendering、terminal shell、web/media 兼容层的机械 lint；对 provider/runtime 协议边界保留带原因的 `expect`。
5. Service kernel：修复错位 doc comment、状态投影、VMM hook、Git diff preview、会话水合与测试断言 lint；对 WebUI 服务边界长参数和复杂 runtime handle bundle 保留带原因的 `expect`。

### 文件变更

- 修改：`crates/vulcan-host-tools/src/command_session_runtime/**`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/**`
- 修改：`crates/vulcan-mcp-runtime/src/sse.rs`
- 修改：`crates/vulcan-agent-runtime/src/**`
- 修改：`crates/vulcan-service-kernel/src/**`
- 修改：`docs/plan/20260705-12-CLIPPY_BLOCKER_CLEANUP.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt`：通过。
- `cargo fmt --check`：通过。
- `cargo clippy -p vulcan-host-tools -p vulcan-workspace-store -p vulcan-mcp-runtime -p vulcan-agent-runtime -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-service-kernel --tests -- --nocapture`：通过，168 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

已完成三轮审核：第一轮用 `git diff --check` 和 diff 范围检查确认无空白错误；第二轮确认没有新增 `allow(clippy)`，仅保留有原因的 `expect`；第三轮重新执行 `fmt --check` 与严格 clippy，未发现新增问题。遗留的 `expect` 是显式架构债，后续应通过 `ServiceRuntimeContext` 和 provider runtime request envelope 做长期收敛。

## 2026-07-05 第 17 轮：Workbench 投影参数对象化

### 本轮目标

继续收敛上一轮留下的 `service-kernel` 长参数债务，优先处理会话切换、启动、懒水合、子代理导航和冷刷新都会经过的 `workbench_from_runtime` 投影链路。

### 执行流分析

本轮确认 `workbench_from_runtime` 有 16 个参数，调用点分布在：

1. `workbench_refresh`：普通工作台刷新。
2. `conversation_history`：懒水合会话正文后重建工作台。
3. `service_bootstrap`：启动期生成首个工作台快照。
4. `subagent_workspace_mutation`：子代理导航/工作区变更后重建工作台。
5. `workspace_persistence_versions`：冷刷新持久化后重建并广播工作台。

这些参数可以明确分为运行时能力句柄、Provider/AI 配置、工作区与 surface 消息、附属摘要四组。继续使用顺序参数容易在新增子代理/水合字段时发生错位，因此应收敛为命名字段请求对象。

### 核心修复

1. 新增 `WorkbenchRuntimeProjection<'a>`，集中承载 workbench 投影所需的运行时句柄、配置、工作区快照、surface 消息、子代理导航、pending questions、日志、上下文预算、会话目标、水合窗口和 native tools。
2. 将 `workbench_from_runtime` 改为单参数请求对象入口，函数内部解构后保持原有投影行为不变。
3. 更新 5 个调用点为命名字段构造，避免继续依赖 16 个顺序参数。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/workbench_refresh.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/service_bootstrap.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/workspace_persistence_versions.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `rg "workbench_from_runtime\\(" crates/vulcan-service-kernel/src -n`：确认仅剩 5 个命名字段调用点和函数定义。
- `cargo fmt`：通过。
- `cargo fmt --check`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-service-kernel --tests -- --nocapture`：通过，168 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮审核确认 `WorkbenchRuntimeProjection` 没有改变 projection 的数据来源和输出语义，只把顺序参数改为命名字段请求对象。调用点覆盖启动、刷新、懒水合、子代理工作区变更和冷刷新五条路径；测试与严格 clippy 通过。后续可继续沿同一方向收敛冷刷新存储函数和服务层 runtime handle bundle。

## 2026-07-05 第 18 轮：Cold Refresh 请求对象化

### 本轮目标

继续沿会话载入与子代理导航的冷路径排查服务边界坏味道，优先处理 `store_and_emit_full_workbench_snapshot_for_cold_refresh` 这一层仍然保留的 10+ 顺序参数和默认包装转发问题。

### 执行流分析

本轮确认冷刷新入口主要由两条路径调用：

1. `service_compact_selected_conversation_context`：上下文压缩进入准备态后，需要提交 workspace 并刷新工作台。
2. `service_stop_conversation_run`：停止主会话/运行中子代理后，需要提交 workspace 并刷新工作台。

原实现还保留了一个 `store_and_emit_full_workbench_snapshot_for_cold_refresh_with_subagent_navigation_mode` 包装入口，默认入口只是转发同一串参数并追加 `SubagentNavigationRefreshMode::Rebuild`。这会让“本次刷新是否重建子代理导航”隐藏在函数名和参数顺序里，不利于继续排查会话切换、子代理导航预载入和冷路径工作台提交之间的状态边界。

### 核心修复

1. 新增 `FullWorkbenchColdRefreshRequest<'a>`，集中承载冷刷新所需的 emitter、共享状态、运行时句柄、Provider/AI 工具配置、workspace snapshot、日志和子代理导航刷新模式。
2. 删除显式模式包装函数，将冷刷新收敛为单一请求对象入口。
3. 更新上下文压缩和停止会话两个调用点，改为命名字段构造，并在调用处显式传入 `SubagentNavigationRefreshMode::Rebuild`。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/workspace_persistence_versions.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_run_services.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `rg "store_and_emit_full_workbench_snapshot_for_cold_refresh_with_subagent_navigation_mode|store_and_emit_full_workbench_snapshot_for_cold_refresh\\(" crates/vulcan-service-kernel/src -n -C 1`：确认旧包装函数已无残留，仅剩两个命名字段调用点和单一函数定义。
- `cargo fmt`：通过。
- `cargo fmt --check`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-service-kernel --tests -- --nocapture`：通过，168 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮审核确认冷刷新行为未改变：仍然会在共享状态中提取 surface 消息、子代理导航、pending questions、目标摘要和命令会话运行时，再调用 `workbench_from_runtime` 构建并广播工作台。变化仅限于把隐式顺序参数和二次包装改为显式请求对象。下一轮可以继续向上排查会话切换“定位会话”耗时异常，重点比较 session hydration 和未关闭子代理预载入是否仍存在串行阻塞或重复扫描。

## 2026-07-05 第 19 轮：会话水合定位阶段瘦身

### 本轮目标

继续排查用户反馈的会话切换异常卡顿：一个只有少量 turn、没有子代理的会话，在“正在定位会话”阶段也会停留数秒；同时未关闭子智能体列表在切换后延迟出现。目标是找出服务端会话水合真实阻塞点，并按长期方式移除无关大对象克隆。

### 执行流分析

本轮确认 `service_session_messages` 的实际流程为：

1. 先尝试 `retained_hydrated_session_messages_response` fast path。
2. 未命中 fast path 时发送 5% `locating` 进度。
3. 进入 `load_hydrated_session_workbench`。
4. 旧路径在 5% 阶段调用 `capture_preserved_workspace_run_state`，该函数会因为 `had_hydrated_window` 克隆所有已水合会话的 `session_messages`。
5. 后续 retained workspace 分支还会克隆完整 `WorkspaceSnapshot`，fast path 也曾为子智能体预载和导航投影克隆完整 workspace。

结论：卡顿不一定来自当前会话，也不一定来自子代理扫描，而是来自“打开任意会话时复制所有已水合正文缓存”的耦合。一个巨大会话被水合后，会拖慢其他小会话。

### 核心修复

1. 新增 `HydratedSessionWorkbenchRuntimeContext`，将会话水合所需的运行时句柄、配置、预算、日志和监听器轻量捕获出来。
2. 新增 `PreservedSessionMessageScope` 与 `capture_preserved_workspace_run_state_for_session_hydration`，让会话正文水合只保留当前会话窗口；普通刷新仍保持原来的“保留全部已水合正文”语义。
3. 新增 `lightweight_workspace_index_snapshot_for_hydration`，用轻量 workspace 索引替代完整 `WorkspaceSnapshot` 克隆，覆盖 retained snapshot、fast path 子代理预载和 fast path 导航投影。
4. 将未关闭子智能体元数据预载纳入会话水合进度，并增加 `session_hydration_*_slow` trace，便于后续区分运行态捕获、索引读取、正文读取、子智能体预载的耗时。
5. 新增回归测试 `session_hydration_state_preserves_only_requested_transcript_window`，确认当前会话水合不会复制无关已水合正文，也不会保留无关 hydrated marker。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/session_hydration/run_state_preservation.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/active_session_state_tests.rs`
- 新增并归档：`docs/completed/20260705/01-SUBAGENT_SESSION_HYDRATION_LOADING_FIX.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt`：通过。
- `cargo fmt --check`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-service-kernel session_hydration_state_preserves_only_requested_transcript_window -- --nocapture`：通过。
- `cargo test -p vulcan-service-kernel --tests -- --nocapture`：通过，169 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮审核确认定位阶段已经不再复制所有已水合正文；主水合 retained snapshot 与 fast path 子代理预载也不再使用完整 workspace 克隆。已有普通刷新保留全部正文的行为由 `PreservedSessionMessageScope::AllHydrated` 继续承担，避免误伤其他刷新路径。后续如果仍有延迟，应优先看新增 slow trace：若慢在子智能体预载，则继续优化 task mirror 扫描；若慢在显示正文读取，则继续优化分段会话窗口读取与前端渲染。

## 2026-07-05 第 20 轮：单会话正文响应聚焦

### 本轮目标

继续排查会话切换慢与子智能体列表延迟显示问题。上一轮已经移除了水合定位阶段的大对象克隆，本轮进一步检查懒加载响应本身是否还在传输无关会话正文。

### 执行流分析

本轮确认 `ConversationMessagesLoadResponse` 在 API 与前端类型中都表示单个懒加载会话正文。前端 `mergeWorkbenchSnapshot` 会保留本地已有但本次 incoming 没覆盖的会话消息，`mergeConversationMessagesWithSubagentSurfaceMessageCache` 也会按会话窗口替换。因此服务端 fast path 不需要、也不应该把所有已水合会话正文重复塞进每次响应。

旧实现的问题在于：`retained_hydrated_session_messages_response` 已经命中内存 fast path 后，仍调用 `append_hydrated_workspace_messages`，导致请求一个小会话也可能把其他大历史会话的正文带回前端。这既违反单会话响应语义，也会削弱上一轮轻量水合修复的收益。

### 核心修复

1. 移除 `retained_hydrated_session_messages_response` 中的全量已水合正文追加逻辑。
2. 保持 fast path 对 selected session、hydrated windows、未关闭子智能体预载和导航投影的更新。
3. 新增回归测试，确认两个会话都已水合时，请求其中一个会话只返回该会话的消息；另一个会话仍保留在服务端 workspace 中。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 新增/修改：`crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`
- 新增并归档：`docs/completed/20260705/02-FOCUSED_CONVERSATION_MESSAGES_RESPONSE.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt`：通过。
- `cargo fmt --check`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-service-kernel retained_conversation_messages_response_is_focused_on_requested_session -- --nocapture`：通过。
- `cargo test -p vulcan-service-kernel --tests -- --nocapture`：通过，170 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮审核确认懒加载 fast path 现在遵守“单响应只携带请求会话正文”的契约；完整工作台刷新路径仍保留追加全部已水合正文的旧语义，没有扩大行为变更范围。下一轮如继续排查切换慢，应优先看前端是否在会话切换时重复触发 `service_session_messages`，以及服务端 `conversation_hydration` 进度事件是否与 UI 初始化遮罩绑定过紧。

## 2026-07-05 第 21 轮：会话选择快照聚焦

### 本轮目标

继续沿会话切换慢和子智能体列表延迟显示的问题排查。上一轮已经让 `service_session_messages` 只返回单个会话正文，本轮检查侧栏点击选择会话这条更早发生的服务端路径是否仍会搬运无关正文。

### 执行流分析

本轮确认前端点击侧栏会话的流程是：

1. `handleSelectConversation` 立即更新本地 `selectedConversationId`。
2. 同时调用 `service_select_conversation` 返回一个选中态快照。
3. `useConversationHydration` 根据 `hydratedConversationWindows` 判断目标会话是否已水合。
4. 如果未水合，再调用 `service_session_messages` 加载正文。

因此，`service_select_conversation` 本身必须是轻量选中态更新，否则即使正文懒加载已经聚焦，点击侧栏仍可能先传回其他大历史会话正文。旧实现中的 `retained_selected_conversation_messages` 会在当前会话正文之后调用 `append_hydrated_workspace_messages`，把所有已水合会话正文追加进选择快照。

### 核心修复

1. 将 `retained_selected_conversation_messages` 改为只返回当前选中会话已保留的正文窗口。
2. 选择未水合会话时返回空 `messages`，等待正文懒加载接口负责加载。
3. 保留 hydrated window 摘要和缓存子智能体导航投影，让前端仍可判断哪些会话已水合、哪些子智能体可见。
4. 更新会话选择测试，覆盖“未水合目标不带任何正文”和“已水合目标只带自身正文”两种情况。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`
- 新增并归档：`docs/completed/20260705/03-FOCUSED_CONVERSATION_SELECTION_SNAPSHOT.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt`：通过。
- `cargo fmt --check`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-service-kernel conversation_selection_tests -- --nocapture`：通过，3 个测试。
- `cargo test -p vulcan-service-kernel --tests -- --nocapture`：通过，170 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮审核确认 `append_hydrated_workspace_messages` 已不再被 `service_select_conversation` 使用，只保留在完整工作台刷新与对应测试中。选择快照现在成为真正的轻量选中态投影，前端已有 merge 机制负责保留本地消息缓存。下一轮如果继续优化切换体验，应优先排查选择快照里的子智能体导航投影是否仍会扫描过多缓存，以及前端是否因为 `conversationHydration` 遮罩策略导致已知导航延迟显示。

## 2026-07-05 第 22 轮：子智能体导航投影去重

### 本轮目标

继续排查会话切换与子智能体导航显示链路中的局部坏味道。上一轮已经把侧栏选择快照改成轻量消息投影，本轮检查选择快照仍会执行的子智能体导航投影是否存在不必要的重复遍历或复制。

### 执行流分析

本轮确认 `subagent_navigation_snapshot_from_cached_state` 并不会全局扫描所有会话，而是从当前 `workspace_snapshot.selected_session` 出发，按 `session_subagent_tasks` 中的父子关系遍历当前根会话的未关闭子智能体树。

但实现里仍有两个局部浪费点：

1. `active_surface_session_id_from_state` 先调用 `cached_subagent_tree_contains_task` 判断活动面板是否属于当前根会话；如果活动面板有效，后续分支又调用同一个函数做第二次树遍历。
2. `normalized_subagent_tree_states_from_cache` 已经返回拥有所有权的 `Vec<SubagentTaskPanelState>`；随后 `select_subagent_navigation_panel_states` 接收切片并执行 `to_vec()`，额外克隆整份状态列表。

### 核心修复

1. 在 `active_surface_session_id_from_state` 中复用第一次活动面板归属判断结果，消除重复树遍历。
2. 将 `select_subagent_navigation_panel_states` 改为消费 `Vec<SubagentTaskPanelState>`。
3. 服务端调用点直接 move 已构建好的 normalized states，避免整表克隆。
4. 更新共享模型测试，确认所有未关闭子智能体仍会进入导航列表。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/workbench_surface_projection.rs`
- 修改：`crates/vulcan-workbench-model/src/subagent/navigation.rs`
- 修改：`crates/vulcan-workbench-model/src/subagent/tests/navigation_store.rs`
- 新增并归档：`docs/completed/20260705/04-SUBAGENT_NAVIGATION_PROJECTION_DEDUP.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt`：通过。
- `cargo fmt --check`：通过。
- `cargo check -p vulcan-workbench-model`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-workbench-model --tests -- -D warnings`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-workbench-model selects_all_unclosed_subagent_navigation_states -- --nocapture`：通过。
- `cargo test -p vulcan-workbench-model --tests -- --nocapture`：通过，46 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation_tests -- --nocapture`：通过，17 个测试。
- `cargo test -p vulcan-service-kernel --tests -- --nocapture`：通过，170 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮审核确认修复没有改变导航业务语义：所有未关闭子智能体仍显示，活动任务仍只在属于当前根树时保留。变化仅限于去掉重复树包含判断和整表状态克隆。后续如果继续优化该区域，可进一步清理 `select_subagent_navigation_panel_states` 中当前保留但未使用的 `has_recoverable_detail` 参数，或评估是否需要把导航树遍历结果与 active surface 选择合并成一次投影。

## 2026-07-05 第 23 轮：子智能体导航选择器 API 清理

### 本轮目标

继续清理上一轮审核指出的导航选择器 API 坏味道：`select_subagent_navigation_panel_states` 暴露了一个看似会参与过滤的 `has_recoverable_detail` 回调，但函数体完全不调用它。目标是移除误导性参数，让“导航列表选择”和“面板可打开性判断”保持清晰边界。

### 执行流分析

本轮确认真实调用链如下：

1. `normalized_subagent_tree_states_from_cache` 已经返回当前根会话下所有未关闭子智能体状态。
2. `select_subagent_navigation_panel_states` 只需要保留这批状态，并验证请求打开的活动任务是否仍在列表中。
3. 旧签名传入的 `has_recoverable_detail` 参数名带下划线，说明它实际未使用。
4. service-kernel 唯一生产调用点把 `webui_subagent_panel_has_recoverable_detail` 传入该参数，容易误导维护者以为导航列表会按可恢复详情过滤。
5. 真正的可打开性判断仍在 `subagent_task_panel_is_openable` 和 `webui_subagent_panel_has_recoverable_detail`，不应挂在导航选择器 API 上。

### 核心修复

1. 删除 `select_subagent_navigation_panel_states` 的泛型回调参数。
2. 更新函数注释，明确它只消费已归一化状态并验证活动任务。
3. 更新 service-kernel 调用点，移除无效闭包。
4. 更新共享模型测试调用点，继续断言所有未关闭子智能体都会显示。

### 文件变更

- 修改：`crates/vulcan-workbench-model/src/subagent/navigation.rs`
- 修改：`crates/vulcan-workbench-model/src/subagent/tests/navigation_store.rs`
- 修改：`crates/vulcan-service-kernel/src/workbench_surface_projection.rs`
- 新增并归档：`docs/completed/20260705/05-SUBAGENT_NAVIGATION_SELECTOR_API_CLEANUP.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt`：通过。
- `cargo test -p vulcan-workbench-model selects_all_unclosed_subagent_navigation_states -- --nocapture`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation_tests -- --nocapture`：通过，17 个测试。
- `cargo check -p vulcan-workbench-model`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo fmt --check`：通过。
- `cargo clippy -p vulcan-workbench-model --tests -- -D warnings`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-workbench-model --tests -- --nocapture`：通过，46 个测试。
- `cargo test -p vulcan-service-kernel --tests -- --nocapture`：通过，170 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮循环审核确认调用点只剩函数定义、共享模型测试和 service-kernel 生产调用点三处；`has_recoverable_detail` 不再出现在选择器 API 上，仍保留在真正的面板可打开性路径。当前没有发现本轮改动引入的编译、测试、clippy 或语义问题。后续可继续单独审计 `subagent_navigation_round_should_remain_visible` 中保留但未使用的排队工作参数。

## 2026-07-05 第 24 轮：子智能体导航轮次 Gate API 清理

### 本轮目标

继续清理上一轮留下的候选债务：`subagent_navigation_round_should_remain_visible` 保留了 `_has_queued_parent_work` 参数，但唯一生产调用点固定传入 `false`，函数体也完全不使用它。目标是把轮次保留 gate 收敛到真实输入，避免后续误以为父级排队提示词会参与子智能体导航轮次保留。

### 执行流分析

本轮确认真实调用链如下：

1. `refresh_webui_subagent_navigation_round_visibility` 在子智能体事件收敛后计算是否保留该父会话的导航轮次。
2. 它分别检查父级 active stream task、session kernel active turn、workspace active phase，以及当前根树中是否仍有运行中子智能体。
3. 旧代码调用 `subagent_navigation_round_should_remain_visible(parent_active, has_running, false)`，第三个参数没有任何动态来源。
4. 共享模型测试还断言 `false, false, true` 返回 `false`，说明这个参数不是漏接，而是历史残留。
5. 父级排队提示词由主会话 pending prompt 与生命周期链路单独处理，不属于子智能体导航轮次 gate 的职责。

### 核心修复

1. 删除 `subagent_navigation_round_should_remain_visible` 的第三个参数。
2. 更新函数注释，将返回值描述收窄为“父级运行或运行中的子任务保持轮次可见”。
3. 更新 service-kernel 调用点，移除固定 `false`。
4. 更新共享模型测试，删除“传 true 也忽略”的历史兼容断言。

### 文件变更

- 修改：`crates/vulcan-workbench-model/src/subagent/navigation.rs`
- 修改：`crates/vulcan-workbench-model/src/subagent/tests/navigation_store.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 新增并归档：`docs/completed/20260705/06-SUBAGENT_NAVIGATION_ROUND_GATE_API_CLEANUP.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt`：通过。
- `cargo test -p vulcan-workbench-model subagent_navigation_round_visibility_tracks_coordination_work -- --nocapture`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation_tests -- --nocapture`：通过，17 个测试。
- `cargo check -p vulcan-workbench-model`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo fmt --check`：通过。
- `cargo clippy -p vulcan-workbench-model --tests -- -D warnings`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-workbench-model --tests -- --nocapture`：通过，46 个测试。
- `cargo test -p vulcan-service-kernel --tests -- --nocapture`：通过，170 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮审核确认 `subagent_navigation_round_should_remain_visible` 调用点只剩函数定义、共享模型测试和 service-kernel 生产调用点；所有 queued parent work 相关残留均已从该链路移除。业务语义未改变，API 现在更准确地表达真实职责。后续可继续沿子智能体导航链路审计是否还有“状态投影层携带历史兼容输入”的类似问题。

## 2026-07-05 第 25 轮：子智能体未关闭索引根镜像唯一来源

### 本轮目标

解决会话切换或重新打开时，未关闭子智能体列表长时间延迟出现、数量先少后多，以及无子代理小会话也卡在定位会话阶段的问题。目标是把未关闭子智能体预载收敛到根 task mirror 这一轻量索引，彻底移除旧的子 task metadata 全目录扫描回退。

### 执行流分析

本轮确认真实调用链如下：

1. 启动期、刷新期、完整会话水合期都会调用未关闭子智能体 metadata preload。
2. 已水合会话快速路径也会在返回前尝试补齐未关闭子智能体索引，因此 preload 不能包含全目录扫描这种重 IO。
3. 旧实现先读根 task mirror；当根 mirror 缺失但 `task/` 目录存在时，会回退调用 `load_task_session_metadata_by_parent` 扫描全部子任务 metadata。
4. 该回退路径能解释用户观察到的“先显示 1 个、过一会儿变 3 个”和“没有子代理的会话也卡几秒”。
5. `SegmentedTaskSessionStore::load_task_session_mirror_summaries_by_parent` 在根 metadata 缺失或 `tasks` 缺失时已经返回空列表，因此不需要额外探测或回退。

### 核心修复

1. `load_webui_unclosed_subagent_metadata_preload` 改为只读取根 task mirror。
2. 删除服务热路径中的旧子 task metadata 扫描回退。
3. 读取根 mirror 失败时显式记录日志并按空索引处理。
4. 将缺根 mirror 测试从“回退恢复”改成“不会凭子目录恢复”。
5. 补齐标准测试夹具的根会话 metadata，确保测试模拟真实主会话目录与根 mirror 写入链路。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 新增并归档：`docs/completed/20260705/13-SUBAGENT_MIRROR_ONLY_PRELOAD.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，21 个筛选测试。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，3 个测试。
- `cargo test -p vulcan-service-kernel --tests -- --nocapture`：通过，170 个测试。
- `cargo check -p vulcan-workbench-model`：通过。
- `cargo clippy -p vulcan-workbench-model --tests -- -D warnings`：通过。
- `cargo test -p vulcan-workbench-model --tests -- --nocapture`：通过，46 个测试。

### 审核结论

本轮三轮审核确认服务热路径中不再引用 `load_task_session_metadata_by_parent`、`task_session_directory_exists`、`root_session_task_mirror_exists` 或 `subagent_metadata_preload_fallback_scan`。未关闭子智能体列表现在只从根 mirror 恢复；缺镜像即视为空索引。后续如果仍有加载卡顿，应继续审计会话快速路径上的其他同步 IO，而不是恢复旧 metadata 扫描。

## 2026-07-05 第 26 轮：工作台消息面收敛与会话水合边界修复

### 本轮目标

解决会话切换和重新打开时，未关闭子智能体列表延迟出现、数量忽隐忽现，以及无子智能体短会话也可能在“定位会话”阶段被拖慢的问题。目标是把全量工作台消息面收敛为当前活动 surface，非当前会话正文交给前端本地缓存保留，不再由后端每次重复广播。

### 执行流分析

本轮确认真实链路如下：

1. `service_select_conversation` 已经是轻量选中态投影，不应承担分段正文读取或未关闭子智能体磁盘预载。
2. `service_session_messages` / `load_hydrated_session_workbench` 是会话初始化边界，会读取目标会话正文窗口并恢复当前会话的未关闭子智能体索引。
3. 后端 `workbench_from_runtime` 仍在全量 workbench 构建时调用旧的 `append_hydrated_workspace_messages`，把所有 `workspace.hydrated_session_ids` 对应正文追加进每次 payload。
4. 前端 `mergeWorkbenchSnapshot` 已经能保留本地已加载会话正文，`mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache` 也能保留已载入子智能体界面，因此后端不需要重复发送所有已水合正文。
5. 旧的全量正文追加会让小会话请求携带其他大历史会话正文，造成构建、序列化、传输和前端合并成本上升，也会让“正文已存在”和“当前会话未关闭子智能体索引已恢复”这两个状态出现错位。

### 核心修复

1. `workbench_from_runtime` 移除 `append_hydrated_workspace_messages`，`messages` 只投影当前活动 surface。
2. `workbench_with_live_pending_question` 不再在待回答问题投影中追加所有已水合会话正文。
3. 删除 `fallback_workbench_projection.rs` 中旧的 `append_hydrated_workspace_messages` helper，避免后续路径误用。
4. 更新活动会话状态测试：用“不会广播无关已水合正文”和“只保留请求会话正文窗口”替代旧的“必须包含全部已水合正文”断言。
5. 审核前端合并路径，确认主要 workbench 更新入口均通过本地缓存合并，不依赖后端全量正文广播。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/workbench_refresh.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_actions.rs`
- 修改：`crates/vulcan-service-kernel/src/fallback_workbench_projection.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/active_session_state_tests.rs`
- 新增并归档：`docs/completed/20260705/14-SUBAGENT_SESSION_HYDRATION_MESSAGE_SURFACE.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-service-kernel live_pending_question_workbench_keeps_messages_on_selected_surface -- --nocapture`：通过。
- `cargo test -p vulcan-service-kernel active_session_state -- --nocapture`：通过，7 个测试。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，3 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，21 个筛选测试。
- `cargo test -p vulcan-service-kernel --lib -- --nocapture`：通过，170 个测试。
- `pnpm -C apps/vulcan-code-web run lint`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮四轮审核确认生产代码不再引用 `append_hydrated_workspace_messages`；前端主要快照入口均通过本地缓存合并；会话选择、水合和子智能体导航测试全部通过。当前语义已经收敛为：后端全量 workbench 只携带当前活动 surface 正文，其他已加载会话正文由前端缓存保留。若后续仍有“定位会话”卡顿，应继续审计前端何时触发正文水合与加载遮罩策略，而不是恢复后端全量正文广播。

## 2026-07-05 第 27 轮：会话水合定位阶段重上下文拆分

### 本轮目标

继续处理用户反馈的“打开或切换很小的会话，也会在正在定位会话阶段停留数秒”的问题。目标是确认定位阶段是否仍承担了不该做的重工作，并用长期方式拆开读取路径与最终工作台投影路径。

### 执行流分析

本轮确认真实链路如下：

1. 前端 `handleSelectConversation` 先调用轻量 `selectConversation`，随后 `useConversationHydration` 异步调用 `loadConversationMessages`。
2. 后端 `service_session_messages` 在未命中已水合 fast path 时，先发送 `locating` 进度，再进入 `load_hydrated_session_workbench`。
3. `load_hydrated_session_workbench` 原本进入后第一步就是捕获完整 `HydratedSessionWorkbenchRuntimeContext`。
4. 这个完整上下文会克隆 LuaSkills 目录、MCP 目录、最近日志、Provider 配置、AI 工具配置、上下文预算、AgentRuntimeBridge 等最终 workbench 投影才需要的数据。
5. 这些数据与“定位会话”和“读取当前消息窗口”无直接关系，却发生在 `readingIndex` 进度之前，因此 UI 会长时间停在“正在定位会话”，容易误导用户以为卡在会话定位。

### 核心修复

1. 新增 `HydratedSessionReadContext`，只包含读取会话需要的 `controller_runtime` 与 `preserved_workspace_run_state`。
2. 新增 `HydratedSessionProjectionContext`，承载最终 workbench 投影所需的目录、配置、日志、预算和运行时摘要。
3. `load_hydrated_session_workbench` 开头只捕获轻量读取上下文。
4. 将重投影上下文捕获延后到消息窗口读取完成后的 `projecting` 阶段。
5. 保持运行态保护语义不变：读取索引前仍捕获 `capture_preserved_workspace_run_state_for_session_hydration`，应用水合时仍使用当前 task runtime 与 live listener 集合。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 新增并归档：`docs/completed/20260705/15-SESSION_HYDRATION_CONTEXT_SPLIT.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-service-kernel active_session_state -- --nocapture`：通过，7 个测试。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，3 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，21 个筛选测试。
- `cargo test -p vulcan-service-kernel --lib -- --nocapture`：通过，170 个测试。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮三轮审核确认旧的 `capture_hydrated_session_workbench_runtime_context` 与 `session_hydration_runtime_context_capture_slow` 已无残留；重字段只在 `capture_hydrated_session_projection_context` 内捕获，并且该函数只在消息窗口读取完成后的 `projecting` 阶段调用。会话选择、水合与子智能体导航相关测试全部通过。后续若仍观察到加载慢，应根据 trace 继续区分是转录窗口读取、未关闭智能体索引恢复，还是前端遮罩策略。

## 2026-07-05 第 28 轮：会话水合在途请求复用修复

### 本轮目标

处理前端会话水合在切换会话时的竞态问题。目标是让已经发出的水合请求可以后台完成并写入缓存，避免“切走再切回同一会话”时既不启动新请求、又丢弃旧请求结果。

### 执行流分析

本轮确认真实链路如下：

1. `handleSelectConversation` 切换会话后先调用轻量 `selectConversation`。
2. `useConversationHydration` 根据 `selectedConversationHydrated` 决定是否调用 `loadConversationMessages`。
3. hook 使用 `hydratingConversationIdsRef` 去重同一会话的在途请求。
4. 旧实现中，effect cleanup 会把当前请求标记为 `cancelled`。
5. 如果用户切走会话 A，再在 A 的请求完成前切回 A，新 effect 会看到 A 已经在 `hydratingConversationIdsRef` 内并直接返回。
6. 旧请求完成时又因为 `cancelled` 为 true，不会 merge response，也不会更新当前 A 的状态。
7. 这会让 A 的水合结果永久丢失，且依赖没有再次变化时不会自动重试。

### 核心修复

1. 新增 `mountedRef`，只在组件真正卸载时阻止异步 state 更新。
2. 新增 `selectedConversationIdRef`，让错误展示只跟随当前选中会话。
3. 移除基于选中态 cleanup 的 `cancelled` 成功结果丢弃逻辑。
4. 在途请求成功后，只要组件仍挂载，就通过 `mergeWorkbenchWithLocalCaches` 写入本地缓存。
5. 当前选中会话如果已有在途请求，直接设置 `loadingConversationId` 为该会话并清理旧错误。
6. 请求结束时只清理匹配当前 loading id 的加载状态，不覆盖其他会话的新加载状态。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-hydration.ts`
- 新增并归档：`docs/completed/20260705/16-CONVERSATION_HYDRATION_INFLIGHT_REUSE.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `pnpm -C apps/vulcan-code-web run lint`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮三轮审核确认：旧 `cancelled` 语义已移除；后台成功结果会进入本地缓存；错误展示受当前选中会话约束；loading 清理按 id 匹配，不会覆盖其他会话的新请求。由于当前 WebUI 没有前端测试脚本，本轮未能补 hook 单测，后续如果引入 Vitest/React Testing Library，应优先覆盖“切走再切回同一会话且旧请求仍在飞”的竞态。

## 2026-07-05 第 29 轮：会话水合旧 revision 聚焦合并修复

### 本轮目标

处理会话正文水合 RPC 返回结果被全局 workbench revision 门禁静默丢弃的问题。目标是保证切换到存在未关闭智能体的会话时，目标会话正文、子智能体导航和水合窗口可以稳定进入前端缓存，不再出现“打开后等很久才出现”“有时 3 个有时 1 个”的随机状态。

### 执行流分析

本轮确认真实链路如下：

1. `useConversationHydration` 调用 `loadConversationMessages` 或 `loadConversationHistoryPage`。
2. 后端 `service_session_messages` 会更新 `guard.workbench`、递增 `workbench_generation`、发送 workbench event，然后返回同一份 RPC response。
3. 前端普通事件和 RPC response 都会进入 `mergeWorkbenchWithLocalCaches`。
4. `mergeWorkbenchWithLocalCaches` 最终调用 `mergeWorkbenchSnapshot`，该函数先判断 `shouldAcceptWorkbenchRevision`。
5. 如果 event 先被前端接受，RPC response 到达时可能已经不是更高 revision，于是完整 response 被直接丢弃。
6. 被丢弃的不只是全局快照，还包括目标会话正文、`hydratedConversationWindows` 和 `subagentNavigationByConversation`。
7. 这解释了用户观察到的子代理列表随机缺失、需要等待或再次触发后才显示完整的问题。

### 核心修复

1. 新增 `mergeConversationHydrationSnapshot`，专门处理会话正文水合响应。
2. 当响应 revision 仍然新鲜时，继续复用完整 `mergeWorkbenchSnapshot`，保持原有全局快照语义。
3. 当响应 revision 已旧时，只合并显式目标会话相关的消息、会话水合窗口、加载进度和子智能体导航。
4. 新增 `focusedConversationId` 选项，避免旧响应根据自身 selected 状态误合并非目标会话。
5. 消息替换只按实际携带 incoming messages 的会话进行，避免只携带窗口/进度的旧响应清空已有消息。
6. 新增 `mergeConversationHydrationSnapshotWithSubagentSurfaceMessageCache`，让聚焦水合响应继续吸收并注入子智能体面板缓存。
7. `useConversationHydration` 的正文水合与历史分页均改用聚焦水合合并器，普通工作台操作继续使用原有全局合并器。

### 文件变更

- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-hydration.ts`
- 新增并归档：`docs/completed/20260705/17-CONVERSATION_HYDRATION_STALE_REVISION_MERGE.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `pnpm -C apps/vulcan-code-web run lint`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮四轮审核确认：普通 workbench 事件没有接入聚焦合并器；旧 revision 水合响应不会放宽全局 revision；显式 `focusedConversationId` 存在时只合并目标会话；没有 incoming messages 时不会清空目标会话已有消息；子智能体面板缓存仍由专用包装函数维护。后续如果继续观察到打开会话后子智能体列表延迟，应继续向后端初始化阶段排查，而不是前端 response 丢弃路径。

## 2026-07-05 第 30 轮：历史分页响应聚焦目标会话修复

### 本轮目标

处理更早历史分页 RPC response 依赖服务端当前 selected conversation 的问题。目标是保证 `loadConversationHistoryPage` 即使遇到服务端 selected 状态与请求会话短暂不一致，也能在 `response.workbench.messages` 中稳定携带请求会话的完整合并窗口。

### 执行流分析

本轮确认真实链路如下：

1. 前端 `handleLoadConversationHistory` 调用 `loadConversationHistoryPage`。
2. 前端只把 `response.workbench` 交给聚焦水合合并器，未直接消费 `response.messages`。
3. 后端 `service_session_history_page` 旧逻辑只在 `guard.workbench.selected_conversation_id == session_id` 时才把分页后的完整目标会话窗口写入 `guard.workbench.messages`。
4. 如果服务端 selected 与本次请求目标短暂不一致，RPC 虽然成功返回，但 response workbench 没有目标会话的完整消息窗口。
5. 这会让上一轮新增的前端 `focusedConversationId` 合并器没有可合并的消息载荷，表现为历史加载或追加状态等待后没有稳定变化。

### 核心修复

1. 在 `service_session_history_page` 中先生成请求会话分页合并后的 `response_messages`。
2. 事件路径继续保持原 selected 语义：只有服务端当前 selected 等于请求会话时才更新 `guard.workbench.messages`。
3. 新增 `focused_history_page_response_workbench`，专门为 RPC response 克隆事件 workbench 并替换为请求会话消息窗口。
4. `ConversationHistoryPageResponse.workbench` 改为返回聚焦 response workbench，`messages` 字段仍保持单页 older messages 语义。
5. 新增测试 `history_page_response_workbench_is_focused_on_requested_session`，锁定 selected 不一致时 response 仍聚焦请求会话。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`
- 新增并归档：`docs/completed/20260705/18-HISTORY_PAGE_FOCUSED_RESPONSE.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，4 个测试。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `pnpm -C apps/vulcan-code-web run lint`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮三轮审核确认：后端 event 与 RPC response 已明确分层；response workbench 稳定携带请求会话窗口；前端聚焦合并器以 `response.sessionId` 为边界消费该窗口；没有发现其他 `ConversationHistoryPageResponse` 消费路径依赖旧 selected 行为。该修复补齐了上一轮前端旧 revision 聚焦合并所需的后端载荷条件。

## 2026-07-05 第 31 轮：历史分页在途请求同步门禁修复

### 本轮目标

修复前端更早历史分页请求只依赖 React state 判断 loading 的问题。目标是让同一会话在上一页历史请求完成前无法再次发起相同分页 RPC，避免重复磁盘 IO 与历史窗口合并竞态。

### 执行流分析

本轮确认真实链路如下：

1. `conversation-workspace.tsx` 的“加载更早历史”按钮通过 `props.historyLoading` 禁用。
2. `props.historyLoading` 来自 `useConversationHydration` 暴露的 `conversationHistoryLoadingIds` state。
3. `handleLoadConversationHistory` 旧逻辑也用同一个 state Set 判断当前会话是否 loading。
4. React state 更新需要下一次渲染才会反映到闭包和按钮禁用状态。
5. 快速重复点击或程序化连续触发时，第二次调用可能仍看到旧 Set，从而放行重复 `loadConversationHistoryPage`。
6. 成功与失败分支各自清理 loading，缺少统一 `finally`，未来中间合并逻辑抛错时存在 loading 残留风险。

### 核心修复

1. 新增 `conversationHistoryLoadingIdsRef` 作为历史分页请求的同步门禁。
2. 新增 `setConversationHistoryRequestLoading`，统一更新 ref 并投影到 UI state。
3. `handleLoadConversationHistory` 发起请求前先查 ref，不再用异步 state 作为并发门禁。
4. 请求放行后立即写入 ref，确保同一渲染周期内的重复触发也会被拦截。
5. 将 loading 清理移动到 `finally`，覆盖成功、失败、合并异常与卸载返回路径。
6. 补充 `mountedRef` 检查，避免历史分页请求在组件卸载后继续调用 `setWorkbenchState`。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-hydration.ts`
- 新增并归档：`docs/completed/20260705/19-HISTORY_PAGING_INFLIGHT_GUARD.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `pnpm -C apps/vulcan-code-web run lint`：通过。
- `git diff --check -- apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-hydration.ts`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮三轮审核确认：同一会话重复分页触发会被同步 ref 拦截；不同会话仍可独立加载；loading 释放集中在 `finally`；卸载后不会继续提交 workbench 合并。当前仓库没有前端单测框架，本轮用 TypeScript 构建与人工链路审核覆盖。

## 2026-07-05 第 32 轮：会话水合保留载荷合并修复

### 本轮目标

修复会话正文水合 fast path 使用轻量 workspace index 后，会把其它已水合会话的正文、运行态与压缩信息从内存快照中清空的问题。目标是保持轻量索引优化，同时确保会话切换后已打开的子智能体与会话载荷不会随机消失或延迟恢复。

### 执行流分析

本轮确认真实链路如下：

1. `load_hydrated_session_workbench` 优先尝试复用内存中的 workspace snapshot。
2. 该 fast path 通过 `lightweight_workspace_index_snapshot_for_hydration` 构造轻量索引，刻意清空 `session_messages`、`session_runtime_state`、`session_compactions` 和 `hydrated_session_ids` 等大载荷。
3. 当前水合流程随后只为本次选中的会话重新填充正文窗口。
4. 最终 `guard.workspace_snapshot = Some(refreshed_snapshot)` 会把这个轻量快照写回服务状态。
5. 因此其它此前已经水合、仍然有效的会话载荷会被丢失，表现为切换会话后子智能体列表或会话正文缓存有时只剩一部分。

### 核心修复

1. 新增 `preserve_retained_hydrated_payloads_after_index_refresh`，在轻量索引写回前恢复其它仍有效会话的保留载荷。
2. 合并范围限定为仍存在于刷新后 workspace 中的常规会话，避免恢复已删除或归档路径的数据。
3. 当前正在水合的会话显式跳过，确保刚读取到的新正文窗口不会被旧缓存覆盖。
4. 合并内容覆盖 `session_messages`、`session_runtime_state`、`session_compactions` 和 `hydrated_session_ids`。
5. 新增单元测试验证：水合 A 会话时，B 会话的正文、运行态、压缩记录与水合标记仍保留，A 会话的新载荷保持优先。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`
- 新增并归档：`docs/completed/20260705/20-HYDRATION_RETAINED_PAYLOAD_PRESERVATION.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，5 个测试。
- `cargo test -p vulcan-service-kernel hydration_index_refresh_preserves_other_retained_payloads -- --nocapture`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `git diff --check -- crates/vulcan-service-kernel/src/conversation_history.rs crates/vulcan-service-kernel/src/lib.rs crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮三轮审核确认：当前会话的新水合载荷不会被旧缓存覆盖；其它仍有效的已水合会话不会因轻量索引写回丢失正文、运行态或压缩信息；已删除会话不会被误恢复；该修复不改变 archived session 语义，也不回退轻量索引优化。

## 2026-07-05 第 33 轮：启动期未关闭智能体预载移除

### 本轮目标

移除 WebUI 服务启动阶段对未关闭智能体元数据的磁盘预载，让智能体树恢复只发生在具体会话正文水合阶段。目标是避免 workspace 初始化被子智能体索引扫描拖慢，并让启动语义与“会话加载时恢复未关闭智能体”的设计保持一致。

### 执行流分析

本轮确认真实链路如下：

1. `run_service_bootstrap` 的 workspace 阶段旧文案声明“正在载入项目、会话与未关闭智能体索引”。
2. 启动链路在 `load_or_seed_workspace_index` 与 `hydrate_selected_session_runtime_if_needed` 后，直接调用 `load_webui_unclosed_subagent_metadata_preload_once`。
3. 随后又调用 `apply_webui_unclosed_subagent_metadata_preload_once`，并基于该缓存构建初始 `bootstrap_subagent_navigation`。
4. 初始 `workbench_from_runtime` 因此可能携带启动期扫描出来的子智能体导航。
5. 具体会话水合路径 `load_hydrated_session_workbench` 已经有 `loadingSubagents` 阶段，负责读取和应用未关闭智能体元数据。
6. 因此启动期预载是重复职责，会把会话级 IO 提前到全局启动阶段。

### 核心修复

1. 从 `service_bootstrap.rs` 移除启动期未关闭智能体元数据读取与应用。
2. 初始 workbench 构建时将 `subagent_navigation` 明确传为 `None`。
3. 启动 workspace 阶段文案改为“项目与会话索引”。
4. 启动日志移除 `unclosed_subagents` 计数字段。
5. `startup_ready_message("workspace")` 改为“项目与会话索引已载入”。
6. 新增测试锁定 workspace 启动文案不再承诺智能体索引预载；现有导航测试继续证明导航投影本身不会执行分段 TaskSession IO，只有显式会话预载后才显示未关闭智能体。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/service_bootstrap.rs`
- 修改：`crates/vulcan-service-kernel/src/service_bootstrap/startup_updates.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`
- 新增并归档：`docs/completed/20260705/21-BOOTSTRAP_SUBAGENT_PRELOAD_REMOVAL.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `rg` 检查 `service_bootstrap.rs` 与 `startup_updates.rs`：无启动期预载函数调用、无旧智能体索引文案残留。
- `cargo fmt --check`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo test -p vulcan-service-kernel workspace_bootstrap_ready_message_excludes_subagent_index_preload -- --nocapture`：通过。
- `cargo test -p vulcan-service-kernel navigation_projection_uses_explicitly_preloaded_unclosed_subagents -- --nocapture`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，22 个测试。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `git diff --check -- crates/vulcan-service-kernel/src/service_bootstrap.rs crates/vulcan-service-kernel/src/service_bootstrap/startup_updates.rs crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs docs/plan/20260705-21-BOOTSTRAP_SUBAGENT_PRELOAD_REMOVAL.md`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮三轮审核确认：启动期不再读取未关闭智能体元数据；初始 workbench 不再携带由启动期磁盘扫描产生的子智能体导航；会话水合路径仍保留智能体恢复入口；启动状态文案与实际执行流一致。完整启动线程未做集成测试，原因是该路径会拉起真实 runtime bootstrap，本轮用目标单测、导航单测、编译检查与源码残留搜索覆盖风险点。

## 2026-07-05 第 34 轮：普通工作台刷新移除智能体预载

### 本轮目标

移除普通 `refresh_workbench_snapshot` 路径中的未关闭智能体元数据磁盘预载，确保项目、目录、技能、模型设置等普通工作台刷新只投影已有内存缓存，不再偷偷承担会话级智能体恢复职责。

### 执行流分析

本轮确认真实链路如下：

1. `refresh_workbench_snapshot` 与 `refresh_workbench_snapshot_for_selected_session` 都进入 `refresh_workbench_snapshot_with_selected_session`。
2. 该函数会重新读取 workspace index、恢复活跃运行态、计算 selected session，并为 selected session 执行 runtime hydration 和 task mirror 状态同步。
3. 旧逻辑随后调用 `preload_webui_unclosed_subagent_metadata_into_state`，从磁盘读取 selected session 的未关闭智能体元数据并写入 WebUI 缓存。
4. `refresh_workbench_snapshot` 被很多非正文水合操作调用，例如项目、目录、技能、VMM、模型设置等。
5. 因此普通 refresh 会把会话级智能体恢复 IO 扩散到大量无关操作上，与上一轮“启动不预载、会话水合预载”的职责边界不一致。
6. 会话正文水合路径 `load_hydrated_session_workbench` 和 retained body fast path 已经保留智能体恢复入口。

### 核心修复

1. 从 `workbench_refresh.rs` 删除普通 refresh 中的 `preload_webui_unclosed_subagent_metadata_into_state` 调用。
2. 删除对应 `refresh_workbench_subagent_preload` slow trace。
3. 普通 refresh 继续通过 `subagent_navigation_snapshot_from_cached_state` 投影已有缓存，不主动从磁盘恢复新智能体。
4. 发现 `preload_webui_unclosed_subagent_metadata_into_state` 已无生产调用者，删除该 helper。
5. 测试改为直接验证 `load_webui_unclosed_subagent_metadata_preload_once` 与 `apply_webui_unclosed_subagent_metadata_preload_once` 的组合语义。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/workbench_refresh.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`
- 新增并归档：`docs/completed/20260705/22-WORKBENCH_REFRESH_SUBAGENT_PRELOAD_REMOVAL.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `rg` 检查 `workbench_refresh.rs`：无 `preload_webui_unclosed_subagent_metadata_into_state` 与 `refresh_workbench_subagent_preload` 残留。
- `rg` 检查全服务内核：`preload_webui_unclosed_subagent_metadata_into_state` 已无残留。
- `cargo fmt --check`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo test -p vulcan-service-kernel select_conversation_projects_cached_subagent_navigation_only -- --nocapture`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，22 个测试。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `git diff --check -- crates/vulcan-service-kernel/src/workbench_refresh.rs crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs docs/plan/20260705-22-WORKBENCH_REFRESH_SUBAGENT_PRELOAD_REMOVAL.md`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮三轮审核确认：普通 workbench refresh 不再触发未关闭智能体磁盘预载；已有缓存导航仍会被投影；会话水合路径仍负责智能体恢复；删除的 helper 没有生产调用者，移除后 check/clippy 无警告。

## 2026-07-05 第 35 轮：会话水合与子智能体恢复完成态一致

### 本轮目标

修复会话切换后子智能体列表延迟出现、半水合状态被前端误判为完成、以及聚焦会话空导航不能清理旧导航的问题，确保当前会话正文和未关闭子智能体元数据在同一会话水合完成语义下对齐。

### 执行流分析

1. 前端 `selectedConversationHydrated` 原本只看 `hydratedConversationWindows` 是否存在。
2. 后端普通 refresh 和会话选择快路可能保留正文窗口，但当前会话的未关闭子智能体元数据尚未通过会话水合路径完成预载。
3. 在这种半水合状态下，前端会跳过 `loadConversationMessages`，导致子智能体导航只能等后续其它路径慢慢恢复，表现为“先显示 1 个，过一阵才变 3 个”或“切回会话后列表短暂消失”。
4. retained fast path 也需要以“子智能体预载标记存在”为前置条件，否则会绕过完整水合流程。

### 核心修复

1. `HydratedConversationWindowSummary` 新增 `subagent_metadata_loaded` 字段，作为“正文窗口 + 会话级子智能体元数据”联合完成态。
2. `hydrated_conversation_window_summaries` 统一接收 `webui_preloaded_unclosed_subagent_roots`，所有后端投影点都从同一集合派生该字段。
3. `selectedConversationHydrated` 改为仅在 `subagentMetadataLoaded === true` 时成立，半水合窗口会继续触发标准会话水合。
4. retained fast path 在两次锁内都检查预载标记，缺失时回退完整水合。
5. 继续保留普通 refresh cache-only 语义，不恢复磁盘预载职责。
6. 补充会话选择测试，覆盖未预载窗口 false、预载后 retained 快路 true、缺标记时回退完整水合。

### 文件变更

- 修改：`crates/vulcan-service-api/src/conversation_hydration.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_actions.rs`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/workbench_refresh.rs`
- 修改：`crates/vulcan-service-kernel/src/workspace_persistence_versions.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`
- 修改：`apps/vulcan-code-web/src/types/workbench.ts`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-hydration.ts`
- 修改：`apps/vulcan-code-web/src/state/workbench-state.ts`
- 新增并归档：`docs/completed/20260705/23-SUBAGENT_SESSION_HYDRATION_CONSISTENCY.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，6 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，22 个测试。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `pnpm --dir apps/vulcan-code-web lint`：通过。
- `git diff --check -- ...`：通过；仅有 Git 行尾转换提示。

### 审核结论

三轮审核确认：协议字段从 Rust 到 TypeScript 闭环；普通 refresh 与启动期仍不执行磁盘预载；前端不会再把缺少子智能体预载标记的窗口视为完成态。审查中修正了一处旧语义注释，并补充选择快路断言。

## 2026-07-05 第 36 轮：子智能体预载失败语义显式化

### 本轮目标

修复子智能体元数据预载链路中“读取失败被当成成功空结果”的问题，避免损坏的根 TaskSession mirror / metadata 被误判为“已成功预载且没有子智能体”，从而让 retained fast path 与前端水合完成态信任错误状态。

### 执行流分析

1. `load_hydrated_session_workbench` 在 `loadingSubagents` 阶段调用 `load_webui_unclosed_subagent_metadata_preload_once`。
2. 旧实现里底层读取失败会被 `eprintln!` 吞掉，并返回空 `Vec`。
3. 调用方收到 `Some(Vec::new())` 后会执行 `apply_webui_unclosed_subagent_metadata_preload_once`，写入 `webui_preloaded_unclosed_subagent_roots`。
4. 这样真实读取失败会被伪装成“成功读取但没有未关闭智能体”。
5. 正确语义应区分三种状态：成功读取有数据、成功读取空数据、读取失败不能确认。

### 核心修复

1. `load_webui_unclosed_subagent_metadata_preload` 改为返回 `Result<Vec<_>, String>`。
2. Task mirror / metadata 读取失败直接上浮，不再降级为空列表。
3. `load_webui_unclosed_subagent_metadata_preload_once` 只在读取成功时返回 `Some(preload)`。
4. `apply_webui_unclosed_subagent_metadata_preload_once` 只在 `Some(preload)` 时写入预载完成标记；`None` 不再制造新标记。
5. 增加损坏根 `metadata.json` 的回归测试，验证失败不会标记 root，也不会污染可见子智能体缓存。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`
- 新增并归档：`docs/completed/20260705/24-SUBAGENT_PRELOAD_ERROR_SEMANTICS.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，23 个测试。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，6 个测试。
- `git diff --check -- crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs docs/plan/20260705-24-SUBAGENT_PRELOAD_ERROR_SEMANTICS.md TASK_LOG.md`：通过；仅有 Git 行尾转换提示。

### 审核结论

三轮审核确认：错误路径在会话水合 apply 前终止；成功空结果仍可作为合法预载完成；重复预载的 `None` 不再创建新完成标记。当前仍沿用 `String` 错误通道，未引入结构化错误枚举。

## 2026-07-05 第 37 轮：子智能体 Mailbox 投递结果语义显式化

### 本轮目标

修复子智能体终态消息回流父会话时，mailbox 入队结果只有 `bool` 导致的语义混淆。核心问题是“无需投递”“幂等重复”“父级/代际硬拒绝”都可能表现为 `false`，上层 detached listener 可能继续完成父级调度，造成结果没有可靠投递但状态继续前进。

### 执行流分析

1. 子智能体启动时 `bind_webui_subagent_parent_turn` 捕获父会话代际和父回合标识。
2. 子智能体终态事件进入 `handle_webui_detached_subagent_task_event`。
3. listener 调用 `enqueue_webui_session_mailbox_input_if_needed` 写入父会话 mailbox。
4. 旧实现继续调用 `enqueue_webui_subagent_mailbox_input`，只拿到 `bool`。
5. `false` 可能是旧代际拒收、父级不匹配、重复 input、已消费 input 或预期跳过，上层无法判断。
6. 更严重的是：如果硬拒绝后清掉父回合绑定，后续同一终态事件重放会失去原始代际约束，可能退回当前代际并误投递。

### 核心修复

1. 新增 `SubagentMailboxEnqueueOutcome`，明确 `Queued`、`AlreadyPresentOrConsumed`、`Rejected`。
2. 新增 `SubagentMailboxEnqueueRejection`，当前覆盖 `ParentMismatch` 和 `GenerationClosed`。
3. `enqueue_webui_session_mailbox_input_if_needed` 将硬拒绝转为 `Err`，幂等重复仍是 `Ok(false)`。
4. detached listener 遇到硬拒绝后跳过父 completion hook 和 mailbox turn 自动唤醒，但仍刷新终态投影并持久化。
5. 硬拒绝后保留父回合绑定，作为同一终态事件重放时的代际保护。
6. 停止通知路径改为记录 outcome 标签，不再使用布尔 accepted 语义。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/session_runtime_kernel.rs`
- 修改：`crates/vulcan-service-kernel/src/agent_stream/detached_subagent.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_panels.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/mailbox_tests.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/session_kernel_tests.rs`
- 新增并归档：`docs/completed/20260705/25-SUBAGENT_MAILBOX_DELIVERY_OUTCOME.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo test -p vulcan-service-kernel mailbox -- --nocapture`：通过，20 个测试。
- `cargo test -p vulcan-service-kernel detached_subagent -- --nocapture`：通过，21 个测试。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `git diff --check -- ...`：通过；仅有 Git 行尾转换提示。

### 审核结论

三轮审核确认：旧代际仍拒收且上浮为错误；重复终态为幂等跳过；硬拒绝后保留父回合绑定，避免事件重放误投递。审核中修复了硬拒绝后仍清理绑定的风险，并修正 outcome 命名残留。

## 2026-07-05 第 38 轮：子智能体启动首屏水合与保留正文快速路径修复

### 本轮目标

修复会话启动或切换时未关闭子智能体列表延迟出现、数量不稳定，以及已经保留正文的会话仍重复走完整水合的问题。目标语义是：当前选中会话的正文窗口、未关闭子智能体 metadata preload、hydrated window 摘要和右侧子智能体导航必须在同一次初始化投影中完成。

### 问题链路

1. 启动阶段原本只把选中会话运行态读入 `workspace_snapshot`，构建 ready `WorkbenchSnapshot` 时仍传空 `surface_messages`、空 `hydrated_conversation_windows` 和空 `subagent_navigation`。
2. 前端收到 ready snapshot 后认为当前会话未水合，于是再次调用 `loadConversationMessages`，导致列表晚出现、切换回来短暂消失。
3. `retained_hydrated_session_messages_response` 把正文是否可快速返回与 `webui_preloaded_unclosed_subagent_roots` marker 绑定，缺少子智能体 marker 时即使正文已在内存里也会回退到磁盘水合。
4. 前端 `selectedConversationHydrated` 曾把 `subagentMetadataLoaded` 当成正文已水合条件，进一步放大重复请求。

### 核心修复

1. `run_service_bootstrap` 启动时直接加载选中会话显示层窗口，并在同一 state lock 内应用未关闭子智能体 preload、记录 hydrated window、计算正文 surface messages 与子智能体导航。
2. 初始 ready snapshot 现在携带当前选中会话的消息、hydrated conversation window 和未关闭子智能体导航。
3. 已保留正文快速路径只检查 session 存在、正文已水合、消息已保留，不再依赖子智能体 preload marker。
4. 前端 `selectedConversationHydrated` 改成只看 hydrated window 是否存在，`subagentMetadataLoaded` 继续作为展示/诊断字段。
5. 更新会话水合 hook 注释，避免后续维护继续混淆正文窗口与子智能体 metadata marker。

### 文件变更

- 新增并归档：`docs/completed/20260705/26-SUBAGENT_SESSION_BOOTSTRAP_HYDRATION.md`
- 修改：`crates/vulcan-service-kernel/src/service_bootstrap.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-hydration.ts`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，7 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，23 个测试。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `pnpm --dir apps/vulcan-code-web lint`：通过。

### 审核结论

四轮审核确认：启动 ready snapshot 已包含选中会话正文与子智能体导航；快速路径不再依赖 preload marker；空 preload 成功会标记已检查，读取失败不会伪造完成；前端不会再用 `subagentMetadataLoaded` 阻塞正文快速路径。

## 2026-07-05 第 39 轮：保留正文快速路径的常规会话校验事务化

### 本轮目标

修复会话正文快速路径中“先确认已保留正文，再进入提交更新”的边界不够紧的问题。目标是确保已保留正文只能用于普通父会话或归档父会话，不能被子智能体 TaskSession 的保留正文误用，也不能在校验与工作台写入之间发生状态语义漂移。

### 问题链路

1. `service_session_messages` 会先尝试 `retained_hydrated_session_messages_response`，命中后不再读取磁盘。
2. 快速路径依据 `workspace_snapshot.session_messages` 与 `hydrated_session_ids` 判断正文是否已在内存。
3. 子智能体 TaskSession 也可能在 `session_messages` 中留下运行态正文，但它不是可被普通会话正文接口直接选择的 regular session。
4. 旧实现缺少同一锁内的 regular session 校验和提交闭环，存在把非 regular 的保留正文投影到主工作台的风险。
5. `workspace_snapshot_contains_regular_session` 的既有语义包含活动会话与归档会话，明确排除持久化 TaskSession，因此测试必须围绕 TaskSession 构造，而不是误用归档会话。

### 核心修复

1. `retained_hydrated_session_messages_response` 在同一个 `ServiceBridgeState` 锁内完成 regular session 校验、hydrated id 校验、正文读取、窗口读取和工作台提交。
2. 快速路径现在先调用 `workspace_snapshot_contains_regular_session`，非 regular id 直接返回 `Ok(None)`，交由完整水合路径处理。
3. 保留窗口读取与 `hydrated_conversation_windows.entry` 写入保持在同一状态事务中，避免旧快照窗口重新写入新状态。
4. 新增回归测试，构造带有保留正文和运行态痕迹、但不存在普通会话行的 TaskSession，验证快速路径不会返回成功。
5. 测试同时断言工作台未被 TaskSession 正文污染，避免只依赖错误字符串判断。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，8 个测试。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。

### 审核结论

三轮审核确认：regular session 语义与现有源码一致，归档会话仍允许快速路径；TaskSession 即使存在保留正文和运行态信息，也不会污染主会话 workbench。当前仍保留快速路径锁内消息投影，后续若继续优化性能，应在引入状态版本校验后再拆出锁外计算。

## 2026-07-05 第 40 轮：MCP 设置保存禁止静默覆盖与吞错

### 本轮目标

修复 MCP 设置保存链路中的两个长期风险：新增服务器时同名生成的 `server_id` 会直接覆盖旧配置；保存/删除后重建 MCP runtime 的错误会被 `.ok()` 静默吞掉，导致 WebUI 显示保存成功但模型侧 MCP 工具实际不可用。

### 问题链路

1. WebUI 保存 MCP 服务器进入 `service_save_mcp_server_configuration`。
2. 新增流程没有传 `server_id`，服务端用 `stable_identifier_from_label(name)` 从名称生成稳定 id。
3. 旧实现直接 `configuration.mcp.servers.insert(server_id, entry)`，如果 id 已存在，会无提示覆盖旧服务器。
4. 编辑流程如果传入一个已经不存在的 `server_id`，旧实现同样会把它当成新条目插入，制造影子配置。
5. 配置保存后调用 `bootstrap_mcp_runtime_access(...).await.ok()`，runtime 重建失败会被吞掉，并继续把 `mcp_runtime` 设置成 `None`，形成假成功。
6. 启动阶段对同一个 `bootstrap_mcp_runtime_access` 是硬错误，说明设置页吞错和启动语义不一致。

### 核心修复

1. 新增 `apply_mcp_server_entry_to_configuration`，集中处理新增/编辑目标校验。
2. 新增时如果目标 `server_id` 已存在，直接返回错误，保留旧配置不变。
3. 编辑时如果目标 `server_id` 不存在，直接返回错误，不再创建影子条目。
4. 保存和删除路径改为先成功重建 MCP runtime，再持久化配置，再切换内存状态。
5. MCP runtime 重建失败改为显式返回“配置未保存”错误，不再 `.ok()` 吞掉。
6. 持久化失败时主动 shutdown 刚构建的新 MCP runtime，避免后台任务泄露。
7. `store_provider_and_mcp_configuration_after_change` 改为接收必选 `McpRuntimeAccess`，从类型层面禁止再次传入 `None` 假成功。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/mcp_settings.rs`
- 修改：`crates/vulcan-service-kernel/src/settings_runtime_projection.rs`
- 修改：`crates/vulcan-service-kernel/src/tests.rs`
- 新增：`crates/vulcan-service-kernel/src/tests/mcp_settings_tests.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo test -p vulcan-service-kernel mcp_settings -- --nocapture`：通过，2 个测试。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `git diff --check -- crates/vulcan-service-kernel/src/mcp_settings.rs crates/vulcan-service-kernel/src/settings_runtime_projection.rs crates/vulcan-service-kernel/src/tests.rs crates/vulcan-service-kernel/src/tests/mcp_settings_tests.rs`：通过；仅有 Git 行尾转换提示。

### 审核结论

两轮审核确认：新增同 id 不再覆盖旧配置，编辑缺失 id 不再创建影子条目；MCP runtime 重建失败会阻断保存，持久化失败会关闭新 runtime。当前 `bootstrap_mcp_runtime_access` 真实失败路径较少，但设置页已经与启动阶段保持硬错误语义一致。

## 2026-07-05 第 41 轮：OpenRouter 二级路由空标识拒绝

### 本轮目标

修复 OpenRouter 二级供应商选择保存链路中的空对象误清除风险。目标是把“用户明确清除二级路由”和“前端或 RPC 传入无效 route 对象”拆成两个不同语义，避免坏请求被静默当作 Auto 清除。

### 问题链路

1. WebUI 选择 OpenRouter 二级供应商时调用 `service_select_workbench_model_secondary_route`。
2. `route: null` 表示用户点击 Auto，应该清除当前模型的二级供应商覆盖。
3. 旧实现对 `Some(route)` 直接执行 `openrouter_route_selection_from_service`，该 helper 即使 `routeId` 和 `tag` 都为空，也会构造一个空 `OpenRouterEndpointRouteSelection`。
4. `set_openrouter_endpoint_route_for_model` 会把空 selection 过滤掉并执行 remove。
5. 因此 malformed `Some({ routeId: "", tag: "" })` 与合法 `None` 的效果相同，都会清除用户当前配置。
6. 这会让 RPC 调用错误、前端状态损坏或第三方脚本误调用表现成无提示配置丢失。

### 核心修复

1. `openrouter_route_selection_from_service` 返回类型改为 `Result<OpenRouterEndpointRouteSelection, String>`。
2. 新版 `tag` 优先，缺少 `tag` 时继续接受旧版 `routeId`，保持当前协议兼容。
3. 当 `tag` 和 `routeId` 归一化后都为空时，直接返回面向用户的错误，不再构造空 selection。
4. `service_select_workbench_model_secondary_route` 使用 `.transpose()?` 显式传播无效 route 错误。
5. `None` 仍然是唯一合法的清除语义，继续传入 provider-config setter 执行 remove。
6. 新增测试覆盖旧版 `routeId` 回退、新版 `tag` 优先和空标识拒绝三类边界。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/workbench_model_routes.rs`
- 修改：`crates/vulcan-service-kernel/src/workbench_model_routes/openrouter_routes.rs`
- 修改：`crates/vulcan-service-kernel/src/tests.rs`
- 新增：`crates/vulcan-service-kernel/src/tests/workbench_model_routes_tests.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo test -p vulcan-service-kernel workbench_model_routes -- --nocapture`：通过，4 个测试。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `git diff --check -- crates/vulcan-service-kernel/src/workbench_model_routes.rs crates/vulcan-service-kernel/src/workbench_model_routes/openrouter_routes.rs crates/vulcan-service-kernel/src/tests.rs crates/vulcan-service-kernel/src/tests/workbench_model_routes_tests.rs`：通过；仅有 Git 行尾转换提示。

### 审核结论

三轮审核确认：前端 Auto 仍发送 `route: null`，明确选择仍从后端 route 摘要复制 `routeId/tag`；provider-config setter 的空 selection 清除语义没有继续暴露给外部坏请求；service-api 的默认空字段已由 service helper 显式拒绝。当前聚焦范围内未发现新的问题。

## 2026-07-05 第 42 轮：Provider 启用模型保存拒绝未知标识

### 本轮目标

修复 Provider 启用模型保存链路中未知模型 id 被静默过滤的问题。目标是让保存请求边界显式拒绝坏输入，而不是让 UI/RPC 误以为保存成功。

### 问题链路

1. WebUI 保存启用模型进入 `service_save_provider_enabled_models`。
2. 正常弹窗只会从 `catalogModels` 中提交模型 id。
3. 旧后端对所有请求 id trim 后，只保留 `catalog_ids.contains` 的项。
4. 非空未知 id 会被静默丢弃，保存仍返回成功。
5. 这与 `service_save_provider_model_override` 对未知模型硬拒绝的边界语义不一致，也会掩盖第三方 RPC/前端状态损坏。
6. 读取层 `resolved_enabled_model_ids` 过滤失效持久化 id 是历史目录刷新容错，不应复用为保存请求容错。

### 核心修复

1. 新增 `normalize_provider_enabled_model_ids_for_save`，集中执行 trim、空白忽略、稳定去重和未知 id 校验。
2. 空白字符串视作表单噪音继续忽略。
3. 非空未知模型 id 直接返回错误，并列出 provider 与被拒绝 id。
4. `service_save_provider_enabled_models` 改为先校验请求，再调用 `set_enabled_models` 持久化。
5. 新增测试覆盖合法列表归一化和未知模型拒绝。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/provider_settings.rs`
- 修改：`crates/vulcan-service-kernel/src/tests.rs`
- 新增：`crates/vulcan-service-kernel/src/tests/provider_settings_tests.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --check`：通过。
- `cargo test -p vulcan-service-kernel provider_settings -- --nocapture`：通过，2 个测试。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `git diff --check -- crates/vulcan-service-kernel/src/provider_settings.rs crates/vulcan-service-kernel/src/tests.rs crates/vulcan-service-kernel/src/tests/provider_settings_tests.rs`：通过；仅有 Git 行尾转换提示。

### 审核结论

三轮审核确认：前端正常提交仍来自 catalog；坏 RPC 不再被静默过滤成成功保存；Provider 模型能力覆盖和启用模型保存的未知 id 语义已对齐。读取层对失效持久化 id 的容错保留在 provider-config，不扩散到保存边界。

## 2026-07-05 第 43 轮：会话水合必须同时恢复未关闭子智能体

### 本轮目标

修复切换会话后子智能体列表延迟出现、时而缺失，以及无子智能体短会话也在定位阶段卡顿的问题。目标是让会话正文窗口和未关闭子智能体元数据成为同一次会话初始化的就绪条件。

### 问题链路

1. 前端只用 hydrated window 是否存在判断会话已水合，没有检查 `subagentMetadataLoaded`。
2. 后端 retained 快路径只要正文已缓存就直接返回，可能跳过未关闭子智能体元数据预载。
3. 子智能体预载入口原本依赖全量 mirror 分组，再从目标 root 做过滤，容易把无关会话的解析成本带进会话切换。
4. 结果是 UI 可能先显示正文、后续才补出子智能体导航，或者必须等下一次工具调用才看见完整列表。

### 核心修复

1. 前端 `selectedConversationHydrated` 改为要求 `subagentMetadataLoaded=true`。
2. 后端 retained 快路径在正文已缓存但元数据未预载时，只补齐当前 root 的子智能体元数据，再返回 ready workbench。
3. 新增 root-tree 聚焦 mirror 读取接口，只遍历目标 root 可达的子智能体树。
4. 空结果也会标记 root 已预载，避免没有子智能体的会话反复走恢复索引。
5. 修复相关测试辅助的 Clippy 默认值重赋值问题。

### 文件变更

- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx`
- 修改：`apps/vulcan-code-web/src/components/workspace/workbench-layout/conversation-hydration.ts`
- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/store_impl.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/mirror.rs`
- 修改：`crates/vulcan-task-session/src/segmented_store/tests.rs`
- 修改：`crates/vulcan-task-session/src/lib.rs`
- 新增完成文档：`docs/completed/20260705/27-SUBAGENT_SESSION_HYDRATION_READY_GATE.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `pnpm --dir apps/vulcan-code-web lint`：通过。
- `cargo check -p vulcan-task-session`：通过。
- `cargo check -p vulcan-service-kernel`：通过。
- `cargo test -p vulcan-task-session mirror -- --nocapture`：通过，3 个测试。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，8 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，23 个测试。
- `cargo clippy -p vulcan-task-session --tests -- -D warnings`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `git diff --check`：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮审核确认：前端就绪门禁已覆盖正文与子智能体元数据；后端 retained 路径不会再返回缺少子智能体索引的 ready 状态；root-tree mirror 读取避免了无关会话全量分组。当前聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 44 轮：保留会话选择必须原子恢复子智能体导航

### 本轮目标

继续修复会话切换后子智能体列表不稳定、打开会话后等待很久才出现子智能体导航，以及无子智能体短会话也在“定位会话”阶段卡顿的问题。目标是让保留会话选择响应本身就包含当前 root 会话的未关闭子智能体状态，并把任务存储定位从项目级扫描改为确定路径优先。

### 问题链路

1. `service_select_conversation` 的保留正文快路径会直接投影 cached `subagent_navigation`，不会保证当前 root 的未关闭子智能体元数据已预载。
2. `subagent_navigation_snapshot_from_cached_state` 是纯缓存读取，不读磁盘；如果预载尚未完成，就会返回 0 或旧数量。
3. 前端收到半成品 workbench 后，会表现为正文已显示、智能体列表晚几秒才出现，或者切出去再回来又短暂消失。
4. 子智能体任务存储根目录定位还存在项目目录后缀扫描，导致没有子智能体的小会话也可能在“定位会话”阶段等待数秒。
5. 这不是单纯前端刷新问题，而是后端会话选择快照的就绪边界不完整。

### 核心修复

1. 保留会话选择前先判断当前 root 是否已经完成子智能体元数据预载。
2. 未完成时，后端释放状态锁后只针对当前 root 做一次未关闭子智能体元数据预载，再重新加锁应用。
3. `select_conversation_in_retained_state` 在投影 `subagent_navigation` 前应用预载结果，保证选择响应内就是完整状态。
4. 新增确定路径定位函数，使用 `SessionSummary.created_at` 与 `project_id` 直接查 active / archived 分段会话目录。
5. `webui_task_session_store_root_for_parent_session` 优先使用确定路径定位，只有历史数据缺少时间或目录不存在时才回退扫描。
6. 补充测试覆盖无子智能体空预载、有子智能体选择即恢复导航、以及分段目录确定路径定位。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/lib.rs`
- 修改：`crates/vulcan-service-kernel/src/project_conversation_services/conversation_services.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/session_lifecycle.rs`
- 新增完成文档：`docs/completed/20260705/28-SUBAGENT_SELECTION_PRELOAD_ATOMICITY.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，10 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，23 个测试。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo test -p vulcan-workspace-store task_store_locator -- --nocapture`：通过。
- `cargo clippy -p vulcan-workspace-store --tests -- -D warnings`：通过。
- `cargo fmt --all`：已执行。

### 审核结论

三轮审核确认：保留会话选择不会持锁读磁盘；子智能体目录定位优先走确定路径，不再把项目级扫描放在主路径；新增测试已设置会话 `created_at` 并真实覆盖确定路径。当前聚焦范围内未发现新的问题。

## 2026-07-05 第 45 轮：分段会话目录定位必须使用稳定创建时间

### 本轮目标

继续审计会话切换和未关闭子智能体恢复路径，修复第 44 轮“确定路径优先”实现中仍可能导致慢加载和恢复不稳定的底层时间语义问题。

### 问题链路

1. 服务层 `SessionSummary.created_at` 是 Unix 毫秒文本，但之前新增的确定路径定位函数语义更像目录日期字符串，测试又手工创建了 `100-{session_id}` 目录，导致错误实现也能通过。
2. workspace-store 的 `segmented_session_locator_from_summary` 曾把 `updated_at` 写入 locator 的 `created_at` 字段，会让分段目录定位随会话更新漂移。
3. 会话消息投影、分叉来源读取、提示词继续执行和子智能体工作区变更路径仍未完全统一目录解析方式。
4. 审核时发现 service 共享 helper 还保留确定路径失败后的项目级扫描兜底；按长期方案不应保留，因为它会让坏数据继续拖慢会话打开。

### 核心修复

1. 新增 `find_segmented_session_directory_for_task_store_created_at`，明确接收原始 Unix 毫秒 `created_at`，由 workspace-store 内部转换生产目录日期。
2. `segmented_session_locator_from_summary` 改用 `normalized_session_created_at(session)`，不再用 `updated_at` 冒充创建时间。
3. service-kernel 增加并统一使用 `segmented_session_directory_for_summary` / `segmented_session_directory_for_workspace_session`，主链路只走确定路径。
4. 分叉来源读取新增已解析目录入口，避免上层拿到目录后再按项目扫描。
5. `subagent_navigation_tests` 的持久化预载夹具改为生产形态目录 `1970-01-01-{session_id}`，并给对应 `SessionSummary` 显式写入同一 `created_at`。
6. 删除 service 主链路的项目级后缀扫描兜底，只保留 workspace-store 底层旧函数定义，不再被会话切换路径调用。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/locators.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/persistence/tests.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/runtime_logs.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/session_lifecycle.rs`
- 修改：`crates/vulcan-service-kernel/src/runtime_workspace_helpers.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_message_projection.rs`
- 修改：`crates/vulcan-service-kernel/src/webui_prompt_lifecycle.rs`
- 修改：`crates/vulcan-service-kernel/src/webui_prompt_lifecycle/precheck_continue.rs`
- 修改：`crates/vulcan-service-kernel/src/subagent_workspace_mutation.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/conversation_selection_tests.rs`
- 修改：`crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs`
- 新增完成文档：`docs/completed/20260705/29-SEGMENTED_SESSION_CREATED_AT_LOCATOR_FIX.md`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --all`：通过。
- `cargo test -p vulcan-workspace-store task_store_locator_finds_session_directory_by_created_at -- --nocapture`：通过。
- `cargo test -p vulcan-workspace-store segmented_session_locator_uses_created_at_not_updated_at -- --nocapture`：通过。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，10 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，23 个测试。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `cargo clippy -p vulcan-workspace-store --tests -- -D warnings`：通过。
- `git diff --check`（聚焦文件）：通过；仅有 Git 行尾转换提示。

### 审核结论

循环审核先发现并修复了 service helper 的旧扫描兜底；随后测试暴露出若干非生产形态 fixture，已统一修复为真实 created_at 定位语义。最终确认 service 主链路不再调用项目级扫描，created_at / updated_at 语义分离，未关闭子智能体恢复测试覆盖了确定路径。

## 2026-07-05 第 46 轮：任务镜像状态修复不应保留项目级扫描入口

### 本轮目标

继续沿会话切换与子智能体恢复性能路径审计，确认第 45 轮之后是否仍有公开 API 或业务调用可以绕过 `created_at` 确定性目录定位，重新触发项目级目录扫描。

### 问题链路

1. `find_segmented_session_directory_for_task_store` 已不再被业务调用，但仍作为 workspace-store 公开 re-export 存在。
2. `load_segmented_session_fork_source_for_project` 同样没有业务调用，却内部保留 `project_id + session_id` 的项目级目录扫描。
3. `task_mirror_status_projection` 仍调用 `load_segmented_session_task_mirrors_for_project`，会在会话水合、刷新和 surface 修复时按项目目录扫描父会话目录。
4. 这些入口与当前长期方向冲突：会话主链路应使用 `SessionSummary.created_at` 解析确定目录，再从已解析目录读取所需数据。

### 核心修复

1. 删除 `find_segmented_session_directory_for_task_store` 公开 re-export 与函数本体，只保留 `find_segmented_session_directory_for_task_store_created_at`。
2. 删除 `load_segmented_session_task_mirrors`、`load_segmented_session_task_mirrors_for_project` 和 `load_segmented_session_fork_source_for_project` 这些会扫描目录的公开入口。
3. 将原私有 `load_segmented_session_task_mirrors_from_dir` 提升为公开 `load_segmented_session_task_mirrors_from_directory`，语义明确为“已解析目录读取”。
4. `load_segmented_session_task_mirrors_for_service` 改为接收 `WorkspaceSnapshot`，通过 `segmented_session_directory_for_workspace_session` 使用 `created_at` 确定性定位目录。
5. `apply_persisted_task_mirror_statuses_to_workspace` 与 `repair_task_mirror_statuses_for_surface_messages_locked` 统一走新的 service helper。
6. 新增 `task_mirror_rows_load_from_resolved_session_directory` 测试，覆盖从已解析目录读取父级 task mirror 行。

### 文件变更

- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/runtime_logs.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/segmented_session_store/tests/session_lifecycle.rs`
- 修改：`crates/vulcan-service-kernel/src/task_mirror_status_projection.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --all`：通过。
- `cargo test -p vulcan-workspace-store task_mirror_rows_load_from_resolved_session_directory -- --nocapture`：通过。
- `cargo test -p vulcan-workspace-store session_lifecycle -- --nocapture`：通过，4 个测试。
- `cargo test -p vulcan-service-kernel stop_task_mirror -- --nocapture`：通过，6 个测试。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，10 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，23 个测试。
- `cargo clippy -p vulcan-workspace-store --tests -- -D warnings`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `rg` 检索 `find_segmented_session_directory_for_task_store`、`load_segmented_session_task_mirrors_for_project`、`load_segmented_session_task_mirrors`、`load_segmented_session_fork_source_for_project` 在代码范围内无结果。
- `git diff --check`（聚焦文件）：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮审核确认：任务镜像状态修复不再通过项目级扫描读取父会话 metadata；旧的公开扫描入口已从代码中删除，后续新调用只能选择 `created_at` 确定路径或已解析目录读取。当前聚焦范围内未发现新的阻塞问题。

## 2026-07-05 第 47 轮：会话水合与历史读取入口禁止回退目录扫描

### 本轮目标

继续沿会话打开变慢、未关闭智能体列表延迟出现的问题审计，清理第 46 轮之后仍保留在会话水合与历史读取路径上的旧扫描入口，确保切换会话时只通过 `SessionSummary.project_id + created_at + session_id` 进行确定性目录定位。

### 问题链路

1. workspace-store 已有 `created_at` 确定定位能力，但 service-kernel 的提交前水合、选中会话展示水合、运行态补水合、历史分页和工具详情磁盘回退仍调用旧 `load_session_*` API。
2. 旧 `load_session_*` API 内部会通过 `session_id` 或 `project_id + session_id` 查找目录，存在全局或项目级目录扫描路径。
3. 这些旧入口虽然在当前主流程里可以被替换，但仍作为 workspace-store 公开 re-export 暴露，容易让后续代码重新接回慢路径。
4. 审核时继续发现未导出的工具详情分页函数也保留 `query_session_locator` 按 id 查目录，虽然无调用点，但与“禁止回退扫描”的长期方向冲突。

### 核心修复

1. `load_workspace_for_submission`、`load_selected_session_display_hydration`、`hydrate_retained_submission_session_if_needed`、`hydrate_selected_session_runtime_if_needed` 统一从 `WorkspaceSnapshot.sessions` 读取 `project_id + created_at`，并调用 created-at 版本水合 API。
2. `service_session_history_page` 改为从当前 workspace snapshot 提取确定定位字段，再调用 `load_session_history_page_for_project_created_at`；缺少定位字段时返回空分页，不再扫描目录。
3. `service_session_tool_detail` 的磁盘回退改为 `load_session_transcript_window_for_project_created_at`，无法确定定位时直接走未找到结果，不再按 id 查目录。
4. workspace-store 顶层只导出 created-at 版本的会话水合、历史分页和最近窗口 API。
5. 删除旧的无 created_at / 仅 project 版本水合与历史窗口函数，删除旧 project-only locator。
6. 删除未使用的内部工具详情分页函数与最后的 `query_session_locator`，避免保留按 id 查找目录的扫描语义。

### 文件变更

- 修改：`crates/vulcan-service-kernel/src/session_hydration.rs`
- 修改：`crates/vulcan-service-kernel/src/session_hydration/loading.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_history.rs`
- 修改：`crates/vulcan-service-kernel/src/conversation_history/tool_detail.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/mod.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/locator.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/hydration.rs`
- 修改：`crates/vulcan-workspace-store/src/workspace_store/session_load/pages.rs`
- 修改：`TASK_LOG.md`

### 验证记录

- `cargo fmt --all`：通过。
- `cargo test -p vulcan-workspace-store session_load -- --nocapture`：通过，10 个测试。
- `cargo test -p vulcan-workspace-store session_lifecycle -- --nocapture`：通过，4 个测试。
- `cargo test -p vulcan-service-kernel conversation_selection -- --nocapture`：通过，10 个测试。
- `cargo test -p vulcan-service-kernel subagent_navigation -- --nocapture`：通过，23 个测试。
- `cargo test -p vulcan-service-kernel stop_task_mirror -- --nocapture`：通过，6 个测试。
- `cargo clippy -p vulcan-workspace-store --tests -- -D warnings`：通过。
- `cargo clippy -p vulcan-service-kernel --tests -- -D warnings`：通过。
- `rg` 检索旧扫描型 `load_session_*`、`query_session_locator`、`find_segmented_session_directory_by_id` 在 service 与 session_load 范围内无结果。
- `git diff --check`（聚焦文件）：通过；仅有 Git 行尾转换提示。

### 审核结论

本轮审核确认：会话切换、水合、历史分页和工具详情回退路径不再暴露或调用旧目录扫描入口；workspace-store 对外只保留 created-at 确定定位 API。当前聚焦范围内未发现新的问题。
