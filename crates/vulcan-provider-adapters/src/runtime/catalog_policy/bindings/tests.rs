//! Frozen protocol affinity and manual-variant identity regressions.
//! 冻结协议归属与手工变体身份回归测试。

use super::*;
use std::collections::BTreeMap;
use vulcan_llm_core::{ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID};

/// Verifies synthetic manual discovery signatures remain stable across catalog generations.
/// 验证合成手工发现签名跨目录代次保持稳定。
#[test]
fn manual_model_discovery_signature_excludes_observation_generation() {
    // baselineModel owns explicit local evidence and one exact custom binding.
    // baselineModel 持有显式本地证据与一个精确自定义绑定。
    let baseline_model = ConnectionModel {
        id: String::from("manual-model"),
        display_name: String::from("Manual Model"),
        capabilities: ModelCapabilities::default(),
        capability_evidence: ModelCapabilityEvidence::default(),
        protocol_variants: BTreeMap::new(),
        resolved_protocol_binding: Some(crate::provider_manifest::ModelProtocolBinding {
            protocol_profile_id: OPENAI_RESPONSES_PROFILE_ID.to_string(),
            source: crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault,
            manifest_revision: 0,
            rule_id: String::new(),
            codex_behavior: None,
        }),
    };
    // firstModel and refreshedModel differ only in their observation generation.
    // firstModel 与 refreshedModel 仅观测代次不同。
    let mut first_model = baseline_model.clone();
    let mut refreshed_model = baseline_model;
    ensure_manual_custom_bound_variant("custom:manual-revision", "default", 1, &mut first_model);
    ensure_manual_custom_bound_variant(
        "custom:manual-revision",
        "default",
        99,
        &mut refreshed_model,
    );
    let first_variant = first_model
        .protocol_variants
        .get(OPENAI_RESPONSES_PROFILE_ID)
        .expect("first manual variant should exist");
    let refreshed_variant = refreshed_model
        .protocol_variants
        .get(OPENAI_RESPONSES_PROFILE_ID)
        .expect("refreshed manual variant should exist");
    assert_eq!(
        first_variant.discovery_signature, refreshed_variant.discovery_signature,
        "manual source identity must be content-addressed"
    );
    assert_ne!(
        first_variant.catalog_generation, refreshed_variant.catalog_generation,
        "observation metadata must still retain the actual refresh generation"
    );
}

/// Verifies a higher-priority protocol addition invalidates Auto but preserves explicit binding.
/// 验证新增更高优先级协议会使 Auto 绑定失效，但保留显式绑定。
#[test]
fn custom_auto_binding_rebinds_when_protocol_priority_changes() {
    // autoBinding represents the catalog frozen while only Anthropic was enabled.
    // autoBinding 表示仅启用 Anthropic 时由目录冻结的绑定。
    let auto_binding = crate::provider_manifest::ModelProtocolBinding {
        protocol_profile_id: ANTHROPIC_MESSAGES_PROFILE_ID.to_string(),
        source: crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault,
        manifest_revision: 1,
        rule_id: String::new(),
        codex_behavior: None,
    };
    assert!(frozen_binding_is_current(
        &auto_binding,
        None,
        &[ANTHROPIC_MESSAGES_PROFILE_ID],
        &[ANTHROPIC_MESSAGES_PROFILE_ID],
    ));
    assert!(
        !frozen_binding_is_current(
            &auto_binding,
            None,
            &[ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,],
            &[ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,],
        ),
        "adding Responses must invalidate an older lower-priority Auto binding"
    );

    // explicitBinding remains authoritative while its exact protocol stays enabled.
    // explicitBinding 在其精确协议仍启用时保持权威。
    let explicit_binding = crate::provider_manifest::ModelProtocolBinding {
        protocol_profile_id: ANTHROPIC_MESSAGES_PROFILE_ID.to_string(),
        source: crate::provider_manifest::ModelProtocolBindingSource::CustomUser,
        manifest_revision: 1,
        rule_id: String::new(),
        codex_behavior: None,
    };
    let explicit_preference = crate::CustomModelProtocolPreference::Explicit {
        protocol: crate::CustomProviderProtocol::AnthropicMessages,
    };
    assert!(frozen_binding_is_current(
        &explicit_binding,
        Some(&explicit_preference),
        &[ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,],
        &[ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,],
    ));
}
