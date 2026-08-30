# Apply Patch 标准函数调用修复与审核

## 范围与结论

本轮按用户确认，将 `apply_patch` 的默认声明统一为标准函数调用，包括普通 Responses 与 Codex
Responses。没有修改供应商清单、用户配置、已选协议或会话文件；没有增加失败后切换协议逻辑。

## 根因证据

1. 原生工具注册 `build_apply_patch_tool` 已有封闭 JSON 对象 Schema：唯一必填字段为字符串 `patch`。
   执行器 `execute_apply_patch_tool` 与流式预览也消费这个对象。
2. `openai.rs::encode_responses_tools` 原先根据工具名，把该普通函数单独改写为 `type: custom` 的
   Lark Grammar 工具。其他工具继续使用 `function`，所以仅暴露 Apply Patch 就足以触发失败。
3. 生产 `ProviderExecutionClient::encode_request` 先构建规范 Responses 正文，再按已选协议转换。
   Chat、Anthropic 与 Gemini 转换器拒绝 custom；失败发生在 HTTP 派发之前，与模型是否返回思考无关。
4. 原先 `encode_responses_input` 又把规范 Apply Patch 调用重新编码为 `custom_tool_call`，因此仅
   修改工具列表不足以修复带历史的后续请求。
5. 原有测试分别调用 Responses 与 Chat 编码器，没有覆盖生产 Responses 基线到目标协议的转换链。
   本次先修改断言，实际复现“声明仍为 custom”和“历史仍为 custom_tool_call”两个失败，再修复实现。

补充日志明确包含 Kimi、TokenRhythm、Alibaba 的 Chat 错误与 MiniMax 的 Anthropic 错误，均为
`invalid_request / prepare / not_dispatched / retryable=false`，符合上述共享根因。

## 修改清单

| 文件 | 修改 |
| --- | --- |
| `crates/vulcan-ai-protocols/src/openai.rs` | 删除 Apply Patch 声明及规范调用的 custom 特例；标准函数参数、调用及输出共用普通工具编码；增加旧记录请求局部投影及回归测试 |
| `crates/vulcan-ai-protocols/src/openai_stream.rs` | 增加标准函数流式参数解码测试；未改变生产解码器 |
| `crates/vulcan-provider-adapters/src/execution.rs` | 增加调用生产编码入口的 9 配置回归矩阵；未改变生产适配逻辑 |
| `docs/provider-manifest-authoring.md` | 记录标准函数声明和旧续传配对边界 |
| 本审核记录 | 保存根因、验证、审核覆盖与未验证项 |

旧 custom 记录只在全量请求副本中转为 `function_call` 与 `function_call_output`，保留调用 ID 和
完整 patch 内容，不写回会话。唯一配对例外是 Codex 已存在的延续游标持有原 custom 调用：未提交
结果仍须使用原 custom 输出类型与服务端配对。该例外不生成新的 custom 工具声明。无关原生工具、
推理项及非 Apply Patch custom 历史保持原样。

## 验证结果

- 修改断言、修复前：两项 Apply Patch 测试均按预期失败。
- `rtk cargo test -p vulcan-llm-core --lib`：141 项通过。
- `rtk cargo test -p vulcan-provider-adapters apply_patch_standard_function --lib -- --test-threads=1`：通过。
  覆盖普通/Codex Responses、Chat、Anthropic 三种配置、Gemini GenerateContent、Interactions、Code Assist；
  各自验证首次请求、规范历史和旧 custom 历史，共 27 组编码场景。
- `rtk cargo test -p vulcan-agent-runtime apply_patch --lib -- --test-threads=1`：5 项通过，包含参数错误和流式预览。
- `rtk cargo test -p vulcan-provider-adapters --lib -- --test-threads=1`：640 项通过，2 项资源索引测试失败，见下文。
- 定向 rustfmt 检查与 `git diff --check` 通过。
- 涉及适配器和运行时的命令显式使用 `VULCAN_CODE_OUTPUT_ROOT=D:/openvulcan/vulcan-code/output`；
  未对该目录进行修改。

## 两轮审核

### 第一轮：声明、参数与生产调用链

逐项核对原生 Schema → 公共 ToolDefinition → Responses 基线 → 目标协议转换 → 标准流式参数解码
→ 原生执行器及预览。检查工具名称、指定工具选择、参数转义、调用 ID、输出配对、原生托管工具与
其他普通工具不受影响。确认不再存在发送 Apply Patch custom 声明的生产入口。

结果：本次修复范围内无新增问题。

### 第二轮：历史、续传与修改边界

重新核对规范历史、原始历史、原始输出、规范输出、游标前缀、非 Apply Patch custom 项、错误字段
和不可变会话数据。测试覆盖中文、引号、反斜杠、换行，确认不会把 patch 再包一层 JSON；没有删除
工具能力或绕过宿主补丁校验。核对差异只有上述文件，无供应商或会话持久化变更。

结果：本次修复范围内无新增问题；连续两轮完成。

## 外部基线与未验证边界

- 两项失败为 `bundled_generation_isolates_one_provider_local_failure` 与
  `bundled_generation_ignores_unindexed_sibling_without_scanning`。源码表明它们直接读取源码资源与
  bundled-index，不调用工具编码器。当前源码 Manifest 字节与索引摘要确有不一致：部分由 CRLF
  差异造成；DeepSeek、Kimi、MiniMax、OpenRouter 在仅做内存 LF 规范化后仍不匹配。相关资源和
  索引不在本轮差异中，未改动它们或放宽校验，不能宣称适配器全套测试通过。
- 没有重启用户正在运行的桌面应用，也没有向真实供应商发起付费请求。需要重新编译并重启应用，
  才能验证新二进制在真实账号下的效果。
- UI 将部分发送前错误显示成“已派发、可重试”的诊断投影问题不属于本轮工具类型调整，未在此记录
  中宣称已经修复。历史错误卡也不会因本轮修复被改写。
