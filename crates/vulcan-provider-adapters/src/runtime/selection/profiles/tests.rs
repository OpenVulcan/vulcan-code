//! Signed profile replacement and resource-constraint regressions.
//! 签名 Profile 替换与资源约束回归测试。

use super::{
    resolve_input_resource_constraints, resolve_request_projection_chain,
    resolve_response_projection_chain, resolve_usage_extraction_chain,
};
use vulcan_llm_core::OPENAI_CHAT_PROFILE_ID;

/// Model MIME configuration replaces provider MIME configuration, while absent restrictions leave host defaults available.
/// 模型MIME配置替换供应商MIME配置，未声明限制时保留宿主默认类型。
#[test]
fn native_media_mime_precedence_preserves_independent_configuration() {
    // Use an isolated compiled manifest clone; no shared registry or signed resource is modified.
    // 使用隔离的已编译清单副本；不修改共享注册表或签名资源。
    let manifests = crate::provider_manifest::load_bundled_resolved_manifests();
    // OpenRouter's known model supplies a real resolved rule with entry and credential ownership.
    // OpenRouter已知模型提供带入口及凭据归属的真实解析规则。
    let mut manifest = manifests
        .get("openrouter")
        .expect("OpenRouter must be bundled")
        .as_ref()
        .clone();
    // Keep exactly the selected rule so this test controls only its media constraints.
    // 仅保留选定规则，使本测试只控制其媒体约束。
    let mut rule = manifest
        .rule_for_model("public-api", Some("api-key"), "openai/gpt-5.6-terra")
        .expect("the fixture model must resolve its exact rule")
        .clone();
    rule.input_resource_constraints.clear();
    manifest.rule_instances = vec![rule.clone()];
    assert!(
        resolve_input_resource_constraints(
            &manifest,
            "public-api",
            Some("api-key"),
            "openai/gpt-5.6-terra",
            Vec::new()
        )
        .is_empty()
    );
    // A provider-level PNG/JPEG list must survive unchanged when the model declares no independent list.
    // 模型未声明独立列表时，供应商级PNG/JPEG列表必须原样保留。
    let provider_constraint = crate::InputResourceConstraint {
        resource_kind: vulcan_ai_contract::ResourceKind::Image,
        accepted_mime_types: vec!["image/png".into(), "image/jpeg".into()],
        max_bytes: None,
        max_count: None,
        reference_policy: crate::ResourceReferencePolicy::Any,
        upload_channel_refs: Vec::new(),
    };
    assert_eq!(
        resolve_input_resource_constraints(
            &manifest,
            "public-api",
            Some("api-key"),
            "openai/gpt-5.6-terra",
            vec![provider_constraint.clone()]
        ),
        vec![provider_constraint.clone()]
    );
    // The model narrows the provider formats to JPEG; PNG and other defaults must not leak back in.
    // 模型将供应商格式收紧为JPEG；PNG及其他默认格式不能重新混入。
    let model_constraint = crate::InputResourceConstraint {
        accepted_mime_types: vec!["image/jpeg".into()],
        ..provider_constraint.clone()
    };
    rule.input_resource_constraints = vec![model_constraint.clone()];
    manifest.rule_instances = vec![rule];
    assert_eq!(
        resolve_input_resource_constraints(
            &manifest,
            "public-api",
            Some("api-key"),
            "openai/gpt-5.6-terra",
            vec![provider_constraint]
        ),
        vec![model_constraint]
    );
}

/// Replays the two captured Step Plan chunks through the selected manifest usage chain.
/// 将 Step Plan 实际捕获的两帧按所选清单用量链重放，验证累计值不会提前冻结或重复相加。
#[test]
fn stepfun_plan_step5_cumulative_usage_does_not_freeze_on_first_chunk() {
    // Compile the actual bundled declaration and resolve the exact observed entry/model.
    // 编译真实随包声明，并解析本次实际观察到的入口与模型。
    let manifests = crate::provider_manifest::load_bundled_resolved_manifests();
    let manifest = manifests.get("stepfun").expect("StepFun must be bundled");
    let profiles = resolve_usage_extraction_chain(
        manifest,
        "step-plan",
        Some("plan-key"),
        "step-5-preview",
        OPENAI_CHAT_PROFILE_ID,
        vulcan_llm_core::standard_usage_profiles(
            vulcan_llm_core::UpstreamProtocol::OpenAiChatCompletions,
        ),
    )
    .expect("the captured route must resolve its usage profiles");
    let mut accumulator = vulcan_llm_core::UsageAccumulator::default();
    let mut observations = 0;
    // The fixture contains only the captured prefix; no synthetic final response is claimed.
    // 夹具仅包含真实捕获前缀，不将合成终态冒充上游完整响应。
    for line in include_str!("fixtures/stepfun-step5-cumulative.sse").lines() {
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        let frame: serde_json::Value =
            serde_json::from_str(data).expect("captured frame must parse");
        for profile in &profiles {
            if let Some(observation) = profile
                .extract(vulcan_llm_core::UsageRecordKind::SseEvent, &frame)
                .expect("captured usage fields must parse")
            {
                accumulator
                    .apply(None, observation)
                    .expect("cumulative chunk must not conflict with an earlier final usage");
                observations += 1;
            }
        }
    }
    assert_eq!(
        observations, 2,
        "each captured chunk must match exactly one usage profile"
    );
    for (actual, expected) in [
        (accumulator.usage().input_tokens, 22862),
        (accumulator.usage().output_tokens, 1),
        (accumulator.usage().total_tokens, 22863),
        (accumulator.usage().cache_read_tokens, 0),
        (accumulator.usage().reasoning_tokens, 0),
    ] {
        assert_eq!(
            actual,
            vulcan_ai_contract::UsageValue::Known {
                value: expected,
                source: vulcan_ai_contract::UsageValueSource::Reported,
            }
        );
    }
    assert_eq!(
        accumulator
            .source_paths()
            .get(&vulcan_llm_core::UsageField::TotalTokens)
            .map(String::as_str),
        Some("/usage/total_tokens")
    );
    assert!(
        !accumulator.is_final(),
        "an unfinished stream must remain a cumulative snapshot"
    );
    assert!(accumulator.validation_flags().is_empty());
}

