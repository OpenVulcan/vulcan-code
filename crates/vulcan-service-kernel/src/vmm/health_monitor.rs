//! VMM background heartbeat and workbench publication.
//! VMM 后台心跳与工作台发布。

use std::time::Duration;

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
/// `state` owns the authoritative external configuration and cached workbench.
/// `state` 持有权威外置配置与缓存工作台。
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
            heartbeat_external_vmm(&state, &app).await;
        }
    });
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
