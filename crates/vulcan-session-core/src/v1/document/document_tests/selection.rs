//! Session selection, pending request promotion, and configuration validation regression tests.
//! 会话选择、待生效请求提升及配置校验回归测试。

use super::*;
use crate::v1::config::{
    ProviderMediaUnderstandingReasoningEffort, ProviderMediaUnderstandingRouteConfig,
};

/// Pending request promotion and Goal terminal updates mutate metadata and
/// append structured display notes.
/// 待生效请求提升与 Goal 终态更新修改 metadata 并追加结构化 display-note。
#[test]
fn request_promotion_and_goal_state_mutations() {
    let mut doc = new_document();
    doc.set_pending_request(
        PendingRequestConfig {
            provider: Some("anthropic".to_string()),
            connection_id: None,
            model: Some("claude-4".to_string()),
            thinking_level: None,
            temperature: None,
            set_at_ms: T0,
        },
        T0,
    )
    .unwrap();
    doc.promote_pending_request(None, T0).unwrap();
    assert_eq!(
        doc.metadata.config.active_request.provider.as_deref(),
        Some("anthropic")
    );
    assert!(doc.metadata.config.pending_request.is_none());
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Metadata));

    doc.metadata.mode.goal.enabled = true;
    doc.metadata.mode.goal.state = GoalState::Active;
    let note_seq = doc.block_goal("goal_blocked", T0 + 1).unwrap();
    assert_eq!(doc.metadata.mode.goal.state, GoalState::Blocked);
    let note = doc.events().iter().find(|e| e.seq() == note_seq).unwrap();
    match &note.event {
        EventBody::DisplayNote(display) => {
            assert_eq!(display.notice_kind, DisplayNoticeKind::GoalStatus);
            assert_eq!(display.reason_code, "goal_blocked");
        }
        _ => unreachable!(),
    }
}

/// Setting a pending request persists the exact typed payload and dirties only
/// metadata without appending a timeline event.
/// 设置待生效请求会持久化精确强类型载荷，并且只标记 metadata dirty，不追加
/// 时间线事件。
#[test]
fn pending_request_setter_dirties_only_metadata_and_serializes() -> Result<(), serde_json::Error> {
    let mut doc = new_document();
    doc.set_pending_request(
        PendingRequestConfig {
            provider: Some("anthropic".to_string()),
            connection_id: None,
            model: Some("claude-4".to_string()),
            thinking_level: Some("high".to_string()),
            temperature: Some(0.2),
            set_at_ms: T0,
        },
        T0,
    )
    .unwrap();

    assert_eq!(
        doc.dirty_files().collect::<Vec<_>>(),
        vec![DirtyFile::Metadata]
    );
    assert!(doc.events().is_empty());
    let Some(stored) = doc.metadata.config.pending_request.as_ref() else {
        panic!("the setter must store one pending request");
    };
    assert_eq!(
        serde_json::to_string(stored)?,
        r#"{"provider":"anthropic","model":"claude-4","thinking_level":"high","temperature":0.2,"set_at_ms":1700000000000}"#
    );
    Ok(())
}

/// Promotion without a pending request fails before changing any document fact.
/// 没有待生效请求时，promotion 在修改任何文档事实前失败。
#[test]
fn pending_request_promotion_rejects_missing_without_mutation() {
    let mut doc = new_document();
    let revision = doc.memory_revision();

    assert!(matches!(
        doc.promote_pending_request(None, T0),
        Err(DomainError::ReferenceViolation {
            kind: "pending_request",
            value: 0
        })
    ));
    assert_eq!(doc.memory_revision(), revision);
    assert!(doc.events().is_empty());
}

/// Promotion rejects an in-flight API and preserves both active and pending
/// request configs without appending a display fact.
/// promotion 拒绝在途 API，并保留 active 与 pending 请求配置，不追加显示事实。
#[test]
fn pending_request_promotion_rejects_unsafe_api_boundary() -> Result<(), DomainError> {
    let mut doc = new_document();
    let turn_id = doc.submit_root_input_fixture("work", Vec::new(), T0)?;
    doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0,
    )?;
    doc.set_pending_request(
        PendingRequestConfig {
            provider: Some("anthropic".to_string()),
            connection_id: None,
            model: Some("claude-4".to_string()),
            thinking_level: None,
            temperature: None,
            set_at_ms: T0 + 1,
        },
        T0 + 1,
    )
    .unwrap();
    let active_before = doc.metadata.config.active_request.clone();
    let pending_before = doc.metadata.config.pending_request.clone();
    let event_count = doc.events().len();
    let revision = doc.memory_revision();

    assert!(matches!(
        doc.promote_pending_request(Some(turn_id), T0 + 2),
        Err(DomainError::PromoteWhileApiOpen)
    ));
    assert_eq!(doc.metadata.config.active_request, active_before);
    assert_eq!(doc.metadata.config.pending_request, pending_before);
    assert_eq!(doc.events().len(), event_count);
    assert_eq!(doc.memory_revision(), revision);
    Ok(())
}

