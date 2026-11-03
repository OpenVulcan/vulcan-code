//! Failures of reading, verifying and producing release formats.
//! 读取、校验与生成发布格式时的失败。

use crate::digest::Sha256Digest;

/// Result of a release format operation.
/// 发布格式操作的结果。
pub type FormatResult<T> = Result<T, FormatError>;

/// A release format failure; every variant means the content is rejected, never partly trusted.
/// 发布格式失败；任何变体都表示内容被拒绝，不会被部分信任。
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    /// A document or stream is larger than its bound.
    /// 文档或数据流超过其上限。
    #[error("{what} exceeds the {limit} byte bound")]
    TooLarge {
        /// What was being read.
        /// 正在读取的内容。
        what: &'static str,
        /// The bound in bytes.
        /// 以字节计的上限。
        limit: u64,
    },
    /// A detached signature does not have the raw Ed25519 length.
    /// 分离签名不是原始 Ed25519 长度。
    #[error("signature must be exactly {expected} bytes, got {actual}")]
    SignatureLength {
        /// Required length.
        /// 要求的长度。
        expected: usize,
        /// Received length.
        /// 收到的长度。
        actual: usize,
    },
    /// No built-in key verifies the signature.
    /// 没有内置公钥能验证该签名。
    #[error("signature does not verify with any trusted key")]
    SignatureRejected,
    /// More than one trusted key verifies the signature, so the key set is unusable.
    /// 多把受信公钥都能验证该签名，公钥集不可用。
    #[error("signature matched more than one trusted key")]
    AmbiguousSigningKey,
    /// The trusted key set itself is invalid.
    /// 受信公钥集本身无效。
    #[error("trusted key `{key_id}` is invalid: {reason}")]
    InvalidTrustedKey {
        /// Identifier of the key.
        /// 公钥标识。
        key_id: String,
        /// Why it is invalid.
        /// 无效原因。
        reason: String,
    },
    /// A signed document names a different key than the one whose signature matched.
    /// 已签名文档声明的公钥与实际匹配签名的公钥不同。
    #[error("document declares key `{declared}` but was signed by `{matched}`")]
    KeyIdMismatch {
        /// Key id the document declares.
        /// 文档声明的公钥标识。
        declared: String,
        /// Key id whose signature matched.
        /// 签名实际匹配的公钥标识。
        matched: String,
    },
    /// Content does not have the digest its parent document promised.
    /// 内容的摘要与上级文档承诺的不一致。
    #[error("{what} digest mismatch: expected {expected}, got {actual}")]
    DigestMismatch {
        /// What was checked.
        /// 被校验的内容。
        what: &'static str,
        /// Promised digest.
        /// 承诺的摘要。
        expected: Sha256Digest,
        /// Actual digest.
        /// 实际摘要。
        actual: Sha256Digest,
    },
    /// Content does not have the size its parent document promised.
    /// 内容的大小与上级文档承诺的不一致。
    #[error("{what} size mismatch: expected {expected}, got {actual}")]
    SizeMismatch {
        /// What was checked.
        /// 被校验的内容。
        what: &'static str,
        /// Promised size.
        /// 承诺的大小。
        expected: u64,
        /// Actual size.
        /// 实际大小。
        actual: u64,
    },
    /// A document is not valid JSON of its schema.
    /// 文档不是符合其结构的有效 JSON。
    #[error("{what} is not valid: {source}")]
    Json {
        /// Which document.
        /// 哪个文档。
        what: &'static str,
        /// Parser failure.
        /// 解析失败原因。
        #[source]
        source: serde_json::Error,
    },
    /// A field or document breaks a format rule.
    /// 字段或文档违反格式规则。
    #[error("{what} is invalid: {reason}")]
    Invalid {
        /// Which field or document.
        /// 哪个字段或文档。
        what: &'static str,
        /// The broken rule.
        /// 违反的规则。
        reason: String,
    },
    /// Another process is installing or updating the same root.
    /// 另一个进程正在安装或更新同一根目录。
    #[error("another install or update is running in this install root")]
    InstallBusy,
    /// Not enough free disk space to assemble the version.
    /// 磁盘剩余空间不足以组装该版本。
    #[error("assembling needs about {needed} free bytes but only {available} are available")]
    InsufficientSpace {
        /// Bytes needed, with headroom.
        /// 所需字节数（含余量）。
        needed: u64,
        /// Bytes available.
        /// 可用字节数。
        available: u64,
    },
    /// Reading, writing or (de)compressing failed.
    /// 读取、写入或压缩解压失败。
    #[error("{what}: {source}")]
    Io {
        /// What was being done.
        /// 正在进行的操作。
        what: &'static str,
        /// Underlying failure.
        /// 底层失败。
        #[source]
        source: std::io::Error,
    },
}

impl FormatError {
    /// Creates an [`FormatError::Invalid`] failure.
    /// 创建 [`FormatError::Invalid`] 失败。
    pub(crate) fn invalid(what: &'static str, reason: impl Into<String>) -> Self {
        Self::Invalid {
            what,
            reason: reason.into(),
        }
    }

    /// Creates an [`FormatError::Io`] failure.
    /// 创建 [`FormatError::Io`] 失败。
    pub(crate) fn io(what: &'static str, source: std::io::Error) -> Self {
        Self::Io { what, source }
    }
}
