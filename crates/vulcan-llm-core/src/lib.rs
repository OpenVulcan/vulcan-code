//! Responses-first LLM conversion and accounting primitives.
//! 以 Responses 为首要基线的 LLM 转换与计费原语。

use std::collections::BTreeMap;

// Keep the audited wire implementation private to the Responses-first core while the old
// workspace package is removed from all runtime dependency graphs.
// 将已审计的 Wire 实现私有收口到 Responses 核心，避免旧 workspace 包继续出现在运行时依赖图中。
#[path = "../../vulcan-ai-protocols/src/lib.rs"]
mod protocol_implementation;

pub use protocol_implementation::shared;
pub use protocol_implementation::*;

mod reasoning;
mod reasoning_bridge;
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
    DEFAULT_RESPONSES_WEBSOCKET_MAX_AGE, RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE,
    ResponsesWebsocketConfig, ResponsesWebsocketFrame, ResponsesWebsocketSession,
};
pub use usage::{
    UsageAccumulator, UsageDerivation, UsageExtractionProfile, UsageField, UsageFieldBinding,
    UsageObservation, UsageRecordKind, UsageValueEncoding, standard_usage_profiles,
};
#[cfg(any(test, feature = "provider-migration-oracles"))]
pub use usage::{
    alibaba_responses_usage_profiles, deepseek_chat_usage_profiles, openrouter_chat_usage_profiles,
};
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
    /// Emits Anthropic-native `thinking.budget_tokens` controls.
    /// 输出 Anthropic 原生的 `thinking.budget_tokens` 控制字段。
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
}

/// Converts one canonical Responses body for the selected provider route.
/// 将一份规范 Responses 正文转换为选定供应商路由所需的正文。
pub fn project_responses_for_provider(
    body: serde_json::Value,
    context: &ProviderRouteContext,
) -> Result<serde_json::Value, LlmCoreError> {
    match context.upstream_protocol {
        UpstreamProtocol::OpenAiResponses => Ok(body),
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
}

impl LlmCoreError {
    /// Creates one conversion failure from a bounded diagnostic.
    /// 使用有界诊断创建一个转换失败。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            websocket_close_code: None,
        }
    }

    /// Attaches one exact peer-supplied WebSocket close code.
    /// 附加一个对端提供的精确 WebSocket 关闭码。
    pub fn with_websocket_close_code(mut self, close_code: u16) -> Self {
        self.websocket_close_code = Some(close_code);
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
        };

        let result = project_responses_for_provider(
            json!({"model":"deepseek/deepseek-chat","input":"hi","reasoning":{"effort":"low"}}),
            &context,
        )
        .expect("Explicit profile should convert");

        assert_eq!(result["reasoning_effort"], "low");
        assert!(result.get("enable_thinking").is_none());
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
            },
        )
        .expect("Anthropic native search should convert");

        assert_eq!(
            converted["tools"],
            json!([{"type":"web_search_20250305","name":"web_search"}])
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
}
