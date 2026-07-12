//! LuaSkills and AgentSkills service handlers.
//! LuaSkills 与 AgentSkills 服务处理器。

use super::*;

/// Serializes user-agent catalog file mutations across concurrent service requests.
/// 串行化并发服务请求之间的用户智能体目录文件变更。
static SUBAGENT_CATALOG_MUTATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Serializes LuaSkills lifecycle requests through catalog storage and domain projection.
/// 将 LuaSkills 生命周期请求串行化到目录存储与领域投影完成为止。
static LUASKILLS_MANAGEMENT_MUTATION_LOCK: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

/// Serializes Agent Skills package, catalog, configuration, and projection mutations.
/// 串行化 Agent Skills 技能包、目录、配置与投影变更。
static AGENT_SKILLS_CATALOG_MUTATION_LOCK: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

/// Acquires the process-wide user-agent catalog mutation guard.
/// 获取进程级用户智能体目录变更守卫。
///
/// # Returns
/// 返回值
/// A guard that keeps catalog discovery, file mutation, and state reprojection in one transaction.
/// 返回一个守卫，使目录发现、文件变更与状态重投影保持在同一事务中。
fn lock_subagent_catalog_mutation() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    SUBAGENT_CATALOG_MUTATION_LOCK
        .lock()
        .map_err(|error| format!("failed to lock subagent catalog mutation: {error}"))
}

/// Acquires the process-wide LuaSkills lifecycle transaction guard.
/// 获取进程级 LuaSkills 生命周期事务守卫。
///
/// # Returns
/// 返回值
/// An asynchronous guard retained until runtime mutation, catalog storage, and projection finish.
/// 返回一个异步守卫，并保持到运行时变更、目录存储与投影全部结束。
async fn lock_luaskills_management_mutation() -> tokio::sync::MutexGuard<'static, ()> {
    LUASKILLS_MANAGEMENT_MUTATION_LOCK.lock().await
}

/// Acquires the complete Agent Skills catalog transaction guard.
/// 获取完整 Agent Skills 目录事务守卫。
pub(crate) async fn lock_agent_skills_catalog_mutation() -> tokio::sync::MutexGuard<'static, ()> {
    AGENT_SKILLS_CATALOG_MUTATION_LOCK.lock().await
}

pub(crate) async fn run_luaskills_management_blocking<T, F>(
    label: &'static str,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    spawn_service_blocking_task(operation)
        .await
        .map_err(|error| format!("LuaSkills 管理操作 `{label}` 任务异常结束：{error}"))?
}

/// Refreshes the LuaSkills catalog used by the WebUI skill page.
/// 刷新 WebUI 技能页面使用的 LuaSkills 目录。
pub(crate) async fn service_refresh_luaskills_catalog(
    state: SharedServiceBridgeState,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Keep the runtime refresh and its returned catalog projection in one ordered transaction.
    // 将运行时刷新及其返回的目录投影保持在同一个有序事务中。
    let _management_guard = lock_luaskills_management_mutation().await;
    let handle = service_luaskills_handle(&state)?;
    let provider_configuration = provider_configuration_from_state(&state)?;
    let system_plugin_runtime = service_system_plugin_runtime(&state)?;
    let preview_catalog = vulcan_plugin_runtime::load_system_plugin_catalog(
        system_plugin_runtime.system_plugin_root(),
        &provider_configuration.luaskills.disabled_system_plugins,
        &provider_configuration.luaskills.enabled_system_plugins,
        0,
    );
    let preview_hooks = preview_catalog
        .enabled_hooks
        .values()
        .cloned()
        .collect::<Vec<_>>();
    validate_service_system_plugin_hooks(&state, &preview_hooks)
        .map_err(|error| format!("System Plugin hook 刷新预检失败：{error}"))?;
    let catalog = run_luaskills_management_blocking("refresh catalog", move || {
        vulcan_service_runtime::luaskills_bridge::refresh_luaskills_catalog(&handle)
    })
    .await?;
    store_luaskills_catalog(&state, catalog)?;
    let system_plugin_catalog = system_plugin_runtime
        .refresh_catalog(
            &provider_configuration.luaskills.disabled_system_plugins,
            &provider_configuration.luaskills.enabled_system_plugins,
        )
        .map_err(|error| error.to_string())?;
    synchronize_service_system_plugin_hooks(&state, system_plugin_runtime, &system_plugin_catalog)?;
    refresh_luaskills_local_state(&state).await
}

