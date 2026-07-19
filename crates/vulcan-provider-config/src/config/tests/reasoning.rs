use super::super::{
    AiToolKind, ProviderRuntimeKind, ReasoningCapabilityKind, ReasoningEffortLevel,
    ReasoningProfileConfig, apply_provider_reasoning_model_overrides, supported_provider_by_id,
};
use super::helpers::{test_model_choice, test_provider_definition};
use serde_json::json;

/// Verifies binary reasoning profiles collapse the selector into the expected on/off pair and remap higher remembered tiers to “开启”.
/// 验证二值思考配置会把选择器收缩为预期的开关二项，并将更高的已记忆档位回退为“开启”。
#[test]
fn binary_reasoning_profile_builds_on_off_selector_and_downgrades_high_to_enabled() {
    let provider = test_provider_definition(
        ProviderRuntimeKind::OpenAiCompatible,
        "bailing",
        Some(ReasoningProfileConfig {
            kind: Some(ReasoningCapabilityKind::Binary),
            off: Some(json!({ "enable_thinking": false })),
            low: Some(json!({ "enable_thinking": true })),
            medium: Some(json!({ "enable_thinking": true })),
            ..ReasoningProfileConfig::default()
        }),
    );
    let model = test_model_choice("qwen-coder", true, None);

    let view = provider.reasoning_selection_view(Some(&model), ReasoningEffortLevel::High);

    assert_eq!(view.options.len(), 2);
    assert_eq!(view.options[0].label, "关闭");
    assert_eq!(view.options[1].label, "开启");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::Low);
    assert_eq!(
        view.note.as_deref(),
        Some("当前模型 `qwen-coder` 不支持 `高` 思考档位，已自动切换到 `开启`。")
    );
}

/// Verifies Alibaba China provider definitions expose one binary reasoning selector instead of falling back to tiered controls.
/// 验证阿里百炼中国区供应商定义会暴露二值思考选择器，而不会回退成多档位控制。
#[test]
fn alibaba_cn_provider_uses_binary_reasoning_selector() {
    let provider = supported_provider_by_id("alibaba-cn").expect("alibaba-cn provider must exist");
    let model = provider
        .models
        .iter()
        .find(|item| item.id == "qwen3.6-plus")
        .expect("qwen3.6-plus model must exist");

    let view = provider.reasoning_selection_view(Some(model), ReasoningEffortLevel::High);

    assert_eq!(view.options.len(), 2);
    assert_eq!(view.options[0].label, "关闭");
    assert_eq!(view.options[1].label, "开启");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::Low);
}

/// Verifies Alibaba Coding Plan China provider definitions also expose the binary reasoning selector.
/// 验证阿里 Coding Plan 中国区供应商定义同样会暴露二值思考选择器。
#[test]
fn alibaba_coding_plan_cn_provider_uses_binary_reasoning_selector() {
    let provider = supported_provider_by_id("alibaba-coding-plan-cn")
        .expect("alibaba-coding-plan-cn provider must exist");
    let model = provider
        .models
        .iter()
        .find(|item| item.id == "qwen3.6-plus")
        .expect("qwen3.6-plus model must exist");

    let view = provider.reasoning_selection_view(Some(model), ReasoningEffortLevel::High);

    assert_eq!(view.options.len(), 2);
    assert_eq!(view.options[0].label, "关闭");
    assert_eq!(view.options[1].label, "开启");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::Low);
}

