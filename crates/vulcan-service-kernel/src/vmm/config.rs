//! VMM config helpers.
//! VMM config 辅助逻辑。

use super::*;
use crate::{mutate_project_configuration, mutate_provider_configuration_with_state_validation};

#[cfg(test)]
#[path = "config_save_test_scope.rs"]
pub(crate) mod config_save_test_scope;

/// A validated enable-time binding plan that is safe to execute after global configuration persistence.
/// 一份已经通过校验、可在全局配置持久化后安全执行的启用期绑定计划。
enum PreparedProjectBindingPlan {
    /// No project binding work is required for the selected configuration.
    /// 所选配置不需要执行项目绑定工作。
    None,
    /// Native bindings resolved against the current local catalog.
    /// 已根据当前本地目录解析的原生绑定。
    Native {
        /// Native maintenance service that owns durable project identities and bindings.
        /// 持有持久项目身份与绑定关系的原生维护服务。
        maintenance: crate::native_vmm::NativeVmmMaintenance,
        /// Stable active host identities covered by this review; names and paths do not own native bindings.
        /// 此次审查覆盖的稳定活动宿主身份；名称与路径不拥有原生绑定。
        host_project_ids: std::collections::BTreeSet<String>,
        /// Validated per-project operations in submitted order.
        /// 按提交顺序保存的已校验逐项目操作。
        entries: Vec<PreparedNativeProjectBinding>,
    },
    /// External gRPC bindings resolved against the current remote catalog.
    /// 已根据当前远端目录解析的外置 gRPC 绑定。
    External {
        /// Standalone external layer used by management requests during execution.
        /// 执行期间管理请求使用的独立外置配置层。
        layer: vulcan_vmm::VmmConfigLayer,
        /// Active host identities and exact paths that own external project configuration writes.
        /// 拥有外置项目配置写入的活动宿主身份及准确路径。
        host_project_paths: std::collections::BTreeMap<String, String>,
        /// Validated per-project operations in stable project order.
        /// 按稳定项目顺序保存的已校验逐项目操作。
        entries: Vec<PreparedExternalProjectBinding>,
    },
}

/// One validated native binding operation.
/// 单个已经通过校验的原生绑定操作。
struct PreparedNativeProjectBinding {
    /// Stable VulcanCode project tag receiving the binding.
    /// 接收绑定的稳定 VulcanCode 项目标签。
    vulcan_project_id: String,
    /// Exact native operation selected by the user.
    /// 用户选择的准确原生操作。
    action: PreparedNativeProjectBindingAction,
}

/// Exact action for one validated native binding operation.
/// 单个已校验原生绑定操作的准确动作。
enum PreparedNativeProjectBindingAction {
    /// Bind to one existing native VMM project identity.
    /// 绑定到一个已有原生 VMM 项目身份。
    BindExisting(u64),
    /// Create one native VMM project with the supplied display name and bind it.
    /// 使用给定显示名称创建一个原生 VMM 项目并绑定。
    CreateAndBind(String),
}

/// One validated external binding operation.
/// 单个已经通过校验的外置绑定操作。
struct PreparedExternalProjectBinding {
    /// Stable VulcanCode project tag receiving the binding.
    /// 接收绑定的稳定 VulcanCode 项目标签。
    vulcan_project_id: String,
    /// Exact project configuration path that persists the remote project identity.
    /// 持久化远端项目身份的准确项目配置路径。
    project_config_path: String,
    /// Exact external operation selected by the user.
    /// 用户选择的准确外置操作。
    action: PreparedExternalProjectBindingAction,
}

/// Exact action for one validated external binding operation.
/// 单个已校验外置绑定操作的准确动作。
#[derive(Debug, PartialEq, Eq)]
enum PreparedExternalProjectBindingAction {
    /// Bind to one existing remote VMM project identity.
    /// 绑定到一个已有远端 VMM 项目身份。
    BindExisting(u64),
    /// Ensure one canonical remote project path and bind its returned identity.
    /// 确保一个规范远端项目路径存在并绑定其返回身份。
    CreateAndBind(String),
}

