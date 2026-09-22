//! Persistence and generation ownership tests for captured child execution profiles.
//! 已捕获子执行配置的持久化及代际归属测试。

use super::*;
use crate::v1::child_execution::{
    ChildExecutionPreparation, ChildExecutionPromptContext, ChildToolOrigin, ChildToolSourceKind,
};

#[path = "tests/creation.rs"]
mod creation;

#[path = "tests/maintenance.rs"]
mod maintenance;

/// Prepared settings survive storage before a turn exists and bind only to allocated coordinates.
/// 已准备设置在回合创建前可持久保存，并仅绑定实际分配的坐标。
/// Returns nothing; checks strict decoding, unchanged settings and canonical publication/reload.
/// 不返回值；检查严格解码、设置原样保留以及规范发布与重载。
#[test]
fn child_preparation_round_trip_binds_real_document_turn() {
    // Reuse populated settings while allocating independent physical identities for this creation.
    // 复用完整设置，同时为本次创建分配独立物理身份。
    let (_, mut expected) = fixture();
    let task_id = uuid::Uuid::new_v4();
    let root_session_id = uuid::Uuid::new_v4();
    let parent_session_id = uuid::Uuid::new_v4();
    let preparation = ChildExecutionPreparation {
        task_id,
        root_session_id,
        parent_session_id,
        execution_id: expected.business_execution_id.clone(),
        runtime_identity: expected.runtime_identity.clone(),
        invocation_config: expected.invocation_config.clone(),
        resolved_config: expected.resolved_config.clone(),
        effective_config: expected.effective_config.clone(),
        runtime_policy: expected.runtime_policy,
        execution_agent_id: expected.execution_agent_id.clone(),
        connection_id: expected.connection_id.clone(),
        temperature: expected.temperature,
        admitted_tools: expected.admitted_tools.clone(),
        request_overrides: expected.request_overrides.clone(),
        fixed_agent_prompt: expected.fixed_agent_prompt.clone(),
        prompt_context: expected.prompt_context.clone(),
    };
    // No turn or segment has been invented in the serializable preparation record.
    // 可序列化准备记录中没有虚构回合或分段。
    let encoded = serde_json::to_value(&preparation).unwrap();
    let decoded: ChildExecutionPreparation = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(decoded, preparation);
    assert!(encoded.get("business_turn_id").is_none());
    assert!(encoded.get("segment_generation").is_none());
    for field in [
        "runtime_identity",
        "invocation_config",
        "resolved_config",
        "request_overrides",
    ] {
        let mut incomplete = encoded.clone();
        incomplete.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ChildExecutionPreparation>(incomplete).is_err());
    }
    let mut unknown = encoded.clone();
    unknown["segment_generation"] = serde_json::json!(1);
    assert!(serde_json::from_value::<ChildExecutionPreparation>(unknown).is_err());
    let mut invalid_identity = encoded;
    invalid_identity["task_id"] = serde_json::json!("display-name");
    assert!(serde_json::from_value::<ChildExecutionPreparation>(invalid_identity).is_err());
    // The real root-input allocator supplies the turn; the document supplies its generation.
    // 真实根输入分配器提供回合；文档提供自身代次。
    let mut document = SessionDocument::new(task_id.to_string(), "project", "Child", 1);
    let turn = document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new(&preparation.execution_id).unwrap(),
                &preparation.invocation_config.prompt,
                vec![],
            ),
            2,
        )
        .unwrap()
        .into_value();
    expected.task_id = SubagentTaskId::try_new(task_id.to_string()).unwrap();
    expected.root_session_id = root_session_id.to_string();
    expected.parent_session_id = parent_session_id.to_string();
    expected.segment_generation = document.segment_generation();
    expected.business_turn_id = turn;
    assert!(preparation.clone().bind_child_turn(0, turn).is_err());
    let mut self_parent = preparation.clone();
    self_parent.parent_session_id = task_id;
    assert!(
        self_parent
            .bind_child_turn(document.segment_generation(), turn)
            .is_err()
    );
    let bound = decoded
        .bind_child_turn(document.segment_generation(), turn)
        .unwrap();
    assert_eq!(bound, expected);
    document.capture_child_execution_profile(bound, 3).unwrap();
    let candidate = document.prepare_flush_candidates().unwrap().unwrap();
    document.advance_durable(candidate.revision).unwrap();
    let reloaded = SessionDocument::from_parts(
        serde_json::from_str(&serde_json::to_string(&document.metadata).unwrap()).unwrap(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )
    .unwrap();
    assert_eq!(reloaded.metadata.child_execution_profile, Some(expected));
}

