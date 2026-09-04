# VCE 原生完整对接五轮审核报告

## 1. 审核结论

- 审核对象：Vulcan Context Engine（VCE）与 VulcanCode 的原生完整对接、CBM 原生主链退出及对应管理面。
- 审核日期：2026-09-04。
- 最终结果：连续五轮完整审核均未发现新增问题，连续零问题轮数为 **5**。
- 架构结论：VCE 是宿主注入的代码语义逻辑内核；AI、SQLite、LanceDB、文件监听、工具注册、RPC 与页面均由 VulcanCode 持有。
- 工具结论：AI 仅新增 `codebase_retrieval(information_request)`；同步、状态、维护与管理查询均为宿主入口。
- 迁移结论：CBM Controller、managed MCP、原生工具、页面入口和安装/同步/打包脚本已退出；旧 `codebase_memory_enabled` 仅用于一次性设置迁移，历史磁盘数据未删除。

审核规则为：任何一轮发现真实问题并修复后，连续计数归零，随后重新从第 1 轮覆盖全部范围。本文只把最后一次连续五轮无新增问题的审核计入完成门槛。

## 2. 正式计数前发现与修复

| 序号 | 发现 | 修复与验证 |
| --- | --- | --- |
| 1 | SQLite 的 `INSERT ... SELECT ... ON CONFLICT` 在当前解析规则下存在语法歧义。 | 在 `SELECT` 后增加明确恒真条件，真实 Controller 初始化与迁移矩阵通过。 |
| 2 | 严格 Clippy 发现两个可折叠条件和一个可改用 `?` 的错误分支。 | 完成等价重构；目标 crate 严格 Clippy 通过。 |
| 3 | 注释曾把 VCE、VMM、知识库的共同 HostAI 契约误写成共同注册实例。 | 明确三者只共享规则与宿主服务，各自使用独立 consumer、registration、owner 和向量空间。 |
| 4 | 两份说明仍称 VMM-Core 直接向 VCE 提供 LLM、Embedding 与 Rerank。 | 改为 VulcanCode 分别注册 VCE、VMM、知识库消费者，业务状态和数据空间完全隔离。 |
| 5 | 初次全量扫描先于 watcher 注册，扫描窗口内文件事件可能丢失。 | 先注册监听并原子发布项目根映射，再启动首次全量扫描；增加竞态回归测试。 |
| 6 | watcher 批次被取消时可能丢弃 dirty set，移除项目后陈旧重试也可能重新进入队列。 | 仅对仍受监听的项目重排失败/取消批次，移除项目无法被陈旧任务复活。 |
| 7 | 检查点提交只验证新增/删除项，没有证明下一成员集合与当前成员的精确转换；exact 查询又在 SQL `LIMIT` 后过滤允许 Blob。 | 检查 `next = (current - deleted) ∪ staged`；exact 查询在 SQL 内先限制 checkpoint Blob，再执行上限。 |
| 8 | WebUI 设置保存发生 revision 冲突时会用陈旧 props 恢复草稿。 | 保存被拒后重新读取并校验当前项目的权威概览，拒绝跨项目恢复响应，并补充回归测试。 |

以上任一修复均使此前连续审核计数失效。最后一项修复提交为 `edff3dc`，之后重新执行下列五轮。

## 3. 最终连续五轮审核

### 第 1 轮：架构、依赖、领域隔离与 CBM 退出

- 直接依赖树仅包含 `futures`、`serde`、`serde_json`、`sha2` 和工作区统一 `workspace-hack`。
- VCE 内核源码与清单不依赖 service kernel、workspace store、agent runtime、VMM、CBM、Milvus、LanceDB、SQLite 或 watcher 实现。
- VCE 使用 `vce` consumer profile、`vce-project:{project_id}:inference` 注册身份和 `code-semantic:{project_id}:{root_hash}` 向量空间；VMM 使用独立 `vmm` 身份，知识库使用独立知识库 consumer。
- `workspace_isolation` 两项黑盒测试通过；原 CBM 运行入口和脚本名在有效运行源码中无命中。
- 结论：零新增问题，连续计数 **1/5**。

### 第 2 轮：状态机、并发、取消、关闭与恢复

