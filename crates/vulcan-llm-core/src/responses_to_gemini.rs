//! Responses to Gemini GenerateContent conversion based on the shared provider rules.
//! 基于共享供应商规则实现 Responses 到 Gemini GenerateContent 的转换。

use crate::{LlmCoreError, reasoning::canonical_reasoning_effort};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// Converts one canonical Responses request into Gemini or Code Assist GenerateContent JSON.
/// 将一份规范 Responses 请求转换为 Gemini 或 Code Assist GenerateContent JSON。
pub fn responses_to_gemini_generate_content(
    body: Value,
    code_assist: bool,
) -> Result<Value, LlmCoreError> {
    responses_to_gemini_generate_content_with_media(body, code_assist, None)
}

/// Converts `body` with the declared `code_assist` envelope and optional trusted internal `media`.
/// 使用声明的 `code_assist` 封装与可选可信内部 `media` 转换 `body`。
/// Returns Gemini JSON or an exact projection error; no protocol selection is inferred.
/// 返回 Gemini JSON 或精确投影错误，不推断协议选择。
pub(crate) fn responses_to_gemini_generate_content_with_media(
    body: Value,
    code_assist: bool,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<Value, LlmCoreError> {
    let model = required_string(&body, "/model", "Responses model")?;
    let mut function_names = BTreeMap::new();
    collect_function_names(body.get("input"), &mut function_names)?;

    let mut system_parts = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        append_instruction_parts(instructions, &mut system_parts)?;
    }
    let mut contents = Vec::new();
    if let Some(input) = body.get("input") {
        append_gemini_input(
            input,
            &mut system_parts,
            &mut contents,
            &function_names,
            media,
        )?;
    }
    normalize_gemini_contents(&mut contents)?;

    let mut request = Map::new();
    request.insert(String::from("contents"), Value::Array(contents));
    if !system_parts.is_empty() {
        request.insert(
            String::from("systemInstruction"),
            json!({"parts":system_parts}),
        );
    }
    if let Some(tools) = body.get("tools") {
        let mapped_tools = map_gemini_tools(tools)?;
        if !mapped_tools.is_empty() {
            request.insert(String::from("tools"), Value::Array(mapped_tools));
        }
    }
    if let Some(choice) = body.get("tool_choice")
        && request
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| {
                tools
                    .iter()
                    .any(|tool| tool.get("functionDeclarations").is_some())
            })
    {
        request.insert(String::from("toolConfig"), map_gemini_tool_choice(choice)?);
    }
    project_gemini_controls(&body, &mut request)?;

    if code_assist {
        Ok(json!({"model":model,"request":Value::Object(request)}))
    } else {
        Ok(Value::Object(request))
    }
}

/// Converts one canonical Responses request into the revised Gemini Interactions schema.
/// 将一份规范 Responses 请求转换为修订版 Gemini Interactions 结构。
pub fn responses_to_gemini_interactions(body: Value) -> Result<Value, LlmCoreError> {
    responses_to_gemini_interactions_with_media(body, None)
}