/// Installs one LuaSkill package into the USER layer and refreshes the WebUI snapshot.
/// 将单个 LuaSkill 包安装到 USER 层并刷新 WebUI 快照。
pub(crate) async fn service_install_luaskill(
    state: SharedServiceBridgeState,
    skill_id: Option<String>,
    source: String,
    source_type: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Keep installation and its returned catalog projection in one ordered transaction.
    // 将安装及其返回的目录投影保持在同一个有序事务中。
    let _management_guard = lock_luaskills_management_mutation().await;
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("install skill", move || {
        vulcan_service_runtime::luaskills_bridge::install_luaskill(
            &handle,
            skill_id,
            source,
            source_type,
        )
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_luaskills_local_state(&state).await
}

/// Updates one managed LuaSkill package and refreshes the WebUI snapshot.
/// 更新单个受管 LuaSkill 包并刷新 WebUI 快照。
pub(crate) async fn service_update_luaskill(
    state: SharedServiceBridgeState,
    skill_id: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Keep update and its returned catalog projection in one ordered transaction.
    // 将更新及其返回的目录投影保持在同一个有序事务中。
    let _management_guard = lock_luaskills_management_mutation().await;
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("update skill", move || {
        vulcan_service_runtime::luaskills_bridge::update_luaskill(&handle, skill_id)
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_luaskills_local_state(&state).await
}

/// Enables or disables one LuaSkill package and refreshes the WebUI snapshot.
/// 启用或停用单个 LuaSkill 包并刷新 WebUI 快照。
pub(crate) async fn service_set_luaskill_enabled(
    state: SharedServiceBridgeState,
    skill_id: String,
    enabled: bool,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Keep lifecycle state mutation and its returned catalog projection in one ordered transaction.
    // 将生命周期状态变更及其返回的目录投影保持在同一个有序事务中。
    let _management_guard = lock_luaskills_management_mutation().await;
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("set skill enabled", move || {
        vulcan_service_runtime::luaskills_bridge::set_luaskill_enabled(&handle, skill_id, enabled)
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_luaskills_local_state(&state).await
}

/// Enables or disables one known host-owned LuaSkills System Plugin.
/// 启用或停用单个已知的宿主自有 LuaSkills System Plugin。
///
/// # Parameters
/// 参数
/// `state` retains service configuration, `plugin_id` identifies the plugin, and `enabled` is the requested state.
/// `state` 保留服务配置，`plugin_id` 标识插件，`enabled` 是请求状态。
///
/// # Returns
/// 返回值
/// A revisioned LuaSkills domain projection after persistence succeeds.
/// 持久化成功后的带版本 LuaSkills 领域投影。
pub(crate) async fn service_set_luaskills_system_plugin_enabled(
    state: SharedServiceBridgeState,
    plugin_id: String,
    enabled: bool,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // stable_plugin_id is the exact persisted and projected System Plugin identifier.
    // stable_plugin_id 是持久化与投影使用的精确 System Plugin 标识。
    let stable_plugin_id = plugin_id.trim();
    if stable_plugin_id.is_empty() {
        return Err(String::from("LuaSkills System Plugin 标识不能为空。"));
    }
    // management_guard serializes this switch with package lifecycle and tool registration mutations.
    // management_guard 使该开关与包生命周期及工具注册变更保持串行。
    let _management_guard = lock_luaskills_management_mutation().await;
    let provider_configuration = provider_configuration_from_state(&state)?;
    let system_plugin_runtime = service_system_plugin_runtime(&state)?;
    let plugin_catalog = system_plugin_runtime
        .refresh_catalog(
            &provider_configuration.luaskills.disabled_system_plugins,
            &provider_configuration.luaskills.enabled_system_plugins,
        )
        .map_err(|error| error.to_string())?;
    let plugin = plugin_catalog
        .plugins
        .get(stable_plugin_id)
        .ok_or_else(|| format!("未发现 LuaSkills System Plugin：{stable_plugin_id}"))?;
    // plugin_default_enabled records the manifest baseline so persistence can store only an explicit override.
    // plugin_default_enabled 记录清单基线，使持久化层只保存显式覆盖。
    let plugin_default_enabled = plugin
        .manifest
        .as_ref()
        .map(|manifest| manifest.default_enabled)
        .ok_or_else(|| {
            format!("LuaSkills System Plugin `{stable_plugin_id}` 缺少有效清单，无法变更状态")
        })?;
    // previous_enabled is the exact effective state restored if any later transaction phase fails.
    // previous_enabled 是后续任一事务阶段失败时需要恢复的精确有效状态。
    let previous_enabled = plugin.desired_enabled;
    // already_settled avoids probing or draining an unchanged healthy lifecycle state.
    // already_settled 避免对未变化且健康的生命周期状态重复探测或排空。
    let already_settled = matches!(
        (enabled, plugin.runtime_state),
        (
            true,
            vulcan_plugin_runtime::SystemPluginRuntimeState::Enabled
        ) | (
            false,
            vulcan_plugin_runtime::SystemPluginRuntimeState::Disabled
        )
    );
    // requires_cleanup_retry prevents re-enabling a plugin while an older retained lease still consumes capacity.
    // requires_cleanup_retry 防止旧保留租约仍占用容量时重新启用插件。
    let requires_cleanup_retry = plugin.runtime_state
        == vulcan_plugin_runtime::SystemPluginRuntimeState::DisabledWithCleanupWarning;
    if !matches!(
        plugin.availability,
        vulcan_plugin_runtime::SystemPluginAvailability::Enabled
            | vulcan_plugin_runtime::SystemPluginAvailability::Disabled
    ) {
        return Err(format!(
            "LuaSkills System Plugin `{stable_plugin_id}` 当前不可管理：{}",
            plugin.diagnostics.join("；")
        ));
    }
    if previous_enabled == enabled && already_settled {
        return refresh_luaskills_local_state(&state).await;
    }
    // prospective_luaskills applies the requested override without mutating persisted configuration.
    // prospective_luaskills 在不修改持久化配置的前提下应用请求覆盖。
    let mut prospective_luaskills = provider_configuration.luaskills.clone();
    prospective_luaskills.set_system_plugin_enabled_with_default(
        stable_plugin_id,
        enabled,
        plugin_default_enabled,
    );
    // prospective_catalog validates global identifiers, package assets, schemas, hooks, and target state before persistence.
    // prospective_catalog 在持久化前校验全局标识、包资产、Schema、Hook 与目标状态。
    let prospective_catalog = vulcan_plugin_runtime::load_system_plugin_catalog(
        system_plugin_runtime.system_plugin_root(),
        &prospective_luaskills.disabled_system_plugins,
        &prospective_luaskills.enabled_system_plugins,
        0,
    );
    let prospective_plugin = prospective_catalog
        .plugins
        .get(stable_plugin_id)
        .ok_or_else(|| format!("目标目录未发现 LuaSkills System Plugin：{stable_plugin_id}"))?;
    let expected_availability = if enabled {
        vulcan_plugin_runtime::SystemPluginAvailability::Enabled
    } else {
        vulcan_plugin_runtime::SystemPluginAvailability::Disabled
    };
    if prospective_plugin.availability != expected_availability {
        return Err(format!(
            "LuaSkills System Plugin `{stable_plugin_id}` 无法进入请求状态：{}",
            prospective_plugin.diagnostics.join("；")
        ));
    }
    if enabled {
        if requires_cleanup_retry {
            let cleanup_report = system_plugin_runtime
                .drain_plugin_sessions(stable_plugin_id)
                .await
                .map_err(|error| {
                    format!(
                        "LuaSkills System Plugin `{stable_plugin_id}` 重新启用前清理失败：{error}"
                    )
                })?;
            if !cleanup_report.failures.is_empty() {
                let warning_catalog = system_plugin_runtime
                    .finish_lifecycle_transition(
                        stable_plugin_id,
                        &provider_configuration.luaskills.disabled_system_plugins,
                        &provider_configuration.luaskills.enabled_system_plugins,
                        cleanup_report.failures.clone(),
                    )
                    .map_err(|error| error.to_string())?;
                let _ = synchronize_service_system_plugin_hooks(
                    &state,
                    system_plugin_runtime.clone(),
                    &warning_catalog,
                );
                return Err(format!(
                    "LuaSkills System Plugin `{stable_plugin_id}` 仍有未清理资源：{}",
                    cleanup_report.failures.join("；")
                ));
            }
        }
        validate_service_system_plugin_hooks(&state, &prospective_plugin.hooks).map_err(
            |error| format!("LuaSkills System Plugin `{stable_plugin_id}` Hook 声明无效：{error}"),
        )?;
        system_plugin_runtime
            .begin_enabling(stable_plugin_id)
            .map_err(|error| error.to_string())?;
        if let Err(error) = synchronize_service_system_plugin_hooks(
            &state,
            system_plugin_runtime.clone(),
            &system_plugin_runtime.snapshot(),
        ) {
            let rollback_catalog = system_plugin_runtime
                .finish_lifecycle_transition(
                    stable_plugin_id,
                    &provider_configuration.luaskills.disabled_system_plugins,
                    &provider_configuration.luaskills.enabled_system_plugins,
                    Vec::new(),
                )
                .map_err(|rollback_error| {
                    format!("{error}; System Plugin hook 回滚失败：{rollback_error}")
                })?;
            let _ = synchronize_service_system_plugin_hooks(
                &state,
                system_plugin_runtime.clone(),
                &rollback_catalog,
            );
            return Err(format!("System Plugin hook 同步失败：{error}"));
        }
        if let Err(error) = system_plugin_runtime.probe_plugin(stable_plugin_id).await {
            let cleanup_failures = match system_plugin_runtime
                .drain_plugin_sessions(stable_plugin_id)
                .await
            {
                Ok(report) => report.failures,
                Err(cleanup_error) => vec![cleanup_error.to_string()],
            };
            if let Ok(rollback_catalog) = system_plugin_runtime.finish_lifecycle_transition(
                stable_plugin_id,
                &provider_configuration.luaskills.disabled_system_plugins,
                &provider_configuration.luaskills.enabled_system_plugins,
                cleanup_failures,
            ) {
                let _ = synchronize_service_system_plugin_hooks(
                    &state,
                    system_plugin_runtime.clone(),
                    &rollback_catalog,
                );
            }
            return Err(format!(
                "LuaSkills System Plugin `{stable_plugin_id}` 启用探测失败：{error}"
            ));
        }
    } else {
        system_plugin_runtime
            .begin_draining(stable_plugin_id)
            .map_err(|error| error.to_string())?;
        if let Err(error) = synchronize_service_system_plugin_hooks(
            &state,
            system_plugin_runtime.clone(),
            &system_plugin_runtime.snapshot(),
        ) {
            let rollback_catalog = system_plugin_runtime
                .finish_lifecycle_transition(
                    stable_plugin_id,
                    &provider_configuration.luaskills.disabled_system_plugins,
                    &provider_configuration.luaskills.enabled_system_plugins,
                    Vec::new(),
                )
                .map_err(|rollback_error| {
                    format!("{error}; System Plugin hook 回滚失败：{rollback_error}")
                })?;
            let _ = synchronize_service_system_plugin_hooks(
                &state,
                system_plugin_runtime.clone(),
                &rollback_catalog,
            );
            return Err(format!("System Plugin hook 同步失败：{error}"));
        }
    }
    let persistence_result = mutate_provider_configuration(
        &state,
        "保存 LuaSkills System Plugin 状态失败",
        |configuration| {
            configuration
                .luaskills
                .set_system_plugin_enabled_with_default(
                    stable_plugin_id,
                    enabled,
                    plugin_default_enabled,
                );
            Ok(())
        },
        |_, _| (),
    )
    .await;
    if let Err(error) = persistence_result {
        if let Ok(rollback_catalog) = system_plugin_runtime.finish_lifecycle_transition(
            stable_plugin_id,
            &provider_configuration.luaskills.disabled_system_plugins,
            &provider_configuration.luaskills.enabled_system_plugins,
            Vec::new(),
        ) {
            let _ = synchronize_service_system_plugin_hooks(
                &state,
                system_plugin_runtime.clone(),
                &rollback_catalog,
            );
        }
        return Err(error);
    }
    let cleanup_failures = if enabled {
        Vec::new()
    } else {
        match system_plugin_runtime
            .drain_plugin_sessions(stable_plugin_id)
            .await
        {
            Ok(report) => report.failures,
            Err(error) => vec![error.to_string()],
        }
    };
    let effective_configuration = provider_configuration_from_state(&state)?;
    let final_catalog = system_plugin_runtime
        .finish_lifecycle_transition(
            stable_plugin_id,
            &effective_configuration.luaskills.disabled_system_plugins,
            &effective_configuration.luaskills.enabled_system_plugins,
            cleanup_failures,
        )
        .map_err(|error| error.to_string())?;
    if let Err(sync_error) = synchronize_service_system_plugin_hooks(
        &state,
        system_plugin_runtime.clone(),
        &final_catalog,
    ) {
        let rollback_persistence = mutate_provider_configuration(
            &state,
            "回滚 LuaSkills System Plugin 状态失败",
            |configuration| {
                configuration
                    .luaskills
                    .set_system_plugin_enabled_with_default(
                        stable_plugin_id,
                        previous_enabled,
                        plugin_default_enabled,
                    );
                Ok(())
            },
            |_, _| (),
        )
        .await;
        if let Err(rollback_error) = rollback_persistence {
            return Err(format!(
                "System Plugin hook 最终同步失败：{sync_error}；配置回滚失败：{rollback_error}"
            ));
        }
        let rollback_configuration = provider_configuration_from_state(&state)?;
        let rollback_catalog = system_plugin_runtime
            .finish_lifecycle_transition(
                stable_plugin_id,
                &rollback_configuration.luaskills.disabled_system_plugins,
                &rollback_configuration.luaskills.enabled_system_plugins,
                Vec::new(),
            )
            .map_err(|rollback_error| {
                format!(
                    "System Plugin hook 最终同步失败：{sync_error}；运行时回滚失败：{rollback_error}"
                )
            })?;
        synchronize_service_system_plugin_hooks(&state, system_plugin_runtime, &rollback_catalog)
            .map_err(|rollback_error| {
            format!(
                "System Plugin hook 最终同步失败：{sync_error}；Hook 回滚失败：{rollback_error}"
            )
        })?;
        return Err(format!("System Plugin hook 最终同步失败：{sync_error}"));
    }
    refresh_luaskills_local_state(&state).await
}

/// Uninstalls one ordinary LuaSkill package and refreshes the WebUI snapshot.
/// 卸载单个普通 LuaSkill 包并刷新 WebUI 快照。
pub(crate) async fn service_uninstall_luaskill(
    state: SharedServiceBridgeState,
    skill_id: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Keep uninstall and its returned catalog projection in one ordered transaction.
    // 将卸载及其返回的目录投影保持在同一个有序事务中。
    let _management_guard = lock_luaskills_management_mutation().await;
    let handle = service_luaskills_handle(&state)?;
    let (message, catalog) = run_luaskills_management_blocking("uninstall skill", move || {
        vulcan_service_runtime::luaskills_bridge::uninstall_luaskill(&handle, skill_id)
    })
    .await?;
    store_luaskills_catalog_with_log(&state, catalog, message)?;
    refresh_luaskills_local_state(&state).await
}

/// Persists one LuaSkill tool registration state and refreshes the WebUI snapshot.
/// 持久化单个 LuaSkill 工具注册状态并刷新 WebUI 快照。
pub(crate) async fn service_set_luaskill_tool_registration_state(
    state: SharedServiceBridgeState,
    skill_id: String,
    local_name: String,
    registration_state: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Keep tool registration ordered with lifecycle catalog mutations so every response projects one coherent domain.
    // 使工具注册与生命周期目录变更保持有序，让每个响应都投影同一个一致领域。
    let _management_guard = lock_luaskills_management_mutation().await;
    let next_state = parse_luaskill_tool_registration_state(&registration_state)?;
    mutate_provider_configuration(
        &state,
        "保存 LuaSkills 工具注册失败",
        |configuration| {
            configuration
                .luaskills
                .set_tool_registration_state(&skill_id, &local_name, next_state);
            Ok(())
        },
        |_guard, _configuration| (),
    )
    .await?;
    refresh_luaskills_local_state(&state).await
}

/// Refreshes only the Agent Skills domain and returns its committed revision.
/// 仅刷新 Agent Skills 领域并返回其已提交版本。
pub(crate) async fn service_refresh_agent_skills_catalog(
    state: SharedServiceBridgeState,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    refresh_agent_skills_local_state(&state).await
}

/// Opens the configured user Agent Skills directory and returns the refreshed catalog.
/// 打开配置后的用户 Agent Skills 目录并返回刷新后的目录。
pub(crate) async fn service_open_agent_skills_user_directory(
    state: SharedServiceBridgeState,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    let (user_root, _) = configured_agent_skill_roots(&state)?;
    std::fs::create_dir_all(&user_root)
        .map_err(|error| format!("创建 Agent Skills 用户目录失败：{error}"))?;
    let update = refresh_agent_skills_local_state(&state).await?;
    shell_host_from_state(&state)?.open_path(&user_root.to_string_lossy())?;
    Ok(update)
}

/// Saves one canonical user Agent Skill package and its per-skill state.
/// 保存一个规范用户 Agent Skill 技能包及其单技能状态。
pub(crate) async fn service_save_agent_skill(
    state: SharedServiceBridgeState,
    request: AgentSkillSaveRequest,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    let (user_root, system_root) = configured_agent_skill_roots(&state)?;
    let name = require_canonical_agent_skill_name(&request.name)?.to_string();
    let original_name = request
        .original_name
        .as_deref()
        .map(require_canonical_agent_skill_name)
        .transpose()?
        .map(str::to_string);
    let source_skill_key = request.source_skill_key.as_deref();
    // catalog captures the exact existing source record used to preserve or derive provenance.
    // catalog 捕获用于保留或推导来源的精确现有来源记录。
    let catalog =
        vulcan_prompt_skills::load_prompt_skills_from_roots(system_root.clone(), user_root.clone());
    // origin is host-assigned and cannot be forged by the WebUI payload.
    // origin 由宿主分配，WebUI 载荷无法伪造。
    let origin = managed_agent_skill_origin(&catalog, original_name.as_deref(), &name);
    // resource_source_dir is resolved from the exact source-aware catalog record selected by the user.
    // resource_source_dir 从用户选中的精确来源感知目录记录解析。
    let resource_source_dir = source_skill_key
        .map(|skill_key| {
            exact_agent_skill_source_record(&catalog, skill_key).map(|skill| skill.base_dir.clone())
        })
        .transpose()?;
    // enabled is captured before request fields move into the package document.
    // enabled 在请求字段移入技能包文档前被捕获。
    let enabled = request.enabled;
    let input = vulcan_prompt_skills::PromptSkillPackageInput {
        original_name: original_name.clone(),
        resource_source_dir,
        name: name.clone(),
        description: request.description,
        license: request.license,
        compatibility: request.compatibility,
        metadata: request.metadata,
        body: request.body,
        interface: vulcan_prompt_skills::PromptSkillInterfaceMetadata {
            display_name: request.interface.display_name,
            short_description: request.interface.short_description,
            emoji: request.interface.emoji,
            color: request.interface.color,
            locales: request
                .interface
                .locales
                .into_iter()
                .map(|(locale, value)| {
                    (
                        locale,
                        vulcan_prompt_skills::PromptSkillLocaleMetadata {
                            display_name: value.display_name,
                            short_description: value.short_description,
                        },
                    )
                })
                .collect(),
        },
        catalog: vulcan_prompt_skills::PromptSkillCatalogMetadata {
            category_id: request.category_id,
            tags: request.tags,
        },
        origin,
    };
    // configuration_before captures exact states required for transactional rollback.
    // configuration_before 捕获事务回滚所需的精确状态。
    let configuration_before = provider_configuration_from_state(&state)?;
    // previous_target_state is the original target-name override.
    // previous_target_state 是原目标名称覆盖状态。
    let previous_target_state = configuration_before
        .prompt_skills
        .skill_overrides
        .get(&name)
        .copied();
    // previous_original_state is the original renamed-source override.
    // previous_original_state 是原重命名来源覆盖状态。
    let previous_original_state = original_name.as_deref().and_then(|value| {
        configuration_before
            .prompt_skills
            .skill_overrides
            .get(value)
            .copied()
    });
    let state_override = if enabled {
        PersistedPromptSkillState::Enabled
    } else {
        PersistedPromptSkillState::Disabled
    };
    let configuration_original_name = original_name.clone();
    let configuration_target_name = name.clone();
    mutate_provider_configuration(
        &state,
        "保存 Agent Skill 状态失败",
        move |configuration| {
            if let Some(original_name) = configuration_original_name
                .as_deref()
                .filter(|value| *value != configuration_target_name)
            {
                configuration
                    .prompt_skills
                    .set_skill_state(original_name, None);
            }
            configuration
                .prompt_skills
                .set_skill_state(&configuration_target_name, Some(state_override));
            Ok(())
        },
        |_guard, _configuration| (),
    )
    .await?;
    if let Err(file_error) = vulcan_prompt_skills::save_user_prompt_skill_package(&user_root, input)
    {
        // rollback_original_name retains the exact source key restored after a file failure.
        // rollback_original_name 保留文件失败后恢复的精确来源键。
        let rollback_original_name = original_name.clone();
        // rollback_target_name retains the exact target key restored after a file failure.
        // rollback_target_name 保留文件失败后恢复的精确目标键。
        let rollback_target_name = name.clone();
        let rollback_result = mutate_provider_configuration(
            &state,
            "回滚 Agent Skill 状态失败",
            move |configuration| {
                configuration
                    .prompt_skills
                    .set_skill_state(&rollback_target_name, previous_target_state);
                if let Some(original_name) = rollback_original_name
                    .as_deref()
                    .filter(|value| *value != rollback_target_name)
                {
                    configuration
                        .prompt_skills
                        .set_skill_state(original_name, previous_original_state);
                }
                Ok(())
            },
            |_guard, _configuration| (),
        )
        .await;
        return match rollback_result {
            Ok(()) => Err(file_error),
            Err(rollback_error) => Err(format!(
                "{file_error}; Agent Skill configuration rollback also failed: {rollback_error}"
            )),
        };
    }
    refresh_agent_skills_local_state(&state).await
}

/// Deletes one user Agent Skill package and removes its obsolete per-skill state.
/// 删除一个用户 Agent Skill 技能包并移除其过期单技能状态。
pub(crate) async fn service_delete_agent_skill(
    state: SharedServiceBridgeState,
    name: String,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    let name = require_canonical_agent_skill_name(&name)?.to_string();
    let (user_root, _) = configured_agent_skill_roots(&state)?;
    // previous_state captures the exact override restored when filesystem deletion fails.
    // previous_state 捕获文件系统删除失败时恢复的精确覆盖状态。
    let previous_state = provider_configuration_from_state(&state)?
        .prompt_skills
        .skill_overrides
        .get(&name)
        .copied();
    let state_name = name.clone();
    mutate_provider_configuration(
        &state,
        "删除 Agent Skill 状态失败",
        move |configuration| {
            configuration
                .prompt_skills
                .set_skill_state(&state_name, None);
            Ok(())
        },
        |_guard, _configuration| (),
    )
    .await?;
    if let Err(file_error) =
        vulcan_prompt_skills::delete_user_prompt_skill_package(&user_root, &name)
    {
        let rollback_name = name.clone();
        let rollback_result = mutate_provider_configuration(
            &state,
            "回滚 Agent Skill 删除状态失败",
            move |configuration| {
                configuration
                    .prompt_skills
                    .set_skill_state(&rollback_name, previous_state);
                Ok(())
            },
            |_guard, _configuration| (),
        )
        .await;
        return match rollback_result {
            Ok(()) => Err(file_error),
            Err(rollback_error) => Err(format!(
                "{file_error}; Agent Skill deletion rollback also failed: {rollback_error}"
            )),
        };
    }
    refresh_agent_skills_local_state(&state).await
}

/// Restores a system Agent Skill by deleting its exact user override package.
/// 通过删除精确用户覆盖技能包恢复系统 Agent Skill。
pub(crate) async fn service_restore_agent_skill_default(
    state: SharedServiceBridgeState,
    name: String,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    let name = require_canonical_agent_skill_name(&name)?.to_string();
    let (user_root, system_root) = configured_agent_skill_roots(&state)?;
    if !system_root
        .join(&name)
        .join(vulcan_prompt_skills::SKILL_FILE_NAME)
        .is_file()
    {
        return Err(format!("Agent Skill `{name}` 没有可恢复的系统版本。"));
    }
    vulcan_prompt_skills::delete_user_prompt_skill_package(&user_root, &name)?;
    refresh_agent_skills_local_state(&state).await
}

/// Saves one user-defined logical Agent Skills category.
/// 保存一个用户定义逻辑 Agent Skills 分类。
pub(crate) async fn service_save_agent_skill_category(
    state: SharedServiceBridgeState,
    request: AgentSkillCategorySaveRequest,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    let (user_root, _) = configured_agent_skill_roots(&state)?;
    vulcan_prompt_skills::save_user_prompt_skill_category(
        &user_root,
        vulcan_prompt_skills::PromptSkillCategory {
            category_id: request.category_id,
            name: request.name,
            description: request.description,
            emoji: request.emoji,
            color: request.color,
            order: request.order,
            builtin: false,
            locales: request
                .locales
                .into_iter()
                .map(|(locale, value)| {
                    (
                        locale,
                        vulcan_prompt_skills::PromptSkillCategoryLocale {
                            name: value.name,
                            description: value.description,
                        },
                    )
                })
                .collect(),
        },
    )?;
    refresh_agent_skills_local_state(&state).await
}

/// Deletes one user-defined Agent Skills category and unassigns its skills.
/// 删除一个用户定义 Agent Skills 分类并解除其技能归属。
pub(crate) async fn service_delete_agent_skill_category(
    state: SharedServiceBridgeState,
    category_id: String,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    let (user_root, system_root) = configured_agent_skill_roots(&state)?;
    vulcan_prompt_skills::delete_user_prompt_skill_category(
        &system_root,
        &user_root,
        &category_id,
    )?;
    refresh_agent_skills_local_state(&state).await
}

/// Assigns one effective Agent Skill to one exact logical category.
/// 将一个有效 Agent Skill 归入一个精确逻辑分类。
pub(crate) async fn service_set_agent_skill_category(
    state: SharedServiceBridgeState,
    name: String,
    category_id: String,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    let (user_root, system_root) = configured_agent_skill_roots(&state)?;
    vulcan_prompt_skills::set_user_prompt_skill_category(
        &system_root,
        &user_root,
        &name,
        &category_id,
    )?;
    refresh_agent_skills_local_state(&state).await
}

/// Resolves the exact configured user and system Agent Skills roots.
/// 解析精确配置的用户与系统 Agent Skills 根目录。
fn configured_agent_skill_roots(
    state: &SharedServiceBridgeState,
) -> Result<(PathBuf, PathBuf), String> {
    let configuration = provider_configuration_from_state(state)?;
    let runtime_config = vulcan_agent_runtime::skills::prompt::global_config_from_persisted(
        &configuration.prompt_skills,
    );
    Ok((
        vulcan_prompt_skills::configured_user_skill_root(&runtime_config),
        vulcan_prompt_skills::configured_system_skill_root(&runtime_config),
    ))
}

/// Resolves auditable provenance for one managed user package save.
/// 为一次受管用户技能包保存解析可审计来源信息。
fn managed_agent_skill_origin(
    catalog: &vulcan_prompt_skills::PromptSkillCatalog,
    original_name: Option<&str>,
    target_name: &str,
) -> vulcan_prompt_skills::PromptSkillOriginMetadata {
    // timestamp is the canonical UTC audit time for this exact save.
    // timestamp 是本次精确保存的规范 UTC 审计时间。
    let timestamp = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("current UTC time must format as RFC 3339");
    // existing_user is the only source whose provenance survives an edit or rename.
    // existing_user 是编辑或重命名时唯一保留来源信息的来源。
    let existing_user = original_name.and_then(|name| {
        catalog.records.iter().find(|skill| {
            skill.scope == vulcan_prompt_skills::PromptSkillScope::User && skill.name == name
        })
    });
    if let Some(existing_user) = existing_user {
        let mut origin = existing_user.origin.clone();
        origin.updated_at = Some(timestamp);
        return origin;
    }
    // system_source identifies an explicit same-name user override creation.
    // system_source 标识一次明确的同名用户覆盖创建。
    let system_source = catalog.records.iter().any(|skill| {
        skill.scope == vulcan_prompt_skills::PromptSkillScope::System && skill.name == target_name
    });
    if system_source {
        return vulcan_prompt_skills::PromptSkillOriginMetadata {
            kind: String::from("local-override"),
            label: Some(String::from("User override")),
            managed: false,
            installed_at: Some(timestamp.clone()),
            updated_at: Some(timestamp),
            ..vulcan_prompt_skills::PromptSkillOriginMetadata::default()
        };
    }
    vulcan_prompt_skills::PromptSkillOriginMetadata {
        kind: String::from("local-created"),
        label: Some(String::from("User-created")),
        managed: false,
        installed_at: Some(timestamp.clone()),
        updated_at: Some(timestamp),
        ..vulcan_prompt_skills::PromptSkillOriginMetadata::default()
    }
}

/// Refreshes the agent catalog and updates the shared task runtime snapshot.
/// 刷新智能体目录并更新共享任务运行时快照。
pub(crate) async fn service_refresh_subagent_catalog(
    state: SharedServiceBridgeState,
) -> Result<SubagentCatalogUpdate, String> {
    // Prevent refresh from committing a catalog loaded between another request's file mutation and projection.
    // 防止刷新提交在其他请求文件变更与投影之间加载到的目录。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    refresh_subagent_catalog_local_state(&state)
}

/// Opens the user-owned subagent definition directory in the host file manager.
/// 在宿主文件管理器中打开用户拥有的智能体定义目录。
pub(crate) async fn service_open_subagent_user_directory(
    state: SharedServiceBridgeState,
) -> Result<SubagentCatalogUpdate, String> {
    let user_definition_root = user_subagent_definition_root();
    std::fs::create_dir_all(&user_definition_root).map_err(|error| {
        format!(
            "创建用户智能体定义目录 `{}` 失败：{error}",
            user_definition_root.display()
        )
    })?;
    // Keep the directory refresh ordered with all catalog file mutations before opening the projected path.
    // 在打开已投影路径前，使目录刷新与全部目录文件变更保持有序。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    let snapshot = refresh_subagent_catalog_local_state(&state)?;
    // Release catalog serialization before the external file-manager request, which does not mutate catalog state.
    // 在不变更目录状态的外部文件管理器请求前释放目录串行守卫。
    drop(_catalog_mutation_guard);
    shell_host_from_state(&state)?.open_path(&user_definition_root.to_string_lossy())?;
    Ok(snapshot)
}

/// Persists one agent definition enabled state through the sidecar configuration layer.
/// 通过 sidecar 配置层持久化单个智能体定义启停状态。
pub(crate) async fn service_set_subagent_definition_enabled(
    state: SharedServiceBridgeState,
    agent_id: String,
    enabled: bool,
) -> Result<SubagentCatalogUpdate, String> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Err(String::from("智能体定义标识不能为空。"));
    }
    // Keep the filesystem override and both retained runtime projections ordered as one catalog transaction.
    // 将文件系统覆盖与两份保留的运行时投影按同一个目录事务排序。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !guard
            .workbench
            .subagent_catalog
            .definitions
            .iter()
            .any(|definition| definition.agent_id == agent_id)
        {
            return Err(format!("未找到智能体定义：{agent_id}"));
        }
    }
    vulcan_task_session::write_user_subagent_enabled_override(
        user_subagent_root().as_path(),
        agent_id,
        enabled,
    )
    .map_err(|error| {
        format!(
            "写入智能体启停覆盖 `{}` 失败：{}",
            error.path.display(),
            error.message
        )
    })?;
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(definition) = guard
        .workbench
        .subagent_catalog
        .definitions
        .iter_mut()
        .find(|definition| definition.agent_id == agent_id)
    else {
        return Err(format!("未找到智能体定义：{agent_id}"));
    };
    definition.enabled = enabled;
    if let Some(runtime) = guard.agent_runtime.as_ref() {
        runtime
            .task_runtime
            .set_subagent_definition_enabled(agent_id, enabled)
            .map_err(|error| format!("更新运行时智能体启停状态失败：{error}"))?;
    }
    let notice = subagent_catalog_notice_from_summary(&guard.workbench.subagent_catalog);
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    guard.workbench.subagent_catalog.notice = notice;
    Ok(SubagentCatalogUpdate {
        revision: guard.workbench_generation,
        subagent_catalog: guard.workbench.subagent_catalog.clone(),
    })
}

/// Saves one user-authored agent definition into the user catalog.
/// 将单个用户编写的智能体定义保存到用户目录。
pub(crate) async fn service_save_subagent_definition(
    state: SharedServiceBridgeState,
    request: SubagentDefinitionSaveRequest,
) -> Result<SubagentCatalogUpdate, String> {
    let requested_agent_id = normalize_agent_file_name_input(&request.agent_id);
    if requested_agent_id.is_empty() {
        return Err(String::from("智能体标识不能为空。"));
    }
    let mode = vulcan_task_session::SubagentDefinitionMode::Expert;
    let target_package_id = normalized_optional_user_package_id(request.package_id.as_deref())?;
    let submitted_original_agent_id = request
        .original_agent_id
        .as_deref()
        .map(normalize_agent_file_name_input)
        .filter(|value| !value.is_empty());
    let original_agent_id = submitted_original_agent_id
        .as_deref()
        .unwrap_or(requested_agent_id.as_str());
    // Prevent concurrent save, delete, restore, and toggle requests from observing an intermediate catalog.
    // 防止并发保存、删除、恢复与启停请求观察到目录中间态。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    let catalog = load_current_subagent_catalog();
    let existing = submitted_original_agent_id
        .as_deref()
        .and_then(|original_agent_id| {
            catalog
                .definitions
                .iter()
                .find(|definition| definition.id == original_agent_id)
                .cloned()
        });
    let provider_configuration = provider_configuration_from_state(&state)?;
    let requested_provider_model = validate_subagent_provider_model_request_with_existing(
        &request,
        existing.as_ref(),
        &provider_configuration,
    )?;
    if let Some(existing_definition) = existing
        .as_ref()
        .filter(|definition| subagent_definition_is_system_backed(definition))
    {
        if requested_agent_id != original_agent_id {
            return Err(String::from("系统智能体不允许修改 agent_id。"));
        }
        if target_package_id.is_some() {
            return Err(String::from("系统智能体不允许转移到用户包。"));
        }
        let mut metadata_override = existing_definition.clone();
        apply_request_tool_policy(&mut metadata_override.tool_exposure, &request);
        apply_request_prompt_skill_policy(&mut metadata_override.prompt_skills, &request);
        metadata_override.provider_model = requested_provider_model;
        metadata_override.system_prompt_append = request
            .system_prompt_append
            .as_deref()
            .and_then(non_empty_request_text);
        metadata_override.enabled = existing_definition.enabled;
        vulcan_task_session::write_user_subagent_metadata_override(
            user_subagent_root().as_path(),
            &metadata_override,
        )
        .map_err(|error| {
            format!(
                "保存系统智能体 `{}` 的用户元数据覆盖失败：{}",
                existing_definition.id, error.message
            )
        })?;
        return refresh_subagent_catalog_local_state(&state);
    }
    let target_agent_id = target_agent_id_for_save(
        &requested_agent_id,
        existing.as_ref(),
        target_package_id.as_deref(),
    )?;
    let target_conflicts_with_other_definition = catalog
        .definitions
        .iter()
        .any(|definition| definition.id == target_agent_id && definition.id != original_agent_id);
    let new_definition_conflicts_with_existing = existing.is_none()
        && catalog
            .definitions
            .iter()
            .any(|definition| definition.id == target_agent_id);
    if target_conflicts_with_other_definition || new_definition_conflicts_with_existing {
        return Err(format!(
            "目标智能体 `{target_agent_id}` 已存在，请更换文件名或先编辑已有智能体。"
        ));
    }
    let user_file = user_subagent_definition_path(&target_agent_id)?;
    let mut definition = existing.unwrap_or_else(|| vulcan_task_session::SubagentDefinition {
        id: target_agent_id.clone(),
        name: target_agent_id.clone(),
        description: None,
        locales: std::collections::BTreeMap::new(),
        origin: None,
        emoji: String::from("🤖"),
        color: String::from("gray"),
        vibe: None,
        mode,
        system_prompt: None,
        system_prompt_append: None,
        provider_model: Default::default(),
        tool_exposure: Default::default(),
        prompt_skills: Default::default(),
        context_compaction: Default::default(),
        runtime_policy: Default::default(),
        max_depth: None,
        enabled: true,
        source: vulcan_task_session::SubagentDefinitionSource {
            scope: vulcan_task_session::SubagentDefinitionScope::User,
            file: user_file.clone(),
            sidecar_file: None,
            source_key: format!("user:{target_agent_id}"),
            category_id: vulcan_task_session::display_subagent_category_id(
                &target_agent_id,
                target_package_id.as_deref(),
            ),
            package_id: target_package_id.clone(),
            remote_url: None,
            override_of: None,
            source_hash: None,
            version: None,
            etag: None,
            last_modified: None,
            updated_at: None,
        },
        extra: serde_json::Value::Null,
    });
    let previous_agent_id = definition.id.clone();
    let previous_source = definition.source.clone();
    let previous_scope = definition.source.scope;
    definition.id = target_agent_id.clone();
    apply_request_tool_policy(&mut definition.tool_exposure, &request);
    apply_request_prompt_skill_policy(&mut definition.prompt_skills, &request);
    let request_locales = normalized_request_definition_locales(request.locales);
    definition.name = preferred_definition_locale_name(&request_locales)
        .or_else(|| non_empty_request_text(&request.name))
        .unwrap_or_else(|| target_agent_id.clone());
    definition.description =
        preferred_definition_locale_description(&request_locales).or_else(|| {
            request
                .description
                .as_deref()
                .and_then(non_empty_request_text)
        });
    definition.locales = request_locales;
    apply_defaulted_text_patch(request.emoji, &mut definition.emoji, "🤖");
    apply_defaulted_text_patch(request.color, &mut definition.color, "gray");
    apply_optional_text_patch(request.vibe, &mut definition.vibe);
    definition.mode = mode;
    definition.enabled = request.enabled;
    definition.system_prompt = request
        .system_prompt
        .as_deref()
        .and_then(non_empty_request_text);
    definition.system_prompt_append = request
        .system_prompt_append
        .as_deref()
        .and_then(non_empty_request_text);
    definition.provider_model = requested_provider_model;
    definition.tool_exposure.luaskills_overrides = request.luaskills_overrides;
    definition.tool_exposure.mcp_server_overrides = request.mcp_server_overrides;
    definition.source.scope = vulcan_task_session::SubagentDefinitionScope::User;
    definition.source.file = user_file;
    definition.source.sidecar_file = None;
    definition.source.source_key = format!("user:{target_agent_id}");
    definition.source.category_id = vulcan_task_session::display_subagent_category_id(
        &target_agent_id,
        target_package_id.as_deref(),
    );
    definition.source.package_id = target_package_id.clone();
    definition.source.updated_at = Some(current_unix_millis_text());
    definition.source.override_of = None;
    let written_user_file = vulcan_task_session::write_user_subagent_definition(
        user_subagent_root().as_path(),
        &definition,
    )
    .map_err(|error| {
        format!(
            "保存智能体定义 `{}` 失败：{}",
            error.path.display(),
            error.message
        )
    })?;
    remove_user_subagent_file_if_inside_root(&user_subagent_override_path(&target_agent_id)?)?;
    if previous_agent_id != target_agent_id
        && previous_scope != vulcan_task_session::SubagentDefinitionScope::System
    {
        remove_stale_user_subagent_files_for_source(
            &previous_agent_id,
            &previous_source,
            &written_user_file,
        )?;
    }
    refresh_subagent_catalog_local_state(&state)
}

/// Deletes one user-owned agent definition from the user catalog.
/// 从用户目录删除单个用户拥有的智能体定义。
pub(crate) async fn service_delete_subagent_definition(
    state: SharedServiceBridgeState,
    agent_id: String,
) -> Result<SubagentCatalogUpdate, String> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Err(String::from("智能体定义标识不能为空。"));
    }
    // Keep catalog discovery and deletion under the same process-wide mutation guard.
    // 在同一个进程级变更守卫下完成目录发现与删除。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    let catalog = load_current_subagent_catalog();
    let definition = catalog
        .definitions
        .iter()
        .find(|definition| definition.id == agent_id)
        .ok_or_else(|| format!("未找到智能体定义：{agent_id}"))?;
    if !matches!(
        definition.source.scope,
        vulcan_task_session::SubagentDefinitionScope::User
            | vulcan_task_session::SubagentDefinitionScope::Package
    ) || definition.source.override_of.is_some()
    {
        return Err(String::from("只有用户自定义智能体允许删除。"));
    }
    let delete_candidates = user_definition_delete_candidates(agent_id, definition)?;
    let mut deleted_any = false;
    for path in delete_candidates {
        if user_subagent_file_exists_inside_root(&path)? {
            remove_user_subagent_file_if_inside_root(&path)?;
            deleted_any = true;
        }
    }
    if !deleted_any {
        return Err(format!("未找到可删除的用户智能体文件：{agent_id}"));
    }
    refresh_subagent_catalog_local_state(&state)
}

/// Restores one system-backed agent definition by deleting its user override.
/// 通过删除用户覆盖来恢复单个系统集成智能体定义。
pub(crate) async fn service_restore_subagent_definition_default(
    state: SharedServiceBridgeState,
    agent_id: String,
) -> Result<SubagentCatalogUpdate, String> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return Err(String::from("智能体定义标识不能为空。"));
    }
    // Keep restore eligibility checks and override deletion in one serialized catalog transaction.
    // 将恢复资格检查与覆盖删除保持在同一个串行目录事务中。
    let _catalog_mutation_guard = lock_subagent_catalog_mutation()?;
    let catalog = load_current_subagent_catalog();
    let definition = catalog
        .definitions
        .iter()
        .find(|definition| definition.id == agent_id)
        .ok_or_else(|| format!("未找到智能体定义：{agent_id}"))?;
    let restore_candidates = restore_default_user_file_candidates(agent_id, definition)?;
    let has_user_layer = restore_candidates.iter().try_fold(false, |found, path| {
        user_subagent_file_exists_inside_root(path).map(|exists| found || exists)
    })?;
    let can_restore = has_user_layer
        && (system_subagent_definition_exists(agent_id)
            || definition
                .source
                .override_of
                .as_deref()
                .is_some_and(system_subagent_definition_exists));
    if !can_restore {
        return Err(format!(
            "当前智能体没有可恢复的系统或包默认定义：{agent_id}"
        ));
    }
    for path in restore_candidates {
        remove_user_subagent_file_if_inside_root(&path)?;
    }
    refresh_subagent_catalog_local_state(&state)
}

