//! Stable opaque identifier contracts.
//! 稳定的不透明标识符契约。

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::{Display, Formatter};

/// Generates a validated UTF-8 opaque identifier type.
/// 生成经过校验的 UTF-8 不透明标识符类型。
macro_rules! opaque_id {
    ($name:ident, $english:literal, $chinese:literal) => {
        #[doc = $english]
        #[doc = $chinese]
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates an identifier after rejecting empty or oversized values.
            /// 在拒绝空值或超长值后创建标识符。
            ///
            /// # Errors
            /// Returns an error when the identifier is empty, longer than 512 bytes, or contains NUL.
            /// 当标识符为空、超过 512 字节或包含 NUL 时返回错误。
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                // Materialize the owned value once so validation and storage observe identical bytes.
                // 仅实例化一次所有权值，确保校验与存储观察到完全相同的字节。
                let value = value.into();
                if value.is_empty() {
                    return Err(IdentifierError::Empty);
                }
                if value.len() > 512 {
                    return Err(IdentifierError::TooLong);
                }
                if value.contains('\0') {
                    return Err(IdentifierError::ContainsNul);
                }
                Ok(Self(value))
            }

            /// Borrows the exact stable identifier text.
            /// 借用精确的稳定标识符文本。
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consumes the identifier and returns its owned text.
            /// 消耗标识符并返回其所有权文本。
            #[must_use]
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl Display for $name {
            /// Formats the identifier without changing its opaque value.
            /// 在不改变不透明值的前提下格式化标识符。
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            /// Deserializes one identifier through the same closed validation as `new`.
            /// 通过与 `new` 相同的封闭校验反序列化一个标识符。
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

/// Describes a stable identifier validation failure.
/// 描述稳定标识符校验失败。
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IdentifierError {
    /// Indicates an empty identifier.
    /// 表示标识符为空。
    #[error("identifier must not be empty")]
    Empty,
    /// Indicates an identifier exceeding the ABI safety limit.
    /// 表示标识符超过 ABI 安全长度限制。
    #[error("identifier exceeds 512 UTF-8 bytes")]
    TooLong,
    /// Indicates an identifier containing a NUL byte.
    /// 表示标识符包含 NUL 字节。
    #[error("identifier contains NUL")]
    ContainsNul,
}

opaque_id!(
    SubjectId,
    "Identifies one host-resolved subject.",
    "标识一个由宿主解析的主体。"
);
opaque_id!(
    ScopeId,
    "Identifies one flat visibility scope.",
    "标识一个扁平的可见作用域。"
);
opaque_id!(
    SessionSourceId,
    "Identifies one host session source.",
    "标识一个宿主会话来源。"
);
opaque_id!(
    TurnId,
    "Identifies one canonical turn.",
    "标识一个规范化对话轮次。"
);
opaque_id!(
    MemoryId,
    "Identifies one memory record.",
    "标识一条记忆记录。"
);
opaque_id!(
    ProfileNodeId,
    "Identifies one profile node.",
    "标识一个画像节点。"
);
opaque_id!(
    ProfileSourceId,
    "Identifies the durable source event or record behind one profile node.",
    "标识一个画像节点背后的持久来源事件或记录。"
);
opaque_id!(
    CommandId,
    "Identifies one resumable command.",
    "标识一个可恢复命令。"
);
opaque_id!(
    EffectId,
    "Identifies one requested host effect.",
    "标识一个请求宿主执行的 Effect。"
);
opaque_id!(
    ConfigSnapshotId,
    "Identifies one immutable configuration snapshot.",
    "标识一个不可变配置快照。"
);
opaque_id!(
    OperationId,
    "Identifies one storage mutation operation.",
    "标识一次存储变更操作。"
);
opaque_id!(
    IdempotencyKey,
    "Identifies one idempotent side effect.",
    "标识一个幂等副作用。"
);
opaque_id!(
    VectorSpaceId,
    "Identifies one embedding vector space.",
    "标识一个嵌入向量空间。"
);

#[cfg(test)]
mod tests {
    use super::SubjectId;
    use serde::Deserialize;
    use serde::de::value::{Error as ValueError, StringDeserializer};

    /// Verifies deserialization cannot bypass opaque identifier invariants.
    /// 验证反序列化无法绕过不透明标识符不变量。
    #[test]
    fn opaque_identifier_deserialization_reuses_validation() {
        let empty = SubjectId::deserialize(StringDeserializer::<ValueError>::new(String::new()));
        assert!(empty.is_err());
        let oversized =
            SubjectId::deserialize(StringDeserializer::<ValueError>::new("x".repeat(513)));
        assert!(oversized.is_err());
        let nul = SubjectId::deserialize(StringDeserializer::<ValueError>::new(
            "subject\0invalid".to_owned(),
        ));
        assert!(nul.is_err());
    }
}
