# 工具结果正文统一与模型载荷精简方案

## 方案状态与目标

- 日期：2026-09-26；当日序号：03。
- 状态：实现、三轮审核及最终定向复验完成；扩大回归中的一项基线失败单独记录，方案已归档。
- 目标：按实际消费者设计输出，降低模型阅读成本、上下文占用及持久化重复成本，保持前端展示和后续工具调用能力。
- 范围：会话中注册并暴露给 AI 调用、最终结果供 AI 阅读和决定下一步的工具。包括符合此条件的原生工具、代理协作工具、供应商工具、媒体扩展、LuaSkills、MCP、插件，以及这些会话工具的错误和后置处理结果。
- 明确排除：本身依赖 JSON 解析的特殊调用、机器到机器接口、结构化输出任务、供应商协议响应、内部模型辅助调用、业务状态交换、工具输入 Schema 和协议封装。这些调用不列入格式转换或载荷精简任务，不能为了统一输出而修改其契约。
- 供应商或插件内部返回 JSON 不等于存在待改的会话工具输出。只审查其向会话 AI 暴露的最终结果投影；专门供程序解析的结果不纳入本方案。
- 本文分为两部分：第一部分处理需要转成 Markdown 的结果及前端配套；第二部分处理保留结构、已经是文本或暂不能转换的结果精简。
- 以当前源码、注册目录和实际调用链为准。动态工具是否启用受运行时与权限控制，前端存在工具名称分支不等于该工具当前已经注册。

## 共同设计约束

1. **先减内容，再改格式。** 不将完整 JSON 机械展开成同样冗长的 Markdown。每个字段须说明模型为何需要、前端如何使用、运行时是否另有权威来源。
2. **正文只设一个权威来源。** 普通工具的最终 Markdown 同时供模型和前端阅读；不在宿主结果中保存另一份正文、逐段正文或重新包装的完整模型结果。
3. **宿主结果只保留有实际消费者的展示或交互元数据。** 工具事件已有名称、输入、状态、时间、调用标识、变更集时直接引用。纯 Markdown 展示不新增宿主结果；需要独立渲染时列明消费字段。
4. **业务状态不依赖 Markdown 反向解析。** 执行结果、权限、投递、重试、进程存活等仍由现有强类型业务状态和事件负责；精简的是输出投影，不删除业务权威数据。
5. **模型必需的稳定标识不能为了缩短而删除。** 后续读取、回复、等待、分页、引用资源所必需的标识及游标保留。内部追踪标识只有在模型能够用它执行下一步时才进入模型输出。
6. **模型与用户文案分层。** 实际生成的模型说明、标题、错误及操作提示使用英文；原始文件、用户回答、检索文献保持原文。前端卡片外壳和状态标签可本地化，正文直接复用，不持久化第二份翻译。
7. **现有专用能力不得丢失。** 行号、文件跳转、差异审阅、终端控制、提问交互、媒体播放和会话跳转需要真实结构；不能为了去 JSON 退化成不可操作的纯文字。
8. **不扩大历史迁移范围。** 新写入使用新投影；历史记录按明确的既有类型或版本读取，不批量重写历史会话。若实际涉及持久协议或元数据版本变化，先按对应迁移契约设计和确认。

# 第一部分：改为 Markdown 的工具及前端配套

## 一、按结果类型选择输出方式

### 1. 纯正文共用方式

适用条件：前端只需要阅读、折叠、复制正文，以及普通 Markdown 链接。

- 后端生成一次最终 Markdown，写入既有模型结果位置。
- 前端读取同一正文，复用现有会话 Markdown 渲染链路。
- 不生成仅为了拆分标题、正文和列表的宿主 JSON。
- 卡片标题、时间、执行状态取自现有事件；无需再放入正文或宿主载荷重复保存。
- 原始正文若本身包含 JSON、代码或日志，作为原文代码块保留，不擅自改写内容。

### 2. Markdown 加最小展示元数据方式

适用条件：文件行号、导航、差异、交互控件或资源播放确实要求独立结构。

- 模型只接收完成任务及下一步操作需要的 Markdown。
- 前端通过现有事件、领域对象或最小宿主载荷完成专用展示。
- 前端需要正文片段时，优先引用唯一正文或既有内容对象；确需范围映射时，映射由正文产出方生成并校验，不让前端猜测 Markdown 结构。
- 不复制完整领域对象；不建立仅供展示的第二套执行状态。
- 宿主最终字段白名单必须附“产出方法—消费方法—字段用途”证据，不能按猜测创建可选字段和备用路径。

## 二、工具改造矩阵

下表的宿主栏目为目标设计；现存 JSON 消费需要随对应工具一起调整。

