//! Shared public AI defaults and system-wide settings mutations.
//! 公共 AI 共享默认值与系统级设置写入。

use serde::Deserialize;
use vulcan_context::ContextWindowPolicyOverrides;
use vulcan_provider_adapters::ContextCompactionWindowConfiguration;
use vulcan_service_api::ServiceEventEmitter;

use super::publication::{
    mutate_public_ai_configuration_and_publish_in_transaction,
    mutate_public_ai_configuration_and_publish_using_in_transaction,
    publish_public_ai_workbench_after_context_compaction_window_nodes_change,
};
use super::{
    PublicAiSettingsSnapshot, build_public_ai_settings_snapshot,
    public_ai_runtime_and_configuration,
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

/// Updates the background account-usage refresh interval.
/// 更新后台账号用量刷新间隔。
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiUsageRefreshSettingsRequest {
    /// Refresh interval in seconds.
    /// 刷新间隔秒数。
    interval_seconds: u64,
}

/// Saves one exact provider priority used by settings presentation order.
/// 保存一个用于设置界面展示顺序的精确供应商优先级。
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiProviderPrioritySaveRequest {
    /// Exact registered provider identifier.
    /// 精确的已注册供应商标识。
    provider_id: String,
    /// Signed priority sorted from high to low; zero restores the inherited default.
    /// 从高到低排序的有符号优先级；零恢复继承默认值。
    priority: i32,
}

/// Saves or clears the system-level Codex context-window policy.
/// 保存或清除系统级 Codex 上下文窗口策略。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiContextWindowPolicyOverridesSaveRequest {
    /// Sparse policy layer; an empty object restores built-in Codex defaults.
    /// 稀疏策略层；空对象恢复内置 Codex 默认值。
    overrides: ContextWindowPolicyOverrides,
}

/// Saves all fixed system context-compaction window node values atomically.
/// 原子保存全部固定系统上下文压缩窗口节点值。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiContextCompactionWindowNodesSaveRequest {
    /// Complete five-node configuration with immutable identifiers and order.
    /// 包含不可变标识与顺序的完整五节点配置。
    nodes: ContextCompactionWindowConfiguration,
}

