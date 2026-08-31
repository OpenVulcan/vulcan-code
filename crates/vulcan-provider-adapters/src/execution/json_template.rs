//! Closed bounded JSON-template rendering shared by request and response mappings.
//! 请求与响应映射共享的封闭有界 JSON 模板渲染。

use serde_json::Value;

/// Maximum array entries emitted by one declarative JSON-template map expression.
/// 单个声明式 JSON 模板映射表达式允许输出的最大数组项数。
const JSON_TEMPLATE_MAX_ARRAY_ITEMS: usize = 256;

/// Maximum serialized byte size emitted by one declarative JSON template.
/// 单份声明式 JSON 模板允许输出的最大序列化字节数。
const JSON_TEMPLATE_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// One internal template rendering outcome, including an explicitly omitted optional value.
/// 一种内部模板渲染结果，包含被显式省略的可选值。
enum JsonTemplateRender {
    /// One concrete rendered JSON value.
    /// 一份具体的已渲染 JSON 值。
    Value(Value),
    /// One absent optional source omitted from its containing object or array.
    /// 一个从所属对象或数组中省略的缺失可选来源。
    Omitted,
}

/// Immutable source context available to one bounded JSON-template expression.
/// 单个有界 JSON 模板表达式可读取的不可变来源上下文。
struct JsonTemplateContext<'a> {
    /// Complete body captured before the rebuild step starts.
    /// 重建步骤开始前捕获的完整正文。
    body: &'a Value,
    /// Current array item while rendering a registered map expression.
    /// 渲染已注册映射表达式时的当前数组项。
    item: Option<&'a Value>,
    /// Exact immutable model or upstream target identifier.
    /// 精确不可变模型或上游目标标识。
    model_id: &'a str,
}

/// Renders one complete body from the closed declarative JSON-template language.
/// 使用封闭的声明式 JSON 模板语言渲染一份完整正文。
///
/// # Parameters
/// 参数
/// `template` is the manifest-owned template, `source` is the exact original body, and
/// `model_id` is the already frozen route identifier.
/// `template` 是 Manifest 持有的模板，`source` 是精确原始正文，`model_id` 是已冻结路由标识。
///
/// # Returns
/// 返回值
/// A bounded JSON value or one controlled validation/rendering error.
/// 返回有界 JSON 值或一条受控校验/渲染错误。
pub(super) fn render_complete_json_template(
    template: &Value,
    source: &Value,
    model_id: &str,
) -> Result<Value, String> {
    let context = JsonTemplateContext {
        body: source,
        item: None,
        model_id,
    };
    let rendered = match render_json_template_value(template, &context, 0)? {
        JsonTemplateRender::Value(value) => value,
        JsonTemplateRender::Omitted => {
            return Err(String::from(
                "A complete JSON template cannot resolve to an omitted optional value.",
            ));
        }
    };
    let serialized = serde_json::to_vec(&rendered)
        .map_err(|error| format!("Rendered JSON template could not be serialized: {error}"))?;
    if serialized.len() > JSON_TEMPLATE_MAX_OUTPUT_BYTES {
        return Err(format!(
            "Rendered JSON template exceeds the {JSON_TEMPLATE_MAX_OUTPUT_BYTES} byte limit."
        ));
    }
    Ok(rendered)
}

/// Renders one recursive declarative JSON-template node under fixed depth and array limits.
/// 在固定深度与数组限制下渲染一个递归声明式 JSON 模板节点。
fn render_json_template_value(
    template: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    if depth > crate::provider_manifest::MAX_JSON_DEPTH {
        return Err(format!(
            "JSON template exceeds the {} level depth limit.",
            crate::provider_manifest::MAX_JSON_DEPTH
        ));
    }
    match template {
        Value::Object(object) => {
            if let Some((tag, specification)) = json_template_registered_form(object)? {
                return render_json_template_form(tag, specification, context, depth + 1);
            }
            let mut rendered = serde_json::Map::new();
            for (key, value) in object {
                match render_json_template_value(value, context, depth + 1)? {
                    JsonTemplateRender::Value(value) => {
                        rendered.insert(key.clone(), value);
                    }
                    JsonTemplateRender::Omitted => {}
                }
            }
            Ok(JsonTemplateRender::Value(Value::Object(rendered)))
        }
        Value::Array(values) => {
            if values.len() > JSON_TEMPLATE_MAX_ARRAY_ITEMS {
                return Err(format!(
                    "JSON template literal array exceeds the {JSON_TEMPLATE_MAX_ARRAY_ITEMS} item limit."
                ));
            }
            let mut rendered = Vec::with_capacity(values.len());
            for value in values {
                if let JsonTemplateRender::Value(value) =
                    render_json_template_value(value, context, depth + 1)?
                {
                    rendered.push(value);
                }
            }
            Ok(JsonTemplateRender::Value(Value::Array(rendered)))
        }
        scalar => Ok(JsonTemplateRender::Value(scalar.clone())),
    }
}