/// Recovery requires durable original business ownership and preserves the source unchanged.
/// 恢复要求可靠原业务归属，并保持来源不变。
/// Returns nothing; checks stop, dirty state, stale execution and missing original history.
/// 不返回值；检查停止、脏状态、过期执行及缺失原历史。
#[test]
fn child_business_recovery_requires_original_published_source() {
    use crate::v1::collaboration::{
        CollaborationExecutionId, CollaborationOperationId, CollaborationRunControl,
        SetRunControlInput,
    };
    // Capture actual root identity and its business profile through production transitions.
    // 通过生产转换捕获实际根身份及其业务配置。
    let (mut document, profile) = fixture();
    let execution = CollaborationExecutionId::try_new(&profile.business_execution_id).unwrap();
    document
        .capture_child_execution_profile(profile.clone(), 3)
        .unwrap();
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("running").unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: 4,
        })
        .unwrap();
    assert!(
        document
            .child_business_recovery_profile(&execution)
            .is_err()
    );
    // Publishing makes this precise source available without changing any historical event.
    // 发布使此精确来源可用，不改变任何历史事件。
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    let before = document.clone();
    assert_eq!(
        document
            .child_business_recovery_profile(&execution)
            .unwrap(),
        &profile
    );
    assert_eq!(document, before);
    assert!(
        document
            .child_business_recovery_profile(
                &CollaborationExecutionId::try_new("other-execution").unwrap()
            )
            .is_err()
    );
    // A valid profile alone cannot authorize a source whose original root is unavailable.
    // 仅有效配置不能授权原根输入不可用的来源。
    let mut missing_root = document.clone();
    missing_root.events.remove(0);
    assert!(
        missing_root
            .child_business_recovery_profile(&execution)
            .is_err()
    );
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("stop-main").unwrap(),
            state: CollaborationRunControl::MainOnly,
            changed_at_ms: 5,
        })
        .unwrap();
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    assert!(
        document
            .child_business_recovery_profile(&execution)
            .is_err()
    );
}

/// Recovery never mistakes an interrupted tool for confirmed absence of external effects.
/// 恢复绝不将中断工具误判为已确认没有外部效果。
/// Returns nothing; an actual API/tool interruption must require reconciliation.
/// 不返回值；实际 API 与工具中断必须要求核对。
#[test]
fn child_business_recovery_rejects_active_api_and_uncertain_tool() {
    use crate::v1::collaboration::{
        CollaborationExecutionId, CollaborationOperationId, CollaborationRunControl,
        SetRunControlInput,
    };
    // Publish an original business profile before starting its actual tool request.
    // 在开始实际工具请求前发布原业务配置。
    let (mut document, profile) = fixture();
    let execution = CollaborationExecutionId::try_new(&profile.business_execution_id).unwrap();
    document
        .capture_child_execution_profile(profile, 3)
        .unwrap();
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("running").unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: 4,
        })
        .unwrap();
    // The physical API and tool represent work that might already have escaped the host.
    // 物理 API 和工具表示可能已逸出宿主的工作。
    let api = document
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("provider", "model"),
            0,
        )
        .unwrap();
    document
        .start_tool(
            api,
            "shell_command",
            crate::v1::ToolPayload::try_new(serde_json::json!({"command":"external-action"}))
                .unwrap(),
            None,
            5,
        )
        .unwrap();
    // Active ownership must be rejected even after the source is fully published.
    // 即使来源已完整发布，也必须拒绝活动所有权。
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    assert!(
        document
            .child_business_recovery_profile(&execution)
            .is_err()
    );
    document.interrupt_active_api_for_process(6).unwrap();
    // Durable interruption is not evidence that an external side effect never happened.
    // 可靠中断不代表外部效果从未发生。
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    assert!(document.active_api().is_none());
    assert!(matches!(
        document.child_business_recovery_profile(&execution),
        Err(DomainError::CollaborationStateViolation {
            field: "child_business_recovery.unconfirmed_tool_result"
        })
    ));
}