/// Saves the background account-usage refresh interval.
/// 保存后台账号用量刷新间隔。
///
/// # Parameters
/// 参数
/// `app` publishes service events, `state` owns the runtime, and `request` carries the exact settings change.
/// `app` 发布服务事件，`state` 持有运行时，`request` 携带本次精确设置变更。
///
/// # Returns
/// 返回值
/// The prepared browser snapshot after successful publication, or the original validation or publication failure.
/// 返回成功发布后的预备浏览器快照，或原校验、发布失败。
pub(crate) async fn service_save_public_ai_usage_refresh_settings(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiUsageRefreshSettingsRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    if !(30..=86_400).contains(&request.interval_seconds) {
        return Err(String::from(
            "Account usage refresh interval must be between 30 and 86400 seconds.",
        ));
    }
    // transaction keeps the settings writer serialized through Workbench publication.
    // transaction 让设置写入方在工作台发布完成前保持串行化。
    let transaction = begin_public_ai_configuration_transaction().await;
    // currentConfiguration is the exact paired before-image for compensation.
    // currentConfiguration 是用于补偿的精确配对前镜像。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // snapshot is prepared before the atomic browser-visible state swap.
    // snapshot 在浏览器可见状态原子交换前完成预备。
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存账号用量刷新间隔失败",
        |configuration| {
            configuration.usage_refresh_interval_seconds = request.interval_seconds;
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves one provider priority after validating its current registered identity.
/// 校验当前已注册身份后保存一个供应商优先级。
///
/// # Parameters
/// 参数
/// `app` publishes service events, `state` owns the runtime, and `request` carries the exact settings change.
/// `app` 发布服务事件，`state` 持有运行时，`request` 携带本次精确设置变更。
///
/// # Returns
/// 返回值
/// The prepared browser snapshot after successful publication, or the original validation or publication failure.
/// 返回成功发布后的预备浏览器快照，或原校验、发布失败。
pub(crate) async fn service_save_public_ai_provider_priority(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiProviderPrioritySaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    // transaction keeps validation, persistence, and publication on one serialized generation.
    // transaction 让校验、持久化和发布保持在同一个串行代际。
    let transaction = begin_public_ai_configuration_transaction().await;
    // currentConfiguration is the exact paired before-image for compensation.
    // currentConfiguration 是用于补偿的精确配对前镜像。
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    if runtime
        .providers()?
        .provider(request.provider_id.as_str())
        .is_none()
    {
        return Err(format!(
            "Provider `{}` is not registered in the current catalog.",
            request.provider_id
        ));
    }
    // snapshot is prepared before the atomic browser-visible state swap.
    // snapshot 在浏览器可见状态原子交换前完成预备。
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存供应商优先级失败",
        |configuration| {
            if request.priority == 0 {
                configuration
                    .provider_priority_overrides
                    .remove(request.provider_id.as_str());
            } else {
                configuration
                    .provider_priority_overrides
                    .insert(request.provider_id.clone(), request.priority);
            }
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves the process-wide default Codex context-window policy.
/// 保存进程级默认 Codex 上下文窗口策略。
///
/// # Parameters
/// 参数
/// `app` publishes service events, `state` owns the runtime, and `request` carries the exact settings change.
/// `app` 发布服务事件，`state` 持有运行时，`request` 携带本次精确设置变更。
///
/// # Returns
/// 返回值
/// The prepared browser snapshot after successful publication, or the original validation or publication failure.
/// 返回成功发布后的预备浏览器快照，或原校验、发布失败。
pub(crate) async fn service_save_public_ai_context_window_policy_overrides(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiContextWindowPolicyOverridesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    request.overrides.validate()?;
    // transaction retains ownership through the paired runtime and Workbench publication.
    // transaction 在配对运行时与工作台发布全程保留所有权。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let overrides = request.overrides;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "保存系统级上下文窗口策略失败",
        |configuration| configuration.set_context_window_policy_overrides(overrides),
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Saves the complete process-wide context-compaction window node configuration.
/// 保存完整的进程级上下文压缩窗口节点配置。
///
/// # Parameters
/// 参数
/// `app` publishes service events, `state` owns the runtime, and `request` carries the exact settings change.
/// `app` 发布服务事件，`state` 持有运行时，`request` 携带本次精确设置变更。
///
/// # Returns
/// 返回值
/// The prepared browser snapshot after successful publication, or the original validation or publication failure.
/// 返回成功发布后的预备浏览器快照，或原校验、发布失败。
pub(crate) async fn service_save_public_ai_context_compaction_window_nodes(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiContextCompactionWindowNodesSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    request.nodes.validate()?;
    // transaction retains ownership through persistence and Workbench publication.
    // transaction 在持久化与 Workbench 发布全程保留所有权。
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let nodes = request.nodes;
    // nodesChanged prevents an idempotent save from invalidating otherwise reusable budgets.
    // nodesChanged 防止幂等保存使原本可复用的预算失效。
    let nodes_changed = nodes != current_configuration.context_compaction_window_nodes;
    let (_configuration, snapshot) =
        mutate_public_ai_configuration_and_publish_using_in_transaction(
            &state,
            runtime.as_ref(),
            &current_configuration,
            "保存上下文压缩窗口节点失败",
            |configuration| configuration.set_context_compaction_window_nodes(nodes),
            |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
            |configuration| {
                publish_public_ai_workbench_after_context_compaction_window_nodes_change(
                    &app,
                    &state,
                    runtime.as_ref(),
                    configuration,
                    nodes_changed,
                )
                .map(|_| ())
            },
        )?;
    drop(transaction);
    Ok(snapshot)
}
