use super::*;

/// Resolves the schema projection profile required for the current provider request context.
/// 解析当前 Provider 请求上下文所需的工具 schema 投影配置。
pub fn resolve_tool_schema_projection_profile(
    context: &VulcanToolSchemaProjectionContext<'_>,
) -> VulcanToolSchemaProjectionProfile {
    // targetDialect takes precedence over the outer OpenAI-compatible envelope because Gemini validates the final function declaration.
    // targetDialect 优先于外层 OpenAI 兼容封装，因为 Gemini 校验的是最终函数声明。
    if context
        .api_family_key
        .eq_ignore_ascii_case("google-gemini-function-declarations")
        || openai_compatible_request_targets_gemini(context)
    {
        return VulcanToolSchemaProjectionProfile::GeminiSlim;
    }

    VulcanToolSchemaProjectionProfile::PassThrough
}

/// Projects one tool-parameter schema into the selected provider-facing dialect.
/// 将单个工具参数 Schema 投影到选定的 Provider 可见方言。
pub fn project_tool_parameters_schema(
    schema: &Value,
    profile: VulcanToolSchemaProjectionProfile,
) -> Value {
    match profile {
        VulcanToolSchemaProjectionProfile::PassThrough => schema.clone(),
        VulcanToolSchemaProjectionProfile::GeminiSlim => project_gemini_slim_schema(schema),
    }
}

/// Returns whether one OpenAI-compatible request ends at a Gemini-family schema validator.
/// 返回单个 OpenAI 兼容请求是否最终进入 Gemini 模型族的 Schema 校验器。
///
/// # Parameters
/// 参数
/// `context` contains the final API-family key and concrete model identifier for one request.
/// `context` 包含单次请求最终使用的接口族键与具体模型标识。
///
/// # Returns
/// 返回值
/// Returns `true` only when both the OpenAI-compatible envelope and a Gemini-family model are explicit.
/// 仅当 OpenAI 兼容外层协议与 Gemini 模型族均明确时返回 `true`。
fn openai_compatible_request_targets_gemini(
    context: &VulcanToolSchemaProjectionContext<'_>,
) -> bool {
    if !context
        .api_family_key
        .to_ascii_lowercase()
        .contains("openai")
    {
        return false;
    }

    model_id_looks_like_gemini(context.model_id)
}

/// Returns whether one model identifier clearly belongs to the Gemini/Gemma family.
/// 返回单个模型标识是否明确属于 Gemini/Gemma 模型族。
fn model_id_looks_like_gemini(model_id: &str) -> bool {
    let normalized = model_id.to_ascii_lowercase();
    normalized.contains("gemini")
        || normalized.contains("gemma")
        || normalized.starts_with("google/")
        || normalized.starts_with("google-")
}

/// Projects one schema into the compact Gemini tool-declaration dialect.
/// 将单个 Schema 投影为紧凑的 Gemini 工具声明方言。
fn project_gemini_slim_schema(schema: &Value) -> Value {
    project_gemini_node(&sanitize_gemini_node(schema)).unwrap_or_else(|| {
        Value::Object(Map::from_iter([(
            String::from("type"),
            Value::String(String::from("object")),
        )]))
    })
}

/// Sanitizes common JSON Schema authoring shapes before Gemini projection.
/// 在 Gemini 投影前清理常见 JSON Schema 编写形态。
fn sanitize_gemini_node(schema: &Value) -> Value {
    match schema {
        Value::Object(object) => sanitize_gemini_object(object),
        Value::Array(items) => Value::Array(items.iter().map(sanitize_gemini_node).collect()),
        other => other.clone(),
    }
}

/// Sanitizes one object-shaped JSON Schema node before Gemini projection.
/// 在 Gemini 投影前清理单个对象形 JSON Schema 节点。
fn sanitize_gemini_object(object: &Map<String, Value>) -> Value {
    let mut sanitized = Map::new();
    for (key, value) in object {
        if key == "enum"
            && let Value::Array(items) = value
        {
            sanitized.insert(
                key.clone(),
                Value::Array(
                    items
                        .iter()
                        .map(|item| Value::String(enum_value_to_string(item)))
                        .collect(),
                ),
            );
            continue;
        }
        sanitized.insert(key.clone(), sanitize_gemini_node(value));
    }

    if matches!(
        sanitized.get("type").and_then(Value::as_str),
        Some("number" | "integer")
    ) && sanitized.contains_key("enum")
    {
        sanitized.insert(String::from("type"), Value::String(String::from("string")));
    }

    infer_gemini_container_type_without_mode_branch_expansion(&mut sanitized);
    filter_gemini_required_to_declared_properties(&mut sanitized);
    ensure_gemini_array_items(&mut sanitized);
    remove_gemini_scalar_object_keywords(&mut sanitized);

    Value::Object(sanitized)
}

