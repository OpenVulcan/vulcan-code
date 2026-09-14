# 全方位能力分析报告：vulcan-code 与五个同类 Agent 项目对比

> 调查方式：全部由本人直接读取源码/配置/资源文件取证，**未使用任何子代理**。所有结论均给出可复核的路径或实测数值；无法证实的一律标注为「未证实」。

## 0. 调查范围与口径

| 项目 | 路径 | 技术栈 |
| --- | --- | --- |
| vulcan-code（本仓库） | `D:\openvulcan\vulcan-code` | Rust (50 crates) + React/Vite + Tauri 2 |
| codex | `D:\openvulcan\third_git\codex` | Rust（约 120 crates）+ Bazel/Cargo |
| OpenCode | `D:\openvulcan\third_git\OpenCode` | TypeScript (Effect) + Go TUI |
| qwen-code | `D:\openvulcan\third_git\qwen-code` | TypeScript |
| kimi-cli | `D:\openvulcan\third_git\kimi-cli` | Python |
| deepseek-harness (dsh) | `D:\dsh\deepseek-harness` | TypeScript + Rust native + Python |

### 0.1 实测规模（自行统计，排除 node_modules/target/dist/vendor）

| 项目 | 源文件数 | 代码行数 | 备注 |
| --- | --- | --- | --- |
| qwen-code | 5 537 | **3 294 446** | 体量最大；含超大测试文件（`shell.test.ts` 356 KB） |
| codex | 4 026 | **1 709 272** | 仅统计 `.rs` |
| vulcan-code | 2 206 | **1 091 962** | Rust 832 392 + TS/TSX 207 999 |
| deepseek-harness | 3 716 | 864 741 | TS + Rust + Python |
| OpenCode | 3 333 | 685 726 | TS + Go |
| kimi-cli | 536 | 138 778 | 仅 Python |

### 0.2 测试与文档

| 项目 | 测试规模（实测） | 文档 |
| --- | --- | --- |
| vulcan-code | Rust `#[test]`/`#[tokio::test]` **5 199** 个；前端测试文件 **263** 个 | `docs/` 18 篇，含 91 KB Provider Manifest 作者手册、89 KB 技能配置指南 |
| codex | 未逐一统计；每模块配套 `*_tests.rs`，`multi_agents_tests.rs` 单文件 163 KB | `docs/` 多为跳转占位文件（`config.md` 仅 741 B） |
| qwen-code | 测试文件与源码同目录成对，单文件可达 356 KB | 独立 docs-site 站点 + 8 语言 README |
| OpenCode | `tests/` 与 `perf/` 并行目录，storybook | 独立 docs 包 + 30 余语言 README |
| deepseek-harness | 全面 `*.spec.ts`，含 e2e（`bwrap.e2e.ts`/`landlock.e2e.ts`） | docs 站点 + 每包 README 三语 |
| kimi-cli | `tests`/`tests_ai`/`tests_e2e` 三套 | 独立文档站；**项目正在被 Kimi Code CLI 取代** |

---

## 1. vulcan-code 分层剖析

按要求路径：WebUI → 代码 → 插件 → LuaSkills → 其他子系统。

### 1.1 从 WebUI 反推功能面（事实基线）

WebUI 顶层区域由 i18n 键 `area.*` 定义，**8 个一级区域**：

| 键 | 中文 | 英文 |
| --- | --- | --- |
| `area.chat` | 工作台 | Workspace |
| `area.skills` | 插件 | Plugins |
| `area.agentSkills` | 技能 | Skills |
| `area.agents` | 智能体 | Agents |
| `area.memory` | 记忆 | Memory |
| `area.contextEngine` | 代码语义索引 | Code semantic index |
| `area.knowledge` | 知识库 | Knowledge |
| `area.settings` | 设置 | Settings |

组件规模：17 个组件域，789 个 TS/TSX 文件。最大的几个文件反映功能重心：
`conversation-tool-presentation-card.tsx`(160 KB)、`workbench-layout.tsx`(114 KB)、
`composer-capability-controls.tsx`(108 KB)、`custom-provider-models-dialog.tsx`(107 KB)、
`provider-management-settings.tsx`(105 KB)、`AgentsArea.tsx`(104 KB)、`code-context-area.tsx`(86 KB)、
`KnowledgeBaseArea.tsx`(85 KB)、`session-todolist-panel.tsx`(77 KB)。

设置面（i18n `settings.*` 二级分组计数，共 1 889 条）反映供应商/模型能力的深度：

| 分组 | 条目 | 分组 | 条目 |
| --- | --- | --- | --- |
| customModels | 364 | customProviders | 174 |
| providerTools | 150 | modelExperience | 134 |
| credentials | 128 | inference | 92 |
| models | 64 | providers | 52 |
| modelOverrides | 48 | mediaDelivery | 48 |
| contextWindowPolicy | 22 | contextCompactionNodes | 22 |
| diagnostics | 12 | runtimeDiagnostics | 2 |

12 个媒体能力族（`settings.ai*`）：图像生成/编辑、视频生成/编辑/续写、语音合成/转写、音频理解、图像理解、视频理解、音乐生成/翻唱、网页搜索/抽取。