| 工具或操作 | AI 真正需要的内容 | 前端实际需要的内容 | 宿主处理与删减要求 |
| --- | --- | --- | --- |
| `file` 的 `read` | 路径、实际范围、原文、截断与续读位置 | 代码正文、源行号、文件导航 | 已有 Markdown 契约作为样板；继续保留最小行映射，不恢复正文副本 |
| `file` 的 `list`、`stat` | 相对路径、条目类型、必要属性、分页或截断信息 | 列表或属性展示，必要的路径跳转 | 默认共用 Markdown；若现有文件卡保留导航，只留准确资源引用；删除完整结果克隆 |
| `codebase_retrieval` | 路径、行范围、相关片段、真实降级情况及可用的后续定位信息 | 相同检索正文与可点击来源 | 在命中集合上生成一次 Markdown，不同时发送命中正文和重复的格式化上下文；评分与内部查询计划无实际用途则不发送 |
| `knowledge_search`、`knowledge_browse`、`knowledge_read` | 标题、来源、必要文档或片段标识、范围、相关正文、续读游标、部分失败 | 来源链接与正文；读取正文当前已经采用 Markdown 展示 | 默认无正文宿主结构；需要专用导航时保留最小定位关系；保留外部内容的不可信边界说明 |
| `vmm_memory_search`、`vmm_turn_details` | 可操作的记忆或回合标识、相关内容、必要来源与前后回合导航 | 同一内容的列表和时间线展示 | 先确认时间线、详情和最终回答的内容关系，按语义去重；不得未核对就删掉独有内容；内部追踪信息不发送 |
| `vmm_memory_write`、`vmm_memory_delete` | 成功、未找到或失败项及必要标识 | 简短操作结果 | 共用短 Markdown；不回显整批写入正文，不列空错误数组或无意义零值 |
| 供应商文本、搜索、网页提取 | 正文、标题、引用地址、必要来源日期、失败与截断信息 | 正文与可点击引用；当前专用搜索卡消费排序、标题、地址、摘要等 | 优先改为共用 Markdown；只有保留专用交互才保留最小引用元数据，不复制摘要与提取正文；站点图标等仅展示信息不发给 AI |
| `image_understand`、`video_understand`、`audio_understand` | 理解结论、相关片段、必要时间戳、实际警告 | 相同报告；若有媒体定位，则需要时间位置与资源关系 | 普通报告共用 Markdown；定位元数据单独保留；不重复保存整份回答和空分段数组 |
| `spawn_agent`、`agent_orchestration_info`、`list_agents` | 必需的代理标识、职责、真实运行状态、适用控制能力 | 名称、状态、会话跳转及展开正文 | 模型移除调试快照和无关运行时结构；宿主引用既有代理身份与状态，不复制完整注册表 |
| `send_message`、`interrupt_agent`、`close_agent` | 目标、执行或投递结果、后续所需标识、实际异常 | 目标跳转和当前结果 | 短 Markdown 回执；不重复发送已知消息全文，不把接纳当成完成 |
| `ask_question`、`ask_parent`、`reply_question`、`cancel_question` | 问题身份、明确状态、答案或必要回复指引 | 原问题、答案、参与者导航、权威状态 | 正文与卡片状态分离；不新增撤销按钮或技术详情，不将完整投递记录发送给 AI；交互数据复用领域权威 |
| `wait_agents_result` | 本次真正到达的信息、关联标识、终态、剩余工作、待回复问题、无法核实或检查失败信息 | 相应协作卡片的正文与状态 | 先核对等待回执与动态上下文的投影时机，确保内容恰好交付一次；保留完成、停止、未知的区别，不复制已投影的完整消息 |
| `list_sessions`、`read_session` | 会话名称与可操作标识、授权范围内的消息、状态、分页位置 | 会话导航和按角色排列的正文 | 只输出所请求范围；内部事件记录转换为必要语义，不展开完整状态快照，不扩大授权范围 |
| `send_session_message`、`send_notification` | 目标、接纳或记录结果、必要投递标识与异常 | 既有卡片附加状态与目标导航 | 删除完整记录回传及内外重复身份字段；投递状态不独立生成空卡 |
| `file` 的写入、删除、移动、`file_edit`、`apply_patch` | 实际影响的路径、操作、变更摘要、必要匹配数量、失败及是否发生部分修改 | 增删行数、变更文件、行号、差异审阅、跳转 | 保留事件的权威变更集；不为模型回传完整操作对象或整份差异；不重复在宿主载荷保存已有变更集 |
| `get_goal`、`create_goal`、`update_goal` | 当前目标、状态、有效预算与必要完成报告 | 目标、进度、预算 | 当前前端直接读取模型 JSON，须同步迁移；有专用进度控件才保留其消费字段；纯属性展示改为共用 Markdown |
| `submit_plan` | 接受结果、名称、路径或可操作标识 | 已接受的计划正文及既有计划操作 | 不把计划全文再次发给模型；前端引用已接受计划正文的权威存储，不能拿未确认输入冒充结果；正文存储归属须在实施前追踪明确 |
| `browser_list`、`browser_open`、`browser_select`、`browser_close` | 标签或浏览器身份、标题、地址、选中状态、结果 | 简单回执与现有浏览器入口 | 默认共用短 Markdown；准确可操作标识保留，不发送内部连接或协议调试数据 |
| `runlua_session` | 会话身份、状态、执行输出、必要下一步信息 | 会话信息和执行正文 | 按启动、执行、状态、关闭逐项设计，不再把漂亮打印 JSON 当作文本输出；求值结果中的原始业务 JSON 按原文保留 |
| `runtime_config` | 配置项、类型、必填性、约束、校验失败及允许的操作 | 当前有配置声明专用展示 | 优先共用 Markdown；若保留专用控件，只保留所消费的声明信息；禁止输出敏感值或扩大既有查询能力 |
| `get_model_extension_parameters` | 精确模型标识、可用参数、必填项、枚举、组合约束及能力限制 | 当前有模型参数专用卡 | 从同一类型化契约生成精简说明，前端同步调整；若保留参数专用展示，结构仅含所消费字段；完整 Schema 的处理见第二部分 |
| 公共工具错误、后置处理错误 | 失败位置、原因、是否已产生副作用、能否安全重试、必要恢复动作 | 错误状态与可读原因 | 模型改为简洁 Markdown；前端错误类型由权威状态决定；禁止后置失败再次把成功正文包成 JSON 或误导模型重做已完成操作 |

## 三、当前前端证据与配套改动

### 1. 普通结果目前仍有结构依赖

`apps/vulcan-code-web/src/components/conversation/tool-presentations/v1-tool-domain-presentation.ts` 的
`resolveV1ToolDomainPresentation` 直接读取目标、检索、知识库、记忆、配置和媒体理解等模型 JSON 字段。
其中知识库正文、媒体理解回答已经单独按 Markdown 展示。

实施要求：

- 对纯正文工具移除新结果对这些 JSON 字段的依赖，直接传入唯一 Markdown。
- 仅历史 JSON 记录保留现有读取分支；以明确工具契约和版本区分新旧，不根据字符串外观猜测。
- 检查 `v1ModelResult` 的解码边界，保证 Markdown 中含 JSON 示例、花括号或以代码块开头时不会被误判为新的结构化输出。
- `conversation-tool-result-tabs.tsx` 的结果、输入、输出标签可以从同一事件即时构建，不为每个标签持久化副本。

