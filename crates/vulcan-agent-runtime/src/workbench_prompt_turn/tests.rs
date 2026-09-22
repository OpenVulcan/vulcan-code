//! Tests for shared prompt-turn routing decisions.
//! 共享提示词回合路由决策测试。

use vulcan_provider_config::config::ProjectVulcanCodeConfigurationFile;
use vulcan_session_boundary::{SessionBoundaryEventKind, SessionEventVisibility};
use vulcan_session_core::v1::ids::ClientActionId;
use vulcan_vmm::{ContextItem, VmmPromptAnchorKind, VmmRecallBatch, VmmSessionMemoryState};
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionMediaUnderstandingSelection,
    SessionProviderToolTargetSelection, SessionRuntimeStateEntry, VmmProfileSyncState,
};

use crate::{
    agent_runtime::conversation_hooks::{
        ConversationHookRuntime, HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT,
        HOOK_METADATA_VMM_RECALL_BATCH_EFFECT, VmmProfileSyncHookEffect, VmmRecallBatchHookEffect,
    },
    conversation_protocol::{SessionConversationPayload, append_user_text_message},
};

use super::*;

/// Shared preparation preserves per-input identities through the actual completed-turn Hook payload.
/// 共享准备通过实际完成回合 Hook 载荷保留逐输入身份。
#[tokio::test]
async fn canonical_in_turn_shared_preparation_does_not_duplicate_the_combined_prompt() {
    use crate::agent_runtime::conversation_hooks::CanonicalInTurnHookInput;
    use vulcan_session_core::v1::{PendingInputId, Seq};
    // The existing runtime-scoped entry creates the one business owner used by both batch attempts.
    // 既有运行态入口创建两次批次尝试共用的唯一业务所有者。
    let session_id = SessionId::from_u128(923);
    let mut state = SessionRuntimeStateEntry::default();
    let mut hooks = ConversationHookRuntime::new();
    let make_input =
        |source, prompt: &'static str, entry: &str| SharedPromptTurnRuntimeBeginInput {
            session_id,
            session_title: "过程续跑",
            workspace_path: "D:/workspace",
            prompt_entry_id: entry.to_owned(),
            continuation_action_id: None,
            prompt,
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: false,
            append_visible_user_prompt_record: false,
            source,
            vmm_runtime_ready: false,
            vmm_precheck_ready: false,
            execution_mode: "build".into(),
            reasoning_effort: None,
            owning_project_id: Some("project-a".into()),
        };
    let root = begin_shared_prompt_turn_with_runtime_state(
        &mut state,
        &mut hooks,
        make_input(
            PendingSessionPromptSource::UserFollowup,
            "Original work",
            "root",
        ),
    )
    .unwrap();
    hooks.record_queued_user_prompt(
        session_id,
        "queue-a",
        None,
        "D:/workspace".into(),
        PendingSessionPromptSource::UserFollowup,
        "Queued work",
    );
    // These coordinates are the same shape supplied by the service's exact pending-input authority.
    // 这些坐标与服务精确待输入权威提供的结构一致。
    let inputs = vec![
        CanonicalInTurnHookInput {
            client_action_id: ClientActionId::try_new("queue-a").unwrap(),
            input_id: PendingInputId::try_new(3).unwrap(),
            input_seq: Seq::try_new(10).unwrap(),
            text: "Queued work".into(),
        },
        CanonicalInTurnHookInput {
            client_action_id: ClientActionId::try_new("direct-b").unwrap(),
            input_id: PendingInputId::try_new(4).unwrap(),
            input_seq: Seq::try_new(11).unwrap(),
            text: "Direct work".into(),
        },
    ];
    for batch_id in ["webui-v1-pending-in-turn-1-9", "retry-batch"] {
        let started = begin_shared_prompt_turn_with_runtime_input_projection(
            &mut state,
            &mut hooks,
            make_input(
                PendingSessionPromptSource::UserInterrupt,
                "Queued work\n\nDirect work",
                batch_id,
            ),
            Some(&inputs),
        )
        .unwrap();
        assert_eq!(started.preparation_turn_id, root.preparation_turn_id);
        assert_eq!(started.prompt, "Queued work\n\nDirect work");
        assert!(state.conversation_state.messages.is_empty());
        assert!(state.conversation_state.records.is_empty());
    }
    // Pure explicit Continue preserves the original owner without inventing a user timeline row.
    // 纯显式 Continue 保留原所有者，不虚构用户时间线条目。
    let resumed = begin_shared_prompt_turn_with_runtime_input_projection(
        &mut state,
        &mut hooks,
        make_input(
            PendingSessionPromptSource::ExecutionRetry,
            "",
            "pure-continue",
        ),
        Some(&[]),
    )
    .unwrap();
    assert_eq!(resumed.preparation_turn_id, root.preparation_turn_id);
    // The actual dispatch report proves the VMM-facing consumer receives individual history only once.
    // 实际派发报告证明面向 VMM 的消费方仅收到一次逐条历史。
    let (_, completed) = hooks.prepare_turn_completion_dispatch(
        session_id,
        None,
        "done",
        1,
        &state.conversation_state,
    );
    let report = completed.run().await.report.unwrap();
    assert_eq!(report.payload.value["user_content"], "Original work");
    let timeline = report.payload.value["timeline"].as_array().unwrap();
    assert_eq!(timeline.len(), 2);
    assert_eq!(timeline[0]["source_id"], "queue-a");
    assert_eq!(timeline[1]["source_id"], "direct-b");
    assert_eq!(timeline[1]["content"], "Direct work");
}

/// Agent continuation retains the original Hook and creates no human history or manual injection.
/// 代理续跑保留原 Hook，不创建人类历史或手动注入。
#[test]
fn agent_continuation_preparation_reuses_business_hook_without_human_side_effects() {
    let session_id = SessionId::from_u128(922);
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut hooks = ConversationHookRuntime::new();
    let make_input = |source| SharedPromptTurnRuntimeBeginInput {
        session_id,
        session_title: "业务续跑",
        workspace_path: "D:/workspace",
        prompt_entry_id: "agent-continuation".into(),
        continuation_action_id: None,
        prompt: "@inject 3 unexpected",
        hidden_model_guidance: None,
        provider_visible_agent_messages: Vec::new(),
        attachments: Vec::new(),
        provider_attachments: None,
        existing_canonical_user_message_id: None,
        append_canonical_user_message: false,
        append_visible_user_prompt_record: false,
        source,
        vmm_runtime_ready: false,
        vmm_precheck_ready: false,
        execution_mode: "Build".into(),
        reasoning_effort: None,
        owning_project_id: Some("project-a".into()),
    };
    assert!(
        begin_shared_prompt_turn_with_runtime_state(
            &mut runtime_state,
            &mut hooks,
            make_input(PendingSessionPromptSource::AgentContinuation)
        )
        .is_err()
    );
    assert!(runtime_state.active_precheck_turn_id.is_none());
    assert!(runtime_state.conversation_state.messages.is_empty());
    assert!(runtime_state.conversation_state.records.is_empty());
    assert!(!hooks.has_active_turn(session_id));
    let root = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hooks,
        SharedPromptTurnRuntimeBeginInput {
            prompt: "Original business",
            ..make_input(PendingSessionPromptSource::UserFollowup)
        },
    )
    .unwrap();
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hooks,
        make_input(PendingSessionPromptSource::AgentContinuation),
    )
    .unwrap();
    assert_eq!(start.preparation_turn_id, root.preparation_turn_id);
    assert_eq!(start.prompt, "@inject 3 unexpected");
    assert!(start.boundary_events.is_empty());
    assert!(runtime_state.conversation_state.messages.is_empty());
    assert!(runtime_state.conversation_state.records.is_empty());
    let mut memory = test_vmm_session_memory(session_id);
    let continued = continue_shared_prompt_turn_after_preparation_with_runtime_state(
        &mut runtime_state,
        &mut memory,
        SharedPromptTurnContinueInput {
            vmm_context_epoch: None,
            vmm_profile_read_source: None,
            precheck_outcome: start.precheck_request.clone().skip(),
            profile_outcome: start.profile_request.clone().skip(),
            start,
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: false,
        },
    )
    .unwrap();
    assert_eq!(
        continued.request.conversation_hook_turn_id.as_deref(),
        Some(root.preparation_turn_id.as_str())
    );
    assert_eq!(
        continued.request.prompt_purpose,
        ExecutionPromptPurpose::UserTurn
    );
    assert!(continued.request.boundary_events.is_empty());
    assert!(continued.request.conversation_state.messages.is_empty());
}