**结论**：vulcan-code 的产品重心是「**桌面工作台 + 供应商/模型治理 + 记忆与知识 + 代码语义索引**」，而不是「终端 CLI」。

### 1.2 服务与 RPC 面

- 桌面壳：`apps/vulcan-code-tauri`（Tauri 2，`tauri.conf.json`，`bundle.active: false`，配置了严格 CSP，媒体走 `vulcan-media:` 自定义协议）。
- WebUI 通过 HTTP + WebSocket 连本地服务：默认 `http://127.0.0.1:17891`（release）/`17921`（dev），并带端口扫描与 token/clientId（`apps/vulcan-code-web/src/service/browser-service-discovery.ts:39-45,177-185`）。
- **RPC 方法实测 494 个**，分布在 9 个 dispatch 模块：

| 模块 | 方法数 | 模块 | 方法数 |
| --- | --- | --- | --- |
| configuration | 154 | conversation_runtime | 94 |
| extensibility | 92 | knowledge_base | 66 |
| project_conversation | 38 | vce | 26 |
| conversation_events_v1 | 10 | workspace_io | 8 |
| session_resources_v1 | 6 | | |

`configuration` 独占 154 个方法（供应商/模型/凭据/协议/媒体投递/上下文窗口策略），印证「供应商治理」是最大功能块。

### 1.3 可执行入口（能力边界的关键事实）

实测 `main.rs` / `[[bin]]` 仅 3 个：
`apps/vulcan-code-tauri`、`crates/vulcan-terminal-host`、`crates/vulcan-provider-manifest-cli`。

**vulcan-code 没有 headless/one-shot Agent CLI**（对比 codex 的 `codex exec`、OpenCode 的 `run`、qwen-code 的 `--prompt`）。这是一个明确的能力缺口。

### 1.4 Agent 编排

**7 个核心子代理工具**（`crates/vulcan-subagent-core/src/lib.rs:56-64`）：
`spawn_agent`、`agent_orchestration_info`、`send_message`、`wait_agent`、`interrupt_agent`、`close_agent`、`list_agents`。

`TaskRuntime`（`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime.rs:1029-1077`）持有：
durable return mailbox、task session 记录、backend 处理器、执行上下文、活动任务 abort 句柄、
**按任务的事务锁**、detached 事件流、session-boundary mailbox、
`wait_agent` 唤醒用的单调活动计数器、每子代理 inbox、`SubagentRegistry`、`SubagentCatalog`、`max_depth`。

关键限制：**`DEFAULT_TASK_NESTING_LIMIT: usize = 1`**（同文件 `:82`）——子代理不能再派生子代理。

子代理目录：`crates/vulcan-runtime-assets/resources/subagents/` 下 **269 个定义 / 19 个分类**：

| 分类 | 数量 | 分类 | 数量 | 分类 | 数量 |
| --- | --- | --- | --- | --- | --- |
| specialized | 61 | engineering | 42 | marketing | 42 |
| gis | 13 | security | 10 | testing | 9 |
| design | 9 | sales | 9 | finance | 8 |
| project-management | 7 | paid-media | 7 | support | 7 |
| spatial-computing | 6 | academic | 6 | game-development | 5 |
| product | 5 | supply-chain | 4 | legal | 2 |
| hr | 2 | | | | |

子代理模式：`SUBAGENT_MODE_DEFAULT` / `SUBAGENT_MODE_EXPERT` / `SUBAGENT_MODE_MIRROR`（`vulcan-task-session`）。

**模式（mode）**：Plan 模式与 Goal 模式是**原子状态转换**，带类型化输入与回执
（`crates/vulcan-session-core/src/v1/document/modes.rs`，`PlanModeTransitionInput`、`GoalModeTransition`、互斥校验）。

Goal 域：`get_goal`/`create_goal`/`update_goal` + token 预算 + 自动续跑 + 终态（complete/blocked/usage_limited/budget_limited）。

### 1.5 工具系统

**原生工具 23 + 1 条件**（`crates/vulcan-agent-runtime/src/tooling/native.rs:487-514`）：

| 类别 | 工具 |
| --- | --- |
| 文件 | `file`、`file_edit`、`apply_patch` |
| 终端 | `shell_command`、`terminal_session` |
| 规划/目标 | `todolist`、`submit_plan`、`get_goal`、`create_goal`、`update_goal` |
| 知识库 | `knowledge_search`、`knowledge_browse`、`knowledge_read` |
| 语义检索 | `codebase_retrieval`（VCE） |
| 技能 | `skill`（prompt skill 详情） |
| 记忆 | `vmm_memory_search`、`vmm_turn_details`、`vmm_memory_write`、`vmm_memory_delete` |
| 宿主自省（`ai_visible:false`） | `native_workspace_describe`、`native_registry_list_agents`、`native_registry_list_skills`、`native_mcp_list_servers` |
| 条件 | `runtime-config`（仅当 LuaSkills 运行时存在） |

另有 `shell_session` 仅 `#[cfg(test)]`，不随产品发布。