/// Saves one VMM memory configuration draft.
/// 保存单个 VMM 记忆配置草稿。
///
/// # Parameters
/// 参数
/// `state` is the shared service state used to resolve and refresh persisted settings.
/// `state` 是用于解析并刷新持久化设置的共享服务状态。
/// `request` is the WebUI configuration save request.
/// `request` 是 WebUI 配置保存请求。
///
/// # Returns
/// 返回值
/// `Ok(None)` after a complete save, or `Ok(Some(message))` when global enablement succeeded but individual bindings failed.
/// 完整保存后返回 `Ok(None)`；全局启用成功但个别绑定失败时返回 `Ok(Some(message))`。
pub(crate) async fn save_vmm_memory_config(
    state: &SharedServiceBridgeState,
    mut request: VmmConfigSaveRequest,
) -> Result<Option<String>, String> {
    let scope = request.scope.trim().to_ascii_lowercase();
    if scope == "project" {
        if !request.binding_plan.is_empty() {
            return Err(String::from("项目级 VMM 设置不接受全局项目绑定计划。"));
        }
        let project_path = vmm_project_scope_path(state, request.project_id.as_deref())?;
        return mutate_project_configuration(
            &PathBuf::from(&project_path),
            "保存项目 VMM 配置失败",
            |project_configuration| {
                project_configuration.vmm = if request.draft.has_override {
                    vmm_config_layer_from_draft(&request.draft, false)?
                } else {
                    vulcan_vmm::VmmConfigLayer::default()
                };
                Ok(())
            },
            |project_configuration| {
                update_cached_project_vmm_layer(&project_path, project_configuration.vmm.clone());
                refresh_exact_vmm_hook_project_projection(
                    state,
                    &project_path,
                    &project_configuration.vmm,
                )
            },
        )
        .await
        .map(|()| None);
    }
    if scope != "global" {
        return Err(format!("不支持的 VMM 配置作用域：{scope}"));
    }
    let current_configuration = provider_configuration_from_state(state)?;
    if request.draft.grpc_api_key.trim() == WEBUI_REDACTED_SECRET_PLACEHOLDER {
        request.draft.grpc_api_key = current_configuration
            .vmm
            .grpc_api_key
            .clone()
            .unwrap_or_default();
    }
    if request.draft.management_bearer_token.trim() == WEBUI_REDACTED_SECRET_PLACEHOLDER {
        request.draft.management_bearer_token = current_configuration
            .vmm
            .management_bearer_token
            .clone()
            .unwrap_or_default();
    }
    let next_layer = vmm_config_layer_from_draft(&request.draft, true)?;
    let current_effective =
        vulcan_vmm::VmmEffectiveConfig::resolve(&current_configuration.vmm, None, None);
    let next_effective = vulcan_vmm::VmmEffectiveConfig::resolve(&next_layer, None, None);
    if next_effective.enabled && next_effective.backend_mode == vulcan_vmm::VmmBackendMode::Native {
        let maintenance = state
            .lock()
            .map_err(|error| format!("锁定原生 VMM 推理预检状态失败：{error}"))?
            .native_vmm_maintenance
            .clone()
            .ok_or_else(|| String::from("原生 VMM 运行时尚未就绪。"))?;
        maintenance.validate_host_ai().await?;
    }
    // Binding reconciliation is mandatory only when a backend becomes active or the caller
    // explicitly submits a retry plan. Ordinary edits on the same active backend must remain
    // possible after an earlier per-project binding failure.
    // 仅在后端开始生效或调用方明确提交重试计划时强制绑定对账；同一活动后端的普通编辑在此前个别绑定失败后仍必须可保存。
    let prepared_binding_plan = if should_prepare_project_binding_plan(
        &current_effective,
        &next_effective,
        !request.binding_plan.is_empty(),
    ) {
        prepare_project_binding_plan(state, &next_layer, &request.binding_plan).await?
    } else {
        PreparedProjectBindingPlan::None
    };
    #[cfg(test)]
    config_save_test_scope::pause_after_preflight().await;
    // Freeze native associations before taking the Provider transaction; no service lock is held across this query.
    // 在获取 Provider 事务之前冻结原生关联，此查询期间不跨异步持有服务锁。
    // Retain the existing maintenance gate through persistence so an unbind cannot invalidate coverage between check and commit.
    // 将既有维护门禁保留至持久化结束，避免解绑在检查与提交之间使覆盖失效。
    let binding_coverage_guard = match &prepared_binding_plan {
        PreparedProjectBindingPlan::Native {
            maintenance,
            host_project_ids,
            entries,
        } => {
            // Entries have already passed exact plan validation; do not substitute targets from the latest catalog.
            // 条目已通过准确计划校验，不从最新目录替换目标。
            let planned_project_ids = entries
                .iter()
                .map(|entry| entry.vulcan_project_id.clone())
                .collect();
            Some(
                maintenance
                    .freeze_project_binding_coverage(host_project_ids, &planned_project_ids)
                    .await?,
            )
        }
        PreparedProjectBindingPlan::None | PreparedProjectBindingPlan::External { .. } => None,
    };
    // A saved global transport may point at a different endpoint, so no previous probe remains authoritative.
    // 保存后的全局传输可能指向其他端点，因此此前的探测结果不再具备权威性。
    mutate_provider_configuration_with_state_validation(
        state,
        "保存 VMM 全局配置失败",
        |guard| validate_prepared_project_binding_scope(guard, &prepared_binding_plan),
        move |configuration| {
            // Preflight and redacted-secret expansion were admitted against this exact VMM layer.
            // 预检及脱敏秘密展开基于这一准确 VMM 层完成准入。
            // Compare inside the existing transaction so concurrent VMM edits cannot be overwritten or bypass activation checks.
            // 在既有事务内比较，避免覆盖并发 VMM 编辑或绕过启用检查。
            if configuration.vmm != current_configuration.vmm {
                return Err(String::from(
                    "VMM 全局配置在预检期间已发生变化，请重新加载配置后保存。",
                ));
            }
            configuration.vmm = next_layer;
            Ok(())
        },
        |guard, configuration| {
            // Publish the non-failing worker switch while the same durable configuration transaction is still owned.
            // 在仍持有同一持久配置事务时发布不会失败的工作器开关。
            // A late request must never overwrite a newer mode after releasing the transaction and service locks.
            // 较早请求释放事务及服务锁后，绝不能再覆盖较新的模式。
            let committed = vulcan_vmm::VmmEffectiveConfig::resolve(&configuration.vmm, None, None);
            if let Some(maintenance) = guard.native_vmm_maintenance.as_ref() {
                maintenance.set_native_backend_enabled(
                    committed.enabled
                        && committed.backend_mode == vulcan_vmm::VmmBackendMode::Native,
                );
            }
            refresh_guard_vmm_hook_snapshot(guard, configuration)
        },
    )
    .await??;
    // Binding execution owns its normal per-project lock and preserves bindings chosen after global commit.
    // 绑定执行持有其常规逐项目锁，并保留全局提交后选择的绑定。
    drop(binding_coverage_guard);
    #[cfg(test)]
    config_save_test_scope::pause_after_commit().await;
    clear_external_vmm_health_probe();
    Ok(execute_prepared_project_binding_plan(state, prepared_binding_plan).await)
}

/// Decides whether one global save must validate and reconcile every unbound project.
/// 判断一次全局保存是否必须校验并对账全部未绑定项目。
fn should_prepare_project_binding_plan(
    current: &vulcan_vmm::VmmEffectiveConfig,
    next: &vulcan_vmm::VmmEffectiveConfig,
    has_explicit_plan: bool,
) -> bool {
    has_explicit_plan
        || (next.enabled && (!current.enabled || current.backend_mode != next.backend_mode))
}

/// Validates every explicit unbound-project decision without mutating project bindings.
/// 在不修改项目绑定的前提下校验每项明确的未绑定项目决定。
async fn prepare_project_binding_plan(
    state: &SharedServiceBridgeState,
    next_layer: &vulcan_vmm::VmmConfigLayer,
    plan: &[vulcan_service_api::VmmProjectBindingPlanEntry],
) -> Result<PreparedProjectBindingPlan, String> {
    let effective = vulcan_vmm::VmmEffectiveConfig::resolve(next_layer, None, None);
    if !effective.enabled {
        if !plan.is_empty() {
            return Err(String::from("VMM 禁用时不允许提交项目绑定计划。"));
        }
        return Ok(PreparedProjectBindingPlan::None);
    }
    match effective.backend_mode {
        vulcan_vmm::VmmBackendMode::Native => {
            prepare_native_project_binding_plan(state, &effective, plan).await
        }
        vulcan_vmm::VmmBackendMode::ExternalGrpc => {
            prepare_external_project_binding_plan(state, next_layer, &effective, plan).await
        }
    }
}

