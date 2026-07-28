//! Strict wire protocol profile registry.
//! 严格 Wire 协议 Profile 注册表。

use crate::{EncodedWireRequest, ProtocolConversionError, ProtocolStreamDecoder};
use std::collections::BTreeMap;
use std::sync::Arc;
use vulcan_ai_contract::AiExecutionRequest;

/// Encodes public requests into one exact wire protocol.
/// 将公共请求编码为一个精确 Wire 协议。
pub trait ProtocolProfile: Send + Sync {
    /// Returns the stable protocol profile identifier.
    /// 返回稳定协议 Profile 标识。
    fn id(&self) -> &'static str;

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
