# 原生文件读取输出契约

## 归属与更新条件

本契约描述公开 `file(action="read")` 的输出。权威实现位于
`crates/vulcan-host-tools/src/file_runtime.rs`，适配入口位于
`crates/vulcan-agent-runtime/src/tooling/native/file.rs`。修改读取输出、行映射或前端读取卡时，
同步更新本文与相应测试。

## 唯一正文

其他会话工具及文件变更集的正文去重边界见 [会话工具结果契约](tool-result-presentation.md)。

- `model_output` 是 Markdown 字符串：文件标题、总行数、实际读取范围、编码、一个代码块。
- 源文件正文只出现一次。代码块每行使用 `L源行号: 原文`（例如 `L123: const value = 1;`），空行同样带编号。展示换行统一为 LF；原文件不被改写。
- 代码围栏长于正文中最长的连续反引号，避免源文件包含 Markdown 围栏时提前闭合。
- 截断时追加英文续读提示与 `start_line`；空文件或超出文件末尾时明确显示空范围。
- 模型投影直接使用字符串，不再次编码成 JSON 文本。

## 宿主元数据

`host_result.kind` 保持 `file_access`。`payload.schema` 为 `vulcan.file_read.v3`。
其载荷只保存路径、资源标识、语言、文件格式、总行数、截断状态及以下映射：

| 字段 | 含义 |
| --- | --- |
| `range.source_start_line` | 对应源文件的一基起始行 |
| `render.output_start_line` | 唯一 Markdown 正文中的一基代码起始行，与源坐标分离 |
| `range.line_count` | 代码实际行数；空读取为零 |
| `requested_range.start_line` / `end_line` | 原始请求范围；未提供结束行时保留空值 |
| `read_status` | `content`、`empty_file` 或 `range_beyond_eof` |
| `encoding_source` | 解码编码的来源；显式指定与自动识别必须区分 |
| `encoding_certain` | 当前证据是否允许自动写回；不表示经过标定的概率 |

不得在宿主载荷增加 `content`、`lines[].content`、`segments[].content` 等正文副本。
工具终态的持久化封装保留唯一 `model_output` 和不含正文的 `host_result`。
前端读取卡直接在 Markdown 源码块中展示同一正文，保留模型看到的行号，不提供输入、输出标签页。历史代码行适配器按映射定位代码行，校验并仅移除与源行号精确匹配的展示前缀；原文自身包含类似前缀时保留原文。无需从磁盘重读文件。
延迟详情加载也必须使用同一详情记录中的正文与元数据，不跨事件拼接。

读取空文件保留成功状态，但正文明确标注 `[empty_file]` 及文件存在、没有文本的说明，卡片显示“空文件”并允许展开。非空文件的越界读取使用 `[range_beyond_eof]`，并提供同名结构化 `read_status`，保留原始请求起始行，不夹到文件末尾。文件访问失败显示“失败”，默认收起，点击后显示同一 Markdown 错误正文。

显式 `encoding` 是调用方选择的解码方式，不是编码探测结论。正文与元数据报告其来源为 `explicit`；字节无效或与 BOM 冲突时失败。ASCII 同时兼容多种编码，不能据此声称声明错误，也不能根据启发式检测覆盖用户声明。

短传统编码样本即使可解码、可往返且重复检测结果一致，也不能证明其原编码。没有独立检查点收敛证据时，读取仍可返回候选解码，但必须附 `[encoding_unverified]` 和 `encoding_certain=false`。不得自动以该候选编码执行写入、精确编辑或补丁；应显式确认源编码。

`file_edit.encoding` 指定源编码，编辑保留该编码。`file.write.source_encoding` 仅用于已有文件的源解码，`encoding` 独立指定目标编码；省略目标编码时保留已确认的源编码。预检确认的变更前后格式必须直接进入持久回执，不能重新自动检测后覆盖。`apply_patch` 保持现有补丁语法；遇到不确定编码时整批不写入，并提示改用支持显式编码的 `file_edit`。

无 BOM UTF-16/32 只有在字节模式提供充分证据时自动识别；中文短样本不能保证自动判断端序。二进制拒绝信息提供显式编码线索，真正二进制仍禁止当作文本修改。`retryable=false` 表示原参数不得自动重试，不禁止确认编码后发出新请求。

文件列表与文件信息使用独立内容卡，不展示通用输入、输出标签页。工作区根目录的单独 `.` 标签省略；父目录 `..` 不省略。仓库概览工具 `vulcan_codekit_repo_map` 显示“查看仓库概览”，展开后展示原始 Markdown 源码块。

## 边界与历史兼容

- 请求参数保持兼容。未指定 `end_line` 时，默认最多读取 `DEFAULT_READ_LINES`（当前 400 行），序列化预算为 `DEFAULT_READ_RESPONSE_BYTES`（当前 64 KiB）。
- 显式指定结束行时，仍受 `MAX_READ_LINES`（当前 10000 行）与 `MAX_READ_RESPONSE_BYTES`（当前 2 MiB）硬上限约束。各常量在 `file_runtime.rs` 唯一定义，工具帮助直接引用。
- 大小预算同时计算唯一正文和元数据的序列化成本；截断后使用 `next_start_line` 续读，不能重漏源行。
- 仅截取完整源文件行；单行无法放入预算时显式失败，避免返回无法推进的续读位置。
- 前端保留历史无编号 `vulcan.file_read.v1`、带编号但映射位于 `range` 的 `vulcan.file_read.v2`、`vulcan.file_access.v2` 和旧多文件结果读取能力；不批量改写历史会话。
- 新格式的映射无效时明确显示映射错误，不猜测替代字段或将标题当作源文件内容。

## 验证

- 宿主测试覆盖实际行号、空范围、源文件空行、Unicode、围栏、换行格式及序列化大小截断。
- 原生工具测试串联调用、规范化和持久化，断言独特源文件标记在最终序列化载荷中只出现一次。
- 前端测试覆盖直接展示、延迟加载、空读取、无效映射与历史记录兼容。

## 文件修改与错误回执

- 创建、删除和文本修改回执只报告路径、动作、实际变更集的增删行数；精确替换另报告替换次数。正文仍只保存于原有变更集，不复制到模型回执。
- 差异裁剪比较完整逻辑行及其终止符。仅增加或删除末尾换行时，保留最后一行的替换 hunk，变更前后 `final_newline` 记录实际差别；不能裁成空 hunk，也不能把真实字节变化误报为 `no_change`。
- 公开错误诊断统一展开到单层：`code`、`message`、`hint`、`requires_read`、`retryable` 与动作相关诊断并列；不再嵌套 `details.details`，也不输出重复的 `requires_reread`、`recovery_hint`。
- `no_change` 提示采用真实参数名：`file` 使用 `content`，`file_edit` 使用 `new_string`。
- `path_not_found` 的 `requires_read` 为假；应先列出存在的父目录寻找路径。源与目标相同的移动返回 `same_path`。
- 精确替换的 `ambiguous_match` 返回 `match_count`（全部匹配数）、`candidate_count`（实际返回候选数）、`candidate_ranges`（一基 `start_line` 与 `line_count`）与 `candidates_truncated`；候选上限唯一来源是 `text_document::MAX_ERROR_CANDIDATE_RANGES`。
- 回滚失败报告 `commit_scope=unknown`、`rollback_state=failed`；执行状态不明不能宣称没有副作用。
- 工具已返回但完成回执持久化失败时，终止继续执行并禁止自动重试。已应用变更集应保留 `side_effect_observed=true` 和 `tool_result_persistence_failed`，明确告知变更已经生效，不得为了补回执重复执行。