/// Validates and prepares enable-time bindings for the native backend.
/// 校验并准备原生后端启用时的项目绑定。
async fn prepare_native_project_binding_plan(
    state: &SharedServiceBridgeState,
    effective: &vulcan_vmm::VmmEffectiveConfig,
    plan: &[vulcan_service_api::VmmProjectBindingPlanEntry],
) -> Result<PreparedProjectBindingPlan, String> {
    let (maintenance, workspace) = {
        let guard = state
            .lock()
            .map_err(|error| format!("锁定原生 VMM 项目绑定状态失败：{error}"))?;
        (
            guard
                .native_vmm_maintenance
                .clone()
                .ok_or_else(|| String::from("原生 VMM 项目绑定运行时尚未就绪。"))?,
            guard
                .workspace_snapshot
                .clone()
                .ok_or_else(|| String::from("工作区项目列表尚未就绪。"))?,
        )
    };
    let catalog = maintenance.project_catalog().await?;
    if effective.native_project_binding_mode == vulcan_vmm::VmmNativeProjectBindingMode::Fixed
        && !catalog
            .projects
            .iter()
            .any(|item| item.project.id == effective.native_fixed_project_id)
    {
        return Err(format!(
            "原生 VMM 固定项目 {} 不存在，请选择已有项目。",
            effective.native_fixed_project_id
        ));
    }
    let bound_ids = catalog
        .bindings
        .iter()
        .map(|binding| binding.vulcan_project_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let unbound_projects = workspace
        .task_projects
        .iter()
        .filter(|project| !bound_ids.contains(project.id.as_str()))
        .map(|project| (project.id.clone(), project.title.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut decisions = std::collections::BTreeMap::new();
    for entry in plan {
        if decisions
            .insert(entry.vulcan_project_id.as_str(), entry)
            .is_some()
        {
            return Err(format!(
                "原生 VMM 绑定计划重复包含项目 `{}`。",
                entry.vulcan_project_id
            ));
        }
    }
    let expected_ids = unbound_projects
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let actual_ids = decisions.keys().copied().collect::<Vec<_>>();
    if expected_ids != actual_ids {
        return Err(format!(
            "原生 VMM 绑定计划必须准确覆盖当前未绑定项目。未绑定项目：[{}]。",
            expected_ids.join(", ")
        ));
    }
    // prepared_entries captures only exact validated actions, so execution never reparses user input.
    // prepared_entries 只保存准确且已校验的动作，因此执行阶段不会重新解析用户输入。
    let mut prepared_entries = Vec::with_capacity(plan.len());
    for entry in plan {
        let action = match entry.action.trim() {
            "bind_existing" => {
                let project_id = entry
                    .target_project_id
                    .filter(|project_id| *project_id > 0)
                    .ok_or_else(|| {
                        format!(
                            "项目 `{}` 缺少待绑定的已有 VMM 项目。",
                            entry.vulcan_project_id
                        )
                    })?;
                if !catalog
                    .projects
                    .iter()
                    .any(|item| item.project.id == project_id)
                {
                    return Err(format!(
                        "项目 `{}` 选择的原生 VMM 项目 {} 不存在。",
                        entry.vulcan_project_id, project_id
                    ));
                }
                PreparedNativeProjectBindingAction::BindExisting(project_id)
            }
            "create_and_bind" => {
                let name = entry
                    .project_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| {
                        format!("项目 `{}` 缺少新 VMM 项目名称。", entry.vulcan_project_id)
                    })?;
                if name.chars().count() > 512 {
                    return Err(format!(
                        "项目 `{}` 的新 VMM 项目名称超过 512 个字符。",
                        entry.vulcan_project_id
                    ));
                }
                PreparedNativeProjectBindingAction::CreateAndBind(name.to_string())
            }
            action => {
                return Err(format!(
                    "项目 `{}` 使用了不支持的绑定动作 `{action}`。",
                    entry.vulcan_project_id
                ));
            }
        };
        prepared_entries.push(PreparedNativeProjectBinding {
            vulcan_project_id: entry.vulcan_project_id.clone(),
            action,
        });
    }
    Ok(PreparedProjectBindingPlan::Native {
        maintenance,
        host_project_ids: workspace
            .task_projects
            .iter()
            .map(|project| project.id.clone())
            .collect(),
        entries: prepared_entries,
    })
}

/// Validates and prepares enable-time bindings for the external gRPC backend.
/// 校验并准备外置 gRPC 后端启用时的项目绑定。
async fn prepare_external_project_binding_plan(
    state: &SharedServiceBridgeState,
    next_layer: &vulcan_vmm::VmmConfigLayer,
    effective: &vulcan_vmm::VmmEffectiveConfig,
    plan: &[vulcan_service_api::VmmProjectBindingPlanEntry],
) -> Result<PreparedProjectBindingPlan, String> {
    if effective.user_id.filter(|user_id| *user_id > 0).is_none() {
        return Err(String::from("启用外置 VMM 前必须选择有效用户。"));
    }
    let workspace = state
        .lock()
        .map_err(|error| format!("锁定外置 VMM 项目绑定状态失败：{error}"))?
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("工作区项目列表尚未就绪。"))?;
    let remote_projects = load_external_vmm_projects(next_layer.clone()).await?;
    if effective.external_project_binding_mode == vulcan_vmm::VmmNativeProjectBindingMode::Fixed {
        let project_id = effective
            .project_id
            .filter(|project_id| *project_id > 0)
            .ok_or_else(|| String::from("启用外置固定项目策略前必须选择有效项目。"))?;
        if !remote_projects
            .iter()
            .any(|project| project.project_id == project_id)
        {
            return Err(format!("外置 VMM 固定项目 {project_id} 不存在。"));
        }
    }
    let unbound_projects = workspace
        .task_projects
        .iter()
        .filter(|project| cached_project_vmm_layer(&project.path).project_id.is_none())
        .map(|project| (project.id.clone(), project.path.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let decisions = validate_binding_plan_coverage(&unbound_projects, plan, "外置 VMM")?;
    // prepared_entries owns exact paths and actions so later failures are isolated per project.
    // prepared_entries 持有准确路径与动作，使后续失败可以按项目隔离。
    let mut prepared_entries = Vec::with_capacity(decisions.len());
    for entry in decisions.values() {
        // The global fixed project is the default for unedited rows, not a restriction on an
        // explicit per-project choice made in the enablement dialog.
        // 全局固定项目只是未编辑行的默认值，不限制用户在启用对话框中作出的逐项目明确选择。
        let action = prepare_external_project_binding_action(effective, &remote_projects, entry)?;
        let project_config_path = unbound_projects
            .get(&entry.vulcan_project_id)
            .ok_or_else(|| String::from("已校验的外置 VMM 项目路径意外缺失"))?;
        prepared_entries.push(PreparedExternalProjectBinding {
            vulcan_project_id: entry.vulcan_project_id.clone(),
            project_config_path: project_config_path.clone(),
            action,
        });
    }
    Ok(PreparedProjectBindingPlan::External {
        layer: next_layer.clone(),
        host_project_paths: workspace
            .task_projects
            .iter()
            .map(|project| (project.id.clone(), project.path.clone()))
            .collect(),
        entries: prepared_entries,
    })
}

/// Rejects a changed reviewed project scope before any global configuration is persisted.
/// 在任何全局配置持久化之前拒绝已发生变化的审查项目范围。
///
/// # Parameters
/// 参数
/// `guard` is the current service state under the existing configuration transaction and service locks.
/// `guard` 是既有配置事务锁与服务锁保护下的当前服务状态。
/// `prepared` owns the exact host catalog used to validate complete binding-plan coverage.
/// `prepared` 持有校验绑定计划完整覆盖时使用的准确宿主目录。
///
/// # Returns
/// 返回值
/// `Ok(())` for an unchanged scope or a save without binding review; otherwise an explicit reload error.
/// 范围未变或保存无需绑定审查时返回 `Ok(())`；否则返回明确的重新加载错误。
fn validate_prepared_project_binding_scope(
    guard: &crate::ServiceBridgeState,
    prepared: &PreparedProjectBindingPlan,
) -> Result<(), String> {
    if matches!(prepared, PreparedProjectBindingPlan::None) {
        return Ok(());
    }
    // Compare only fields consumed by the selected backend, not unrelated workbench revisions.
    // 仅比较所选后端实际消费的字段，不比较无关工作台版本。
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("工作区项目列表尚未就绪。"))?;
    // A changed full catalog invalidates the reviewed scope even when a newly added row is already bound.
    // 即使新增行已有绑定，完整目录变化仍使已审查范围失效。
    let unchanged = match prepared {
        PreparedProjectBindingPlan::None => true,
        PreparedProjectBindingPlan::Native {
            host_project_ids, ..
        } => {
            workspace
                .task_projects
                .iter()
                .map(|project| project.id.clone())
                .collect::<std::collections::BTreeSet<_>>()
                == *host_project_ids
        }
        PreparedProjectBindingPlan::External {
            host_project_paths, ..
        } => {
            workspace
                .task_projects
                .iter()
                .map(|project| (project.id.clone(), project.path.clone()))
                .collect::<std::collections::BTreeMap<_, _>>()
                == *host_project_paths
        }
    };
    if unchanged {
        Ok(())
    } else {
        Err(String::from(
            "VMM 项目绑定范围在预检期间已发生变化，请重新加载项目列表并确认绑定计划后保存。",
        ))
    }
}

/// Resolves one explicit external row decision without treating the global default as a lock.
/// 解析一条明确的外置逐行决定，且不把全局默认值当作强制锁定。
///
/// # Parameters
/// 参数
/// `effective` supplies the automatic-create Team/Space prefix.
/// `effective` 提供自动创建所用的 Team/Space 前缀。
/// `remote_projects` is the authoritative remote catalog used to validate stable IDs.
/// `remote_projects` 是校验稳定 ID 使用的权威远端目录。
/// `entry` is the exact user-confirmed row decision.
/// `entry` 是用户确认的准确逐行决定。
///
/// # Returns
/// 返回值
/// Returns a prepared bind-existing or create-and-bind action, or an explicit validation error.
/// 返回已准备的绑定已有或创建并绑定动作；校验失败时返回明确错误。
fn prepare_external_project_binding_action(
    effective: &vulcan_vmm::VmmEffectiveConfig,
    remote_projects: &[vulcan_vmm::ProjectEntry],
    entry: &vulcan_service_api::VmmProjectBindingPlanEntry,
) -> Result<PreparedExternalProjectBindingAction, String> {
    match entry.action.trim() {
        "bind_existing" => {
            let project_id = entry
                .target_project_id
                .filter(|project_id| *project_id > 0)
                .ok_or_else(|| {
                    format!(
                        "项目 `{}` 缺少待绑定的已有外置 VMM 项目。",
                        entry.vulcan_project_id
                    )
                })?;
            if !remote_projects
                .iter()
                .any(|project| project.project_id == project_id)
            {
                return Err(format!(
                    "项目 `{}` 选择的外置 VMM 项目 {} 不存在。",
                    entry.vulcan_project_id, project_id
                ));
            }
            Ok(PreparedExternalProjectBindingAction::BindExisting(
                project_id,
            ))
        }
        "create_and_bind" => {
            validate_external_project_leaf(entry)?;
            let project_name = entry
                .project_name
                .as_deref()
                .ok_or_else(|| String::from("已校验的外置 VMM 项目名称意外缺失"))?;
            Ok(PreparedExternalProjectBindingAction::CreateAndBind(
                format!("{}/{}", effective.external_project_prefix, project_name),
            ))
        }
        action => Err(format!(
            "项目 `{}` 使用了不支持的外置 VMM 绑定动作 `{action}`。",
            entry.vulcan_project_id
        )),
    }
}

/// Executes a validated binding plan and returns one aggregated user-facing failure message.
/// 执行已校验的绑定计划，并返回一条汇总后的用户可见失败消息。
async fn execute_prepared_project_binding_plan(
    state: &SharedServiceBridgeState,
    prepared: PreparedProjectBindingPlan,
) -> Option<String> {
    // failures retains every project failure so one unavailable target cannot block unrelated bindings.
    // failures 保留每个项目失败，使一个不可用目标不会阻断其他绑定。
    let mut failures = Vec::new();
    match prepared {
        PreparedProjectBindingPlan::None => return None,
        PreparedProjectBindingPlan::Native {
            maintenance,
            entries,
            ..
        } => {
            for entry in entries {
                let result = match entry.action {
                    PreparedNativeProjectBindingAction::BindExisting(project_id) => maintenance
                        .bind_existing_project_if_unbound(&entry.vulcan_project_id, project_id)
                        .await
                        .map(|_| ()),
                    PreparedNativeProjectBindingAction::CreateAndBind(project_name) => maintenance
                        .create_and_bind_project_if_unbound(&entry.vulcan_project_id, &project_name)
                        .await
                        .map(|_| ()),
                };
                if let Err(error) = result {
                    failures.push(format!("项目 `{}`：{error}", entry.vulcan_project_id));
                }
            }
        }
        PreparedProjectBindingPlan::External { layer, entries, .. } => {
            for entry in entries {
                let remote_project_id = match entry.action {
                    PreparedExternalProjectBindingAction::BindExisting(project_id) => {
                        Ok(project_id)
                    }
                    PreparedExternalProjectBindingAction::CreateAndBind(project_path) => {
                        ensure_external_vmm_project(layer.clone(), project_path)
                            .await
                            .map(|project| project.project_id)
                    }
                };
                let result = match remote_project_id {
                    Ok(project_id) => persist_external_project_id_if_unbound(
                        state,
                        &entry.project_config_path,
                        project_id,
                    )
                    .await
                    .map(|_| ()),
                    Err(error) => Err(error),
                };
                if let Err(error) = result {
                    failures.push(format!("项目 `{}`：{error}", entry.vulcan_project_id));
                }
            }
        }
    }
    format_project_binding_failures(&failures)
}

/// Formats per-project binding failures after global enablement has already succeeded.
/// 在全局启用已经成功后格式化逐项目绑定失败。
fn format_project_binding_failures(failures: &[String]) -> Option<String> {
    (!failures.is_empty()).then(|| {
        format!(
            "VMM 已启用，但以下项目绑定失败；其他项目的成功绑定已保留，可在项目设置中重新绑定：{}",
            failures.join("；")
        )
    })
}

/// Validates that one binding plan exactly covers the supplied unbound project ids.
/// 校验一份绑定计划是否准确覆盖给定的未绑定项目标识。
fn validate_binding_plan_coverage<'a>(
    unbound_projects: &std::collections::BTreeMap<String, String>,
    plan: &'a [vulcan_service_api::VmmProjectBindingPlanEntry],
    label: &str,
) -> Result<
    std::collections::BTreeMap<&'a str, &'a vulcan_service_api::VmmProjectBindingPlanEntry>,
    String,
> {
    let mut decisions = std::collections::BTreeMap::new();
    for entry in plan {
        if decisions
            .insert(entry.vulcan_project_id.as_str(), entry)
            .is_some()
        {
            return Err(format!(
                "{label} 绑定计划重复包含项目 `{}`。",
                entry.vulcan_project_id
            ));
        }
    }
    let expected_ids = unbound_projects
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let actual_ids = decisions.keys().copied().collect::<Vec<_>>();
    if expected_ids != actual_ids {
        return Err(format!(
            "{label} 绑定计划必须准确覆盖当前未绑定项目。未绑定项目：[{}]。",
            expected_ids.join(", ")
        ));
    }
    Ok(decisions)
}

/// Validates one external project leaf without allowing hierarchy injection.
/// 校验一个外置项目末级名称，不允许注入额外层级。
fn validate_external_project_leaf(
    entry: &vulcan_service_api::VmmProjectBindingPlanEntry,
) -> Result<(), String> {
    let project_name = entry
        .project_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("项目 `{}` 缺少外置 VMM 项目名称。", entry.vulcan_project_id))?;
    if project_name.contains('/') {
        return Err(format!(
            "项目 `{}` 的外置 VMM 项目名称不得包含 `/`。",
            entry.vulcan_project_id
        ));
    }
    if project_name.chars().count() > 128 {
        return Err(format!(
            "项目 `{}` 的外置 VMM 项目名称超过 128 个字符。",
            entry.vulcan_project_id
        ));
    }
    Ok(())
}

