//! Tests for manifest-only target authority, activation boundaries, and account-scoped access.
//! 仅清单持有目标准入权威、激活边界及账号作用域访问的测试。

use super::super::test_support::{
    connection, custom_connection, install_source_catalog, multi_protocol_custom_definition,
    with_protocol_variants,
};
use super::*;
use crate::parse_openai_model_catalog;
use std::sync::Arc;
use vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID;

/// Verifies tool policy belongs to the published generation rather than the ignored caller configuration.
/// 验证工具策略属于已发布代际，而不是被忽略的调用者配置参数。
#[test]
fn provider_tool_media_policy_is_frozen_with_published_generation() {
    // Isolated runtime and configuration use a bundled non-network target for resolution only.
    // 隔离运行时与配置仅使用内置目标进行解析，不发起网络调用。
    let runtime = PublicAiRuntime::unavailable().expect("isolated runtime");
    let mut configuration = PersistedAiConfiguration::default();
    let mut account = connection("policy-account");
    account.provider_id = String::from("tavily");
    account.service_entry_id = String::from("public-api");
    configuration
        .connection_graph
        .add_connection(&runtime.providers().expect("bundled providers"), account)
        .expect("policy account");
    // Distinct valid values make an accidental caller-configuration read observable.
    // 不同的有效值使意外读取调用者配置能够被观察到。
    let mut first_policy = crate::MediaDeliveryPolicy::default();
    first_policy
        .independent_extension
        .video
        .with_upload_max_bytes = 1;
    configuration
        .set_media_delivery_policy(first_policy)
        .expect("first policy");
    // Explicit activation is the only publication boundary used by this test.
    // 显式激活是本测试唯一使用的发布边界。
    let executor = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test executor");
    executor
        .block_on(runtime.activate_staged_manifest_catalogs(&configuration, &[]))
        .expect("first generation");
    // The exact available target supplies identity instead of a guessed target name.
    // 精确可用目标提供身份，不猜测目标名称。
    let available = runtime
        .available_provider_tool_targets(&configuration)
        .expect("available targets")
        .into_iter()
        .find(|available| {
            available.target.contract.capability == ProviderToolCapabilityKind::WebSearch
        })
        .expect("bundled search target");
    let selection = ProviderToolTargetSelection {
        connection_id: available.connection_id,
        target_id: available.target.target_id,
    };
    let first = runtime
        .resolve_provider_tool_target(
            &configuration,
            ProviderToolCapabilityKind::WebSearch,
            &selection,
        )
        .expect("frozen first target");
    assert_eq!(first.media_delivery_policy, Some(first_policy));
    // Merely passing an unpublished configuration must not override the retained generation.
    // 仅传入未发布配置不得覆盖保留代际。
    let mut second_policy = first_policy;
    second_policy
        .independent_extension
        .video
        .with_upload_max_bytes = 2;
    configuration
        .set_media_delivery_policy(second_policy)
        .expect("second policy");
    let unpublished = runtime
        .resolve_provider_tool_target(
            &configuration,
            ProviderToolCapabilityKind::WebSearch,
            &selection,
        )
        .expect("retained generation");
    assert_eq!(unpublished.media_delivery_policy, Some(first_policy));
    executor
        .block_on(runtime.activate_staged_manifest_catalogs(&configuration, &[]))
        .expect("second generation");
    let second = runtime
        .resolve_provider_tool_target(
            &configuration,
            ProviderToolCapabilityKind::WebSearch,
            &selection,
        )
        .expect("newly frozen target");
    assert_eq!(second.media_delivery_policy, Some(second_policy));
    assert_eq!(first.media_delivery_policy, Some(first_policy));
}

