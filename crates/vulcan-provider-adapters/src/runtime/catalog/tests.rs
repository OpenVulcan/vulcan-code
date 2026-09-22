//! Atomic observation staging and closed failure-category regression tests.
//! 原子观测暂存与封闭失败分类回归测试。

use super::*;
use crate::parse_openai_model_catalog;

/// Verifies multi-connection staging publishes once and rejects a stale before-image.
/// 验证多连接暂存只发布一次，并拒绝过期前镜像。
#[test]
fn staged_connection_observation_batch_is_atomic_and_cas_guarded() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // beforeImage is the only state accepted by the batch CAS.
    // beforeImage 是批次 CAS 接受的唯一状态。
    let before_image = runtime
        .capture_activation_snapshot()
        .expect("before image should capture");
    // firstCatalog and secondCatalog represent one complete validated credential batch.
    // firstCatalog 与 secondCatalog 表示一批完整的已验证凭据。
    let first_catalog =
        parse_openai_model_catalog("first-account", 1, r#"{"data":[{"id":"first-model"}]}"#)
            .expect("first catalog should parse");
    let second_catalog =
        parse_openai_model_catalog("second-account", 1, r#"{"data":[{"id":"second-model"}]}"#)
            .expect("second catalog should parse");
    // readyStatus freezes one deterministic closed discovery result.
    // readyStatus 冻结一个确定性的封闭发现结果。
    let ready_status = CatalogDiscoveryStatus {
        state: CatalogDiscoveryState::Ready,
        failure_kind: None,
        message: None,
        model_count: 1,
        generation: 1,
        observed_at_ms: 1,
    };
    let after_image = runtime
        .stage_connection_catalog_observations_if_current(
            &before_image,
            &[
                StagedConnectionCatalogObservation {
                    connection_id: String::from("first-account"),
                    catalog: Some(first_catalog),
                    status: ready_status.clone(),
                    refresh_error: None,
                },
                StagedConnectionCatalogObservation {
                    connection_id: String::from("second-account"),
                    catalog: Some(second_catalog),
                    status: ready_status.clone(),
                    refresh_error: None,
                },
            ],
        )
        .expect("atomic observation batch should stage");
    assert_eq!(
        after_image.state_revision,
        before_image.state_revision + 1,
        "the whole batch must publish exactly once"
    );
    assert!(after_image.source_catalogs.get("first-account").is_some());
    assert!(after_image.source_catalogs.get("second-account").is_some());

    let stale_error = runtime
        .stage_connection_catalog_observations_if_current(
            &before_image,
            &[StagedConnectionCatalogObservation {
                connection_id: String::from("third-account"),
                catalog: None,
                status: ready_status,
                refresh_error: None,
            }],
        )
        .err()
        .expect("a stale before image must fail closed");
    assert!(stale_error.contains("staging refused"), "{stale_error}");
    let retained_image = runtime
        .capture_activation_snapshot()
        .expect("retained image should capture");
    assert_eq!(retained_image.state_revision, after_image.state_revision);
    assert!(
        retained_image
            .source_catalogs
            .get("third-account")
            .is_none()
    );
}

/// Verifies raw provider metadata with an oversized output ceiling does not block staging.
/// 验证输出上限超过上下文窗口的原始供应商元数据不会阻塞暂存。
#[test]
fn staged_source_catalog_accepts_output_above_context_window() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let before_image = runtime
        .capture_activation_snapshot()
        .expect("before image should capture");
    let catalog = parse_openai_model_catalog(
        "raw-account",
        1,
        r#"{"data":[{"id":"raw-model","max_output_tokens":300000}]}"#,
    )
    .expect("raw provider metadata should parse");
    let status = CatalogDiscoveryStatus {
        state: CatalogDiscoveryState::Ready,
        failure_kind: None,
        message: None,
        model_count: 1,
        generation: 1,
        observed_at_ms: 1,
    };

    let after_image = runtime
        .stage_connection_catalog_observations_if_current(
            &before_image,
            &[StagedConnectionCatalogObservation {
                connection_id: String::from("raw-account"),
                catalog: Some(catalog),
                status,
                refresh_error: None,
            }],
        )
        .expect("raw metadata should not fail the staging boundary");

    let staged_model = after_image
        .source_catalogs
        .get("raw-account")
        .and_then(|catalog| catalog.models.first())
        .expect("raw model should remain staged");
    assert_eq!(staged_model.capabilities.context_limit, None);
    assert_eq!(staged_model.capabilities.output_limit, Some(300_000));
}

