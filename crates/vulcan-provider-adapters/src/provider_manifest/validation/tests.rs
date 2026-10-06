use std::collections::BTreeMap;

use super::*;
use crate::{
    AccountUsageQueryKind, CredentialBillingKind, CredentialMode,
    provider_manifest::matcher::{MatcherKind, RuleMatcher},
    provider_manifest::schema::{
        EngineContract, ManifestCapabilityPatch, ManifestCatalogModel, ManifestDisplay,
        ManifestModelCatalog, ManifestProvenance, ModelDefaults, ProvenanceStatus,
        ProviderManifestV1, RuleScope,
    },
};

/// Verify Batch endpoints remain inside the declared service origin and API root.
/// 验证 Batch 端点始终位于声明的服务源地址及 API 根路径内。
#[test]
fn native_batch_endpoint_rejects_origin_and_path_escape() {
    assert!(valid_native_batch_endpoint("/batches/{batch_id}"));
    assert!(!valid_native_batch_endpoint("//attacker.invalid/batches"));
    assert!(!valid_native_batch_endpoint("/batches/../files"));
    assert!(!valid_native_batch_endpoint("/batches?limit=1"));
    assert!(!valid_native_batch_endpoint("/batches\\escape"));
}

/// Builds a minimal valid manifest fixture.
/// 构建最小有效清单 fixture。
fn minimal_manifest() -> ProviderManifestV1 {
    ProviderManifestV1 {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        provider_id: String::from("example"),
        revision: 1,
        published_at: String::from("2026-08-06T00:00:00Z"),
        engine_contract: EngineContract {
            minimum_manifest_engine: 1,
            required_protocol_profiles: vec![String::from("openai-responses.v1")],
            required_execution_primitives: Vec::new(),
            required_upload_primitives: Vec::new(),
            required_transformers: Vec::new(),
        },
        display: ManifestDisplay {
            name: String::from("Example"),
            category: Default::default(),
            credential_directory_visibility: Default::default(),
            promotion: None,
        },
        credential_registration: Default::default(),
        services: vec![crate::provider_manifest::schema::ManifestServiceEntry {
            service_entry_id: String::from("public-api"),
            display_name: String::from("Public API"),
            kind: Default::default(),
            base_url: String::from("https://api.example.invalid/v1"),
            credential_profiles: vec![
                crate::provider_manifest::schema::ManifestCredentialProfile {
                    credential_profile_id: String::from("api-key"),
                    display_name: String::from("API Key"),
                    credential_mode: CredentialMode::Token,
                    authorization: None,
                    billing_kind: CredentialBillingKind::ProviderManaged,
                    account_usage_query: AccountUsageQueryKind::Unsupported,
                    account_model_access: None,
                    authentication: None,
                    key_prefix: None,
                    credential_role: None,
                    default_usage_scopes: Default::default(),
                },
            ],
            default_authentication: None,
            request_headers: Default::default(),
            session_header: None,
            service_origin_override_policy: None,
            native_inference_bindings: Vec::new(),
            request_projector: None,
            model_catalog: Some(ManifestModelCatalog {
                source_policy: ModelSourcePolicy::ManualOnly,
                discovery_kind: Some(ModelDiscoveryKind::Manual),
                path: None,
                models: None,
            }),
            protocol_routes: vec![crate::provider_manifest::schema::ManifestProtocolRoute {
                protocol_profile_id: String::from("openai-responses.v1"),
                endpoint_path: Some(String::from("/responses")),
                base_url_override: None,
                authentication: None,
                request_headers: Default::default(),
                model_discovery: None,
                model_discovery_path: None,
                model_discovery_base_url_override: None,
                responses_features: None,
                input_resource_constraints: Vec::new(),
                usage_profile_refs: Vec::new(),
                request_profile_refs: Vec::new(),
                response_profile_refs: Vec::new(),
                provider_request_id_headers: Vec::new(),
                verified: true,
                reasoning_replay_contract: None,
            }],
            model_defaults_override: None,
        }],
        model_defaults: ModelDefaults {
            default_enabled: true,
            catalog_membership_policy: Default::default(),
            unknown_model_policy: Default::default(),
            supported_protocols: vec![String::from("openai-responses.v1")],
            selected_protocol: Some(String::from("openai-responses.v1")),
            capabilities: None,
            tool_schema_projection: Default::default(),
            default_usage_profile_refs: Vec::new(),
            default_request_profile_refs: Vec::new(),
            default_response_profile_refs: Vec::new(),
            default_operation_refs: Vec::new(),
            provenance_ref: None,
        },
        model_rules: Vec::new(),
        usage_profiles: Vec::new(),
        request_profiles: Vec::new(),
        response_profiles: Vec::new(),
        upload_channels: Vec::new(),
        operations: Vec::new(),
        tool_targets: Vec::new(),
        tool_target_templates: Vec::new(),
        tool_target_template_bindings: Vec::new(),
        provenance: ManifestProvenance {
            status: ProvenanceStatus::ExampleOnly,
            sources: Vec::new(),
            verified_at: None,
        },
    }
}

fn registry() -> EnginePrimitiveRegistry {
    EnginePrimitiveRegistry::current_protocol_baseline()
}

/// Verifies the minimal manifest validates cleanly.
/// 验证最小清单能干净通过校验。
#[test]
fn minimal_manifest_validates() {
    let manifest = minimal_manifest();
    let result = validate_manifest(&manifest, ManifestOwnership::System, &registry());
    assert_eq!(result, Ok(()), "minimal manifest must validate: {result:?}");
}

/// Verifies inline Responses compaction V2 cannot be declared before its streamed trigger executor exists.
/// 验证流式触发执行器存在之前不能声明 Responses 内联压缩 V2。
#[test]
fn inline_responses_compaction_v2_fails_closed_without_runtime_primitive() {
    let mut manifest = minimal_manifest();
    let profile_id = String::from(vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID);
    manifest.engine_contract.required_protocol_profiles = vec![profile_id.clone()];
    manifest.model_defaults.supported_protocols = vec![profile_id.clone()];
    manifest.model_defaults.selected_protocol = Some(profile_id.clone());
    let route = manifest.services[0]
        .protocol_routes
        .first_mut()
        .expect("minimal manifest should contain one route");
    route.protocol_profile_id = profile_id;
    route.responses_features = Some(crate::ResponsesRouteFeatures {
        remote_compaction: crate::ResponsesRemoteCompactionPolicy::InlineV2,
        websocket: Default::default(),
        recovery: Default::default(),
    });

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("inline V2 must fail until its runtime primitive is registered");
    assert!(
        errors
            .iter()
            .any(|error| { error.code == "responses_inline_compaction_v2_runtime_unavailable" })
    );
}

/// Verifies every used protocol must be declared by the manifest engine contract.
/// 验证每个已使用协议都必须由 Manifest 引擎契约声明。
#[test]
fn used_protocol_requires_engine_contract_declaration() {
    let mut manifest = minimal_manifest();
    manifest.engine_contract.required_protocol_profiles.clear();
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("a used but undeclared protocol must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "engine_contract_missing_used_protocol")
    );
}

/// Verifies used execution and mapping primitives cannot hide outside the engine contract.
/// 验证已使用执行与映射原语不能隐藏在引擎契约之外。
#[test]
fn used_execution_and_transformer_require_contract_declarations() {
    let path =
        vulcan_runtime_assets::source_bundled_provider_manifests_directory().join("tavily.json");
    let text = std::fs::read_to_string(path).expect("Tavily manifest must be readable");
    let manifest: ProviderManifestV1 =
        serde_json::from_str(text.as_str()).expect("Tavily manifest must parse");

    let mut missing_execution = manifest.clone();
    missing_execution
        .engine_contract
        .required_execution_primitives
        .clear();
    let execution_errors =
        validate_manifest(&missing_execution, ManifestOwnership::System, &registry())
            .expect_err("a used but undeclared execution primitive must fail");
    assert!(
        execution_errors
            .iter()
            .any(|error| { error.code == "engine_contract_missing_used_execution_primitive" })
    );

    let mut missing_transformer = manifest;
    missing_transformer
        .engine_contract
        .required_transformers
        .clear();
    let transformer_errors =
        validate_manifest(&missing_transformer, ManifestOwnership::System, &registry())
            .expect_err("a used but undeclared transformer must fail");
    assert!(
        transformer_errors
            .iter()
            .any(|error| error.code == "engine_contract_missing_used_transformer")
    );
}

/// Verifies engine-contract arrays are bounded, unique, and stable identifiers.
/// 验证引擎契约数组有界、唯一且只包含稳定标识。
#[test]
fn engine_contract_requirement_lists_are_closed() {
    let mut manifest = minimal_manifest();
    manifest.engine_contract.required_protocol_profiles =
        vec![String::from("openai-responses.v1"); MAX_ENGINE_CONTRACT_REQUIREMENTS + 1];
    manifest.engine_contract.required_execution_primitives = vec![
        String::from("http_json_sync"),
        String::from("http_json_sync"),
    ];
    manifest.engine_contract.required_transformers = vec![String::from("bad/transformer")];

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("unbounded, duplicate, or unstable engine requirements must fail");
    for expected in [
        "too_many_engine_contract_requirements",
        "duplicate_engine_contract_requirement",
        "invalid_identifier_charset",
    ] {
        assert!(
            errors.iter().any(|error| error.code == expected),
            "missing {expected}: {errors:?}"
        );
    }
}

/// Verifies a declared reasoning replay contract must close over the engine contract.
/// 验证已声明的推理回传契约必须在引擎契约中闭合。
#[test]
fn reasoning_replay_contract_requires_declared_engine_primitive() {
    // contract is the exact closed value a DeepSeek Responses route declares.
    // contract 是 DeepSeek Responses 路由声明的精确封闭取值。
    let contract =
        crate::provider_manifest::schema::ReasoningReplayContract::DeepseekResponsesPlaintextV1;
    let mut manifest = minimal_manifest();
    manifest.services[0].protocol_routes[0].reasoning_replay_contract = Some(contract);

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("an undeclared reasoning replay contract must fail closed");
    assert!(
        errors.iter().any(|error| {
            error.code == "engine_contract_missing_used_execution_primitive"
                && error.path.ends_with("reasoning_replay_contract")
        }),
        "the reasoning replay contract gate did not fire: {errors:?}"
    );

    // declared closes the same contract, so the manifest must then validate cleanly.
    // declared 闭合同一契约，因此该 Manifest 必须通过校验。
    manifest
        .engine_contract
        .required_execution_primitives
        .push(String::from(contract.primitive_id()));
    validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect("a declared reasoning replay contract must validate");
}

/// Verifies publication and provenance timestamps are real canonical UTC instants.
/// 验证发布时间与来源验证时间均为真实、标准的 UTC 时刻。
#[test]
fn audit_timestamps_require_canonical_utc_rfc3339() {
    let mut manifest = minimal_manifest();
    manifest.published_at = String::from("2026-99-99T00:00:00Z");
    manifest.provenance.verified_at = Some(String::from("not-a-dateZ"));

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("malformed audit timestamps must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "invalid_published_at")
    );
    assert!(
        errors
            .iter()
            .any(|error| error.code == "invalid_provenance_verified_at")
    );
}

/// Verifies a verified manifest cannot claim authority without an exact evidence source.
/// 验证已验证清单不能在缺少精确证据来源时声明权威。
#[test]
fn verified_manifest_requires_evidence_source() {
    let mut manifest = minimal_manifest();
    manifest.provenance.status = ProvenanceStatus::Verified;
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("verified provenance without evidence must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "verified_manifest_requires_evidence_source")
    );
}

