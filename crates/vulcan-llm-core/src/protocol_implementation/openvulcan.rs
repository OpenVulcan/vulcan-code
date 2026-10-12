//! Transport-only profile for versioned native OpenVulcan operations.
//! 版本化 OpenVulcan 原生操作的纯传输配置档。

use crate::{
    EncodedWireRequest, ProtocolConversionError, ProtocolHttpMethod, ProtocolProfile,
    ProtocolStreamDecoder, ProtocolStreamFraming, ProtocolTransportProfile,
};
use vulcan_ai_contract::{AiExecutionRequest, AiStreamEvent};

/// Registered native profile; request projection belongs to the provider-native executor.
/// 已注册原生配置档；请求投影属于供应商原生执行器。
pub struct OpenVulcanNativeProfile;

/// Stable identity shared by the registry and manifest engine capabilities.
/// 注册表与 Manifest 引擎能力共享的稳定身份。
pub const OPENVULCAN_NATIVE_PROFILE_ID: &str = "openvulcan-native.v1";

/// Complete native JSON frame and buffer limit; both boundaries retain the same document.
/// 完整原生 JSON 帧及缓冲限制；两个边界保留同一文档。
const NATIVE_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

impl ProtocolProfile for OpenVulcanNativeProfile {
    /// Returns the stable native protocol identifier.
    /// 返回稳定原生协议标识。
    fn id(&self) -> &'static str {
        OPENVULCAN_NATIVE_PROFILE_ID
    }
    /// Returns bounded single-document JSON transport for embedding and decision endpoints.
    /// 返回向量和判定端点使用的有界单文档 JSON 传输。
    fn transport_profile(&self) -> ProtocolTransportProfile {
        ProtocolTransportProfile {
            method: ProtocolHttpMethod::Post,
            request_content_type: "application/json",
            accepted_response_content_types: &["application/json"],
            framing: ProtocolStreamFraming::SingleJson,
            max_frame_bytes: NATIVE_DOCUMENT_BYTES,
            max_buffer_bytes: NATIVE_DOCUMENT_BYTES,
            max_tool_arguments_bytes: 0,
            max_error_body_bytes: 64 * 1024,
            response_headers_timeout_ms: 120_000,
            stream_idle_timeout_ms: 30_000,
        }
    }
    /// Rejects conversation encoding for request because this profile has no chat contract.
    /// 因本配置档没有聊天契约，拒绝 request 的对话编码。
    fn encode(
        &self,
        _request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        Err(refusal())
    }
    /// Returns an explicitly refusing decoder for every attempted conversation stream.
    /// 为任何尝试的对话流返回显式拒绝解码器。
    fn create_stream_decoder(
        &self,
        _execution_id: String,
        _request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        Box::new(NativeOnlyDecoder)
    }
}

/// Returns the controlled unsupported-conversation failure.
/// 返回受控的不支持对话错误。
fn refusal() -> ProtocolConversionError {
    ProtocolConversionError::new(
        "OpenVulcan native protocol supports embedding and decision operations only.",
    )
}
/// Decoder that never accepts conversation frames.
/// 绝不接受对话帧的解码器。
struct NativeOnlyDecoder;
impl ProtocolStreamDecoder for NativeOnlyDecoder {
    /// Rejects event_name/data because native operations return one complete JSON response.
    /// 因原生操作返回单份完整 JSON 响应，拒绝 event_name/data。
    fn push_sse(
        &mut self,
        _event_name: Option<&str>,
        _data: &str,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        Err(refusal())
    }
    /// Rejects finishing a conversation that this profile never started.
    /// 拒绝结束本配置档从未启动的对话。
    fn finish(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        Err(refusal())
    }
}