/// Persists one Agent Skill state and refreshes only the Agent Skills catalog.
/// 持久化单个 Agent Skill 状态并仅刷新 Agent Skills 目录。
pub(crate) async fn service_set_agent_skill_enabled(
    state: SharedServiceBridgeState,
    name: String,
    enabled: bool,
) -> Result<AgentSkillsCatalogUpdate, String> {
    let _catalog_guard = lock_agent_skills_catalog_mutation().await;
    let name = require_existing_agent_skill_name(&state, &name)?;
    let state_override = if enabled {
        PersistedPromptSkillState::Enabled
    } else {
        PersistedPromptSkillState::Disabled
    };
    mutate_provider_configuration(
        &state,
        "保存 Agent Skill 状态失败",
        |configuration| {
            configuration
                .prompt_skills
                .set_skill_state(&name, Some(state_override));
            Ok(())
        },
        |_guard, _configuration| (),
    )
    .await?;
    refresh_agent_skills_local_state(&state).await
}

/// Requires one exact canonical Agent Skills identity without rewriting caller input.
/// 要求一个精确的 Agent Skills 规范身份，不改写调用方输入。
fn require_canonical_agent_skill_name(name: &str) -> Result<&str, String> {
    if name.is_empty() || name != name.trim() {
        return Err(String::from(
            "Agent Skill name must already be canonical and cannot be empty or contain surrounding whitespace.",
        ));
    }
    Ok(name)
}