/// Verifies one full custom manifest publishes tool targets only at the explicit activation boundary.
/// 验证一份完整自定义 Manifest 仅在显式激活边界发布工具目标。
#[test]
fn custom_manifest_tool_targets_publish_as_one_frozen_generation() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = multi_protocol_custom_definition(
        "custom:manifest-tools",
        &[crate::CustomProviderProtocol::OpenAiChat],
    );
    configuration
        .custom_providers
        .insert(provider.id.clone(), provider.clone());
    let effective_providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    configuration
        .connection_graph
        .add_connection(
            &effective_providers,
            custom_connection("manifest-tools-account", provider.id.as_str()),
        )
        .expect("custom connection should register");
    // sourceCatalog supplies one exact executable model whose binding must carry the custom
    // manifest revision across activation boundaries.
    // sourceCatalog 提供一个精确可执行模型，其绑定必须跨激活边界携带自定义 Manifest 修订。
    let source_catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "manifest-tools-account",
            1,
            r#"{"data":[{"id":"manifest/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("custom manifest model catalog should parse"),
        &[OPENAI_CHAT_PROFILE_ID],
    );
    install_source_catalog(&runtime, source_catalog);
    configuration.set_model_enabled(provider.id.as_str(), "manifest/model", true);

    let mut document = serde_json::to_value(
        crate::provider_manifest::project_custom_descriptor_manifest(&provider)
            .expect("custom descriptor should project to a manifest"),
    )
    .expect("custom manifest should serialize");
    document["engine_contract"]["required_execution_primitives"] =
        serde_json::json!(["http_json_sync"]);
    document["tool_targets"] = serde_json::json!([{
        "target_id": "default.search",
        "description": "Generic custom-provider search fixture.",
        "service_entry_id": "default",
        "credential_profile_ids": [],
        "upstream_target_id": "search-v1",
        "display_name": "Manifest Search V1",
        "tool_id": "vulcan_web_search",
        "capability": "web_search",
        "input_schema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {"query": {"type": "string", "minLength": 1}},
            "required": ["query"]
        },
        "output": {
            "kind": "search",
            "source_path": "/results",
            "normalized_result_path": "search",
            "resource_kind": null,
            "mime_types": []
        },
        "execution": {
            "endpoint_path": "/search",
            "authentication": "bearer_authorization",
            "adapter": "generic_json",
            "execution_kind": "http_json_sync"
        }
    }]);
    document["operations"] = serde_json::json!([{
        "operation_id": "manifest-tools.search",
        "operation": "web_search",
        "carrier": "provider_tool",
        "input_resources": [],
        "output_resources": [],
        "tool_target_refs": ["default.search"],
        "request_profile_refs": [],
        "response_profile_refs": [],
        "upload_channel_refs": [],
        "upload_bindings": [],
        "timeout_ms": 60000,
        "idempotent": false
    }]);
    let manifest: crate::provider_manifest::ProviderManifestV1 =
        serde_json::from_value(document.clone()).expect("custom manifest should deserialize");
    configuration
        .custom_provider_manifests
        .insert(provider.id.clone(), manifest);

    assert!(
        runtime
            .available_provider_tool_targets(&configuration)
            .expect("pre-activation targets should resolve")
            .is_empty(),
        "an unactivated custom manifest must remain invisible"
    );
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("custom manifest generation should publish");
    let activated = runtime
        .available_provider_tool_targets(&configuration)
        .expect("activated custom targets should resolve");
    assert_eq!(activated.len(), 1);
    assert!(
        activated[0].target.credential_profile_ids.is_empty(),
        "an empty target credential scope must authorize every profile"
    );
    assert_eq!(activated[0].target.display_name, "Manifest Search V1");
    // firstBinding proves custom model provenance retains the activated manifest revision.
    // firstBinding 证明自定义模型来源保留已激活 Manifest 修订。
    let first_binding = runtime
        .resolve_connection_model_selection(
            &configuration,
            "manifest-tools-account",
            provider.id.as_str(),
            "manifest/model",
        )
        .expect("first custom manifest model must resolve")
        .protocol_binding;
    assert_eq!(first_binding.manifest_revision, 1);

    document["revision"] = serde_json::json!(2);
    document["tool_targets"][0]["display_name"] = serde_json::json!("Manifest Search V2");
    configuration.custom_provider_manifests.insert(
        provider.id.clone(),
        serde_json::from_value(document).expect("replacement manifest should deserialize"),
    );
    let still_frozen = runtime
        .available_provider_tool_targets(&configuration)
        .expect("frozen custom targets should resolve");
    assert_eq!(still_frozen[0].target.display_name, "Manifest Search V1");
    // stillFrozenBinding proves an unactivated file edit cannot alter model provenance.
    // stillFrozenBinding 证明未激活的文件编辑不能改变模型来源。
    let still_frozen_binding = runtime
        .resolve_connection_model_selection(
            &configuration,
            "manifest-tools-account",
            provider.id.as_str(),
            "manifest/model",
        )
        .expect("pre-activation custom model must remain selectable")
        .protocol_binding;
    assert_eq!(still_frozen_binding.manifest_revision, 1);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("replacement custom generation should publish");
    let replaced = runtime
        .available_provider_tool_targets(&configuration)
        .expect("replacement custom targets should resolve");
    assert_eq!(replaced[0].target.display_name, "Manifest Search V2");
    // replacedBinding proves the next activation atomically advances binding provenance.
    // replacedBinding 证明下一次激活会原子推进绑定来源修订。
    let replaced_binding = runtime
        .resolve_connection_model_selection(
            &configuration,
            "manifest-tools-account",
            provider.id.as_str(),
            "manifest/model",
        )
        .expect("replacement custom manifest model must resolve")
        .protocol_binding;
    assert_eq!(replaced_binding.manifest_revision, 2);
}

