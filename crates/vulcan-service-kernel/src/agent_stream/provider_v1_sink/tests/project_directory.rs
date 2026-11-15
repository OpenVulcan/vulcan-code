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
    // A is the last path actually sent to this session before B and C are selected.
    // 用户先后选择 B、C 前，实际发给此会话的最后路径是 A。
    let mut project = TaskProjectSummary::new(
        "project-1",
        "Project",
        r"E:\old project",
        false,
        0,
        None,
        None,
        Vec::new(),
    );
    state
        .lock()
        .unwrap()
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .task_projects
        .push(project.clone());
    let mut initial = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    let mut candidate = candidate_for(&handle);
    candidate.dynamic_prompt_layers.runtime_context = format!(
        "Current workspace: {}\nCurrent execution mode: Build",
        project.path
    );
    let first = ProviderV1RequestFacts {
        request: ApiRequestSnapshot::new("provider-1", "model-1"),
        prepared_at_ms: 3,
        ..request_facts()
    };
    let PreparedPhysicalApi::SessionV1 { core_start, .. } = initial
        .prepare_project_request(&state, first.clone(), candidate, Some(&project))
        .await
        .unwrap()
    else {
        panic!("the original workspace must prepare");
    };
    finish_directory_fixture_api(&handle, &first, core_start.api_call_id).await;

    // Persisted project history contains just the most recent B to C link.
    // 项目持久记录只包含最近的 B 到 C 变更。
    project.path = r"D:\new project".into();
    project.root_directory_change = Some(ProjectRootDirectoryChange {
        previous_path: r"F:\never dispatched".into(),
        changed_at_ms: 8,
    });
    let mut sink = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    // Install the exact binding required by the final permit check.
    // 安装最终许可复核要求的精确绑定。
    state
        .lock()
        .unwrap()
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .task_projects[0] = project.clone();
    let facts = ProviderV1RequestFacts {
        request: ApiRequestSnapshot::new("provider-1", "model-1"),
        prepared_at_ms: 10,
        ..request_facts()
    };
    let mut candidate = candidate_for(&handle);
    candidate.dynamic_prompt_layers.runtime_context = format!(
        "Current workspace: {}\nCurrent execution mode: Build",
        project.path
    );
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

    assert!(!notices[0].contains("never dispatched"));
    finish_directory_fixture_api(&handle, &facts, core_start.api_call_id).await;
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

/// An unused project path creates no previous-directory notice for the first real request.
/// 尚未使用过的项目路径不会让首次真实请求显示虚假的旧路径。
#[tokio::test]
async fn project_directory_first_request_has_no_unused_previous_path() {
    let (_directory, handle) = published_stored_open_turn();
    // Reuse the live producer with a saved change and a session that never dispatched.
    // 使用实时生产器，输入已保存的变更以及从未实际派发过的会话。
    let mut project = TaskProjectSummary::new(
        "project-1",
        "Project",
        "/current",
        false,
        0,
        None,
        None,
        vec![],
    );
    project.root_directory_change = Some(ProjectRootDirectoryChange {
        previous_path: "/never-used".into(),
        changed_at_ms: 5,
    });
    context_injection::synchronize_project_request_dynamic_context(
        &handle,
        &request_facts(),
        &candidate_for(&handle),
        Some(&project),
    )
    .unwrap();
    assert!(
        !handle
            .read(|document| document
                .metadata
                .dynamic_context
                .contains_key(context_injection::PROJECT_DIRECTORY_CONTEXT_KEY))
            .unwrap()
    );
}