/// Converts `body` into Interactions while using `media` only for trusted internal resources.
/// 将 `body` 转换为 Interactions，仅对可信内部资源使用 `media`。
/// Returns the exact Interactions shape or an unsupported resource/control error.
/// 返回精确 Interactions 结构，或不支持的资源与控制项错误。
pub(crate) fn responses_to_gemini_interactions_with_media(
    body: Value,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<Value, LlmCoreError> {
    let model = required_string(&body, "/model", "Responses model")?.to_string();
    let generated = responses_to_gemini_generate_content_with_media(body, false, media)?;
    let input =
        gemini_contents_to_interaction_steps(generated.get("contents").unwrap_or(&Value::Null))?;
    let mut interaction = json!({
        "model": model,
        "input": input,
        "stream": true,
        "store": false,
    });
    if let Some(system) = gemini_system_instruction_text(generated.get("systemInstruction"))? {
        interaction["system_instruction"] = Value::String(system);
    }
    if let Some(tools) = generated.get("tools") {
        let tools = gemini_tools_to_interactions(tools)?;
        if !tools.is_empty() {
            interaction["tools"] = Value::Array(tools);
        }
    }
    project_interaction_generation_controls(&generated, &mut interaction)?;
    Ok(interaction)
}

/// Joins GenerateContent system text parts into the Interactions instruction string.
/// 将 GenerateContent 系统文本 Part 合并为 Interactions 指令字符串。
fn gemini_system_instruction_text(value: Option<&Value>) -> Result<Option<String>, LlmCoreError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let parts = value
        .get("parts")
        .and_then(Value::as_array)
        .ok_or_else(|| LlmCoreError::new("Gemini systemInstruction.parts must be an array."))?;
    let mut text = Vec::new();
    for part in parts {
        text.push(
            part.get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    LlmCoreError::new("Gemini Interactions system instruction must be text.")
                })?
                .to_string(),
        );
    }
    Ok((!text.is_empty()).then(|| text.join("\n")))
}

/// Converts ordered GenerateContent entries into ordered Interactions steps.
/// 将有序 GenerateContent 内容转换为有序 Interactions Step。
fn gemini_contents_to_interaction_steps(value: &Value) -> Result<Vec<Value>, LlmCoreError> {
    let contents = value
        .as_array()
        .ok_or_else(|| LlmCoreError::new("Gemini contents must be an array."))?;
    let mut steps = Vec::new();
    for content in contents {
        let role = required_string(content, "/role", "Gemini content role")?;
        let step_type = match role {
            "user" => "user_input",
            "model" => "model_output",
            other => {
                return Err(LlmCoreError::new(format!(
                    "Gemini content role `{other}` cannot be projected to Interactions."
                )));
            }
        };
        let parts = content
            .get("parts")
            .and_then(Value::as_array)
            .ok_or_else(|| LlmCoreError::new("Gemini content parts must be an array."))?;
        let mut ordinary_parts = Vec::new();
        for part in parts {
            if part.get("thought").and_then(Value::as_bool) == Some(true) {
                flush_interaction_content_step(&mut steps, step_type, &mut ordinary_parts);
                let text = required_string(part, "/text", "Gemini thought text")?;
                let mut thought = json!({
                    "type": "thought",
                    "content": [{"type": "text", "text": text}],
                });
                if let Some(signature) = part
                    .get("thoughtSignature")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    thought["signature"] = Value::String(signature.to_string());
                }
                steps.push(thought);
            } else if let Some(call) = part.get("functionCall") {
                flush_interaction_content_step(&mut steps, step_type, &mut ordinary_parts);
                steps.push(json!({
                    "type": "function_call",
                    "call_id": required_string(call, "/id", "Gemini function call id")?,
                    "name": required_string(call, "/name", "Gemini function call name")?,
                    "arguments": call.get("args").cloned().unwrap_or_else(|| json!({})),
                }));
            } else if let Some(response) = part.get("functionResponse") {
                flush_interaction_content_step(&mut steps, step_type, &mut ordinary_parts);
                let result = response
                    .pointer("/response/result")
                    .cloned()
                    .unwrap_or(Value::Null);
                steps.push(json!({
                    "type": "function_result",
                    "call_id": required_string(response, "/id", "Gemini function response id")?,
                    "name": required_string(response, "/name", "Gemini function response name")?,
                    "result": result,
                }));
            } else {
                ordinary_parts.push(gemini_part_to_interaction_content(part)?);
            }
        }
        flush_interaction_content_step(&mut steps, step_type, &mut ordinary_parts);
    }
    Ok(steps)
}

/// Flushes one non-empty group of ordinary Interactions content parts.
/// 刷新一组非空的普通 Interactions 内容 Part。
fn flush_interaction_content_step(steps: &mut Vec<Value>, step_type: &str, parts: &mut Vec<Value>) {
    if parts.is_empty() {
        return;
    }
    steps.push(json!({"type": step_type, "content": std::mem::take(parts)}));
}