/// Resolves one exact source-aware Agent Skill record without effective-source substitution.
/// 解析一条精确的来源感知 Agent Skill 记录且不替换为有效来源。
fn exact_agent_skill_source_record<'catalog>(
    catalog: &'catalog vulcan_prompt_skills::PromptSkillCatalog,
    skill_key: &str,
) -> Result<&'catalog vulcan_prompt_skills::PromptSkill, String> {
    if skill_key.is_empty() || skill_key != skill_key.trim() {
        return Err(String::from(
            "Agent Skill source key must be exact and cannot be empty or contain surrounding whitespace.",
        ));
    }
    catalog
        .source_record(skill_key)
        .ok_or_else(|| format!("Agent Skill source record `{skill_key}` does not exist."))
}

/// Resolves one canonical name only when the current effective catalog contains it.
/// 仅当当前有效目录包含技能时解析其规范名称。
fn require_existing_agent_skill_name(
    state: &SharedServiceBridgeState,
    name: &str,
) -> Result<String, String> {
    let name = require_canonical_agent_skill_name(name)?;
    let (user_root, system_root) = configured_agent_skill_roots(state)?;
    let catalog = vulcan_prompt_skills::load_prompt_skills_from_roots(system_root, user_root);
    if catalog.skill(name).is_none() {
        return Err(format!("Agent Skill `{name}` does not exist."));
    }
    Ok(name.to_string())
}

