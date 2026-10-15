# VCE 退役与 CodeKit 收口方案

> 状态：执行中。用户于 2026-10-15 明确授权独立工作树并行执行，覆盖第 3 节的等待顺序；不主动合并、不推送。
> 方案依据 2026-10-15 对当前源码的查证；执行前按第 4 节逐项复核，事实以当时源码为准（AGENTS 第一节第 1 条）。

## 1. 背景与目标

代码导航先后尝试过两种“主机预建整库索引”的方案：

- `codebase_memory`（代码图谱，无向量成本）：启动慢、内存高，并与 CodeKit 严重冲突，已移除。
- VCE（`vulcan-context-engine`，向量 + BM25 + 重排）：本意补足 CodeKit，实际建库慢、查询慢、有嵌入与重排成本，
  效果常不如零 token 的 CodeKit。

两次失败的共同原因是：编码代理需要在**实时工作树**上快速、精确、可迭代的查询，这正是 CodeKit 的形态；
预建索引的建库成本、过期问题与资源占用换来的能力 CodeKit 基本都能覆盖。

目标：

1. 完整退役 VCE：AI 工具、引擎、服务集成、存储、WebUI/TUI 界面、持久配置与文档全部移除。
2. CodeKit 成为唯一内置代码导航方案，确认退役后其使用说明仍完整送达模型。
3. 需要语义代码检索的用户自行接入外部 MCP 服务，主机不再维护。
4. “不知道名称、只能描述概念”的需求由 AI 管理知识库中的架构笔记承担（见
   `docs/plan/20261015-01-AI_MANAGED_KNOWLEDGE_BASE.md`），本方案不另建任何索引。

## 2. 决策

| 编号 | 决策 |
| --- | --- |
| D1 | 不保留 VCE 的任何开关、隐藏入口或降级路径；不新增替代索引引擎 |
| D2 | 共享基础设施保留：LanceDB 引擎、SQLite/Jieba 分词、嵌入调用基础、`workspace_watcher`（Git 仍使用）；只删除 VCE 消费者 |
| D3 | 用户数据目录（如 `~/.vulcan-debug/code/vce/`、`~/.vulcan/code/vce/`）不由程序自动删除；执行记录写明路径，由用户手动删除 |
| D4 | 持久格式按 AGENTS 第一节第 12 条处理：应用库保持 v1，经 `vulcan-debug-db` 显式映射删除 VCE 表；其他持久文件递增各自版本并明确拒绝旧格式，不写兼容分支 |
| D5 | CodeKit 上游仓库（`LuaSkills/vulcan-codekit`）本方案不修改：当前安装的 0.2.5 已具备路由说明、长行预览与归属标注（见 4.3） |
| D6 | 不新增“调用者/引用查询”工具：`vulcan-codekit-rg` 对标识符检索时已返回所属函数，即调用者；有评测证据后再议 |

## 3. 与并行工作的协调（硬性前置条件）

Codex 会话正在执行 AI 管理知识库方案（`20261015-01`），截至 2026-10-15 已提交第一阶段（`3b11db3`），
第二阶段在工作树中未提交。两份方案大量共用文件，**本方案第 6 节的全部阶段必须在以下条件满足后才开始**：

1. `20261015-01` 文末执行记录显示全部阶段完成并已提交，或用户明确告知 Codex 会话已结束；
2. `git status` 中不存在知识库方案的未提交改动（存在则停止，不得暂存、还原或覆盖他人改动）；
3. 开始前重新执行第 4 节复核，因为知识库方案会改变下列共用文件的内容与行号。

共用冲突面（两份方案都会改）：

- AI 工具链：`tooling/native.rs`、`tooling/bridge.rs`、`tooling/bridge/descriptors.rs`、
  `agent_runtime/execution_models.rs`、`agent_runtime/runtime.rs`、`execution_preparation/dedicated_session.rs`、
  `tool_permission` 测试、`task_runtime/child_request_overrides.rs`
