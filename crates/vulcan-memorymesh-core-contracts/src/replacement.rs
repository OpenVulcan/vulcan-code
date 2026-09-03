//! Explicit candidate filtering and supersede authority independent of host hierarchy and new-record ownership.
//! 独立于宿主层级及新记录归属的显式候选过滤与替代授权。

use crate::{MAX_VISIBLE_SCOPES, MemoryRecord, ScopeAccess, ScopeId, SessionSourceId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Restricts physical candidates before ranking and truncation without granting any write permission.
/// 在排序和截断前限制物理候选，不授予任何写权限。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCandidateScope {
    /// Carries exact readable candidate scopes in Host-defined precedence order.
    /// 按宿主定义的优先级携带精确可读候选作用域。
    pub scope_ids: Vec<ScopeId>,
    /// Restricts every candidate to one exact origin session when present, including direct-write records.
    /// 存在时将全部候选限定到精确来源会话，包括主动写入记录。
    pub origin_session_source_id: Option<SessionSourceId>,
}

impl MemoryCandidateScope {
    /// Checks that a nonempty bounded candidate set is contained in the explicit command read grant.
    /// 检查非空有界候选集合是否包含于显式命令读取授权中。
    ///
    /// # Errors
    /// Rejects invalid parent scopes, empty/duplicate/oversized candidates, or candidates outside the parent grant.
    /// 拒绝非法父作用域、空或重复或超限候选，以及父授权外的候选。
    pub fn validate(&self, access: &ScopeAccess) -> Result<(), &'static str> {
        access.validate()?;
        validate_scope_subset(&self.scope_ids, &access.visible_scopes, false)
    }

    /// Tests the two physical filter dimensions without changing the record's real owner or origin.
    /// 检查两个物理过滤维度，不改变记录的真实所有者或来源。
    /// Subject, lifecycle, vector-space and authorization validation remain separate mandatory checks.
    /// 主体、生命周期、向量空间及授权校验仍是独立必需检查。
    #[must_use]
    pub fn matches(&self, memory: &MemoryRecord) -> bool {
        self.scope_ids.contains(&memory.scope_id)
            && self
                .origin_session_source_id
                .as_ref()
                .is_none_or(|session| memory.origin_session_source_id.as_ref() == Some(session))
    }
}

/// Separates semantic replacement candidates from the scopes whose existing records may be superseded.
/// 区分语义替代候选与允许替代既有记录的作用域。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryReplacementScope {
    /// Defines the independent physical candidate filter used by hard dedupe and semantic review.
    /// 定义硬去重及语义审核使用的独立物理候选过滤。
    pub candidates: MemoryCandidateScope,
    /// Explicitly permits superseding existing records in these candidate scopes; empty means read-only review.
    /// 显式允许替代这些候选作用域中的既有记录；空集合表示只读审核。
    pub authorized_supersede_scopes: Vec<ScopeId>,
}

impl MemoryReplacementScope {
    /// Validates that replacement authority is explicit and contained in the allowed candidate set.
    /// 校验替代授权明确给出且包含于允许的候选集合中。
    ///
    /// # Errors
    /// Rejects invalid candidates or duplicated, oversized and out-of-candidate supersede grants.
    /// 拒绝非法候选，以及重复、超限或候选集合外的替代授权。
    pub fn validate(&self, access: &ScopeAccess) -> Result<(), &'static str> {
        self.candidates.validate(access)?;
        validate_scope_subset(
            &self.authorized_supersede_scopes,
            &self.candidates.scope_ids,
            true,
        )
    }
}

/// Verifies explicit subsets without sorting or deduplicating away caller mistakes or precedence.
/// 校验显式子集，不通过排序或去重抹除调用方错误及优先级。
fn validate_scope_subset(
    values: &[ScopeId],
    allowed: &[ScopeId],
    allow_empty: bool,
) -> Result<(), &'static str> {
    if (!allow_empty && values.is_empty()) || values.len() > MAX_VISIBLE_SCOPES {
        return Err("replacement scope collection is empty or exceeds the supported limit");
    }
    let unique = values.iter().collect::<BTreeSet<_>>();
    if unique.len() != values.len() || values.iter().any(|scope| !allowed.contains(scope)) {
        return Err("replacement scopes are duplicated or outside the explicit parent grant");
    }
    Ok(())
}