工具执行请求携带完整宿主权限上下文（`NativeToolExecutionRequest`）：
session id、host project id、workspace root、controller runtime、goal runtime、knowledge access、
VCE retrieval handler、spill root、prompt skills、project/session VMM 配置层。

**三源合流**：原生工具 → LuaSkills 入口 → Vulcan 插件工具 → MCP 工具，在同一 `VulcanToolsManager` 中按公开 ID 抢占去重（`crates/vulcan-agent-runtime/src/tooling/bridge.rs`）。

### 1.6 LuaSkills 子系统（自研上游）

- 版本：根 `Cargo.toml:151` `luaskills = "=0.5.5"`；上游仓库在 `D:\projects\vulcan-luaskills`。
- 形态：**进程内嵌入 LuaJIT**（`mlua` + `luajit52` + `vendored`），非子进程、非 RPC。
- 宿主句柄：`Arc<Mutex<LuaEngine>>`（`crates/vulcan-agent-runtime/src/luaskills_runtime/types.rs:50`）。
- 技能包：`output/lua_runtime/skills/` 下 `vulcan-codekit`(0.1.11)、`vulcan-curl`、`vulcan-file`。
- codekit 7 个工具：`ast-detail`、`ast-tree`、`rg`、`markdown-menu`、`node-source`、`patch`（+ help）。
- **FFI 依赖机制**：`dependencies.yaml` 的 `ffi_dependencies` 声明 `ast-grep-ffi 0.1.7`，运行时下拉
  `vulcan_codekit_ast_grep_ffi.dll`（实测 **42.4 MB**），Lua 侧经 LuaJIT `ffi.load` 调用 3 个 C 导出
  （`vulcan_codekit_ast_grep_version/scan_json/free_string`）。这是仓库中唯一「Lua 调用 Rust 原生库」的先例。
- 宿主回调面（Vulcan 实际安装）：`set_sqlite_provider_callback`、`set_lancedb_provider_callback`、
  `set_host_tool_callback`、`set_skill_operation_progress_callback`。
  **未安装**：`set_model_llm_callback`、`set_model_embed_callback`、`set_entry_registry_callback`、
  `set_skill_management_callback`、`set_skill_lifecycle_callback`、`set_log_callback`、
  `set_managed_session_wake_callback`（上游文档明确这些是宿主职责）。

### 1.7 Vulcan 插件子系统

- 清单：`plugin.yaml`，`schema_version: 1`，`#[serde(deny_unknown_fields)]` 严格校验。
- 15 个权限：`runtime_lua_modules`、`network_https`、`process_binary`、`process_node`、`process_python`、
  `process_session`、`config`、`secrets_read`、`ui_host_context`、`model_catalog_read`、
  `system_vldb_access`、`model_invoke`、`dependency_registry_network`、`storage_sqlite`、`storage_lancedb`。
- 能力声明：`execution.{native_binaries,node_entries,python_entries}`、`lua_runtime.{bundle,actions,tools,hooks,lifecycle}`、
  `mcp.servers`（stdio/http/sse + literal/config/secret/environment 值来源 + OAuth）、`ui.mounts`、
  `config.{schema_file,defaults_file}`、`prompt_contributions`、`locales`。
- 已装插件 4 个：`context7`、`vulcan-rtk`、`vulcan-debug-runtime`、`vulcan-usage-statistics`。
- 关键事实：**插件运行时自身零进程创建**（仅 pnpm/uv 依赖安装器会 spawn），Node/原生二进制/Python
  全部是 LuaSkills 的子进程；Vulcan 只做「清单解析 + 路径约束 + 能力门 + 进程规格校验」后交回桥。
- 沙箱：插件 Lua 环境由宿主预载 `bridge.lua` 构建，**按构造移除 `require/io/os/package/load/debug/ffi/jit/bit`**，
  原生模块仅 3 名白名单（`lcurl.safe`、`openssl.cipher`、`openssl.rand`）。

### 1.8 其他关键子系统

| 子系统 | 事实 |
| --- | --- |
| VCE 代码语义索引 | `crates/vulcan-context-engine`，`engine.rs` 单文件 **535 KB**；含 `symbol`/`embedding`/`retrieval`/`indexing`/`structure`/`query`/`workspace`/`management` |
| VMM 记忆 | `crates/vulcan-vmm`，`session_state.rs` **150 KB**；后端模式 `Native`（默认，Rust 原生 VMM Core）或 `ExternalGrpc` |
| 会话状态机 v1 | `vulcan-session-core/src/v1/document`：`api.rs` 96 KB、`input.rs` 93 KB、`tools.rs` 44 KB、`turns.rs` 42 KB、`modes.rs` 42 KB、`generation.rs` 47 KB、`event.rs` 61 KB、`hydration.rs` 36 KB |
| 上下文压缩 | `vulcan-agent-runtime/src/agent_runtime/prompt_compaction.rs`(81 KB) + `vulcan-context-compaction` + 可配置压缩窗口节点 |
| 知识库 | BM25（jieba 分词）+ LanceDB 向量 + rerank；导入/重建/修复/完整性/自定义词/包导入导出 |
| MCP 客户端 | `vulcan-mcp-runtime`：`stdio`(24 KB)、`sse`(35 KB)、`http`(13 KB)、`auth`(16 KB)、`rmcp_support`(21 KB) |
| 供应商清单 | 18 份 bundled manifest；`alibaba.json` 350 KB、`minimax.json` 141 KB；Schema `provider-manifest-v2.schema.json` **90 KB** |
| 清单热更新 | Ed25519 签名 index（`bundled-index.json`，`index_revision: 60`）+ 原子激活/回滚/隔离 |
| 供应商系统提示词 | 8 份：`anthropic`、`beast`、`codex`、`default`、`gemini`、`gpt`、`kimi`、`trinity` |
| 桌面通知 | `desktop_notifications.rs`(28 KB)；终端宿主独立二进制 `vulcan-terminal-host` |