- 服务：`rpc_dispatch.rs`、`state.rs`、`service_bootstrap.rs`、`lib.rs`
- 存储：`data_plane.rs`、`bin/debug_database/*`、`local_database_runtime.rs`
- 配置：`vulcan-provider-adapters/src/configuration.rs`（推理配置档）
- 界面：WebUI 工作台布局、项目设置、输入区、工具展示、`crates/vulcan-i18n/catalogs/**`；TUI 页面、服务接口与模拟后端
- 文档：`AGENTS.md`、`tui-service-adaptation.md`、`tui-design.md`、`tool-result-presentation.md`、
  `debug-database-maintenance.md`、`dedicated-sessions.md`

**例外**：第 6 节阶段 0 只读取与新增测试，不改上述文件，可在前置条件满足前执行。

## 4. 现状事实（执行前逐项复核）

### 4.1 VCE 范围

| 层 | 位置 |
| --- | --- |
| 引擎 crate | `crates/vulcan-context-engine/`（约 3 万行，含 `docs/`、`tests/`）；工作区 `Cargo.toml` 第 28 行成员与第 101 行依赖 |
| AI 工具 | `tooling/native/codebase_retrieval.rs`；注册于 `tooling/native.rs`；`bridge.rs` 以 `vce_retrieval_handler` 决定是否可见；`execution_preparation/codebase_tool_routing_prompt.rs`（仅在 VCE 可见时注入的 VCE/CodeKit 路由提示）；专职会话过滤引用 `CODEBASE_RETRIEVAL_TOOL_ID` |
| 执行上下文 | `ExecutionPreparationRequest` 等处的 `vce_retrieval_handler` 字段（多处测试夹具填 `None`，如 `git/commit_summary.rs`） |
| 服务内核 | `crates/vulcan-service-kernel/src/vce/`（约 2.56 万行）、`rpc_dispatch/vce.rs`、`public_ai_settings/vce_embedding_settings.rs` 与 `publication/vce_compensation_tests.rs`、`configured_inference.rs` 中 VCE 嵌入诊断与 Batch 产物、`service_bootstrap/vce_startup_tests.rs`、`tests/vce_*`、`workspace_watcher.rs` 与 `workbench_refresh.rs` 中的 VCE 消费者、`webui_prompt_lifecycle/precheck_continue.rs` |
| 服务 API | `crates/vulcan-service-api/src/vce.rs`，以及 `startup.rs`、`workspace_context.rs`、`command_events.rs` 中的 VCE 字段 |
| 存储 | `crates/vulcan-workspace-store/src/vce_*.rs`、`vce_data_plane/`、`workspace_store/vce_batch_artifacts.rs`；`LogicalDatabase::VceMetadata`；应用库表 `project_vce_settings`（`data_plane.rs` 约 461 行）；`workspace_store/{loading,models,queries,persistence/projects}.rs` 中的项目 VCE 设置 |
| 工作台模型 | `crates/vulcan-workbench-model/src/project.rs::ProjectVceSettings` 及 `lib.rs` 导出 |
| 供应商适配 | `vulcan-provider-adapters/src/context_engine_embedding_policy.rs`；`native_inference_batch.rs`（Batch 嵌入，日志前缀 `vce-batch-artifact`） |
| 持久 AI 配置 | `configuration.rs`：`PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION = 19`；`inference_profiles` 默认含 `"vce"`，`validate_current_schema` 强制要求 `vce` 与 `vmm`；`context_engine_embedding_compatibility` 字段 |
| WebUI | `components/features/code-context/`、`components/features/context-engine/`、`service/vce-service.ts`、`service/vce-maintenance-service.test.ts`、`types/vce.ts`、项目设置与概览的 VCE 区块、工作台布局导航、`settings-actions-service.ts`、`browserTransport.ts`、`types/public-ai.ts`、`types/projects-files.ts`、输入区 `/semantic`（`composer-model.ts`，“打开 ContextEngine 控制”）、工具展示中的代码检索卡片、`tests/vce-live/` |
| TUI | `service_api/vce.rs`、`app/pages/*`、`backend/mock/service.rs` 与 `service-fixtures.json`、`backend/service/{events,worker}.rs`、`apps/vulcan-code-tui/AGENTS.md` |
| 文档与脚本 | `explicit-test-gates.md`（第 50、53、76、78 行）、`tool-result-presentation.md` 第 21 行、`tui-design.md`（第 112、183 行）、`tui-service-adaptation.md`、`provider-manifest-authoring.md`（约 800–825 行 Batch 段落）、`debug-database-maintenance.md`（`project_vce_settings.migrated_from_codebase_memory` 映射）、`scripts/backend-cjk-literals-baseline.json`、`scripts/wording-assertions-baseline.json`、`THIRD_PARTY_NOTICES*` |

