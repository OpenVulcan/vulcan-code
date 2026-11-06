use super::*;

/// Projects one tool-parameter schema into the selected provider-facing dialect.
/// 将单个工具参数 Schema 投影到选定的 Provider 可见方言。
pub fn project_tool_parameters_schema(
    schema: &Value,
    profile: VulcanToolSchemaProjectionProfile,
) -> Value {
    match profile {
        VulcanToolSchemaProjectionProfile::PassThrough => schema.clone(),
        VulcanToolSchemaProjectionProfile::StandardObjectRoot => {
            project_standard_object_root_schema(schema)
        }
        VulcanToolSchemaProjectionProfile::GeminiSlim => project_gemini_slim_schema(schema),
    }
}

/// Projects one canonical tool schema into the portable lightweight function-tool subset.
/// 将单个规范工具 Schema 投影到可移植的轻量函数工具子集。
///
/// # Parameters
/// 参数
/// `schema` is the canonical host-owned tool parameter schema.
/// `schema` 是宿主拥有的规范工具参数 Schema。
///
/// # Returns
/// 返回值
/// Returns an object-root schema containing only basic scalar, object, array, enum, and description declarations.
/// 返回仅包含基础标量、对象、数组、枚举与描述声明的对象根 Schema。
fn project_standard_object_root_schema(schema: &Value) -> Value {
    // Local references carry real parameter shapes (pydantic enums, nested models); inline them first.
    // 本地引用承载真实参数形状（pydantic 枚举、嵌套模型）；先将其内联。
    let mut expansions = 0;
    let resolved = inline_local_schema_refs(schema, schema, &mut Vec::new(), &mut expansions);
    let mut projected = project_lightweight_schema_node(&resolved)
        .and_then(|schema| schema.as_object().cloned())
        .unwrap_or_default();

    // Function-tool protocols require an object root even when the canonical schema is malformed or union-only.
    // 即使规范 Schema 异常或仅含联合分支，函数工具协议仍要求对象根。
    projected.insert(String::from("type"), Value::String(String::from("object")));
    projected.remove("items");
    projected.remove("enum");

    let has_explicit_empty_properties = matches!(
        projected.get("properties"),
        Some(Value::Object(properties)) if properties.is_empty()
    );
    let has_closed_object_boundary =
        projected.get("additionalProperties") == Some(&Value::Bool(false));
    if has_explicit_empty_properties && has_closed_object_boundary {
        // Emit the smallest explicit no-argument object accepted by strict OpenAI-compatible validators.
        // 输出严格 OpenAI 兼容校验器接受的最小显式无参数对象。
        projected.insert(String::from("required"), Value::Array(Vec::new()));
        projected.remove("additionalProperties");
    }
    Value::Object(projected)
}

/// Upper bound on local `$ref` expansions per projected schema, guarding against shared-definition blowup.
/// 每个投影 Schema 的本地 `$ref` 展开上限，防止共享定义导致膨胀。
const MAX_LIGHTWEIGHT_REF_EXPANSIONS: usize = 256;

