//! Persisted user decisions for exact ContextEngine embedding spaces, isolated from peer consumers.
//! 精确 ContextEngine 向量空间的持久化用户决定，与其他消费者隔离。

use super::InferenceRouteConfiguration;
use serde::{Deserialize, Serialize};

/// Explicit human decision for one source-to-target embedding-space relationship.
/// 针对一项来源到目标向量空间关系的明确用户决定。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextEngineEmbeddingDecision {
    /// The user confirms these exact contracts represent the same semantic vector space.
    /// 用户确认这些精确契约表示同一语义向量空间。
    Compatible,
    /// The user requires a new index before the candidate may serve this project.
    /// 用户要求先建立新索引，候选才能服务此项目。
    Rebuild,
}

/// Immutable scope of one existing index, excluding mutable project settings and display labels.
/// 一个现有索引的不可变作用域，不包含可变项目设置及展示标签。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextEngineEmbeddingScope {
    /// Stable host project identifier.
    /// 稳定宿主项目标识。
    pub project_id: String,
    /// Exact canonical root hash; moving the project does not inherit approval.
    /// 精确规范根哈希；移动项目不会继承批准。
    pub root_hash: String,
    /// Exact physical index namespace.
    /// 精确物理索引命名空间。
    pub namespace_id: String,
    /// Exact checkpoint chain; approvals never apply to a future replacement chain.
    /// 精确检查点链；批准绝不适用于未来替换链。
    pub checkpoint_chain_id: String,
    /// Original index fingerprint, retained even after approved provider changes.
    /// 原始索引指纹，即使批准更换供应商也保留。
    pub original_fingerprint: String,
    /// Exact index format represented by this scope.
    /// 此作用域表示的精确索引格式。
    pub index_schema_version: u32,
    /// Exact positive output dimension; approval cannot bypass a dimension change.
    /// 精确正输出维度；批准不能绕过维度变化。
    pub dimension: u32,
    /// Exact preprocessing semantics of the stored values.
    /// 已存值的精确预处理语义。
    pub preprocessing_version: String,
    /// Exact semantic chunker version used by the stored documents.
    /// 已存文档使用的精确语义切块器版本。
    pub chunker_version: String,
}

impl ContextEngineEmbeddingScope {
    /// Validate a bounded exact identity without resolving any model or filesystem path.
    /// 校验有界精确身份，不解析任何模型或文件系统路径。
    ///
    /// No parameters. Returns an explicit policy error for empty, malformed, or oversized fields.
    /// 无参数。字段为空、错误或过大时返回明确策略错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.dimension == 0 || self.index_schema_version == 0 || !is_sha256_hex(&self.root_hash)
        {
            return Err(String::from(
                "ContextEngine embedding scope has an invalid root, dimension, or format",
            ));
        }
        for (name, value, maximum) in [
            ("project", self.project_id.as_str(), 512),
            ("namespace", self.namespace_id.as_str(), 2048),
            ("checkpoint chain", self.checkpoint_chain_id.as_str(), 2048),
            (
                "original fingerprint",
                self.original_fingerprint.as_str(),
                4096,
            ),
            (
                "preprocessing version",
                self.preprocessing_version.as_str(),
                512,
            ),
            ("chunker version", self.chunker_version.as_str(), 512),
        ] {
            if value.trim().is_empty() || value.len() > maximum {
                return Err(format!(
                    "ContextEngine embedding scope has an invalid {name}"
                ));
            }
        }
        Ok(())
    }
}

