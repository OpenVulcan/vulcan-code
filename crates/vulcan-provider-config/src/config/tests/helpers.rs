use super::super::{
    ModelChoice, ProviderBaseUrlConfig, ProviderBaseUrlKind, ProviderModelDiscoveryConfig,
    ProviderModelDiscoveryKind, ProviderRuntimeKind, ProviderUsageCaptureConfig,
    ReasoningProfileConfig, SupportedProviderDefinition,
};
use serde_json::Map as JsonMap;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Builds a unique temporary directory path for provider configuration tests.
/// 为 Provider 配置测试构建一个唯一临时目录路径。
pub(super) fn unique_temp_dir(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{label}-{}-{nanos}", std::process::id()))
}

/// Builds one minimal supported-provider definition used by the reasoning-selection tests.
/// 构建供思考选择测试复用的最小受支持 Provider 定义。
pub(super) fn test_provider_definition(
    runtime: ProviderRuntimeKind,
    provider_id: &str,
    reasoning_profile: Option<ReasoningProfileConfig>,
) -> SupportedProviderDefinition {
    SupportedProviderDefinition {
        id: provider_id.to_string(),
        name: provider_id.to_string(),
        runtime,
        user_agent: None,
        credential_env_keys: Vec::new(),
        extra_headers: Default::default(),
        extra_body: JsonMap::new(),
        usage_capture: ProviderUsageCaptureConfig::default(),
        model_runtime_overrides: BTreeMap::new(),
        reasoning_profile,
        ai_tools: Vec::new(),
        fields: Vec::new(),
        base_url: ProviderBaseUrlConfig {
            kind: ProviderBaseUrlKind::Native,
            value: None,
            template: None,
            parameter: None,
            allow_override: false,
        },
        model_discovery: ProviderModelDiscoveryConfig {
            kind: ProviderModelDiscoveryKind::None,
        },
        models: Vec::new(),
    }
}

/// Builds one minimal model choice used by the reasoning-selection tests.
/// 构建供思考选择测试复用的最小模型选项。
pub(super) fn test_model_choice(
    model_id: &str,
    supports_reasoning: bool,
    reasoning_profile: Option<ReasoningProfileConfig>,
) -> ModelChoice {
    ModelChoice {
        id: model_id.to_string(),
        name: model_id.to_string(),
        default_enabled: false,
        status: None,
        family: None,
        release_date: None,
        context_limit: None,
        input_limit: None,
        output_limit: None,
        reasoning: Some(supports_reasoning),
        tool_call: Some(true),
        temperature: Some(true),
        attachment: None,
        interleaved: None,
        reasoning_profile,
        ai_tools: Vec::new(),
        input_modalities: Vec::new(),
        output_modalities: Vec::new(),
    }
}