/// Exercises auxiliary preparation without creating human history, business boundaries or Hook ownership.
/// 验证辅助准备不会创建人类历史、业务边界或 Hook 归属。
#[test]
fn question_execution_preparation_has_no_business_turn_side_effects() {
    // The auxiliary execution belongs to an existing idle session.
    // 辅助执行属于既有空闲会话。
    let session_id = SessionId::from_u128(919);
    // Local state remains separate from the authoritative V1 collaboration events.
    // 本地状态与权威 V1 协作事件保持分离。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    // No business Hook should be created by this shared preparation entry point.
    // 此共享准备入口不应创建业务 Hook。
    let mut hook_runtime = ConversationHookRuntime::new();
    // Actual question content is projected from V1, never from a synthetic user prompt.
    // 真实问题内容由 V1 投影，绝不来自虚构用户提示。
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id,
            session_title: "答疑",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("qa-execution-1"),
            continuation_action_id: None,
            prompt: "",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: false,
            append_visible_user_prompt_record: false,
            source: PendingSessionPromptSource::QuestionExecution,
            vmm_runtime_ready: false,
            vmm_precheck_ready: false,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("auxiliary preparation");
    assert!(start.canonical_user_message_id.is_none());
    assert!(start.boundary_events.is_empty());
    assert!(runtime_state.conversation_state.messages.is_empty());
    assert!(runtime_state.conversation_state.records.is_empty());
    // Keep the existing VMM readiness contract while proving no business turn is registered.
    // 保留既有 VMM 就绪契约，同时证明没有注册业务回合。
    let mut memory = test_vmm_session_memory(session_id);
    // Finish through the production reducer with separately correlated skipped Hook outcomes.
    // 使用分别关联的已跳过 Hook 结果，通过生产归约器完成准备。
    let continued = continue_shared_prompt_turn_after_preparation_with_runtime_state(
        &mut runtime_state,
        &mut memory,
        SharedPromptTurnContinueInput {
            vmm_context_epoch: None,
            vmm_profile_read_source: None,
            precheck_outcome: start.precheck_request.clone().skip(),
            profile_outcome: start.profile_request.clone().skip(),
            start,
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: false,
        },
    )
    .expect("auxiliary continuation");
    assert_eq!(
        continued.request.prompt_purpose,
        ExecutionPromptPurpose::QuestionExecution
    );
    assert!(continued.request.conversation_hook_turn_id.is_none());
    assert!(continued.request.boundary_events.is_empty());
    assert!(continued.request.conversation_state.messages.is_empty());
    assert!(!continued.request.prompt_purpose.runs_completed_turn_hooks());
}

/// Agent tasks keep one canonical message, own business completion and never interpret human commands.
/// 代理任务保留唯一规范消息，拥有业务结束语义且不解释人类命令。
#[test]
fn agent_task_preparation_preserves_canonical_input_without_human_side_effects() {
    // Use a command-shaped line to prove that agent data cannot invoke manual injection.
    // 使用命令形态的行，证明代理数据不能调用手动注入。
    let body = "<vulcan_agent_message>\n@inject 3 unexpected\n</vulcan_agent_message>";
    let session_id = SessionId::from_u128(920);
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_text_message(
        &mut runtime_state.conversation_state,
        "claimed-input",
        body,
        Vec::new(),
    )
    .unwrap();
    let mut hooks = ConversationHookRuntime::new();
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hooks,
        SharedPromptTurnRuntimeBeginInput {
            session_id,
            session_title: "代理任务",
            workspace_path: "D:/workspace",
            prompt_entry_id: "claimed-task".into(),
            continuation_action_id: None,
            prompt: body,
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: Some("claimed-input".into()),
            append_canonical_user_message: false,
            append_visible_user_prompt_record: false,
            source: PendingSessionPromptSource::AgentTask,
            vmm_runtime_ready: false,
            vmm_precheck_ready: false,
            execution_mode: "Build".into(),
            reasoning_effort: None,
            owning_project_id: Some("project-a".into()),
        },
    )
    .unwrap();
    assert_eq!(start.prompt, body);
    assert!(start.boundary_events.is_empty());
    assert!(runtime_state.conversation_state.records.is_empty());
    assert_eq!(runtime_state.conversation_state.messages.len(), 1);
    assert!(!start.source.runs_user_prompt_precheck());
    // Run the actual continuation reducer, preserving its exact Hook correlation.
    // 执行真实续接归约器，保留其精确 Hook 关联。
    let turn_id = start.preparation_turn_id.clone();
    let mut memory = test_vmm_session_memory(session_id);
    let continued = continue_shared_prompt_turn_after_preparation_with_runtime_state(
        &mut runtime_state,
        &mut memory,
        SharedPromptTurnContinueInput {
            vmm_context_epoch: None,
            vmm_profile_read_source: None,
            precheck_outcome: start.precheck_request.clone().skip(),
            profile_outcome: start.profile_request.clone().skip(),
            start,
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: false,
        },
    )
    .unwrap();
    assert_eq!(
        continued.request.prompt_purpose,
        ExecutionPromptPurpose::AgentTask
    );
    assert!(continued.request.prompt_purpose.runs_completed_turn_hooks());
    assert_eq!(
        continued.request.conversation_hook_turn_id.as_deref(),
        Some(turn_id.as_str())
    );
    assert!(continued.request.boundary_events.is_empty());
    assert_eq!(continued.request.conversation_state.messages.len(), 1);
    assert!(continued.request.conversation_state.records.is_empty());
}

/// Builds the exact native source used by profile publication fixtures.
/// 构建画像发布夹具使用的准确原生来源。
fn test_profile_read_source() -> vulcan_vmm::VmmMemoryReadSource {
    vulcan_vmm::VmmMemoryReadSource::new(
        vulcan_vmm::VmmMemorySource::native(7).expect("profile owner"),
        vec![7],
    )
    .expect("profile read source")
}