/// Verifies every client-side failure category maps without message parsing.
/// 验证每一种客户端失败分类都无需解析消息即可完成映射。
#[test]
fn catalog_failure_mapping_preserves_every_typed_category() {
    for (source, expected) in [
        (
            ModelDiscoveryFailureKind::Unsupported,
            CatalogDiscoveryFailureKind::Unsupported,
        ),
        (
            ModelDiscoveryFailureKind::AuthenticationFailed,
            CatalogDiscoveryFailureKind::AuthenticationFailed,
        ),
        (
            ModelDiscoveryFailureKind::RateLimited,
            CatalogDiscoveryFailureKind::RateLimited,
        ),
        (
            ModelDiscoveryFailureKind::TemporarilyUnavailable,
            CatalogDiscoveryFailureKind::TemporarilyUnavailable,
        ),
        (
            ModelDiscoveryFailureKind::InvalidResponse,
            CatalogDiscoveryFailureKind::InvalidResponse,
        ),
        (
            ModelDiscoveryFailureKind::Failed,
            CatalogDiscoveryFailureKind::Failed,
        ),
    ] {
        assert_eq!(catalog_discovery_failure_kind(source), expected);
    }
}

use crate::PersistedAiConfiguration;
use crate::runtime::PublicAiRuntime;
use crate::runtime::test_support::{
    connection, custom_connection, custom_definition_with_discovery_mode, install_source_catalog,
};
use vulcan_ai_contract::{AiOperation, ModelCapabilities};

/// Verifies manual models remain usable when a provider has no discovered source catalog.
/// 验证供应商没有发现源目录时手工模型仍然可用。
#[test]
fn manual_custom_model_builds_catalog_without_provider_discovery() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = custom_definition_with_discovery_mode(
        "custom:manual-catalog",
        &[crate::CustomProviderProtocol::OpenAiChat],
        crate::CustomProviderModelDiscoveryMode::Manual,
    );
    configuration
        .custom_providers
        .insert(provider.id.clone(), provider);
    configuration.connection_graph.connections.insert(
        String::from("one"),
        custom_connection("one", "custom:manual-catalog"),
    );
    configuration
        .custom_models
        .entry(String::from("custom:manual-catalog"))
        .or_default()
        .insert(
            String::from("manual/chat"),
            crate::CustomModelDefinition {
                id: String::from("manual/chat"),
                display_name: String::from("Manual Chat"),
                enabled: true,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    context_limit: Some(200_000),
                    ..ModelCapabilities::default()
                },
                request_projection: crate::CustomModelRequestProjection::default(),
                tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
            },
        );

    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("manual-only catalog should rebuild");

    assert!(
        runtime
            .source_catalogs
            .read()
            .expect("source catalog lock should remain healthy")
            .get("one")
            .is_none(),
        "manual fallback must not masquerade as provider discovery"
    );
    let catalogs = runtime
        .catalog_snapshot()
        .expect("effective catalog should remain readable");
    let model = catalogs
        .get("one")
        .and_then(|catalog| catalog.models.first())
        .expect("manual model should be installed");
    assert_eq!(model.id, "manual/chat");
    assert_eq!(model.capabilities.context_limit, Some(200_000));
}

