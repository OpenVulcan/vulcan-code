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
| 文件读取 | `tooling/native/file_read.rs`、既有文件读取契约 | 路径、行范围、准确原文、续读说明 | 原有行映射与导航；没有正文副本 |
| 文件列表、属性 | `tooling/native/file_result.rs` | 条目类型、路径、大小、截断；实际编码和换行属性 | 无 |
| 文件写改删移、补丁 | 同上及 `file_write.rs`、`file_edit.rs`、`apply_patch.rs` | 实际变更路径、操作、增删统计、匹配数 | 变更集进入工具事件；可靠提取后从输出宿主字段移除副本 |
| 知识列表、读取、搜索、写入 | `tooling/native/knowledge_base/result_markdown.rs` | 库用途、逻辑目录、当前文档身份与修订、正文、引用行、来源、续读及部分失败；写入只返回变更回执与有界大纲 | 写入附 `knowledge_write_v1` 最小身份与计数；其余无 |
| VMM 搜索、详情、写删 | `tooling/native/vmm/result_markdown.rs` | 可操作身份、记忆、时间线独有信息、最终回答、逐项结果 | 无；领域存储不变 |
| 供应商文字、搜索、提取 | `provider_tool_bridge/result_markdown.rs` | 正文、排序、来源、图片证据、失败来源 | 无；不再复制完整归一化结果 |
| 媒体理解 | `media_understanding/result_markdown.rs` | 答案、独有片段、时间戳、警告；未知时间不写零 | 无 |
| 当前模型媒体读取 | `native_media_input/tool.rs` | `read_media` 接收单个本地路径或归属明确的资源定位符；图片、音频、视频、PDF 由当前会话模型理解；界面显示读取回执 | `$vulcan_native_media_input` 版本 1 保存原始内联内容或上传引用及其归属，供请求重放；正文不展示媒体字节 |
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
| 知识写入／`knowledge_write_v1` | `library_id`、`change_seq`、`documents[].id/path/revision`、`pending_embeddings` | `v1-tool-domain-presentation.ts` 展示最小变更回执；不复制文档正文或搬运字节，内容及历史边界见 [知识库契约](knowledge-base.md) |
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

- 原生媒体工具的暴露依据本回合冻结模型的输入能力、协议编码能力和 MIME／数量／来源约束。
  能够直接读取的模态自动隐藏对应 `image_understand`、`audio_understand`、`video_understand`，
  同时覆盖供应商工具和替代模型后端；只过滤回合内配置副本，不回写用户配置。不支持的模态保留外部理解。
  PDF 使用文件输入能力及 `application/pdf`，当前没有独立的 PDF 外部理解工具。
- `read_media` 的模型可见说明逐类列出图片、音频、视频、PDF 的本轮支持状态和精确 MIME；参数说明使用
  同一准入列表。不支持的模态明确禁止交给 `read_media`，并指向实际工具列表中可用的对应外部理解工具；
  外部工具不可用时明确报告能力缺失。图片支持不代表视频支持，误调用时返回相同边界及工具选择指引。
- `read_media` 界面使用单行资源读取回执：读取中、已读取或读取失败，加资源文件名；完整请求路径及失败原因
  通过悬停提示查看。卡片不展开正文，不显示媒体预览、转换技术说明或原始载荷；资源定位符的可读名称仅取自
  规范结果中的 `resource.file_name`，不从模型说明或供应商引用猜测名称。
- MIME 复用既有配置解析：模型专属约束优先于供应商协议路由约束；未声明独立限制时使用宿主默认格式表，
  最后按实际协议编码能力收敛。独立 MIME 白名单不与默认列表取并集；模型声明仍须通过既有供应商约束校验。
  工具说明、参数说明及执行校验共同消费本轮冻结后的结果，不在工具内部维护另一份供应商或模型格式表。
- 图片转换由 `vulcan-agent-runtime::image_conversion` 统一提供，覆盖 `read_media`、输入区粘贴／上传附件、
  替代模型图片理解及供应商工具图片理解。目标已接受原格式时保持原字节；目标拒绝原格式且允许 PNG 时才转换。
  工具说明区分原生格式与额外自动转换格式，并列出当前冻结模型／后端身份。默认原生图片仍为 PNG、JPEG、WebP、
  GIF、BMP；独立模型／供应商配置优先，新增解码格式不会自动扩展默认原生声明。
- 转换注册表支持 JPEG、WebP、GIF、BMP、TIFF、ICO、PBM、PGM、PPM、PNM／PAM、TGA、QOI；
  实际额外转换列表剔除当前目标已原生支持的格式。只转换静态图片，动画 GIF／WebP 和多页 TIFF 返回错误；
  ICO 使用解码器选定的最佳图标版本，JPEG／WebP／TIFF 应用 EXIF 方向。源文件、解码像素和 PNG 输出分别受
  `BUFFER_LIMIT_BYTES` 单一定义约束，随后按转换后真实大小再次执行既有交付策略。
  SVG、HEIC、AVIF 等未注册格式不承诺转换。音频、视频及 PDF 均不转换，不启用音视频转码程序。
- 粘贴／上传先保留原资产，附件规划在整批任何上传意图冻结前完成转换。PNG 使用新资产身份、MIME、大小及摘要；
  前端按原附件身份和规划代际安装新资产，协调草稿租约后重新规划。转换失败显示具体原因并阻止发送；
  已冻结交付意图的历史附件不被静默重写。工具转换失败返回调用错误，不发送半成品或额外调用理解模型。