/// Returns a copy of `node` with local `#/...` references replaced by their targets inside `root`.
/// 返回 `node` 的副本，其中本地 `#/...` 引用被替换为 `root` 内的目标。
///
/// # Parameters
/// 参数
/// `root` is the canonical document, `stack` holds references being expanded, `expansions` counts work done.
/// `root` 是规范文档，`stack` 保存正在展开的引用，`expansions` 统计已完成的展开。
///
/// # Returns
/// 返回值
/// Sibling keywords override the target. A cyclic reference keeps only the target's string `type`;
/// remote or unresolved references and any reference past the budget stay as-is and are dropped later.
/// 同级关键字覆盖目标。循环引用只保留目标的字符串 `type`；远程或无法解析的引用以及超出预算的引用保持原样，随后被丢弃。
fn inline_local_schema_refs(
    root: &Value,
    node: &Value,
    stack: &mut Vec<String>,
    expansions: &mut usize,
) -> Value {
    match node {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| inline_local_schema_refs(root, item, stack, expansions))
                .collect(),
        ),
        Value::Object(object) => {
            let target = object
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|reference| {
                    let pointer = reference.strip_prefix('#')?;
                    root.pointer(pointer)
                        .filter(|target| target.is_object())
                        .map(|target| (reference.to_owned(), target))
                });
            let Some((reference, target)) = target else {
                // Definition tables are never projected, so they must not spend the expansion budget.
                // 定义表永不投影，因此不得消耗展开预算。
                return Value::Object(
                    object
                        .iter()
                        .filter(|(key, _)| !matches!(key.as_str(), "$defs" | "definitions"))
                        .map(|(key, value)| {
                            (
                                key.clone(),
                                inline_local_schema_refs(root, value, stack, expansions),
                            )
                        })
                        .collect(),
                );
            };
            let mut merged = if stack.contains(&reference) {
                target
                    .get("type")
                    .filter(|type_name| type_name.is_string())
                    .map(|type_name| Map::from_iter([(String::from("type"), type_name.clone())]))
                    .unwrap_or_default()
            } else if *expansions >= MAX_LIGHTWEIGHT_REF_EXPANSIONS {
                return node.clone();
            } else {
                *expansions += 1;
                stack.push(reference);
                let expanded = inline_local_schema_refs(root, target, stack, expansions);
                stack.pop();
                expanded.as_object().cloned().unwrap_or_default()
            };
            for (key, value) in object.iter().filter(|(key, _)| key.as_str() != "$ref") {
                merged.insert(
                    key.clone(),
                    inline_local_schema_refs(root, value, stack, expansions),
                );
            }
            Value::Object(merged)
        }
        other => other.clone(),
    }
}

/// Recursively projects one schema node into the lightweight provider subset.
/// 递归将单个 Schema 节点投影到轻量 Provider 子集。
///
/// # Parameters
/// 参数
/// `schema` is one canonical JSON Schema node owned by the host.
/// `schema` 是一个由宿主拥有的规范 JSON Schema 节点。
///
/// # Returns
/// 返回值
/// Returns `None` when the node has no portable structural meaning.
/// 当节点不包含可移植的结构语义时返回 `None`。
fn project_lightweight_schema_node(schema: &Value) -> Option<Value> {
    let source = schema.as_object()?;
    let mut projected = Map::new();

    insert_optional_clone(&mut projected, source, "description");
    insert_lightweight_type(&mut projected, source);
    insert_lightweight_enum(&mut projected, source);
    insert_lightweight_properties(&mut projected, source);
    insert_lightweight_items(&mut projected, source);
    insert_lightweight_additional_properties(&mut projected, source);

    // Composition keywords are consumed here and never copied to the provider-facing schema.
    // 组合关键字仅在此处用于降级，绝不复制到 Provider 可见 Schema。
    for (keyword, conjunctive) in [("allOf", true), ("anyOf", false), ("oneOf", false)] {
        merge_lightweight_combinator(&mut projected, source, keyword, conjunctive);
    }

    infer_lightweight_container_type(&mut projected);
    insert_lightweight_required(&mut projected, source);
    ensure_lightweight_array_items(&mut projected);
    lightweight_node_has_shape(&projected).then_some(Value::Object(projected))
}

/// Inserts one portable single-valued JSON Schema type.
/// 插入单个可移植的 JSON Schema 类型。
///
/// # Parameters
/// 参数
/// `target` receives the projected field and `source` is the canonical schema object.
/// `target` 接收投影字段，`source` 是规范 Schema 对象。
///
/// # Returns
/// 返回值
/// This function has no return value and ignores unsupported or null-only type declarations.
/// 此函数无返回值，并忽略不支持或仅为 null 的类型声明。
fn insert_lightweight_type(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    let type_name = match source.get("type") {
        Some(Value::String(type_name)) if lightweight_type_name_is_portable(type_name) => {
            Some(type_name.clone())
        }
        Some(Value::Array(type_names)) => type_names
            .iter()
            .filter_map(Value::as_str)
            .find(|type_name| lightweight_type_name_is_portable(type_name))
            .map(str::to_owned),
        _ => None,
    };
    if let Some(type_name) = type_name {
        target.insert(String::from("type"), Value::String(type_name));
    }
}

