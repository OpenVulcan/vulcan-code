//! Transactional Vulcan Plugin enable and disable orchestration.
//! Vulcan 插件启用与停用事务编排。

use super::*;

mod target_publication;
pub(super) use target_publication::{
    TargetPluginExtensionPublication, publish_activation_target_plugin_extension_generation,
};
mod predecessor_publication;
pub(super) use predecessor_publication::{
    PredecessorPluginExtensionPublication,
    publish_activation_predecessor_plugin_extension_generation,
};

#[cfg(test)]
mod owner_drain_tests;

/// Temporary orchestration evidence preserves diagnostic origin without another persistent ledger.
/// 临时编排证据保留诊断来源，不建立另一个持久账本。
#[derive(Clone)]
enum CleanupEvidenceItem {
    /// Runtime has already classified and recorded this unchanged public message.
    /// 运行时已分类并登记此未变公开消息。
    RuntimeRecorded(String),
    /// External cleanup failed and must enter the runtime's unknown Failure authority.
    /// 外部清理失败，必须进入运行时未知 Failure 权威。
    AdditionalFailure(String),
}

/// One request-local ordered collection projects all messages but publishes only new external failures.
/// 单个请求局部有序集合投影全部消息，仅发布新的外部失败。
#[derive(Clone, Default)]
struct CleanupEvidence {
    /// Original messages retain their actual producing boundary across compensation.
    /// 原消息跨补偿保留实际生产边界。
    entries: Vec<CleanupEvidenceItem>,
}

impl CleanupEvidence {
    /// Preserve result's already-recorded lifecycle message; context prefixes only an accounting error.
    /// 保留 result 已登记生命周期消息；context 仅为记账错误加前缀。
    /// Return request-local evidence without reclassifying the runtime producer or dropping real failures.
    /// 返回请求局部证据，不重新分类运行时生产者或丢弃真实失败。
    fn from_recorded_lifecycle(
        result: Result<Option<String>, vulcan_plugin_runtime::VulcanPluginRuntimeError>,
        context: &str,
    ) -> Self {
        match result {
            Ok(None) => Self::default(),
            Ok(Some(message)) => Self {
                entries: vec![CleanupEvidenceItem::RuntimeRecorded(message)],
            },
            Err(error) => Self::additional(vec![format!("{context}: {error}")]),
        }
    }

    /// Preserve result's recorded report or its unrecorded error; return origin-bearing evidence.
    /// 保留 result 已登记报告或未登记错误；返回携带来源的证据。
    fn from_runtime(
        result: Result<
            vulcan_plugin_runtime::VulcanPluginDrainReport,
            vulcan_plugin_runtime::VulcanPluginRuntimeError,
        >,
    ) -> Self {
        match result {
            Ok(report) => Self {
                entries: report
                    .failures
                    .into_iter()
                    .map(CleanupEvidenceItem::RuntimeRecorded)
                    .collect(),
            },
            Err(error) => Self::additional(vec![error.to_string()]),
        }
    }

    /// Bind messages from a non-runtime producer as additional failures; return request-local evidence.
    /// 将非运行时生产者的 messages 绑定为额外失败；返回请求局部证据。
    fn additional(messages: Vec<String>) -> Self {
        Self {
            entries: messages
                .into_iter()
                .map(CleanupEvidenceItem::AdditionalFailure)
                .collect(),
        }
    }

    /// Append message from an external failure; return no value and retain its origin.
    /// 追加外部失败的 message；不返回值并保留来源。
    fn push(&mut self, message: String) {
        self.entries
            .push(CleanupEvidenceItem::AdditionalFailure(message));
    }

    /// Append other in original order without changing classifications; return no value.
    /// 按原顺序追加 other，不改变分类；不返回值。
    fn extend(&mut self, other: Self) {
        self.entries.extend(other.entries);
    }

    /// Return whether every cleanup producer completed without retained evidence.
    /// 返回是否全部清理生产者已完成且无保留证据。
    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Project all original messages with separator for unchanged compensation errors; return text.
    /// 使用 separator 投影全部原消息以保持补偿错误；返回文本。
    fn join(&self, separator: &str) -> String {
        self.entries
            .iter()
            .map(|entry| match entry {
                CleanupEvidenceItem::RuntimeRecorded(message)
                | CleanupEvidenceItem::AdditionalFailure(message) => message.as_str(),
            })
            .collect::<Vec<_>>()
            .join(separator)
    }

    /// Consume this collection and return only unrecorded failures for the unchanged runtime finish API.
    /// 消费此集合，仅返回尚未登记的失败给未变运行时 finish 接口。
    fn additional_failures(self) -> Vec<String> {
        self.entries
            .into_iter()
            .filter_map(|entry| match entry {
                CleanupEvidenceItem::RuntimeRecorded(_) => None,
                CleanupEvidenceItem::AdditionalFailure(message) => Some(message),
            })
            .collect()
    }
}

/// Publish one original native control under the real service slot lock, preserving its first host issuer.
/// 在真实服务槽锁内发布单个原原生控制，保留其首次宿主签发者。
/// state owns the actual bridge, native is its ready original, and catalog is that original source snapshot.
/// state 持有实际桥，native 为其就绪原实例，catalog 为该原来源快照。
/// Returns the actual full commit witness; shutdown or source errors precede the pointer write and do not replay initialization.
/// 返回实际完整提交见证；关闭或来源错误先于指针写入，且不重播初始化。
#[cfg(test)]
pub(super) fn publish_native_published_plugin_extension_generation(
    state: &SharedServiceBridgeState,
    native: &vulcan_agent_runtime::mcp::PublishedPluginMcpRuntime,
    catalog: Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
) -> Result<
    Arc<vulcan_agent_runtime::agent_runtime::extension_generation::ExtensionGeneration>,
    String,
> {
    // Acquire service ownership before source and native guards to avoid native-to-service lock inversion.
    // 在来源及原生守卫之前取得服务所有权，避免原生到服务的锁反序。
    let mut guard = state
        .lock()
        .map_err(|error| format!("Failed to lock the service state: {error}"))?;
    crate::service_state_access::require_plugin_publication_shutdown_admission(&guard)?;
    crate::service_state_access::require_plugin_update_full_publication_admission(&guard, None)?;
    // This actual bridge contains the sole full slot consumed by new executions.
    // 此实际桥包含新执行消费的唯一完整槽。
    let bridge = guard
        .agent_runtime
        .as_mut()
        .ok_or_else(|| String::from("The agent extension generation is not initialized yet."))?;
    // The same original prepared lease transfers without allocation of a replacement runtime or issuer.
    // 同一原已准备租约转交，不分配替代运行时或签发者。
    let (committed, displaced) =
        bridge.publish_native_plugin_extension_generation(native, catalog)?;
    drop(guard);
    displaced.retire_plugin_partitions_after(
        native.observer().identity().0,
        AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
    );
    Ok(committed)
}

/// One exact desired lifecycle state selected by the service request.
/// 服务请求选择的唯一目标生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VulcanPluginTargetState {
    /// The plugin must become enabled and ready for hook traffic.
    /// 插件必须进入已启用状态并准备接收 Hook 流量。
    Enabled,
    /// The plugin must reject new work and become disabled.
    /// 插件必须拒绝新任务并进入已停用状态。
    Disabled,
}