/// Loads the exact durable project catalog from an external VMM endpoint.
/// 从外置 VMM 端点加载准确的持久项目目录。
async fn load_external_vmm_projects(
    layer: vulcan_vmm::VmmConfigLayer,
) -> Result<Vec<vulcan_vmm::ProjectEntry>, String> {
    match vulcan_service_runtime::vmm_management::run_vmm_workbench_action(
        vulcan_service_runtime::vmm_management::VmmWorkbenchAction::LoadUsersAndProjects,
        layer,
        String::new(),
        String::new(),
        String::new(),
    )
    .await?
    {
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::UsersAndProjects {
            projects,
            ..
        } => Ok(projects.projects),
        _ => Err(String::from("外置 VMM 返回了不匹配的项目目录结果。")),
    }
}

/// Ensures one external canonical project path and returns its durable identity.
/// 确保一个外置规范项目路径存在，并返回其持久身份。
async fn ensure_external_vmm_project(
    layer: vulcan_vmm::VmmConfigLayer,
    project_path: String,
) -> Result<vulcan_vmm::ProjectEntry, String> {
    match vulcan_service_runtime::vmm_management::run_vmm_workbench_action(
        vulcan_service_runtime::vmm_management::VmmWorkbenchAction::EnsureProject {
            project_path,
            confirm_create: true,
        },
        layer,
        String::new(),
        String::new(),
        String::new(),
    )
    .await?
    {
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::EnsureProject(response) => {
            response
                .project
                .ok_or_else(|| format!("外置 VMM 未返回已确保项目：{}", response.message))
        }
        _ => Err(String::from("外置 VMM 返回了不匹配的项目创建结果。")),
    }
}

