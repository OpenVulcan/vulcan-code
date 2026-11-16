//! Durable context generation and summary planning use different instruction frontiers.
//! 可靠上下文代次与摘要规划使用不同的指令前沿。

use super::*;
use vulcan_session_core::v1::event::{SystemContextOperation, SystemContextScope};

/// Publishes one typed memory update or removal through the real durable owner.
/// 通过真实可靠所有者发布一项类型化记忆更新或移除。
/// The operation, key and body identify the host fact; returns its committed sequence.
/// 操作、键及正文标识宿主事实；返回其已提交序号。
fn append_memory(
    handle: &SessionHandle,
    operation: SystemContextOperation,
    key: &str,
    body: &str,
    now_ms: i64,
) -> Seq {
    handle
        .mutate_durable(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::ContextChanged,
            |document| {
                document.append_system_context(
                    None,
                    key,
                    operation,
                    SystemContextScope::Session,
                    None,
                    None,
                    body,
                    now_ms,
                )
            },
        )
        .unwrap()
        .value
}

/// Extracts typed User text from a snapshot without interpreting envelope strings as authority.
/// 从快照提取类型化 User 正文，不将信封字符串解释为权威。
/// Returns concatenated text solely for assertions.
/// 仅为断言返回连接后的正文。
fn user_text(snapshot: &SessionProviderProjectionSnapshot) -> String {
    snapshot
        .items
        .iter()
        .filter_map(|item| match item {
            ProjectedItem::UserInput { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Verifies an unused session previews current project rules in the initial system prompt without dispatch.
/// 验证未使用的会话无需派发即可在初始系统提示词中预览当前项目规则。
#[test]
fn request_context_simulation_projects_fresh_project_rules() {
    // Store the project and session in the same isolated fixture root.
    // 在同一个隔离夹具根目录保存项目和会话。
    let temporary = tempfile::tempdir().unwrap();
    let session_id = SessionId::new_v4();
    let locator =
        create_session_v1_storage_fixture(temporary.path(), &session_id.to_string(), 1).unwrap();
    // The canonical file is read before any baseline has been published.
    // 在基线发布之前读取规范项目规则文件。
    std::fs::write(
        temporary.path().join("AGENTS.md"),
        "## fixture project rules",
    )
    .unwrap();
    let agent_config =
        vulcan_agent_runtime::agent_runtime::config::AgentRuntimeConfig::from_sources(
            &Default::default(),
            &Default::default(),
        );
    let preview = simulate_request_context(
        &locator,
        temporary.path().to_str().unwrap(),
        &agent_config,
        |_| {
            Ok(vec![serde_json::json!({
                "type": "function",
                "name": "fixture_tool",
                "parameters": {"type": "object"},
            })])
        },
    )
    .unwrap();
    assert!(preview["nodes"].as_array().unwrap().iter().any(|row| {
        row["field"] == "tools"
            && serde_json::from_str::<serde_json::Value>(row["content"].as_str().unwrap())
                .is_ok_and(|tool| {
                    tool["name"] == "fixture_tool" && tool["parameters"]["type"] == "object"
                })
    }));
    let rules = preview["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| {
            row["text"]
                .as_str()
                .is_some_and(|body| body.contains("fixture project rules"))
        })
        .expect("the local AGENTS.md is previewed");
    assert_eq!(rules["role"], "system");
    assert!(
        preview["promptOrigins"]
            .as_array()
            .unwrap()
            .iter()
            .any(|origin| {
                origin["kind"] == "agents_md" && origin["tag"] == "vulcan_context_agent_md"
            })
    );
    assert!(preview.get("dispatch").is_none());
    assert!(
        load_session_v1_full(&locator)
            .unwrap()
            .document
            .events()
            .is_empty()
    );
}

/// Verifies persisted compaction freezes prefix selection while summary planning prunes expired instructions.
/// 验证持久压缩冻结前缀选择，同时摘要规划裁剪失效指令。
/// Exercises real files, a genuine compaction API and a cold owner; no provider network is used.
/// 覆盖真实文件、真正压缩 API 及冷所有者；不使用供应商网络。
#[test]
fn durable_context_generation_preserves_prefix_after_expiry_and_cold_load() {
    // The exact locator and owner are the same persistence path used by physical request preparation.
    // 精确定位器与所有者沿用物理请求准备的持久化路径。
    let temporary = tempfile::tempdir().unwrap();
    let session_id = SessionId::new_v4();
    let locator =
        create_session_v1_storage_fixture(temporary.path(), &session_id.to_string(), 1).unwrap();
    let loaded = load_session_v1_full(&locator).unwrap();
    let handle = SessionHandle::new(session_id, locator.clone(), loaded.document);
    append_memory(
        &handle,
        SystemContextOperation::Upsert,
        "memory",
        "retain-until-later",
        2,
    );
    handle
        .submit_root_input(test_client_input("root", "user-owned-request"), 3)
        .unwrap();
    // Root admission also emits control facts; only the exact admitted input owns the retained user boundary.
    // 根准入还会发出控制事实；只有精确准入输入拥有用户保留边界。
    let retained = handle
        .read(|document| {
            document
                .events()
                .iter()
                .find_map(|event| match &event.event {
                    EventBody::UserInput(input) if input.client_action_id.as_str() == "root" => {
                        Some(event.seq())
                    }
                    _ => None,
                })
                .unwrap()
        })
        .unwrap();
    append_memory(
        &handle,
        SystemContextOperation::Upsert,
        "old-tail",
        "expired-before-compaction",
        4,
    );
    append_memory(
        &handle,
        SystemContextOperation::Remove,
        "old-tail",
        "old-tail-expired",
        5,
    );
    let initial_provider = handle.capture_controller_projection_snapshot().unwrap();
    let initial_summary = handle
        .capture_context_compaction_planning_snapshot()
        .unwrap();
    // Source ownership is captured from the same real open turn as the durable history.
    // 来源归属从可靠历史所属的同一真实开放回合捕获。
    let open_turn_id = handle
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    assert_eq!(initial_summary.open_turn_id, Some(open_turn_id));
    assert_eq!(initial_provider.open_turn_id, Some(open_turn_id));
    assert!(initial_summary.child_business_execution_id.is_none());
    assert!(user_text(&initial_provider).contains("expired-before-compaction"));
    assert!(!user_text(&initial_summary).contains("expired-before-compaction"));

    // Commit a genuine compaction node so ordinary requests bind to its immutable API terminal sequence.
    // 提交真正压缩节点，使普通请求绑定其不可变 API 终态序号。
    let request = ApiRequestSnapshot::new("openai", "gpt-5");
    let input = handle
        .read(|document| {
            test_prepare_api_input(document, ApiPurposeKind::Compaction, request.clone(), 6)
        })
        .unwrap();
    let api = handle
        .prepare_and_start_api_durable(input)
        .unwrap()
        .value
        .api_call_id;
    // Reasoning and text are separate durable events with their own revisions.
    // 思考和正文是各有独立修订号的可靠事件。
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            |document| {
                document.append_reasoning(
                    api,
                    "summary reasoning",
                    AssistantSemanticStatus::Completed,
                    7,
                )
            },
        )
        .unwrap();
    // Persist the answer after its reasoning prefix in the same physical call.
    // 在同一次实发调用内，于思考前缀后持久化回答。
    let summary_seq = handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            |document| {
                document.append_assistant_text(
                    api,
                    "summary",
                    AssistantSemanticStatus::Completed,
                    7,
                )
            },
        )
        .unwrap()
        .value
        .into_value();
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.complete_api_group(
                    api,
                    ApiCompletion {
                        request,
                        usage: UsageSnapshot::default(),
                        status: ApiCallStatus::Completed,
                        end_reason: Some(ApiEndReason::CompletedSuccessfully),
                        error: None,
                        continuation_eligibility: ContinuationEligibility::Forbidden,
                        timing: ApiTimings::default(),
                        provider_source: None,
                        completed_at_ms: 8,
                    },
                )
            },
        )
        .unwrap();
    handle
        .record_compaction(
            CompactionCommitInput::new(
                api,
                summary_seq,
                retained,
                "summary",
                LockedBaselineProjection::empty(),
            ),
            9,
        )
        .unwrap();
    let frozen = handle.capture_controller_projection_snapshot().unwrap();
    assert!(user_text(&frozen).contains("retain-until-later"));
    assert!(!user_text(&frozen).contains("expired-before-compaction"));
    append_memory(
        &handle,
        SystemContextOperation::Remove,
        "memory",
        "later-expiry-marker",
        10,
    );
    let current = handle.capture_controller_projection_snapshot().unwrap();
    assert!(current.sourced_items.starts_with(&frozen.sourced_items));
    assert!(user_text(&current).contains("later-expiry-marker"));
    let summary = handle
        .capture_context_compaction_planning_snapshot()
        .unwrap();
    assert!(!user_text(&summary).contains("retain-until-later"));
    assert!(user_text(&summary).contains("user-owned-request"));
    assert_eq!(current.current_compaction_id, summary.current_compaction_id);
    assert_eq!(current.document_revision, summary.document_revision);

    // Preview the existing generation directly from disk, without using the runtime request slot.
    // 直接从磁盘预览已有代际，无需使用运行时请求槽。
    // Resolve a real project override even when the durable session is opened from disk.
    // 即使会话从磁盘打开，也解析真实的项目级提示词覆盖。
    let prompt_path =
        vulcan_provider_config::config::project_system_prompt_file_path(temporary.path());
    vulcan_provider_config::config::save_system_prompt_file(
        &prompt_path,
        "project-owned-foundation",
    )
    .unwrap();
    // Supply normal agent definitions without accessing a provider.
    // 提供正常代理定义，无需访问供应商。
    let agent_config =
        vulcan_agent_runtime::agent_runtime::config::AgentRuntimeConfig::from_sources(
            &Default::default(),
            &Default::default(),
        );
    // Assemble without dispatching or changing the generation.
    // 在无实发、无代际变更的情况下组装。
    let preview = simulate_request_context(
        &locator,
        temporary.path().to_str().unwrap(),
        &agent_config,
        |_| Ok(Vec::new()),
    )
    .unwrap();
    let rows = preview["nodes"].as_array().unwrap();
    let first = rows
        .first()
        .expect("the fixed system instruction is present");
    assert_eq!(first["role"], "system");
    assert!(
        first["content"]
            .as_str()
            .unwrap()
            .contains("project-owned-foundation")
    );
    assert!(rows.iter().any(|row| {
        row["content"]
            .as_str()
            .is_some_and(|text| text.contains("user-owned-request"))
    }));
    assert!(rows.iter().any(|row| {
        row["content"]
            .as_str()
            .is_some_and(|text| text.contains("summary"))
    }));
    assert!(preview.get("dispatch").is_none());
    // A response must belong to the inspected API and must not be borrowed by another one.
    // 回复必须属于受检 API，其他 API 不能借用该回复。
    handle
        .read(|document| {
            let response =
                latest_context_assistant_response(document.events(), api.value()).unwrap();
            // The final answer carries reasoning only from its own completed API group.
            // 最终回答只携带其所属已完成 API 组的思考。
            let response = serde_json::to_value(response).unwrap();
            assert_eq!(response["content"], "summary");
            assert_eq!(response["reasoning"], "summary reasoning");
            assert!(latest_context_assistant_response(document.events(), u64::MAX).is_none());
        })
        .unwrap();

    // Reading a new owner must reconstruct the same immutable prefix without relying on cached state.
    // 新所有者读取必须重建相同不可变前缀，不依赖缓存状态。
    let reloaded = load_session_v1_full(&locator).unwrap();
    assert_eq!(
        reloaded.document.memory_revision(),
        current.document_revision
    );
    let cold = SessionHandle::new(session_id, locator, reloaded.document);
    let cold_provider = cold.capture_controller_projection_snapshot().unwrap();
    assert_eq!(cold_provider.sourced_items, current.sourced_items);
    let cold_summary = cold.capture_context_compaction_planning_snapshot().unwrap();
    assert_eq!(cold_summary.sourced_items, summary.sourced_items);
}