/// Verifies model-level evidence references must resolve to an exact top-level source.
/// 验证模型级证据引用必须精确解析到顶层来源。
#[test]
fn model_provenance_reference_must_resolve() {
    let mut manifest = minimal_manifest();
    manifest
        .provenance
        .sources
        .push(crate::provider_manifest::ProvenanceSource {
            url: String::from("https://docs.example.invalid/models"),
            note: String::from("Defines the reviewed model contract."),
        });
    manifest.model_defaults.provenance_ref =
        Some(String::from("https://docs.example.invalid/other"));
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("a dangling model evidence reference must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "dangling_provenance_reference")
    );
}

/// Verifies default binding requires a signed capability snapshot and a verified entry route.
/// 验证默认绑定要求签名能力快照与已验证入口路由。
#[test]
fn default_binding_authority_is_complete_per_entry() {
    let mut manifest = minimal_manifest();
    manifest.model_defaults.unknown_model_policy = UnknownModelPolicy::DefaultBindingAllowed;
    let missing_capabilities = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("missing default capabilities must fail");
    assert!(
        missing_capabilities
            .iter()
            .any(|error| error.code == "default_binding_allowed_requires_capabilities")
    );

    manifest.model_defaults.capabilities = Some(ManifestCapabilityPatch {
        operations: Some(vec![vulcan_ai_contract::AiOperation::ConversationRespond]),
        evidence_revision: Some(String::from("validation.default-capabilities.v1")),
        ..ManifestCapabilityPatch::default()
    });
    manifest.services[0].protocol_routes[0].verified = false;
    let unverified_route = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("an unverified default route must fail");
    assert!(
        unverified_route
            .iter()
            .any(|error| error.code == "default_protocol_route_not_verified")
    );
}

/// Verifies every Manifest identity needs scoped rule authority under declared-rule policy.
/// 验证 declared-rule 策略下每个 Manifest 身份都需要作用域规则权威。
#[test]
fn non_static_manifest_identity_requires_rule_coverage() {
    let mut manifest = minimal_manifest();
    manifest.services[0].model_catalog = Some(ManifestModelCatalog {
        source_policy: ModelSourcePolicy::ManifestAndDiscovery,
        discovery_kind: Some(ModelDiscoveryKind::OpenAi),
        path: Some(String::from("/models")),
        models: Some(vec![crate::provider_manifest::ManifestCatalogModel {
            model_id: String::from("declared-model"),
            display_name: String::from("Declared Model"),
        }]),
    });
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("an unbound Manifest identity must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "manifest_model_requires_rule")
    );
}

/// Verifies manifest-static discovery requires closed inventory and rule authority.
/// 验证 Manifest 静态发现要求封闭清单与规则权威。
#[test]
fn manifest_static_discovery_requires_inventory_and_rule_coverage() {
    // missing_inventory_manifest declares static authority without its required inventory.
    // missing_inventory_manifest 声明静态权威但缺少必需清单。
    let mut missing_inventory_manifest = minimal_manifest();
    missing_inventory_manifest.services[0].model_catalog = Some(ManifestModelCatalog {
        source_policy: ModelSourcePolicy::ManifestAndDiscovery,
        discovery_kind: Some(ModelDiscoveryKind::ManifestStatic),
        path: None,
        models: None,
    });
    // missing_inventory_errors must identify the exact closed-contract violation.
    // missing_inventory_errors 必须标识精确的封闭契约违规。
    let missing_inventory_errors = validate_manifest(
        &missing_inventory_manifest,
        ManifestOwnership::System,
        &registry(),
    )
    .expect_err("manifest-static discovery without models must fail");
    assert!(
        missing_inventory_errors
            .iter()
            .any(|error| error.code == "manifest_static_requires_catalog_models")
    );

    // missing_rule_manifest adds one identity but deliberately omits its execution rule.
    // missing_rule_manifest 添加一个身份但刻意省略其执行规则。
    let mut missing_rule_manifest = missing_inventory_manifest.clone();
    missing_rule_manifest.services[0]
        .model_catalog
        .as_mut()
        .expect("catalog must exist")
        .models = Some(vec![ManifestCatalogModel {
        model_id: String::from("future-model"),
        display_name: String::from("Future Model"),
    }]);
    // missing_rule_errors proves binary/common defaults cannot authorize a static identity.
    // missing_rule_errors 证明二进制/公共默认不能授权静态身份。
    let missing_rule_errors = validate_manifest(
        &missing_rule_manifest,
        ManifestOwnership::System,
        &registry(),
    )
    .expect_err("manifest-static identity without a rule must fail");
    assert!(
        missing_rule_errors
            .iter()
            .any(|error| error.code == "manifest_model_requires_rule")
    );

    // remote_location_manifest attempts to attach a network path to a non-network catalog.
    // remote_location_manifest 尝试给非网络目录附加网络路径。
    let mut remote_location_manifest = missing_rule_manifest;
    remote_location_manifest.services[0]
        .model_catalog
        .as_mut()
        .expect("catalog must exist")
        .path = Some(String::from("/models"));
    // remote_location_errors proves ManifestStatic cannot disguise a remote request.
    // remote_location_errors 证明 ManifestStatic 不能伪装远程请求。
    let remote_location_errors = validate_manifest(
        &remote_location_manifest,
        ManifestOwnership::System,
        &registry(),
    )
    .expect_err("manifest-static remote location must fail");
    assert!(
        remote_location_errors
            .iter()
            .any(|error| { error.code == "manifest_static_forbids_remote_discovery_location" })
    );
}

/// Verifies unknown schema versions are rejected.
/// 验证未知 Schema 版本被拒绝。
#[test]
fn unknown_schema_version_rejected() {
    let mut manifest = minimal_manifest();
    manifest.schema_version = String::from("vulcan.provider-manifest.v1");
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("unsupported schema must fail");
    assert!(
        errors
            .iter()
            .any(|e| e.code == "unsupported_schema_version")
    );
    assert!(errors.iter().any(|error| {
        error.code == "unsupported_schema_version" && error.message.contains("upgrade")
    }));
}

/// Verifies a manifest requiring a newer engine fails closed and requests a client upgrade.
/// 验证要求更高引擎版本的 Manifest 会失败关闭并要求升级客户端。
#[test]
fn newer_manifest_engine_requires_client_upgrade() {
    // manifest requires the next unsupported engine generation.
    // manifest 要求下一个尚不支持的引擎代际。
    let mut manifest = minimal_manifest();
    manifest.engine_contract.minimum_manifest_engine = super::super::CURRENT_MANIFEST_ENGINE + 1;
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("newer manifest engine must fail closed");
    assert!(errors.iter().any(|error| {
        error.code == "unsupported_engine_contract" && error.message.contains("upgrade")
    }));
}

/// Verifies additive total derivation cannot be advertised under an older engine contract.
/// 验证总额加法派生不能在旧版引擎契约下发布。
#[test]
fn additive_total_usage_derivation_requires_engine_two() {
    // manifest intentionally declares the new derivation while retaining engine one.
    // manifest 故意在保留引擎一的同时声明新增派生。
    let mut manifest = minimal_manifest();
    manifest.usage_profiles = vec![crate::provider_manifest::schema::UsageProfileDeclaration {
        profile_id: String::from("test.additive-total.v1"),
        description: String::from("test additive total"),
        payload: Some(vulcan_llm_core::UsageExtractionProfile {
            profile_id: String::from("test.additive-total.v1"),
            revision: 1,
            record_kind: vulcan_llm_core::UsageRecordKind::JsonBody,
            update_kind: vulcan_ai_contract::UsageUpdateKind::Final,
            event_type_pointer: None,
            event_type_value: None,
            required_pointer: Some(String::from("/usage")),
            bindings: vec![
                vulcan_llm_core::UsageFieldBinding {
                    field: vulcan_llm_core::UsageField::UncachedInputTokens,
                    pointers: vec![String::from("/usage/input_tokens")],
                    encoding: vulcan_llm_core::UsageValueEncoding::UnsignedInteger,
                },
                vulcan_llm_core::UsageFieldBinding {
                    field: vulcan_llm_core::UsageField::OutputTokens,
                    pointers: vec![String::from("/usage/output_tokens")],
                    encoding: vulcan_llm_core::UsageValueEncoding::UnsignedInteger,
                },
            ],
            derivations: vec![vulcan_llm_core::UsageDerivation::TotalFromInputComponentsAndOutput],
        }),
    }];

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("additive total usage derivation must require engine two");
    assert!(errors.iter().any(|error| {
        error.code == "engine_contract_missing_usage_derivation_version"
            && error.path == "$.usage_profiles[0].payload.derivations"
    }));
}

/// Verifies revision zero is rejected.
/// 验证修订号为零被拒绝。
#[test]
fn revision_zero_rejected() {
    let mut manifest = minimal_manifest();
    manifest.revision = 0;
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("zero revision must fail");
    assert!(errors.iter().any(|e| e.code == "revision_must_be_positive"));
}

/// Verifies session-header names accept valid names while runtime projection handles conflicts.
/// 验证会话请求头名称接受合法名称，并由运行时投影处理冲突。
#[test]
fn session_header_name_contract_is_validated() {
    let mut manifest = minimal_manifest();
    manifest.services[0].session_header = Some(String::from("x-opencode-session"));
    validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect("safe session header name should compile");

    manifest.services[0].session_header = Some(String::from("authorization"));
    validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect("host-owned session header may be ignored by runtime projection");

    manifest.services[0].session_header = Some(String::from("x invalid"));
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("invalid session header name must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "invalid_session_header")
    );
}

/// Verifies provider-tool entries cannot declare a regular-conversation session header.
/// 验证供应商工具入口不能声明常规会话请求头。
#[test]
fn provider_tool_entry_rejects_session_header() {
    let mut manifest = minimal_manifest();
    manifest.services[0].kind = ManifestEntryKind::ProviderTools;
    manifest.services[0].model_catalog = None;
    manifest.services[0].session_header = Some(String::from("x-provider-session"));
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("provider-tool session headers must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "session_header_requires_conversation")
    );
}

/// Verifies system manifests cannot use the custom prefix.
/// 验证系统清单不能使用 custom 前缀。
#[test]
fn system_manifest_rejects_custom_prefix() {
    let mut manifest = minimal_manifest();
    manifest.provider_id = String::from("custom:example");
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("system custom prefix must fail");
    assert!(
        errors
            .iter()
            .any(|e| e.code == "system_provider_cannot_use_custom_prefix")
    );
}

/// Verifies custom manifests require the custom prefix.
/// 验证自定义清单必须使用 custom 前缀。
#[test]
fn custom_manifest_requires_custom_prefix() {
    let mut manifest = minimal_manifest();
    manifest.provider_id = String::from("example");
    let errors = validate_manifest(&manifest, ManifestOwnership::Custom, &registry())
        .expect_err("custom manifest without prefix must fail");
    assert!(
        errors
            .iter()
            .any(|e| e.code == "custom_provider_requires_custom_prefix")
    );
}

/// Verifies custom manifests cannot activate a system credential-onboarding primitive.
/// 验证自定义清单不能激活系统凭据接入原语。
#[test]
fn custom_manifest_requires_direct_credential_registration() {
    let mut manifest = minimal_manifest();
    manifest.provider_id = String::from("custom:example");
    manifest.credential_registration = crate::CredentialRegistrationKind::MiniMaxRegionalApiKey;
    let errors = validate_manifest(&manifest, ManifestOwnership::Custom, &registry())
        .expect_err("custom manifest with system onboarding must fail");
    assert!(
        errors.iter().any(|error| {
            error.code == "custom_provider_requires_direct_credential_registration"
        })
    );
}