### 2. 文件变更有独立权威来源

`tool-presentations/file-change-presentation.ts` 的 `resolveFileChangeContent` 优先消费工具事件上的变更集，
并支持请求构造期间的预览字段。

实施要求：

- 模型摘要改为 Markdown 不影响变更集、增删行数和差异审阅。
- 知道工具身份后及时显示卡片，参数流入时继续更新构造预览；不能等待最终 Markdown 才展示工具。
- 最终结果不能覆盖或丢失已经确认的变更事实；部分失败仍展示已发生的修改。
- 历史宿主变更集兼容分支与当前事件权威分开处理，不重新引入双写。

### 3. 搜索和提取不应为当前卡片保留整份正文克隆

`tool-presentations/provider-tool-presentation.ts` 当前搜索卡消费排序、标题、地址、域名、摘要；
提取卡消费地址、正文、图标和图片列表。

实施要求：

- 默认以 Markdown 标题、链接、摘要、正文替代普通结构展示，引用仍可点击。
- 若决定保留某项专用交互，先证明对应字段用途；正文仍只有一份，图标等仅前端字段不进入模型正文。
- 不因移除复制字段而丢失提取失败列表或部分成功信息。

### 4. 交互类前端保留业务结构

协作卡、Ask、目标进度、待办、计划操作和媒体播放不根据 Markdown 反推业务状态。
优先使用现有领域事件和接受后的快照，正文渲染复用现有链路。

- `conversation-collaboration-card.tsx` 已按类型化协作状态生成参与者、正文、状态及导航。
- `conversation-model-parameters.tsx` 当前存在模型参数专用卡，改输出时必须同步适配。
- `conversation-media-generation-result.tsx` 当前从模型结果取资源信息，再按资产身份匹配宿主本地路径；任何资源投影精简都必须保持准确关联。
- 遵守 `docs/contracts/ask-and-markdown-interaction.md`；不增加技术详情、独立投递卡或代理问题撤销控件。

## 四、输出简洁性规则

1. 短操作结果用一至数行，不强制添加多级标题、表格或结尾总结。
2. 长正文只保留一份；标题、路径、行范围、时间戳按阅读和定位需要呈现。
3. 不回显完整输入；只有实际采用值与请求不同、需要消歧或后续调用依赖时才反馈。
4. 不输出没有语义价值的空数组、空对象、空字段和零统计；表示“未找到”“未修改”“已无剩余任务”等关键事实必须明确，不能被精简掉。
5. 不重复状态：同一回执不在标题、字段列表和末尾总结中说三遍成功。
6. 截断必须显式标注，并提供真实可用的续读方式；续读游标视为不透明值，禁止格式化改写。
7. 外部材料与工具说明分开，维持来源和不可信内容边界；文件代码使用安全围栏，不增加影响内容准确性的装饰。
8. 不设置无依据的全局新截断常量；沿用现有唯一预算来源，必要的新预算必须统一定义并说明理由。

# 第二部分：保留结构或不做 Markdown 转换的结果精简

## 一、保留结构的判定标准

仅因“后端当前返回对象”或“前端当前解析 JSON”不构成保留理由。
本部分仍只针对会话工具：协议明确要求原生内容块、用户需要保留原始业务数据，
或者尚未确认外部工具的结果契约，转换可能破坏语义时，可以保留结构。

**本身依赖 JSON 解析的特殊调用直接排除，不作为本部分的精简对象。**
若同一能力同时存在会话阅读入口和机器解析入口，只改已经确认的会话输出投影，不修改共享机器返回值。

所有保留项都要继续审核模型投影，不允许把整个内部对象直接序列化后视作完成设计。

## 二、分类精简方案

| 类别 | 为什么保留或暂不转换 | 模型最小必要内容 | 精简与前端处理 |
| --- | --- | --- | --- |
| 原生图片、音频等多模态内容块 | 模型感知能力与协议要求依赖原生表示 | 必要内容块、资源引用、识别所需类型信息 | 不降成纯 Markdown 链接；不同时附带同一内容的编码文本和完整协议快照；保留协议必需字段 |
| 媒体生成资源描述 | 资源需要后续工具准确引用，前端需要本地展示 | 可操作资源引用、类型、必要名称及失败信息 | 去除仅前端需要的本地播放信息和内部供应商载荷；前端按现有资产身份读取宿主展示结构；各字段是否保留以实际消费者核定 |
| 会话工具返回的数值或排序结果 | AI 明确请求查看数据时，原始值或结果顺序本身就是任务内容 | 所请求的数值、顺序、必要索引或标识 | 只审查最终会话阅读结果，不自动回传整份输入文档；内部向量计算、重排序接口及任何依赖 JSON 解析的特殊调用均排除 |
| 会话中的参数查询结果 | AI 需要了解下一步调用的精确约束 | 模型执行下一步确实需要的契约说明或片段 | 只修改查询工具的阅读结果，不修改工具输入 Schema 或机器配置契约；不可省略会影响有效调用的约束，不整包发送内部发布证据和运行时信息 |
| 用户通过会话工具明确请求的原始数据 | 格式本身可能就是任务对象 | 请求范围内的响应、必要会话身份、序号和继续读取位置 | 原始内容保持准确，仅审查会话工具外围冗余；调试协议及本身依赖解析的特殊调用排除 |
| 未知第三方 MCP 或插件对象 | 宿主没有证据判定哪些字段可以删除 | 外部工具声明的有效结果 | 不做猜测性字段删除或通用对象扁平化；有契约后逐项适配；保留未适配状态并在审计清单记录 |
| 原本就是代码、JSON、日志或数据文件的正文 | 原文是用户需要分析的内容 | 请求的原始内容 | 不因外观看起来像 JSON 就转成属性表；精简外层元数据和重复副本即可 |

## 三、已经是文本的工具也要复核