/// Infers missing Gemini container types without expanding required-only mode-selector branches.
/// 在不膨胀仅含 required 的模式选择分支的前提下推断缺失的 Gemini 容器类型。
fn infer_gemini_container_type_without_mode_branch_expansion(object: &mut Map<String, Value>) {
    if object.contains_key("type") {
        return;
    }
    if object.contains_key("properties")
        || object.contains_key("additionalProperties")
        || object.contains_key("patternProperties")
        || object.contains_key("required")
    {
        object.insert(String::from("type"), Value::String(String::from("object")));
        return;
    }
    if object.contains_key("items") || object.contains_key("prefixItems") {
        object.insert(String::from("type"), Value::String(String::from("array")));
    }
}

/// Converts one enum value into the string form accepted by Gemini function declarations.
/// 将单个枚举值转换为 Gemini 函数声明可接受的字符串形式。
fn enum_value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Filters `required` entries to names declared in the same `properties` object.
/// 将 `required` 条目过滤为同一 `properties` 对象中声明过的字段名。
fn filter_gemini_required_to_declared_properties(object: &mut Map<String, Value>) {
    let Some(Value::Object(properties)) = object.get("properties") else {
        return;
    };
    let declared = properties.keys().cloned().collect::<BTreeSet<_>>();
    let Some(Value::Array(required)) = object.get_mut("required") else {
        return;
    };
    required.retain(|item| {
        item.as_str()
            .map(|name| declared.contains(name))
            .unwrap_or(false)
    });
}

/// Ensures array schemas have an item schema when no combinator already defines the item shape.
/// 当没有组合器定义元素形状时，确保数组 Schema 拥有元素 Schema。
fn ensure_gemini_array_items(object: &mut Map<String, Value>) {
    if object.get("type").and_then(Value::as_str) != Some("array") {
        return;
    }
    if schema_has_combinator(object) {
        return;
    }
    let needs_default_items = object
        .get("items")
        .map(|items| !schema_node_has_intent(items))
        .unwrap_or(true);
    if needs_default_items {
        object.insert(
            String::from("items"),
            Value::Object(Map::from_iter([(
                String::from("type"),
                Value::String(String::from("string")),
            )])),
        );
    }
}

/// Removes object-only keywords from non-object scalar schemas when no combinator needs them.
/// 当没有组合器需要这些字段时，从非对象标量 Schema 中移除对象专属关键字。
fn remove_gemini_scalar_object_keywords(object: &mut Map<String, Value>) {
    if schema_has_combinator(object) {
        return;
    }
    let Some(type_name) = object.get("type").and_then(Value::as_str) else {
        return;
    };
    if type_name == "object" {
        return;
    }
    object.remove("properties");
    object.remove("required");
}

/// Returns whether one schema object contains a JSON Schema combinator.
/// 返回单个 Schema 对象是否包含 JSON Schema 组合器。
fn schema_has_combinator(object: &Map<String, Value>) -> bool {
    ["anyOf", "oneOf", "allOf"]
        .iter()
        .any(|key| object.contains_key(*key))
}

/// Returns whether one schema node carries enough structural intent to preserve.
/// 返回单个 Schema 节点是否携带足够的结构意图而应被保留。
fn schema_node_has_intent(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return !value.is_null();
    };
    [
        "type",
        "properties",
        "items",
        "enum",
        "const",
        "anyOf",
        "oneOf",
        "allOf",
        "$ref",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
}

/// Projects one sanitized schema node into the Gemini allowlisted schema dialect.
/// 将单个已清理 Schema 节点投影到 Gemini 白名单 Schema 方言。
fn project_gemini_node(schema: &Value) -> Option<Value> {
    let object = schema.as_object()?;
    let mut projected = Map::new();

    insert_optional_clone(&mut projected, object, "description");
    insert_optional_clone(&mut projected, object, "required");
    insert_optional_clone(&mut projected, object, "format");
    insert_gemini_projected_type(&mut projected, object);
    insert_optional_clone(&mut projected, object, "nullable");
    insert_optional_clone(&mut projected, object, "enum");
    insert_projected_gemini_const(&mut projected, object);
    insert_projected_gemini_properties(&mut projected, object);
    insert_projected_gemini_items(&mut projected, object);
    insert_projected_gemini_combinator(&mut projected, object, "allOf");
    insert_projected_gemini_combinator(&mut projected, object, "anyOf");
    insert_projected_gemini_combinator(&mut projected, object, "oneOf");
    insert_optional_clone(&mut projected, object, "minLength");
    hydrate_gemini_required_combinator_branch_properties(&mut projected);

    // Combinator-only root objects remain meaningful even without root-level properties or required fields.
    // 仅含组合器的根对象即使没有根级 properties 或 required 字段仍具有完整语义。
    if projected.get("type").and_then(Value::as_str) == Some("object")
        && !projected.contains_key("properties")
        && !projected.contains_key("required")
        && !schema_has_combinator(&projected)
    {
        return None;
    }

    (!projected.is_empty()).then_some(Value::Object(projected))
}

/// Inserts one cloned allowlisted field when it exists.
/// 当字段存在时插入单个克隆的白名单字段。
fn insert_optional_clone(target: &mut Map<String, Value>, source: &Map<String, Value>, key: &str) {
    if let Some(value) = source.get(key) {
        target.insert(key.to_string(), value.clone());
    }
}