impl VulcanPluginTargetState {
    /// Converts the request boolean at the transport boundary into a domain state.
    /// 在传输边界把请求布尔值转换为领域状态。
    ///
    /// # Parameters
    /// 参数
    /// `enabled` is the exact state requested by the caller.
    /// `enabled` 是调用方请求的精确状态。
    ///
    /// # Returns
    /// 返回值
    /// Returns the corresponding strongly typed target state.
    /// 返回对应的强类型目标状态。
    fn from_enabled(enabled: bool) -> Self {
        if enabled {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }

    /// Returns whether this target persists an enabled override.
    /// 返回该目标是否持久化启用覆盖。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only for the enabled target.
    /// 仅对启用目标返回 `true`。
    fn is_enabled(self) -> bool {
        self == Self::Enabled
    }
}

/// Immutable policy sets used to rebuild one exact catalog projection.
/// 用于重建唯一目录投影的不可变策略集合。
#[derive(Debug, Clone)]
struct VulcanPluginPolicySnapshot {
    /// Explicit plugin identifiers disabled by configuration.
    /// 配置中显式停用的插件标识集合。
    disabled_plugin_ids: BTreeSet<String>,
    /// Explicit plugin identifiers enabled by configuration.
    /// 配置中显式启用的插件标识集合。
    enabled_plugin_ids: BTreeSet<String>,
}

/// Complete immutable facts required by one lifecycle transaction and every rollback path.
/// 单次生命周期事务及其所有回滚路径所需的完整不可变事实。
#[derive(Debug, Clone)]
struct VulcanPluginTransition {
    /// Exact manifest-owned plugin identifier.
    /// 清单持有的精确插件标识。
    plugin_id: String,
    /// Requested domain target state.
    /// 请求的领域目标状态。
    target: VulcanPluginTargetState,
    /// Manifest default used to persist only meaningful overrides.
    /// 用于仅持久化有效覆盖的清单默认值。
    default_enabled: bool,
    /// Effective state that must be restored after failure.
    /// 失败后必须恢复的原始生效状态。
    previous_enabled: bool,
    /// Installed package generation that every candidate MCP descriptor must match.
    /// 每个候选 MCP 描述符都必须匹配的已安装包代次。
    plugin_generation: u64,
    /// Complete MCP descriptor set validated for the requested target catalog.
    /// 针对请求目标目录完成校验的完整 MCP 描述符集合。
    target_mcp_servers: Vec<vulcan_plugin_runtime::VulcanPluginMcpServerDescriptor>,
    /// Original descriptors required to restart global MCP after a failed committed disable.
    /// 已提交停用失败后重启全局 MCP 所需的原始描述符。
    previous_mcp_servers: Vec<vulcan_plugin_runtime::VulcanPluginMcpServerDescriptor>,
    /// Policy projection captured before any mutation.
    /// 任意变更前捕获的策略投影。
    previous_policy: VulcanPluginPolicySnapshot,
    /// Prevalidated policy projection for the requested state.
    /// 针对请求状态预校验的策略投影。
    target_policy: VulcanPluginPolicySnapshot,
}

/// Enables or disables one host-owned Vulcan Plugin as a compensating transaction.
/// 以补偿事务方式启用或停用单个宿主持有的 Vulcan 插件。
///
/// # Parameters
/// 参数
/// `state` owns service configuration, `plugin_id` selects the plugin, and `enabled` selects the target state.
/// `state` 持有服务配置，`plugin_id` 选择插件，`enabled` 选择目标状态。
///
/// # Returns
/// 返回值
/// Returns the refreshed LuaSkills projection after every lifecycle phase commits.
/// 在全部生命周期阶段提交后返回刷新的 LuaSkills 投影。
pub(super) async fn set_vulcan_plugin_enabled(
    state: SharedServiceBridgeState,
    plugin_id: String,
    enabled: bool,
) -> Result<PluginCatalogProjectionUpdate, String> {
    // stable_plugin_id is owned because every asynchronous compensation path uses the same identity.
    // stable_plugin_id 采用所有权字符串，因为所有异步补偿路径都使用同一标识。
    let stable_plugin_id = plugin_id.trim().to_owned();
    if stable_plugin_id.is_empty() {
        return Err(String::from(
            "The Vulcan plugin identifier must not be empty.",
        ));
    }
    {
        // Core refresh also replaces the exact catalog Arc, so reject before any lifecycle or policy mutation begins.
        // 核心刷新也替换精确目录 Arc，因此在任何生命周期或策略变更开始前拒绝。
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock the service state: {error}"))?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard, None,
        )?;
    }
    // management_guard serializes catalog, lifecycle, persistence, and hook ownership transfer.
    // management_guard 串行化目录、生命周期、持久化与 Hook 所有权交接。
    // provider_configuration is the immutable rollback baseline for this transaction.
    // provider_configuration 是本次事务的不可变回滚基线。
    let provider_configuration = provider_configuration_from_state(&state)?;
    // runtime owns every transient state and managed plugin lease.
    // runtime 持有全部临时状态与受管插件租约。
    let runtime = service_vulcan_plugin_runtime(&state)?;
    // current_catalog projects the exact persisted policy before request evaluation.
    // current_catalog 在请求评估前投影精确的持久化策略。
    let current_catalog = runtime
        .refresh_catalog(
            &provider_configuration.luaskills.disabled_vulcan_plugins,
            &provider_configuration.luaskills.enabled_vulcan_plugins,
        )
        .map_err(|error| error.to_string())?;
    // current_plugin is the unique discovered package selected by the stable identifier.
    // current_plugin 是稳定标识选择的唯一已发现插件包。
    let current_plugin = current_catalog
        .plugins
        .get(&stable_plugin_id)
        .ok_or_else(|| format!("Vulcan plugin not found: {stable_plugin_id}"))?;
    // default_enabled is the manifest baseline required by configuration normalization.
    // default_enabled 是配置归一化所需的清单基线。
    let default_enabled = current_plugin
        .manifest
        .as_ref()
        .map(|manifest| manifest.default_enabled)
        .ok_or_else(|| format!("Vulcan plugin `{stable_plugin_id}` has no valid manifest; its state cannot be changed"))?;
    // previous_enabled is the exact effective state restored by every failed transaction.
    // previous_enabled 是每个失败事务都要恢复的精确生效状态。
    let previous_enabled = current_plugin.desired_enabled;
    // plugin_generation is the installed content generation retained across this enable/disable policy transition.
    // plugin_generation 是在本次启停策略转换期间保持不变的已安装内容代次。
    let plugin_generation = current_plugin.generation;
    // target converts the transport boolean once so later branches carry domain meaning.
    // target 仅转换一次传输布尔值，使后续分支携带领域语义。
    let target = VulcanPluginTargetState::from_enabled(enabled);
    // requires_cleanup_retry observes retained cleanup independently from immutable routing.
    // requires_cleanup_retry 独立于不可变路由观测保留清理。
    let requires_cleanup_retry = runtime
        .has_cleanup_diagnostics(&stable_plugin_id)
        .map_err(|error| error.to_string())?;
    // already_settled excludes cleanup-warning states that require an explicit retry.
    // already_settled 排除需要显式重试的清理告警状态。
    let already_settled = matches!(
        (target, current_plugin.runtime_state),
        (
            VulcanPluginTargetState::Enabled,
            vulcan_plugin_runtime::VulcanPluginRuntimeState::Enabled
        ) | (
            VulcanPluginTargetState::Disabled,
            vulcan_plugin_runtime::VulcanPluginRuntimeState::Disabled
        )
    ) && !requires_cleanup_retry;
    if !matches!(
        current_plugin.availability,
        vulcan_plugin_runtime::VulcanPluginAvailability::Enabled
            | vulcan_plugin_runtime::VulcanPluginAvailability::Disabled
    ) {
        return Err(format!(
            "Vulcan plugin `{stable_plugin_id}` is currently not manageable: {}",
            current_plugin.diagnostics.join("; ")
        ));
    }
    if previous_enabled == target.is_enabled() && already_settled {
        return refresh_plugin_catalog_local_state(&state).await;
    }

    // target_luaskills applies the requested override without mutating retained state.
    // target_luaskills 在不修改保留状态的前提下应用请求覆盖。
    let mut target_luaskills = provider_configuration.luaskills.clone();
    target_luaskills.set_vulcan_plugin_enabled_with_default(
        &stable_plugin_id,
        target.is_enabled(),
        default_enabled,
    );
    // target_catalog validates the entire requested package and hook surface before side effects.
    // target_catalog 在产生副作用前校验完整的目标插件与 Hook 能力面。
    let target_catalog = vulcan_plugin_runtime::load_vulcan_plugin_catalog(
        runtime.vulcan_plugin_root(),
        Some(runtime.user_plugin_root()),
        &target_luaskills.disabled_vulcan_plugins,
        &target_luaskills.enabled_vulcan_plugins,
        0,
    );
    // target_plugin proves the selected package survives target catalog validation.
    // target_plugin 证明所选插件通过目标目录校验后仍然存在。
    let target_plugin = target_catalog
        .plugins
        .get(&stable_plugin_id)
        .ok_or_else(|| {
            format!("Vulcan plugin not found in the target catalog: {stable_plugin_id}")
        })?;
    // expected_availability is the only availability accepted for the requested target.
    // expected_availability 是请求目标唯一允许的可用性。
    let expected_availability = if target.is_enabled() {
        vulcan_plugin_runtime::VulcanPluginAvailability::Enabled
    } else {
        vulcan_plugin_runtime::VulcanPluginAvailability::Disabled
    };
    if target_plugin.availability != expected_availability {
        return Err(format!(
            "Vulcan plugin `{stable_plugin_id}` cannot enter the requested state: {}",
            target_plugin.diagnostics.join("; ")
        ));
    }
    // target_hooks are cloned from the prevalidated catalog before it leaves this planning phase.
    // target_hooks 在离开规划阶段前从预校验目录中克隆。
    let target_hooks = target_plugin.hooks.clone();
    // target_mcp_servers receives the installed generation because the standalone validation catalog intentionally starts descriptors at zero.
    // target_mcp_servers 使用已安装代次，因为独立校验目录会有意把描述符初始代次设为零。
    let target_mcp_servers = target_plugin
        .mcp_servers
        .iter()
        .cloned()
        .map(|mut server| {
            server.plugin_generation = plugin_generation;
            server
        })
        .collect();
    // transition freezes both policy projections so rollback never rereads mutable service state.
    // transition 固定两个策略投影，使回滚绝不重新读取可变服务状态。
    let transition = VulcanPluginTransition {
        plugin_id: stable_plugin_id,
        target,
        default_enabled,
        previous_enabled,
        plugin_generation,
        target_mcp_servers,
        previous_mcp_servers: current_plugin.mcp_servers.clone(),
        previous_policy: VulcanPluginPolicySnapshot {
            disabled_plugin_ids: provider_configuration
                .luaskills
                .disabled_vulcan_plugins
                .clone(),
            enabled_plugin_ids: provider_configuration
                .luaskills
                .enabled_vulcan_plugins
                .clone(),
        },
        target_policy: VulcanPluginPolicySnapshot {
            disabled_plugin_ids: target_luaskills.disabled_vulcan_plugins,
            enabled_plugin_ids: target_luaskills.enabled_vulcan_plugins,
        },
    };

