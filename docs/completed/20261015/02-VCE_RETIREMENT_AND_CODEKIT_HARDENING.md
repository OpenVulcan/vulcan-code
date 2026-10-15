# VCE 退役与 CodeKit 收口方案

> 状态：已完成独立分支实现与验收，连续五轮审核无问题。用户于 2026-10-15 授权隔离并行，覆盖第 3 节等待顺序；未合并、未推送，等待用户后续指令。
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

### 隔离基线修正

- 初次工作区检查发现知识库第一阶段 `3b11db3` 已替换存储接口，而旧工具仍引用被删除的类型与方法，产生九处编译错误。
  相关知识库文件相对初始基线无本任务差异，不能越界修复或把并行会话尚未完成的改动直接带入。
- 改为将本任务提交重放到知识库开工前的父提交 `5b6d65537603f79f38b23cd34dd07294ff1a0646`；
  调整前保留独立快照分支。该操作只调整本任务分支，不合并知识库分支，不操作原工作树。
- 因此最终验收针对完整旧基线加本任务改动；知识库最终整合后的验证仍须用户另行指令。

### 已取得的独立证据

- CodeKit 主帮助注入行为测试已添加；默认根技能清单包含 `vulcan-codekit`，不回写用户工具选择。
- 真实运行已安装的 CodeKit 0.2.5：六十万字节单行保留全部匹配并输出四千字节内预览，
  无结构归属的匹配明确标为“无已索引结构归属”。现场旧 MCP 运行时未注入新包依赖，验收按包源码实际要求
  在单次 Lua 调用内显式设置其工具与原生库目录，不修改包或运行时持久配置。
- 主机工具结果分页夹具测试通过；WebUI 十个受影响测试文件的一百项测试通过，完整构建与体积预算通过；
  TUI 一百六十二项测试通过。最终源码调整后还将执行最终检查。
- 本机 Debug 与 Release 的 `code/ai-connections.toml` 均为内部配置版本十九且包含旧推理配置档；
  两个全局 `vulcan-code.json` 未发现退役字段。本任务不改写这些用户配置。
- 为保护运行中的并行知识库工作，真实 Debug 应用库通过 SQLite 只读备份接口复制到私有验收目录；
  维护操作只对该副本执行，原库不应用结构变化。此项替代原方案要求退出共享 Core 后直接维护原库的步骤。


### 阶段提交与共享件判定

累计审核范围为 `5b6d65537603f79f38b23cd34dd07294ff1a0646..e697e35ee7482f060fbb65cb335ddfaa3ca9d68a`。
调整前快照保留于 `codex/vce-retirement-before-base-adjustment`。所有阶段提交均有规定署名，没有推送或合并。

| 阶段 | 主要提交 | 完成内容 |
| --- | --- | --- |
| 0 | `62e96444` | 包主帮助注入与默认预算分页测试；查证安装及可见性 |
| 1 | `5cf32631` | 删除工具、回调、请求字段、路由提示、权限与专职过滤分支 |
| 2 | `ddd7cf55` | 删除服务运行时、状态、启动、RPC、事件、设置预检与发布补偿 |
| 3 | `74982869` | 删除专属存储及项目设置，增加显式备份后整表退役，AI 配置内部版本升至二十 |
| 4 | `743d6186` | 删除引擎 crate、工作区成员和依赖接入，保留共享基础设施 |
| 5 | `10796f00` | 删除 WebUI 页面、观察器、设置、卡片、导航、指令与专属语言键 |
| 6 | `c262c665` | 删除 TUI 页面、RPC、槽位、事件、预检和模拟状态 |
| 7 | `ca4f0521` | 新增代码导航契约，修订按需文档、AGENTS 入口和棘轮 |
| 收口 | `fb3fed5b`、`b51a917b`、`5bb3bd87`、`2cffedbf`、`328406a5` | 清除剩余指令、依赖、测试调用、孤立 Batch 辅助函数与属性，精确校验历史表 |
| 验证修正 | `a2710f8a`、`555ad032`、`f4f3498f` | 清理孤立测试接口、修正格式门禁、以完整基线恢复未改知识库测试的原有限额 |
| 文档说明 | `c9dbe531`、`d12b29eb`、`ced601b8`、`17145cd7` | 同步 Git 和存储说明、保持凭据导出注释归属、修订所有语言中的共享路由帮助 |
| 最后修复 | `e697e35e` | 按 SQLite 标识符规则核对不同大小写的同一退役表及外键引用，补充回归测试 |

第 4.2 节逐项判定：

| 对象 | 判定与依据 |
| --- | --- |
| 异步向量 Batch | 唯一业务消费者随引擎退役；删除原生执行、公共运行时接入、已配置推理与内部 HostAI 覆盖。保留受签名清单、Schema、通用类型及默认拒绝的可选接口；当前快照不再宣称可执行异步 Batch |
| 推理诊断与产物 | 删除退役专属请求追踪、Batch 产物与遥测辅助函数；保留 VMM 和通用同步推理的追踪、取消、限额、计费及错误投影 |
| 工作区监听 | 保留物理监听中心及 Git 消费者；真实通知回归证明释放同路径的并行订阅后，Git 仍接收变更；原有 Git 测试也通过 |
| SQLite、LanceDB、Jieba | 仅更新退役说明，不改行为。反向依赖显示 LanceDB 引擎仍由服务内核和工作区存储使用，Jieba 仍由 SQLite 及 Lance 分词链使用 |
| 第三方依赖与声明 | futures、serde、sha2、tokio 等仍有共享消费者，保留；hakari 仅移除生成清单中的直接 futures-core 统一项，再生成无差异。许可证用权威生成器更新，仅去掉退役工作区条目，随后显式同步 |

