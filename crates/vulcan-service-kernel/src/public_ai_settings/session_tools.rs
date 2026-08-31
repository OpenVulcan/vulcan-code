//! Session provider-tool metadata, inheritance, conflict checks, and effective media bridges.
//! 会话供应商工具元数据、继承、冲突检查与生效媒体桥接。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::ResourceKind;
use vulcan_provider_adapters::{
    MediaUnderstandingBackendSelection, PersistedAiConfiguration, ProviderToolCapabilityKind,
    ProviderToolTargetSelection, PublicAiRuntime,
};
use vulcan_service_api::ServiceEventEmitter;
use vulcan_session_core::v1::config::{
    ProviderExtensionTargetSetConfig, ProviderMediaUnderstandingMode,
    ProviderMediaUnderstandingSelection, ProviderToolCapabilitySelection,
    ProviderToolSelectionConfig, ProviderToolTargetConfig,
};

use super::{
    media_candidates::{
        build_media_understanding_model_candidates, media_understanding_resource_kind,
    },
    provider_tools::{
        media_understanding_backend_is_available, validate_provider_extension_target_sets,
    },
    public_ai_runtime_and_configuration,
    publication::{
        PublicAiWorkspaceReconciliation,
        publish_public_ai_workbench_with_workspace_reconciliation_and_commit,
    },
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

/// Browser-facing conflict retained for one session provider tool capability.
/// 面向浏览器保留的一项会话供应商工具能力冲突。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiSessionProviderToolConflict {
    /// Exact semantic capability whose saved state is invalid.
    /// 已保存状态无效的精确语义能力。
    capability: ProviderToolCapabilityKind,
    /// Stable machine-readable conflict kind.
    /// 稳定的机器可读冲突类型。
    kind: String,
    /// Actionable user-visible diagnostic.
    /// 可操作的用户可见诊断。
    message: String,
}

/// Browser-facing complete provider tool snapshot owned by one session.
/// 面向浏览器且由单个会话持有的完整供应商工具快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiSessionProviderToolSettingsSnapshot {
    /// Capabilities enabled only for this session.
    /// 仅为当前会话启用的能力。
    enabled_capabilities: BTreeSet<ProviderToolCapabilityKind>,
    /// Exact model and account selected for every retained capability.
    /// 每项保留能力选择的精确模型与账号。
    selections: BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    /// Exact native model-extension target sets selected for this session.
    /// 当前会话选择的精确原生模型扩展目标集合。
    extension_target_sets:
        BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
    /// Session-selected backend mode for each enabled media-understanding capability.
    /// 当前会话为每项启用的媒体理解能力选择的后端模式。
    media_understanding_modes:
        BTreeMap<ProviderToolCapabilityKind, vulcan_workbench_model::SessionMediaUnderstandingMode>,
    /// Invalid retained choices that require explicit user confirmation.
    /// 需要用户显式确认的无效保留选择。
    conflicts: Vec<PublicAiSessionProviderToolConflict>,
}

/// Replaces the complete provider tool configuration for one session.
/// 替换单个会话完整的供应商工具配置。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiSessionProviderToolSelectionsSaveRequest {
    /// Conversation that owns the selection override.
    /// 拥有选择覆盖的会话。
    conversation_id: String,
    /// Semantic capabilities enabled only for this session.
    /// 仅为当前会话启用的语义能力。
    enabled_capabilities: BTreeSet<ProviderToolCapabilityKind>,
    /// Exact target retained independently from capability activation.
    /// 与能力启用状态独立保留的精确目标。
    selections: BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    /// Complete native model-extension target sets retained by this session.
    /// 当前会话保留的完整原生模型扩展目标集合。
    extension_target_sets:
        BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
    /// Session-selected provider or model-replacement mode per media capability.
    /// 当前会话为每项媒体能力选择的供应商专用或模型代替模式。
    #[serde(default)]
    media_understanding_modes:
        BTreeMap<ProviderToolCapabilityKind, vulcan_workbench_model::SessionMediaUnderstandingMode>,
}

/// Converts one exact public-AI target into the Provider-agnostic V1 metadata contract.
/// 把一个精确公共 AI 目标转换为与 Provider 无关的 V1 metadata 契约。
///
/// # Parameters
/// 参数
/// `selection` supplies one exact account and target identity.
/// `selection` 提供一个精确账号与目标身份。
///
/// # Returns
/// 返回值
/// Returns the same identity in the provider-independent V1 metadata shape.
/// 返回同一身份的供应商无关 V1 元数据形状。
fn provider_tool_target_config_v1(
    selection: &ProviderToolTargetSelection,
) -> ProviderToolTargetConfig {
    ProviderToolTargetConfig {
        connection_id: selection.connection_id.clone(),
        target_id: selection.target_id.clone(),
    }
}