/// Promoting an identical request clears pending metadata without manufacturing
/// a model-switch timeline event.
/// 提升相同请求时只清除 pending metadata，不制造模型切换时间线事件。
#[test]
fn identical_pending_request_clears_without_switch_notice() -> Result<(), DomainError> {
    let mut doc = new_document();
    let active = ActiveRequestConfig {
        provider: Some("openai".to_string()),
        connection_id: Some("conn-1".to_string()),
        model: Some("gpt-5".to_string()),
        thinking_level: Some("high".to_string()),
        temperature: Some(0.3),
    };
    doc.metadata.config.active_request = active.clone();
    doc.set_pending_request(
        PendingRequestConfig {
            provider: active.provider.clone(),
            connection_id: active.connection_id.clone(),
            model: active.model.clone(),
            thinking_level: active.thinking_level.clone(),
            temperature: active.temperature,
            set_at_ms: T0,
        },
        T0,
    )
    .unwrap();

    assert_eq!(doc.promote_pending_request(None, T0 + 1)?, None);
    assert_eq!(doc.metadata.config.active_request, active);
    assert!(doc.metadata.config.pending_request.is_none());
    assert!(doc.events().is_empty());
    assert_eq!(
        doc.dirty_files().collect::<Vec<_>>(),
        vec![DirtyFile::Metadata]
    );
    Ok(())
}

/// Conditional pending cleanup removes only the exact intent consumed by an admitted boundary.
/// 条件式待生效清理只移除已准入边界实际消费的精确意图。
#[test]
fn pending_request_cleanup_preserves_a_newer_different_intent() -> Result<(), DomainError> {
    let mut doc = new_document();
    let pending = PendingRequestConfig {
        provider: Some(String::from("openai")),
        connection_id: Some(String::from("connection")),
        model: Some(String::from("gpt-5")),
        thinking_level: Some(String::from("high")),
        temperature: None,
        set_at_ms: T0,
    };
    doc.set_pending_request(pending, T0)?;
    let revision = doc.memory_revision();
    let different = ActiveRequestConfig {
        provider: Some(String::from("openai")),
        connection_id: Some(String::from("connection")),
        model: Some(String::from("gpt-5")),
        thinking_level: Some(String::from("low")),
        temperature: None,
    };
    assert!(!doc.clear_pending_request_if_matches(&different, T0 + 1)?);
    assert_eq!(doc.memory_revision(), revision);
    let exact = ActiveRequestConfig {
        thinking_level: Some(String::from("high")),
        ..different
    };
    assert!(doc.clear_pending_request_if_matches(&exact, T0 + 2)?);
    assert!(doc.metadata.config.pending_request.is_none());
    assert_eq!(doc.memory_revision(), revision + 1);
    assert!(doc.events().is_empty());
    Ok(())
}

/// Initializes the first route without appending a misleading model-switch event.
/// 初始化首次路线，不追加误导的模型切换事件。
/// Parameters: none. Returns: success after checking metadata and unchanged event history.
/// 参数：无。返回值：检查元数据与未改变的事件历史后返回成功。
#[test]
fn initial_pending_request_initializes_without_switch_notice() -> Result<(), DomainError> {
    // doc starts with the production default empty active route.
    // doc 使用生产默认的空活动路线初始化。
    let mut doc = new_document();
    doc.set_pending_request(
        PendingRequestConfig {
            provider: Some("configured-provider".to_string()),
            connection_id: Some("configured-account".to_string()),
            model: Some("configured-model".to_string()),
            thinking_level: Some("high".to_string()),
            temperature: None,
            set_at_ms: T0,
        },
        T0,
    )?;
    assert_eq!(doc.promote_pending_request(None, T0 + 1)?, None);
    assert_eq!(
        doc.metadata.config.active_request.provider.as_deref(),
        Some("configured-provider")
    );
    assert_eq!(
        doc.metadata.config.active_request.model.as_deref(),
        Some("configured-model")
    );
    assert!(doc.metadata.config.pending_request.is_none());
    assert!(doc.events().is_empty());
    Ok(())
}

