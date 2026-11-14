# 环境变量管理

本文是 Vulcan Code 用户自定义环境变量的契约：用户在插件区的“环境变量”页或 Ask 面板中填写变量，
Vulcan 把它们注入代理启动的命令、终端与 MCP 服务；AI 通过 `env` 工具只能看到用户定义的变量（不含系统
环境），并可以请用户填写值。实现以下列源码为准，修改其中任一处时同步本文与对应测试：

- 存储、编码、边界常量与优先级合并：`crates/vulcan-workspace-store/src/workspace_store/environment_variables.rs`
  （SQLite 表 `environment_variables`、`environment_variable_project_bindings`；会话侧车 `environment.json`）
- AI 工具、注入与脱敏：`crates/vulcan-agent-runtime/src/tooling/native/environment.rs`（工具 `env`）、
  计划模式策略 `tooling/bridge/descriptors.rs`（`native_plan_policy`）
- 取值请求挂起与恢复：`crates/vulcan-agent-runtime/src/agent_runtime/question_runtime/environment.rs`
- MCP 全局覆盖层：`crates/vulcan-agent-runtime/src/mcp/environment_overlay.rs`、`mcp/config.rs`
- 终端快照与指纹：`crates/vulcan-host-tools/src/command_session_runtime.rs`、
  `crates/vulcan-agent-runtime/src/tooling/native/terminal/`
- 线协议类型：`crates/vulcan-service-api/src/environment_variables.rs`、`planning_state.rs`
  （`PendingEnvironmentRequestSnapshot`）
- 内核宿主与 RPC：`crates/vulcan-service-kernel/src/environment_variables.rs`、`rpc_dispatch/environment.rs`
- 界面：`apps/vulcan-code-web/src/components/features/environment/`、
  `apps/vulcan-code-web/src/components/ask/environment-values-request.tsx`；TUI 只显示请求并可拒绝
  （`apps/vulcan-code-tui/src/ui/ask.rs`）

## 作用域与存储

1. **三层加系统。** 优先级为会话 > 项目 > 全局 > 进程（系统）环境；同名比较不区分大小写（按 ASCII 大写键）。
   全局变量适用于所有项目，项目变量绑定一个或多个项目；两者存放在应用 SQLite 数据库，不写配置文件。
   删除项目时级联移除其绑定，并在同一事务中删除因此不再绑定任何项目的项目变量（绝不转为全局）；
   列表读取前执行同一清理，以修复此前遗留的无绑定行。存储值一旦为密钥，以同名写入普通值也保持密钥。
2. **会话变量只属于会话。** 会话层只写入该会话目录下的侧车 `environment.json`，不进入数据库，也不出现在
   全局列表中；子代理读写其根会话的侧车。
3. **只做简单遮蔽。** 存储值为 `v1:` 加 base64（固定掩码异或），目的只是避免明文直接暴露，不是加密。
   不得把它描述或升级为强加密承诺；格式变化按“其他持久格式”规则递增前缀版本并拒绝旧格式。
4. **边界值唯一来源。** 名称、值、说明的上限、推荐前缀与保留名都定义在存储模块的常量中；界面通过
   `EnvironmentVariableRules` 读取，不得另写字面量。
5. **保留名禁止存储。** `RESERVED_ENVIRONMENT_VARIABLE_NAMES` 列出 Vulcan 自身设置、不允许覆盖的变量
   （当前只有 `VULCAN_HOME`）。新增 Vulcan 核心变量时，按实际是否会被覆盖决定是否加入该清单，
   而不是一律禁止 `VULCAN_` 前缀。推荐前缀 `VULCAN_ENV_` 只是提示，不强制。
6. **系统已定义只提示。** 名称已存在于服务进程环境时，界面提示“Vulcan 启动的进程将改用此值，系统不受
   影响”，保存照常进行；Vulcan 从不修改系统或用户级环境变量。

## 注入范围

1. **命令与终端。** `shell_command` 与 `terminal_session` 启动的子进程在继承的进程环境之上叠加有效变量。
   终端在 `start` 时固定环境并记录指纹（变量名与值的 SHA-256；无变量时为空串）；之后变量变化不影响已启动
   的终端，`snapshot` 会提示“环境已变化，重启终端生效”。项目终端面板（用户手动打开的终端）不注入。
