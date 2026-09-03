use super::*;
use crate::{
    SessionMediaUnderstandingModelSelection, SessionMediaUnderstandingReasoningEffort,
    SessionMediaUnderstandingSelection, SessionOutputStyleState, SessionPendingModelSelection,
};
#[test]
fn session_addon_revision_changes_only_when_namespace_value_changes() {
    let session_id = SessionId::new_v4();
    let mut state = SessionAddonState::new(session_id);
    let work_mode = SessionWorkMode {
        execution_mode: String::from("plan"),
        plan_mode: SessionPlanModeState::default(),
    };

    assert!(state.set_work_mode(&work_mode, String::from("1000")));
    assert_eq!(state.revision, 1);
    let namespace_revision = state
        .namespace(SessionAddonNamespace::WorkMode)
        .map(|entry| entry.revision);

    assert!(!state.set_work_mode(&work_mode, String::from("2000")));
    assert_eq!(state.revision, 1);
    assert_eq!(
        state
            .namespace(SessionAddonNamespace::WorkMode)
            .map(|entry| entry.revision),
        namespace_revision
    );
}

#[test]
fn session_addon_extracts_runtime_controls_and_work_mode() {
    let session_id = SessionId::new_v4();
    let runtime_state = SessionRuntimeStateEntry {
        active_provider_id: Some(String::from("minimax")),
        active_connection_id: Some(String::from("minimax-current-account")),
        active_model_id: Some(String::from("minimax-text")),
        pending_model_selection: Some(SessionPendingModelSelection {
            provider_id: String::from("command-code"),
            connection_id: String::from("command-code-account"),
            model_id: String::from("deepseek"),
        }),
        active_reasoning_effort: Some(ReasoningEffortLevel::None),
        active_reasoning_effort_explicit: true,
        media_understanding_selections: BTreeMap::from([(
            String::from("video_understand"),
            SessionMediaUnderstandingSelection::ModelReplacement {
                selection: SessionMediaUnderstandingModelSelection {
                    provider_id: String::from("minimax"),
                    model_id: String::from("minimax-text"),
                    connection_id: String::from("minimax-current-account"),
                },
                reasoning_effort: Some(SessionMediaUnderstandingReasoningEffort::Adaptive),
            },
        )]),
        context_compaction_window_node_id: Some(ContextCompactionWindowNodeId::CompressionWindow2),
        active_work_mode: Some(String::from("plan")),
        ..SessionRuntimeStateEntry::default()
    };

    let addon_state =
        SessionAddonState::from_runtime_state(session_id, &runtime_state, String::from("1"));

    assert_eq!(
        addon_state
            .runtime_controls()
            .and_then(|controls| controls.active_provider_id),
        Some(String::from("minimax"))
    );
    assert_eq!(
        addon_state
            .work_mode()
            .map(|work_mode| work_mode.execution_mode),
        Some(String::from("plan"))
    );
    let controls = addon_state
        .runtime_controls()
        .expect("runtime controls should preserve explicit reasoning provenance");
    assert_eq!(
        controls.active_reasoning_effort,
        Some(ReasoningEffortLevel::None)
    );
    assert!(controls.active_reasoning_effort_explicit);
    assert_eq!(
        controls.context_compaction_window_node_id,
        Some(ContextCompactionWindowNodeId::CompressionWindow2)
    );
    let serialized = serde_json::to_string(&addon_state).expect("addon state should serialize");
    assert!(serialized.contains("compression-window-2"));
    assert!(!serialized.contains("256000"));

    let mut restored_runtime_state = SessionRuntimeStateEntry::default();
    addon_state.apply_to_runtime_state(&mut restored_runtime_state);
    assert_eq!(
        restored_runtime_state.context_compaction_window_node_id,
        Some(ContextCompactionWindowNodeId::CompressionWindow2)
    );
    assert_eq!(
        restored_runtime_state.pending_model_selection,
        runtime_state.pending_model_selection
    );
    assert_eq!(
        restored_runtime_state.media_understanding_selections,
        runtime_state.media_understanding_selections
    );
}

