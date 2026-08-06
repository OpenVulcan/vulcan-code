//! Standard OpenAI Responses northbound HTTP contracts.
//! 标准 OpenAI Responses 北向 HTTP 契约。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Carries the supported Responses Create request surface while retaining unknown fields for explicit rejection.
/// 承载受支持的 Responses Create 请求界面，并保留未知字段以便显式拒绝。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ResponsesCreateRequest {
    /// Exact public model identity returned by `/v1/models`.
    /// `/v1/models` 返回的精确公共模型标识。
    pub model: String,
    /// Standard string or ordered Responses input item array.
    /// 标准字符串或有序 Responses 输入项目数组。
    pub input: Value,
    /// Optional high-priority instructions.
    /// 可选高优先级指令。
    #[serde(default)]
    pub instructions: Option<String>,
    /// Whether the caller requests standard SSE streaming.
    /// 调用方是否请求标准 SSE 流式响应。
    #[serde(default)]
    pub stream: bool,
    /// Optional standard streaming behavior controls used by Codex clients.
    /// Codex 客户端使用的可选标准流式行为控制。
    #[serde(default)]
    pub stream_options: Option<Value>,
    /// Unsupported server-side background execution request.
    /// 尚不支持的服务端后台执行请求。
    #[serde(default)]
    pub background: bool,
    /// Optional upstream storage preference.
    /// 可选上游存储偏好。
    #[serde(default)]
    pub store: Option<bool>,
    /// Optional standard previous-response continuation identity.
    /// 可选标准前序响应延续标识。
    #[serde(default)]
    pub previous_response_id: Option<String>,
    /// Optional maximum output token count.
    /// 可选最大输出 Token 数。
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    /// Optional standard reasoning configuration object.
    /// 可选标准 reasoning 配置对象。
    #[serde(default)]
    pub reasoning: Option<Value>,
    /// Ordered standard tool definitions.
    /// 有序标准工具定义。
    #[serde(default)]
    pub tools: Vec<Value>,
    /// Optional standard tool-choice value.
    /// 可选标准工具选择值。
    #[serde(default)]
    pub tool_choice: Option<Value>,
    /// Optional parallel tool-call switch.
    /// 可选并行工具调用开关。
    #[serde(default)]
    pub parallel_tool_calls: Option<bool>,
    /// Optional standard text output configuration.
    /// 可选标准文本输出配置。
    #[serde(default)]
    pub text: Option<Value>,
    /// Optional caller metadata forwarded only through an allowlisted projection.
    /// 仅通过允许列表投影转发的可选调用方元数据。
    #[serde(default)]
    pub metadata: Option<Map<String, Value>>,
    /// Optional response fields requested by the caller.
    /// 调用方请求的可选响应字段。
    #[serde(default)]
    pub include: Vec<String>,
    /// Optional provider service tier; unsupported tiers are rejected before dispatch.
    /// 可选的供应商服务层级；不支持的层级会在分派前被拒绝。
    #[serde(default)]
    pub service_tier: Option<String>,
    /// Optional caller cache identity, isolated by the authenticated caller before forwarding.
    /// 可选的调用方缓存身份，转发前按已认证调用方隔离。
    #[serde(default)]
    pub prompt_cache_key: Option<String>,
    /// Optional Codex transport metadata accepted only as validated string pairs.
    /// 可选的 Codex 传输元数据，仅接受经过校验的字符串键值对。
    #[serde(default)]
    pub client_metadata: Option<Map<String, Value>>,
    /// Additional standard fields not implemented by this compatibility level.
    /// 当前兼容级别尚未实现的其他标准字段。
    #[serde(flatten)]
    pub additional_fields: Map<String, Value>,
}