### 4.2 需要判定是否随 VCE 删除的共享件

| 对象 | 判定规则 |
| --- | --- |
| `native_inference_batch.rs` 与其 `model_execution.rs` 接入 | 查证除 VCE 外是否有消费者。无则整体删除；供应商清单中的 Batch 能力声明属于受签名的供应商事实，不在本方案修改，在执行记录中列为后续事项 |
| `configured_inference.rs` 中 VCE 嵌入诊断、Batch 产物 | 删除 VCE 专属部分；通用推理逻辑保留 |
| `workspace_watcher.rs` | 保留，删除 VCE 订阅；确认 Git 消费者行为不变 |
| `vulcan-sqlite-engine/src/tokenizer.rs`、`vulcan-lancedb-engine` | 只更新提到 VCE 的注释，不改行为 |
| VCE 独有的第三方依赖 | 删除 crate 后执行 `cargo tree` 查证；无其他使用者的依赖从工作区 `Cargo.toml`、`workspace-hack` 与第三方声明中移除 |

### 4.3 CodeKit 现况（2026-10-15，已安装副本 `output/lua_runtime/skills/vulcan-codekit/`，版本 0.2.5）

- `help/main.md` 已有“选择工具”路由（repo-map → ast-tree → ast-detail → rg → node-source → patch）、
  证据规则（“结构工具只定位，不证明行为；声称行为前必须读正文”）与缩小范围规则。
- `rg` 对超过 4 KiB 的行使用以匹配为中心的预览，并报告原始长度；主机默认分页预算为
  `DEFAULT_INLINE_TOOL_RESULT_BYTES = 200_000`，因此 CodeKit 输出不会触发 `build_chunk_plan` 的单行超限错误。
- `rg` 返回每个匹配的所属结构，按标识符检索即得到调用者。
- VCE 存在时才注入的路由提示（`codebase_tool_routing_prompt.rs`）是 CodeKit 之外的唯一主机侧代码导航说明；
  CodeKit 自身说明通过 LuaSkills 帮助注入（提交 `4799cda2d` 引入的按包主帮助与调用块）。

## 5. 持久数据处理

| 数据 | 处理 |
| --- | --- |
| 应用库 `project_vce_settings` | 从 `build_application_schema_script()` 删除；`vulcan-debug-db` 增加显式映射：允许删除整表（原数据留在备份），并删除已失效的 `migrated_from_codebase_memory` 映射；补成功与拒绝回归测试；本机 Debug 库执行 `--check` → `--apply` → `--check`（先确认 Core 已退出） |
| 持久 AI 配置 | 删除 `context_engine_embedding_compatibility` 与 `vce` 推理配置档（含校验中的强制要求），`PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION` 递增一位并明确拒绝旧版本；先查证该文件既有的版本递增惯例与契约，按惯例执行。执行记录列出受影响的本机文件与处理方式，只检查键名，不读取或输出凭据值 |
| `LogicalDatabase::VceMetadata` 与 VCE LanceDB 路径 | 删除逻辑库枚举与路径派生；不自动删除磁盘目录（D3） |
| 会话元数据、子执行覆盖 | 查证是否持久化任何 VCE 字段；若有，按 `session-metadata-migrations.md` 递增版本（注意知识库方案已将 `METADATA_SCHEMA_VERSION` 改到 8，届时以源码为准） |
| 全局配置 `vulcan-code.json` | 查证是否含 VCE 字段（`deny_unknown_fields`，删除字段会使旧文件无法启动）；若有，执行记录写明受影响文件与需删除的行 |