/// Converts one ordinary GenerateContent part into Interactions content.
/// 将一个普通 GenerateContent Part 转换为 Interactions 内容。
fn gemini_part_to_interaction_content(part: &Value) -> Result<Value, LlmCoreError> {
    if let Some(text) = part.get("text").and_then(Value::as_str) {
        return Ok(json!({"type": "text", "text": text}));
    }
    if let Some(inline) = part.get("inlineData") {
        let mime_type = required_string(inline, "/mimeType", "Gemini inline MIME type")?;
        let data = required_string(inline, "/data", "Gemini inline data")?;
        let kind = if mime_type.starts_with("image/") {
            "image"
        } else if mime_type.starts_with("audio/") {
            "audio"
        } else if mime_type.starts_with("video/") {
            "video"
        } else {
            "document"
        };
        return Ok(json!({
            "type": kind,
            "mime_type": mime_type,
            "data": data,
        }));
    }
    Err(LlmCoreError::new(
        "Gemini content part cannot be projected to Interactions.",
    ))
}

/// Converts GenerateContent tool declarations and hosted tools into Interactions tools.
/// 将 GenerateContent 函数声明与托管工具转换为 Interactions 工具。
fn gemini_tools_to_interactions(value: &Value) -> Result<Vec<Value>, LlmCoreError> {
    let tools = value
        .as_array()
        .ok_or_else(|| LlmCoreError::new("Gemini tools must be an array."))?;
    let mut converted = Vec::new();
    for tool in tools {
        if let Some(declarations) = tool.get("functionDeclarations") {
            let declarations = declarations.as_array().ok_or_else(|| {
                LlmCoreError::new("Gemini functionDeclarations must be an array.")
            })?;
            for declaration in declarations {
                let mut converted_tool = json!({
                    "type": "function",
                    "name": required_string(declaration, "/name", "Gemini function name")?,
                    "parameters": declaration.get("parameters").cloned().unwrap_or_else(|| json!({"type":"object"})),
                });
                if let Some(description) = declaration.get("description") {
                    converted_tool["description"] = description.clone();
                }
                converted.push(converted_tool);
            }
        } else if tool.get("googleSearch").is_some() {
            converted.push(json!({"type": "google_search"}));
        } else if tool.get("codeExecution").is_some() {
            converted.push(json!({"type": "code_execution"}));
        } else {
            return Err(LlmCoreError::new(
                "Gemini hosted tool cannot be projected to Interactions.",
            ));
        }
    }
    Ok(converted)
}

/// Projects GenerateContent generation and tool controls into Interactions fields.
/// 将 GenerateContent 生成控制与工具控制投影为 Interactions 字段。
fn project_interaction_generation_controls(
    generated: &Value,
    interaction: &mut Value,
) -> Result<(), LlmCoreError> {
    let mut generation = Map::new();
    if let Some(config) = generated.get("generationConfig") {
        let config = config
            .as_object()
            .ok_or_else(|| LlmCoreError::new("Gemini generationConfig must be an object."))?;
        for (source, target) in [
            ("maxOutputTokens", "max_output_tokens"),
            ("stopSequences", "stop_sequences"),
            ("temperature", "temperature"),
            ("topP", "top_p"),
        ] {
            if let Some(value) = config.get(source) {
                generation.insert(String::from(target), value.clone());
            }
        }
        if let Some(thinking) = config.get("thinkingConfig") {
            if let Some(level) = thinking.get("thinkingLevel") {
                let level = level.as_str().ok_or_else(|| {
                    LlmCoreError::new("Gemini thinkingLevel must be a string enum.")
                })?;
                generation.insert(
                    String::from("thinking_level"),
                    Value::String(level.to_ascii_lowercase()),
                );
            }
            if let Some(include) = thinking.get("includeThoughts").and_then(Value::as_bool) {
                generation.insert(
                    String::from("thinking_summaries"),
                    Value::String(if include { "auto" } else { "none" }.to_string()),
                );
            }
        }
        if let Some(mime_type) = config.get("responseMimeType") {
            let mut format = json!({"type": "text", "mime_type": mime_type});
            if let Some(schema) = config.get("responseJsonSchema") {
                format["schema"] = schema.clone();
            }
            interaction["response_format"] = format;
        }
    }
    if let Some(tool_config) = generated.pointer("/toolConfig/functionCallingConfig") {
        match tool_config.get("mode").and_then(Value::as_str) {
            Some("AUTO") | None => {}
            Some("ANY") => {
                if let Some(names) = tool_config.get("allowedFunctionNames") {
                    generation.insert(
                        String::from("tool_choice"),
                        json!({"allowed_tools": {"mode": "any", "tools": names}}),
                    );
                } else {
                    generation.insert(String::from("tool_choice"), json!("any"));
                }
            }
            Some("NONE") => {
                generation.insert(String::from("tool_choice"), json!("none"));
            }
            Some(other) => {
                return Err(LlmCoreError::new(format!(
                    "Gemini function calling mode `{other}` cannot be projected to Interactions."
                )));
            }
        }
    }
    if !generation.is_empty() {
        interaction["generation_config"] = Value::Object(generation);
    }
    Ok(())
}