/// Verifies account model access remains authoritative for extension target availability.
/// 验证账号模型访问仍对扩展目标可用性保持权威。
#[test]
fn provider_tool_targets_obey_account_model_access() {
    // runtime owns the bundled APIKEY.FUN manifest and its reviewed fixed targets.
    // runtime 持有内置 APIKEY.FUN Manifest 及其已审核固定目标。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // configuration owns one exact APIKEY.FUN credential account.
    // configuration 持有一个精确的 APIKEY.FUN 凭据账号。
    let mut configuration = PersistedAiConfiguration::default();
    // apikeyfunConnection binds the exact public service and API-key credential profile.
    // apikeyfunConnection 绑定精确公共服务与 API Key 凭据配置档。
    let mut apikeyfun_connection = connection("apikeyfun-media-account");
    apikeyfun_connection.provider_id = String::from("apikeyfun");
    apikeyfun_connection.service_entry_id = String::from("grok-media");
    apikeyfun_connection.credential_profile_id = String::from("api-key");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            apikeyfun_connection,
        )
        .expect("APIKEY.FUN media connection should register");
    // sourceCatalog reproduces the exact per-key `/models` evidence required by APIKEY.FUN.
    // sourceCatalog 复现 APIKEY.FUN 所需的精确逐密钥 `/models` 证据。
    let source_catalog = parse_openai_model_catalog(
        "apikeyfun-media-account",
        1,
        r#"{"data":[{"id":"grok-imagine-video"}]}"#,
    )
    .expect("APIKEY.FUN discovery fixture should parse");
    install_source_catalog(&runtime, source_catalog);
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test async runtime should initialize")
        .block_on(runtime.activate_staged_manifest_catalogs(&configuration, &[]))
        .expect("APIKEY.FUN generation should publish");
    // initialTarget proves the account catalog initially exposes the fixed video model.
    // initialTarget 证明账号目录最初公开固定视频模型。
    let initial_target = runtime
        .available_provider_tool_targets(&configuration)
        .expect("initial extension target inventory should resolve")
        .into_iter()
        .find(|available| available.target.upstream_target_id == "grok-imagine-video")
        .expect("Grok Imagine video target should initially be available");
    // selection freezes the exact credential and extension target used for execution revalidation.
    // selection 冻结用于执行重新校验的精确凭据与扩展目标。
    let selection = ProviderToolTargetSelection {
        connection_id: String::from("apikeyfun-media-account"),
        target_id: initial_target.target.target_id,
    };
    // Account access remains authoritative even though conversation publication is independent.
    // 即使会话发布保持独立，账号访问仍保持权威。
    configuration.set_connection_model_enabled(
        "apikeyfun-media-account",
        "grok-imagine-video",
        false,
    );
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("account access update should rebuild effective catalogs");
    // filteredTargets proves the disabled account model disappears from extension inventory.
    // filteredTargets 证明被账号禁用的模型会从扩展清单消失。
    let filtered_targets = runtime
        .available_provider_tool_targets(&configuration)
        .expect("account-filtered target inventory should resolve");
    assert!(
        filtered_targets
            .iter()
            .all(|available| available.target.upstream_target_id != "grok-imagine-video")
    );
    // error proves direct execution cannot bypass the account-filtered extension inventory.
    // error 证明直接执行无法绕过账号权限过滤后的扩展清单。
    let error = runtime
        .resolve_provider_tool_target(
            &configuration,
            ProviderToolCapabilityKind::VideoGenerate,
            &selection,
        )
        .map(|_| ())
        .expect_err("account-disabled extension target must reject direct resolution");
    assert!(
        error.contains("unavailable for the current credential model catalog"),
        "{error}"
    );
}

