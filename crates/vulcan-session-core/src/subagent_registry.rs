//! Shared subagent registry models for main sessions and task sessions.
//! 主会话与子代理会话共享的子代理注册表模型。

use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
    /// Optional reason for the most recent stopped or closed state.
    /// 最近一次停止或关闭状态的可选原因。
    #[serde(default)]
    pub stop_reason: Option<String>,
    /// Creation timestamp in Unix milliseconds.
    /// 创建时间戳，单位为 Unix 毫秒。
    pub created_at_ms: u64,
    /// Last update timestamp in Unix milliseconds.
    /// 最近更新时间戳，单位为 Unix 毫秒。
    pub updated_at_ms: u64,
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
            stop_reason: None,
            created_at_ms: timestamp_ms,
            updated_at_ms: timestamp_ms,
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
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SubagentRegistry {
    /// Registry entries keyed first by direct parent and then by parent-scoped public agent id.
    /// 先按直接父级、再按父级范围公开代理标识索引的注册表条目。
    entries_by_parent: BTreeMap<SessionNodeId, BTreeMap<String, SubagentRegistryEntry>>,
}

/// Registry mutation or projection error that protects public slots and session-tree invariants.
/// 保护公开槽位与会话树不变量的注册表变更或投影错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentRegistryError {
    /// A public agent id cannot be represented as one stable tree-path segment.
    /// 一个公开代理标识无法表示为单个稳定树路径段。
    InvalidAgentPathSegment {
        /// Invalid public agent id.
        /// 无效的公开代理标识。
        agent_id: String,
    },
    /// Two parent-scoped public agent slots claim the same stable session id.
    /// 两个父级范围公开代理槽位声明了同一个稳定会话标识。
    DuplicateSessionId {
        /// Duplicated stable session id.
        /// 重复的稳定会话标识。
        session_id: SessionNodeId,
        /// Direct parent of the first owning slot.
        /// 首个所有者槽位的直接父级。
        first_parent_session_id: SessionNodeId,
        /// First public agent id owning the session.
        /// 首个拥有该会话的公开代理标识。
        first_agent_id: String,
        /// Direct parent of the second owning slot.
        /// 第二个所有者槽位的直接父级。
        second_parent_session_id: SessionNodeId,
        /// Second public agent id claiming the session.
        /// 第二个声明该会话的公开代理标识。
        second_agent_id: String,
    },
    /// A non-archived public agent slot is already bound to another session.
    /// 一个未归档公开代理槽位已经绑定到另一个会话。
    OccupiedAgentSlot {
        /// Direct parent that owns the public slot.
        /// 拥有公开槽位的直接父级。
        parent_session_id: SessionNodeId,
        /// Parent-scoped public agent id.
        /// 父级范围公开代理标识。
        agent_id: String,
        /// Stable session currently bound to the slot.
        /// 当前绑定到槽位的稳定会话。
        existing_session_id: SessionNodeId,
        /// Stable session attempting to claim the slot.
        /// 尝试声明槽位的稳定会话。
        incoming_session_id: SessionNodeId,
    },
    /// Reusing an archived public slot would discard one still-open descendant.
    /// 复用已归档公开槽位会丢弃一个仍未关闭的后代。
    ArchivedSlotSubtreeStillOpen {
        /// Archived root session selected for replacement.
        /// 被选中用于替换的已归档根会话。
        root_session_id: SessionNodeId,
        /// Still-open descendant session that blocks replacement.
        /// 阻止替换的仍未关闭后代会话。
        descendant_session_id: SessionNodeId,
        /// Public agent id of the still-open descendant.
        /// 仍未关闭后代的公开代理标识。
        descendant_agent_id: String,
    },
    /// A new public slot cannot be created beneath an archived parent agent.
    /// 不能在已归档父代理下创建新的公开槽位。
    ArchivedParentSession {
        /// Archived parent session that rejects the child.
        /// 拒绝子节点的已归档父会话。
        parent_session_id: SessionNodeId,
        /// Incoming child session.
        /// 传入的子会话。
        incoming_session_id: SessionNodeId,
        /// Incoming public child-agent id.
        /// 传入的公开子代理标识。
        incoming_agent_id: String,
    },
    /// An archived stable session cannot return to an open lifecycle state.
    /// 一个已归档稳定会话不能回到开放生命周期状态。
    ArchivedSessionReopened {
        /// Archived stable session selected for update.
        /// 被选中更新的已归档稳定会话。
        session_id: SessionNodeId,
        /// Public agent id bound to the archived session.
        /// 绑定到已归档会话的公开代理标识。
        agent_id: String,
        /// Incoming non-archived status.
        /// 传入的非归档状态。
        incoming_status: SubagentStatus,
    },
    /// An exact existing-entry update cannot find its parent-scoped public slot.
    /// 一次精确现有条目更新无法找到其父级范围公开槽位。
    MissingAgentSlot {
        /// Direct parent expected to own the slot.
        /// 预期拥有槽位的直接父级。
        parent_session_id: SessionNodeId,
        /// Expected parent-scoped public agent id.
        /// 预期的父级范围公开代理标识。
        agent_id: String,
        /// Stable session expected in the slot.
        /// 预期位于槽位中的稳定会话。
        session_id: SessionNodeId,
    },
    /// One atomic replacement batch contains the same public slot more than once.
    /// 一个原子替换批次多次包含同一公开槽位。
    DuplicateBatchAgentSlot {
        /// Direct parent that owns the duplicated slot.
        /// 拥有重复槽位的直接父级。
        parent_session_id: SessionNodeId,
        /// Duplicated parent-scoped public agent id.
        /// 重复的父级范围公开代理标识。
        agent_id: String,
    },
    /// Removing one exact entry would detach registered descendants.
    /// 移除一个精确条目会导致已注册后代脱离父级。
    EntryHasDescendants {
        /// Stable session selected for removal.
        /// 被选中移除的稳定会话。
        session_id: SessionNodeId,
        /// Public agent id selected for removal.
        /// 被选中移除的公开代理标识。
        agent_id: String,
    },
    /// An exact lifecycle mutation observed a different current status.
    /// 一次精确生命周期变更观察到了不同的当前状态。
    UnexpectedEntryStatus {
        /// Stable session selected for mutation.
        /// 被选中变更的稳定会话。
        session_id: SessionNodeId,
        /// Public agent id selected for mutation.
        /// 被选中变更的公开代理标识。
        agent_id: String,
        /// Status required by the mutation.
        /// 变更要求的状态。
        expected_status: SubagentStatus,
        /// Status currently stored in the registry.
        /// 注册表当前保存的状态。
        actual_status: SubagentStatus,
    },
    /// The requested subtree root does not exist in the registry snapshot.
    /// 请求的子树根不存在于注册表快照中。
    MissingRootSession {
        /// Missing stable root session id.
        /// 缺失的稳定根会话标识。
        session_id: SessionNodeId,
    },
    /// A registry parent chain contains a cycle.
    /// 注册表父级链包含环路。
    ParentCycle {
        /// Stable session id revisited by the parent chain.
        /// 父级链再次访问的稳定会话标识。
        session_id: SessionNodeId,
        /// Public agent id that owns the revisited session.
        /// 拥有被再次访问会话的公开代理标识。
        agent_id: String,
    },
}