/// An old captured profile cannot resume a completed turn or a later task in the same child.
/// 旧捕获配置不能恢复已完成回合或同一子会话的后续任务。
/// Returns nothing; actual root submission proves execution boundaries remain distinct.
/// 不返回值；实际根提交证明执行边界保持独立。
#[test]
fn child_business_recovery_rejects_reused_child_turn() {
    use crate::v1::collaboration::{
        CollaborationExecutionId, CollaborationOperationId, CollaborationRunControl,
        SetRunControlInput,
    };
    // The original profile remains useful for QA but cannot grant new business ownership.
    // 原配置仍可用于答疑，但不能授予新业务归属。
    let (mut document, profile) = fixture();
    let execution = CollaborationExecutionId::try_new(&profile.business_execution_id).unwrap();
    document
        .capture_child_execution_profile(profile, 3)
        .unwrap();
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("running").unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: 4,
        })
        .unwrap();
    document.close_turn(TurnOutcome::Completed, 1, 5).unwrap();
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    assert!(
        document
            .child_business_recovery_profile(&execution)
            .is_err()
    );
    document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("later-business").unwrap(),
                "later task",
                vec![],
            ),
            6,
        )
        .unwrap();
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    assert!(
        document
            .child_business_recovery_profile(&execution)
            .is_err()
    );
    assert!(
        document
            .child_business_recovery_profile(
                &CollaborationExecutionId::try_new("later-business").unwrap()
            )
            .is_err()
    );
}

/// Creates a real child business turn and its resolved non-executable configuration.
/// 创建真实子业务回合及其已解析的不可执行配置。
/// Returns the document and matching profile for mutation and replay checks.
/// 返回文档与匹配配置，用于修改及重放检查。
fn fixture() -> (SessionDocument, ChildExecutionProfile) {
    // The business owner comes from the real root-input allocator.
    // 业务所有者来自真实根输入分配器。
    let mut document = SessionDocument::new("child", "project", "Child", 1);
    let turn = document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("business-execution").unwrap(),
                "Inspect the code",
                vec![],
            ),
            2,
        )
        .unwrap()
        .into_value();
    let profile = ChildExecutionProfile {
        version: 1,
        root_session_id: String::from("root"),
        parent_session_id: String::from("parent"),
        task_id: SubagentTaskId::try_new("child").unwrap(),
        segment_generation: document.segment_generation(),
        business_turn_id: turn,
        business_execution_id: String::from("business-execution"),
        invocation_config: crate::TaskInvocationConfig::with_profile(
            "reviewer",
            "Inspect the code",
        ),
        resolved_config: crate::TaskSessionResolvedConfig {
            requested_config: crate::SessionRequestedConfig::default(),
            effective_config: crate::SessionEffectiveConfig::default(),
            functional_system_prompt_layers: vec![String::from("Inspect ownership boundaries")],
            runtime_policy: crate::ExecutionRuntimePolicy::task_session(),
        },
        runtime_identity: crate::v1::child_execution::ChildExecutionRuntimeIdentity {
            public_task_id: String::from("code-reviewer"),
            display_title: String::from("Review original code"),
            description: Some(String::from("Inspect ownership")),
            subagent: Some(String::from("reviewer")),
            depth: 2,
        },
        effective_config: crate::SessionEffectiveConfig::default(),
        runtime_policy: crate::ExecutionRuntimePolicy::task_session(),
        execution_agent_id: Some(String::from("main")),
        connection_id: String::from("captured-child-connection"),
        temperature: Some(0.25),
        request_overrides: crate::v1::child_execution::ChildExecutionRequestOverrides {
            enabled_capabilities: Some(Default::default()),
            session_vmm: crate::v1::config::VmmSessionPolicyConfig {
                precheck_enabled: Some(false),
                memory_writeback_enabled: Some(true),
            },
            ..Default::default()
        },
        admitted_tools: [(
            String::from("reply_message"),
            ChildToolOrigin {
                source_kind: ChildToolSourceKind::Native,
                namespace: String::from("collaboration"),
                original_name: String::from("reply_message"),
            },
        )]
        .into_iter()
        .collect(),
        fixed_agent_prompt: Some(String::from("Fixed parent prompt")),
        prompt_context: ChildExecutionPromptContext {
            identity: String::from("reviewer"),
            delegated_task: String::from("Inspect the code"),
            delegation: Some(String::from("Report findings to the parent")),
            functional_system_prompt: Some(String::from("Inspect ownership boundaries")),
        },
    };
    (document, profile)
}

