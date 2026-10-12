//! Opaque credential before-images for explicit repair and transaction compensation.
//! 用于显式修复和事务补偿的不透明凭据前镜像。

use crate::{CredentialSecretEnvelope, CredentialSecretMaterial};
use std::any::Any;

/// Retains a host record without interpreting its credential format or exposing execution material.
/// 保留宿主记录，不解释其凭据格式，也不暴露执行材料。
pub struct CredentialSecretSnapshot {
    /// Exact storage identity, preventing restoration under a different reference.
    /// 精确存储身份，防止恢复到不同引用。
    reference: String,
    /// Storage revision used for compare-and-replace, independent of credential format.
    /// 比较替换使用的存储修订号，独立于凭据格式。
    revision: u64,
    /// Backend-owned before-image; deliberately neither serializable nor debuggable.
    /// 后端持有的前镜像；刻意不支持序列化或调试输出。
    record: Box<dyn Any + Send + Sync>,
}

impl CredentialSecretSnapshot {
    /// Captures `record` under its exact `reference` and positive storage `revision`.
    /// 按精确 `reference` 和正数存储修订号 `revision` 捕获 `record`。
    /// Returns a compensation-only snapshot, or an invalid storage identity error.
    /// 返回仅供补偿使用的快照，或无效存储身份错误。
    pub fn new<T: Any + Send + Sync>(
        reference: &str,
        revision: u64,
        record: T,
    ) -> Result<Self, String> {
        if reference.trim().is_empty() || revision == 0 {
            return Err(String::from("Invalid AI credential snapshot identity."));
        }
        Ok(Self {
            reference: reference.to_string(),
            revision,
            record: Box::new(record),
        })
    }

    /// Returns the stored revision used to guard an explicit credential replacement.
    /// 返回用于保护显式凭据替换的存储修订号。
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Retrieves the host's exact record type for the supplied `reference` during compensation.
    /// 在补偿时为传入 `reference` 取回宿主的精确记录类型。
    /// Returns a borrowed record, or an identity/backend mismatch without secret contents.
    /// 返回借用记录，或不包含秘密内容的身份／后端不匹配错误。
    pub fn record<T: Any>(&self, reference: &str) -> Result<&T, String> {
        if reference != self.reference {
            return Err(String::from(
                "AI credential snapshot reference does not match.",
            ));
        }
        self.record.downcast_ref::<T>().ok_or_else(|| {
            String::from("AI credential snapshot belongs to a different storage backend.")
        })
    }

    /// Builds a current-format envelope from newly supplied `material`, advancing only the revision.
    /// 根据新提供的 `material` 构建当前格式封装，仅沿用递增的修订号。
    /// Returns a validated replacement without reading or migrating the previous credential material.
    /// 返回已验证替换凭据，不读取或迁移先前的凭据材料。
    pub fn replacement(
        &self,
        material: CredentialSecretMaterial,
    ) -> Result<CredentialSecretEnvelope, String> {
        // Replacement authority comes entirely from fresh user input or a completed authorization.
        // 替换权威完全来自新的用户输入或已完成的授权。
        let mut replacement = CredentialSecretEnvelope::new(material)?;
        replacement.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| String::from("AI credential secret revision is exhausted."))?;
        Ok(replacement)
    }
}
