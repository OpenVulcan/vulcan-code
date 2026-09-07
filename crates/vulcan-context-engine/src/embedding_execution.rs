//! Host-approved execution identity without rewriting the original indexed vector space.
//! 宿主批准的执行身份，不改写原始已索引向量空间。

use crate::{EmbeddingSpace, VceConfigError, VceIndexBinding};

/// One exact host authorization joining an immutable index to its current embedding producer.
/// 将不可变索引与当前向量生产来源关联的一份精确宿主授权。
/// The host verifies human decisions and all live origins; core enforces scope and physical guards.
/// 宿主校验用户决定及全部现存来源；核心强制校验作用域与物理限制。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceEmbeddingExecutionContract {
    /// Original binding; private fields prevent a validated contract from being partially rewritten.
    /// 原始绑定；私有字段防止已验证契约被部分改写。
    original: VceIndexBinding,
    /// Exact space that the admitted provider must actually return.
    /// 已准入供应商必须实际返回的精确空间。
    execution: EmbeddingSpace,
    /// Opaque non-secret host evidence identity, never interpreted as provider configuration.
    /// 不透明的非敏感宿主证据身份，绝不作为供应商配置解析。
    authorization_id: String,
}

impl VceEmbeddingExecutionContract {
    /// Validate a host grant for an original binding and one explicit execution space.
    /// 为原始绑定及一个明确执行空间校验宿主授权。
    /// `authorization_id` identifies checked host evidence; returns an immutable grant or a hard error.
    /// `authorization_id` 标识已检查宿主证据；返回不可变授权或硬性错误。
    /// Same dimensions alone never establish compatibility; callers must first obtain host approval.
    /// 仅维度相同绝不构成兼容；调用方必须先取得宿主批准。
    pub fn new(
        original: VceIndexBinding,
        execution: EmbeddingSpace,
        authorization_id: impl Into<String>,
    ) -> Result<Self, VceConfigError> {
        // Evidence is retained separately from both the original and execution fingerprints.
        // 证据与原始及执行指纹分别保留。
        let authorization_id = authorization_id.into();
        if authorization_id.trim().is_empty()
            || authorization_id.len() > 4096
            || original.namespace_id.trim().is_empty()
            || original.checkpoint_chain_id.trim().is_empty()
            || original.schema_version == 0
            || !original.embedding_space.is_well_formed()
            || !execution.is_well_formed()
            || original.embedding_space.physical_fingerprint.is_none()
            || execution.physical_fingerprint.is_none()
            || original.embedding_space.vector_space_id != execution.vector_space_id
            || original.embedding_space.dimension != execution.dimension
            || original.embedding_space.preprocessing_version != execution.preprocessing_version
            || original.embedding_space.chunker_version != execution.chunker_version
        {
            return Err(VceConfigError::InvalidEmbeddingExecution);
        }
        Ok(Self {
            original,
            execution,
            authorization_id,
        })
    }

    /// Return the immutable storage binding; takes no parameters and never substitutes the producer.
    /// 返回不可变存储绑定；无参数，绝不将其替换成生产来源。
    pub fn original(&self) -> &VceIndexBinding {
        &self.original
    }

    /// Return the exact expected provider response space; takes no parameters.
    /// 返回精确预期供应商响应空间；无参数。
    pub fn execution(&self) -> &EmbeddingSpace {
        &self.execution
    }

    /// Return the host evidence identifier for diagnostics; takes no parameters and exposes no secret.
    /// 返回用于诊断的宿主证据标识；无参数且不暴露密钥。
    pub fn authorization_id(&self) -> &str {
        &self.authorization_id
    }
}