/// Loads project-level Agent Skill overrides for the WebUI project settings modal.
/// 为 WebUI 项目设置弹窗加载项目级 Agent Skill 覆盖。
pub(crate) fn service_load_project_agent_skill_overrides(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<BTreeMap<String, String>, String> {
    let project_path = vmm_project_scope_path(&state, Some(&project_id))?;
    let project_configuration = load_project_configuration(&project_path);
    Ok(project_configuration
        .prompt_skills
        .skill_overrides
        .into_iter()
        .map(|(name, state)| {
            let value = if state.is_enabled() {
                "enabled"
            } else {
                "disabled"
            };
            (name, value.to_string())
        })
        .collect())
}

/// Persists one project-level Agent Skill tri-state override without rebuilding the full snapshot.
/// 持久化单个项目级 Agent Skill 三态覆盖，但不重建完整快照。
pub(crate) async fn service_set_agent_skill_project_state(
    state: SharedServiceBridgeState,
    project_id: String,
    name: String,
    state_key: String,
) -> Result<(), String> {
    let project_path = vmm_project_scope_path(&state, Some(&project_id))?;
    let stable_name = name.trim();
    if stable_name.is_empty() {
        return Err(String::from("项目级 Agent Skill 名称不能为空。"));
    }
    let state_override = parse_agent_skill_project_state(&state_key)?;
    mutate_project_configuration(
        &PathBuf::from(&project_path),
        "保存项目 Agent Skill 状态失败",
        |project_configuration| {
            match state_override {
                Some(state_override) => {
                    project_configuration
                        .prompt_skills
                        .skill_overrides
                        .insert(stable_name.to_string(), state_override);
                }
                None => {
                    project_configuration
                        .prompt_skills
                        .skill_overrides
                        .remove(stable_name);
                }
            }
            Ok(())
        },
        |_project_configuration| Ok(()),
    )
    .await
}

/// Reprojects agent catalog after a focused catalog write or refresh.
/// 在聚焦的目录写入或刷新后重新投影智能体目录。
fn refresh_subagent_catalog_local_state(
    state: &SharedServiceBridgeState,
) -> Result<SubagentCatalogUpdate, String> {
    refresh_subagent_catalog_local_state_with_errors(state, Vec::new())
}

/// Reprojects agent catalog and appends transient refresh errors.
/// 重新投影智能体目录并追加临时刷新错误。
fn refresh_subagent_catalog_local_state_with_errors(
    state: &SharedServiceBridgeState,
    transient_errors: Vec<vulcan_task_session::SubagentCatalogLoadError>,
) -> Result<SubagentCatalogUpdate, String> {
    let mut catalog = load_current_subagent_catalog();
    catalog.errors.extend(transient_errors);
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(runtime) = guard.agent_runtime.as_ref() {
        runtime
            .task_runtime
            .replace_subagent_catalog(catalog.clone())
            .map_err(|error| format!("刷新运行时智能体目录失败：{error}"))?;
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    let provider_configuration = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.provider_configuration.clone())
        .unwrap_or_default();
    guard.workbench.subagent_catalog =
        subagent_catalog_summary_from_catalog(&catalog, &provider_configuration);
    Ok(SubagentCatalogUpdate {
        revision: guard.workbench_generation,
        subagent_catalog: guard.workbench.subagent_catalog.clone(),
    })
}

/// Builds the agent catalog notice from the current projected summary.
/// 从当前已投影摘要构建智能体目录提示。
fn subagent_catalog_notice_from_summary(catalog: &SubagentCatalogSummary) -> String {
    let enabled_count = catalog
        .definitions
        .iter()
        .filter(|definition| definition.enabled)
        .count();
    format!(
        "智能体定义 {} 个，已启用 {} 个，加载警告 {}。",
        catalog.definitions.len(),
        enabled_count,
        catalog.errors.len()
    )
}

/// Loads the current two-layer agent catalog from system and user roots.
/// 从系统根与用户根加载当前双层智能体目录。
fn load_current_subagent_catalog() -> vulcan_task_session::SubagentCatalog {
    vulcan_task_session::load_subagent_catalog_from_roots(
        vulcan_runtime_assets::resolve_system_subagents_resource_directory(),
        user_subagent_root(),
    )
}

/// Returns whether a system-integrated agent definition exists before user overrides are applied.
/// 返回应用用户覆盖前是否存在系统集成智能体定义。
fn system_subagent_definition_exists(agent_id: &str) -> bool {
    let catalog = vulcan_task_session::load_subagent_catalog_from_roots(
        vulcan_runtime_assets::resolve_system_subagents_resource_directory(),
        PathBuf::new(),
    );
    catalog.definitions.iter().any(|definition| {
        definition.id == agent_id
            && definition.source.scope == vulcan_task_session::SubagentDefinitionScope::System
    })
}

/// Returns whether one effective definition still represents a system-integrated agent.
/// 返回单个生效定义是否仍代表系统集成智能体。
fn subagent_definition_is_system_backed(
    definition: &vulcan_task_session::SubagentDefinition,
) -> bool {
    definition.source.scope == vulcan_task_session::SubagentDefinitionScope::System
        || definition
            .source
            .override_of
            .as_deref()
            .is_some_and(system_subagent_definition_exists)
        || (definition.source.package_id.is_none()
            && system_subagent_definition_exists(&definition.id))
}

/// Returns the user-owned agent catalog root.
/// 返回用户拥有的智能体目录根路径。
fn user_subagent_root() -> PathBuf {
    vulcan_workspace_store::data_plane::application_data_root().join("agents")
}

/// Returns the user-owned agent definition directory.
/// 返回用户拥有的智能体定义目录。
fn user_subagent_definition_root() -> PathBuf {
    user_subagent_root().join("definitions")
}

/// Returns the user-owned file path for one agent definition id.
/// 返回单个智能体定义标识对应的用户文件路径。
fn user_subagent_definition_path(agent_id: &str) -> Result<PathBuf, String> {
    vulcan_task_session::user_subagent_definition_path(user_subagent_root().as_path(), agent_id)
        .map_err(|error| error.message)
}

/// Returns the user-owned sidecar file path for one agent definition id.
/// 返回单个智能体定义标识对应的用户 sidecar 文件路径。
fn user_subagent_sidecar_path(agent_id: &str) -> Result<PathBuf, String> {
    vulcan_task_session::user_subagent_sidecar_path(user_subagent_root().as_path(), agent_id)
        .map_err(|error| error.message)
}

/// Returns the user-owned override file path for one agent definition id.
/// 返回单个智能体定义标识对应的用户覆盖文件路径。
fn user_subagent_override_path(agent_id: &str) -> Result<PathBuf, String> {
    vulcan_task_session::user_subagent_override_path(user_subagent_root().as_path(), agent_id)
        .map_err(|error| error.message)
}

/// Returns trimmed request text only when it is non-empty.
/// 仅在请求文本非空时返回裁剪后的文本。
fn non_empty_request_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Applies one tri-state text patch to a defaulted string field.
/// 将一个三态文本补丁应用到带默认值的字符串字段。
fn apply_defaulted_text_patch(patch: OptionalTextPatch, value: &mut String, default_value: &str) {
    match patch {
        OptionalTextPatch::Missing => {}
        OptionalTextPatch::Clear => {
            *value = default_value.to_string();
        }
        OptionalTextPatch::Set(next_value) => {
            *value =
                non_empty_request_text(&next_value).unwrap_or_else(|| default_value.to_string());
        }
    }
}

/// Applies one tri-state text patch to an optional string field.
/// 将一个三态文本补丁应用到可选字符串字段。
fn apply_optional_text_patch(patch: OptionalTextPatch, value: &mut Option<String>) {
    match patch {
        OptionalTextPatch::Missing => {}
        OptionalTextPatch::Clear => {
            *value = None;
        }
        OptionalTextPatch::Set(next_value) => {
            *value = non_empty_request_text(&next_value);
        }
    }
}

/// Validates a fixed provider/model request against existing metadata and effective model choices.
/// 根据既有元数据与有效模型列表校验固定供应商/模型请求。
pub(crate) fn validate_subagent_provider_model_request_with_existing(
    request: &SubagentDefinitionSaveRequest,
    existing: Option<&vulcan_task_session::SubagentDefinition>,
    provider_configuration: &ProviderConfigurationFile,
) -> Result<vulcan_session_core::ProviderModelSnapshot, String> {
    let provider_id = request
        .provider_model
        .provider_id
        .as_deref()
        .and_then(non_empty_request_text);
    let model_id = request
        .provider_model
        .model_id
        .as_deref()
        .and_then(non_empty_request_text);
    let snapshot = match (provider_id, model_id) {
        (None, None) => vulcan_session_core::ProviderModelSnapshot::default(),
        (Some(provider_id), Some(model_id)) => vulcan_session_core::ProviderModelSnapshot {
            provider_id: Some(provider_id),
            model_id: Some(model_id),
            reasoning_effort: None,
            execution_mode: None,
        },
        _ => {
            return Err(String::from(
                "固定模型需要同时提供 Provider 和模型；如需不固定模型，请清空两者以跟随会话模型。",
            ));
        }
    };
    let Some(provider_id) = snapshot.provider_id.as_deref() else {
        return Ok(snapshot);
    };
    let Some(model_id) = snapshot.model_id.as_deref() else {
        return Ok(snapshot);
    };
    if existing
        .map(|definition| subagent_provider_model_pair_matches(definition, provider_id, model_id))
        .unwrap_or(false)
    {
        return Ok(snapshot);
    }
    let available = provider_config::effective_model_choices(provider_id, provider_configuration)
        .into_iter()
        .any(|choice| choice.id == model_id);
    if available {
        Ok(snapshot)
    } else {
        Err(format!(
            "固定模型 `{provider_id}/{model_id}` 不在当前已启用模型列表中，请选择可用模型或恢复跟随会话模型。"
        ))
    }
}

/// Returns whether one existing definition already carries the same provider/model pair.
/// 返回既有定义是否已经携带同一组供应商/模型。
fn subagent_provider_model_pair_matches(
    definition: &vulcan_task_session::SubagentDefinition,
    provider_id: &str,
    model_id: &str,
) -> bool {
    definition.provider_model.provider_id.as_deref() == Some(provider_id)
        && definition.provider_model.model_id.as_deref() == Some(model_id)
}

/// Applies the save request's unified tool policy to one exposure snapshot.
/// 将保存请求中的统一工具策略应用到可见性快照。
fn apply_request_tool_policy(
    snapshot: &mut vulcan_session_core::ToolExposureSnapshot,
    request: &SubagentDefinitionSaveRequest,
) {
    let policy = request_capability_policy(
        request.tool_policy_mode.as_deref(),
        request.tool_policy_tools.clone(),
        request.allowed_tools.clone(),
        request.disabled_tools.clone(),
    );
    vulcan_session_core::apply_tool_capability_policy(snapshot, policy);
    vulcan_session_core::sanitize_child_agent_tool_exposure(snapshot);
    snapshot.allowed_ai_capabilities.clear();
    snapshot.disabled_ai_capabilities.clear();
}

/// Applies the save request's unified prompt-skill policy to one snapshot.
/// 将保存请求中的统一提示词技能策略应用到快照。
fn apply_request_prompt_skill_policy(
    snapshot: &mut vulcan_session_core::PromptSkillSnapshot,
    request: &SubagentDefinitionSaveRequest,
) {
    let policy = request_capability_policy(
        request.prompt_skill_policy_mode.as_deref(),
        request.prompt_skill_policy_skills.clone(),
        request.enabled_prompt_skills.clone(),
        request.disabled_prompt_skills.clone(),
    );
    vulcan_session_core::apply_prompt_skill_capability_policy(snapshot, policy);
}

/// Builds a capability policy from new request fields or legacy allow/deny lists.
/// 从新版请求字段或旧版允许/禁止列表构建能力策略。
fn request_capability_policy(
    mode: Option<&str>,
    selectors: Vec<String>,
    legacy_allowed: Vec<String>,
    legacy_disabled: Vec<String>,
) -> vulcan_session_core::CapabilityPolicySnapshot {
    vulcan_session_core::capability_policy_from_mode_or_selector_lists(
        mode,
        selectors,
        legacy_allowed,
        legacy_disabled,
    )
}

/// Normalizes localized definition metadata from a save request.
/// 规范化保存请求中的智能体本地化元数据。
fn normalized_request_definition_locales(
    locales: std::collections::BTreeMap<String, SubagentDefinitionLocaleSummary>,
) -> std::collections::BTreeMap<
    String,
    vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition,
> {
    locales
        .into_iter()
        .filter_map(|(locale, metadata)| {
            let locale = locale.trim().to_string();
            if locale.is_empty() {
                return None;
            }
            let name = metadata.name.as_deref().and_then(non_empty_request_text);
            let description = metadata
                .description
                .as_deref()
                .and_then(non_empty_request_text);
            if name.is_none() && description.is_none() {
                return None;
            }
            Some((
                locale,
                vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition {
                    name,
                    description,
                },
            ))
        })
        .collect()
}

/// Selects the default name from localized metadata with English taking priority.
/// 从本地化元数据中选择默认名称，并优先使用英文。
fn preferred_definition_locale_name(
    locales: &std::collections::BTreeMap<
        String,
        vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition,
    >,
) -> Option<String> {
    preferred_definition_locale_field(locales, |metadata| metadata.name.as_ref())
}

/// Selects the default description from localized metadata with English taking priority.
/// 从本地化元数据中选择默认说明，并优先使用英文。
fn preferred_definition_locale_description(
    locales: &std::collections::BTreeMap<
        String,
        vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition,
    >,
) -> Option<String> {
    preferred_definition_locale_field(locales, |metadata| metadata.description.as_ref())
}

/// Selects a localized text field using English first and Chinese as fallback.
/// 按英文优先、中文兜底的顺序选择本地化文本字段。
fn preferred_definition_locale_field(
    locales: &std::collections::BTreeMap<
        String,
        vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition,
    >,
    field: impl Fn(&vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition) -> Option<&String>,
) -> Option<String> {
    ["en-US", "en", "zh-CN", "zh"].iter().find_map(|preferred| {
        let preferred_lower = preferred.to_ascii_lowercase();
        locales
            .iter()
            .find(|(locale, metadata)| {
                let locale_lower = locale.to_ascii_lowercase();
                (locale_lower == preferred_lower
                    || locale_lower.starts_with(&format!("{preferred_lower}-")))
                    && field(metadata).is_some()
            })
            .and_then(|(_, metadata)| field(metadata).cloned())
    })
}

/// Normalizes one package id for filesystem storage.
/// 规范化一个用于文件系统存储的包标识。
fn normalized_package_id(value: &str) -> Result<String, String> {
    let package_id = value.trim();
    if package_id.is_empty()
        || package_id == "."
        || package_id == ".."
        || package_id.contains('/')
        || package_id.contains('\\')
        || package_id.contains(':')
    {
        return Err(format!("无效的智能体包标识：{value}"));
    }
    Ok(package_id.to_string())
}

/// Normalizes an optional user package id and rejects system package collisions.
/// 规范化可选用户包标识并拒绝与系统包冲突。
fn normalized_optional_user_package_id(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let package_id = normalized_package_id(value)?;
    if system_subagent_package_exists(&package_id) {
        return Err(format!("用户自定义包不能与系统包 `{package_id}` 重名。"));
    }
    Ok(Some(package_id))
}

/// Returns whether the application ships a system package/category with the id.
/// 返回应用是否内置了同名系统包或分类。
fn system_subagent_package_exists(package_id: &str) -> bool {
    let system_root = vulcan_runtime_assets::resolve_system_subagents_resource_directory();
    std::fs::read_dir(system_root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .any(|entry| {
            entry.path().is_dir()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name == package_id)
        })
}

/// Builds the target agent id for a package assignment or transfer.
/// 为包分配或转移构建目标智能体标识。
fn target_agent_id_for_save(
    requested_agent_id: &str,
    existing: Option<&vulcan_task_session::SubagentDefinition>,
    target_package_id: Option<&str>,
) -> Result<String, String> {
    let mut base_id = normalize_agent_file_name_input(requested_agent_id);
    if let Some(existing_package_id) = existing
        .and_then(|definition| definition.source.package_id.as_deref())
        .filter(|package_id| !package_id.is_empty())
    {
        let prefix = format!("{existing_package_id}/");
        if let Some(stripped) = base_id.strip_prefix(&prefix) {
            base_id = stripped.to_string();
        } else if let Some(stripped) = existing
            .map(|definition| definition.id.as_str())
            .and_then(|id| id.strip_prefix(&prefix))
        {
            base_id = stripped.to_string();
        }
    }
    if let Some(target_package_id) = target_package_id {
        let prefix = format!("{target_package_id}/");
        if let Some(stripped) = base_id.strip_prefix(&prefix) {
            base_id = stripped.to_string();
        }
        base_id = format!("{target_package_id}/{base_id}");
    }
    user_subagent_definition_path(&base_id)?;
    Ok(base_id)
}

/// Normalizes an editable agent file name into a stable definition id.
/// 将可编辑智能体文件名规范化为稳定定义标识。
fn normalize_agent_file_name_input(value: &str) -> String {
    // Keep one pending segment so only the final path segment loses its extension.
    // 保留一个待定片段，确保只有最后一个路径片段会移除扩展名。
    let mut normalized = String::new();
    let mut pending_segment = None;
    for segment in value
        .trim()
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty())
    {
        if let Some(previous_segment) = pending_segment.replace(segment) {
            push_agent_file_name_segment(&mut normalized, previous_segment);
        }
    }
    if let Some(last_segment) = pending_segment {
        push_agent_file_name_segment(
            &mut normalized,
            strip_agent_file_segment_extension(last_segment),
        );
    }
    normalized
}