impl ResponsesCreateRequest {
    /// Validates the implemented “Responses Create + Streaming” compatibility level without silently ignoring fields.
    /// 校验已实现的“Responses Create + Streaming”兼容级别，且不静默忽略字段。
    pub fn validate_compatibility_level(&self) -> Result<(), ResponsesContractError> {
        if self.model.trim().is_empty() {
            return Err(ResponsesContractError::invalid_parameter(
                "model",
                "The `model` field must not be empty.",
            ));
        }
        if self.background {
            return Err(ResponsesContractError::unsupported_parameter(
                "background",
                "Background Responses are not supported by this Vulcan compatibility level.",
            ));
        }
        if self.previous_response_id.is_some() {
            return Err(ResponsesContractError::unsupported_parameter(
                "previous_response_id",
                "Northbound stored-response continuation is not implemented.",
            ));
        }
        if self.max_output_tokens == Some(0) {
            return Err(ResponsesContractError::invalid_parameter(
                "max_output_tokens",
                "The `max_output_tokens` field must be greater than zero.",
            ));
        }
        if let Some(prompt_cache_key) = self.prompt_cache_key.as_deref()
            && prompt_cache_key.trim().is_empty()
        {
            return Err(ResponsesContractError::invalid_parameter(
                "prompt_cache_key",
                "The `prompt_cache_key` field must not be empty.",
            ));
        }
        if let Some(stream_options) = self.stream_options.as_ref() {
            let object = stream_options.as_object().ok_or_else(|| {
                ResponsesContractError::invalid_parameter(
                    "stream_options",
                    "The `stream_options` field must be an object.",
                )
            })?;
            if let Some((field, _)) = object
                .iter()
                .find(|(field, _)| field.as_str() != "reasoning_summary_delivery")
            {
                return Err(ResponsesContractError::unsupported_parameter(
                    "stream_options",
                    format!("The stream option `{field}` is not supported."),
                ));
            }
            if object
                .get("reasoning_summary_delivery")
                .and_then(Value::as_str)
                != Some("sequential_cutoff")
            {
                return Err(ResponsesContractError::invalid_parameter(
                    "stream_options.reasoning_summary_delivery",
                    "The supported reasoning summary delivery mode is `sequential_cutoff`.",
                ));
            }
        }
        if let Some(client_metadata) = self.client_metadata.as_ref()
            && let Some((field, _)) = client_metadata.iter().find(|(_, value)| !value.is_string())
        {
            return Err(ResponsesContractError::invalid_parameter(
                "client_metadata",
                format!("Client metadata field `{field}` must be a string."),
            ));
        }
        if let Some((field, _)) = self.additional_fields.iter().next() {
            return Err(ResponsesContractError::unsupported_parameter(
                field,
                format!("The `{field}` field is not supported by this Vulcan compatibility level."),
            ));
        }
        if !(self.input.is_string() || self.input.is_array()) {
            return Err(ResponsesContractError::invalid_parameter(
                "input",
                "The `input` field must be a string or an array of Responses input items.",
            ));
        }
        Ok(())
    }
}

/// Carries one deterministic standard Responses request-contract error.
/// 承载一个确定性的标准 Responses 请求契约错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsesContractError {
    /// Stable OpenAI-compatible error type.
    /// 稳定的 OpenAI 兼容错误类型。
    pub error_type: String,
    /// Stable machine-readable error code.
    /// 稳定机器可读错误码。
    pub code: String,
    /// Exact request parameter when applicable.
    /// 适用时对应的精确请求参数。
    pub param: Option<String>,
    /// Controlled diagnostic message.
    /// 受控诊断消息。
    pub message: String,
}

impl ResponsesContractError {
    /// Builds one invalid-parameter error.
    /// 构建一个无效参数错误。
    pub fn invalid_parameter(param: &str, message: impl Into<String>) -> Self {
        Self {
            error_type: String::from("invalid_request_error"),
            code: String::from("invalid_parameter"),
            param: Some(param.to_string()),
            message: message.into(),
        }
    }

    /// Builds one explicitly unsupported-parameter error.
    /// 构建一个显式不支持参数错误。
    pub fn unsupported_parameter(param: &str, message: impl Into<String>) -> Self {
        Self {
            error_type: String::from("invalid_request_error"),
            code: String::from("unsupported_parameter"),
            param: Some(param.to_string()),
            message: message.into(),
        }
    }
}

/// Standard OpenAI-compatible error envelope.
/// 标准 OpenAI 兼容错误信封。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenAiErrorEnvelope {
    /// Structured error object.
    /// 结构化错误对象。
    pub error: OpenAiErrorObject,
}

