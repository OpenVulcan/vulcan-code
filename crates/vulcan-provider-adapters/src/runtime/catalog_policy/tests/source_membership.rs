//! Independent manual identities and the complete source-membership policy matrix.
//! 独立手工身份与完整来源成员策略矩阵。

use crate::runtime::catalog_policy::{
    apply_model_catalog_overlays, apply_model_catalog_overlays_with_account_access,
};
use crate::{
    ConnectionModel, ConnectionModelCatalog, CredentialConnection, CredentialMode,
    ModelCapabilityEvidence, PersistedAiConfiguration, ProtocolModelVariant,
};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{AiOperation, ModelCapabilities};
use vulcan_llm_core::OPENAI_CHAT_PROFILE_ID;

/// Verifies a non-empty discovery result is unioned with independent manual identities.
/// 验证非空发现结果与独立手工模型身份取并集。
#[test]
fn custom_discovery_catalog_unions_absent_manual_model_unless_explicitly_disabled() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_models
        .entry(String::from("custom:test"))
        .or_default()
        .insert(
            String::from("manual/chat"),
            crate::CustomModelDefinition {
                id: String::from("manual/chat"),
                display_name: String::from("Manual Chat"),
                enabled: true,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    ..ModelCapabilities::default()
                },
                request_projection: crate::CustomModelRequestProjection::default(),
                tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                system_binding: None,
            },
        );
    let source = crate::parse_openai_model_catalog(
        "custom-account",
        1,
        r#"{"data":[{"id":"discovered/chat"}]}"#,
    )
    .expect("authoritative catalog should parse");
    let mut merged = source.clone();
    apply_model_catalog_overlays(&configuration, "custom:test", &mut merged)
        .expect("catalog should resolve");
    assert!(merged.models.iter().any(|model| model.id == "manual/chat"));
    assert!(
        merged
            .models
            .iter()
            .any(|model| model.id == "discovered/chat")
    );

    configuration
        .custom_model_connection_enabled_overrides
        .insert(
            PersistedAiConfiguration::connection_model_key("custom-account", "manual/chat"),
            false,
        );
    let mut explicitly_disabled = source;
    apply_model_catalog_overlays(&configuration, "custom:test", &mut explicitly_disabled)
        .expect("explicitly disabled catalog should resolve");
    assert!(
        explicitly_disabled
            .models
            .iter()
            .all(|model| model.id != "manual/chat")
    );
}

/// Verifies an empty discovery result retains the independently authorized manual source.
/// 验证空发现结果仍保留独立授权的手工来源。
#[test]
fn empty_custom_discovery_catalog_retains_manual_model_unless_explicitly_disabled() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_models
        .entry(String::from("custom:test"))
        .or_default()
        .insert(
            String::from("manual/chat"),
            crate::CustomModelDefinition {
                id: String::from("manual/chat"),
                display_name: String::from("Manual Chat"),
                enabled: true,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    ..ModelCapabilities::default()
                },
                request_projection: crate::CustomModelRequestProjection::default(),
                tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                system_binding: None,
            },
        );
    let source = crate::parse_openai_model_catalog("custom-account", 1, r#"{"data":[]}"#)
        .expect("authoritative empty catalog should parse");
    let mut merged = source.clone();
    apply_model_catalog_overlays(&configuration, "custom:test", &mut merged)
        .expect("empty discovery catalog should merge with manual identities");
    assert!(merged.models.iter().any(|model| model.id == "manual/chat"));

    configuration
        .custom_model_connection_enabled_overrides
        .insert(
            PersistedAiConfiguration::connection_model_key("custom-account", "manual/chat"),
            false,
        );
    let mut explicitly_disabled = source;
    apply_model_catalog_overlays(&configuration, "custom:test", &mut explicitly_disabled)
        .expect("explicitly disabled empty catalog should resolve");
    assert!(explicitly_disabled.models.is_empty());
}