/// Reads one required non-empty string from a JSON object path.
/// 从 JSON 对象路径读取一个必需的非空字符串。
fn required_string<'a>(body: &'a Value, path: &str, label: &str) -> Result<&'a str, LlmCoreError> {
    body.pointer(path)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| LlmCoreError::new(format!("{label} is missing or empty at `{path}`.")))
}

/// Collects function-call names so Gemini function responses retain their exact identity.
/// 收集函数调用名称，使 Gemini 函数响应保留精确身份。
fn collect_function_names(
    input: Option<&Value>,
    names: &mut BTreeMap<String, String>,
) -> Result<(), LlmCoreError> {
    let Some(Value::Array(items)) = input else {
        return Ok(());
    };
    for item in items {
        if item.get("type").and_then(Value::as_str) == Some("function_call") {
            let id = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    LlmCoreError::new("Responses function_call is missing `call_id`.")
                })?;
            let name = required_string(item, "/name", "Responses function_call name")?;
            names.insert(id.to_string(), name.to_string());
        }
    }
    Ok(())
}

/// Appends instruction values as Gemini text parts.
/// 将指令值追加为 Gemini 文本 Part。
fn append_instruction_parts(value: &Value, parts: &mut Vec<Value>) -> Result<(), LlmCoreError> {
    match value {
        Value::String(text) => {
            if !text.is_empty() {
                parts.push(json!({"text":text}));
            }
        }
        Value::Array(values) => {
            for value in values {
                if let Some(text) = value.get("text").and_then(Value::as_str) {
                    parts.push(json!({"text":text}));
                } else if let Some(text) = value.as_str() {
                    parts.push(json!({"text":text}));
                } else {
                    return Err(LlmCoreError::new(
                        "Gemini instructions contains a non-text part.",
                    ));
                }
            }
        }
        Value::Null => {}
        _ => {
            return Err(LlmCoreError::new(
                "Gemini instructions must be a string or text array.",
            ));
        }
    }
    Ok(())
}

