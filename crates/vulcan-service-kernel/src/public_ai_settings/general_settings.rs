//! Shared public AI defaults and system-wide settings mutations.
//! 公共 AI 共享默认值与系统级设置写入。

use serde::Deserialize;
use std::collections::BTreeSet;
use vulcan_context::{ContextCompactionWindowNodeId, ContextWindowPolicyOverrides};
use vulcan_provider_adapters::{
    ContextCompactionWindowConfiguration, MAX_USAGE_REFRESH_INTERVAL_SECONDS,
    MIN_USAGE_REFRESH_INTERVAL_SECONDS, ModelContextWindowDefault,
};
use vulcan_service_api::ServiceEventEmitter;
use vulcan_service_api::StartupModelReconciliationNotice;

use super::publication::{
    mutate_public_ai_configuration_and_publish_in_transaction,
    mutate_public_ai_configuration_and_publish_using_in_transaction,
    publish_public_ai_workbench_after_context_compaction_window_nodes_change,
};
use super::snapshot::enabled_conversation_model_windows;
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

/// Saves the default planning window for conversations created in the future.
/// 保存以后新建会话的默认规划窗口。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiNewSessionContextWindowDefaultSaveRequest {
    /// A fixed node identity, or None to follow the selected model's window.
    /// 固定节点身份，或空值表示遵循所选模型窗口。
    node_id: Option<ContextCompactionWindowNodeId>,
}

/// Saves or clears one model's new-session window default.
/// 保存或清除一个模型的新会话窗口默认值。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiModelContextWindowDefaultSaveRequest {
    /// Provider that owns the model.
    /// 拥有该模型的供应商。
    provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    model_id: String,
    /// The model's default, or None so the model inherits the system new-session default.
    /// 模型默认值；为空时模型继承系统新会话默认值。
    default: Option<ModelContextWindowDefault>,
}

/// Acknowledges signed model takeovers after their startup notice is closed.
/// 在启动接管提示关闭后确认对应的签名模型接管。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiModelTakeoverNoticeAcknowledgeRequest {
    /// Exact notices displayed by the user interface.
    /// 用户界面实际展示的精确提示集合。
    notices: Vec<StartupModelReconciliationNotice>,
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
    if !(MIN_USAGE_REFRESH_INTERVAL_SECONDS..=MAX_USAGE_REFRESH_INTERVAL_SECONDS)
        .contains(&request.interval_seconds)
    {
        return Err(format!(
            "Account usage refresh interval must be between {MIN_USAGE_REFRESH_INTERVAL_SECONDS} and {MAX_USAGE_REFRESH_INTERVAL_SECONDS} seconds."
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
        "Failed to save the account usage refresh interval",
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
        "Failed to save the provider priority",
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
        "Failed to save the system-level context window policy",
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
            "Failed to save the context compaction window nodes",
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

/// Persists a new-session default without changing any existing session metadata.
/// 持久化新建会话默认值，不改变任何已有会话元数据。
///
/// `app` publishes the settings snapshot, `state` owns configuration, and `request` selects a node.
/// `app` 发布设置快照，`state` 持有配置，`request` 选择节点。
pub(crate) async fn service_save_public_ai_new_session_context_window_default(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiNewSessionContextWindowDefaultSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the new session context window default",
        |configuration| {
            configuration.set_new_session_context_compaction_window_node_id(request.node_id);
            Ok(())
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Persists one model's new-session window default after checking it against the model's window.
/// 按模型窗口校验后持久化一个模型的新会话窗口默认值。
///
/// Only an enabled conversation model can receive a default; existing sessions never change.
/// 只有已启用的会话模型才能设置默认值；已有会话不受影响。
pub(crate) async fn service_save_public_ai_model_context_window_default(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiModelContextWindowDefaultSaveRequest,
) -> Result<PublicAiSettingsSnapshot, String> {
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    // Removing a default needs no model lookup, so a stale entry of a disabled model can still be cleared.
    // 删除默认值无需查找模型，因此已停用模型的陈旧条目仍可清除。
    let model_context_limit = if request.default.is_some() {
        enabled_model_context_limit(
            runtime.as_ref(),
            &current_configuration,
            request.provider_id.as_str(),
            request.model_id.as_str(),
        )?
    } else {
        None
    };
    let (_configuration, snapshot) = mutate_public_ai_configuration_and_publish_in_transaction(
        &app,
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to save the model context window default",
        |configuration| {
            configuration.set_model_context_window_default(
                request.provider_id.as_str(),
                request.model_id.as_str(),
                model_context_limit,
                request.default,
            )
        },
        |configuration| build_public_ai_settings_snapshot(runtime.as_ref(), configuration),
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Returns the declared context window of one enabled conversation model.
/// 返回一个已启用会话模型声明的上下文窗口。
///
/// # Returns
/// 返回值
/// `None` when no enabled account declares a window; an error when no enabled account offers the model.
/// 没有已启用账号声明窗口时返回 `None`；没有已启用账号提供该模型时返回错误。
fn enabled_model_context_limit(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
) -> Result<Option<u64>, String> {
    let catalogs = runtime.catalog_snapshot()?;
    enabled_conversation_model_windows(runtime, configuration, &catalogs)?
        .into_iter()
        .find(|candidate| candidate.provider_id == provider_id && candidate.model_id == model_id)
        .map(|candidate| candidate.context_limit)
        .ok_or_else(|| {
            format!(
                "Provider `{provider_id}` model `{model_id}` is not an enabled conversation model."
            )
        })
}

/// Clears only displayed takeover notices after persisting the acknowledgement.
/// 持久化确认后仅清除已展示的接管提示。
///
/// `state` owns the runtime and startup snapshot; `request` names the exact displayed notices.
/// `state` 持有运行时与启动快照；`request` 指定精确的已展示提示。
pub(crate) async fn service_ack_public_ai_model_takeover_notices(
    state: SharedServiceBridgeState,
    request: PublicAiModelTakeoverNoticeAcknowledgeRequest,
) -> Result<(), String> {
    let notices = request
        .notices
        .into_iter()
        .map(|notice| (notice.provider_id, notice.model_id))
        .collect::<BTreeSet<_>>();
    if notices.is_empty() {
        return Ok(());
    }
    let transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, current_configuration) = public_ai_runtime_and_configuration(&state)?;
    mutate_public_ai_configuration_and_publish_using_in_transaction(
        &state,
        runtime.as_ref(),
        &current_configuration,
        "Failed to acknowledge the system model takeover notices",
        |configuration| {
            configuration.acknowledge_system_model_takeover_notices(
                &notices.iter().cloned().collect::<Vec<_>>(),
            );
            Ok(())
        },
        |_| Ok(()),
        |_| Ok(()),
    )?;
    let mut guard = state.lock().map_err(|error| {
        format!("Failed to lock startup state after notice acknowledgement: {error}")
    })?;
    guard
        .startup
        .model_reconciliation_notices
        .retain(|notice| !notices.contains(&(notice.provider_id.clone(), notice.model_id.clone())));
    drop(guard);
    drop(transaction);
    Ok(())
}