/// Reports whether one JSON Schema type name belongs to the portable lightweight subset.
/// 报告单个 JSON Schema 类型名是否属于可移植轻量子集。
///
/// # Parameters
/// 参数
/// `type_name` is one canonical JSON Schema type string.
/// `type_name` 是单个规范 JSON Schema 类型字符串。
///
/// # Returns
/// 返回值
/// Returns `true` only for the six non-null JSON value categories accepted by basic function schemas.
/// 仅对基础函数 Schema 接受的六种非 null JSON 值类别返回 `true`。
fn lightweight_type_name_is_portable(type_name: &str) -> bool {
    matches!(
        type_name,
        "object" | "array" | "string" | "number" | "integer" | "boolean"
    )
}

/// Inserts a portable enum and converts a canonical `const` into a one-item enum.
/// 插入可移植的枚举，并将规范 `const` 转换为单项枚举。
///
/// # Parameters
/// 参数
/// `target` receives the projected field and `source` is the canonical schema object.
/// `target` 接收投影字段，`source` 是规范 Schema 对象。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn insert_lightweight_enum(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    let values = if let Some(Value::Array(values)) = source.get("enum") {
        values
            .iter()
            .filter(|value| lightweight_enum_value_is_portable(value))
            .cloned()
            .collect::<Vec<_>>()
    } else if let Some(value) = source.get("const")
        && lightweight_enum_value_is_portable(value)
    {
        vec![value.clone()]
    } else {
        Vec::new()
    };
    if !values.is_empty() {
        target.insert(String::from("enum"), Value::Array(values));
    }
}

/// Reports whether one enum value is a portable non-null JSON scalar.
/// 报告单个枚举值是否为可移植的非 null JSON 标量。
///
/// # Parameters
/// 参数
/// `value` is one canonical enum or const value.
/// `value` 是单个规范枚举或 const 值。
///
/// # Returns
/// 返回值
/// Returns `true` for strings, numbers, and booleans.
/// 对字符串、数字和布尔值返回 `true`。
fn lightweight_enum_value_is_portable(value: &Value) -> bool {
    matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_))
}

/// Recursively inserts declared object properties.
/// 递归插入已声明的对象属性。
///
/// # Parameters
/// 参数
/// `target` receives the projected properties and `source` is the canonical schema object.
/// `target` 接收投影属性，`source` 是规范 Schema 对象。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn insert_lightweight_properties(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    let Some(Value::Object(properties)) = source.get("properties") else {
        return;
    };
    let projected = properties
        .iter()
        .filter_map(|(name, schema)| {
            project_lightweight_schema_node(schema).map(|schema| (name.clone(), schema))
        })
        .collect::<Map<_, _>>();
    target.insert(String::from("properties"), Value::Object(projected));
}

/// Recursively inserts one homogeneous array item schema.
/// 递归插入单个同构数组元素 Schema。
///
/// # Parameters
/// 参数
/// `target` receives the projected item schema and `source` is the canonical schema object.
/// `target` 接收投影元素 Schema，`source` 是规范 Schema 对象。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn insert_lightweight_items(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    let item_schema = match source.get("items") {
        Some(Value::Array(items)) => items.first(),
        item => item,
    };
    if let Some(projected) = item_schema.and_then(project_lightweight_schema_node) {
        target.insert(String::from("items"), projected);
    }
}

/// Preserves only the portable boolean form of `additionalProperties`.
/// 仅保留 `additionalProperties` 的可移植布尔形式。
///
/// # Parameters
/// 参数
/// `target` receives the projected field and `source` is the canonical schema object.
/// `target` 接收投影字段，`source` 是规范 Schema 对象。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn insert_lightweight_additional_properties(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
) {
    if let Some(Value::Bool(value)) = source.get("additionalProperties") {
        target.insert(String::from("additionalProperties"), Value::Bool(*value));
    }
}