/// Appends one normalized agent id segment to an existing slash-separated id.
/// 将单个规范化智能体标识片段追加到现有斜杠分隔标识。
fn push_agent_file_name_segment(normalized: &mut String, segment: &str) {
    if !normalized.is_empty() {
        normalized.push('/');
    }
    normalized.push_str(segment);
}

/// Removes the last extension-like suffix from one agent id path segment.
/// 从单个智能体标识路径片段移除最后一个扩展名形式的后缀。
fn strip_agent_file_segment_extension(segment: &str) -> &str {
    segment
        .rsplit_once('.')
        .filter(|(stem, _extension)| !stem.is_empty())
        .map(|(stem, _extension)| stem)
        .unwrap_or(segment)
}

/// Collects all possible user-layer files that can override one system agent definition.
/// 收集可能覆盖某个系统智能体定义的所有用户层文件。
fn restore_default_user_file_candidates(
    agent_id: &str,
    definition: &vulcan_task_session::SubagentDefinition,
) -> Result<Vec<PathBuf>, String> {
    let mut candidates = Vec::new();
    push_user_subagent_generated_files(&mut candidates, agent_id, true)?;
    push_user_subagent_source_files(
        &mut candidates,
        &definition.source,
        definition.source.scope == vulcan_task_session::SubagentDefinitionScope::User,
    );
    Ok(candidates)
}