/// A real safe-boundary request switch atomically updates metadata and appends
/// the exact permanent before/after request-config event.
/// 真实安全边界请求切换会原子更新 metadata，并追加精确的永久请求配置前后值事件。
#[test]
fn real_pending_request_switch_appends_serialized_notice() -> Result<(), Box<dyn std::error::Error>>
{
    let mut doc = new_document();
    doc.metadata.config.active_request = ActiveRequestConfig {
        provider: Some("openai".to_string()),
        connection_id: Some("conn-1".to_string()),
        model: Some("gpt-5".to_string()),
        thinking_level: Some("high".to_string()),
        temperature: Some(0.3),
    };
    let turn_id = doc.submit_root_input_fixture("switch", Vec::new(), T0)?;
    doc.set_pending_request(
        PendingRequestConfig {
            provider: Some("anthropic".to_string()),
            connection_id: Some("conn-2".to_string()),
            model: Some("claude-4".to_string()),
            thinking_level: Some("max".to_string()),
            temperature: Some(0.2),
            set_at_ms: T0 + 1,
        },
        T0 + 1,
    )
    .unwrap();

    let Some(notice_seq) = doc.promote_pending_request(Some(turn_id), T0 + 2)? else {
        panic!("a real request switch must append a notice");
    };
    assert_eq!(
        doc.metadata.config.active_request,
        ActiveRequestConfig {
            provider: Some("anthropic".to_string()),
            connection_id: Some("conn-2".to_string()),
            model: Some("claude-4".to_string()),
            thinking_level: Some("max".to_string()),
            temperature: Some(0.2),
        }
    );
    assert!(doc.metadata.config.pending_request.is_none());
    let Some(notice) = doc.events().iter().find(|event| event.seq() == notice_seq) else {
        panic!("the promoted switch must exist in the timeline");
    };
    assert_eq!(
        serde_json::to_string(notice)?,
        r#"{"seq":2,"turn_id":2,"created_at_ms":1700000000002,"updated_at_ms":1700000000002,"event_revision":1,"visibility":"ui_only","event":{"kind":"request_config_changed","previous":{"provider":"openai","connection_id":"conn-1","model":"gpt-5","thinking_level":"high","temperature":0.3},"current":{"provider":"anthropic","connection_id":"conn-2","model":"claude-4","thinking_level":"max","temperature":0.2}}}"#
    );
    assert!(doc.dirty_files().any(|file| file == DirtyFile::Metadata));
    assert!(doc.dirty_files().any(|file| file == DirtyFile::ActiveTail));
    Ok(())
}