#[test]
fn session_addon_drops_unknown_legacy_namespaces() {
    let session_id = SessionId::new_v4();
    let raw = format!(
        r#"{{
            "schemaVersion": 1,
            "sessionId": "{session_id}",
            "revision": 2,
            "namespaces": {{
                "todo_plan": {{
                    "revision": 1,
                    "hash": "legacy",
                    "updatedAt": "1000",
                    "value": {{"items": [{{"step": "old", "status": "pending"}}]}}
                }},
                "todolist": {{
                    "revision": 1,
                    "hash": "current",
                    "updatedAt": "2000",
                    "value": {{"items": [{{"title": "new", "status": "pending"}}]}}
                }}
            }}
        }}"#
    );

    let state: SessionAddonState =
        serde_json::from_str(&raw).expect("legacy namespace should not break addon loading");

    assert!(
        !state
            .namespaces
            .contains_key(&SessionAddonNamespace::Unknown)
    );
    assert!(state.namespace(SessionAddonNamespace::Todolist).is_some());
    let serialized = serde_json::to_string(&state).expect("serialize cleaned addon state");
    assert!(!serialized.contains("todo_plan"));
    assert!(serialized.contains("todolist"));
}

#[test]
fn session_addon_roundtrips_output_style_boundary_state() {
    let session_id = SessionId::new_v4();
    let runtime_state = SessionRuntimeStateEntry {
        output_style: SessionOutputStyleState {
            override_style_id: Some(String::from("rigorous")),
            applied_style_id: Some(String::from("balanced")),
            applied_prompt_text: Some(String::from("exact prompt")),
            applied_label: Some(String::from("Balanced")),
            applied_fingerprint: Some(42),
            applied_source_label: Some(String::from("test")),
            applied_boundary: Some(String::from("new-session")),
            applied_compaction_id: None,
            applied_at: Some(String::from("1000")),
        },
        ..SessionRuntimeStateEntry::default()
    };

    let addon =
        SessionAddonState::from_runtime_state(session_id, &runtime_state, String::from("1000"));
    let mut restored = SessionRuntimeStateEntry::default();
    addon.apply_to_runtime_state(&mut restored);
    assert_eq!(restored.output_style, runtime_state.output_style);
}

/// Verifies pre-extension provider-tool state is discarded instead of semantically migrated.
/// 验证模型扩展前的 Provider Tool 状态会被丢弃而不是执行语义迁移。
#[test]
fn session_addon_discards_pre_extension_provider_tool_state() {
    // sessionId identifies the durable addon document under test.
    // sessionId 标识当前测试的持久附属文档。
    let session_id = SessionId::new_v4();
    // raw carries schema-3 scalar and mixed-media selections that must not survive schema 4.
    // raw 携带不得进入 Schema 4 的 Schema 3 标量与混合媒体选择。
    let raw = serde_json::json!({
        "schemaVersion": 3,
        "sessionId": session_id,
        "revision": 4,
        "namespaces": {
            "runtime_controls": {
                "revision": 2,
                "hash": "stale-provider-tool-hash",
                "updatedAt": "1000",
                "value": {
                    "providerToolSelections": {
                        "media_analyze": {"connectionId": "old", "targetId": "mixed"}
                    },
                    "providerToolEnabledCapabilities": ["media_analyze"],
                    "providerExtensionTargetSets": {
                        "image_generate": [{"connectionId": "old", "targetId": "scalar"}]
                    },
                    "providerToolSettingsInitialized": true,
                    "providerToolSelectionsOverridden": true
                }
            }
        }
    });
    // state performs the only supported schema transition while deserializing.
    // state 在反序列化时执行唯一受支持的结构转换。
    let state: SessionAddonState =
        serde_json::from_value(raw).expect("schema-3 addon should load without old tool state");
    // controls is the normalized current runtime-control payload.
    // controls 是规范化后的当前运行时控制载荷。
    let controls = state
        .runtime_controls()
        .expect("runtime controls namespace should remain");
    assert_eq!(state.schema_version, SESSION_ADDON_SCHEMA_VERSION);
    assert!(controls.provider_tool_selections.is_empty());
    assert!(controls.provider_tool_enabled_capabilities.is_empty());
    assert!(controls.provider_extension_target_sets.is_empty());
    assert!(!controls.provider_tool_settings_initialized);
    assert!(!controls.provider_tool_selections_overridden);
    assert_ne!(
        state
            .namespace(SessionAddonNamespace::RuntimeControls)
            .expect("runtime controls namespace")
            .hash,
        "stale-provider-tool-hash"
    );
}