| 工具 | 当前方向 | 仍需检查的冗余 |
| --- | --- | --- |
| `file(read)` | 保持现有 Markdown 与行映射 | 同一终态记录正文是否只有一份；直读和延迟加载是否共用同一内容 |
| `shell_command`、`terminal_session` | 保持命令输出文本 | 不重复命令参数与全部输出；保留退出、运行、超时、分页及副作用语义；现有进程控制和输出硬上限不变 |
| `ask` | 保持已生成的 Markdown 回答 | 宿主仅保留真实提问交互及逐题关联所需内容；不可通过重新解析回答文本恢复选择状态，也不重复保存同一答案的多个展示副本 |
| `todolist` | 保持简短模型回执与专用待办展示 | 模型不重复收到整份已提交清单；宿主保留已接受条目状态，不把请求当作已接受结果 |
| `skill`、`luaskill`、`call_luaskill`、`runlua` | 保持当前文本路径 | 检查前后包装、执行说明、输入回显和宿主正文复制；原始脚本输出不按猜测重写 |

## 四、MCP、LuaSkills 与插件专项

### 1. MCP 文本与结构选择

现状证据：`crates/vulcan-mcp-runtime/src/rmcp_support.rs` 的 `rmcp_call_tool_result_to_value`：

- 无图片时优先使用结构化内容。
- 普通文本分支只取第一个文本块。
- 文本可解析为 JSON 时再次转为结构化值。

改造方案：

- 汇总所有有效文本块并保持顺序，不能遗漏后续文本。
- 对有明确契约、文本就是完整模型结果的会话工具，使用文本，不同时把两份等价内容发给模型；机器解析入口不在本次调整范围内。
- 不能全局假设文本与结构等价：有独有结果仅存在结构中时必须保留其语义；先核对上游结果契约和样本。
- 多模态块、资源链接与嵌入资源单独按协议处理，不使用文本优先规则吞掉它们。
- 对未知工具不猜测可删字段，不把合法纯文本仅因能解析成 JSON 就自动改变其语义类型。

### 2. LuaSkills 与插件

- 普通技能输出本身可能已是 Markdown，桥接不再包装完整结果对象。
- `runlua_session` 是明确改造项，不能因返回类型为字符串而漏审。
- 插件后置处理失败时保留原成功结果及明确警告，不重复完整结果，也不能误标为可安全重试。
- 技能配置、版本、宿主注入等实现若涉及变更，先读 `docs/contracts/luaskills-skill-config-guide.md`；外部技能仓库另列责任边界。
- 权威资源变更后显式执行同步；不修改运行产物目录。

## 五、结构载荷的字段审计方法

每个保留字段逐项回答并形成记录：

1. 哪个具体产出方生成它？哪个模型动作或前端方法消费它？
2. 它是否已经在输入、事件、资源、领域对象或同一结果另一字段中存在？
3. 删除后是否影响下一步调用、权限判断、错误恢复、引用或结果准确性？
4. 它是业务必需信息，还是版本、调试、缓存、完整内部快照等实现信息？
5. 文本及宿主是否都需要它？如果只为前端使用，为何进入模型上下文？
6. 如果采用引用，读取入口是否真实可用，跨重启、历史回放、延迟加载是否仍有效？

最终将字段归为：模型必需、前端必需、领域或诊断内部必需、可删除。分类允许多个真实消费者，但重复正文必须改为共享权威来源。

# 实施顺序、技术选择与验收

## 一、技术选择

- 使用现有 Rust 类型和工具结果投影边界，按工具或结果类型显式生成 Markdown；不引入通用递归 JSON 转 Markdown 库。
- 共用必要的围栏、来源链接、续读提示等小型格式化能力；字段选择仍归工具拥有者负责。
- 前端复用现有会话 Markdown 与专用卡片，不建立第二套 Markdown 解析器。
- 保留工具事件、变更集、领域状态、资源存储的现有归属。正文引用优先利用已有标识；无法在现有存储内满足时先形成明确变更设计，不能私自新增整套内容仓库。
- 不改变工具请求 Schema、业务行为或权限来换取输出简洁；必要的公开契约变化单独列出影响与迁移方式。

## 二、执行步骤

- [x] **步骤一：冻结审计清单。** 从实际会话工具注册与可见性入口列出当前工具和动态扩展；先排除本身依赖 JSON 解析的特殊调用和机器接口。范围内每项登记成功、部分成功、失败、取消、超时与后置失败样本，追踪模型投影及前端消费者。把本方案中需进一步确认的正文归属、引用入口和动态工具契约核实完毕。
- [x] **步骤二：建立公共输出约束。** 明确纯文本、最小宿主结构和原生多模态各自的边界；完成公共错误与后置失败的结果设计，防止迁移后再次包回 JSON。补充面向 AI 的结果契约文档。
- [x] **步骤三：处理大正文和重复存储。** 优先供应商文本与提取、代码检索、知识库、记忆查询、媒体理解；同步修改前端结构读取，落实唯一正文及来源引用。
- [x] **步骤四：处理文件修改和操作回执。** 文件写改删移、浏览器、记忆写删、Lua 会话等改为简洁结果；保留变更集、流式预览和控制状态。
- [x] **步骤五：处理协作和交互工具。** 子代理、跨会话消息、问题、等待、目标、计划和配置逐项投影；保证稳定标识、授权、终态与回复义务不被精简掉。
- [x] **步骤六：处理会话工具保留结构的载荷。** 对范围内的媒体资源、原始数据、参数查询、MCP 和插件阅读结果逐项削减无消费者字段，修复会话文本块遗漏；不改依赖 JSON 解析的特殊调用。未取得契约的外部工具明确记录，不声称已完成适配。
- [x] **步骤七：核对已是文本的路径。** 检查命令、Ask、待办、技能及文件读取，确认没有重复正文和额外全量回显。
- [x] **步骤八：验证与收尾。** 按验收矩阵回归并记录大小对比；更新相关契约。全部完成后追加执行变更总结，再迁移本计划至完成目录。

每个实施批次包含对应后端投影、前端消费、契约和必要测试，形成可独立验证的提交；不只改后端输出就交付。

## 三、验收标准

### 功能与语义

