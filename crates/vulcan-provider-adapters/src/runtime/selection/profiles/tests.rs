//! Signed profile replacement and resource-constraint regressions.
//! 签名 Profile 替换与资源约束回归测试。

use super::{
    resolve_input_resource_constraints, resolve_request_projection_chain,
    resolve_response_projection_chain, resolve_usage_extraction_chain,
};
use vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID;

/// Verifies provider-wide and model-specific resource constraints merge into frozen routes.
/// 验证供应商通用与模型专属资源约束会合并到冻结路由。
#[test]
fn bundled_resource_constraints_resolve_without_provider_hardcoding() {
    let manifests = crate::provider_manifest::load_bundled_resolved_manifests();
    let minimax = manifests
        .get("minimax")
        .expect("MiniMax manifest must be bundled");
    let minimax_constraints = resolve_input_resource_constraints(
        minimax,
        "global",
        Some("payg-api-key"),
        "MiniMax-M3",
        Vec::new(),
    );
    assert_eq!(minimax_constraints.len(), 2);
    assert_eq!(
        minimax_constraints
            .iter()
            .find(|constraint| {
                constraint.resource_kind == vulcan_ai_contract::ResourceKind::Video
            })
            .and_then(|constraint| constraint.max_bytes),
        Some(50_000_000)
    );

    let openrouter = manifests
        .get("openrouter")
        .expect("OpenRouter manifest must be bundled");
    let chat_route = openrouter.descriptor.service_entries[0]
        .protocol_routes
        .iter()
        .find(|route| route.protocol_profile_id == OPENAI_CHAT_PROFILE_ID)
        .expect("OpenRouter Chat route must exist");
    assert_eq!(chat_route.input_resource_constraints.len(), 2);
    assert!(
        chat_route
            .input_resource_constraints
            .iter()
            .any(|constraint| {
                constraint.resource_kind == vulcan_ai_contract::ResourceKind::File
                    && constraint.accepted_mime_types == [String::from("application/pdf")]
            })
    );
}

/// Verifies model rules and default bindings replace all three route profile chains.
/// 验证模型规则与默认绑定会替换三类路由 Profile 链。
#[test]
fn model_scoped_profiles_freeze_into_the_execution_route() {
    // manifests supplies validated payloads so this test exercises only runtime precedence.
    // manifests 提供已验证载荷，使本测试只验证运行时优先级。
    let manifests = crate::provider_manifest::load_bundled_resolved_manifests();
    let mut manifest = manifests
        .get("deepseek")
        .expect("DeepSeek manifest must be bundled")
        .as_ref()
        .clone();
    // This precedence fixture intentionally enables a generic unmatched-model default so the
    // test can compare rule-owned and default-owned profile chains independently of DeepSeek's
    // production fixed-inventory policy.
    // 本优先级夹具有意启用通用未匹配模型默认值，使测试可以脱离 DeepSeek 生产固定清单
    // 策略，独立比较规则持有与默认持有的 Profile 链。
    manifest.unknown_model_policy =
        crate::provider_manifest::UnknownModelPolicy::DefaultBindingAllowed;
    let minimax = manifests
        .get("minimax")
        .expect("MiniMax manifest must be bundled");
    let default_response = minimax
        .response_profiles
        .first()
        .expect("MiniMax must declare a response profile")
        .clone();
    let rule_response = minimax
        .response_profiles
        .get(1)
        .expect("MiniMax must declare a second response profile")
        .clone();
    manifest.response_profiles.push(default_response.clone());
    manifest.response_profiles.push(rule_response.clone());
    let defaults = manifest
        .entry_model_defaults
        .get_mut("official-api")
        .expect("DeepSeek official entry defaults must compile");
    defaults.usage_profile_refs = vec![String::from("deepseek.chat.body.v1")];
    defaults.request_profile_refs = vec![String::from("deepseek.responses.docs.v1")];
    defaults.response_profile_refs = vec![default_response.profile_id.clone()];
    let rule = manifest
        .rule_instances
        .iter_mut()
        .find(|rule| rule.matcher.matches("deepseek-v4-flash"))
        .expect("DeepSeek flash rule must compile");
    rule.usage_profile_refs = vec![String::from("deepseek.chat.sse.usage.v1")];
    rule.request_profile_refs = vec![String::from("deepseek.chat.thinking.v1")];
    rule.response_profile_refs = vec![rule_response.profile_id.clone()];

    let matched_usage = resolve_usage_extraction_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-v4-flash",
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
        Vec::new(),
    )
    .expect("matched usage profile must resolve");
    let default_usage = resolve_usage_extraction_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-future-model",
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
        Vec::new(),
    )
    .expect("default usage profile must resolve");
    assert_eq!(matched_usage[0].profile_id, "deepseek.chat.sse.usage.v1");
    assert_eq!(default_usage[0].profile_id, "deepseek.chat.body.v1");

    let matched_request = resolve_request_projection_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-v4-flash",
        Vec::new(),
    )
    .expect("matched request profile must resolve");
    let default_request = resolve_request_projection_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-future-model",
        Vec::new(),
    )
    .expect("default request profile must resolve");
    assert_eq!(matched_request[0].profile_id, "deepseek.chat.thinking.v1");
    assert_eq!(default_request[0].profile_id, "deepseek.responses.docs.v1");

    let matched_response = resolve_response_projection_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-v4-flash",
        Vec::new(),
    )
    .expect("matched response profile must resolve");
    let default_response_chain = resolve_response_projection_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-future-model",
        Vec::new(),
    )
    .expect("default response profile must resolve");
    assert_eq!(matched_response[0].profile_id, rule_response.profile_id);
    assert_eq!(
        default_response_chain[0].profile_id,
        default_response.profile_id
    );
}

