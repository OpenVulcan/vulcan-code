//! Host-owned Gemini native protocol models used by the direct Google Gemini transport.
//! 供直连 Google Gemini 传输复用的宿主自有 Gemini 原生协议模型。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Stable provider-state namespace used to persist Gemini-native assistant content for future history replays.
/// 用于持久化 Gemini 原生助手内容并服务后续历史回放的稳定 Provider 状态命名空间。
pub(crate) const GEMINI_PROVIDER_STATE_KEY: &str = "googleGemini";

/// Gemini-native content payload composed of one ordered part list and one optional producer role.
/// 由有序 Part 列表与可选生产者角色组成的 Gemini 原生内容载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiContent {
    /// Ordered Gemini parts that must be replayed exactly when the provider requires native history circulation.
    /// 当 Provider 需要原生历史回流时必须原样回放的有序 Gemini Part 集合。
    pub(crate) parts: Vec<GeminiPart>,
    /// Optional Gemini role string such as `user` or `model`.
    /// 可选的 Gemini 角色字符串，例如 `user` 或 `model`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) role: Option<String>,
}

impl GeminiContent {
    /// Builds one Gemini user content wrapper from the supplied part list.
    /// 使用给定 Part 列表构建一条 Gemini 用户内容包装对象。
    pub(crate) fn user(parts: Vec<GeminiPart>) -> Self {
        Self {
            parts,
            role: Some(String::from("user")),
        }
    }

    /// Builds one Gemini model content wrapper from the supplied part list.
    /// 使用给定 Part 列表构建一条 Gemini 模型内容包装对象。
    pub(crate) fn model(parts: Vec<GeminiPart>) -> Self {
        Self {
            parts,
            role: Some(String::from("model")),
        }
    }
}

/// One Gemini-native content part preserved for direct transport and provider-state replay.
/// 为直连传输与 Provider 状态回放保留的一条 Gemini 原生内容 Part。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiPart {
    /// Optional plain-text payload.
    /// 可选的纯文本载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
    /// Optional Gemini-native function-call payload.
    /// 可选的 Gemini 原生函数调用载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) function_call: Option<GeminiFunctionCall>,
    /// Optional Gemini-native function-response payload.
    /// 可选的 Gemini 原生函数结果载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) function_response: Option<GeminiFunctionResponse>,
    /// Optional flag marking the part as one thinking/thought fragment.
    /// 将该 Part 标记为思考片段的可选布尔值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) thought: Option<bool>,
    /// Optional Gemini thought signature that must be replayed inside the original part.
    /// 必须在原始 Part 中回放的可选 Gemini thought signature。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) thought_signature: Option<String>,
    /// Any additional native Gemini fields that are not yet modeled explicitly but still must survive replay.
    /// 尚未显式建模但仍必须在回放中保留的其他 Gemini 原生字段。
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) additional_fields: BTreeMap<String, Value>,
}

impl GeminiPart {
    /// Builds one visible plain-text Gemini part.
    /// 构建一条可见纯文本 Gemini Part。
    pub(crate) fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            thought: Some(false),
            ..Self::default()
        }
    }

    /// Builds one Gemini reasoning/thought part that may optionally carry a replay signature.
    /// 构建一条可选携带回放签名的 Gemini 思考 Part。
    #[cfg(test)]
    pub(crate) fn reasoning(text: impl Into<String>, thought_signature: Option<String>) -> Self {
        Self {
            text: Some(text.into()),
            thought: Some(true),
            thought_signature,
            ..Self::default()
        }
    }

    /// Builds one Gemini-native function-call part.
    /// 构建一条 Gemini 原生函数调用 Part。
    pub(crate) fn function_call(function_call: GeminiFunctionCall) -> Self {
        Self {
            function_call: Some(function_call),
            thought: Some(false),
            ..Self::default()
        }
    }

    /// Builds one Gemini-native function-response part.
    /// 构建一条 Gemini 原生函数结果 Part。
    pub(crate) fn function_response(function_response: GeminiFunctionResponse) -> Self {
        Self {
            function_response: Some(function_response),
            thought: Some(false),
            ..Self::default()
        }
    }

    /// Returns whether this part is one visible reasoning fragment.
    /// 返回当前 Part 是否是一条可见思考片段。
    pub(crate) fn is_reasoning(&self) -> bool {
        self.thought == Some(true) && self.text.as_ref().is_some_and(|text| !text.is_empty())
    }
}

/// Gemini-native function-call payload emitted by one model turn.
/// 单次模型回合发出的 Gemini 原生函数调用载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiFunctionCall {
    /// Optional Gemini 3 call identifier that should be mirrored back in the matching function response.
    /// 应在对应函数结果中镜像回传的可选 Gemini 3 调用标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<String>,
    /// Stable function name selected by the model.
    /// 模型选中的稳定函数名称。
    pub(crate) name: String,
    /// Structured JSON argument object selected by the model.
    /// 模型选中的结构化 JSON 参数对象。
    pub(crate) args: Value,
}