    // prepared_mcp_runtime is a fully Ready candidate access generation and remains unpublished until every durable phase succeeds.
    // prepared_mcp_runtime 是完全 Ready 的候选访问代次，在全部持久化阶段成功前保持未发布。
    let prepared_mcp_runtime = match transition.target {
        VulcanPluginTargetState::Enabled => {
            prepare_enable_transition(
                &state,
                &runtime,
                &transition,
                &target_hooks,
                requires_cleanup_retry,
            )
            .await?
        }
        VulcanPluginTargetState::Disabled => {
            prepare_disable_transition(&state, &runtime, &transition).await?;
            None
        }
    };
    if let Err(error) = persist_transition_target(&state, &transition).await {
        // rollback_result preserves runtime, lifecycle, and Hook compensation diagnostics.
        // rollback_result 保留运行时、生命周期与 Hook 补偿诊断。
        let rollback_result = rollback_prepared_transition(
            &state,
            &runtime,
            &transition,
            prepared_mcp_runtime.as_ref(),
        )
        .await;
        return Err(append_rollback_error(error, rollback_result));
    }
    match transition.target {
        VulcanPluginTargetState::Enabled => {
            commit_enable_transition(
                &state,
                &runtime,
                &transition,
                prepared_mcp_runtime.ok_or_else(|| {
                    format!(
                        "Vulcan plugin `{}` has no prepared MCP access generation.",
                        transition.plugin_id
                    )
                })?,
            )
            .await?;
        }
        VulcanPluginTargetState::Disabled => {
            commit_disable_transition(&state, &runtime, &transition).await?;
        }
    }
    refresh_plugin_catalog_local_state(&state).await
}

/// Prepares an enable transition through validation, lifecycle callback, and probe.
/// 通过校验、生命周期回调与探测准备启用转换。
///
/// # Parameters
/// 参数
/// Service state, runtime, immutable transition, target hooks, and cleanup retry flag define the phase.
/// 服务状态、运行时、不可变转换、目标 Hook 与清理重试标记共同定义该阶段。
///
/// # Returns
/// 返回值
/// Returns success only while the plugin remains safely in `Enabling`.
/// 仅当插件安全保持在 `Enabling` 状态时返回成功。
async fn prepare_enable_transition(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
    target_hooks: &[vulcan_plugin_runtime::VulcanPluginHookDescriptor],
    requires_cleanup_retry: bool,
) -> Result<Option<McpRuntimeAccess>, String> {
    if requires_cleanup_retry {
        // cleanup_report proves retained sessions are gone before capacity is reused.
        // cleanup_report 证明在复用容量前遗留会话已经消失。
        let cleanup_report = runtime
            .drain_plugin_sessions(&transition.plugin_id)
            .await
            .map_err(|error| {
                format!(
                    "Vulcan plugin `{}` cleanup before re-enabling failed: {error}",
                    transition.plugin_id
                )
            })?;
        if !cleanup_report.failures.is_empty() {
            finish_transition_and_sync(
                state,
                runtime,
                transition,
                &transition.previous_policy,
                CleanupEvidence::from_runtime(Ok(cleanup_report.clone())),
            )?;
            return Err(format!(
                "Vulcan plugin `{}` still has uncleaned resources: {}",
                transition.plugin_id,
                cleanup_report.failures.join("; ")
            ));
        }
    }
    validate_service_vulcan_plugin_hooks(state, target_hooks).map_err(|error| {
        format!(
            "Vulcan plugin `{}` has an invalid hook declaration: {error}",
            transition.plugin_id
        )
    })?;
    // environment_ready is a read-only preflight so enabling never downloads dependencies.
    // environment_ready 是只读前置检查，确保启用过程绝不下载依赖。
    let environment_ready = runtime
        .plugin_environment_ready(&transition.plugin_id)
        .map_err(|error| error.to_string())?;
    if !environment_ready {
        return Err(format!(
            "The execution environment of Vulcan plugin `{}` is not installed yet; install it in the plugin settings first.",
            transition.plugin_id
        ));
    }
    runtime
        .begin_enabling(&transition.plugin_id)
        .map_err(|error| error.to_string())?;
    // Keep owner MCP admission closed for the whole preparation window, including a previously plain Disabled state.
    // 在整个启用准备窗口内保持所有者 MCP 准入关闭，包括此前只是普通 Disabled 的状态。
    begin_plugin_mcp_drain(state, &transition.plugin_id).map_err(|error| {
        append_rollback_error(
            format!("Failed to open the Vulcan plugin owner MCP drain gate: {error}"),
            restore_previous_projection(state, runtime, transition, CleanupEvidence::default()),
        )
    })?;
    if let Err(error) = wait_plugin_mcp_drain(state, &transition.plugin_id).await {
        return Err(append_rollback_error(
            format!("MCP drain before re-enabling failed: {error}"),
            restore_previous_projection(
                state,
                runtime,
                transition,
                CleanupEvidence::additional(vec![error]),
            ),
        ));
    }
    synchronize_transient_hooks(state, runtime).map_err(|error| {
        append_rollback_error(
            format!("Vulcan plugin hook sync failed: {error}"),
            restore_previous_projection(state, runtime, transition, CleanupEvidence::default()),
        )
    })?;
    if let Err(error) = runtime
        .invoke_lifecycle(
            &transition.plugin_id,
            vulcan_plugin_runtime::VulcanPluginLifecycleEvent::Enable,
        )
        .await
    {
        // cleanup_failures captures partial callback side effects through the declared disable callback.
        // cleanup_failures 通过声明的停用回调捕获部分回调副作用。
        let cleanup_failures =
            compensate_failed_enable(state, runtime, &transition.plugin_id).await;
        return Err(append_rollback_error(
            format!(
                "Vulcan plugin `{}` enable callback failed: {error}",
                transition.plugin_id
            ),
            restore_previous_projection(state, runtime, transition, cleanup_failures),
        ));
    }
    if let Err(error) = runtime.probe_plugin(&transition.plugin_id).await {
        // cleanup_failures compensates a successful callback whose runtime probe failed.
        // cleanup_failures 补偿回调成功但运行时探测失败的情况。
        let cleanup_failures =
            compensate_failed_enable(state, runtime, &transition.plugin_id).await;
        return Err(append_rollback_error(
            format!(
                "Vulcan plugin `{}` enable probe failed: {error}",
                transition.plugin_id
            ),
            restore_previous_projection(state, runtime, transition, cleanup_failures),
        ));
    }
    // Build from this operation's validated target rather than a later catalog scan.
    // 根据此次操作已校验的目标构建，不读取后续目录扫描。
    let candidate_result = prepare_transition_mcp(
        state,
        runtime,
        &transition.plugin_id,
        transition.plugin_generation,
        &transition.target_mcp_servers,
    )
    .await;
    let candidate_mcp_runtime = match candidate_result {
        Ok(candidate) => candidate,
        Err(error) => {
            let cleanup_failures =
                compensate_failed_enable(state, runtime, &transition.plugin_id).await;
            return Err(append_rollback_error(
                format!(
                    "Vulcan plugin `{}` MCP readiness check failed: {error}",
                    transition.plugin_id
                ),
                restore_previous_projection(state, runtime, transition, cleanup_failures),
            ));
        }
    };
    Ok(Some(candidate_mcp_runtime))
}

/// Prepares a disable transition by rejecting new work and removing hooks atomically.
/// 通过拒绝新任务并原子移除 Hook 来准备停用转换。
///
/// # Parameters
/// 参数
/// Service state, runtime, and immutable transition identify the exact operation.
/// 服务状态、运行时与不可变转换标识精确操作。
///
/// # Returns
/// 返回值
/// Returns success only while the plugin remains safely in `Draining`.
/// 仅当插件安全保持在 `Draining` 状态时返回成功。
async fn prepare_disable_transition(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
) -> Result<(), String> {
    runtime
        .begin_draining(&transition.plugin_id)
        .map_err(|error| error.to_string())?;
    // Close the shared owner MCP gate before any asynchronous drain or durable publication.
    // 在任何异步排空或可靠发布前关闭共享所有者 MCP 闸门。
    begin_plugin_owner_mcp_drain(state, &transition.plugin_id).map_err(|error| {
        append_rollback_error(
            format!("Failed to open the Vulcan plugin owner MCP drain gate: {error}"),
            restore_previous_projection(state, runtime, transition, CleanupEvidence::default()),
        )
    })?;
    if let Err(error) = synchronize_transient_hooks(state, runtime) {
        let rollback =
            restore_previous_projection(state, runtime, transition, CleanupEvidence::default());
        // Restore the gate to the immutable pre-transition policy instead of reopening it blindly.
        // 按不可变的转换前策略恢复闸门，不能无条件重新开放。
        let gate = if transition.previous_enabled {
            finish_plugin_owner_mcp_drain(state, &transition.plugin_id).await
        } else {
            begin_plugin_owner_mcp_drain(state, &transition.plugin_id)
        };
        let rollback = append_rollback_error(append_rollback_error(error, rollback), gate);
        return Err(format!("Vulcan plugin hook sync failed: {rollback}"));
    }
    Ok(())
}

/// Persists the requested target policy after runtime preparation succeeds.
/// 在运行时准备成功后持久化请求的目标策略。
///
/// # Parameters
/// 参数
/// `state` owns persisted configuration and `transition` supplies normalized target facts.
/// `state` 持有持久化配置，`transition` 提供归一化目标事实。
///
/// # Returns
/// 返回值
/// Returns success after durable and retained configuration both change.
/// 在持久化配置与保留配置都完成变更后返回成功。
async fn persist_transition_target(
    state: &SharedServiceBridgeState,
    transition: &VulcanPluginTransition,
) -> Result<(), String> {
    persist_transition_enabled_state(
        state,
        transition,
        transition.target.is_enabled(),
        "Failed to save the Vulcan plugin state",
    )
    .await
}