CodeKit 默认链路沿源码核实：根技能清单包含该包，缺失根技能由既有入口安装；
无用户覆盖时入口按明确注册状态暴露，请求按最终可见工具注入包主帮助。
新增测试按稳定来源 `luaskills/vulcan-codekit` 定位，以夹具载荷证明主帮助进入请求，不锁定发布文案。
用户禁用状态、项目工具策略与持久设置不改写。

会话元数据在本分支保持原版本七：退役回调只存在于内存执行上下文，项目投影不持久化该字段，
持久来源只有退役应用表；不新增兼容分支。知识库分支后续版本不提前带入，整合时以最终源码核验。

### 最终验证结果

Rust 验证先清除 `NoDefaultCurrentDirectoryInExePath`，独立产物目录为
`D:\openvulcan\vce-retirement-target`。保留十六个编译任务、四个常规测试线程限制；
共享服务、数据库和端口夹具按命令串行执行。以下均针对本分支，不代表知识库整合后的验收。

| 验证命令 | 结果 |
| --- | --- |
| `rtk cargo check --workspace --all-targets` | 通过，含测试目标 |
| `rtk cargo test -p vulcan-agent-runtime --lib tooling` | 124 项通过 |
| `rtk cargo test -p vulcan-agent-runtime --lib dedicated_session` | 1 项通过，覆盖两档访问 |
| `rtk cargo test -p vulcan-agent-runtime --lib tool_permission` | 16 项通过 |
| `rtk cargo test -p vulcan-agent-runtime --lib visible_codekit_injects_package_main_help` | 1 项通过 |
| `rtk cargo test -p vulcan-service-kernel --lib rpc_dispatch -- --test-threads=1` | 15 项通过 |
| `rtk cargo test -p vulcan-service-kernel --lib public_ai_settings -- --test-threads=1` | 76 项通过 |
| `rtk cargo test -p vulcan-service-kernel --lib workspace_watcher -- --test-threads=1` | 1 项通过，真实通知 |
| `rtk cargo test -p vulcan-service-kernel --lib git_watcher -- --test-threads=1` | 3 项通过 |
| `rtk cargo test -p vulcan-provider-adapters --lib configuration` | 50 项通过，含版本十九及未知版本拒绝 |
| `rtk cargo test -p vulcan-workspace-store --lib data_plane::tests -- --test-threads=1` | 19 项通过 |
| `rtk cargo test -p vulcan-workspace-store --features debug-db-maintenance --bin vulcan-debug-db -- --test-threads=1` | 7 项通过 |
| `rtk cargo test -p vulcan-code-tui` | 3 个套件共 162 项通过 |
| `rtk cargo test -p vulcan-host-tools --test codekit_paging` | 1 项通过 |
| `rtk cargo test -p vulcan-agent-runtime --lib task_runtime_preparation_request -- --test-threads=1` | 额外 9 项通过，核对子执行瞬时状态及知识库授权继承 |
| `rtk cargo test -p vulcan-service-kernel --lib shutdown -- --test-threads=1` | 额外 27 项通过 |
| `rtk cargo clippy --locked -p vulcan-workspace-store --features debug-db-maintenance --bin vulcan-debug-db -- -D warnings` | 额外维护入口门禁通过 |
| `rtk cargo hakari generate --diff` | 通过，无差异 |
| WebUI 十个受影响文件的 Vitest | 共 100 项通过，清单见下方 |
| WebUI `node --test scripts/vite-cache-isolation.test.mjs` | 1 项通过 |
| WebUI `pnpm run build` | lint、类型、界面架构、Vite 与体积预算通过 |
| `make.ps1 sync` | 本工作树资源显式同步成功；两份许可证源与部署副本字节相同 |
| `make.ps1 check` | 九个静态门禁全部通过 |
| `rtk git diff --check` | 通过；归档提交时再次核对 |

九个静态门禁为 Rust 格式、工作区所有目标 Clippy、WebUI lint/类型/架构、桌面类型、
文案棘轮、后端字面量棘轮及测试主目录隔离。后端字面量棘轮条目十一减至十，
文案断言棘轮条目二百零九减至二百，逐项相对完整基线只减不增。

Vitest 文件位于 `apps/vulcan-code-web/`，在该目录执行：

- `src/components/project/project-overview-section.test.tsx`
- `src/components/project/project-settings-utils.test.tsx`
- `src/components/workspace/workspace-skills-settings.test.tsx`
- `src/components/workspace/workspace-control-preaudit.test.tsx`
- `src/components/workspace/workbench-layout/plugin-resource-owner-navigation.test.tsx`
- `src/service/browserTransport.test.ts`
- `src/service/settings-actions-service.test.ts`
- `src/components/conversation/tool-presentations/v1-tool-domain-presentation.test.ts`
- `src/components/composer/composer-add-menu-interactions.test.tsx`
- `src/components/composer/composer-memory-menu.test.tsx`