/// Converts one Responses input value into Gemini system parts and contents.
/// 将一个 Responses input 值转换为 Gemini system parts 与 contents。
fn append_gemini_input(
    input: &Value,
    system_parts: &mut Vec<Value>,
    contents: &mut Vec<Value>,
    function_names: &BTreeMap<String, String>,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<(), LlmCoreError> {
    match input {
        Value::String(text) => contents.push(json!({"role":"user","parts":[{"text":text}]})),
        Value::Array(items) => {
            for item in items {
                append_gemini_item(item, system_parts, contents, function_names, media)?;
            }
        }
        Value::Object(_) => {
            append_gemini_item(input, system_parts, contents, function_names, media)?
        }
        Value::Null => {}
        _ => {
            return Err(LlmCoreError::new(
                "Responses input must be a string, object, or array for Gemini conversion.",
            ));
        }
    }
    Ok(())
}

/// Converts one Responses item into one or more Gemini content parts.
/// 将一个 Responses 项转换为一个或多个 Gemini 内容 Part。
fn append_gemini_item(
    item: &Value,
    system_parts: &mut Vec<Value>,
    contents: &mut Vec<Value>,
    function_names: &BTreeMap<String, String>,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<(), LlmCoreError> {
    let kind = item
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    match kind {
        "input_text" => push_gemini_part(
            contents,
            "user",
            json!({"text":required_string(item,"/text","Responses input_text")?}),
        ),
        "input_image" => push_gemini_part(contents, "user", gemini_image_part(item)?),
        "function_call" => {
            let name = required_string(item, "/name", "Responses function_call name")?;
            let call_id = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    LlmCoreError::new("Responses function_call is missing `call_id`.")
                })?;
            let arguments = object_arguments(item.get("arguments"), name)?;
            let mut part = json!({"functionCall":{"id":call_id,"name":name,"args":arguments}});
            if let Some(signature) = item
                .get("x_vulcan_provider_signature")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                part["thoughtSignature"] = Value::String(signature.to_string());
            }
            push_gemini_part(contents, "model", part);
        }
        "function_call_output" => {
            let call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    LlmCoreError::new("Responses function_call_output is missing `call_id`.")
                })?;
            let name = function_names.get(call_id).ok_or_else(|| {
                LlmCoreError::new(format!(
                    "Gemini function response `{call_id}` has no preceding function name."
                ))
            })?;
            let response = item.get("output").cloned().unwrap_or(Value::Null);
            push_gemini_part(
                contents,
                "user",
                json!({"functionResponse":{"id":call_id,"name":name,"response":{"result":response}}}),
            );
        }
        "reasoning" => {
            let text = item
                .pointer("/summary/0/text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !text.is_empty() {
                let mut part = json!({"text":text,"thought":true});
                if let Some(signature) = item
                    .get("encrypted_content")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    part["thoughtSignature"] = Value::String(signature.to_string());
                }
                push_gemini_part(contents, "model", part);
            }
        }
        "message" => {
            let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
            if matches!(role, "system" | "developer") {
                append_message_parts(item, system_parts, "system", function_names, media)?;
            } else {
                let gemini_role = if role == "assistant" { "model" } else { "user" };
                append_message_parts(item, contents, gemini_role, function_names, media)?;
            }
        }
        "input_file" => push_gemini_part(contents, "user", gemini_file_part(item)?),
        other => {
            return Err(LlmCoreError::new(format!(
                "Responses input item type `{other}` is not supported by Gemini conversion."
            )));
        }
    }
    Ok(())
}

/// Converts a role-bearing Responses message into Gemini parts.
/// 将一个带角色的 Responses 消息转换为 Gemini Part。
fn append_message_parts(
    item: &Value,
    target: &mut Vec<Value>,
    role: &str,
    _function_names: &BTreeMap<String, String>,
    media: Option<crate::internal_media::InternalMediaProjector<'_>>,
) -> Result<(), LlmCoreError> {
    let content = item.get("content").unwrap_or(&Value::Null);
    match content {
        Value::String(text) if role == "system" => target.push(json!({"text":text})),
        Value::String(text) => target.push(json!({"role":role,"parts":[{"text":text}]})),
        Value::Array(parts) => {
            let mut converted = Vec::new();
            for part in parts {
                let kind = part
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("input_text");
                match kind {
                    "input_text" | "output_text" | "text" => {
                        converted.push(
                            json!({"text":required_string(part,"/text","Responses text part")?}),
                        );
                    }
                    "input_image" | "image_url" => converted.push(gemini_image_part(part)?),
                    "input_file" => converted.push(gemini_file_part(part)?),
                    crate::INTERNAL_RESOURCE_PART_TYPE => converted.push(
                        crate::internal_media::project_internal_part(part, role, media)?,
                    ),
                    "refusal" => converted.push(
                        json!({"text":required_string(part,"/refusal","Responses refusal")?}),
                    ),
                    other => {
                        return Err(LlmCoreError::new(format!(
                            "Responses message content type `{other}` is not supported by Gemini conversion."
                        )));
                    }
                }
            }
            if role == "system" {
                target.extend(converted);
            } else if !converted.is_empty() {
                target.push(json!({"role":role,"parts":converted}));
            }
        }
        Value::Null => {}
        _ => {
            return Err(LlmCoreError::new(
                "Responses message content must be a string or array for Gemini conversion.",
            ));
        }
    }
    Ok(())
}