## 6. 执行阶段

每个阶段一个或多个独立提交，提交信息英文，结尾附项目要求的署名行；不推送。

### 阶段 0：CodeKit 收口验证（可先行）

1. 确认 CodeKit 主帮助在 CodeKit 工具可见时进入模型请求：按现有 LuaSkills 帮助注入链路添加或核对一条
   行为测试（断言注入块包含该包主帮助的标识，不锁定帮助正文措辞，AGENTS 第四节第 8 条）。
2. 查证 CodeKit 在新安装与新项目中的默认可见性（根技能配置 `config/luaskills_root_skills.yaml` 与工具注册状态）。
   不修改任何用户持久配置（AGENTS 第一节第 5 条）；若默认不可见，在执行记录中说明，由用户决定。
3. 用夹具确认：CodeKit `rg` 的超长行预览在默认预算下不会触发 `build_chunk_plan` 的单行超限错误。
   非 CodeKit 工具的单行超限问题不在本方案范围，记入执行记录的后续事项。

### 阶段 1：AI 工具链

删除 `codebase_retrieval` 工具、`vce_retrieval_handler` 字段及其全部传递、`codebase_tool_routing_prompt.rs`、
专职会话中的 VCE 过滤分支、权限与工具展示中的代码检索分支。按 AGENTS 第一节第 4 条确认暴露链再无残留：
注册、请求构建、模式可见性、策略过滤、执行、结果投影。

### 阶段 2：服务内核与服务 API

删除 `vce/` 模块、`rpc_dispatch/vce.rs` 及 `RPC_METHODS` 条目、启动与状态中的 VCE 部分、公共 AI 设置中的
VCE 嵌入设置与补偿、`configured_inference.rs` 中的 VCE 部分、`workspace_watcher`/`workbench_refresh`/
`precheck_continue` 中的 VCE 消费者，以及全部 `tests/vce_*`。服务 API 删除 `vce.rs` 与其他文件中的 VCE 字段。

### 阶段 3：存储、模型与持久配置

删除 `vce_*` 存储模块、`VceMetadata`、项目 VCE 设置读写、`ProjectVceSettings`、应用库表与维护映射（第 5 节）、
持久 AI 配置字段与版本递增、供应商适配中的 VCE 专属代码（按 4.2 判定）。

### 阶段 4：引擎 crate 与依赖

删除 `crates/vulcan-context-engine/` 与工作区成员和依赖声明；按 4.2 清理独有依赖、`workspace-hack` 与第三方声明。

### 阶段 5：WebUI

删除代码上下文与 ContextEngine 功能区、VCE 服务与类型、项目设置与概览的 VCE 区块、工作台导航入口、
`/semantic` 指令、代码检索工具卡片、`tests/vce-live/`。目录文案从所有语言同步删除（非默认语言不得残留多余键）。
提交前执行完整 `pnpm run build` 检查体积预算。

### 阶段 6：TUI

先读 `apps/vulcan-code-tui/AGENTS.md`。删除 `service_api/vce.rs`、VCE 页面与动作、模拟后端夹具与事件处理，
与 WebUI 保持一对一镜像；TUI 文案目录同步删除。

### 阶段 7：契约、文档与棘轮

- 更新第 4.1 节所列契约，删除 VCE 段落与 RPC 映射；`explicit-test-gates.md` 删除 VCE 门控行；
  `tool-result-presentation.md` 删除代码检索行；`provider-manifest-authoring.md` 中仅描述 VCE 行为的句子改为
  中性描述或删除（Batch 能力本身的清单规则保留）。
- 新增 `docs/contracts/code-navigation.md`（短文）：CodeKit 是唯一内置代码导航；不建主机预建整库索引；
  外部语义检索走 MCP；概念层由知识库笔记承担；记录两次失败的原因以免重复尝试。
- `AGENTS.md` 第二节新增一行，红线示例：“代码导航只用 CodeKit 与实时工作树；不得新增主机预建整库代码索引或
  向量检索；语义检索需求通过外部 MCP 接入”。