### 1.9 vulcan-code 的安全模型（重要短板）

实测：**crates/apps 全仓搜索 `landlock|seatbelt|seccomp|bwrap|AppContainer|RestrictedToken` 零命中**。

即 vulcan-code **没有实现任何 OS 级沙箱**。其现有约束是「策略级」而非「内核级」：

- 工具可见性策略：`VulcanPlanToolPolicy`、工具暴露快照（`allowed_ai_capabilities`/`disabled_ai_capabilities`/`luaskills_overrides`/`mcp_server_overrides`）；
- 子代理能力策略 sidecar（`vulcan-task-session/src/catalog/capability_policy.rs`）；
- 插件 15 权限 + 路径包含校验 + `allowed_env` 白名单 + 进程规格不可覆盖；
- Plan 模式 gate（`available_in_plan`）；
- 凭据走 OS 凭据库（`keyring`），Secret 不入清单/日志。

这是与 codex/dsh 差距最大的一维。

---

## 2. 五个对比项目剖析

### 2.1 codex（Rust，约 120 crates）

**形态**：CLI/TUI、`codex exec` headless、IDE（VS Code/Cursor/Windsurf）、`codex app` 桌面、
Web、**app-server 协议**（`app-server`/`-protocol`/`-transport`/`-daemon`/`-client`）、`exec-server`、SDK。

**工具面**（从 `core/src/tools` 取证）：`exec_command`、`write_stdin`、`apply_patch`、`update_plan`、
`view_image`、`request_user_input`、`request_permissions`、`send_message_to_user`、`tool_search`、
`mcp` + MCP 资源（`list_mcp_resources`/`list_mcp_resource_templates`/`read_mcp_resource`）、
`list_available_plugins_to_install`、`request_plugin_install`、`dynamic`、`extension_tools`、
`current_time`、`get_context_remaining`、`new_context_window`、`sleep`、`wait_for_environment`、
`test_sync`、`web_search`（hosted）、**`code_mode`**（execute/wait/delegate）、多智能体两代。

**多智能体**：v1（`spawn`/`send_input`/`wait`/`close_agent`/`resume_agent`）+ v2
（`spawn`/`send_message`/`wait_agent`/`interrupt_agent`/`list_agents`/`followup_task`）；
配套 `agent-roles`、`agent-identity`、`agent-graph-store`、`collaboration-mode-templates`。

**安全（最强项）**：`sandboxing`（seatbelt 41 KB、landlock、windows 17 KB、mxc、bwrap、violation、denial、
policy_transforms）、`linux-sandbox`（**bundled bwrap 110 KB**）、`windows-sandbox-rs`、`windows-sandbox-service`、
`execpolicy`、`process-hardening`、`network-proxy`、`shell-escalation`、approvals 多模式 +
`request_permissions` 的 `sandbox_permissions` 升级机制。

**模型**：OpenAI 优先（ChatGPT 登录、Responses API、`responses-api-proxy`）；
`model-provider`/`models-manager`/`ollama`/`lmstudio`/`aws-auth`；`realtime-webrtc`+`voice-host`（实时语音）。

**其他**：`rollout`/`rollout-trace`/`thread-store`/`history`/`message-history`（会话格式）、`memories`、
`file-search`/`file-watcher`/`worktree`/`git-utils`、`otel`+`otel-trace-websocket`+`analytics`（可观测性最强）、
`cloud-tasks`/`chatgpt`/`backend-client`、`skills`/`plugin`/`core-plugins`/`hooks`/`ext`、`code-mode-runtime`。

### 2.2 OpenCode（TypeScript + Go）

**形态**：CLI、`run` headless、Go TUI、server、web、desktop、console、slack、session-ui、
storybook；SDK（`sdk`/`sdk-next`）、client；`plugin`、`protocol`、`schema`、`llm`、`identity`、
`enterprise`、`containers`、`http-recorder`、`httpapi-codegen`。

**工具面**（`packages/opencode/src/tool`）：`apply_patch`、`code-mode`、`edit`、`external-directory`、
`glob`、`grep`、**`lsp`**、`mcp-websearch`、`plan`、`question`、`read`、`shell`、`skill`、`task`、
`todo`、`webfetch`、`websearch`、`write`。

**Agent**：`agent.ts` 定义 `mode: ["subagent","primary","all"]`、`permission: PermissionV1.Ruleset`、
`model` 覆盖；内置 build/plan/general/explore 等；插件可注册 agent。

