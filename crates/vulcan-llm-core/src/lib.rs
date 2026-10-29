//! Responses-style internal semantics, exact target wire conversion and accounting primitives.
//! Responses 风格内部语义、精确目标线路转换与计费原语。

use std::collections::BTreeMap;

// Own the wire implementation inside this crate; public re-exports keep the existing API.
// 在线路核心 crate 内持有协议实现，通过公开再导出保持现有 API。
mod protocol_implementation;

pub use protocol_implementation::shared;
pub use protocol_implementation::*;

mod internal_media;
mod reasoning;
pub use internal_media::project_internal_request_for_provider;
mod media_wire_validation;
pub use media_wire_validation::validate_target_media_projection;
mod reasoning_bridge;
mod responses_plaintext_replay;
mod responses_to_anthropic;
mod responses_to_chat;
mod responses_to_gemini;
mod responses_websocket;
mod usage;

pub(crate) use reasoning_bridge::anthropic_block_from_openai_reasoning_item;
pub use reasoning_bridge::{
    decode_openai_reasoning_item, openai_reasoning_item_from_anthropic_block,
};

pub use responses_to_anthropic::responses_to_anthropic_messages;
use responses_to_anthropic::responses_to_anthropic_messages_for_route;
pub use responses_to_chat::responses_to_chat_completions;
pub use responses_to_gemini::{
    responses_to_gemini_generate_content, responses_to_gemini_interactions,
};
pub use responses_websocket::{
    DEFAULT_RESPONSES_WEBSOCKET_HEARTBEAT_INTERVAL, DEFAULT_RESPONSES_WEBSOCKET_MAX_AGE,
    RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE, ResponsesWebsocketConfig, ResponsesWebsocketFrame,
    ResponsesWebsocketRawFrame, ResponsesWebsocketSession,
};
pub use usage::{
    UsageAccumulator, UsageDerivation, UsageExtractionProfile, UsageField, UsageFieldBinding,
    UsageObservation, UsageRecordKind, UsageValueEncoding, standard_usage_profiles,
};
/// VulcanCode's own client name, sent wherever a protocol carries a client or platform label.
/// VulcanCode 自身的客户端名称，用于协议携带客户端或平台标签的位置。
pub const VULCAN_CODE_CLIENT_NAME: &str = "vulcan_code";
/// VulcanCode release version, shared by every client identity field.
/// VulcanCode 发行版本，由所有客户端身份字段共用。
pub const VULCAN_CODE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// VulcanCode's own user agent, sent to every upstream service.
/// VulcanCode 自身的用户代理，发送给所有上游服务。
pub const VULCAN_CODE_USER_AGENT: &str = concat!("vulcan-code/", env!("CARGO_PKG_VERSION"));

/// Selects the provider-side wire protocol reached from the Responses baseline.
/// 选择从 Responses 基线到达的供应商侧 Wire 协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamProtocol {
    /// Keeps the OpenAI Responses request shape.
    /// 保持 OpenAI Responses 请求形状。
    OpenAiResponses,
    /// Converts to OpenAI-compatible Chat Completions.
    /// 转换为 OpenAI 兼容 Chat Completions。
    OpenAiChatCompletions,
    /// Converts to Anthropic Messages.
    /// 转换为 Anthropic Messages。
    AnthropicMessages,
    /// Converts to Gemini GenerateContent.
    /// 转换为 Gemini GenerateContent。
    GeminiGenerateContent,
    /// Converts to the revised Gemini Interactions schema.
    /// 转换为修订版 Gemini Interactions 结构。
    GeminiInteractions,
    /// Converts to the Google Code Assist Gemini envelope.
    /// 转换为 Google Code Assist Gemini 封装。
    GoogleCodeAssist,
}

/// Selects whether Anthropic reasoning uses the native budget contract or a provider-specific projector.
/// 选择 Anthropic 推理使用原生预算契约，还是交由供应商专用投影器处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnthropicReasoningMode {
    /// Emits Anthropic-native `thinking.budget_tokens` controls and eager tool-input streaming.
    /// 输出 Anthropic 原生的 `thinking.budget_tokens` 控制字段与工具输入即时流式。
    NativeBudget,
    /// Leaves reasoning controls to the exact provider profile selected by the caller.
    /// 将推理控制字段留给调用方选定的精确供应商配置处理。
    ProviderSpecific,
}

/// Carries the exact provider facts required by the conversion core.
/// 承载转换核心所需的精确供应商事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRouteContext {
    /// Exact upstream protocol selected by the provider route.
    /// 供应商路由选中的精确上游协议。
    pub upstream_protocol: UpstreamProtocol,
    /// Exact reasoning projection contract for Anthropic-shaped routes.
    /// Anthropic 形状路由所采用的精确推理投影契约。
    pub anthropic_reasoning_mode: AnthropicReasoningMode,
    /// Exact reasoning replay contract declared by this frozen route, if any.
    /// 当前冻结路由声明的精确推理回传契约（如有）。
    ///
    /// # Remarks
    /// 说明
    /// `None` keeps every existing target converter on its current byte-exact behavior.
    /// `None` 使每个既有目标转换器保持其当前逐字节行为。
    pub reasoning_replay_contract: Option<vulcan_ai_contract::ReasoningReplayContract>,
}

/// Converts one canonical Responses body for the selected provider route.
/// 将一份规范 Responses 正文转换为选定供应商路由所需的正文。
pub fn project_responses_for_provider(
    body: serde_json::Value,
    context: &ProviderRouteContext,
) -> Result<serde_json::Value, LlmCoreError> {
    match context.upstream_protocol {
        UpstreamProtocol::OpenAiResponses => {
            validate_target_media_projection(OPENAI_RESPONSES_PROFILE_ID, &body)?;
            Ok(body)
        }
        UpstreamProtocol::OpenAiChatCompletions => responses_to_chat_completions(body),
        UpstreamProtocol::AnthropicMessages => {
            responses_to_anthropic_messages_for_route(body, context.anthropic_reasoning_mode)
        }
        UpstreamProtocol::GeminiGenerateContent => {
            responses_to_gemini_generate_content(body, false)
        }
        UpstreamProtocol::GeminiInteractions => responses_to_gemini_interactions(body),
        UpstreamProtocol::GoogleCodeAssist => responses_to_gemini_generate_content(body, true),
    }
}

/// Returns the non-secret headers required by the selected converted protocol.
/// 返回选定转换协议所需的非秘密请求头。
///
/// These declarations are kept separate from request-body encoding so a provider adapter can
/// preserve protocol transport requirements without invoking another body encoder.
/// 这些声明与请求正文编码分离，使供应商适配层能够保留协议传输要求，而无需再次调用正文编码器。
pub fn converted_protocol_headers(protocol: UpstreamProtocol) -> BTreeMap<String, String> {
    match protocol {
        UpstreamProtocol::AnthropicMessages => BTreeMap::from([(
            String::from("anthropic-version"),
            String::from("2023-06-01"),
        )]),
        UpstreamProtocol::GeminiInteractions => {
            BTreeMap::from([(String::from("api-revision"), String::from("2026-05-20"))])
        }
        _ => BTreeMap::new(),
    }
}

