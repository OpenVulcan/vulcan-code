# 原生文件读取输出契约

## 归属与更新条件

本契约描述公开 `file_read` 的输出。权威实现位于
`crates/vulcan-host-tools/src/file_runtime.rs`，适配入口位于
`crates/vulcan-agent-runtime/src/tooling/native/file_read.rs`。修改读取输出、行映射或前端读取卡时，
同步更新本文与相应测试。

## 工具边界

- `file_read` 读取；`file_write` 新建或整文件覆盖；`file_edit` 精确局部替换；`apply_patch` 应用补丁。
- `file` 仅支持 `list/stat/move/delete`；模型目录不再暴露其旧读写动作，执行层也拒绝这些动作及旧多目标参数。
- 计划模式开放 `file_read` 与 `file` 的列表、属性动作；写入、编辑和补丁不可执行。
- 所有入口复用既有文件运行时与结果投影；旧会话保留展示兼容，不修改持久化数据。

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

`file_edit.encoding` 指定源编码，编辑保留该编码。`file_write.source_encoding` 仅用于已有文件的源解码，`encoding` 独立指定目标编码；省略目标编码时保留已确认的源编码。预检确认的变更前后格式必须直接进入持久回执，不能重新自动检测后覆盖。`apply_patch` 保持现有补丁语法；遇到不确定编码时整批不写入，并提示改用支持显式编码的 `file_edit`。

### 编码参数对照

| 操作 | 未声明源编码 | 显式源编码 | 写出编码 |
| --- | --- | --- | --- |
| `file_read` | 自动检测；不确定时返回候选及警告，或拒绝无法识别的文本 | `encoding` | 不写入 |
| `file_edit` | 仅允许证据足够的自动检测结果 | `encoding` | 保持已确认的源编码 |
| `file_write` 修改已有文件 | 仅允许证据足够的自动检测结果 | `source_encoding` | `encoding` 指定目标；省略则保持源编码 |
| `file_write` 新建文件 | 没有源文件 | 不允许 `source_encoding` | `encoding` 指定目标；省略使用默认 UTF-8 |
| `apply_patch` 修改已有文件 | 仅允许证据足够的自动检测结果；不确定时整批拒绝 | 当前补丁语法不提供编码参数，应改用 `file_edit` | 保持源编码 |

`encoding_source` 表示本次解码的证据来源：`utf8_validation` 为严格 UTF-8 校验，`bom` 为字节序标记，`detector` 为编码检测器，`unicode_pattern` 为无 BOM Unicode 字节模式，`explicit` 为调用方显式选择。`encoding_certain` 表示当前证据是否允许自动写回，不是概率，也不证明调用方选择的编码就是文件历史上的原始编码。显式源编码仍须通过字节及 BOM 兼容性校验。

两种写入工具共用源编码检查。比较防护行为时，应比较 `file_edit.encoding` 与 `file_write.source_encoding`，不能把显式确认源编码的编辑和未确认源编码的整文件写入当作相同输入。

无 BOM UTF-16/32 只有在字节模式提供充分证据时自动识别；中文短样本不能保证自动判断端序。二进制拒绝信息提供显式编码线索，真正二进制仍禁止当作文本修改。`retryable=false` 表示原参数不得自动重试，不禁止确认编码后发出新请求。

文件列表与文件信息使用独立内容卡，不展示通用输入、输出标签页。工作区根目录的单独 `.` 标签省略；父目录 `..` 不省略。仓库概览工具 `vulcan_codekit_repo_map` 显示“查看仓库概览”，展开后展示原始 Markdown 源码块。

## 边界与历史兼容

