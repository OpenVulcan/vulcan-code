# 会话临时 MCP

本文是会话临时 MCP 连接的契约：AI 在会话内临时连接一个 MCP 服务器，查看其工具并调用，连接不写入任何配置。
需要长期使用的服务器由用户在 MCP 设置页配置，或经 AI 配置工具 `mcp_config` 写入用户配置（见
[`ai-configuration-tools.md`](ai-configuration-tools.md)）。实现以下列源码为准，
修改其中任一处时同步本文与对应测试：

- 连接注册表、边界常量、Schema 校验与断线跟踪：`crates/vulcan-agent-runtime/src/mcp/session.rs`
  （测试 `mcp/session/tests.rs`）
- 三个工具、参数 Schema、密钥取值与结果投影：`crates/vulcan-agent-runtime/src/tooling/native/session_mcp.rs`
- 计划模式策略：`crates/vulcan-agent-runtime/src/tooling/bridge/descriptors.rs`（`native_plan_policy`），
  按参数启发式 `VulcanPlanToolPolicy::HeuristicStringArgument`（`crates/vulcan-tools-manager/src/manager.rs`）
- 权限分类：`crates/vulcan-agent-runtime/src/agent_runtime/tool_permission.rs`（`classify_mcp_connect`、`mcp_call` 分支）
- 根会话解析与关闭入口：`crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/runtime_state.rs`
  （`root_session_of`、`close_session_mcp_connections`）
- 归档清理：`crates/vulcan-service-kernel/src/service_state_access.rs`（`close_session_mcp_connections_for_owners`），
  调用点为会话归档与项目归档/移除
- 传输层：`crates/vulcan-mcp-runtime`（与已配置服务器共用 initialize 超时、启动信号量与 `tools/list` 超时常量，
  见 `crates/vulcan-agent-runtime/src/mcp/runtime/handle.rs`）
- 界面：只有工具卡（`apps/vulcan-code-web/src/conversation/tools/cards/document-card.tsx`），没有专门面板

## 工具

1. **三个固定工具。** `mcp_connect`（`connect` / `status` / `disconnect`）、`mcp_tools`（列出工具，
   或用 `tool` 读取单个工具的说明与输入输出 Schema）、`mcp_call`（调用）。工具列表不随连接变化，
   被连接服务器的工具不会进入 Provider 请求的工具清单。
2. **稳定连接 ID。** `connect` 返回 `mcp_<8 位十六进制>` 形式的随机 ID，之后用它调用；名称只用于辨认，
   在会话内不区分大小写唯一，最长 `SESSION_MCP_NAME_MAX_CHARS` 个字符。
3. **数量上限。** 每个根会话最多 `SESSION_MCP_MAX_CONNECTIONS` 个连接；超限或重名在启动服务器之前拒绝，
   注册阶段再次检查，被拒绝的传输立即关闭。
4. **调用前校验。** `mcp_call` 的参数必须是对象，并按工具的输入 Schema 校验；不合格的调用不到达服务器，
   错误提示用 `mcp_tools` 读取 Schema。未知工具名先重新列出一次工具再判定。
5. **结果原样返回。** 调用结果按 MCP 工具的会话受众原样投影给模型；密钥只经全局脱敏机制替换
   （环境变量登记的密钥值）。

## 连接生命周期

1. **属于根会话。** 连接登记在根会话名下；子代理通过 `TaskRuntime` 的会话记录解析到根会话，可以查看与调用，
   但不能 `connect` 或 `disconnect`。其他会话按“不存在”处理。
2. **不写配置，不自动重连。** 连接只在内存中；Core 重启后连接 ID 失效，调用明确报错并指引用
   `mcp_connect` `status` 查看或重新连接。服务器断开（进程退出、传输关闭）后连接标记为已关闭并保留原因，
   不重连；`disconnect` 移除它。
3. **工具变化。** 服务器发送 `tools/list_changed`（或事件流滞后）时工具列表标记为过期，下次读取时重新列出；
   `mcp_tools` 的 `refresh` 可强制重新列出。
4. **关闭时机。** 会话归档、项目归档或移除时关闭该会话的全部连接（删除前必然已归档）；Core 退出时
   stdio 子进程随进程族作业对象结束。
5. **启动边界复用。** 连接与已配置服务器共用启动信号量；信号量许可覆盖 initialize 与首次 `tools/list`，
   超时使用与已配置服务器相同的常量，不另写字面量。

## 传输与密钥

1. **stdio。** `command` 加 `args` 启动子进程；`cwd` 相对工作区根目录，缺省为工作区根目录。子进程继承服务进程
   环境，并叠加本会话生效的全部 Vulcan 环境变量（会话 > 项目 > 全局），与 `shell_command` 一致。
2. **http / sse。** `url` 必须是 http(s)；不接受字面请求头或令牌，`bearer_token_env` 与 `headers_from_env`
   只接受环境变量名，取值先查本会话生效的 Vulcan 变量（不区分大小写）再查服务进程环境；缺失时报错并指引用
   `env` `request` 请用户填写。不支持 OAuth。
3. **字段不得混用。** stdio 字段与 http/sse 字段同时出现时拒绝。
4. **结果中的目标已脱敏。** 连接快照只显示程序名或源站，不显示参数、路径或查询串。

## 权限与计划模式

1. **stdio 连接按执行命令处理。** 权限主体为 `Command`（命令行由 `session_mcp_command_line` 拼接，含空白的参数
   加引号），不指定 shell；完全访问以外的模式一律升级，高风险规则照常生效。
2. **http/sse 连接与格式错误的连接请求为 `Opaque`。** 完全访问以外一律升级（自动审批模式下按管线交给判定模型、
   复核与用户）。`status` 与 `disconnect` 不受管控。
3. **调用按工具名启发式。** `mcp_call` 的 `tool` 看起来只读（与已配置 MCP 工具相同的 `text_looks_read_only`）时
   不受管控，否则为 `Opaque`。不使用服务器声明的 `readOnlyHint`：工具 Schema 不保留注解，且注解由服务器自报。
   `mcp_tools` 不受管控。
4. **计划模式。** `mcp_connect` 只允许 `status`；`mcp_tools` 可用；`mcp_call` 只允许按同一启发式看起来只读的工具。