/// Degrades one composition keyword into a single portable basic schema shape.
/// 将单个组合关键字降级为单一可移植基础 Schema 形态。
///
/// # Parameters
/// 参数
/// `target` is the current basic node, `source` owns the branches, `keyword` selects them, and `conjunctive` distinguishes `allOf` from alternatives.
/// `target` 是当前基础节点，`source` 拥有分支，`keyword` 选择分支，`conjunctive` 用于区分 `allOf` 与备选联合。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn merge_lightweight_combinator(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
    keyword: &str,
    conjunctive: bool,
) {
    let Some(Value::Array(branches)) = source.get(keyword) else {
        return;
    };
    let projected_branches = branches
        .iter()
        .filter_map(project_lightweight_schema_node)
        .filter_map(|branch| branch.as_object().cloned())
        .collect::<Vec<_>>();
    if projected_branches.is_empty() {
        return;
    }

    if projected_branches
        .iter()
        .all(|branch| branch.get("type").and_then(Value::as_str) == Some("object"))
    {
        merge_lightweight_object_branches(target, &projected_branches, conjunctive);
        return;
    }

    if !lightweight_node_has_shape(target) {
        merge_lightweight_schema_fields(target, &projected_branches[0]);
    }
}

/// Merges object alternatives without retaining their composition keyword.
/// 在不保留组合关键字的前提下合并对象分支。
///
/// # Parameters
/// 参数
/// `target` receives merged properties, `branches` are portable object nodes, and `conjunctive` controls required-name union versus intersection.
/// `target` 接收合并属性，`branches` 是可移植对象节点，`conjunctive` 控制必填名的并集或交集。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn merge_lightweight_object_branches(
    target: &mut Map<String, Value>,
    branches: &[Map<String, Value>],
    conjunctive: bool,
) {
    target
        .entry(String::from("type"))
        .or_insert_with(|| Value::String(String::from("object")));

    let mut merged_properties = target
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for properties in branches
        .iter()
        .filter_map(|branch| branch.get("properties").and_then(Value::as_object))
    {
        for (name, schema) in properties {
            if let Some(existing) = merged_properties.get_mut(name) {
                merge_lightweight_property_variants(existing, schema, conjunctive);
            } else {
                merged_properties.insert(name.clone(), schema.clone());
            }
        }
    }
    if !merged_properties.is_empty() {
        target.insert(String::from("properties"), Value::Object(merged_properties));
    }

    let branch_required = branches
        .iter()
        .map(lightweight_required_names)
        .collect::<Vec<_>>();
    let derived_required = if conjunctive {
        branch_required
            .into_iter()
            .flatten()
            .collect::<BTreeSet<_>>()
    } else {
        intersect_lightweight_required_names(&branch_required)
    };
    merge_lightweight_required_names(target, derived_required);
}

/// Merges enum values for duplicate properties with the same portable type.
/// 为具有相同可移植类型的重复属性合并枚举值。
///
/// # Parameters
/// 参数
/// `existing` is the retained property schema, `candidate` is one branch schema, and `conjunctive` selects intersection versus union semantics.
/// `existing` 是已保留的属性 Schema，`candidate` 是一个分支 Schema，`conjunctive` 选择交集或并集语义。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn merge_lightweight_property_variants(existing: &mut Value, candidate: &Value, conjunctive: bool) {
    let (Some(existing), Some(candidate)) = (existing.as_object_mut(), candidate.as_object())
    else {
        return;
    };
    if existing.get("type") != candidate.get("type") {
        return;
    }
    match (
        existing.get("enum").and_then(Value::as_array).cloned(),
        candidate.get("enum").and_then(Value::as_array),
    ) {
        (Some(mut values), Some(candidate_values)) => {
            if conjunctive {
                values.retain(|value| candidate_values.contains(value));
            } else {
                for value in candidate_values {
                    if !values.contains(value) {
                        values.push(value.clone());
                    }
                }
            }
            if values.is_empty() {
                existing.remove("enum");
            } else {
                existing.insert(String::from("enum"), Value::Array(values));
            }
        }
        (None, Some(candidate_values)) if conjunctive => {
            existing.insert(String::from("enum"), Value::Array(candidate_values.clone()));
        }
        // One unrestricted alternative makes a union unrestricted, while an allOf keeps the other restriction.
        // 任一备选分支不限制枚举时并集也不限制，而 allOf 仍保留另一个限制。
        _ if !conjunctive => {
            existing.remove("enum");
        }
        _ => {}
    }
}