- 复核首次激活、全量/增量刷新、dirty 合并、失败重排、项目移除、关闭取消、重启恢复和错误投影。
- VCE 全特性测试 191 项通过、1 项性能基线忽略；Service Kernel 的 VCE 生命周期测试 25 项、关闭测试 3 项通过。
- VCE、Service Kernel VCE、Workspace Store VCE 生产路径未发现 `panic!`、`unwrap`、`expect`、`todo!`、`unimplemented!` 或 `unreachable!`。
- Service Kernel `--lib -D warnings` Clippy 通过。
- 结论：零新增问题，连续计数 **2/5**。

### 第 3 轮：SQLite、LanceDB、CAS、幂等与迁移

- 复核 generation 暂存、Blob/Chunk/符号写入、检查点精确成员转换、SQLite 权威发布、LanceDB 过滤、失败中止、重复回放和孤立数据识别。
- Workspace Store VCE 单元测试 5 项通过；完整库测试 208 项通过、8 项忽略。
- 三项串行真实 Controller 测试分别验证绑定轮换与伪造检查点拒绝、引擎端到端及重启/多项目隔离、CBM 设置迁移矩阵幂等，全部通过。
- Workspace Store `--lib -D warnings` Clippy 通过。
- 结论：零新增问题，连续计数 **3/5**。

### 第 4 轮：工具暴露链、项目权限、RPC 与 WebUI

- `VceToolCatalog` 恰有一个 AI 可见入口；Schema 只接收非空 `information_request`，额外的 project/path/namespace/model 参数均被拒绝。
- Native Tool Catalog、分析策略、工具桥接、执行上下文 handler、当前项目身份和模型结果投影链完整；HostOnly 定义不会进入模型工具列表。
- 所有 RPC 从权威 workbench 项目集合解析项目；设置写入使用 optimistic revision，维护执行使用 project/revision/checkpoint/action IDs，重试和取消校验可见 operation。
- revision 冲突后 WebUI 重新读取权威概览，不再用陈旧 props 恢复草稿；浏览器无服务模式显式呈现 unavailable/failed，不伪造 ready。
- Agent Runtime 818 项、Service API 58 项、Workbench Model 60 项、工具契约 10 项、工具桥接 7 项、代码上下文定向 7 项测试通过。
- 结论：零新增问题，连续计数 **4/5**。

### 第 5 轮：全量回归、性能、文档、资源与死代码

- WebUI 全量 142 个测试文件、868 项测试通过；TypeScript 构建检查通过。
- Service Kernel 全量 883 项、Workspace Store 全量 208 项、Agent Runtime 818 项、Service API 58 项、Workbench Model 60 项测试通过。
- Rustfmt、`git diff --check`、VCE doctest、VCE 全目标全特性 Clippy及各宿主目标库 Clippy 通过。
- 复核 Make 入口、runtime asset 同步边界、开发环境说明和 CBM 退役说明；没有遗留 CBM 可执行资源或脚本入口。
- 复核全部任务提交路径与当前工作树；并行 VMM 文件未进入 VCE 提交。
- 结论：零新增问题，连续计数 **5/5**，达到完成门槛。

## 4. 验证矩阵

| 验证 | 结果 |
| --- | --- |
| `cargo test -p vulcan-context-engine --all-features` | 191 通过，1 个显式性能基线忽略 |
| `cargo clippy -p vulcan-context-engine --all-targets --all-features -- -D warnings` | 通过 |
| `cargo test -p vulcan-context-engine --doc --all-features` | 通过，当前无 doctest 用例 |
| `cargo test -p vulcan-workspace-store --lib` | 208 通过，8 个显式集成/性能用例忽略 |
| 三项真实 Controller 测试（`--ignored --test-threads=1`） | 全部通过 |
| `cargo test -p vulcan-service-kernel --lib` | 883 通过 |
| `cargo test -p vulcan-agent-runtime --lib` | 818 通过 |
| `cargo test -p vulcan-service-api --lib` | 58 通过 |
| `cargo test -p vulcan-workbench-model --lib` | 60 通过 |
| WebUI `pnpm test` | 142 个文件、868 项通过 |
| WebUI `pnpm run lint` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `git diff --check` | 通过 |

补充基线：Workspace Store 的全目标历史测试代码存在 487 项既有 Clippy 告警，扫描未命中 VCE 新增路径；本任务没有越界整理旧测试。用于发布路径的 `--lib -D warnings` 为零问题。