/// Collects user-owned files that define one deletable user agent.
/// 收集定义单个可删除用户智能体的用户文件。
fn user_definition_delete_candidates(
    agent_id: &str,
    definition: &vulcan_task_session::SubagentDefinition,
) -> Result<Vec<PathBuf>, String> {
    let mut candidates = Vec::new();
    push_user_subagent_source_files(&mut candidates, &definition.source, true);
    push_user_subagent_generated_files(&mut candidates, agent_id, true)?;
    Ok(candidates)
}

/// Removes stale user-owned source files after the definition has moved to a new id.
/// 在定义移动到新标识后删除旧的用户来源文件。
fn remove_stale_user_subagent_files_for_source(
    previous_agent_id: &str,
    previous_source: &vulcan_task_session::SubagentDefinitionSource,
    retained_file: &Path,
) -> Result<(), String> {
    let candidates = stale_user_subagent_file_candidates(previous_agent_id, previous_source)?;
    for path in candidates {
        remove_user_subagent_file_if_safe(&path, retained_file)?;
    }
    Ok(())
}

/// Collects stale user-owned source files for one previous agent id.
/// 收集单个旧智能体标识对应的旧用户来源文件。
fn stale_user_subagent_file_candidates(
    previous_agent_id: &str,
    previous_source: &vulcan_task_session::SubagentDefinitionSource,
) -> Result<Vec<PathBuf>, String> {
    let mut candidates = Vec::new();
    push_user_subagent_source_files(&mut candidates, previous_source, true);
    push_user_subagent_generated_files(&mut candidates, previous_agent_id, false)?;
    Ok(candidates)
}