/// Verifies physical summary preparation and its owned recapture preserve the current-instruction view.
/// 验证物理摘要准备及其自有重捕获保持当前有效指令视图。
/// Uses real durable preparation and API admission; ordinary requests retain the sent prefix.
/// 使用真实可靠准备及 API 准入；普通请求仍保留已经发送的前缀。
#[tokio::test]
async fn physical_compaction_preparation_uses_current_instruction_view() {
    // Each purpose owns a separate real store so one active API cannot influence the other.
    // 每种用途拥有独立真实存储，避免一个活动 API 影响另一用途。
    for purpose in [ApiPurposeKind::MainGeneration, ApiPurposeKind::Compaction] {
        let temporary = tempfile::tempdir().unwrap();
        let session_id = SessionId::new_v4();
        let locator =
            create_session_v1_storage_fixture(temporary.path(), &session_id.to_string(), 1)
                .unwrap();
        let loaded = load_session_v1_full(&locator).unwrap();
        let handle = SessionHandle::new(session_id, locator.clone(), loaded.document);
        handle
            .submit_root_input(test_client_input("root", "real-user-request"), 2)
            .unwrap();
        append_memory(
            &handle,
            SystemContextOperation::Upsert,
            "memory",
            "expired-summary-secret",
            3,
        );
        append_memory(
            &handle,
            SystemContextOperation::Remove,
            "memory",
            "memory-expired",
            4,
        );
        append_memory(
            &handle,
            SystemContextOperation::Upsert,
            "live",
            "current-summary-memory",
            5,
        );
        let ordinary = handle.capture_controller_projection_snapshot().unwrap();
        let is_summary = purpose == ApiPurposeKind::Compaction;
        let receipt = handle
            .prepare_physical_api_durable(
                purpose,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                LockedBaselineProjection::empty(),
                None,
                6,
                move |snapshot, preparation| async move {
                    assert_eq!(
                        user_text(&snapshot).contains("expired-summary-secret"),
                        !is_summary
                    );
                    assert!(user_text(&snapshot).contains("real-user-request"));
                    assert!(user_text(&snapshot).contains("current-summary-memory"));
                    // Production encoding performs this exact recapture after media work.
                    // 生产编码在媒体工作后执行此精确重捕获。
                    let recaptured = preparation
                        .recapture()?
                        .expect("unchanged durable frontier");
                    assert_eq!(snapshot.sourced_items, recaptured.sourced_items);
                    assert!(snapshot.open_turn_id.is_some());
                    assert_eq!(snapshot.open_turn_id, recaptured.open_turn_id);
                    assert_eq!(
                        snapshot.child_business_execution_id,
                        recaptured.child_business_execution_id
                    );
                    assert_eq!(
                        user_text(&recaptured).contains("expired-summary-secret"),
                        !is_summary
                    );
                    Ok(Some(SessionPreparedPhysicalRequest {
                        transport: (),
                        user_prompt_media: Vec::new(),
                        projection: ProviderProjectionProof {
                            segment_generation: recaptured.segment_generation,
                            document_revision: recaptured.document_revision,
                            retained_from_seq: recaptured.retained_from_seq,
                            through_seq: recaptured.through_seq,
                            fingerprint_sha256: "a".repeat(64),
                        },
                        request_budget_evidence: test_provider_request_budget_evidence(),
                    }))
                },
            )
            .await
            .unwrap()
            .expect("physical API admitted");
        assert_eq!(
            load_session_v1_full(&locator)
                .unwrap()
                .document
                .active_api()
                .unwrap()
                .api_call_id,
            receipt.value.core_start.api_call_id
        );
        let after = handle.capture_controller_projection_snapshot().unwrap();
        assert!(after.sourced_items.starts_with(&ordinary.sourced_items));
        assert!(user_text(&after).contains("expired-summary-secret"));
    }
}
