use super::*;
use crate::tooling::native::{
    build_memory_outline_tool, build_memory_search_tool, build_memory_write_tool,
    resolve_markdown_memory_access_in,
};
use std::collections::BTreeSet;
use vulcan_markdown_memory::{
    MarkdownMemorySettings, MarkdownMemoryStore, MemoryMode, WriteOperation,
};

/// Memory is reachable only in `markdown` mode and never for a project whose memory is off.
/// 记忆只在 `markdown` 模式下可达，且记忆关闭的项目永远不可达。
#[test]
fn access_follows_mode_and_project_switch() {
    let root = tempfile::tempdir().expect("temp root");
    let workspace = root.path().join("repo");
    let resolve = |host: Option<&str>| {
        resolve_markdown_memory_access_in(root.path(), host, Some(workspace.as_path()))
            .expect("resolve")
    };
    assert!(resolve(Some("host-1")).is_none());

    let store = MarkdownMemoryStore::new(root.path());
    store
        .save_settings(&MarkdownMemorySettings {
            mode: MemoryMode::Markdown,
            ..MarkdownMemorySettings::default()
        })
        .expect("save settings");
    let access = resolve(Some("host-1")).expect("markdown mode is active");
    assert_eq!(access.context.host_project_id.as_deref(), Some("host-1"));
    assert!(resolve(None).is_some());

    store
        .update_registry(|registry| {
            let id = registry.create_for_host(
                "host-1",
                &workspace.display().to_string(),
                None,
                "2026-10-20T00:00Z",
            );
            registry.set_enabled(&id, false, "2026-10-20T00:00Z")
        })
        .expect("disable project");
    assert!(resolve(Some("host-1")).is_none());
    assert!(resolve(Some("host-2")).is_some());
}

/// Every property of the write schema is accepted by some action, and examples using them parse.
/// 写入 Schema 的每个属性都被某个动作接受，使用这些属性的示例均可解析。
#[test]
fn write_schema_matches_operation_fields() {
    let schema = build_memory_write_tool().args_schema.clone();
    let declared = schema["properties"]["operations"]["items"]["properties"]
        .as_object()
        .expect("item properties")
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let examples = [
        json!({"action": "create", "scope": "space", "section": "base", "title": "T", "body": "B"}),
        json!({"action": "create", "scope": "user", "parentId": "m-1", "title": "T"}),
        json!({"action": "create", "scope": "project", "newSection": true, "title": "T"}),
        json!({"action": "replace", "id": "m-1", "expectedRevision": "r", "body": "B"}),
        json!({"action": "append", "id": "m-1", "expectedRevision": "r", "text": "B"}),
        json!({"action": "rename", "id": "m-1", "expectedRevision": "r", "title": "T"}),
        json!({"action": "move", "id": "m-1", "expectedRevision": "r", "scope": "space", "section": "base"}),
        json!({"action": "delete", "id": "m-1", "expectedRevision": "r", "withChildren": true}),
    ];
    let mut used = BTreeSet::new();
    for example in examples {
        used.extend(example.as_object().expect("object").keys().cloned());
        serde_json::from_value::<WriteOperation>(example.clone())
            .unwrap_or_else(|error| panic!("{example} should parse: {error}"));
    }
    assert_eq!(used, declared);
    let actions = schema["properties"]["operations"]["items"]["properties"]["action"]["enum"]
        .as_array()
        .expect("action enum")
        .len();
    assert_eq!(actions, 6);
}

/// Reads stay available in Plan mode while writes are hidden there.
/// 读取在计划模式下保持可用，写入在计划模式下隐藏。
#[test]
fn only_reads_are_available_in_plan_mode() {
    assert!(build_memory_outline_tool().descriptor.available_in_plan);
    assert!(build_memory_search_tool().descriptor.available_in_plan);
    assert!(!build_memory_write_tool().descriptor.available_in_plan);
}
