//! Shared protocol helpers for the `call_luaskill` host dispatcher.
//! `call_luaskill` 宿主调度器的共享协议辅助模块。

use serde_json::{Map, Value, json};

/// Stable AI-visible dispatcher tool name.
/// 稳定的 AI 可见调度器工具名称。
pub const CALL_LUASKILL_TOOL_NAME: &str = "call_luaskill";

/// AI-facing usage guide bundled with this dispatcher protocol.
/// 与当前调度器协议绑定的 AI 使用说明。
pub const CALL_LUASKILL_USAGE_MD: &str = vulcan_runtime_assets::CALL_LUASKILL_USAGE_MD;

/// Lightweight target-entry view required for dispatcher argument restoration.
/// 调度参数恢复所需的轻量目标入口视图。
#[derive(Debug, Clone, Copy)]
pub struct LuaSkillCallEntryView<'a> {
    /// Canonical LuaSkills runtime entry name.
    /// LuaSkills 运行时 canonical 入口名。
    pub canonical_name: &'a str,
    /// Resolved input schema for the target entry.
    /// 目标入口解析后的输入 schema。
    pub input_schema: &'a Value,
}

/// Parsed argument payload for the `call_luaskill` dispatcher.
/// `call_luaskill` 调度器解析后的参数载荷。
#[derive(Debug, Clone)]
pub struct LuaSkillCallRequest {
    /// Complete target LuaSkills canonical tool name.
    /// 完整目标 LuaSkills canonical 工具名。
    pub name: String,
    /// Target LuaSkills entry arguments after pair-list conversion.
    /// 参数组转换后的目标 LuaSkills 入口参数。
    pub args: Value,
}

/// Returns the compact JSON schema accepted by the `call_luaskill` dispatcher.
/// 返回 `call_luaskill` 调度器接受的紧凑 JSON Schema。
pub fn call_luaskill_args_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": {
                "type": "string",
                "description": "Complete LuaSkills canonical tool name to call, such as vulcan-codekit-ast-tree."
            },
            "args": {
                "type": "array",
                "description": "Argument pairs passed to the target LuaSkills entry. Each value is a string; the host restores JSON types using the target entry schema when possible.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "LuaSkills entry argument name."
                        },
                        "value": {
                            "type": "string",
                            "description": "Argument value encoded as text. Use JSON text for object or array parameters."
                        }
                    },
                    "required": ["name", "value"]
                }
            }
        },
        "required": ["name", "args"]
    })
}

/// Parses and validates one `call_luaskill` dispatcher request.
/// 解析并校验单条 `call_luaskill` 调度请求。
pub fn parse_call_luaskill_request(args: Value) -> Result<LuaSkillCallRequest, String> {
    let Value::Object(mut object) = args else {
        return Err(String::from(
            "`call_luaskill` arguments must be a JSON object.",
        ));
    };
    let name = object
        .remove("name")
        .and_then(|value| value.as_str().map(str::trim).map(str::to_string))
        .filter(|value| !value.is_empty());
    let entry_args = parse_call_luaskill_args_value(object.remove("args"))?;
    if !entry_args.is_object() {
        return Err(String::from(
            "`call_luaskill.args` must resolve to a JSON object because LuaSkills entries use object-shaped arguments.",
        ));
    }

    match name {
        Some(name) => Ok(LuaSkillCallRequest {
            name,
            args: entry_args,
        }),
        _ => Err(String::from(
            "`call_luaskill` expects a non-empty name field.",
        )),
    }
}

/// Resolves a dispatchable LuaSkills entry by its complete canonical tool name.
/// 根据完整 canonical 工具名解析可调度的 LuaSkills 入口。
pub fn resolve_call_luaskill_entry_by_name<'a>(
    dispatch_entries: &'a [LuaSkillCallEntryView<'a>],
    name: &str,
) -> Option<LuaSkillCallEntryView<'a>> {
    let stable_name = name.trim();
    dispatch_entries
        .iter()
        .copied()
        .find(|entry| entry.canonical_name == stable_name)
}