/// Verifies Alibaba Token Plan exposes only Qwen 3.8 with always-on low/high/xhigh reasoning, defaults unsupported preferences to xhigh, and declares implicit native web search without a tools-array patch.
/// 验证阿里云 Token Plan 仅暴露千问 3.8，提供始终开启的 low/high/xhigh 推理档位，将不支持的偏好默认回退为 xhigh，并声明无需 tools 数组补丁的隐式原生联网搜索。
#[test]
fn alibaba_token_plan_cn_exposes_exact_qwen_3_8_contract() {
    // provider is the embedded Token Plan definition resolved through the same catalog path used at runtime.
    // provider 是通过运行时同一目录路径解析的内嵌 Token Plan 定义。
    let provider = supported_provider_by_id("alibaba-token-plan-cn")
        .expect("alibaba-token-plan-cn provider must exist");
    assert_eq!(provider.runtime, ProviderRuntimeKind::OpenAiCompatible);
    assert_eq!(
        provider.credential_env_keys,
        vec![String::from("ALIBABA_TOKEN_PLAN_API_KEY")]
    );
    assert_eq!(provider.extra_body.get("tool_stream"), Some(&json!(true)));
    assert_eq!(provider.models.len(), 1);

    // model is the sole catalog entry authorized by the Token Plan request.
    // model 是 Token Plan 需求授权的唯一目录条目。
    let model = &provider.models[0];
    assert_eq!(model.id, "qwen3.8-max-preview");
    assert_eq!(model.context_limit, Some(983_616));
    assert_eq!(model.output_limit, Some(131_072));
    assert_eq!(model.input_modalities, ["text", "image"]);
    assert_eq!(model.output_modalities, ["text"]);

    // view starts at xhigh when the remembered shared medium preference is unsupported.
    // view 在共享记忆的 medium 偏好不受支持时从 xhigh 开始。
    let view = provider.reasoning_selection_view(Some(model), ReasoningEffortLevel::Medium);
    assert_eq!(
        view.options
            .iter()
            .map(|option| option.effort)
            .collect::<Vec<_>>(),
        vec![
            ReasoningEffortLevel::Low,
            ReasoningEffortLevel::High,
            ReasoningEffortLevel::XHigh,
        ]
    );
    assert_eq!(view.selected_effort, ReasoningEffortLevel::XHigh);
    assert_eq!(
        provider
            .reasoning_profile
            .as_ref()
            .and_then(|profile| profile.patch_for_effort(ReasoningEffortLevel::XHigh)),
        Some(json!({ "reasoning_effort": "xhigh" }))
    );

    // web_search remains provider-native while its empty request patch prevents any web_search object from being appended to the OpenAI-compatible tools array.
    // web_search 保持为 Provider 原生能力，同时空请求补丁会阻止向 OpenAI 兼容 tools 数组追加任何 web_search 对象。
    assert_eq!(model.ai_tools.len(), 1);
    assert_eq!(model.ai_tools[0].kind, AiToolKind::WebSearch);
    assert_eq!(model.ai_tools[0].native_provider_tool, Some(true));
    assert_eq!(model.ai_tools[0].native_request_patch, Some(json!({})));
    assert!(model.ai_tools[0].native_tool_type.is_none());
}

/// Verifies DeepSeek official provider definitions expose the expected three-state selector and remap the shared medium default onto `高`.
/// 验证 DeepSeek 官方供应商定义会暴露预期的三态选择器，并将共享中档默认映射到 `高`。
#[test]
fn deepseek_provider_uses_off_high_max_selector() {
    let provider = supported_provider_by_id("deepseek").expect("deepseek provider must exist");
    let model = provider
        .models
        .iter()
        .find(|item| item.id == "deepseek-v4-pro")
        .expect("deepseek-v4-pro model must exist");

    let view = provider.reasoning_selection_view(Some(model), ReasoningEffortLevel::Medium);

    assert_eq!(view.options.len(), 3);
    assert_eq!(view.options[0].label, "关闭");
    assert_eq!(view.options[1].label, "高");
    assert_eq!(view.options[2].label, "Max");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::High);
}

