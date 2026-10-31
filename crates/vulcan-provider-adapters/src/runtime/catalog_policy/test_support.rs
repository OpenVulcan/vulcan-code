//! Explicit descriptor fixtures that exercise the production catalog projection pipeline.
//! 使用显式描述器夹具验证生产目录投影流水线。

use super::super::catalog_sources::resolve_custom_model_service_entry;
use super::{
    MANUAL_MODEL_CATALOG_SCHEMA_REVISION, apply_model_catalog_overlays_with_account_access,
};
use crate::{ConnectionModelCatalog, ModelSourcePolicy, PersistedAiConfiguration};
use std::collections::BTreeMap;

/// Applies immutable common defaults, custom definitions, and user differences to one catalog.
/// 将不可变公共默认、自定义定义与用户差异应用到一个目录。
///
/// # Parameters
/// 参数
/// `configuration` owns user data, `provider_id` is exact, and `catalog` is rebuilt in place.
/// `configuration` 持有用户数据，`provider_id` 为精确标识，`catalog` 会被原地重建。
#[cfg(test)]
pub(in crate::runtime) fn apply_model_catalog_overlays(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    catalog: &mut ConnectionModelCatalog,
) -> Result<(), String> {
    // sourcePolicy is explicit test input derived from the fixture's authoritative source marker.
    // sourcePolicy 是从夹具权威来源标记派生的显式测试输入。
    let source_policy = if catalog.discovery_schema_revision == MANUAL_MODEL_CATALOG_SCHEMA_REVISION
    {
        ModelSourcePolicy::ManualOnly
    } else if provider_id.starts_with("custom:") {
        ModelSourcePolicy::DiscoveryAndManual
    } else {
        ModelSourcePolicy::DiscoveryOnly
    };
    // fixtureConfiguration supplies the explicit descriptor that production callers always own;
    // unit-level overlay tests predating custom descriptors use one reviewed Chat fixture instead
    // of exercising an impossible descriptor-free runtime state.
    // fixtureConfiguration 提供生产调用方始终持有的显式描述；早于自定义描述器的覆盖单元测试
    // 使用一份已审核 Chat 夹具，而不是构造运行时不可能存在的无描述状态。
    let mut fixture_configuration = configuration.clone();
    if provider_id.starts_with("custom:")
        && !fixture_configuration
            .custom_providers
            .contains_key(provider_id)
    {
        let discovery_mode = if source_policy == ModelSourcePolicy::ManualOnly {
            crate::CustomProviderModelDiscoveryMode::Manual
        } else {
            crate::CustomProviderModelDiscoveryMode::Standard
        };
        let descriptor = crate::CustomProviderDefinition {
            id: provider_id.to_string(),
            display_name: String::from("Catalog Overlay Fixture"),
            base_url: String::from("https://fixture.example.test/v1"),
            protocols: vec![crate::CustomProviderProtocol::OpenAiChat],
            authentication: crate::CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: discovery_mode,
            model_discovery_protocol: crate::CustomProviderModelDiscoveryProtocol::OpenAi,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: None,
        }
        .into_descriptor()?;
        fixture_configuration
            .custom_providers
            .insert(provider_id.to_string(), descriptor);
    }
    let service_entry_id = if provider_id.starts_with("custom:") {
        resolve_custom_model_service_entry(&fixture_configuration, provider_id, "")?
            .id
            .clone()
    } else {
        String::new()
    };
    apply_model_catalog_overlays_with_account_access(
        &fixture_configuration,
        provider_id,
        service_entry_id.as_str(),
        None,
        source_policy,
        None,
        catalog,
    )
}