**权限**：`permission` 模块提供 `evaluate(permission, pattern, ...rulesets)`，动作 `allow`/`deny`/`ask`，
支持通配 `*`、按 agent 合并规则集（`Permission.merge`）、`external_directory` 独立权限。
**无 OS 级沙箱**。

**模型**：以 models.dev 目录为基座，供应商覆盖最广；`llm` 包做统一抽象；处理 OpenAI OAuth 特例。

**生态**：插件 API + hooks（含 `experimental.chat.system.transform`）、自定义命令、AGENTS.md、
skills、30 余语言 README、主题系统、Go/TS 双 SDK。

### 2.3 qwen-code（TypeScript，体量最大）

**形态（最广）**：CLI、TUI、desktop app、web UI、**Chrome 扩展**、IDE（VS Code/Zed/JetBrains）、
chat channels（Telegram/DingTalk/WeChat/Feishu）、SDK（Java/Python/TypeScript）、`acp-bridge`、
`mobile-mcp`、**`cua-driver`（computer use）**、`audio-capture`、`live-host`、`qwen-live`、
`node-repl`、`web-shell`、`web-templates`。

**工具面 50 个**（`tool-names.ts` 权威常量）：
`exec`、`edit`、`write_file`、`read_file`、`zoom_image`、`grep_search`、`glob`、`run_shell_command`、
`todo_write`、`save_memory`、`agent`、`skill`、`enter_plan_mode`、`exit_plan_mode`、`web_fetch`、
`web_search`、`image_gen`、`list_directory`、`lsp`、`ask_user_question`、`cron_create`、`cron_list`、
`cron_delete`、`loop_wakeup`、`create_sub_session`、`list_agents`、`task_stop`、`task_create`、
`task_update`、`task_list`、`team_create`、`team_delete`、`team_plan_approval`、`request_shutdown`、
`send_message`、`structured_output`、`monitor`、`notebook_edit`、`tool_search`、`read_mcp_resource`、
`enter_worktree`、`exit_worktree`、`workflow`、`artifact`、`record_artifact`、`record_source`、
`report_findings`、`get_goal`、`update_goal`、`propose_goal`、`display_image`。

独特能力：**Agent Teams**（团队创建/删除/计划审批）、**cron 定时任务**、**loop_wakeup**、
**worktree 进出**、**子会话**、**artifact/source/findings 记录**、**goal 提议**、`monitor`、
`notebook_edit`、`structured_output`。

**MCP 投入巨大**：`mcp-client-manager.ts` 142 KB、`mcp-tool.ts` 44 KB、`mcp-transport-pool.ts` 59 KB、
`mcp-pool-entry.ts` 59 KB、资源读取、重试、状态、工作区预算。

**权限**：`ApprovalMode`（含 `yolo`）、`file-read-permission`、`priorReadEnforcement`、
`grepReadTracking`；识别 macOS `sandbox-exec` 环境但未见自建 OS 沙箱。

**模型**：OpenAI/Anthropic/Gemini/Qwen + Ollama/vLLM，运行时切换；web-search 多后端（dashscope）。

### 2.4 kimi-cli（Python，正在被取代）

**形态**：CLI/TUI + ACP（编辑器集成）+ Shell 命令模式（Ctrl-X）。

**工具面**（`src/kimi_cli/tools`）：`agent`、`ask_user`、`background`、`dmail`、`think`、
`file.{glob,grep_local,plan_mode,read_media,read,replace,write}`、`plan.{enter,heroes}`、
`shell`、`todo`、`web.{fetch,search}`、`test`。

**权限**：`approval_runtime` 定义 `ApprovalResponseKind = approve | approve_for_session | reject`；
后台任务状态含 `awaiting_approval`；`hooks/runner.py` 支持 `permissionDecision == "deny"`。

**重要**：README 明确 *"Kimi CLI is evolving into Kimi Code CLI … This project will be gradually wound down"*。
故其评分应视为「存量参考」，不代表 Moonshot 现行能力上限。

### 2.5 deepseek-harness / dsh（everything-is-a-plugin）

**形态**：`npx @deepseek-ai/dsh web`（Web UI 默认 `127.0.0.1:3080`）、CLI、desktop、desktop-host、web；
`acp`、`sdk`；43 个功能包（`acp`…`workspace`）。

**工具面 62 个**（`packages/core/tools/tests/gen-tool-catalog.spec.ts:28-41` 权威列出）：
`ask_user_question`、`bash`、`pwsh`、`cordis_define/inspect_list/inspect_query/inspect_self/run/stop/undefine`、
`create_goal`、`edit`、`exit_plan_mode`、`get_goal`、`glob`、`grep`、`interrupt_agent`、
`job_kill/job_list/job_output`、`list_agents`、`list_subagent_models`、`lsp`、`present`、`ralph`、
`read`、`read_image`、`run_code`、`schedule_create/delete/list`、`send_message`、
`session_event_read/search/trace`、`session_search`、`session_trace`、`skill`、`spawn_teammate`、
`str_replace_editor`、`subagent`(别名 `subagent_fork`)、`team_task_create/get/list/update`、
`terminal_close/list/open/read/send/signal`、`todo_write`、`update_goal`、`wait_agent`、
`web_fetch`、`web_search`、`workflow`、`write`。

