use serde_json::{Map, Value, json};
use vulcan_provider_config::config::{
    RouterParameterPredicateOperator, RouterParameterSchema, RouterParameterSchemaCondition,
    RouterParameterSchemaKind, RouterParameterSchemaPredicate, RouterParameterSchemaValue,
};

/// Converts one Router-owned parameter schema into JSON Schema Draft 2020-12 for AI tool registration.
/// 将一个 Router 所有参数 Schema 转换为用于 AI 工具注册的 JSON Schema Draft 2020-12。
pub fn router_parameter_schema_to_json_schema(
    schema: &RouterParameterSchema,
) -> Result<Value, String> {
    let mut document = convert_schema_node(schema)?;
    let object = document.as_object_mut().ok_or_else(|| {
        String::from("Router parameter schema conversion did not produce a JSON Schema object.")
    })?;
    object.insert(
        String::from("$schema"),
        Value::String(String::from("https://json-schema.org/draft/2020-12/schema")),
    );
    Ok(document)
}

/// Converts one recursive Router schema node without adding a document-level dialect marker.
/// 转换一个递归 Router Schema 节点且不添加文档级方言标记。
fn convert_schema_node(schema: &RouterParameterSchema) -> Result<Value, String> {
    let mut output = Map::new();
    if let Some(kind) = schema.kind {
        output.insert(
            String::from("type"),
            Value::String(json_schema_type(kind).to_string()),
        );
    }
    if !schema.description.is_empty() {
        output.insert(
            String::from("description"),
            Value::String(schema.description.clone()),
        );
    }
    if schema.kind == Some(RouterParameterSchemaKind::Object) {
        let mut properties = Map::new();
        let mut required = Vec::new();
        for property in &schema.properties {
            if property.name.trim().is_empty() || properties.contains_key(&property.name) {
                return Err(String::from(
                    "Router parameter schema contains an empty or duplicate object property.",
                ));
            }
            properties.insert(
                property.name.clone(),
                convert_schema_node(&property.schema)?,
            );
            if property.required {
                required.push(Value::String(property.name.clone()));
            }
        }
        output.insert(String::from("properties"), Value::Object(properties));
        output.insert(String::from("additionalProperties"), Value::Bool(false));
        if !required.is_empty() {
            output.insert(String::from("required"), Value::Array(required));
        }
    }
    if schema.kind == Some(RouterParameterSchemaKind::Array) {
        let items = schema.items.as_ref().ok_or_else(|| {
            String::from("Router array parameter schema does not declare an item schema.")
        })?;
        output.insert(String::from("items"), convert_schema_node(items)?);
    }
    insert_literal_array(&mut output, "enum", &schema.allowed_values)?;
    insert_optional_literal(&mut output, "const", schema.r#const.as_ref())?;
    insert_optional_literal(&mut output, "default", schema.default.as_ref())?;
    insert_literal_array(&mut output, "examples", &schema.examples)?;
    insert_optional_number(&mut output, "minimum", schema.minimum_integer);
    insert_optional_number(&mut output, "maximum", schema.maximum_integer);
    insert_optional_number(
        &mut output,
        "exclusiveMinimum",
        schema.exclusive_minimum_integer,
    );
    insert_optional_number(
        &mut output,
        "exclusiveMaximum",
        schema.exclusive_maximum_integer,
    );
    insert_optional_number(&mut output, "multipleOf", schema.multiple_of_integer);
    insert_optional_float(&mut output, "minimum", schema.minimum_number)?;
    insert_optional_float(&mut output, "maximum", schema.maximum_number)?;
    insert_optional_float(
        &mut output,
        "exclusiveMinimum",
        schema.exclusive_minimum_number,
    )?;
    insert_optional_float(
        &mut output,
        "exclusiveMaximum",
        schema.exclusive_maximum_number,
    )?;
    insert_optional_float(&mut output, "multipleOf", schema.multiple_of_number)?;
    insert_optional_usize(&mut output, "minLength", schema.minimum_length);
    insert_optional_usize(&mut output, "maxLength", schema.maximum_length);
    insert_optional_usize(&mut output, "minItems", schema.minimum_items);
    insert_optional_usize(&mut output, "maxItems", schema.maximum_items);
    if schema.unique_items {
        output.insert(String::from("uniqueItems"), Value::Bool(true));
    }
    if !schema.pattern.is_empty() {
        output.insert(
            String::from("pattern"),
            Value::String(schema.pattern.clone()),
        );
    }
    if !schema.format.is_empty() {
        output.insert(String::from("format"), Value::String(schema.format.clone()));
    } else if schema.kind == Some(RouterParameterSchemaKind::Resource) {
        output.insert(
            String::from("format"),
            Value::String(String::from("vulcan-local-path")),
        );
    }
    insert_recursive_array(&mut output, "oneOf", &schema.one_of)?;
    insert_recursive_array(&mut output, "anyOf", &schema.any_of)?;
    insert_recursive_array(&mut output, "allOf", &schema.all_of)?;
    insert_optional_schema(&mut output, "not", schema.not.as_deref())?;
    insert_optional_schema(&mut output, "if", schema.r#if.as_deref())?;
    insert_optional_schema(&mut output, "then", schema.then.as_deref())?;
    insert_optional_schema(&mut output, "else", schema.r#else.as_deref())?;
    append_cross_field_constraints(&mut output, schema)?;
    Ok(Value::Object(output))
}

/// Returns the JSON Schema scalar or container type for one Router kind.
/// 返回一个 Router 类型对应的 JSON Schema 标量或容器类型。
fn json_schema_type(kind: RouterParameterSchemaKind) -> &'static str {
    match kind {
        RouterParameterSchemaKind::Object => "object",
        RouterParameterSchemaKind::Array => "array",
        RouterParameterSchemaKind::String | RouterParameterSchemaKind::Resource => "string",
        RouterParameterSchemaKind::Integer => "integer",
        RouterParameterSchemaKind::Number => "number",
        RouterParameterSchemaKind::Boolean => "boolean",
        RouterParameterSchemaKind::Null => "null",
    }
}

/// Converts one typed Router scalar literal into a JSON value.
/// 将一个类型化 Router 标量字面量转换为 JSON 值。
fn literal_to_json(value: &RouterParameterSchemaValue) -> Result<Value, String> {
    let mut literals = Vec::new();
    if let Some(value) = value.string.as_ref() {
        literals.push(Value::String(value.clone()));
    }
    if let Some(value) = value.integer {
        literals.push(Value::Number(value.into()));
    }
    if let Some(value) = value.number {
        literals.push(
            serde_json::Number::from_f64(value)
                .map(Value::Number)
                .ok_or_else(|| {
                    String::from("Router schema contains a non-finite number literal.")
                })?,
        );
    }
    if let Some(value) = value.boolean {
        literals.push(Value::Bool(value));
    }
    if value.null {
        literals.push(Value::Null);
    }
    if literals.len() != 1 {
        return Err(String::from(
            "Router schema literal must contain exactly one typed value.",
        ));
    }
    Ok(literals.remove(0))
}

/// Inserts one non-empty typed literal array into a JSON Schema object.
/// 向 JSON Schema 对象插入一个非空类型化字面量数组。
fn insert_literal_array(
    output: &mut Map<String, Value>,
    key: &str,
    values: &[RouterParameterSchemaValue],
) -> Result<(), String> {
    if values.is_empty() {
        return Ok(());
    }
    let converted = values
        .iter()
        .map(literal_to_json)
        .collect::<Result<Vec<_>, _>>()?;
    output.insert(key.to_string(), Value::Array(converted));
    Ok(())
}

/// Inserts one optional typed literal into a JSON Schema object.
/// 向 JSON Schema 对象插入一个可选类型化字面量。
fn insert_optional_literal(
    output: &mut Map<String, Value>,
    key: &str,
    value: Option<&RouterParameterSchemaValue>,
) -> Result<(), String> {
    if let Some(value) = value {
        output.insert(key.to_string(), literal_to_json(value)?);
    }
    Ok(())
}

/// Inserts one optional signed integer keyword.
/// 插入一个可选有符号整数关键字。
fn insert_optional_number(output: &mut Map<String, Value>, key: &str, value: Option<i64>) {
    if let Some(value) = value {
        output.insert(key.to_string(), Value::Number(value.into()));
    }
}

/// Inserts one optional finite floating-point keyword.
/// 插入一个可选有限浮点关键字。
fn insert_optional_float(
    output: &mut Map<String, Value>,
    key: &str,
    value: Option<f64>,
) -> Result<(), String> {
    if let Some(value) = value {
        let number = serde_json::Number::from_f64(value).ok_or_else(|| {
            String::from("Router schema contains a non-finite numeric constraint.")
        })?;
        output.insert(key.to_string(), Value::Number(number));
    }
    Ok(())
}

/// Inserts one optional non-negative size keyword.
/// 插入一个可选非负大小关键字。
fn insert_optional_usize(output: &mut Map<String, Value>, key: &str, value: Option<usize>) {
    if let Some(value) = value {
        output.insert(
            key.to_string(),
            Value::Number(serde_json::Number::from(value)),
        );
    }
}

/// Inserts one non-empty recursive schema array.
/// 插入一个非空递归 Schema 数组。
fn insert_recursive_array(
    output: &mut Map<String, Value>,
    key: &str,
    schemas: &[RouterParameterSchema],
) -> Result<(), String> {
    if schemas.is_empty() {
        return Ok(());
    }
    let converted = schemas
        .iter()
        .map(convert_schema_node)
        .collect::<Result<Vec<_>, _>>()?;
    output.insert(key.to_string(), Value::Array(converted));
    Ok(())
}

/// Inserts one optional recursive schema.
/// 插入一个可选递归 Schema。
fn insert_optional_schema(
    output: &mut Map<String, Value>,
    key: &str,
    schema: Option<&RouterParameterSchema>,
) -> Result<(), String> {
    if let Some(schema) = schema {
        output.insert(key.to_string(), convert_schema_node(schema)?);
    }
    Ok(())
}

/// Appends Router conditional and dependent-required rules as closed JSON Schema constraints.
/// 将 Router 条件与依赖必填规则追加为封闭 JSON Schema 约束。
fn append_cross_field_constraints(
    output: &mut Map<String, Value>,
    schema: &RouterParameterSchema,
) -> Result<(), String> {
    if schema.conditions.is_empty() && schema.dependent_required.is_empty() {
        return Ok(());
    }
    let all_of = output
        .entry(String::from("allOf"))
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| String::from("Router JSON Schema allOf projection is not an array."))?;
    for condition in &schema.conditions {
        all_of.push(condition_to_json_schema(condition)?);
    }
    for dependency in &schema.dependent_required {
        let predicate = path_presence_schema(&dependency.property, true)?;
        let requirement = path_list_requirement_schema(&dependency.require, &[])?;
        all_of.push(json!({"if": predicate, "then": requirement}));
    }
    Ok(())
}

/// Converts one Router cross-field condition into a JSON Schema if/then constraint.
/// 将一条 Router 跨字段条件转换为 JSON Schema if/then 约束。
fn condition_to_json_schema(condition: &RouterParameterSchemaCondition) -> Result<Value, String> {
    let predicate = predicate_to_json_schema(&condition.predicate)?;
    let requirement = path_list_requirement_schema(&condition.require, &condition.forbid)?;
    Ok(json!({"if": predicate, "then": requirement}))
}

/// Converts one exact Router path predicate into a nested JSON Schema condition.
/// 将一个精确 Router 路径谓词转换为嵌套 JSON Schema 条件。
fn predicate_to_json_schema(predicate: &RouterParameterSchemaPredicate) -> Result<Value, String> {
    match predicate.operator {
        RouterParameterPredicateOperator::Present => path_presence_schema(&predicate.path, true),
        RouterParameterPredicateOperator::Absent => path_presence_schema(&predicate.path, false),
        RouterParameterPredicateOperator::Equals | RouterParameterPredicateOperator::NotEquals => {
            let value = predicate.value.as_ref().ok_or_else(|| {
                String::from("Router equality predicate does not contain a typed value.")
            })?;
            let leaf = json!({"const": literal_to_json(value)?});
            let equality = nested_property_constraint(&predicate.path, leaf)?;
            if predicate.operator == RouterParameterPredicateOperator::NotEquals {
                Ok(json!({"not": equality}))
            } else {
                Ok(equality)
            }
        }
    }
}

/// Builds one nested presence or absence predicate for a dotted object path.
/// 为一个点分对象路径构建嵌套存在或不存在谓词。
fn path_presence_schema(path: &str, present: bool) -> Result<Value, String> {
    let segments = split_dotted_path(path)?;
    let leaf = json!({"required": [segments[segments.len() - 1]]});
    let constraint = wrap_parent_segments(&segments[..segments.len() - 1], leaf);
    if present {
        Ok(constraint)
    } else {
        Ok(json!({"not": constraint}))
    }
}

/// Builds one nested property-value constraint for a dotted object path.
/// 为一个点分对象路径构建嵌套属性值约束。
fn nested_property_constraint(path: &str, leaf: Value) -> Result<Value, String> {
    let segments = split_dotted_path(path)?;
    let last = segments[segments.len() - 1];
    let mut properties = Map::new();
    properties.insert(last.to_string(), leaf);
    let constraint = json!({
        "properties": Value::Object(properties),
        "required": [last]
    });
    Ok(wrap_parent_segments(
        &segments[..segments.len() - 1],
        constraint,
    ))
}

/// Builds one conjunction of required and forbidden dotted paths.
/// 构建一个必填与禁止点分路径的合取约束。
fn path_list_requirement_schema(
    required: &[String],
    forbidden: &[String],
) -> Result<Value, String> {
    let mut constraints = Vec::new();
    for path in required {
        constraints.push(path_presence_schema(path, true)?);
    }
    for path in forbidden {
        constraints.push(path_presence_schema(path, false)?);
    }
    Ok(match constraints.len() {
        0 => json!({}),
        1 => constraints.remove(0),
        _ => json!({"allOf": constraints}),
    })
}

/// Wraps a leaf constraint in required parent object properties.
/// 将叶子约束包裹到必需的父级对象属性中。
fn wrap_parent_segments(parents: &[&str], mut constraint: Value) -> Value {
    for segment in parents.iter().rev() {
        let mut properties = Map::new();
        properties.insert((*segment).to_string(), constraint);
        constraint = json!({
            "properties": Value::Object(properties),
            "required": [*segment]
        });
    }
    constraint
}

/// Splits and validates one non-empty dotted object path.
/// 拆分并校验一个非空点分对象路径。
fn split_dotted_path(path: &str) -> Result<Vec<&str>, String> {
    let segments = path.split('.').collect::<Vec<_>>();
    if segments.is_empty()
        || segments
            .iter()
            .any(|segment| segment.is_empty() || segment.trim() != *segment)
    {
        return Err(format!(
            "Router contract contains an invalid dotted path `{path}`."
        ));
    }
    Ok(segments)
}
