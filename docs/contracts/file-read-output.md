# 原生文件读取输出契约

## 归属与更新条件

本契约描述公开 `file(action="read")` 的输出。权威实现位于
`crates/vulcan-host-tools/src/file_runtime.rs`，适配入口位于
`crates/vulcan-agent-runtime/src/tooling/native/file.rs`。修改读取输出、行映射或前端读取卡时，
同步更新本文与相应测试。

## 唯一正文

- `model_output` 是 Markdown 字符串：文件标题、总行数、实际读取范围、编码、一个代码块。
- 源文件正文只出现一次。代码块不插入行号前缀，展示换行统一为 LF；原文件不被改写。
- 代码围栏长于正文中最长的连续反引号，避免源文件包含 Markdown 围栏时提前闭合。
- 截断时追加英文续读提示与 `start_line`；空文件或超出文件末尾时明确显示空范围。
- 模型投影直接使用字符串，不再次编码成 JSON 文本。

## 宿主元数据

`host_result.kind` 保持 `file_access`。`payload.schema` 为 `vulcan.file_read.v1`。
其载荷只保存路径、资源标识、语言、文件格式、总行数、截断状态及以下映射：

| 字段 | 含义 |
| --- | --- |
| `range.source_start_line` | 对应源文件的一基起始行 |
| `range.output_start_line` | 唯一 Markdown 正文中的一基代码起始行 |
| `range.line_count` | 代码实际行数；空读取为零 |

不得在宿主载荷增加 `content`、`lines[].content`、`segments[].content` 等正文副本。
工具终态的持久化封装保留唯一 `model_output` 和不含正文的 `host_result`。
前端直接按映射生成代码行与行号，无需重新解析 Markdown 或从磁盘重读文件。
延迟详情加载也必须使用同一详情记录中的正文与元数据，不跨事件拼接。

## 边界与历史兼容

- 保持现有请求参数、默认读取范围与行数上限。
- 大小预算仍以 `MAX_READ_RESPONSE_BYTES` 为唯一来源，同时计算唯一正文和元数据的序列化成本。
- 仅截取完整源文件行；单行无法放入预算时显式失败，避免返回无法推进的续读位置。
- 前端保留历史 `vulcan.file_access.v2` 和旧多文件结果读取能力；不批量改写历史会话。
- 新格式的映射无效时明确显示映射错误，不猜测替代字段或将标题当作源文件内容。

## 验证

- 宿主测试覆盖实际行号、空范围、源文件空行、Unicode、围栏、换行格式及序列化大小截断。
- 原生工具测试串联调用、规范化和持久化，断言独特源文件标记在最终序列化载荷中只出现一次。
- 前端测试覆盖直接展示、延迟加载、空读取、无效映射与历史记录兼容。
