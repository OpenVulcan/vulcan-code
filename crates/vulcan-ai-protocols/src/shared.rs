//! Shared wire request and protocol error types.
//! 共享 Wire 请求与协议错误类型。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use vulcan_ai_contract::{AiExecutionRequest, AiStreamEvent};

/// Rejects provider-native tool semantics for a profile that has not registered them.
/// 为尚未注册供应商原生工具语义的 Profile 拒绝此类工具。
///
/// # Parameters
/// 参数
/// `request` is the immutable public request and `profile_id` identifies the rejecting profile.
/// `request` 是不可变公共请求，`profile_id` 标识执行拒绝的 Profile。
///
/// # Returns
/// 返回值
/// Success when no native tools were requested, otherwise a controlled conversion error.
/// 未请求原生工具时返回成功，否则返回受控转换错误。
pub(crate) fn require_no_native_tools(
    request: &AiExecutionRequest,
    profile_id: &str,
) -> Result<(), ProtocolConversionError> {
    if request.native_tools.is_empty() {
        return Ok(());
    }
    Err(ProtocolConversionError::new(format!(
        "Protocol profile `{profile_id}` does not register provider-native tools."
    )))
}

/// Carries one encoded HTTP request without provider credentials.
/// 承载一个不含供应商凭据的编码 HTTP 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncodedWireRequest {
    /// Protocol-relative endpoint path.
    /// 协议相对端点路径。
    pub path: String,
    /// Protocol-required non-secret headers.
    /// 协议要求的非秘密请求头。
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Encoded JSON request body.
    /// 编码后的 JSON 请求体。
    pub body: Value,
}

/// Decodes one protocol's streamed JSON events into public events.
/// 将一种协议的流式 JSON 事件解码为公共事件。
pub trait ProtocolStreamDecoder: Send {
    /// Pushes one complete SSE data record.
    /// 推入一条完整的 SSE 数据记录。
    ///
    /// # Parameters
    /// 参数
    /// `event_name` is the optional SSE event field and `data` is the complete decoded data text.
    /// `event_name` 是可选 SSE event 字段，`data` 是完整解码后的 data 文本。
    ///
    /// # Returns
    /// 返回值
    /// Zero or more normalized events, or a deterministic protocol failure.
    /// 返回零个或多个归一化事件，或确定性协议失败。
    fn push_sse(
        &mut self,
        event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError>;

    /// Finishes the wire stream and rejects incomplete protocol state.
    /// 结束 Wire 流并拒绝不完整协议状态。
    ///
    /// # Returns
    /// 返回值
    /// Final normalized events or a truncation failure.
    /// 返回最终归一化事件或截断失败。
    fn finish(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError>;
}

/// Identifies a deterministic protocol conversion failure.
/// 标识一个确定性的协议转换失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolConversionError {
    /// Controlled English error message.
    /// 受控英文错误消息。
    pub message: String,
}

impl ProtocolConversionError {
    /// Creates one conversion error.
    /// 创建一个转换错误。
    ///
    /// # Parameters
    /// 参数
    /// `message` is the controlled English diagnostic.
    /// `message` 是受控英文诊断信息。
    ///
    /// # Returns
    /// 返回值
    /// A new conversion error.
    /// 一个新的转换错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ProtocolConversionError {
    /// Formats the controlled conversion error.
    /// 格式化受控转换错误。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message.as_str())
    }
}

impl std::error::Error for ProtocolConversionError {}