/// Verifies unregistered protocol profiles are rejected.
/// 验证未注册协议 Profile 被拒绝。
#[test]
fn unregistered_protocol_rejected() {
    let mut manifest = minimal_manifest();
    manifest.services[0].protocol_routes[0].protocol_profile_id =
        String::from("unknown-protocol.v9");
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("unknown protocol must fail");
    assert!(errors.iter().any(|e| e.code == "unknown_protocol_profile"));
}

/// Verifies forbidden headers are rejected.
/// 验证禁用请求头被拒绝。
#[test]
fn forbidden_header_rejected() {
    let mut manifest = minimal_manifest();
    manifest.services[0]
        .request_headers
        .insert(String::from("Authorization"), String::from("Bearer x"));
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("forbidden header must fail");
    assert!(errors.iter().any(|e| e.code == "forbidden_header"));
}

/// Verifies request-id response headers are bounded, valid, unique, and non-sensitive.
/// 验证请求标识响应头有界、有效、唯一且不敏感。
#[test]
fn invalid_provider_request_id_headers_are_rejected() {
    // manifest owns one route whose diagnostic header declaration is deliberately malformed.
    // manifest 持有一条诊断响应头声明被刻意破坏的路由。
    let mut manifest = minimal_manifest();
    // route is the exact protocol declaration under semantic validation.
    // route 是接受语义校验的精确协议声明。
    let route = &mut manifest.services[0].protocol_routes[0];
    route.provider_request_id_headers = vec![
        String::from("x-request-id"),
        String::from("X-Request-ID"),
        String::from("set-cookie"),
        String::from("bad header"),
        String::from("x-request-id-2"),
        String::from("x-request-id-3"),
        String::from("x-request-id-4"),
        String::from("x-request-id-5"),
        String::from("x-request-id-6"),
    ];

    // errors contains every independent violation so one bad field never hides another.
    // errors 包含每项独立违规，使单个坏字段不会掩盖其他问题。
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("invalid request-id headers must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "too_many_request_id_headers")
    );
    assert!(
        errors
            .iter()
            .any(|error| error.code == "invalid_request_id_header_name")
    );
    assert!(
        errors
            .iter()
            .any(|error| error.code == "forbidden_request_id_header")
    );
    assert!(
        errors
            .iter()
            .any(|error| error.code == "duplicate_request_id_header")
    );
}

/// Verifies non-HTTPS URLs are rejected for system manifests.
/// 验证系统清单的非 HTTPS URL 被拒绝。
#[test]
fn system_manifest_rejects_http_url() {
    let mut manifest = minimal_manifest();
    manifest.services[0].base_url = String::from("http://api.example.invalid/v1");
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("http url must fail for system manifest");
    assert!(errors.iter().any(|e| e.code == "invalid_url_scheme"));
}

/// Verifies conflicting same-rank rules reject the whole manifest.
/// 验证同优先级冲突规则拒绝整份清单。
#[test]
fn conflicting_same_rank_rules_reject_manifest() {
    let mut manifest = minimal_manifest();
    manifest.model_rules = vec![
        ModelRule {
            rule_id: String::from("r1"),
            scope: Default::default(),
            matcher: RuleMatcher {
                kind: MatcherKind::Exact,
                value: String::from("model-a"),
            },
            display_name: None,
            supported_protocols: vec![String::from("openai-responses.v1")],
            selected_protocol: Some(String::from("openai-responses.v1")),
            codex_behavior: None,
            capabilities: None,
            tool_schema_projection: Default::default(),
            protocol_capabilities: Default::default(),
            input_resource_constraints: Vec::new(),
            usage_profile_refs: None,
            request_profile_refs: None,
            response_profile_refs: None,
            model_parameters: Vec::new(),
            operation_refs: None,
            allowed_account_plan_names: Vec::new(),
            account_access_groups: Vec::new(),
            default_enabled: None,
            provenance_ref: None,
        },
        ModelRule {
            rule_id: String::from("r2"),
            scope: Default::default(),
            matcher: RuleMatcher {
                kind: MatcherKind::Exact,
                value: String::from("model-a"),
            },
            display_name: None,
            supported_protocols: vec![String::from("openai-responses.v1")],
            selected_protocol: Some(String::from("openai-responses.v1")),
            codex_behavior: None,
            capabilities: None,
            tool_schema_projection: Default::default(),
            protocol_capabilities: Default::default(),
            input_resource_constraints: Vec::new(),
            usage_profile_refs: None,
            request_profile_refs: None,
            response_profile_refs: None,
            model_parameters: Vec::new(),
            operation_refs: None,
            allowed_account_plan_names: Vec::new(),
            account_access_groups: Vec::new(),
            default_enabled: None,
            provenance_ref: None,
        },
    ];
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("conflicting rules must fail");
    assert!(
        errors.iter().any(|e| e.code == "duplicate_rule_authority")
            || errors.iter().any(|e| e.code == "incomparable_rule_scopes"),
        "identical same-rank scopes are duplicate authority: {errors:?}"
    );
}

/// Verifies overlapping-but-incomparable same-rank scopes reject the whole manifest.
/// 验证相交但不可比较的同优先级作用域拒绝整份清单。
#[test]
fn incomparable_same_rank_scopes_reject_manifest() {
    let mut manifest = minimal_manifest();
    // A second credential profile and a second entry make the two scopes intersect
    // without containment: r1 owns every profile of public-api, r2 owns api-key of
    // both entries.
    // 第二个凭据配置档与第二个入口使两个作用域相交但不互为子集：
    // r1 拥有 public-api 的全部配置档，r2 拥有两个入口的 api-key。
    manifest.services[0].credential_profiles.push(
        crate::provider_manifest::schema::ManifestCredentialProfile {
            credential_profile_id: String::from("admin-key"),
            display_name: String::from("Admin Key"),
            credential_mode: CredentialMode::Token,
            authorization: None,
            billing_kind: CredentialBillingKind::ProviderManaged,
            account_usage_query: AccountUsageQueryKind::Unsupported,
            account_model_access: None,
            authentication: None,
            key_prefix: None,
            credential_role: None,
            default_usage_scopes: Default::default(),
        },
    );
    manifest
        .services
        .push(crate::provider_manifest::schema::ManifestServiceEntry {
            service_entry_id: String::from("tools-api"),
            display_name: String::from("Tools API"),
            kind: Default::default(),
            base_url: String::from("https://tools.example.invalid/v1"),
            credential_profiles: vec![
                crate::provider_manifest::schema::ManifestCredentialProfile {
                    credential_profile_id: String::from("api-key"),
                    display_name: String::from("API Key"),
                    credential_mode: CredentialMode::Token,
                    authorization: None,
                    billing_kind: CredentialBillingKind::ProviderManaged,
                    account_usage_query: AccountUsageQueryKind::Unsupported,
                    account_model_access: None,
                    authentication: None,
                    key_prefix: None,
                    credential_role: None,
                    default_usage_scopes: Default::default(),
                },
            ],
            default_authentication: None,
            request_headers: Default::default(),
            session_header: None,
            service_origin_override_policy: None,
            native_inference_bindings: Vec::new(),
            request_projector: None,
            model_catalog: None,
            protocol_routes: Vec::new(),
            model_defaults_override: None,
        });
    manifest.model_rules = vec![
        ModelRule {
            rule_id: String::from("r1"),
            scope: RuleScope {
                service_entry_ids: vec![String::from("public-api")],
                credential_profile_ids: Vec::new(),
            },
            matcher: RuleMatcher {
                kind: MatcherKind::Exact,
                value: String::from("model-a"),
            },
            display_name: None,
            supported_protocols: vec![String::from("openai-responses.v1")],
            selected_protocol: Some(String::from("openai-responses.v1")),
            codex_behavior: None,
            capabilities: None,
            tool_schema_projection: Default::default(),
            protocol_capabilities: Default::default(),
            input_resource_constraints: Vec::new(),
            usage_profile_refs: None,
            request_profile_refs: None,
            response_profile_refs: None,
            model_parameters: Vec::new(),
            operation_refs: None,
            allowed_account_plan_names: Vec::new(),
            account_access_groups: Vec::new(),
            default_enabled: None,
            provenance_ref: None,
        },
        ModelRule {
            rule_id: String::from("r2"),
            scope: RuleScope {
                service_entry_ids: Vec::new(),
                credential_profile_ids: vec![String::from("api-key")],
            },
            matcher: RuleMatcher {
                kind: MatcherKind::Exact,
                value: String::from("model-a"),
            },
            display_name: None,
            supported_protocols: vec![String::from("openai-responses.v1")],
            selected_protocol: Some(String::from("openai-responses.v1")),
            codex_behavior: None,
            capabilities: None,
            tool_schema_projection: Default::default(),
            protocol_capabilities: Default::default(),
            input_resource_constraints: Vec::new(),
            usage_profile_refs: None,
            request_profile_refs: None,
            response_profile_refs: None,
            model_parameters: Vec::new(),
            operation_refs: None,
            allowed_account_plan_names: Vec::new(),
            account_access_groups: Vec::new(),
            default_enabled: None,
            provenance_ref: None,
        },
    ];
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("incomparable rules must fail");
    assert!(
        errors.iter().any(|e| e.code == "incomparable_rule_scopes"),
        "overlapping same-rank scopes without a subset relation must fail: {errors:?}"
    );
}

/// Verifies a rule protocol missing on the covered entry route is rejected at compile time.
/// 验证规则协议在覆盖入口路由缺失时编译期拒绝。
#[test]
fn rule_protocol_missing_on_entry_route_rejected() {
    let mut manifest = minimal_manifest();
    manifest.model_rules = vec![ModelRule {
        rule_id: String::from("r1"),
        scope: Default::default(),
        matcher: RuleMatcher {
            kind: MatcherKind::Exact,
            value: String::from("model-a"),
        },
        display_name: None,
        supported_protocols: vec![String::from("anthropic-messages.v1")],
        selected_protocol: Some(String::from("anthropic-messages.v1")),
        codex_behavior: None,
        capabilities: None,
        tool_schema_projection: Default::default(),
        protocol_capabilities: Default::default(),
        input_resource_constraints: Vec::new(),
        usage_profile_refs: None,
        request_profile_refs: None,
        response_profile_refs: None,
        model_parameters: Vec::new(),
        operation_refs: None,
        allowed_account_plan_names: Vec::new(),
        account_access_groups: Vec::new(),
        default_enabled: None,
        provenance_ref: None,
    }];
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("missing route must fail");
    assert!(
        errors
            .iter()
            .any(|e| e.code == "rule_protocol_missing_on_entry_route")
    );
}

/// Verifies unknown fields are rejected during parsing.
/// 验证解析期间未知字段被拒绝。
#[test]
fn unknown_field_rejected_during_parse() {
    let text = r#"{"schema_version":"vulcan.provider-manifest.v2","unknown_top_level":1}"#;
    let errors = parse_and_validate(text, ManifestOwnership::System, &registry())
        .expect_err("unknown field must fail");
    assert!(
        errors
            .iter()
            .any(|e| e.code == "invalid_manifest_structure")
    );
}

/// Verifies JSON size limits reject oversized documents.
/// 验证 JSON 大小限制拒绝超限文档。
#[test]
fn oversized_manifest_rejected() {
    let mut text = String::from("{\"padding\":\"");
    text.push_str(&"x".repeat(super::super::MAX_MANIFEST_BYTES + 1));
    text.push_str("\"}");
    let errors = parse_and_validate(&text, ManifestOwnership::System, &registry())
        .expect_err("oversized manifest must fail");
    assert!(errors.iter().any(|e| e.code == "manifest_too_large"));
}

