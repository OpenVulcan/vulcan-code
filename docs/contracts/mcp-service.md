# MCP 服务（供外部 AI 调用）

本文是 Vulcan Code 对外 MCP 端点的契约：其他 AI 工具（Claude Code、Codex、Cursor 等 MCP 客户端）通过它
使用 Vulcan 的会话与已授权的图片、视频模型。它与“Vulcan 作为 MCP 客户端连接外部服务器”（插件管理下的
MCP 服务器）是两件事，互不共享配置。实现以下列源码为准，修改其中任一处时同步本文与对应测试：

- 持久配置、边界与规范化：`crates/vulcan-provider-config/src/config/mcp_service.rs`（全局配置字段 `mcpService`）
- 设置页线协议类型：`crates/vulcan-service-api/src/mcp_service.rs`
- 协议、认证、工具目录、等待与媒体任务：`crates/vulcan-mcp-service/`
- 内核后端、设置 RPC 与安装：`crates/vulcan-service-kernel/src/mcp_service.rs`、`mcp_service/backend.rs`、
  `mcp_service/transcript.rs`；事件尾读 `conversation_events_v1.rs`（`read_session_events_after`）
- 后台激活：`crates/vulcan-service-kernel/src/webui_prompt_context.rs`（`SubmissionActivation`）
- HTTP 路由：`crates/vulcan-service-host/src/lib.rs`（`/mcp`）
- 媒体结果落盘：`crates/vulcan-agent-runtime/src/provider_tool_bridge/standalone.rs`
- 界面：`apps/vulcan-code-web/src/components/features/mcp-service/`；TUI 镜像
  `apps/vulcan-code-tui/src/app/pages/mcp_service.rs`

## 传输与认证

1. **端点。** 服务宿主已有的回环 HTTP 端口上的 `/mcp`，实现 MCP Streamable HTTP 的无状态子集：POST 一条
   JSON-RPC 消息，请求返回 `application/json`，通知返回 202；不支持批量；GET 与 DELETE 返回 405 并带
   `Allow: POST`；不使用 SSE。协议版本按 `MCP_SUPPORTED_PROTOCOL_VERSIONS` 协商，未知版本回答最新版本。
2. **浏览器一律拒绝。** 带 `Origin` 请求头的请求在宿主层返回 403（防 DNS 重绑定与网页跨站调用）。
   请求体上限由宿主常量 `WEBUI_SERVICE_MCP_BODY_LIMIT_BYTES` 定义。
3. **每个客户端一枚令牌。** 令牌为 `vmcp_` 前缀加 32 字节随机数，配置只保存 SHA-256 摘要与末 4 位提示；
   原文只在新建或轮换时返回一次，之后任何接口都不能取回。认证用 `Authorization: Bearer <token>`，常量时间
   比较摘要。服务关闭返回 503；令牌缺失或无效返回 401 并带 `WWW-Authenticate`；停用的客户端视为无效令牌。
4. **TUI 不签发令牌。** TUI 的机密清理会清空所有令牌字段，因此新建客户端与轮换令牌只在 WebUI 提供；
   TUI 只镜像端点开关、授权修改与删除。

## 授权

1. **会话授权**是五个开关：读取、发送、新建、停止、回答提问。发送、新建、停止、回答都要求读取；配置规范化
   拒绝违反该依赖的文档，界面开启写权限时同时开启读取，关闭读取时清除全部写权限。
2. **项目范围**为 `all` 或 `selected{projectIds}`。开启读取且范围为 `selected` 时至少一个项目。范围外的会话
   对客户端不可见，按 ID 访问与不存在同样报错。保存时只校验新加入的项目 ID，既有授权中已删除的项目保留，
   由用户自行移除。
3. **媒体授权**是精确的 `{connectionId, targetId}` 清单，只接受能力为 `image_generate`、`image_edit`、
   `video_generate`、`video_edit`、`video_extend` 的模型扩展目标（`MCP_MEDIA_CAPABILITIES`），排除自定义层
   目标与没有扩展配置的目标。每次调用都按实时账号与模型重新解析；已不可用的授权在界面上标为“已不可用”，
   不会自动删除。
4. **工具按授权暴露。** `tools/list` 只列出该客户端获授权的工具；未授权工具的调用返回 `isError` 结果。
   授权修改从客户端的下一次请求起生效。
5. **审批永远属于用户。** 权限审批不能经 MCP 放行或拒绝：等待审批的会话状态为 `waiting_for_approval`，
   `answer_question` 拒绝回答审批类问题。