WebUI 最终预算：启动脚本 264116 字节、压缩后 84763 字节；工作台块 1693274 字节、
压缩后 480004 字节；首次工作台脚本合计 2335357 字节、压缩后 658523 字节；
样式 497700 字节、压缩后 73188 字节，均在现有预算内。
Vite 通用大块提示及 Node 既有脚本弃用提示未阻断门禁。
未将编译和模拟服务测试称为桌面实机端到端验收，没有调用真实付费供应商。

早期失败已处理：知识库未完成提交的九处编译错误通过隔离基线调整排除；
终端测试所需宿主程序先在独立产物目录构建；既有纯格式差异仅在本工作树修正；
未改知识库测试的棘轮以完整基线恢复，没有提高额度。最终上述验证均通过。

### 真实 Debug 数据库副本验收

原库为 `C:\Users\lingeasy\.vulcan-debug\code\sqlite\app-workspace.sqlite3`。
通过 SQLite 只读连接的备份接口获取包含已提交 WAL 的一致副本，不直接复制活动主文件，不停止并行 Core。
仅对私有副本执行生产维护模块的真实入口；临时示例使用既有测试根目录接口，导入未修改的维护模块，
验收后已移除，不作为产品入口提交。

私有副本实际位置：

`C:\Users\lingeasy\AppData\Local\Packages\OpenAI.Codex_2p2nqsd0c76g0\LocalCache\Local\VulcanCodeAcceptance\vce-retirement-9629abf25b174bec9af368d7824d5056\user-vulcan\code\sqlite\app-workspace.sqlite3`

最终源码 `e697e35e` 的复验结果：

| 步骤 | 退出码 | 结构诊断 |
| --- | --- | --- |
| `--check` | 1（预期漂移） | 仅 `Tables to retire: project_vce_settings`，其他表和索引无变化 |
| `--apply` | 0 | 创建并验证完整备份，事务内退役表；全部保留值核验通过，版本仍为一 |
| 再次 `--check` | 0 | `Schema v1 is current; no changes.` |
| 独立只读复核 | 0 | 副本和备份完整性均正常；副本无退役表，备份仍有退役表 |

最终备份与副本同目录，名为 `app-workspace-before-sync-2VD4uP.sqlite3`；
首次成功演练备份 `app-workspace-before-sync-GhVRNL.sqlite3` 亦保留。
维护逐列、逐类型并按重复记录次数比较所有保留表，不输出凭据值。

SQLite 外键元数据保留声明拼写；大小写不同且子表为空时，完整性检查不能单独识别悬空依赖。
最终修复按 ASCII 大小写不敏感规则核对表身份与引用，增加成功和拒绝用例。
七项维护测试同时覆盖备份、未知映射拒绝、失败回滚、WAL、写锁争用及版本拒绝。

此验收替代第 5 节直接维护共享原库，理由是保护仍在进行的知识库结构工作。
原库未执行本分支 `--apply`；整合获准后应退出 Core、重新备份当时原库，按最终结构维护。
不能用本次副本备份覆盖未来仍在发展的原库。

### 连续五轮审核

本会话逐轮源码自审，每轮重新核对累计变更及调用链，并结合行为测试与静态门禁。
早期问题修复后均清零，不拼接旧轮次。下列五轮统一锁定
`e697e35ee7482f060fbb65cb335ddfaa3ca9d68a`，期间源码未变；
此后仅追加本文证据并归档计划。

| 轮次 | 审核范围 | 发现问题 |
| --- | --- | --- |
| 第 1 轮 | 累计变更初审：工具暴露与请求链、CodeKit 主帮助/默认可见性、专职与子执行边界；复核 SQL 退役与服务删除范围 | 0 |
| 第 2 轮 | 累计变更复审：持久配置和数据保全，覆盖准确列映射、ASCII 表身份、外部依赖拒绝、事务备份/回滚及保留行验证 | 0 |
| 第 3 轮 | 累计变更再审：服务启动/关闭与失败恢复、公共 AI 发布事务、Git 订阅生命周期、同步嵌入与 Batch 能力边界 | 0 |
| 第 4 轮 | 累计变更交叉审核：WebUI/TUI 页面与事件、共享服务参数、mock 项目身份、历史工具结果及多语言目录 | 0 |
| 第 5 轮 | 累计交付终审：依赖与共享基础设施、资源/许可证、文档契约、棘轮、验证矩阵、隔离范围与 Git 状态 | 0 |


各轮结论：工具全暴露链无退役入口，主帮助及授权边界保留；
配置拒绝旧版，表退役映射明确且数据保全闭环；
服务不可逆关闭栅栏、Git 物理监听和同步推理保留；
两端页面、事件、参数、模拟身份与语言目录一致；
共享依赖有实际消费者，资源已同步，棘轮只减不增，范围与 Git 状态符合隔离要求。

### 本机文件与后续边界

只读核查的受影响 AI 文件：

