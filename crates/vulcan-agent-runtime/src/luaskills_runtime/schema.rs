//! LuaSkills runtime entry schema projection helpers.
//! LuaSkills 运行时入口 schema 投影辅助逻辑。

use super::*;

/// Build one JSON-Schema object definition from the LuaSkills runtime entry parameter descriptors.
/// 根据 LuaSkills 运行时入口参数描述构建一份 JSON-Schema 对象定义。
pub fn luaskills_entry_args_schema(
    entry: &RuntimeEntryDescriptor,
    hide_host_managed_sid: bool,
    hide_host_managed_pwd: bool,
) -> Value {
    let mut schema = entry.input_schema.clone();
    if hide_host_managed_sid {
        hide_reserved_luaskills_property_from_visible_schema(
            &mut schema,
            LUASKILL_SID_PARAMETER_NAME,
        );
    }
    if hide_host_managed_pwd {
        hide_reserved_luaskills_property_from_visible_schema(&mut schema, PWD_PARAMETER_NAME);
    }
    schema
}

/// Stable LuaSkills reserved managed-identity parameter name used by host projections.
/// 宿主投影使用的稳定 LuaSkills 保留托管身份参数名称。
pub const LUASKILL_SID_PARAMETER_NAME: &str = "LUASKILL_SID";

/// Stable LuaSkills reserved project-path parameter name used by host projections.
/// 宿主投影使用的稳定 LuaSkills 保留项目路径参数名称。
pub const PWD_PARAMETER_NAME: &str = "PWD";

/// Stable host-managed prefix reserved by LuaSkills for injected managed identities.
/// LuaSkills 为注入托管身份保留的稳定宿主管理前缀。
pub const LUASKILLS_HOST_MANAGED_SID_PREFIX: &str = "LUASKILLS-SID-";

/// Hide one reserved host-managed property from one visible LuaSkills input schema clone.
/// 从单份可见 LuaSkills 输入 schema 副本中隐藏一个保留宿主管理属性。
fn hide_reserved_luaskills_property_from_visible_schema(schema: &mut Value, field_name: &str) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    let Some(properties_value) = object.get_mut("properties") else {
        return;
    };
    let Some(properties) = properties_value.as_object_mut() else {
        return;
    };
    if properties.remove(field_name).is_none() {
        return;
    }
    remove_required_name_from_schema(schema, field_name);
}

/// Remove one top-level required-field name from one visible LuaSkills input schema clone.
/// 从单份可见 LuaSkills 输入 schema 副本中移除一个顶层必填字段名。
fn remove_required_name_from_schema(schema: &mut Value, field_name: &str) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    let Some(required_value) = object.get_mut("required") else {
        return;
    };
    let Some(required) = required_value.as_array_mut() else {
        return;
    };
    required.retain(|value| value.as_str() != Some(field_name));
    if required.is_empty() {
        object.remove("required");
    }
}

#[cfg(test)]
mod tests {
    use super::luaskills_entry_args_schema;
    use super::{LUASKILL_SID_PARAMETER_NAME, PWD_PARAMETER_NAME};
    use luaskills::{RuntimeEntryDescriptor, RuntimeEntryParameterDescriptor};
    use serde_json::json;

    #[test]
    fn luaskills_schema_returns_resolved_input_schema_verbatim() {
        let entry = build_entry(
            "vulcan-codekit",
            "node-source",
            vec![RuntimeEntryParameterDescriptor {
                name: String::from("nodes"),
                param_type: String::from("array"),
                description: String::from("Required node input."),
                required: true,
            }],
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "nodes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "file": { "type": "string" },
                                "structural_path": { "type": "string" }
                            },
                            "required": ["file", "structural_path"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["nodes"]
            }),
        );

        let schema = luaskills_entry_args_schema(&entry, false, false);

        assert_eq!(schema, entry.input_schema);
    }

    #[test]
    fn luaskills_schema_hides_managed_sid_when_requested() {
        let entry = build_entry(
            "vulcan-stateful",
            "run",
            vec![
                RuntimeEntryParameterDescriptor {
                    name: String::from(LUASKILL_SID_PARAMETER_NAME),
                    param_type: String::from("string"),
                    description: String::from("managed sid"),
                    required: false,
                },
                RuntimeEntryParameterDescriptor {
                    name: String::from("args"),
                    param_type: String::from("array"),
                    description: String::from("argument array"),
                    required: false,
                },
            ],
            json!({
                "type": "object",
                "properties": {
                    "LUASKILL_SID": { "type": "string" },
                    "args": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["LUASKILL_SID"]
            }),
        );

        let schema = luaskills_entry_args_schema(&entry, true, false);

        assert_eq!(
            schema,
            json!({
                "type": "object",
                "properties": {
                    "args": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                }
            })
        );
    }

    #[test]
    fn luaskills_schema_hides_managed_pwd_when_requested() {
        let entry = build_entry(
            "vulcan-codekit",
            "run",
            vec![
                RuntimeEntryParameterDescriptor {
                    name: String::from(PWD_PARAMETER_NAME),
                    param_type: String::from("string"),
                    description: String::from("workspace root"),
                    required: false,
                },
                RuntimeEntryParameterDescriptor {
                    name: String::from("query"),
                    param_type: String::from("string"),
                    description: String::from("search query"),
                    required: false,
                },
            ],
            json!({
                "type": "object",
                "properties": {
                    "PWD": { "type": "string" },
                    "query": { "type": "string" }
                },
                "required": ["PWD", "query"]
            }),
        );

        let schema = luaskills_entry_args_schema(&entry, false, true);

        assert_eq!(
            schema,
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            })
        );
    }

    fn build_entry(
        skill_id: &str,
        local_name: &str,
        parameters: Vec<RuntimeEntryParameterDescriptor>,
        input_schema: serde_json::Value,
    ) -> RuntimeEntryDescriptor {
        RuntimeEntryDescriptor {
            canonical_name: format!("{skill_id}-{local_name}"),
            skill_id: skill_id.to_string(),
            local_name: local_name.to_string(),
            root_name: String::from("ROOT"),
            skill_dir: String::from("skill-dir"),
            description: String::from("entry"),
            parameters,
            input_schema,
        }
    }
}