/// Persists an automatic external binding only when the project has no stable remote identity.
/// 仅当项目没有稳定远端身份时持久化自动外置绑定。
///
/// # Parameters
/// `state` owns the project-layer cache and exact hook projection.
/// `state` 持有项目层缓存与准确 hook 投影。
/// `project_path` is the authoritative VulcanCode workspace path.
/// `project_path` 是权威 VulcanCode 工作区路径。
/// `remote_project_id` is the default remote project selected or created by automatic setup.
/// `remote_project_id` 是自动配置选择或创建的默认远端项目。
///
/// # Returns
/// Returns the effective durable project id; a concurrent explicit binding always wins.
/// 返回最终耐久项目 ID；并发完成的显式绑定始终优先。
pub(super) async fn persist_external_project_id_if_unbound(
    state: &SharedServiceBridgeState,
    project_path: &str,
    remote_project_id: u64,
) -> Result<u64, String> {
    if remote_project_id == 0 {
        return Err(String::from("外置 VMM 项目 ID 必须是正整数。"));
    }
    let project_config_path = project_path.to_string();
    mutate_project_configuration(
        &PathBuf::from(&project_config_path),
        "保存外置 VMM 自动项目 ID 绑定失败",
        |project_configuration| {
            if project_configuration
                .vmm
                .project_id
                .filter(|project_id| *project_id > 0)
                .is_none()
            {
                project_configuration.vmm.project_id = Some(remote_project_id);
            }
            Ok(())
        },
        |project_configuration| {
            let project_id = project_configuration
                .vmm
                .project_id
                .filter(|project_id| *project_id > 0)
                .ok_or_else(|| String::from("外置 VMM 自动绑定提交后缺少有效项目 ID。"))?;
            update_cached_project_vmm_layer(
                &project_config_path,
                project_configuration.vmm.clone(),
            );
            refresh_exact_vmm_hook_project_projection(
                state,
                &project_config_path,
                &project_configuration.vmm,
            )?;
            Ok(project_id)
        },
    )
    .await
}

