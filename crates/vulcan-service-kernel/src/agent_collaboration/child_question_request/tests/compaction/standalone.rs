//! Real HTTPS acceptance for the standalone compaction producer and its confirmation owner.
//! 独立压缩生产者及其确认所有者的真实 HTTPS 验收。

use super::*;
use vulcan_agent_runtime::agent_runtime::prompt_compaction::{
    PromptCompactionFoundationJobRequest, PromptCompactionLaunchContext,
    PromptCompactionModelTarget, PromptCompactionOwnership,
};

/// Drives an actual 401 and token refresh through the standalone confirmation owner.
/// 经独立确认所有者驱动真实 401 及令牌刷新。
#[test]
fn standalone_native_compaction_drives_real_http_refresh_and_confirmation() {
    // The refreshed retry uses the same physical producer as ordinary standalone compaction.
    // 刷新重试使用与普通独立压缩相同的物理生产者。
    let executor = crate::service_bridge_io::webui_service_runtime_builder()
        .worker_threads(2)
        .build()
        .unwrap();
    let task = executor.spawn(Box::pin(run_child_maintenance(true, true, true)));
    executor.block_on(task).unwrap();
}

/// Exercises RuntimeOnly native compaction without mutating the source V1 ledger.
/// 验证 RuntimeOnly 原生压缩，不修改来源 V1 账本。
#[test]
fn standalone_native_compaction_drives_real_http_and_confirmation() {
    // Keep the production stack contract and repository concurrency limits.
    // 保持生产线程栈契约及仓库并发限制。
    let executor = crate::service_bridge_io::webui_service_runtime_builder()
        .worker_threads(2)
        .build()
        .unwrap();
    let task = executor.spawn(Box::pin(run_child_maintenance(true, false, true)));
    executor.block_on(task).unwrap();
}

/// Drives exact `preparation`, `target`, `configuration` and `foundation` against `server`.
/// 使用精确的 `preparation`、`target`、`configuration` 和 `foundation` 驱动 `server`。
/// Verifies real attempts and immutable source `handle`; returns after all barriers and assertions.
/// 验证真实尝试及不可变来源 `handle`；全部屏障和断言完成后返回。
pub(super) async fn verify(
    preparation: vulcan_agent_runtime::agent_runtime::ExecutionPreparationRequest,
    target: PromptCompactionModelTarget,
    configuration: vulcan_agent_runtime::provider_config::ProviderConfigurationFile,
    foundation: Arc<RuntimeFoundation>,
    handle: &Arc<SessionHandle>,
    server: &https::Server,
    refresh: bool,
) {
    // Use the real runtime conversation and planner, rather than a fabricated stream or checkpoint.
    // 使用真实运行时会话及规划器，而非伪造流或检查点。
    let conversation = preparation.conversation_state.clone();
    let messages =
        vulcan_context::conversation_state_to_compaction_source_messages(&conversation).unwrap();
    let plan = vulcan_context_compaction::plan_context_compaction(
        vulcan_context_compaction::CompactionPlanInput {
            session_label: &preparation.session_title,
            messages: &messages,
            all_messages: &messages,
            context_start_message_id: None,
            source_until_message_id: None,
            policy: target.context_compaction_policy,
        },
    )
    .expect("the real completed history must contain a compactable prefix");
    // A RuntimeOnly owner must leave the original durable source untouched.
    // RuntimeOnly 所有者必须保持原可靠来源不变。
    let before = handle.read(|document| document.memory_revision()).unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(50),
        vulcan_agent_runtime::agent_runtime::task_runtime::run_standalone_compaction_for_testing(
            PromptCompactionFoundationJobRequest {
                ownership: PromptCompactionOwnership::RuntimeOnly,
                session_id: preparation.session_id,
                session_title: preparation.session_title.clone(),
                workspace_path: preparation.workspace_path.clone(),
                plan,
                full_context_state: conversation,
                input_budget: None,
                observed_input_tokens: None,
                estimated_input_tokens: None,
                provider_configuration: configuration,
                model_target: Some(target),
                execution_agent_id: preparation.execution_agent_id.clone(),
                launch_context: PromptCompactionLaunchContext::from_execution_request(&preparation),
                model_label: None,
                reasoning_effort: preparation.reasoning_effort,
                foundation,
                luaskills_runtime: None,
            },
        ),
    )
    .await
    .expect("standalone barriers must settle")
    .expect("real standalone compaction");
    assert_eq!(result.provider_request_count, if refresh { 2 } else { 1 });
    assert_eq!(
        result.compaction_result.provider_context_items,
        vec![json!({"type":"compaction","encrypted_content":"native-maintenance-checkpoint"})]
    );
    // This exact HTTP trace proves that the standalone owner released every real retry barrier.
    // 此精确 HTTP 轨迹证明独立所有者释放了每个真实重试屏障。
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), if refresh { 2 } else { 1 });
    assert_eq!(requests.first(), requests.last());
    assert_eq!(
        server.refreshes.load(std::sync::atomic::Ordering::SeqCst),
        usize::from(refresh)
    );
    assert_eq!(
        handle.read(|document| document.memory_revision()).unwrap(),
        before
    );
    let reloaded = load_session_v1_for_runtime(&handle.storage_locator().unwrap())
        .unwrap()
        .document;
    assert!(reloaded.metadata.context_maintenance.is_none());
    assert!(reloaded.compactions.is_empty());
    assert!(reloaded.active_api().is_none());
}