- `C:\Users\lingeasy\.vulcan-debug\code\ai-connections.toml`
- `C:\Users\lingeasy\.vulcan\code\ai-connections.toml`

两者外层版本均为一，`configuration.schema_version` 均为十九，
均有 `configuration.inference_profiles.vce`，均无 `context_engine_embedding_compatibility` 键。
本分支要求内部版本二十，明确拒绝旧格式。本任务未改写用户文件。
整合获准后先备份，再按最终源码完整核对结构，显式去除退役配置档及兼容字段，验证其他设置保留；
不能只改版本标记掩盖结构差异。知识库整合若再改版本，以最终类型与源码为准。

以下全局文件未发现退役字段，无需为本任务修改：

- `C:\Users\lingeasy\.vulcan-debug\vulcan-code.json`
- `C:\Users\lingeasy\.vulcan\vulcan-code.json`

退役目录由用户决定是否手动删除，程序不自动删除：

- `C:\Users\lingeasy\.vulcan-debug\code\vce`：核查时存在。
- `C:\Users\lingeasy\.vulcan\code\vce`：核查时不存在，列出固定派生路径供识别。

非 CodeKit 工具的单行超限分页仍属非目标；受签名 Batch 事实继续保留，将来有消费者时须重新设计执行链。
没有修改 CodeKit 上游、签名清单或用户持久策略。
知识库整合、整合后验证、真实原库维护和发布等待用户后续明确指令。

## 执行变更总结

### 1. 核心修复与调整概述

阶段零至七在独立工作树完成，导航收敛到实时 CodeKit，退役工具、引擎、服务、存储与两端界面完整删除。
共享数据库引擎、分词、同步嵌入及 Git 监听保留；主帮助和长行分页有行为证据；
持久配置拒绝旧版，应用表经独立、可备份及可回滚的显式映射退役。
要求的验证通过，连续五轮审核无问题；没有推送或主动合并。

### 2. 📂文件变更清单

相对完整稳定基线，新增三个、修改一百八十五个、删除一百二十六个文件。
下方将本计划列为归档地址，代码范围固定在审核提交；
完成目录仅新增本次记录，未改历史完成、审核及分析文档。

#### 新增（3 个文件）

```text
crates/vulcan-host-tools/tests/codekit_paging.rs
docs/contracts/code-navigation.md
docs/completed/20261015/02-VCE_RETIREMENT_AND_CODEKIT_HARDENING.md
```

#### 修改（185 个文件）