/// Inserts the Gemini-projected `type` field and nullable marker.
/// 插入 Gemini 投影后的 `type` 字段与可空标记。
fn insert_gemini_projected_type(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    match source.get("type") {
        Some(Value::Array(types)) => {
            let mut nullable = source
                .get("nullable")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut first_non_null = None::<String>;
            for value in types {
                let Some(type_name) = value.as_str() else {
                    continue;
                };
                if type_name.eq_ignore_ascii_case("null") {
                    nullable = true;
                } else if first_non_null.is_none() {
                    first_non_null = Some(type_name.to_string());
                }
            }
            if let Some(type_name) = first_non_null {
                target.insert(String::from("type"), Value::String(type_name));
            }
            if nullable {
                target.insert(String::from("nullable"), Value::Bool(true));
            }
        }
        Some(value) => {
            target.insert(String::from("type"), value.clone());
        }
        None => {}
    }
}

/// Inserts a single-value enum converted from a JSON Schema `const` value.
/// 插入由 JSON Schema `const` 值转换而来的单值枚举。
fn insert_projected_gemini_const(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    if target.contains_key("enum") {
        return;
    }
    if let Some(value) = source.get("const") {
        target.insert(String::from("enum"), Value::Array(vec![value.clone()]));
    }
}

/// Inserts projected Gemini object properties when at least one child schema remains.
/// 当至少一个子 Schema 保留时插入投影后的 Gemini 对象属性。
fn insert_projected_gemini_properties(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
) {
    let Some(Value::Object(properties)) = source.get("properties") else {
        return;
    };
    let projected_properties = properties
        .iter()
        .filter_map(|(key, value)| project_gemini_node(value).map(|schema| (key.clone(), schema)))
        .collect::<Map<String, Value>>();
    if !projected_properties.is_empty() {
        target.insert(
            String::from("properties"),
            Value::Object(projected_properties),
        );
    }
}

/// Inserts projected Gemini array items when a child schema remains.
/// 当子 Schema 保留时插入投影后的 Gemini 数组元素。
fn insert_projected_gemini_items(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    let Some(items) = source.get("items") else {
        return;
    };
    match items {
        Value::Array(values) => {
            let projected_items = values
                .iter()
                .filter_map(project_gemini_node)
                .collect::<Vec<_>>();
            if !projected_items.is_empty() {
                target.insert(String::from("items"), Value::Array(projected_items));
            }
        }
        value => {
            if let Some(projected) = project_gemini_node(value) {
                target.insert(String::from("items"), projected);
            }
        }
    }
}

/// Inserts one projected Gemini combinator when it contains at least one child schema.
/// 当组合器至少包含一个子 Schema 时插入单个投影后的 Gemini 组合器。
fn insert_projected_gemini_combinator(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
    key: &str,
) {
    let Some(Value::Array(values)) = source.get(key) else {
        return;
    };
    let projected_values = values
        .iter()
        .filter_map(project_gemini_node)
        .collect::<Vec<_>>();
    if !projected_values.is_empty() {
        target.insert(key.to_string(), Value::Array(projected_values));
    }
}

/// Hydrates Gemini combinator branches with only the property schemas referenced by their own `required` list.
/// 仅用组合分支自身 `required` 列表引用到的属性 Schema 补充分支，满足 Gemini 严格校验并控制 Token 膨胀。
fn hydrate_gemini_required_combinator_branch_properties(schema: &mut Map<String, Value>) {
    let Some(Value::Object(parent_properties)) = schema.get("properties").cloned() else {
        return;
    };

    for key in ["allOf", "anyOf", "oneOf"] {
        let Some(Value::Array(branches)) = schema.get_mut(key) else {
            continue;
        };
        for branch in branches {
            hydrate_gemini_required_branch_properties(branch, &parent_properties);
        }
    }
}

/// Hydrates one Gemini object branch with a minimal `properties` map matching its declared required fields.
/// 使用与必填字段匹配的最小 `properties` 映射补充单个 Gemini 对象分支。
fn hydrate_gemini_required_branch_properties(
    branch: &mut Value,
    parent_properties: &Map<String, Value>,
) {
    let Some(branch_object) = branch.as_object_mut() else {
        return;
    };
    if branch_object.get("type").and_then(Value::as_str) != Some("object") {
        return;
    }
    if branch_object.contains_key("properties") {
        return;
    }

    let required_names = branch_object
        .get("required")
        .and_then(Value::as_array)
        .map(|required| {
            required
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if required_names.is_empty() {
        return;
    }

    let mut required_properties = Map::new();
    let mut retained_required = Vec::new();
    for required_name in required_names {
        let Some(property_schema) = parent_properties.get(&required_name) else {
            continue;
        };
        required_properties.insert(required_name.clone(), property_schema.clone());
        retained_required.push(Value::String(required_name));
    }

    if required_properties.is_empty() {
        branch_object.remove("required");
        return;
    }

    branch_object.insert(String::from("required"), Value::Array(retained_required));
    branch_object.insert(
        String::from("properties"),
        Value::Object(required_properties),
    );
}