/// Standard OpenAI-compatible error object.
/// 标准 OpenAI 兼容错误对象。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenAiErrorObject {
    /// Human-readable controlled message.
    /// 人类可读的受控消息。
    pub message: String,
    /// Stable error type.
    /// 稳定错误类型。
    #[serde(rename = "type")]
    pub error_type: String,
    /// Exact request parameter when applicable.
    /// 适用时对应的精确请求参数。
    pub param: Option<String>,
    /// Stable machine-readable code.
    /// 稳定机器可读错误码。
    pub code: String,
}

impl From<ResponsesContractError> for OpenAiErrorEnvelope {
    /// Converts one contract error without dropping its exact parameter.
    /// 转换一个契约错误且不丢失其精确参数。
    fn from(error: ResponsesContractError) -> Self {
        Self {
            error: OpenAiErrorObject {
                message: error.message,
                error_type: error.error_type,
                param: error.param,
                code: error.code,
            },
        }
    }
}

/// Standard `/v1/models` list response.
/// 标准 `/v1/models` 列表响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenAiModelList {
    /// Stable object kind.
    /// 稳定对象类型。
    pub object: &'static str,
    /// Consumer-authorized routable models.
    /// 消费者已授权且可路由的模型。
    pub data: Vec<OpenAiModel>,
}

/// Standard public model descriptor without internal route or credential facts.
/// 不包含内部路由或凭据事实的标准公共模型描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenAiModel {
    /// Exact public model identity.
    /// 精确公共模型标识。
    pub id: String,
    /// Stable object kind.
    /// 稳定对象类型。
    pub object: &'static str,
    /// Catalog creation time when known; zero denotes an unspecified imported value.
    /// 已知时的目录创建时间；零表示未指定的导入值。
    pub created: u64,
    /// Non-secret owner label.
    /// 非秘密所有者标签。
    pub owned_by: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verifies unsupported standard fields produce explicit parameter errors.
    /// 验证不支持的标准字段会产生显式参数错误。
    #[test]
    fn unsupported_field_is_not_silently_ignored() {
        let request: ResponsesCreateRequest = serde_json::from_value(json!({
            "model": "gpt-5",
            "input": "hello",
            "background": true
        }))
        .expect("request should deserialize");

        let error = request
            .validate_compatibility_level()
            .expect_err("background should be rejected");
        assert_eq!(error.code, "unsupported_parameter");
        assert_eq!(error.param.as_deref(), Some("background"));
    }

    /// Verifies unknown request fields remain visible to the compatibility validator.
    /// 验证未知请求字段会保留给兼容性校验器处理。
    #[test]
    fn unknown_field_is_rejected_explicitly() {
        let request: ResponsesCreateRequest = serde_json::from_value(json!({
            "model": "gpt-5",
            "input": [],
            "future_field": true
        }))
        .expect("request should deserialize");

        let error = request
            .validate_compatibility_level()
            .expect_err("unknown field should be rejected");
        assert_eq!(error.param.as_deref(), Some("future_field"));
    }

    /// Verifies the Codex Responses transport fields are retained and validated explicitly.
    /// 验证 Codex Responses 传输字段会被保留并显式校验。
    #[test]
    fn codex_transport_fields_are_accepted_without_unknown_field_fallback() {
        let request: ResponsesCreateRequest = serde_json::from_value(json!({
            "model": "gpt-5-codex",
            "input": [],
            "stream": true,
            "stream_options": {"reasoning_summary_delivery": "sequential_cutoff"},
            "store": false,
            "include": ["reasoning.encrypted_content"],
            "prompt_cache_key": "thread-1",
            "client_metadata": {"traceparent": "00-test"}
        }))
        .expect("Codex request should deserialize");

        request
            .validate_compatibility_level()
            .expect("supported Codex fields should validate");
        assert!(request.additional_fields.is_empty());
    }

    /// Verifies unsupported Codex stream-option extensions fail at their exact parameter.
    /// 验证不支持的 Codex 流选项扩展会在精确参数处失败。
    #[test]
    fn codex_stream_options_reject_unknown_fields() {
        let request: ResponsesCreateRequest = serde_json::from_value(json!({
            "model": "gpt-5-codex",
            "input": [],
            "stream_options": {
                "reasoning_summary_delivery": "sequential_cutoff",
                "future": true
            }
        }))
        .expect("request should deserialize");

        let error = request
            .validate_compatibility_level()
            .expect_err("unknown stream option should fail");
        assert_eq!(error.param.as_deref(), Some("stream_options"));
    }
}