## 5. 性能与存储基线

固定离线夹具为 128 个 Rust 文件、每文件 12 个函数：

| 指标 | 本轮结果 |
| --- | --- |
| 首次全量刷新 | 80 ms |
| 单文件增量刷新 | 1 ms |
| 检索 | 252 μs |
| 首次 Embedding 调用 | 256 |
| 增量 Embedding 调用 | 2 |
| 默认 Query Rewrite 调用 | 0 |
| Rerank 调用 | 1 |
| 序列化状态快照 | 49,384 bytes |
| 引擎保留源码正文 | 0 bytes |

真实 Controller 单 Blob 夹具存储：SQLite 196,608 bytes，LanceDB 18,940 bytes，总计 215,622 bytes。该数据仅是固定小夹具回归基线，不外推为大型项目容量承诺。

## 6. 外部验收边界

- 本地测试没有使用生产 Provider 凭据，因而没有把真实网络延迟、配额、计费和生产模型质量宣称为已验收；HostAI 请求/响应、取消和错误映射已由伪宿主与契约测试覆盖。
- 没有一份冻结的、可在相同项目与相同模型配置下复现的 CBM 质量语料，因此不宣称 VCE 与 CBM 检索质量数值等价；主链不存在运行时双写或自动回退 CBM。
- 当前产品没有已确认的权威“未保存编辑器文档”事件源，因此本次接入保持 filesystem-only；VCE 的显式 `observe/remove` 入口保留为宿主功能入口，但未虚构 UI 事件字段。
- 用户历史 CBM 磁盘数据和既有同步二进制未自动删除；后续如提供 CBM 插件，应使用独立生命周期和工具身份。

## 7. 逐文件覆盖台账

状态说明：`通过` 表示源码、契约、测试或文档已纳入五轮复核；`删除复核` 表示确认删除后不存在运行入口、引用或资源同步残留。

### 7.1 根工作区、构建与文档

| 文件 | 状态 |
| --- | --- |
| `AGENTS.md` | 通过 |
| `Cargo.toml` | 通过 |
| `Cargo.lock` | 通过 |
| `Makefile` | 通过 |
| `make.ps1` | 通过 |
| `make.sh` | 通过 |
| `docs/development-environment.md` | 通过 |
| `docs/architecture/codebase-memory-managed-mcp.md` | 通过 |
| `docs/reviews/20260904-vce-workspace-import-review.md` | 通过 |
| `docs/completed/20260904/01-VCE_FULL_NATIVE_INTEGRATION.md` | 通过（最终归档方案） |

### 7.2 VCE crate

| 文件 | 状态 |
| --- | --- |
| `crates/vulcan-context-engine/Cargo.toml` | 通过 |
| `crates/vulcan-context-engine/LICENSE` | 通过 |
| `crates/vulcan-context-engine/README.md` | 通过 |
| `crates/vulcan-context-engine/docs/oce-code-semantic-logic-extraction.md` | 通过 |
| `crates/vulcan-context-engine/docs/vce-functional-entry.md` | 通过 |
| `crates/vulcan-context-engine/docs/vce-tool-surface.md` | 通过 |
| `crates/vulcan-context-engine/docs/vulcancode-native-integration.md` | 通过 |
| `crates/vulcan-context-engine/docs/vulcancode-preintegration.md` | 通过 |
| `crates/vulcan-context-engine/src/chunking.rs` | 通过 |
| `crates/vulcan-context-engine/src/embedding.rs` | 通过 |
| `crates/vulcan-context-engine/src/engine.rs` | 通过 |
| `crates/vulcan-context-engine/src/host_adapter.rs` | 通过 |
| `crates/vulcan-context-engine/src/identity.rs` | 通过 |
| `crates/vulcan-context-engine/src/indexing.rs` | 通过 |
| `crates/vulcan-context-engine/src/lib.rs` | 通过 |
| `crates/vulcan-context-engine/src/management.rs` | 通过 |
| `crates/vulcan-context-engine/src/model.rs` | 通过 |
| `crates/vulcan-context-engine/src/path_index.rs` | 通过 |
| `crates/vulcan-context-engine/src/ports.rs` | 通过 |
| `crates/vulcan-context-engine/src/query.rs` | 通过 |
| `crates/vulcan-context-engine/src/retrieval.rs` | 通过 |
| `crates/vulcan-context-engine/src/state.rs` | 通过 |
| `crates/vulcan-context-engine/src/structure.rs` | 通过 |
| `crates/vulcan-context-engine/src/symbol.rs` | 通过 |
| `crates/vulcan-context-engine/src/tooling.rs` | 通过 |
| `crates/vulcan-context-engine/src/workspace.rs` | 通过 |
| `crates/vulcan-context-engine/tests/host_ai_contract.rs` | 通过 |
| `crates/vulcan-context-engine/tests/public_surface.rs` | 通过 |
| `crates/vulcan-context-engine/tests/workspace_isolation.rs` | 通过 |

