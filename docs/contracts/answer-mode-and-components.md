# 回答模式与回答组件

## 权威与同步触发点

本契约归属于 `vulcan-session-core::v1::document::answer_mode`（会话偏好与上下文同步）、
`vulcan-session-core::v1::answer_components`（组件容量边界）与运行时资源
`crates/vulcan-runtime-assets/resources/agent-prompts/answer-mode-interactive.md`（组件语法的唯一
权威）。修改模式语义、持久字段、注入方式或组件语法时，必须同步本文、互动提示词、WebUI 与 TUI 两个
渲染器及其行为测试。

## 模式语义

- 回答模式只有 `standard` 与 `interactive`（系统默认）两值，与输出风格相互独立：输出风格决定语气与
  篇幅，回答模式决定最终回答的呈现形式：标准模式只用普通 Markdown；互动模式要求最终回答优先以 `ui-*`
  组件呈现（凡有合适组件即不用 Markdown 列表或表格），散文只用于一两句结论、衔接文字及无结构的极短回答，
  工作中的进度说明仍用普通 Markdown。是否使用组件不交由模型自行判断。
  提示词同时规定回答的组织方法（结论先行、证据后接行动、每个组件一个要点并有引导句、按重要性排序、
  内容决定组件数量）及常见回答类型的骨架，而非只列出内容与组件的对应关系。
- `SessionConfig.answer_mode` 是会话唯一权威，含 `mode`、独立比较交换 `revision` 与可空
  `prepared` 快照。`service_set_session_answer_mode` 接收 `sessionId`、`mode`、`expectedRevision`，
  冲突时返回当前值，不自动覆盖或重试；生成中只保存期望，下一请求生效。
- 系统默认只定义于 `AnswerMode` 的 `Default`（`interactive`），WebUI 浏览器回退快照经生成常量
  `DEFAULT_ANSWER_MODE` 派生，TUI 直接引用该类型。提示词配置 `answerMode` 只记录用户的明确选择：缺省表示
  从未手动设置，读取时采用系统默认且不回写；通过 `service_save_prompt_answer_mode` 保存，并在工作台快照
  `defaultAnswerMode` 中投影。子代理会话固定为 `standard`，其回复由父模型而非用户阅读。
- **快照语义**：新会话在首次提交物化时一次性取得草稿显式选择（`sessionSettings.answerMode`），
  缺省时复制当时的全局默认。之后修改全局默认不影响任何已有会话；明确分叉继承期望并重置准备快照。
- 回答模式自元数据模式 4 起持久化；发布前版本不迁移旧会话，见
  [会话元数据版本](session-metadata-migrations.md)。

## 提示词注入

请求准备通过动态上下文键 `dynamic.html_answers`（沿用已持久化的键值）注入：互动模式注入
`answer-mode-interactive.md`；切回标准模式时，仅当该键曾经存在才注入 `answer-mode-standard.md`
停止说明。从未开启互动模式的会话不发送任何额外说明。重复同步幂等。互动提示词中的
`{{max_rows}}`、`{{max_series}}`、`{{code_max_lines}}` 与 `{{code_max_excerpts}}` 分别由
`ANSWER_COMPONENT_MAX_ROWS`、`ANSWER_CHART_MAX_SERIES`、`ANSWER_CODE_MAX_LINES`、
`ANSWER_CODE_MAX_EXCERPTS` 派生。

## 组件渲染

组件是信息串为 `ui-<name>` 的围栏代码块，名称与语法以互动提示词为准，渲染器不得接受文档外的
名称。规则：

- 只渲染已闭合的围栏；结束围栏的反引号数不少于开始围栏，容器内嵌组件需用更长的外层围栏。
  流式中尚未闭合或语法无效的围栏一律保持代码显示，不猜测修复。
- 兼容模型常见写法：不带参数的行组件（`ui-kv`、`ui-metrics`、`ui-checklist`、`ui-code` 等）名称后的
  文本作为组件标题显示，不使围栏无效；`ui-code` 的快照扫描按信息串的首个词匹配名称，因此带标题的围栏同样
  抓取。提示词不宣传该写法，WebUI、TUI 与快照扫描同步实现。
- `ui-math` 第一个空行之后若没有任何 `符号 | 含义` 行，这些行按更多公式处理（模型常用空行分隔公式）；
  图例行与非图例行混杂时整个组件无效。WebUI 与 TUI 同步实现。
