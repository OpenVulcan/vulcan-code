//! Shared subagent registry models for main sessions and task sessions.
//! 主会话与子代理会话共享的子代理注册表模型。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::SessionNodeId;

/// Describes how a child agent prompt was derived.
/// 描述子代理提示词的来源方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentKind {
    /// Child agent cloned from the parent agent context.
    /// 从父代理上下文克隆出的子代理。
    Clone,
    /// Child agent created from a concrete agent.md profile.
    /// 从具体 agent.md 画像创建出的子代理。
    AgentMd,
    /// Child agent created from custom host-provided instructions.
    /// 从宿主提供的自定义指令创建出的子代理。
    Custom,
}

/// Runtime and persisted status for one child agent.
/// 单个子代理的运行时与持久化状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    /// Child agent metadata exists but execution has not started.
    /// 子代理元数据已存在但尚未开始执行。
    Created,
    /// Child agent is actively running.
    /// 子代理正在运行。
    Running,
    /// Child agent is waiting for its own child agents.
    /// 子代理正在等待自己的子代理。
    WaitingChildren,
    /// Child agent is waiting for follow-up mailbox input.
    /// 子代理正在等待后续 mailbox 输入。
    WaitingFollowup,
    /// Child agent completed successfully.
    /// 子代理已成功完成。
    Completed,
    /// Child agent failed.
    /// 子代理已失败。
    Failed,
    /// Child agent was stopped by the host or parent session.
    /// 子代理已被宿主或父会话停止。
    Stopped,
    /// Child agent is compacting its context.
    /// 子代理正在压缩上下文。
    Compacting,
    /// Child agent has been archived with its parent session.
    /// 子代理已随父会话归档。
    Archived,
}

impl SubagentStatus {
    /// Returns whether this status represents active work.
    /// 返回该状态是否代表仍有活动工作。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the child agent should still be considered running.
    /// 当子代理仍应被视为运行中时返回 `true`。
    pub const fn is_running(self) -> bool {
        matches!(
            self,
            Self::Created
                | Self::Running
                | Self::WaitingChildren
                | Self::WaitingFollowup
                | Self::Compacting
        )
    }

    /// Returns whether this status is terminal.
    /// 返回该状态是否为终态。
    ///
    /// # Returns
    /// 返回值
    /// `true` when no further child-agent execution is expected.
    /// 当不再预期有进一步子代理执行时返回 `true`。
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Stopped)
    }
}

/// Summary for one segmented session JSONL file.
/// 单个分段会话 JSONL 文件的摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSegmentSummary {
    /// Segment file name relative to the owning session directory.
    /// 相对于所属会话目录的分段文件名。
    pub file_name: String,
    /// Number of JSONL lines stored in this segment.
    /// 当前分段中存储的 JSONL 行数。
    pub line_count: u64,
}

impl SessionSegmentSummary {
    /// Builds one segment summary.
    /// 构建一个分段摘要。
    ///
    /// # Parameters
    /// 参数
    /// `file_name` is the segment file name relative to the session directory.
    /// `file_name` 是相对于会话目录的分段文件名。
    /// `line_count` is the number of JSONL lines in the segment.
    /// `line_count` 是分段中的 JSONL 行数。
    ///
    /// # Returns
    /// 返回值
    /// A segment summary ready for metadata serialization.
    /// 可用于元数据序列化的分段摘要。
    pub fn new(file_name: impl Into<String>, line_count: u64) -> Self {
        Self {
            file_name: file_name.into(),
            line_count,
        }
    }
}