/// Restores dispatcher string values into schema-shaped JSON values before invoking LuaSkills.
/// 在调用 LuaSkills 前，将调度器字符串值恢复为符合目标 schema 的 JSON 值。
pub fn restore_call_luaskill_args_for_entry(
    args: Value,
    entry: LuaSkillCallEntryView<'_>,
) -> Result<Value, String> {
    let Value::Object(mut object) = args else {
        return Err(format!(
            "LuaSkills entry `{}` expects object-shaped dispatcher arguments.",
            entry.canonical_name
        ));
    };
    let Some(properties) = entry
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
    else {
        return Ok(Value::Object(object));
    };

    let names = object.keys().cloned().collect::<Vec<_>>();
    for name in names {
        let Some(value) = object.remove(&name) else {
            continue;
        };
        let restored = properties
            .get(&name)
            .map(|schema| restore_call_luaskill_value(value.clone(), schema, &name))
            .transpose()?
            .unwrap_or(value);
        object.insert(name, restored);
    }
    Ok(Value::Object(object))
}

/// Parses the dispatcher `args` field from fixed name/value pairs while preserving legacy object or string input for host-side callers.
/// 从固定 name/value 参数组解析调度器 `args` 字段，同时保留宿主侧调用方的旧对象或字符串输入兼容。
fn parse_call_luaskill_args_value(args: Option<Value>) -> Result<Value, String> {
    match args {
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(json!({}));
            }
            serde_json::from_str::<Value>(trimmed).map_err(|error| {
                format!("`call_luaskill.args` must be a valid JSON object string: {error}.")
            })
        }
        Some(object @ Value::Object(_)) => Ok(object),
        Some(Value::Array(items)) => parse_call_luaskill_arg_pairs(items),
        Some(_) => Err(String::from(
            "`call_luaskill.args` must be an array of {name,value} objects.",
        )),
        None => Ok(json!({})),
    }
}

/// Converts fixed name/value dispatcher argument pairs into a JSON object with string values.
/// 将固定 name/value 调度参数组转换成字符串值 JSON 对象。
fn parse_call_luaskill_arg_pairs(items: Vec<Value>) -> Result<Value, String> {
    let mut object = Map::new();
    for (index, item) in items.into_iter().enumerate() {
        let Value::Object(mut pair) = item else {
            return Err(format!(
                "`call_luaskill.args[{index}]` must be an object with name and value strings."
            ));
        };
        let name = pair
            .remove("name")
            .and_then(|value| value.as_str().map(str::trim).map(str::to_string))
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!("`call_luaskill.args[{index}].name` must be a non-empty string.")
            })?;
        let value = pair
            .remove("value")
            .and_then(|value| value.as_str().map(str::to_string))
            .ok_or_else(|| format!("`call_luaskill.args[{index}].value` must be a string."))?;
        object.insert(name, Value::String(value));
    }
    Ok(Value::Object(object))
}

/// Restores one dispatcher argument value according to one JSON-schema property.
/// 根据单个 JSON Schema 属性恢复一条调度参数值。
fn restore_call_luaskill_value(
    value: Value,
    schema: &Value,
    field_name: &str,
) -> Result<Value, String> {
    let Value::String(text) = value else {
        return Ok(value);
    };
    match schema.get("type").and_then(Value::as_str) {
        Some("string") | None => Ok(Value::String(text)),
        Some("integer") => text
            .trim()
            .parse::<i64>()
            .map(Value::from)
            .map_err(|error| {
                format!("`call_luaskill.args.{field_name}` must be an integer: {error}.")
            }),
        Some("number") => text
            .trim()
            .parse::<f64>()
            .map(Value::from)
            .map_err(|error| {
                format!("`call_luaskill.args.{field_name}` must be a number: {error}.")
            }),
        Some("boolean") => parse_call_luaskill_bool(&text)
            .map(Value::Bool)
            .ok_or_else(|| {
                format!(
                    "`call_luaskill.args.{field_name}` must be a boolean string like true or false."
                )
            }),
        Some("object") | Some("array") => {
            let parsed = serde_json::from_str::<Value>(text.trim()).map_err(|error| {
                format!(
                    "`call_luaskill.args.{field_name}` must be valid JSON text for the target schema: {error}."
                )
            })?;
            let expected_type = schema.get("type").and_then(Value::as_str);
            if expected_type == Some("object") && !parsed.is_object() {
                return Err(format!(
                    "`call_luaskill.args.{field_name}` must decode to a JSON object."
                ));
            }
            if expected_type == Some("array") && !parsed.is_array() {
                return Err(format!(
                    "`call_luaskill.args.{field_name}` must decode to a JSON array."
                ));
            }
            Ok(parsed)
        }
        Some(_) => Ok(Value::String(text)),
    }
}

