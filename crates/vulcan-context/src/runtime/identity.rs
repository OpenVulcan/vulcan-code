//! Shared runtime identity normalization helpers.
//! 共享运行时身份规范化辅助函数。

pub(super) use crate::protocol::identity::{
    CanonicalMessageIdentifierError, register_canonical_message_identifier,
};

/// Normalizes one optional external identifier without inventing an alternative source.
/// 规范化一个可选外部标识，且不臆造替代来源。
///
/// # Parameters
/// 参数
/// - `identifier`: Optional external identifier whose surrounding whitespace is insignificant.
/// - `identifier`：首尾空白无语义的可选外部标识。
///
/// # Returns
/// 返回值
/// A trimmed non-empty identifier, or `None` when the input is absent or blank.
/// 返回去除首尾空白后的非空标识；输入缺失或空白时返回 `None`。
pub(super) fn normalized_optional_identifier(identifier: Option<&str>) -> Option<String> {
    identifier
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
