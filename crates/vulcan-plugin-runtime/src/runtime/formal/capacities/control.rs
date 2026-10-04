//! Exact remote control shares internal lifecycle mutation and rejects stale ownership.
//! 精确远程控制共享内部生命周期变更，并拒绝陈旧归属。

use super::*;
use serde::Deserialize;

/// Explicit operation selects closure or acknowledgement, never implicit replacement.
/// 明确操作选择关闭或确认，绝不隐式替换。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginCapacityControlAction {
    /// Permanently close admission and request native member shutdown.
    /// 永久关闭入场并请求原生成员关闭。
    Close,
    /// Release closed and drained metadata; this does not remove persisted policy.
    /// 释放已关闭且排空的元数据；此操作不移除持久策略。
    Forget,
}

/// A management observation authorizes only its original native capacity and policy.
/// 管理观测仅授权其原始原生容量及策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginCapacityControlRequest {
    /// Exact attached parent observed before the explicit operation.
    /// 明确操作前观测的精确附加父级。
    pub runtime_id: String,
    /// Original core-issued capacity identity, never a semantic lookup substitute.
    /// 原始核心签发容量身份，绝不以语义查找替代。
    pub capacity_id: String,
    /// Opaque policy predecessor prevents closing a concurrently revised capacity.
    /// 不透明策略前驱阻止关闭已被并发修订的容量。
    pub expected_revision: String,
    /// Exact plugin owns the observed capacity.
    /// 精确插件拥有所观测容量。
    pub plugin_id: String,
    /// Original host purpose selects the retained semantic mapping.
    /// 原宿主用途选择保留语义映射。
    pub purpose: VulcanPluginCapacityPurpose,
    /// Explicit operation never advances an unrelated management transaction.
    /// 明确操作绝不推进无关管理事务。
    pub action: VulcanPluginCapacityControlAction,
}

impl VulcanPluginRuntimeManager {
    /// Apply request under the original publication gate; return whether its original owner was present.
    /// 在原发布门下应用 request；返回原所有者是否存在。
    /// Absence returns false; a different parent, replacement owner or policy predecessor is an error.
    /// 缺失返回 false；另一父级、替代所有者或策略前驱为错误。
    pub fn control_formal_capacity(
        &self,
        request: VulcanPluginCapacityControlRequest,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        for identity in [
            &request.runtime_id,
            &request.capacity_id,
            &request.expected_revision,
        ] {
            if identity.is_empty() || identity.contains('\0') {
                return Err(VulcanPluginRuntimeError::new(
                    "Capacity control identities must be nonempty and contain no zero byte.",
                ));
            }
        }
        self.control_capacity_owner(
            &request.plugin_id,
            request.purpose,
            Some(&request),
            request.action,
        )
    }

    /// Apply action to plugin_id/purpose with optional trusted-internal observation; return actual mapping presence.
    /// 对 plugin_id／purpose 应用 action，可选携带可信内部观测；返回真实映射存在性。
    /// Remote callers always supply expected; only existing in-process lifecycle callers omit the observation.
    /// 远程调用方始终提供 expected；仅既有进程内生命周期调用方省略观测。
    pub(super) fn control_capacity_owner(
        &self,
        plugin_id: &str,
        purpose: VulcanPluginCapacityPurpose,
        expected: Option<&VulcanPluginCapacityControlRequest>,
        action: VulcanPluginCapacityControlAction,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        crate::catalog::validate_identifier(plugin_id, "capacity owner")
            .map_err(VulcanPluginRuntimeError::new)?;
        // Hold publication from identity validation through native mutation and mapping acknowledgement.
        // 从身份校验到原生变更及映射确认始终持有发布门。
        let _publication =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        // Remote absence must not silently acknowledge a request for an unknown parent.
        // 远程缺失不得静默确认针对未知父级的请求。
        let Some(owner) = self.formal_runtime_owner()? else {
            return if expected.is_some() {
                Err(VulcanPluginRuntimeError::new(
                    "Formal runtime is not attached.",
                ))
            } else {
                Ok(false)
            };
        };
        if expected.is_some_and(|request| request.runtime_id != owner.host.runtime().id()) {
            return Err(VulcanPluginRuntimeError::new(
                "Capacity control belongs to another runtime.",
            ));
        }
        // Freeze semantic ownership and effective policy together; no check-then-call lock gap exists.
        // 同时冻结语义归属及生效策略；不存在检查后再调用的锁间隙。
        let mut registry = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        // The exact key is retained until native forgetting has actually succeeded.
        // 精确键保持保留，直到原生遗忘真实成功。
        let key = (plugin_id.to_owned(), purpose);
        // Missing mapping is observable but does not prove physical drainage of any former owner.
        // 映射缺失可以观测，但不证明任何此前所有者已经物理排空。
        let Some(registration) = registry.registrations.get(&key) else {
            return Ok(false);
        };
        if expected.is_some_and(|request| request.capacity_id != registration.identity) {
            return Err(VulcanPluginRuntimeError::new(
                "Capacity control owner was replaced; the original request cannot target it.",
            ));
        }
        // The native query verifies host ownership and returns the exact current policy predecessor.
        // 原生查询校验宿主归属，并返回精确当前策略前驱。
        let policy =
            owner.verify_capacity(plugin_id, purpose, registration, &registry.policy.settings)?;
        if expected.is_some_and(|request| request.expected_revision != policy.revision) {
            return Err(VulcanPluginRuntimeError::new(
                "Capacity control policy predecessor is stale.",
            ));
        }
        match action {
            VulcanPluginCapacityControlAction::Close => {
                owner
                    .host
                    .runtime()
                    .close_capacity(&registration.identity)
                    .map_err(calls::core_error)?;
            }
            VulcanPluginCapacityControlAction::Forget => {
                registry.require_policy_admission(plugin_id, purpose)?;
                if registration.preparing_members != 0 {
                    return Err(VulcanPluginRuntimeError::new(
                        "Plugin capacity still owns unfinished member preparation.",
                    ));
                }
                owner
                    .host
                    .runtime()
                    .forget_capacity(&registration.identity)
                    .map_err(calls::core_error)?;
                registry.remove_registration(&key);
            }
        }
        Ok(true)
    }
}