/// Output-style normalization is explicit, while request selections stay strict and idempotent.
/// 输出风格会显式规范化，而请求选择保持严格且幂等。
#[test]
fn selection_and_pending_request_setters_are_atomic_and_idempotent() -> Result<(), DomainError> {
    let mut doc = new_document();
    assert!(doc.set_output_style(Some("  concise  ".to_string()), T0 + 1)?);
    assert_eq!(
        doc.metadata.config.selection.output_style.as_deref(),
        Some("concise")
    );
    let style_revision = doc.memory_revision();
    assert!(!doc.set_output_style(Some("concise".to_string()), T0 + 2)?);
    assert_eq!(doc.memory_revision(), style_revision);
    assert!(doc.set_output_style(Some("   ".to_string()), T0 + 3)?);
    assert_eq!(doc.metadata.config.selection.output_style, None);
    let before_bad_style_time = doc.clone();
    assert!(
        doc.set_output_style(Some("concise".to_string()), -1)
            .is_err()
    );
    assert_eq!(doc, before_bad_style_time);

    let pending = PendingRequestConfig {
        provider: Some("openai".to_string()),
        connection_id: Some("connection".to_string()),
        model: Some("gpt-5".to_string()),
        thinking_level: Some("high".to_string()),
        temperature: Some(0.2),
        set_at_ms: T0 + 4,
    };
    assert!(doc.set_pending_request(pending, T0 + 4)?);
    let stored = doc
        .metadata
        .config
        .pending_request
        .as_ref()
        .expect("pending request must be stored");
    assert_eq!(stored.provider.as_deref(), Some("openai"));
    assert_eq!(stored.connection_id.as_deref(), Some("connection"));
    assert_eq!(stored.model.as_deref(), Some("gpt-5"));
    assert_eq!(stored.thinking_level.as_deref(), Some("high"));
    let pending_revision = doc.memory_revision();
    assert!(!doc.set_pending_request(
        PendingRequestConfig {
            provider: Some("openai".to_string()),
            connection_id: Some("connection".to_string()),
            model: Some("gpt-5".to_string()),
            thinking_level: Some("high".to_string()),
            temperature: Some(0.2),
            set_at_ms: T0 + 5,
        },
        T0 + 5,
    )?);
    assert_eq!(doc.memory_revision(), pending_revision);
    let before_invalid_pending = doc.clone();
    assert!(matches!(
        doc.set_pending_request(
            PendingRequestConfig {
                provider: Some(" openai".to_string()),
                connection_id: None,
                model: Some("gpt-5".to_string()),
                thinking_level: None,
                temperature: None,
                set_at_ms: T0 + 6,
            },
            T0 + 6,
        ),
        Err(DomainError::InvalidRequestConfiguration {
            field: "pending.provider"
        })
    ));
    assert_eq!(doc, before_invalid_pending);
    assert!(
        doc.set_pending_request(
            PendingRequestConfig {
                provider: Some("openai".to_string()),
                connection_id: None,
                model: Some("gpt-5".to_string()),
                thinking_level: None,
                temperature: Some(f64::NAN),
                set_at_ms: T0 + 7,
            },
            T0 + 7,
        )
        .is_err()
    );
    assert_eq!(doc, before_invalid_pending);
    Ok(())
}

/// Provider-tool selection is complete, canonical, idempotent, and reload-validated.
/// 供应商工具选择保持完整、规范、幂等，并在重载时校验。
#[test]
fn provider_tool_selection_is_atomic_idempotent_and_strict() -> Result<(), DomainError> {
    let mut doc = new_document();
    let selection = ProviderToolSelectionConfig {
        initialized: true,
        overridden: true,
        enabled_capabilities: vec!["image_generation".to_string(), "web_search".to_string()],
        selections: vec![ProviderToolCapabilitySelection {
            capability: "web_search".to_string(),
            target: ProviderToolTargetConfig {
                connection_id: "connection-a".to_string(),
                target_id: "search-a".to_string(),
            },
        }],
        media_understanding_selections: vec![ProviderMediaUnderstandingSelection {
            capability: "image_understanding".to_string(),
            mode: ProviderMediaUnderstandingMode::ProviderTool,
            selection: ProviderMediaUnderstandingRouteConfig::ProviderTool(
                ProviderToolTargetConfig {
                    connection_id: "connection-c".to_string(),
                    target_id: "image-understand-a".to_string(),
                },
            ),
            reasoning_effort: None,
        }],
        extension_target_sets: vec![ProviderExtensionTargetSetConfig {
            capability: "image_generation".to_string(),
            targets: vec![ProviderToolTargetConfig {
                connection_id: "connection-b".to_string(),
                target_id: "image-a".to_string(),
            }],
        }],
    };

    assert!(doc.set_provider_tool_selection(selection.clone(), T0 + 1)?);
    assert_eq!(doc.metadata.config.selection.provider_tools, selection);
    let revision = doc.memory_revision();
    assert!(!doc.set_provider_tool_selection(selection.clone(), T0 + 2)?);
    assert_eq!(doc.memory_revision(), revision);

    let before_invalid = doc.clone();
    let mut invalid = selection.clone();
    invalid.enabled_capabilities.reverse();
    assert!(matches!(
        doc.set_provider_tool_selection(invalid, T0 + 3),
        Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.enabled_capabilities.order"
        })
    ));
    assert_eq!(doc, before_invalid);

    let mut invalid_reasoning_mode = selection;
    invalid_reasoning_mode.media_understanding_selections[0].reasoning_effort =
        Some(ProviderMediaUnderstandingReasoningEffort::Adaptive);
    assert!(matches!(
        doc.set_provider_tool_selection(invalid_reasoning_mode, T0 + 4),
        Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.media_understanding_selections.reasoning_effort_mode"
        })
    ));
    assert_eq!(doc, before_invalid);

    let mut invalid_metadata = doc.metadata.clone();
    invalid_metadata.config.selection.provider_tools.initialized = false;
    assert!(matches!(
        SessionDocument::from_parts(
            invalid_metadata,
            doc.runtime_lock.clone(),
            doc.compactions.clone(),
            doc.segment_catalog.clone(),
            doc.events().to_vec(),
        ),
        Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.initialized"
        })
    ));
    Ok(())
}