- 输入区“自动压缩图片”为查看者本地选择，默认开启，由规划参数 `compressImages` 显式传递，服务不猜测。开启时
  新图片在同一规划阶段先转换不支持的格式，再按 `COMPRESSION_MAX_EDGE_PX` 缩小并重新编码（不透明转 JPEG、
  透明转 PNG）；只有缩小或体积明显下降时才替换原图，否则保持原字节。派生图沿用上述新资产安装与重新规划
  流程，规划期间图片块显示一圈冷却遮罩。视频、音频与 PDF 不压缩；上传重试不重新压缩。
- 草稿规划把超出单次请求编码预算的附件标为 `encoded_budget`（原因字段使用 `MediaDeliveryRejection::code()`
  的稳定代码）。用户提示词提交时若存在此类附件，先弹出“图片附加过多”确认，确认后才发送；工具调用不经过此确认。
- 会话压缩提交后，压缩前沿（`compaction_frontier_seq`）及之前的用户附件和 `read_media` 工具媒体一律只保留
  地址说明（`compacted_history`），不再重新内联或复用历史上传引用；前沿之后的媒体照常规划。
- 转换依赖 `image` 的显式格式特性及 `tiff` 页数检查，关闭默认特性，不启用 AVIF 原生解码或外部进程。
  当前锁定依赖许可为 MIT、BSD、Apache、Zlib 等宽松许可，无 GPL／AGPL；依赖升级时须重新核对实际特性与许可闭包。
- `read_media` 读取、按需转换、校验及按已有会话策略上传字节，不额外发起理解模型请求。V1 实际请求、预览与压缩
  共享无输入输出的结果投影，原生媒体放在连续工具结果批次之后，避免打断工具调用／结果拓扑。
- 工具媒体不挤占已经准备好的用户附件预算；使用剩余会话编码预算，并再次校验当前模型、MIME、硬限制及数量。
  上下文预算复用附件估算，不把 Base64 当作正文字符；仅转换记账快照，保留持久结果的完整内容。
  历史内联内容不会因偏好变化而上传；历史上传引用绑定原模型、账号、路由、会话、上传契约及原过期时间，
  不可用时只返回明确未发送内容的说明，绝不在重放时重新上传。
- 当前模型原生媒体读取与 MCP 图片结果是不同的封闭载体；上述行为不会改写第三方 MCP 工具或其外部协议。

- 新写入有明确宿主类型或 `format` 标记；旧 JSON 展示保留，历史文件不批量改写。
- 等待消费确认先识别新宿主类型，仅无该新宿主时读取旧模型 JSON。错误的新标记不得回退猜测确认。
- 正文转换只在工具执行后发生；流式请求注册、参数增量、工具预览、调用批次、权限与调度不变。
- 变更集必须成功进入事件后才能移除重复宿主对象。
- 用量成本比较采用规范结果实际 UTF-8 字节，不用字符数冒充 Token。
- 回归覆盖正文唯一、Unicode 和围栏、来源和分页、部分失败、真实零值与未知、
  Ask 持久化恢复、协作确认重启兼容、文件差异恢复、直接与延迟前端展示、MCP 机器模式。
- 不需要修改会话元数据 Schema；该 Schema 的升级规则仍以元数据迁移契约为准。

## 五、工具构造进度与事件连接

- 参数片段全部按序交给工具解析器，但不是每个片段都生成界面事件。`ToolRequestProgressPolicy`
  是计数与语义预览共同使用的发布节奏权威：普通更新受最小时间间隔约束；字节增长阈值不能绕过该间隔。
  达到最大间隔时，下一个收到的片段可发布当前完整快照。
- 工具身份、首次安全语义预览、参数根值封闭、就绪、失败与取消必须立即投影。同一普通片段最多生成一条
  包含计数和预览的事件。解析器始终消费全部片段，延迟采样只合并中间显示状态，不影响实际参数或最终行数。
- 节流发生在运行时投影分配工作台代际之前；不得丢弃已经编号的工作台事件，以免制造代际缺口。
- `file_write` 写入预览直接使用工具身份，不等待动作字段；即使正文先于 `path` 到达，也发布已解码正文行数。
  未知路径省略 `title_fields.path`；前端显示“写入”与“已生成 N 行”，不伪造文件名，不显示代表差异的 `+N`。路径到达后补入同一卡片，执行后的变更集才是实际增删行数的权威。
- 卡片按工具身份区分读取、写入、编辑、应用补丁；新建或修改的变更类型独立保留于差异数据。旧 `file` 会话按已记录动作继续展示。
- 桌面与网页各有独立事件连接及有界队列；共享会话增量仍广播。慢连接溢出不能影响其他连接，诊断必须包含客户端身份。
- 只有物理 WebSocket 的 `open` 表示推送已就绪。HTTP 探测与 RPC 成功不触发重连恢复；补拉必须在新连接打开后执行。
  会话正文补拉失败纳入工作台既有退避重试，成功前不能消耗恢复标记或宣告恢复完成。
- 验证入口：`tool_request_stream::tests::native_edit_preview_bursts_share_the_progress_rate_limit`、
  `vulcan-service-host` 的双客户端积压测试、`browserTransport-recovery.test.ts` 与选中会话 registry 测试。
