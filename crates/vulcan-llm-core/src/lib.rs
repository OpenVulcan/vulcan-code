//! Responses-first LLM conversion and accounting primitives.
//! 以 Responses 为首要基线的 LLM 转换与计费原语。

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
    CostBreakdown, CostCalculator, ModelPricing, UsageAccumulator, UsageDerivation,
    UsageExtractionProfile, UsageField, UsageFieldBinding, UsageObservation, UsageRecordKind,
    UsageValueEncoding, alibaba_responses_usage_profiles, deepseek_chat_usage_profiles,
    openrouter_chat_usage_profiles, standard_usage_profiles,
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
