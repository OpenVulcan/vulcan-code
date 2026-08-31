//! Semantic catalog and execution-route identity regressions.
//! 目录与执行路由语义身份回归测试。

use super::{effective_model_catalog_content_revision, provider_execution_route_revision};
use crate::runtime::test_support::{catalog, two_account_runtime};

/// Verifies effective catalog revisions ignore refresh counters but retain semantic changes.
/// 验证生效目录修订忽略刷新计数器，但保留语义变化。
#[test]
fn effective_catalog_revision_is_content_addressed() {
    // firstCatalog is the semantic baseline observed in generation one.
    // firstCatalog 是在第一代观测到的语义基线。
    let first_catalog = catalog("revision-connection");
    // refreshedCatalog reproduces the same catalog after another process-local refresh.
    // refreshedCatalog 表示另一次进程内刷新后得到的相同目录。
    let mut refreshed_catalog = first_catalog.clone();
    refreshed_catalog.generation = 41;
    for model in &mut refreshed_catalog.models {
        for variant in model.protocol_variants.values_mut() {
            variant.catalog_generation = 41;
        }
    }
    assert_eq!(
        effective_model_catalog_content_revision(&first_catalog)
            .expect("baseline catalog revision should serialize"),
        effective_model_catalog_content_revision(&refreshed_catalog)
            .expect("refreshed catalog revision should serialize"),
        "process-local generations must not change content identity"
    );

    // changedCatalog proves one execution-relevant capability still changes the digest.
    // changedCatalog 证明任一与执行相关的能力变化仍会改变摘要。
    let mut changed_catalog = refreshed_catalog;
    changed_catalog.models[0].capabilities.context_limit = Some(16_384);
    assert_ne!(
        effective_model_catalog_content_revision(&first_catalog)
            .expect("baseline catalog revision should serialize"),
        effective_model_catalog_content_revision(&changed_catalog)
            .expect("changed catalog revision should serialize"),
        "semantic catalog drift must change content identity"
    );
}

/// Verifies route identity ignores presentation and unrelated catalog facts but detects wire drift.
/// 验证路由身份忽略展示与无关目录事实，同时检测 Wire 漂移。
#[test]
fn route_revision_tracks_only_exact_execution_semantics() {
    // baselineSelection provides one fully resolved route with real bundled contracts.
    // baselineSelection 提供一条带真实内置契约的完整已解析路由。
    let (runtime, configuration) = two_account_runtime();
    let baseline_selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-5.6-sol",
        )
        .expect("baseline route should resolve");
    let baseline_revision = provider_execution_route_revision(
        &baseline_selection.route,
        &baseline_selection.model,
        &baseline_selection.overrides,
    )
    .expect("baseline revision should serialize");

    // presentationRoute changes only UI labels, audit counters, and an unrelated provider entry.
    // presentationRoute 仅修改 UI 标签、审计计数器与无关供应商入口。
    let mut presentation_route = baseline_selection.route.clone();
    presentation_route.provider.display_name = String::from("Renamed Provider");
    presentation_route.entry.display_name = String::from("Renamed Entry");
    presentation_route.connection.display_name = String::from("Renamed Account");
    presentation_route.manifest_revision = presentation_route.manifest_revision.saturating_add(9);
    presentation_route.manifest_generation =
        presentation_route.manifest_generation.saturating_add(11);
    if let Some(profile) = presentation_route.entry.credential_profiles.first_mut() {
        profile.display_name = String::from("Renamed Credential Profile");
    }
    let mut unrelated_entry = presentation_route.entry.clone();
    unrelated_entry.id = String::from("new-unrelated-entry");
    unrelated_entry.display_name = String::from("New Unrelated Entry");
    presentation_route
        .provider
        .service_entries
        .push(unrelated_entry);
    // presentationModel changes only the user-facing model label.
    // presentationModel 仅修改面向用户的模型标签。
    let mut presentation_model = baseline_selection.model.clone();
    presentation_model.display_name = String::from("Renamed Model");
    let presentation_revision = provider_execution_route_revision(
        &presentation_route,
        &presentation_model,
        &baseline_selection.overrides,
    )
    .expect("presentation-only revision should serialize");
    assert_eq!(
        presentation_revision, baseline_revision,
        "presentation and unrelated catalog changes must preserve route identity"
    );

    // changedRoute changes the exact selected endpoint and must therefore produce a new identity.
    // changedRoute 修改精确已选端点，因此必须产生新身份。
    let mut changed_route = baseline_selection.route.clone();
    changed_route.protocol_route.endpoint_path = Some(String::from("/different-wire-endpoint"));
    let changed_revision = provider_execution_route_revision(
        &changed_route,
        &baseline_selection.model,
        &baseline_selection.overrides,
    )
    .expect("changed route revision should serialize");
    assert_ne!(
        changed_revision, baseline_revision,
        "wire-semantic changes must invalidate route identity"
    );

    // schemaRoute changes the provider-visible function contract and must invalidate the route.
    // schemaRoute 修改 Provider 可见函数契约，因此必须使路由失效。
    let mut schema_route = baseline_selection.route.clone();
    schema_route.tool_schema_projection = crate::ToolSchemaProjectionMode::Passthrough;
    let schema_revision = provider_execution_route_revision(
        &schema_route,
        &baseline_selection.model,
        &baseline_selection.overrides,
    )
    .expect("tool-schema route revision should serialize");
    assert_ne!(
        schema_revision, baseline_revision,
        "tool-schema projection changes must invalidate route identity"
    );
}