- [x] 实施清单全部属于会话工具；依赖 JSON 解析的特殊调用、机器接口及共享底层协议保持原契约。
- [x] 第一部分每项都有明确的模型输出设计和前端消费结论，无仅按类别推测的宿主字段。
- [x] 指定工具的新结果为直接文本，包含代码或 JSON 原文时仍按文本契约处理，不再次整体 JSON 编码。
- [x] 搜索、读取、提取、理解结果保留来源、范围、引用及必要续读能力。
- [x] 零命中、无变更、空内容、部分失败、停止、未知状态都准确表达；不能将未知表现为完成。
- [x] 协作的等待、回复、取消、继续读取所需标识完整；消息通过既有动态上下文交付时不与回执重复交付。
- [x] 错误与后置失败明确区分工具未执行、部分执行、已完成但展示处理失败，避免重复副作用。

### 前端与历史

- [x] 普通结果与模型使用同一 Markdown 正文；不新增无用途宿主对象。
- [x] 文件行号、差异、导航、媒体资产匹配、计划操作、Ask 和待办交互保持可用。
- [x] 工具构造期间仍及时出现卡片及动态预览，不等待终态正文。
- [x] 历史 JSON、当前 Markdown、直接渲染、延迟详情、重启回放均有代表性验证。
- [x] 不引入新的技术详情、重复投递卡和空内容卡，不破坏既有协作卡布局与状态归属。

### 成本与载荷

- [x] 对同一组真实结果样本记录改造前后的模型文本字节数、宿主字节数和完整终态封装字节数；可使用现有可靠分词入口时再记录 Token，不以字符估算冒充精确 Token。
- [x] 对具备唯一正文要求的结果，用独特正文标记断言同一终态持久化封装内只出现一次；不将业务上必须独立存储的历史事件误判为展示副本。
- [x] 检查前端展示的必要字段和模型必要字段分别存在，内部调试结构不进入模型结果。
- [x] 范围内会话数据和媒体内容不产生额外编码副本；摘要所带资源引用具有可用读取路径。
- [x] 成本缩减以逐工具样本实测为准，不预设无依据的统一百分比，也不为缩短而丢掉独有业务信息。

### 验证执行范围

- Rust 优先相关 crate 的定向测试；涉及公开签名或测试类型时使用全部目标检查。保持仓库编译并发上限，共享注册表或供应商资源测试串行。
- 前端运行对应展示、历史兼容、延迟加载和交互测试，再执行类型检查和相关格式检查。
- 重点端到端验证真实工具结果经过公共转换、持久化、恢复、前端和供应商请求投影后仍符合契约；不能只测格式化函数。
- 正文围栏、Unicode、空行、超长行、多文本块、链接、分页、部分成功和真实原始 JSON 内容均纳入针对性样本。

## 四、源码依据与实施入口

以下为本次已核对的关键入口；路径及消费字段在实施前仍以实时源码复核。

| 领域 | 入口 |
| --- | --- |
| 公共模型序列化 | `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs` 的 `canonicalize_tool_invocation_result` |
| 公共错误 | `crates/vulcan-tools-manager/src/types.rs` 的 `from_tool_error_details` |
| 后置处理结果 | `crates/vulcan-agent-runtime/src/agent_runtime/vulcan_plugin_hooks.rs` 的 `completed_with_post_hook_failure` |
| 原生注册与桥接 | `crates/vulcan-agent-runtime/src/tooling/native.rs`、`tooling/bridge.rs` |
| 文件结果与既有契约 | `crates/vulcan-host-tools/src/file_runtime.rs`、`docs/contracts/file-read-output.md` |
| 原生领域结果 | `crates/vulcan-agent-runtime/src/tooling/native/` 下的知识库、VMM、目标、计划、配置和浏览器实现 |
| 代码检索 | `crates/vulcan-service-kernel/src/vce/services.rs` 的 `vce_retrieval_response` |
| 供应商与媒体理解 | `crates/vulcan-agent-runtime/src/provider_tool_bridge.rs`、`media_understanding.rs` |
| 子代理结果 | `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/` |
| 跨会话结果 | `crates/vulcan-service-kernel/src/agent_collaboration.rs`、`agent_collaboration/` |
| Lua 输出 | `crates/vulcan-agent-runtime/src/luaskills_runtime/runlua.rs`、`tooling/bridge/luaskills_dispatch.rs` |
| MCP 转换 | `crates/vulcan-mcp-runtime/src/rmcp_support.rs` 的 `rmcp_call_tool_result_to_value` |
| 前端领域分发 | `apps/vulcan-code-web/src/components/conversation/tool-presentations/v1-tool-domain-presentation.ts` |
| 前端专用展示 | 同目录下文件、命令、供应商展示；会话目录下的协作卡、媒体结果、模型参数与结果标签组件 |

## 五、实施前需明确的边界

- 当前工作区存在其他会话的前端未提交修改；后续实施需重新检查所有权，不覆盖并行工作。
- `hits[].content` 与 `formatted_context` 的具体重叠范围、VMM 时间线与详情的重叠、计划正文权威引用方式，必须沿各自生产者核实后再删减。
- 范围内的媒体资源或大型会话结果若需要新引用方式，必须证明可读取和持久有效；现有能力不足时单独设计，不能先输出占位引用。
- 第三方工具不能仅依据一个样本决定删字段；动态工具审计以实际启用目录和可确认的上游契约为准。
- 方案编制完成不等于实施完成。下方保存分阶段实施记录；最终验收、四项执行总结完成后归档为 `docs/completed/20260926/03-TOOL_RESULT_MARKDOWN_AND_PAYLOAD_MINIMIZATION.md`。

## 六、实施进度记录

### 工作隔离

- 工作树：`C:/Users/lingeasy/.codex/worktrees/tool-result-markdown/vulcan-code`。
- 分支：`codex/tool-result-markdown`。
- 基点：`96c0c7600855adbfeb2831ce7e89d0578206aa6a`；按用户要求使用创建时最新提交，未复制主工作区未提交文件。
- 主工作区并行流式输出调整未被修改；合并前仍须逐文件复核并行差异。