/// Observes and settles the fixture's API in the same logical turn before the next request prepares.
/// 在同一逻辑回合内观测并结算夹具的 API，然后准备下一次请求。
async fn finish_directory_fixture_api(
    handle: &Arc<SessionHandle>,
    facts: &ProviderV1RequestFacts,
    api_call_id: ApiCallId,
) {
    handle
        .mark_api_transport_observed_durable(api_call_id, facts.prepared_at_ms + 1)
        .await
        .unwrap();
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.complete_api_group(
                    api_call_id,
                    ApiCompletion {
                        request: facts.request.clone(),
                        usage: UsageSnapshot::default(),
                        status: ApiCallStatus::Completed,
                        end_reason: Some(ApiEndReason::CompletedSuccessfully),
                        error: None,
                        continuation_eligibility: ContinuationEligibility::Forbidden,
                        timing: ApiTimings::default(),
                        provider_source: None,
                        completed_at_ms: facts.prepared_at_ms + 2,
                    },
                )
            },
        )
        .unwrap();
}

/// Exercises the detached physical preparation entry with stale and refreshed tool bindings.
/// 使用过时和已刷新的工具绑定测试分离物理请求准备入口。
#[tokio::test]
async fn detached_project_directory_rebuilds_before_releasing_model_payload() {
    let (_directory, handle) = published_stored_open_turn();
    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(handle.session_id());
    service
        .session_runtime_registry
        .register(Arc::clone(&handle))
        .unwrap();
    let old = TaskProjectSummary::new(
        "project-1",
        "Project",
        "/old",
        false,
        0,
        None,
        None,
        Vec::new(),
    );
    let mut current = old.clone();
    current.path = "/current".into();
    current.root_directory_change = Some(ProjectRootDirectoryChange {
        previous_path: old.path.clone(),
        changed_at_ms: 5,
    });
    service
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .task_projects
        .push(old.clone());
    let state = Arc::new(std::sync::Mutex::new(service));
    // Establish the directory actually used by the detached session before rebinding.
    // 在换目录前建立独立会话实际使用过的目录。
    let mut initial = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    let mut candidate = candidate_for(&handle);
    candidate.project_directory = Some(old.clone());
    candidate.dynamic_prompt_layers.runtime_context = format!(
        "Current workspace: {}\nCurrent execution mode: Build",
        old.path
    );
    let first = ProviderV1RequestFacts {
        request: ApiRequestSnapshot::new("provider-1", "model-1"),
        prepared_at_ms: 3,
        ..request_facts()
    };
    let PreparedPhysicalApi::SessionV1 { core_start, .. } = initial
        .prepare_request(&state, first.clone(), candidate)
        .await
        .unwrap()
    else {
        panic!("the original detached workspace must prepare");
    };
    finish_directory_fixture_api(&handle, &first, core_start.api_call_id).await;
    *state
        .lock()
        .unwrap()
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .task_projects
        .iter_mut()
        .find(|project| project.id == current.id)
        .unwrap() = current.clone();
    let mut sink = ProviderV1StreamSink::new(
        Arc::clone(&handle),
        Arc::new(ProjectResourceCatalogRegistry::default()),
    );
    let facts = ProviderV1RequestFacts {
        request: ApiRequestSnapshot::new("provider-1", "model-1"),
        prepared_at_ms: 10,
        ..request_facts()
    };
    let mut stale = candidate_for(&handle);
    stale.project_directory = Some(old);
    assert!(matches!(
        sink.prepare_request(&state, facts.clone(), stale)
            .await
            .unwrap(),
        PreparedPhysicalApi::RequestConfigurationChanged
    ));
    assert!(
        handle
            .read(|document| document.active_api().is_none())
            .unwrap()
    );

    let mut fresh = candidate_for(&handle);
    fresh.project_directory = Some(current);
    let PreparedPhysicalApi::SessionV1 { exact_request, .. } =
        sink.prepare_request(&state, facts, fresh).await.unwrap()
    else {
        panic!("the detached retry must obtain the new request");
    };
    assert!(exact_request.messages.iter().flat_map(|message| &message.content).any(|item| matches!(
        item, ContentItem::Text { text } if text.contains(context_injection::PROJECT_DIRECTORY_CONTEXT_KEY)
            && text.contains("/old") && text.contains("/current")
    )));
}