/// Pushes source-owned user agent files into one candidate list.
/// 将来源记录拥有的用户智能体文件推入候选列表。
fn push_user_subagent_source_files(
    candidates: &mut Vec<PathBuf>,
    source: &vulcan_task_session::SubagentDefinitionSource,
    include_primary_file: bool,
) {
    if include_primary_file {
        push_unique_path(candidates, source.file.clone());
    }
    if let Some(sidecar_file) = source.sidecar_file.as_ref() {
        push_unique_path(candidates, sidecar_file.clone());
    }
}

/// Pushes generated user agent files for one agent id into one candidate list.
/// 将单个智能体标识对应的派生用户文件推入候选列表。
fn push_user_subagent_generated_files(
    candidates: &mut Vec<PathBuf>,
    agent_id: &str,
    include_definition_file: bool,
) -> Result<(), String> {
    if include_definition_file {
        push_unique_path(candidates, user_subagent_definition_path(agent_id)?);
    }
    push_unique_path(candidates, user_subagent_sidecar_path(agent_id)?);
    push_unique_path(candidates, user_subagent_override_path(agent_id)?);
    Ok(())
}

/// Returns whether one existing file is inside the user agent root.
/// 返回一个已存在文件是否位于用户智能体根目录内。
fn user_subagent_file_exists_inside_root(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let user_root = user_subagent_root();
    let canonical_user_root = std::fs::canonicalize(&user_root).map_err(|error| {
        format!(
            "解析用户智能体根目录 `{}` 失败：{error}",
            user_root.display()
        )
    })?;
    let canonical_path = std::fs::canonicalize(path)
        .map_err(|error| format!("解析智能体用户层文件 `{}` 失败：{error}", path.display()))?;
    Ok(canonical_path.starts_with(canonical_user_root))
}

/// Removes one file when it is an existing user-layer agent file.
/// 当文件是已存在的用户层智能体文件时删除它。
fn remove_user_subagent_file_if_inside_root(path: &Path) -> Result<(), String> {
    if !user_subagent_file_exists_inside_root(path)? {
        return Ok(());
    }
    std::fs::remove_file(path)
        .map_err(|error| format!("删除用户智能体文件 `{}` 失败：{error}", path.display()))
}

/// Pushes one filesystem path while preserving first occurrence order.
/// 按首次出现顺序推入单个文件系统路径。
fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

/// Removes one user catalog file only when it is still inside the user agent root.
/// 仅在文件仍位于用户智能体根目录内时删除单个用户目录文件。
fn remove_user_subagent_file_if_safe(path: &Path, retained_file: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let user_root = user_subagent_root();
    let canonical_user_root = std::fs::canonicalize(&user_root).map_err(|error| {
        format!(
            "解析用户智能体根目录 `{}` 失败：{error}",
            user_root.display()
        )
    })?;
    let canonical_path = std::fs::canonicalize(path)
        .map_err(|error| format!("解析旧智能体文件 `{}` 失败：{error}", path.display()))?;
    if std::fs::canonicalize(retained_file)
        .ok()
        .as_ref()
        .is_some_and(|canonical_retained| canonical_retained == &canonical_path)
    {
        return Ok(());
    }
    if !canonical_path.starts_with(&canonical_user_root) {
        return Err(format!(
            "拒绝删除用户智能体目录外的旧文件：{}",
            path.display()
        ));
    }
    std::fs::remove_file(path)
        .map_err(|error| format!("删除旧智能体文件 `{}` 失败：{error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one minimal user subagent definition for file-candidate tests.
    /// 为文件候选测试构建一个最小用户智能体定义。
    fn test_user_subagent_definition() -> vulcan_task_session::SubagentDefinition {
        vulcan_task_session::SubagentDefinition {
            id: String::from("engineering/test-agent"),
            name: String::from("Test Agent"),
            description: None,
            locales: std::collections::BTreeMap::new(),
            origin: None,
            emoji: String::from("AI"),
            color: String::from("gray"),
            vibe: None,
            mode: vulcan_task_session::SubagentDefinitionMode::Expert,
            system_prompt: None,
            system_prompt_append: None,
            provider_model: Default::default(),
            tool_exposure: Default::default(),
            prompt_skills: Default::default(),
            context_compaction: Default::default(),
            runtime_policy: Default::default(),
            max_depth: None,
            enabled: true,
            source: vulcan_task_session::SubagentDefinitionSource {
                scope: vulcan_task_session::SubagentDefinitionScope::User,
                file: std::path::PathBuf::from("custom/test-agent.md"),
                sidecar_file: Some(std::path::PathBuf::from("custom/test-agent.sidecar.json")),
                source_key: String::from("user:engineering/test-agent"),
                category_id: Some(String::from("engineering")),
                package_id: None,
                remote_url: None,
                override_of: None,
                source_hash: None,
                version: None,
                etag: None,
                last_modified: None,
                updated_at: None,
            },
            extra: serde_json::Value::Null,
        }
    }

    /// Verifies delete candidates include source and generated definition files.
    /// 验证删除候选会包含来源文件与派生定义文件。
    #[test]
    fn user_definition_delete_candidates_include_source_and_generated_files() {
        let definition = test_user_subagent_definition();
        let candidates = user_definition_delete_candidates("engineering/test-agent", &definition)
            .expect("delete candidates should resolve");

        assert!(candidates.contains(&std::path::PathBuf::from("custom/test-agent.md")));
        assert!(candidates.contains(&std::path::PathBuf::from("custom/test-agent.sidecar.json")));
        assert!(
            candidates.contains(
                &user_subagent_definition_path("engineering/test-agent")
                    .expect("definition path should resolve")
            )
        );
        assert!(
            candidates.contains(
                &user_subagent_sidecar_path("engineering/test-agent")
                    .expect("sidecar path should resolve")
            )
        );
        assert!(
            candidates.contains(
                &user_subagent_override_path("engineering/test-agent")
                    .expect("override path should resolve")
            )
        );
    }

    /// Verifies editable agent file names normalize separators and strip the final extension.
    /// 验证可编辑智能体文件名会归一化分隔符并移除最终扩展名。
    #[test]
    fn normalize_agent_file_name_input_normalizes_separators_and_final_extension() {
        // editable_name mirrors a Windows-style file path submitted from the WebUI.
        // editable_name 模拟 WebUI 提交的 Windows 风格文件路径。
        let editable_name = r"  engineering\reviewer.md  ";

        assert_eq!(
            normalize_agent_file_name_input(editable_name),
            "engineering/reviewer"
        );
        assert_eq!(
            normalize_agent_file_name_input("//engineering//reviewer.sidecar.json"),
            "engineering/reviewer.sidecar"
        );
    }

    /// Verifies agent file-name normalization preserves non-final segment text.
    /// 验证智能体文件名归一化会保留非最终片段文本。
    #[test]
    fn normalize_agent_file_name_input_preserves_non_final_segments() {
        // dotted_category proves only the last path segment is treated as a file name.
        // dotted_category 证明只有最后一个路径片段会按文件名处理。
        let dotted_category = "team.v1/reviewer.md";

        assert_eq!(
            normalize_agent_file_name_input(dotted_category),
            "team.v1/reviewer"
        );
        assert_eq!(normalize_agent_file_name_input(".md"), ".md");
    }

    /// Verifies saved tool policies drop tools that child agents may never use.
    /// 验证保存的工具策略会移除智能体绝不能使用的工具。
    #[test]
    fn apply_request_tool_policy_sanitizes_child_forbidden_tools() {
        let request = SubagentDefinitionSaveRequest {
            tool_policy_mode: Some(String::from("allow_only")),
            tool_policy_tools: vec![
                String::from("shell_command"),
                String::from("todolist"),
                String::from("ask"),
            ],
            ..SubagentDefinitionSaveRequest::default()
        };
        let mut snapshot = vulcan_session_core::ToolExposureSnapshot::default();

        apply_request_tool_policy(&mut snapshot, &request);

        assert_eq!(
            snapshot.policy.mode,
            vulcan_session_core::CapabilityPolicyMode::AllowOnly
        );
        assert_eq!(
            snapshot.policy.selectors,
            vec![String::from("shell_command")]
        );
        assert_eq!(snapshot.allowed_tools, vec![String::from("shell_command")]);
        assert!(snapshot.disabled_tools.is_empty());
    }

    /// Verifies stale cleanup candidates keep source files but skip generated definitions.
    /// 验证旧文件清理候选保留来源文件但跳过派生定义文件。
    #[test]
    fn stale_user_subagent_file_candidates_skip_generated_definition_file() {
        let definition = test_user_subagent_definition();
        let candidates =
            stale_user_subagent_file_candidates("engineering/test-agent", &definition.source)
                .expect("stale candidates should resolve");

        assert!(candidates.contains(&std::path::PathBuf::from("custom/test-agent.md")));
        assert!(candidates.contains(&std::path::PathBuf::from("custom/test-agent.sidecar.json")));
        assert!(
            !candidates.contains(
                &user_subagent_definition_path("engineering/test-agent")
                    .expect("definition path should resolve")
            )
        );
        assert!(
            candidates.contains(
                &user_subagent_sidecar_path("engineering/test-agent")
                    .expect("sidecar path should resolve")
            )
        );
        assert!(
            candidates.contains(
                &user_subagent_override_path("engineering/test-agent")
                    .expect("override path should resolve")
            )
        );
    }
}