/// Registry entry for one child-agent session.
/// 单个子代理会话的注册表条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentRegistryEntry {
    /// Stable public child-agent id used by model-visible tools.
    /// 模型可见工具使用的稳定公开子代理标识。
    pub agent_id: String,
    /// Session node id used by storage and runtime kernels.
    /// 存储与运行时内核使用的会话节点标识。
    pub session_id: SessionNodeId,
    /// Parent session node id that owns this child agent.
    /// 拥有该子代理的父会话节点标识。
    pub parent_session_id: SessionNodeId,
    /// Parent turn id that spawned this child agent.
    /// 创建该子代理的父级回合标识。
    pub parent_turn_id: Option<String>,
    /// Child agent derivation kind.
    /// 子代理派生类型。
    pub agent_kind: SubagentKind,
    /// Optional prompt source such as an agent.md path or custom label.
    /// 可选提示词来源，例如 agent.md 路径或自定义标签。
    pub prompt_source: Option<String>,
    /// Current child-agent status.
    /// 当前子代理状态。
    pub status: SubagentStatus,
    /// Creation timestamp in Unix milliseconds.
    /// 创建时间戳，单位为 Unix 毫秒。
    pub created_at_ms: u64,
    /// Last update timestamp in Unix milliseconds.
    /// 最近更新时间戳，单位为 Unix 毫秒。
    pub updated_at_ms: u64,
    /// Segment summaries used to reconstruct the child transcript.
    /// 用于重建子代理转录内容的分段摘要。
    pub segments: Vec<SessionSegmentSummary>,
    /// Most recent task or follow-up instruction delivered to this child agent.
    /// 最近一次投递给该子代理的任务或后续指令。
    #[serde(default)]
    pub last_task_message: Option<String>,
    /// Optional terminal result summary.
    /// 可选终态结果摘要。
    pub result_summary: Option<String>,
}

impl SubagentRegistryEntry {
    /// Builds one child-agent registry entry.
    /// 构建一个子代理注册表条目。
    ///
    /// # Parameters
    /// 参数
    /// `agent_id` is the stable public tool-facing child-agent id.
    /// `agent_id` 是稳定的工具可见子代理公开标识。
    /// `session_id` is the child session node id.
    /// `session_id` 是子代理会话节点标识。
    /// `parent_session_id` is the parent session node id.
    /// `parent_session_id` 是父会话节点标识。
    /// `agent_kind` describes how the child prompt is derived.
    /// `agent_kind` 描述子代理提示词的派生方式。
    /// `timestamp_ms` is the creation and initial update timestamp.
    /// `timestamp_ms` 是创建与初始更新时间戳。
    ///
    /// # Returns
    /// 返回值
    /// A registry entry in `created` status.
    /// 一个处于 `created` 状态的注册表条目。
    pub fn new(
        agent_id: impl Into<String>,
        session_id: SessionNodeId,
        parent_session_id: SessionNodeId,
        agent_kind: SubagentKind,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            session_id,
            parent_session_id,
            parent_turn_id: None,
            agent_kind,
            prompt_source: None,
            status: SubagentStatus::Created,
            created_at_ms: timestamp_ms,
            updated_at_ms: timestamp_ms,
            segments: Vec::new(),
            last_task_message: None,
            result_summary: None,
        }
    }

    /// Sets the parent turn id that spawned this child agent.
    /// 设置创建该子代理的父级回合标识。
    ///
    /// # Parameters
    /// 参数
    /// `parent_turn_id` is the parent turn id.
    /// `parent_turn_id` 是父级回合标识。
    ///
    /// # Returns
    /// 返回值
    /// The updated registry entry.
    /// 更新后的注册表条目。
    pub fn with_parent_turn_id(mut self, parent_turn_id: impl Into<String>) -> Self {
        self.parent_turn_id = Some(parent_turn_id.into());
        self
    }

    /// Sets the prompt source for this child agent.
    /// 设置该子代理的提示词来源。
    ///
    /// # Parameters
    /// 参数
    /// `prompt_source` is the prompt source label or path.
    /// `prompt_source` 是提示词来源标签或路径。
    ///
    /// # Returns
    /// 返回值
    /// The updated registry entry.
    /// 更新后的注册表条目。
    pub fn with_prompt_source(mut self, prompt_source: impl Into<String>) -> Self {
        self.prompt_source = Some(prompt_source.into());
        self
    }

    /// Updates the status and timestamp for this child agent.
    /// 更新该子代理的状态与时间戳。
    ///
    /// # Parameters
    /// 参数
    /// `status` is the new child-agent status.
    /// `status` 是新的子代理状态。
    /// `timestamp_ms` is the update timestamp.
    /// `timestamp_ms` 是更新时间戳。
    pub fn set_status(&mut self, status: SubagentStatus, timestamp_ms: u64) {
        self.status = status;
        self.updated_at_ms = timestamp_ms;
    }

    /// Upserts one segment line count summary.
    /// 更新或插入一个分段行数摘要。
    ///
    /// # Parameters
    /// 参数
    /// `file_name` is the segment file name.
    /// `file_name` 是分段文件名。
    /// `line_count` is the current segment line count.
    /// `line_count` 是当前分段行数。
    pub fn upsert_segment(&mut self, file_name: impl Into<String>, line_count: u64) {
        let file_name = file_name.into();
        if let Some(segment) = self
            .segments
            .iter_mut()
            .find(|segment| segment.file_name == file_name)
        {
            segment.line_count = line_count;
            return;
        }
        self.segments
            .push(SessionSegmentSummary::new(file_name, line_count));
    }

    /// Sets the terminal result summary and update timestamp.
    /// 设置终态结果摘要与更新时间戳。
    ///
    /// # Parameters
    /// 参数
    /// `result_summary` is the terminal result summary.
    /// `result_summary` 是终态结果摘要。
    /// `timestamp_ms` is the update timestamp.
    /// `timestamp_ms` 是更新时间戳。
    pub fn set_result_summary(&mut self, result_summary: impl Into<String>, timestamp_ms: u64) {
        self.result_summary = Some(result_summary.into());
        self.updated_at_ms = timestamp_ms;
    }
}