- 行组件单元格中 `\|` 始终是字面竖线（提示词要求模型这样写）。兼容未转义的写法：某行单元格多于上限时，
  再拆一次，把成对反引号代码内的竖线与两侧紧贴非空白字符的竖线（如 `fetch|iex`）视为字面字符；仍不符合
  列数则整个组件按语法无效处理。两侧带空格的竖线始终分隔单元格，因此不会猜测内容属于哪一列。WebUI 与
  TUI 同步实现（TUI 保留 `\|` 交给 Markdown 显示，并在决策矩阵表格中转义字面竖线）。
- 兼容模型套用其他组件列形态的写法（提示词不宣传，WebUI 与 TUI 同步实现）：`ui-metrics` 恰有三格且第三格
  不是数字（色调词已先拆出）时，第三格读作单位、不显示进度条，数字第三格仍是最大值；最大值与单位两格可留空以对齐列
  （如 `标签 | 数值 | | | 色调`），留空即该项缺省，标签、数值或其他位置为空仍按语法无效处理；`ui-files` 的行数格可只写
  一侧（`+145` 或 `-3`），未写的一侧不显示也不计入合计，三格行的中间格不是行数时作为变更标签（如“新建”）原样显示；
  `ui-checklist` 允许第四格位置，仅当其呈路径形态（与下文文件位置规则相同，WebUI 的唯一实现是
  `answer-file-location.ts`）时接受，否则整个组件仍按语法无效处理。
- 渲染只基于回复原文，不经服务端投影，不执行任何脚本；唯一例外是 `ui-code` 的回答当时快照（见下文）。
  需要真实交互时由模型写出独立 HTML 文件并以链接交给内置浏览器打开。
- WebUI 由 `apps/vulcan-code-web/src/components/conversation/answer-components/` 原生绘制：
  `answer-component-model.ts` 是纯解析器（与 TUI 解析规则一致），`conversation-markdown.tsx` 的
  `MarkdownPreBlock` 依据 AST 源码位置确认围栏已闭合后，才把原始围栏交给延迟加载的 `AnswerFenceView` 解析
  并渲染（解析器不进入工作台启动包，语法无效时保留代码块）。流式回复中已闭合的组件即时渲染；
  Mermaid 仍只在回复完成后绘制。TUI 由 `apps/vulcan-code-tui/src/ui/answer_components.rs` 改写为
  终端可读的 Markdown（状态词转符号、图形组件转 Mermaid、树形带引导线、容器正文置于着色竖线之后）。
- 组件渲染器 `answer-component-view.tsx` 及其样式 `src/ui-kit/styles/answer.css`（数学组件另有 `answer-math.css`）延迟加载，首次遇到已闭合
  组件围栏时才取得，到达前该围栏保持代码显示；回答令牌随该样式表加载；共享外框（`vk-figure`）与表格样式在 `src/ui-kit/styles/prose.css`，随
  工作台启动加载（普通 Markdown 表格也使用）。工作台 CSS 受构建预算门禁约束，新增组件样式放入延迟样式表。
- 视觉分两类：**框架型**承载可复用数据——Markdown
  表格、`ui-findings`、`ui-compare`（悬停复制为 Markdown）、`ui-chart`（图表与数据切换、复制数据）、
  `ui-files`（标题条显示文件数与增删合计）、`ui-tree`；**融入型**随正文阅读——其余组件。普通 Markdown
  表格同样使用框架型外框，纯数字列靠右；Mermaid 使用 `base` 主题并从工作台色票取色。
- 文件位置（`ui-findings` 位置、`ui-files` 路径）仅在呈路径形态（无空白与全角标点，含分隔符或扩展名）
  且可由项目根解析时成为 `file:` 链接，行号后缀只用于显示；图库路径按 Markdown 图片的同一规则解析为本地媒体地址。
- 大图仍使用可缩放的 Mermaid。`ui-flow` 与 `ui-er` 是分层图：`answer-graph-layout.ts` 以 dagre
  （`dagre-d3-es`，与 Mermaid 同源）分层排序，`answer-diagrams.tsx` 绘制为外框内的 SVG（同名即同一节点，
  每一跳一条连线；只出现在新增或删除行上的节点带对应标记，含标记时外框显示图例）。布局后画布按节点与连线点
  一并留边，回边绕行不会被裁切。`ui-flow` 兼容 Mermaid 节点 ID：`B{OK?}` 读作形状与文字，此后单独的 `B` 指同一
  节点；单独出现过的 `名(参数)` 同样可被名字引用，否则 `load(config)` 之类保持为文字（WebUI 与 TUI 同步）。图形按自然尺寸绘制，
  随回答宽度缩小至下限后横向滚动；布局引擎与图形模块单独延迟加载，到达前围栏保持代码显示。`ui-er` 的
  引用从字段行连到被引用表，未声明的目标表使整个组件无效。`ui-sequence` 以参与者列与生命线排布消息、
  注释与阶段，单参与者注释是生命线上的紧凑方框。