/// Exercises the actual prompt reducer with current, stale, disabled and mixed-source recall/status effects.
/// 使用当前、过期、停用与混合来源召回及状态效果验证实际提示词归约器。
/// Returns after proving only the exact current source reaches the request and lifecycle state.
/// 证明只有准确当前来源进入请求及生命周期状态后返回。
#[test]
fn precheck_publication_rejects_stale_recall_and_lifecycle_together() {
    for case in [
        "matched",
        "owner",
        "linked",
        "disabled",
        "backend",
        "mixed_status",
        "empty_stale",
    ] {
        // Each scenario has a real correlated prompt boundary and an independent pristine memory controller state.
        // 每个场景都有真实关联提示词边界与独立纯净记忆控制器状态。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        // The current host identity is deliberately independent from the incoming response.
        // 当前宿主身份刻意独立于传入响应。
        let current = test_profile_read_source();
        // Freeze the source to be admitted at the host publication boundary.
        // 冻结宿主发布边界准入的来源。
        let expected = (case != "disabled").then_some(current.clone());
        // Select the exact source divergence under test, not a numeric-memory-ID fallback.
        // 选择待测准确来源差异，而非数字记忆 ID 兜底。
        let incoming = match case {
            "owner" | "empty_stale" => vulcan_vmm::VmmMemoryReadSource::new(
                vulcan_vmm::VmmMemorySource::native(8).expect("other owner"),
                vec![8],
            )
            .expect("other source"),
            "linked" => vulcan_vmm::VmmMemoryReadSource::new(current.owner.clone(), vec![7, 8])
                .expect("linked source"),
            "backend" => vulcan_vmm::VmmMemoryReadSource::new(
                vulcan_vmm::VmmMemorySource::external(
                    &vulcan_vmm::VmmTransportConfig::from_effective(&VmmEffectiveConfig {
                        grpc_target: "127.0.0.1:50051".to_owned(),
                        ..Default::default()
                    }),
                    1,
                    7,
                )
                .expect("remote owner"),
                Vec::new(),
            )
            .expect("remote source"),
            _ => current.clone(),
        };
        // Mixed metadata must not let a valid recall conceal a foreign lifecycle decision.
        // 混合元数据不能让合法召回掩盖其他来源的生命周期决定。
        let status_source = if case == "mixed_status" {
            vulcan_vmm::VmmMemoryReadSource::new(
                vulcan_vmm::VmmMemorySource::native(8).expect("status owner"),
                vec![8],
            )
            .expect("status source")
        } else {
            incoming.clone()
        };
        // Begin through the existing canonical user-message path.
        // 通过既有规范用户消息路径启动。
        let start = begin_user_prompt(&mut runtime_state);
        // The profile line is independent and intentionally skipped.
        // 画像线路独立并刻意跳过。
        let profile_outcome = start.profile_request.clone().skip();
        // Only typed VMM effects are populated, preserving normal precheck correlation.
        // 仅填充类型化 VMM 效果，保留正常 precheck 关联。
        let mut precheck_outcome = start.precheck_request.clone().skip();
        // The report is built once before the real reducer receives it.
        // 在真实归约器接收前只构建一次报告。
        let report = precheck_outcome.report.as_mut().expect("report");
        report.metadata.insert(
            crate::agent_runtime::conversation_hooks::HOOK_METADATA_VMM_SESSION_STATUS_EFFECT
                .to_owned(),
            vec![serde_json::json!(
                crate::agent_runtime::conversation_hooks::VmmSessionStatusHookEffect {
                    read_source: status_source,
                    status: if case == "matched" {
                        vulcan_vmm::SessionMemoryStatus::Active
                    } else {
                        vulcan_vmm::SessionMemoryStatus::Recycled
                    },
                }
            )],
        );
        if case != "empty_stale" {
            report.metadata.insert(
                HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_owned(),
                vec![serde_json::json!(VmmRecallBatchHookEffect {
                    read_source: incoming,
                    batch: VmmRecallBatch {
                        ttl_turns: 2,
                        injection_tag: "vmm".to_owned(),
                        injection_description: "memory".to_owned(),
                        items: vec![ContextItem {
                            memory_id: 17,
                            text: "source-fenced-memory".to_owned(),
                            ..Default::default()
                        }],
                    },
                })],
            );
        }
        // The memory state is independently owned by the correlated host session.
        // 记忆状态独立归属于关联宿主会话。
        let mut state = test_vmm_session_memory(start.session_id);
        // Run the production reducer rather than only calling a metadata helper.
        // 执行生产归约器，而非仅调用元数据辅助函数。
        let result = continue_shared_prompt_turn_after_preparation_with_runtime_state(
            &mut runtime_state,
            &mut state,
            SharedPromptTurnContinueInput {
                vmm_context_epoch: Some(
                    vulcan_vmm::VmmContextEpoch::new(1, None).expect("fixture context epoch"),
                ),
                vmm_profile_read_source: expected,
                start,
                precheck_outcome,
                profile_outcome,
                project_configuration: ProjectVulcanCodeConfigurationFile::default(),
                runtime_reasoning_effort: None,
                provider_reasoning_effort: None,
                patch_visible_user_prompt_record: true,
            },
        )
        .expect(case);
        assert_eq!(
            state.session_memory_status,
            vulcan_vmm::SessionMemoryStatus::Active,
            "{case}"
        );
        assert_eq!(
            state.active_leases.len(),
            usize::from(case == "matched"),
            "{case}"
        );
        assert_eq!(
            result.precheck_failure_detail.is_some(),
            case != "matched",
            "{case}"
        );
        assert_eq!(
            state
                .cache_epoch
                .anchors
                .iter()
                .any(|anchor| anchor.prefix.contains("source-fenced-memory")),
            case == "matched",
            "{case}"
        );
    }
}

/// Verifies ordinary admission resynchronizes edited, rebound and rendering-mode cache lineages.
/// 验证普通准入会重新同步编辑、重绑及渲染模式变化后的缓存沿革。
/// Runs three explicit host transitions and checks the actual outgoing overlay; no Provider is called.
/// 执行三种明确宿主转换并检查实际传出覆盖层，不调用 Provider。
#[test]
fn ordinary_admission_reconciles_context_after_edit_rebind_and_mode_change() {
    // Each case owns an isolated controller candidate; source replacement must not retain old memory.
    // 每种情况持有隔离控制器候选；来源替换不能保留旧记忆。
    for (generation, project_id, initial_stable_mode) in [(2, 1, true), (1, 2, true), (1, 1, false)]
    {
        // The start creates the exact current canonical user independently of the old cache target.
        // 启动过程独立于旧缓存目标，创建准确当前规范用户。
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let start = begin_user_prompt(&mut runtime_state);
        let target = start
            .canonical_user_message_id
            .clone()
            .expect("current user");
        let mut state = test_vmm_session_memory(start.session_id);
        let source = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::native(1).expect("original owner"),
            vec![1],
        )
        .expect("original source");
        state
            .select_read_source(&start.session_id.to_string(), Some(&source), "1")
            .expect("source");
        state
            .synchronize_stable_anchor_rendering(initial_stable_mode, "1")
            .expect("mode");
        state
            .synchronize_context_epoch(
                &vulcan_vmm::VmmContextEpoch::new(1, None).expect("old context"),
                "1",
            )
            .expect("old epoch");
        state
            .apply_recall_batch(
                &start.session_id.to_string(),
                "old-position",
                &VmmRecallBatch {
                    ttl_turns: 3,
                    injection_tag: "VMM_MEMORY".to_owned(),
                    injection_description: "Relevant memories.".to_owned(),
                    items: vec![ContextItem {
                        memory_id: 17,
                        text: "resident-body".to_owned(),
                        ..Default::default()
                    }],
                },
                "1",
            )
            .expect("old recall");
        // Current binding and V1 lineage are independently supplied by their declared host owners.
        // 当前绑定与 V1 沿革由各自声明的宿主归属方独立提供。
        let current_source = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::native(project_id).expect("owner"),
            vec![project_id],
        )
        .expect("current source");
        let context = vulcan_vmm::VmmContextEpoch::new(generation, None).expect("current context");
        let continuation = continue_shared_prompt_turn_after_preparation_with_runtime_state(
            &mut runtime_state,
            &mut state,
            SharedPromptTurnContinueInput {
                vmm_context_epoch: Some(context.clone()),
                vmm_profile_read_source: Some(current_source),
                precheck_outcome: start.precheck_request.clone().skip(),
                profile_outcome: start.profile_request.clone().skip(),
                start,
                project_configuration: ProjectVulcanCodeConfigurationFile::default(),
                runtime_reasoning_effort: None,
                provider_reasoning_effort: None,
                patch_visible_user_prompt_record: true,
            },
        )
        .expect("ordinary admission");
        assert_eq!(state.cache_epoch.epoch_id, context.cache_epoch_id());
        assert!(state.cache_epoch.covered_compaction_id.is_none());
        assert!(
            state
                .cache_epoch
                .anchors
                .iter()
                .all(|anchor| anchor.target_message_id == target)
        );
        if project_id == 2 {
            assert!(state.active_leases.is_empty());
            assert!(state.cache_epoch.anchors.is_empty());
        } else {
            assert_eq!(state.active_leases.len(), 1);
            assert_eq!(state.cache_epoch.anchors.len(), 1);
            // Validate the emitted Provider overlay rather than only the controller's intermediate state.
            // 验证发出的 Provider 覆盖层，而非只检查控制器中间状态。
            let injection = continuation
                .request
                .context_injection
                .expect("current overlay");
            assert_eq!(
                injection.cache_epoch_id.as_deref(),
                Some(context.cache_epoch_id().as_str())
            );
            assert_eq!(injection.anchored_prefixes[0].target_message_id, target);
            assert!(
                injection.anchored_prefixes[0]
                    .prefix
                    .contains("resident-body")
            );
        }
    }
}

/// Creates strict controller-owned VMM state for one prompt-turn test session.
/// 为一个提示词回合测试会话创建严格归属于控制器的 VMM 状态。
fn test_vmm_session_memory(session_id: SessionId) -> VmmSessionMemoryState {
    VmmSessionMemoryState::new(session_id.to_string(), "1")
        .expect("test VMM session owner should be valid")
}