/// Verifies source and membership policies jointly expose the exact identity union.
/// 验证来源策略与成员策略共同公开精确身份并集。
#[test]
fn model_source_policies_control_runtime_identity_union() {
    use crate::provider_manifest::{CatalogMembershipPolicy, ManifestOwnership, ModelSourcePolicy};

    // sourcePolicies and membershipPolicies form the complete 6x3 closed policy matrix.
    // sourcePolicies 与 membershipPolicies 组成完整的 6x3 封闭策略矩阵。
    let source_policies = [
        ModelSourcePolicy::ManifestOnly,
        ModelSourcePolicy::DiscoveryOnly,
        ModelSourcePolicy::ManifestAndDiscovery,
        ModelSourcePolicy::ManualOnly,
        ModelSourcePolicy::DiscoveryAndManual,
        ModelSourcePolicy::ManifestDiscoveryAndManual,
    ];
    let membership_policies = [
        CatalogMembershipPolicy::DeclaredOnly,
        CatalogMembershipPolicy::DiscoveryEvidence,
        CatalogMembershipPolicy::DeclaredAndDiscovery,
    ];
    for source_policy in source_policies {
        for membership_policy in membership_policies {
            let discovery_kind = if source_policy.includes_discovery() {
                "open_ai"
            } else if source_policy.includes_manifest() {
                "manifest_static"
            } else {
                "manual"
            };
            // modelCatalog is assembled from the policy truth table without optional fallback fields.
            // modelCatalog 依据策略真值表组装，不包含可选兜底字段。
            let mut model_catalog = serde_json::json!({
                "source_policy": serde_json::to_value(source_policy).expect("policy must serialize"),
                "discovery_kind": discovery_kind
            });
            if source_policy.includes_manifest() {
                model_catalog["models"] = serde_json::json!([
                    {"model_id": "manifest-model", "display_name": "Manifest Model"}
                ]);
            }
            let manifest_value = serde_json::json!({
                "schema_version": "vulcan.provider-manifest.v2",
                "provider_id": "source-policy-probe",
                "revision": 1,
                "published_at": "2026-08-09T00:00:00Z",
                "engine_contract": {
                    "minimum_manifest_engine": 1,
                    "required_protocol_profiles": [OPENAI_CHAT_PROFILE_ID]
                },
                "display": {"name": "Source Policy Probe", "category": "official"},
                "services": [{
                    "service_entry_id": "public-api",
                    "display_name": "Public API",
                    "kind": "conversation",
                    "base_url": "https://source-policy.example.test/v1",
                    "credential_profiles": [{
                        "credential_profile_id": "api-key",
                        "display_name": "API Key",
                        "credential_mode": "token"
                    }],
                    "default_authentication": "bearer_authorization",
                    "model_catalog": model_catalog,
                    "protocol_routes": [{
                        "protocol_profile_id": OPENAI_CHAT_PROFILE_ID,
                        "endpoint_path": "/chat/completions",
                        "verified": true
                    }]
                }],
                "model_defaults": {
                    "default_enabled": true,
                    "catalog_membership_policy": serde_json::to_value(membership_policy)
                        .expect("membership policy must serialize"),
                    "unknown_model_policy": "declared_rule_required",
                    "supported_protocols": [OPENAI_CHAT_PROFILE_ID],
                    "selected_protocol": OPENAI_CHAT_PROFILE_ID,
                    "capabilities": {"operations": ["conversation_respond"]}
                },
                "model_rules": [
                    {
                        "rule_id": "manifest-model",
                        "matcher": {"kind": "exact", "value": "manifest-model"},
                        "supported_protocols": [OPENAI_CHAT_PROFILE_ID],
                        "selected_protocol": OPENAI_CHAT_PROFILE_ID
                    },
                    {
                        "rule_id": "discovery-model",
                        "matcher": {"kind": "exact", "value": "discovery-model"},
                        "supported_protocols": [OPENAI_CHAT_PROFILE_ID],
                        "selected_protocol": OPENAI_CHAT_PROFILE_ID
                    }
                ],
                "provenance": {"status": "migrated", "sources": []}
            });
            let manifest = crate::provider_manifest::compile_manifest_text(
                serde_json::to_string(&manifest_value)
                    .expect("source-policy manifest must serialize")
                    .as_str(),
                ManifestOwnership::System,
            )
            .expect("source-policy manifest must compile");
            let connection = CredentialConnection {
                id: String::from("connection-source-policy"),
                provider_id: String::from("source-policy-probe"),
                service_entry_id: String::from("public-api"),
                credential_profile_id: String::from("api-key"),
                credential_mode: CredentialMode::Token,
                credential_id: String::from("credential-source-policy"),
                display_name: String::from("Source Policy"),
                secret_reference: String::from("secret-source-policy"),
                management_secret_reference: None,
                service_origin_override: None,
                prompt_cache_by_protocol: BTreeMap::new(),
            };
            let mut configuration = PersistedAiConfiguration::default();
            configuration
                .connection_graph
                .connections
                .insert(connection.id.clone(), connection.clone());
            configuration.custom_models.insert(
                String::from("source-policy-probe"),
                BTreeMap::from([(
                    String::from("manual-model"),
                    crate::CustomModelDefinition {
                        id: String::from("manual-model"),
                        display_name: String::from("Manual Model"),
                        enabled: true,
                        capabilities: ModelCapabilities {
                            operations: vec![AiOperation::ConversationRespond],
                            context_limit: Some(200_000),
                            output_limit: Some(8_000),
                            ..ModelCapabilities::default()
                        },
                        request_projection: Default::default(),
                        tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                        system_binding: Some(crate::SystemManualModelBinding {
                            service_entry_id: String::from("public-api"),
                            protocol_profile_id: OPENAI_CHAT_PROFILE_ID.to_string(),
                        }),
                    },
                )]),
            );
            let mut source_catalog = ConnectionModelCatalog {
                connection_id: connection.id.clone(),
                models: ["discovery-model", "unmatched-discovery-model"]
                    .into_iter()
                    .map(|model_id| ConnectionModel {
                        id: model_id.to_string(),
                        display_name: model_id.to_string(),
                        capabilities: ModelCapabilities {
                            operations: vec![AiOperation::ConversationRespond],
                            ..ModelCapabilities::default()
                        },
                        capability_evidence: ModelCapabilityEvidence::default(),
                        protocol_variants: BTreeMap::from([(
                            OPENAI_CHAT_PROFILE_ID.to_string(),
                            ProtocolModelVariant {
                                capabilities: ModelCapabilities {
                                    operations: vec![AiOperation::ConversationRespond],
                                    ..ModelCapabilities::default()
                                },
                                capability_evidence: ModelCapabilityEvidence::default(),
                                discovery_signature: String::from("sha256:discovery"),
                                catalog_generation: 7,
                            },
                        )]),
                        resolved_protocol_binding: None,
                    })
                    .collect(),
                generation: 7,
                page_count: 1,
                discovery_schema_revision: String::from("openai-compatible.models.v1"),
            };
            apply_model_catalog_overlays_with_account_access(
                &configuration,
                "source-policy-probe",
                "public-api",
                None,
                source_policy,
                Some(&manifest),
                &mut source_catalog,
            )
            .expect("source-policy overlay must resolve");
            let mut expected_model_ids = BTreeSet::new();
            if source_policy.includes_manifest() && membership_policy.includes_declared_identities()
            {
                expected_model_ids.insert(String::from("manifest-model"));
            }
            if source_policy.includes_discovery() {
                expected_model_ids.insert(String::from("discovery-model"));
                if membership_policy.admits_unmatched_discovery() {
                    expected_model_ids.insert(String::from("unmatched-discovery-model"));
                }
            }
            // Explicit system-user identity is independent from the Manifest's source policy.
            // 系统用户显式身份独立于 Manifest 的来源策略。
            expected_model_ids.insert(String::from("manual-model"));
            let actual_model_ids = source_catalog
                .models
                .iter()
                .map(|model| model.id.clone())
                .collect::<BTreeSet<_>>();
            assert_eq!(
                actual_model_ids, expected_model_ids,
                "source {source_policy:?}, membership {membership_policy:?}"
            );
            let manual_model = source_catalog
                .models
                .iter()
                .find(|model| model.id == "manual-model")
                .expect("explicit user source must admit the manual identity");
            assert!(
                manual_model
                    .protocol_variants
                    .contains_key(OPENAI_CHAT_PROFILE_ID),
                "explicit user choice must produce the manual model's executable variant"
            );
            assert_eq!(
                manual_model
                    .resolved_protocol_binding
                    .as_ref()
                    .expect("manual model binding")
                    .source,
                crate::ModelProtocolBindingSource::SystemUser,
            );
        }
    }
}