```text
AGENTS.md
Cargo.lock
Cargo.toml
apps/vulcan-code-tui/AGENTS.md
apps/vulcan-code-tui/src/app/action.rs
apps/vulcan-code-tui/src/app/catalog_tests.rs
apps/vulcan-code-tui/src/app/commands.rs
apps/vulcan-code-tui/src/app/input/mod.rs
apps/vulcan-code-tui/src/app/mod.rs
apps/vulcan-code-tui/src/app/pages/inference.rs
apps/vulcan-code-tui/src/app/pages/mod.rs
apps/vulcan-code-tui/src/app/pages/plugins.rs
apps/vulcan-code-tui/src/app/preview.rs
apps/vulcan-code-tui/src/app/service_tests.rs
apps/vulcan-code-tui/src/backend/mock/service-fixtures.json
apps/vulcan-code-tui/src/backend/mock/service.rs
apps/vulcan-code-tui/src/backend/mock/service_tests.rs
apps/vulcan-code-tui/src/backend/service/events.rs
apps/vulcan-code-tui/src/backend/service/test_support.rs
apps/vulcan-code-tui/src/backend/service/worker.rs
apps/vulcan-code-tui/src/service_api/contracts_tests.rs
apps/vulcan-code-tui/src/service_api/mod.rs
apps/vulcan-code-web/scripts/check-ui-architecture.mjs
apps/vulcan-code-web/scripts/vite-cache-isolation.test.mjs
apps/vulcan-code-web/src/components/composer/composer-add-menu-interactions.test.tsx
apps/vulcan-code-web/src/components/composer/composer-attachment-controls.tsx
apps/vulcan-code-web/src/components/composer/composer-memory-menu.test.tsx
apps/vulcan-code-web/src/components/composer/composer-model.ts
apps/vulcan-code-web/src/components/composer/composer-settings-request.ts
apps/vulcan-code-web/src/components/conversation/conversation-tool-result-tabs.tsx
apps/vulcan-code-web/src/components/conversation/session-todolist-panel.tsx
apps/vulcan-code-web/src/components/conversation/tool-presentations/v1-tool-domain-presentation.test.ts
apps/vulcan-code-web/src/components/conversation/tool-presentations/v1-tool-domain-presentation.ts
apps/vulcan-code-web/src/components/features/feature-shell-sidebar.tsx
apps/vulcan-code-web/src/components/features/skills/skill-area-header.ts
apps/vulcan-code-web/src/components/features/skills/skill-area-types.ts
apps/vulcan-code-web/src/components/features/skills/skills-area.tsx
apps/vulcan-code-web/src/components/project/project-overview-section.test.tsx
apps/vulcan-code-web/src/components/project/project-overview-section.tsx
apps/vulcan-code-web/src/components/project/project-permission-judge-section.test.tsx
apps/vulcan-code-web/src/components/project/project-settings-dialog.tsx
apps/vulcan-code-web/src/components/project/project-settings-types.ts
apps/vulcan-code-web/src/components/project/project-settings-utils.test.tsx
apps/vulcan-code-web/src/components/project/project-settings-utils.tsx
apps/vulcan-code-web/src/components/workspace/workbench-dialogs.tsx
apps/vulcan-code-web/src/components/workspace/workbench-layout.tsx
apps/vulcan-code-web/src/components/workspace/workbench-layout/plugin-resource-owner-navigation.test.tsx
apps/vulcan-code-web/src/components/workspace/workbench-layout/use-workbench-revision-sync.ts
apps/vulcan-code-web/src/components/workspace/workspace-control-preaudit.test.tsx
apps/vulcan-code-web/src/components/workspace/workspace-shell.tsx
apps/vulcan-code-web/src/components/workspace/workspace-skills-settings.test.tsx
apps/vulcan-code-web/src/components/workspace/workspace-skills-settings.tsx
apps/vulcan-code-web/src/service/browser-fallback-workbench.ts
apps/vulcan-code-web/src/service/browserTransport.test.ts
apps/vulcan-code-web/src/service/browserTransport.ts
apps/vulcan-code-web/src/service/settings-actions-service.test.ts
apps/vulcan-code-web/src/service/settings-actions-service.ts
apps/vulcan-code-web/src/state/workbench-state.ts
apps/vulcan-code-web/src/styles.css
apps/vulcan-code-web/src/types.ts
apps/vulcan-code-web/src/types/conversation.ts
apps/vulcan-code-web/src/types/projects-files.ts
apps/vulcan-code-web/src/types/public-ai.ts
apps/vulcan-code-web/src/types/workbench.ts
crates/vulcan-agent-runtime/Cargo.toml
crates/vulcan-agent-runtime/src/agent_runtime/config.rs
crates/vulcan-agent-runtime/src/agent_runtime/execution_models.rs
crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation.rs
crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation/dedicated_session.rs
crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation/tests.rs
crates/vulcan-agent-runtime/src/agent_runtime/mod.rs
crates/vulcan-agent-runtime/src/agent_runtime/prompt_compaction.rs
crates/vulcan-agent-runtime/src/agent_runtime/runtime_stream_launch_payload.rs
crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/preparation_config.rs
crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tests/runtime_flow.rs
crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge/tests/file_durability.rs
crates/vulcan-agent-runtime/src/lib.rs
crates/vulcan-agent-runtime/src/tooling/bridge.rs
crates/vulcan-agent-runtime/src/tooling/bridge/tests.rs
crates/vulcan-agent-runtime/src/tooling/native.rs
crates/vulcan-agent-runtime/src/tooling/native/browser.rs
crates/vulcan-agent-runtime/src/tooling/native/tests.rs
crates/vulcan-agent-runtime/src/tooling/native/tests/planning.rs
crates/vulcan-agent-runtime/src/tooling/native/tests/shell.rs
crates/vulcan-agent-runtime/src/tooling/native/tests/terminal.rs
crates/vulcan-agent-runtime/src/workbench_prompt_turn.rs
crates/vulcan-ai-contract/src/inference.rs
crates/vulcan-file-watcher/src/live/health.rs
crates/vulcan-i18n/catalogs/tui/en-US.json
crates/vulcan-i18n/catalogs/tui/zh-CN.json
crates/vulcan-i18n/catalogs/tui/zh-HK.json
crates/vulcan-i18n/catalogs/tui/zh-TW.json
crates/vulcan-i18n/catalogs/webui/de-DE.json
crates/vulcan-i18n/catalogs/webui/en-US.json
crates/vulcan-i18n/catalogs/webui/es-ES.json
crates/vulcan-i18n/catalogs/webui/fr-FR.json
crates/vulcan-i18n/catalogs/webui/ja-JP.json
crates/vulcan-i18n/catalogs/webui/ko-KR.json
crates/vulcan-i18n/catalogs/webui/pt-BR.json
crates/vulcan-i18n/catalogs/webui/ru-RU.json
crates/vulcan-i18n/catalogs/webui/zh-CN.json
crates/vulcan-i18n/catalogs/webui/zh-HK.json
crates/vulcan-i18n/catalogs/webui/zh-TW.json
crates/vulcan-lancedb-engine/src/lib.rs
crates/vulcan-mcp-runtime/src/lib.rs
crates/vulcan-provider-adapters/src/configuration.rs
crates/vulcan-provider-adapters/src/configuration/tests.rs
crates/vulcan-provider-adapters/src/execution/request_encoding/tests.rs
crates/vulcan-provider-adapters/src/lib.rs
crates/vulcan-provider-adapters/src/native_inference.rs
crates/vulcan-provider-adapters/src/runtime/model_execution.rs
crates/vulcan-runtime-assets/resources/licenses/THIRD_PARTY_NOTICES.md
crates/vulcan-runtime-assets/resources/licenses/THIRD_PARTY_NOTICES_INDEX.json
crates/vulcan-service-api/src/command_events.rs
crates/vulcan-service-api/src/conversation.rs
crates/vulcan-service-api/src/lib.rs
crates/vulcan-service-api/src/startup.rs
crates/vulcan-service-api/src/subagents.rs
crates/vulcan-service-api/src/workbench.rs
crates/vulcan-service-api/src/workspace_context.rs
crates/vulcan-service-kernel/Cargo.toml
crates/vulcan-service-kernel/src/agent_collaboration/child_question_request.rs
crates/vulcan-service-kernel/src/browser/tests.rs
crates/vulcan-service-kernel/src/configured_inference.rs
crates/vulcan-service-kernel/src/fallback_workbench_projection.rs
crates/vulcan-service-kernel/src/git/commit_summary.rs
crates/vulcan-service-kernel/src/internal_ai.rs
crates/vulcan-service-kernel/src/lib.rs
crates/vulcan-service-kernel/src/native_vmm/binding_cache_projection.rs
crates/vulcan-service-kernel/src/native_vmm/native_tool_dispatch_acceptance_tests.rs
crates/vulcan-service-kernel/src/public_ai_settings.rs
crates/vulcan-service-kernel/src/public_ai_settings/inference_settings.rs
crates/vulcan-service-kernel/src/public_ai_settings/inference_settings/tests.rs
crates/vulcan-service-kernel/src/public_ai_settings/inference_testing.rs
crates/vulcan-service-kernel/src/public_ai_settings/inference_testing/tests.rs
crates/vulcan-service-kernel/src/public_ai_settings/publication.rs
crates/vulcan-service-kernel/src/rpc_dispatch.rs
crates/vulcan-service-kernel/src/rpc_dispatch/configuration.rs
crates/vulcan-service-kernel/src/service_bootstrap.rs
crates/vulcan-service-kernel/src/service_bridge_io.rs
crates/vulcan-service-kernel/src/service_state_access.rs
crates/vulcan-service-kernel/src/session_runtime_registry_v1.rs
crates/vulcan-service-kernel/src/settings_runtime_projection.rs
crates/vulcan-service-kernel/src/skills_services/vulcan_plugin_lifecycle/generation_tests/shutdown.rs
crates/vulcan-service-kernel/src/state.rs
crates/vulcan-service-kernel/src/tests.rs
crates/vulcan-service-kernel/src/tests/detached_subagent_tests.rs
crates/vulcan-service-kernel/src/tests/public_ai_persistence_scope.rs
crates/vulcan-service-kernel/src/tests/subagent_navigation_tests.rs
crates/vulcan-service-kernel/src/tests/test_support.rs
crates/vulcan-service-kernel/src/webui_prompt_lifecycle/precheck_continue.rs
crates/vulcan-service-kernel/src/workbench_refresh.rs
crates/vulcan-service-kernel/src/workspace_watcher.rs
crates/vulcan-service-runtime/src/luaskills_bridge/local_github_tests.rs
crates/vulcan-sqlite-engine/src/lib.rs
crates/vulcan-sqlite-engine/src/runtime.rs
crates/vulcan-sqlite-engine/src/tokenizer.rs
crates/vulcan-workbench-model/src/lib.rs
crates/vulcan-workbench-model/src/project.rs
crates/vulcan-workspace-store/Cargo.toml
crates/vulcan-workspace-store/src/bin/debug_database/mod.rs
crates/vulcan-workspace-store/src/bin/debug_database/tests.rs
crates/vulcan-workspace-store/src/data_plane.rs
crates/vulcan-workspace-store/src/lib.rs
crates/vulcan-workspace-store/src/local_database_runtime.rs
crates/vulcan-workspace-store/src/local_sql.rs
crates/vulcan-workspace-store/src/workspace_store/index_read.rs
crates/vulcan-workspace-store/src/workspace_store/loading.rs
crates/vulcan-workspace-store/src/workspace_store/mod.rs
crates/vulcan-workspace-store/src/workspace_store/models.rs
crates/vulcan-workspace-store/src/workspace_store/persistence/projects.rs
crates/vulcan-workspace-store/src/workspace_store/queries.rs
crates/vulcan-workspace-store/src/workspace_store/session_v1_store.rs
docs/contracts/debug-database-maintenance.md
docs/contracts/dedicated-sessions.md
docs/contracts/development-environment.md
docs/contracts/explicit-test-gates.md
docs/contracts/provider-manifest-authoring.md
docs/contracts/providers/stepfun-usage-evidence.md
docs/contracts/tool-result-presentation.md
docs/contracts/tui-design.md
docs/contracts/tui-service-adaptation.md
scripts/backend-cjk-literals-baseline.json
scripts/wording-assertions-baseline.json
workspace-hack/Cargo.toml
```