### 首批实施快照（最终状态见后续批次及验证记录）

| 范围 | 实现与消费者结论 |
| --- | --- |
| 供应商文本、搜索、提取及纯阅读复合结果 | 单一 Markdown；保留排名、链接、摘要、证据图片与部分失败；去除宿主正文副本。结构化向量与混合资源结果尚待第二部分核对。 |
| 知识库三类工具 | 服务内部 JSON 不变；会话投影为 Markdown，保留库、文档、切片标识、范围、续读位置及失败警告。 |
| 代码检索 | 只在会话回调生成 Markdown；不再同时返回命中正文和其拼接全文；普通服务 API 保持 JSON。 |
| 原生文件列表与属性 | 共享 Markdown；前端即时与延迟详情均已适配，旧 JSON 展示继续可用。 |
| 文件写改删移 | 模型仅收到真实变更路径、操作与匹配数；变更集先严格提取为事件字段，再去除持久化输出的重复宿主差异。后者正在验证。 |
| VMM 查询与写删 | 会话 Markdown；保留来源轮次及支持、反驳、匹配情境证据；写入保留顺序与复用状态，删除区分实际删除和不可用项。 |
| VMM 轮次详情 | 已核对原生产者：用户、时间线、最终回答来自规范对话拆分，详情来自独立分析字段；均保留，不假定重复而删除。 |
| 媒体理解 | 内部规范结果校验保持不变；最终会话正文一次存储，保留时间线、未知端点及警告。 |
| 浏览器管理 | 保留可操作浏览器标识、真实状态、选中情况及观察到的地址和标题。 |
| Lua 持久会话 | 已核对当前锁定 LuaSkills 0.5.9 源码；去掉内部 SID、代际、引擎路径等；保留会话标识、有效期、终态与求值原始业务数据。 |
| 目标与计划 | 目标为 Markdown，共享领域快照继续服务目标控件；创建指引保持原内容。计划回执不回显全文，前端正文取成功写入边界的已接受产物快照。 |

### 当前验证证据

- 第一轮原生工具定向测试 102 项通过；追加 VMM 和浏览器后第二轮 103 项通过。之后的目标、Lua、媒体改动需要新一轮测试，不能沿用旧结果认定通过。
- 供应商桥接 22 项通过；代码检索投影内核测试 1 项通过。
- 前端首批 4 文件共 113 项通过；追加目标、浏览器、媒体和计划契约后，领域展示文件 33 项通过。
- 前端类型检查通过；后续修改仍需最后统一复核。
- 新文件列表/属性即时与延迟加载测试初次失败，原因是测试夹具漏掉生产投影的 `arguments: event.input`；核对真实生产者后修正夹具，40 项文件展示测试通过。

### 首轮源码审核发现

1. 原生文件专用卡会绕过普通结果正文，单改后端会导致列表或属性详情空白：已配套增加共享 Markdown 正文及即时、延迟测试。
2. 目标旧宿主结果无当前展示消费者，目标控件由领域快照维护：移除该无用途副本，保留领域读写。
3. 文件差异原来同时存在事件 `changeset` 与输出 `host_result`：已在严格提取成功后的持久化边界去重，正在执行对应回归。
4. 未知时间、用量和检索证据不得默认为零或不存在：结果格式化保留明确未知状态。

### 后续批次记录

- 完成子代理六个会话工具、跨会话回执、读取与统一等待的投影。内核确认等待结果所需身份移到最小宿主回执，内部机器接口保持不变。
- 完成只读技能配置及模型参数查询投影；保留准确调用约束和必要 Schema，移除内部发布信息和宿主正文副本。
- 完成嵌入、重排、复合结果的外围精简；媒体资产保留专用展示和后续工具引用所需字段。
- MCP 增加会话消费者边界：完整保留文本块、顺序、原始 JSON 文本、原生图片、资源 URI；机器入口保持旧契约，未知第三方字段不猜测删除。
- 公共错误去掉原始参数回显；后置失败保留完成结果并明确避免重复副作用。
- Ask 去除宿主中的两份答案副本，前端直接共用最终 Markdown，不通过反向解析恢复选择状态。
- 已核对命令、终端、待办和技能。终端帧、流身份和控制数据有独立消费者，保留其控制契约；不把未知外部技能载荷笼统宣称为已适配。
- 新契约：`docs/contracts/tool-result-presentation.md`；同步 Ask 与文件读取契约入口。

## 七、循环审核记录

### 第一轮：生产者与实际消费者

发现并修复：

1. 等待结果从 JSON 改为正文后，内核仍依赖旧模型 JSON 确认报告消费。新增精确宿主确认字段，保留旧记录读取，畸形新记录不能猜测回退。
2. 关闭代理宿主结果仍承担面板关闭的机器消费者职责。保留该类型和精确身份，删除完整注册表正文；创建代理监听器继续使用独立委派事件。
3. 文件差异在工具输出与事件变更集重复。严格转换成功后仅保留事件变更集，前端即时与延迟路径复用该事件。
4. MCP 底层转换同时服务机器调用。新增明确消费者选择，避免把内部 Lua 和程序接口也改成 Markdown。

### 第二轮：前端、状态和失败

发现并修复：

1. Ask 旧卡片与当前 V1 卡片是两个实际入口，均需支持新结果。
2. 曾尝试为答案生成字符范围；审核发现后续凭据脱敏与插件处理会改变偏移，因此移除该设计，直接共用最终 Markdown。
3. 阅读会话时补回答案投递失败原因、取消原因和任务投递失败原因；未知状态与实际停止不折叠为成功。
4. 结构化数值结果与无媒体复合结果不再生成完整宿主克隆，保留原始数据。
5. 执行后的子代理或协作投影异常明确提示操作已返回，禁止为恢复展示自动重新执行。

### 第三轮：持久化与集成边界