/// Verifies an explicit-empty model profile override clears provider special cases while
/// preserving the standard protocol usage baseline.
/// 验证显式空模型 Profile 覆盖会清除供应商特例，同时保留标准协议 usage 基线。
#[test]
fn explicit_empty_model_profiles_clear_route_special_cases() {
    // manifests provides real compiled provider-specific payloads for all three chains.
    // manifests 为三类链提供真实已编译的供应商专属载荷。
    let manifests = crate::provider_manifest::load_bundled_resolved_manifests();
    let mut manifest = manifests
        .get("deepseek")
        .expect("DeepSeek manifest must be bundled")
        .as_ref()
        .clone();
    let minimax = manifests
        .get("minimax")
        .expect("MiniMax manifest must be bundled");
    // routeUsageDefault, routeRequestDefault, and routeResponseDefault emulate the selected
    // route's provider special cases before model-level replacement.
    // routeUsageDefault、routeRequestDefault 与 routeResponseDefault 模拟模型级替换前
    // 所选路由的供应商特例。
    let route_usage_default = vec![
        manifest
            .usage_profiles
            .first()
            .and_then(|declaration| declaration.payload.clone())
            .expect("DeepSeek must declare a compiled usage profile"),
    ];
    let route_request_default = vec![
        manifest
            .request_profiles
            .first()
            .and_then(|declaration| declaration.payload.clone())
            .expect("DeepSeek must declare a compiled request profile"),
    ];
    let route_response_default = vec![
        minimax
            .response_profiles
            .first()
            .and_then(|declaration| declaration.payload.clone())
            .expect("MiniMax must declare a compiled response profile"),
    ];
    // rule carries the compiler fact produced by an explicit empty list, including rule-owned
    // provenance even though the frozen references are empty.
    // rule 携带显式空列表产生的编译事实；即使冻结引用为空，来源仍归规则所有。
    let rule = manifest
        .rule_instances
        .iter_mut()
        .find(|rule| rule.matcher.matches("deepseek-v4-flash"))
        .expect("DeepSeek flash rule must compile");
    rule.usage_profile_refs.clear();
    rule.request_profile_refs.clear();
    rule.response_profile_refs.clear();
    for field in [
        "usage_profile_refs",
        "request_profile_refs",
        "response_profile_refs",
    ] {
        rule.field_provenance
            .get_mut(field)
            .expect("profile provenance must exist")
            .source = crate::provider_manifest::FieldSourceKind::ModelRulePrefix;
    }

    // clearedUsage must restore only the protocol baseline; request and response have no
    // lower protocol baseline and therefore become empty.
    // clearedUsage 必须只恢复协议基线；请求与响应不存在更低协议基线，因此变为空。
    let cleared_usage = resolve_usage_extraction_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-v4-flash",
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
        route_usage_default,
    )
    .expect("explicit usage clear must restore the standard baseline");
    let cleared_request = resolve_request_projection_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-v4-flash",
        route_request_default,
    )
    .expect("explicit request clear must resolve");
    let cleared_response = resolve_response_projection_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-v4-flash",
        route_response_default,
    )
    .expect("explicit response clear must resolve");
    let expected_usage = vulcan_llm_core::standard_usage_profiles(
        vulcan_llm_core::UpstreamProtocol::OpenAiChatCompletions,
    );
    assert_eq!(
        cleared_usage
            .iter()
            .map(|profile| profile.profile_id.as_str())
            .collect::<Vec<_>>(),
        expected_usage
            .iter()
            .map(|profile| profile.profile_id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(cleared_request.is_empty());
    assert!(cleared_response.is_empty());
}
