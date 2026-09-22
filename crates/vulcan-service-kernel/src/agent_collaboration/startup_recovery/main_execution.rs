//! Main-business cold recovery after descendant journals have been reconciled.
//! 后代日志完成对账之后的主业务冷恢复。

use super::*;

/// Recovers `handle`'s original idle business through the shared WebUI preparation owner.
/// 通过共享 WebUI 准备所有者恢复 `handle` 的原空闲业务。
/// `registry` is the startup worker owner; returns false for another active or intentionally stopped owner.
/// `registry` 是启动任务所有者；其他活动或主动停止的所有者返回 false。
pub(super) async fn recover(
    app: &crate::ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller: &vulcan_workspace_store::data_plane::HostRuntimeHandle,
    registry: &Arc<SessionRuntimeRegistry>,
    handle: Arc<SessionHandle>,
) -> Result<bool, String> {
    worker::require_active_owner(state, registry)?;
    // Full history IO stays off the async worker; the preparation reservation revalidates its frontier.
    // 完整历史 IO 不占用异步任务；准备预留重新校验其前沿。
    let read_handle = handle.clone();
    let authority = crate::spawn_service_blocking_task(move || {
        crate::webui_prompt_lifecycle::main_business_startup_authority(&read_handle)
    })
    .await
    .map_err(|error| error.to_string())??;
    let Some(authority) = authority else {
        return Ok(false);
    };
    worker::require_active_owner(state, registry)?;
    let runtime = state
        .lock()
        .map_err(|error| error.to_string())?
        .agent_runtime
        .clone()
        .ok_or_else(|| {
            String::from("Main business recovery requires an initialized agent runtime.")
        })?;
    // Restoring the original Hook belongs inside dispatch exclusion, never in this advisory scan.
    // 恢复原 Hook 必须位于分发互斥内部，不能放在此提示性扫描中。
    // A busy preparation is not a delivered recovery; retain the startup worker until ownership is established.
    // 准备繁忙不代表恢复已交付；在建立归属之前保留启动任务。
    let started = match authority {
        crate::webui_prompt_lifecycle::MainBusinessStartupAuthority::Recovery(authority) => {
            crate::webui_prompt_lifecycle::maybe_prepare_webui_business_recovery(
                app,
                state,
                controller,
                &runtime,
                handle.session_id(),
                authority,
            )
            .await?
        }
        crate::webui_prompt_lifecycle::MainBusinessStartupAuthority::InitialHuman(source) => {
            crate::webui_prompt_lifecycle::maybe_prepare_initial_human_business(
                app,
                state,
                controller,
                &runtime,
                handle.session_id(),
                source,
            )
            .await?
        }
    };
    if !started {
        return Err(String::from(
            "Main business recovery remains pending behind preparation admission.",
        ));
    }
    Ok(true)
}