- `scripts/backend-cjk-literals-baseline.json`、`scripts/wording-assertions-baseline.json` 删除已删文件的条目
  （棘轮只减不增）。
- `docs/completed/`、`docs/reviews/`、`docs/analysis/` 下的历史文档不改。

## 7. 测试与验证

- 删除 VCE 测试的同时，保留或补充以下行为测试：工具目录中不存在 `codebase_retrieval`；专职会话两档的工具过滤
  仍正确；CodeKit 主帮助注入（阶段 0）；应用库维护工具的新映射；持久 AI 配置旧版本被明确拒绝；
  `workspace_watcher` 的 Git 消费者行为不变。
- 不锁定文案或帮助正文措辞；断言工具标识、错误码与状态。

```powershell
rtk cargo check --workspace --all-targets
rtk cargo test -p vulcan-agent-runtime --lib tooling
rtk cargo test -p vulcan-agent-runtime --lib dedicated_session
rtk cargo test -p vulcan-agent-runtime --lib tool_permission
rtk cargo test -p vulcan-service-kernel --lib rpc_dispatch -- --test-threads=1
rtk cargo test -p vulcan-service-kernel --lib public_ai_settings -- --test-threads=1
rtk cargo test -p vulcan-service-kernel --lib workspace_watcher -- --test-threads=1
rtk cargo test -p vulcan-provider-adapters --lib configuration
rtk cargo test -p vulcan-workspace-store --lib data_plane::tests -- --test-threads=1
rtk cargo test -p vulcan-workspace-store --features debug-db-maintenance --bin vulcan-debug-db -- --test-threads=1
rtk cargo test -p vulcan-code-tui
```

WebUI：受影响组件的 vitest 与完整 `pnpm run build`。最后执行 `make check`。运行 cargo 前清除
`NoDefaultCurrentDirectoryInExePath`。不调用付费服务。服务内核重新编译耗时长，端到端验证前先告知用户。
`rg -i "vce|context.engine|codebase_retrieval"` 在 `crates/`、`apps/`、`docs/contracts/`、`AGENTS.md` 中
应只剩第 6 节阶段 7 允许的历史说明与 `code-navigation.md`。

## 8. 非目标

- 修改 CodeKit 上游仓库或新增 CodeKit 工具（D5、D6）。
- 新增任何代码索引、向量检索或替代引擎。
- 修改供应商清单中的 Batch 能力声明。
- 自动删除用户磁盘上的 VCE 数据目录（D3）。
- 修复非 CodeKit 工具的单行超限分页问题（阶段 0 第 3 项只记录）。

## 9. 交付审核要求

执行会话完成后在本节下方追加“执行记录”：

- 第 3 节前置条件满足的证据（知识库方案完成的提交或用户确认）。
- 每个阶段的提交哈希与一句说明。
- 4.2 各项的判定结果与依据。
- 与本方案的偏差及理由（含第 4 节复核时发现的事实差异）。
- 第 7 节每条命令的结果；本机 Debug 库 `--check` 输出。
- 受影响的本机配置文件与需手动处理的行；需手动删除的 VCE 数据目录路径。
- 未完成项与后续事项。

## 执行记录

### 协调与隔离

- 用户明确选择“允许隔离并行，后续合并验证”，随后要求全部完成后告知，禁止主动合并，合并须等待用户另行指令。
- 独立工作树：`C:\Users\lingeasy\.codex\worktrees\vce-retirement-codekit\vulcan-code`。
- 独立分支：`codex/vce-retirement-codekit`；初始基线：`3b11db34f3f7061f82030ef25d8fe37c2fe34eb5`。
- 原工作树的知识库未提交修改不暂存、不还原、不覆盖；本任务不操作原分支的文件与索引。
- 按阶段提交。实施与验证完成后循环审核并自动修复，问题修复后重新计数，直到连续五轮无问题。
- 知识库最终分支与本分支的合并及合并后验证，等待用户明确指令；不把隔离分支验证表述为合并验证。