独特能力：
- **`cordis_*` 6 个工具 = Agent 可在运行时定义/运行/停止/注销新插件**（自扩展闭环）；
- **`ralph`**（fresh-agent 迭代循环）；
- **`workflow`**（JS 脚本编排多子代理，含 `parallel`/`pipeline`）；
- **`job_*`**（后台作业带 id、流式输出、wait/kill）；
- **`schedule_*`**（定时）；**`session_*`**（会话事件检索/追踪）；
- **`team_task_*` + `spawn_teammate`**（团队任务）；
- **`run_code`**；**`present`**（交付物声明）；**`lsp`**；`str_replace_editor`。

**安全**：`sandbox`（escalation/roots）+ `sandbox-local`（27 KB，**带 `landlock.e2e.ts`/`bwrap.e2e.ts` 端到端测试**）
+ `native/landlock-run` + `e2b`（云端沙箱）+ `guard`（repeat-tool-reminder、timeout-policy）+ `user-approval`。

**架构**：基于 Cordis 的「everything-is-a-plugin」，客户端插件支持 HMR。

---

## 3. 能力维度打分

评分口径：**0–5 分，5 = 该维度同类最佳**。打分基于上文实测证据，非主观印象。

| # | 维度 | vulcan-code | codex | OpenCode | qwen-code | kimi-cli | dsh |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 交付形态广度 | **2.5** | 4.5 | 4.0 | **5.0** | 2.0 | 3.5 |
| 2 | Agent 编排与多智能体 | 4.0 | **5.0** | 3.0 | 4.5 | 2.5 | 4.5 |
| 3 | 工具系统完备度 | 4.0 | 4.5 | 3.5 | **5.0** | 2.5 | **5.0** |
| 4 | 扩展与插件生态 | 4.5 | 4.5 | 4.0 | 4.0 | 2.5 | **5.0** |
| 5 | 上下文与记忆 | **5.0** | 4.5 | 3.0 | 4.0 | 3.0 | 4.5 |
| 6 | 模型/供应商与多模态 | **5.0** | 4.0 | 4.5 | 4.0 | 2.5 | 3.5 |
| 7 | 权限与安全沙箱 | 2.0 | **5.0** | 3.5 | 3.0 | 3.0 | 4.5 |
| 8 | 会话持久化与可恢复 | **5.0** | 4.5 | 3.5 | 4.0 | 3.0 | 4.5 |
| 9 | 并发与长任务 | 3.0 | 4.5 | 3.5 | 4.5 | 3.0 | **5.0** |
| 10 | 可观测性 | 3.5 | **5.0** | 3.0 | 3.5 | 2.0 | 3.5 |
| 11 | 工程成熟度 | 4.5 | **5.0** | 4.0 | 4.5 | 3.0 | 4.0 |
| 12 | 人机交互 UI 丰富度 | **5.0** | 3.5 | 4.5 | 4.5 | 2.5 | 4.0 |
| | **合计 /60** | **48.0** | **54.5** | **44.0** | **50.5** | **31.5** | **51.5** |

### 3.1 各维度打分依据（简证）

| 维度 | vulcan-code 打分依据 |
| --- | --- |
| 1 = 2.5 | 仅 Tauri 桌面 + 本地 Web 服务；**无 headless CLI**；实测 main.rs 仅 3 个 |
| 2 = 4.0 | 7 子代理工具 + 269 定义 + registry/mailbox + Plan/Goal 原子模式；**但 nesting=1**、无 workflow/团队原语 |
| 3 = 4.0 | 24 原生 + LuaSkills 10 + 插件 15+ + MCP + Provider 工具；但缺 LSP、notebook、artifact、cron 类工具 |
| 4 = 4.5 | 自研 LuaSkills（含 FFI 依赖链）+ Vulcan 插件（15 权限/UI 挂载/MCP/生命周期）+ 供应商清单热更新；生态成熟度高 |
| 5 = 5.0 | VCE 语义索引(535 KB engine) + VMM 持久记忆(150 KB) + 知识库(BM25/向量/rerank) + 压缩窗口策略；**同类最深** |
| 6 = 5.0 | 18 manifest + 90 KB Schema + 签名热同步回滚 + 4 协议 + 逐模型协议绑定 + 12 媒体族 + 用量成本 + 密钥故障转移 + 8 份供应商系统提示词 |
| 7 = 2.0 | **无 OS 沙箱**（实测零命中）；仅策略级约束 |
| 8 = 5.0 | v1 document 引擎（api 96 KB/input 93 KB/event 61 KB/hydration 36 KB）+ 草稿 + 会话资源 + 归档恢复 + 事件窗口 |
| 9 = 3.0 | 有终端会话/分离事件流/取消；但子代理不可嵌套（nesting=1）＋ LuaSkills 引擎锁持锁范围问题（见 §6 专项） |
| 10 = 3.5 | provider_request_telemetry(148 KB)、API trace 目录、turn stats、用量统计插件、运行时诊断 |
| 11 = 4.5 | 5 199 Rust 测试 + 263 前端测试 + AGENTS.md 纪律 + 18 篇文档（含 91 KB 作者手册）+ deny.toml + nextest 配置 |
| 12 = 5.0 | 789 TS 文件/17 组件域/完整桌面工作台（diff 审阅、浮动终端、媒体播放、工具呈现卡 160 KB）+ 690 KB i18n |