2. **MCP 服务只见全局层。** MCP 客户端配置里的直通变量与请求头取值先查全局覆盖层再查进程环境；
   `inherit_environment` 的服务器另外继承全局变量（显式配置的值优先）。覆盖层在服务启动与每次全局变量增删改后
   重新发布，已运行的 MCP 服务器保持原环境直到重启。项目与会话变量不进入已配置的 MCP 服务器。
3. **会话临时 MCP 见全部会话层。** `mcp_connect` 启动的 stdio 服务器与命令一样叠加本会话生效的全部变量；
   http/sse 连接的 `bearer_token_env` 与 `headers_from_env` 先查本会话生效变量再查进程环境。
   详见 [会话临时 MCP](session-mcp.md)。
4. **启动发布失败只告警。** 启动时发布全局覆盖层失败只写警告日志，不阻止服务启动。
5. **API Key 只复制一次。** `provider_config` 的 `api_key_env` 在执行时按本会话生效变量（会话 > 项目 > 全局）
   读取值并复制进凭据存储，值不进入工具结果；之后修改或删除变量不影响已存密钥。详见
   [AI 配置工具](ai-configuration-tools.md)。

## AI 工具 `env`

1. **动作。** `list` 列出用户定义的变量（名称、类型、说明、层级；普通值可见，密钥值永不返回），不列系统
   环境；`set_session` 写入当前会话的普通变量，不能覆盖用户的密钥变量；`request` 请用户在 Ask 面板填写一个
   或多个变量。
2. **模式与会话限制。** 计划模式只允许 `list`；子代理可用 `list` 与 `set_session`，不能 `request`；无人值守
   会话拒绝 `request`。
3. **脱敏。** 每次解析有效变量时登记其中的密钥值。读取历史 shell 任务时，WebUI 和模型侧工具登记当前
   仍保存在会话、所属项目与全局层的适用密钥，包括被同名覆盖的值。项目归属以持久化的会话定位器为准，
   解析归属时无需恢复会话运行态。任务日志先在连续的原始字节上匹配密钥（包括含换行的值），再按原始行号
   展示。跨行匹配的可见片段逐行遮蔽，`shell_job search` 匹配遮蔽后的文本。尾部读取和长行裁剪会保留匹配所需的
   相邻上下文。滚动保留丢弃了原始上下文时，缺口两侧可能横跨缺失字节的范围会被遮蔽。脱敏服务不可用时，历史文本读取返回错误。
   工具结果中长度不少于 `ENVIRONMENT_SECRET_REDACTION_MIN_CHARS` 的密钥值替换为 `[redacted:NAME]`。
   已轮换或删除的旧值可能留在历史日志中，重启后只能从当前存储的变量补齐脱敏登记。

## 取值请求

1. **值从不经过问题运行时。** `request` 通过 `QuestionRuntime` 挂起为带 `environment` 的待处理请求；WebUI
   调用 `service_submit_environment_values` 直接存储值，再以固定标签 `Saved` 或 `Declined` 恢复工具调用。
   `service_reply_pending_question` 拒绝环境请求；钩子跳过非模型提问。
2. **拒绝不是中断。** “拒绝”回复 `Declined`，工具调用照常返回；“停止会话”才是取消请求并中断会话。
3. **Ask 面板。** 一次请求可包含多个变量；每个输入框默认遮蔽，各自带眼睛开关；每个变量可选保存到本会话、
   本项目（会话无项目时不可选，建议值回落为本会话）或所有项目；只提交已填写的变量；Enter 不保存。
4. **只有用户能回答。** TUI 只显示名称并可拒绝（值在 WebUI 中输入）；对外 MCP 服务把环境请求按用户专属
   请求处理，MCP 客户端不能回答。

## 管理界面

1. **位置。** 插件区侧栏“环境变量”行（与 MCP 服务器、知识库并列），由 `SkillsArea` 渲染。
2. **密钥值永不显示。** 服务对密钥只返回空值；编辑时值输入框为空，留空即保留已存储的值。
3. **会话变量区。** 选择一个会话查看其自身变量并可删除；新增会话变量只经 Ask 面板或 AI 的 `set_session`。
