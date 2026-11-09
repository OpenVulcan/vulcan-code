//! Tool-visibility inputs for manual context compaction.
//! 手动上下文压缩使用的工具可见性输入。
//!
//! A cache-preserving compaction request must declare exactly the tool list of the ordinary turn,
//! because the provider prompt-cache prefix starts with the tool declarations. Automatic compaction
//! copies these inputs from its source request; manual compaction has no source request, so this
//! module derives them from the same live owners the ordinary turn admission reads.
//! 保留缓存的压缩请求必须声明与普通回合完全相同的工具清单，因为供应商提示缓存前缀从工具声明开始。
//! 自动压缩从来源请求复制这些输入；手动压缩没有来源请求，因此本模块从普通回合准入读取的同一实时所有者派生。

use super::*;

/// Fills the visibility-only launch inputs of one manual compaction request from live session state.
/// 依据实时会话状态填充单个手动压缩请求中仅影响可见性的启动输入。
///
/// # Parameters
/// 参数
/// `app`, `state` and `agent_runtime` are the live service owners; `request` is the manual job whose
/// launch context receives the same tool-gating inputs an ordinary turn of this session would use.
/// `app`、`state` 与 `agent_runtime` 是实时服务所有者；`request` 是手动任务，其启动上下文接收本会话普通回合
/// 会使用的同一组工具闸门输入。
///
/// # Returns
/// 返回值
/// Success after the inputs are applied; an error leaves the launch context unchanged. Tool calls are
/// rejected during compaction either way, so these inputs only affect prompt-cache reuse.
/// 输入应用后返回成功；出错时启动上下文保持不变。压缩期间工具调用一律被拒绝，因此这些输入只影响提示缓存复用。
pub(super) async fn apply_manual_compaction_tool_visibility(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    request: &mut PromptCompactionJobRequest,
) -> Result<(), String> {
    let session_id = request.session_id;
    let workspace_path = request.workspace_path.clone();
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    // Dedicated sessions shape their tools from a frozen profile the compaction request does not carry.
    // 专职会话依据冻结配置塑形工具，压缩请求不携带该配置。
    let dedicated = handle
        .read(|document| document.dedicated_profile().is_some())
        .map_err(|error| format!("Read dedicated session profile failed: {error}"))?;
    if dedicated {
        return Ok(());
    }
    let (project_id, session_vmm) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        (
            owning_project_id_for_session(workspace, session_id),
            workspace
                .session_runtime_state
                .get(&session_id)
                .map(|runtime_state| runtime_state.vmm.clone())
                .unwrap_or_default(),
        )
    };
    // VMM readiness follows the ordinary-turn admission rule: feature, live service and effective policy.
    // VMM 就绪遵循普通回合准入规则：特性开关、实时服务与有效策略。
    let effective_vmm_enabled = agent_runtime
        .vmm_hook_runtime
        .resolve_publication_config(session_id, &workspace_path)?
        .enabled;
    let vmm_runtime_ready = vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().session_state_enabled
        && webui_vmm_runtime_readiness(state)?.service_ready
        && effective_vmm_enabled;
    let subagent_model_policy =
        crate::subagent_models::session_subagent_model_policy(handle.as_ref())?;
    let native_browser_runtime = crate::browser::native_browser_runtime(state.clone())?;
    let supplemental_mcp_tools =
        crate::browser::prepare_browser_mcp_bindings(state.clone(), session_id).await?;
    let knowledge_access = match crate::resolve_turn_knowledge_access(state, session_id).await {
        Ok(access) => access,
        Err(error) => {
            tracing::warn!(
                "[webui-service] manual compaction knowledge access resolution failed: {error}"
            );
            None
        }
    };
    let context = &mut request.launch_context;
    context.project_id = project_id;
    context.subagent_model_policy = subagent_model_policy;
    context.vmm_enabled = true;
    context.project_vmm = crate::load_project_configuration(&workspace_path).vmm;
    context.session_vmm = session_vmm;
    context.vmm_runtime_ready = vmm_runtime_ready;
    context.native_browser_runtime = native_browser_runtime;
    context.native_agent_collaboration_runtime = Some(
        crate::agent_collaboration::session_v1_native_agent_collaboration_runtime(
            state.clone(),
            app.clone(),
        ),
    );
    context.supplemental_mcp_tools = supplemental_mcp_tools;
    context.knowledge_access = knowledge_access;
    Ok(())
}
