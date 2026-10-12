//! Strict wire protocol profile registry.
//! 严格 Wire 协议 Profile 注册表。

use crate::{EncodedWireRequest, ProtocolConversionError, ProtocolStreamDecoder};
use std::collections::BTreeMap;
use std::sync::Arc;
use vulcan_ai_contract::AiExecutionRequest;

/// Shared upper bound for one accumulated streamed tool argument document.
/// 单个累计流式工具参数文档的共享上限。
pub const MAX_STREAMED_TOOL_ARGUMENT_BYTES: usize = 16 * 1024 * 1024;

/// Identifies the HTTP method used by one immutable protocol profile.
/// 标识一个不可变协议 Profile 使用的 HTTP 方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolHttpMethod {
    /// HTTP POST.
    /// HTTP POST。
    Post,
}

/// Identifies the framing applied to one response byte stream.
/// 标识一条响应字节流使用的分帧方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolStreamFraming {
    /// Server-Sent Events records separated by blank lines.
    /// 由空行分隔的 Server-Sent Events 记录。
    Sse,
    /// One JSON value per LF-delimited line.
    /// 每个 LF 分隔行包含一个 JSON 值。
    JsonLines,
    /// Newline-delimited JSON with blank-line tolerance.
    /// 允许空行的换行分隔 JSON。
    Ndjson,
    /// One complete JSON document delivered at transport end.
    /// 在传输结束时交付的一份完整 JSON 文档。
    SingleJson,
}

/// Declares the transport and resource bounds for one exact protocol profile.
/// 声明一个精确协议 Profile 的传输方式与资源边界。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolTransportProfile {
    /// HTTP method.
    /// HTTP 方法。
    pub method: ProtocolHttpMethod,
    /// Request content type.
    /// 请求内容类型。
    pub request_content_type: &'static str,
    /// Accepted response media types without parameters.
    /// 允许的无参数响应媒体类型。
    pub accepted_response_content_types: &'static [&'static str],
    /// Incremental response framing.
    /// 增量响应分帧方式。
    pub framing: ProtocolStreamFraming,
    /// Maximum bytes in one decoded frame.
    /// 单个解码帧的最大字节数。
    pub max_frame_bytes: usize,
    /// Maximum unconsumed transport bytes retained by the framer.
    /// 分帧器保留的最大未消费传输字节数。
    pub max_buffer_bytes: usize,
    /// Maximum bytes accepted for one accumulated tool argument document.
    /// 单个累计工具参数文档允许的最大字节数。
    pub max_tool_arguments_bytes: usize,
    /// Maximum bytes retained from a non-success response body.
    /// 非成功响应体允许保留的最大字节数。
    pub max_error_body_bytes: usize,
    /// Maximum time allowed before response headers arrive.
    /// 等待响应头到达的最长时间。
    pub response_headers_timeout_ms: u64,
    /// Maximum idle time between two response-stream chunks.
    /// 两个响应流分片之间允许的最长空闲时间。
    pub stream_idle_timeout_ms: u64,
}

impl ProtocolTransportProfile {
    /// Builds the registered bounded SSE transport used by current streaming profiles.
    /// 构建当前流式 Profile 使用的已注册有界 SSE 传输。
    ///
    /// # Returns
    /// 返回值
    /// A POST JSON transport accepting only `text/event-stream` responses.
    /// 返回仅接受 `text/event-stream` 响应的 POST JSON 传输。
    pub const fn bounded_sse() -> Self {
        Self {
            method: ProtocolHttpMethod::Post,
            request_content_type: "application/json",
            accepted_response_content_types: &["text/event-stream"],
            framing: ProtocolStreamFraming::Sse,
            max_frame_bytes: 16 * 1024 * 1024,
            max_buffer_bytes: 32 * 1024 * 1024,
            max_tool_arguments_bytes: MAX_STREAMED_TOOL_ARGUMENT_BYTES,
            max_error_body_bytes: 1024 * 1024,
            response_headers_timeout_ms: 60_000,
            stream_idle_timeout_ms: 120_000,
        }
    }
}

/// Encodes public requests into one exact wire protocol.
/// 将公共请求编码为一个精确 Wire 协议。
pub trait ProtocolProfile: Send + Sync {
    /// Returns the stable protocol profile identifier.
    /// 返回稳定协议 Profile 标识。
    fn id(&self) -> &'static str;

    /// Returns the immutable transport declaration for this protocol.
    /// 返回当前协议的不可变传输声明。
    ///
    /// # Returns
    /// 返回值
    /// Exact method, content types, framing, and resource limits.
    /// 返回精确的方法、内容类型、分帧方式与资源上限。
    fn transport_profile(&self) -> ProtocolTransportProfile;

