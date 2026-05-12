//! Shared provider transport entry-input preparation helpers reused by provider-specific stream entrypoints.
//! 供各 Provider 专用流式入口复用的共享传输入口输入准备辅助模块。

use reqwest::header::HeaderMap;
use serde_json::Value;
use vulcan_core::types::ControllerConfig as CoreControllerConfig;

/// Prepared host-controlled stream inputs shared by provider-specific entrypoints before they build the concrete adapter.
/// 在各 Provider 专用入口构建具体适配器之前复用的宿主控制流输入对象。
pub(crate) struct PreparedProviderTransportEntryInputs {
    /// Stable provider identifier reused by controller config and transport adapters.
    /// 被控制器配置与传输适配器复用的稳定 Provider 标识。
    pub(crate) provider_id: String,
    /// Stable model identifier reused by controller config and transport adapters.
    /// 被控制器配置与传输适配器复用的稳定模型标识。
    pub(crate) model_id: String,
    /// Final outgoing prompt body forwarded into the host-controlled runtime stream.
    /// 被转发到宿主控制运行时流中的最终出站提示词正文。
    pub(crate) prompt: String,
    /// Fully cloned request-header map reused by the concrete transport adapter.
    /// 被具体传输适配器复用的完整克隆请求头映射。
    pub(crate) request_headers: HeaderMap,
    /// Prebuilt controller configuration handed to `vulcan-core`.
    /// 交给 `vulcan-core` 的预构建控制器配置。
    pub(crate) controller_config: CoreControllerConfig,
}

/// Prepares the shared provider transport entry inputs from lightweight borrowed arguments.
/// 根据轻量借用参数准备共享 Provider 传输入口输入对象。
pub(crate) fn prepare_provider_transport_entry_inputs(
    provider_id: &str,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    additional_params: Option<Value>,
) -> PreparedProviderTransportEntryInputs {
    let provider_id = provider_id.to_string();
    let model_id = model_id.to_string();

    PreparedProviderTransportEntryInputs {
        provider_id: provider_id.clone(),
        model_id: model_id.clone(),
        prompt: prompt.to_string(),
        request_headers: headers.clone(),
        controller_config: CoreControllerConfig {
            provider_id,
            model_id,
            system_prompt: system_preamble.to_string(),
            temperature,
            max_turns,
            additional_params,
        },
    }
}

/// Normalizes one optional base URL override by dropping empty strings and returning an owned value when present.
/// 规范化单个可选基础地址覆盖值，在存在时返回拥有所有权的值，并丢弃空字符串。
pub(crate) fn normalize_optional_base_url(base_url: Option<&str>) -> Option<String> {
    base_url
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}