/// Inserts canonical required names after filtering them to projected properties.
/// 在将规范必填名过滤到已投影属性后进行插入。
///
/// # Parameters
/// 参数
/// `target` receives required names and `source` is the canonical schema object.
/// `target` 接收必填名，`source` 是规范 Schema 对象。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn insert_lightweight_required(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    let Some(Value::Array(required)) = source.get("required") else {
        return;
    };
    let declared = target
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let required = required
        .iter()
        .filter_map(Value::as_str)
        .filter(|name| declared.contains(*name))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    merge_lightweight_required_names(target, required);
}

/// Returns the required-name set declared by one projected object branch.
/// 返回单个已投影对象分支声明的必填名集合。
///
/// # Parameters
/// 参数
/// `branch` is one projected object branch.
/// `branch` 是一个已投影对象分支。
///
/// # Returns
/// 返回值
/// Returns the deduplicated required names.
/// 返回去重后的必填名。
fn lightweight_required_names(branch: &Map<String, Value>) -> BTreeSet<String> {
    branch
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// Intersects required-name sets across alternative branches.
/// 对备选分支之间的必填名集合求交集。
///
/// # Parameters
/// 参数
/// `sets` contains one required-name set per alternative branch.
/// `sets` 包含每个备选分支的一个必填名集合。
///
/// # Returns
/// 返回值
/// Returns only names required by every branch.
/// 仅返回所有分支都必填的名称。
fn intersect_lightweight_required_names(sets: &[BTreeSet<String>]) -> BTreeSet<String> {
    let Some((first, rest)) = sets.split_first() else {
        return BTreeSet::new();
    };
    rest.iter().fold(first.clone(), |intersection, names| {
        intersection.intersection(names).cloned().collect()
    })
}

/// Merges required names into the target while preserving deterministic ordering.
/// 将必填名合并到目标中，同时保持确定性顺序。
///
/// # Parameters
/// 参数
/// `target` receives the merged array and `names` contains additional required property names.
/// `target` 接收合并数组，`names` 包含额外必填属性名。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn merge_lightweight_required_names(target: &mut Map<String, Value>, names: BTreeSet<String>) {
    let mut merged = lightweight_required_names(target);
    merged.extend(names);
    if merged.is_empty() {
        target.remove("required");
    } else {
        target.insert(
            String::from("required"),
            Value::Array(merged.into_iter().map(Value::String).collect()),
        );
    }
}

/// Merges missing portable fields from one selected fallback branch.
/// 从一个选定的回退分支合并缺失的可移植字段。
///
/// # Parameters
/// 参数
/// `target` receives missing fields and `source` is the selected projected branch.
/// `target` 接收缺失字段，`source` 是选定的已投影分支。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn merge_lightweight_schema_fields(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    for (key, value) in source {
        target.entry(key.clone()).or_insert_with(|| value.clone());
    }
}

/// Infers a missing object or array type from portable structural fields.
/// 根据可移植结构字段推断缺失的对象或数组类型。
///
/// # Parameters
/// 参数
/// `target` is the projected schema node to complete.
/// `target` 是待补全的已投影 Schema 节点。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn infer_lightweight_container_type(target: &mut Map<String, Value>) {
    if target.contains_key("type") {
        return;
    }
    if target.contains_key("properties") || target.contains_key("additionalProperties") {
        target.insert(String::from("type"), Value::String(String::from("object")));
    } else if target.contains_key("items") {
        target.insert(String::from("type"), Value::String(String::from("array")));
    } else if let Some(type_name) = target
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(lightweight_enum_value_type_name)
    {
        target.insert(String::from("type"), Value::String(String::from(type_name)));
    }
}