/// Parent-scoped registry of child-agent sessions.
/// 父会话范围内的子代理会话注册表。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentRegistry {
    /// Registry entries keyed by public child-agent id.
    /// 按公开子代理标识索引的注册表条目。
    entries: BTreeMap<String, SubagentRegistryEntry>,
}

impl SubagentRegistry {
    /// Builds an empty subagent registry.
    /// 构建一个空子代理注册表。
    ///
    /// # Returns
    /// 返回值
    /// An empty registry.
    /// 一个空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces one child-agent entry.
    /// 插入或替换一个子代理条目。
    ///
    /// # Parameters
    /// 参数
    /// `entry` is the child-agent registry entry.
    /// `entry` 是子代理注册表条目。
    ///
    /// # Returns
    /// 返回值
    /// The previous entry for the same agent id, if any.
    /// 同一子代理标识的旧条目，如果存在。
    pub fn upsert(&mut self, entry: SubagentRegistryEntry) -> Option<SubagentRegistryEntry> {
        self.entries.insert(entry.agent_id.clone(), entry)
    }

    /// Gets one child-agent entry by public id.
    /// 按公开标识获取一个子代理条目。
    ///
    /// # Parameters
    /// 参数
    /// `agent_id` is the public child-agent id.
    /// `agent_id` 是公开子代理标识。
    ///
    /// # Returns
    /// 返回值
    /// The matching entry when it exists.
    /// 存在时返回匹配条目。
    pub fn get(&self, agent_id: &str) -> Option<&SubagentRegistryEntry> {
        self.entries.get(agent_id)
    }

    /// Gets one mutable child-agent entry by public id.
    /// 按公开标识获取一个可变子代理条目。
    ///
    /// # Parameters
    /// 参数
    /// `agent_id` is the public child-agent id.
    /// `agent_id` 是公开子代理标识。
    ///
    /// # Returns
    /// 返回值
    /// The mutable matching entry when it exists.
    /// 存在时返回可变匹配条目。
    pub fn get_mut(&mut self, agent_id: &str) -> Option<&mut SubagentRegistryEntry> {
        self.entries.get_mut(agent_id)
    }

    /// Lists child-agent entries for one parent session.
    /// 列出单个父会话下的子代理条目。
    ///
    /// # Parameters
    /// 参数
    /// `parent_session_id` is the parent session node id.
    /// `parent_session_id` 是父会话节点标识。
    ///
    /// # Returns
    /// 返回值
    /// Entries that belong to the requested parent session.
    /// 属于指定父会话的条目。
    pub fn list_for_parent(&self, parent_session_id: SessionNodeId) -> Vec<&SubagentRegistryEntry> {
        self.entries
            .values()
            .filter(|entry| entry.parent_session_id == parent_session_id)
            .collect()
    }

    /// Lists every child-agent entry in this registry.
    /// 列出该注册表中的所有子代理条目。
    ///
    /// # Returns
    /// 返回值
    /// All registry entries without parent filtering.
    /// 不按父级过滤的全部注册表条目。
    pub fn list_all(&self) -> Vec<&SubagentRegistryEntry> {
        self.entries.values().collect()
    }

    /// Counts running child agents for one parent session.
    /// 统计单个父会话下仍在运行的子代理数量。
    ///
    /// # Parameters
    /// 参数
    /// `parent_session_id` is the parent session node id.
    /// `parent_session_id` 是父会话节点标识。
    ///
    /// # Returns
    /// 返回值
    /// Number of child agents that still have active work.
    /// 仍有活动工作的子代理数量。
    pub fn running_count_for_parent(&self, parent_session_id: SessionNodeId) -> usize {
        self.entries
            .values()
            .filter(|entry| {
                entry.parent_session_id == parent_session_id && entry.status.is_running()
            })
            .count()
    }