- 新调用使用 `file_read`，参数不含 `action`；旧会话仍可展示。未指定 `end_line` 时，默认最多读取 `DEFAULT_READ_LINES`（当前 400 行），序列化预算为 `DEFAULT_READ_RESPONSE_BYTES`（当前 64 KiB）。
- 显式指定结束行时，仍受 `MAX_READ_LINES`（当前 10000 行）与 `MAX_READ_RESPONSE_BYTES`（当前 2 MiB）硬上限约束。各常量在 `file_runtime.rs` 唯一定义，工具帮助直接引用。
- 大小预算按 `FileToolResponse` 经 `serde_json::to_vec` 序列化后的 UTF-8 字节数计算，包含唯一 Markdown 正文、元数据、字段名和 JSON 转义开销。正文中的 `Lnn: ` 行号、围栏、标题及续读提示都计入；不是源文件字节数，也不是仅计算渲染后的正文。默认预算为 64 × 1024 字节，不包含之后其他层追加的外部消息信封。截断后使用 `next_start_line` 续读，不能重漏源行。
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
- `no_change` 提示采用真实参数名：`file_write` 使用 `content`，`file_edit` 使用 `new_string`。
- `path_not_found` 的 `requires_read` 为假；应先列出存在的父目录寻找路径。源与目标相同的移动返回 `same_path`。
- 精确替换的 `ambiguous_match` 返回 `match_count`（全部匹配数）、`candidate_count`（实际返回候选数）、`candidate_ranges`（一基 `start_line` 与 `line_count`）与 `candidates_truncated`；候选上限唯一来源是 `text_document::MAX_ERROR_CANDIDATE_RANGES`。
- 回滚失败报告 `commit_scope=unknown`、`rollback_state=failed`；执行状态不明不能宣称没有副作用。
- 工具已返回但完成回执持久化失败时，终止继续执行并禁止自动重试。已应用变更集应保留 `side_effect_observed=true` 和 `tool_result_persistence_failed`，明确告知变更已经生效，不得为了补回执重复执行。

### 常见错误码与调用方动作

下表覆盖文件工具的常见公开错误，不是全部内部诊断码的封闭枚举。

| 错误码 | 触发条件 | 调用方动作 |
| --- | --- | --- |
| `path_not_found` | 操作需要的源路径不存在 | 检查路径或列出现存父目录；不要重读不存在的路径 |
| `unsupported_file_type` | 当前操作不支持目录、链接或其他目标类型 | 用 `stat` 确认类型；目录发现使用 `list` |
| `unsupported_encoding` | 编码名称不支持、源字节不满足声明编码或声明与 BOM 冲突 | 根据编码证据修正声明；不要仅更换标签掩盖错误 |
| `encoding_ambiguous` | 修改所需源编码尚未确认 | 先确认源编码，再使用 `file_edit.encoding` 或 `file_write.source_encoding` |
| `binary_file` | 自动读取或文本修改识别到二进制特征 | 已知是无 BOM 文本时显式声明源编码；真正二进制不得按文本修改 |
| `no_change` | 请求没有产生实际字节变化 | 跳过操作；确需修改时提供不同内容 |
| `ambiguous_match` | 单次精确替换命中多处 | 检查候选行范围，增加唯一上下文；确需全部替换才指定 `replace_all` |
| `match_not_found` | 精确替换的旧内容不存在 | 重读相关范围，再生成匹配原文的请求 |
| `same_path` | 移动的源与目标是同一路径 | 跳过移动或选择不同目标 |
| `destination_exists` | 创建或移动的目标已存在 | 检查目标，选择新路径或明确采用修改操作 |
| `patch_context_not_found` | 补丁上下文无法匹配 | 重读相关文件，再生成补丁 |
| `invalid_change_set` | 宿主生成的变更回执违反 V1 契约 | 视为内部缺陷，保留诊断并报告；不要调整业务参数或重复执行绕过 |
| `output_limit_exceeded` | 元数据或单行无法放入当前读取预算 | 缩小范围；显式范围可使用较高硬预算，仍无法容纳时需其他读取方式 |
| `tool_result_persistence_failed` | 工具已经返回，但终态回执保存被拒绝；属于执行链错误 | 检查已经产生的结果，禁止为了补回执自动重放操作 |

`requires_read` 表示修正请求前是否需要重新获取相关文件内容或编码证据，不表示必须原样重试同一个读操作。`retryable=false` 禁止原请求自动重试，不禁止经确认后提交语义不同的新请求。`commit_scope` 与 `rollback_state` 只在相关错误附带时解释：`none` / `not_started` 表示未提交，`unknown` 或回滚 `failed` 必须先核对磁盘状态，不能推断无副作用。

`invalid_change_set` 的校验发生在文件提交前，必须保留生产运行时的受控拒绝，不能替换为仅调试有效的断言或进程崩溃。它和用户路径、匹配参数错误不是同一类缺陷。

### 回执失败的隔离验证

测试使用真实原生文件工具和流式执行器，在临时工作区完成文件修改后，通过 `reject_runtime_event_projection_barrier_from_event_with_reason` 拒绝 `ToolFinished` 屏障，模拟终态存储拒绝。验证磁盘结果已生效、工具只调用一次、错误保留副作用且禁止自动重试、控制器不再发送下一次模型请求。此接口是宿主已有的回执确认机制，不是新增的生产环境故障开关；测试不修改实际会话数据库的权限或内容。