/// Verifies every enabled media and direct capability requires its exact session route.
/// 验证每项已启用的媒体与直连能力都需要精确会话路由。
#[test]
fn session_provider_tool_target_validation_requires_exact_media_routes() {
    // runtimeState represents one initialized session with all three media switches enabled.
    // runtimeState 表示一个已初始化且启用三项媒体开关的会话。
    let mut runtime_state = SessionRuntimeStateEntry {
        provider_tool_settings_initialized: true,
        provider_tool_enabled_capabilities: [
            String::from("image_understand"),
            String::from("video_understand"),
            String::from("audio_understand"),
        ]
        .into_iter()
        .collect(),
        media_understanding_selections: [
            "image_understand",
            "video_understand",
            "audio_understand",
        ]
        .into_iter()
        .map(|capability| {
            (
                String::from(capability),
                SessionMediaUnderstandingSelection::ProviderTool {
                    selection: SessionProviderToolTargetSelection {
                        connection_id: format!("{capability}-account"),
                        target_id: format!("{capability}-target"),
                    },
                },
            )
        })
        .collect(),
        ..SessionRuntimeStateEntry::default()
    };

    validate_session_provider_tool_target_completeness(&runtime_state)
        .expect("media switches with exact session routes must pass");

    runtime_state
        .media_understanding_selections
        .remove("video_understand");
    let error = validate_session_provider_tool_target_completeness(&runtime_state)
        .expect_err("media capability without an exact session route must fail");
    assert!(error.contains("video_understand"), "{error}");
    runtime_state.media_understanding_selections.insert(
        String::from("video_understand"),
        SessionMediaUnderstandingSelection::ProviderTool {
            selection: SessionProviderToolTargetSelection {
                connection_id: String::from("video-account"),
                target_id: String::from("video-target"),
            },
        },
    );

    // webSearch proves a non-media direct capability cannot omit its session target.
    // webSearch 证明非媒体直连能力不能缺少会话目标。
    runtime_state
        .provider_tool_enabled_capabilities
        .insert(String::from("web_search"));
    let error = validate_session_provider_tool_target_completeness(&runtime_state)
        .expect_err("direct capability without target must fail");
    assert!(error.contains("web_search"), "{error}");

    // selection completes the direct capability without changing exact media route ownership.
    // selection 补全直连能力，不改变精确媒体路由的归属。
    runtime_state.provider_tool_selections.insert(
        String::from("web_search"),
        SessionProviderToolTargetSelection {
            connection_id: String::from("search-account"),
            target_id: String::from("search-target"),
        },
    );
    validate_session_provider_tool_target_completeness(&runtime_state)
        .expect("complete direct and media session routes must pass");
}

/// Verifies canonical user ids cannot collide when multiple starts occur within one timestamp tick.
/// 验证多次启动发生在同一时间戳粒度内时规范用户标识仍不会碰撞。
#[test]
fn canonical_user_message_ids_are_unique_and_session_scoped() {
    // session_id is the shared owner embedded in both generated identities.
    // session_id 是嵌入两个生成身份中的共享会话所有者。
    let session_id = SessionId::from_u128(902);
    // first and second prove UUID generation remains collision resistant within one timestamp tick.
    // first 和 second 用于证明 UUID 生成在同一时间戳粒度内仍具备抗碰撞能力。
    let first = new_canonical_user_message_id(session_id);
    let second = new_canonical_user_message_id(session_id);

    assert_ne!(first, second);
    assert!(first.starts_with(&format!("session-{session_id}-protocol-user-")));
    assert!(second.starts_with(&format!("session-{session_id}-protocol-user-")));
}

/// Verifies a visible user turn stores the exact canonical id returned to VMM anchor ownership.
/// 验证可见用户回合会存储返回给 VMM 锚点归属使用的同一个规范身份。
#[test]
fn visible_user_prompt_reuses_authoritative_canonical_message_id() {
    // runtime_state receives both the canonical user message and its visible transcript record.
    // runtime_state 同时接收规范用户消息及其可见转录记录。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    // start carries the sole canonical identity generated by the prompt-turn owner.
    // start 携带由提示词回合所有者生成的唯一规范身份。
    let start = begin_user_prompt(&mut runtime_state);
    // canonical_user_message_id is the identity later consumed by VMM lease and anchor state.
    // canonical_user_message_id 是后续由 VMM 租约和锚点状态消费的身份。
    let canonical_user_message_id = start
        .canonical_user_message_id
        .as_deref()
        .expect("visible user prompt must expose its canonical identity");
    // matching_messages proves the returned identity exists exactly once in canonical history.
    // matching_messages 用于证明返回身份在规范历史中恰好存在一次。
    let matching_messages = runtime_state
        .conversation_state
        .messages
        .iter()
        .filter(|message| message.id == canonical_user_message_id)
        .collect::<Vec<_>>();

    assert_eq!(matching_messages.len(), 1);
    assert!(matches!(
        matching_messages[0].payload,
        SessionConversationPayload::UserText { .. }
    ));
    assert_eq!(runtime_state.conversation_state.records.len(), 1);
    // visibleRecordId is the distinct transcript-row identity required by segmented JSONL persistence.
    // visibleRecordId 是分段 JSONL 持久化所需的独立转录行身份。
    let visible_record_id = start
        .visible_user_prompt_record_id
        .as_deref()
        .expect("visible user prompt must expose its transcript record identity");
    assert_ne!(visible_record_id, canonical_user_message_id);
    assert_eq!(
        runtime_state
            .conversation_state
            .records
            .iter()
            .filter(|record| record.id == visible_record_id)
            .count(),
        1
    );
}

/// Verifies an edited durable user input reuses its projected canonical identity without appending a duplicate message.
/// 验证编辑后的持久用户输入会复用其投影得到的规范身份，且不会追加重复消息。
#[test]
fn edited_user_prompt_reuses_existing_canonical_message_id() {
    // runtime_state represents the rebuilt controller state for an existing V1 conversation.
    // runtime_state 表示从既有 V1 会话重建得到的控制器状态。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_text_message(
        &mut runtime_state.conversation_state,
        "v1-provider-message-1",
        "编辑后的第一条消息",
        Vec::new(),
    )
    .expect("rebuilt V1 user message should be valid");
    // original_message_count proves prompt preparation never duplicates the durable edited row.
    // original_message_count 用于证明提示词准备不会重复追加这条持久编辑记录。
    let original_message_count = runtime_state.conversation_state.messages.len();
    let mut hook_runtime = ConversationHookRuntime::new();

    // start receives the exact identity resolved from the authoritative V1 source projection.
    // start 接收从权威 V1 来源投影解析出的精确身份。
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(903),
            session_title: "既有会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("edited-root-entry"),
            continuation_action_id: None,
            prompt: "编辑后的第一条消息",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: Some(String::from("v1-provider-message-1")),
            append_canonical_user_message: false,
            append_visible_user_prompt_record: false,
            source: PendingSessionPromptSource::UserFollowup,
            vmm_runtime_ready: true,
            vmm_precheck_ready: true,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("edited durable user input should begin with its existing canonical identity");

    assert_eq!(
        start.canonical_user_message_id.as_deref(),
        Some("v1-provider-message-1")
    );
    assert_eq!(
        runtime_state.conversation_state.messages.len(),
        original_message_count
    );
    assert!(start.visible_user_prompt_record_id.is_none());
}

/// Verifies a newly admitted V1 root keeps its durable message identity while gaining one visible row.
/// 验证新接纳的 V1 根输入在新增一条可见记录时仍保留其可靠消息身份。
#[test]
fn durable_v1_root_adds_visible_record_without_second_canonical_message() {
    // runtime_state starts with the exact message reconstructed from the V1 Provider projection.
    // runtime_state 从 V1 Provider 投影重建出的精确消息开始。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_text_message(
        &mut runtime_state.conversation_state,
        "v1-provider-message-1",
        "首次输入",
        Vec::new(),
    )
    .expect("durable root message should be valid");
    let mut hook_runtime = ConversationHookRuntime::new();

    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(904),
            session_title: "新会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("durable-root-entry"),
            continuation_action_id: None,
            prompt: "首次输入",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: Some(String::from("v1-provider-message-1")),
            append_canonical_user_message: false,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::UserFollowup,
            vmm_runtime_ready: true,
            vmm_precheck_ready: true,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("durable root should create a visible record without a second message");

    assert_eq!(
        start.canonical_user_message_id.as_deref(),
        Some("v1-provider-message-1")
    );
    assert_eq!(runtime_state.conversation_state.messages.len(), 1);
    assert_eq!(runtime_state.conversation_state.records.len(), 1);
    assert_eq!(
        start.visible_user_prompt_record_id.as_deref(),
        Some(runtime_state.conversation_state.records[0].id.as_str())
    );
    assert_eq!(
        start.boundary_events[1].payload["visible_user_prompt_record_id"],
        runtime_state.conversation_state.records[0].id
    );
}

