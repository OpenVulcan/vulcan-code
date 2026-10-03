//! Retain and close the service's sole original native parent through its existing manager.
//! 通过既有管理器保留并关闭服务唯一原生父级。

use crate::{ServiceBridgeState, SharedServiceBridgeState};
use vulcan_plugin_runtime::VulcanPluginRuntimeManager;

/// Admit bootstrap publication using the existing shutdown gate and actual HTTP failure fact.
/// 使用既有关闭门及真实 HTTP 失败事实准入引导发布。
/// `service` is already locked; returns an error before any late initialization or ready publication.
/// `service` 已加锁；在任何迟到初始化或就绪发布之前返回错误。
pub(crate) fn require_bootstrap_admission(service: &ServiceBridgeState) -> Result<(), String> {
    crate::service_state_access::require_plugin_publication_shutdown_admission(service)?;
    if service.service_lifecycle_state.http_bridge_failed {
        return Err(String::from(
            "HTTP bridge failed before plugin bootstrap publication.",
        ));
    }
    Ok(())
}

/// Retain the original manager before attaching its engine and constructing its sole native parent.
/// 附加引擎及构造唯一原生父级之前保留原管理器。
/// `state` is the actual service owner, `manager` is bootstrap's exact manager, and `initialize` only constructs native metadata/workers.
/// `state` 是真实服务所有者，`manager` 是引导精确管理器，`initialize` 仅构造原生元数据及工作线程。
/// Returns the initializer outcome; failures retain the same owner and never reopen or replace the slot.
/// 返回初始化结果；失败保留同一所有者，绝不重新开放或替换槽位。
pub(crate) fn retain_and_initialize_original_manager(
    state: &SharedServiceBridgeState,
    manager: &VulcanPluginRuntimeManager,
    initialize: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    // The service lock is the common short gate with begin_shutdown_drain; no Lua, callbacks or drain waits run here.
    // 服务锁是与 begin_shutdown_drain 共用的短门；此处不执行 Lua、回调或排空等待。
    let mut service = state.lock().map_err(|error| error.to_string())?;
    require_bootstrap_admission(&service)?;
    if service
        .service_lifecycle_state
        .original_plugin_manager
        .is_some()
    {
        return Err(String::from(
            "The original plugin manager is already retained.",
        ));
    }
    service.service_lifecycle_state.original_plugin_manager = Some(manager.clone());
    initialize()
}

/// Close the original manager and confirm real execution and retirement worker joins outside the service lock.
/// 在服务锁外关闭原管理器，并确认真实执行及退役线程已等待退出。
/// `state` must have crossed the existing irreversible boundary; returns success only for explicit absence or actual complete closure.
/// `state` 必须已越过既有不可逆边界；仅在明确缺失或真实完整关闭时返回成功。
/// Timeout and observation errors preserve the original manager for another closing attempt with admission permanently blocked.
/// 超时及观测错误保留原管理器供再次关闭尝试，并永久阻止准入。
pub(crate) fn close_original_plugin_runtime(
    state: &SharedServiceBridgeState,
) -> Result<(), String> {
    // Snapshot the single original owner under the same gate that forbids late initialization.
    // 在禁止迟到初始化的同一门下获取唯一原所有者快照。
    let manager = {
        // No service lock survives into request_close, callbacks, VM retirement or polling.
        // 服务锁不会跨越 request_close、回调、VM 退役或轮询。
        let service = state.lock().map_err(|error| error.to_string())?;
        if !service.service_lifecycle_state.shutdown_recovery_blocked {
            return Err(String::from(
                "Native shutdown requires irreversible service admission closure.",
            ));
        }
        service
            .service_lifecycle_state
            .original_plugin_manager
            .clone()
    };
    // Missing manager is exact pre-bootstrap absence, never a search through extension generations.
    // 管理器缺失是精确的引导前缺失，绝不遍历扩展代次查找。
    let Some(manager) = manager else {
        return Ok(());
    };
    manager
        .close_formal_runtime_blocking()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