/// Runtime identities survive strict decoding and cannot rename the same original execution.
/// 运行时身份经严格解码保留，不能重命名同一个原执行。
/// Malformed identities fail atomically; editable session presentation is not lookup authority.
/// 畸形身份原子失败；可编辑会话展示不充当查找权威。
#[test]
fn child_profile_runtime_identity_is_required_and_immutable() {
    let (mut document, profile) = fixture();
    let before = document.clone();
    for (name, depth) in [
        ("", 2),
        (" reviewer ", 2),
        ("parent/reviewer", 2),
        ("reviewer", 0),
        ("reviewer", 1),
    ] {
        let mut invalid = profile.clone();
        invalid.runtime_identity.public_task_id = name.into();
        invalid.runtime_identity.depth = depth;
        assert!(
            document
                .capture_child_execution_profile(invalid, 3)
                .is_err()
        );
        assert_eq!(document, before);
    }
    document
        .capture_child_execution_profile(profile.clone(), 3)
        .unwrap();
    let captured = document.clone();
    let mut renamed = profile.clone();
    renamed.runtime_identity.public_task_id = String::from("another-reviewer");
    assert!(
        document
            .capture_child_execution_profile(renamed, 4)
            .is_err()
    );
    assert_eq!(document, captured);
    let mut moved = profile.clone();
    moved.parent_session_id = String::from("another-parent");
    assert!(document.capture_child_execution_profile(moved, 4).is_err());
    assert_eq!(document, captured);
    // The metadata display title already differs from the frozen task title and public lookup name.
    // 元数据展示标题本来就与冻结任务标题及公开查找名称不同。
    assert_ne!(
        document.metadata.title,
        profile.runtime_identity.public_task_id
    );
    assert_ne!(
        document.metadata.title,
        profile.runtime_identity.display_title
    );
    let encoded = serde_json::to_value(&profile).unwrap();
    let decoded: ChildExecutionProfile = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(decoded.runtime_identity, profile.runtime_identity);
    let mut missing = encoded;
    missing.as_object_mut().unwrap().remove("runtime_identity");
    assert!(serde_json::from_value::<ChildExecutionProfile>(missing).is_err());
}