/// Verifies manifest-owned tool targets are the sole authority for availability and hot replacement.
/// 验证 Manifest 持有的工具目标是可用性与热替换的唯一权威。
#[test]
fn manifest_owned_tool_targets_drive_availability_and_resolution() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let mut tavily_connection = connection("tavily-account");
    tavily_connection.provider_id = String::from("tavily");
    tavily_connection.service_entry_id = String::from("public-api");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            tavily_connection,
        )
        .expect("connection should register");

    // The initial target inventory comes directly from the bundled Tavily manifest.
    // 初始目标清单直接来自内置 Tavily Manifest。
    let initial_manifest = runtime
        .available_provider_tool_targets(&configuration)
        .expect("bundled manifest targets must resolve");
    assert_eq!(
        initial_manifest.len(),
        2,
        "the bundled tavily manifest exposes two targets"
    );

    // A newly compiled manifest atomically replaces the same authoritative target set.
    // 新编译的 Manifest 原子替换同一份权威目标集合。
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let text = std::fs::read_to_string(directory.join("tavily.json"))
        .expect("bundled tavily manifest must exist");
    let mut document: serde_json::Value =
        serde_json::from_str(&text).expect("manifest should parse");
    let search_schema = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "query": {"type": "string", "minLength": 1},
            "max_results": {"type": "integer", "minimum": 1, "maximum": 20, "default": 5}
        },
        "required": ["query"]
    });
    document["tool_targets"] = serde_json::json!([
        {
            "target_id": "public-api.search",
            "description": "Tavily structured web search.",
            "service_entry_id": "public-api",
            "credential_profile_ids": ["api-key"],
            "upstream_target_id": "search",
            "display_name": "Tavily Search",
            "tool_id": "vulcan_web_search",
            "capability": "web_search",
            "input_schema": search_schema,
            "output": {
                "kind": "search",
                "source_path": "/results",
                "normalized_result_path": "search",
                "resource_kind": null,
                "mime_types": []
            },
            "execution": {
                "endpoint_path": "/search",
                "authentication": "bearer_authorization",
                "adapter": "generic_json",
                "execution_kind": "http_json_sync"
            }
        },
        {
            "target_id": "public-api.extract",
            "description": "Tavily structured web extraction.",
            "service_entry_id": "public-api",
            "credential_profile_ids": ["api-key"],
            "upstream_target_id": "extract",
            "display_name": "Tavily Extract",
            "tool_id": "vulcan_web_extract",
            "capability": "web_extract",
            "input_schema": serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "urls": {"type": "array", "minItems": 1, "maxItems": 20, "uniqueItems": true, "items": {"type": "string"}}
                },
                "required": ["urls"]
            }),
            "output": {
                "kind": "extract",
                "source_path": "/results",
                "normalized_result_path": "extract",
                "resource_kind": null,
                "mime_types": []
            },
            "execution": {
                "endpoint_path": "/extract",
                "authentication": "bearer_authorization",
                "adapter": "generic_json",
                "execution_kind": "http_json_sync"
            }
        }
    ]);
    let compiled = crate::provider_manifest::compile_manifest_text(
        &serde_json::to_string_pretty(&document).expect("manifest should serialize"),
        crate::provider_manifest::ManifestOwnership::System,
    )
    .expect("manifest with tool targets must compile");
    runtime
        .replace_system_manifest("tavily", Arc::new(compiled))
        .expect("hot replace should succeed");
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test async runtime should initialize")
        .block_on(runtime.activate_staged_manifest_catalogs(&configuration, &[]))
        .expect("tavily target generation should publish");

    let manifest_driven = runtime
        .available_provider_tool_targets(&configuration)
        .expect("manifest targets must resolve");
    assert_eq!(
        manifest_driven.len(),
        2,
        "the replacement manifest must publish both tavily targets"
    );
    for available in &manifest_driven {
        assert_eq!(
            available.target.provider_id, "tavily",
            "manifest targets keep the compiled provider id"
        );
        assert!(
            matches!(
                available.target.contract.tool_id.as_str(),
                "vulcan_web_search" | "vulcan_web_extract"
            ),
            "manifest targets keep the declared tool id"
        );
    }
}