/// Creates one Gemini inline-data or remote-file part from an image URL.
/// 从图片 URL 创建一个 Gemini 内联数据或远程文件 Part。
fn gemini_image_part(item: &Value) -> Result<Value, LlmCoreError> {
    if item.get("detail").is_some() {
        return Err(LlmCoreError::new(
            "Gemini conversion cannot preserve Responses image detail.",
        ));
    }
    if item.get("file_id").is_some() {
        return Err(LlmCoreError::new(
            "Responses image file_id cannot be projected across providers to Gemini.",
        ));
    }
    let url = item
        .get("image_url")
        .or_else(|| item.get("url"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| LlmCoreError::new("Responses input_image is missing `image_url`."))?;
    if let Some((media_type, data)) = parse_data_url(url) {
        return Ok(json!({"inlineData":{"mimeType":media_type,"data":data}}));
    }
    Err(LlmCoreError::new(
        "Gemini conversion requires remote image URLs to be host-resolved into inline data with an exact MIME type.",
    ))
}

/// Creates one Gemini inline-data file part from standard Responses file_data.
/// 根据标准 Responses file_data 创建一个 Gemini 内联数据文件 Part。
fn gemini_file_part(item: &Value) -> Result<Value, LlmCoreError> {
    if item.get("file_id").is_some() {
        return Err(LlmCoreError::new(
            "Responses file_id cannot be projected across providers to Gemini.",
        ));
    }
    if item.get("file_url").is_some() {
        return Err(LlmCoreError::new(
            "Gemini conversion requires remote file URLs to be host-resolved into inline data with an exact MIME type.",
        ));
    }
    let data_url = item
        .get("file_data")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| LlmCoreError::new("Responses input_file is missing `file_data`."))?;
    let (media_type, data) = parse_data_url(data_url).ok_or_else(|| {
        LlmCoreError::new("Gemini file_data conversion requires a Base64 data URL with MIME type.")
    })?;
    Ok(json!({"inlineData":{"mimeType":media_type,"data":data}}))
}

/// Parses a base64 data URL into its media type and encoded data.
/// 将 base64 data URL 解析为媒体类型与编码数据。
fn parse_data_url(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix("data:")?;
    let (metadata, data) = rest.split_once(',')?;
    let media_type = metadata.split(';').next()?.trim();
    metadata
        .split(';')
        .any(|part| part == "base64")
        .then_some((media_type, data))
}

/// Parses function arguments and requires a JSON object for Gemini functionCall.
/// 解析函数参数并要求 Gemini functionCall 使用 JSON 对象。
fn object_arguments(value: Option<&Value>, name: &str) -> Result<Value, LlmCoreError> {
    let empty_arguments = Value::Object(Map::new());
    let value = value.unwrap_or(&empty_arguments);
    let parsed = match value {
        Value::String(text) if text.trim().is_empty() => json!({}),
        Value::String(text) => serde_json::from_str::<Value>(text).map_err(|error| {
            LlmCoreError::new(format!(
                "Responses function_call `{name}` arguments are invalid JSON: {error}"
            ))
        })?,
        other => other.clone(),
    };
    if !parsed.is_object() {
        return Err(LlmCoreError::new(format!(
            "Responses function_call `{name}` arguments must be a JSON object."
        )));
    }
    Ok(parsed)
}