/// Loads one persisted project VMM layer and resolves its effective editable fallback.
/// 加载单个已持久化的项目 VMM 层，并解析其可编辑回退值。
pub(crate) fn load_project_vmm_config(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<VmmConfigDraftSummary, String> {
    let project_path = vmm_project_scope_path(state, Some(project_id))?;
    let provider_configuration = provider_configuration_from_state(state)?;
    let project_layer = cached_project_vmm_layer(&project_path);
    let project_effective = vulcan_vmm::VmmEffectiveConfig::resolve(
        &provider_configuration.vmm,
        Some(&project_layer),
        None,
    );
    Ok(vmm_config_draft_summary(
        &project_layer,
        &project_effective,
        !project_layer.is_empty(),
        false,
    ))
}

/// Converts a WebUI VMM draft into a persisted configuration layer.
/// 将 WebUI VMM 草稿转换为持久化配置层。
///
/// # Parameters
/// 参数
/// `draft` is the editable WebUI VMM configuration summary.
/// `draft` 是 WebUI 可编辑的 VMM 配置摘要。
/// `include_transport` controls whether transport fields should be persisted for this layer.
/// `include_transport` 控制当前配置层是否持久化传输字段。
///
/// # Returns
/// 返回值
/// The persisted VMM configuration layer represented by the draft.
/// 返回该草稿表示的持久化 VMM 配置层。
pub(crate) fn vmm_config_layer_from_draft(
    draft: &VmmConfigDraftSummary,
    include_transport: bool,
) -> Result<vulcan_vmm::VmmConfigLayer, String> {
    let layer = vulcan_vmm::VmmConfigLayer {
        backend_mode: include_transport
            .then(|| vmm_backend_mode_from_draft(&draft.backend_mode))
            .transpose()?,
        native_project_binding_mode: include_transport
            .then(|| vmm_native_binding_mode_from_draft(&draft.native_project_binding_mode))
            .transpose()?,
        external_project_binding_mode: include_transport
            .then(|| vmm_native_binding_mode_from_draft(&draft.external_project_binding_mode))
            .transpose()?,
        native_fixed_project_id: include_transport
            .then(|| {
                vmm_parse_optional_positive_u64(
                    &draft.native_fixed_project_id,
                    "原生 VMM 固定项目 ID",
                )
            })
            .transpose()?
            .flatten(),
        external_project_prefix: include_transport
            .then(|| vmm_external_project_prefix(&draft.external_project_prefix))
            .transpose()?,
        enabled: Some(draft.enabled),
        grpc_target: include_transport
            .then(|| vmm_non_empty_text(&draft.grpc_target))
            .flatten(),
        grpc_api_key: include_transport
            .then(|| vmm_non_empty_text(&draft.grpc_api_key))
            .flatten(),
        management_base_url: include_transport
            .then(|| vmm_non_empty_text(&draft.management_base_url))
            .flatten(),
        management_bearer_token: include_transport
            .then(|| vmm_non_empty_text(&draft.management_bearer_token))
            .flatten(),
        management_timeout_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.management_timeout_ms, "VMM 管理请求超时"))
            .transpose()?
            .flatten(),
        handshake_timeout_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.handshake_timeout_ms, "VMM 握手超时"))
            .transpose()?
            .flatten(),
        receive_timeout_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.receive_timeout_ms, "VMM 请求超时"))
            .transpose()?
            .flatten(),
        max_receive_message_bytes: include_transport
            .then(|| {
                vmm_parse_optional_usize(&draft.max_receive_message_bytes, "VMM 最大接收字节数")
            })
            .transpose()?
            .flatten(),
        grpc_keepalive_time_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.grpc_keepalive_time_ms, "VMM keepalive 间隔"))
            .transpose()?
            .flatten(),
        grpc_keepalive_timeout_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.grpc_keepalive_timeout_ms, "VMM keepalive 超时"))
            .transpose()?
            .flatten(),
        grpc_keepalive_permit_without_calls: include_transport
            .then_some(draft.grpc_keepalive_permit_without_calls),
        user_id: vmm_parse_optional_positive_u64(&draft.user_id, "VMM 用户 ID")?,
        project_id: vmm_parse_optional_positive_u64(&draft.project_id, "VMM 项目 ID")?,
        visible_memory_injection: Some(false),
        implicit_memory_turns: vmm_parse_optional_u32(
            &draft.implicit_memory_turns,
            "临时记忆 TTL",
        )?,
        profile_refresh_turns: vmm_parse_optional_u32(
            &draft.profile_refresh_turns,
            "画像刷新间隔",
        )?,
        profile_node_limit: vmm_parse_optional_u32(&draft.profile_node_limit, "画像节点上限")?,
        session_compact_recall: Some(true),
        precheck_enabled: vmm_policy_from_draft_mode(
            &draft.precheck_mode,
            draft.precheck_enabled,
            include_transport,
            "前置记忆注入",
        )?,
        memory_writeback_enabled: vmm_policy_from_draft_mode(
            &draft.memory_writeback_mode,
            draft.memory_writeback_enabled,
            include_transport,
            "后置记忆提交",
        )?,
        ai_tools_enabled: Some(draft.ai_tools_enabled),
        injection_tag: vmm_non_empty_text(&draft.injection_tag),
        injection_description: vmm_non_empty_text(&draft.injection_description),
    };
    validate_external_management_layer(&layer)?;
    Ok(layer)
}

/// Parses the explicit global backend selector without accepting ambiguous aliases.
/// 解析明确的全局后端选择器，不接受含义不明确的别名。
fn vmm_backend_mode_from_draft(value: &str) -> Result<vulcan_vmm::VmmBackendMode, String> {
    match value.trim() {
        "" | "native" => Ok(vulcan_vmm::VmmBackendMode::Native),
        "external_grpc" => Ok(vulcan_vmm::VmmBackendMode::ExternalGrpc),
        other => Err(format!(
            "VMM 后端模式无效：{other}，只允许 native 或 external_grpc"
        )),
    }
}

/// Parses the global native project binding policy without accepting aliases.
/// 解析全局原生项目绑定策略，不接受别名。
fn vmm_native_binding_mode_from_draft(
    value: &str,
) -> Result<vulcan_vmm::VmmNativeProjectBindingMode, String> {
    match value.trim() {
        "fixed" => Ok(vulcan_vmm::VmmNativeProjectBindingMode::Fixed),
        "per_project" => Ok(vulcan_vmm::VmmNativeProjectBindingMode::PerProject),
        value => Err(format!("不支持的原生 VMM 项目绑定策略：{value}")),
    }
}

/// Validates the external automatic-project Team/Space prefix.
/// 校验外置自动项目使用的 Team/Space 前缀。
fn vmm_external_project_prefix(value: &str) -> Result<String, String> {
    let segments = value.split('/').map(str::trim).collect::<Vec<_>>();
    if segments.len() != 2 || segments.iter().any(|segment| segment.is_empty()) {
        return Err(String::from(
            "外置 VMM 项目前缀必须使用非空 team/space 格式。",
        ));
    }
    if segments.iter().any(|segment| segment.chars().count() > 128) {
        return Err(String::from(
            "外置 VMM 项目前缀的单个层级不得超过 128 个字符。",
        ));
    }
    Ok(format!("{}/{}", segments[0], segments[1]))
}

/// Validates the optional external management endpoint and secret as one inseparable pair.
/// 将可选外部管理端点与密钥作为不可分割的一组进行校验。
fn validate_external_management_layer(layer: &vulcan_vmm::VmmConfigLayer) -> Result<(), String> {
    let base_url = layer.management_base_url.as_deref().map(str::trim);
    let token = layer.management_bearer_token.as_deref().map(str::trim);
    match (
        base_url.filter(|value| !value.is_empty()),
        token.filter(|value| !value.is_empty()),
    ) {
        (None, None) => return Ok(()),
        (Some(_), None) => return Err(String::from("外部 VMM 管理地址已填写，但缺少管理 Token。")),
        (None, Some(_)) => {
            return Err(String::from("外部 VMM 管理 Token 已填写，但缺少管理地址。"));
        }
        (Some(_), Some(_)) => {}
    }
    let base_url = reqwest::Url::parse(base_url.unwrap_or_default())
        .map_err(|error| format!("外部 VMM 管理地址无效：{error}"))?;
    if !base_url.username().is_empty() || base_url.password().is_some() {
        return Err(String::from("外部 VMM 管理地址不能包含账号或密码。"));
    }
    if base_url.query().is_some() || base_url.fragment().is_some() {
        return Err(String::from("外部 VMM 管理地址不能包含查询参数或片段。"));
    }
    let loopback = base_url
        .host_str()
        .is_some_and(|host| matches!(host, "127.0.0.1" | "localhost" | "::1"));
    if base_url.scheme() != "https" && !(base_url.scheme() == "http" && loopback) {
        return Err(String::from("非回环 VMM 管理地址必须使用 HTTPS。"));
    }
    if layer
        .management_timeout_ms
        .is_some_and(|value| !(1_000..=120_000).contains(&value))
    {
        return Err(String::from(
            "VMM 管理请求超时必须位于 1000 到 120000 毫秒之间。",
        ));
    }
    Ok(())
}