/// Persists one exact effective state using the transition's manifest default.
/// 使用转换的清单默认值持久化一个精确生效状态。
///
/// # Parameters
/// 参数
/// Service state, transition facts, effective state, and error context define the write.
/// 服务状态、转换事实、生效状态与错误上下文共同定义该写入。
///
/// # Returns
/// 返回值
/// Returns success after the provider configuration transaction commits.
/// 在 Provider 配置事务提交后返回成功。
async fn persist_transition_enabled_state(
    state: &SharedServiceBridgeState,
    transition: &VulcanPluginTransition,
    enabled: bool,
    error_context: &str,
) -> Result<(), String> {
    // persisted_plugin_id moves the immutable identity into the mutation closure.
    // persisted_plugin_id 把不可变标识移入变更闭包。
    let persisted_plugin_id = transition.plugin_id.clone();
    // default_enabled is copied because it belongs to the immutable manifest baseline.
    // default_enabled 被复制，因为它属于不可变清单基线。
    let default_enabled = transition.default_enabled;
    mutate_provider_configuration(
        state,
        error_context,
        move |configuration| {
            configuration
                .luaskills
                .set_vulcan_plugin_enabled_with_default(
                    &persisted_plugin_id,
                    enabled,
                    default_enabled,
                );
            Ok(())
        },
        |_, _| (),
    )
    .await
}

/// Commits a prepared enable transition and activates its hook runtime.
/// 提交已准备的启用转换并激活其 Hook 运行时。
///
/// # Parameters
/// 参数
/// Service state, runtime, and transition own final catalog and hook publication.
/// 服务状态、运行时与转换持有最终目录及 Hook 发布过程。
///
/// # Returns
/// 返回值
/// Returns success only after final hook synchronization and prewarming complete.
/// 仅在最终 Hook 同步与预热全部完成后返回成功。
async fn commit_enable_transition(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
    candidate_mcp_runtime: McpRuntimeAccess,
) -> Result<(), String> {
    // final_catalog publishes the prevalidated target policy without rereading mutable configuration.
    // final_catalog 使用预校验目标策略发布，而不重新读取可变配置。
    let final_catalog = match finish_transition(
        runtime,
        transition,
        &transition.target_policy,
        CleanupEvidence::default(),
    ) {
        Ok(catalog) => catalog,
        Err(error) => {
            // cleanup_failures compensates while the snapshot still authorizes EnableRollback.
            // cleanup_failures 在快照仍允许 EnableRollback 时完成补偿。
            let mut cleanup_failures =
                drain_prepared_transition_mcp(&candidate_mcp_runtime, &transition.plugin_id).await;
            cleanup_failures
                .extend(compensate_failed_enable(state, runtime, &transition.plugin_id).await);
            // persistence_rollback restores the durable baseline before catalog restoration.
            // persistence_rollback 在目录恢复前还原持久化基线。
            let persistence_rollback = persist_transition_enabled_state(
                state,
                transition,
                transition.previous_enabled,
                "Failed to roll back the Vulcan plugin state",
            )
            .await;
            if let Err(rollback_error) = persistence_rollback {
                return Err(format!(
                    "Vulcan plugin enable final publication failed: {error}; configuration rollback failed: {rollback_error}"
                ));
            }
            return Err(append_rollback_error(
                format!("Vulcan plugin enable final publication failed: {error}"),
                restore_previous_projection(state, runtime, transition, cleanup_failures),
            ));
        }
    };
    // activation_error joins atomic hook replacement and persistent runtime cold initialization.
    // activation_error 合并原子 Hook 替换与持久运行时冷初始化错误。
    let activation_error =
        synchronize_service_vulcan_plugin_hooks(state, runtime.clone(), &final_catalog).err();
    // activation_error is extended asynchronously only when hook replacement succeeded.
    // activation_error 仅在 Hook 替换成功时异步扩展预热错误。
    let activation_error = if activation_error.is_none() {
        runtime
            .prewarm_plugin_runtime(&transition.plugin_id)
            .await
            .map(|_| ())
            .map_err(|error| format!("Plugin hook runtime warm-up failed: {error}"))
            .err()
    } else {
        activation_error
    };
    // activation_error includes the final atomic extension publication only after hooks and runtime prewarm succeed.
    // activation_error 仅在 Hook 与运行时预热成功后才包含最终原子扩展发布错误。
    let activation_error = if activation_error.is_none() {
        publish_extension_generation(
            state,
            &transition.plugin_id,
            Arc::clone(&final_catalog),
            candidate_mcp_runtime.clone(),
        )
        .err()
    } else {
        activation_error
    };
    if let Some(error) = activation_error {
        // An unpublished candidate is not in the current service view; retire it explicitly first.
        // 未发布候选不在当前服务视图中；先显式退役它。
        let cleanup =
            drain_prepared_transition_mcp(&candidate_mcp_runtime, &transition.plugin_id).await;
        // Preserve cleanup evidence without losing the original activation failure.
        // 保留清理证据，不丢失原激活失败。
        let error = if cleanup.is_empty() {
            error
        } else {
            format!("{error}; {}", cleanup.join("; "))
        };
        // rollback_result performs a real disable lifecycle because the plugin is already Enabled.
        // rollback_result 执行真实停用生命周期，因为插件已经进入 Enabled。
        let rollback_result = rollback_finalized_enable_to_policy(
            state,
            runtime,
            transition,
            transition.previous_enabled,
            &transition.previous_policy,
        )
        .await;
        return Err(append_rollback_error(
            format!("Vulcan plugin hook final activation failed: {error}"),
            rollback_result,
        ));
    }
    // Reopen owner MCP admission only after the enabled catalog and matching runtime generation are published.
    // 仅在启用目录及其匹配运行时代次发布后重新开放所有者 MCP 准入。
    if let Err(error) = finish_plugin_owner_mcp_drain(state, &transition.plugin_id).await {
        let rollback_result = rollback_finalized_enable_to_policy(
            state,
            runtime,
            transition,
            transition.previous_enabled,
            &transition.previous_policy,
        )
        .await;
        return Err(append_rollback_error(
            format!("Failed to restore the Vulcan plugin owner MCP admission: {error}"),
            rollback_result,
        ));
    }
    Ok(())
}