### 7.3 Agent Runtime 与工具链

| 文件 | 状态 |
| --- | --- |
| `crates/vulcan-agent-runtime/Cargo.toml` | 通过 |
| `crates/vulcan-agent-runtime/src/lib.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/config.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/execution_models.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation/codebase_tool_routing_prompt.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation/tests.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation/tool_exposure.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/mod.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/prompt_compaction.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_launch_payload.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/preparation_config.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/tooling/bridge.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/tooling/bridge/descriptors.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/tooling/bridge/tests.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/tooling/native.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/tooling/native/codebase_retrieval.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/tooling/native/tests.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/tooling/native/tests/planning.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/tooling/native/tests/shell.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/tooling/native/tests/terminal.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/workbench_prompt_turn.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/mcp/config.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/mcp/mod.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/mcp/runtime.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/mcp/runtime/handle.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/mcp/runtime/tools.rs` | 通过 |
| `crates/vulcan-agent-runtime/src/mcp/managed_codebase_memory.rs` | 删除复核 |

### 7.4 Service API、Kernel、Workbench 与 Store

| 文件 | 状态 |
| --- | --- |
| `crates/vulcan-service-api/src/command_events.rs` | 通过 |
| `crates/vulcan-service-api/src/conversation.rs` | 通过 |
| `crates/vulcan-service-api/src/lib.rs` | 通过 |
| `crates/vulcan-service-api/src/vce.rs` | 通过 |
| `crates/vulcan-service-api/src/workbench.rs` | 通过 |
| `crates/vulcan-service-api/src/workspace_context.rs` | 通过 |
| `crates/vulcan-service-kernel/Cargo.toml` | 通过 |
| `crates/vulcan-service-kernel/src/fallback_workbench_projection.rs` | 通过 |
| `crates/vulcan-service-kernel/src/git/commit_summary.rs` | 通过 |
| `crates/vulcan-service-kernel/src/git_diff_services.rs` | 通过 |
| `crates/vulcan-service-kernel/src/git_watcher.rs` | 通过 |
| `crates/vulcan-service-kernel/src/lib.rs` | 通过 |
| `crates/vulcan-service-kernel/src/project_conversation_services/project_services.rs` | 通过 |
| `crates/vulcan-service-kernel/src/rpc_dispatch.rs` | 通过 |
| `crates/vulcan-service-kernel/src/rpc_dispatch/extensibility.rs` | 通过 |
| `crates/vulcan-service-kernel/src/rpc_dispatch/vce.rs` | 通过 |
| `crates/vulcan-service-kernel/src/service_bootstrap.rs` | 通过 |
| `crates/vulcan-service-kernel/src/service_state_access.rs` | 通过 |
| `crates/vulcan-service-kernel/src/settings_runtime_projection.rs` | 通过 |
| `crates/vulcan-service-kernel/src/state.rs` | 通过 |
| `crates/vulcan-service-kernel/src/tests/detached_subagent_tests.rs` | 通过 |
| `crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs` | 通过 |
| `crates/vulcan-service-kernel/src/tests/test_support.rs` | 通过 |
| `crates/vulcan-service-kernel/src/vce/ai_port.rs` | 通过 |
| `crates/vulcan-service-kernel/src/vce/manager.rs` | 通过 |
| `crates/vulcan-service-kernel/src/vce/mod.rs` | 通过 |
| `crates/vulcan-service-kernel/src/vce/services.rs` | 通过 |
| `crates/vulcan-service-kernel/src/vce/source_port.rs` | 通过 |
| `crates/vulcan-service-kernel/src/vce/watcher.rs` | 通过 |
| `crates/vulcan-service-kernel/src/webui_prompt_lifecycle/precheck_continue.rs` | 通过 |
| `crates/vulcan-service-kernel/src/workbench_refresh.rs` | 通过 |
| `crates/vulcan-service-kernel/src/workspace_files.rs` | 通过 |
| `crates/vulcan-service-kernel/src/workspace_watcher.rs` | 通过 |
| `crates/vulcan-service-kernel/src/codebase_memory_controller.rs` | 删除复核 |
| `crates/vulcan-workbench-model/src/lib.rs` | 通过 |
| `crates/vulcan-workbench-model/src/project.rs` | 通过 |
| `crates/vulcan-workspace-store/Cargo.toml` | 通过 |
| `crates/vulcan-workspace-store/src/data_plane.rs` | 通过 |
| `crates/vulcan-workspace-store/src/lib.rs` | 通过 |
| `crates/vulcan-workspace-store/src/vce_data_plane.rs` | 通过 |
| `crates/vulcan-workspace-store/src/workspace_store/loading.rs` | 通过 |
| `crates/vulcan-workspace-store/src/workspace_store/mod.rs` | 通过 |
| `crates/vulcan-workspace-store/src/workspace_store/models.rs` | 通过 |
| `crates/vulcan-workspace-store/src/workspace_store/persistence/projects.rs` | 通过 |
| `crates/vulcan-workspace-store/src/workspace_store/queries.rs` | 通过 |
| `crates/vulcan-provider-adapters/src/configuration.rs` | 通过 |
| `crates/vulcan-tools-manager/src/tests.rs` | 通过 |