/// Converts one WebUI policy mode into an optional persisted layer override.
/// 将单个 WebUI 策略模式转换为可选持久化层级覆盖。
fn vmm_policy_from_draft_mode(
    mode: &str,
    enabled: bool,
    force_explicit: bool,
    label: &str,
) -> Result<Option<bool>, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "" => Ok(Some(enabled)),
        "inherit" | "inherit_project" | "inheritproject" | "inherit_system" | "inheritsystem" => {
            Ok(force_explicit.then_some(enabled))
        }
        "enabled" | "on" | "true" => Ok(Some(true)),
        "disabled" | "off" | "false" => Ok(Some(false)),
        other => Err(format!("{label} 策略模式不受支持：{other}")),
    }
}

/// Parses one profile target key from WebUI.
/// 从 WebUI 解析单个画像目标键。
pub(crate) fn parse_vmm_profile_target(value: &str) -> Result<vulcan_vmm::ProfileTarget, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "user" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetUser),
        "project" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetProject),
        "team" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetTeam),
        "space" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetSpace),
        "all" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetAll),
        other => Err(format!("不支持的 VMM 画像目标：{other}")),
    }
}

/// Reads one optional string from an action payload.
/// 从动作载荷读取一个可选字符串。
pub(crate) fn vmm_payload_text(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Reads one boolean from an action payload.
/// 从动作载荷读取一个布尔值。
pub(crate) fn vmm_payload_bool(payload: &Value, key: &str) -> bool {
    payload.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Reads one optional u32 from an action payload.
/// 从动作载荷读取一个可选 u32。
pub(crate) fn vmm_payload_u32(payload: &Value, key: &str) -> Option<u32> {
    payload
        .get(key)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
        })
        .and_then(|value| u32::try_from(value).ok())
}

/// Reads one optional i32 from an action payload.
/// 从动作载荷读取一个可选 i32。
pub(crate) fn vmm_payload_i32(payload: &Value, key: &str) -> Option<i32> {
    payload
        .get(key)
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str()?.trim().parse::<i64>().ok())
        })
        .and_then(|value| i32::try_from(value).ok())
}

/// Reads one optional u64 from an action payload.
/// 从动作载荷读取一个可选 u64。
pub(crate) fn vmm_payload_u64(payload: &Value, key: &str) -> Option<u64> {
    payload.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
    })
}

/// Reads a string list from an action payload.
/// 从动作载荷读取字符串列表。
pub(crate) fn vmm_payload_string_list(payload: &Value, key: &str) -> Vec<String> {
    if let Some(values) = payload.get(key).and_then(Value::as_array) {
        return values
            .iter()
            .filter_map(Value::as_str)
            .flat_map(vmm_split_text_list)
            .collect();
    }
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(vmm_split_text_list)
        .unwrap_or_default()
}

/// Reads a u64 list from an action payload.
/// 从动作载荷读取 u64 列表。
pub(crate) fn vmm_payload_u64_list(payload: &Value, key: &str) -> Result<Vec<u64>, String> {
    if let Some(values) = payload.get(key).and_then(Value::as_array) {
        return values
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
                    .ok_or_else(|| format!("无法解析 VMM u64 id：{value}"))
            })
            .collect();
    }
    vmm_payload_string_list(payload, key)
        .into_iter()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("无法解析 VMM u64 id：{value}"))
        })
        .collect()
}

/// Splits compact text list syntax used by the WebUI forms.
/// 拆分 WebUI 表单使用的紧凑文本列表语法。
pub(crate) fn vmm_split_text_list(value: &str) -> Vec<String> {
    value
        .split(['|', ',', '\n'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Returns normalized non-empty text.
/// 返回规范化后的非空文本。
pub(crate) fn vmm_non_empty_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Parses an optional u64 draft field.
/// 解析可选 u64 草稿字段。
pub(crate) fn vmm_parse_optional_u64(value: &str, label: &str) -> Result<Option<u64>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<u64>()
        .map(Some)
        .map_err(|_| format!("{label} 必须是非负整数。"))
}

/// Parses an optional positive u64 identity field.
/// 解析可选的正整数 u64 身份字段。
pub(crate) fn vmm_parse_optional_positive_u64(
    value: &str,
    label: &str,
) -> Result<Option<u64>, String> {
    let parsed = vmm_parse_optional_u64(value, label)?;
    match parsed {
        Some(0) => Err(format!("{label} 必须是正整数。")),
        value => Ok(value),
    }
}

/// Parses an optional u32 draft field.
/// 解析可选 u32 草稿字段。
pub(crate) fn vmm_parse_optional_u32(value: &str, label: &str) -> Result<Option<u32>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|_| format!("{label} 必须是 32 位非负整数。"))
}