/// Verifies typed VMM recall anchors target the exact visible canonical user message.
/// 验证类型化 VMM 召回锚点会指向精确的可见规范用户消息。
#[test]
fn typed_vmm_recall_targets_visible_canonical_user_message() {
    // runtime_state owns the canonical conversation and independently persisted VMM session state.
    // runtime_state 拥有规范会话和独立持久化的 VMM 会话状态。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    // start appends the visible user prompt before the synthetic precheck result is applied.
    // start 会在应用模拟 precheck 结果前追加可见用户提示词。
    let start = begin_user_prompt(&mut runtime_state);
    // target_message_id is the authoritative identity transported from prompt start into VMM state.
    // target_message_id 是从提示词启动阶段传递到 VMM 状态的权威身份。
    let target_message_id = start
        .canonical_user_message_id
        .clone()
        .expect("visible user prompt must create one canonical identity");
    // precheck_outcome starts from the correctly correlated skipped report before recall metadata is attached.
    // precheck_outcome 从正确关联的跳过报告开始，再附加召回元数据。
    let mut precheck_outcome = start.precheck_request.clone().skip();
    // profile_outcome remains independent and empty because this test targets recall ownership only.
    // profile_outcome 保持独立且为空，因为该测试只验证召回归属。
    let profile_outcome = start.profile_request.clone().skip();
    // Freeze the same native owner in the response and the host's independent publication input.
    // 在响应与宿主独立发布输入中冻结同一原生归属。
    let read_source = vulcan_vmm::VmmMemoryReadSource::new(
        vulcan_vmm::VmmMemorySource::native(1).expect("owner"),
        vec![1],
    )
    .expect("read source");
    // report is the single successful hook report consumed by the shared prompt continuation.
    // report 是共享提示词续接逻辑消费的唯一成功 Hook 报告。
    let report = precheck_outcome
        .report
        .as_mut()
        .expect("skipped precheck should produce an empty successful report");
    report.metadata.insert(
        HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_string(),
        vec![serde_json::json!(VmmRecallBatchHookEffect {
            read_source: read_source.clone(),
            batch: VmmRecallBatch {
                ttl_turns: 2,
                injection_tag: String::from("VMM_MEMORY"),
                injection_description: String::from("Relevant memories."),
                items: vec![ContextItem {
                    text: String::from("stable recalled memory"),
                    memory_id: 17,
                    ..ContextItem::default()
                }],
            },
        })],
    );
    // vmm_session_memory models the Core controller candidate mutated by this continuation.
    // vmm_session_memory 模拟由本次续接修改的 Core 控制器候选状态。
    let mut vmm_session_memory = test_vmm_session_memory(start.session_id);
    // continuation applies typed recall state and builds the provider-facing exact-message anchors.
    // continuation 会应用类型化召回状态并构建 Provider 可见的精确消息锚点。
    let continuation = continue_shared_prompt_turn_after_preparation_with_runtime_state(
        &mut runtime_state,
        &mut vmm_session_memory,
        SharedPromptTurnContinueInput {
            vmm_context_epoch: Some(
                vulcan_vmm::VmmContextEpoch::new(1, None).expect("fixture context epoch"),
            ),
            vmm_profile_read_source: Some(read_source),
            start,
            precheck_outcome,
            profile_outcome,
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: true,
        },
    )
    .expect("typed VMM recall should prepare a provider request");
    // context_injection carries the immutable VMM anchor emitted for this provider turn.
    // context_injection 携带本次 Provider 回合生成的不可变 VMM 锚点。
    let context_injection = continuation
        .request
        .context_injection
        .as_ref()
        .expect("typed recall should create provider context injection");
    // target_anchors isolates the one anchor that must own the recalled memory batch.
    // target_anchors 筛选出必须承载召回记忆批次的唯一目标锚点。
    let target_anchors = context_injection
        .anchored_prefixes
        .iter()
        .filter(|anchor| anchor.target_message_id == target_message_id)
        .collect::<Vec<_>>();
    // canonical_target_count proves Provider exact lookup will resolve one and only one user message.
    // canonical_target_count 用于证明 Provider 精确查询只会解析到一条用户消息。
    let canonical_target_count = continuation
        .request
        .conversation_state
        .messages
        .iter()
        .filter(|message| {
            message.id == target_message_id
                && matches!(message.payload, SessionConversationPayload::UserText { .. })
        })
        .count();

    assert_eq!(target_anchors.len(), 1);
    assert_eq!(canonical_target_count, 1);
}

/// Verifies a stopped VMM service cannot enter local state, recall, profile, or anchor paths.
/// 验证已停止的 VMM 服务不能进入本地状态、召回、画像或锚点链路。
#[test]
fn stopped_vmm_service_bypasses_every_memory_path_without_canonical_user_id() {
    let session_id = SessionId::from_u128(903);
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut hook_runtime = ConversationHookRuntime::new();
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id,
            session_title: "目标续轮",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("stopped-vmm-goal"),
            continuation_action_id: None,
            prompt: "继续目标",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: false,
            append_visible_user_prompt_record: false,
            source: PendingSessionPromptSource::GoalContinuation,
            vmm_runtime_ready: false,
            vmm_precheck_ready: false,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("a normal prompt turn must start while VMM is stopped");
    assert!(start.canonical_user_message_id.is_none());

    let mut vmm_session_memory = test_vmm_session_memory(session_id);
    vmm_session_memory
        .apply_recall_batch(
            &session_id.to_string(),
            "historical-user",
            &VmmRecallBatch {
                ttl_turns: 2,
                injection_tag: String::from("VMM_MEMORY"),
                injection_description: String::from("Historical memory."),
                items: vec![ContextItem {
                    text: String::from("must not be recalled while stopped"),
                    memory_id: 99,
                    ..ContextItem::default()
                }],
            },
            "1",
        )
        .expect("historical VMM fixture should be valid");
    let original_vmm_state = vmm_session_memory.clone();
    let continuation = continue_shared_prompt_turn_after_preparation_with_runtime_state(
        &mut runtime_state,
        &mut vmm_session_memory,
        SharedPromptTurnContinueInput {
            vmm_context_epoch: Some(
                vulcan_vmm::VmmContextEpoch::new(1, None).expect("fixture context epoch"),
            ),
            vmm_profile_read_source: None,
            precheck_outcome: start.precheck_request.clone().skip(),
            profile_outcome: start.profile_request.clone().skip(),
            start,
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: false,
        },
    )
    .expect("a stopped VMM service must not block the ordinary Provider request");

    assert!(!continuation.request.vmm_runtime_ready);
    assert!(continuation.request.context_injection.is_none());
    assert!(continuation.request.system_prompt_hook_blocks.is_empty());
    assert!(vmm_session_memory.active_leases.is_empty());
    assert!(vmm_session_memory.cache_epoch.anchors.is_empty());
    assert!(vmm_session_memory.pending_prompt_events.is_empty());
    assert!(vmm_session_memory.revision > original_vmm_state.revision);
    assert_eq!(
        vmm_session_memory.committed_command_hashes,
        original_vmm_state.committed_command_hashes
    );
    assert_eq!(
        vmm_session_memory.completed_turn_sequence,
        original_vmm_state.completed_turn_sequence
    );
}

/// Begins one user-authored prompt turn used by session boundary tests.
/// 启动一条供 会话边界测试使用的用户提示词回合。
fn begin_user_prompt(runtime_state: &mut SessionRuntimeStateEntry) -> SharedPromptTurnStart {
    let mut hook_runtime = ConversationHookRuntime::new();
    begin_shared_prompt_turn_with_runtime_state(
        runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(902),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("user-entry"),
            continuation_action_id: None,
            prompt: "生成一张图片",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::UserFollowup,
            vmm_runtime_ready: true,
            vmm_precheck_ready: true,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("user prompt should start")
}

/// Verifies an interrupt that missed the active boundary becomes a correlated user turn at idle dispatch.
/// 验证错过活动边界的插队消息在空闲分发时会变为带关联标识的用户回合。
#[test]
fn idle_dispatched_interrupt_produces_conversation_hook_turn_id() {
    let session_id = SessionId::from_u128(918);
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut hook_runtime = ConversationHookRuntime::new();
    let source = PendingSessionPromptSource::UserInterrupt.for_idle_dispatch();
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id,
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("late-interrupt-entry"),
            continuation_action_id: None,
            prompt: "继续检查",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source,
            vmm_runtime_ready: true,
            vmm_precheck_ready: true,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("idle-dispatched interrupt should begin as a user follow-up");
    let expected_turn_id = start.preparation_turn_id.clone();
    let mut vmm_session_memory = test_vmm_session_memory(session_id);
    let continuation = continue_shared_prompt_turn_after_preparation_with_runtime_state(
        &mut runtime_state,
        &mut vmm_session_memory,
        SharedPromptTurnContinueInput {
            vmm_context_epoch: Some(
                vulcan_vmm::VmmContextEpoch::new(1, None).expect("fixture context epoch"),
            ),
            vmm_profile_read_source: None,
            precheck_outcome: start.precheck_request.clone().skip(),
            profile_outcome: start.profile_request.clone().skip(),
            start,
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: true,
        },
    )
    .expect("idle-dispatched interrupt should build a provider request");

    assert_eq!(source, PendingSessionPromptSource::UserFollowup);
    assert_eq!(
        continuation.request.conversation_hook_turn_id.as_deref(),
        Some(expected_turn_id.as_str())
    );
}

/// Verifies user prompt begins expose visible session user-input boundary events.
/// 验证用户提示词开始时会暴露可见的 会话用户输入边界事件。
#[test]
fn user_prompt_begin_exports_visible_boundary_events() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let start = begin_user_prompt(&mut runtime_state);

    assert_eq!(start.boundary_events.len(), 2);
    assert_eq!(
        start.boundary_events[0].kind,
        SessionBoundaryEventKind::TurnStarted
    );
    assert_eq!(
        start.boundary_events[0].visibility,
        SessionEventVisibility::Hidden
    );
    assert_eq!(
        start.boundary_events[1].kind,
        SessionBoundaryEventKind::UserInput
    );
    assert_eq!(
        start.boundary_events[1].visibility,
        SessionEventVisibility::Visible
    );
    assert_eq!(
        start.boundary_events[1].payload["source"],
        serde_json::json!("user_followup")
    );
    assert_eq!(
        start.boundary_events[1].payload["visible_user_prompt_record_id"],
        serde_json::json!(start.visible_user_prompt_record_id)
    );
}