/// Verifies that parse_and_validate round-trips a serialized minimal manifest.
/// 验证 parse_and_validate 能往返序列化的最小清单。
#[test]
fn serialized_manifest_round_trips() {
    let manifest = minimal_manifest();
    let text = serde_json::to_string_pretty(&manifest).expect("serialize manifest");
    let parsed = parse_and_validate(&text, ManifestOwnership::System, &registry())
        .expect("round trip must validate");
    assert_eq!(parsed, manifest);
}

/// Verifies network-bound upload channels require an explicit safe host and bounded size.
/// 验证网络绑定上传通道要求显式安全宿主与有界大小。
#[test]
fn upload_channels_require_safe_host_and_bounded_size() {
    let valid = serde_json::json!({
        "upload_channel_id": "oss-signed",
        "description": "Alibaba temporary OSS upload",
        "primitive": "signed_object_upload",
        "signing_primitive": "temporary_oss_policy",
        "endpoint_host": "dashscope.aliyuncs.com",
        "endpoint_path": "/api/v1/uploads",
        "response_host_suffixes": [".aliyuncs.com"],
        "method": "GET",
        "accepted_mime_types": ["image/png"],
        "max_bytes": 10485760,
        "max_count": 1,
        "timeout_ms": 60000,
        "reference_encoding": "response_url",
        "progress_supported": true,
        "cancellation_supported": false
    });
    let parsed: super::super::UploadChannelDeclaration =
        serde_json::from_value(valid.clone()).expect("valid upload channel should parse");
    let errors = validate_upload_channel_errors(&parsed);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    let mutations: &[(&str, &dyn Fn(serde_json::Value) -> serde_json::Value)] = &[
        ("missing host", &|mut v: serde_json::Value| {
            v.as_object_mut().expect("object").remove("endpoint_host");
            v
        }),
        ("host with port", &|mut v: serde_json::Value| {
            v["endpoint_host"] = serde_json::json!("dashscope.aliyuncs.com:8443");
            v
        }),
        ("host with path", &|mut v: serde_json::Value| {
            v["endpoint_host"] = serde_json::json!("dashscope.aliyuncs.com/v1");
            v
        }),
        ("host with credentials", &|mut v: serde_json::Value| {
            v["endpoint_host"] = serde_json::json!("user@example.com");
            v
        }),
        ("zero max bytes", &|mut v: serde_json::Value| {
            v["max_bytes"] = serde_json::json!(0);
            v
        }),
        ("missing max count", &|mut v: serde_json::Value| {
            v.as_object_mut().expect("object").remove("max_count");
            v
        }),
        ("unsafe endpoint", &|mut v: serde_json::Value| {
            v["endpoint_path"] = serde_json::json!("/uploads?target=https://evil");
            v
        }),
    ];
    for (label, mutate) in mutations {
        let candidate = mutate(valid.clone());
        let parsed_candidate: super::super::UploadChannelDeclaration =
            serde_json::from_value(candidate).expect("channel should parse");
        let errors = validate_upload_channel_errors(&parsed_candidate);
        assert!(
            !errors.is_empty(),
            "{label}: unsafe upload channel must be rejected"
        );
    }
}

/// Verifies every network upload channel declares a bounded total timeout.
/// 验证每个网络上传通道都声明有界总超时。
#[test]
fn network_upload_channel_requires_bounded_total_timeout() {
    let declaration: super::super::UploadChannelDeclaration =
        serde_json::from_value(serde_json::json!({
            "upload_channel_id": "provider-upload",
            "description": "Provider upload channel",
            "primitive": "multipart_file",
            "endpoint_host": "api.example.com",
            "endpoint_path": "/v1/uploads",
            "method": "POST",
            "file_field": "file",
            "accepted_mime_types": ["image/png"],
            "max_bytes": 1048576,
            "max_count": 1,
            "response_id_pointer": "/id"
        }))
        .expect("upload channel should parse");
    let errors = validate_upload_channel_errors(&declaration);
    assert!(
        errors
            .iter()
            .any(|code| code == "network_upload_timeout_required"),
        "a network upload channel without a timeout must fail closed: {errors:?}"
    );
}

/// Verifies non-network materialization cannot declare a meaningless timeout.
/// 验证非网络物化不能声明无意义的超时。
#[test]
fn non_network_upload_channel_forbids_total_timeout() {
    let declaration: super::super::UploadChannelDeclaration =
        serde_json::from_value(serde_json::json!({
            "upload_channel_id": "inline-data",
            "description": "Inline upload materialization",
            "primitive": "inline_data_url",
            "accepted_mime_types": ["image/png"],
            "max_bytes": 1024,
            "max_count": 1,
            "timeout_ms": 60000
        }))
        .expect("upload channel should parse");
    let errors = validate_upload_channel_errors(&declaration);
    assert!(
        errors
            .iter()
            .any(|code| code == "non_network_upload_forbids_timeout"),
        "a non-network upload channel with a timeout must fail closed: {errors:?}"
    );
}

/// Verifies schema-known but unimplemented upload primitives are rejected before activation.
/// 验证 Schema 已知但尚未实现的上传原语在激活前被拒绝。
#[test]
fn unregistered_upload_primitive_fails_closed() {
    let declaration: super::super::UploadChannelDeclaration =
        serde_json::from_value(serde_json::json!({
            "upload_channel_id": "future-presigned",
            "description": "Unimplemented presigned upload",
            "primitive": "presigned_form_upload",
            "endpoint_host": "upload.example.com",
            "endpoint_path": "/v1/presign",
            "accepted_mime_types": ["image/png"],
            "max_bytes": 1024,
            "max_count": 1
        }))
        .expect("schema-known primitive should parse");
    let errors = validate_upload_channel_errors(&declaration);
    assert!(
        errors.iter().any(|code| code == "unknown_upload_primitive"),
        "an unregistered primitive must fail before runtime: {errors:?}"
    );
}

/// Verifies remote URL passthrough requires an explicit bounded host allowlist.
/// 验证远端 URL 直通要求显式且有界的宿主允许列表。
#[test]
fn remote_url_passthrough_requires_host_allowlist() {
    let declaration: super::super::UploadChannelDeclaration =
        serde_json::from_value(serde_json::json!({
            "upload_channel_id": "remote-source",
            "description": "Remote source passthrough",
            "primitive": "remote_url_passthrough",
            "accepted_mime_types": ["image/png"],
            "max_count": 1,
            "reference_encoding": "response_url"
        }))
        .expect("remote channel should parse");
    let errors = validate_upload_channel_errors(&declaration);
    assert!(
        errors
            .iter()
            .any(|code| code == "remote_url_upload_requires_host_allowlist"),
        "a remote passthrough without host authority must fail: {errors:?}"
    );
}

/// Verifies upload reference templates close every placeholder against response pointers.
/// 验证上传引用模板的每个占位符都与响应指针闭合。
#[test]
fn upload_reference_template_requires_matching_pointer() {
    let declaration: super::super::UploadChannelDeclaration =
        serde_json::from_value(serde_json::json!({
            "upload_channel_id": "multipart-template",
            "description": "Multipart template fixture",
            "primitive": "multipart_file",
            "endpoint_host": "upload.example.com",
            "endpoint_path": "/v1/files",
            "method": "POST",
            "file_field": "file",
            "accepted_mime_types": ["video/mp4"],
            "max_bytes": 1024,
            "max_count": 1,
            "response_id_pointer": "/id",
            "reference_template": "provider://{url}",
            "reference_encoding": "template"
        }))
        .expect("multipart channel should parse");
    let errors = validate_upload_channel_errors(&declaration);
    assert!(
        errors
            .iter()
            .any(|code| code == "upload_template_requires_url_pointer"),
        "a template placeholder without its response pointer must fail: {errors:?}"
    );
}

/// Verifies response identifier scalar encodings cannot exist without an identifier pointer.
/// 验证响应标识标量编码不能脱离响应标识指针单独存在。
#[test]
fn upload_response_id_encoding_requires_pointer() {
    let declaration: super::super::UploadChannelDeclaration =
        serde_json::from_value(serde_json::json!({
            "upload_channel_id": "multipart-numeric-id",
            "description": "Multipart numeric identifier fixture",
            "primitive": "multipart_file",
            "endpoint_host": "upload.example.com",
            "endpoint_path": "/v1/files",
            "method": "POST",
            "file_field": "file",
            "accepted_mime_types": ["video/mp4"],
            "max_bytes": 1024,
            "max_count": 1,
            "response_id_encoding": "unsigned_integer",
            "reference_encoding": "provider_file_id"
        }))
        .expect("multipart channel should parse");
    let errors = validate_upload_channel_errors(&declaration);
    assert!(
        errors
            .iter()
            .any(|code| code == "upload_response_id_encoding_requires_pointer"),
        "an identifier encoding without its pointer must fail: {errors:?}"
    );
}

/// Verifies primitives without host-side network I/O do not require an upload endpoint.
/// 验证不发起宿主侧网络 I/O 的原语不需要上传端点。
#[test]
fn non_network_upload_primitives_need_no_endpoint() {
    for (primitive, value) in [
        (
            "inline_data_url",
            serde_json::json!({
                "upload_channel_id": "inline-data",
                "description": "Inline upload materialization",
                "primitive": "inline_data_url",
                "accepted_mime_types": ["image/png"],
                "max_bytes": 1024,
                "max_count": 1
            }),
        ),
        (
            "remote_url_passthrough",
            serde_json::json!({
                "upload_channel_id": "remote-url",
                "description": "Remote URL materialization",
                "primitive": "remote_url_passthrough",
                "remote_url_host_suffixes": ["example.com"],
                "accepted_mime_types": ["image/png"],
                "max_count": 1
            }),
        ),
    ] {
        let parsed: super::super::UploadChannelDeclaration =
            serde_json::from_value(value).expect("non-network channel should parse");
        let errors = validate_upload_channel_errors(&parsed);
        assert!(
            errors.is_empty(),
            "{primitive} produced unexpected errors: {errors:?}"
        );
    }
}

/// Verifies an upload channel cannot reference a credential role absent from the manifest.
/// 验证上传通道不能引用 Manifest 中不存在的凭据角色。
#[test]
fn upload_channel_rejects_unknown_credential_role() {
    // The minimal manifest declares one credential profile without an upload role.
    // 最小 Manifest 声明了一个不带上传角色的凭据配置档。
    let declaration: super::super::UploadChannelDeclaration =
        serde_json::from_value(serde_json::json!({
            "upload_channel_id": "provider-upload",
            "description": "Provider upload channel",
            "primitive": "multipart_file",
            "endpoint_host": "api.example.com",
            "endpoint_path": "/v1/uploads",
            "method": "POST",
            "credential_role": "file-upload",
            "file_field": "file",
            "accepted_mime_types": ["image/png"],
            "max_bytes": 1048576,
            "max_count": 1,
            "response_id_pointer": "/id"
        }))
        .expect("upload channel should parse");
    let errors = validate_upload_channel_errors(&declaration);
    assert!(
        errors
            .iter()
            .any(|code| code == "upload_channel_unknown_credential_role"),
        "an unknown credential role must fail closed: {errors:?}"
    );
}

/// Verifies declaration identifiers use the stable manifest identifier alphabet.
/// 验证声明标识使用稳定的 Manifest 标识字符集。
#[test]
fn declaration_identifier_rejects_unstable_characters() {
    // A slash would make references ambiguous with paths and must be rejected before compile.
    // 斜杠会使引用与路径产生歧义，必须在编译前拒绝。
    let mut manifest = minimal_manifest();
    manifest.usage_profiles = vec![crate::provider_manifest::schema::UsageProfileDeclaration {
        profile_id: String::from("usage/invalid"),
        description: String::from("invalid identifier fixture"),
        payload: None,
    }];
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("an unstable declaration identifier must fail");
    assert!(
        errors.iter().any(|error| {
            error.code == "invalid_identifier_charset"
                && error.path == "$.usage_profiles[0].profile_id"
        }),
        "the exact invalid declaration path must be reported: {errors:?}"
    );
}