    /// Encodes one public request.
    /// 编码一个公共请求。
    ///
    /// # Parameters
    /// 参数
    /// `request` is the validated provider-neutral request.
    /// `request` 是已校验的供应商无关请求。
    ///
    /// # Returns
    /// 返回值
    /// An encoded request or a deterministic conversion failure.
    /// 编码后的请求或确定性转换失败。
    fn encode(
        &self,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError>;

    /// Creates one isolated stream decoder for an execution.
    /// 为一次执行创建一个隔离的流解码器。
    ///
    /// # Parameters
    /// 参数
    /// `execution_id` is the stable public execution identifier and `request`
    /// is the immutable validated request whose registered tools constrain
    /// streamed tool identity reconstruction.
    /// `execution_id` 是稳定公共执行标识，`request` 是不可变的已校验请求，
    /// 其中注册的工具用于约束流式工具身份重建。
    ///
    /// # Returns
    /// 返回值
    /// A fresh stateful decoder.
    /// 返回新的有状态解码器。
    fn create_stream_decoder(
        &self,
        execution_id: String,
        request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder>;
}

/// Stores strict protocol profile registrations.
/// 保存严格协议 Profile 注册项。
#[derive(Clone, Default)]
pub struct ProtocolRegistry {
    /// Registered profiles keyed by stable identifier.
    /// 按稳定标识索引的已注册 Profile。
    profiles: BTreeMap<String, Arc<dyn ProtocolProfile>>,
}

impl ProtocolRegistry {
    /// Builds the first-batch built-in protocol registry.
    /// 构建首批内置协议注册表。
    ///
    /// # Returns
    /// 返回值
    /// Anthropic Messages, OpenAI Chat Completions, OpenAI Responses, Gemini
    /// GenerateContent, and Cohere v2 Chat profiles.
    /// 返回 Anthropic Messages、OpenAI Chat Completions、OpenAI Responses、
    /// Gemini GenerateContent 与 Cohere v2 Chat Profile。
    pub fn first_batch() -> Result<Self, ProtocolConversionError> {
        let mut registry = Self::default();
        registry.register(Arc::new(crate::AnthropicMessagesProfile))?;
        registry.register(Arc::new(crate::AnthropicMessagesVideoProfile))?;
        registry.register(Arc::new(crate::ClaudeCodeMessagesProfile))?;
        registry.register(Arc::new(crate::OpenAiChatProfile))?;
        registry.register(Arc::new(crate::OpenAiResponsesProfile))?;
        registry.register(Arc::new(crate::OpenAiCodexResponsesProfile))?;
        registry.register(Arc::new(crate::GeminiGenerateContentProfile))?;
        registry.register(Arc::new(crate::GeminiInteractionsProfile))?;
        registry.register(Arc::new(crate::GoogleCodeAssistProfile))?;
        registry.register(Arc::new(crate::CohereChatProfile))?;
        registry.register(Arc::new(crate::TypeSafeSystemOneProfile))?;
        registry.register(Arc::new(crate::OpenVulcanNativeProfile))?;
        Ok(registry)
    }

    /// Registers one unique protocol profile.
    /// 注册一个唯一协议 Profile。
    ///
    /// # Parameters
    /// 参数
    /// `profile` is the profile implementation.
    /// `profile` 是 Profile 实现。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when unique, otherwise a duplicate-registration error.
    /// 唯一时返回 `Ok(())`，否则返回重复注册错误。
    pub fn register(
        &mut self,
        profile: Arc<dyn ProtocolProfile>,
    ) -> Result<(), ProtocolConversionError> {
        let profile_id = profile.id().to_string();
        if self.profiles.contains_key(profile_id.as_str()) {
            return Err(ProtocolConversionError::new(format!(
                "Protocol profile `{profile_id}` is already registered."
            )));
        }
        self.profiles.insert(profile_id, profile);
        Ok(())
    }

    /// Resolves one exact protocol profile.
    /// 解析一个精确协议 Profile。
    ///
    /// # Parameters
    /// 参数
    /// `profile_id` is the stable identifier.
    /// `profile_id` 是稳定标识。
    ///
    /// # Returns
    /// 返回值
    /// The registered profile or a strict missing-profile error.
    /// 已注册 Profile 或严格的 Profile 缺失错误。
    pub fn require(
        &self,
        profile_id: &str,
    ) -> Result<Arc<dyn ProtocolProfile>, ProtocolConversionError> {
        self.profiles.get(profile_id).cloned().ok_or_else(|| {
            ProtocolConversionError::new(format!(
                "Protocol profile `{profile_id}` is not registered."
            ))
        })
    }
}