#### 删除（126 个文件）

```text
apps/vulcan-code-tui/src/service_api/vce.rs
apps/vulcan-code-web/src/components/features/code-context/code-context-area.test.tsx
apps/vulcan-code-web/src/components/features/code-context/code-context-area.tsx
apps/vulcan-code-web/src/components/features/code-context/code-context-editor.test.tsx
apps/vulcan-code-web/src/components/features/code-context/code-context-pagination.test.ts
apps/vulcan-code-web/src/components/features/code-context/code-context-pagination.ts
apps/vulcan-code-web/src/components/features/code-context/use-vce-action-scope.ts
apps/vulcan-code-web/src/components/features/code-context/use-vce-project-activity.ts
apps/vulcan-code-web/src/components/features/code-context/use-vce-project-job.ts
apps/vulcan-code-web/src/components/features/code-context/use-vce-project-management.ts
apps/vulcan-code-web/src/components/features/code-context/vce-activity-observer-store.test.ts
apps/vulcan-code-web/src/components/features/code-context/vce-activity-observer-store.ts
apps/vulcan-code-web/src/components/features/code-context/vce-activity-test-fixture.ts
apps/vulcan-code-web/src/components/features/code-context/vce-index-controls.tsx
apps/vulcan-code-web/src/components/features/code-context/vce-job-observer-context-state.ts
apps/vulcan-code-web/src/components/features/code-context/vce-job-observer-context.test.tsx
apps/vulcan-code-web/src/components/features/code-context/vce-job-observer-context.tsx
apps/vulcan-code-web/src/components/features/code-context/vce-job-observer-store.test.ts
apps/vulcan-code-web/src/components/features/code-context/vce-job-observer-store.ts
apps/vulcan-code-web/src/components/features/code-context/vce-job-progress.test.tsx
apps/vulcan-code-web/src/components/features/code-context/vce-job-progress.tsx
apps/vulcan-code-web/src/components/features/code-context/vce-maintenance-plan-preview.tsx
apps/vulcan-code-web/src/components/features/code-context/vce-management-details.tsx
apps/vulcan-code-web/src/components/features/code-context/vce-observer-timeouts.ts
apps/vulcan-code-web/src/components/features/code-context/vce-project-controls.tsx
apps/vulcan-code-web/src/components/features/code-context/vce-project-surface.test.tsx
apps/vulcan-code-web/src/components/features/code-context/vce-project-surface.tsx
apps/vulcan-code-web/src/components/features/code-context/vce-retrieval-settings.tsx
apps/vulcan-code-web/src/components/features/context-engine/context-engine-area-lifecycle.test.tsx
apps/vulcan-code-web/src/components/features/context-engine/context-engine-area.test.ts
apps/vulcan-code-web/src/components/features/context-engine/context-engine-area.tsx
apps/vulcan-code-web/src/components/features/context-engine/vce-action-button.tsx
apps/vulcan-code-web/src/components/features/context-engine/vce-embedding-dimensions.test.ts
apps/vulcan-code-web/src/components/features/context-engine/vce-embedding-dimensions.ts
apps/vulcan-code-web/src/components/features/context-engine/vce-embedding-settings.test.tsx
apps/vulcan-code-web/src/components/features/context-engine/vce-embedding-settings.tsx
apps/vulcan-code-web/src/components/features/context-engine/vce-help-tooltip.tsx
apps/vulcan-code-web/src/components/features/context-engine/vce-route-picker.ts
apps/vulcan-code-web/src/components/project/project-settings-vce.test.tsx
apps/vulcan-code-web/src/service/vce-maintenance-service.test.ts
apps/vulcan-code-web/src/service/vce-service.ts
apps/vulcan-code-web/src/styles/project-code-context.css
apps/vulcan-code-web/src/types/vce.ts
apps/vulcan-code-web/tests/vce-live/README.md
apps/vulcan-code-web/tests/vce-live/vite.config.ts
crates/vulcan-agent-runtime/src/agent_runtime/execution_preparation/codebase_tool_routing_prompt.rs
crates/vulcan-agent-runtime/src/tooling/native/codebase_retrieval.rs
crates/vulcan-context-engine/Cargo.toml
crates/vulcan-context-engine/LICENSE
crates/vulcan-context-engine/README.md
crates/vulcan-context-engine/src/chunking.rs
crates/vulcan-context-engine/src/embedding.rs
crates/vulcan-context-engine/src/embedding_execution.rs
crates/vulcan-context-engine/src/engine.rs
crates/vulcan-context-engine/src/engine/tests.rs
crates/vulcan-context-engine/src/host_adapter.rs
crates/vulcan-context-engine/src/identity.rs
crates/vulcan-context-engine/src/indexing.rs
crates/vulcan-context-engine/src/lib.rs
crates/vulcan-context-engine/src/management.rs
crates/vulcan-context-engine/src/manifest.rs
crates/vulcan-context-engine/src/model.rs
crates/vulcan-context-engine/src/path_index.rs
crates/vulcan-context-engine/src/ports.rs
crates/vulcan-context-engine/src/query.rs
crates/vulcan-context-engine/src/refresh_control.rs
crates/vulcan-context-engine/src/retrieval.rs
crates/vulcan-context-engine/src/state.rs
crates/vulcan-context-engine/src/structure.rs
crates/vulcan-context-engine/src/symbol.rs
crates/vulcan-context-engine/src/tooling.rs
crates/vulcan-context-engine/src/workspace.rs
crates/vulcan-context-engine/tests/host_ai_contract.rs
crates/vulcan-context-engine/tests/public_surface.rs
crates/vulcan-context-engine/tests/workspace_isolation.rs
crates/vulcan-provider-adapters/src/context_engine_embedding_policy.rs
crates/vulcan-provider-adapters/src/native_inference_batch.rs
crates/vulcan-service-api/src/vce.rs
crates/vulcan-service-kernel/src/public_ai_settings/publication/vce_compensation_tests.rs
crates/vulcan-service-kernel/src/public_ai_settings/vce_embedding_settings.rs
crates/vulcan-service-kernel/src/rpc_dispatch/vce.rs
crates/vulcan-service-kernel/src/service_bootstrap/vce_startup_tests.rs
crates/vulcan-service-kernel/src/tests/vce_configuration_readiness_tests.rs
crates/vulcan-service-kernel/src/tests/vce_conversation_isolation_tests.rs
crates/vulcan-service-kernel/src/tests/vce_embedding_save_tests.rs
crates/vulcan-service-kernel/src/tests/vce_live_model_acceptance_tests.rs
crates/vulcan-service-kernel/src/tests/vce_project_settings_tests.rs
crates/vulcan-service-kernel/src/tests/vce_request_preparation_tests.rs
crates/vulcan-service-kernel/src/tests/vce_stream_fixture.rs
crates/vulcan-service-kernel/src/tests/vce_stream_interleaving_tests.rs
crates/vulcan-service-kernel/src/vce/ai_port.rs
crates/vulcan-service-kernel/src/vce/embedding_dimensions.rs
crates/vulcan-service-kernel/src/vce/embedding_policy.rs
crates/vulcan-service-kernel/src/vce/job_query_tests.rs
crates/vulcan-service-kernel/src/vce/jobs.rs
crates/vulcan-service-kernel/src/vce/jobs_tests.rs
crates/vulcan-service-kernel/src/vce/manager.rs
crates/vulcan-service-kernel/src/vce/mod.rs
crates/vulcan-service-kernel/src/vce/publication_index.rs
crates/vulcan-service-kernel/src/vce/readiness.rs
crates/vulcan-service-kernel/src/vce/replacement_jobs.rs
crates/vulcan-service-kernel/src/vce/runtime_configuration.rs
crates/vulcan-service-kernel/src/vce/services.rs
crates/vulcan-service-kernel/src/vce/services/tool_result.rs
crates/vulcan-service-kernel/src/vce/source_port.rs
crates/vulcan-service-kernel/src/vce/watcher.rs
crates/vulcan-service-kernel/src/vce/watcher_clock.rs
crates/vulcan-service-kernel/src/vce/watcher_freshness.rs
crates/vulcan-workspace-store/src/vce_artifact_retention.rs
crates/vulcan-workspace-store/src/vce_candidate_allocation.rs
crates/vulcan-workspace-store/src/vce_data_plane.rs
crates/vulcan-workspace-store/src/vce_data_plane/tests.rs
crates/vulcan-workspace-store/src/vce_data_plane_performance_tests.rs
crates/vulcan-workspace-store/src/vce_embedding_manifest_store.rs
crates/vulcan-workspace-store/src/vce_embedding_provenance.rs
crates/vulcan-workspace-store/src/vce_initial_allocation.rs
crates/vulcan-workspace-store/src/vce_job_store.rs
crates/vulcan-workspace-store/src/vce_read_generations.rs
crates/vulcan-workspace-store/src/vce_shared_artifacts.rs
crates/vulcan-workspace-store/src/vce_shared_manifest_store.rs
crates/vulcan-workspace-store/src/vce_shared_result_store.rs
crates/vulcan-workspace-store/src/vce_shared_storage_tests.rs
crates/vulcan-workspace-store/src/vce_storage_catalog.rs
crates/vulcan-workspace-store/src/vce_storage_paths.rs
crates/vulcan-workspace-store/src/vce_writer_lease.rs
crates/vulcan-workspace-store/src/workspace_store/vce_batch_artifacts.rs
```

