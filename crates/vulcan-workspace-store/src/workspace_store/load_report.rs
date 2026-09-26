//! Completeness evidence for a workspace index load, separate from the mutable runtime snapshot.
//! 工作区索引加载的完整性证据，与可变运行时快照分离。

use super::WorkspaceSnapshot;

/// The exact stage whose failure makes an index load incomplete.
/// 导致索引加载不完整的精确阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceIndexLoadStage {
    /// Session locator schema could not be prepared.
    /// 无法准备会话定位索引结构。
    Schema,
    /// Active session locators could not be listed.
    /// 无法列出活动会话定位器。
    ActiveList,
    /// Archived session locators could not be listed.
    /// 无法列出归档会话定位器。
    ArchivedList,
    /// A current-partition session has an invalid identity or lifecycle relationship.
    /// 当前分区的会话身份或生命周期归属非法。
    SessionIdentity,
    /// An exact metadata path could not be checked; this is not confirmed absence.
    /// 无法检查精确元数据路径；这不等于已经确认不存在。
    Metadata,
}

/// One reportable index failure, never represented as an authoritative empty list.
/// 一项可报告的索引失败，绝不表示为权威空列表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceIndexLoadIssue {
    /// The failed stage.
    /// 失败阶段。
    pub stage: WorkspaceIndexLoadStage,
    /// Owning project, when the operation was project-scoped.
    /// 项目级操作的所属项目。
    pub project_id: Option<String>,
    /// Exact affected session when known.
    /// 已知时受影响的精确会话。
    pub session_id: Option<String>,
    /// Original failure detail, retained for diagnosis rather than control-flow matching.
    /// 保留用于诊断的原始失败详情，不用于匹配控制流。
    pub detail: String,
}

/// A usable partial startup result plus explicit evidence of everything that could not be loaded.
/// 可用于启动的部分结果，以及所有未能加载部分的明确证据。
#[derive(Debug, Clone)]
pub struct WorkspaceIndexLoadReport {
    /// Validated index data only; no session history is hydrated by this operation.
    /// 仅包含已校验索引数据；此操作不水合会话历史。
    pub snapshot: WorkspaceSnapshot,
    /// Empty only when every supported retained index row was successfully inspected.
    /// 仅当全部受支持的保留索引记录均成功检查时为空。
    pub issues: Vec<WorkspaceIndexLoadIssue>,
}

impl WorkspaceIndexLoadReport {
    /// Returns true only for an authoritative complete load.
    /// 仅对权威完整加载返回真。
    pub fn is_complete(&self) -> bool {
        self.issues.is_empty()
    }

    /// Rejects partial refreshes so callers retain their last good projection on failure.
    /// 拒绝部分刷新，使调用方在失败时保留上一份有效投影。
    /// Startup may explicitly display `snapshot` only while also displaying its `issues`.
    /// 启动可以显式展示 snapshot，但必须同时展示 issues。
    pub fn into_complete(self) -> Result<WorkspaceSnapshot, String> {
        if self.is_complete() {
            return Ok(self.snapshot);
        }
        let first = &self.issues[0];
        Err(format!(
            "工作区索引读取不完整（{} 项问题）；未发布替换快照。阶段={:?} 项目={} 会话={}：{}",
            self.issues.len(),
            first.stage,
            first.project_id.as_deref().unwrap_or("-"),
            first.session_id.as_deref().unwrap_or("-"),
            first.detail,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn empty_snapshot() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: Default::default(),
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Empty complete data is valid, whereas failed empty data must never erase a previous view.
    /// 完整空数据有效；读取失败的空数据绝不能擦除上一份视图。
    #[test]
    fn failed_empty_load_is_distinct_from_confirmed_empty_workspace() {
        let complete = WorkspaceIndexLoadReport {
            snapshot: empty_snapshot(),
            issues: Vec::new(),
        };
        assert!(complete.is_complete());
        assert!(complete.into_complete().is_ok());
        let partial = WorkspaceIndexLoadReport {
            snapshot: empty_snapshot(),
            issues: vec![WorkspaceIndexLoadIssue {
                stage: WorkspaceIndexLoadStage::Metadata,
                project_id: Some("project".into()),
                session_id: Some("session".into()),
                detail: "permission denied".into(),
            }],
        };
        assert!(!partial.is_complete());
        let error = partial
            .into_complete()
            .expect_err("partial loads cannot publish a complete snapshot");
        assert!(error.contains("Metadata") && error.contains("permission denied"));
    }
}
