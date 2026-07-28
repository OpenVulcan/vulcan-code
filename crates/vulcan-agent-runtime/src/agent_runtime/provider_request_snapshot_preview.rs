//! Core request-preview provider adapter used before real transport dispatch.
//! 面向真实传输分发前 core 请求预览构建的 Provider 适配器模块。

use crate::agent_runtime::provider_continuation::scope_continuation_to_active_provider;
use crate::agent_runtime::provider_request_bridge_messages::core_messages_to_provider_visible_history;
use async_trait::async_trait;
use serde_json::Value;
use vulcan_core::provider::VulcanProviderAdapter;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    CoreError as VulcanCoreError, ProviderPromptLayers as CoreProviderPromptLayers,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema,
};

/// Core request-preview provider adapter used to let `vulcan-core` own request construction before real transport execution is migrated.
/// 在真实传输执行迁移完成前，供 core 请求预览使用的 Provider 适配器，用于让 `vulcan-core` 持有请求构造权。
#[derive(Debug, Clone)]
pub(crate) struct CoreRequestPreviewProviderAdapter {
    /// Stable provider identifier used by the request-preview provider adapter.
    /// 请求预览 Provider 适配器使用的稳定 Provider 标识。
    provider_id: String,
    /// Stable model identifier used by the request-preview provider adapter.
    /// 请求预览 Provider 适配器使用的稳定模型标识。
    model_id: String,
    /// Latest outgoing prompt body attached to the snapshot request.
    /// 附着在快照请求中的最新出站提示词正文。
    prompt: String,
    /// Serialized request headers captured from the active provider blueprint.
    /// 从当前 Provider 蓝图捕获的序列化请求头。
    request_headers: Option<Value>,
    /// Tool schemas exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的工具 schema 集合。
    tool_schemas: Vec<CoreProviderRequestToolSchema>,
}

impl CoreRequestPreviewProviderAdapter {
    /// Creates one core request-preview provider adapter from already resolved provider-visible request parts.
    /// 根据已经解析完成的 Provider 可见请求组成部分创建一份 core 请求预览 Provider 适配器。
    pub(crate) fn new(
        provider_id: String,
        model_id: String,
        prompt: String,
        request_headers: Option<Value>,
        tool_schemas: Vec<CoreProviderRequestToolSchema>,
    ) -> Self {
        Self {
            provider_id,
            model_id,
            prompt,
            request_headers,
            tool_schemas,
        }
    }
}

#[async_trait]
impl VulcanProviderAdapter for CoreRequestPreviewProviderAdapter {
    type Request = CoreProviderRequestSnapshot;
    type Response = ();

    /// Returns the stable provider identifier used by the request-preview provider adapter.
    /// 返回请求预览 Provider 适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the request-preview provider adapter.
    /// 返回请求预览 Provider 适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one structured request snapshot owned by `vulcan-core`.
    /// 构建一份由 `vulcan-core` 持有的结构化请求快照。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        Ok(CoreProviderRequestSnapshot {
            provider_id: config.provider_id.clone(),
            model_id: config.model_id.clone(),
            system_prompt: config.system_prompt.clone(),
            prompt_layers: if config.prompt_layers.is_empty() {
                CoreProviderPromptLayers::from_system_prompt(config.system_prompt.clone())
            } else {
                config.prompt_layers.clone()
            },
            prompt: self.prompt.clone(),
            history: core_messages_to_provider_visible_history(&snapshot.messages),
            temperature: config.temperature,
            max_turns: config.max_turns,
            tool_schemas: self.tool_schemas.clone(),
            additional_params: config.additional_params.clone(),
            request_headers: self.request_headers.clone(),
            continuation: scope_continuation_to_active_provider(
                &snapshot.continuation,
                config.provider_id.as_str(),
                config.model_id.as_str(),
            ),
        })
    }

    /// Request-preview provider adapters do not perform real network execution and therefore always return a no-op response.
    /// 请求预览 Provider 适配器不会执行真实网络调用，因此始终返回一次空操作响应。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Ok(())
    }
}