---

## 4. 结论

### 4.1 vulcan-code 的真实定位

vulcan-code 不是「又一个终端 coding agent」，而是一个**桌面级 Agent 工作台 + 供应商/模型治理平台**。
它在三个维度上是六个项目里最强的：

1. **模型/供应商治理（5.0）**：18 份 manifest + 90 KB Schema + Ed25519 签名热更新与回滚 + 逐模型协议绑定 + 12 个媒体能力族。这是它最深的护城河。
2. **上下文与记忆（5.0）**：代码语义索引 + 持久记忆 + 知识库三套独立子系统，深度超过 codex 的 memories 与 qwen-code 的 Auto-Memory。
3. **人机交互（5.0）**：唯一一个把完整桌面工作台（会话、diff、终端、媒体、插件 UI、记忆面板）做进开源仓库的项目。
4. **会话状态机（5.0）**：v1 document 引擎的原子性与可恢复性设计（generation/turns/hydration/event）在同类中最严谨。

### 4.2 三个明确短板

| 短板 | 证据 | 影响 |
| --- | --- | --- |
| **无 OS 级沙箱** | 全仓 `landlock/seatbelt/seccomp/bwrap/AppContainer` 零命中 | 与 codex(5.0)/dsh(4.5) 差距最大；企业/多租户场景是硬门槛 |
| **无 headless/CLI 形态** | 实测仅 3 个 binary，无 Agent CLI | 无法进 CI、脚本、远程/无 GUI 环境；生态位受限 |
| **LuaSkills 全局串行** | `Arc<Mutex<LuaEngine>>` 且调用期持锁 | 多 Agent 并发时全部 LuaSkills 工具排队；且上游 `LuaEngine` 全部入口为 `&self`、字段皆为 `Arc` 不可变——**这把锁是 Vulcan 侧多余的** |

### 4.3 与各项目的能力互补关系

| 若要对标… | 应看 | 理由 |
| --- | --- | --- |
| 安全/沙箱 | codex | seatbelt/landlock/windows/bwrap + execpolicy + 升级审批，同类最完整 |
| 工具数量与 Agent 自扩展 | dsh | 62 工具 + `cordis_*` 运行时定义插件 + workflow/ralph/jobs |
| 交付形态与外部集成 | qwen-code | CLI/IDE×3/浏览器/桌面/4 个 IM 渠道/3 语言 SDK/CUA |
| 供应商覆盖广度 | OpenCode | models.dev 基座，供应商数量最多 |
| 供应商治理深度 | **vulcan-code 自己** | 签名热更新 + Schema 化 + 媒体能力族，另五家均无同级机制 |
| 会话状态严谨性 | **vulcan-code 自己** | v1 document 引擎的原子转换与 hydration 设计 |

---

## 5. 专项：LuaSkills 并发与串行化的取证结论

本节针对「串行究竟发生插件初始化还是 LuaSkills 工具调用」这一分歧，逐条列出**代码事实**，不下结论式断言。

### 5.1 事实 A：VM 池数量确实可由宿主配置（用户指正成立）

| 证据 | 内容 |
| --- | --- |
| `crates/vulcan-agent-runtime/src/luaskills_runtime.rs:125-129` | `LuaVmPoolConfig { min_size: runtime_settings.vm_pool.min_size, max_size: .., idle_ttl_secs: .. }` → 池参数来自宿主运行时设置 |
| `crates/vulcan-luaskills-config/src/lib.rs:384-393` | 默认值：`vm_pool {1, 2, 30}`、`run_lua_pool {1, 4, 60}` |
| `rpc_dispatch/extensibility.rs:26, 192-199` | RPC `service_save_luaskills_runtime_settings` 可持久化修改 |
| 上游 `src/runtime/engine.rs:9304-9306` | `acquire_vm(&self)` → `self.pool.acquire(..)`，池具备租约式并发取用能力 |

→ **池层面的并发能力存在且可调**。此前报告未强调「可配置」这一点，属于表述不足。

### 5.2 事实 B：插件初始化确实在每次 lease 创建时重读并重编译（用户指正成立）

| 证据 | 内容 |
| --- | --- |
| `crates/vulcan-plugin-runtime/src/runtime/lease.rs:67` | `fs::read_to_string(&bundle_path)` — **每次** lease 创建都从磁盘读 `plugin.bundle.lua` |
| 同文件 `:107` | `fs::read_to_string(&bridge_module_path)` — 同时读 `bridge.lua`（22.6 KB） |
| `plugin-bridge/.../bridge.lua:364` | `load(bundle_source, chunk_name, "t", plugin_environment)` — 把整份 bundle 编译进新建沙箱环境 |
| `runtime/sessions.rs:81-180` | `stateless` 工具每次调用都走 create_lease → dispatch → shutdown → close |
| 实测 bundle 体积 | context7 87.7 KB、usage-statistics 79.3 KB |

