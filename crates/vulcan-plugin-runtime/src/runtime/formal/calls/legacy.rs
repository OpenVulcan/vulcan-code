//! Retained guarded lifecycle calls with explicit physical-retirement acknowledgement.
//! 显式确认物理退役的受保护生命周期保留调用。

use super::*;
use crate::VulcanPluginDeactivationReservation;

/// Original ownership is either a declared real call or a verified absent declaration.
/// 原所有权为已声明真实调用或已核验缺失声明。
enum LegacyCallOwnership {
    /// Actual parent and record remain unchanged through cleanup recovery.
    /// 实际父级及记录在清理恢复期间保持不变。
    Declared {
        /// The original formal parent owns all physical operation identities.
        /// 原正式父级持有全部物理操作身份。
        owner: Arc<ManagedFormalRuntime>,
        /// The registered call owns the only execution source.
        /// 已注册调用持有唯一执行来源。
        call: Arc<FormalCall>,
    },
    /// Absent callbacks retain the guard without requiring a formal executor.
    /// 缺失回调保留护栏，不要求正式执行器。
    NoDeclaration(Arc<LegacyLifecycleExecution>),
}

/// Explicitly reserved managed-Uninstall handle; waiting never acknowledges ownership.
/// 明确预留受管卸载句柄；等待绝不确认所有权。
pub struct VulcanPluginLegacyLifecycleCall {
    /// The actual original manager remains available for same-call cleanup.
    /// 实际原管理器持续可用于同调用清理。
    manager: VulcanPluginRuntimeManager,
    /// Successful explicit acknowledgement consumes only this original ownership.
    /// 成功显式确认仅消费此原所有权。
    ownership: Option<LegacyCallOwnership>,
}

/// Cancellation of an observer retains every original resource and evidence record.
/// 观测者取消时保留每个原资源及证据记录。
struct LegacyObservation {
    /// The actual call is never acknowledged by this observer.
    /// 此观测者绝不确认实际调用。
    call: Arc<FormalCall>,
    /// Normal observation disarms cancellation without claiming retirement.
    /// 正常观测解除取消，不宣称退役。
    completed: bool,
}

impl Drop for LegacyObservation {
    /// Request cancellation on interrupted waiting without releasing the original guard.
    /// 等待中断时请求取消，不释放原护栏。
    fn drop(&mut self) {
        if !self.completed {
            self.call.cancelled.store(true, Ordering::Release);
        }
    }
}

impl VulcanPluginLegacyLifecycleCall {
    /// Start the registered original input once; repeated starts never replay callbacks.
    /// 仅启动已登记原输入一次；重复启动绝不重放回调。
    /// Returns executor or ownership errors while retaining unstarted input.
    /// 返回执行器或所有权错误，同时保留未启动输入。
    pub fn start(&self) -> Result<(), VulcanPluginRuntimeError> {
        match self.ownership.as_ref().ok_or_else(consumed_error)? {
            LegacyCallOwnership::Declared { owner, call } => {
                self.manager.start_registered_formal_call(owner, call)
            }
            LegacyCallOwnership::NoDeclaration(authority) => {
                // Preserve pending ownership until the same physical original passes coordination admission.
                // 相同物理原实例通过协调准入之前保留待处理所有权。
                let registration = self
                    .manager
                    .state
                    .session_registration_gate
                    .lock()
                    .map_err(|_| {
                        VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
                    })?;
                self.manager.require_routing_publication_locked(
                    &registration,
                    Some(
                        crate::runtime::routing::ActivationRoutingOriginal::LegacyMutation(
                            authority.mutation_guard(),
                        ),
                    ),
                )?;
                authority.validate_owner(&self.manager.state)
            }
        }
    }