/// Verifies every object template form rejects keys owned by another form.
/// 验证每个对象模板形式都会拒绝属于其他形式的键。
#[test]
fn json_template_forms_reject_cross_form_keys() {
    let cases = [
        (
            "$vulcan_ref",
            serde_json::json!({"path": "/value", "flatten": true}),
        ),
        (
            "$vulcan_map",
            serde_json::json!({"path": "/values", "template": true, "non_empty": true}),
        ),
        (
            "$vulcan_enum",
            serde_json::json!({"path": "/value", "mapping": {"a": "b"}, "then": true}),
        ),
        (
            "$vulcan_if_equals",
            serde_json::json!({"path": "/value", "equals": true, "optional": true}),
        ),
        (
            "$vulcan_if_present",
            serde_json::json!({"path": "/value", "then": true, "omit_if_empty": true}),
        ),
        (
            "$vulcan_if_array_length",
            serde_json::json!({"path": "/values", "equals": 1, "flatten": true}),
        ),
    ];
    for (tag, specification) in cases {
        let mut errors = Vec::new();
        validate_json_template_form(tag, &specification, 0, "$.template", &mut errors);
        assert!(
            errors
                .iter()
                .any(|error| error.code == "unregistered_json_template_form_key"),
            "{tag} must reject cross-form keys: {errors:?}"
        );
    }
}

/// Runs the channel validator and returns only error codes.
/// 运行通道校验器并只返回错误码。
fn validate_upload_channel_errors(
    declaration: &super::super::UploadChannelDeclaration,
) -> Vec<String> {
    let mut errors = Vec::new();
    let mut manifest = minimal_manifest();
    if let Some(primitive) = declaration.primitive {
        manifest
            .engine_contract
            .required_upload_primitives
            .push(upload_primitive_id(primitive).to_string());
    }
    validate_upload_channel(&manifest, declaration, &registry(), &mut errors);
    errors
        .iter()
        .map(|error| error.code.to_string())
        .collect::<Vec<_>>()
}

/// Verifies the node counter accumulates across sibling subtrees so a document whose
/// branches each stay under the limit but whose total exceeds it is rejected.
/// 验证节点计数器在兄弟子树间累计，使每条分支都低于上限但总量超限的文档被拒绝。
#[test]
fn json_node_limit_accumulates_across_sibling_branches() {
    // Two sibling arrays each hold more than half the node budget; per-branch counting
    // would pass, cumulative counting must reject.
    // 两个兄弟数组各持有超过节点预算一半的条目；按分支计数会通过，累计计数必须拒绝。
    let half = MAX_JSON_NODES / 2 + 1;
    let value = serde_json::json!({
        "first": vec![0_u8; half],
        "second": vec![0_u8; half],
    });
    let mut count = 0_usize;
    let result = check_bounds(&value, 0, &mut count);
    assert!(
        matches!(result, Err(errors) if errors.iter().any(|error| error.code == "json_too_large")),
        "the total node count must exceed the hard limit"
    );
    // A single branch at exactly half stays below the limit.
    // 单条分支恰好一半时保持在限制内。
    let value = serde_json::json!({
        "first": vec![0_u8; half],
    });
    let mut count = 0_usize;
    assert!(
        check_bounds(&value, 0, &mut count).is_ok(),
        "a document under the limit must pass"
    );
}

/// Builds a minimal manifest whose only protocol route references one request profile.
/// 构建一条仅协议路由引用一个请求 Profile 的最小清单。
fn manifest_with_request_profile(
    declaration: crate::provider_manifest::schema::RequestProfileDeclaration,
) -> ProviderManifestV1 {
    let mut manifest = minimal_manifest();
    manifest.request_profiles = vec![declaration];
    manifest.services[0].protocol_routes[0].request_profile_refs =
        vec![String::from("test.request.v1")];
    manifest
}

/// Verifies a projection step may never write a host-authoritative root (plan §R8.3).
/// 验证投影步骤绝不能写入宿主权威根（方案 §R8.3）。
#[test]
fn projection_step_cannot_write_protected_request_root() {
    let manifest = manifest_with_request_profile(
        crate::provider_manifest::schema::RequestProfileDeclaration {
            profile_id: String::from("test.request.v1"),
            description: String::from("test"),
            payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                profile_id: String::from("test.request.v1"),
                revision: 1,
                steps: vec![crate::provider_manifest::schema::MappingStep::SetLiteral {
                    path: String::from("/model"),
                    value: serde_json::json!("overwritten"),
                }],
            }),
        },
    );
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("protected root must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "protected_request_root"),
        "protected root write must be rejected: {errors:?}"
    );
}

/// Verifies the payload profile id must match its declaration id.
/// 验证载荷 Profile 标识必须与声明标识一致。
#[test]
fn projection_payload_id_mismatch_rejected() {
    let manifest = manifest_with_request_profile(
        crate::provider_manifest::schema::RequestProfileDeclaration {
            profile_id: String::from("test.request.v1"),
            description: String::from("test"),
            payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                profile_id: String::from("other.request.v1"),
                revision: 1,
                steps: vec![crate::provider_manifest::schema::MappingStep::SetLiteral {
                    path: String::from("/temperature"),
                    value: serde_json::json!(0.2),
                }],
            }),
        },
    );
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("payload id mismatch must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "profile_payload_id_mismatch"),
        "payload id mismatch must be rejected: {errors:?}"
    );
}

/// Verifies a referenced profile without a payload is rejected at compile time.
/// 验证被引用但无载荷的 Profile 在编译期被拒绝。
#[test]
fn referenced_profile_without_payload_rejected() {
    let manifest = manifest_with_request_profile(
        crate::provider_manifest::schema::RequestProfileDeclaration {
            profile_id: String::from("test.request.v1"),
            description: String::from("test"),
            payload: None,
        },
    );
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("empty payload must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "profile_declared_without_payload"),
        "empty payload reference must be rejected: {errors:?}"
    );
}

/// Verifies duplicate write targets and invalid pointers are rejected.
/// 验证重复写入目标与非法指针被拒绝。
#[test]
fn duplicate_projection_target_and_invalid_pointer_rejected() {
    let manifest = manifest_with_request_profile(
        crate::provider_manifest::schema::RequestProfileDeclaration {
            profile_id: String::from("test.request.v1"),
            description: String::from("test"),
            payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                profile_id: String::from("test.request.v1"),
                revision: 1,
                steps: vec![
                    crate::provider_manifest::schema::MappingStep::SetLiteral {
                        path: String::from("/temperature"),
                        value: serde_json::json!(0.1),
                    },
                    crate::provider_manifest::schema::MappingStep::SetLiteral {
                        path: String::from("/temperature"),
                        value: serde_json::json!(0.2),
                    },
                    crate::provider_manifest::schema::MappingStep::SetLiteral {
                        path: String::from("temperature"),
                        value: serde_json::json!(0.3),
                    },
                ],
            }),
        },
    );
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("duplicate and invalid targets must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "duplicate_projection_target"),
        "duplicate target must be rejected: {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|error| error.code == "invalid_projection_pointer"),
        "invalid pointer must be rejected: {errors:?}"
    );
}

/// Verifies an unregistered request field is rejected.
/// 验证未注册的请求字段被拒绝。
#[test]
fn unregistered_request_field_rejected() {
    let manifest = manifest_with_request_profile(
        crate::provider_manifest::schema::RequestProfileDeclaration {
            profile_id: String::from("test.request.v1"),
            description: String::from("test"),
            payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                profile_id: String::from("test.request.v1"),
                revision: 1,
                steps: vec![
                    crate::provider_manifest::schema::MappingStep::CopyRequestField {
                        path: String::from("/secret_parameter"),
                        field: String::from("authorization_token"),
                    },
                ],
            }),
        },
    );
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("unregistered field must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "unregistered_request_field"),
        "unregistered request field must be rejected: {errors:?}"
    );
}

/// Verifies typed request-field sources require the explicit host-field marker.
/// 验证强类型请求字段来源必须带显式宿主字段标记。
#[test]
fn typed_request_field_source_requires_marker() {
    let manifest = manifest_with_request_profile(
        crate::provider_manifest::schema::RequestProfileDeclaration {
            profile_id: String::from("test.request.v1"),
            description: String::from("test"),
            payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                profile_id: String::from("test.request.v1"),
                revision: 1,
                steps: vec![
                    crate::provider_manifest::schema::MappingStep::RejectIfPresent {
                        source: crate::provider_manifest::schema::MappingStepSource::RequestField,
                        source_path: String::from("reasoning_effort"),
                        message: String::from("test rejection"),
                    },
                ],
            }),
        },
    );

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("an unmarked request-field source must fail");

    assert!(
        errors
            .iter()
            .any(|error| error.code == "invalid_request_field_source"),
        "an unmarked request-field source must be rejected: {errors:?}"
    );
}

/// Verifies typed request-field sources remain restricted to the bounded registry.
/// 验证强类型请求字段来源仍受有界注册表限制。
#[test]
fn typed_request_field_source_rejects_unregistered_field() {
    let manifest = manifest_with_request_profile(
        crate::provider_manifest::schema::RequestProfileDeclaration {
            profile_id: String::from("test.request.v1"),
            description: String::from("test"),
            payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                profile_id: String::from("test.request.v1"),
                revision: 1,
                steps: vec![
                    crate::provider_manifest::schema::MappingStep::RejectIfPresent {
                        source: crate::provider_manifest::schema::MappingStepSource::RequestField,
                        source_path: String::from("@authorization_token"),
                        message: String::from("test rejection"),
                    },
                ],
            }),
        },
    );

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("an unregistered request-field source must fail");

    assert!(
        errors
            .iter()
            .any(|error| error.code == "unregistered_request_field"),
        "an unregistered request-field source must be rejected: {errors:?}"
    );
}

/// Verifies all-present guards have bounded, unique typed conditions.
/// 验证全部存在守卫只接受有界且唯一的强类型条件。
#[test]
fn reject_if_all_present_conditions_are_bounded_and_unique() {
    let duplicate_condition = crate::provider_manifest::schema::MappingPresenceCondition {
        source: crate::provider_manifest::schema::MappingStepSource::RequestField,
        source_path: String::from("@reasoning_effort"),
    };
    let manifest = manifest_with_request_profile(
        crate::provider_manifest::schema::RequestProfileDeclaration {
            profile_id: String::from("test.request.v1"),
            description: String::from("test"),
            payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                profile_id: String::from("test.request.v1"),
                revision: 1,
                steps: vec![
                    crate::provider_manifest::schema::MappingStep::RejectIfAllPresent {
                        conditions: vec![duplicate_condition.clone()],
                        message: String::from("too few conditions"),
                    },
                    crate::provider_manifest::schema::MappingStep::RejectIfAllPresent {
                        conditions: vec![duplicate_condition.clone(), duplicate_condition],
                        message: String::from("duplicate conditions"),
                    },
                ],
            }),
        },
    );

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("invalid all-present guards must fail");

    assert!(
        errors
            .iter()
            .any(|error| error.code == "presence_condition_count_out_of_bounds"),
        "an undersized guard must be rejected: {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|error| error.code == "duplicate_presence_condition"),
        "duplicate guard conditions must be rejected: {errors:?}"
    );
}

