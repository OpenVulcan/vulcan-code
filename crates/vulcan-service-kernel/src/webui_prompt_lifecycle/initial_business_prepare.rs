//! First human-business preparation from a proven original durable input.
//! 从已证明的原可靠输入进行首次人类业务准备。

use super::pending_prepare::{ExactPendingPromptDispatch, prepare_webui_pending_prompt};
use super::*;
use crate::session_runtime_registry_v1::SessionHandle;
use vulcan_session_core::v1::business_hook::InitialHumanBusiness;
use vulcan_workbench_model::PendingSessionPromptEntry;

/// Revalidates the entire original source before a host may create its first Hook.
/// 在宿主可以创建首个 Hook 前重验完整原始来源。
/// `handle` is canonical; `source` fixes its generation, revision and input, with no fallback.
/// `handle` 是规范所有者；`source` 固定代次、修订和输入，不提供兜底来源。
pub(super) fn validate_initial_human_business(
    handle: &SessionHandle,
    source: &InitialHumanBusiness,
) -> Result<(), String> {
    let current = handle
        .read_full_durable(|document| document.main_business_initial_human_input())
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    if &current != source {
        return Err(String::from(
            "Initial human business authority changed before preparation.",
        ));
    }
    Ok(())
}

/// Prepares the original root or queued successor without submitting another user input.
/// 准备原根或排队后继，不再次提交用户输入。
/// Service owners are preserved; returns whether the shared reservation launched preparation.
/// 保留服务所有者；返回共享预留是否已启动准备。
pub(crate) async fn maybe_prepare_initial_human_business(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    source: InitialHumanBusiness,
) -> Result<bool, String> {
    // The exact registered target supplies both storage and runtime settings.
    // 精确已登记目标同时提供存储和运行配置。
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    validate_initial_human_business(&handle, &source)?;
    // Hydrate the target without changing the user's selected conversation.
    // 水合目标，不改变用户当前选择的会话。
    let workspace = crate::session_hydration::load_workspace_for_submission(
        state,
        controller_runtime.clone(),
        Some(session_id),
    )
    .await?;
    // Resolve resource identities through their owning project catalog.
    // 通过所属项目目录解析资源身份。
    let directory = handle
        .storage_locator()
        .map_err(|error| error.to_string())?
        .project_directory();
    let catalog =
        vulcan_workspace_store::workspace_store::load_project_resource_catalog(&directory)
            .map_err(|error| error.to_string())?;
    let attachments = crate::session_goal_lifecycle::conversation_attachments_from_resource_ids(
        &directory,
        &catalog,
        &source.input.resource_ids,
    );
    // The normal user source retains manual prompt handling and the ordinary precheck policy.
    // 正常用户来源保留手动提示处理及常规预检策略。
    let entry = PendingSessionPromptEntry {
        id: source.input.client_action_id.as_str().to_owned(),
        prompt: source.input.text.clone(),
        hidden_model_guidance: None,
        preview: source.input.text.clone(),
        attachments,
        model_reference: None,
        runtime_options: crate::conversation_submission::exact_v1_dispatch_runtime_options(
            &handle,
        )?,
        source: PendingSessionPromptSource::UserFollowup,
        task_id: None,
        panel_entry_id: None,
        created_at: current_unix_millis_text(),
    };
    prepare_webui_pending_prompt(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        ExactPendingPromptDispatch::InitialHumanBusiness {
            authority: source,
            workspace,
            entry,
        },
    )
    .await
}