    /// Observe the original business result; cancellation requests cleanup but never acknowledges it.
    /// 观测原业务结果；取消请求清理，但绝不确认清理。
    /// A returned error can still require physical retirement and explicit cleanup recovery.
    /// 返回错误后仍可能需要物理退役及显式清理恢复。
    pub async fn observe(&self) -> Result<(), VulcanPluginRuntimeError> {
        match self.ownership.as_ref().ok_or_else(consumed_error)? {
            LegacyCallOwnership::NoDeclaration(authority) => {
                authority.validate_owner(&self.manager.state)
            }
            LegacyCallOwnership::Declared { call, .. } => {
                tokio::runtime::Handle::try_current().map_err(|_| {
                    VulcanPluginRuntimeError::new(
                        "Legacy lifecycle observation requires a Tokio runtime.",
                    )
                })?;
                // Retain original call ownership during this observation interval.
                // 在此观测区间保留原调用所有权。
                let mut observer = LegacyObservation {
                    call: Arc::clone(call),
                    completed: false,
                };
                loop {
                    if let Some(result) = call.lock()?.result.clone() {
                        observer.completed = true;
                        return result.map(|_| ());
                    }
                    tokio::time::sleep(OBSERVATION_INTERVAL).await;
                }
            }
        }
    }

    /// Drain only the original call using its parent's finite budget, without replay or acknowledgement.
    /// 使用原父级有限预算仅排空原调用，不重放或确认。
    /// Returns actual cleanup evidence; unfinished resources remain owned by this handle and registry.
    /// 返回实际清理证据；未完成资源仍由此句柄及注册表持有。
    pub async fn drain_cleanup(&self) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        match self.ownership.as_ref().ok_or_else(consumed_error)? {
            LegacyCallOwnership::NoDeclaration(authority) => {
                authority.validate_owner(&self.manager.state)?;
                Ok(VulcanPluginDrainReport::default())
            }
            LegacyCallOwnership::Declared { owner, call } => {
                tokio::runtime::Handle::try_current().map_err(|_| {
                    VulcanPluginRuntimeError::new(
                        "Legacy lifecycle cleanup requires a Tokio runtime.",
                    )
                })?;
                self.manager
                    .drain_selected_formal_calls(owner, vec![Arc::clone(call)])
                    .await
            }
        }
    }

    /// Acknowledge only sole proven physical retirement and return the original business result.
    /// 仅确认唯一已证明物理退役，并返回原业务结果。
    /// Outer errors retain ownership; inner errors describe an already-retired callback failure.
    /// 外层错误保留所有权；内层错误描述已退役回调失败。
    pub fn confirm_retired(
        &mut self,
    ) -> Result<Result<(), VulcanPluginRuntimeError>, VulcanPluginRuntimeError> {
        // Preserve the original business result independently of retirement acknowledgement.
        // 独立于退役确认保留原业务结果。
        let result = match self.ownership.as_ref().ok_or_else(consumed_error)? {
            LegacyCallOwnership::NoDeclaration(authority) => {
                authority.validate_owner(&self.manager.state)?;
                Ok(())
            }
            LegacyCallOwnership::Declared { owner, call } => {
                let CallExecutionSource::LegacyLifecycle(authority) = &call.source else {
                    return Err(VulcanPluginRuntimeError::new(
                        "Original call lacks legacy lifecycle authority.",
                    ));
                };
                authority.validate_owner(&self.manager.state)?;
                // Only this original record can be acknowledged; unrelated housekeeping cannot change its result.
                // 仅此原记录可以被确认；无关维护不能改变其结果。
                self.manager
                    .acknowledge_original_internal_call(owner, call)?
                    .map(|_| ())
            }
        };
        self.ownership.take();
        Ok(result)
    }
}

/// Return an explicit error for a handle whose original retirement was acknowledged.
/// 为原退役已被确认的句柄返回明确错误。
fn consumed_error() -> VulcanPluginRuntimeError {
    VulcanPluginRuntimeError::new("Original legacy lifecycle was already acknowledged.")
}