/// Verifies response profiles cannot depend on host request fields.
/// 验证响应 Profile 不能依赖宿主请求字段。
#[test]
fn response_profile_request_context_is_rejected() {
    let mut manifest = minimal_manifest();
    manifest.response_profiles = vec![
        crate::provider_manifest::schema::ResponseProfileDeclaration {
            profile_id: String::from("test.response.v1"),
            description: String::from("test"),
            payload: Some(
                crate::provider_manifest::schema::ResponseProjectionProfile {
                    profile_id: String::from("test.response.v1"),
                    revision: 1,
                    steps: vec![
                        crate::provider_manifest::schema::MappingStep::CopyRequestField {
                            path: String::from("/model"),
                            field: String::from("model_id"),
                        },
                    ],
                    transient_reasoning_status: None,
                },
            ),
        },
    ];
    manifest.services[0].protocol_routes[0].response_profile_refs =
        vec![String::from("test.response.v1")];

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("response request context must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "response_profile_requires_request_context"),
        "response request context must be rejected: {errors:?}"
    );
}

/// Verifies an invalid usage payload is rejected at compile time.
/// 验证无效 usage 载荷在编译期被拒绝。
#[test]
fn invalid_usage_payload_rejected_at_compile_time() {
    let mut manifest = minimal_manifest();
    manifest.usage_profiles = vec![crate::provider_manifest::schema::UsageProfileDeclaration {
        profile_id: String::from("test.usage.v1"),
        description: String::from("test"),
        payload: Some(vulcan_llm_core::UsageExtractionProfile {
            profile_id: String::from("test.usage.v1"),
            revision: 1,
            record_kind: vulcan_llm_core::UsageRecordKind::JsonBody,
            update_kind: vulcan_ai_contract::UsageUpdateKind::Snapshot,
            event_type_pointer: Some(String::from("not-a-pointer")),
            event_type_value: Some(String::from("response.completed")),
            required_pointer: None,
            bindings: Vec::new(),
            derivations: Vec::new(),
        }),
    }];
    manifest.services[0].protocol_routes[0].usage_profile_refs =
        vec![String::from("test.usage.v1")];
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("invalid usage payload must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "invalid_usage_profile_payload"),
        "invalid usage payload must be rejected: {errors:?}"
    );
}

/// Verifies a WebSocket usage record cannot be attached to a non-Responses protocol route.
/// 验证 WebSocket 用量记录不能绑定到非 Responses 协议路由。
#[test]
fn usage_record_kind_must_match_protocol_route() {
    let mut manifest = minimal_manifest();
    manifest.usage_profiles = vec![crate::provider_manifest::schema::UsageProfileDeclaration {
        profile_id: String::from("test.usage.websocket.v1"),
        description: String::from("test"),
        payload: Some(vulcan_llm_core::UsageExtractionProfile {
            profile_id: String::from("test.usage.websocket.v1"),
            revision: 1,
            record_kind: vulcan_llm_core::UsageRecordKind::WebSocketFrame,
            update_kind: vulcan_ai_contract::UsageUpdateKind::Final,
            event_type_pointer: Some(String::from("/type")),
            event_type_value: Some(String::from("response.completed")),
            required_pointer: Some(String::from("/response/usage")),
            bindings: vec![vulcan_llm_core::UsageFieldBinding {
                field: vulcan_llm_core::UsageField::InputTokens,
                pointers: vec![String::from("/response/usage/input_tokens")],
                encoding: vulcan_llm_core::UsageValueEncoding::UnsignedInteger,
            }],
            derivations: Vec::new(),
        }),
    }];
    manifest.services[0].protocol_routes[0].protocol_profile_id =
        String::from(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID);
    manifest.services[0].protocol_routes[0].usage_profile_refs =
        vec![String::from("test.usage.websocket.v1")];

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("WebSocket usage on a Chat route must fail");

    assert!(
        errors
            .iter()
            .any(|error| error.code == "usage_record_kind_protocol_mismatch"),
        "transport-incompatible usage profile must be rejected: {errors:?}"
    );
}

/// Verifies a referenced usage profile without a payload is rejected.
/// 验证被引用但无载荷的 usage Profile 被拒绝。
#[test]
fn referenced_usage_profile_without_payload_rejected() {
    let mut manifest = minimal_manifest();
    manifest.usage_profiles = vec![crate::provider_manifest::schema::UsageProfileDeclaration {
        profile_id: String::from("test.usage.v1"),
        description: String::from("test"),
        payload: None,
    }];
    manifest.services[0].protocol_routes[0].usage_profile_refs =
        vec![String::from("test.usage.v1")];
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("empty usage payload must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "profile_declared_without_payload"),
        "empty usage payload reference must be rejected: {errors:?}"
    );
}

/// Verifies a valid projection payload compiles without errors.
/// 验证有效投影载荷编译无错误。
#[test]
fn valid_projection_payload_accepts_bounded_steps() {
    let mut manifest = manifest_with_request_profile(
        crate::provider_manifest::schema::RequestProfileDeclaration {
            profile_id: String::from("test.request.v1"),
            description: String::from("test"),
            payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                profile_id: String::from("test.request.v1"),
                revision: 1,
                steps: vec![
                    crate::provider_manifest::schema::MappingStep::DeleteJsonPointer {
                        path: String::from("/reasoning_effort"),
                    },
                    crate::provider_manifest::schema::MappingStep::SetLiteral {
                        path: String::from("/enable_thinking"),
                        value: serde_json::json!(true),
                    },
                    crate::provider_manifest::schema::MappingStep::StringTemplate {
                        path: String::from("/model_name"),
                        template: String::from("private/{model_id}"),
                    },
                    crate::provider_manifest::schema::MappingStep::CopyRequestField {
                        path: String::from("/effort"),
                        field: String::from("reasoning_effort"),
                    },
                ],
            }),
        },
    );
    manifest.engine_contract.required_transformers = [
        "delete_json_pointer",
        "set_literal",
        "string_template",
        "copy_request_field",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect("bounded projection must validate");
}

/// Verifies an operation referencing an undeclared tool target is rejected.
/// 验证引用未声明工具目标的操作被拒绝。
#[test]
fn operation_dangling_tool_target_reference_rejected() {
    let mut manifest = minimal_manifest();
    manifest.operations = vec![crate::provider_manifest::schema::OperationDeclaration {
        operation_id: String::from("op.image-generate"),
        operation: String::from("image_generate"),
        carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
        input_resources: Vec::new(),
        output_resources: Vec::new(),
        tool_target_refs: vec![String::from("missing.target")],
        tool_target_template_refs: Vec::new(),
        request_profile_refs: Vec::new(),
        response_profile_refs: Vec::new(),
        upload_channel_refs: Vec::new(),
        upload_bindings: Vec::new(),
        async_execution: None,
        timeout_ms: Some(60_000),
        idempotent: false,
    }];
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("dangling tool target reference must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "dangling_tool_target_reference"),
        "undeclared tool target must be rejected: {errors:?}"
    );
}

/// Verifies an operation referencing an undeclared upload channel is rejected.
/// 验证引用未声明上传通道的操作被拒绝。
#[test]
fn operation_dangling_upload_channel_reference_rejected() {
    let mut manifest = minimal_manifest();
    manifest.operations = vec![crate::provider_manifest::schema::OperationDeclaration {
        operation_id: String::from("op.image-generate"),
        operation: String::from("image_generate"),
        carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
        input_resources: Vec::new(),
        output_resources: Vec::new(),
        tool_target_refs: Vec::new(),
        tool_target_template_refs: Vec::new(),
        request_profile_refs: Vec::new(),
        response_profile_refs: Vec::new(),
        upload_channel_refs: vec![String::from("missing.channel")],
        upload_bindings: Vec::new(),
        async_execution: None,
        timeout_ms: Some(60_000),
        idempotent: false,
    }];
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("dangling upload channel reference must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "dangling_upload_channel_reference"),
        "undeclared upload channel must be rejected: {errors:?}"
    );
}

/// Verifies a tool target referencing an unknown service entry is rejected.
/// 验证引用未知服务入口的工具目标被拒绝。
#[test]
fn tool_target_unknown_service_entry_rejected() {
    let mut manifest = minimal_manifest();
    manifest.tool_targets = vec![tool_target_declaration(
        "missing.service",
        crate::ProviderToolCapabilityKind::WebSearch,
        crate::ProviderToolExecutionKind::HttpJsonSync,
    )];
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("unknown service entry must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "unknown_tool_target_service_entry"),
        "unknown tool target service entry must be rejected: {errors:?}"
    );
}

/// Verifies a tool target referencing an unknown credential profile is rejected.
/// 验证引用未知凭据配置档的工具目标被拒绝。
#[test]
fn tool_target_unknown_credential_profile_rejected() {
    let mut manifest = minimal_manifest();
    manifest.tool_targets = vec![tool_target_declaration(
        "public-api",
        crate::ProviderToolCapabilityKind::WebSearch,
        crate::ProviderToolExecutionKind::HttpJsonSync,
    )];
    manifest.tool_targets[0].credential_profile_ids =
        BTreeSet::from([String::from("missing-profile")]);
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("unknown credential profile must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "unknown_tool_target_credential_profile"),
        "unknown tool target credential profile must be rejected: {errors:?}"
    );
}

/// Verifies fixed provider-tool targets cannot bypass system endpoint URL policy.
/// 验证固定供应商工具目标无法绕过系统端点 URL 策略。
#[test]
fn tool_target_endpoint_base_url_uses_manifest_security_policy() {
    let mut manifest = minimal_manifest();
    let mut target = tool_target_declaration(
        "public-api",
        crate::ProviderToolCapabilityKind::WebSearch,
        crate::ProviderToolExecutionKind::HttpJsonSync,
    );
    target.execution.endpoint_base_url = Some(String::from(
        "https://embedded-user:embedded-secret@example.com",
    ));
    manifest.tool_targets = vec![target];
    manifest.operations = vec![crate::provider_manifest::schema::OperationDeclaration {
        operation_id: String::from("op.web-search"),
        operation: String::from("web_search"),
        carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
        input_resources: Vec::new(),
        output_resources: Vec::new(),
        tool_target_refs: vec![String::from("public-api.search")],
        tool_target_template_refs: Vec::new(),
        request_profile_refs: Vec::new(),
        response_profile_refs: Vec::new(),
        upload_channel_refs: Vec::new(),
        upload_bindings: Vec::new(),
        async_execution: None,
        timeout_ms: Some(60_000),
        idempotent: false,
    }];
    manifest
        .engine_contract
        .required_execution_primitives
        .push(String::from("http_json_sync"));

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("embedded endpoint credentials must fail closed");
    assert!(
        errors.iter().any(|error| {
            error.code == "invalid_url"
                && error.path == "$.tool_targets[0].execution.endpoint_base_url"
        }),
        "provider-tool endpoint credentials must be rejected: {errors:?}"
    );
}