impl std::fmt::Display for SubagentRegistryError {
    /// Formats one stable diagnostic for registry invariant failures.
    /// 为注册表不变量失败格式化稳定诊断信息。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the rendered structural diagnostic.
    /// `formatter` 接收渲染后的结构诊断信息。
    ///
    /// # Returns
    /// 返回值
    /// Formatting success or the formatter error.
    /// 格式化成功结果或格式化器错误。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAgentPathSegment { agent_id } => write!(
                formatter,
                "Subagent registry agent id `{agent_id}` must be one non-empty trimmed path segment without `/`."
            ),
            Self::DuplicateSessionId {
                session_id,
                first_parent_session_id,
                first_agent_id,
                second_parent_session_id,
                second_agent_id,
            } => write!(
                formatter,
                "Subagent registry session `{session_id}` is claimed by `{first_agent_id}` under parent `{first_parent_session_id}` and `{second_agent_id}` under parent `{second_parent_session_id}`."
            ),
            Self::OccupiedAgentSlot {
                parent_session_id,
                agent_id,
                existing_session_id,
                incoming_session_id,
            } => write!(
                formatter,
                "Subagent registry slot `{parent_session_id}/{agent_id}` is bound to session `{existing_session_id}` and cannot accept session `{incoming_session_id}`."
            ),
            Self::ArchivedSlotSubtreeStillOpen {
                root_session_id,
                descendant_session_id,
                descendant_agent_id,
            } => write!(
                formatter,
                "Subagent registry archived root `{root_session_id}` cannot be replaced while descendant `{descendant_agent_id}` at session `{descendant_session_id}` remains open."
            ),
            Self::ArchivedParentSession {
                parent_session_id,
                incoming_session_id,
                incoming_agent_id,
            } => write!(
                formatter,
                "Subagent registry archived parent session `{parent_session_id}` cannot accept child `{incoming_agent_id}` at session `{incoming_session_id}`."
            ),
            Self::ArchivedSessionReopened {
                session_id,
                agent_id,
                incoming_status,
            } => write!(
                formatter,
                "Subagent registry archived agent `{agent_id}` at session `{session_id}` cannot transition back to `{incoming_status:?}`."
            ),
            Self::MissingAgentSlot {
                parent_session_id,
                agent_id,
                session_id,
            } => write!(
                formatter,
                "Subagent registry slot `{parent_session_id}/{agent_id}` does not contain expected session `{session_id}`."
            ),
            Self::DuplicateBatchAgentSlot {
                parent_session_id,
                agent_id,
            } => write!(
                formatter,
                "Subagent registry replacement batch contains slot `{parent_session_id}/{agent_id}` more than once."
            ),
            Self::EntryHasDescendants {
                session_id,
                agent_id,
            } => write!(
                formatter,
                "Subagent registry agent `{agent_id}` at session `{session_id}` cannot be removed while it owns descendants."
            ),
            Self::UnexpectedEntryStatus {
                session_id,
                agent_id,
                expected_status,
                actual_status,
            } => write!(
                formatter,
                "Subagent registry agent `{agent_id}` at session `{session_id}` has status `{actual_status:?}` but the mutation requires `{expected_status:?}`."
            ),
            Self::MissingRootSession { session_id } => write!(
                formatter,
                "Subagent registry subtree root session `{session_id}` does not exist."
            ),
            Self::ParentCycle {
                session_id,
                agent_id,
            } => write!(
                formatter,
                "Subagent registry parent chain cycles through agent `{agent_id}` at session `{session_id}`."
            ),
        }
    }
}

impl std::error::Error for SubagentRegistryError {}

/// Ephemeral unique-session indexes shared by registry subtree and path projections.
/// 注册表子树与路径投影共享的临时唯一会话索引。
struct SubagentRegistryTreeIndex<'a> {
    /// Unique registry entry keyed by stable session id.
    /// 按稳定会话标识索引的唯一注册表条目。
    entries_by_session: BTreeMap<SessionNodeId, &'a SubagentRegistryEntry>,
    /// Direct child entries keyed by parent session id.
    /// 按父会话标识索引的直接子条目。
    children_by_parent: BTreeMap<SessionNodeId, Vec<&'a SubagentRegistryEntry>>,
}

impl SubagentRegistryTreeIndex<'_> {
    /// Validates every registry-owned parent chain before any subtree projection begins.
    /// 在任何子树投影开始前校验注册表拥有的全部父级链。
    ///
    /// # Returns
    /// 返回值
    /// Success when the registry is a forest, or the first deterministic parent-cycle error.
    /// 注册表构成森林时返回成功，否则返回首个确定性的父级环路错误。
    fn validate_parent_chains(&self) -> Result<(), SubagentRegistryError> {
        let mut validated_session_ids = BTreeSet::new();
        for start_session_id in self.entries_by_session.keys().copied() {
            if validated_session_ids.contains(&start_session_id) {
                continue;
            }
            let mut current_session_id = start_session_id;
            let mut current_chain = Vec::new();
            let mut current_chain_session_ids = BTreeSet::new();
            loop {
                if validated_session_ids.contains(&current_session_id) {
                    break;
                }
                if !current_chain_session_ids.insert(current_session_id) {
                    let entry = self.entries_by_session[&current_session_id];
                    return Err(SubagentRegistryError::ParentCycle {
                        session_id: current_session_id,
                        agent_id: entry.agent_id.clone(),
                    });
                }
                current_chain.push(current_session_id);
                let entry = self.entries_by_session[&current_session_id];
                let parent_session_id = entry.parent_session_id;
                // A parent outside the registry is an external main-session root boundary, not evidence of an orphan.
                // 注册表外的父节点是外部主会话根边界，并不能据此判定为孤儿节点。
                if !self.entries_by_session.contains_key(&parent_session_id) {
                    break;
                }
                current_session_id = parent_session_id;
            }
            validated_session_ids.extend(current_chain);
        }
        Ok(())
    }

    /// Validates that every descendant of an archived registry parent is also archived.
    /// 校验已归档注册表父节点的每个后代也都处于已归档状态。
    ///
    /// # Returns
    /// 返回值
    /// Success for lifecycle-consistent ancestry, or the first open descendant error.
    /// 对生命周期一致的祖先链返回成功，否则返回首个开放后代错误。
    fn validate_archived_ancestry(&self) -> Result<(), SubagentRegistryError> {
        for entry in self.entries_by_session.values() {
            if entry.status == SubagentStatus::Archived {
                continue;
            }
            if let Some(parent) = self.entries_by_session.get(&entry.parent_session_id)
                && parent.status == SubagentStatus::Archived
            {
                return Err(SubagentRegistryError::ArchivedSlotSubtreeStillOpen {
                    root_session_id: parent.session_id,
                    descendant_session_id: entry.session_id,
                    descendant_agent_id: entry.agent_id.clone(),
                });
            }
        }
        Ok(())
    }
}

