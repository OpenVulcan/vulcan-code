// Field editors built from confirmed WebUI requests.
// 基于已确认 WebUI 请求构建的字段编辑器。

use serde_json::Value;

use crate::model::SettingValue;

use super::ServiceCall;

/// Describes an edit of an existing DTO or a request requiring a dedicated builder.
/// 描述现有 DTO 的编辑，或需要专用构建函数的请求。
#[derive(Clone)]
pub enum Setter {
    /// Replaces `pointer` in `base`; `save` receives the complete updated DTO.
    /// 替换 `base` 中的 `pointer`；`save` 接收完整的更新后 DTO。
    Dto {
        /// The authoritative DTO before this edit.
        /// 本次编辑前的权威 DTO。
        base: Value,
        /// Exact JSON pointer established by the WebUI type definition.
        /// 由 WebUI 类型定义确认的精确 JSON 指针。
        pointer: String,
        /// Converts the updated DTO to its matching service request.
        /// 将更新后的 DTO 转为对应的服务请求。
        save: fn(Value) -> ServiceCall,
    },
    /// Builds a request from confirmed context and the new scalar value.
    /// 根据已确认的上下文与新的标量值构建请求。
    Build {
        /// Identifiers and request context; never speculative alternate paths.
        /// 标识与请求上下文；不包含推测的候选路径。
        ctx: Value,
        /// Returns the request, or `None` when input cannot satisfy its contract.
        /// 返回请求；输入不满足协议时返回 `None`。
        build: fn(&Value, &SettingValue) -> Option<ServiceCall>,
    },
    /// Validates fractional numeric input before replacing one exact DTO field.
    /// 替换准确 DTO 字段前校验小数输入。
    Decimal {
        /// Complete authoritative document before the edit.
        /// 编辑前的完整权威文档。
        base: Value,
        /// Exact decimal field in that document.
        /// 文档中的准确小数字段。
        pointer: String,
        /// Inclusive lower limit declared by the upstream policy.
        /// 上游策略声明的包含下限。
        min: f64,
        /// Inclusive upper limit declared by the upstream policy.
        /// 上游策略声明的包含上限。
        max: f64,
        /// Builds the whole-document save request after validation.
        /// 校验后构建完整文档保存请求。
        save: fn(Value) -> ServiceCall,
    },
    /// Splits a user-entered multiline list into the schema's string array.
    /// 将用户输入的多行列表拆为结构要求的字符串数组。
    StringList {
        /// Complete authoritative document before the edit.
        /// 编辑前的完整权威文档。
        base: Value,
        /// Exact string-array field in that document.
        /// 文档中的准确字符串数组字段。
        pointer: String,
        /// Builds the whole-document save request with the parsed list.
        /// 使用已解析列表构建完整文档保存请求。
        save: fn(Value) -> ServiceCall,
    },
}

impl std::fmt::Debug for Setter {
    /// Formats only the editor kind so transient credential input cannot leak.
    /// 仅格式化编辑器种类，避免临时凭据输入泄漏。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Dto { .. } => "Setter::Dto",
            Self::Build { .. } => "Setter::Build",
            Self::Decimal { .. } => "Setter::Decimal",
            Self::StringList { .. } => "Setter::StringList",
        })
    }
}

impl Setter {
    /// Applies `value` to the verified field and returns its service call; missing fields fail.
    /// 将 `value` 应用到已确认字段并返回服务调用；字段缺失时明确失败。
    pub fn apply(&self, value: &SettingValue) -> Option<ServiceCall> {
        match self {
            Self::Dto {
                base,
                pointer,
                save,
            } => {
                // Keep sibling fields because WebUI save APIs accept complete documents.
                // 保留同级字段，因为 WebUI 保存接口接收完整文档。
                let mut document = base.clone();
                *document.pointer_mut(pointer)? = match value {
                    SettingValue::Bool(value) => Value::Bool(*value),
                    SettingValue::Number(value) => Value::from(*value),
                    SettingValue::Text(value) | SettingValue::Choice(value) => {
                        Value::from(value.clone())
                    }
                };
                Some(save(document))
            }
            Self::Build { ctx, build } => build(ctx, value),
            Self::Decimal {
                base,
                pointer,
                min,
                max,
                save,
            } => {
                // Number.
                // 通过解析的数值。
                let number: f64 = value.as_str().parse().ok()?;
                if !number.is_finite() || number < *min || number > *max {
                    return None;
                }
                // Complete document preserving sibling fields.
                // 保留同级字段的完整文档。
                let mut document = base.clone();
                *document.pointer_mut(pointer)? = Value::from(number);
                Some(save(document))
            }
            Self::StringList {
                base,
                pointer,
                save,
            } => {
                // Complete document preserving sibling fields.
                // 保留同级字段的完整文档。
                let mut document = base.clone();
                // Lines.
                // 当前文本行。
                let lines: Vec<&str> = value
                    .as_str()
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .collect();
                *document.pointer_mut(pointer)? = serde_json::json!(lines);
                Some(save(document))
            }
        }
    }
}