/// Knowledge and VMM session selections are strict, idempotent metadata facts.
/// 知识库与 VMM 会话选择是严格且幂等的 metadata 事实。
#[test]
fn knowledge_and_vmm_selections_are_atomic_idempotent_and_strict() -> Result<(), DomainError> {
    let mut doc = new_document();
    let knowledge = KnowledgeLibrarySelectionConfig {
        mode: KnowledgeLibrarySelectionMode::Explicit,
        library_ids: vec!["library-a".to_string(), "library-b".to_string()],
        retrieval_mode: Some(crate::v1::config::KnowledgeRetrievalMode::DisplayHybrid),
    };
    assert!(doc.set_knowledge_library_selection(knowledge.clone(), T0 + 1)?);
    assert_eq!(doc.metadata.config.selection.knowledge_libraries, knowledge);
    let knowledge_revision = doc.memory_revision();
    assert!(!doc.set_knowledge_library_selection(knowledge.clone(), T0 + 2)?);
    assert_eq!(doc.memory_revision(), knowledge_revision);

    let before_invalid = doc.clone();
    let mut invalid_order = knowledge;
    invalid_order.library_ids.reverse();
    assert!(matches!(
        doc.set_knowledge_library_selection(invalid_order, T0 + 3),
        Err(DomainError::InvalidSelectionConfiguration {
            field: "knowledge_libraries.library_ids.order"
        })
    ));
    assert_eq!(doc, before_invalid);

    let mut invalid_metadata = doc.metadata.clone();
    invalid_metadata.config.selection.knowledge_libraries = KnowledgeLibrarySelectionConfig {
        mode: KnowledgeLibrarySelectionMode::Inherit,
        library_ids: vec!["library-a".to_string()],
        retrieval_mode: None,
    };
    assert!(matches!(
        SessionDocument::from_parts(
            invalid_metadata,
            doc.runtime_lock.clone(),
            doc.compactions.clone(),
            doc.segment_catalog.clone(),
            doc.events().to_vec(),
        ),
        Err(DomainError::InvalidSelectionConfiguration {
            field: "knowledge_libraries.inherit_ids"
        })
    ));

    let vmm = VmmSessionPolicyConfig {
        precheck_enabled: Some(false),
        memory_writeback_enabled: Some(true),
    };
    assert!(doc.set_vmm_session_policy(vmm.clone(), T0 + 4)?);
    assert_eq!(doc.metadata.config.selection.vmm, vmm);
    let vmm_revision = doc.memory_revision();
    assert!(!doc.set_vmm_session_policy(vmm, T0 + 5)?);
    assert_eq!(doc.memory_revision(), vmm_revision);
    Ok(())
}