    /// Removes one child-agent entry by public id.
    /// 按公开标识移除一个子代理条目。
    ///
    /// # Parameters
    /// 参数
    /// `agent_id` is the public child-agent id.
    /// `agent_id` 是公开子代理标识。
    ///
    /// # Returns
    /// 返回值
    /// The removed entry when it existed.
    /// 存在时返回被移除的条目。
    pub fn remove(&mut self, agent_id: &str) -> Option<SubagentRegistryEntry> {
        self.entries.remove(agent_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{SubagentKind, SubagentRegistry, SubagentRegistryEntry, SubagentStatus};
    use crate::model::SessionNodeId;

    /// Verifies registry entries preserve agent kind and segment line counts.
    /// 验证注册表条目会保留代理类型与分段行数。
    #[test]
    fn registry_entry_tracks_kind_and_segment_line_count() {
        let parent_id = SessionNodeId::new_v4();
        let child_id = SessionNodeId::new_v4();
        let mut entry =
            SubagentRegistryEntry::new("image-a", child_id, parent_id, SubagentKind::AgentMd, 10)
                .with_parent_turn_id("turn-1")
                .with_prompt_source("agents/image.md");

        entry.upsert_segment("s001.jsonl", 3000);
        entry.upsert_segment("s001.jsonl", 3200);
        entry.set_status(SubagentStatus::Completed, 20);
        entry.set_result_summary("image.png", 21);

        assert_eq!(entry.agent_kind, SubagentKind::AgentMd);
        assert_eq!(entry.parent_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(entry.prompt_source.as_deref(), Some("agents/image.md"));
        assert_eq!(entry.segments.len(), 1);
        assert_eq!(entry.segments[0].line_count, 3200);
        assert_eq!(entry.result_summary.as_deref(), Some("image.png"));
        assert!(entry.status.is_terminal());
    }

    /// Verifies parent-scoped listing excludes child agents from other parents.
    /// 验证按父会话列出时会排除其他父会话的子代理。
    #[test]
    fn registry_lists_entries_for_one_parent_only() {
        let first_parent = SessionNodeId::new_v4();
        let second_parent = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();

        registry.upsert(SubagentRegistryEntry::new(
            "a",
            SessionNodeId::new_v4(),
            first_parent,
            SubagentKind::Clone,
            10,
        ));
        registry.upsert(SubagentRegistryEntry::new(
            "b",
            SessionNodeId::new_v4(),
            second_parent,
            SubagentKind::Custom,
            10,
        ));

        let entries = registry.list_for_parent(first_parent);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].agent_id, "a");
    }

    /// Verifies running counts include only active child-agent states.
    /// 验证运行中数量只包含活动子代理状态。
    #[test]
    fn registry_counts_running_entries_only() {
        let parent_id = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();
        let mut completed = SubagentRegistryEntry::new(
            "done",
            SessionNodeId::new_v4(),
            parent_id,
            SubagentKind::Clone,
            10,
        );
        completed.set_status(SubagentStatus::Completed, 20);
        let mut running = SubagentRegistryEntry::new(
            "run",
            SessionNodeId::new_v4(),
            parent_id,
            SubagentKind::Clone,
            10,
        );
        running.set_status(SubagentStatus::Running, 20);

        registry.upsert(completed);
        registry.upsert(running);

        assert_eq!(registry.running_count_for_parent(parent_id), 1);
    }

    /// Verifies older persisted entries without last_task_message still deserialize.
    /// 验证缺少 last_task_message 的旧持久化条目仍可反序列化。
    #[test]
    fn registry_entry_deserializes_without_last_task_message() {
        let parent_id = SessionNodeId::new_v4();
        let child_id = SessionNodeId::new_v4();
        let entry =
            SubagentRegistryEntry::new("legacy", child_id, parent_id, SubagentKind::Clone, 10);
        let mut value = serde_json::to_value(entry).expect("entry should serialize");
        value
            .as_object_mut()
            .expect("entry should be a JSON object")
            .remove("last_task_message");

        let restored: SubagentRegistryEntry =
            serde_json::from_value(value).expect("legacy entry should deserialize");

        assert_eq!(restored.agent_id, "legacy");
        assert!(restored.last_task_message.is_none());
    }
}