/// Original invocation and resolved policy survive persistence without accepting reinterpretation.
/// 原调用及已解析策略持久保存，拒绝重新解释。
/// Returns nothing; verifies optional inheritance, strict required fields and atomic recapture rejection.
/// 不返回值；验证可选继承、严格必需字段和重新捕获的原子拒绝。
#[test]
fn child_profile_original_task_config_is_required_and_immutable() {
    // The unresolved absence and the resolved value deliberately differ.
    // 未解析的缺省与已解析值有意不同。
    let (mut document, mut profile) = fixture();
    profile.invocation_config.inherit_parent_config = None;
    profile
        .resolved_config
        .requested_config
        .inherit_parent_config = false;
    profile
        .resolved_config
        .effective_config
        .inherit_parent_config = false;
    profile.invocation_config.extra = serde_json::json!({"max_turns": 17});
    profile.resolved_config.effective_config.extra = serde_json::json!({"max_turns": 17});
    document
        .capture_child_execution_profile(profile.clone(), 3)
        .unwrap();
    // Snapshot the entire document so rejected updates cannot alter unrelated metadata either.
    // 保存整个文档，确保拒绝更新时也不能改变无关元数据。
    let before = document.clone();
    let mut changed_invocation = profile.clone();
    changed_invocation.invocation_config.inherit_parent_config = Some(false);
    let mut changed_resolution = profile.clone();
    changed_resolution
        .resolved_config
        .functional_system_prompt_layers
        .push("New catalog prompt".into());
    for changed in [changed_invocation, changed_resolution] {
        assert!(matches!(
            document.capture_child_execution_profile(changed, 4),
            Err(DomainError::CollaborationStateViolation {
                field: "child_execution_profile.immutable_task_config"
            })
        ));
        assert_eq!(document, before);
    }
    // Reload the actual document metadata rather than trusting the process-local request.
    // 重载实际文档元数据，不依赖进程内请求。
    let encoded = serde_json::to_value(&document.metadata).unwrap();
    let reloaded: crate::v1::config::MetadataDocument = serde_json::from_value(encoded).unwrap();
    let captured = reloaded.child_execution_profile.unwrap();
    assert_eq!(captured.invocation_config, profile.invocation_config);
    assert_eq!(captured.resolved_config, profile.resolved_config);
    for required in ["invocation_config", "resolved_config"] {
        let mut missing = serde_json::to_value(&profile).unwrap();
        missing.as_object_mut().unwrap().remove(required);
        assert!(serde_json::from_value::<ChildExecutionProfile>(missing).is_err());
    }
    // Final host registrations can still refresh without re-resolving the task configuration.
    // 最终宿主注册仍可刷新，而无需重新解析任务配置。
    let mut refreshed = profile;
    refreshed.admitted_tools.clear();
    assert!(
        document
            .capture_child_execution_profile(refreshed, 5)
            .unwrap()
    );
}

/// Invalid override identities cannot mutate metadata, and missing current-contract fields cannot decode.
/// 无效覆盖身份不能修改元数据，缺少当前契约字段时不能解码。
#[test]
fn child_profile_request_overrides_are_strict_and_failure_atomic() {
    // A real open business turn supplies the exact capture owner.
    // 真实开放业务回合提供精确捕获所有者。
    let (mut document, profile) = fixture();
    // The prior metadata is the complete failure-atomicity reference.
    // 先前元数据是完整的失败原子性参照。
    let before = serde_json::to_value(&document.metadata).unwrap();
    // Malformed route identity must be rejected before profile publication.
    // 格式错误路由身份必须在配置发布前被拒绝。
    let mut invalid = profile.clone();
    invalid.request_overrides.provider_tool_selections =
        Some(std::collections::BTreeMap::from([(
            String::from("web_search"),
            crate::v1::config::ProviderToolTargetConfig {
                connection_id: String::from("connection"),
                target_id: String::from(" "),
            },
        )]));
    assert!(
        document
            .capture_child_execution_profile(invalid, 3)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&document.metadata).unwrap(), before);
    // The unreleased contract deliberately has no missing-overrides compatibility path.
    // 未发布契约明确不提供缺少覆盖记录时的兼容路径。
    let mut missing_field = serde_json::to_value(profile).unwrap();
    missing_field
        .as_object_mut()
        .unwrap()
        .remove("request_overrides");
    assert!(serde_json::from_value::<ChildExecutionProfile>(missing_field).is_err());
}

