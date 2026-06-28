use super::*;

pub(super) fn dynamic_model_value<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<&'a serde_json::Value> {
    keys.iter().find_map(|key| object.get(*key))
}

#[cfg(test)]
/// Parses one optional dynamic model string field from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态模型字符串字段。
pub(super) fn dynamic_model_string(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    dynamic_model_value(object, keys)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
/// Parses one optional dynamic model boolean field from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态模型布尔字段。
pub(super) fn dynamic_model_bool(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<bool> {
    dynamic_model_value(object, keys).and_then(|value| match value {
        serde_json::Value::Bool(flag) => Some(*flag),
        serde_json::Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

#[cfg(test)]
/// Parses one optional dynamic model numeric field from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态模型数值字段。
pub(super) fn dynamic_model_usize(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<usize> {
    dynamic_model_value(object, keys).and_then(|value| match value {
        serde_json::Value::Number(number) => number.as_u64().map(|value| value as usize),
        serde_json::Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    })
}

#[cfg(test)]
/// Parses one optional dynamic interleaved reasoning capability from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态交错推理能力描述。
pub(super) fn dynamic_model_interleaved(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<ModelInterleavedCapability> {
    dynamic_model_value(object, keys)
        .and_then(|value| serde_json::from_value::<ModelInterleavedCapability>(value.clone()).ok())
}

#[cfg(test)]
/// Parses one optional dynamic reasoning profile from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个可选动态推理档位配置。
pub(super) fn dynamic_model_reasoning_profile(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<ReasoningProfileConfig> {
    dynamic_model_value(object, keys)
        .and_then(|value| serde_json::from_value::<ReasoningProfileConfig>(value.clone()).ok())
}

#[cfg(test)]
/// Parses dynamic AI-tool declarations from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析动态 AI 工具声明集合。
pub(super) fn dynamic_model_ai_tools(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Vec<AiToolDeclaration> {
    dynamic_model_value(object, keys)
        .and_then(|value| serde_json::from_value::<Vec<AiToolDeclaration>>(value.clone()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
/// Parses one ordered dynamic modality list while tolerating both flat and nested payload shapes.
/// 在兼容扁平与嵌套载荷形状的前提下解析一组有序动态模态列表。
pub(super) fn dynamic_model_modalities(
    object: &serde_json::Map<String, serde_json::Value>,
) -> (Vec<String>, Vec<String>) {
    let flat_input = dynamic_model_string_array(object, &["inputModalities", "input_modalities"]);
    let flat_output =
        dynamic_model_string_array(object, &["outputModalities", "output_modalities"]);

    let nested = object
        .get("modalities")
        .and_then(serde_json::Value::as_object);
    let nested_input = nested
        .map(|modalities| dynamic_model_string_array(modalities, &["input"]))
        .unwrap_or_default();
    let nested_output = nested
        .map(|modalities| dynamic_model_string_array(modalities, &["output"]))
        .unwrap_or_default();

    let architecture = object
        .get("architecture")
        .and_then(serde_json::Value::as_object);
    let architecture_input = architecture
        .map(|modalities| {
            dynamic_model_string_array(modalities, &["input_modalities", "inputModalities"])
        })
        .unwrap_or_default();
    let architecture_output = architecture
        .map(|modalities| {
            dynamic_model_string_array(modalities, &["output_modalities", "outputModalities"])
        })
        .unwrap_or_default();
    let (architecture_modality_input, architecture_modality_output) = architecture
        .and_then(|modalities| dynamic_model_string(modalities, &["modality"]))
        .map(|modality| dynamic_model_modalities_from_descriptor(&modality))
        .unwrap_or_default();

    let input_modalities = if flat_input.is_empty() {
        if nested_input.is_empty() {
            if architecture_input.is_empty() {
                architecture_modality_input
            } else {
                architecture_input
            }
        } else {
            nested_input
        }
    } else {
        flat_input
    };
    let output_modalities = if flat_output.is_empty() {
        if nested_output.is_empty() {
            if architecture_output.is_empty() {
                architecture_modality_output
            } else {
                architecture_output
            }
        } else {
            nested_output
        }
    } else {
        flat_output
    };

    (input_modalities, output_modalities)
}

#[cfg(test)]
/// Parses one compact architecture modality descriptor like `text+image->text` into ordered input/output lists.
/// 将 `text+image->text` 这样的紧凑架构模态描述解析为有序输入输出列表。
pub(super) fn dynamic_model_modalities_from_descriptor(
    modality: &str,
) -> (Vec<String>, Vec<String>) {
    let Some((raw_input, raw_output)) = modality.split_once("->") else {
        return (Vec::new(), Vec::new());
    };

    (
        dynamic_model_modalities_from_half(raw_input),
        dynamic_model_modalities_from_half(raw_output),
    )
}

#[cfg(test)]
/// Parses one modality descriptor half like `text+image` into an ordered normalized list.
/// 将 `text+image` 这样的半段模态描述解析为有序归一化列表。
pub(super) fn dynamic_model_modalities_from_half(raw_modalities: &str) -> Vec<String> {
    raw_modalities
        .split('+')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
}

#[cfg(test)]
/// Parses one ordered dynamic string-array field from the loosely shaped remote `/models` payload.
/// 从宽松形状的远端 `/models` 载荷中解析一个有序动态字符串数组字段。
pub(super) fn dynamic_model_string_array(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Vec<String> {
    dynamic_model_value(object, keys)
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}
