//! Transactional Vulcan Plugin enable and disable orchestration.
//! Vulcan 插件启用与停用事务编排。

use super::*;

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
        return Err(String::from("Vulcan 插件标识不能为空。"));
    }
    // management_guard serializes catalog, lifecycle, persistence, and hook ownership transfer.
    // management_guard 串行化目录、生命周期、持久化与 Hook 所有权交接。
    let _management_guard = lock_luaskills_management_mutation().await;
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
        .ok_or_else(|| format!("未发现 Vulcan 插件：{stable_plugin_id}"))?;
    // default_enabled is the manifest baseline required by configuration normalization.
    // default_enabled 是配置归一化所需的清单基线。
    let default_enabled = current_plugin
        .manifest
        .as_ref()
        .map(|manifest| manifest.default_enabled)
        .ok_or_else(|| format!("Vulcan 插件 `{stable_plugin_id}` 缺少有效清单，无法变更状态"))?;
    // previous_enabled is the exact effective state restored by every failed transaction.
    // previous_enabled 是每个失败事务都要恢复的精确生效状态。
    let previous_enabled = current_plugin.desired_enabled;
    // plugin_generation is the installed content generation retained across this enable/disable policy transition.
    // plugin_generation 是在本次启停策略转换期间保持不变的已安装内容代次。
    let plugin_generation = current_plugin.generation;
    // target converts the transport boolean once so later branches carry domain meaning.
    // target 仅转换一次传输布尔值，使后续分支携带领域语义。
    let target = VulcanPluginTargetState::from_enabled(enabled);
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
    );
    // requires_cleanup_retry identifies retained resources from an earlier disable attempt.
    // requires_cleanup_retry 标识早期停用尝试遗留的资源。
    let requires_cleanup_retry = current_plugin.runtime_state
        == vulcan_plugin_runtime::VulcanPluginRuntimeState::DisabledWithCleanupWarning;
    if !matches!(
        current_plugin.availability,
        vulcan_plugin_runtime::VulcanPluginAvailability::Enabled
            | vulcan_plugin_runtime::VulcanPluginAvailability::Disabled
    ) {
        return Err(format!(
            "Vulcan 插件 `{stable_plugin_id}` 当前不可管理：{}",
            current_plugin.diagnostics.join("；")
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
        .ok_or_else(|| format!("目标目录未发现 Vulcan 插件：{stable_plugin_id}"))?;
    // expected_availability is the only availability accepted for the requested target.
    // expected_availability 是请求目标唯一允许的可用性。
    let expected_availability = if target.is_enabled() {
        vulcan_plugin_runtime::VulcanPluginAvailability::Enabled
    } else {
        vulcan_plugin_runtime::VulcanPluginAvailability::Disabled
    };
    if target_plugin.availability != expected_availability {
        return Err(format!(
            "Vulcan 插件 `{stable_plugin_id}` 无法进入请求状态：{}",
            target_plugin.diagnostics.join("；")
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
            prepare_disable_transition(&state, &runtime, &transition)?;
            None
        }
    };
    if let Err(error) = persist_transition_target(&state, &transition).await {
        // rollback_result preserves runtime, lifecycle, and Hook compensation diagnostics.
        // rollback_result 保留运行时、生命周期与 Hook 补偿诊断。
        let rollback_result = rollback_prepared_transition(&state, &runtime, &transition).await;
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
                        "Vulcan 插件 `{}` 缺少已准备的 MCP 访问代次。",
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
                    "Vulcan 插件 `{}` 重新启用前清理失败：{error}",
                    transition.plugin_id
                )
            })?;
        if !cleanup_report.failures.is_empty() {
            finish_transition_and_sync(
                state,
                runtime,
                transition,
                &transition.previous_policy,
                cleanup_report.failures.clone(),
            )?;
            return Err(format!(
                "Vulcan 插件 `{}` 仍有未清理资源：{}",
                transition.plugin_id,
                cleanup_report.failures.join("；")
            ));
        }
    }
    validate_service_vulcan_plugin_hooks(state, target_hooks).map_err(|error| {
        format!(
            "Vulcan 插件 `{}` Hook 声明无效：{error}",
            transition.plugin_id
        )
    })?;
    runtime
        .begin_enabling(&transition.plugin_id)
        .map_err(|error| error.to_string())?;
    synchronize_transient_hooks(state, runtime).map_err(|error| {
        append_rollback_error(
            format!("Vulcan Plugin hook 同步失败：{error}"),
            restore_previous_projection(state, runtime, transition, Vec::new()),
        )
    })?;
    // preparation_runtime moves blocking environment work away from the async executor.
    // preparation_runtime 把阻塞式环境准备移出异步执行器。
    let preparation_runtime = runtime.clone();
    // preparation_plugin_id preserves the exact transaction identity in the blocking task.
    // preparation_plugin_id 在阻塞任务中保留精确事务标识。
    let preparation_plugin_id = transition.plugin_id.clone();
    if let Err(error) = run_extension_management_blocking("prepare plugin environment", move || {
        preparation_runtime
            .prepare_plugin_environment(&preparation_plugin_id)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
    .await
    {
        return Err(append_rollback_error(
            format!(
                "Vulcan 插件 `{}` 环境准备失败：{error}",
                transition.plugin_id
            ),
            restore_previous_projection(state, runtime, transition, Vec::new()),
        ));
    }
    if let Err(error) = runtime
        .invoke_lifecycle(
            &transition.plugin_id,
            vulcan_plugin_runtime::VulcanPluginLifecycleEvent::Enable,
        )
        .await
    {
        // cleanup_failures captures partial callback side effects through the declared disable callback.
        // cleanup_failures 通过声明的停用回调捕获部分回调副作用。
        let cleanup_failures = compensate_failed_enable(runtime, &transition.plugin_id).await;
        return Err(append_rollback_error(
            format!(
                "Vulcan 插件 `{}` 启用回调失败：{error}",
                transition.plugin_id
            ),
            restore_previous_projection(state, runtime, transition, cleanup_failures),
        ));
    }
    if let Err(error) = runtime.probe_plugin(&transition.plugin_id).await {
        // cleanup_failures compensates a successful callback whose runtime probe failed.
        // cleanup_failures 补偿回调成功但运行时探测失败的情况。
        let cleanup_failures = compensate_failed_enable(runtime, &transition.plugin_id).await;
        return Err(append_rollback_error(
            format!(
                "Vulcan 插件 `{}` 启用探测失败：{error}",
                transition.plugin_id
            ),
            restore_previous_projection(state, runtime, transition, cleanup_failures),
        ));
    }
    let candidate_result = async {
        // currentMcpRuntime is captured once before asynchronous candidate startup so the replacement cannot mix service generations.
        // currentMcpRuntime 在异步候选启动前仅捕获一次，避免替换过程混用服务代次。
        let current_mcp_runtime = state
            .lock()
            .map_err(|error| format!("锁定服务状态失败：{error}"))?
            .agent_runtime
            .as_ref()
            .map(|agent_runtime| agent_runtime.extension_generation.mcp_runtime())
            .ok_or_else(|| String::from("Agent 扩展代次尚未初始化。"))?;
        let secret_store = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
        let server_configs = plugin_mcp_server_configs_for_descriptors(
            runtime,
            &transition.target_mcp_servers,
            &secret_store,
        )?;
        if server_configs.is_empty() {
            Ok(remove_plugin_mcp_runtime_partition(
                &current_mcp_runtime,
                &transition.plugin_id,
            ))
        } else {
            replace_plugin_mcp_runtime_partition(
                &current_mcp_runtime,
                &transition.plugin_id,
                transition.plugin_generation,
                server_configs,
            )
            .await
        }
    }
    .await;
    let candidate_mcp_runtime = match candidate_result {
        Ok(candidate) => candidate,
        Err(error) => {
            let cleanup_failures = compensate_failed_enable(runtime, &transition.plugin_id).await;
            return Err(append_rollback_error(
                format!(
                    "Vulcan 插件 `{}` MCP 就绪检查失败：{error}",
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
fn prepare_disable_transition(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
) -> Result<(), String> {
    runtime
        .begin_draining(&transition.plugin_id)
        .map_err(|error| error.to_string())?;
    synchronize_transient_hooks(state, runtime).map_err(|error| {
        append_rollback_error(
            format!("Vulcan Plugin hook 同步失败：{error}"),
            restore_previous_projection(state, runtime, transition, Vec::new()),
        )
    })
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
        "保存 Vulcan 插件状态失败",
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
    let final_catalog =
        match finish_transition(runtime, transition, &transition.target_policy, Vec::new()) {
            Ok(catalog) => catalog,
            Err(error) => {
                // cleanup_failures compensates while the snapshot still authorizes EnableRollback.
                // cleanup_failures 在快照仍允许 EnableRollback 时完成补偿。
                let cleanup_failures =
                    compensate_failed_enable(runtime, &transition.plugin_id).await;
                // persistence_rollback restores the durable baseline before catalog restoration.
                // persistence_rollback 在目录恢复前还原持久化基线。
                let persistence_rollback = persist_transition_enabled_state(
                    state,
                    transition,
                    transition.previous_enabled,
                    "回滚 Vulcan 插件状态失败",
                )
                .await;
                if let Err(rollback_error) = persistence_rollback {
                    return Err(format!(
                        "Vulcan Plugin 启用最终发布失败：{error}；配置回滚失败：{rollback_error}"
                    ));
                }
                return Err(append_rollback_error(
                    format!("Vulcan Plugin 启用最终发布失败：{error}"),
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
            .map_err(|error| format!("插件 Hook 运行时预热失败：{error}"))
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
            candidate_mcp_runtime,
        )
        .err()
    } else {
        activation_error
    };
    if let Some(error) = activation_error {
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
            format!("Vulcan Plugin hook 最终激活失败：{error}"),
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
    // cleanup_failures retains non-fatal drain and lifecycle diagnostics in the final runtime state.
    // cleanup_failures 在最终运行时状态中保留非致命排空与生命周期诊断。
    let mut cleanup_failures = match runtime.drain_plugin_sessions(&transition.plugin_id).await {
        Ok(report) => report.failures,
        Err(error) => vec![error.to_string()],
    };
    if let Err(error) = runtime
        .invoke_lifecycle(
            &transition.plugin_id,
            vulcan_plugin_runtime::VulcanPluginLifecycleEvent::Disable,
        )
        .await
    {
        cleanup_failures.push(format!("插件停用生命周期回调失败：{error}"));
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
                format!("Vulcan Plugin 停用最终发布失败：{error}"),
                rollback_result,
            ));
        }
    };
    let current_mcp_runtime = state
        .lock()
        .map_err(|error| format!("锁定服务状态失败：{error}"))?
        .agent_runtime
        .as_ref()
        .map(|agent_runtime| agent_runtime.extension_generation.mcp_runtime())
        .ok_or_else(|| String::from("Agent 扩展代次尚未初始化。"))?;
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
            format!("Vulcan Plugin 停用扩展代次发布失败：{error}"),
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
///
/// # Returns
/// 返回值
/// Returns success after lifecycle and Hook state return to the previous projection.
/// 在生命周期与 Hook 状态返回原始投影后返回成功。
async fn rollback_prepared_transition(
    state: &SharedServiceBridgeState,
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    transition: &VulcanPluginTransition,
) -> Result<(), String> {
    match transition.target {
        VulcanPluginTargetState::Enabled => {
            // cleanup_failures compensates the completed enable callback before projection rollback.
            // cleanup_failures 在投影回滚前补偿已完成的启用回调。
            let cleanup_failures = compensate_failed_enable(runtime, &transition.plugin_id).await;
            restore_previous_projection(state, runtime, transition, cleanup_failures)
        }
        VulcanPluginTargetState::Disabled => {
            // No disable callback ran before persistence, so only catalog and hooks need restoration.
            // 持久化前尚未执行停用回调，因此只需恢复目录与 Hook。
            restore_previous_projection(state, runtime, transition, Vec::new())
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
        "回滚 Vulcan 插件状态失败",
    )
    .await?;
    runtime
        .begin_draining(&transition.plugin_id)
        .map_err(|error| error.to_string())?;
    synchronize_transient_hooks(state, runtime)?;
    // cleanup_failures records whether the real disable compensation was complete.
    // cleanup_failures 记录真实停用补偿是否完整。
    let mut cleanup_failures = match runtime.drain_plugin_sessions(&transition.plugin_id).await {
        Ok(report) => report.failures,
        Err(error) => vec![error.to_string()],
    };
    if let Err(error) = runtime
        .invoke_lifecycle(
            &transition.plugin_id,
            vulcan_plugin_runtime::VulcanPluginLifecycleEvent::Disable,
        )
        .await
    {
        cleanup_failures.push(format!("插件启用回滚的停用回调失败：{error}"));
    }
    // incomplete_cleanup is reported even though the disabled warning projection is published.
    // incomplete_cleanup 即使已经发布停用告警投影也必须上报。
    let incomplete_cleanup = cleanup_failures.clone();
    finish_transition_and_sync(
        state,
        runtime,
        transition,
        restored_policy,
        cleanup_failures,
    )?;
    if incomplete_cleanup.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "插件启用回滚清理不完整：{}",
            incomplete_cleanup.join("；")
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
    cleanup_failures: Vec<String>,
    finalized: bool,
) -> Result<(), String> {
    if !transition.previous_enabled {
        // A cleanup retry started disabled, so rollback must never run an enable lifecycle.
        // 清理重试开始时就是停用状态，因此回滚绝不能执行启用生命周期。
        return restore_previous_projection(state, runtime, transition, cleanup_failures);
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
        let cleanup_failures = compensate_failed_enable(runtime, &transition.plugin_id).await;
        return Err(append_rollback_error(
            format!("停用回滚的启用回调失败：{error}"),
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
        let cleanup_failures = compensate_failed_enable(runtime, &transition.plugin_id).await;
        return Err(append_rollback_error(
            format!("停用回滚的启用探测失败：{error}"),
            finish_transition_and_sync(
                state,
                runtime,
                transition,
                &transition.target_policy,
                cleanup_failures,
            ),
        ));
    }
    if let Err(error) = persist_transition_enabled_state(
        state,
        transition,
        transition.previous_enabled,
        "回滚 Vulcan 插件状态失败",
    )
    .await
    {
        let cleanup_failures = compensate_failed_enable(runtime, &transition.plugin_id).await;
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
    let recovered_catalog =
        match finish_transition(runtime, transition, &transition.previous_policy, Vec::new()) {
            Ok(catalog) => catalog,
            Err(error) => {
                // cleanup_failures compensates the reverse enable while it remains in Enabling.
                // cleanup_failures 在反向启用仍处于 Enabling 时进行补偿。
                let cleanup_failures =
                    compensate_failed_enable(runtime, &transition.plugin_id).await;
                persist_transition_enabled_state(
                    state,
                    transition,
                    transition.target.is_enabled(),
                    "恢复 Vulcan 插件停用状态失败",
                )
                .await?;
                return Err(append_rollback_error(
                    format!("停用回滚的启用发布失败：{error}"),
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
            .map_err(|error| format!("停用回滚的 Hook 运行时预热失败：{error}"))
            .err()
    } else {
        activation_error
    };
    if let Some(error) = activation_error {
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
/// `runtime` owns the enabling transition and `plugin_id` selects the exact plugin.
/// `runtime` 持有启用转换，`plugin_id` 选择精确插件。
///
/// # Returns
/// 返回值
/// Returns every cleanup failure that prevents complete compensation.
/// 返回所有阻止完整补偿的清理失败。
async fn compensate_failed_enable(
    runtime: &vulcan_plugin_runtime::VulcanPluginRuntimeManager,
    plugin_id: &str,
) -> Vec<String> {
    // failures combines lease drain and lifecycle compensation diagnostics.
    // failures 合并租约排空与生命周期补偿诊断。
    let mut failures = match runtime.drain_plugin_sessions(plugin_id).await {
        Ok(report) => report.failures,
        Err(error) => vec![error.to_string()],
    };
    if let Err(error) = runtime
        .invoke_lifecycle(
            plugin_id,
            vulcan_plugin_runtime::VulcanPluginLifecycleEvent::EnableRollback,
        )
        .await
    {
        failures.push(format!("插件启用补偿回调失败：{error}"));
    }
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
    cleanup_failures: Vec<String>,
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
    cleanup_failures: Vec<String>,
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
    let mut guard = state
        .lock()
        .map_err(|error| format!("锁定服务状态失败：{error}"))?;
    let agent_runtime = guard
        .agent_runtime
        .as_mut()
        .ok_or_else(|| String::from("Agent 扩展代次尚未初始化。"))?;
    // retiredMcpRuntime remains owned by old in-flight ExtensionGeneration arcs until natural release or forced drain expiry.
    // retiredMcpRuntime 由旧执行中的 ExtensionGeneration Arc 保留，直到自然释放或强制排空到期。
    let retired_mcp_runtime = agent_runtime.extension_generation.mcp_runtime();
    let revision = agent_runtime
        .extension_generation
        .revision()
        .saturating_add(1);
    agent_runtime.extension_generation = Arc::new(ExtensionGeneration::new(
        revision,
        Some(catalog),
        mcp_runtime,
    ));
    retired_mcp_runtime
        .retire_plugin_partitions_after(plugin_id, std::time::Duration::from_secs(30));
    Ok(())
}

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
    cleanup_failures: Vec<String>,
) -> Result<Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>, String> {
    runtime
        .finish_lifecycle_transition(
            &transition.plugin_id,
            &policy.disabled_plugin_ids,
            &policy.enabled_plugin_ids,
            cleanup_failures,
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
        Err(rollback_error) => format!("{primary_error}；回滚失败：{rollback_error}"),
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
            "主失败；回滚失败：补偿失败"
        );
    }
}