/// Maps Responses tools into Gemini function declarations.
/// 将 Responses 工具转换为 Gemini 函数声明。
fn map_gemini_tools(tools: &Value) -> Result<Vec<Value>, LlmCoreError> {
    let values = tools
        .as_array()
        .ok_or_else(|| LlmCoreError::new("Responses tools must be an array."))?;
    let mut declarations = Vec::new();
    let mut mapped = Vec::new();
    for tool in values {
        let kind = tool
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("function");
        match kind {
            "function" => {
                if tool.get("strict").and_then(Value::as_bool) == Some(true) {
                    return Err(LlmCoreError::new(
                        "Gemini conversion cannot preserve strict function-tool semantics.",
                    ));
                }
                let name = tool
                    .get("name")
                    .or_else(|| tool.pointer("/function/name"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        LlmCoreError::new("Responses function tool is missing `name`.")
                    })?;
                let parameters = tool
                    .get("parameters")
                    .or_else(|| tool.pointer("/function/parameters"))
                    .cloned()
                    .unwrap_or_else(|| json!({"type":"object","properties":{}}));
                let mut declaration = json!({"name":name,"parameters":parameters});
                if let Some(description) = tool.get("description") {
                    declaration["description"] = description.clone();
                }
                declarations.push(declaration);
            }
            "web_search" | "web_search_preview" => mapped.push(json!({"googleSearch": {}})),
            "code_interpreter" => mapped.push(json!({"codeExecution": {}})),
            "image_generation" => {
                return Err(LlmCoreError::new(
                    "Gemini conversion has no registered native image-generation tool projector.",
                ));
            }
            other => {
                return Err(LlmCoreError::new(format!(
                    "Responses tool type `{other}` is not supported by Gemini conversion."
                )));
            }
        }
    }
    if !declarations.is_empty() {
        mapped.insert(0, json!({"functionDeclarations": declarations}));
    }
    Ok(mapped)
}

/// Maps OpenAI tool choice into Gemini function-calling configuration.
/// 将 OpenAI 工具选择转换为 Gemini 函数调用配置。
fn map_gemini_tool_choice(value: &Value) -> Result<Value, LlmCoreError> {
    let config = match value {
        Value::String(choice) => match choice.as_str() {
            "auto" => json!({"mode":"AUTO"}),
            "required" => json!({"mode":"ANY"}),
            "none" => json!({"mode":"NONE"}),
            other => {
                return Err(LlmCoreError::new(format!(
                    "Gemini does not support tool choice `{other}`."
                )));
            }
        },
        Value::Object(object) => {
            let name = object
                .get("name")
                .or_else(|| {
                    object
                        .get("function")
                        .and_then(|function| function.get("name"))
                })
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    LlmCoreError::new("Responses tool_choice is missing function name.")
                })?;
            json!({"mode":"ANY","allowedFunctionNames":[name]})
        }
        _ => {
            return Err(LlmCoreError::new(
                "Responses tool_choice has an unsupported shape for Gemini conversion.",
            ));
        }
    };
    Ok(json!({"functionCallingConfig":config}))
}

