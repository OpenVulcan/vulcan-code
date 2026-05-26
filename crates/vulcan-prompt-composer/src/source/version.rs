use crate::source::hashing::stable_text_hash;

/// Stable version marker for one external prompt source.
/// 单个外部提示词来源的稳定版本标记。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSourceVersion {
    /// Source identifier such as a path or catalog id.
    /// 来源标识，例如路径或目录标识。
    pub source_id: String,
    /// Stable content fingerprint for the source.
    /// 来源的稳定内容指纹。
    pub fingerprint: u64,
}

impl PromptSourceVersion {
    /// Build one source version from arbitrary text content.
    /// 从任意文本内容构建一个来源版本。
    pub fn from_text(source_id: impl Into<String>, text: &str) -> Self {
        Self {
            source_id: source_id.into(),
            fingerprint: stable_text_hash(text),
        }
    }
}