/// Gemini-native function-response payload sent back after one host-side tool execution.
/// 宿主完成一次工具执行后回传给 Gemini 的原生函数结果载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiFunctionResponse {
    /// Optional function-call identifier mirrored from the originating Gemini function call.
    /// 从原始 Gemini 函数调用镜像回传的可选调用标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<String>,
    /// Stable function name that matches the originating call.
    /// 与原始调用保持一致的稳定函数名称。
    pub(crate) name: String,
    /// Structured JSON result payload returned by the host.
    /// 宿主返回的结构化 JSON 结果载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) response: Option<Value>,
}

/// Gemini-native tool declaration bundle passed inside one `tools` array entry.
/// 作为 `tools` 数组项传入的 Gemini 原生工具声明集合。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiTool {
    /// Function declarations visible to the model in the current request.
    /// 在当前请求中暴露给模型的函数声明集合。
    pub(crate) function_declarations: Vec<GeminiFunctionDeclaration>,
}

/// Gemini-native function declaration mirrored from one host tool definition.
/// 从单条宿主工具定义镜像而来的 Gemini 原生函数声明。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiFunctionDeclaration {
    /// Stable function name exposed to the model.
    /// 暴露给模型的稳定函数名称。
    pub(crate) name: String,
    /// Human-readable function description exposed to the model.
    /// 暴露给模型的人类可读函数描述。
    pub(crate) description: String,
    /// JSON Schema parameters visible to the model.
    /// 暴露给模型的 JSON Schema 参数定义。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parameters: Option<Value>,
}

/// Minimal Gemini-native tool-config wrapper used by host-owned direct function calling.
/// 供宿主自有直连函数调用使用的最小 Gemini 原生工具配置包装对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiToolConfig {
    /// Function-calling configuration for the current tool set.
    /// 当前工具集合对应的函数调用配置。
    pub(crate) function_calling_config: GeminiFunctionCallingConfig,
}

/// Minimal Gemini-native function-calling config used by the current direct transport.
/// 当前直连传输使用的最小 Gemini 原生函数调用配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiFunctionCallingConfig {
    /// Native Gemini function-calling mode such as `AUTO`, `ANY`, or `NONE`.
    /// 原生 Gemini 函数调用模式，例如 `AUTO`、`ANY` 或 `NONE`。
    pub(crate) mode: String,
}

/// Minimal Gemini-native streaming chunk decoded from one SSE data payload.
/// 从单条 SSE 数据载荷解码出的最小 Gemini 原生流式块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiStreamingChunk {
    /// Candidate list returned by the current streaming chunk.
    /// 当前流式块返回的候选列表。
    #[serde(default)]
    pub(crate) candidates: Vec<GeminiCandidate>,
    /// Optional upstream model version emitted by Gemini.
    /// Gemini 发出的可选上游模型版本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) model_version: Option<String>,
    /// Optional usage metadata emitted by Gemini.
    /// Gemini 发出的可选用量元数据。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) usage_metadata: Option<GeminiUsageMetadata>,
}

/// Minimal Gemini-native streaming candidate.
/// 最小 Gemini 原生流式候选对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiCandidate {
    /// Optional content payload emitted by the current candidate.
    /// 当前候选发出的可选内容载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<GeminiContent>,
    /// Optional finish reason emitted by Gemini.
    /// Gemini 发出的可选结束原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) finish_reason: Option<String>,
}

/// Minimal Gemini-native usage metadata used by the host direct transport for diagnostics.
/// 供宿主直连传输记录诊断使用的最小 Gemini 原生用量元数据。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiUsageMetadata {
    /// Prompt token count reported by Gemini.
    /// Gemini 报告的提示词 Token 数量。
    #[serde(default)]
    pub(crate) prompt_token_count: i64,
    /// Optional generated candidate token count.
    /// 可选的生成候选 Token 数量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) candidates_token_count: Option<i64>,
    /// Optional reasoning/thought token count.
    /// 可选的思考 Token 数量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) thoughts_token_count: Option<i64>,
    /// Optional total token count.
    /// 可选的总 Token 数量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) total_token_count: Option<i64>,
}

/// Wraps one Gemini-native content payload into the generic provider-message-state value stored in canonical assistant messages.
/// 将一份 Gemini 原生内容载荷包装成存入规范助手消息中的通用 provider_message_state 值。
pub(crate) fn build_gemini_provider_message_state(content: &GeminiContent) -> Value {
    serde_json::json!({
        GEMINI_PROVIDER_STATE_KEY: {
            "content": content
        }
    })
}

/// Restores one Gemini-native content payload from the generic provider-message-state value when the assistant history needs native replay fidelity.
/// 当助手历史需要原生回放精度时，从通用 provider_message_state 值中恢复 Gemini 原生内容载荷。
pub(crate) fn parse_gemini_provider_message_state(state: &Value) -> Option<GeminiContent> {
    state
        .get(GEMINI_PROVIDER_STATE_KEY)?
        .get("content")
        .cloned()
        .and_then(|value| serde_json::from_value::<GeminiContent>(value).ok())
}
