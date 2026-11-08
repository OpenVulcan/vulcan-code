# 会话标题生成

本文是会话标题生成的运行时契约。实现位于
`crates/vulcan-service-kernel/src/session_title.rs`，提示词位于
`crates/vulcan-runtime-assets/resources/agent-prompts/session-title-generator.md`。

## 一、行为

1. 新会话的首个提交被接纳后，后台根据首条用户提示词生成标题。只处理新建会话且提示词非空；
   已有会话、只有附件的会话不生成。
2. 开关为 `vulcan-code.json` 的 `sessionTitleGenerationEnabled`，缺省视为开启，只在内存中补默认值，
   不回写。关闭时标题保持原有截断方式（提示词开头，上限 `SESSION_TITLE_MAX_CHARS`）。
3. 生成结果只在会话标题仍等于创建时的截断标题时写入（比较并设置）；期间用户手动重命名则保留
   用户标题，且不提示。
4. 失败不重试。成功或失败都发出 `ServiceEvent::SessionTitleGenerated`（线协议种类
   `sessionTitleGenerated`，WebUI 主题 `session.title.generated`），携带会话 ID、模型显示名及结果
   （`renamed` 带新标题，或 `failed`）。WebUI 与 TUI 据此弹出提示；功能关闭、无可用模型或被手动重命名
   时不发事件。

## 二、模型与思考级别

1. 路由保存在推理配置档 `session_title` 的 LLM 用途 `session_title_generation`，经
   `service_save_public_ai_inference_route` 保存；只接受 `conversation_respond` 与
   `internal_background` 用途范围，不要求结构化输出。
2. 已保存路由：按该路由执行；显式思考档位必须是模型声明的档位，未保存档位时用模型默认值。
   已保存路由无法解析时按该模型失败处理并提示，不替换为其他模型。
3. 未保存路由：使用会话本身所选的模型；模型允许时关闭思考，否则用模型默认值。
4. 标题请求以该会话 ID 作为缓存作用域（`cache_scope_id`），与会话本身的请求一致；需要会话请求头
   （如 OpenCode Go 的 `x-opencode-session`）或稳定前缀标识（Responses Lite）的供应商缺少它会直接拒绝。
5. 界面建议选择可关闭思考的模型并将思考设为关闭：候选中可关闭思考的模型排在前面并加标注，
   选择声明 `none` 的模型时默认以关闭思考保存。判断规则与 VMM 的关闭思考资格一致
   （`candidateSupportsDisabledReasoning`）。

## 三、用量与边界

1. 用量记入消费者 `vulcan-code-session-title`，用量统计插件按该 ID 显示“会话标题生成”分类；
   新增语言时同步 `plugins/vulcan-usage-statistics/locales/`。
2. 调用方预算、输出 Token 上限 `SESSION_TITLE_OUTPUT_TOKEN_LIMIT` 与执行时长
   `SESSION_TITLE_TIMEOUT` 各有唯一定义；发送给模型的提示词最多取开头
   `SESSION_TITLE_PROMPT_MAX_CHARS` 个字符。
3. 模型输出只取第一行非空文本，去掉引号、`Title:` 前缀与结尾标点，并截断到
   `SESSION_TITLE_MAX_CHARS`；结果为空按失败处理。
