//! Explicit user-authorized recovery for classified service bootstrap failures.
//! 针对已分类服务引导失败的明确用户授权恢复。

use super::*;
use uuid::Uuid;

/// Discards one classified historical session VMM state and starts a clean bootstrap retry.
/// 丢弃一个已分类的历史会话 VMM 状态并启动一次干净的引导重试。
///
/// # Parameters
/// 参数
/// `state` owns the failed startup snapshot, controller runtime, and VMM registry.
/// `state` 持有失败启动快照、控制器运行时与 VMM 注册表。
/// `event_emitter` publishes the recovery and subsequent bootstrap snapshots.
/// `event_emitter` 发布恢复过程与后续引导快照。
///
/// # Returns
/// 返回值
/// The recovering startup snapshot after exact VMM artifact removal and retry scheduling.
/// 在精确移除 VMM 工件并安排重试后返回恢复中的启动快照。
pub(crate) fn service_rebuild_startup_session_vmm(
    state: SharedServiceBridgeState,
    event_emitter: ServiceEventEmitter,
) -> Result<StartupSnapshot, String> {
    // retained values leave the service-state lock before filesystem mutation and event emission.
    // retained values 在文件系统变更与事件发出前离开服务状态锁。
    let (recovery, controller_runtime, registry, recovering_startup, workbench) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock startup recovery state: {error}"))?;
        if guard.startup.phase != "failed" {
            return Err(String::from("当前启动状态不允许执行 VMM 重建恢复。"));
        }
        // recovery comes only from a server-classified historical or damaged VMM open failure.
        // recovery 仅来自服务端已分类的历史或损坏 VMM 打开失败。
        let recovery = guard
            .startup
            .recovery
            .clone()
            .ok_or_else(|| String::from("当前启动失败没有可执行的恢复操作。"))?;
        if recovery.kind != StartupRecoveryKind::RebuildSessionVmm {
            return Err(String::from("当前启动恢复类型不受支持。"));
        }
        // controller_runtime was retained when the recoverable VMM failure was classified for this exact recovery.
        // controller_runtime 在可恢复 VMM 失败被分类时保留，专用于此精确恢复。
        let controller_runtime = guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("启动恢复缺少 Controller Runtime。"))?;
        // registry coordinates exact-session deletion with every possible controller reference.
        // registry 协调精确会话删除与所有可能的控制器引用。
        let registry = guard.vmm_controller_registry.clone();
        guard.startup.ready = false;
        guard.startup.phase = String::from("recovering-vmm");
        guard.startup.recovery = None;
        if let Some(workspace_step) = guard
            .startup
            .steps
            .iter_mut()
            .find(|step| step.id == "workspace")
        {
            workspace_step.status = StartupStepStatus::Running;
            workspace_step.message =
                Some(String::from("正在重建损坏的 VMM 记忆附件并重新加载工作区"));
            workspace_step.updated_at = Some(OffsetDateTime::now_utc());
        }
        guard.startup.recent_logs.push(format!(
            "[webui-service] rebuilding damaged VMM attachment for session {}",
            recovery.session_id
        ));
        (
            recovery,
            controller_runtime,
            registry,
            guard.startup.clone(),
            guard.workbench.clone(),
        )
    };
    emit_service_snapshots(&event_emitter, &recovering_startup, &workbench);

    // session_id is parsed from the service-authored recovery projection, never from caller parameters.
    // session_id 从服务端生成的恢复投影解析，绝不来自调用方参数。
    let session_id = match Uuid::parse_str(&recovery.session_id) {
        Ok(session_id) => session_id,
        Err(error) => {
            return Err(restore_startup_recovery_failure(
                &state,
                &event_emitter,
                recovery,
                format!("恢复会话标识无效：{error}"),
            ));
        }
    };
    // lifecycle_fence closes any stale controller and blocks reopening during exact artifact removal.
    // lifecycle_fence 关闭任何陈旧控制器，并在精确工件移除期间阻止重新打开。
    let lifecycle_fence = match registry.begin_lifecycle_transition([session_id]) {
        Ok(lifecycle_fence) => lifecycle_fence,
        Err(error) => {
            return Err(restore_startup_recovery_failure(
                &state,
                &event_emitter,
                recovery,
                format!("建立 VMM 生命周期栅栏失败：{error}"),
            ));
        }
    };
    // rebuilt_at becomes the creation timestamp of the clean current-format VMM attachment.
    // rebuilt_at 成为干净当前格式 VMM 附件的创建时间戳。
    let rebuilt_at = current_unix_millis_text();
    let rebuilt =
        match vulcan_workspace_store::workspace_store::rebuild_damaged_core_session_vmm_state(
            &controller_runtime.data_root,
            session_id,
            &rebuilt_at,
        ) {
            Ok(rebuilt) => rebuilt,
            Err(error) => {
                drop(lifecycle_fence);
                return Err(restore_startup_recovery_failure(
                    &state,
                    &event_emitter,
                    recovery,
                    format!("重建损坏 VMM 附件失败：{error}"),
                ));
            }
        };
    if !rebuilt {
        drop(lifecycle_fence);
        return Err(restore_startup_recovery_failure(
            &state,
            &event_emitter,
            recovery,
            String::from("未找到恢复目标会话目录，未执行 VMM 重建。"),
        ));
    }
    if let Err(error) = lifecycle_fence.release() {
        return Err(restore_startup_recovery_failure(
            &state,
            &event_emitter,
            recovery,
            format!("释放 VMM 生命周期栅栏失败：{error}"),
        ));
    }

    if let Ok(mut guard) = state.lock() {
        // controller_runtime was retained solely to resolve the exact recovery data root and must not masquerade as retry readiness.
        // controller_runtime 仅为解析精确恢复数据根而保留，不得伪装成重试就绪状态。
        guard.controller_runtime = None;
    }
    mark_webui_service_discovery_starting(&state);
    spawn_service_bootstrap(state, event_emitter);
    Ok(recovering_startup)
}

/// Restores the classified recovery action after one recovery attempt fails.
/// 在一次恢复尝试失败后恢复已分类的恢复操作。
///
/// # Parameters
/// 参数
/// `state` and `event_emitter` own the failed startup projection update.
/// `state` 与 `event_emitter` 持有失败启动投影更新。
/// `recovery` is the exact action restored for a later user retry.
/// `recovery` 是为后续用户重试恢复的精确操作。
/// `message` is the concrete recovery failure diagnostic.
/// `message` 是具体的恢复失败诊断。
///
/// # Returns
/// 返回值
/// The same user-facing diagnostic stored in the failed startup snapshot.
/// 返回写入失败启动快照的同一用户可见诊断。
fn restore_startup_recovery_failure(
    state: &SharedServiceBridgeState,
    event_emitter: &ServiceEventEmitter,
    recovery: StartupRecoverySnapshot,
    message: String,
) -> String {
    let diagnostic = format!("重建会话 VMM 状态失败：{message}");
    update_service_failure(
        state,
        event_emitter,
        ServiceBootstrapError::new("workspace", "Workspace", diagnostic.clone())
            .with_recovery(recovery),
    );
    diagnostic
}