/// Verifies an activated custom catalog preserves discovery evidence and adds manual identities.
/// 验证已激活的自定义目录保留发现证据并添加手工模型身份。
#[test]
fn activated_custom_catalog_publishes_discovery_and_manual_union() {
    // runtime owns separate immutable source and activated effective generations.
    // runtime 分别持有不可变来源与已激活有效代际。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // configuration declares remote discovery and one independent manual identity.
    // configuration 声明远程发现与一个独立手工模型身份。
    let mut configuration = PersistedAiConfiguration::default();
    // provider uses the standard remote discovery contract, which resolves to the explicit
    // discovery-and-manual source policy.
    // provider 使用标准远程发现契约，其会解析为显式的发现与手工并集策略。
    let provider = custom_definition_with_discovery_mode(
        "custom:merged-catalog",
        &[crate::CustomProviderProtocol::OpenAiChat],
        crate::CustomProviderModelDiscoveryMode::Standard,
    );
    configuration
        .custom_providers
        .insert(provider.id.clone(), provider);
    configuration.connection_graph.connections.insert(
        String::from("merged-account"),
        custom_connection("merged-account", "custom:merged-catalog"),
    );
    configuration
        .custom_models
        .entry(String::from("custom:merged-catalog"))
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
            },
        );
    // sourceCatalog contains only provider-observed identity evidence.
    // sourceCatalog 仅包含供应商观测到的身份证据。
    let source_catalog = crate::parse_openai_model_catalog(
        "merged-account",
        7,
        r#"{"data":[{"id":"discovered/chat"}]}"#,
    )
    .expect("discovery catalog should parse");
    install_source_catalog(&runtime, source_catalog);

    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("merged catalog should activate");

    // activatedCatalog is the immutable reader-visible union after the explicit boundary.
    // activatedCatalog 是跨过显式边界后读取方可见的不可变并集。
    let activated_catalog = runtime
        .catalog_snapshot()
        .expect("activated catalog should remain readable")
        .remove("merged-account")
        .expect("merged account catalog should exist");
    assert!(
        activated_catalog
            .models
            .iter()
            .any(|model| model.id == "discovered/chat")
    );
    assert!(
        activated_catalog
            .models
            .iter()
            .any(|model| model.id == "manual/chat")
    );
    // retainedSource proves the manual identity did not masquerade as discovery evidence.
    // retainedSource 证明手工身份没有伪装成发现证据。
    let retained_source = runtime
        .source_catalogs
        .read()
        .expect("source catalog lock should remain healthy")
        .get("merged-account")
        .cloned()
        .expect("discovery source should remain available");
    assert_eq!(retained_source.models.len(), 1);
    assert_eq!(retained_source.models[0].id, "discovered/chat");
}

/// Verifies APIKEY.FUN authorizes Manifest discovery before reading the credential.
/// 验证 APIKEY.FUN 会先授权 Manifest 发现路由，再读取凭据。
#[tokio::test]
async fn apikeyfun_manifest_authorizes_discovery_before_credential_access() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    // apikeyfunConnection stores only credential and service identity.
    // apikeyfunConnection 仅保存凭据与服务身份，不保存任何协议历史。
    let mut apikeyfun_connection = connection("apikeyfun-account");
    apikeyfun_connection.provider_id = String::from("apikeyfun");
    apikeyfun_connection.service_entry_id = String::from("api");
    configuration.connection_graph.connections.insert(
        apikeyfun_connection.id.clone(),
        apikeyfun_connection.clone(),
    );

    // The unavailable store is the first expected boundary after discovery authorization. A
    // discovery-identity error here would prove the Manifest route was not used.
    // 不可用存储是发现授权后的首个预期边界；若此处出现发现身份错误，则证明未使用 Manifest 路由。
    let error = runtime
        .refresh_connection_catalog(&configuration, apikeyfun_connection.id.as_str())
        .await
        .expect_err("the unavailable test store must reject the credential read");
    assert!(
        error.contains("credential secret resolver"),
        "signed discovery should reach the credential boundary, got: {error}"
    );
    assert!(
        !error.contains("discovery identity without a remote discovery implementation"),
        "signed discovery must not be rejected by stale protocol history: {error}"
    );
}