/// Builds one canonical V1 Provider-tool selection from exact validated configuration parts.
/// 从精确且已校验的配置组成部分构建一份规范 V1 Provider 工具选择。
///
/// # Parameters
/// 参数
/// The collection parameters are the exact enabled capabilities, targets, media modes, and
/// extension target sets; `overridden` records whether the browser supplied an explicit session
/// override.
/// 各集合参数分别是精确的启用能力、目标、媒体模式与扩展目标集合；`overridden` 记录浏览器是否
/// 提供了显式会话覆盖。
///
/// # Returns
/// 返回值
/// Returns the strongly typed canonical metadata contract ordered by final serialized keys.
/// 返回按最终序列化键排序的强类型规范 metadata 契约。
pub(crate) fn provider_tool_selection_config_from_parts_v1(
    enabled_capabilities: &BTreeSet<ProviderToolCapabilityKind>,
    selections: &BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    media_understanding_modes: &BTreeMap<
        ProviderToolCapabilityKind,
        vulcan_workbench_model::SessionMediaUnderstandingMode,
    >,
    extension_target_sets: &BTreeMap<
        ProviderToolCapabilityKind,
        BTreeSet<ProviderToolTargetSelection>,
    >,
    overridden: bool,
) -> ProviderToolSelectionConfig {
    // config owns the final string-keyed persistence representation before Core validation.
    // config 在 Core 校验前持有最终以字符串为键的持久化表示。
    let mut config = ProviderToolSelectionConfig {
        initialized: true,
        overridden,
        enabled_capabilities: enabled_capabilities
            .iter()
            .map(|capability| provider_tool_capability_key(*capability))
            .collect(),
        selections: selections
            .iter()
            .map(|(capability, selection)| ProviderToolCapabilitySelection {
                capability: provider_tool_capability_key(*capability),
                target: provider_tool_target_config_v1(selection),
            })
            .collect(),
        media_understanding_modes: media_understanding_modes
            .iter()
            .map(|(capability, mode)| ProviderMediaUnderstandingSelection {
                capability: provider_tool_capability_key(*capability),
                mode: match mode {
                    vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool => {
                        ProviderMediaUnderstandingMode::ProviderTool
                    }
                    vulcan_workbench_model::SessionMediaUnderstandingMode::ModelReplacement => {
                        ProviderMediaUnderstandingMode::ModelReplacement
                    }
                },
            })
            .collect(),
        extension_target_sets: extension_target_sets
            .iter()
            .map(
                |(capability, selections)| ProviderExtensionTargetSetConfig {
                    capability: provider_tool_capability_key(*capability),
                    targets: selections
                        .iter()
                        .map(provider_tool_target_config_v1)
                        .collect(),
                },
            )
            .collect(),
    };
    // Enum Ord follows declaration order, while the V1 contract orders final serialized keys.
    // 枚举 Ord 遵循声明顺序，而 V1 契约按最终序列化键排序。
    config.enabled_capabilities.sort();
    config
        .selections
        .sort_by(|left, right| left.capability.cmp(&right.capability));
    config
        .media_understanding_modes
        .sort_by(|left, right| left.capability.cmp(&right.capability));
    for target_set in &mut config.extension_target_sets {
        target_set.targets.sort();
    }
    config
        .extension_target_sets
        .sort_by(|left, right| left.capability.cmp(&right.capability));
    config
}

/// Builds the complete canonical V1 Provider-tool selection from one validated browser snapshot.
/// 从一份已校验浏览器快照构建完整规范的 V1 Provider 工具选择。
///
/// # Parameters
/// 参数
/// `snapshot` contains validated capability choices; `overridden` preserves their inheritance authority.
/// `snapshot` 包含已校验能力选择；`overridden` 保留其继承权威状态。
///
/// # Returns
/// 返回值
/// Returns one canonical V1 configuration ordered by final serialized keys.
/// 返回按最终序列化键排序的一份规范 V1 配置。
fn provider_tool_selection_config_v1(
    snapshot: &PublicAiSessionProviderToolSettingsSnapshot,
    overridden: bool,
) -> ProviderToolSelectionConfig {
    provider_tool_selection_config_from_parts_v1(
        &snapshot.enabled_capabilities,
        &snapshot.selections,
        &snapshot.media_understanding_modes,
        &snapshot.extension_target_sets,
        overridden,
    )
}

/// Returns the effective direct, media-mode, and extension configuration for one session.
/// 返回单个会话生效的直连、媒体模式与扩展配置。
///
/// # Parameters
/// 参数
/// `app` publishes capability-only projection changes; `state` owns metadata access and global defaults; `conversation_id` identifies the session.
/// `app` 发布仅能力投影变化；`state` 持有元数据访问与全局默认值；`conversation_id` 标识会话。
///
/// # Returns
/// 返回值
/// Returns effective settings and conflicts without writing metadata or loading conversation history.
/// 返回生效设置及冲突，不写入元数据，也不加载会话历史。
pub(crate) async fn service_public_ai_session_provider_tool_selections(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<PublicAiSessionProviderToolSettingsSnapshot, String> {
    // The global transaction freezes defaults, while metadata remains the session authority.
    // 全局事务冻结默认值，metadata 始终是会话配置权威。
    let session_id = conversation_id
        .parse::<crate::SessionId>()
        .map_err(|error| format!("Invalid conversation id `{conversation_id}`: {error}"))?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let access = crate::session_request_config_v1::SessionProviderToolConfiguration::resolve(
        &state, session_id,
    )
    .await?;
    let persisted = access.read()?;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    let (snapshot, planned) =
        resolved_session_provider_tool_settings(runtime.as_ref(), &configuration, &persisted)?;
    // Compare only capability fields, leaving execution, goals and model selection untouched.
    // 仅比较能力字段，不触碰执行、目标及模型选择。
    let requires_publication = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        // A missing projection is normal after reopening; it is not missing configuration.
        // 重开后投影缺失属于正常情况，并不代表配置缺失。
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .is_some_and(|current| {
                current.provider_tool_selections != planned.provider_tool_selections
                    || current.provider_tool_enabled_capabilities
                        != planned.provider_tool_enabled_capabilities
                    || current.media_understanding_modes != planned.media_understanding_modes
                    || current.provider_extension_target_sets
                        != planned.provider_extension_target_sets
                    || current.provider_tool_settings_initialized
                        != planned.provider_tool_settings_initialized
                    || current.provider_tool_selections_overridden
                        != planned.provider_tool_selections_overridden
            })
    };
    if requires_publication {
        // Inheritance is an in-memory view. Merely opening settings must not rewrite user metadata.
        // 继承只是内存视图；仅打开设置绝不能回写用户元数据。
        let effective = provider_tool_selection_config_v1(
            &snapshot,
            planned.provider_tool_selections_overridden,
        );
        publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
            &app,
            &state,
            runtime.as_ref(),
            &configuration,
            |workspace| {
                Ok(project_session_provider_tool_settings(
                    workspace, session_id, &effective,
                ))
            },
            false,
            "provider_tool_settings_projection",
            || Ok(()),
        )?;
    }
    drop(transaction);
    Ok(snapshot)
}