/// Parses one model-provided boolean string for dispatcher argument restoration.
/// 解析模型提供的布尔字符串，用于调度参数恢复。
fn parse_call_luaskill_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the shared LuaSkills dispatcher protocol.
    //! 共享 LuaSkills 调度协议的单元测试。

    use super::{
        LuaSkillCallEntryView, parse_call_luaskill_request, resolve_call_luaskill_entry_by_name,
        restore_call_luaskill_args_for_entry,
    };
    use serde_json::json;

    /// The shipped guide is compiled from the canonical resource and its example follows the parser.
    /// 发布指引从规范资源编译，其示例遵循实际解析器。
    #[test]
    fn bundled_usage_guide_example_matches_dispatch_protocol() {
        // Git may check out Markdown as CRLF; fence parsing must be platform independent.
        // Git 可能将 Markdown 检出为 CRLF；围栏解析必须独立于平台换行。
        let guide = super::CALL_LUASKILL_USAGE_MD.replace("\r\n", "\n");
        assert!(guide.contains("`luaskill_help`") && guide.contains("complete canonical"));
        let example = guide
            .split_once("```json\n")
            .expect("documented JSON example")
            .1
            .split_once("\n```")
            .expect("closed JSON example")
            .0;
        let request =
            parse_call_luaskill_request(serde_json::from_str(example).expect("valid example JSON"))
                .expect("documented request is accepted");
        let schema = json!({"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}}});
        let canonical_name = request.name.clone();
        let restored = restore_call_luaskill_args_for_entry(
            request.args,
            LuaSkillCallEntryView {
                canonical_name: &canonical_name,
                input_schema: &schema,
            },
        )
        .expect("documented types are restored");
        assert_eq!(restored["limit"], 4);
        assert_eq!(restored["query"], "source ownership");
    }

    /// Verifies the dispatcher resolves entries by complete canonical tool names only.
    /// 验证调度器只通过完整 canonical 工具名解析入口。
    #[test]
    fn dispatcher_entry_resolves_complete_canonical_names() {
        let schema = json!({ "type": "object" });
        let entries = vec![LuaSkillCallEntryView {
            canonical_name: "skill-a-tool-a",
            input_schema: &schema,
        }];

        assert_eq!(
            resolve_call_luaskill_entry_by_name(&entries, "skill-a-tool-a")
                .map(|entry| entry.canonical_name),
            Some("skill-a-tool-a")
        );
        assert!(resolve_call_luaskill_entry_by_name(&entries, "tool-a").is_none());
    }

    /// Verifies dispatcher argument pairs parse into object-shaped target arguments.
    /// 验证调度器参数组会解析为对象形目标参数。
    #[test]
    fn dispatcher_request_parses_argument_pairs() {
        let parsed = parse_call_luaskill_request(json!({
            "name": "skill-a-tool-a",
            "args": [
                {
                    "name": "value",
                    "value": "ok"
                }
            ]
        }))
        .expect("dispatcher request should parse");

        assert_eq!(parsed.name, "skill-a-tool-a");
        assert_eq!(parsed.args["value"], "ok");
    }

    /// Verifies dispatcher argument pairs are restored into JSON types using the target entry schema.
    /// 验证调度器参数组会根据目标入口 schema 恢复成 JSON 类型。
    #[test]
    fn dispatcher_restores_schema_typed_values() {
        let schema = json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" },
                "strict": { "type": "boolean" },
                "headers": { "type": "object" },
                "items": { "type": "array" },
                "name": { "type": "string" }
            }
        });
        let parsed = parse_call_luaskill_request(json!({
            "name": "skill-a-tool-a",
            "args": [
                { "name": "count", "value": "42" },
                { "name": "strict", "value": "true" },
                { "name": "headers", "value": "{\"Accept\":\"application/json\"}" },
                { "name": "items", "value": "[\"a\",\"b\"]" },
                { "name": "name", "value": "demo" }
            ]
        }))
        .expect("dispatcher request should parse");
        let entry = LuaSkillCallEntryView {
            canonical_name: "skill-a-tool-a",
            input_schema: &schema,
        };

        let restored = restore_call_luaskill_args_for_entry(parsed.args, entry)
            .expect("typed args should restore");

        assert_eq!(restored["count"], 42);
        assert_eq!(restored["strict"], true);
        assert_eq!(restored["headers"]["Accept"], "application/json");
        assert_eq!(restored["items"][0], "a");
        assert_eq!(restored["name"], "demo");
    }
}
