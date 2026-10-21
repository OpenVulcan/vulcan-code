//! Stable semantic identities for effective catalogs and exact execution routes.
//! 生效目录与精确执行路由的稳定语义身份。

use crate::{ConnectionModel, ConnectionModelCatalog, ProviderExecutionRoute};
use sha2::{Digest, Sha256};

/// Builds a stable content revision for one effective connection model catalog.
/// 为一个生效连接模型目录构建稳定内容修订号。
///
/// # Parameters
/// 参数
/// `catalog` is the complete catalog frozen at one activation boundary.
/// `catalog` 是在一个激活边界冻结的完整目录。
///
/// # Returns
/// 返回值
/// A SHA-256 revision over semantic catalog content, excluding process-local observation counters.
/// 返回覆盖目录语义内容、排除进程内观测计数器的 SHA-256 修订号。
pub(super) fn effective_model_catalog_content_revision(
    catalog: &ConnectionModelCatalog,
) -> Result<String, String> {
    // canonicalCatalog preserves every execution-relevant fact while removing refresh counters
    // and response ordering that cannot identify content across a process restart.
    // canonicalCatalog 保留每项与执行相关的事实，同时移除无法跨进程重启标识内容的刷新计数器
    // 与响应顺序。
    let mut canonical_catalog = catalog.clone();
    canonical_catalog.generation = 0;
    canonical_catalog
        .models
        .sort_by(|left, right| left.id.cmp(&right.id));
    for model in &mut canonical_catalog.models {
        for variant in model.protocol_variants.values_mut() {
            variant.catalog_generation = 0;
        }
    }
    // material is a closed serialization of the canonical catalog shape.
    // material 是规范目录形态的封闭序列化结果。
    let material = serde_json::to_vec(&canonical_catalog).map_err(|error| {
        format!("failed to serialize effective model catalog revision: {error}")
    })?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(material))))
}

/// Builds the immutable revision for one exact LLM execution route.
/// 为一条精确 LLM 执行路由构建不可变修订号。
///
/// # Parameters
/// 参数
/// `route`, `model`, and `overrides` are the validated execution snapshots frozen at selection
/// time. Catalog, manifest, discovery, and presentation revisions are deliberately excluded
/// because they are audit facts rather than wire semantics (plan §17.7).
/// `route`、`model` 与 `overrides` 是选择时冻结的已校验执行快照。目录、Manifest、发现与
/// 展示修订被刻意排除，因为它们是审计事实而非 Wire 语义（方案 §17.7）。
///
/// # Returns
/// 返回值
/// Returns a SHA-256 content revision or a serialization error.
/// 返回 SHA-256 内容修订号或序列化错误。
pub(super) fn provider_execution_route_revision(
    route: &ProviderExecutionRoute,
    model: &ConnectionModel,
    overrides: &crate::ModelExecutionOverrides,
) -> Result<String, String> {
    // selectedCredentialProfile is the only profile whose authentication and upload contract can
    // affect this route; other profiles may be added without changing an active session.
    // selectedCredentialProfile 是唯一会影响此路由鉴权与上传契约的配置档；新增其他配置档
    // 不得改变活动会话。
    let selected_credential_profile = route
        .entry
        .credential_profile(route.connection.credential_profile_id.as_str())
        .ok_or_else(|| {
            format!(
                "credential profile `{}` is absent from execution route `{}`",
                route.connection.credential_profile_id, route.entry.id
            )
        })?;
    // canonicalCredentialProfile removes its user-facing label while preserving the complete
    // selected credential contract.
    // canonicalCredentialProfile 移除面向用户的标签，同时保留完整的已选凭据契约。
    let mut canonical_credential_profile = selected_credential_profile.clone();
    canonical_credential_profile.display_name.clear();
    // canonicalOriginPolicy excludes the non-secret UI example but retains every enforced host
    // and path restriction.
    // canonicalOriginPolicy 排除非敏感 UI 示例，同时保留全部强制主机与路径限制。
    let mut canonical_origin_policy = route.entry.service_origin_override_policy.clone();
    if let Some(policy) = canonical_origin_policy.as_mut() {
        policy.example_origin.clear();
    }
    // canonicalProtocolRoute removes model-discovery controls because discovery has completed
    // before this immutable execution route is built.
    // canonicalProtocolRoute 移除模型发现控制，因为不可变执行路由构建前发现已经完成。
    let mut canonical_protocol_route = route.protocol_route.clone();
    canonical_protocol_route.model_discovery = None;
    canonical_protocol_route.model_discovery_path = None;
    canonical_protocol_route.model_discovery_base_url_override = None;
    // entryExecutionIdentity contains exactly the selected service entry facts consumed after
    // selection, excluding its label, discovery policy, unrelated credential profiles, and
    // unrelated protocol routes.
    // entryExecutionIdentity 仅包含选择后消费的服务入口事实，排除标签、发现策略、无关凭据档
    // 与无关协议路由。
    let entry_execution_identity = (
        &route.entry.id,
        route.entry.kind,
        &route.entry.base_url,
        &canonical_credential_profile,
        route.entry.authentication,
        &route.entry.request_headers,
        &canonical_origin_policy,
        &route.entry.native_inference_bindings,
        route.entry.request_projector,
        &route.entry.default_usage_scopes,
    );
    // connectionExecutionIdentity retains credential, origin, and authorization ownership while
    // excluding the account label, management-only secret, and policies already resolved into
    // `overrides`.
    // connectionExecutionIdentity 保留凭据、源地址与授权归属，同时排除账号标签、仅管理秘密，
    // 以及已解析进 `overrides` 的策略。
    let connection_execution_identity = (
        &route.connection.id,
        &route.connection.provider_id,
        &route.connection.service_entry_id,
        &route.connection.credential_profile_id,
        route.connection.credential_mode,
        &route.connection.credential_id,
        &route.connection.secret_reference,
        &route.connection.service_origin_override,
    );
    // modelExecutionIdentity contains the only model facts read during request construction and
    // response normalization. Display labels, discovery evidence, other protocol variants, and
    // manifest provenance remain available on the frozen selection for diagnostics.
    // modelExecutionIdentity 包含请求构造与响应归一化期间读取的唯一模型事实。展示标签、发现
    // 证据、其他协议变体与 Manifest 来源仍保留在冻结选择中供诊断。
    let model_execution_identity = (&model.id, &model.capabilities);
    let material = serde_json::to_vec(&(
        (&route.provider.id, route.provider.kind),
        entry_execution_identity,
        connection_execution_identity,
        &canonical_protocol_route,
        &route.secondary_route,
        &route.custom_model_projection,
        route.tool_schema_projection,
        route.tool_mode,
        &route.codex_behavior,
        &route.request_projection_profiles,
        &route.response_projection_profiles,
        &route.upload_channels,
        model_execution_identity,
        overrides,
    ))
    .map_err(|error| format!("failed to serialize provider route revision: {error}"))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(material))))
}

#[cfg(test)]
mod tests;