impl VulcanPluginRuntimeManager {
    /// Register an inert managed Uninstall using this retained reservation's same actual namespace File.
    /// 使用此保留预留的同一实际命名空间文件，登记未启动受管卸载。
    /// original must precede publication and select the complete disabled managed package; returns one original call handle.
    /// original 必须先于发布，且选择完整停用受管包；返回单个原调用句柄。
    /// The host must strongly retain this handle before start and recover it without calling this factory again.
    /// 宿主必须在启动之前强保留此句柄，并恢复此句柄，不再次调用本工厂。
    pub fn prepare_deactivation_lifecycle_call(
        &self,
        original: &VulcanPluginDeactivationReservation,
    ) -> Result<VulcanPluginLegacyLifecycleCall, VulcanPluginRuntimeError> {
        // Serialize literal manager/catalog identity before borrowing the same original namespace.
        // 借用同一原命名空间之前，串行化字面管理器／目录身份。
        let authority = {
            // The existing publication gate excludes manager descriptor changes during original source binding.
            // 原来源绑定期间，既有发布门排除管理器描述符变更。
            let _registration = self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
            self.require_routing_publication_locked(
                &_registration,
                Some(crate::runtime::routing::ActivationRoutingOriginal::Deactivation(original)),
            )?;
            // Resolve only the original plugin's actual selected descriptor.
            // 仅解析原插件实际选定描述符。
            let plugin = self
                .snapshot()
                .plugins
                .get(&original.original().plugin_id)
                .cloned()
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new(
                        "Original managed removal package is not installed.",
                    )
                })?;
            LegacyLifecycleExecution::for_deactivation(&self.state, plugin, original)?
        };
        self.register_guarded_lifecycle(authority)
    }

    /// Share the sole inert registration, native source, deadline and physical-retirement protocol across explicit guarded entries.
    /// 在明确受保护入口之间共用唯一未启动注册、原生来源、期限及物理退役协议。
    /// authority binds its actual namespace and declaration; returns retained original ownership without starting callbacks.
    /// authority 已绑定实际命名空间及声明；返回保留原所有权，不启动回调。
    fn register_guarded_lifecycle(
        &self,
        authority: Arc<LegacyLifecycleExecution>,
    ) -> Result<VulcanPluginLegacyLifecycleCall, VulcanPluginRuntimeError> {
        // The same actual descriptor owns the one plugin identity used by all admission effects.
        // 同一实际描述符拥有全部准入效果使用的单个插件身份。
        let plugin_id = &authority.plugin().id;
        // Retain either a declared actual call or verified absence without mixed authorities.
        // 保留已声明实际调用或已核验缺失声明，不混合权威。
        let ownership = if let Some(export) = authority.export() {
            // Retain the actual original formal runtime parent.
            // 保留实际原正式运行时父级。
            let owner = self.required_formal_owner()?;
            {
                // Serialize capacity admission with the existing registration gate.
                // 使用既有注册门串行化容量入场。
                let publication = self.state.session_registration_gate.lock().map_err(|_| {
                    VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
                })?;
                self.require_routing_publication_locked(
                    &publication,
                    Some(
                        crate::runtime::routing::ActivationRoutingOriginal::LegacyMutation(
                            authority.mutation_guard(),
                        ),
                    ),
                )?;
                authority.validate(&self.state, authority.plugin(), authority.scope())?;
                if owner.has_retiring_capacities(plugin_id)? {
                    owner.prepare_plugin_capacities(plugin_id, &publication)?;
                }
            }
            // Derive one absolute deadline from the existing lifecycle budget.
            // 从既有生命周期预算派生单个绝对期限。
            let deadline = Instant::now()
                .checked_add(Duration::from_millis(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS))
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Legacy lifecycle deadline is out of range.")
                })?;
            // Construct the original bounded input before registering any execution effects.
            // 登记任何执行副作用前构造原有界输入。
            let input = Preparation {
                workspace: CallWorkspace::LifecycleEnvironment,
                arguments: json!({"event": "uninstall", "plugin_id": plugin_id}),
                finalizer: None,
                admission: CallAdmission::LegacyLifecycle {
                    authority: Arc::clone(&authority),
                    export: export.to_owned(),
                },
                scope: authority.scope(),
                security_partition: format!("legacy-lifecycle:uninstall:{}", Uuid::new_v4()),
                deadline,
            };
            // Retain the same registered call and its sole execution source.
            // 保留同一已注册调用及其唯一执行来源。
            let call =
                self.register_owned_formal_call(&owner, authority.plugin().clone(), input)?;
            LegacyCallOwnership::Declared { owner, call }
        } else {
            LegacyCallOwnership::NoDeclaration(authority)
        };
        Ok(VulcanPluginLegacyLifecycleCall {
            manager: self.clone(),
            ownership: Some(ownership),
        })
    }
}