## 工具

| 工具 | 授权 | 说明 |
| --- | --- | --- |
| `list_projects`、`list_sessions`、`get_session`、`read_messages`、`wait_for_reply`、`list_chat_models` | 读取 | 读取不改变任何状态 |
| `send_message` | 发送 | 发送后等待回复，最长 `wait_seconds` |
| `create_session` | 新建 | 在授权项目中新建会话并发送首条消息 |
| `stop_session` | 停止 | 停止当前运行 |
| `answer_question` | 回答提问 | 回答或拒绝会话提出的问题，不含审批 |
| `list_media_models`、`generate_media`、`get_media_job`、`cancel_media_job` | 媒体授权非空 | 付费模型；先读取模型参数 Schema |

工具描述、参数 Schema 与结果全部为英文；`initialize` 返回的使用说明同为英文。

## 会话与等待

1. **后台激活。** MCP 提交使用 `SubmissionActivation::Background`：不改变 WebUI 当前选中的会话、不移动
   输入区的模型与模式控件；会话摘要增量不携带 `selectedConversationId`，前端保持原选中。
2. **执行模式。** `plan_mode: true` 以计划模式提交；省略或 `false` 保持会话当前模式，不强制改为执行。
3. **等待。** `wait_seconds` 默认 `MCP_DEFAULT_WAIT_SECONDS`，上限 `MCP_MAX_WAIT_SECONDS`；等待按
   `MCP_STATUS_POLL_INTERVAL` 读取实时运行阶段、待回答问题与排队提示词。已保存、等待派发的用户回合也计入待处理输入。结果状态为 `running`、
   `waiting_for_answer`、`waiting_for_approval`、`idle`、`failed`、`stopped` 之一，并附 `cursor`；
   仍在运行时用 `wait_for_reply` 以 `after_seq` 带上 `cursor` 继续等待，不会重复读到已返回的内容。
4. **读取。** `read_messages` 合并已持久化与常驻内存的事件，按 `after_seq` 分页并返回 `cursor` 与 `has_more`，单次上限
   `MCP_MAX_READ_LIMIT`；工具输出只保留 `MCP_TOOL_OUTPUT_PREVIEW_CHARS` 字符预览，宿主专用事件不投影。
   常驻事件在读取闭包内过滤，状态轮询不复制整段会话。

## 媒体任务

1. 媒体任务由服务在后台执行，所有者 ID 为 `mcp-media:<job>`，与会话内的供应商任务分开；单客户端同时运行
   不超过 `MCP_MAX_RUNNING_MEDIA_JOBS_PER_CLIENT`，保留不超过 `MCP_MAX_RETAINED_MEDIA_JOBS_PER_CLIENT`，
   已结束的按最早优先淘汰。任务注册表只在内存中，服务重启后丢失。
2. `wait_seconds` 内完成直接返回结果，否则返回 `job_id`，再用 `get_media_job` 等待或 `cancel_media_job`
   取消。取消会请求供应商任务取消，不保证已计费的调用退款。
3. 输出写入用户 Vulcan 根目录下 `mcp-media/<client>/<job>/`，结果返回本地路径；不超过
   `MCP_INLINE_IMAGE_MAX_BYTES` 的图片同时内联返回。删除客户端会取消其未完成的任务。
4. 同一模型经多个账号授权时拒绝执行并提示用户只保留一个，不自行挑选付费账号。

## 设置与持久化

设置中的入口称为「对外 AI 服务」；添加 Vulcan 自己调用的 MCP 服务器仍在插件管理。

1. 配置位于全局配置 `mcpService`：开关、客户端清单（ID、名称、令牌摘要与提示、创建时间、开关、授权）。
   边界常量 `MAX_MCP_SERVICE_CLIENTS`、`MAX_MCP_CLIENT_NAME_CHARS`、`MAX_MCP_CLIENT_PROJECTS`、
   `MAX_MCP_CLIENT_MEDIA_TARGETS` 只在该文件定义，界面经快照 `limits` 读取。
2. 设置 RPC：`service_mcp_service_snapshot`、`service_set_mcp_service_enabled`、`service_create_mcp_client`、
   `service_update_mcp_client`、`service_rotate_mcp_client_token`、`service_delete_mcp_client`。新建请求不得携带
   `clientId`；更新不改变令牌。保存后服务丢弃已删除客户端的用量与媒体任务。
3. 最近使用时间与调用次数只在内存中统计，不写回配置；服务启动后显示“未使用”。