/// The complete session-selection bundle commits once and rejects any invalid member atomically.
/// 完整会话选择配置只提交一次，并以原子方式拒绝任意无效成员。
#[test]
fn session_selection_bundle_is_single_revision_idempotent_and_failure_atomic()
-> Result<(), DomainError> {
    let mut doc = new_document();
    let input = SessionSelectionBundleInput {
        output_style: Some(" concise ".to_string()),
        provider_tools: ProviderToolSelectionConfig {
            initialized: true,
            overridden: true,
            enabled_capabilities: vec!["web_search".to_string()],
            selections: vec![ProviderToolCapabilitySelection {
                capability: "web_search".to_string(),
                target: ProviderToolTargetConfig {
                    connection_id: "connection-a".to_string(),
                    target_id: "search-a".to_string(),
                },
            }],
            media_understanding_selections: Vec::new(),
            extension_target_sets: Vec::new(),
        },
        knowledge_libraries: KnowledgeLibrarySelectionConfig {
            mode: KnowledgeLibrarySelectionMode::Explicit,
            library_ids: vec!["library-a".to_string()],
            retrieval_mode: Some(crate::v1::config::KnowledgeRetrievalMode::DisplayBm25),
        },
        vmm: VmmSessionPolicyConfig {
            precheck_enabled: Some(true),
            memory_writeback_enabled: Some(false),
        },
    };
    let before_revision = doc.memory_revision();
    assert!(doc.set_session_selection_bundle(input.clone(), T0 + 1)?);
    assert_eq!(doc.memory_revision(), before_revision + 1);
    assert_eq!(
        doc.metadata.config.selection.output_style.as_deref(),
        Some("concise")
    );
    let committed_revision = doc.memory_revision();
    assert!(!doc.set_session_selection_bundle(input.clone(), T0 + 2)?);
    assert_eq!(doc.memory_revision(), committed_revision);

    let mut invalid = input;
    invalid.knowledge_libraries.library_ids =
        vec!["library-b".to_string(), "library-a".to_string()];
    let before_invalid = doc.clone();
    assert!(matches!(
        doc.set_session_selection_bundle(invalid, T0 + 3),
        Err(DomainError::InvalidSelectionConfiguration {
            field: "knowledge_libraries.library_ids.order"
        })
    ));
    assert_eq!(doc, before_invalid);
    Ok(())
}

/// API request selection validation is identical at mutation and reload boundaries.
/// API 请求选择在修改边界与重载边界使用完全相同的校验契约。
#[test]
fn api_request_selection_validation_is_symmetric_and_atomic() -> Result<(), DomainError> {
    let mut doc = new_document();
    doc.submit_root_input_fixture("hello", Vec::new(), T0)?;
    let projection = ProviderProjectionProof {
        segment_generation: doc.segment_generation(),
        document_revision: doc.memory_revision(),
        retained_from_seq: Seq::try_new(1)?,
        through_seq: doc.events().last().expect("root input").seq,
        fingerprint_sha256: "0".repeat(64),
    };
    let invalid_requests = [
        (ApiRequestSnapshot::new("", "gpt-5"), "provider"),
        (ApiRequestSnapshot::new(" openai", "gpt-5"), "provider"),
        (ApiRequestSnapshot::new("openai", ""), "model"),
        (
            ApiRequestSnapshot {
                profile: Some(" ".to_string()),
                ..ApiRequestSnapshot::new("openai", "gpt-5")
            },
            "profile",
        ),
        (
            ApiRequestSnapshot {
                connection_id: Some("connection ".to_string()),
                ..ApiRequestSnapshot::new("openai", "gpt-5")
            },
            "connection_id",
        ),
        (
            ApiRequestSnapshot {
                thinking_level: Some(String::new()),
                ..ApiRequestSnapshot::new("openai", "gpt-5")
            },
            "thinking_level",
        ),
        (
            ApiRequestSnapshot {
                temperature: Some(f64::INFINITY),
                ..ApiRequestSnapshot::new("openai", "gpt-5")
            },
            "temperature",
        ),
    ];
    for (request, field) in invalid_requests {
        let before = doc.clone();
        assert!(matches!(
            doc.prepare_and_start_api(PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request,
                request_budget_evidence: test_provider_request_budget_evidence(),
                projection: projection.clone(),
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: T0 + 1,
            }),
            Err(DomainError::InvalidRequestConfiguration { field: actual })
                if actual == field
        ));
        assert_eq!(doc, before);
    }

    let api = doc.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    let mut invalid_events = doc.events().to_vec();
    let started = invalid_events
        .iter_mut()
        .find_map(|event| match &mut event.event {
            EventBody::ApiStarted(started) if started.api_call_id == api => Some(started),
            _ => None,
        })
        .expect("api_started event");
    started.request.profile = Some(" responses".to_string());
    assert!(matches!(
        SessionDocument::from_parts(
            doc.metadata.clone(),
            doc.runtime_lock.clone(),
            doc.compactions.clone(),
            doc.segment_catalog().to_vec(),
            invalid_events,
        ),
        Err(DomainError::InvalidRequestConfiguration { field: "profile" })
    ));

    let mut invalid_metadata = new_document().metadata;
    invalid_metadata.config.active_request.provider = Some("openai ".to_string());
    assert!(matches!(
        SessionDocument::from_parts(
            invalid_metadata,
            RuntimeLock::default(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ),
        Err(DomainError::InvalidRequestConfiguration {
            field: "active.provider"
        })
    ));
    Ok(())
}
