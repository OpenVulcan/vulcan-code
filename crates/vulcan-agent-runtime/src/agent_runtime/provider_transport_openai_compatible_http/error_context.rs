use super::*;

pub(super) fn augment_openai_compatible_error_with_tool_context(
    base_message: String,
    body: &str,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> String {
    let tool_indices = collect_openai_compatible_function_declaration_indices(body);
    if tool_indices.is_empty() {
        return base_message;
    }

    let mut tool_segments = Vec::new();
    for index in tool_indices {
        let Some(definition) = provider_tool_definitions.get(index) else {
            continue;
        };
        tool_segments.push(format!("tool[{index}]={}", definition.name));
    }

    if tool_segments.is_empty() {
        return base_message;
    }

    format!("{base_message} | {}", tool_segments.join(" | "))
}

/// Collects unique upstream function-declaration indices from one provider error body by scanning for `function_declarations[n]` fragments.
/// 通过扫描 `function_declarations[n]` 片段，从单条 Provider 错误正文中收集唯一的上游函数声明索引。
pub(super) fn collect_openai_compatible_function_declaration_indices(body: &str) -> Vec<usize> {
    let needle = "function_declarations[";
    let mut indices = BTreeSet::new();
    let mut search_offset = 0usize;

    while let Some(relative_start) = body[search_offset..].find(needle) {
        let start = search_offset + relative_start + needle.len();
        let Some(relative_end) = body[start..].find(']') else {
            break;
        };
        let end = start + relative_end;
        if let Ok(index) = body[start..end].trim().parse::<usize>() {
            indices.insert(index);
        }
        search_offset = end + 1;
    }

    indices.into_iter().collect()
}

/// Serializes one tool-arguments JSON value into the OpenAI-compatible string form expected inside assistant tool calls.
/// 将单个工具参数 JSON 值序列化为助手工具调用中期望的 OpenAI 兼容字符串形式。
pub(super) fn stringify_openai_compatible_tool_arguments(arguments: &Value) -> String {
    match arguments {
        Value::String(value) => value.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}
