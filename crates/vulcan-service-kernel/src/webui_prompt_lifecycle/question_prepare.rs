//! Preparation dispatch for an explicitly admitted question execution.
//! 显式准入问题执行的准备分发。

use super::*;
use vulcan_session_core::v1::collaboration::CollaborationQuestionExecutionRecord;

/// Runs profile preparation and starts only the supplied QA execution under the exact kernel owner.
/// 在精确内核所有者下运行画像准备，且仅启动传入的答疑执行。
/// Runtime arguments provide existing services; `start`, generation and `execution` come from the admitted launcher.
/// 运行时参数提供现有服务；`start`、代际和 `execution` 来自已准入启动器。
/// Returns after scheduling, or rejects a mismatched source before spawning any work.
/// 调度后返回；来源不匹配时，在启动任何工作之前拒绝。
pub(crate) fn spawn_webui_question_prepare_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: HostRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    expected_kernel_generation: u64,
    execution: CollaborationQuestionExecutionRecord,
) -> Result<(), String> {
    if start.source != PendingSessionPromptSource::QuestionExecution
        || start.session_id.to_string() != execution.participant.session_id
        || start.continuation_action_id.is_some()
        || start.visible_user_prompt_record_id.is_some()
        || start.canonical_user_message_id.is_some()
        || !start.prompt.is_empty()
        || !start.attachments.is_empty()
        || !start.provider_attachments.is_empty()
    {
        return Err(String::from(
            "Question preparation requires an exact auxiliary source without user input.",
        ));
    }
    // session_id and preparation_turn_id remain available after start moves into request construction.
    // session_id 与 preparation_turn_id 在 start 移入请求构建后仍可用于归属校验。
    let session_id = start.session_id;
    let preparation_turn_id = start.preparation_turn_id.clone();
    if crate::capture_webui_question_preparation_generation(
        &state,
        session_id,
        &preparation_turn_id,
        &execution.execution_id,
    )? != Some(expected_kernel_generation)
    {
        return Err(String::from(
            "Question preparation kernel owner has changed.",
        ));
    }
    // Keep the preparation cancellable until its Provider stream has been registered.
    // 在 Provider 流登记之前保持准备可取消。
    let registration_state = state.clone();
    let registration_turn_id = preparation_turn_id.clone();
    let registration_execution_id = execution.execution_id.clone();
    crate::agent_collaboration_execution::preparation_tasks::spawn_question_preparation_task(
        &registration_state,
        session_id,
        expected_kernel_generation,
        &registration_turn_id,
        &registration_execution_id,
        async move {
            // profile_outcome refreshes profile state without opening another human prompt recall round.
            // profile_outcome 刷新画像状态，不开启另一轮人类提示词召回。
            let profile_outcome = start.profile_request.clone().run().await;
            // precheck_outcome is explicitly skipped because this input is an agent question, not human input.
            // precheck_outcome 明确跳过，因为该输入为代理问题而非人类输入。
            let precheck_outcome = start.precheck_request.clone().skip();
            // preparation_result preserves the original execution across publication and request-building errors.
            // preparation_result 跨发布与请求构建错误保留原始执行。
            let preparation_result = async {
                // publication is held through shared request construction, matching the existing native VMM contract.
                // publication 持有到共享请求构建完成，遵循现有原生 VMM 契约。
                let publication =
                    prepare_native_recall_publication(&state, &agent_runtime, &start).await?;
                let request = continue_webui_prompt_turn_after_preparation_for_purpose(
                    &app,
                    &state,
                    &controller_runtime,
                    &agent_runtime,
                    start,
                    expected_kernel_generation,
                    profile_outcome,
                    precheck_outcome,
                    publication.as_ref(),
                    PromptContinuationPurpose::QuestionExecution,
                )?;
                // Native publication is complete; browser discovery must not retain the global mutation barrier.
                // 原生发布已经完成；浏览器发现不得继续持有全局变更屏障。
                drop(publication);
                // None means the exact kernel owner was closed, so no replacement may be started here.
                // None 表示精确内核所有者已关闭，因此这里不能启动替代执行。
                let Some(mut request) = request else {
                    return Ok::<(), String>(());
                };
                request.supplemental_mcp_tools =
                    crate::browser::prepare_browser_mcp_bindings(state.clone(), session_id).await?;
                crate::agent_stream::spawn_question_execution_stream_task(
                    app.clone(),
                    state.clone(),
                    controller_runtime.clone(),
                    agent_runtime.clone(),
                    request,
                    expected_kernel_generation,
                    preparation_turn_id.clone(),
                    &execution,
                )
                .await
            }
            .await;
            if let Err(error) = preparation_result {
                // Scoped failure preserves parent Hook/Goal ownership and never calls business preparation cleanup.
                // 带范围失败保留父 Hook/Goal 归属，绝不调用业务准备清理。
                if let Err(cleanup_error) =
                    crate::agent_stream::fail_question_execution_preparation(
                        &app,
                        &state,
                        session_id,
                        expected_kernel_generation,
                        &preparation_turn_id,
                        &execution,
                        &error,
                    )
                    .await
                {
                    tracing::warn!(
                        "[webui-service] question preparation cleanup failed: {cleanup_error}; original error: {error}"
                    );
                }
                if let Err(diagnostic_error) = push_webui_service_diagnostic(
                    &state,
                    format!("Failed to prepare the question request: {error}"),
                ) {
                    tracing::warn!(
                        "[webui-service] question preparation diagnostic failed: {diagnostic_error}"
                    );
                }
            }
        },
    )?;
    Ok(())
}