/// Detects one exact registered template form and rejects ambiguous reserved-key objects.
/// 识别一个精确已注册模板形式，并拒绝含糊的保留键对象。
fn json_template_registered_form(
    object: &serde_json::Map<String, Value>,
) -> Result<Option<(&str, &Value)>, String> {
    let reserved = object
        .iter()
        .filter(|(key, _)| key.starts_with("$vulcan_"))
        .collect::<Vec<_>>();
    if reserved.is_empty() {
        return Ok(None);
    }
    if object.len() != 1 || reserved.len() != 1 {
        return Err(String::from(
            "A registered JSON template form must be the object's only property.",
        ));
    }
    let (tag, specification) = reserved[0];
    match tag.as_str() {
        "$vulcan_ref"
        | "$vulcan_model_id"
        | "$vulcan_map"
        | "$vulcan_enum"
        | "$vulcan_if_equals"
        | "$vulcan_if_present"
        | "$vulcan_concat"
        | "$vulcan_if_array_length"
        | "$vulcan_array_concat"
        | "$vulcan_coalesce"
        | "$vulcan_trim" => Ok(Some((tag.as_str(), specification))),
        _ => Err(format!("JSON template form `{tag}` is not registered.")),
    }
}

/// Executes one exact registered template form without dynamic code or reflective field lookup.
/// 在不执行动态代码或反射字段查找的情况下执行一个精确已注册模板形式。
fn render_json_template_form(
    tag: &str,
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    match tag {
        "$vulcan_ref" => render_json_template_reference(specification, context),
        "$vulcan_model_id" => {
            if specification != &Value::Bool(true) {
                return Err(String::from(
                    "$vulcan_model_id must carry the literal boolean true.",
                ));
            }
            Ok(JsonTemplateRender::Value(Value::String(
                context.model_id.to_string(),
            )))
        }
        "$vulcan_map" => render_json_template_map(specification, context, depth),
        "$vulcan_enum" => render_json_template_enum(specification, context),
        "$vulcan_if_equals" => render_json_template_condition(specification, context, depth),
        "$vulcan_if_present" => {
            render_json_template_presence_condition(specification, context, depth)
        }
        "$vulcan_if_array_length" => {
            render_json_template_array_length_condition(specification, context, depth)
        }
        "$vulcan_concat" => render_json_template_concat(specification, context, depth),
        "$vulcan_array_concat" => render_json_template_array_concat(specification, context, depth),
        "$vulcan_coalesce" => render_json_template_coalesce(specification, context, depth),
        "$vulcan_trim" => render_json_template_trimmed_string(specification, context),
        _ => Err(format!("JSON template form `{tag}` is not registered.")),
    }
}

/// Resolves one body or current-item JSON Pointer declared by a template form.
/// 解析模板形式声明的一个正文或当前项 JSON Pointer。
fn json_template_source<'a>(
    specification: &'a serde_json::Map<String, Value>,
    context: &'a JsonTemplateContext<'a>,
) -> Result<Option<&'a Value>, String> {
    let scope = specification
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("body");
    let path = specification
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| String::from("JSON template source requires a string `path`."))?;
    if !path.is_empty() && !path.starts_with('/') {
        return Err(String::from(
            "JSON template source path must be empty or an absolute JSON Pointer.",
        ));
    }
    let source = match scope {
        "body" => context.body,
        "item" => context.item.ok_or_else(|| {
            String::from("JSON template item scope is only available inside $vulcan_map.")
        })?,
        _ => {
            return Err(format!(
                "JSON template source scope `{scope}` is not registered."
            ));
        }
    };
    Ok(if path.is_empty() {
        Some(source)
    } else {
        source.pointer(path)
    })
}

