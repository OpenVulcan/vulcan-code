//! Memory cache publication after an authoritative V1 compaction commit.
//! 权威 V1 压缩提交后的记忆缓存发布。

use super::*;

/// Reconciles current memory visibility and advances its derived epoch after an exact durable commit.
/// 在准确持久提交后，对账当前记忆可见性并推进其派生纪元。
/// Takes the current host, Hook runtime, session and committed ID; does not perform inference or change the V1 ledger.
/// 接收当前宿主、Hook 运行时、会话及已提交 ID，不执行推理或修改 V1 账本。
pub(super) async fn publish_vmm_compaction_epoch(
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    context: &vulcan_vmm::VmmContextEpoch,
) -> Result<(), String> {
    if !vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().session_state_enabled
        || !webui_vmm_runtime_readiness(state)?.service_ready
    {
        return Ok(());
    }
    // Capture only exact active-session ownership and release the service mutex before awaiting the native gate.
    // 只捕获准确活动会话归属，在等待原生屏障前释放服务互斥锁。
    let (workspace_path, project_id, maintenance) = {
        let guard = state.lock().map_err(|error| error.to_string())?;
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            "VMM compaction publication requires the active workspace.".to_owned()
        })?;
        if !workspace
            .sessions
            .iter()
            .any(|session| session.id == session_id)
        {
            return Err("VMM compaction publication requires the exact active session.".to_owned());
        }
        (
            workspace_path_for_session(workspace, session_id),
            owning_project_id_for_session(workspace, session_id),
            guard.native_vmm_maintenance.clone(),
        )
    };
    // The Hook projection is the same live configuration authority used by ordinary publication.
    // Hook 投影与普通发布使用相同的活动配置权威。
    let config = agent_runtime
        .vmm_hook_runtime
        .resolve_publication_config(session_id, &workspace_path)?;
    if !config.enabled {
        return Ok(());
    }
    // Open the unique controller before holding any Hook publication read guard.
    // 在持有任何 Hook 发布读取守卫之前打开唯一控制器。
    let updated_at = current_unix_millis_text();
    let controller = crate::vmm_session_controller::open_registered_session_vmm_controller(
        state,
        session_id,
        &updated_at,
    )?;
    // Native mode alone owns local visibility reconciliation; external mode retains its opaque server scope.
    // 只有原生模式拥有本地可见性对账；外置模式保留不透明服务端范围。
    let publication = if config.backend_mode == vulcan_vmm::VmmBackendMode::Native {
        Some(
            maintenance
                .ok_or_else(|| "Native VMM compaction maintenance is not ready.".to_owned())?
                .prepare_recall_publication(project_id.as_deref(), &session_id.to_string())
                .await?,
        )
    } else {
        None
    };
    // This identity is derived from the selected backend's declared owner, not the old cache's identity.
    // 此身份来自所选后端声明的归属方，而非旧缓存身份。
    let source = match publication.as_ref() {
        Some(publication) => publication.profile_read_source()?,
        None => vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::external(
                &vulcan_vmm::VmmTransportConfig::from_effective(&config),
                config
                    .user_id
                    .ok_or_else(|| "External VMM compaction requires a user ID.".to_owned())?,
                config
                    .project_id
                    .ok_or_else(|| "External VMM compaction requires a project ID.".to_owned())?,
            )
            .map_err(|error| error.to_string())?,
            Vec::new(),
        )
        .map_err(|error| error.to_string())?,
    };
    agent_runtime
        .vmm_hook_runtime
        .with_current_profile_publication(
            session_id,
            &workspace_path,
            &source,
            || -> Result<(), String> {
                if let Some(publication) = &publication {
                    publication.reconcile_controller_cache(&controller, None, &updated_at)?;
                }
                controller.reconcile_context_epoch(context, &source, &updated_at)?;
                Ok(())
            },
        )??;
    Ok(())
}