- `ui-metrics` 的数值可超过最大值，此时为超限：进度条以危险色填满并标出超出百分比，未写色调时默认 `no`。
- 公式由 KaTeX 排版（`answer-math.tsx`，`trust: false`，只输出静态标记），模块、样式与字体仅在回答含公式时
  加载。`ui-math` 每行一个独立公式（使用 `\begin` 环境时整体为一个），空行后为 `符号 | 含义` 图例；行内公式
  写作 `` $`…`$ ``，由 `conversation-markdown` 在清理之后的 rehype 步骤识别，普通 `$…$` 保持文字，金额不会
  误判。KaTeX 无法解析的公式保持为代码显示。TUI 以 TeX 代码块与图例列表显示 `ui-math`，行内公式以斜体显示
  TeX 源码；`ui-er` 以树形引导线显示各表与字段。
- `ui-decision` 与组件批注是**读者回复**，不是 `ask` 工具：不阻塞回合，也不经服务端。读者的选择与批注保存在
  `ChatArea` 持有的 `AnswerReplyStore`（`answer-reply.ts`，按会话界面新建，转录行卸载后仍保留），以围栏原文为键。
  输入区通过 `connectComposer` 接入存储（`answer-reply-slot.tsx`，编辑排队消息时断开）。已作答的决策下方出现
  “插入输入框”与“直接发送”，回复条（`answer-reply-tray.tsx`）提供插入；三者都经 `deliver` 把 `formatAnswerReply`
  汇总的**全部**条目（决策列表在前、批注列表在后，说明以 `>` 引用，标题走目录）**追加到草稿**并清空存储。直接发送
  在追加后走输入区自身的 `handleComposerSend`，保留全部发送闸门与运行中排队；只有读者点击才会发送。没有接入输入区时
  决策不显示这两个按钮，`deliver` 保留条目。没有
  `AnswerReplyContext` 的界面（助手面板、详情弹窗等）中决策只读显示、组件不提供批注。框架型组件
  （findings、compare、chart、files）的批注按钮并入自身操作组（`AnswerFrameActionContext`），其他组件显示在悬停角落。
  TUI 只读显示问题与选项（建议项带星标），读者在自己的消息中作答。

## 代码摘录与回答当时快照

`ui-code` 每行引用一段真实文件行：`path:start-end | 高亮行 | 说明`（`path:42` 为单行）。模型不在行中写代码；
宿主抓取文件内容，因此回答始终显示其写作当时的代码，而不是文件的当前内容。

- **抓取时机。** 唯一一次抓取发生在 Provider V1 sink 封口 assistant 正文时
  （`provider_v1_sink/answer_code.rs`，在阻塞线程读取文件），结果与“流式 → 终态”状态变化在同一次事件修订中
  写入 `AssistantTextEvent.code_snapshots`。会话核心只在离开流式状态的那次修订接受快照，流式正文携带快照、
  已封口后替换快照都会被拒绝；加载时同样校验边界。快照仅用于展示，不进入 Provider 请求投影。
- **位置解析与边界。** 位置扫描与解析的唯一实现是 `answer_components::{answer_code_locations,
  parse_answer_code_location}`：只认已闭合的 `ui-code` 围栏（`ui-*` 容器内计入，其他代码块内不计入），以最后一个
  冒号分隔范围，按位置原文去重。`ANSWER_CODE_MAX_LINES` 限制单段行数（超出则围栏无效），
  `ANSWER_CODE_MAX_EXCERPTS` 同时限制单条回答抓取的不同摘录数与单个围栏的行数，`ANSWER_CODE_MAX_BYTES`
  限制单段字节数。相对路径以会话工作区（`workspace_path_for_session`）解析，绝对路径直接读取。
- **结果。** 每个快照以位置原文为键，结果为 `captured`（自首行起、以 `\n` 连接的各行，文件提前结束时较短）或
  `unavailable` 及稳定原因（`not_found`、`no_workspace`、`beyond_end`、`not_text`、`too_large`、`read_failed`），
  原因由客户端经目录本地化。没有工作区根的 sink（如子代理执行）对相对路径记录 `no_workspace`。
- **展示。** WebUI 把已封口事件的快照放入展示消息，经 `AnswerCodeSnapshotsContext` 交给 `MarkdownPreBlock`
  与组件宿主；流式中显示“抓取中”，已封口但缺少某位置时说明未抓取（超过摘录上限时说明上限）。
  高亮沿用代码块的 Shiki 管线，语言按文件扩展名推断。TUI 以带行号的代码框显示同一快照，高亮行加标记。
