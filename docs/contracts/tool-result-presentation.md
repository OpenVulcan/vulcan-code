# 会话工具结果与宿主载荷

## 适用范围与权威

本文记录会话 AI 阅读投影，源码和类型仍是权威。修改下列生产者、前端消费者、MCP 版本或公共结果封套时，
必须同步检查本文和对应测试。机器解析入口、工具输入、供应商协议、内部向量计算与状态机不适用本次格式转换。

- 公共边界：`vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs`。
- 纯文本使用 `VulcanToolInvocationResult.model_output = Value::String`；规范结果直接保存字符串，
  不再对整篇 Markdown 进行第二次 JSON 编码。
- 结构化原始数据仍可使用对象；公共边界没有通用 JSON 转 Markdown 或猜测字段删除逻辑。
- 本文所有 crate 相对路径均相对仓库 `crates/`。

## 一、会话阅读投影

| 工具范围 | 生产入口 | 模型和界面共用内容 | 宿主结果 |
| --- | --- | --- | --- |
| 文件读取 | `tooling/native/file.rs`、既有文件读取契约 | 路径、行范围、准确原文、续读说明 | 原有行映射与导航；没有正文副本 |
| 文件列表、属性 | `tooling/native/file_result.rs` | 条目类型、路径、大小、截断；实际编码和换行属性 | 无 |
| 文件写改删移、补丁 | 同上及 `file_edit.rs`、`apply_patch.rs` | 实际变更路径、操作、增删统计、匹配数 | 变更集进入工具事件；可靠提取后从输出宿主字段移除副本 |
| 代码检索 | `vulcan-service-kernel/src/vce/services/tool_result.rs` | 来源、行范围、每个片段一次、真实降级信息 | 无；内部 VCE 响应不变 |
| 知识搜索、浏览、读取 | `tooling/native/knowledge_base/result_markdown.rs` | 查询、文档与片段身份、正文、来源、续读游标、部分失败 | 无 |
| VMM 搜索、详情、写删 | `tooling/native/vmm/result_markdown.rs` | 可操作身份、记忆、时间线独有信息、最终回答、逐项结果 | 无；领域存储不变 |
| 供应商文字、搜索、提取 | `provider_tool_bridge/result_markdown.rs` | 正文、排序、来源、图片证据、失败来源 | 无；不再复制完整归一化结果 |
| 媒体理解 | `media_understanding/result_markdown.rs` | 答案、独有片段、时间戳、警告；未知时间不写零 | 无 |
| 浏览器管理 | `tooling/native/browser.rs` | 浏览器和标签身份、标题、地址、选择结果 | 无；真实控制对象不变 |
| 目标管理 | `tooling/native/goal.rs` | 目标、状态、有效预算、已知用量；空目标明确说明 | 无；进度仍由会话目标快照提供 |
| 计划提交 | `tooling/native/submit_plan.rs` | 已接受的名称、文件位置、简短回执 | `plan_artifact` 保留已接受计划正文，模型不再收到全文副本 |
| Lua 会话 | `luaskills_runtime/runlua/result_markdown.rs` | 启停和状态、会话身份、执行输出；业务 JSON 原文围栏 | 无新增正文副本 |
| 技能配置 | `tooling/native/runtime_config/result_markdown.rs` | 正式类型中的声明、约束、缺项、无效项、交叉校验 | 无；强制不披露已配置值；不改变读写能力 |
| 模型参数查询 | `provider_tool_bridge/result_markdown/extension_parameters.rs` | 精确模型选择器、输入模式、参数限制、完整参数 Schema | 无；Schema 是下一次调用所需数据，原样放入围栏 |
| 子代理六个工具 | `agent_runtime/task_runtime/tool_surface/result_markdown.rs` | 调度与终态区分、目标、控制结果、列表、可用选择器 | 见下表；内部 TaskRuntime JSON 不变 |
| 跨会话发送、问题、通知、列表 | `tooling/collaboration/result_markdown/receipts.rs` | 身份、投递状态、回复义务、部分失败、必要已知答案 | `collaboration_receipt_v1` 仅阅读标记和实际返回的问题身份 |
| 会话读取 | `tooling/collaboration/result_markdown/session_read.rs` | 已授权事件页的用户、助手、推理、工具结果、协作及回合语义 | 阅读标记；不展开 API 重放、系统上下文和内部快照 |
| 统一等待 | `tooling/collaboration/result_markdown.rs` | 本次观测、剩余工作、未核实目标、待回答问题、检查失败 | `agent_wait_ack_v1` 保存消费确认需要的身份与状态 |
| 用户提问 Ask | `agent_runtime/question_runtime.rs` | 原有回答 Markdown，一份正文 | `ask.payload.format = markdown`；题目来自原调用，待回答状态仍由 QuestionRuntime 管理 |
| 公共错误、后置失败 | `tool_runtime_bridge/result_markdown.rs`、`vulcan_plugin_hooks.rs` | 错误和部分副作用；后置失败追加明确禁止自动重做的警告 | 权威错误类型和业务恢复字段；移除原始参数副本 |

