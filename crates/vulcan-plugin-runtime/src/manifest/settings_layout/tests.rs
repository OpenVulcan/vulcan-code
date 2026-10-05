use super::*;
use serde_json::json;

/// action builds one declared action with the given effect and the plugin UI caller.
/// action 构建一个具有给定效果与 plugin UI 调用方的已声明 Action。
fn action(id: &str, effect: VulcanPluginActionEffect) -> VulcanPluginActionManifest {
    serde_yaml::from_value(serde_yaml::to_value(json!({
        "id": id,
        "function": id.replace('-', "_"),
        "timeout_ms": 1000,
        "callers": ["plugin_ui"],
        "effect": if effect == VulcanPluginActionEffect::Read { "read" } else { "mutation" },
    })).expect("action value"))
    .expect("action manifest")
}

/// Fixture owns the declared actions and messages borrowed by a validation context.
/// Fixture 持有校验上下文借用的已声明 Action 与文案。
struct Fixture {
    actions: Vec<VulcanPluginActionManifest>,
    messages: BTreeMap<String, String>,
}

impl Fixture {
    fn new() -> Self {
        let actions = vec![
            action("settings-get", VulcanPluginActionEffect::Read),
            action("settings-update", VulcanPluginActionEffect::Mutation),
            action("credential-status", VulcanPluginActionEffect::Read),
            action("credential-set", VulcanPluginActionEffect::Mutation),
            action("credential-delete", VulcanPluginActionEffect::Mutation),
            action("connection-test", VulcanPluginActionEffect::Mutation),
        ];
        let messages = ["title", "label", "hint", "fast", "mode", "mode_a", "mode_b", "routes", "group", "key", "test", "docs"]
            .into_iter()
            .map(|key| (key.to_owned(), format!("{key} text")))
            .collect();
        Self { actions, messages }
    }

    fn context(&self) -> SettingsLayoutContext<'_> {
        let schemas = |id: &str| match id {
            "credential-status" => (None, Some(json!({"type": "object", "properties": {"configured": {"type": "boolean"}}}))),
            "credential-set" => (Some(json!({"type": "object", "properties": {"apiKey": {"type": "string"}}, "required": ["apiKey"]})), None),
            _ => (None, None),
        };
        SettingsLayoutContext {
            actions: self.actions.iter().map(|manifest| {
                let (input, output) = schemas(&manifest.id);
                (manifest.id.as_str(), SettingsActionContract { manifest, input, output })
            }).collect(),
            config_schema: Some(json!({
                "type": "object",
                "properties": {
                    "fast": {"type": "boolean"},
                    "mode": {"type": "string", "enum": ["a", "b"]},
                    "timeout": {"type": "integer", "minimum": 10, "maximum": 60},
                    "routes": {"type": "array", "items": {"type": "string", "pattern": "^[a-z]+$"}}
                }
            })),
            messages: &self.messages,
        }
    }
}

/// layout decodes one layout document, failing the test when the shape itself is invalid.
/// layout 解码一份版面文档；结构本身无效时测试失败。
fn layout(value: serde_json::Value) -> VulcanPluginSettingsLayout {
    serde_json::from_value(value).expect("layout shape")
}

/// valid returns a layout that exercises every item and control kind.
/// valid 返回覆盖全部条目与控件类型的版面。
fn valid() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "config": {"read_action": "settings-get", "write_action": "settings-update"},
        "sections": [
            {
                "id": "behavior",
                "title_message_key": "title",
                "items": [
                    {"type": "field", "key": "fast", "control": "switch", "label_message_key": "fast"},
                    {"type": "field", "key": "mode", "control": "select", "label_message_key": "mode",
                     "options": [{"value": "a", "label_message_key": "mode_a"}, {"value": "b", "label_message_key": "mode_b"}]},
                    {"type": "field", "key": "timeout", "control": "select", "label_message_key": "label",
                     "value_format": "duration_ms", "options": [{"value": 10}, {"value": 60}]},
                    {"type": "field", "key": "routes", "control": "checklist", "label_message_key": "routes",
                     "selection": "exclude", "visible_when": {"key": "mode", "values": ["b"]},
                     "option_groups": [{"id": "main", "title_message_key": "group", "values": ["git", "ls"]}]}
                ]
            },
            {
                "id": "auth",
                "title_message_key": "title",
                "links": [{"url": "https://example.com/docs", "label_message_key": "docs"}],
                "items": [
                    {"type": "credential", "id": "api-key", "label_message_key": "key", "status_action": "credential-status",
                     "set_action": "credential-set", "set_argument": "apiKey", "delete_action": "credential-delete"},
                    {"type": "action", "id": "test", "action": "connection-test", "label_message_key": "test"}
                ]
            }
        ]
    })
}