→ 插件初始化是**真实的串行且高成本**环节，且 `vulcan-plugin-runtime` 无 bundle 体积上限。

### 5.3 事实 C：Vulcan 侧同一把引擎锁在多个路径上被持有到操作结束

| 取锁站点 | 用途 | 持锁范围 |
| --- | --- | --- |
| `luaskills_runtime.rs:671-682` | **工具调用** `call_skill` | 守卫活到闭包结束，**覆盖整次 `call_skill`** |
| `luaskills_runtime.rs:485` | 技能管理（reload/install/…） | 覆盖整个管理动作 |
| `luaskills_runtime.rs:464` | 重建技能目录 | 覆盖目录构建 |
| `luaskills_runtime.rs:631` | 渲染 entry help | 覆盖 help 渲染 |
| `runlua.rs:383,459,664,737,804,824,1018` | runlua / 持久租约 | 覆盖各自 Lua 操作 |
| `vulcan-plugin-runtime/src/runtime/lease.rs:321-325` | 插件操作 `call_engine_blocking` | `engine.lock()` 后 `operation(&engine)`，**覆盖整个操作** |

关键旁证（代码自身措辞）：

- `luaskills_runtime.rs:668-669` 注释：*"measures only contention for the **process-wide** LuaSkills engine"*（进程级）；
- `:673-677` 专设 `[luaskills-runtime][engine-lock-slow]`（≥1 s 竞争打点）；
- `:684-689` 另设 `[luaskills-runtime][skill-call-slow]`，两者分离说明「锁等待」与「Lua 体耗时」被区别观测。

引擎共享范围：`service_bootstrap.rs:780` `Arc::clone(&runtime_handle.engine)`，
经 `attach_runtime_engine`（`vulcan-plugin-runtime/src/runtime.rs:525`）交给插件运行时。
**Agent 技能调用与插件操作共用同一个 `Arc<Mutex<LuaEngine>>`。**

### 5.4 事实 D：上游并不要求这把锁

| 证据 | 内容 |
| --- | --- |
| `D:\projects\vulcan-luaskills` 全仓 grep | `Mutex<LuaEngine>` **零命中** |
| 上游 `src/runtime/engine.rs` | **零个** `pub fn ...(&mut self)`；`call_skill` 等入口均为 `&self` |
| 上游 `LuaEngine` 字段 | `skills`/`entry_registry`/`pool`/`runlua_pool`/各服务均为 `Arc<..>` 共享；无 `Rc`/`RefCell`/`Cell` |
| 上游 `src/ffi.rs:50-52` | 其自带 `Mutex<Option<LuaEngine>>`，注释说明用途是 *"removal forms a quiescent free barrier"*（**引擎替换/释放屏障**），非逐次调用串行 |
| 上游 `src/ffi.rs:76-84` | 线程局部 `ACTIVE_FFI_ENGINE_IDS`，守卫语义为 *"reject **same-thread reentrant** access"*（仅拒同线程重入） |

→ 上游的锁用于「引擎代次替换」与「同线程重入保护」；**跨线程并发未被上游禁止**。

### 5.5 综合与待裁决点

两条观察可以同时为真：

1. 锁确实在工具调用期间被持有（事实 C）→ 池的 `max_size` 对入口调用吞吐**不产生**提升；
2. 但**实际体感的主要串行源是插件初始化**（事实 B）——因为 init 持锁时长（磁盘读 + 编译约 80 KB）远大于常规技能调用。

因此「串行在插件初始化」与「锁在工具调用期间被持有」并不矛盾：**同一把锁，init 持有得久得多。**

**需要项目方裁决的一个问题**：`lock_runtime_engine` 覆盖 `call_skill` 全程是**有意**（认定 `LuaEngine` 不保证并发安全，池仅用于 runlua 隔离），还是**锁范围过宽**（本意只保护引擎代次/句柄）？

**可一次性裁决的实验**（无需改代码，只需配置）：
将 `vm_pool.max_size` 由 2 调到 8，并发发起 8 次重 LuaSkills 调用（如 8 个不同目录的 `codekit-ast-tree`），测量墙钟时间。

- 若耗时 ≈ 单次 × 8 → 锁在调用期被持有，池无效；
- 若耗时 ≈ 单次 × 1 → 池生效，此前「全局串行」判断错误。

日志中同时观察 `[engine-lock-slow]`（锁竞争）与 `[skill-call-slow]`(Lua 体) 的分布，可直接区分两类耗时。

---

## 6. 未证实事项（明确标注，不猜测）

1. codex 与 dsh 的确切测试用例总数未逐一统计（仅统计了文件规模与测试文件存在性）。
2. kimi-cli 的 MCP 支持范围未完整取证（未在 `src/` 找到独立 MCP 客户端实现）。
3. OpenCode 的上下文压缩与长任务恢复细节未深入取证。
4. 各项目的真实端到端性能未做基准测试；本报告的并发结论基于代码结构而非实测吞吐。
5. `output/` 目录与权威资源的一致性此前已抽样验证（vulcan-debug-runtime 15 文件、bridge.lua 逐字节相同），但未全量校验。