### 不得误删的语义

- 未开始、接纳、投递、任务完成是不同状态。无运行所有者、人工停止、未知、检查失败不能变成成功。
- 等待回执只列消息身份，正文继续由既有动态上下文准确投递，不再从回执重复复制。
- `read_session` 的续读位置是本页最后扫描的事件；全局排他边界不是可直接替代的续读游标。
- 按不可变来源身份去除会话事件正文副本，不能按文本相同判断两条消息是同一条。
- 文件操作已经生效而结果投影失败时，明确告知已修改，不诱导模型重复写入。
- 外部材料和代码保留原文；安全围栏长度根据原文反引号计算，链接使用转义。

## 二、最小宿主字段及消费者

| 生产者／宿主类型 | 保留字段 | 实际消费者与原因 |
| --- | --- | --- |
| 文件修改／`change_set` | 过渡期间完整已应用变更集 | `agent_stream/provider_v1_sink.rs` 严格转换到事件 `changeset`；成功后移除输出副本；前端 `file-change-presentation.ts` 直接消费事件 |
| 文件读取映射 | 原文件路径、源行范围及正文对应行号 | `file-access-card.tsx`，定位和行号，不存正文 |
| Ask／`ask` | `format` | `ask-question-presentation.ts`、`v1-tool-domain-presentation.ts` 明确区分新正文和旧结构；直接读取脱敏及后置处理后的正文，不反向解析选择状态 |
| 计划／`plan_artifact` | 名称、路径、已接受正文、更新时间 | V1 计划展示与计划操作；不能使用未接受输入替代文件写入结果 |
| 代理创建／`subagent_receipt_v1` | `task_id`、`subagent_task_id`、`display_title`、`execution_id`、`execution_status`、`format` | 前端导航、执行状态关联；监听器仍消费独立 `DelegateStarted` 事件，未改变事件契约 |
| 代理发送／同上 | `target`、`subagent_task_id`、`queued`、`delivery_mode`、`format` | 目标跳转、投递提示；不保存发送正文副本 |
| 代理列表、编排信息／同上 | `agent_count`、`format` | 卡片标题；其余可读信息只在 Markdown |
| 关闭代理／`subagent_control` | 工具名、精确匹配身份、停止身份与数量、已关闭名称与数量、关闭标志、清队列数量、阅读标记 | 既有关闭面板解析器及前端状态；不能因去 JSON 丢失关闭通知 |
| 等待／`agent_wait_ack_v1` | `reports[].work_id/outcome/reason`、`status`、`interrupted`、`context_messages_pending`、`format` | 内核 `result_wait::persisted_report_identities` 确认消费，前端状态；没有消息正文 |
| 跨会话／`collaboration_receipt_v1` | `format`、返回时的 `question_id` | 问题卡关联现有领域状态，正文延迟加载 |
| 待办／`todolist` | 已接受条目和解释 | `selectConversationSessionTodolistV1`、待办控件；模型只收到短回执 |
| 媒体资源／`generated_attachments` | 资产描述、实际本地路径、媒体属性 | `conversation-media-generation-result.tsx` 按资产身份匹配、播放；模型只得到可操作引用、名称、类型和大小 |
| 命令／`command_run`、`command_session` | 实际命令、工作目录、Shell、身份、退出状态、字节计数、失败与溢出信息 | 命令卡和实际执行提示；未保存完整命令输出副本 |
| 终端／`terminal_session` | 控制身份、输入能力、帧身份和尺寸、屏幕快照、流事件、退出与分页状态 | `terminalSummary`、`terminalSessionOutputFromEvents` 和 TUI 实时恢复；屏幕状态与 stdout/stderr 流并非等价正文，保留这些实际控制消费者需要的数据，不做猜测合并 |

