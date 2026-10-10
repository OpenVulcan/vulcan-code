# 专职会话（专职助手）

## 权威与同步触发点

本契约归属于 `vulcan-session-core::v1::dedicated`（配置形状、区间不变量与 Provider 事件筛选）和
`vulcan-session-core::v1::document::dedicated`（安装、记忆锁定与冻结设置守卫）。修改专职会话的持久字段、
记忆规则、请求组成或界面语义时，必须同步本文、行为测试与 WebUI/TUI 渲染。持久字段变化同时遵守
[会话元数据版本](session-metadata-migrations.md)。

## 会话种类

- 专职会话是只能在新建时选择的独立会话种类，`MetadataDocument.dedicated` 为 `Some` 即为专职会话，
  普通会话为 `None`。两者之间不可相互转换。
- 专职会话只能由草稿首次提交创建：`sessionSettings.dedicated`（`toolAccess`、`basePrompt`、
  `lockImmediately`）随首轮发送；已存在的会话收到该字段时返回 `ERR_DEDICATED_SESSION_NOT_CONVERTIBLE`，
  同时设置 Goal 或 Plan 模式返回 `ERR_DEDICATED_SESSION_MODE_UNSUPPORTED`。
- 安装只能发生在刚创建、尚无任何事件的会话上，并先于根输入写入（因此立即锁定的锁定标记位于首条用户
  消息之前）。随后首轮的会话设置（回答模式、输出风格、知识库、VMM 关闭）照常写入。
- **冻结点**：首轮已开放、全部事件仍驻留且尚未出现 `ApiStarted` 的“创建窗口”内允许写入上述设置；窗口
  关闭后，以下设置的真实修改一律返回 `DedicatedSettingFrozen`：回答模式、输出风格、选择配置包、知识库
  选择、VMM 策略、Goal、Plan 模式，以及编辑重生成与分叉。
- 基础提示词由用户在创建时提供（可借助当前模型生成或优化），创建后永不修改；上限只在
  `DEDICATED_BASE_PROMPT_MAX_CHARS` 定义。行为调整通过解锁后的补充对话完成，而不是改写基础提示词。
- 基础提示词可通过 `service_draft_dedicated_prompt` 起草或优化：使用输入区所选的精确供应商、连接与模型
  （不改道），系统提示词为 `agent-prompts/dedicated-prompt-generator.md`；输入与输出都受
  `DEDICATED_BASE_PROMPT_MAX_CHARS` 约束，结果只返回给客户端审阅，不写入任何会话。
- `tool_access`：`none` 不开放通用工具（仅保留创建时启用的知识库工具），也不注入提示词技能、工作区
  信息与权限模式通知；`standard` 开放通用工具但排除子代理、协作、Goal、Plan、VMM 记忆与语意检索工具。
  两种方式都不使用子代理、上下文压缩（自动压缩跳过，手动压缩报错）、VMM 记忆与语意引擎。
- 请求组成：基础提示词作为代理基础提示词发送，不附加能力指引；`none` 使用提示词组装模式
  `dedicated_minimal`（代理基础、输出风格、工具说明），`standard` 使用 `dedicated_tooling`（另含供应商基础、
  项目规则与环境上下文）。回答模式提示照常提供。

## 记忆规则

- `locked_spans` 是有序、互不重叠的锁定区间，只有最后一个可以仍然开放（`unlocked_at_seq: null`）。
  每次锁定或解锁在同一修订中追加一条会话级 `DisplayNote`（`memory_locked` / `memory_unlocked`，
  `UiOnly`）并更新区间；区间端点就是这两条通知的 seq。
- 锁定与解锁只能在回合之间进行，开放回合期间返回 `DedicatedMemoryLockWhileTurnOpen`
  （服务错误码 `ERR_DEDICATED_MEMORY_LOCK_TURN_OPEN`）。服务入口为 `service_set_dedicated_memory_lock`，
  重复当前状态为幂等空操作；响应携带更新后的配置。客户端从事件窗口的 `session_config.dedicated` 读取配置。
- 起点位于锁定区间内、且已不是当前开放回合的回合称为“不记忆回合”。`dedicated_provider_events` 按整回合
  移除这些回合的事件（保持 API 组结构完整），但始终保留 `SystemContext` 语义，因为它们是会话级指令，
  幂等同步不会重新发出。当前开放回合始终完整保留，因此一轮之内可以调用工具和插入补充输入。
- 解锁后从解锁点重新累积记忆；锁定期间的不记忆回合不会被带回。再次锁定产生新区间，之后每轮以
  “全部已记忆内容”为基准。

## 界面语义

- 不记忆回合照常显示并标注“不记忆”；锁定、解锁通知显示为时间线标记。
- 基础提示词在时间线开头以用户消息样式显示，并标注其实际作为系统提示词发送。
