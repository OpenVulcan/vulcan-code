//! Exact declared upload input traversal shared by host resolution and physical materialization.
//! 宿主解析与物理物化共用的精确声明上传输入遍历。

use serde_json::Value;

/// One declared resource occurrence, borrowed from the canonical arguments without reading its bytes.
/// 从规范参数借用的一次声明资源出现，不读取其字节。
pub(crate) struct OperationUploadInput<'a> {
    /// Position in the frozen operation's upload binding list.
    /// 冻结操作上传绑定列表中的位置。
    pub(crate) binding_index: usize,
    /// Exact resource string; no guessed alternative field or path is inspected.
    /// 精确资源字符串；不检查猜测的替代字段或路径。
    pub(crate) reference: &'a str,
    /// Position inside a declared object array; scalar and string-array sources have no item destination.
    /// 声明对象数组中的位置；标量及字符串数组来源没有逐项目标。
    pub(crate) item_index: Option<usize>,
    /// Original scalar-versus-array shape retained for output projection.
    /// 为输出投影保留的原标量或数组形态。
    pub(crate) source_was_array: bool,
}

/// Collects all `operation` bindings from canonical `arguments` in binding/item order.
/// 按绑定/条目顺序从规范`arguments`收集`operation`全部绑定。
/// Returns exact borrowed occurrences or the first declared-shape error, before any host lookup or I/O.
/// 在任何宿主查询或输入输出之前返回精确借用出现，或首个声明形态错误。
pub(crate) fn operation_upload_inputs<'a>(
    operation: &crate::OperationDeclaration,
    arguments: &'a Value,
) -> Result<Vec<OperationUploadInput<'a>>, String> {
    // Validate the whole shape before returning any source eligible for side effects.
    // 返回任何可产生副作用的来源之前验证整个形态。
    let mut inputs = Vec::new();
    for (binding_index, binding) in operation.upload_bindings.iter().enumerate() {
        // Only the Manifest's absolute pointer owns this source.
        // 只有Manifest中的绝对指针持有本来源。
        let Some(source) = arguments.pointer(&binding.source_path) else {
            if binding.required {
                return Err(format!(
                    "Operation `{}` requires upload source `{}`.",
                    operation.operation_id, binding.source_path
                ));
            }
            continue;
        };
        if let Some(item_source_path) = binding.item_source_path.as_deref() {
            // The declared item discriminator decides whether an object belongs to this binding.
            // 声明的条目判别值决定对象是否属于本绑定。
            let items = source.as_array().ok_or_else(|| {
                format!(
                    "Operation `{}` item upload source `{}` must be an array.",
                    operation.operation_id, binding.source_path
                )
            })?;
            if items.is_empty() && binding.required && binding.item_match_path.is_none() {
                return Err(format!(
                    "Operation `{}` requires a non-empty item upload source `{}`.",
                    operation.operation_id, binding.source_path
                ));
            }
            for (item_index, item) in items.iter().enumerate() {
                if let (Some(match_path), Some(match_value)) = (
                    binding.item_match_path.as_deref(),
                    binding.item_match_value.as_ref(),
                ) && item.pointer(match_path) != Some(match_value)
                {
                    continue;
                }
                // Optional absence is allowed only where the binding explicitly permits it.
                // 只有绑定明确允许时，才允许可选字段缺失。
                let Some(value) = item.pointer(item_source_path) else {
                    if binding.required {
                        return Err(format!(
                            "Operation `{}` item upload source `{}{}/{}` is required.",
                            operation.operation_id,
                            binding.source_path,
                            item_index,
                            item_source_path.trim_start_matches('/')
                        ));
                    }
                    continue;
                };
                // Resource strings are preserved verbatim; URI validation belongs to their exact resolver.
                // 资源字符串保持原样；URI验证属于其精确解析器。
                let reference = value.as_str().filter(|value| !value.trim().is_empty()).ok_or_else(|| format!("Operation `{}` item upload source `{}{}/{}` must be a non-empty string.", operation.operation_id, binding.source_path, item_index, item_source_path.trim_start_matches('/')))?;
                inputs.push(OperationUploadInput {
                    binding_index,
                    reference,
                    item_index: Some(item_index),
                    source_was_array: true,
                });
            }
            continue;
        }
        // Scalar strings and string arrays are the two non-item shapes declared by the Schema.
        // 标量字符串和字符串数组是Schema声明的两种非逐项形态。
        let (values, source_was_array) = match source {
            Value::String(_) => (std::slice::from_ref(source), false),
            Value::Array(values) if !values.is_empty() => (values.as_slice(), true),
            Value::Array(_) if !binding.required => continue,
            _ => {
                return Err(format!(
                    "Operation `{}` upload source `{}` must be a non-empty string or string array.",
                    operation.operation_id, binding.source_path
                ));
            }
        };
        for value in values {
            // A malformed later value rejects the entire traversal before an earlier value is consumed.
            // 格式错误的后项在前项被消费之前拒绝整个遍历。
            let reference = value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "Operation `{}` upload source `{}` contains a non-string or empty item.",
                        operation.operation_id, binding.source_path
                    )
                })?;
            inputs.push(OperationUploadInput {
                binding_index,
                reference,
                item_index: None,
                source_was_array,
            });
        }
    }
    Ok(inputs)
}
