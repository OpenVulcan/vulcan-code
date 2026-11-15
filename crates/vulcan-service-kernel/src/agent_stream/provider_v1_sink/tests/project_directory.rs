//! Physical request and timeline regressions for a directory replacement.
//! 目录更换的物理请求与时间线回归测试。

use super::host_guidance::candidate_for;
use super::*;
use vulcan_ai_contract::ContentItem;
use vulcan_session_core::v1::event::SystemContextOperation;
use vulcan_workbench_model::{ProjectRootDirectoryChange, TaskProjectSummary};

/// Verifies old and new paths reach the exact next model request and appear once in the saved timeline.
/// 验证新旧路径进入精确的下一次模型请求，并在已保存的时间线只显示一次。
#[tokio::test]
async fn project_directory_change_reaches_the_physical_request_and_durable_timeline() {
    // The test preserves both the real V1 preparation gate and the on-disk canonical event ledger.
    // 测试保留真实 V1 准备闸门及磁盘上的规范事件账本。
    let (_directory, handle) = published_stored_open_turn();
    let service =
        crate::tests::test_support::test_service_state_with_parent_session(handle.session_id());
    service
        .session_runtime_registry
        .register(Arc::clone(&handle))
        .unwrap();
    let state = Arc::new(std::sync::Mutex::new(service));
    let mut sink = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    // Use names that distinguish model-escaped path data from display formatting.
    // 使用可区分模型路径数据转义与界面格式的目录名称。
    let mut project = TaskProjectSummary::new(
        "project-1",
        "Project",
        r"D:\new project",
        false,
        0,
        None,
        None,
        Vec::new(),
    );
    project.root_directory_change = Some(ProjectRootDirectoryChange {
        previous_path: r"E:\old project".into(),
        changed_at_ms: 5,
    });
    // Install the exact binding required by the final permit check.
    // 安装最终许可复核要求的精确绑定。
    state
        .lock()
        .unwrap()
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .task_projects
        .push(project.clone());
    let facts = ProviderV1RequestFacts {
        request: ApiRequestSnapshot::new("provider-1", "model-1"),
        prepared_at_ms: 10,
        ..request_facts()
    };
    let candidate = candidate_for(&handle);
    // Preparation retries before a permit must preserve exactly one copy of the update.
    // 许可前重试准备时，必须只保留一份变更注入。
    for _ in 0..2 {
        context_injection::synchronize_project_request_dynamic_context(
            &handle,
            &facts,
            &candidate,
            Some(&project),
        )
        .unwrap();
    }
    let PreparedPhysicalApi::SessionV1 {
        exact_request,
        core_start,
        ..
    } = sink
        .prepare_project_request(&state, facts.clone(), candidate, Some(&project))
        .await
        .unwrap()
    else {
        panic!("directory change must release a physical Provider request");
    };
    // Inspect the actual transport messages, including the host envelope and quoted path data.
    // 检查实际传输消息，包括宿主信封和引用的路径数据。
    let notices = exact_request
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|item| match item {
            ContentItem::Text { text }
                if text.contains(context_injection::PROJECT_DIRECTORY_CONTEXT_KEY) =>
            {
                Some(text)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].contains(r"E:\\old project"));
    assert!(notices[0].contains(r"D:\\new project"));

    // The model update is consumed by actual transport. Settle that API before preparing the next one.
    // 模型更新由实际传输消耗。准备下一次请求之前先结算该 API。
    handle
        .mark_api_transport_observed_durable(core_start.api_call_id, 11)
        .await
        .unwrap();
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.complete_api_group(
                    core_start.api_call_id,
                    ApiCompletion {
                        request: facts.request.clone(),
                        usage: UsageSnapshot::default(),
                        status: ApiCallStatus::Completed,
                        end_reason: Some(ApiEndReason::CompletedSuccessfully),
                        error: None,
                        continuation_eligibility: ContinuationEligibility::Forbidden,
                        timing: ApiTimings::default(),
                        provider_source: None,
                        completed_at_ms: 12,
                    },
                )
            },
        )
        .unwrap();
    // Reopen canonical storage and ensure synchronization expires the active instruction.
    // 重新打开规范存储，并验证同步操作使活动指令失效。
    let locator = handle.storage_locator().unwrap();
    let loaded = vulcan_workspace_store::workspace_store::load_session_v1_full(&locator).unwrap();
    let cold = SessionHandle::new(handle.session_id(), locator, loaded.document);
    let next_facts = ProviderV1RequestFacts {
        prepared_at_ms: 13,
        ..facts
    };
    context_injection::synchronize_project_request_dynamic_context(
        &cold,
        &next_facts,
        &candidate_for(&cold),
        Some(&project),
    )
    .unwrap();
    cold.read(|document| {
        let updates = document.events().iter().filter(|event| matches!(
            &event.event,
            EventBody::SystemContext(context)
                if context.context_key == context_injection::PROJECT_DIRECTORY_CONTEXT_KEY
                    && context.operation == SystemContextOperation::Upsert
        )).collect::<Vec<_>>();
        assert_eq!(updates.len(), 1);
        assert!(document.events().iter().any(|event| {
            event.seq > updates[0].seq
                && matches!(&event.event, EventBody::ApiStarted(api) if api.api_call_id == core_start.api_call_id)
        }));
        assert!(!document.metadata.dynamic_context.contains_key(context_injection::PROJECT_DIRECTORY_CONTEXT_KEY));
    }).unwrap();
}