/// One direct, non-transitive, versioned compatibility decision committed with a VCE model route.
/// 与 VCE 模型路由一同提交的一项直接、非传递、版本化兼容决定。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextEngineEmbeddingCompatibilityRecord {
    /// Exact record format; unknown versions must never authorize a query or writer.
    /// 精确记录格式；未知版本绝不能授权查询或写入。
    pub schema_version: u32,
    /// Existing index scope chosen by the user; no wildcard projects or roots.
    /// 用户选择的现有索引作用域；不允许项目或根目录通配。
    pub scope: ContextEngineEmbeddingScope,
    /// One actual source represented in that index, not merely the last selected model.
    /// 该索引包含的一个实际来源，而不只是上次选择的模型。
    pub source_fingerprint: String,
    /// Exact candidate contract digest, including its explicit output dimension.
    /// 包含明确输出维度的精确候选契约摘要。
    pub target_fingerprint: String,
    /// Exact non-secret candidate route, retained for inspection and audit.
    /// 保留用于检查与审计的精确非敏感候选路由。
    pub target_route: InferenceRouteConfiguration,
    /// Model capability evidence frozen during confirmation.
    /// 确认期间冻结的模型能力证据。
    pub target_capability_revision: String,
    /// Exact execution-wire revision frozen during confirmation.
    /// 确认期间冻结的精确执行 Wire 修订。
    pub target_route_revision: String,
    /// Explicit user decision, never inferred from matching names.
    /// 明确用户决定，绝不由名称相同推断。
    pub decision: ContextEngineEmbeddingDecision,
    /// Confirmation timestamp, not an inferred model test or source modification time.
    /// 确认时间戳，不是推测的模型测试或源码修改时间。
    pub confirmed_at_unix_ms: u64,
    /// Non-secret preflight digest identifying the exact saved policy transaction.
    /// 标识精确已保存策略事务的非敏感预检摘要。
    pub configuration_revision: String,
}

impl ContextEngineEmbeddingCompatibilityRecord {
    /// Validate direct-decision structure without approving unknown execution capabilities.
    /// 校验直接决定结构，不批准未知执行能力。
    ///
    /// No parameters. Returns success for a well-formed record; live host admission still rechecks
    /// scope, actual source membership, exact target digest, and the selected model's capabilities.
    /// 无参数。记录格式正确时返回成功；实时宿主准入仍重新核对作用域、实际来源成员、
    /// 精确目标摘要和所选模型能力。
    pub fn validate(&self) -> Result<(), String> {
        self.scope.validate()?;
        if self.schema_version != 1
            || self.confirmed_at_unix_ms == 0
            || self.target_route.operation != vulcan_ai_contract::AiOperation::EmbeddingCreate
            || self.target_route.usage_scope
                != vulcan_ai_contract::InferenceUsageScope::InternalBackground
            || !is_tagged_sha256(&self.target_fingerprint)
            || !is_tagged_sha256(&self.configuration_revision)
        {
            return Err(String::from(
                "ContextEngine embedding decision has an invalid version, target, or confirmation",
            ));
        }
        for (name, value) in [
            ("source fingerprint", self.source_fingerprint.as_str()),
            (
                "capability revision",
                self.target_capability_revision.as_str(),
            ),
            ("route revision", self.target_route_revision.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 4096 {
                return Err(format!(
                    "ContextEngine embedding decision has an invalid {name}"
                ));
            }
        }
        Ok(())
    }
}

/// Validate the canonical lowercase SHA-256 digest format used by host path identities.
/// 校验宿主路径身份使用的规范小写 SHA-256 摘要格式。
/// `value` contains the digest; returns true only for exactly sixty-four lowercase hex bytes.
/// `value` 包含摘要；仅精确六十四个小写十六进制字节返回真。
fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Validate a version-independent tagged SHA-256 content identity without hashing secrets.
/// 校验与版本无关的带标签 SHA-256 内容身份，不散列秘密。
/// `value` contains the identity; returns false for unknown tags or malformed digests.
/// `value` 包含身份；未知标签或格式错误的摘要返回假。
fn is_tagged_sha256(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(is_sha256_hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify legacy configurations have no implicit compatibility grants or peer profile edits.
    /// 验证旧配置没有隐式兼容授权或其他消费者配置改动。
    /// No parameters; returns unit after sparse serialization and legacy-load assertions.
    /// 无参数；稀疏序列化及旧版加载断言通过后返回空值。
    #[test]
    fn legacy_configuration_keeps_context_engine_policy_empty_and_sparse() {
        // Empty input represents the old document shape, not an automatically saved migration.
        // 空输入表示旧文档形态，而不是自动保存的迁移。
        let configuration: super::super::PersistedAiConfiguration =
            serde_json::from_str("{}").expect("legacy configuration");
        assert!(
            configuration
                .context_engine_embedding_compatibility
                .is_empty()
        );
        // Serialization must not persist a new policy when no explicit decision exists.
        // 不存在明确决定时，序列化不得持久化新策略。
        let serialized = serde_json::to_value(configuration).unwrap();
        assert!(
            serialized
                .get("context_engine_embedding_compatibility")
                .is_none()
        );
    }
}