/// Verifies preauthorized target templates receive the complete target validation chain.
/// 验证预授权目标模板会进入完整目标校验链。
#[test]
fn tool_target_template_contract_is_validated_before_expansion() {
    let mut manifest = minimal_manifest();
    let target = tool_target_declaration(
        "public-api",
        crate::ProviderToolCapabilityKind::WebSearch,
        crate::ProviderToolExecutionKind::HttpJsonSync,
    );
    let mut execution = target.execution;
    execution.endpoint_base_url = Some(String::from("http://untrusted.example"));
    execution.fixed_headers.insert(
        String::from("Authorization"),
        String::from("forbidden-static-secret"),
    );
    manifest.tool_target_templates = vec![
        crate::provider_manifest::schema::ToolTargetTemplateDeclaration {
            template_id: String::from("template.web-search"),
            description: String::from("test template"),
            display_name_suffix: String::from("Search"),
            allowed_service_entry_ids: BTreeSet::from([String::from("public-api")]),
            credential_profile_ids: BTreeSet::from([String::from("api-key")]),
            tool_id: String::from("vulcan_web_search"),
            capability: crate::ProviderToolCapabilityKind::WebSearch,
            input_schema: serde_json::json!({"type": "string"}),
            input_numeric_product_ranges: Vec::new(),
            extension_profile: None,
            output: target.output,
            execution,
        },
    ];
    manifest.operations = vec![crate::provider_manifest::schema::OperationDeclaration {
        operation_id: String::from("op.template-web-search"),
        operation: String::from("web_search"),
        carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
        input_resources: Vec::new(),
        output_resources: Vec::new(),
        tool_target_refs: Vec::new(),
        tool_target_template_refs: vec![String::from("template.web-search")],
        request_profile_refs: Vec::new(),
        response_profile_refs: Vec::new(),
        upload_channel_refs: Vec::new(),
        upload_bindings: Vec::new(),
        async_execution: None,
        timeout_ms: Some(60_000),
        idempotent: false,
    }];
    manifest
        .engine_contract
        .required_execution_primitives
        .push(String::from("http_json_sync"));

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("unsafe template contract must fail before expansion");
    assert!(
        errors.iter().any(|error| {
            error.code == "invalid_url_scheme"
                && error.path == "$.tool_target_templates[0].execution.endpoint_base_url"
        }),
        "template endpoint scheme must be rejected: {errors:?}"
    );
    assert!(
        errors.iter().any(|error| {
            error.code == "invalid_provider_tool_fixed_header_name"
                && error
                    .path
                    .contains("$.tool_target_templates[0].execution.fixed_headers")
        }),
        "template authorization header must be rejected: {errors:?}"
    );
    assert!(
        errors.iter().any(|error| {
            error.code == "provider_tool_input_schema_root_not_object"
                && error.path == "$.tool_target_templates[0].input_schema"
        }),
        "template input schema must be rejected: {errors:?}"
    );
}

/// Verifies a tool target using a retired non-AI-visible capability is rejected.
/// 验证使用已退役非 AI 可见能力的工具目标被拒绝。
#[test]
fn tool_target_retired_capability_rejected() {
    let mut manifest = minimal_manifest();
    manifest.tool_targets = vec![tool_target_declaration(
        "public-api",
        crate::ProviderToolCapabilityKind::EmbeddingCreate,
        crate::ProviderToolExecutionKind::HttpJsonSync,
    )];
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("retired capability must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "tool_target_capability_not_ai_visible"),
        "retired capability must be rejected: {errors:?}"
    );
}

/// Verifies operation request profiles cannot read conversation-only host request fields.
/// 验证操作请求 Profile 不能读取仅会话可用的宿主请求字段。
#[test]
fn operation_request_profile_rejects_conversation_host_context() {
    let mut manifest = minimal_manifest();
    manifest.request_profiles = vec![
        crate::provider_manifest::schema::RequestProfileDeclaration {
            profile_id: String::from("operation.request.v1"),
            description: String::from("test operation request"),
            payload: Some(crate::provider_manifest::schema::RequestProjectionProfile {
                profile_id: String::from("operation.request.v1"),
                revision: 1,
                steps: vec![
                    crate::provider_manifest::schema::MappingStep::CopyRequestField {
                        path: String::from("/effort"),
                        field: String::from("reasoning_effort"),
                    },
                ],
            }),
        },
    ];
    let mut target = tool_target_declaration(
        "public-api",
        crate::ProviderToolCapabilityKind::WebSearch,
        crate::ProviderToolExecutionKind::HttpJsonSync,
    );
    target.execution.adapter = crate::ProviderToolAdapterKind::GenericJson;
    manifest.tool_targets = vec![target];
    manifest.operations = vec![operation_with_request_profile()];

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("operation host request context must fail");
    assert!(
        errors
            .iter()
            .any(|error| { error.code == "operation_request_profile_requires_host_context" }),
        "operation host context must be rejected: {errors:?}"
    );
}

/// Verifies a fully closed operation/target/channel manifest validates cleanly.
/// 验证完全闭合的操作/目标/通道清单能干净通过校验。
#[test]
fn closed_operation_tool_target_manifest_validates() {
    let mut manifest = minimal_manifest();
    manifest
        .engine_contract
        .required_upload_primitives
        .push(String::from("inline_data_url"));
    manifest
        .engine_contract
        .required_execution_primitives
        .push(String::from("http_json_sync"));
    manifest.upload_channels = vec![crate::provider_manifest::schema::UploadChannelDeclaration {
        upload_channel_id: String::from("inline.data-url"),
        description: String::from("inline data url channel"),
        primitive: Some(crate::provider_manifest::schema::UploadPrimitiveKind::InlineDataUrl),
        signing_primitive: None,
        endpoint_host: None,
        endpoint_path: None,
        response_host_suffixes: Vec::new(),
        remote_url_host_suffixes: Vec::new(),
        method: None,
        credential_role: None,
        file_field: None,
        accepted_mime_types: vec![String::from("image/png")],
        max_bytes: Some(1_024),
        max_count: Some(1),
        timeout_ms: None,
        response_id_pointer: None,
        response_id_encoding: None,
        response_url_pointer: None,
        reference_template: None,
        progress_supported: false,
        cancellation_supported: false,
        fixed_fields: Default::default(),
        cache_duration_ms: None,
        reference_encoding: None,
        reference_scope: Default::default(),
    }];
    manifest.tool_targets = vec![tool_target_declaration(
        "public-api",
        crate::ProviderToolCapabilityKind::WebSearch,
        crate::ProviderToolExecutionKind::HttpJsonSync,
    )];
    manifest.operations = vec![crate::provider_manifest::schema::OperationDeclaration {
        operation_id: String::from("op.web-search"),
        operation: String::from("web_search"),
        carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
        input_resources: Vec::new(),
        output_resources: Vec::new(),
        tool_target_refs: vec![String::from("public-api.search")],
        tool_target_template_refs: Vec::new(),
        request_profile_refs: Vec::new(),
        response_profile_refs: Vec::new(),
        upload_channel_refs: vec![String::from("inline.data-url")],
        upload_bindings: Vec::new(),
        async_execution: None,
        timeout_ms: Some(60_000),
        idempotent: false,
    }];
    validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect("closed operation manifest must validate");
}

/// Verifies every operation requires an explicit total execution deadline.
/// 验证每项操作都必须声明显式总执行截止时间。
#[test]
fn operation_without_timeout_is_rejected() {
    let mut manifest = minimal_manifest();
    manifest.operations = vec![operation_with_request_profile()];
    manifest.operations[0].timeout_ms = None;

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("operation without timeout must fail closed");

    assert!(
        errors
            .iter()
            .any(|error| error.code == "operation_timeout_required")
    );
}

/// Verifies asynchronous failure backoff cannot poll faster than the declared base interval.
/// 验证异步失败退避不能比声明的基础轮询间隔更频繁。
#[test]
fn async_operation_backoff_cannot_undercut_poll_interval() {
    // sourcePath selects the production Alibaba manifest because it owns reviewed async tasks.
    // sourcePath 选择生产 Alibaba 清单，因为它持有已审核异步任务。
    let source_path =
        vulcan_runtime_assets::source_bundled_provider_manifests_directory().join("alibaba.json");
    // manifest retains every surrounding production contract while one bound is mutated.
    // manifest 保留全部生产周边契约，只修改一个边界。
    let mut manifest: ProviderManifestV1 = serde_json::from_str(
        &std::fs::read_to_string(source_path).expect("Alibaba manifest must be readable"),
    )
    .expect("Alibaba manifest must parse");
    // asyncExecution is the first real asynchronous lifecycle in the bundled manifest.
    // asyncExecution 是随包清单中的第一条真实异步生命周期。
    let async_execution = manifest
        .operations
        .iter_mut()
        .find_map(|operation| operation.async_execution.as_mut())
        .expect("Alibaba must declare an asynchronous operation");
    async_execution.max_backoff_ms = async_execution.poll_interval_ms - 1;

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("backoff below poll interval must fail closed");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "async_backoff_out_of_bounds"),
        "backoff bound must be classified exactly: {errors:?}"
    );
}

/// Verifies model bindings cannot instantiate a reviewed template on an unauthorized service.
/// 验证模型绑定不能在未经授权的服务入口实例化已审核模板。
#[test]
fn tool_target_template_binding_rejects_unauthorized_service_entry() {
    let path =
        vulcan_runtime_assets::source_bundled_provider_manifests_directory().join("alibaba.json");
    let text = std::fs::read_to_string(path).expect("Alibaba manifest must be readable");
    let mut manifest: ProviderManifestV1 =
        serde_json::from_str(&text).expect("Alibaba manifest must parse");
    let binding = manifest
        .tool_target_template_bindings
        .iter_mut()
        .find(|binding| {
            binding
                .template_refs
                .iter()
                .any(|reference| reference == "alibaba.model-studio-cn.qwen-image-generate.v1")
        })
        .expect("Alibaba must expose the reviewed image template binding");
    binding.service_entry_id = String::from("model-studio-singapore");

    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("an unauthorized template service entry must fail closed");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "tool_target_template_service_not_authorized"),
        "unauthorized template service must be classified exactly: {errors:?}"
    );
}

/// Verifies resource URLs cannot compile without an explicit reviewed host authority.
/// 验证资源 URL 在没有显式已审核宿主权限时无法编译。
#[test]
fn resource_url_decoder_requires_reviewed_host_suffix() {
    // The minimal manifest supplies the exact service and credential identities for the target.
    // 最小清单为目标提供精确服务与凭据身份。
    let mut manifest = minimal_manifest();
    // The target models one syntactically complete URL resource contract with no host authority.
    // 该目标模拟一个语法完整但没有宿主权限的 URL 资源契约。
    let mut target = tool_target_declaration(
        "public-api",
        crate::ProviderToolCapabilityKind::ImageGenerate,
        crate::ProviderToolExecutionKind::HttpJsonSync,
    );
    target.output = crate::ProviderToolOutputContract {
        kind: crate::ProviderToolOutputKind::Resources,
        source_path: String::from("/assets"),
        normalized_result_path: String::from("resources"),
        resource_kind: Some(String::from("image")),
        mime_types: vec![String::from("image/png")],
        resource_decoders: vec![crate::ProviderToolResourceDecoder {
            source_path: String::from("/assets"),
            encoding: crate::ProviderToolResourceEncoding::HttpsUrl,
            download_host_suffixes: Vec::new(),
            resource_kind: String::from("image"),
            mime_type: Some(String::from("image/png")),
            mime_type_request_path: None,
            mime_types_by_request_value: BTreeMap::new(),
            optional: false,
            max_bytes: None,
            output_id_prefix: Some(String::from("image")),
        }],
    };
    manifest.tool_targets = vec![target];

    // The exact diagnostic proves schema shape cannot substitute for network authority.
    // 精确诊断证明 Schema 形状不能替代网络权限。
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("URL decoder without host authority must fail closed");
    assert!(
        errors
            .iter()
            .any(|error| { error.code == "url_resource_decoder_requires_download_host_suffix" }),
        "missing resource host authority must be classified exactly: {errors:?}"
    );
}