/// check validates value against the shared fixture context.
/// check 依据共享夹具上下文校验 value。
fn check(value: serde_json::Value) -> Result<(), String> {
    let fixture = Fixture::new();
    validate_settings_layout("settings", &layout(value), &fixture.context())
}

/// Verifies a layout covering every item and control kind is accepted.
/// 验证覆盖全部条目与控件类型的版面被接受。
#[test]
fn accepts_complete_layout() {
    check(valid()).expect("valid layout");
}

/// Verifies unknown keys are rejected at every level, including inside tagged items.
/// 验证每一层（包括带标签条目内部）的未知键都被拒绝。
#[test]
fn rejects_unknown_keys() {
    let mut value = valid();
    value["sections"][0]["items"][0]["style"] = json!("color: red");
    assert!(serde_json::from_value::<VulcanPluginSettingsLayout>(value).is_err());
    let mut value = valid();
    value["sections"][0]["className"] = json!("custom");
    assert!(serde_json::from_value::<VulcanPluginSettingsLayout>(value).is_err());
}

/// Verifies each semantic rule reports its own violation.
/// 验证每条语义规则都报告自身的违规。
#[test]
fn rejects_semantic_violations() {
    // cases pairs one mutation of the valid layout with a fragment of the expected error.
    // cases 将对有效版面的一次修改与预期错误片段配对。
    let cases: Vec<(fn(&mut serde_json::Value), &str)> = vec![
        (|value| value["schema_version"] = json!(2), "schema_version"),
        (|value| value["sections"][0]["items"][0]["key"] = json!("absent"), "not a top-level configuration property"),
        (|value| value["sections"][0]["items"][0]["control"] = json!("text"), "does not match its schema type"),
        (|value| value["sections"][0]["items"][1]["options"][0]["value"] = json!("c"), "Invalid plugin settings mount"),
        (|value| value["sections"][0]["items"][2]["options"][0]["value"] = json!(5), "Invalid plugin settings mount"),
        (|value| value["sections"][0]["items"][3]["option_groups"][0]["values"][0] = json!("Git!"), "Invalid plugin settings mount"),
        (|value| value["sections"][0]["items"][3]["visible_when"]["key"] = json!("timeout-absent"), "undeclared field"),
        (|value| value["sections"][0]["title_message_key"] = json!("missing"), "message key `missing`"),
        (|value| value["sections"][1]["links"][0]["url"] = json!("http://example.com"), "absolute HTTPS URL"),
        (|value| value["config"]["write_action"] = json!("settings-get"), "effect `mutation`"),
        (|value| value["sections"][1]["items"][0]["set_argument"] = json!("token"), "exactly one required string property"),
        (|value| value["sections"][1]["items"][0]["status_action"] = json!("settings-get"), "boolean `configured`"),
        (|value| value["sections"][1]["items"][1]["id"] = json!("api-key"), "duplicate item id"),
        (|value| { value.as_object_mut().expect("object").remove("config"); }, "require a `config` binding"),
        (|value| value["sections"][0]["items"][0]["options"] = json!([{"value": "x"}]), "options exactly"),
    ];
    for (mutate, expected) in cases {
        let mut value = valid();
        mutate(&mut value);
        let error = check(value).expect_err(expected);
        assert!(error.contains(expected), "expected `{expected}` in `{error}`");
    }
}

/// Verifies the projection key of every mount kind is exactly its manifest spelling.
/// 验证每种挂载类型的投影键与其清单拼写完全一致。
#[test]
fn mount_kind_keys_match_manifest_spelling() {
    for kind in [super::super::VulcanPluginUiMountKind::Module, super::super::VulcanPluginUiMountKind::Settings] {
        let decoded: super::super::VulcanPluginUiMountKind = serde_json::from_value(json!(kind.key())).expect("kind key");
        assert_eq!(decoded, kind);
    }
}