/// Parses an optional usize draft field.
/// 解析可选 usize 草稿字段。
pub(crate) fn vmm_parse_optional_usize(value: &str, label: &str) -> Result<Option<usize>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<usize>()
        .map(Some)
        .map_err(|_| format!("{label} 必须是非负整数。"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the external review owns exact configuration paths, but not titles or display order.
    /// 验证外置审查归属于准确配置路径，而不依赖标题或显示顺序。
    /// Takes no input and returns after checking accepted metadata edits and rejected scope changes.
    /// 不接收输入，在检查允许的元数据编辑与拒绝的范围变化后返回。
    #[test]
    fn external_binding_scope_checks_paths_without_freezing_display_metadata() {
        // This state fixture is used only by the pure validator; no external connection is opened.
        // 此状态夹具仅用于纯校验器，不打开任何外置连接。
        let mut guard = crate::tests::test_support::test_service_state_with_parent_session(
            uuid::Uuid::new_v4(),
        );
        // Two exact catalog entries make order invariance observable.
        // 两项准确目录条目使顺序无关性可被观察。
        let projects = vec![
            vulcan_workbench_model::TaskProjectSummary::new(
                "project-a",
                "Project A",
                "D:/project-a",
                false,
                1,
                None,
                None,
                Vec::new(),
            ),
            vulcan_workbench_model::TaskProjectSummary::new(
                "project-b",
                "Project B",
                "D:/project-b",
                false,
                2,
                None,
                None,
                Vec::new(),
            ),
        ];
        // The external prepared plan captures the same identity/path pairs as production preflight.
        // 已准备外置计划捕获与生产预检相同的身份及路径对。
        let prepared = PreparedProjectBindingPlan::External {
            layer: vulcan_vmm::VmmConfigLayer::default(),
            host_project_paths: projects
                .iter()
                .map(|project| (project.id.clone(), project.path.clone()))
                .collect(),
            entries: Vec::new(),
        };
        guard
            .workspace_snapshot
            .as_mut()
            .expect("binding scope validation fixture value")
            .task_projects = projects.clone();
        validate_prepared_project_binding_scope(&guard, &prepared).expect("unchanged catalog");
        {
            // Display-only changes must not force another binding review.
            // 仅显示变化不得强制再次审查绑定。
            let workspace = guard
                .workspace_snapshot
                .as_mut()
                .expect("binding scope validation fixture value");
            workspace.task_projects[0].title = "Renamed".to_owned();
            workspace.task_projects.reverse();
        }
        validate_prepared_project_binding_scope(&guard, &prepared).expect("display edits");
        guard
            .workspace_snapshot
            .as_mut()
            .expect("binding scope validation fixture value")
            .task_projects[0]
            .path = "D:/relocated-project-b".to_owned();
        assert!(
            validate_prepared_project_binding_scope(&guard, &prepared)
                .expect_err("binding scope validation fixture must reject invalid input")
                .contains("重新加载")
        );
        guard
            .workspace_snapshot
            .as_mut()
            .expect("binding scope validation fixture value")
            .task_projects = projects.clone();
        guard
            .workspace_snapshot
            .as_mut()
            .expect("binding scope validation fixture value")
            .task_projects
            .pop();
        assert!(validate_prepared_project_binding_scope(&guard, &prepared).is_err());
        guard
            .workspace_snapshot
            .as_mut()
            .expect("binding scope validation fixture value")
            .task_projects = projects.clone();
        // Adding another stable ID is invalid even when it shares an existing source path.
        // 即使共享已有源码路径，新增另一个稳定 ID 也会使原审查失效。
        let mut added = projects[0].clone();
        added.id = "project-c".to_owned();
        guard
            .workspace_snapshot
            .as_mut()
            .expect("binding scope validation fixture value")
            .task_projects
            .push(added);
        assert!(validate_prepared_project_binding_scope(&guard, &prepared).is_err());
        guard.workspace_snapshot = None;
        assert!(validate_prepared_project_binding_scope(&guard, &prepared).is_err());
        validate_prepared_project_binding_scope(&guard, &PreparedProjectBindingPlan::None)
            .expect("ordinary saves do not require project review");
    }

    /// Builds one exact binding-plan entry for validation tests.
    /// 为校验测试构造一个准确的绑定计划条目。
    fn binding_entry(vulcan_project_id: &str) -> vulcan_service_api::VmmProjectBindingPlanEntry {
        vulcan_service_api::VmmProjectBindingPlanEntry {
            vulcan_project_id: vulcan_project_id.to_string(),
            action: String::from("bind_existing"),
            project_name: None,
            target_project_id: Some(1),
        }
    }

    /// Exact coverage validation rejects duplicate project decisions before persistence.
    /// 准确覆盖校验会在持久化前拒绝重复项目决定。
    #[test]
    fn binding_plan_coverage_rejects_duplicate_decisions() {
        // unbound_projects is the authoritative set that must be covered exactly once.
        // unbound_projects 是必须且只能覆盖一次的权威未绑定集合。
        let unbound_projects = std::collections::BTreeMap::from([(
            String::from("project-a"),
            String::from("D:/project-a"),
        )]);
        // plan deliberately repeats the same stable VulcanCode project tag.
        // plan 有意重复同一个稳定 VulcanCode 项目标签。
        let plan = vec![binding_entry("project-a"), binding_entry("project-a")];

        let error = validate_binding_plan_coverage(&unbound_projects, &plan, "测试 VMM")
            .expect_err("duplicate decisions must fail validation");

        assert!(error.contains("重复包含项目 `project-a`"));
    }

    /// Failure aggregation keeps all project errors and explains that enablement succeeded.
    /// 失败汇总会保留全部项目错误，并说明启用状态已经成功。
    #[test]
    fn project_binding_failure_message_preserves_every_failure() {
        // failures models independent per-project failures after global configuration persistence.
        // failures 模拟全局配置持久化后的独立逐项目失败。
        let failures = vec![
            String::from("项目 `project-a`：远端不可用"),
            String::from("项目 `project-b`：写入失败"),
        ];

        let message = format_project_binding_failures(&failures)
            .expect("non-empty failures must produce one message");

        assert!(message.starts_with("VMM 已启用"));
        assert!(message.contains("项目 `project-a`：远端不可用"));
        assert!(message.contains("项目 `project-b`：写入失败"));
        assert!(message.contains("可在项目设置中重新绑定"));
        assert_eq!(format_project_binding_failures(&[]), None);
    }

    /// Binding plans are mandatory for activation and backend switches but not ordinary repairs.
    /// 绑定计划在启用和后端切换时必需，但同一活动后端的普通修正不应被阻断。
    #[test]
    fn binding_plan_requirement_distinguishes_activation_from_ordinary_save() {
        let disabled_native = vulcan_vmm::VmmEffectiveConfig {
            enabled: false,
            backend_mode: vulcan_vmm::VmmBackendMode::Native,
            ..vulcan_vmm::VmmEffectiveConfig::default()
        };
        let enabled_native = vulcan_vmm::VmmEffectiveConfig {
            enabled: true,
            backend_mode: vulcan_vmm::VmmBackendMode::Native,
            ..vulcan_vmm::VmmEffectiveConfig::default()
        };
        let enabled_external = vulcan_vmm::VmmEffectiveConfig {
            enabled: true,
            backend_mode: vulcan_vmm::VmmBackendMode::ExternalGrpc,
            ..vulcan_vmm::VmmEffectiveConfig::default()
        };

        assert!(should_prepare_project_binding_plan(
            &disabled_native,
            &enabled_native,
            false
        ));
        assert!(should_prepare_project_binding_plan(
            &enabled_native,
            &enabled_external,
            false
        ));
        assert!(should_prepare_project_binding_plan(
            &enabled_native,
            &enabled_native,
            true
        ));
        assert!(!should_prepare_project_binding_plan(
            &enabled_native,
            &enabled_native,
            false
        ));
    }

    /// External fixed mode remains a default and accepts an explicit alternate existing project.
    /// 外置固定模式仍然只是默认值，并接受明确选择的其他已有项目。
    #[test]
    fn external_fixed_mode_preserves_explicit_row_override() {
        let effective = vulcan_vmm::VmmEffectiveConfig {
            external_project_binding_mode: vulcan_vmm::VmmNativeProjectBindingMode::Fixed,
            project_id: Some(7),
            external_project_prefix: String::from("vulcan/space"),
            ..vulcan_vmm::VmmEffectiveConfig::default()
        };
        let remote_projects = vec![
            vulcan_vmm::ProjectEntry {
                project_id: 7,
                ..vulcan_vmm::ProjectEntry::default()
            },
            vulcan_vmm::ProjectEntry {
                project_id: 42,
                ..vulcan_vmm::ProjectEntry::default()
            },
        ];
        let entry = vulcan_service_api::VmmProjectBindingPlanEntry {
            vulcan_project_id: String::from("project-a"),
            action: String::from("bind_existing"),
            project_name: None,
            target_project_id: Some(42),
        };

        let action = prepare_external_project_binding_action(&effective, &remote_projects, &entry)
            .expect("an explicit alternate project must remain valid in fixed-default mode");

        assert_eq!(
            action,
            PreparedExternalProjectBindingAction::BindExisting(42)
        );
    }
}
