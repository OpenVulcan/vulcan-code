# AI 配置工具

本文是 AI 配置工具的契约：`mcp_config` 管理用户已配置的 MCP 服务器，`provider_config` 添加并切换模型供应商
连接。两个工具让 AI 按用户要求写入用户配置，例如“加一个 MCP 并启用”“加一个 DeepSeek 账号”。会话内临时连接
不属于本文，见 [`session-mcp.md`](session-mcp.md)。实现以下列源码为准，修改其中任一处时同步本文与对应测试：

- 工具定义、参数类型与宿主端口：`crates/vulcan-agent-runtime/src/tooling/native/configuration.rs`
  （`ConfigurationRequest`、`NativeConfigurationHost`）
- 权限分类与配置类别闸门：`crates/vulcan-agent-runtime/src/agent_runtime/tool_permission.rs`
  （`PermissionSubject::Configuration`、`admit_configuration_change`、`CONFIGURATION_UNATTENDED_DENIAL`），
  测试 `tool_permission/tests.rs` 中的 `configuration_changes_need_the_user`
- 计划模式策略：`crates/vulcan-agent-runtime/src/tooling/bridge/descriptors.rs`（`native_plan_policy`）
- 内核宿主、预览与规划：`crates/vulcan-service-kernel/src/ai_configuration.rs`（测试 `ai_configuration/tests.rs`），
  安装点 `service_bootstrap.rs`，预览入口 `tool_permission_host.rs` 的 `configuration_preview`
- 共享提交路径：`crates/vulcan-service-kernel/src/mcp_settings.rs`（`commit_mcp_configuration_change`）与
  `public_ai_settings/{connections,custom_providers}.rs` 的设置服务
- 审批线协议：`crates/vulcan-service-api/src/planning_state.rs`（`PendingApprovalSubject::Configuration`）
- 界面：WebUI `apps/vulcan-code-web/src/components/ask/tool-approval-bar.tsx`（`ConfigurationChangeBody`）与工具卡
  `conversation/tools/cards/document-card.tsx`；TUI `apps/vulcan-code-tui/src/ui/ask.rs`；文案键 `ask.approval.config.*`

## 红线

1. **只有一条写入路径。** 修改走设置页使用的同一提交函数与请求类型：MCP 经 `commit_mcp_configuration_change`
   与 `service_remove_mcp_server_configuration`，供应商经 `service_add_public_ai_connection`、
   `service_save_public_ai_custom_provider`、`service_save_public_ai_connection_enabled`、
   `service_save_public_ai_default_connection`，请求按设置 RPC 的 `deny_unknown_fields` 类型解析。不另建写入逻辑。
2. **密钥绝不经过模型。** 参数不接受密钥、令牌、字面请求头或环境变量值。API Key 只经 `api_key_env` 指名的
   Vulcan 环境变量（会话 > 项目 > 全局，名称不区分大小写）在执行时复制进凭据存储；之后修改变量不改变已存密钥。
   MCP 只接受 `env_passthrough` 与 `headers_from_env` 的变量名；已有的 Bearer 令牌、字面请求头、环境值与 OAuth
   在更新时保留，列表只显示其名称或是否存在。
3. **配置类别只由用户批准。** 完全访问直接执行；其他模式（含自动审批）一律跳过判定模型与复核 LLM 直接询问用户；
   无人值守会话直接拒绝（`CONFIGURATION_UNATTENDED_DENIAL`），因此无人值守不能修改配置。`list` 不受管控。
4. **先预览再执行。** 审批卡显示内核按实时配置计算的变更：对象种类、名称、动作、逐字段旧值与新值、后果提示。
   预览出错（参数无效、对象不存在、变量缺失、派生代理调用）时直接拒绝，不询问用户。
5. **启用即告知。** 启用 MCP 服务器或连接、停用、移除、因切换传输丢弃密钥、复制 API Key、联系供应商校验都以
   提示键出现在同一张审批卡中，不另设第二次确认。
6. **只属于用户会话。** 派生代理可以 `list`，修改在预览与执行两处都拒绝。计划模式只允许 `list`。

## 工具

1. **`mcp_config`。** `list` 返回已配置服务器及其实时状态与工具数；`save` 不带 `server_id` 时创建（`name`、
   `transport` 必填，标识由名称派生，重名拒绝），带 `server_id` 时更新，省略的字段保持原值；`remove` 移除。
   新服务器缺省为停用，只有显式 `enabled: true` 才启用。在 stdio 与 http/sse 之间切换时清空另一族字段并提示
   `drops_secret_fields`。已启用服务器的工具从下一轮起进入会话（工具清单按执行快照）。
2. **`provider_config`。** `list` 返回接受 API Key 的供应商（含服务入口与凭据档）和全部连接（启用、待验证、默认）；
   `add_connection` 为内置供应商添加 API Key 连接，入口或凭据档只有一个时可省略；`add_custom_provider` 添加兼容
   OpenAI、Anthropic 或 Gemini 的供应商及首个连接，认证缺省为 Bearer；`model_discovery_protocol`
   独立选择唯一目录格式（`open_ai`、`anthropic`、`gemini`，默认 `open_ai`），目录使用共享地址与认证，
   不随 `protocols` 逐个拉取或失败切换；`set_connection_enabled` 与
   `set_default_connection` 切换已有连接。浏览器登录类账号、核心账号与删除只在设置页处理。
3. **执行时重新规划。** 执行针对提交事务持有的配置重新规划同一请求；预览与执行之间配置被他处修改时以执行时为准，
   不做额外防护。

## 预览键

审批载荷中的种类、动作、字段与提示都是稳定键，客户端按目录渲染，未知键原样显示：

- 种类：`mcp_server`、`provider_connection`、`custom_provider`、`provider`
- 动作：`create`、`update`、`remove`、`enable`、`disable`、`set_default`
- 字段：`name`、`enabled`、`transport`、`command`、`args`、`cwd`、`env_passthrough`、`url`、`headers_from_env`、
  `provider`、`service_entry`、`credential_profile`、`api_key_env`、`base_url`、`protocols`、`model_discovery_protocol`、`authentication`、
  `default_connection`
- 提示：`activates_for_all_sessions`、`deactivates_for_all_sessions`、`removes_for_all_sessions`、
  `drops_secret_fields`、`copies_api_key`、`validates_with_provider`

新增键时同时补齐内核、WebUI 映射、TUI 映射与 11 个语言目录。