/// Verifies session-mailbox turns cannot accidentally expose visible user boundary or transcript records.
/// 验证会话 mailbox 回合不会意外暴露可见用户边界事件或转录记录。
#[test]
fn session_mailbox_begin_keeps_user_input_hidden_even_if_visibility_flag_is_true() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut hook_runtime = ConversationHookRuntime::new();
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(903),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("mailbox-entry"),
            continuation_action_id: None,
            prompt: "<subagent_mailbox_event>\n{}\n</subagent_mailbox_event>",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::SessionMailbox,
            vmm_runtime_ready: true,
            vmm_precheck_ready: true,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("mailbox prompt should start");

    assert!(runtime_state.conversation_state.messages.is_empty());
    assert!(runtime_state.conversation_state.records.is_empty());
    assert_eq!(
        start.boundary_events[1].visibility,
        SessionEventVisibility::Hidden
    );
    assert_eq!(
        start.boundary_events[1].payload["source"],
        serde_json::json!("session_mailbox")
    );
    assert_eq!(
        start.boundary_events[1].payload["canonical"],
        serde_json::json!(false)
    );
}

/// Verifies goal continuation turns never persist hidden prompts as reopen-visible user text.
/// 验证目标续跑回合绝不会把隐藏提示持久化成重开后可见的用户文本。
#[test]
fn goal_continuation_begin_never_persists_user_message() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut hook_runtime = ConversationHookRuntime::new();
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(904),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("goal-entry"),
            continuation_action_id: None,
            prompt: "Continue working toward the active thread goal.",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::GoalContinuation,
            vmm_runtime_ready: true,
            vmm_precheck_ready: true,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("goal continuation prompt should start");

    assert!(runtime_state.conversation_state.messages.is_empty());
    assert!(runtime_state.conversation_state.records.is_empty());
    assert_eq!(
        start.boundary_events[1].visibility,
        SessionEventVisibility::Hidden
    );
    assert_eq!(
        start.boundary_events[1].payload["source"],
        serde_json::json!("goal_continuation")
    );
    assert_eq!(
        start.boundary_events[1].payload["canonical"],
        serde_json::json!(false)
    );
}

/// Verifies manual Continue correlates the restored process without claiming user-authored completion semantics.
/// 验证手动 Continue 会关联恢复过程，但不会领取用户编写回合的完成语义。
#[test]
fn manual_continuation_preparation_preserves_exact_hook_correlation() {
    // runtime_state owns the retained provider-visible state resumed by Continue.
    // runtime_state 持有 Continue 要恢复的 Provider 可见状态。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    // hook_runtime creates the shared preparation identity without a user-authored message.
    // hook_runtime 在没有用户编写消息的情况下创建共享准备身份。
    let mut hook_runtime = ConversationHookRuntime::new();
    // start models the current prompt-free Continue path before it migrates to retry snapshots.
    // start 模拟迁移到重试快照前的当前无提示词 Continue 路径。
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(908),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("continuation-entry"),
            continuation_action_id: Some(
                ClientActionId::try_new("manual-continuation-action")
                    .expect("valid continuation action"),
            ),
            prompt: "",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: false,
            append_visible_user_prompt_record: false,
            source: PendingSessionPromptSource::ExecutionRetry,
            vmm_runtime_ready: true,
            vmm_precheck_ready: true,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("manual continuation should prepare retained state");
    // expected_hook_turn_id is the new process Hook owner created after Stop closed the former turn.
    // expected_hook_turn_id 是 Stop 关闭原回合后新建的过程 Hook 所有者。
    let expected_hook_turn_id = start.preparation_turn_id.clone();
    // vmm_session_memory remains unchanged because Continue owns no new user boundary.
    // vmm_session_memory 保持不变，因为 Continue 不拥有新的用户边界。
    let mut vmm_session_memory = test_vmm_session_memory(start.session_id);
    // continuation builds the provider request from the retained canonical state.
    // continuation 从保留的规范状态构建 Provider 请求。
    let continuation = continue_shared_prompt_turn_after_preparation_with_runtime_state(
        &mut runtime_state,
        &mut vmm_session_memory,
        SharedPromptTurnContinueInput {
            vmm_context_epoch: Some(
                vulcan_vmm::VmmContextEpoch::new(1, None).expect("fixture context epoch"),
            ),
            vmm_profile_read_source: None,
            precheck_outcome: start.precheck_request.clone().skip(),
            profile_outcome: start.profile_request.clone().skip(),
            start,
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: false,
        },
    )
    .expect("manual continuation should build a provider request");

    assert_eq!(
        continuation.request.prompt_purpose,
        ExecutionPromptPurpose::ExecutionRetry
    );
    assert_eq!(
        continuation.request.conversation_hook_turn_id.as_deref(),
        Some(expected_hook_turn_id.as_str())
    );
    assert_eq!(
        continuation.request.turn_start_mode,
        Some(ExecutionTurnStartMode::RestoredCheckpoint)
    );
    assert!(
        !continuation
            .request
            .prompt_purpose
            .runs_completed_turn_hooks()
    );
}

/// Verifies a skipped Goal-continuation precheck still reuses the persisted VMM profile baseline.
/// 验证目标续轮跳过 precheck 时仍会复用持久化的 VMM 画像基线。
#[test]
fn goal_continuation_reuses_persisted_vmm_profile_baseline() {
    // runtime_state owns the baseline created by the first successful VMM profile synchronization.
    // runtime_state 持有首次 VMM 画像同步成功后创建的基线。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.vmm_profile_sync.base_profile_content =
        Some(String::from("[USER]\n[P1][L2][W0] stable profile"));
    runtime_state.vmm_profile_sync.read_source = Some(test_profile_read_source());
    // hook_runtime prepares the Goal continuation identity without running remote precheck handlers.
    // hook_runtime 在不运行远端 precheck 处理器的情况下准备目标续轮身份。
    let mut hook_runtime = ConversationHookRuntime::new();
    // start models the exact hidden Goal-continuation path used by automatic turn chaining.
    // start 模拟自动回合续接使用的精确隐藏目标续轮路径。
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(906),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("goal-baseline-entry"),
            continuation_action_id: None,
            prompt: "Continue working toward the active thread goal.",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: false,
            append_visible_user_prompt_record: false,
            source: PendingSessionPromptSource::GoalContinuation,
            vmm_runtime_ready: true,
            vmm_precheck_ready: true,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("goal continuation prompt should start");
    // expected_hook_turn_id binds hidden Goal work to the exact active process Hook turn.
    // expected_hook_turn_id 将隐藏目标工作绑定到精确的活动过程 Hook 回合。
    let expected_hook_turn_id = start.preparation_turn_id.clone();
    // precheck_outcome is intentionally empty because Goal continuation skips every user precheck hook.
    // precheck_outcome 刻意为空，因为目标续轮会跳过全部用户 precheck Hook。
    let precheck_outcome = start.precheck_request.clone().skip();
    // profile_outcome models an independently completed profile line with no remote refresh.
    // profile_outcome 模拟不执行远端刷新的独立画像线路完成结果。
    let profile_outcome = start.profile_request.clone().skip();
    // vmm_session_memory models the independent Core controller state for this continuation.
    // vmm_session_memory 模拟本次续接使用的独立 Core 控制器状态。
    let mut vmm_session_memory = test_vmm_session_memory(start.session_id);
    // continuation must reconstruct the stable system block from persisted runtime state.
    // continuation 必须从持久化运行态重新构建稳定系统块。
    let continuation = continue_shared_prompt_turn_after_preparation_with_runtime_state(
        &mut runtime_state,
        &mut vmm_session_memory,
        SharedPromptTurnContinueInput {
            vmm_context_epoch: Some(
                vulcan_vmm::VmmContextEpoch::new(1, None).expect("fixture context epoch"),
            ),
            vmm_profile_read_source: Some(test_profile_read_source()),
            start,
            precheck_outcome,
            profile_outcome,
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: false,
        },
    )
    .expect("goal continuation should prepare a provider request");
    // blocks is the exact provider-visible system-hook collection for the continued request.
    // blocks 是续轮请求中 Provider 精确可见的系统 Hook 集合。
    let blocks = &continuation.request.system_prompt_hook_blocks;

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].tag, "vmm_profile");
    assert_eq!(
        blocks[0].phase,
        vulcan_hooks::SystemPromptHookPhase::FirstSession
    );
    assert_eq!(blocks[0].source.as_deref(), Some("VMM profile full"));
    assert_eq!(blocks[0].content, "[USER]\n[P1][L2][W0] stable profile");
    assert_eq!(
        continuation.request.conversation_hook_turn_id.as_deref(),
        Some(expected_hook_turn_id.as_str())
    );
}