终端仍使用既有文本输出与控制协议。屏幕快照、原始流事件可能与模型所读文字重叠，但它们是带帧和流身份的控制数据，
本次没有把它们改成 Markdown 再反向解析。此保留不允许普通阅读工具照搬完整结果作为宿主载荷。

## 三、保留结构与外部边界

1. 嵌入向量、重排结果是任务数据，保留值与顺序；去掉会话层 `upstream_response_id` 和完整宿主克隆。
   复合数据结果省略空成员，带媒体资产时仍保留前端所需资产描述。
2. MCP 新增明确的 `McpToolResultAudience`：
   - 原 `call_tool`、`invoke`、`invoke_for_owner`、内部 Lua 调用继续走机器模式。
   - 只有已注册会话候选调用选择会话模式。请求、所有者校验、撤销检查、串行租约和上游调用次数不变。
   - 会话模式保留全部文本块与顺序；纯文本即使合法 JSON 也仍是文本。
   - 只在单个文本块解码后与结构值精确相等时去除兼容副本。其他未知组合保留独有结构。
   - 原生图片保留类型、字节及顺序，不复制为 Markdown 数据链接；资源链接和文本资源保留真实 URI。
   - 当前宿主原生工具结果载体支持图片，不支持 MCP 音频和二进制嵌入资源。遇到这些格式明确返回
     “上游已执行、结果类型不支持、不要自动重做”，不能静默吞掉。不在本次擅自扩展媒体协议。
   - 自定义传输和未知第三方对象没有可核对字段契约时保持原始返回，不宣称完成逐字段适配。
3. `skill`、`luaskill`、`runlua` 的既有阅读文本保持原样；
   `call_luaskill` 的外部技能宿主载荷仍由技能声明拥有，宿主不猜测删除字段。
4. 插件后置 Hook 的输入和输出属于程序契约；不全局改变其形状。
   后置失败仅改变会话回执表达，保留已完成结果和受支持多模态内容，不重新执行原工具。

## 四、历史、可靠性与回归

- 新写入有明确宿主类型或 `format` 标记；旧 JSON 展示保留，历史文件不批量改写。
- 等待消费确认先识别新宿主类型，仅无该新宿主时读取旧模型 JSON。错误的新标记不得回退猜测确认。
- 正文转换只在工具执行后发生；流式请求注册、参数增量、工具预览、调用批次、权限与调度不变。
- 变更集必须成功进入事件后才能移除重复宿主对象。
- 用量成本比较采用规范结果实际 UTF-8 字节，不用字符数冒充 Token。
- 回归覆盖正文唯一、Unicode 和围栏、来源和分页、部分失败、真实零值与未知、
  Ask 持久化恢复、协作确认重启兼容、文件差异恢复、直接与延迟前端展示、MCP 机器模式。
- 不需要修改会话元数据 Schema；该 Schema 的升级规则仍以元数据迁移契约为准。