/// Projects structured output, stop sequences, and reasoning into generationConfig.
/// 将结构化输出、停止序列与推理投影为 generationConfig。
fn project_gemini_controls(
    body: &Value,
    request: &mut Map<String, Value>,
) -> Result<(), LlmCoreError> {
    let mut generation = Map::new();
    if let Some(limit) = body.get("max_output_tokens") {
        let limit = limit.as_u64().filter(|value| *value > 0).ok_or_else(|| {
            LlmCoreError::new("Gemini `max_output_tokens` must be a positive integer.")
        })?;
        generation.insert(String::from("maxOutputTokens"), Value::Number(limit.into()));
    }
    if let Some(stop) = body.get("stop") {
        generation.insert(String::from("stopSequences"), stop.clone());
    }
    if let Some(format) = body.pointer("/text/format") {
        match format.get("type").and_then(Value::as_str) {
            Some("json_object") => {
                generation.insert(String::from("responseMimeType"), json!("application/json"));
            }
            Some("json_schema") => {
                let schema = format.get("schema").ok_or_else(|| {
                    LlmCoreError::new("Responses JSON schema format is missing `schema`.")
                })?;
                generation.insert(String::from("responseMimeType"), json!("application/json"));
                generation.insert(String::from("responseJsonSchema"), schema.clone());
            }
            Some(other) => {
                return Err(LlmCoreError::new(format!(
                    "Responses text format `{other}` is not supported by Gemini conversion."
                )));
            }
            None => {
                return Err(LlmCoreError::new(
                    "Responses text format is missing `type`.",
                ));
            }
        }
    }
    if let Some(reasoning) = body.get("reasoning") {
        let effort = reasoning
            .get("effort")
            .and_then(Value::as_str)
            .unwrap_or("none");
        if !matches!(effort, "none" | "off" | "disabled") {
            let level = canonical_reasoning_effort(effort).ok_or_else(|| {
                LlmCoreError::new(format!("Unsupported Gemini reasoning effort `{effort}`."))
            })?;
            // Native generateContent uses the REST enum spelling; Interactions keeps its separate
            // lowercase generation_config contract in project_interaction_generation_controls.
            // 原生 generateContent 使用 REST 枚举拼写；Interactions 仍在
            // project_interaction_generation_controls 中保持独立的小写 generation_config 契约。
            let level = level.to_ascii_uppercase();
            generation.insert(
                String::from("thinkingConfig"),
                json!({"thinkingLevel":level,"includeThoughts":true}),
            );
        } else if body
            .get("reasoning")
            .and_then(|value| value.get("summary"))
            .is_some()
        {
            generation.insert(
                String::from("thinkingConfig"),
                json!({"includeThoughts":false}),
            );
        }
    }
    if let Some(value) = body.get("temperature") {
        generation.insert(String::from("temperature"), value.clone());
    }
    if let Some(value) = body.get("top_p") {
        generation.insert(String::from("topP"), value.clone());
    }
    if !generation.is_empty() {
        request.insert(String::from("generationConfig"), Value::Object(generation));
    }
    Ok(())
}

/// Merges adjacent Gemini content entries with the same role.
/// 合并相邻同角色的 Gemini 内容项。
fn normalize_gemini_contents(contents: &mut Vec<Value>) -> Result<(), LlmCoreError> {
    let mut normalized: Vec<Value> = Vec::new();
    for content in contents.drain(..) {
        let role = content
            .get("role")
            .and_then(Value::as_str)
            .ok_or_else(|| LlmCoreError::new("Gemini content is missing `role`."))?;
        if let Some(previous) = normalized.last_mut()
            && previous.get("role").and_then(Value::as_str) == Some(role)
        {
            let previous_parts = previous
                .get_mut("parts")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| LlmCoreError::new("Gemini content parts are not an array."))?;
            let current_parts = content
                .get("parts")
                .and_then(Value::as_array)
                .ok_or_else(|| LlmCoreError::new("Gemini content parts are not an array."))?;
            previous_parts.extend(current_parts.iter().cloned());
        } else {
            normalized.push(content);
        }
    }
    *contents = normalized;
    Ok(())
}

/// Appends one Gemini part to the last content entry when its role matches.
/// 将一个 Gemini Part 追加到最后一个同角色内容项中。
fn push_gemini_part(contents: &mut Vec<Value>, role: &str, part: Value) {
    if let Some(previous) = contents.last_mut()
        && previous.get("role").and_then(Value::as_str) == Some(role)
        && let Some(parts) = previous.get_mut("parts").and_then(Value::as_array_mut)
    {
        parts.push(part);
        return;
    }
    contents.push(json!({"role":role,"parts":[part]}));
}