- 检查正文经过规范化和序列化恢复后不二次编码，Ask 答案只存一份。
- 检查文件变更集持久化唯一、协作确认新旧兼容、消息义务和人工停止仍准确。
- 检查 MCP 会话模式与机器模式分离；后置 Hook 失败保留已经完成的结果。
- 本轮发现一处新引入的无调用方法警告，已让机器分支继续使用原入口消除，未增加忽略警告属性。
- 前端用量 Tooltip 测试在基线组件下同样失败，记录为既有问题，不越界改动另一项界面功能。

## 八、成本实测

通过当前真实生产函数生成结果，以冻结的旧封套形状作为对照，再经过同一规范结果序列化。
样本使用确定性文本，便于重复比较；不是生产用户数据，也不代表所有结果的统一缩减比例。
以下均为 UTF-8 字节，没有将字节或字符冒充 Token。

| 样本 | 模型：原值 → 新值 | 宿主：原值 → 新值 | 完整规范结果：原值 → 新值 |
| --- | --- | --- | --- |
| 用户提问答案 | 2,122 → 2,122 | 4,458 → 46 | 6,680 → 2,268 |
| 搜索摘要 | 1,256 → 1,118 | 1,298 → 0 | 2,687 → 1,226 |
| 网页提取 | 5,342 → 4,965 | 5,384 → 0 | 11,110 → 5,330 |

对应测试：`ask_payload_cost_sample`、`provider_payload_cost_samples`。
每个样本同时断言独特正文标记在最终封套中仅出现一次。

## 九、验证记录

| 范围 | 结果 |
| --- | --- |
| 原生工具及文件流式预览 | 104 项通过 |
| 供应商桥接、媒体与模型参数 | 23 项通过 |
| 公共工具桥接 | 25 项通过 |
| Markdown 投影集合 | 22 项通过，包含真实代理目录及状态生产者复验 |
| 公共 Markdown 围栏与来源转义 | 2 项通过 |
| 代码检索正文唯一投影 | 1 项通过 |
| Ask 运行时、持久化恢复、成本 | 17 项通过 |
| 插件真实后置失败集成 | 1 项通过 |
| MCP 转换、机器兼容、多文本、图片与错误 | 16 项通过 |
| 统一等待、人工停止、缺失所有者 | 11 项通过 |
| 等待确认新旧持久化兼容 | 1 项通过 |
| 变更集持久化及拒绝畸形结果 | 4 项通过 |
| 前端工具展示、历史及延迟加载、媒体和控制选择器 | 238 项通过，1 项既有 Tooltip 测试失败 |
| 前端类型检查 | 通过 |
| 本次修改前端文件 ESLint | 通过 |
| 本次修改 Rust 文件格式与差异空白检查 | 通过 |
| 四个相关 crate 的全部目标编译检查 | 通过；随后移除仅测试使用的导入在生产模块中的警告，并通过内核定向编译测试 |

既有失败：`conversation-tool-presentation-card.test.tsx` 的
`portals focused subagent usage outside a clipped tool fold`，
测试寻找的 `data-subagent-usage-tooltip` 触发器不存在。将该卡片临时恢复为基线 `96c0c760` 的版本后，
同一测试仍在相同位置失败；恢复过程在结束时完整恢复了本任务文件。未删测试、未跳过后声称全绿。

## 十、交付边界

- 独立工作树基于最近已提交节点 `96c0c7600855adbfeb2831ce7e89d0578206aa6a`，
  分支 `codex/tool-result-markdown`；未复制原仓库未提交流式输出修改。
- 原仓库另有服务生命周期和事件窗口修改，继续由原会话拥有。
- 未启动付费供应商请求或真实用户对话；验证采用本地真实生产入口、确定性测试、持久化与前端回归。
- MCP 音频及二进制嵌入资源没有现成受支持的原生载体；现在明确报出能力边界和已执行风险，不静默丢弃。
- 未知第三方工具和外部技能对象仍保持原契约；本计划不授权猜测删除其业务字段。
- 未修改权威运行时资源，因而没有执行资源同步或替换用户正在运行的服务。

## 十一、执行变更总结

### 1. 核心修复与调整概述

- 会话工具按已确认生产者显式生成英文 Markdown；普通阅读结果由模型和前端共用单一正文。
- 宿主载荷按真实消费者裁剪：保留导航、等待消费确认、差异、资产、计划及交互标记，移除无用途正文克隆。
- 文件差异在严格转换成功后只写入事件变更集；Ask 答案只保存最终正文，避免脱敏和后置处理造成偏移失效。
- MCP 会话消费与机器消费分离，保留所有独有文本、结构和受支持多模态；内部 JSON 调用不受影响。
- 三轮审核完成生产者、消费者、历史、持久化、失败及集成边界复核；已发现的本任务问题均修正并复测。

### 2. 📂文件变更清单

以下路径相对本工作树仓库；不包含依赖安装、共享编译产物和其他会话文件。

#### 新增

- `apps/vulcan-code-web/src/components/conversation/tool-presentations/ask-question-presentation.test.ts`
- `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface/result_markdown.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge/result_markdown.rs`
- `crates/vulcan-agent-runtime/src/luaskills_runtime/runlua/result_markdown.rs`
- `crates/vulcan-agent-runtime/src/media_understanding/result_markdown.rs`
- `crates/vulcan-agent-runtime/src/provider_tool_bridge/result_markdown.rs`
- `crates/vulcan-agent-runtime/src/provider_tool_bridge/result_markdown/extension_parameters.rs`
- `crates/vulcan-agent-runtime/src/tooling/collaboration/result_markdown.rs`
- `crates/vulcan-agent-runtime/src/tooling/collaboration/result_markdown/receipts.rs`
- `crates/vulcan-agent-runtime/src/tooling/collaboration/result_markdown/session_read.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/file_result.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/knowledge_base/result_markdown.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/runtime_config/result_markdown.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/vmm/result_markdown.rs`
- `crates/vulcan-mcp-runtime/src/rmcp_support/conversation_result.rs`
- `crates/vulcan-service-kernel/src/vce/services/tool_result.rs`
- `crates/vulcan-tools-manager/src/result_markdown.rs`
- `docs/contracts/tool-result-presentation.md`
- `docs/completed/20260926/03-TOOL_RESULT_MARKDOWN_AND_PAYLOAD_MINIMIZATION.md`