/// Commits a prepared disable transition after draining all retained work.
/// 在排空全部遗留任务后提交已准备的停用转换。
///
/// # Parameters
/// 参数
/// Service state, runtime, and transition own cleanup, callback, and final publication.
/// 服务状态、运行时与转换持有清理、回调与最终发布过程。
///
/// # Returns
/// 返回值
/// Returns success after the disabled catalog is published.
/// 在停用目录发布后返回成功。
async fn commit_disable_transition(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
) -> Result<(), String> {
    // Close all original global partitions only after the disabled policy is durable.
    // 仅在停用策略持久化之后关闭全部原全局分区。
    begin_plugin_mcp_drain(state, &transition.plugin_id)?;
    // cleanup_failures retains non-fatal drain and lifecycle diagnostics in the final runtime state.
    // cleanup_failures 在最终运行时状态中保留非致命排空与生命周期诊断。
    let mut cleanup_failures =
        CleanupEvidence::from_runtime(runtime.drain_plugin_sessions(&transition.plugin_id).await);
    if let Err(error) = wait_plugin_mcp_drain(state, &transition.plugin_id).await {
        cleanup_failures.push(format!("Plugin MCP cleanup failed: {error}"));
    }
    // This exact existing prefix is shared by recording and accounting-error projection.
    // 此精确既有前缀由记账及记账错误投影共享。
    let diagnostic_context = "The plugin disable lifecycle callback failed";
    cleanup_failures.extend(CleanupEvidence::from_recorded_lifecycle(
        runtime
            .invoke_lifecycle_for_cleanup(
                &transition.plugin_id,
                vulcan_plugin_runtime::VulcanPluginLifecycleEvent::Disable,
                diagnostic_context,
            )
            .await,
        diagnostic_context,
    ));
    if transition.plugin_id == "vulcan-browser" {
        // Browser cleanup is part of the disable transaction and must be observed before the
        // final catalog publication, otherwise a failed retirement is reported as plain Disabled.
        // 浏览器清理属于停用事务，必须在最终目录发布前纳入结果，否则撤销失败会被误报为普通 Disabled。
        let browser_manager = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .browser_manager
            .clone();
        if let Err(error) = browser_manager.shutdown_ai_connections().await {
            cleanup_failures.push(format!("Browser AI MCP cleanup failed: {error}"));
        }
    }
    // finish_result is the only post-callback state publication and therefore owns disable rollback.
    // finish_result 是回调后唯一的状态发布，因此持有停用回滚责任。
    // rollback_cleanup_failures preserves diagnostics if final catalog publication itself fails.
    // rollback_cleanup_failures 在最终目录发布自身失败时保留诊断。
    let rollback_cleanup_failures = cleanup_failures.clone();
    let final_catalog = match finish_transition(
        runtime,
        transition,
        &transition.target_policy,
        cleanup_failures,
    ) {
        Ok(catalog) => catalog,
        Err(error) => {
            let rollback_result =
                rollback_disable(state, runtime, transition, rollback_cleanup_failures, false)
                    .await;
            return Err(append_rollback_error(
                format!("Vulcan plugin disable final publication failed: {error}"),
                rollback_result,
            ));
        }
    };
    let current_mcp_runtime = state
        .lock()
        .map_err(|error| format!("Failed to lock the service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|agent_runtime| agent_runtime.extension_generation.mcp_runtime())
        .ok_or_else(|| String::from("The agent extension generation is not initialized yet."))?;
    let next_mcp_runtime =
        remove_plugin_mcp_runtime_partition(&current_mcp_runtime, &transition.plugin_id);
    if let Err(error) = publish_extension_generation(
        state,
        &transition.plugin_id,
        final_catalog,
        next_mcp_runtime,
    ) {
        let rollback_result =
            rollback_disable(state, runtime, transition, rollback_cleanup_failures, true).await;
        return Err(append_rollback_error(
            format!("Vulcan plugin disable extension generation publication failed: {error}"),
            rollback_result,
        ));
    }
    // Draining and Disabled both expose an empty hook surface, so another dispatcher write is redundant.
    // Draining 与 Disabled 都暴露空 Hook 面，因此无需再次写入调度器。
    Ok(())
}

/// Rolls back a transition whose target configuration could not be persisted.
/// 回滚目标配置无法持久化的转换。
///
/// # Parameters
/// 参数
/// Service state, runtime, and transition determine the exact compensation direction.
/// 服务状态、运行时与转换决定精确补偿方向。
/// The enabling branch receives its exact unpublished MCP candidate; disabling has no candidate.
/// 启用分支接收其精确未发布 MCP 候选；停用分支没有候选。
///
/// # Returns
/// 返回值
/// Returns success after lifecycle and Hook state return to the previous projection.
/// 在生命周期与 Hook 状态返回原始投影后返回成功。
async fn rollback_prepared_transition(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
    candidate_mcp: Option<&McpRuntimeAccess>,
) -> Result<(), String> {
    match transition.target {
        VulcanPluginTargetState::Enabled => {
            // cleanup_failures compensates the completed enable callback before projection rollback.
            // cleanup_failures 在投影回滚前补偿已完成的启用回调。
            // Enabling preparation always owns a ready candidate; disabled preparation owns none.
            // 启用准备始终持有就绪候选；停用准备不持有候选。
            let candidate = candidate_mcp.ok_or_else(|| {
                String::from("The enable compensation has no original MCP candidate.")
            })?;
            // Keep original candidate cleanup diagnostics alongside lifecycle compensation.
            // 将原候选清理诊断与生命周期补偿一起保留。
            let mut cleanup_failures =
                drain_prepared_transition_mcp(candidate, &transition.plugin_id).await;
            cleanup_failures
                .extend(compensate_failed_enable(state, runtime, &transition.plugin_id).await);
            let result = restore_previous_projection(state, runtime, transition, cleanup_failures);
            let gate_result = if transition.previous_enabled {
                finish_plugin_owner_mcp_drain(state, &transition.plugin_id).await
            } else {
                begin_plugin_owner_mcp_drain(state, &transition.plugin_id)
            };
            match (result, gate_result) {
                (Ok(()), Ok(())) => Ok(()),
                (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
                (Err(error), Err(gate_error)) => Err(format!(
                    "{error}; owner MCP gate compensation failed: {gate_error}"
                )),
            }
        }
        VulcanPluginTargetState::Disabled => {
            // No disable callback ran before persistence, so only catalog and hooks need restoration.
            // 持久化前尚未执行停用回调，因此只需恢复目录与 Hook。
            let result =
                restore_previous_projection(state, runtime, transition, CleanupEvidence::default());
            let gate_result = if transition.previous_enabled {
                finish_plugin_owner_mcp_drain(state, &transition.plugin_id).await
            } else {
                begin_plugin_owner_mcp_drain(state, &transition.plugin_id)
            };
            match (result, gate_result) {
                (Ok(()), Ok(())) => Ok(()),
                (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
                (Err(error), Err(gate_error)) => Err(format!(
                    "{error}; owner MCP gate compensation failed: {gate_error}"
                )),
            }
        }
    }
}

/// Rolls back an enable that already reached `Enabled` after hook activation fails.
/// 在 Hook 激活失败后回滚已经进入 `Enabled` 的启用操作。
///
/// # Parameters
/// 参数
/// Service state, runtime, and transition provide durable and runtime rollback facts.
/// 服务状态、运行时与转换提供持久化及运行时回滚事实。
///
/// # Returns
/// 返回值
/// Returns success only after a full disable lifecycle restores the previous policy.
/// 仅在完整停用生命周期恢复原始策略后返回成功。
async fn rollback_finalized_enable_to_policy(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
    restored_enabled: bool,
    restored_policy: &VulcanPluginPolicySnapshot,
) -> Result<(), String> {
    persist_transition_enabled_state(
        state,
        transition,
        restored_enabled,
        "Failed to roll back the Vulcan plugin state",
    )
    .await?;
    runtime
        .begin_draining(&transition.plugin_id)
        .map_err(|error| error.to_string())?;
    // Keep the owner MCP gate closed while compensating a failed finalized enable.
    // 在补偿已完成但失败的启用事务时保持所有者 MCP 闸门关闭。
    begin_plugin_mcp_drain(state, &transition.plugin_id)?;
    synchronize_transient_hooks(state, runtime)?;
    // cleanup_failures records whether the real disable compensation was complete.
    // cleanup_failures 记录真实停用补偿是否完整。
    let mut cleanup_failures =
        CleanupEvidence::from_runtime(runtime.drain_plugin_sessions(&transition.plugin_id).await);
    if let Err(error) = wait_plugin_mcp_drain(state, &transition.plugin_id).await {
        cleanup_failures.push(format!("Plugin MCP cleanup failed: {error}"));
    }
    // The compensation keeps its original prefix and the runtime's actual recorded origin.
    // 补偿保留原前缀及运行时实际登记来源。
    let diagnostic_context = "The disable callback of the plugin enable rollback failed";
    cleanup_failures.extend(CleanupEvidence::from_recorded_lifecycle(
        runtime
            .invoke_lifecycle_for_cleanup(
                &transition.plugin_id,
                vulcan_plugin_runtime::VulcanPluginLifecycleEvent::Disable,
                diagnostic_context,
            )
            .await,
        diagnostic_context,
    ));
    // incomplete_cleanup is reported even though the disabled warning projection is published.
    // incomplete_cleanup 即使已经发布停用告警投影也必须上报。
    let incomplete_cleanup = cleanup_failures.clone();
    // Remove the revoked global partition from the same recovered extension publication.
    // 从同一恢复扩展发布中移除已撤销的全局分区。
    let catalog = finish_transition(runtime, transition, restored_policy, cleanup_failures)?;
    synchronize_service_vulcan_plugin_hooks(state, runtime.clone(), &catalog)?;
    // Capture current access synchronously; no service lock crosses an await.
    // 同步捕获当前访问；服务锁不跨越异步等待。
    let current_mcp = state
        .lock()
        .map_err(|error| format!("Failed to lock the service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|bridge| bridge.extension_generation.mcp_runtime())
        .ok_or_else(|| String::from("The agent extension generation is not initialized yet."))?;
    publish_extension_generation(
        state,
        &transition.plugin_id,
        catalog,
        remove_plugin_mcp_runtime_partition(&current_mcp, &transition.plugin_id),
    )?;
    if incomplete_cleanup.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "The plugin enable rollback cleanup is incomplete: {}",
            incomplete_cleanup.join("; ")
        ))
    }
}

/// Compensates a disable from either its draining state or its finalized disabled state.
/// 从排空状态或已经完成的停用状态补偿一次停用操作。
///
/// # Parameters
/// 参数
/// Service state, runtime, transition, cleanup diagnostics, and finalization flag define the exact reverse enable transaction.
/// 服务状态、运行时、转换、清理诊断与完成标记定义精确的反向启用事务。
///
/// # Returns
/// 返回值
/// Returns success after enable callback, probe, persistence, hooks, and prewarm recover.
/// 在启用回调、探测、持久化、Hook 与预热全部恢复后返回成功。
async fn rollback_disable(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
    cleanup_failures: CleanupEvidence,
    finalized: bool,
) -> Result<(), String> {
    if !transition.previous_enabled {
        // A cleanup retry started disabled, so rollback must never run an enable lifecycle.
        // 清理重试开始时就是停用状态，因此回滚绝不能执行启用生命周期。
        return restore_previous_projection(state, runtime, transition, cleanup_failures);
    }
    // Never start reverse enable effects while old MCP processes remain alive.
    // 旧 MCP 进程仍存活时绝不开始反向启用副作用。
    if let Err(error) = wait_plugin_mcp_drain(state, &transition.plugin_id).await {
        // Preserve both earlier cleanup evidence and this original resource failure.
        // 保留先前清理证据及此次原资源失败。
        let mut failures = cleanup_failures;
        failures.push(error.clone());
        return Err(append_rollback_error(
            format!("MCP drain before the disable rollback failed: {error}"),
            // The disabled target is already durable; incomplete cleanup cannot restore enabled hooks.
            // 停用目标已经持久化；清理不完整不能恢复启用 Hook。
            finish_transition_and_sync(
                state,
                runtime,
                transition,
                &transition.target_policy,
                failures,
            ),
        ));
    }
    if finalized {
        // A finalized disable no longer owns a Draining override, so recovery must use the regular Disabled -> Enabling transition.
        // 已完成停用不再持有 Draining 覆盖，因此恢复必须使用常规的 Disabled -> Enabling 转换。
        runtime
            .begin_enabling(&transition.plugin_id)
            .map_err(|error| error.to_string())?;
    } else {
        // An uncommitted disable still owns the exact Draining override and must reverse that state directly.
        // 尚未提交的停用仍持有精确的 Draining 覆盖，必须直接反转该状态。
        runtime
            .rollback_draining_to_enabling(&transition.plugin_id)
            .map_err(|error| error.to_string())?;
    }
    synchronize_transient_hooks(state, runtime)?;
    if let Err(error) = runtime
        .invoke_lifecycle(
            &transition.plugin_id,
            vulcan_plugin_runtime::VulcanPluginLifecycleEvent::Enable,
        )
        .await
    {
        let cleanup_failures =
            compensate_failed_enable(state, runtime, &transition.plugin_id).await;
        return Err(append_rollback_error(
            format!("The enable callback of the disable rollback failed: {error}"),
            finish_transition_and_sync(
                state,
                runtime,
                transition,
                &transition.target_policy,
                cleanup_failures,
            ),
        ));
    }
    if let Err(error) = runtime.probe_plugin(&transition.plugin_id).await {
        let cleanup_failures =
            compensate_failed_enable(state, runtime, &transition.plugin_id).await;
        return Err(append_rollback_error(
            format!("The enable probe of the disable rollback failed: {error}"),
            finish_transition_and_sync(
                state,
                runtime,
                transition,
                &transition.target_policy,
                cleanup_failures,
            ),
        ));
    }
    // Recreate the stopped original partition from its frozen descriptors before restoring enabled policy.
    // 恢复启用策略前，依据固定描述符重建已经停止的原分区。
    let candidate_mcp = match prepare_transition_mcp(
        state,
        runtime,
        &transition.plugin_id,
        transition.plugin_generation,
        &transition.previous_mcp_servers,
    )
    .await
    {
        Ok(candidate) => candidate,
        Err(error) => {
            // Preserve failed reverse-enable cleanup in the durable disabled projection.
            // 在持久停用投影中保留反向启用的清理失败。
            let cleanup_failures =
                compensate_failed_enable(state, runtime, &transition.plugin_id).await;
            return Err(append_rollback_error(
                format!("The MCP readiness check of the disable rollback failed: {error}"),
                finish_transition_and_sync(
                    state,
                    runtime,
                    transition,
                    &transition.target_policy,
                    cleanup_failures,
                ),
            ));
        }
    };
    if let Err(error) = persist_transition_enabled_state(
        state,
        transition,
        transition.previous_enabled,
        "Failed to roll back the Vulcan plugin state",
    )
    .await
    {
        // Stop the ready candidate before compensating the reverse enable.
        // 补偿反向启用之前停止就绪候选。
        let mut cleanup_failures =
            drain_prepared_transition_mcp(&candidate_mcp, &transition.plugin_id).await;
        cleanup_failures
            .extend(compensate_failed_enable(state, runtime, &transition.plugin_id).await);
        return Err(append_rollback_error(
            error,
            finish_transition_and_sync(
                state,
                runtime,
                transition,
                &transition.target_policy,
                cleanup_failures,
            ),
        ));
    }
    // recovered_catalog returns the plugin to its exact previous enabled projection.
    // recovered_catalog 把插件恢复到精确的原始启用投影。
    let recovered_catalog = match finish_transition(
        runtime,
        transition,
        &transition.previous_policy,
        CleanupEvidence::default(),
    ) {
        Ok(catalog) => catalog,
        Err(error) => {
            // cleanup_failures compensates the reverse enable while it remains in Enabling.
            // cleanup_failures 在反向启用仍处于 Enabling 时进行补偿。
            let mut cleanup_failures =
                drain_prepared_transition_mcp(&candidate_mcp, &transition.plugin_id).await;
            cleanup_failures
                .extend(compensate_failed_enable(state, runtime, &transition.plugin_id).await);
            persist_transition_enabled_state(
                state,
                transition,
                transition.target.is_enabled(),
                "Failed to restore the Vulcan plugin disabled state",
            )
            .await?;
            return Err(append_rollback_error(
                format!("The enable publication of the disable rollback failed: {error}"),
                finish_transition_and_sync(
                    state,
                    runtime,
                    transition,
                    &transition.target_policy,
                    cleanup_failures,
                ),
            ));
        }
    };
    // activation_error joins recovered Hook replacement and persistent runtime initialization.
    // activation_error 合并恢复后的 Hook 替换与持久运行时初始化错误。
    let activation_error =
        synchronize_service_vulcan_plugin_hooks(state, runtime.clone(), &recovered_catalog).err();
    // activation_error is extended with prewarm failure only after Hook replacement succeeds.
    // activation_error 仅在 Hook 替换成功后扩展预热失败。
    let activation_error = if activation_error.is_none() {
        runtime
            .prewarm_plugin_runtime(&transition.plugin_id)
            .await
            .map(|_| ())
            .map_err(|error| {
                format!("The hook runtime warm-up of the disable rollback failed: {error}")
            })
            .err()
    } else {
        activation_error
    };
    // Publish the recreated global MCP and recovered catalog as one extension generation.
    // 将重建的全局 MCP 与恢复目录作为同一扩展代次发布。
    let activation_error = if activation_error.is_none() {
        publish_extension_generation(
            state,
            &transition.plugin_id,
            recovered_catalog,
            candidate_mcp.clone(),
        )
        .err()
    } else {
        activation_error
    };
    if let Some(error) = activation_error {
        // Retain candidate cleanup errors alongside the activation failure.
        // 将候选清理错误与激活失败一起保留。
        let cleanup = drain_prepared_transition_mcp(&candidate_mcp, &transition.plugin_id).await;
        let error = if cleanup.is_empty() {
            error
        } else {
            format!("{error}; {}", cleanup.join("; "))
        };
        return Err(append_rollback_error(
            error,
            rollback_finalized_enable_to_policy(
                state,
                runtime,
                transition,
                transition.target.is_enabled(),
                &transition.target_policy,
            )
            .await,
        ));
    }
    // Reopen owner MCP admission only after the reverse enable transaction is fully recovered.
    // 仅在反向启用事务完整恢复后重新开放所有者 MCP 准入。
    if let Err(error) = finish_plugin_owner_mcp_drain(state, &transition.plugin_id).await {
        return Err(append_rollback_error(
            error,
            rollback_finalized_enable_to_policy(
                state,
                runtime,
                transition,
                transition.target.is_enabled(),
                &transition.target_policy,
            )
            .await,
        ));
    }
    Ok(())
}

/// Drains transient work and invokes the declared disable callback for a failed enable.
/// 排空临时任务并调用声明的停用回调以补偿失败的启用操作。
///
/// # Parameters
/// 参数
/// `state` owns original MCP observations, `runtime` owns the enabling transition, and `plugin_id` selects the exact plugin.
/// `state` 持有原 MCP 观察，`runtime` 持有启用转换，`plugin_id` 选择精确插件。
///
/// # Returns
/// 返回值
/// Returns every cleanup failure that prevents complete compensation.
/// 返回所有阻止完整补偿的清理失败。
async fn compensate_failed_enable(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    plugin_id: &str,
) -> CleanupEvidence {
    // failures combines lease drain and lifecycle compensation diagnostics.
    // failures 合并租约排空与生命周期补偿诊断。
    let mut failures =
        CleanupEvidence::additional(match begin_plugin_mcp_drain(state, plugin_id) {
            Ok(()) => wait_plugin_mcp_drain(state, plugin_id)
                .await
                .err()
                .into_iter()
                .collect::<Vec<_>>(),
            Err(error) => vec![error],
        });
    // Lua cleanup contributes to the same evidence instead of replacing MCP failures.
    // Lua 清理补充同一证据，不覆盖 MCP 失败。
    failures.extend(CleanupEvidence::from_runtime(
        runtime.drain_plugin_sessions(plugin_id).await,
    ));
    // Preserve this existing host prefix for both recorded evidence and accounting failures.
    // 为已登记证据及记账失败保留此既有宿主前缀。
    let diagnostic_context = "The plugin enable compensation callback failed";
    failures.extend(CleanupEvidence::from_recorded_lifecycle(
        runtime
            .invoke_lifecycle_for_cleanup(
                plugin_id,
                vulcan_plugin_runtime::VulcanPluginLifecycleEvent::EnableRollback,
                diagnostic_context,
            )
            .await,
        diagnostic_context,
    ));
    failures
}

/// Synchronizes the current transient runtime snapshot into the shared dispatcher.
/// 把当前临时运行时快照同步到共享调度器。
///
/// # Parameters
/// 参数
/// Service state owns the dispatcher and runtime owns the transient snapshot.
/// 服务状态持有调度器，运行时持有临时快照。
///
/// # Returns
/// 返回值
/// Returns success after atomic Hook replacement.
/// 在原子 Hook 替换后返回成功。
fn synchronize_transient_hooks(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
) -> Result<(), String> {
    synchronize_service_vulcan_plugin_hooks(state, runtime.clone(), &runtime.snapshot())
}

/// Closes owner-scoped MCP admission for one plugin before its disable transaction can await.
/// 在停用事务进入异步等待前关闭指定插件的所有者范围 MCP 准入。
fn begin_plugin_owner_mcp_drain(
    state: &SharedServiceBridgeState,
    plugin_id: &str,
) -> Result<(), String> {
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to lock the service state: {error}"))?;
    let browser_manager = guard.browser_manager.clone();
    let mcp_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|agent_runtime| agent_runtime.extension_generation.mcp_runtime())
        .ok_or_else(|| String::from("The agent extension generation is not initialized yet."))?;
    drop(guard);
    mcp_runtime.begin_plugin_owner_drain(plugin_id)?;
    if plugin_id == "vulcan-browser" {
        // The MCP gate closes first; Core's latch then prevents any new transition while the
        // async disable barrier waits for work that was already admitted.
        // MCP 闸门先关闭；Core 锁存位随后阻止新转换，异步停用屏障再等待此前已准入的任务。
        browser_manager.begin_ai_drain();
    }
    Ok(())
}

/// Builds a ready global partition from an operation's frozen descriptors without publishing it.
/// 根据操作固定的描述符构建就绪全局分区，不发布它。
/// State supplies current access, runtime supplies package materialization, and identity/generation/descriptors fix the candidate.
/// state 提供当前访问，runtime 提供插件包物化，身份、代次及描述符固定候选。
/// Returns an unpublished matching access surface or the original readiness error.
/// 返回未发布的匹配访问面，或原始就绪错误。
async fn prepare_transition_mcp(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    plugin_id: &str,
    generation: u64,
    descriptors: &[vulcan_plugin_runtime::VulcanPluginMcpServerDescriptor],
) -> Result<McpRuntimeAccess, String> {
    // Capture once before awaiting so a candidate cannot combine extension generations.
    // 等待前仅捕获一次，避免候选混用扩展代次。
    let current_mcp_runtime = state
        .lock()
        .map_err(|error| format!("Failed to lock the service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|bridge| bridge.extension_generation.mcp_runtime())
        .ok_or_else(|| String::from("The agent extension generation is not initialized yet."))?;
    // Resolve credentials only through the exact descriptor binding.
    // 仅通过精确描述符绑定解析凭据。
    let secret_store = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
    // Materialization retains the descriptor's immutable package identity.
    // 物化保留描述符的不可变插件包身份。
    let server_configs =
        plugin_mcp_server_configs_for_descriptors(runtime, descriptors, &secret_store)?;
    if server_configs.is_empty() {
        Ok(remove_plugin_mcp_runtime_partition(
            &current_mcp_runtime,
            plugin_id,
        ))
    } else {
        replace_plugin_mcp_runtime_partition(
            &current_mcp_runtime,
            plugin_id,
            generation,
            server_configs,
        )
        .await
    }
}

/// Stops an unpublished candidate before reverse lifecycle effects can run.
/// 在反向生命周期副作用运行之前停止未发布候选。
/// The exact candidate and plugin select resources; failures retain their original receipts.
/// 精确候选及插件选择资源；失败时保留原回执。
/// Returns cleanup diagnostics without shutting down any other plugin.
/// 返回清理诊断，不关闭其他任何插件。
async fn drain_prepared_transition_mcp(
    candidate: &McpRuntimeAccess,
    plugin_id: &str,
) -> CleanupEvidence {
    if let Err(error) = candidate.begin_plugin_mcp_drain(plugin_id) {
        return CleanupEvidence::additional(vec![error]);
    }
    match candidate
        .wait_plugin_mcp_drain(
            plugin_id,
            AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
        )
        .await
    {
        Ok(()) => CleanupEvidence::default(),
        Err(error) => {
            CleanupEvidence::additional(vec![format!("Candidate MCP cleanup failed: {error}")])
        }
    }
}

/// Closes owner, browser and global admission for the exact plugin without affecting other plugins.
/// 关闭精确插件的所有者、浏览器及全局入场，不影响其他插件。
/// Receives service state and plugin identity; returns a failure while preserving existing closures.
/// 接收服务状态及插件身份；失败时保留已经关闭的入场。
fn begin_plugin_mcp_drain(state: &SharedServiceBridgeState, plugin_id: &str) -> Result<(), String> {
    begin_plugin_owner_mcp_drain(state, plugin_id)?;
    // The serialized management operation keeps this published access authoritative.
    // 串行化管理操作保持此发布访问的权威性。
    let mcp_runtime = state
        .lock()
        .map_err(|error| format!("Failed to lock the service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|bridge| bridge.extension_generation.mcp_runtime())
        .ok_or_else(|| String::from("The agent extension generation is not initialized yet."))?;
    mcp_runtime.begin_plugin_mcp_drain(plugin_id)
}

/// Waits for original owner and global resources with one service drain budget, keeping admission closed.
/// 使用同一服务排空预算等待原所有者与全局资源，保持入场关闭。
/// Receives the service state and plugin identity; reports release failure without removing evidence.
/// 接收服务状态及插件身份；报告释放失败，不移除证据。
async fn wait_plugin_mcp_drain(
    state: &SharedServiceBridgeState,
    plugin_id: &str,
) -> Result<(), String> {
    // Capture shared access without holding service state across asynchronous cleanup.
    // 捕获共享访问，不跨异步清理持有服务状态。
    let mcp_runtime = state
        .lock()
        .map_err(|error| format!("Failed to lock the service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|bridge| bridge.extension_generation.mcp_runtime())
        .ok_or_else(|| String::from("The agent extension generation is not initialized yet."))?;
    mcp_runtime
        .wait_plugin_mcp_drain(
            plugin_id,
            AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
        )
        .await
}

/// Reopens owner and browser admission only after the original resource scopes have released.
/// 仅在原资源范围释放之后重新开放所有者及浏览器入场。
/// Receives the service state and plugin identity; errors retain closed admission.
/// 接收服务状态及插件身份；发生错误时保持入场关闭。
async fn finish_plugin_owner_mcp_drain(
    state: &SharedServiceBridgeState,
    plugin_id: &str,
) -> Result<(), String> {
    // End the synchronous lock scope before awaiting any original resource.
    // 等待任何原资源之前结束同步锁作用域。
    let (browser_manager, mcp_runtime) = {
        // This guard exists only within the synchronous capture scope.
        // 此锁守卫仅存在于同步捕获作用域内。
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock the service state: {error}"))?;
        // Preserve the exact access whose original resource receipts will be observed.
        // 保留将要观察其原资源回执的精确访问。
        let mcp_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|bridge| bridge.extension_generation.mcp_runtime())
            .ok_or_else(|| {
                String::from("The agent extension generation is not initialized yet.")
            })?;
        (guard.browser_manager.clone(), mcp_runtime)
    };
    mcp_runtime
        .wait_plugin_owner_drain(
            plugin_id,
            AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
        )
        .await?;
    mcp_runtime.finish_plugin_owner_drain(plugin_id)?;
    if plugin_id == "vulcan-browser" {
        // Publish Core admission only after the exact owner MCP runtime has reopened.
        // 仅在精确所有者 MCP 运行时重新开放后发布 Core 准入。
        browser_manager.finish_ai_drain();
    }
    Ok(())
}

/// Restores the previous catalog and Hook projection from immutable transaction facts.
/// 根据不可变事务事实恢复原始目录与 Hook 投影。
///
/// # Parameters
/// 参数
/// Service state, runtime, transition, and cleanup diagnostics define the restored projection.
/// 服务状态、运行时、转换与清理诊断共同定义恢复投影。
///
/// # Returns
/// 返回值
/// Returns success after runtime publication and Hook synchronization both complete.
/// 在运行时发布与 Hook 同步全部完成后返回成功。
fn restore_previous_projection(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
    cleanup_failures: CleanupEvidence,
) -> Result<(), String> {
    finish_transition_and_sync(
        state,
        runtime,
        transition,
        &transition.previous_policy,
        cleanup_failures,
    )
}

/// Finishes one transient state and synchronizes the resulting Hook surface.
/// 完成单个临时状态并同步产生的 Hook 能力面。
///
/// # Parameters
/// 参数
/// Service state, runtime, transition, policy, and diagnostics define final publication.
/// 服务状态、运行时、转换、策略与诊断共同定义最终发布。
///
/// # Returns
/// 返回值
/// Returns success after catalog and Hook publication both complete.
/// 在目录与 Hook 发布全部完成后返回成功。
fn finish_transition_and_sync(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
    policy: &VulcanPluginPolicySnapshot,
    cleanup_failures: CleanupEvidence,
) -> Result<(), String> {
    // catalog is the immutable post-transition surface supplied to Hook replacement.
    // catalog 是提供给 Hook 替换的不可变转换后能力面。
    let catalog = finish_transition(runtime, transition, policy, cleanup_failures)?;
    synchronize_service_vulcan_plugin_hooks(state, runtime.clone(), &catalog)
}

/// Atomically publishes one plugin catalog and its matching MCP access generation, then retires only the replaced plugin leases.
/// 原子发布单个插件目录及其匹配的 MCP 访问代次，随后仅退休被替换插件的租约。
///
/// # Parameters
/// 参数
/// `state` owns the publication slot, `plugin_id` selects leases to retire, `catalog` is the exact finalized snapshot, and `mcp_runtime` is its prepared matching access surface.
/// `state` 持有发布槽，`plugin_id` 选择需要退休的租约，`catalog` 是精确最终快照，`mcp_runtime` 是与其匹配且已准备的访问面。
///
/// # Returns
/// 返回值
/// Returns success after one lock-protected pointer replacement; old leases drain for at most thirty seconds.
/// 在单次锁保护指针替换后返回成功；旧租约最多排空三十秒。
pub(super) fn publish_extension_generation(
    state: &SharedServiceBridgeState,
    plugin_id: &str,
    catalog: Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
    mcp_runtime: McpRuntimeAccess,
) -> Result<(), String> {
    publish_extension_generation_with_current_mcp(state, plugin_id, catalog, |_| Ok(mcp_runtime))
}

/// Publish the committed credential `catalog` together with the ready `replacement` of the plugin's global servers.
/// 将已提交凭据 `catalog` 与插件全局服务器的已就绪 `replacement` 一同发布。
/// The plugin must still be enabled at the same generation in the current full pointer; errors precede replacement.
/// 插件在当前完整指针中须仍以相同代次启用；错误先于替换发生。
pub(super) fn publish_credential_extension_generation(
    state: &SharedServiceBridgeState,
    plugin_id: &str,
    catalog: Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
    replacement: &vulcan_agent_runtime::mcp::RetainedPluginMcpReplacement,
) -> Result<(), String> {
    let mut guard = state.lock().map_err(|error| error.to_string())?;
    crate::service_state_access::require_plugin_publication_shutdown_admission(&guard)?;
    crate::service_state_access::require_plugin_update_full_publication_admission(&guard, None)?;
    let current_catalog = guard
        .agent_runtime
        .as_ref()
        .and_then(|runtime| runtime.extension_generation.plugin_catalog());
    // The committed edit changes only credentials, so routing must be the one the candidate started against.
    // 已提交编辑只改变凭据，因此路由必须是候选启动时所针对的路由。
    let committed = catalog.plugins.get(plugin_id).ok_or_else(|| {
        String::from("Credential publication plugin is absent from its committed catalog.")
    })?;
    if !current_catalog.as_deref().is_some_and(|current| {
        current.service_epoch == catalog.service_epoch
            && current.plugins.get(plugin_id).is_some_and(|published| {
                published.generation == committed.generation
                    && published.runtime_state
                        == vulcan_plugin_runtime::VulcanPluginRuntimeState::Enabled
            })
    }) {
        return Err(String::from(
            "Credential publication plugin routing changed during readiness.",
        ));
    }
    let bridge = guard
        .agent_runtime
        .as_mut()
        .ok_or_else(|| String::from("The agent extension generation is not initialized yet."))?;
    let expected = Arc::clone(&bridge.extension_generation);
    let current = expected.mcp_runtime();
    let access = replacement.publication_access(&current)?;
    let next = bridge.prepare_plugin_extension_generation(catalog, access)?;
    replacement.publish_prepared(&mut bridge.extension_generation, &expected, next, || {})?;
    // Retire displaced native owners only after the native guards are released.
    // 仅在原生守卫释放后退役被替换原生所有者。
    drop(guard);
    current.retire_plugin_partitions_after(
        plugin_id,
        AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
    );
    Ok(())
}

/// Publish a disabled plugin while preserving every other partition from the actual current service generation.
/// 发布停用插件，同时保留实际当前服务代次中的全部其他分区。
/// state owns the sole pointer; plugin_id selects only the removed partition; catalog is the exact disabled snapshot.
/// state 持有唯一指针；plugin_id 仅选择要移除的分区；catalog 是精确停用快照。
/// Returns confirmed replacement or an explicit error before replacement, including shutdown admission rejection.
/// 返回已确认替换，或替换前的明确错误，包括关闭准入拒绝。
pub(super) fn publish_disabled_plugin_extension_generation(
    state: &SharedServiceBridgeState,
    plugin_id: &str,
    catalog: Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
) -> Result<(), String> {
    publish_extension_generation_with_current_mcp(state, plugin_id, catalog, |current| {
        Ok(remove_plugin_mcp_runtime_partition(current, plugin_id))
    })
}

/// Prepare and replace the complete extension using its current MCP surface under the sole service publication lock.
/// 在唯一服务发布锁内，使用当前 MCP 暴露面准备并替换完整扩展。
/// state owns the pointer; plugin_id identifies retired leases; catalog supplies exact routes; prepare_mcp merges current partitions.
/// state 持有指针；plugin_id 标识退休租约；catalog 提供精确路由；prepare_mcp 合并当前分区。
/// The synchronous closure must not await, reenter service state, or execute plugin business.
/// 同步闭包不得等待、重入服务状态或执行插件业务。
/// Returns success after one pointer replacement; any preparation or shutdown error preserves the original pointer.
/// 单次指针替换后返回成功；任何准备或关闭错误均保留原指针。
pub(super) fn publish_extension_generation_with_current_mcp(
    state: &SharedServiceBridgeState,
    plugin_id: &str,
    catalog: Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
    prepare_mcp: impl FnOnce(&McpRuntimeAccess) -> Result<McpRuntimeAccess, String>,
) -> Result<(), String> {
    // The service lock publishes catalog, MCP access, and plugin conversation handlers together.
    // 服务锁共同发布目录、MCP 访问面及插件会话处理器。
    let mut guard = state
        .lock()
        .map_err(|error| format!("Failed to lock the service state: {error}"))?;
    // Check shutdown admission under the same lock that replaces the extension pointer.
    // 在替换扩展指针的同一锁内检查关闭准入。
    crate::service_state_access::require_plugin_publication_shutdown_admission(&guard)?;
    crate::service_state_access::require_plugin_update_full_publication_admission(&guard, None)?;
    // This bridge owns the single extension pointer consumed by new executions.
    // 本桥持有新执行消费的唯一扩展指针。
    let agent_runtime = guard
        .agent_runtime
        .as_mut()
        .ok_or_else(|| String::from("The agent extension generation is not initialized yet."))?;
    // Capture the actual original extension only after acquiring its publication lock.
    // 仅在取得其发布锁后捕获实际原扩展。
    let expected = Arc::clone(&agent_runtime.extension_generation);
    // The actual original extension also keeps displaced resources alive until adjacent confirmation completes.
    // 实际原扩展也使被替换资源存续，直至相邻确认完成。
    let current_mcp = expected.mcp_runtime();
    // The caller replaces only its own partition while preserving unrelated current owners.
    // 调用方仅替换自己的分区，同时保留当前无关所有者。
    let mcp_runtime = prepare_mcp(&current_mcp)?;
    // Validate all handlers before replacing the complete published pointer.
    // 替换完整已发布指针前校验全部处理器。
    let next = agent_runtime.prepare_plugin_extension_generation(catalog, mcp_runtime)?;
    vulcan_agent_runtime::agent_runtime::extension_generation::ExtensionGeneration::publish_prepared(
        &mut agent_runtime.extension_generation,
        &expected,
        next,
        |_| {},
    )?;
    // Retirement can acquire native observation locks only after leaving the sole service publication lock.
    // 仅离开唯一服务发布锁之后，退休才可取得原生观察锁。
    drop(guard);
    current_mcp.retire_plugin_partitions_after(
        plugin_id,
        AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
    );
    Ok(())
}

#[cfg(test)]
pub(in crate::skills_services) mod generation_tests;

/// Finishes one runtime transition from an immutable policy snapshot.
/// 根据不可变策略快照完成单个运行时转换。
///
/// # Parameters
/// 参数
/// Runtime, transition, policy, and cleanup diagnostics define the exact catalog.
/// 运行时、转换、策略与清理诊断共同定义精确目录。
///
/// # Returns
/// 返回值
/// Returns the newly published immutable catalog snapshot.
/// 返回新发布的不可变目录快照。
fn finish_transition(
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
    policy: &VulcanPluginPolicySnapshot,
    cleanup_failures: CleanupEvidence,
) -> Result<Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>, String> {
    runtime
        .finish_lifecycle_transition(
            &transition.plugin_id,
            &policy.disabled_plugin_ids,
            &policy.enabled_plugin_ids,
            cleanup_failures.additional_failures(),
        )
        .map_err(|error| error.to_string())
}

/// Appends rollback diagnostics without hiding the primary failure.
/// 追加回滚诊断且不隐藏主失败。
///
/// # Parameters
/// 参数
/// `primary_error` is the triggering failure and `rollback_result` reports compensation.
/// `primary_error` 是触发失败，`rollback_result` 报告补偿结果。
///
/// # Returns
/// 返回值
/// Returns the primary error alone after successful rollback, otherwise both diagnostics.
/// 回滚成功时仅返回主错误，否则返回两类诊断。
fn append_rollback_error(primary_error: String, rollback_result: Result<(), String>) -> String {
    match rollback_result {
        Ok(()) => primary_error,
        Err(rollback_error) => format!("{primary_error}; rollback failed: {rollback_error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies request booleans are converted once into exact domain targets.
    /// 验证请求布尔值仅转换一次并形成精确领域目标。
    #[test]
    fn target_state_conversion_is_exact() {
        assert_eq!(
            VulcanPluginTargetState::from_enabled(true),
            VulcanPluginTargetState::Enabled
        );
        assert_eq!(
            VulcanPluginTargetState::from_enabled(false),
            VulcanPluginTargetState::Disabled
        );
        assert!(VulcanPluginTargetState::Enabled.is_enabled());
        assert!(!VulcanPluginTargetState::Disabled.is_enabled());
    }

    /// Verifies rollback formatting preserves the primary failure and adds only real rollback errors.
    /// 验证回滚格式化保留主失败，且仅追加真实回滚错误。
    #[test]
    fn rollback_error_formatting_preserves_both_failure_layers() {
        assert_eq!(
            append_rollback_error(String::from("主失败"), Ok(())),
            "主失败"
        );
        assert_eq!(
            append_rollback_error(String::from("主失败"), Err(String::from("补偿失败"))),
            "主失败; rollback failed: 补偿失败"
        );
    }
}