/// Encodes the canonical Codex Responses request through the LLM core boundary.
/// 通过 LLM 核心边界编码规范 Codex Responses 请求。
///
/// The provider adapter may still use protocol-owned transport metadata during this migration,
/// but it must not select or invoke a second request encoder for the canonical body.
/// 在迁移期间供应商适配层仍可使用协议拥有的传输元数据，但不得再为规范正文选择或调用第二个请求编码器。
pub fn encode_codex_responses_request(
    request: &vulcan_ai_contract::AiExecutionRequest,
) -> Result<EncodedWireRequest, LlmCoreError> {
    <OpenAiCodexResponsesProfile as ProtocolProfile>::encode(&OpenAiCodexResponsesProfile, request)
        .map_err(|error| {
            LlmCoreError::new(format!("Codex Responses request encoding failed: {error}"))
        })
}

/// Owns the remaining registered wire transport and decoder operations behind the LLM core boundary.
/// 将剩余的注册 Wire 传输与解码操作收口到 LLM 核心边界之后。
///
/// The registry is intentionally injected during the migration so callers retain the existing
/// profile set while provider adapters stop selecting protocol implementations directly.
/// 迁移期间仍然注入既有注册表，以保留当前 Profile 集合，同时阻止供应商适配层直接选择协议实现。
#[derive(Clone)]
pub struct LlmProtocolRuntime {
    /// Exact registered profiles used by the core-owned transport facade.
    /// 核心传输门面使用的精确注册 Profile 集合。
    registry: std::sync::Arc<ProtocolRegistry>,
}

impl LlmProtocolRuntime {
    /// Builds the immutable first-batch protocol runtime owned by the LLM core.
    /// 构建由 LLM 核心拥有的不可变首批协议运行时。
    pub fn first_batch() -> Result<Self, LlmCoreError> {
        let registry = ProtocolRegistry::first_batch()
            .map_err(|error| LlmCoreError::new(error.to_string()))?;
        Ok(Self::new(std::sync::Arc::new(registry)))
    }

    /// Creates one core-owned protocol runtime from the existing immutable registry.
    /// 使用既有不可变注册表创建一个由核心拥有的协议运行时。
    ///
    /// # Parameters
    /// 参数
    /// `registry` contains the exact profiles enabled by the host.
    /// `registry` 包含宿主启用的精确 Profile。
    ///
    /// # Returns
    /// 返回值
    /// A cloneable protocol runtime facade.
    /// 一个可克隆的协议运行时门面。
    pub fn new(registry: std::sync::Arc<ProtocolRegistry>) -> Self {
        Self { registry }
    }

    /// Resolves one exact transport profile without exposing registry selection to adapters.
    /// 解析一个精确传输 Profile，避免向适配层暴露注册表选择逻辑。
    ///
    /// # Parameters
    /// 参数
    /// `profile_id` is the route-owned stable protocol identifier.
    /// `profile_id` 是路由拥有的稳定协议标识。
    ///
    /// # Returns
    /// 返回值
    /// The registered transport limits or a core-owned resolution failure.
    /// 已注册的传输限制，或由核心拥有的解析失败。
    pub fn transport_profile(
        &self,
        profile_id: &str,
    ) -> Result<ProtocolTransportProfile, LlmCoreError> {
        self.registry
            .require(profile_id)
            .map(|profile| profile.transport_profile())
            .map_err(|error| LlmCoreError::new(error.to_string()))
    }

    /// Creates one isolated stream decoder for the exact route-owned profile.
    /// 为路由拥有的精确 Profile 创建一个隔离的流解码器。
    ///
    /// # Parameters
    /// 参数
    /// `profile_id` identifies the registered upstream response protocol, `execution_id`
    /// identifies the stream, and `request` supplies the validated tool identity context.
    /// `profile_id` 标识已注册的上游响应协议，`execution_id` 标识流，`request` 提供已校验的工具身份上下文。
    ///
    /// # Returns
    /// 返回值
    /// A fresh decoder or a core-owned missing-profile failure.
    /// 一个新的解码器，或由核心拥有的 Profile 缺失失败。
    pub fn create_stream_decoder(
        &self,
        profile_id: &str,
        execution_id: String,
        request: &vulcan_ai_contract::AiExecutionRequest,
    ) -> Result<Box<dyn ProtocolStreamDecoder>, LlmCoreError> {
        self.registry
            .require(profile_id)
            .map(|profile| profile.create_stream_decoder(execution_id, request))
            .map_err(|error| LlmCoreError::new(error.to_string()))
    }
}

/// Describes a deterministic conversion failure in the extracted LLM core.
/// 描述提取后的 LLM 核心中的确定性转换失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmCoreError {
    /// Stable human-readable diagnostic.
    /// 稳定的人类可读诊断信息。
    pub message: String,
    /// WebSocket close code when the peer supplied a close frame.
    /// 对端提供关闭帧时的 WebSocket 关闭码。
    pub websocket_close_code: Option<u16>,
    /// HTTP status of a WebSocket handshake the peer answered without upgrading.
    /// 对端未升级而直接应答 WebSocket 握手时的 HTTP 状态码。
    pub websocket_handshake_status: Option<u16>,
}

impl LlmCoreError {
    /// Creates one conversion failure from a bounded diagnostic.
    /// 使用有界诊断创建一个转换失败。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            websocket_close_code: None,
            websocket_handshake_status: None,
        }
    }

    /// Attaches one exact peer-supplied WebSocket close code.
    /// 附加一个对端提供的精确 WebSocket 关闭码。
    pub fn with_websocket_close_code(mut self, close_code: u16) -> Self {
        self.websocket_close_code = Some(close_code);
        self
    }

    /// Attaches the HTTP status of a WebSocket handshake the peer refused to upgrade.
    /// 附加对端拒绝升级的 WebSocket 握手的 HTTP 状态码。
    pub fn with_websocket_handshake_status(mut self, status: u16) -> Self {
        self.websocket_handshake_status = Some(status);
        self
    }
}

impl std::fmt::Display for LlmCoreError {
    /// Formats the conversion failure for provider diagnostics.
    /// 为供应商诊断格式化转换失败。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for LlmCoreError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deepseek_reasoning_uses_thinking_object_and_effort() {
        let context = ProviderRouteContext {
            upstream_protocol: UpstreamProtocol::OpenAiChatCompletions,
            anthropic_reasoning_mode: AnthropicReasoningMode::NativeBudget,
            reasoning_replay_contract: None,
        };