/// Capturing is idempotent and remains readable after business completion and durable reconstruction.
/// 捕获具有幂等性，并在业务完成及可靠重建之后保持可读。
#[test]
fn child_profile_capture_survives_completion_and_reload() {
    // Configuration is metadata-only and does not append synthetic model-visible messages.
    // 配置仅属于元数据，不追加合成的模型可见消息。
    let (mut document, profile) = fixture();
    let event_count = document.events().len();
    let revision = document.memory_revision();
    assert!(
        document
            .capture_child_execution_profile(profile.clone(), 3)
            .unwrap()
    );
    assert_eq!(document.memory_revision(), revision + 1);
    assert_eq!(document.events().len(), event_count);
    let captured = document.clone();
    assert!(
        !document
            .capture_child_execution_profile(profile.clone(), 4)
            .unwrap()
    );
    assert_eq!(document, captured);
    document.close_turn(TurnOutcome::Completed, 1, 5).unwrap();
    let reloaded = SessionDocument::from_parts(
        serde_json::from_str(&serde_json::to_string(&document.metadata).unwrap()).unwrap(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )
    .unwrap();
    assert_eq!(reloaded.metadata.child_execution_profile, Some(profile));
    assert!(reloaded.active_turn().is_none());
}

/// Wrong task, generation, business turn and future profile versions fail without partial writes.
/// 错误任务、代际、业务回合及未来配置版本不会产生部分写入。
#[test]
fn child_profile_rejects_foreign_or_unknown_owner() {
    // Every invalid candidate is compared with the same untouched document.
    // 每个无效候选均与相同的未修改文档比较。
    let (mut document, profile) = fixture();
    let before = document.clone();
    for invalid in [
        ChildExecutionProfile {
            version: 2,
            ..profile.clone()
        },
        ChildExecutionProfile {
            task_id: SubagentTaskId::try_new("another-child").unwrap(),
            ..profile.clone()
        },
        ChildExecutionProfile {
            segment_generation: 2,
            ..profile.clone()
        },
        ChildExecutionProfile {
            business_turn_id: TurnId::try_new(999).unwrap(),
            ..profile.clone()
        },
        ChildExecutionProfile {
            parent_session_id: String::from("child"),
            ..profile.clone()
        },
        ChildExecutionProfile {
            business_execution_id: String::from("another-execution"),
            ..profile.clone()
        },
    ] {
        assert!(
            document
                .capture_child_execution_profile(invalid, 3)
                .is_err()
        );
        assert_eq!(document, before);
    }
    document
        .capture_child_execution_profile(profile, 3)
        .unwrap();
    document
        .metadata
        .child_execution_profile
        .as_mut()
        .unwrap()
        .version = 2;
    assert!(
        SessionDocument::from_parts(
            document.metadata.clone(),
            document.runtime_lock.clone(),
            document.compactions.clone(),
            document.segment_catalog().to_vec(),
            document.events().to_vec()
        )
        .is_err()
    );
}

/// Forking retains readable history but clears executable configuration ownership from the old child.
/// 分叉保留可读历史，但清除旧子代理的可执行配置所有权。
#[test]
fn child_profile_does_not_cross_fork_generation() {
    // A completed business boundary is required by the real fork builder.
    // 真实分叉构建器要求已完成业务边界。
    let (mut document, profile) = fixture();
    document
        .capture_child_execution_profile(profile, 3)
        .unwrap();
    document.close_turn(TurnOutcome::Completed, 1, 4).unwrap();
    // Forks require the source revision to have crossed the actual writer admission boundary.
    // 分叉要求来源修订已经越过实际写入器准入边界。
    let candidate = document.prepare_flush_candidates().unwrap().unwrap();
    document.advance_durable(candidate.revision).unwrap();
    let end_seq = document.events().last().unwrap().seq;
    let fork = document
        .build_fork_generation(super::super::generation::ForkGenerationInput {
            child_session_id: String::from("fork"),
            title: String::from("Fork"),
            source_turn_end_seq: end_seq,
            now_ms: 5,
        })
        .unwrap();
    assert!(fork.metadata.child_execution_profile.is_none());
    assert!(document.metadata.child_execution_profile.is_some());
}