/// Verifies custom reasoning labels override the shared compact labels when one provider declares them explicitly.
/// 验证当供应商显式声明自定义思考标签时，会覆盖共享的紧凑标签。
#[test]
fn custom_reasoning_labels_override_shared_labels() {
    let provider = test_provider_definition(
        ProviderRuntimeKind::OpenAiCompatible,
        "custom-provider",
        Some(ReasoningProfileConfig {
            off: Some(json!({"thinking": {"type": "disabled"}})),
            high: Some(json!({"thinking": {"type": "enabled"}, "reasoning_effort": "high"})),
            x_high: Some(json!({"thinking": {"type": "enabled"}, "reasoning_effort": "max"})),
            high_label: Some(String::from("高")),
            x_high_label: Some(String::from("Max")),
            ..ReasoningProfileConfig::default()
        }),
    );
    let model = test_model_choice("custom-model", true, None);

    let view = provider.reasoning_selection_view(Some(&model), ReasoningEffortLevel::High);

    assert_eq!(view.options[1].label, "高");
    assert_eq!(view.options[2].label, "Max");
}

/// Verifies default-only reasoning profiles collapse the selector into one non-editable “默认” entry without forcing explicit patches.
/// 验证默认思考配置会把选择器收缩成一个不可编辑的“默认”项，而不会强制显式 Patch。
#[test]
fn default_only_reasoning_profile_builds_default_selector() {
    let provider = test_provider_definition(
        ProviderRuntimeKind::OpenAiCompatible,
        "default-provider",
        Some(ReasoningProfileConfig {
            kind: Some(ReasoningCapabilityKind::DefaultOnly),
            ..ReasoningProfileConfig::default()
        }),
    );
    let model = test_model_choice("default-model", true, None);

    let view = provider.reasoning_selection_view(Some(&model), ReasoningEffortLevel::High);

    assert_eq!(view.options.len(), 1);
    assert_eq!(view.options[0].label, "默认");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::Off);
}

/// Verifies Alibaba-family models outside the maintained binary-toggle matrix fall back to the non-editable default reasoning mode.
/// 验证未进入已维护二值开关矩阵的阿里系模型会回退到不可编辑的默认思考模式。
#[test]
fn alibaba_unmaintained_model_falls_back_to_default_only_reasoning_mode() {
    let mut models = vec![test_model_choice("future-ali-model", false, None)];
    apply_provider_reasoning_model_overrides("alibaba-cn", &mut models);
    let model = models.pop().expect("model override result");

    assert_eq!(model.reasoning, Some(true));
    assert_eq!(
        model
            .reasoning_profile
            .as_ref()
            .and_then(|profile| profile.kind),
        Some(ReasoningCapabilityKind::DefaultOnly)
    );
}

/// Verifies a model that does not support reasoning forces the selector into a disabled off-only state and reports the downgrade.
/// 验证不支持思考的模型会强制将选择器收缩为仅关闭状态，并报告回退说明。
#[test]
fn non_reasoning_model_forces_selector_to_off_only() {
    let provider = test_provider_definition(ProviderRuntimeKind::OpenAi, "openai", None);
    let model = test_model_choice("gpt-lite", false, None);

    let view = provider.reasoning_selection_view(Some(&model), ReasoningEffortLevel::Medium);

    assert_eq!(view.options.len(), 1);
    assert_eq!(view.options[0].label, "关闭");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::Off);
    assert_eq!(
        view.note.as_deref(),
        Some("当前模型 `gpt-lite` 不支持思考能力，已自动切换到关闭思考。")
    );
}

/// Verifies Gemini 3 model families expose only the runtime-supported low/high compatibility tiers by default.
/// 验证 Gemini 3 模型家族默认仅暴露运行时真正支持的低/高兼容档位。
#[test]
fn gemini_three_defaults_only_expose_low_and_high() {
    let provider = test_provider_definition(ProviderRuntimeKind::GoogleGemini, "google", None);
    let model = test_model_choice("gemini-3-pro", true, None);

    let supported = provider.supported_reasoning_efforts(Some(&model));

    assert_eq!(
        supported,
        vec![ReasoningEffortLevel::Low, ReasoningEffortLevel::High]
    );
}