### 3. 💻关键代码调整详情

- 工具和请求：移除专属类型、回调、注册、策略、提示词及结果卡片；保留宿主项目身份与知识库授权。
  测试按包来源验证帮助注入，并以统一预算验证完整分页落盘。
- 服务和推理：去除专属启动、状态、RPC、发布补偿及执行；保留不可逆关闭栅栏及共享关闭顺序。
  Batch 实现无其他消费者，删除执行后保留默认拒绝接口与受签名事实。
- 持久层：从 DDL、模型、项目查询及写入移除退役设置；维护器核对准确历史列、外部依赖及表身份，
  备份后整表退役，其他记录继续按值、类型与重复次数验证，失败整体回滚。
- 界面和模拟：删除专属页面、观察器、入口、指令、事件和类型；模拟项目成员从同一项目夹具派生；
  畸形 WebSocket 事件拒绝测试改用共享 Git 事件，继续验证通用边界。
- 依赖和资源：按依赖图清理成员及接入，更新 hakari 与许可证并显式同步；
  三处既有文件仅修正格式以通过门禁，不引入无关业务改动。

### 4. ⚠️遗留问题与注意事项

本分支实现和隔离验收无未完成项。
合并、冲突处理、整合验证、共享原库及本机旧 AI 文件的维护，在用户发出后续合并指令后执行。
原工作树知识库改动未暂存、还原或覆盖；本次验证不提前宣称未来整合结果。
退役目录及完整备份保留，不自动清理。