/// Resolves `persisted` against global `configuration` and `runtime` without execution state.
/// 不依赖执行状态，从全局 `configuration` 与 `runtime` 解析 `persisted`。
/// Returns the API snapshot and its capability-only projection, including inherited defaults.
/// 返回接口快照及仅能力投影，包含继承的默认值。
fn resolved_session_provider_tool_settings(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    persisted: &vulcan_session_core::v1::config::ProviderToolSelectionConfig,
) -> Result<
    (
        PublicAiSessionProviderToolSettingsSnapshot,
        vulcan_workbench_model::SessionRuntimeStateEntry,
    ),
    String,
> {
    let mut planned = vulcan_workbench_model::SessionRuntimeStateEntry::default();
    crate::session_request_config_v1::apply_provider_tool_selection_v1(&mut planned, persisted);
    let (enabled_capabilities, selections, media_understanding_modes, extension_target_sets, _) =
        reconcile_session_provider_tool_settings(&mut planned, configuration)?;
    let conflicts = provider_tool_session_conflicts(
        runtime,
        configuration,
        &enabled_capabilities,
        &selections,
        &media_understanding_modes,
        &extension_target_sets,
    )?;
    Ok((
        PublicAiSessionProviderToolSettingsSnapshot {
            enabled_capabilities,
            selections,
            media_understanding_modes,
            extension_target_sets,
            conflicts,
        },
        planned,
    ))
}

/// Updates only an existing capability projection in `workspace` for `session_id` using `selection`.
/// 使用 `selection` 仅更新 `workspace` 中 `session_id` 已存在的能力投影。
/// Returns changed projection IDs; an unloaded session needs no adapter and no execution hydration.
/// 返回变化的投影 ID；未加载会话不需要适配器，也不需要加载执行数据。
fn project_session_provider_tool_settings(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    session_id: crate::SessionId,
    selection: &vulcan_session_core::v1::config::ProviderToolSelectionConfig,
) -> PublicAiWorkspaceReconciliation {
    let Some(current) = workspace.session_runtime_state.get_mut(&session_id) else {
        return PublicAiWorkspaceReconciliation::default();
    };
    crate::session_request_config_v1::apply_provider_tool_selection_v1(current, selection);
    PublicAiWorkspaceReconciliation::workspace_only(BTreeSet::from([session_id]))
}

