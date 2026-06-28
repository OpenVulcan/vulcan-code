use super::*;
use vulcan_provider_config::config::PersistedAiToolConfiguration;

#[test]
fn session_addon_revision_changes_only_when_namespace_value_changes() {
    let session_id = SessionId::new_v4();
    let mut state = SessionAddonState::new(session_id);
    let configuration = PersistedAiToolConfiguration::default();

    assert!(state.set_ai_tool_configuration(&configuration, String::from("1000")));
    assert_eq!(state.revision, 1);
    let namespace_revision = state
        .namespace(SessionAddonNamespace::AiCapabilities)
        .map(|entry| entry.revision);

    assert!(!state.set_ai_tool_configuration(&configuration, String::from("2000")));
    assert_eq!(state.revision, 1);
    assert_eq!(
        state
            .namespace(SessionAddonNamespace::AiCapabilities)
            .map(|entry| entry.revision),
        namespace_revision
    );
}

#[test]
fn session_addon_migrates_ai_configuration_and_runtime_controls() {
    let session_id = SessionId::new_v4();
    let runtime_state = SessionRuntimeStateEntry {
        active_provider_id: Some(String::from("minimax")),
        active_model_id: Some(String::from("minimax-text")),
        active_work_mode: Some(String::from("plan")),
        ai_tool_configuration: Some(PersistedAiToolConfiguration::default()),
        ..SessionRuntimeStateEntry::default()
    };

    let addon_state =
        SessionAddonState::from_runtime_state(session_id, &runtime_state, None, String::from("1"));

    assert_eq!(
        addon_state
            .runtime_controls()
            .and_then(|controls| controls.active_provider_id),
        Some(String::from("minimax"))
    );
    assert!(addon_state.ai_tool_configuration().is_some());
    assert_eq!(
        addon_state
            .work_mode()
            .map(|work_mode| work_mode.execution_mode),
        Some(String::from("plan"))
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
