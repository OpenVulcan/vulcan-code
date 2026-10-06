use super::*;
use serde_json::json;

/// action builds one declared action with the given effect and the plugin UI caller.
/// action 构建一个具有给定效果与 plugin UI 调用方的已声明 Action。
fn action(id: &str, effect: VulcanPluginActionEffect) -> VulcanPluginActionManifest {
    serde_yaml::from_value(
        serde_yaml::to_value(json!({
            "id": id,
            "function": id.replace('-', "_"),
            "timeout_ms": 1000,
            "callers": ["plugin_ui"],
            "effect": if effect == VulcanPluginActionEffect::Read { "read" } else { "mutation" },
        }))
        .expect("action value"),
    )
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
            action("stats-get", VulcanPluginActionEffect::Read),
        ];
        let messages = [
            "title", "label", "hint", "fast", "mode", "mode_a", "mode_b", "routes", "group", "key",
            "test", "docs", "tab_a", "tab_b", "ready", "down", "repo", "license", "range", "empty",
            "daily", "saved", "top",
        ]
        .into_iter()
        .map(|key| (key.to_owned(), format!("{key} text")))
        .collect();
        let mut messages: BTreeMap<String, String> = messages;
        messages.insert("top".into(), "Top {{count}}".into());
        Self { actions, messages }
    }

    fn context(&self) -> SettingsLayoutContext<'_> {
        let schemas = |id: &str| match id {
            "credential-status" => (
                None,
                Some(json!({"type": "object", "properties": {"configured": {"type": "boolean"}}})),
            ),
            "credential-set" => (
                Some(
                    json!({"type": "object", "properties": {"apiKey": {"type": "string"}}, "required": ["apiKey"]}),
                ),
                None,
            ),
            "stats-get" => (
                Some(
                    json!({"type": "object", "properties": {"days": {"type": "integer", "enum": [7, 30]}}, "required": ["days"], "additionalProperties": false}),
                ),
                Some(json!({
                    "type": "object",
                    "$defs": {"row": {"type": "object", "properties": {"name": {"type": "string"}, "saved": {"type": "integer"}}, "additionalProperties": false}},
                    "properties": {
                        "available": {"type": "boolean"},
                        "error": {"type": ["string", "null"]},
                        "summary": {"type": "object"},
                        "quota": {"type": "object", "properties": {"left": {"type": ["integer", "null"]}}, "additionalProperties": false},
                        "daily": {"type": "array"},
                        "rows": {"type": "array", "items": {"$ref": "#/$defs/row"}}
                    },
                    "additionalProperties": false
                })),
            ),
            _ => (None, None),
        };
        SettingsLayoutContext {
            actions: self
                .actions
                .iter()
                .map(|manifest| {
                    let (input, output) = schemas(&manifest.id);
                    (
                        manifest.id.as_str(),
                        SettingsActionContract {
                            manifest,
                            input,
                            output,
                        },
                    )
                })
                .collect(),
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
            text_assets: ["LICENSE"].into_iter().collect(),
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

/// One in-place mutation applied to a valid layout document.
/// 作用于有效版面文档的一次原地修改。
type LayoutMutation = fn(&mut serde_json::Value);

/// Verifies each semantic rule reports its own violation.
/// 验证每条语义规则都报告自身的违规。
#[test]
fn rejects_semantic_violations() {
    // cases pairs one mutation of the valid layout with a fragment of the expected error.
    // cases 将对有效版面的一次修改与预期错误片段配对。
    let cases: Vec<(LayoutMutation, &str)> = vec![
        (|value| value["schema_version"] = json!(2), "schema_version"),
        (
            |value| value["sections"][0]["items"][0]["key"] = json!("absent"),
            "not a top-level configuration property",
        ),
        (
            |value| value["sections"][0]["items"][0]["control"] = json!("text"),
            "does not match its schema type",
        ),
        (
            |value| value["sections"][0]["items"][1]["options"][0]["value"] = json!("c"),
            "Invalid plugin settings mount",
        ),
        (
            |value| value["sections"][0]["items"][2]["options"][0]["value"] = json!(5),
            "Invalid plugin settings mount",
        ),
        (
            |value| {
                value["sections"][0]["items"][3]["option_groups"][0]["values"][0] = json!("Git!")
            },
            "Invalid plugin settings mount",
        ),
        (
            |value| {
                value["sections"][0]["items"][3]["visible_when"]["key"] = json!("timeout-absent")
            },
            "undeclared field",
        ),
        (
            |value| value["sections"][0]["title_message_key"] = json!("missing"),
            "message key `missing`",
        ),
        (
            |value| value["sections"][1]["links"][0]["url"] = json!("http://example.com"),
            "absolute HTTPS URL",
        ),
        (
            |value| value["config"]["write_action"] = json!("settings-get"),
            "effect `mutation`",
        ),
        (
            |value| value["sections"][1]["items"][0]["set_argument"] = json!("token"),
            "exactly one required string property",
        ),
        (
            |value| value["sections"][1]["items"][0]["status_action"] = json!("settings-get"),
            "boolean `configured`",
        ),
        (
            |value| value["sections"][1]["items"][1]["id"] = json!("api-key"),
            "duplicate item id",
        ),
        (
            |value| {
                value.as_object_mut().expect("object").remove("config");
            },
            "require a `config` binding",
        ),
        (
            |value| value["sections"][0]["items"][0]["options"] = json!([{"value": "x"}]),
            "options exactly",
        ),
    ];
    for (mutate, expected) in cases {
        let mut value = valid();
        mutate(&mut value);
        let error = check(value).expect_err(expected);
        assert!(
            error.contains(expected),
            "expected `{expected}` in `{error}`"
        );
    }
}

/// Verifies the projection key of every mount kind is exactly its manifest spelling.
/// 验证每种挂载类型的投影键与其清单拼写完全一致。
#[test]
fn mount_kind_keys_match_manifest_spelling() {
    for kind in [
        super::super::VulcanPluginUiMountKind::Module,
        super::super::VulcanPluginUiMountKind::Settings,
    ] {
        let decoded: super::super::VulcanPluginUiMountKind =
            serde_json::from_value(json!(kind.key())).expect("kind key");
        assert_eq!(decoded, kind);
    }
}

/// paged returns the valid layout extended with a header, two tabs, an inline section and a data section.
/// paged 返回在有效版面上加入标题区、两个标签、行内区块与数据区块的版面。
fn paged() -> serde_json::Value {
    let mut value = valid();
    value["sections"][0]["items"]
        .as_array_mut()
        .expect("items")
        .remove(0);
    value["sections"].as_array_mut().expect("sections").extend([
        json!({"id": "toggles", "title_message_key": "title", "layout": "inline",
               "items": [{"type": "field", "key": "fast", "control": "switch", "label_message_key": "fast", "description_message_key": "hint"}]}),
        json!({"id": "stats", "title_message_key": "title",
               "data": {"action": "stats-get", "refreshable": true, "show_loaded_at": true,
                        "available_path": "available", "error_path": "error", "unavailable_message_key": "down",
                        "refresh_topics": ["stats.changed"],
                        "arguments": [{"name": "days", "label_message_key": "range", "value_format": "days", "default": 30,
                                       "options": [{"value": 7}, {"value": 30}]}]},
               "blocks": [
                   {"type": "metrics", "web": {"variant": "tiles", "columns": 4}, "items": [
                       {"label_message_key": "saved", "path": "summary.total", "format": "compact"},
                       {"label_message_key": "label", "ratio": {"numerator": "summary.input", "denominator": "summary.output"}, "format": "ratio"}]},
                   {"type": "metrics", "title_message_key": "title", "help_message_key": "hint", "source_path": "quota",
                    "empty_message_key": "empty", "items": [{"label_message_key": "label", "path": "left"}]},
                   {"type": "chart", "value_label_message_key": "saved", "value_path": "saved_tokens", "value_format": "compact",
                    "empty_message_key": "empty", "web": {"variant": "bars"}, "tui": {"variant": "sparkline"},
                    "periods": [{"id": "daily", "label_message_key": "daily", "path": "daily", "label_paths": ["date"], "label_format": "date"}]},
                   {"type": "table", "path": "rows", "title_message_key": "title", "count_message_key": "top", "rank": true,
                    "empty_message_key": "empty", "columns": [
                        {"path": "name", "label_message_key": "label", "format": "code"},
                        {"path": "saved", "label_message_key": "saved", "format": "bar"}]}
               ]}),
    ]);
    value["tabs"] = json!([
        {"id": "settings", "title_message_key": "tab_a", "sections": ["behavior", "auth", "toggles"]},
        {"id": "statistics", "title_message_key": "tab_b", "sections": ["stats"]}
    ]);
    value["header"] = json!({
        "status": {"ready_message_key": "ready", "unavailable_message_key": "down"},
        "links": [{"url": "https://example.com/repo", "label_message_key": "repo", "icon": "github"}],
        "documents": [{"id": "license", "asset": "LICENSE", "label_message_key": "license", "title_message_key": "license", "icon": "license"}]
    });
    value
}

/// Verifies a tabbed layout with a header, an inline section and every data block kind is accepted.
/// 验证带标题区、行内区块与全部数据块类型的分页版面被接受。
#[test]
fn accepts_paged_layout_with_data_views() {
    check(paged()).expect("paged layout");
}

/// Verifies page chrome and data views report their own violations.
/// 验证页面框架与数据视图报告各自的违规。
#[test]
fn rejects_page_and_data_violations() {
    // stats addresses the data section appended last by `paged`.
    // stats 指向 `paged` 最后追加的数据区块。
    fn stats(value: &mut serde_json::Value) -> &mut serde_json::Value {
        value["sections"]
            .as_array_mut()
            .expect("sections")
            .last_mut()
            .expect("stats section")
    }
    let cases: Vec<(LayoutMutation, &str)> = vec![
        (
            |value| value["tabs"][1]["sections"] = json!(["stats", "behavior"]),
            "more than one tab",
        ),
        (
            |value| value["tabs"][0]["sections"] = json!(["behavior"]),
            "every section must belong to a tab",
        ),
        (
            |value| value["tabs"][1]["sections"] = json!(["absent"]),
            "undeclared section",
        ),
        (
            |value| value["header"]["documents"][0]["asset"] = json!("NOTICE"),
            "required_files",
        ),
        (
            |value| {
                value.as_object_mut().expect("object").remove("config");
                value["sections"]
                    .as_array_mut()
                    .expect("sections")
                    .retain(|section| section["id"] == "stats");
                value["tabs"] = json!([]);
            },
            "header status requires",
        ),
        (
            |value| value["sections"][2]["items"][0]["control"] = json!("select"),
            "Plugin settings mount",
        ),
        (
            |value| stats(value)["data"]["action"] = json!("settings-update"),
            "effect `read`",
        ),
        (
            |value| stats(value)["data"]["arguments"][0]["default"] = json!(90),
            "Invalid plugin settings mount",
        ),
        (
            |value| stats(value)["data"]["arguments"][0]["options"][1]["value"] = json!(365),
            "Invalid plugin settings mount",
        ),
        (
            |value| stats(value)["data"]["available_path"] = json!("error"),
            "format cannot show",
        ),
        (
            |value| stats(value)["blocks"][1]["items"][0]["path"] = json!("right"),
            "not in the action output schema",
        ),
        (
            |value| stats(value)["blocks"][3]["columns"][0]["path"] = json!("missing"),
            "not in the action output schema",
        ),
        (
            |value| stats(value)["blocks"][3]["columns"][1]["path"] = json!("name"),
            "format cannot show",
        ),
        (
            |value| stats(value)["blocks"][2]["periods"][0]["path"] = json!("summary"),
            "format cannot show",
        ),
        (
            |value| stats(value)["blocks"][0]["web"]["variant"] = json!("line"),
            "WebUI variant",
        ),
        (
            |value| stats(value)["blocks"][3]["tui"]["variant"] = json!("table"),
            "applies only to charts",
        ),
        (
            |value| stats(value)["blocks"][0]["items"][0]["format"] = json!("bar"),
            "not valid for a path value",
        ),
        (
            |value| stats(value)["blocks"][2]["limit"] = json!(0),
            "chart limit",
        ),
        (
            |value| stats(value)["blocks"] = json!([]),
            "at least one block",
        ),
        (
            |value| stats(value)["items"] = json!([{"type": "action", "id": "x", "action": "connection-test", "label_message_key": "test"}]),
            "may not declare items",
        ),
        (
            |value| stats(value)["blocks"][1]["items"][0]["path"] = json!("left.0"),
            "dot-separated identifiers",
        ),
        (
            |value| stats(value)["blocks"][3]["count_message_key"] = json!("saved"),
            "must contain `{{count}}`",
        ),
    ];
    for (mutate, expected) in cases {
        let mut value = paged();
        mutate(&mut value);
        let error = check(value).expect_err(expected);
        assert!(
            error.contains(expected),
            "expected `{expected}` in `{error}`"
        );
    }
}
