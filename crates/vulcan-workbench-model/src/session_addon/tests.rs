use super::*;
use crate::SessionOutputStyleState;
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
        active_model_id: Some(String::from("minimax-text")),
        active_reasoning_effort: Some(ReasoningEffortLevel::None),
        active_reasoning_effort_explicit: true,
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
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.output_style = SessionOutputStyleState {
        override_style_id: Some(String::from("rigorous")),
        applied_style_id: Some(String::from("balanced")),
        applied_prompt_text: Some(String::from("exact prompt")),
        applied_label: Some(String::from("Balanced")),
        applied_fingerprint: Some(42),
        applied_source_label: Some(String::from("test")),
        applied_boundary: Some(String::from("new-session")),
        applied_compaction_id: None,
        applied_at: Some(String::from("1000")),
    };

    let addon =
        SessionAddonState::from_runtime_state(session_id, &runtime_state, String::from("1000"));
    let mut restored = SessionRuntimeStateEntry::default();
    addon.apply_to_runtime_state(&mut restored);
    assert_eq!(restored.output_style, runtime_state.output_style);
}