/// Renders one required or optional JSON Pointer reference.
/// 渲染一个必需或可选的 JSON Pointer 引用。
fn render_json_template_reference(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_ref requires an object specification."))?;
    let optional = object
        .get("optional")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let omit_if_empty = object
        .get("omit_if_empty")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match json_template_source(object, context)? {
        Some(value)
            if !value.is_null()
                && !(omit_if_empty
                    && (matches!(value, Value::String(text) if text.is_empty())
                        || matches!(value, Value::Array(items) if items.is_empty())
                        || matches!(value, Value::Object(items) if items.is_empty()))) =>
        {
            Ok(JsonTemplateRender::Value(value.clone()))
        }
        _ if optional => Ok(JsonTemplateRender::Omitted),
        _ => Err(String::from(
            "$vulcan_ref could not resolve its required source value.",
        )),
    }
}

/// Resolves and trims one bounded string source without changing any other value type.
/// 解析并裁剪一个有界字符串源，不改变任何其他值类型。
///
/// # Parameters
/// 参数
/// `specification` declares the exact source pointer and optionality; `context` owns the frozen
/// body and current-item roots.
/// `specification` 声明精确源 Pointer 与可选性；`context` 持有冻结正文与当前条目根。
///
/// # Returns
/// 返回值
/// A non-empty trimmed string, an omitted optional value, or a deterministic type/presence error.
/// 返回非空裁剪字符串、省略的可选值，或确定性的类型与存在性错误。
fn render_json_template_trimmed_string(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_trim requires an object specification."))?;
    let optional = object
        .get("optional")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(value) = json_template_source(object, context)? else {
        return if optional {
            Ok(JsonTemplateRender::Omitted)
        } else {
            Err(String::from(
                "$vulcan_trim could not resolve its required source value.",
            ))
        };
    };
    let trimmed = value
        .as_str()
        .ok_or_else(|| String::from("$vulcan_trim source must be a string."))?
        .trim();
    if trimmed.is_empty() {
        return if optional {
            Ok(JsonTemplateRender::Omitted)
        } else {
            Err(String::from(
                "$vulcan_trim source must contain non-whitespace text.",
            ))
        };
    }
    Ok(JsonTemplateRender::Value(Value::String(
        trimmed.to_string(),
    )))
}

/// Renders one bounded source array through a nested template for each item.
/// 使用嵌套模板逐项渲染一个有界来源数组。
fn render_json_template_map(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_map requires an object specification."))?;
    let template = object
        .get("template")
        .ok_or_else(|| String::from("$vulcan_map requires a `template`."))?;
    let optional = object
        .get("optional")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let flatten = object
        .get("flatten")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(source) = json_template_source(object, context)? else {
        return if optional {
            Ok(JsonTemplateRender::Omitted)
        } else {
            Err(String::from(
                "$vulcan_map could not resolve its source array.",
            ))
        };
    };
    let items = source
        .as_array()
        .ok_or_else(|| String::from("$vulcan_map source must be an array."))?;
    if items.len() > JSON_TEMPLATE_MAX_ARRAY_ITEMS {
        return Err(format!(
            "$vulcan_map source exceeds the {JSON_TEMPLATE_MAX_ARRAY_ITEMS} item limit."
        ));
    }
    let mut rendered = Vec::with_capacity(items.len());
    for item in items {
        let item_context = JsonTemplateContext {
            body: context.body,
            item: Some(item),
            model_id: context.model_id,
        };
        if let JsonTemplateRender::Value(value) =
            render_json_template_value(template, &item_context, depth + 1)?
        {
            if flatten {
                let values = value.as_array().ok_or_else(|| {
                    String::from("$vulcan_map with flatten=true must render arrays.")
                })?;
                if rendered.len().saturating_add(values.len()) > JSON_TEMPLATE_MAX_ARRAY_ITEMS {
                    return Err(format!(
                        "$vulcan_map flattened output exceeds the {JSON_TEMPLATE_MAX_ARRAY_ITEMS} item limit."
                    ));
                }
                rendered.extend(values.iter().cloned());
            } else {
                rendered.push(value);
            }
        }
    }
    Ok(JsonTemplateRender::Value(Value::Array(rendered)))
}

