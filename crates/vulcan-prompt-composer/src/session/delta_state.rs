use crate::source::snapshot::ProjectRulesSnapshot;

/// Project-rules delta observation result.
/// 项目规则增量观察结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectRulesDeltaKind {
    /// No rules were observed.
    /// 未观察到规则。
    None,
    /// Rules did not change.
    /// 规则未变化。
    Unchanged,
    /// Rules changed and should emit a delta.
    /// 规则已变化且应发出增量。
    Changed,
}

/// Session-scoped project-rules delta tracker.
/// 会话级项目规则增量跟踪器。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectRulesDeltaState {
    /// Last observed rules hash.
    /// 最近观察到的规则哈希。
    pub last_rules_hash: Option<u64>,
}

impl ProjectRulesDeltaState {
    /// Observe the next project-rules snapshot and classify the delta.
    /// 观察下一份项目规则快照并分类增量结果。
    pub fn observe(&mut self, snapshot: Option<&ProjectRulesSnapshot>) -> ProjectRulesDeltaKind {
        let Some(snapshot) = snapshot else {
            return ProjectRulesDeltaKind::None;
        };
        let next_hash = snapshot.content_hash;
        let result = match self.last_rules_hash {
            Some(previous_hash) if previous_hash == next_hash => ProjectRulesDeltaKind::Unchanged,
            Some(_) => ProjectRulesDeltaKind::Changed,
            None => ProjectRulesDeltaKind::Changed,
        };
        self.last_rules_hash = Some(next_hash);
        result
    }
}