### 7.5 WebUI

| 文件 | 状态 |
| --- | --- |
| `apps/vulcan-code-web/src/components/features/code-context/code-context-area.tsx` | 通过 |
| `apps/vulcan-code-web/src/components/features/code-context/code-context-area.test.tsx` | 通过 |
| `apps/vulcan-code-web/src/components/features/code-context/code-context-pagination.ts` | 通过 |
| `apps/vulcan-code-web/src/components/features/code-context/code-context-pagination.test.ts` | 通过 |
| `apps/vulcan-code-web/src/components/project/project-settings-dialog.tsx` | 通过 |
| `apps/vulcan-code-web/src/components/project/project-settings-sections.tsx` | 通过 |
| `apps/vulcan-code-web/src/components/project/project-settings-types.ts` | 通过 |
| `apps/vulcan-code-web/src/components/project/project-settings-utils.tsx` | 通过 |
| `apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx` | 通过 |
| `apps/vulcan-code-web/src/i18n-messages.ts` | 通过 |
| `apps/vulcan-code-web/src/service/browser-fallback-workbench.ts` | 通过 |
| `apps/vulcan-code-web/src/service/browserTransport.ts` | 通过 |
| `apps/vulcan-code-web/src/service/vce-service.ts` | 通过 |
| `apps/vulcan-code-web/src/service/codebase-memory-service.ts` | 删除复核 |
| `apps/vulcan-code-web/src/state/workbench-state.ts` | 通过 |
| `apps/vulcan-code-web/src/types.ts` | 通过 |
| `apps/vulcan-code-web/src/types/conversation.ts` | 通过 |
| `apps/vulcan-code-web/src/types/projects-files.ts` | 通过 |
| `apps/vulcan-code-web/src/types/vce.ts` | 通过 |
| `apps/vulcan-code-web/src/types/workbench.ts` | 通过 |

### 7.6 运行资源与脚本

| 文件 | 状态 |
| --- | --- |
| `scripts/sync_tauri_runtime.ps1` | 通过 |
| `scripts/sync_tauri_runtime.sh` | 通过 |
| `scripts/install_codebase_memory_runtime.ps1` | 删除复核 |
| `scripts/package_codebase_memory_runtime.ps1` | 删除复核 |
| `scripts/sync_codebase_memory_runtime.ps1` | 删除复核 |

## 8. Git 与并行工作边界

- VCE 实施与修复均以独立中文提交保存。
- 最终文档提交只包含本报告与归档计划。
- 当前工作树中的 Native VMM 管理页面、API 和 Kernel 修改属于并行任务，未被本任务修改、暂存或提交。
- 已对集成主提交建立临时 detached worktree，并完成目标 crate 独立编译，避免仅在混合脏工作树上通过的假阳性。