/// Maps one exact string source through a bounded literal table.
/// 通过有界字面量表映射一个精确字符串来源。
fn render_json_template_enum(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_enum requires an object specification."))?;
    let mapping = object
        .get("mapping")
        .and_then(Value::as_object)
        .ok_or_else(|| String::from("$vulcan_enum requires an object `mapping`."))?;
    let optional = object
        .get("optional")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(source) = json_template_source(object, context)? else {
        return match object.get("default") {
            Some(value) => Ok(JsonTemplateRender::Value(value.clone())),
            None if optional => Ok(JsonTemplateRender::Omitted),
            None => Err(String::from("$vulcan_enum could not resolve its source.")),
        };
    };
    let source = source
        .as_str()
        .ok_or_else(|| String::from("$vulcan_enum source must be a string."))?;
    match mapping.get(source).or_else(|| object.get("default")) {
        Some(value) => Ok(JsonTemplateRender::Value(value.clone())),
        None if optional => Ok(JsonTemplateRender::Omitted),
        None => Err(format!(
            "$vulcan_enum source value `{source}` is not in its mapping table."
        )),
    }
}

/// Selects and renders one of two templates using exact JSON equality.
/// 使用精确 JSON 相等判断选择并渲染两个模板之一。
fn render_json_template_condition(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_if_equals requires an object specification."))?;
    let expected = object
        .get("equals")
        .ok_or_else(|| String::from("$vulcan_if_equals requires `equals`."))?;
    let selected = if json_template_source(object, context)? == Some(expected) {
        object.get("then")
    } else {
        object.get("otherwise")
    };
    match selected {
        Some(template) => render_json_template_value(template, context, depth + 1),
        None => Ok(JsonTemplateRender::Omitted),
    }
}

/// Renders one nested template only when its exact JSON Pointer resolves to a non-null value.
/// 仅当精确 JSON Pointer 解析为非 null 值时渲染一个嵌套模板。
///
/// # Parameters
/// 参数
/// `specification` declares the source and the required `then` template; `context` is immutable.
/// `specification` 声明来源与必需的 `then` 模板；`context` 保持不可变。
///
/// # Returns
/// 返回值
/// The rendered branch when present, otherwise an explicit omission marker.
/// 当来源存在时返回已渲染分支，否则返回显式省略标记。
fn render_json_template_presence_condition(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_if_present requires an object specification."))?;
    if !object.contains_key("then") && !object.contains_key("otherwise") {
        return Err(String::from(
            "$vulcan_if_present requires `then` or `otherwise`.",
        ));
    }
    let non_empty = object
        .get("non_empty")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let selected = match json_template_source(object, context)? {
        Some(value)
            if !value.is_null()
                && !(non_empty
                    && (matches!(value, Value::String(text) if text.is_empty())
                        || matches!(value, Value::Array(items) if items.is_empty())
                        || matches!(value, Value::Object(items) if items.is_empty()))) =>
        {
            object.get("then")
        }
        _ => object.get("otherwise"),
    };
    match selected {
        Some(template) => render_json_template_value(template, context, depth + 1),
        None => Ok(JsonTemplateRender::Omitted),
    }
}

/// Selects one template branch by the exact length of a resolved JSON array.
/// 按已解析 JSON 数组的精确长度选择一个模板分支。
///
/// # Parameters
/// 参数
/// `specification` declares the source, expected length, and optional branches.
/// `specification` 声明来源、期望长度与可选分支。
///
/// # Returns
/// 返回值
/// The selected rendered branch, or omission when the selected branch is absent.
/// 返回所选已渲染分支；所选分支缺失时返回省略标记。
fn render_json_template_array_length_condition(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_if_array_length requires an object specification."))?;
    let expected = object
        .get("equals")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value <= JSON_TEMPLATE_MAX_ARRAY_ITEMS)
        .ok_or_else(|| {
            format!(
                "$vulcan_if_array_length equals must be an integer from 0 through {JSON_TEMPLATE_MAX_ARRAY_ITEMS}."
            )
        })?;
    let actual = json_template_source(object, context)?
        .and_then(Value::as_array)
        .map(Vec::len);
    let selected = if actual == Some(expected) {
        object.get("then")
    } else {
        object.get("otherwise")
    };
    match selected {
        Some(template) => render_json_template_value(template, context, depth + 1),
        None => Ok(JsonTemplateRender::Omitted),
    }
}

