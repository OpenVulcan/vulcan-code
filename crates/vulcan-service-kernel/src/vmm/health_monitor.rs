//! VMM background heartbeat and workbench publication.
//! VMM 后台心跳与工作台发布。

use std::{sync::Arc, time::Duration};

use serde_json::json;
use vulcan_service_api::{ServiceEventEmitter, VmmDeploymentMode, VmmMemoryActionRequest};

use super::*;

/// Interval between serialized VMM heartbeat attempts.
/// 串行 VMM 心跳尝试之间的间隔。
const VMM_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Starts one process-wide VMM heartbeat loop after service bootstrap.
/// 在服务引导完成后启动一个进程级 VMM 心跳循环。
///
/// # Parameters
/// 参数
/// `state` owns the current managed child, provider configuration, and cached workbench.
/// `state` 持有当前托管子进程、Provider 配置与缓存工作台。
/// `app` broadcasts fresh workbench snapshots to every connected WebUI client.
/// `app` 将新鲜工作台快照广播给所有已连接的 WebUI 客户端。
///
/// # Returns
/// 返回值
/// This function returns immediately after spawning the serialized heartbeat task.
/// 本函数在生成串行心跳任务后立即返回。
pub(crate) fn start_vmm_health_monitor(state: SharedServiceBridgeState, app: ServiceEventEmitter) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(VMM_HEARTBEAT_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if heartbeat_managed_vmm(&state, &app).await {
                continue;
            }
            heartbeat_external_vmm(&state, &app).await;
        }
    });
}

/// Probes the currently owned managed child and publishes the cached projection.
/// 探测当前持有的托管子进程并发布缓存投影。
///
/// # Parameters
/// 参数
/// `state` owns the exact child generation and its transport configuration.
/// `state` 持有准确的子进程代次及其传输配置。
/// `app` broadcasts the resulting workbench snapshot.
/// `app` 广播生成的工作台快照。
///
/// # Returns
/// 返回值
/// `true` when a managed child owns the VMM lifecycle, including stopped heartbeat targets.
/// 当托管子进程持有 VMM 生命周期时返回 `true`，包括暂不可探测的目标。
async fn heartbeat_managed_vmm(
    state: &SharedServiceBridgeState,
    app: &ServiceEventEmitter,
) -> bool {
    let (runtime, vmm_config) = match state.lock() {
        Ok(guard) => {
            let Some(runtime) = guard.managed_memory_mesh_runtime.as_ref().map(Arc::clone) else {
                return false;
            };
            let Some(agent_runtime) = guard.agent_runtime.as_ref() else {
                return true;
            };
            (runtime, agent_runtime.provider_configuration.vmm.clone())
        }
        Err(error) => {
            eprintln!("[webui-service] failed to lock VMM heartbeat state: {error}");
            return true;
        }
    };
    let target = match runtime.lock() {
        Ok(runtime) => runtime.heartbeat_target(),
        Err(error) => {
            eprintln!("[webui-service] failed to lock managed VMM heartbeat target: {error}");
            return true;
        }
    };
    let Some(target) = target else {
        publish_managed_heartbeat_workbench(state, app);
        return true;
    };
    let result = crate::managed_memory_mesh_runtime::health_check(
        target.grpc_target.as_str(),
        target.access_token.as_str(),
        &vmm_config,
    )
    .await;
    if let Ok(mut runtime) = runtime.lock() {
        runtime.record_heartbeat_result(result);
    } else {
        eprintln!("[webui-service] failed to record managed VMM heartbeat result");
    }
    publish_managed_heartbeat_workbench(state, app);
    true
}

/// Publishes the latest managed lifecycle cache after one heartbeat tick.
/// 在一次心跳节拍后发布最新托管生命周期缓存。
///
/// # Parameters
/// 参数
/// `state` owns the canonical workbench projection.
/// `state` 持有规范工作台投影。
/// `app` broadcasts the projection to connected clients.
/// `app` 将投影广播给已连接客户端。
///
/// # Returns
/// 返回值
/// This function returns no value; projection failures are logged for diagnosis.
/// 本函数不返回值；投影失败会记录日志供诊断。
fn publish_managed_heartbeat_workbench(
    state: &SharedServiceBridgeState,
    app: &ServiceEventEmitter,
) {
    match crate::managed_memory_mesh_runtime::synchronize_workbench_runtime(state) {
        Ok(workbench) => crate::emit_workbench_event(app, &workbench),
        Err(error) => {
            eprintln!("[webui-service] failed to publish managed VMM heartbeat: {error}");
        }
    }
}

/// Probes an enabled external VMM endpoint through the canonical management action path.
/// 通过规范管理动作链路探测已启用的外部 VMM 端点。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative external configuration.
/// `state` 持有权威外部配置。
/// `app` broadcasts the resulting cached health projection.
/// `app` 广播生成的缓存健康投影。
///
/// # Returns
/// 返回值
/// This function returns after one serialized probe or an intentional disabled-state skip.
/// 本函数在完成一次串行探测或有意跳过禁用状态后返回。
async fn heartbeat_external_vmm(state: &SharedServiceBridgeState, app: &ServiceEventEmitter) {
    if !external_vmm_heartbeat_enabled(state) {
        return;
    }
    let request = VmmMemoryActionRequest {
        action: String::from("healthz"),
        command_id: uuid::Uuid::new_v4().to_string(),
        scope: String::from("global"),
        project_id: None,
        conversation_id: None,
        payload: json!({}),
    };
    let _ = service_run_vmm_memory_action(app.clone(), state.clone(), request).await;
}

/// Decides whether the authoritative global configuration owns an external heartbeat target.
/// 判断权威全局配置是否持有外部心跳目标。
///
/// # Parameters
/// 参数
/// `state` owns the latest provider configuration.
/// `state` 持有最新 Provider 配置。
///
/// # Returns
/// 返回值
/// `true` only for an enabled external VMM with a non-blank endpoint.
/// 仅对已启用且端点非空的外部 VMM 返回 `true`。
fn external_vmm_heartbeat_enabled(state: &SharedServiceBridgeState) -> bool {
    let configuration = match state.lock() {
        Ok(guard) => guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.vmm.clone()),
        Err(error) => {
            eprintln!("[webui-service] failed to inspect external VMM heartbeat state: {error}");
            None
        }
    };
    let Some(configuration) = configuration else {
        return false;
    };
    let effective = vulcan_vmm::VmmEffectiveConfig::resolve(&configuration, None, None);
    vmm_deployment_mode(&configuration) == VmmDeploymentMode::External
        && effective.enabled
        && !effective.grpc_target.trim().is_empty()
}