impl SubagentRegistry {
    /// Validates one public agent id as a unique tree-path segment.
    /// 将一个公开代理标识校验为唯一树路径段。
    ///
    /// # Parameters
    /// 参数
    /// `agent_id` is the public child-agent id to validate.
    /// `agent_id` 是需要校验的公开子代理标识。
    ///
    /// # Returns
    /// 返回值
    /// Success for one non-empty trimmed segment, or a deterministic identity error.
    /// 对单个非空且已去除首尾空白的路径段返回成功，否则返回确定性身份错误。
    fn validate_agent_id(agent_id: &str) -> Result<(), SubagentRegistryError> {
        if agent_id.is_empty() || agent_id.trim() != agent_id || agent_id.contains('/') {
            return Err(SubagentRegistryError::InvalidAgentPathSegment {
                agent_id: agent_id.to_string(),
            });
        }
        Ok(())
    }

    /// Builds deterministic tree indexes and validates registry-wide structural invariants.
    /// 构建确定性的树索引，并校验注册表范围的结构不变量。
    ///
    /// # Returns
    /// 返回值
    /// Unique-session indexes, or the first deterministic structural error.
    /// 唯一会话索引，或首个确定性的结构错误。
    fn tree_index(&self) -> Result<SubagentRegistryTreeIndex<'_>, SubagentRegistryError> {
        let mut entries_by_session = BTreeMap::new();
        let mut children_by_parent = BTreeMap::<SessionNodeId, Vec<&SubagentRegistryEntry>>::new();
        for entry in self
            .entries_by_parent
            .values()
            .flat_map(|entries| entries.values())
        {
            Self::validate_agent_id(&entry.agent_id)?;
            if let Some(previous) = entries_by_session.insert(entry.session_id, entry) {
                return Err(SubagentRegistryError::DuplicateSessionId {
                    session_id: entry.session_id,
                    first_parent_session_id: previous.parent_session_id,
                    first_agent_id: previous.agent_id.clone(),
                    second_parent_session_id: entry.parent_session_id,
                    second_agent_id: entry.agent_id.clone(),
                });
            }
            children_by_parent
                .entry(entry.parent_session_id)
                .or_default()
                .push(entry);
        }
        for children in children_by_parent.values_mut() {
            children.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
        }
        let index = SubagentRegistryTreeIndex {
            entries_by_session,
            children_by_parent,
        };
        index.validate_parent_chains()?;
        index.validate_archived_ancestry()?;
        Ok(index)
    }

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

    /// Inserts one child-agent entry or replaces its exact parent-scoped public slot.
    /// 插入一个子代理条目，或替换其精确的父级范围公开槽位。
    ///
    /// # Parameters
    /// 参数
    /// `entry` is the child-agent registry entry.
    /// `entry` 是子代理注册表条目。
    ///
    /// # Returns
    /// 返回值
    /// The previous exact-slot entry, or a deterministic invariant error without mutation.
    /// 精确槽位中的旧条目，或不产生变更的确定性不变量错误。
    pub fn upsert(
        &mut self,
        entry: SubagentRegistryEntry,
    ) -> Result<Option<SubagentRegistryEntry>, SubagentRegistryError> {
        Self::validate_agent_id(&entry.agent_id)?;
        let parent_session_id = entry.parent_session_id;
        let agent_id = entry.agent_id.clone();
        let incoming_session_id = entry.session_id;
        let existing = self.get_for_parent(parent_session_id, &agent_id).cloned();
        let current_index = self.tree_index()?;
        if let Some(session_owner) = current_index.entries_by_session.get(&incoming_session_id)
            && (session_owner.parent_session_id != parent_session_id
                || session_owner.agent_id != agent_id)
        {
            return Err(SubagentRegistryError::DuplicateSessionId {
                session_id: incoming_session_id,
                first_parent_session_id: session_owner.parent_session_id,
                first_agent_id: session_owner.agent_id.clone(),
                second_parent_session_id: parent_session_id,
                second_agent_id: agent_id,
            });
        }
        let exact_slot_update = existing
            .as_ref()
            .is_some_and(|existing| existing.session_id == incoming_session_id);
        if exact_slot_update
            && let Some(existing) = existing.as_ref()
            && existing.status == SubagentStatus::Archived
            && entry.status != SubagentStatus::Archived
        {
            return Err(SubagentRegistryError::ArchivedSessionReopened {
                session_id: incoming_session_id,
                agent_id,
                incoming_status: entry.status,
            });
        }
        if !exact_slot_update
            && let Some(parent) = current_index.entries_by_session.get(&parent_session_id)
            && parent.status == SubagentStatus::Archived
        {
            return Err(SubagentRegistryError::ArchivedParentSession {
                parent_session_id,
                incoming_session_id,
                incoming_agent_id: agent_id,
            });
        }
        drop(current_index);
        let replacing_archived_session = existing
            .as_ref()
            .is_some_and(|existing| existing.session_id != incoming_session_id);
        if let Some(existing) = existing.as_ref()
            && replacing_archived_session
            && existing.status != SubagentStatus::Archived
        {
            return Err(SubagentRegistryError::OccupiedAgentSlot {
                parent_session_id,
                agent_id,
                existing_session_id: existing.session_id,
                incoming_session_id,
            });
        }
        let archived_subtree = if let Some(existing) = existing
            .as_ref()
            .filter(|existing| existing.session_id != incoming_session_id)
        {
            let root_session_id = existing.session_id;
            let subtree = self
                .list_subtree_from_session(root_session_id)?
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
            if let Some(descendant) = subtree
                .iter()
                .find(|descendant| descendant.status != SubagentStatus::Archived)
            {
                return Err(SubagentRegistryError::ArchivedSlotSubtreeStillOpen {
                    root_session_id,
                    descendant_session_id: descendant.session_id,
                    descendant_agent_id: descendant.agent_id.clone(),
                });
            }
            subtree
        } else {
            Vec::new()
        };
        for archived_entry in &archived_subtree {
            self.remove_unchecked(archived_entry.parent_session_id, &archived_entry.agent_id);
        }
        let previous = self.insert_unchecked(entry);
        if let Err(error) = self.tree_index().map(|_| ()) {
            self.remove_unchecked(parent_session_id, &agent_id);
            if archived_subtree.is_empty() {
                if let Some(previous) = previous.as_ref() {
                    self.insert_unchecked(previous.clone());
                }
            } else {
                for archived_entry in archived_subtree {
                    self.insert_unchecked(archived_entry);
                }
            }
            return Err(error);
        }
        Ok(existing.or(previous))
    }

    /// Replaces a batch of exact existing entries after one complete preflight validation.
    /// 在一次完整预检后批量替换精确匹配的现有条目。
    ///
    /// # Parameters
    /// 参数
    /// `entries` are exact session-bound projections that must already exist.
    /// `entries` 是必须已存在的精确会话绑定投影。
    ///
    /// # Returns
    /// 返回值
    /// Success after the whole batch commits, or an error before any entry changes.
    /// 整批提交后的成功结果，或任何条目变更前的错误。
    pub fn replace_existing_entries(
        &mut self,
        entries: &[SubagentRegistryEntry],
    ) -> Result<(), SubagentRegistryError> {
        self.tree_index()?;
        let mut previous_entries = Vec::with_capacity(entries.len());
        let mut batch_slots = BTreeSet::new();
        for entry in entries {
            if !batch_slots.insert((entry.parent_session_id, entry.agent_id.clone())) {
                return Err(SubagentRegistryError::DuplicateBatchAgentSlot {
                    parent_session_id: entry.parent_session_id,
                    agent_id: entry.agent_id.clone(),
                });
            }
            match self.get_for_parent(entry.parent_session_id, &entry.agent_id) {
                Some(existing) if existing.session_id == entry.session_id => {
                    if existing.status == SubagentStatus::Archived
                        && entry.status != SubagentStatus::Archived
                    {
                        return Err(SubagentRegistryError::ArchivedSessionReopened {
                            session_id: entry.session_id,
                            agent_id: entry.agent_id.clone(),
                            incoming_status: entry.status,
                        });
                    }
                    previous_entries.push(existing.clone());
                }
                Some(existing) => {
                    return Err(SubagentRegistryError::OccupiedAgentSlot {
                        parent_session_id: entry.parent_session_id,
                        agent_id: entry.agent_id.clone(),
                        existing_session_id: existing.session_id,
                        incoming_session_id: entry.session_id,
                    });
                }
                None => {
                    return Err(SubagentRegistryError::MissingAgentSlot {
                        parent_session_id: entry.parent_session_id,
                        agent_id: entry.agent_id.clone(),
                        session_id: entry.session_id,
                    });
                }
            }
        }
        for entry in entries {
            self.insert_unchecked(entry.clone());
        }
        if let Err(error) = self.tree_index().map(|_| ()) {
            for previous_entry in previous_entries {
                self.insert_unchecked(previous_entry);
            }
            return Err(error);
        }
        Ok(())
    }

    /// Gets one child-agent entry by direct parent and public id.
    /// 按直接父级与公开标识获取一个子代理条目。
    ///
    /// # Parameters
    /// 参数
    /// `parent_session_id` is the direct parent that owns the public slot.
    /// `parent_session_id` 是拥有公开槽位的直接父级。
    /// `agent_id` is the public child-agent id.
    /// `agent_id` 是公开子代理标识。
    ///
    /// # Returns
    /// 返回值
    /// The exact parent-scoped entry when it exists.
    /// 存在时返回精确的父级范围条目。
    pub fn get_for_parent(
        &self,
        parent_session_id: SessionNodeId,
        agent_id: &str,
    ) -> Option<&SubagentRegistryEntry> {
        self.entries_by_parent
            .get(&parent_session_id)
            .and_then(|entries| entries.get(agent_id))
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
        self.entries_by_parent
            .get(&parent_session_id)
            .map(|entries| entries.values().collect())
            .unwrap_or_default()
    }

    /// Lists every child-agent entry in this registry.
    /// 列出该注册表中的所有子代理条目。
    ///
    /// # Returns
    /// 返回值
    /// All registry entries without parent filtering.
    /// 不按父级过滤的全部注册表条目。
    pub fn list_all(&self) -> Vec<&SubagentRegistryEntry> {
        self.entries_by_parent
            .values()
            .flat_map(|entries| entries.values())
            .collect()
    }

    /// Lists one child-agent session and every registry descendant in deterministic breadth-first order.
    /// 按确定性的广度优先顺序列出一个子代理会话及其全部注册表后代。
    ///
    /// # Parameters
    /// 参数
    /// `root_session_id` is the session node that owns the requested subtree root.
    /// `root_session_id` 是拥有所请求子树根的会话节点。
    ///
    /// # Returns
    /// 返回值
    /// Root-first entries with siblings sorted by agent id, or a structural error.
    /// 根节点优先且同级按代理标识排序的条目，或结构错误。
    /// Registry-wide duplicate ownership and parent cycles are validated before root selection.
    /// 选择根节点前会校验注册表范围的重复归属与父级环路。
    pub fn list_subtree_from_session(
        &self,
        root_session_id: SessionNodeId,
    ) -> Result<Vec<&SubagentRegistryEntry>, SubagentRegistryError> {
        let index = self.tree_index()?;
        let root_entry = index
            .entries_by_session
            .get(&root_session_id)
            .copied()
            .ok_or(SubagentRegistryError::MissingRootSession {
                session_id: root_session_id,
            })?;
        let mut pending_entries = VecDeque::from([root_entry]);
        let mut subtree_entries = Vec::new();
        while let Some(entry) = pending_entries.pop_front() {
            subtree_entries.push(entry);
            if let Some(children) = index.children_by_parent.get(&entry.session_id) {
                pending_entries.extend(children.iter().copied());
            }
        }
        Ok(subtree_entries)
    }

    /// Projects every registry entry reachable from one root session into a unique tree path.
    /// 将从一个根会话可达的全部注册表条目投影为唯一树路径。
    ///
    /// # Parameters
    /// 参数
    /// `root_session_id` is the root main-session or task-session node.
    /// `root_session_id` 是根主会话或任务会话节点。
    ///
    /// # Returns
    /// 返回值
    /// Lexicographically sorted `/root/...` paths paired with entries, or a structural error.
    /// 按字典序排序并与条目配对的 `/root/...` 路径，或结构错误。
    /// Registry-wide duplicate ownership and parent cycles are validated before projection.
    /// 开始投影前会校验注册表范围的重复归属与父级环路。
    pub fn list_tree_paths_from_root(
        &self,
        root_session_id: SessionNodeId,
    ) -> Result<Vec<(String, &SubagentRegistryEntry)>, SubagentRegistryError> {
        let index = self.tree_index()?;
        let mut pending = VecDeque::from([(root_session_id, String::from("/root"))]);
        let mut projections = Vec::new();
        while let Some((parent_session_id, parent_path)) = pending.pop_front() {
            let Some(children) = index.children_by_parent.get(&parent_session_id) else {
                continue;
            };
            for child in children {
                let path = format!("{parent_path}/{}", child.agent_id);
                projections.push((path.clone(), *child));
                pending.push_back((child.session_id, path));
            }
        }
        projections.sort_by(|(left_path, left_entry), (right_path, right_entry)| {
            left_path
                .cmp(right_path)
                .then_with(|| left_entry.agent_id.cmp(&right_entry.agent_id))
        });
        Ok(projections)
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
        self.entries_by_parent
            .get(&parent_session_id)
            .into_iter()
            .flat_map(|entries| entries.values())
            .filter(|entry| entry.status.is_running())
            .count()
    }

    /// Removes one exact leaf entry without detaching registered descendants.
    /// 在不分离已注册后代的前提下移除一个精确叶子条目。
    ///
    /// # Parameters
    /// 参数
    /// `parent_session_id` is the direct parent that owns the public slot.
    /// `parent_session_id` 是拥有公开槽位的直接父级。
    /// `agent_id` is the public child-agent id.
    /// `agent_id` 是公开子代理标识。
    /// `expected_session_id` is the stable session that must own the slot.
    /// `expected_session_id` 是必须拥有该槽位的稳定会话。
    /// `expected_status` is the exact lifecycle status required for removal.
    /// `expected_status` 是移除操作要求的精确生命周期状态。
    ///
    /// # Returns
    /// 返回值
    /// The removed exact leaf, or an invariant error without mutation.
    /// 被移除的精确叶子条目，或不产生变更的不变量错误。
    pub fn remove_exact_leaf(
        &mut self,
        parent_session_id: SessionNodeId,
        agent_id: &str,
        expected_session_id: SessionNodeId,
        expected_status: SubagentStatus,
    ) -> Result<SubagentRegistryEntry, SubagentRegistryError> {
        let index = self.tree_index()?;
        let entry = match self.get_for_parent(parent_session_id, agent_id) {
            Some(entry) if entry.session_id == expected_session_id => entry,
            Some(entry) => {
                return Err(SubagentRegistryError::OccupiedAgentSlot {
                    parent_session_id,
                    agent_id: agent_id.to_string(),
                    existing_session_id: entry.session_id,
                    incoming_session_id: expected_session_id,
                });
            }
            None => {
                return Err(SubagentRegistryError::MissingAgentSlot {
                    parent_session_id,
                    agent_id: agent_id.to_string(),
                    session_id: expected_session_id,
                });
            }
        };
        if entry.status != expected_status {
            return Err(SubagentRegistryError::UnexpectedEntryStatus {
                session_id: expected_session_id,
                agent_id: agent_id.to_string(),
                expected_status,
                actual_status: entry.status,
            });
        }
        if index
            .children_by_parent
            .get(&expected_session_id)
            .is_some_and(|children| !children.is_empty())
        {
            return Err(SubagentRegistryError::EntryHasDescendants {
                session_id: expected_session_id,
                agent_id: agent_id.to_string(),
            });
        }
        drop(index);
        self.remove_unchecked(parent_session_id, agent_id).ok_or(
            SubagentRegistryError::MissingAgentSlot {
                parent_session_id,
                agent_id: agent_id.to_string(),
                session_id: expected_session_id,
            },
        )
    }

    /// Removes one parent-scoped entry after the caller has proven the complete transaction.
    /// 在调用方已经证明完整事务后移除一个父级范围条目。
    ///
    /// # Parameters
    /// 参数
    /// `parent_session_id` is the direct parent that owns the slot.
    /// `parent_session_id` 是拥有槽位的直接父级。
    /// `agent_id` is the public child-agent id.
    /// `agent_id` 是公开子代理标识。
    ///
    /// # Returns
    /// 返回值
    /// The removed entry when it existed.
    /// 存在时返回被移除的条目。
    fn remove_unchecked(
        &mut self,
        parent_session_id: SessionNodeId,
        agent_id: &str,
    ) -> Option<SubagentRegistryEntry> {
        let (removed, parent_is_empty) = {
            let entries = self.entries_by_parent.get_mut(&parent_session_id)?;
            let removed = entries.remove(agent_id);
            (removed, entries.is_empty())
        };
        if parent_is_empty {
            self.entries_by_parent.remove(&parent_session_id);
        }
        removed
    }

    /// Inserts one entry without validation after the caller has proven the complete transaction.
    /// 在调用方已经证明完整事务后，不经校验插入一个条目。
    ///
    /// # Parameters
    /// 参数
    /// `entry` is the exact parent-scoped registry entry to store.
    /// `entry` 是需要保存的精确父级范围注册表条目。
    ///
    /// # Returns
    /// 返回值
    /// The previous entry in the same parent-scoped public slot, if any.
    /// 同一父级范围公开槽位中的旧条目，如果存在。
    fn insert_unchecked(&mut self, entry: SubagentRegistryEntry) -> Option<SubagentRegistryEntry> {
        self.entries_by_parent
            .entry(entry.parent_session_id)
            .or_default()
            .insert(entry.agent_id.clone(), entry)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SubagentKind, SubagentRegistry, SubagentRegistryEntry, SubagentRegistryError,
        SubagentStatus,
    };
    use crate::model::SessionNodeId;

    /// Inserts one valid registry fixture and fails the owning test on invariant rejection.
    /// 插入一个有效注册表夹具，并在不变量拒绝时使所属测试失败。
    ///
    /// # Parameters
    /// 参数
    /// `registry` is the registry fixture being populated.
    /// `registry` 是正在填充的注册表夹具。
    /// `entry` is the valid parent-scoped entry to insert.
    /// `entry` 是需要插入的有效父级范围条目。
    fn insert_registry_entry(registry: &mut SubagentRegistry, entry: SubagentRegistryEntry) {
        registry
            .upsert(entry)
            .expect("valid registry fixture should satisfy all invariants");
    }

    /// Verifies registry entries preserve agent identity and process-local lifecycle facts.
    /// 验证注册表条目会保留代理身份与进程内生命周期事实。
    #[test]
    fn registry_entry_tracks_identity_and_lifecycle_facts() {
        let parent_id = SessionNodeId::new_v4();
        let child_id = SessionNodeId::new_v4();
        let mut entry =
            SubagentRegistryEntry::new("image-a", child_id, parent_id, SubagentKind::AgentMd, 10)
                .with_parent_turn_id("turn-1")
                .with_prompt_source("agents/image.md");

        entry.set_status(SubagentStatus::Completed, 20);
        entry.set_result_summary("image.png", 21);

        assert_eq!(entry.agent_kind, SubagentKind::AgentMd);
        assert_eq!(entry.parent_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(entry.prompt_source.as_deref(), Some("agents/image.md"));
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

        let first_session = SessionNodeId::new_v4();
        let second_session = SessionNodeId::new_v4();
        registry
            .upsert(SubagentRegistryEntry::new(
                "shared-name",
                first_session,
                first_parent,
                SubagentKind::Clone,
                10,
            ))
            .expect("first parent-scoped slot should insert");
        registry
            .upsert(SubagentRegistryEntry::new(
                "shared-name",
                second_session,
                second_parent,
                SubagentKind::Custom,
                10,
            ))
            .expect("same public id under another parent should insert");

        let entries = registry.list_for_parent(first_parent);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].agent_id, "shared-name");
        assert_eq!(entries[0].session_id, first_session);
        assert_eq!(
            registry
                .get_for_parent(second_parent, "shared-name")
                .expect("second parent-scoped slot should remain distinct")
                .session_id,
            second_session
        );
    }

    /// Verifies an occupied non-archived public slot rejects another session without mutation.
    /// 验证已占用的未归档公开槽位会无变更地拒绝另一个会话。
    #[test]
    fn registry_rejects_non_archived_public_slot_replacement() {
        let parent_id = SessionNodeId::new_v4();
        let existing_session_id = SessionNodeId::new_v4();
        let incoming_session_id = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();
        registry
            .upsert(SubagentRegistryEntry::new(
                "review",
                existing_session_id,
                parent_id,
                SubagentKind::Clone,
                10,
            ))
            .expect("initial public slot should insert");

        let error = registry
            .upsert(SubagentRegistryEntry::new(
                "review",
                incoming_session_id,
                parent_id,
                SubagentKind::Clone,
                20,
            ))
            .expect_err("non-archived public slot replacement should fail");

        assert!(matches!(
            error,
            SubagentRegistryError::OccupiedAgentSlot {
                parent_session_id,
                agent_id,
                existing_session_id: actual_existing_session_id,
                incoming_session_id: actual_incoming_session_id,
            } if parent_session_id == parent_id
                && agent_id == "review"
                && actual_existing_session_id == existing_session_id
                && actual_incoming_session_id == incoming_session_id
        ));
        assert_eq!(
            registry
                .get_for_parent(parent_id, "review")
                .expect("failed replacement must preserve the existing entry")
                .session_id,
            existing_session_id
        );
    }

    /// Verifies reusing an archived public slot removes its entire archived subtree atomically.
    /// 验证复用已归档公开槽位时会原子清除其完整已归档子树。
    #[test]
    fn registry_reuses_archived_slot_and_purges_complete_archived_subtree() {
        // Stable identities describe one archived subtree plus one unrelated sibling.
        // 稳定标识描述一个已归档子树以及一个无关同级节点。
        let root_parent_id = SessionNodeId::new_v4();
        let archived_root_id = SessionNodeId::new_v4();
        let archived_child_id = SessionNodeId::new_v4();
        let archived_grandchild_id = SessionNodeId::new_v4();
        let unrelated_id = SessionNodeId::new_v4();
        let replacement_id = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();
        let archived_root = SubagentRegistryEntry::new(
            "review",
            archived_root_id,
            root_parent_id,
            SubagentKind::Clone,
            10,
        );
        let archived_child = SubagentRegistryEntry::new(
            "child",
            archived_child_id,
            archived_root_id,
            SubagentKind::Clone,
            10,
        );
        let archived_grandchild = SubagentRegistryEntry::new(
            "grandchild",
            archived_grandchild_id,
            archived_child_id,
            SubagentKind::Clone,
            10,
        );
        insert_registry_entry(&mut registry, archived_root.clone());
        insert_registry_entry(&mut registry, archived_child.clone());
        insert_registry_entry(&mut registry, archived_grandchild.clone());
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new(
                "unrelated",
                unrelated_id,
                root_parent_id,
                SubagentKind::Clone,
                10,
            ),
        );

        // The complete subtree must cross the archival boundary in one atomic batch.
        // 完整子树必须在一个原子批次中跨越归档边界。
        let mut archived_root_update = archived_root;
        let mut archived_child_update = archived_child;
        let mut archived_grandchild_update = archived_grandchild;
        archived_root_update.set_status(SubagentStatus::Archived, 20);
        archived_child_update.set_status(SubagentStatus::Archived, 20);
        archived_grandchild_update.set_status(SubagentStatus::Archived, 20);
        registry
            .replace_existing_entries(&[
                archived_root_update,
                archived_child_update,
                archived_grandchild_update,
            ])
            .expect("complete archived subtree should commit atomically");

        let replaced = registry
            .upsert(SubagentRegistryEntry::new(
                "review",
                replacement_id,
                root_parent_id,
                SubagentKind::Custom,
                30,
            ))
            .expect("fully archived public slot should accept a new session")
            .expect("slot reuse should return the archived root entry");

        assert_eq!(replaced.session_id, archived_root_id);
        assert_eq!(replaced.status, SubagentStatus::Archived);
        assert_eq!(registry.list_all().len(), 2);
        assert_eq!(
            registry
                .get_for_parent(root_parent_id, "review")
                .expect("replacement root should own the reused public slot")
                .session_id,
            replacement_id
        );
        assert!(registry.get_for_parent(archived_root_id, "child").is_none());
        assert!(
            registry
                .get_for_parent(archived_child_id, "grandchild")
                .is_none()
        );
        assert_eq!(
            registry
                .get_for_parent(root_parent_id, "unrelated")
                .expect("unrelated sibling must survive archived subtree replacement")
                .session_id,
            unrelated_id
        );
    }

    /// Verifies partially archiving a parent with an open descendant fails and rolls back.
    /// 验证仅归档仍有开放后代的父级会失败并回滚。
    #[test]
    fn registry_rejects_partial_subtree_archive_without_mutation() {
        // Stable identities describe one parent and its still-open direct child.
        // 稳定标识描述一个父节点及其仍开放的直接子节点。
        let root_parent_id = SessionNodeId::new_v4();
        let root_id = SessionNodeId::new_v4();
        let child_id = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();
        let root_entry =
            SubagentRegistryEntry::new("root", root_id, root_parent_id, SubagentKind::Clone, 10);
        insert_registry_entry(&mut registry, root_entry.clone());
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new("child", child_id, root_id, SubagentKind::Clone, 10),
        );
        let mut archived_root = root_entry;
        archived_root.set_status(SubagentStatus::Archived, 20);

        let error = registry
            .replace_existing_entries(&[archived_root])
            .expect_err("open descendant should block partial subtree archival");

        assert!(matches!(
            error,
            SubagentRegistryError::ArchivedSlotSubtreeStillOpen {
                root_session_id,
                descendant_session_id,
                descendant_agent_id,
            } if root_session_id == root_id
                && descendant_session_id == child_id
                && descendant_agent_id == "child"
        ));
        assert_eq!(
            registry
                .get_for_parent(root_parent_id, "root")
                .expect("failed batch must restore the parent entry")
                .status,
            SubagentStatus::Created
        );
        assert_eq!(
            registry
                .get_for_parent(root_id, "child")
                .expect("failed batch must preserve the open descendant")
                .status,
            SubagentStatus::Created
        );
    }

    /// Verifies a complete subtree can transition to archived in one exact replacement batch.
    /// 验证完整子树可在一个精确替换批次中转换为已归档状态。
    #[test]
    fn registry_archives_complete_subtree_in_one_batch() {
        // Stable identities describe a three-level subtree.
        // 稳定标识描述一个三层子树。
        let root_parent_id = SessionNodeId::new_v4();
        let root_id = SessionNodeId::new_v4();
        let child_id = SessionNodeId::new_v4();
        let grandchild_id = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();
        let root_entry =
            SubagentRegistryEntry::new("root", root_id, root_parent_id, SubagentKind::Clone, 10);
        let child_entry =
            SubagentRegistryEntry::new("child", child_id, root_id, SubagentKind::Clone, 10);
        let grandchild_entry = SubagentRegistryEntry::new(
            "grandchild",
            grandchild_id,
            child_id,
            SubagentKind::Clone,
            10,
        );
        insert_registry_entry(&mut registry, root_entry.clone());
        insert_registry_entry(&mut registry, child_entry.clone());
        insert_registry_entry(&mut registry, grandchild_entry.clone());
        let mut archived_root = root_entry;
        let mut archived_child = child_entry;
        let mut archived_grandchild = grandchild_entry;
        archived_root.set_status(SubagentStatus::Archived, 20);
        archived_child.set_status(SubagentStatus::Archived, 20);
        archived_grandchild.set_status(SubagentStatus::Archived, 20);

        registry
            .replace_existing_entries(&[archived_root, archived_child, archived_grandchild])
            .expect("complete subtree archival should satisfy ancestry invariants");

        assert_eq!(registry.list_all().len(), 3);
        assert!(
            registry
                .list_all()
                .into_iter()
                .all(|entry| entry.status == SubagentStatus::Archived)
        );
    }

    /// Verifies archived parents reject new children and archived sessions reject reopening.
    /// 验证已归档父级拒绝新子节点，且已归档会话拒绝重新开放。
    #[test]
    fn registry_rejects_children_below_archived_parents_and_archived_session_reopen() {
        // Stable identities describe one archived root and one rejected incoming child.
        // 稳定标识描述一个已归档根节点及一个被拒绝的传入子节点。
        let root_parent_id = SessionNodeId::new_v4();
        let root_id = SessionNodeId::new_v4();
        let incoming_child_id = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();
        let mut archived_root =
            SubagentRegistryEntry::new("root", root_id, root_parent_id, SubagentKind::Clone, 10);
        archived_root.set_status(SubagentStatus::Archived, 20);
        insert_registry_entry(&mut registry, archived_root.clone());

        let child_error = registry
            .upsert(SubagentRegistryEntry::new(
                "child",
                incoming_child_id,
                root_id,
                SubagentKind::Clone,
                30,
            ))
            .expect_err("archived parent must reject a new child slot");

        assert!(matches!(
            child_error,
            SubagentRegistryError::ArchivedParentSession {
                parent_session_id,
                incoming_session_id,
                incoming_agent_id,
            } if parent_session_id == root_id
                && incoming_session_id == incoming_child_id
                && incoming_agent_id == "child"
        ));
        assert!(registry.get_for_parent(root_id, "child").is_none());

        let mut reopened_root = archived_root;
        reopened_root.set_status(SubagentStatus::Running, 40);
        let reopen_error = registry
            .upsert(reopened_root)
            .expect_err("archived stable session must not return to an open status");

        assert!(matches!(
            reopen_error,
            SubagentRegistryError::ArchivedSessionReopened {
                session_id,
                agent_id,
                incoming_status: SubagentStatus::Running,
            } if session_id == root_id && agent_id == "root"
        ));
        assert_eq!(
            registry
                .get_for_parent(root_parent_id, "root")
                .expect("rejected reopen must preserve the archived root")
                .status,
            SubagentStatus::Archived
        );
    }

    /// Verifies duplicate slots in one exact replacement batch fail before any mutation.
    /// 验证同一精确替换批次中的重复槽位会在任何变更前失败。
    #[test]
    fn registry_rejects_duplicate_batch_slots_without_mutation() {
        // Stable identities describe the single slot duplicated by the incoming batch.
        // 稳定标识描述被传入批次重复包含的单一槽位。
        let parent_id = SessionNodeId::new_v4();
        let session_id = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();
        let entry =
            SubagentRegistryEntry::new("review", session_id, parent_id, SubagentKind::Clone, 10);
        insert_registry_entry(&mut registry, entry.clone());
        let mut running_update = entry.clone();
        let mut completed_update = entry;
        running_update.set_status(SubagentStatus::Running, 20);
        completed_update.set_status(SubagentStatus::Completed, 30);

        let error = registry
            .replace_existing_entries(&[running_update, completed_update])
            .expect_err("duplicate batch slot should fail preflight");

        assert!(matches!(
            error,
            SubagentRegistryError::DuplicateBatchAgentSlot {
                parent_session_id,
                agent_id,
            } if parent_session_id == parent_id && agent_id == "review"
        ));
        assert_eq!(
            registry
                .get_for_parent(parent_id, "review")
                .expect("duplicate batch rejection must preserve the original entry")
                .status,
            SubagentStatus::Created
        );
    }

    /// Verifies exact leaf removal rejects descendants and status mismatches without mutation.
    /// 验证精确叶子移除会无变更地拒绝存在后代及状态不匹配的情况。
    #[test]
    fn registry_remove_exact_leaf_rejects_descendants_and_status_mismatch() {
        // Stable identities describe one parent entry and its leaf child.
        // 稳定标识描述一个父条目及其叶子子节点。
        let root_parent_id = SessionNodeId::new_v4();
        let root_id = SessionNodeId::new_v4();
        let child_id = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new("root", root_id, root_parent_id, SubagentKind::Clone, 10),
        );
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new("child", child_id, root_id, SubagentKind::Clone, 10),
        );

        let descendant_error = registry
            .remove_exact_leaf(root_parent_id, "root", root_id, SubagentStatus::Created)
            .expect_err("entry with descendants must not be detached");
        assert!(matches!(
            descendant_error,
            SubagentRegistryError::EntryHasDescendants {
                session_id,
                agent_id,
            } if session_id == root_id && agent_id == "root"
        ));

        let status_error = registry
            .remove_exact_leaf(root_id, "child", child_id, SubagentStatus::Running)
            .expect_err("leaf removal must require the exact current status");
        assert!(matches!(
            status_error,
            SubagentRegistryError::UnexpectedEntryStatus {
                session_id,
                agent_id,
                expected_status: SubagentStatus::Running,
                actual_status: SubagentStatus::Created,
            } if session_id == child_id && agent_id == "child"
        ));
        assert_eq!(registry.list_all().len(), 2);
        assert!(registry.get_for_parent(root_parent_id, "root").is_some());
        assert!(registry.get_for_parent(root_id, "child").is_some());
    }

    /// Verifies public agent ids containing a path separator are rejected without mutation.
    /// 验证包含路径分隔符的公开代理标识会被拒绝且不产生变更。
    #[test]
    fn registry_rejects_agent_id_with_path_separator() {
        // Stable identities describe the rejected entry's external parent and session.
        // 稳定标识描述被拒绝条目的外部父级与会话。
        let parent_id = SessionNodeId::new_v4();
        let session_id = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();

        let error = registry
            .upsert(SubagentRegistryEntry::new(
                "review/child",
                session_id,
                parent_id,
                SubagentKind::Clone,
                10,
            ))
            .expect_err("agent id containing slash must be rejected");

        assert!(matches!(
            error,
            SubagentRegistryError::InvalidAgentPathSegment { agent_id }
                if agent_id == "review/child"
        ));
        assert!(registry.list_all().is_empty());
    }

    /// Verifies subtree listing returns only the selected root and reachable descendants.
    /// 验证子树列出只返回选中根及其可达后代。
    #[test]
    fn registry_lists_complete_subtree_in_breadth_first_order() {
        let root_parent = SessionNodeId::new_v4();
        let root_id = SessionNodeId::new_v4();
        let first_child_id = SessionNodeId::new_v4();
        let second_child_id = SessionNodeId::new_v4();
        let grandchild_id = SessionNodeId::new_v4();
        let unrelated_id = SessionNodeId::new_v4();
        let second_root_parent = SessionNodeId::new_v4();
        let second_root_child_id = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new("root", root_id, root_parent, SubagentKind::Clone, 10),
        );
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new(
                "second-child",
                second_child_id,
                root_id,
                SubagentKind::Clone,
                10,
            ),
        );
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new(
                "first-child",
                first_child_id,
                root_id,
                SubagentKind::Clone,
                10,
            ),
        );
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new(
                "grandchild",
                grandchild_id,
                first_child_id,
                SubagentKind::Clone,
                10,
            ),
        );
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new(
                "unrelated",
                unrelated_id,
                root_parent,
                SubagentKind::Clone,
                10,
            ),
        );
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new(
                "second-root-child",
                second_root_child_id,
                second_root_parent,
                SubagentKind::Clone,
                10,
            ),
        );

        let agent_ids = registry
            .list_subtree_from_session(root_id)
            .expect("valid subtree should project")
            .into_iter()
            .map(|entry| entry.agent_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            agent_ids,
            vec!["root", "first-child", "second-child", "grandchild"]
        );
        let missing_root_id = SessionNodeId::new_v4();
        let missing_root_error = registry
            .list_subtree_from_session(missing_root_id)
            .expect_err("missing subtree root should be rejected");
        assert!(matches!(
            missing_root_error,
            SubagentRegistryError::MissingRootSession { session_id }
                if session_id == missing_root_id
        ));
        let tree_paths = registry
            .list_tree_paths_from_root(root_parent)
            .expect("valid registry tree should project paths")
            .into_iter()
            .map(|(path, entry)| (path, entry.agent_id.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            tree_paths,
            vec![
                (String::from("/root/root"), String::from("root")),
                (
                    String::from("/root/root/first-child"),
                    String::from("first-child")
                ),
                (
                    String::from("/root/root/first-child/grandchild"),
                    String::from("grandchild")
                ),
                (
                    String::from("/root/root/second-child"),
                    String::from("second-child")
                ),
                (String::from("/root/unrelated"), String::from("unrelated")),
            ]
        );
        let second_root_paths = registry
            .list_tree_paths_from_root(second_root_parent)
            .expect("second valid registry root should project independently")
            .into_iter()
            .map(|(path, entry)| (path, entry.agent_id.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            second_root_paths,
            vec![(
                String::from("/root/second-root-child"),
                String::from("second-root-child")
            )]
        );
    }

    /// Verifies one cycle-producing write is rejected and rolled back before projection.
    /// 验证一次产生环路的写入会在投影前被拒绝并回滚。
    #[test]
    fn registry_upsert_rejects_and_rolls_back_parent_cycles() {
        let first_id = SessionNodeId::new(uuid::Uuid::from_u128(1));
        let second_id = SessionNodeId::new(uuid::Uuid::from_u128(2));
        let mut registry = SubagentRegistry::new();
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new("first", first_id, second_id, SubagentKind::Clone, 10),
        );
        let error = registry
            .upsert(SubagentRegistryEntry::new(
                "second",
                second_id,
                first_id,
                SubagentKind::Clone,
                10,
            ))
            .expect_err("cycle-producing write should be rejected");

        assert!(matches!(
            error,
            SubagentRegistryError::ParentCycle {
                session_id,
                agent_id,
            } if session_id == first_id && agent_id == "first"
        ));
        assert_eq!(registry.list_all().len(), 1);
        assert!(registry.get_for_parent(first_id, "second").is_none());
        assert!(
            registry
                .list_tree_paths_from_root(SessionNodeId::new_v4())
                .expect("rolled-back registry should remain structurally valid")
                .is_empty()
        );
    }

    /// Verifies duplicate session ownership is rejected and rolled back at the write boundary.
    /// 验证重复会话所有权会在写入边界被拒绝并回滚。
    #[test]
    fn registry_upsert_rejects_duplicate_session_ownership() {
        let root_id = SessionNodeId::new_v4();
        let duplicate_session_id = SessionNodeId::new_v4();
        let mut registry = SubagentRegistry::new();
        insert_registry_entry(
            &mut registry,
            SubagentRegistryEntry::new(
                "first-owner",
                duplicate_session_id,
                root_id,
                SubagentKind::Clone,
                10,
            ),
        );
        let error = registry
            .upsert(SubagentRegistryEntry::new(
                "second-owner",
                duplicate_session_id,
                root_id,
                SubagentKind::Clone,
                10,
            ))
            .expect_err("duplicate session ownership should be rejected");

        assert!(matches!(
            error,
            SubagentRegistryError::DuplicateSessionId {
                session_id,
                first_parent_session_id,
                first_agent_id,
                second_parent_session_id,
                second_agent_id,
            } if session_id == duplicate_session_id
                && first_parent_session_id == root_id
                && first_agent_id == "first-owner"
                && second_parent_session_id == root_id
                && second_agent_id == "second-owner"
        ));
        assert_eq!(registry.list_all().len(), 1);
        assert!(registry.get_for_parent(root_id, "second-owner").is_none());
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

        insert_registry_entry(&mut registry, completed);
        insert_registry_entry(&mut registry, running);

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