/// Verifies resource output contracts cannot invoke the removed shape-guessing decoder.
/// 验证资源输出契约无法调用已移除的形状猜测解码器。
#[test]
fn resource_output_requires_closed_decoder() {
    // The minimal manifest supplies one otherwise valid system-provider container.
    // 最小清单提供一个其他方面有效的系统供应商容器。
    let mut manifest = minimal_manifest();
    // The target intentionally declares a resource kind without any carrier extractor.
    // 该目标刻意声明资源类别但不提供任何载体提取器。
    let mut target = tool_target_declaration(
        "public-api",
        crate::ProviderToolCapabilityKind::ImageGenerate,
        crate::ProviderToolExecutionKind::HttpJsonSync,
    );
    target.output.kind = crate::ProviderToolOutputKind::Resources;
    target.output.resource_kind = Some(String::from("image"));
    target.output.mime_types = vec![String::from("image/png")];
    manifest.tool_targets = vec![target];

    // The validation result must name the missing closed decoder rather than fail at runtime.
    // 校验结果必须指出缺少封闭解码器，而不是延迟到运行时失败。
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("resource output without decoder must fail closed");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "resource_output_requires_declared_decoder"),
        "missing closed decoder must be classified exactly: {errors:?}"
    );
}

/// Verifies contradictory model-source declarations fail before compilation or runtime I/O.
/// 验证矛盾的模型来源声明会在编译或运行时 I/O 前失败。
#[test]
fn model_source_policy_validation_fails_closed() {
    // missingPolicy proves every conversation entry must make identity authority explicit.
    // missingPolicy 证明每个对话入口都必须显式声明身份权威。
    let mut missing_policy = minimal_manifest();
    missing_policy.services[0].model_catalog = None;
    let errors = validate_manifest(&missing_policy, ManifestOwnership::System, &registry())
        .expect_err("missing source policy must fail");
    assert!(
        errors
            .iter()
            .any(|error| { error.code == "conversation_entry_requires_model_source_policy" })
    );

    // discoveryWithoutRemote cannot disguise the manual marker as remote evidence.
    // discoveryWithoutRemote 不能把手工标记伪装成远端证据。
    let mut discovery_without_remote = minimal_manifest();
    let catalog = discovery_without_remote.services[0]
        .model_catalog
        .as_mut()
        .expect("minimal fixture has a catalog");
    catalog.source_policy = ModelSourcePolicy::DiscoveryOnly;
    catalog.discovery_kind = Some(ModelDiscoveryKind::Manual);
    let errors = validate_manifest(
        &discovery_without_remote,
        ManifestOwnership::System,
        &registry(),
    )
    .expect_err("discovery source without remote implementation must fail");
    assert!(
        errors
            .iter()
            .any(|error| { error.code == "discovery_policy_requires_remote_discovery_kind" })
    );

    // manualWithRemote cannot perform discovery under a non-discovery policy.
    // manualWithRemote 不能在非发现策略下执行远端发现。
    let mut manual_with_remote = minimal_manifest();
    manual_with_remote.services[0]
        .model_catalog
        .as_mut()
        .expect("minimal fixture has a catalog")
        .discovery_kind = Some(ModelDiscoveryKind::OpenAi);
    let errors = validate_manifest(&manual_with_remote, ManifestOwnership::System, &registry())
        .expect_err("manual-only source with remote discovery must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "source_policy_forbids_remote_discovery")
    );

    // discoveryCatalog cannot smuggle Manifest identities through a discovery-only policy.
    // discoveryCatalog 不能通过 discovery-only 策略夹带 Manifest 身份。
    let mut discovery_catalog = minimal_manifest();
    let catalog = discovery_catalog.services[0]
        .model_catalog
        .as_mut()
        .expect("minimal fixture has a catalog");
    catalog.source_policy = ModelSourcePolicy::DiscoveryOnly;
    catalog.discovery_kind = Some(ModelDiscoveryKind::OpenAi);
    catalog.models = Some(vec![ManifestCatalogModel {
        model_id: String::from("manifest-model"),
        display_name: String::from("Manifest Model"),
    }]);
    let errors = validate_manifest(&discovery_catalog, ManifestOwnership::System, &registry())
        .expect_err("discovery-only source with Manifest inventory must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "catalog_models_require_manifest_source")
    );

    // manifestWithoutInventory cannot leave the sole identity source ambiguous.
    // manifestWithoutInventory 不能让唯一身份来源处于歧义状态。
    let mut manifest_without_inventory = minimal_manifest();
    let catalog = manifest_without_inventory.services[0]
        .model_catalog
        .as_mut()
        .expect("minimal fixture has a catalog");
    catalog.source_policy = ModelSourcePolicy::ManifestOnly;
    catalog.discovery_kind = Some(ModelDiscoveryKind::ManifestStatic);
    catalog.models = None;
    let errors = validate_manifest(
        &manifest_without_inventory,
        ManifestOwnership::System,
        &registry(),
    )
    .expect_err("manifest-only source without inventory must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "manifest_only_requires_catalog_models")
    );
}

/// Verifies credential modes and authorization declarations remain a closed pairing.
/// 验证凭据模式与授权声明保持封闭配对关系。
#[test]
fn credential_authorization_must_match_credential_mode() {
    let mut missing = minimal_manifest();
    missing.services[0].credential_profiles[0].credential_mode = CredentialMode::Auth;
    let errors = validate_manifest(&missing, ManifestOwnership::System, &registry())
        .expect_err("auth profile without authorization must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "missing_credential_authorization")
    );

    let mut unexpected = minimal_manifest();
    unexpected.services[0].credential_profiles[0].authorization =
        Some(CredentialAuthorization::RegisteredOauth {
            provider: crate::OAuthProviderKind::OpenAiCodex,
        });
    let errors = validate_manifest(&unexpected, ManifestOwnership::System, &registry())
        .expect_err("token profile with authorization must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "unexpected_credential_authorization")
    );
}

/// Verifies browser callbacks are system-only and enforce listener and mapping bounds.
/// 验证浏览器回调仅限系统供应商并强制监听器与映射边界。
#[test]
fn browser_callback_authorization_is_bounded_and_system_only() {
    let mut manifest = minimal_manifest();
    let profile = &mut manifest.services[0].credential_profiles[0];
    profile.credential_mode = CredentialMode::Auth;
    profile.authorization = Some(CredentialAuthorization::BrowserCallback {
        profile: Box::new(crate::BrowserCallbackAuthorization {
            authorization_url: String::from("https://commandcode.ai/studio/auth/cli"),
            callback_query_parameter: String::from("callback"),
            state_query_parameter: String::from("state"),
            listener: crate::BrowserCallbackListener {
                url_host: crate::BrowserCallbackUrlHost::Localhost,
                path: String::from("/callback"),
                port_start: 5959,
                port_end: 5968,
                timeout_seconds: 300,
                max_body_bytes: 10 * 1024,
            },
            response: crate::BrowserCallbackResponseMapping {
                secret_json_pointer: String::from("/apiKey"),
                state_json_pointer: String::from("/state"),
                subject_json_pointer: String::from("/userId"),
                username_json_pointer: String::from("/userName"),
                credential_label_json_pointer: String::from("/keyName"),
                error_json_pointer: Some(String::from("/error")),
                error_description_json_pointer: Some(String::from("/error_description")),
                secret_projection: crate::BrowserCallbackSecretProjection::Bearer,
            },
        }),
    });
    validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect("reviewed system callback should validate");

    if let Some(CredentialAuthorization::BrowserCallback { profile }) = manifest.services[0]
        .credential_profiles[0]
        .authorization
        .as_mut()
    {
        profile.authorization_url = String::from("https://commandcode.ai:8443/studio/auth/cli");
    }
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("browser authorization on a non-default port must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "browser_authorization_non_default_port")
    );
    if let Some(CredentialAuthorization::BrowserCallback { profile }) = manifest.services[0]
        .credential_profiles[0]
        .authorization
        .as_mut()
    {
        profile.authorization_url = String::from("https://commandcode.ai/studio/auth/cli");
    }

    if let Some(CredentialAuthorization::BrowserCallback { profile }) = manifest.services[0]
        .credential_profiles[0]
        .authorization
        .as_mut()
    {
        profile.response.error_json_pointer = Some(String::from("/state"));
    }
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("duplicate callback pointers must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "duplicate_browser_callback_json_pointer")
    );
    if let Some(CredentialAuthorization::BrowserCallback { profile }) = manifest.services[0]
        .credential_profiles[0]
        .authorization
        .as_mut()
    {
        profile.response.error_json_pointer = Some(String::from("/error"));
    }

    let errors = validate_manifest(&manifest, ManifestOwnership::Custom, &registry())
        .expect_err("custom callback must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "custom_credential_authorization_forbidden")
    );

    if let Some(CredentialAuthorization::BrowserCallback { profile }) = manifest.services[0]
        .credential_profiles[0]
        .authorization
        .as_mut()
    {
        profile.listener.port_end = 6000;
        profile.response.state_json_pointer = String::from("invalid");
    }
    let errors = validate_manifest(&manifest, ManifestOwnership::System, &registry())
        .expect_err("oversized range and invalid pointer must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.code == "invalid_browser_callback_port_range")
    );
    assert!(
        errors
            .iter()
            .any(|error| error.code == "invalid_browser_callback_json_pointer")
    );
}

/// Builds one default tool target declaration for closure tests.
/// 为闭合测试构建一条默认工具目标声明。
fn tool_target_declaration(
    service_entry_id: &str,
    capability: crate::ProviderToolCapabilityKind,
    execution_kind: crate::ProviderToolExecutionKind,
) -> crate::provider_manifest::schema::ToolTargetDeclaration {
    crate::provider_manifest::schema::ToolTargetDeclaration {
        target_id: String::from("public-api.search"),
        description: String::from("test search target"),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_ids: BTreeSet::from([String::from("api-key")]),
        upstream_target_id: String::from("search"),
        availability_model_ids_any: BTreeSet::new(),
        display_name: String::from("Test Search"),
        tool_id: String::from("vulcan_web_search"),
        capability,
        input_schema: serde_json::json!({"type": "object"}),
        input_numeric_product_ranges: Vec::new(),
        extension_profile: None,
        media_input_projection: None,
        output: crate::ProviderToolOutputContract {
            kind: crate::ProviderToolOutputKind::Search,
            source_path: String::from("/results"),
            normalized_result_path: String::from("search"),
            resource_kind: None,
            mime_types: Vec::new(),
            resource_decoders: Vec::new(),
        },
        execution: crate::ProviderToolExecutionContract {
            endpoint_base_url: None,
            endpoint_path: String::from("/search"),
            authentication: crate::AuthenticationProjection::BearerAuthorization,
            adapter: crate::ProviderToolAdapterKind::GenericJson,
            execution_kind,
            multipart_request: None,
            sse_events: None,
            fixed_headers: BTreeMap::new(),
            start_fixed_headers: BTreeMap::new(),
            start_conditional_headers: Vec::new(),
            mcp_http_call: None,
        },
    }
}

/// Builds one closed web-search operation referencing the operation request test profile.
/// 构建一个引用操作请求测试 Profile 的闭合网页搜索操作。
fn operation_with_request_profile() -> crate::provider_manifest::schema::OperationDeclaration {
    crate::provider_manifest::schema::OperationDeclaration {
        operation_id: String::from("op.web-search"),
        operation: String::from("web_search"),
        carrier: crate::provider_manifest::schema::OperationCarrier::ProviderTool,
        input_resources: Vec::new(),
        output_resources: Vec::new(),
        tool_target_refs: vec![String::from("public-api.search")],
        tool_target_template_refs: Vec::new(),
        request_profile_refs: vec![String::from("operation.request.v1")],
        response_profile_refs: Vec::new(),
        upload_channel_refs: Vec::new(),
        upload_bindings: Vec::new(),
        async_execution: None,
        timeout_ms: Some(60_000),
        idempotent: false,
    }
}
