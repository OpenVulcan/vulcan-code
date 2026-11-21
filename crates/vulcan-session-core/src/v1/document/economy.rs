//! Root-turn ownership, snapshot admission and transactional settlement for economy mode.
//! 节约模式的根回合归属、快照准入和事务结算。

use super::{DirtyFile, SessionDocument, TurnOrigin};
use crate::v1::economy::{EconomyCandidate, EconomyTurn, SettledEconomyTurn};
use crate::v1::event::{
    ApiPurposeKind, AssistantSemanticStatus, EventBody, ToolCallEvent, ToolCallStatus,
};
use crate::v1::{DomainError, TurnId};
use vulcan_ai_contract::economy_window::EconomyWindow;

impl SessionDocument {
    /// Returns the compaction used by request history, replay validation and cleanup protection.
    /// 返回请求历史、回放证明及清理保护共同采用的压缩节点。
    pub fn effective_context_compaction_id(&self) -> Option<crate::v1::CompactionId> {
        self.metadata
            .config
            .economy
            .visible_compaction(self.runtime_lock.current_compaction_id)
    }

    /// Applies an explicit selection at the expected revision and invalidates in-flight candidates.
    /// 在期望修订上应用显式选择，并使在途候选失效。
    pub fn set_economy_mode(
        &mut self,
        enabled: bool,
        revision: u64,
        now: i64,
    ) -> Result<bool, DomainError> {
        if self.dedicated_profile().is_some()
            || self.metadata.child_execution_profile.is_some()
            || self.metadata.archived_at_ms.is_some()
        {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "economy.regular_root_required",
            });
        }
        let state = &self.metadata.config.economy;
        if state.revision != revision || revision >= crate::v1::ids::JS_SAFE_INTEGER_MAX {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "economy.revision",
            });
        }
        if state.enabled == enabled {
            return Ok(false);
        }
        self.next_mutation_revision()?;
        let deferred = self.active_turn().filter(|t| !t.closed).map(|t| t.turn_id);
        let state = &mut self.metadata.config.economy;
        state.enabled = enabled;
        state.revision += 1;
        state.active = None;
        state.deferred_turn = deferred;
        state.initialize = enabled;
        self.bump_revision(&[DirtyFile::Metadata], now, &[]);
        Ok(true)
    }

    /// Enables an explicitly selected draft preference before the first physical request of a fresh document.
    /// 在新建文档首次物理请求前应用用户显式选择的草稿偏好。
    pub fn enable_draft_economy(
        &mut self,
        window: EconomyWindow,
        now: i64,
    ) -> Result<(), DomainError> {
        if self.resident_start_seq > 1
            || self
                .events()
                .iter()
                .any(|e| matches!(e.event, EventBody::ApiStarted(_)))
        {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "economy.draft_dispatched",
            });
        }
        self.apply_atomic_transition(|doc| {
            doc.set_economy_mode(true, 0, now)?;
            doc.metadata.config.economy.deferred_turn = None;
            doc.prepare_economy_turn(window, now)?;
            Ok(())
        })
    }

    /// Freezes the root's window before any preflight compression; reports whether state changed.
    /// 在预检压缩前冻结根回合窗口，并返回状态是否改变。
    pub fn prepare_economy_turn(
        &mut self,
        window: EconomyWindow,
        now: i64,
    ) -> Result<bool, DomainError> {
        let Some(root) = self
            .active_turn()
            .filter(|t| !t.closed && t.origin == TurnOrigin::UserRoot)
            .cloned()
        else {
            return Ok(false);
        };
        let state = &self.metadata.config.economy;
        if !state.enabled
            || state.deferred_turn == Some(root.turn_id)
            || (self.metadata.mode.goal.enabled
                && self.metadata.mode.goal.state == crate::v1::config::GoalState::Active)
            || self.metadata.child_execution_profile.is_some()
            || self.dedicated_profile().is_some()
        {
            return Ok(false);
        }
        if state
            .active
            .as_ref()
            .is_some_and(|active| active.turn_id == root.turn_id)
        {
            return Ok(false);
        }
        window
            .validate()
            .map_err(|_| DomainError::InvalidSelectionConfiguration {
                field: "economy.window",
            })?;
        self.next_mutation_revision()?;
        let baseline = state.visible_compaction(self.runtime_lock.current_compaction_id);
        // A cold resident suffix can lack old events. The durable business usage proves earlier execution.
        // 冷载入后驻留事件可能缺少历史；已持久化的业务事件用于识别先前执行。
        let has_history = self.resident_start_seq > 1
            || self.events().iter().any(|event| {
                event.seq < root.start_seq
                    && matches!(
                        &event.event,
                        EventBody::UserInput(_)
                            | EventBody::AssistantText(_)
                            | EventBody::SubagentReturn(_)
                    )
            });
        let state = &mut self.metadata.config.economy;
        state.initialize &= has_history;
        state.deferred_turn = None;
        state.active = Some(EconomyTurn {
            turn_id: root.turn_id,
            start: root.start_seq,
            baseline,
            window,
        });
        self.bump_revision(&[DirtyFile::Metadata], now, &[]);
        Ok(true)
    }

    /// Publishes the activation baseline only for the root and revision that requested it.
    /// 只为发起初始化的根回合及其修订发布基线。
    pub fn finish_economy_initialization(
        &mut self,
        owner: TurnId,
        revision: u64,
        now: i64,
    ) -> Result<bool, DomainError> {
        let state = &self.metadata.config.economy;
        if !state.initialize
            || state.revision != revision
            || state.active.as_ref().map(|a| a.turn_id) != Some(owner)
        {
            return Ok(false);
        }
        let baseline = self.runtime_lock.current_compaction_id.ok_or(
            DomainError::InvalidSelectionConfiguration {
                field: "economy.initial_baseline",
            },
        )?;
        self.next_mutation_revision()?;
        let state = &mut self.metadata.config.economy;
        state.initialize = false;
        if let Some(active) = &mut state.active {
            active.baseline = Some(baseline);
            // Previous receipts are now represented by the activation baseline, including protected turns.
            // 旧回执已进入此次启用基线，含受保护回合；后续批量窗口从当前回合重新计数。
            for receipt in state.settled.iter_mut().filter(|r| r.retired_at.is_none()) {
                receipt.retired_at = Some(active.start);
            }
        }
        self.bump_revision(&[DirtyFile::Metadata], now, &[]);
        Ok(true)
    }

    /// Returns a revision only for a prepared, initialized and currently owned user task.
    /// 仅为当前已准备且完成初始化的用户任务返回修订。
    pub fn economy_revision_for_request(&self) -> Option<u64> {
        let state = &self.metadata.config.economy;
        let root = self
            .active_turn()
            .filter(|t| !t.closed && t.origin == TurnOrigin::UserRoot)?;
        let goal = &self.metadata.mode.goal;
        (state.enabled
            && !state.initialize
            && state.active.as_ref()?.turn_id == root.turn_id
            && !(goal.enabled && goal.state == crate::v1::config::GoalState::Active))
            .then_some(state.revision)
    }

    /// Records the handoff within the owning close transaction when all sources remain verifiable.
    /// 当来源仍可核验时，在所属闭合事务中记录交接信息。
    pub fn settle_economy_turn(
        &mut self,
        owner: TurnId,
        candidate: &EconomyCandidate,
        sealed_prefix: &[crate::v1::event::ConversationEvent],
        tool_settled: impl Fn(&ToolCallEvent) -> bool,
    ) -> Result<(), DomainError> {
        if self.economy_revision_for_request() != Some(candidate.revision)
            || !self.metadata.input_state.pending_in_turn_inputs.is_empty()
        {
            return Ok(());
        }
        let Some(mut turn) = self
            .metadata
            .config
            .economy
            .active
            .clone()
            .filter(|t| t.turn_id == owner)
        else {
            return Ok(());
        };
        // A range read may contribute only the contiguous missing prefix of this exact generation.
        // 范围读取仅补齐这一代次缺少的连续前缀，防止把不完整的冷启动回合误判为完整。
        let prefix_complete = self.resident_start_seq <= turn.start.value()
            || (sealed_prefix.first().is_some_and(|e| e.seq == turn.start)
                && sealed_prefix
                    .last()
                    .is_some_and(|e| e.seq.value() + 1 == self.resident_start_seq)
                && sealed_prefix.len() as u64 == self.resident_start_seq - turn.start.value());
        let mut retained_reason = (!prefix_complete).then_some("incomplete_event_range");
        // Reuse the canonical outstanding-delivery test, including questions and notifications across turns.
        // 复用规范的未结投递判定，涵盖跨轮的问题与通知。
        let collaboration = self.collaboration_state();
        let missing_projection = collaboration
            .outbound_task_messages
            .iter()
            .any(|(id, task)| {
                task.message.source.turn_id == Some(owner)
                    && !collaboration.task_result_projections.contains_key(id)
            });
        let missing_child_return = collaboration.child_tasks.values().filter(|task| task.parent_turn_id == owner).any(|task| {
            task.non_success_projection.is_none() && !sealed_prefix.iter().chain(self.events()).any(|event| {
                event.turn_id == Some(owner) && matches!(&event.event, EventBody::SubagentReturn(result)
                    if result.child_root_action_id.as_str() == task.execution_id.as_str() && result.child_task_id == task.task_id)
            })
        });
        if collaboration.requires_edit_or_delete_settlement()
            || missing_projection
            || missing_child_return
        {
            retained_reason = Some("pending_collaboration");
        }
        // The published receipt must describe inputs actually visible to the latest successful business response.
        // 已发布回执仅覆盖最后一次成功业务响应实际看到的输入。
        let events = || {
            sealed_prefix
                .iter()
                .chain(self.events())
                .filter(|e| e.turn_id == Some(owner))
        };
        let Some(start) = events()
            .filter_map(|e| match &e.event {
                EventBody::ApiStarted(start) => Some(start),
                _ => None,
            })
            .next_back()
        else {
            return Ok(());
        };
        if start.api_call_id != candidate.api
            || !matches!(start.purpose, ApiPurposeKind::MainGeneration | ApiPurposeKind::RetryGeneration | ApiPurposeKind::Continue)
            || start.projection.segment_generation != self.segment_generation()
            || start.projection.through_seq < turn.start
            || !events().any(|e| matches!(&e.event, EventBody::ApiStats(stats)
                if stats.api_call_id == candidate.api && stats.status == crate::v1::event::ApiCallStatus::Completed))
            || events().any(|e| e.seq > start.projection.through_seq && matches!(&e.event, EventBody::UserInput(_) | EventBody::SubagentReturn(_)))
        {
            return Ok(());
        }
        let projection = start.projection.clone();
        let mut answer = None;
        let mut used_tools = false;
        let mut protected = self.metadata.mode.goal.origin_turn_id == Some(owner);
        let mut ask_batches = std::collections::BTreeMap::new();
        for event in self
            .events()
            .iter()
            .rev()
            .chain(sealed_prefix.iter().rev())
            .take_while(|e| e.seq >= turn.start)
            .filter(|e| e.turn_id == Some(owner))
        {
            // Signed or opaque provider items cannot be reconstructed from a human-readable handoff.
            // 签名或不透明的供应商项目无法从交接摘要重建。
            let source = match &event.event {
                EventBody::AssistantReasoning(t) => t.provider_source.as_ref(),
                EventBody::AssistantText(t) => t.provider_source.as_ref(),
                EventBody::ToolCall(t) => t.provider_source.as_ref(),
                _ => None,
            };
            if source
                .and_then(|source| source.replay.as_ref())
                .is_some_and(|replay| {
                    !replay.provider_context_items.is_empty()
                        || !replay.message_phases.is_empty()
                        || replay.content.iter().any(|item| {
                            !matches!(
                                item,
                                vulcan_ai_contract::ContentItem::Text { .. }
                                    | vulcan_ai_contract::ContentItem::ToolCall { .. }
                                    | vulcan_ai_contract::ContentItem::Reasoning {
                                        signature: None,
                                        ..
                                    }
                            )
                        })
                })
            {
                protected = true;
                retained_reason.get_or_insert("native_replay");
            }
            match &event.event {
                EventBody::AssistantText(t)
                    if t.api_call_id == candidate.api
                        && t.status == AssistantSemanticStatus::Completed
                        && !t.text.trim().is_empty() =>
                {
                    answer = Some(event.seq);
                }
                EventBody::UserInput(u) => protected |= !u.resource_ids.is_empty(),

                EventBody::ToolCall(tool) => {
                    used_tools = true;
                    let settled = if tool.tool_name == "spawn_agent"
                        && tool.status == ToolCallStatus::Completed
                    {
                        matches!(tool.external_refs.as_slice(), [child] if child.kind == crate::v1::event::ExternalResourceKind::Subagent
                            && collaboration.child_tasks.values().any(|task| task.parent_turn_id == owner && task.child.session_id == child.id))
                    } else {
                        tool_settled(tool)
                    };
                    if !matches!(
                        tool.status,
                        ToolCallStatus::Completed | ToolCallStatus::Failed
                    ) || !settled
                    {
                        protected = true;
                        retained_reason.get_or_insert("unverified_tool_completion");
                    }
                    if !tool.generated_resources.is_empty() {
                        protected = true;
                        retained_reason.get_or_insert("generated_resources");
                    }
                    let mixed = ask_batches
                        .entry(tool.api_call_id)
                        .or_insert((false, false));
                    if tool.tool_name == "ask" {
                        mixed.0 = true
                    } else {
                        mixed.1 = true
                    }
                    if mixed.0 && mixed.1 {
                        retained_reason.get_or_insert("mixed_ask_batch");
                    }
                }
                _ => {}
            }
        }
        let Some(answer) = answer else { return Ok(()) };
        if used_tools && candidate.summary.is_none() {
            retained_reason.get_or_insert("missing_summary");
        }
        if retained_reason.is_some() {
            turn.baseline = self
                .metadata
                .config
                .economy
                .visible_compaction(self.runtime_lock.current_compaction_id);
        }
        // Missing handoff text needs an intact turn while retained; only live or replay dependencies pin its age.
        // 漏写摘要的回合在窗口中保留原文；仅活动依赖或回放依赖阻止其按龄移出。
        protected |= retained_reason
            .is_some_and(|reason| !matches!(reason, "missing_summary" | "mixed_ask_batch"));
        let window = turn.window;
        let state = &mut self.metadata.config.economy;
        state.settled.push(SettledEconomyTurn {
            turn,
            answer,
            api: candidate.api,
            selection_revision: candidate.revision,
            projection,
            physical_tip: self.runtime_lock.current_compaction_id,
            summary: candidate
                .summary
                .clone()
                .filter(|_| used_tools && retained_reason.is_none()),
            retired_at: None,
            protected,
            retained_reason: retained_reason.map(str::to_owned),
        });
        let alive = state
            .settled
            .iter()
            .filter(|r| r.retired_at.is_none())
            .count();
        let remove = window.turns_to_remove(alive).map_err(|_| {
            DomainError::InvalidSelectionConfiguration {
                field: "economy.window",
            }
        })?;
        // Protect an entire batch if any member still anchors a resource or live obligation.
        // 一批中只要有成员仍是资源或待办锚点，就整体保留该批。
        let removable = state
            .settled
            .iter()
            .filter(|r| r.retired_at.is_none())
            .take(remove)
            .take_while(|r| !r.protected)
            .count();
        let batch = window.batch_turns as usize;
        for old in state
            .settled
            .iter_mut()
            .filter(|r| r.retired_at.is_none())
            .take(removable / batch * batch)
        {
            old.retired_at = Some(answer);
        }
        state.active = None;
        Ok(())
    }
}