/// Returns the portable JSON Schema type name for one enum scalar.
/// 返回单个枚举标量对应的可移植 JSON Schema 类型名。
///
/// # Parameters
/// 参数
/// `value` is one already filtered enum scalar.
/// `value` 是单个已过滤枚举标量。
///
/// # Returns
/// 返回值
/// Returns the matching string, number, or boolean type name.
/// 返回匹配的字符串、数字或布尔类型名。
fn lightweight_enum_value_type_name(value: &Value) -> Option<&'static str> {
    match value {
        Value::String(_) => Some("string"),
        Value::Number(number) if number.is_i64() || number.is_u64() => Some("integer"),
        Value::Number(_) => Some("number"),
        Value::Bool(_) => Some("boolean"),
        _ => None,
    }
}

/// Ensures a projected array always exposes one basic item schema.
/// 确保已投影数组始终公开一个基础元素 Schema。
///
/// # Parameters
/// 参数
/// `target` is the projected schema node to complete.
/// `target` 是待补全的已投影 Schema 节点。
///
/// # Returns
/// 返回值
/// This function has no return value.
/// 此函数无返回值。
fn ensure_lightweight_array_items(target: &mut Map<String, Value>) {
    if target.get("type").and_then(Value::as_str) == Some("array") && !target.contains_key("items")
    {
        target.insert(
            String::from("items"),
            Value::Object(Map::from_iter([(
                String::from("type"),
                Value::String(String::from("string")),
            )])),
        );
    }
}

/// Reports whether a lightweight node already owns a concrete structural shape.
/// 报告轻量节点是否已拥有具体结构形态。
///
/// # Parameters
/// 参数
/// `node` is one projected schema object.
/// `node` 是一个已投影 Schema 对象。
///
/// # Returns
/// 返回值
/// Returns `true` for typed, object-property, or array-item nodes.
/// 对已定型、包含对象属性或数组元素的节点返回 `true`。
fn lightweight_node_has_shape(node: &Map<String, Value>) -> bool {
    ["type", "properties", "items"]
        .iter()
        .any(|key| node.contains_key(*key))
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
    let mut non_string_enum = None;
    for (key, value) in object {
        if key == "enum"
            && let Value::Array(items) = value
        {
            // Gemini declarations accept only string enums. A non-string enum keeps its real type
            // and moves its values into the description, so arguments keep their JSON type.
            // Gemini 声明只接受字符串枚举。非字符串枚举保留真实类型，并把可选值移入描述，使参数保持其
            // JSON 类型。
            if items.iter().all(Value::is_string) {
                sanitized.insert(key.clone(), value.clone());
            } else {
                non_string_enum = Some(items);
            }
            continue;
        }
        sanitized.insert(key.clone(), sanitize_gemini_node(value));
    }
    if let Some(items) = non_string_enum {
        move_gemini_enum_into_description(&mut sanitized, items);
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

/// Records a non-string enum as allowed values in the description, inferring a missing type from
/// the first value.
/// 把非字符串枚举作为可选值记入描述，并在缺少类型时按第一个值推断。
fn move_gemini_enum_into_description(object: &mut Map<String, Value>, items: &[Value]) {
    if !object.contains_key("type")
        && let Some(type_name) = items.first().and_then(lightweight_enum_value_type_name)
    {
        object.insert(String::from("type"), Value::String(String::from(type_name)));
    }
    let allowed = items
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let note = format!("Allowed values: {allowed}.");
    let description = match object.get("description").and_then(Value::as_str) {
        Some(existing) if !existing.trim().is_empty() => format!("{} {note}", existing.trim_end()),
        _ => note,
    };
    object.insert(String::from("description"), Value::String(description));
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