/// Failure and delayed success never publish a former source, while same-source failure preserves its baseline.
/// 失败和迟到成功绝不发布此前来源，同来源失败则保留其基线。
#[test]
fn profile_publication_rechecks_sources_for_failure_and_delayed_success() {
    // Exercise the actual shared continuation output, not only a cache comparison helper.
    // 验证实际共享继续执行的输出，而不只是缓存比较辅助函数。
    for (same_source, failed) in [(false, true), (false, false), (true, true)] {
        let old_source = test_profile_read_source();
        let expected = if same_source {
            old_source.clone()
        } else {
            vulcan_vmm::VmmMemoryReadSource::new(old_source.owner.clone(), vec![7, 8])
                .expect("linked scope changed")
        };
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.vmm_profile_sync = VmmProfileSyncState {
            read_source: Some(old_source),
            base_profile_content: Some("old-profile-secret".to_owned()),
            ..Default::default()
        };
        let old_profile = runtime_state.vmm_profile_sync.clone();
        let start = begin_user_prompt(&mut runtime_state);
        let precheck_outcome = start.precheck_request.clone().skip();
        let mut profile_outcome = start.profile_request.clone().skip();
        if failed {
            profile_outcome.report = Err("simulated profile read failure".to_owned());
        } else {
            profile_outcome
                .report
                .as_mut()
                .expect("successful delayed report")
                .metadata
                .insert(
                    HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT.to_owned(),
                    vec![serde_json::json!(VmmProfileSyncHookEffect {
                        state: old_profile
                    })],
                );
        }
        let mut controller = test_vmm_session_memory(start.session_id);
        let continuation = continue_shared_prompt_turn_after_preparation_with_runtime_state(
            &mut runtime_state,
            &mut controller,
            SharedPromptTurnContinueInput {
                vmm_context_epoch: Some(
                    vulcan_vmm::VmmContextEpoch::new(1, None).expect("fixture context epoch"),
                ),
                start,
                precheck_outcome,
                profile_outcome,
                vmm_profile_read_source: Some(expected.clone()),
                project_configuration: Default::default(),
                runtime_reasoning_effort: None,
                provider_reasoning_effort: None,
                patch_visible_user_prompt_record: true,
            },
        )
        .expect("ordinary prompt continues");
        assert!(continuation.profile_failure_detail.is_some());
        assert_eq!(runtime_state.vmm_profile_sync.read_source, Some(expected));
        assert_eq!(
            continuation
                .request
                .system_prompt_hook_blocks
                .iter()
                .any(|block| block.content.contains("old-profile-secret")),
            same_source,
        );
        assert_eq!(
            runtime_state
                .vmm_profile_sync
                .base_profile_content
                .is_some(),
            same_source
        );
    }
}

/// Verifies precheck output cannot mutate or replace the system-profile line.
/// 验证 precheck 输出无法修改或替换系统画像线路。
#[test]
fn precheck_cannot_own_vmm_profile_state_or_system_block() {
    // runtime_state owns the authoritative baseline before either preparation line completes.
    // runtime_state 在两条准备线路完成前持有权威画像基线。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.vmm_profile_sync.base_profile_content =
        Some(String::from("[USER]\nauthoritative profile"));
    runtime_state.vmm_profile_sync.read_source = Some(test_profile_read_source());
    // start prepares sibling profile and precheck requests for one correlated prompt turn.
    // start 为一个关联提示词回合准备同级画像与 precheck 请求。
    let start = begin_user_prompt(&mut runtime_state);
    // profile_outcome carries the sole legitimate profile-state update for this regression scenario.
    // profile_outcome 在该回归场景中携带唯一合法的画像状态更新。
    let mut profile_outcome = start.profile_request.clone().skip();
    let profile_report = profile_outcome
        .report
        .as_mut()
        .expect("skipped profile request should produce an empty successful report");
    profile_report.metadata.insert(
        HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT.to_string(),
        vec![serde_json::json!(VmmProfileSyncHookEffect {
            state: VmmProfileSyncState {
                read_source: Some(test_profile_read_source()),
                base_profile_content: Some(String::from("[USER]\nprofile-line update")),
                ..VmmProfileSyncState::default()
            }
        })],
    );
    // precheck_outcome deliberately carries forbidden profile metadata and a duplicate reserved block.
    // precheck_outcome 刻意携带禁止的画像元数据及重复保留块。
    let mut precheck_outcome = start.precheck_request.clone().skip();
    let precheck_report = precheck_outcome
        .report
        .as_mut()
        .expect("skipped precheck should produce an empty successful report");
    precheck_report.metadata.insert(
        HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT.to_string(),
        vec![serde_json::json!(VmmProfileSyncHookEffect {
            state: VmmProfileSyncState {
                base_profile_content: Some(String::from("[USER]\nprecheck overwrite")),
                ..VmmProfileSyncState::default()
            }
        })],
    );
    precheck_report
        .system_prompt_blocks
        .push(vulcan_hooks::SystemPromptHookBlock {
            tag: String::from("vmm_profile"),
            description: String::from("foreign precheck block"),
            content: String::from("[USER]\nprecheck block"),
            source: Some(String::from("precheck")),
            phase: vulcan_hooks::SystemPromptHookPhase::SubsequentSession,
            baseline_provenance: None,
        });

    // vmm_session_memory models the independent Core controller state for this continuation.
    // vmm_session_memory 模拟本次续接使用的独立 Core 控制器状态。
    let mut vmm_session_memory = test_vmm_session_memory(start.session_id);
    let continuation = continue_shared_prompt_turn_after_preparation_with_runtime_state(
        &mut runtime_state,
        &mut vmm_session_memory,
        SharedPromptTurnContinueInput {
            vmm_context_epoch: Some(
                vulcan_vmm::VmmContextEpoch::new(1, None).expect("fixture context epoch"),
            ),
            vmm_profile_read_source: Some(test_profile_read_source()),
            start,
            precheck_outcome,
            profile_outcome,
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: true,
        },
    )
    .expect("independent preparation lines should produce a provider request");
    let profile_blocks = continuation
        .request
        .system_prompt_hook_blocks
        .iter()
        .filter(|block| block.tag == "vmm_profile")
        .collect::<Vec<_>>();

    assert_eq!(
        runtime_state
            .vmm_profile_sync
            .base_profile_content
            .as_deref(),
        Some("[USER]\nprofile-line update")
    );
    assert_eq!(profile_blocks.len(), 1);
    assert_eq!(profile_blocks[0].content, "[USER]\nprofile-line update");
    assert_eq!(
        profile_blocks[0].source.as_deref(),
        Some("VMM profile full")
    );
}