/// Rebuilds only `conversation_id` capability settings to inherited global defaults.
/// 仅将 `conversation_id` 的能力设置重建为继承全局默认值。
/// Returns the effective settings; conversation history, goals and other metadata stay untouched.
/// 返回生效设置；会话历史、目标及其他元数据保持不变。
///
/// # Parameters
/// 参数
/// `app` publishes capability changes; `state` owns metadata access/global defaults; `conversation_id` identifies the independent rebuild target.
/// `app` 发布能力变化；`state` 持有元数据访问与全局默认值；`conversation_id` 标识独立重建目标。
///
/// # Returns
/// 返回值
/// Returns inherited settings after an explicit metadata rebuild, without reading corrupt old addon data or rewriting history.
/// 显式重建元数据后返回继承设置，不先读取损坏旧附加项，也不改写历史。
pub(crate) async fn service_rebuild_public_ai_session_provider_tool_selections(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<PublicAiSessionProviderToolSettingsSnapshot, String> {
    let session_id = conversation_id
        .parse::<crate::SessionId>()
        .map_err(|error| format!("Invalid conversation id `{conversation_id}`: {error}"))?;
    let transaction = begin_public_ai_configuration_transaction().await;
    let access = crate::session_request_config_v1::SessionProviderToolConfiguration::resolve(
        &state, session_id,
    )
    .await?;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    // Explicit rebuild replaces the addon only; reading the corrupt old addon would defeat recovery.
    // 显式重建仅替换附加项；先读取损坏的旧附加项会让恢复无法执行。
    let (snapshot, _) = resolved_session_provider_tool_settings(
        runtime.as_ref(),
        &configuration,
        &Default::default(),
    )?;
    let selection = provider_tool_selection_config_v1(&snapshot, false);
    let now_ms = crate::session_goal_lifecycle::current_v1_goal_unix_millis()?;
    publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
        &app,
        &state,
        runtime.as_ref(),
        &configuration,
        |workspace| {
            Ok(project_session_provider_tool_settings(
                workspace, session_id, &selection,
            ))
        },
        false,
        "provider_tool_settings_rebuild",
        || {
            access.rebuild(selection.clone(), now_ms)?;
            Ok(())
        },
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Normalizes one session capability projection against its override state and current global defaults.
/// 根据覆盖状态与当前全局默认值规范化一个会话能力投影。
///
/// # Parameters
/// 参数
/// `runtime_state` is the detached capability projection; `configuration` owns the current
/// default selections and enabled capability set.
/// `runtime_state` 是独立能力投影；`configuration` 持有当前默认选择与已启用能力集合。
///
/// # Returns
/// 返回值
/// The normalized direct capabilities, direct selections, media modes, extension target sets, and whether projection fields changed.
/// 返回规范化的直连能力、直连选择、媒体模式、扩展目标集合，以及投影字段是否发生变化。
fn reconcile_session_provider_tool_settings(
    runtime_state: &mut vulcan_workbench_model::SessionRuntimeStateEntry,
    configuration: &PersistedAiConfiguration,
) -> Result<
    (
        BTreeSet<ProviderToolCapabilityKind>,
        BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
        BTreeMap<ProviderToolCapabilityKind, vulcan_workbench_model::SessionMediaUnderstandingMode>,
        BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>,
        bool,
    ),
    String,
> {
    // stateChanged reports projection changes; callers independently decide whether a write is authorized.
    // stateChanged 报告投影变化；由调用方独立决定是否获得写入授权。
    let mut state_changed = false;
    // mediaCapabilityKeys identifies legacy direct targets that must never remain in session state.
    // mediaCapabilityKeys 标识绝不能继续保留在会话状态中的历史媒体直连目标。
    let media_capability_keys = [
        ProviderToolCapabilityKind::ImageUnderstand,
        ProviderToolCapabilityKind::VideoUnderstand,
        ProviderToolCapabilityKind::AudioUnderstand,
    ]
    .into_iter()
    .map(provider_tool_capability_key)
    .collect::<BTreeSet<_>>();
    if runtime_state.provider_tool_settings_initialized {
        if !runtime_state.provider_tool_selections_overridden {
            // nextSelections mirrors the exact current global target defaults.
            // nextSelections 镜像精确的当前全局目标默认值。
            let next_selections = configuration
                .provider_tool_selections
                .iter()
                .filter(|(capability, _)| media_understanding_resource_kind(**capability).is_none())
                .map(|(capability, selection)| {
                    (
                        provider_tool_capability_key(*capability),
                        vulcan_workbench_model::SessionProviderToolTargetSelection {
                            connection_id: selection.connection_id.clone(),
                            target_id: selection.target_id.clone(),
                        },
                    )
                })
                .collect();
            // nextEnabledCapabilities mirrors the exact current global enabled defaults.
            // nextEnabledCapabilities 镜像精确的当前全局启用默认值。
            let next_enabled_capabilities = configuration
                .provider_tool_enabled_capabilities
                .iter()
                .map(|capability| provider_tool_capability_key(*capability))
                .collect();
            // nextMediaUnderstandingModes mirrors the active global media backend family while
            // keeping the session field independent from exact model and account selections.
            // nextMediaUnderstandingModes 镜像活动全局媒体后端类型，同时让会话字段独立于精确模型与账号选择。
            let next_media_understanding_modes = session_media_understanding_modes_to_runtime(
                &session_media_understanding_modes_from_configuration(configuration),
            );
            // nextExtensionTargetSets mirrors the exact current global extension defaults.
            // nextExtensionTargetSets 镜像精确的当前全局扩展目标默认集合。
            let next_extension_target_sets = configuration
                .provider_extension_target_sets
                .iter()
                .map(|(capability, selections)| {
                    (
                        provider_tool_capability_key(*capability),
                        selections
                            .iter()
                            .map(session_provider_tool_selection_from_contract)
                            .collect(),
                    )
                })
                .collect();
            if runtime_state.provider_tool_selections != next_selections
                || runtime_state.provider_tool_enabled_capabilities != next_enabled_capabilities
                || runtime_state.media_understanding_modes != next_media_understanding_modes
                || runtime_state.provider_extension_target_sets != next_extension_target_sets
            {
                runtime_state.provider_tool_selections = next_selections;
                runtime_state.provider_tool_enabled_capabilities = next_enabled_capabilities;
                runtime_state.media_understanding_modes = next_media_understanding_modes;
                runtime_state.provider_extension_target_sets = next_extension_target_sets;
                state_changed = true;
            }
        }
    } else {
        // Schema 4 intentionally discards pre-extension Provider Tool state and rematerializes
        // the current defaults instead of guessing an old-to-new semantic conversion.
        // Schema 4 有意丢弃模型扩展前的 Provider Tool 状态，并重新物化当前默认值，
        // 而不是猜测旧语义到新语义的转换。
        runtime_state.provider_tool_selections = configuration
            .provider_tool_selections
            .iter()
            .filter(|(capability, _)| media_understanding_resource_kind(**capability).is_none())
            .map(|(capability, selection)| {
                (
                    provider_tool_capability_key(*capability),
                    session_provider_tool_selection_from_contract(selection),
                )
            })
            .collect();
        runtime_state.provider_tool_enabled_capabilities = configuration
            .provider_tool_enabled_capabilities
            .iter()
            .map(|capability| provider_tool_capability_key(*capability))
            .collect();
        runtime_state.media_understanding_modes = session_media_understanding_modes_to_runtime(
            &session_media_understanding_modes_from_configuration(configuration),
        );
        runtime_state.provider_extension_target_sets = configuration
            .provider_extension_target_sets
            .iter()
            .map(|(capability, selections)| {
                (
                    provider_tool_capability_key(*capability),
                    selections
                        .iter()
                        .map(session_provider_tool_selection_from_contract)
                        .collect(),
                )
            })
            .collect();
        runtime_state.provider_tool_selections_overridden = false;
        runtime_state.provider_tool_settings_initialized = true;
        state_changed = true;
    }
    // cleanedSelections removes legacy media targets even when the session has explicit overrides.
    // cleanedSelections 即使会话存在显式覆盖，也会移除历史媒体目标。
    let cleaned_selections = runtime_state
        .provider_tool_selections
        .iter()
        .filter(|(capability, _)| !media_capability_keys.contains(*capability))
        .map(|(capability, selection)| (capability.clone(), selection.clone()))
        .collect::<BTreeMap<_, _>>();
    if cleaned_selections != runtime_state.provider_tool_selections {
        runtime_state.provider_tool_selections = cleaned_selections;
        state_changed = true;
    }
    // cleanedMediaModes removes malformed legacy capability keys and fills only missing media
    // defaults for an explicit session override created before this field existed.
    // cleanedMediaModes 清理旧的非法能力键，并仅为新增字段之前创建的显式会话补齐缺失媒体默认值。
    let mut cleaned_media_modes = runtime_state
        .media_understanding_modes
        .iter()
        .filter(|(capability, _)| media_capability_keys.contains(*capability))
        .map(|(capability, mode)| (capability.clone(), *mode))
        .collect::<BTreeMap<_, _>>();
    if runtime_state.provider_tool_selections_overridden {
        for (capability, mode) in
            session_media_understanding_modes_from_configuration(configuration)
        {
            cleaned_media_modes
                .entry(provider_tool_capability_key(capability))
                .or_insert(mode);
        }
    }
    if cleaned_media_modes != runtime_state.media_understanding_modes {
        runtime_state.media_understanding_modes = cleaned_media_modes;
        state_changed = true;
    }
    if state_changed {
        runtime_state.updated_at = crate::current_unix_millis_text();
    }
    let enabled_capabilities =
        session_provider_tool_enabled_capabilities_from_runtime(runtime_state)?;
    let selections = session_provider_tool_selections_from_runtime(runtime_state)?;
    let media_understanding_modes = session_media_understanding_modes_from_runtime(runtime_state)?;
    let extension_target_sets = session_provider_extension_target_sets_from_runtime(runtime_state)?;
    Ok((
        enabled_capabilities,
        selections,
        media_understanding_modes,
        extension_target_sets,
        state_changed,
    ))
}

/// Saves the complete direct, media-mode, and extension configuration for one session.
/// 保存单个会话完整的直连、媒体模式与扩展配置。
///
/// # Parameters
/// 参数
/// `app` publishes capability changes; `state` owns metadata access/global defaults; `request` supplies the complete explicit session override.
/// `app` 发布能力变化；`state` 持有元数据访问与全局默认值；`request` 提供完整显式会话覆盖。
///
/// # Returns
/// 返回值
/// Returns validated settings after the V1 metadata commit, or an explicit conflict/persistence failure.
/// V1 元数据提交后返回已校验设置，或返回显式冲突/持久化失败。
pub(crate) async fn service_save_public_ai_session_provider_tool_selections(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PublicAiSessionProviderToolSelectionsSaveRequest,
) -> Result<PublicAiSessionProviderToolSettingsSnapshot, String> {
    // normalizedRequest preserves only the session enable switch and backend mode for media capabilities.
    // normalizedRequest 对媒体能力只保留会话启用开关与后端模式。
    let mut request = request;
    request
        .selections
        .retain(|capability, _| media_understanding_resource_kind(*capability).is_none());
    request
        .media_understanding_modes
        .retain(|capability, _| media_understanding_resource_kind(*capability).is_some());
    let session_id = request
        .conversation_id
        .parse::<crate::SessionId>()
        .map_err(|error| {
            format!(
                "Invalid conversation id `{}`: {error}",
                request.conversation_id
            )
        })?;
    // transaction keeps target validation and detached workspace publication on one generation.
    // transaction 让目标校验与独立工作区发布保持在同一代际。
    let transaction = begin_public_ai_configuration_transaction().await;
    // Resolve only metadata ownership, not the execution document or its Turn history.
    // 仅解析元数据所有权，不解析执行文档及其回合历史。
    let access = crate::session_request_config_v1::SessionProviderToolConfiguration::resolve(
        &state, session_id,
    )
    .await?;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    validate_provider_tool_session_settings(
        runtime.as_ref(),
        &configuration,
        &request.enabled_capabilities,
        &request.selections,
        &request.media_understanding_modes,
        &request.extension_target_sets,
    )?;
    // snapshot is complete before any browser-visible or authoritative workspace assignment.
    // snapshot 在任何浏览器可见或权威工作区赋值前保持完整。
    let snapshot = PublicAiSessionProviderToolSettingsSnapshot {
        enabled_capabilities: request.enabled_capabilities,
        selections: request.selections,
        media_understanding_modes: request.media_understanding_modes,
        extension_target_sets: request.extension_target_sets,
        conflicts: Vec::new(),
    };
    let selection = provider_tool_selection_config_v1(&snapshot, true);
    let provider_tools_updated_at_ms =
        crate::session_goal_lifecycle::current_v1_goal_unix_millis()?;
    publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
        &app,
        &state,
        runtime.as_ref(),
        &configuration,
        |workspace| {
            Ok(project_session_provider_tool_settings(
                workspace, session_id, &selection,
            ))
        },
        false,
        "provider_tool_settings",
        || {
            access.save(selection.clone(), provider_tools_updated_at_ms)?;
            Ok(())
        },
    )?;
    drop(transaction);
    Ok(snapshot)
}

/// Validates one complete session provider tool snapshot before persistence or dispatch.
/// 在持久化或分派前校验一份完整会话供应商工具快照。
///
/// # Parameters
/// 参数
/// `runtime` and `configuration` fix the target generation; the collections supply enabled capabilities, direct targets, media modes, and extension sets.
/// `runtime` 与 `configuration` 固定目标代际；各集合提供启用能力、直连目标、媒体模式及扩展集合。
///
/// # Returns
/// 返回值
/// Returns success for a conflict-free configuration, otherwise one explicit combined conflict error.
/// 配置无冲突时返回成功，否则返回显式合并冲突错误。
pub(crate) fn validate_provider_tool_session_settings(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    enabled_capabilities: &BTreeSet<ProviderToolCapabilityKind>,
    selections: &BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    media_understanding_modes: &BTreeMap<
        ProviderToolCapabilityKind,
        vulcan_workbench_model::SessionMediaUnderstandingMode,
    >,
    extension_target_sets: &BTreeMap<
        ProviderToolCapabilityKind,
        BTreeSet<ProviderToolTargetSelection>,
    >,
) -> Result<(), String> {
    let conflicts = provider_tool_session_conflicts(
        runtime,
        configuration,
        enabled_capabilities,
        selections,
        media_understanding_modes,
        extension_target_sets,
    )?;
    if conflicts.is_empty() {
        return Ok(());
    }
    Err(format!(
        "Provider tool session settings require confirmation: {}",
        conflicts
            .iter()
            .map(|conflict| conflict.message.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    ))
}

/// Finds every unavailable or incomplete provider tool choice retained by one session.
/// 查找单个会话保留的全部不可用或不完整供应商工具选择。
///
/// # Parameters
/// 参数
/// `runtime` and `configuration` fix target/backend facts; the collections describe the session's complete retained capability choices.
/// `runtime` 与 `configuration` 固定目标/后端事实；各集合描述会话完整保留的能力选择。
///
/// # Returns
/// 返回值
/// Returns exact retained conflicts, or an inventory-resolution failure without changing global persistence.
/// 返回精确保留冲突，或目录解析失败，不修改全局持久化。
fn provider_tool_session_conflicts(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    enabled_capabilities: &BTreeSet<ProviderToolCapabilityKind>,
    selections: &BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>,
    media_understanding_modes: &BTreeMap<
        ProviderToolCapabilityKind,
        vulcan_workbench_model::SessionMediaUnderstandingMode,
    >,
    extension_target_sets: &BTreeMap<
        ProviderToolCapabilityKind,
        BTreeSet<ProviderToolTargetSelection>,
    >,
) -> Result<Vec<PublicAiSessionProviderToolConflict>, String> {
    // effectiveConfiguration applies session-selected media modes without mutating global persistence.
    // effectiveConfiguration 应用会话选定的媒体模式，但不修改全局持久化配置。
    let mut effective_configuration = configuration.clone();
    let mut media_mode_errors = BTreeMap::new();
    for (capability, mode) in media_understanding_modes {
        let backend_mode = match mode {
            vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool => {
                vulcan_provider_adapters::MediaUnderstandingBackendMode::ProviderTool
            }
            vulcan_workbench_model::SessionMediaUnderstandingMode::ModelReplacement => {
                vulcan_provider_adapters::MediaUnderstandingBackendMode::ModelReplacement
            }
        };
        if let Err(error) =
            effective_configuration.apply_media_understanding_mode(*capability, backend_mode)
        {
            media_mode_errors.insert(*capability, error);
        }
    }
    // settingsCatalog supplies the exact conversation routes needed to validate session-selected media backends.
    // settingsCatalog 提供校验会话选定媒体后端所需的精确会话路由。
    let settings_catalog = runtime.settings_catalog_snapshot(&effective_configuration)?;
    // available belongs to the same immutable catalog generation as every media model candidate.
    // available 与全部媒体模型候选归属于同一份不可变目录代际。
    let available = &settings_catalog.available_provider_tool_targets;
    // mediaCapabilitiesEnabled avoids deriving an unrelated model inventory for sessions that use only search or fetch.
    // mediaCapabilitiesEnabled 避免为仅使用搜索或抓取的会话派生无关模型目录。
    let media_capabilities_enabled = enabled_capabilities
        .iter()
        .copied()
        .any(|capability| media_understanding_resource_kind(capability).is_some());
    // mediaModelCandidates applies the same structured-output and media-input route projection as settings.
    // mediaModelCandidates 应用与设置页相同的结构化输出及媒体输入路由投影。
    let media_model_candidates = if media_capabilities_enabled {
        build_media_understanding_model_candidates(
            &settings_catalog.runtime_configuration,
            &settings_catalog.providers,
            &settings_catalog.catalogs,
        )?
    } else {
        Vec::new()
    };
    let capabilities = enabled_capabilities.clone();
    let mut conflicts = Vec::new();
    for capability in capabilities {
        if !capability.is_direct_selection() {
            conflicts.push(PublicAiSessionProviderToolConflict {
                capability,
                kind: String::from("invalid_direct_capability"),
                message: format!(
                    "Capability `{:?}` must be configured as a model-extension target set.",
                    capability
                ),
            });
            continue;
        }
        if media_understanding_resource_kind(capability).is_some() {
            let Some(mode) = media_understanding_modes.get(&capability) else {
                conflicts.push(PublicAiSessionProviderToolConflict {
                    capability,
                    kind: String::from("missing_selection"),
                    message: format!(
                        "Media-understanding capability `{:?}` is enabled but has no selected backend mode.",
                        capability
                    ),
                });
                continue;
            };
            if let Some(error) = media_mode_errors.get(&capability) {
                conflicts.push(PublicAiSessionProviderToolConflict {
                    capability,
                    kind: String::from("target_unavailable"),
                    message: error.clone(),
                });
            } else if !media_understanding_backend_is_available(
                &settings_catalog.runtime_configuration,
                capability,
                available,
                &media_model_candidates,
            ) {
                conflicts.push(PublicAiSessionProviderToolConflict {
                    capability,
                    kind: String::from("target_unavailable"),
                    message: format!(
                        "The `{mode:?}` backend for media-understanding capability `{:?}` is no longer available; configure it again before sending.",
                        capability,
                    ),
                });
            }
            continue;
        }
        let Some(selection) = selections.get(&capability) else {
            conflicts.push(PublicAiSessionProviderToolConflict {
                capability,
                kind: String::from("missing_selection"),
                message: format!(
                    "Capability `{:?}` is enabled but has no selected model and account.",
                    capability
                ),
            });
            continue;
        };
        if !available.iter().any(|candidate| {
            candidate.connection_id == selection.connection_id
                && candidate.target.target_id == selection.target_id
                && candidate.target.contract.capability == capability
        }) {
            conflicts.push(PublicAiSessionProviderToolConflict {
                capability,
                kind: String::from("target_unavailable"),
                message: format!(
                    "The saved model target `{}` or account `{}` for capability `{:?}` is no longer available; select them again before sending.",
                    selection.target_id, selection.connection_id, capability
                ),
            });
        }
    }
    if let Err(error) =
        validate_provider_extension_target_sets(runtime, configuration, extension_target_sets)
    {
        conflicts.push(PublicAiSessionProviderToolConflict {
            capability: extension_target_sets
                .keys()
                .next()
                .copied()
                .unwrap_or(ProviderToolCapabilityKind::ImageGenerate),
            kind: String::from("invalid_extension_target_set"),
            message: error,
        });
    }
    Ok(conflicts)
}

/// Converts the active global media selections into session backend modes.
/// 将活动的全局媒体选择转换为会话后端模式。
///
/// # Parameters
/// 参数
/// `configuration` owns the currently active global media backend selection for each capability.
/// `configuration` 持有每项能力当前活动的全局媒体后端选择。
///
/// # Returns
/// 返回值
/// A closed map containing only image, video, and audio understanding modes.
/// 仅包含图片、视频与音频理解模式的封闭映射。
pub(crate) fn session_media_understanding_modes_from_configuration(
    configuration: &PersistedAiConfiguration,
) -> BTreeMap<ProviderToolCapabilityKind, vulcan_workbench_model::SessionMediaUnderstandingMode> {
    configuration
        .media_understanding_selections
        .iter()
        .filter_map(|(capability, selection)| {
            let mode = match selection {
                MediaUnderstandingBackendSelection::ProviderTool { .. } => {
                    vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool
                }
                MediaUnderstandingBackendSelection::ModelReplacement { .. } => {
                    vulcan_workbench_model::SessionMediaUnderstandingMode::ModelReplacement
                }
            };
            media_understanding_resource_kind(*capability).map(|_| (*capability, mode))
        })
        .collect()
}

/// Converts one session runtime mode map into the typed provider contract.
/// 将会话运行时模式映射转换为类型化供应商契约。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the persisted snake_case capability keys and closed mode values.
/// `runtime_state` 持有持久化的 snake_case 能力键与封闭模式值。
///
/// # Returns
/// 返回值
/// The typed media mode map, or an explicit invalid-key error.
/// 类型化媒体模式映射；键非法时返回明确错误。
fn session_media_understanding_modes_from_runtime(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
) -> Result<
    BTreeMap<ProviderToolCapabilityKind, vulcan_workbench_model::SessionMediaUnderstandingMode>,
    String,
> {
    runtime_state
        .media_understanding_modes
        .iter()
        .map(|(capability, mode)| {
            let capability = serde_json::from_value(serde_json::Value::String(capability.clone()))
                .map_err(|error| {
                    format!(
                        "Session media-understanding capability `{capability}` is invalid: {error}"
                    )
                })?;
            if media_understanding_resource_kind(capability).is_none() {
                return Err(format!(
                    "Session capability `{capability:?}` cannot use a media-understanding mode."
                ));
            }
            Ok((capability, *mode))
        })
        .collect()
}

/// Converts one typed media mode map into the session runtime persistence contract.
/// 将类型化媒体模式映射转换为会话运行时持久化契约。
///
/// # Parameters
/// 参数
/// `modes` supplies typed media capabilities and closed backend modes.
/// `modes` 提供类型化媒体能力与封闭后端模式。
///
/// # Returns
/// 返回值
/// Returns the same modes keyed by their canonical serialized capability names.
/// 返回按规范序列化能力名称建键的同一模式集合。
fn session_media_understanding_modes_to_runtime(
    modes: &BTreeMap<
        ProviderToolCapabilityKind,
        vulcan_workbench_model::SessionMediaUnderstandingMode,
    >,
) -> BTreeMap<String, vulcan_workbench_model::SessionMediaUnderstandingMode> {
    modes
        .iter()
        .map(|(capability, mode)| (provider_tool_capability_key(*capability), *mode))
        .collect()
}

/// Converts one provider-contract selection into the session persistence contract.
/// 把一个供应商契约选择转换为会话持久化契约。
///
/// # Parameters
/// 参数
/// `selection` contains one exact provider-contract account and target.
/// `selection` 包含一个供应商契约中的精确账号与目标。
///
/// # Returns
/// 返回值
/// Returns the same account/target tuple in the session projection shape.
/// 返回同一账号/目标二元组的会话投影形状。
fn session_provider_tool_selection_from_contract(
    selection: &ProviderToolTargetSelection,
) -> vulcan_workbench_model::SessionProviderToolTargetSelection {
    vulcan_workbench_model::SessionProviderToolTargetSelection {
        connection_id: selection.connection_id.clone(),
        target_id: selection.target_id.clone(),
    }
}

/// Converts one runtime-state selection map into the provider contract.
/// 将运行状态选择映射转换为供应商契约。
///
/// # Parameters
/// 参数
/// `runtime_state` owns canonical string-keyed direct target selections.
/// `runtime_state` 持有规范字符串键的直连目标选择。
///
/// # Returns
/// 返回值
/// Returns typed exact selections, or an explicit invalid capability-key error.
/// 返回类型化精确选择，或显式非法能力键错误。
fn session_provider_tool_selections_from_runtime(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
) -> Result<BTreeMap<ProviderToolCapabilityKind, ProviderToolTargetSelection>, String> {
    runtime_state
        .provider_tool_selections
        .iter()
        .map(|(capability, selection)| {
            Ok((
                serde_json::from_value(serde_json::Value::String(capability.clone())).map_err(
                    |error| {
                        format!(
                            "Session provider tool capability `{capability}` is invalid: {error}"
                        )
                    },
                )?,
                ProviderToolTargetSelection {
                    connection_id: selection.connection_id.clone(),
                    target_id: selection.target_id.clone(),
                },
            ))
        })
        .collect()
}

/// Converts session model-extension target sets into the provider contract.
/// 把会话模型扩展目标集合转换为供应商契约。
///
/// # Parameters
/// 参数
/// `runtime_state` owns canonical string-keyed native extension target sets.
/// `runtime_state` 持有规范字符串键的原生扩展目标集合。
///
/// # Returns
/// 返回值
/// Returns typed exact target sets, or an explicit invalid capability-key error.
/// 返回类型化精确目标集合，或显式非法能力键错误。
fn session_provider_extension_target_sets_from_runtime(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
) -> Result<BTreeMap<ProviderToolCapabilityKind, BTreeSet<ProviderToolTargetSelection>>, String> {
    runtime_state
        .provider_extension_target_sets
        .iter()
        .map(|(capability, selections)| {
            let capability = serde_json::from_value(serde_json::Value::String(capability.clone()))
                .map_err(|error| {
                    format!("Session model-extension capability `{capability}` is invalid: {error}")
                })?;
            let selections = selections
                .iter()
                .map(|selection| ProviderToolTargetSelection {
                    connection_id: selection.connection_id.clone(),
                    target_id: selection.target_id.clone(),
                })
                .collect();
            Ok((capability, selections))
        })
        .collect()
}

/// Converts one runtime-state capability set into the provider contract.
/// 将一个运行状态能力集合转换为供应商契约。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the serialized session capability set.
/// `runtime_state` 持有序列化的会话能力集合。
///
/// # Returns
/// 返回值
/// Returns closed typed capabilities, or an explicit invalid capability-key error.
/// 返回封闭类型化能力，或显式非法能力键错误。
fn session_provider_tool_enabled_capabilities_from_runtime(
    runtime_state: &vulcan_workbench_model::SessionRuntimeStateEntry,
) -> Result<BTreeSet<ProviderToolCapabilityKind>, String> {
    runtime_state
        .provider_tool_enabled_capabilities
        .iter()
        .map(|capability| {
            serde_json::from_value(serde_json::Value::String(capability.clone())).map_err(|error| {
                format!("Session provider tool capability `{capability}` is invalid: {error}")
            })
        })
        .collect()
}

/// Serializes one capability key exactly as the public JSON contract.
/// 按公共 JSON 契约精确序列化一项能力键。
///
/// # Parameters
/// 参数
/// `capability` is one closed semantic provider-tool capability.
/// `capability` 是一项封闭语义供应商工具能力。
///
/// # Returns
/// 返回值
/// Returns its canonical JSON string key; the declared enum must serialize as a string.
/// 返回规范 JSON 字符串键；已声明枚举必须序列化为字符串。
pub(crate) fn provider_tool_capability_key(capability: ProviderToolCapabilityKind) -> String {
    serde_json::to_value(capability)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .expect("provider tool capability must serialize as a string")
}

/// Returns every media resource backed by a valid understanding bridge for the effective session.
/// 返回当前会话生效配置中由有效理解桥接承载的全部媒体资源。
///
/// Inherited sessions read the current global defaults, while explicit session overrides remain isolated.
/// 继承全局设置的会话读取当前全局默认值，会话显式覆盖则保持隔离。
///
/// # Parameters
/// 参数
/// `runtime` resolves currently available provider-tool targets; `configuration` owns global selections; `runtime_state` is the selected session state.
/// `runtime` 解析当前可用的供应商工具目标；`configuration` 持有全局选择；`runtime_state` 是选中会话的运行态。
///
/// # Returns
/// 返回值
/// A closed set containing image, video, or audio only when the corresponding capability is enabled
/// and its exact provider-tool or model-replacement backend remains executable.
/// 返回封闭集合；仅当对应能力已启用且精确供应商工具或模型代替后端仍可执行时，
/// 集合才包含图片、视频或音频。
pub(crate) fn public_ai_session_media_understanding_bridge_kinds(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    runtime_state: Option<&vulcan_workbench_model::SessionRuntimeStateEntry>,
) -> Result<BTreeSet<ResourceKind>, String> {
    let explicit_session_settings = runtime_state.is_some_and(|state| {
        state.provider_tool_settings_initialized && state.provider_tool_selections_overridden
    });
    // enabledCapabilities preserves the session switch layer while media modes are applied to a
    // detached configuration copy for this bridge calculation.
    // enabledCapabilities 保留会话开关层，同时把媒体模式应用到独立配置副本供本次桥接计算使用。
    let enabled_capabilities = if explicit_session_settings {
        let runtime_state = runtime_state.expect("explicit session settings require runtime state");
        session_provider_tool_enabled_capabilities_from_runtime(runtime_state)?
    } else {
        configuration.provider_tool_enabled_capabilities.clone()
    };
    let mut effective_configuration = configuration.clone();
    if explicit_session_settings {
        let runtime_state = runtime_state.expect("explicit session settings require runtime state");
        for (capability, mode) in session_media_understanding_modes_from_runtime(runtime_state)? {
            let backend_mode = match mode {
                vulcan_workbench_model::SessionMediaUnderstandingMode::ProviderTool => {
                    vulcan_provider_adapters::MediaUnderstandingBackendMode::ProviderTool
                }
                vulcan_workbench_model::SessionMediaUnderstandingMode::ModelReplacement => {
                    vulcan_provider_adapters::MediaUnderstandingBackendMode::ModelReplacement
                }
            };
            effective_configuration.apply_media_understanding_mode(capability, backend_mode)?;
        }
    }
    let settings_catalog = runtime.settings_catalog_snapshot(&effective_configuration)?;
    // availableProviderTargets shares one immutable generation with modelCandidates.
    // availableProviderTargets 与 modelCandidates 共享同一份不可变目录代际。
    let available_provider_targets = &settings_catalog.available_provider_tool_targets;
    let model_candidates = build_media_understanding_model_candidates(
        &settings_catalog.runtime_configuration,
        &settings_catalog.providers,
        &settings_catalog.catalogs,
    )?;
    let mut bridged_media_kinds = BTreeSet::new();
    for capability in [
        ProviderToolCapabilityKind::ImageUnderstand,
        ProviderToolCapabilityKind::VideoUnderstand,
        ProviderToolCapabilityKind::AudioUnderstand,
    ] {
        if !enabled_capabilities.contains(&capability) {
            continue;
        }
        if media_understanding_backend_is_available(
            &settings_catalog.runtime_configuration,
            capability,
            available_provider_targets,
            &model_candidates,
        ) {
            bridged_media_kinds.insert(
                media_understanding_resource_kind(capability)
                    .expect("closed media-understanding capability must map to a resource"),
            );
        }
    }
    Ok(bridged_media_kinds)
}

#[cfg(test)]
mod tests;