/// Keeps the cumulative exception within the captured entry/model and preserves JSON final usage.
/// 将累计统计特例限制在已捕获的入口与模型，并保留非流式 JSON 的最终用量语义。
#[test]
fn stepfun_cumulative_profile_is_limited_to_observed_route() {
    let manifests = crate::provider_manifest::load_bundled_resolved_manifests();
    let manifest = manifests.get("stepfun").expect("StepFun must be bundled");
    let standard = vulcan_llm_core::standard_usage_profiles(
        vulcan_llm_core::UpstreamProtocol::OpenAiChatCompletions,
    );
    let standard_body = standard
        .iter()
        .find(|profile| profile.record_kind == vulcan_llm_core::UsageRecordKind::JsonBody)
        .expect("standard Chat must define JSON-body usage");
    // Different regions/products and sibling models do not inherit unverified wire facts.
    // 其他地区、产品及同系列模型不继承未经验证的线路事实。
    for (entry, credential) in [
        ("official-api", "api-key"),
        ("step-plan", "plan-key"),
        ("international-api", "api-key"),
        ("international-step-plan", "plan-key"),
    ] {
        for model in [
            "step-3.7-flash",
            "step-3.5-flash-2603",
            "step-3.5-flash",
            "step-5-preview",
        ] {
            let profiles = resolve_usage_extraction_chain(
                manifest,
                entry,
                Some(credential),
                model,
                OPENAI_CHAT_PROFILE_ID,
                standard.clone(),
            )
            .expect("declared StepFun route must resolve");
            if entry != "step-plan" || model != "step-5-preview" {
                assert_eq!(
                    profiles, standard,
                    "unexpected usage override for {entry}/{model}"
                );
                continue;
            }
            let body = profiles
                .iter()
                .find(|profile| profile.record_kind == vulcan_llm_core::UsageRecordKind::JsonBody)
                .expect("the captured route must retain non-streaming usage");
            assert_eq!(body.update_kind, vulcan_ai_contract::UsageUpdateKind::Final);
            assert_eq!(body.bindings, standard_body.bindings);
            assert_eq!(body.derivations, standard_body.derivations);
            let stream = profiles
                .iter()
                .find(|profile| profile.record_kind == vulcan_llm_core::UsageRecordKind::SseEvent)
                .expect("the captured route must declare streaming usage");
            assert_eq!(
                stream.update_kind,
                vulcan_ai_contract::UsageUpdateKind::Snapshot
            );
            assert_eq!(
                profiles.len(),
                2,
                "the route must not also apply the standard final SSE profile"
            );
        }
    }
}

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
    defaults.unknown_model_policy =
        crate::provider_manifest::UnknownModelPolicy::DefaultBindingAllowed;
    defaults.usage_profile_refs = vec![String::from("deepseek.chat.body.v1")];
    defaults.request_profile_refs = vec![String::from("deepseek.responses.docs.v1")];
    defaults.response_profile_refs = vec![default_response.profile_id.clone()];
    let rule = manifest
        .rule_instances
        .iter_mut()
        .find(|rule| rule.matcher.matches("deepseek-flash"))
        .expect("DeepSeek flash rule must compile");
    rule.usage_profile_refs = vec![String::from("deepseek.chat.sse.usage.v1")];
    rule.request_profile_refs = vec![String::from("deepseek.chat.thinking.v1")];
    rule.response_profile_refs = vec![rule_response.profile_id.clone()];

    let matched_usage = resolve_usage_extraction_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-flash",
        vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
        Vec::new(),
    )
    .expect("matched usage profile must resolve");
    let default_usage = resolve_usage_extraction_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-future-model",
        vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
        Vec::new(),
    )
    .expect("default usage profile must resolve");
    assert_eq!(matched_usage[0].profile_id, "deepseek.chat.sse.usage.v1");
    assert_eq!(default_usage[0].profile_id, "deepseek.chat.body.v1");

    let matched_request = resolve_request_projection_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-flash",
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
        "deepseek-flash",
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
        .find(|rule| rule.matcher.matches("deepseek-flash"))
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
        "deepseek-flash",
        vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
        route_usage_default,
    )
    .expect("explicit usage clear must restore the standard baseline");
    let cleared_request = resolve_request_projection_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-flash",
        route_request_default,
    )
    .expect("explicit request clear must resolve");
    let cleared_response = resolve_response_projection_chain(
        &manifest,
        "official-api",
        Some("api-key"),
        "deepseek-flash",
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