/// Verifies hidden model guidance reaches the provider request without polluting transcript records.
/// 验证隐藏模型引导会进入 Provider 请求，但不会污染转录记录。
#[test]
fn hidden_model_guidance_stays_out_of_visible_transcript() {
    let mut runtime_state = SessionRuntimeStateEntry {
        active_temperature: Some(0.7),
        ..SessionRuntimeStateEntry::default()
    };
    let mut hook_runtime = ConversationHookRuntime::new();
    let start = begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(905),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("plan-execute-entry"),
            continuation_action_id: None,
            prompt: "执行计划",
            hidden_model_guidance: Some("Plan file: D:/workspace/docs/plan/demo.md"),
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::UserFollowup,
            vmm_runtime_ready: true,
            vmm_precheck_ready: true,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("plan execution prompt should start");
    let precheck_outcome = start.precheck_request.clone().skip();
    // profile_outcome is separately correlated even when no profile handler contributes state.
    // profile_outcome 即使没有画像处理器贡献状态也保持独立关联。
    let profile_outcome = start.profile_request.clone().skip();
    // expected_hook_turn_id is the shared preparation identity that must survive request construction.
    // expected_hook_turn_id 是必须在请求构建后保留的共享准备身份。
    let expected_hook_turn_id = start.preparation_turn_id.clone();
    // vmm_session_memory models the independent Core controller state for this continuation.
    // vmm_session_memory 模拟本次续接使用的独立 Core 控制器状态。
    let mut vmm_session_memory = test_vmm_session_memory(start.session_id);
    let continuation = continue_shared_prompt_turn_after_preparation_with_runtime_state(
        &mut runtime_state,
        &mut vmm_session_memory,
        SharedPromptTurnContinueInput {
            vmm_context_epoch: Some(
                vulcan_vmm::VmmContextEpoch::new(1, None).expect("fixture context epoch"),
            ),
            vmm_profile_read_source: None,
            start,
            precheck_outcome,
            profile_outcome,
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: true,
        },
    )
    .expect("prompt should continue");
    let transcript_text = runtime_state
        .conversation_state
        .records
        .iter()
        .map(|record| record.body.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(transcript_text, "执行计划");
    assert_eq!(
        continuation.request.conversation_hook_turn_id.as_deref(),
        Some(expected_hook_turn_id.as_str())
    );
    assert!(continuation.request.prompt.contains("执行计划"));
    assert!(continuation.request.temperature.is_none());
    assert_eq!(
        continuation.request.reasoning_effort,
        vulcan_provider_config::config::ReasoningEffortLevel::Off
    );
    assert!(
        continuation
            .request
            .prompt
            .contains("<hidden_host_guidance>\nPlan file: D:/workspace/docs/plan/demo.md")
    );
}

/// Verifies remote PreCheck failure leaves expired VMM state resident without adding a prompt anchor.
/// 验证远端 PreCheck 失败时，过期 VMM 状态保持驻留且不会新增提示词锚点。
#[test]
fn precheck_failure_preserves_expired_vmm_resident_and_registers_terminal_turn() {
    // session_id matches the fixed owner used by begin_user_prompt.
    // session_id 与 begin_user_prompt 使用的固定所有者一致。
    let session_id = SessionId::from_u128(902);
    // runtime_state starts with one canonical message and one lease that expires before the tested turn.
    // runtime_state 初始包含一条规范消息，以及一条在被测回合前到期的租约。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_text_message(
        &mut runtime_state.conversation_state,
        "old-user",
        "old question",
        Vec::new(),
    )
    .expect("old canonical user message should append");
    // vmm_session_memory models the Core controller state that already owns one expired lease.
    // vmm_session_memory 模拟已经持有一条过期租约的 Core 控制器状态。
    let mut vmm_session_memory = test_vmm_session_memory(session_id);
    // The failure fallback is valid only for a cache whose source still matches the current host binding.
    // 失败复用仅对来源仍匹配当前宿主绑定的缓存有效。
    let read_source = test_profile_read_source();
    vmm_session_memory
        .select_read_source(&session_id.to_string(), Some(&read_source), "1")
        .expect("select cache source");
    // The expired resident and the next request share one proven, uncompacted host lineage.
    // 过期驻留项与下一请求共享同一已证明的未压缩宿主沿革。
    vmm_session_memory
        .synchronize_context_epoch(
            &vulcan_vmm::VmmContextEpoch::new(1, None).expect("resident context"),
            "1",
        )
        .expect("seed resident lineage");
    vmm_session_memory
        .apply_recall_batch(
            &session_id.to_string(),
            "old-user",
            &VmmRecallBatch {
                ttl_turns: 1,
                injection_tag: String::from("VMM_MEMORY"),
                injection_description: String::from("Relevant memories."),
                items: vec![ContextItem {
                    text: String::from("expiring memory"),
                    memory_id: 7,
                    ..ContextItem::default()
                }],
            },
            "1",
        )
        .expect("initial VMM recall should succeed");
    vmm_session_memory
        .register_active_turn("old-turn", "1")
        .expect("old logical turn should register");
    vmm_session_memory
        .consume_turn("old-turn", "2")
        .expect("old logical turn should expire the lease");
    // start creates the exact next canonical user message before the failing hook outcome is applied.
    // start 会在应用失败 Hook 结果前创建精确的下一条规范用户消息。
    let start = begin_user_prompt(&mut runtime_state);
    // target_message_id is the authoritative new anchor target under test.
    // target_message_id 是被测的新锚点权威目标。
    let target_message_id = start
        .canonical_user_message_id
        .clone()
        .expect("user followup must create a canonical message");
    // precheck_outcome preserves correlation while replacing the remote report with a failure.
    // precheck_outcome 保留关联标识，同时将远端报告替换为失败结果。
    let mut precheck_outcome = start.precheck_request.clone().skip();
    precheck_outcome.report = Err(String::from("simulated remote precheck failure"));
    // profile_outcome proves a precheck failure does not alter the independent profile line.
    // profile_outcome 证明 precheck 失败不会改变独立画像线路。
    let profile_outcome = start.profile_request.clone().skip();
    // expected_turn_id is the shared preparation boundary registered before Provider execution.
    // expected_turn_id 是 Provider 执行前登记的共享准备边界。
    let expected_turn_id = start.preparation_turn_id.clone();

    let continuation = continue_shared_prompt_turn_after_preparation_with_runtime_state(
        &mut runtime_state,
        &mut vmm_session_memory,
        SharedPromptTurnContinueInput {
            vmm_context_epoch: Some(
                vulcan_vmm::VmmContextEpoch::new(1, None).expect("fixture context epoch"),
            ),
            vmm_profile_read_source: Some(read_source),
            start,
            precheck_outcome,
            profile_outcome,
            project_configuration: ProjectVulcanCodeConfigurationFile::default(),
            runtime_reasoning_effort: None,
            provider_reasoning_effort: None,
            patch_visible_user_prompt_record: true,
        },
    )
    .expect("local VMM state should continue despite remote precheck failure");

    assert!(continuation.precheck_failure_detail.is_some());
    assert!(vmm_session_memory.pending_prompt_events.is_empty());
    assert!(vmm_session_memory.active_panel_card().is_none());
    assert!(vmm_session_memory.expired_panel_card().is_some());
    assert_eq!(
        vmm_session_memory.active_turn_id.as_deref(),
        Some(expected_turn_id.as_str())
    );
    assert_eq!(
        vmm_session_memory
            .cache_epoch
            .anchors
            .last()
            .map(|anchor| (&anchor.kind, anchor.target_message_id.as_str())),
        Some((&VmmPromptAnchorKind::Memory, "old-user"))
    );
    assert_ne!(
        vmm_session_memory
            .cache_epoch
            .anchors
            .last()
            .map(|anchor| anchor.target_message_id.as_str()),
        Some(target_message_id.as_str())
    );
}

/// Verifies a loaded dialogue message window is not overwritten by display-only fallback rows.
/// 验证已加载的对话层消息窗口不会被仅显示层的回退行覆盖。
#[test]
fn begin_prompt_keeps_runtime_messages_when_records_are_empty() {
    // runtime_state simulates a restarted session whose API context restored messages but not display records.
    // runtime_state 模拟重启后恢复了 API 消息上下文但没有显示记录的会话。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_text_message(
        &mut runtime_state.conversation_state,
        "dialogue-history",
        "对话层历史",
        Vec::new(),
    )
    .expect("valid user prompt identity");
    let mut hook_runtime = ConversationHookRuntime::new();

    begin_shared_prompt_turn_with_runtime_state(
        &mut runtime_state,
        &mut hook_runtime,
        SharedPromptTurnRuntimeBeginInput {
            session_id: SessionId::from_u128(906),
            session_title: "主会话",
            workspace_path: "D:/workspace",
            prompt_entry_id: String::from("followup-entry"),
            continuation_action_id: None,
            prompt: "继续",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: true,
            append_visible_user_prompt_record: true,
            source: PendingSessionPromptSource::UserFollowup,
            vmm_runtime_ready: true,
            vmm_precheck_ready: true,
            execution_mode: String::from("Build"),
            reasoning_effort: None,
            owning_project_id: Some(String::from("project-a")),
        },
    )
    .expect("prompt should start");

    let bodies = runtime_state
        .conversation_state
        .messages
        .iter()
        .map(|message| match &message.payload {
            SessionConversationPayload::UserText { text, .. } => text.as_str(),
            _ => "",
        })
        .collect::<Vec<_>>();
    assert_eq!(bodies, vec!["对话层历史", "继续"]);
}