/// Concatenates rendered scalar template values into one bounded string.
/// 把已渲染的标量模板值拼接为一个有界字符串。
fn render_json_template_concat(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let values = specification
        .as_array()
        .ok_or_else(|| String::from("$vulcan_concat requires an array specification."))?;
    if values.len() > JSON_TEMPLATE_MAX_ARRAY_ITEMS {
        return Err(format!(
            "$vulcan_concat exceeds the {JSON_TEMPLATE_MAX_ARRAY_ITEMS} item limit."
        ));
    }
    let mut output = String::new();
    for value in values {
        let rendered = match render_json_template_value(value, context, depth + 1)? {
            JsonTemplateRender::Value(value) => value,
            JsonTemplateRender::Omitted => continue,
        };
        match rendered {
            Value::String(value) => output.push_str(value.as_str()),
            Value::Number(value) => output.push_str(value.to_string().as_str()),
            Value::Bool(value) => output.push_str(if value { "true" } else { "false" }),
            _ => {
                return Err(String::from(
                    "$vulcan_concat items must render to strings, numbers, or booleans.",
                ));
            }
        }
        if output.len() > JSON_TEMPLATE_MAX_OUTPUT_BYTES {
            return Err(format!(
                "$vulcan_concat exceeds the {JSON_TEMPLATE_MAX_OUTPUT_BYTES} byte limit."
            ));
        }
    }
    Ok(JsonTemplateRender::Value(Value::String(output)))
}

/// Concatenates rendered array templates into one bounded ordered array.
/// 把已渲染的数组模板拼接为一个有界有序数组。
///
/// # Parameters
/// 参数
/// `specification` is an ordered bounded template list and `context` is the immutable source.
/// `specification` 是有序有界模板列表，`context` 是不可变来源。
///
/// # Returns
/// 返回值
/// One flattened array whose total item count never exceeds the shared template ceiling.
/// 返回一个扁平数组，其总元素数量永不超过共享模板上限。
fn render_json_template_array_concat(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let templates = specification
        .as_array()
        .filter(|templates| {
            !templates.is_empty() && templates.len() <= JSON_TEMPLATE_MAX_ARRAY_ITEMS
        })
        .ok_or_else(|| {
            format!(
                "$vulcan_array_concat requires between 1 and {JSON_TEMPLATE_MAX_ARRAY_ITEMS} templates."
            )
        })?;
    let mut output = Vec::new();
    for template in templates {
        let rendered = match render_json_template_value(template, context, depth + 1)? {
            JsonTemplateRender::Value(value) => value,
            JsonTemplateRender::Omitted => continue,
        };
        let values = rendered
            .as_array()
            .ok_or_else(|| String::from("$vulcan_array_concat templates must render to arrays."))?;
        if output.len().saturating_add(values.len()) > JSON_TEMPLATE_MAX_ARRAY_ITEMS {
            return Err(format!(
                "$vulcan_array_concat exceeds the {JSON_TEMPLATE_MAX_ARRAY_ITEMS} item limit."
            ));
        }
        output.extend(values.iter().cloned());
    }
    Ok(JsonTemplateRender::Value(Value::Array(output)))
}

/// Selects the first non-omitted value from an ordered bounded template list.
/// 从一份有序有界模板列表中选择首个未省略值。
///
/// # Parameters
/// 参数
/// `specification` is the closed ordered template array and `context` is the immutable source.
/// `specification` 是封闭有序模板数组，`context` 是不可变来源。
///
/// # Returns
/// 返回值
/// The first rendered value, or `Omitted` when every candidate is explicitly optional and absent.
/// 返回首个已渲染值；当每个候选均显式可选且缺失时返回 `Omitted`。
fn render_json_template_coalesce(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    // Candidate count is bounded by the same collection ceiling as literal arrays and maps.
    // 候选数量与字面数组、映射使用相同集合上限。
    let candidates = specification
        .as_array()
        .filter(|candidates| {
            !candidates.is_empty() && candidates.len() <= JSON_TEMPLATE_MAX_ARRAY_ITEMS
        })
        .ok_or_else(|| {
            format!(
                "$vulcan_coalesce requires between 1 and {JSON_TEMPLATE_MAX_ARRAY_ITEMS} candidates."
            )
        })?;
    for candidate in candidates {
        match render_json_template_value(candidate, context, depth + 1)? {
            JsonTemplateRender::Value(value) => {
                return Ok(JsonTemplateRender::Value(value));
            }
            JsonTemplateRender::Omitted => {}
        }
    }
    Ok(JsonTemplateRender::Omitted)
}

#[cfg(test)]
mod tests;