#### 修改

- `Cargo.lock`
- `apps/vulcan-code-web/src/components/conversation/conversation-media-generation-result.tsx`
- `apps/vulcan-code-web/src/components/conversation/conversation-tool-metadata.ts`
- `apps/vulcan-code-web/src/components/conversation/tool-presentations/ask-question-card.tsx`
- `apps/vulcan-code-web/src/components/conversation/tool-presentations/ask-question-presentation.ts`
- `apps/vulcan-code-web/src/components/conversation/tool-presentations/file-access-card.tsx`
- `apps/vulcan-code-web/src/components/conversation/tool-presentations/file-change-presentation.test.ts`
- `apps/vulcan-code-web/src/components/conversation/tool-presentations/file-change-presentation.ts`
- `apps/vulcan-code-web/src/components/conversation/tool-presentations/subagent-orchestration-card.tsx`
- `apps/vulcan-code-web/src/components/conversation/tool-presentations/subagent-orchestration-presentation.test.ts`
- `apps/vulcan-code-web/src/components/conversation/tool-presentations/subagent-orchestration-presentation.ts`
- `apps/vulcan-code-web/src/components/conversation/tool-presentations/types.ts`
- `apps/vulcan-code-web/src/components/conversation/tool-presentations/v1-tool-domain-presentation.test.ts`
- `apps/vulcan-code-web/src/components/conversation/tool-presentations/v1-tool-domain-presentation.ts`
- `crates/vulcan-agent-runtime/src/agent_runtime/question_runtime.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/task_runtime/tool_surface.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/tool_result_media.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/tool_runtime_bridge/tests.rs`
- `crates/vulcan-agent-runtime/src/agent_runtime/vulcan_plugin_hooks.rs`
- `crates/vulcan-agent-runtime/src/luaskills_runtime/runlua.rs`
- `crates/vulcan-agent-runtime/src/mcp/runtime/handle.rs`
- `crates/vulcan-agent-runtime/src/mcp/runtime/tools.rs`
- `crates/vulcan-agent-runtime/src/media_understanding.rs`
- `crates/vulcan-agent-runtime/src/provider_tool_bridge.rs`
- `crates/vulcan-agent-runtime/src/tooling/bridge.rs`
- `crates/vulcan-agent-runtime/src/tooling/collaboration.rs`
- `crates/vulcan-agent-runtime/src/tooling/native.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/apply_patch.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/browser.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/file.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/file_edit.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/goal.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/knowledge_base.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/runtime_config.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/submit_plan.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/tests/file_tools.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/tests/planning.rs`
- `crates/vulcan-agent-runtime/src/tooling/native/vmm.rs`
- `crates/vulcan-mcp-runtime/Cargo.toml`
- `crates/vulcan-mcp-runtime/src/http.rs`
- `crates/vulcan-mcp-runtime/src/lib.rs`
- `crates/vulcan-mcp-runtime/src/rmcp_support.rs`
- `crates/vulcan-mcp-runtime/src/sse.rs`
- `crates/vulcan-mcp-runtime/src/stdio.rs`
- `crates/vulcan-mcp-runtime/src/transport.rs`
- `crates/vulcan-service-kernel/src/agent_collaboration/native_dispatch_tests/unified_wait.rs`
- `crates/vulcan-service-kernel/src/agent_collaboration/result_wait.rs`
- `crates/vulcan-service-kernel/src/agent_stream/provider_v1_sink.rs`
- `crates/vulcan-service-kernel/src/vce/services.rs`
- `crates/vulcan-tools-manager/src/lib.rs`
- `docs/contracts/ask-and-markdown-interaction.md`
- `docs/contracts/file-read-output.md`

#### 删除与迁移

- 无源码文件删除。
- 本方案从 `docs/plan/20260926-03-TOOL_RESULT_MARKDOWN_AND_PAYLOAD_MINIMIZATION.md` 归档至上述完成目录，日期和序号保持不变。

### 3. 💻关键代码调整详情

- 公共格式函数负责原文围栏、行内代码及来源链接转义；各工具自己的强类型投影负责业务字段选择，没有通用猜测式 JSON 递归转换。
- `canonicalize_tool_invocation_result` 沿用规范载体并让字符串保持直接文本；结构化错误使用明确语义投影，去掉原始请求参数回显。
- `persistent_tool_completion_payloads` 先严格提取变更集，再移除输出中的重复宿主差异。
- `persisted_report_identities` 读取新等待确认宿主字段；仅无新宿主时支持历史模型 JSON，畸形新记录不猜测回退。
- `build_question_reply_projection` 返回单一 Markdown；两条实际 Ask 前端入口读取最终正文，旧记录继续按明确旧结构展示。
- `McpToolResultAudience` 区分机器调用和会话阅读，传输层一次执行后按消费者投影；保留原所有者、租约与撤销检查。
- 插件后置处理失败追加警告并保留既有正文和原生图片，明确禁止为了恢复展示自动重复执行。
- 未改变流式请求构造、增量预览、工具输入、权限、业务状态机或会话元数据版本。

### 4. ⚠️遗留问题与注意事项

- 前端一项用量 Tooltip 测试在基线组件下同样失败，见验证记录；本次没有越界修复、删除测试或声称全量通过。
- 未知外部技能和自定义 MCP 传输没有逐字段契约，保留它们的原始业务输出；这是明确排除的机器／未知接口边界。
- MCP 音频与二进制嵌入资源缺少当前宿主原生载体支持，明确报错并说明上游已经执行，不静默丢弃；扩展载体须另行设计。
- 成本数据来自本地真实生产函数生成的确定性样本，不能外推成所有工具或真实账单的统一节省比例。
- 没有调用付费供应商验证、批量迁移历史记录、替换运行中的服务或自动合并主工作区；集成前需重新核对并行流式输出改动。