        let result = project_responses_for_provider(
            json!({
                "model": "deepseek-reasoner",
                "instructions": "Be precise.",
                "input": [{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]}],
                "reasoning": {"effort": "high"},
                "stream": true
            }),
            &context,
        )
        .expect("DeepSeek request should convert");

        assert_eq!(result["reasoning_effort"], "high");
        assert!(result.get("thinking").is_none());
        assert_eq!(result["stream_options"]["include_usage"], true);
    }

    #[test]
    fn bailian_reasoning_uses_enable_thinking_without_effort() {
        let context = ProviderRouteContext {
            upstream_protocol: UpstreamProtocol::OpenAiChatCompletions,
            anthropic_reasoning_mode: AnthropicReasoningMode::NativeBudget,
            reasoning_replay_contract: None,
        };

        let result = project_responses_for_provider(
            json!({"model":"qwen3-max","input":"hi","reasoning":{"effort":"high"}}),
            &context,
        )
        .expect("Bailian request should convert");

        assert_eq!(result["reasoning_effort"], "high");
        assert!(result.get("enable_thinking").is_none());
    }

    #[test]
    fn explicit_profile_overrides_model_name_inference() {
        let context = ProviderRouteContext {
            upstream_protocol: UpstreamProtocol::OpenAiChatCompletions,
            anthropic_reasoning_mode: AnthropicReasoningMode::NativeBudget,
            reasoning_replay_contract: None,
        };

        let result = project_responses_for_provider(
            json!({"model":"deepseek/deepseek-chat","input":"hi","reasoning":{"effort":"low"}}),
            &context,
        )
        .expect("Explicit profile should convert");

        assert_eq!(result["reasoning_effort"], "low");
        assert!(result.get("enable_thinking").is_none());
    }

    /// Verifies Chat maps developer instructions to system without losing array content or ordinary roles.
    /// 验证 Chat 将 developer 指令映射到 system，且不丢失数组文本或普通角色。
    #[test]
    fn chat_developer_instructions_preserve_order_and_text_parts() {
        // body mixes all supported instruction carriers with a user message containing literal role text.
        // body 混合全部支持的指令载体，并包含带角色字样的普通用户消息。
        let body = json!({
            "model":"test-chat", "instructions":"root",
            "input":[
                {"role":"system","content":[{"type":"input_text","text":"system-a"},{"type":"input_text","text":"system-b"}]},
                {"role":"developer","content":[{"type":"input_text","text":"developer-a"},{"type":"input_text","text":"developer-b"}]},
                {"role":"user","content":"literal developer text"},
                {"type":"message","role":"developer","content":"later policy"},
                {"type":"message","role":"assistant","content":"answer"},
                {"type":"function_call","call_id":"call-1","name":"lookup","arguments":"{}"},
                {"type":"function_call_output","call_id":"call-1","output":"done"}
            ]
        });
        // converted consumes only a request copy, never rewriting canonical roles.
        // converted 仅消费请求副本，绝不改写规范角色。
        let converted = responses_to_chat_completions(body.clone()).expect("Chat instructions");
        let messages = converted["messages"].as_array().expect("Chat messages");
        assert_eq!(
            messages[0],
            json!({"role":"system","content":"root\n\nsystem-a\nsystem-b\n\ndeveloper-a\ndeveloper-b\n\nlater policy"})
        );
        assert!(
            !messages
                .iter()
                .any(|message| message["role"] == "developer")
        );
        assert_eq!(
            messages[1],
            json!({"role":"user","content":"literal developer text"})
        );
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["tool_calls"][0]["id"], "call-1");
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["tool_call_id"], "call-1");
        assert_eq!(body["input"][1]["role"], "developer");
        assert_eq!(
            project_responses_for_provider(
                body.clone(),
                &ProviderRouteContext {
                    upstream_protocol: UpstreamProtocol::OpenAiResponses,
                    anthropic_reasoning_mode: AnthropicReasoningMode::NativeBudget,
                    reasoning_replay_contract: None,
                }
            )
            .expect("Responses role preservation"),
            body
        );
    }

    /// Verifies unsupported instruction media fails explicitly rather than disappearing during system merging.
    /// 验证不支持的指令媒体明确失败，而非在 system 合并时静默消失。
    #[test]
    fn chat_developer_instruction_media_is_not_silently_discarded() {
        for role in ["system", "developer"] {
            // body intentionally violates the text-only instruction boundary.
            // body 特意违反仅文本指令边界。
            let body = json!({"model":"test-chat","input":[{"role":role,"content":[
                {"type":"input_text","text":"keep this"},
                {"type":"input_image","image_url":"https://example.com/image.png"}
            ]}]});
            let error = responses_to_chat_completions(body)
                .expect_err("instruction image must not be dropped");
            assert!(
                error
                    .to_string()
                    .contains("system messages require text content")
            );
        }
    }

    #[test]
    fn function_call_and_output_keep_tool_identity() {
        let result = responses_to_chat_completions(json!({
            "model": "qwen3-max",
            "input": [
                {"type":"function_call","call_id":"call-1","name":"exec_command","arguments":"{\"cmd\":\"pwd\"}"},
                {"type":"function_call_output","call_id":"call-1","output":"/tmp"}
            ],
            "tools": [{"type":"function","name":"exec_command","description":"run","parameters":{"type":"object"}}]
        }))
        .expect("tool continuation should convert");

        assert_eq!(result["messages"][0]["tool_calls"][0]["id"], "call-1");
        assert_eq!(result["messages"][1]["tool_call_id"], "call-1");
        assert_eq!(result["tools"][0]["function"]["name"], "exec_command");
    }

    /// Verifies parallel Responses calls remain one Chat assistant tool-call batch.
    /// 验证并行 Responses 调用会保留为同一批 Chat assistant 工具调用。
    #[test]
    fn parallel_function_calls_share_one_chat_assistant_message() {
        let result = responses_to_chat_completions(json!({
            "model": "deepseek/deepseek-v4-flash",
            "input": [
                {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Fetching both pages."}]},
                {"type":"function_call","call_id":"call-1","name":"vulcan_curl_get","arguments":"{\"url\":\"https://example.com/a\"}"},
                {"type":"function_call","call_id":"call-2","name":"vulcan_curl_get","arguments":"{\"url\":\"https://example.com/b\"}"},
                {"type":"function_call_output","call_id":"call-1","output":"page-a"},
                {"type":"function_call_output","call_id":"call-2","output":"page-b"}
            ],
            "tools": [{"type":"function","name":"vulcan_curl_get","description":"fetch","parameters":{"type":"object"}}]
        }))
        .expect("parallel tool continuation should convert");

        let messages = result["messages"]
            .as_array()
            .expect("Chat messages should be an array");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["tool_calls"].as_array().map(Vec::len), Some(2));
        assert_eq!(messages[0]["tool_calls"][0]["id"], "call-1");
        assert_eq!(messages[0]["tool_calls"][1]["id"], "call-2");
        assert_eq!(messages[1]["tool_call_id"], "call-1");
        assert_eq!(messages[2]["tool_call_id"], "call-2");
    }

    /// Verifies Responses structured output keeps the exact Chat JSON Schema shape.
    /// 验证 Responses 结构化输出会保留精确的 Chat JSON Schema 结构。
    #[test]
    fn responses_structured_output_maps_to_chat_response_format() {
        let result = responses_to_chat_completions(json!({
            "model": "qwen3-max",
            "input": "return json",
            "text": {
                "format": {
                    "type": "json_schema",
                    "name": "answer",
                    "description": "Answer payload",
                    "schema": {"type": "object", "properties": {"ok": {"type": "boolean"}}},
                    "strict": true
                }
            }
        }))
        .expect("structured output should convert");

        assert_eq!(result["response_format"]["json_schema"]["name"], "answer");
        assert_eq!(
            result["response_format"]["json_schema"]["schema"]["properties"]["ok"]["type"],
            "boolean"
        );
        assert_eq!(result["response_format"]["json_schema"]["strict"], true);
    }

    /// Verifies generic Chat conversion never infers wire fields from a model-name prefix.
    /// 验证通用 Chat 转换绝不根据模型名前缀推断线路字段。
    #[test]
    fn responses_chat_output_limit_does_not_guess_from_model_name() {
        let result = responses_to_chat_completions(json!({
            "model": "o-explicit-profile-required",
            "input": "hello",
            "max_output_tokens": 128
        }))
        .expect("output token limit should convert");

        assert_eq!(result["max_tokens"], 128);
        assert!(result.get("max_completion_tokens").is_none());
    }

    /// Verifies Responses native search becomes the versioned Anthropic server tool.
    /// 验证 Responses 原生搜索转换为版本化 Anthropic 服务端工具。
    #[test]
    fn anthropic_conversion_preserves_native_web_search() {
        // body is the canonical Responses request produced before provider conversion.
        // body 是供应商转换前生成的规范 Responses 请求。
        let body = json!({
            "model": "deepseek-v4-flash",
            "input": "Search current information.",
            "max_output_tokens": 1024,
            "tools": [{"type": "web_search"}]
        });
        // converted is the exact Anthropic Messages wire body sent to DeepSeek.
        // converted 是发送给 DeepSeek 的精确 Anthropic Messages 线路正文。
        let converted = project_responses_for_provider(
            body,
            &ProviderRouteContext {
                upstream_protocol: UpstreamProtocol::AnthropicMessages,
                anthropic_reasoning_mode: AnthropicReasoningMode::ProviderSpecific,
                reasoning_replay_contract: None,
            },
        )
        .expect("Anthropic native search should convert");

        assert_eq!(
            converted["tools"],
            json!([{"type":"web_search_20250305","name":"web_search"}])
        );
    }

    /// Verifies only native streaming Anthropic routes ask for eager function-tool input.
    /// 验证只有原生流式 Anthropic 路由请求即时流式的函数工具输入。
    #[test]
    fn anthropic_conversion_streams_tool_input_only_on_native_streaming_routes() {
        let body = |stream: bool| {
            json!({
                "model": "claude-test",
                "input": "Patch the file.",
                "max_output_tokens": 1024,
                "stream": stream,
                "tools": [
                    {"type": "function", "name": "apply_patch", "parameters": {"type": "object"}},
                    {"type": "web_search"}
                ]
            })
        };
        let eager = |mode: AnthropicReasoningMode, stream: bool| {
            let converted = responses_to_anthropic_messages_for_route(body(stream), mode)
                .expect("Anthropic tools should convert");
            (
                converted["tools"][0].get("eager_input_streaming").cloned(),
                converted["tools"][1].get("eager_input_streaming").cloned(),
            )
        };

        assert_eq!(
            eager(AnthropicReasoningMode::NativeBudget, true),
            (Some(json!(true)), None)
        );
        assert_eq!(
            eager(AnthropicReasoningMode::NativeBudget, false),
            (None, None)
        );
        assert_eq!(
            eager(AnthropicReasoningMode::ProviderSpecific, true),
            (None, None)
        );
    }

    /// Verifies Gemini conversion preserves hosted tools and same-provider replay signatures.
    /// 验证 Gemini 转换会保留托管工具与同供应商重放签名。
    #[test]
    fn gemini_conversion_preserves_native_tools_and_replay_signatures() {
        let result = responses_to_gemini_generate_content(
            json!({
                "model": "gemini-test",
                "input": [
                    {"type":"reasoning","summary":[{"type":"summary_text","text":"think"}],"encrypted_content":"thought-signature"},
                    {"type":"function_call","call_id":"call-1","name":"lookup","arguments":"{\"q\":\"x\"}","x_vulcan_provider_signature":"call-signature"}
                ],
                "tools": [
                    {"type":"function","name":"lookup","parameters":{"type":"object"}},
                    {"type":"web_search"},
                    {"type":"code_interpreter","container":{"type":"auto"}}
                ],
                "tool_choice": "auto"
            }),
            false,
        )
        .expect("Gemini replay request should convert");

        assert_eq!(
            result["contents"][0]["parts"][0]["thoughtSignature"],
            "thought-signature"
        );
        assert_eq!(
            result["contents"][0]["parts"][1]["thoughtSignature"],
            "call-signature"
        );
        assert!(result["tools"].as_array().is_some_and(|tools| {
            tools.iter().any(|tool| tool.get("googleSearch").is_some())
                && tools.iter().any(|tool| tool.get("codeExecution").is_some())
        }));
    }

    /// Verifies the revised Interactions route is projected from the canonical Responses body.
    /// 验证修订版 Interactions 路由由规范 Responses 正文投影生成。
    #[test]
    fn responses_baseline_projects_revised_gemini_interactions() {
        let context = ProviderRouteContext {
            upstream_protocol: UpstreamProtocol::GeminiInteractions,
            anthropic_reasoning_mode: AnthropicReasoningMode::NativeBudget,
            reasoning_replay_contract: None,
        };
        let result = project_responses_for_provider(
            json!({
                "model": "gemini-interactions-test",
                "instructions": "Follow policy.",
                "input": [
                    {"type":"message","role":"user","content":[{"type":"input_text","text":"lookup"}]},
                    {"type":"reasoning","summary":[{"type":"summary_text","text":"reason"}],"encrypted_content":"interaction-signature"},
                    {"type":"function_call","call_id":"call-1","name":"lookup","arguments":"{\"q\":\"x\"}"},
                    {"type":"function_call_output","call_id":"call-1","output":{"value":"ok"}}
                ],
                "tools": [
                    {"type":"function","name":"lookup","description":"Lookup","parameters":{"type":"object"}},
                    {"type":"web_search"},
                    {"type":"code_interpreter","container":{"type":"auto"}}
                ],
                "tool_choice": "required",
                "max_output_tokens": 256,
                "reasoning": {"effort":"high"},
                "text": {"format":{"type":"json_schema","name":"answer","schema":{"type":"object"}}}
            }),
            &context,
        )
        .expect("Responses request should project to Interactions");

        assert_eq!(result["system_instruction"], "Follow policy.");
        assert_eq!(result["input"][0]["type"], "user_input");
        assert_eq!(result["input"][1]["type"], "thought");
        assert_eq!(result["input"][1]["signature"], "interaction-signature");
        assert_eq!(result["input"][2]["type"], "function_call");
        assert_eq!(result["input"][3]["type"], "function_result");
        assert_eq!(result["generation_config"]["max_output_tokens"], 256);
        assert_eq!(result["generation_config"]["thinking_level"], "high");
        assert_eq!(result["generation_config"]["tool_choice"], "any");
        assert_eq!(result["response_format"]["mime_type"], "application/json");
        assert!(result["tools"].as_array().is_some_and(|tools| {
            tools.iter().any(|tool| tool["type"] == "google_search")
                && tools.iter().any(|tool| tool["type"] == "code_execution")
        }));
    }

    /// Builds one minimal Chat-bound request carrying the supplied ordered messages.
    /// 构建一个携带给定有序消息的最小 Chat 请求。
    fn declared_chat_request(
        messages: Vec<vulcan_ai_contract::AiMessage>,
    ) -> vulcan_ai_contract::AiExecutionRequest {
        declared_chat_request_with_tools(messages, Vec::new())
    }

    /// Builds one minimal Chat-bound request that also advertises executable tools.
    /// 构建一个同时公布可执行工具的最小 Chat 请求。
    fn declared_chat_request_with_tools(
        messages: Vec<vulcan_ai_contract::AiMessage>,
        tools: Vec<vulcan_ai_contract::ToolDefinition>,
    ) -> vulcan_ai_contract::AiExecutionRequest {
        vulcan_ai_contract::AiExecutionRequest {
            operation: vulcan_ai_contract::AiOperation::ConversationRespond,
            model_id: String::from("deepseek-chat"),
            cache_scope_id: None,
            continuation: None,
            messages,
            tools,
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            stop_sequences: Vec::new(),
            prompt_cache: Default::default(),
            reasoning_effort: None,
            reasoning_output: Default::default(),
            model_parameters: std::collections::BTreeMap::new(),
            resources: Vec::new(),
            output_token_limit: None,
        }
    }

    /// Projects one minimal request down the exact declared Chat contract.
    /// 把一份最小请求沿精确已声明 Chat 契约投影。
    fn project_declared_chat(
        messages: Vec<vulcan_ai_contract::AiMessage>,
    ) -> Result<serde_json::Value, LlmCoreError> {
        project_declared_chat_with_tools(messages, Vec::new())
    }

    /// Projects one tool-carrying request down the exact declared Chat contract.
    /// 把一份携带工具的请求沿精确已声明 Chat 契约投影。
    fn project_declared_chat_with_tools(
        messages: Vec<vulcan_ai_contract::AiMessage>,
        tools: Vec<vulcan_ai_contract::ToolDefinition>,
    ) -> Result<serde_json::Value, LlmCoreError> {
        let internal =
            encode_internal_responses_request(&declared_chat_request_with_tools(messages, tools))
                .map_err(|error| LlmCoreError::new(error.to_string()))?;
        let context = ProviderRouteContext {
            upstream_protocol: UpstreamProtocol::OpenAiChatCompletions,
            anthropic_reasoning_mode: AnthropicReasoningMode::ProviderSpecific,
            reasoning_replay_contract: Some(
                vulcan_ai_contract::ReasoningReplayContract::DeepseekChatReasoningContentV1,
            ),
        };
        Ok(project_internal_request_for_provider(
            internal,
            &context,
            crate::OPENAI_CHAT_PROFILE_ID,
        )?
        .body)
    }

    /// Builds one assistant message carrying the exact durable provider evidence of one turn.
    /// 构建一条携带一个回合精确可持久化供应商证据的助手消息。
    fn durable_assistant(
        items: Vec<serde_json::Value>,
        content: Vec<vulcan_ai_contract::ContentItem>,
    ) -> vulcan_ai_contract::AiMessage {
        vulcan_ai_contract::AiMessage {
            role: vulcan_ai_contract::MessageRole::Assistant,
            content,
            provider_context_items: items,
        }
    }

    /// Verifies one declared Chat turn lands reasoning, body and tool calls on one message.
    /// 验证一个已声明 Chat 回合把推理、正文与工具调用落在同一条消息上。
    #[test]
    fn declared_chat_contract_merges_one_assistant_message_per_turn() {
        // durableItems are exactly what a V1 rebuild replays verbatim for one assistant turn.
        // durableItems 是 V1 重建为一个助手回合原样回放的内容。
        let body = project_declared_chat(vec![
            vulcan_ai_contract::AiMessage {
                role: vulcan_ai_contract::MessageRole::User,
                content: vec![vulcan_ai_contract::ContentItem::Text {
                    text: String::from("hi"),
                }],
                provider_context_items: Vec::new(),
            },
            durable_assistant(
                vec![
                    json!({
                        "type": "reasoning",
                        "content": [{"type": "reasoning_text", "text": "durable-reasoning"}]
                    }),
                    json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": "durable-answer"}]
                    }),
                    json!({
                        "type": "function_call",
                        "call_id": "call-1",
                        "name": "read_file",
                        "arguments": "{}"
                    }),
                ],
                Vec::new(),
            ),
        ])
        .expect("the declared Chat contract must project");

        let assistants = body["messages"]
            .as_array()
            .expect("Chat messages")
            .iter()
            .filter(|message| message["role"] == "assistant")
            .collect::<Vec<_>>();
        assert_eq!(
            assistants.len(),
            1,
            "one declared turn must produce exactly one assistant message: {assistants:#?}"
        );
        assert_eq!(assistants[0]["reasoning_content"], "durable-reasoning");
        assert_eq!(assistants[0]["content"], "durable-answer");
        assert_eq!(assistants[0]["tool_calls"][0]["id"], "call-1");
        assert_eq!(
            assistants[0]["tool_calls"][0]["function"]["name"],
            "read_file"
        );
    }

    /// Verifies two declared turns never collapse into one Chat assistant message.
    /// 验证两个已声明回合绝不塌缩成一条 Chat 助手消息。
    ///
    /// # Remarks
    /// 说明
    /// Provider-side adjacent-message merging makes cross-turn binding impossible to recover
    /// afterwards, so one original turn must stay exactly one Chat message.
    /// 一旦供应商把相邻消息归并，跨回合归属便无法事后恢复，
    /// 因此一个原始回合必须恰好保持一条 Chat 消息。
    #[test]
    fn declared_chat_contract_keeps_two_turns_as_two_assistant_messages() {
        let body = project_declared_chat(vec![
            vulcan_ai_contract::AiMessage {
                role: vulcan_ai_contract::MessageRole::User,
                content: vec![vulcan_ai_contract::ContentItem::Text {
                    text: String::from("hi"),
                }],
                provider_context_items: Vec::new(),
            },
            durable_assistant(
                vec![
                    json!({
                        "type": "reasoning",
                        "content": [{"type": "reasoning_text", "text": "first-reasoning"}]
                    }),
                    json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": "first-answer"}]
                    }),
                ],
                Vec::new(),
            ),
            vulcan_ai_contract::AiMessage {
                role: vulcan_ai_contract::MessageRole::User,
                content: vec![vulcan_ai_contract::ContentItem::Text {
                    text: String::from("again"),
                }],
                provider_context_items: Vec::new(),
            },
            durable_assistant(
                vec![
                    json!({
                        "type": "reasoning",
                        "content": [{"type": "reasoning_text", "text": "second-reasoning"}]
                    }),
                    json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": "second-answer"}]
                    }),
                ],
                Vec::new(),
            ),
        ])
        .expect("the declared Chat contract must project");

        let assistants = body["messages"]
            .as_array()
            .expect("Chat messages")
            .iter()
            .filter(|message| message["role"] == "assistant")
            .collect::<Vec<_>>();
        assert_eq!(
            assistants.len(),
            2,
            "two declared turns must stay two assistant messages: {assistants:#?}"
        );
        // Each turn keeps its own reasoning and body; nothing is concatenated across the boundary.
        // 每个回合保留自己的推理与正文；跨边界不发生任何拼接。
        assert_eq!(assistants[0]["reasoning_content"], "first-reasoning");
        assert_eq!(assistants[0]["content"], "first-answer");
        assert_eq!(assistants[1]["reasoning_content"], "second-reasoning");
        assert_eq!(assistants[1]["content"], "second-answer");
    }

    /// Verifies parallel tool calls of one declared turn stay one ordered tool-call batch.
    /// 验证一个已声明回合的并行工具调用保持为一个有序工具调用批次。
    #[test]
    fn declared_chat_contract_keeps_parallel_tool_calls_in_one_message() {
        let body = project_declared_chat(vec![
            vulcan_ai_contract::AiMessage {
                role: vulcan_ai_contract::MessageRole::User,
                content: vec![vulcan_ai_contract::ContentItem::Text {
                    text: String::from("read both"),
                }],
                provider_context_items: Vec::new(),
            },
            durable_assistant(
                vec![
                    json!({
                        "type": "reasoning",
                        "content": [{"type": "reasoning_text", "text": "durable-reasoning"}]
                    }),
                    json!({
                        "type": "function_call",
                        "call_id": "call-1",
                        "name": "read_file",
                        "arguments": "{\"path\":\"a\"}"
                    }),
                    json!({
                        "type": "function_call",
                        "call_id": "call-2",
                        "name": "read_file",
                        "arguments": "{\"path\":\"b\"}"
                    }),
                ],
                Vec::new(),
            ),
        ])
        .expect("the declared Chat contract must project");

        let assistants = body["messages"]
            .as_array()
            .expect("Chat messages")
            .iter()
            .filter(|message| message["role"] == "assistant")
            .collect::<Vec<_>>();
        assert_eq!(
            assistants.len(),
            1,
            "one turn stays one message no matter how many calls it made: {assistants:#?}"
        );
        let calls = assistants[0]["tool_calls"]
            .as_array()
            .expect("parallel calls stay one ordered array");
        assert_eq!(
            calls.len(),
            2,
            "both parallel calls must survive: {calls:#?}"
        );
        assert_eq!(calls[0]["id"], "call-1");
        assert_eq!(calls[1]["id"], "call-2");
        // The empty-string body is the verified form for a tool-only turn.
        // 空字符串正文是仅工具回合的已验证形态。
        assert_eq!(assistants[0]["content"], "");
        assert_eq!(assistants[0]["reasoning_content"], "durable-reasoning");
    }

    /// Verifies a tool-carrying request replays every earlier reasoned turn, tool-free included.
    /// 验证携带工具的请求回放此前每一个已推理回合，包括未调用工具的回合。
    ///
    /// # Remarks
    /// 说明
    /// DeepSeek enables reasoning by default and requires the whole prior reasoning to be returned
    /// once the request advertises tools, so a turn that answered without calling a tool must still
    /// carry its `reasoning_content` instead of being dropped as "not a tool turn".
    /// DeepSeek 默认开启推理，且一旦请求公布工具就要求回传全部先前推理，
    /// 因此没有调用工具就作答的回合仍必须携带其 `reasoning_content`，
    /// 而不能因为它「不是工具回合」被丢弃。
    #[test]
    fn declared_chat_contract_replays_every_reasoned_turn_when_tools_are_advertised() {
        let body = project_declared_chat_with_tools(
            vec![
                vulcan_ai_contract::AiMessage {
                    role: vulcan_ai_contract::MessageRole::User,
                    content: vec![vulcan_ai_contract::ContentItem::Text {
                        text: String::from("what is the cache key?"),
                    }],
                    provider_context_items: Vec::new(),
                },
                // toolFree is a fully answered turn that never called a tool.
                // toolFree 是一个完整作答、从未调用工具的回合。
                durable_assistant(
                    vec![
                        json!({
                            "type": "reasoning",
                            "content": [{"type": "reasoning_text", "text": "tool-free-reasoning"}]
                        }),
                        json!({
                            "type": "message",
                            "role": "assistant",
                            "content": [{"type": "output_text", "text": "tool-free-answer"}]
                        }),
                    ],
                    Vec::new(),
                ),
                vulcan_ai_contract::AiMessage {
                    role: vulcan_ai_contract::MessageRole::User,
                    content: vec![vulcan_ai_contract::ContentItem::Text {
                        text: String::from("now read the file"),
                    }],
                    provider_context_items: Vec::new(),
                },
                durable_assistant(
                    vec![
                        json!({
                            "type": "reasoning",
                            "content": [{"type": "reasoning_text", "text": "tool-calling-reasoning"}]
                        }),
                        json!({
                            "type": "function_call",
                            "call_id": "call-1",
                            "name": "read_file",
                            "arguments": "{}"
                        }),
                    ],
                    Vec::new(),
                ),
            ],
            vec![vulcan_ai_contract::ToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read one file"),
                input_schema: json!({"type": "object", "properties": {}}),
                input_format: Default::default(),
                strict: false,
            }],
        )
        .expect("a tool-carrying declared Chat request must project");

        // The advertised tools survive, so the reasoning requirement is exercised under them.
        // 公布的工具确实存在，因此推理要求是在带工具的前提下被检验的。
        assert_eq!(body["tools"][0]["function"]["name"], "read_file");
        let reasonings = body["messages"]
            .as_array()
            .expect("Chat messages")
            .iter()
            .filter_map(|message| message["reasoning_content"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            reasonings,
            vec!["tool-free-reasoning", "tool-calling-reasoning"],
            "every earlier reasoned turn must be replayed in order: {:#?}",
            body["messages"]
        );
        // The tool-free turn keeps its own body; the tool turn keeps the verified empty form.
        // 无工具回合保留自己的正文；工具回合保留已验证的空字符串形态。
        assert_eq!(body["messages"][1]["content"], "tool-free-answer");
        assert_eq!(body["messages"][3]["content"], "");
        assert_eq!(body["messages"][3]["tool_calls"][0]["id"], "call-1");
    }

    /// Verifies a turn with a tool call but no body keeps the verified empty-string form.
    /// 验证带工具调用但没有正文的回合保持已验证的空字符串形态。
    #[test]
    fn declared_chat_contract_uses_empty_body_for_tool_only_turn() {
        let body = project_declared_chat(vec![durable_assistant(
            vec![
                json!({
                    "type": "reasoning",
                    "content": [{"type": "reasoning_text", "text": "durable-reasoning"}]
                }),
                json!({
                    "type": "function_call",
                    "call_id": "call-1",
                    "name": "read_file",
                    "arguments": "{}"
                }),
            ],
            Vec::new(),
        )])
        .expect("the declared Chat contract must project");

        let assistant = &body["messages"][0];
        assert_eq!(assistant["role"], "assistant");
        assert_eq!(assistant["reasoning_content"], "durable-reasoning");
        assert_eq!(assistant["content"], "");
        assert_eq!(assistant["tool_calls"][0]["id"], "call-1");
    }

    /// Verifies a canonical unsigned reasoning turn still reaches Chat through the sidecar.
    /// 验证一条无签名的规范推理回合仍能通过侧车抵达 Chat。
    #[test]
    fn declared_chat_contract_replays_canonical_reasoning_without_a_signature() {
        // canonical is the exact shape the generic encoder drops because it carries no signature.
        // canonical 是通用编码器因缺少签名而丢弃的精确形态。
        let body = project_declared_chat(vec![durable_assistant(
            Vec::new(),
            vec![vulcan_ai_contract::ContentItem::Reasoning {
                text: String::from("unsigned-reasoning"),
                signature: None,
            }],
        )])
        .expect("the declared Chat contract must project");

        let assistant = &body["messages"][0];
        assert_eq!(assistant["reasoning_content"], "unsigned-reasoning");
        assert_eq!(assistant["content"], "");
        assert!(assistant.get("tool_calls").is_none());
    }

    /// Verifies every interleaving Chat cannot represent fails before dispatch instead of flattening.
    /// 验证每一种 Chat 无法表示的交错都在派发前失败，而不是被压平。
    ///
    /// # Remarks
    /// 说明
    /// Chat carries one body field beside the tool-call array, so any body placed after a call is
    /// reordered on the wire. Both shapes below must be refused, including the one where the body
    /// only appears after the call and no earlier body existed to make the split obvious.
    /// Chat 在工具调用数组之外只有一个正文字段，因此任何位于调用之后的正文都会在线路上被重排。
    /// 以下两种形态都必须被拒绝，包括正文只出现在调用之后、此前没有正文因而拆分不显眼的那种。
    #[test]
    fn declared_chat_contract_rejects_unrepresentable_interleaving() {
        // interleaved splits one turn's body around its tool call.
        // interleaved 把一个回合的正文拆在工具调用两侧。
        let interleaved = vec![
            json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "first"}]
            }),
            json!({
                "type": "function_call",
                "call_id": "call-1",
                "name": "read_file",
                "arguments": "{}"
            }),
            json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "second"}]
            }),
        ];
        // trailingBody places the only body after the call, which the earlier guard let through.
        // trailingBody 把唯一的正文放在调用之后，此前那个守卫会放行这种形态。
        let trailing_body = vec![
            json!({
                "type": "reasoning",
                "content": [{"type": "reasoning_text", "text": "durable-reasoning"}]
            }),
            json!({
                "type": "function_call",
                "call_id": "call-1",
                "name": "read_file",
                "arguments": "{}"
            }),
            json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "after-the-call"}]
            }),
        ];

        for items in [interleaved, trailing_body] {
            let outcome = project_declared_chat(vec![durable_assistant(items.clone(), Vec::new())]);
            let error =
                outcome.expect_err("an unrepresentable interleaving must fail before dispatch");
            assert!(
                error.to_string().contains("after a tool call"),
                "the failure must name the reordering for {items:#?}: {error}"
            );
        }
    }

    /// Builds one assistant message carrying only canonical content.
    /// 构建一条仅携带规范内容的助手消息。
    fn canonical_assistant(
        content: Vec<vulcan_ai_contract::ContentItem>,
    ) -> vulcan_ai_contract::AiMessage {
        vulcan_ai_contract::AiMessage {
            role: vulcan_ai_contract::MessageRole::Assistant,
            content,
            provider_context_items: Vec::new(),
        }
    }

    /// Projects one minimal request down the exact declared Anthropic contract.
    /// 把一份最小请求沿精确已声明 Anthropic 契约投影。
    fn project_declared_anthropic(
        messages: Vec<vulcan_ai_contract::AiMessage>,
    ) -> Result<serde_json::Value, LlmCoreError> {
        // Anthropic requires a positive output budget, which the shared minimal request omits.
        // Anthropic 要求正输出预算，而共用最小请求没有设置它。
        let mut request = declared_chat_request(messages);
        request.output_token_limit = Some(1_024);
        let internal = encode_internal_responses_request(&request)
            .map_err(|error| LlmCoreError::new(error.to_string()))?;
        let context = ProviderRouteContext {
            upstream_protocol: UpstreamProtocol::AnthropicMessages,
            anthropic_reasoning_mode: AnthropicReasoningMode::ProviderSpecific,
            reasoning_replay_contract: Some(
                vulcan_ai_contract::ReasoningReplayContract::DeepseekAnthropicThinkingV1,
            ),
        };
        Ok(project_internal_request_for_provider(
            internal,
            &context,
            crate::ANTHROPIC_MESSAGES_PROFILE_ID,
        )?
        .body)
    }

    /// Returns every content block of one projected Anthropic body in message order.
    /// 按消息顺序返回一份已投影 Anthropic 正文的全部内容块。
    fn anthropic_blocks(body: &serde_json::Value) -> Vec<serde_json::Value> {
        body["messages"]
            .as_array()
            .expect("Anthropic messages")
            .iter()
            .flat_map(|message| message["content"].as_array().cloned().unwrap_or_default())
            .collect()
    }

    /// Verifies the declared Anthropic route replays the original signature without any bridge.
    /// 验证已声明 Anthropic 路由回放原始签名且完全不经桥接。
    #[test]
    fn declared_anthropic_contract_replays_original_signature_without_a_bridge() {
        let body = project_declared_anthropic(vec![
            vulcan_ai_contract::AiMessage {
                role: vulcan_ai_contract::MessageRole::User,
                content: vec![vulcan_ai_contract::ContentItem::Text {
                    text: String::from("hi"),
                }],
                provider_context_items: Vec::new(),
            },
            canonical_assistant(vec![
                vulcan_ai_contract::ContentItem::Reasoning {
                    text: String::from("recorded-thinking"),
                    signature: Some(String::from("anthropic#original-payload")),
                },
                vulcan_ai_contract::ContentItem::Text {
                    text: String::from("recorded-body"),
                },
                vulcan_ai_contract::ContentItem::ToolCall {
                    call: vulcan_ai_contract::ToolCall {
                        id: String::from("call-1"),
                        name: String::from("read_file"),
                        arguments: json!({}),
                        signature: None,
                    },
                },
            ]),
        ])
        .expect("the declared Anthropic contract must project");

        let blocks = anthropic_blocks(&body);
        let kinds = blocks
            .iter()
            .filter_map(|block| block.get("type").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec!["text", "thinking", "text", "tool_use"],
            "thinking, body and tool_use must keep their original order: {blocks:#?}"
        );
        let thinking = blocks
            .iter()
            .find(|block| block["type"] == "thinking")
            .expect("the recorded thinking block must be present");
        // The signature must be the exact original payload, never a synthesized bridge envelope.
        // 签名必须是精确原始载荷，绝不是合成桥接封装。
        assert_eq!(thinking["signature"], "original-payload");
        assert_eq!(thinking["thinking"], "recorded-thinking");
        assert!(
            blocks
                .iter()
                .all(|block| block["type"] != "redacted_thinking"),
            "a declared DeepSeek route must never emit redacted_thinking: {blocks:#?}"
        );
    }

    /// Verifies recorded reasoning without a proven signature replays as text and invents nothing.
    /// 验证缺少已证实签名的已记录推理以文本回放，且不编造任何签名。
    ///
    /// # Remarks
    /// 说明
    /// Verified against the real DeepSeek endpoint: a `thinking` block without a signature is
    /// accepted, so failing the request would be an unnecessary compatibility break, while emitting
    /// an empty or foreign signature is forbidden.
    /// 已对真实 DeepSeek 端点验证：不带签名的 `thinking` 块被接受，
    /// 因此让请求失败是不必要的兼容破坏，而输出空签名或外来签名则被禁止。
    #[test]
    fn declared_anthropic_contract_replays_text_without_inventing_a_signature() {
        let unsigned = [
            None,
            Some(String::from("openai#foreign-payload")),
            Some(String::from("untagged-payload")),
        ];

        for signature in unsigned {
            let body = project_declared_anthropic(vec![canonical_assistant(vec![
                vulcan_ai_contract::ContentItem::Reasoning {
                    text: String::from("recorded-thinking"),
                    signature,
                },
            ])])
            .expect("recorded reasoning without a proven signature must still be sendable");

            let thinking = anthropic_blocks(&body)
                .into_iter()
                .find(|block| block["type"] == "thinking")
                .expect("the recorded thinking block must be present");
            assert_eq!(thinking["thinking"], "recorded-thinking");
            assert!(
                thinking.get("signature").is_none(),
                "an unproven signature must be omitted, never replaced: {thinking:#?}"
            );
        }
    }

    /// Verifies parallel tool calls keep one-to-one identity pairing with their results.
    /// 验证并行工具调用与其结果保持一一对应的身份配对。
    ///
    /// # Remarks
    /// 说明
    /// Anthropic pairs a `tool_result` to its `tool_use` by identifier, so a reordered, duplicated or
    /// dropped identifier would silently attach a result to the wrong call.
    /// Anthropic 以标识把 `tool_result` 配到对应的 `tool_use`，
    /// 因此标识一旦错序、重复或丢失，结果就会被静默挂到错误的调用上。
    #[test]
    fn declared_anthropic_contract_pairs_parallel_tool_calls_with_their_results() {
        let body = project_declared_anthropic(vec![
            vulcan_ai_contract::AiMessage {
                role: vulcan_ai_contract::MessageRole::User,
                content: vec![vulcan_ai_contract::ContentItem::Text {
                    text: String::from("read both"),
                }],
                provider_context_items: Vec::new(),
            },
            canonical_assistant(vec![
                vulcan_ai_contract::ContentItem::Reasoning {
                    text: String::from("recorded-thinking"),
                    signature: Some(String::from("anthropic#original-payload")),
                },
                vulcan_ai_contract::ContentItem::ToolCall {
                    call: vulcan_ai_contract::ToolCall {
                        id: String::from("call-1"),
                        name: String::from("read_file"),
                        arguments: json!({"path": "a"}),
                        signature: None,
                    },
                },
                vulcan_ai_contract::ContentItem::ToolCall {
                    call: vulcan_ai_contract::ToolCall {
                        id: String::from("call-2"),
                        name: String::from("read_file"),
                        arguments: json!({"path": "b"}),
                        signature: None,
                    },
                },
            ]),
            vulcan_ai_contract::AiMessage {
                role: vulcan_ai_contract::MessageRole::Tool,
                content: vec![
                    vulcan_ai_contract::ContentItem::ToolResult {
                        result: vulcan_ai_contract::ToolResult {
                            call_id: String::from("call-1"),
                            is_error: false,
                            content: json!("body-a"),
                        },
                    },
                    vulcan_ai_contract::ContentItem::ToolResult {
                        result: vulcan_ai_contract::ToolResult {
                            call_id: String::from("call-2"),
                            is_error: false,
                            content: json!("body-b"),
                        },
                    },
                ],
                provider_context_items: Vec::new(),
            },
        ])
        .expect("the declared Anthropic contract must project parallel calls with their results");

        let blocks = anthropic_blocks(&body);
        let kinds = blocks
            .iter()
            .filter_map(|block| block["type"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                "text",
                "thinking",
                "tool_use",
                "tool_use",
                "tool_result",
                "tool_result"
            ],
            "parallel calls and their results must keep their order: {blocks:#?}"
        );
        let call_ids = blocks
            .iter()
            .filter(|block| block["type"] == "tool_use")
            .map(|block| block["id"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>();
        let result_ids = blocks
            .iter()
            .filter(|block| block["type"] == "tool_result")
            .map(|block| {
                block["tool_use_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect::<Vec<_>>();
        // Both directions matter: a missing id loses a result and a duplicated id rebinds one.
        // 两个方向都要紧：标识缺失会丢结果，标识重复会重新绑定结果。
        assert_eq!(call_ids, vec!["call-1", "call-2"]);
        assert_eq!(result_ids, vec!["call-1", "call-2"]);
    }

    /// Verifies a recorded reasoning item with no text at all fails before dispatch.
    /// 验证完全不含文本的已记录推理项在派发前失败。
    #[test]
    fn declared_anthropic_contract_rejects_reasoning_without_text() {
        let outcome = project_declared_anthropic(vec![canonical_assistant(vec![
            vulcan_ai_contract::ContentItem::Reasoning {
                text: String::new(),
                signature: Some(String::from("anthropic#original-payload")),
            },
        ])]);

        let error = outcome.expect_err("reasoning with no text must fail before dispatch");
        assert!(
            error.to_string().contains("thinking"),
            "the failure must name the thinking block: {error}"
        );
    }

    /// Verifies a bare-envelope replay still reaches Anthropic through its persisted items.
    /// 验证裸封套回放仍能通过其可持久化项抵达 Anthropic。
    ///
    /// # Remarks
    /// 说明
    /// `ProviderItems` replay carries no canonical content, so the persisted items are the only
    /// authority; the original signature is replayed only when its provenance tag proves ownership.
    /// `ProviderItems` 回放不携带规范内容，因此可持久化项是唯一权威；
    /// 仅当来源标记证明归属时才回放原始签名。
    #[test]
    fn declared_anthropic_contract_replays_bare_envelope_items_without_redacting() {
        let item = json!({
            "type": "reasoning",
            "summary": [{"type": "summary_text", "text": "recorded-thinking"}],
            "encrypted_content": "anthropic#original-payload"
        });

        let body = project_declared_anthropic(vec![durable_assistant(vec![item], Vec::new())])
            .expect("a bare envelope replay must reach Anthropic");
        let thinking = anthropic_blocks(&body)
            .into_iter()
            .find(|block| block["type"] == "thinking")
            .expect("the persisted reasoning item must become a thinking block");
        assert_eq!(thinking["thinking"], "recorded-thinking");
        assert_eq!(thinking["signature"], "original-payload");

        // An untagged value is never replayed as a signature, and never becomes redacted thinking.
        // 未标记的值绝不被当作签名回放，也绝不变成 redacted thinking。
        let untagged = project_declared_anthropic(vec![durable_assistant(
            vec![json!({
                "type": "reasoning",
                "summary": [{"type": "summary_text", "text": "recorded-thinking"}],
                "encrypted_content": "untagged-opaque"
            })],
            Vec::new(),
        )])
        .expect("a persisted item without a proven signature must still be sendable");
        let blocks = anthropic_blocks(&untagged);
        assert!(
            blocks
                .iter()
                .all(|block| block["type"] != "redacted_thinking"),
            "a declared DeepSeek route must never emit redacted_thinking: {blocks:#?}"
        );
        let thinking = blocks
            .iter()
            .find(|block| block["type"] == "thinking")
            .expect("the persisted reasoning item must become a thinking block");
        assert!(thinking.get("signature").is_none());
    }

    /// Verifies a persisted reasoning item with no text fails instead of becoming a redaction.
    /// 验证不含文本的可持久化推理项失败，而不是变成 redaction。
    #[test]
    fn declared_anthropic_contract_rejects_textless_persisted_reasoning() {
        // encryptedOnly is exactly the shape the bridge would have turned into redacted_thinking.
        // encryptedOnly 正是桥接会转成 redacted_thinking 的形态。
        let outcome = project_declared_anthropic(vec![durable_assistant(
            vec![json!({"type": "reasoning", "encrypted_content": "opaque"})],
            Vec::new(),
        )]);

        assert!(
            outcome.is_err(),
            "a textless record must fail instead of becoming redacted_thinking: {outcome:#?}"
        );
    }
}
