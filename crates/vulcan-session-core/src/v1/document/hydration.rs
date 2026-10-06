//! Strict reconstruction of derived state from the authoritative resident event stream.
//! 从权威常驻事件流严格重建派生状态。

use super::api::{
    validate_api_dispatch_history, validate_api_terminal_facts, validate_execution_control_event,
    validate_provider_request_budget_evidence,
};
use super::compaction::validate_provider_projection_proof_shape;
use super::input::{
    validate_client_action_history, validate_queued_input_resolution_history, validate_user_input,
};
use super::selection::{
    validate_active_request_config, validate_api_request_snapshot,
    validate_knowledge_library_selection_config, validate_pending_request_config,
    validate_provider_tool_selection_config,
};
use super::subagent::{validate_subagent_return_history, validate_turn_stat_buckets};
use super::tools::validate_tool_lifecycle_shape;
use super::{
    ActiveApiState, ActiveTurnState, REASONING_RANK, STARTED_RANK, STATS_RANK, SessionDocument,
    TOOL_RANK, TurnOrigin, ensure_api_request_field, group_event_api_id, group_rank,
    provider_source_of, validate_timestamp,
};
use crate::v1::collaboration::{
    CollaborationAfterState, CollaborationEventKind, CollaborationTaskMessageEffectKind,
};
use crate::v1::config::{ForkOrigin, GoalState};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ApiEndReason, ApiPurposeKind, DisplayNoticeKind, EventBody, SystemContextOperation, TurnOutcome,
};
use crate::v1::ids::{ApiCallId, IdAllocator, JS_SAFE_INTEGER_MAX, Seq, TurnId};
use crate::v1::projection::{validate_provider_fingerprint, validate_system_context_event};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Validates the immutable source boundary of one forked session.
/// 校验一个分叉会话的不可变来源边界。
fn validate_fork_origin(
    child_session_id: &str,
    fork_origin: &ForkOrigin,
) -> Result<(), DomainError> {
    if fork_origin.source_session_id.trim().is_empty()
        || fork_origin.source_session_id.trim() != fork_origin.source_session_id
    {
        return Err(DomainError::InvalidGenerationInput {
            field: "fork_origin.source_session_id",
        });
    }
    if fork_origin.source_session_id == child_session_id {
        return Err(DomainError::InvalidGenerationInput {
            field: "fork_origin.source_session_id_matches_child",
        });
    }
    if !(1..=JS_SAFE_INTEGER_MAX).contains(&fork_origin.source_segment_generation) {
        return Err(DomainError::InvalidGenerationInput {
            field: "fork_origin.source_segment_generation",
        });
    }
    Ok(())
}

// Keep all mutations on the single authoritative document instance.
// 所有修改仍作用于唯一权威文档实例。
impl SessionDocument {
    /// Rebuilds all derived state (indexes, active turn/API, allocator
    /// frontier) from the resident events, strictly validating the stream.
    /// 从常驻事件重建全部派生状态(索引、活动回合/API、分配器 frontier)，
    /// 并严格校验事件流。
    ///
    /// This is the loader's single validation entry: seq contiguity, API-group
    /// ordering, `provider_source` uniqueness/placement, and allocator frontier
    /// reconstruction. It never rewrites or repairs anything.
    /// 这是加载器的唯一校验入口：seq 连续性、API 组顺序、`provider_source`
    /// 唯一性与位置、分配器 frontier 重建。它绝不重写或修复任何内容。
    pub fn rebuild_derived_state(&mut self) -> Result<(), DomainError> {
        let mut tool_seq_by_id = HashMap::new();
        let mut max_seq = self.resident_start_seq.saturating_sub(1);
        let prefix_next_event_local_id = self
            .segment_catalog
            .iter()
            .take_while(|entry| entry.start_seq.value() < self.resident_start_seq)
            .map(|entry| entry.next_event_local_id)
            .last()
            .unwrap_or(1);
        if prefix_next_event_local_id == 0 {
            return Err(DomainError::invalid_id(
                "next_event_local_id",
                prefix_next_event_local_id,
            ));
        }
        let mut max_local_id = prefix_next_event_local_id - 1;
        let mut turn_ends: HashMap<u64, u64> = HashMap::new();

        // Non-event documents also own local ids. They are facts, not caches,
        // and therefore participate in allocator restoration.
        // 非事件文档同样持有本地 id；它们是事实而非缓存，因此必须参与分配器恢复。
        for compaction in &self.compactions {
            max_local_id = max_local_id
                .max(compaction.compaction_id.value())
                .max(compaction.api_call_id.value());
        }
        if let Some(compaction_id) = self.runtime_lock.current_compaction_id {
            max_local_id = max_local_id.max(compaction_id.value());
        }
        for pending in &self.metadata.input_state.pending_in_turn_inputs {
            max_local_id = max_local_id
                .max(pending.input_id.value())
                .max(pending.turn_id.value());
        }
        for queued in &self.metadata.input_state.queued_inputs {
            max_local_id = max_local_id.max(queued.input_id.value());
        }
        if let Some(identity) = &self.metadata.input_state.draft.restored_from_queue {
            max_local_id = max_local_id.max(identity.input_id.value());
        }

        for (index, event) in self.events.iter().enumerate() {
            let seq_value = event.seq.value();
            let expected = self.resident_start_seq + index as u64;
            if seq_value != expected {
                return Err(DomainError::SequenceViolation {
                    expected,
                    actual: seq_value,
                });
            }
            if !(crate::v1::event::EVENT_REVISION_INITIAL..=JS_SAFE_INTEGER_MAX)
                .contains(&event.event_revision)
            {
                return Err(DomainError::invalid_id(
                    "event_revision",
                    event.event_revision,
                ));
            }
            max_seq = max_seq.max(seq_value);
            max_local_id = max_local_id.max(event.max_referenced_local_id());
            if let EventBody::SystemContext(context) = &event.event {
                validate_system_context_event(context)?;
            }
            if let EventBody::ExecutionControl(control) = &event.event {
                validate_execution_control_event(control)?;
            }
            if let EventBody::TurnStats(stats) = &event.event {
                validate_turn_stat_buckets(&stats.buckets)?;
            }
            if let Some(turn_id) = event.turn_id
                && let EventBody::TurnBoundary(_) = &event.event
            {
                turn_ends.insert(turn_id.value(), seq_value);
            }
            if let EventBody::ToolCall(tool) = &event.event {
                validate_tool_lifecycle_shape(tool)?;
                if tool_seq_by_id
                    .insert(tool.tool_call_id, event.seq)
                    .is_some()
                {
                    return Err(DomainError::ReferenceViolation {
                        kind: "tool_call_id",
                        value: tool.tool_call_id.value(),
                    });
                }
            }
        }
        validate_subagent_return_history(&self.events)?;
        validate_queued_input_resolution_history(&self.metadata, &self.events)?;
        validate_client_action_history(&self.metadata, &self.events)?;
        self.validate_context_maintenance_history()?;
        validate_api_dispatch_history(&self.events)?;

        // Validate API-group ordering and provider_source placement.
        let mut group_ranks: BTreeMap<u64, Vec<(usize, usize)>> = BTreeMap::new();
        for (index, event) in self.events.iter().enumerate() {
            if let Some(api_id) = group_event_api_id(event) {
                group_ranks
                    .entry(api_id)
                    .or_default()
                    .push((index, group_rank(&event.event)));
            }
        }
        let mut last_open_api: Option<(u64, bool)> = None;
        for (api_id, mut entries) in group_ranks {
            entries.sort_by_key(|(index, _)| *index);
            let mut previous_rank: Option<usize> = None;
            let mut stats_rank = usize::MAX;
            let mut started_count = 0usize;
            let mut last_provider_index: Option<usize> = None;
            let mut provider_source_index: Option<usize> = None;
            for &(index, rank) in &entries {
                if let Some(before) = previous_rank
                    && rank < before
                {
                    return Err(DomainError::SequenceViolation {
                        expected: before as u64,
                        actual: rank as u64,
                    });
                }
                if rank == STARTED_RANK {
                    started_count += 1;
                } else if rank == STATS_RANK {
                    if stats_rank != usize::MAX {
                        return Err(DomainError::ReferenceViolation {
                            kind: "api_stats",
                            value: api_id,
                        });
                    }
                    stats_rank = rank;
                } else if (REASONING_RANK..=TOOL_RANK).contains(&rank) {
                    last_provider_index = Some(index);
                }
                if let Some(source) = provider_source_of(&self.events[index]) {
                    if rank == STATS_RANK || provider_source_index.replace(index).is_some() {
                        return Err(DomainError::ProviderSourceAlreadySet {
                            api_call_id: api_id,
                        });
                    }
                    // The checkpoint coverage must equal the hosting event's
                    // own seq (plan 5.5.6).
                    // 检查点覆盖必须等于承载事件自身的 seq(计划 5.5.6)。
                    if source.covered_until_seq.value() != self.events[index].seq.value() {
                        return Err(DomainError::SequenceViolation {
                            expected: self.events[index].seq.value(),
                            actual: source.covered_until_seq.value(),
                        });
                    }
                    validate_provider_fingerprint(&self.events, &source)?;
                    if let Some(replay) = &source.replay
                        && !replay.matches_route(
                            &source.provider,
                            source.connection_id.as_deref().unwrap_or_default(),
                            &replay.route_fingerprint,
                            source.profile.as_deref().unwrap_or_default(),
                            &source.model,
                        )
                    {
                        return Err(DomainError::ApiRequestMismatch {
                            field: "provider_source.replay.route",
                            expected: format!(
                                "{}/{}/{}",
                                source.provider,
                                source.profile.as_deref().unwrap_or_default(),
                                source.model
                            ),
                            actual: format!(
                                "{}/{}/{}",
                                replay.provider_id, replay.protocol_profile_id, replay.model_id
                            ),
                        });
                    }
                }
                previous_rank = Some(rank);
            }
            if let Some(source_index) = provider_source_index
                && last_provider_index != Some(source_index)
            {
                return Err(DomainError::ProviderSourcePlacement {
                    api_call_id: api_id,
                    source_seq: self.events[source_index].seq.value(),
                    expected_seq: last_provider_index.map(|index| self.events[index].seq.value()),
                    api_closed: stats_rank != usize::MAX,
                });
            }
            if started_count != 1 {
                return Err(DomainError::ReferenceViolation {
                    kind: "api_started",
                    value: api_id,
                });
            }
            // Every lookup below stays inside this group's own indexed events, never the whole stream.
            // 以下查找只在本组已索引的事件内进行，绝不扫描整个事件流。
            let group_events = || entries.iter().map(|&(index, _)| &self.events[index]);
            let started_event = group_events()
                .find(|event| {
                    matches!(
                        &event.event,
                        EventBody::ApiStarted(started)
                            if started.api_call_id.value() == api_id
                    )
                })
                .ok_or_else(|| DomainError::reference("api_started", api_id))?;
            let EventBody::ApiStarted(started) = &started_event.event else {
                unreachable!("api_started event selected above")
            };
            validate_api_request_snapshot(&started.request)?;
            validate_provider_projection_proof_shape(&started.projection)?;
            validate_provider_request_budget_evidence(&started.request_budget_evidence)?;
            if started.projection.through_seq.value() + 1 != started_event.seq.value() {
                return Err(DomainError::InvalidProviderProjection {
                    field: "through_seq",
                });
            }
            if started.projection.segment_generation > self.metadata.segment_generation {
                return Err(DomainError::InvalidProviderProjection {
                    field: "segment_generation",
                });
            }
            if let Some(stats) = group_events().find_map(|event| match &event.event {
                EventBody::ApiStats(stats) if stats.api_call_id.value() == api_id => Some(stats),
                _ => None,
            }) {
                validate_api_terminal_facts(
                    stats.status,
                    stats.end_reason,
                    stats.error.as_ref(),
                    stats.continuation_eligibility,
                    &stats.timing,
                )?;
                ensure_api_request_field("purpose", &started.purpose, &stats.purpose)?;
                ensure_api_request_field("provider", &started.request.provider, &stats.provider)?;
                ensure_api_request_field("profile", &started.request.profile, &stats.profile)?;
                ensure_api_request_field("model", &started.request.model, &stats.model)?;
                ensure_api_request_field(
                    "connection_id",
                    &started.request.connection_id,
                    &stats.connection_id,
                )?;
                ensure_api_request_field(
                    "thinking_level",
                    &started.request.thinking_level,
                    &stats.thinking_level,
                )?;
                ensure_api_request_field(
                    "temperature",
                    &started.request.temperature,
                    &stats.temperature,
                )?;
            }
            for source in group_events().filter_map(provider_source_of) {
                ensure_api_request_field(
                    "provider_source.provider",
                    &started.request.provider,
                    &source.provider,
                )?;
                ensure_api_request_field(
                    "provider_source.profile",
                    &started.request.profile,
                    &source.profile,
                )?;
                ensure_api_request_field(
                    "provider_source.model",
                    &started.request.model,
                    &source.model,
                )?;
                ensure_api_request_field(
                    "provider_source.connection_id",
                    &started.request.connection_id,
                    &source.connection_id,
                )?;
            }
            if stats_rank == usize::MAX {
                let has_source = group_events().any(|event| provider_source_of(event).is_some());
                last_open_api = Some((api_id, has_source));
            }
        }

        // Strict stream invariants (plan 12.21.1.5, audit P0#5): no group
        // spans turns, no group reopens after its stats, two open groups
        // never interleave, and every turn closure sequence is unique and
        // ordered (boundary strictly before the single stats, no same-turn
        // event after the stats).
        // 严格流不变量(计划 12.21.1.5、审计 P0#5)：组不跨回合、组在其统计后
        // 不得重开、两个开放组不得交错、每个回合的闭合序列唯一且有序(boundary
        // 严格先于唯一 stats，stats 之后无同回合事件)。
        let mut turn_boundaries: BTreeMap<u64, u64> = BTreeMap::new();
        let mut turn_stats_seen: BTreeMap<u64, u64> = BTreeMap::new();
        for event in &self.events {
            let Some(turn_id) = event.turn_id else {
                continue;
            };
            let turn_value = turn_id.value();
            match &event.event {
                EventBody::TurnBoundary(boundary) => {
                    // The host reason and aggregate outcome must describe the same immutable closure.
                    // 宿主原因与聚合结果必须描述同一次不可变闭合。
                    if boundary.reason.outcome() != boundary.outcome {
                        return Err(DomainError::TurnClosureMismatch {
                            turn_id: turn_value,
                        });
                    }
                    if turn_boundaries
                        .insert(turn_value, event.seq.value())
                        .is_some()
                    {
                        return Err(DomainError::TurnClosureMismatch {
                            turn_id: turn_value,
                        });
                    }
                    if turn_stats_seen.contains_key(&turn_value) {
                        return Err(DomainError::TurnClosureMismatch {
                            turn_id: turn_value,
                        });
                    }
                }
                EventBody::TurnStats(_) => {
                    // Stats without a prior boundary is a mis-ordered closure.
                    if !turn_boundaries.contains_key(&turn_value) {
                        return Err(DomainError::TurnClosureMismatch {
                            turn_id: turn_value,
                        });
                    }
                    if turn_stats_seen
                        .insert(turn_value, event.seq.value())
                        .is_some()
                    {
                        return Err(DomainError::TurnClosureMismatch {
                            turn_id: turn_value,
                        });
                    }
                }
                _ => {
                    if turn_stats_seen.contains_key(&turn_value) {
                        return Err(DomainError::TurnClosureMismatch {
                            turn_id: turn_value,
                        });
                    }
                }
            }
        }

        let mut group_turn: BTreeMap<u64, u64> = BTreeMap::new();
        let mut group_closed: BTreeSet<u64> = BTreeSet::new();
        let mut last_group: Option<u64> = None;
        for event in &self.events {
            let Some(api_id) = group_event_api_id(event) else {
                continue;
            };
            let turn_value = event
                .turn_id
                .ok_or_else(|| DomainError::reference("api_group_turn", api_id))?
                .value();
            match group_turn.get(&api_id) {
                Some(known) => {
                    if *known != turn_value {
                        return Err(DomainError::GroupSpansTurns {
                            api_call_id: api_id,
                        });
                    }
                }
                None => {
                    group_turn.insert(api_id, turn_value);
                }
            }
            if group_closed.contains(&api_id) {
                return Err(DomainError::ClosedGroupReopened {
                    api_call_id: api_id,
                });
            }
            if matches!(&event.event, EventBody::ApiStats(_)) {
                group_closed.insert(api_id);
            }
            // A new group may only open after the previous one is closed.
            // 新组只能在上一组闭合后打开。
            if let Some(previous) = last_group
                && previous != api_id
                && !group_closed.contains(&previous)
            {
                return Err(DomainError::GroupsInterleaved {
                    first: previous,
                    second: api_id,
                });
            }
            last_group = Some(api_id);
        }

        // Rebuild the active turn from turn starts and boundaries.
        let mut active_turn: Option<ActiveTurnState> = None;
        let mut turn_state: BTreeMap<u64, (u64, TurnOrigin)> = BTreeMap::new();
        for event in &self.events {
            match &event.event {
                EventBody::UserInput(_) => {
                    if let Some(turn_id) = event.turn_id {
                        turn_state
                            .entry(turn_id.value())
                            .or_insert((event.seq.value(), TurnOrigin::UserRoot));
                    }
                }
                EventBody::ContextMaintenance(record) if record.terminal.is_none() => {
                    turn_state.insert(
                        record.turn_id.value(),
                        (record.started_seq.value(), TurnOrigin::ContextMaintenance),
                    );
                }
                EventBody::SubagentReturn(_) if let Some(turn_id) = event.turn_id => {
                    turn_state
                        .entry(turn_id.value())
                        .or_insert((event.seq.value(), TurnOrigin::SubagentReturn));
                }
                EventBody::SystemContext(context)
                    if (context.context_key == "goal_continuation"
                        || (context.context_key == "goal_state"
                            && context.operation == SystemContextOperation::Upsert
                            && self.metadata.open_turn_start_seq == Some(event.seq)))
                        && let Some(turn_id) = event.turn_id =>
                {
                    turn_state
                        .entry(turn_id.value())
                        .or_insert((event.seq.value(), TurnOrigin::GoalContinuation));
                }
                EventBody::Collaboration(collaboration)
                    if let CollaborationAfterState::TaskMessageEffect { effect } =
                        &collaboration.after_state
                        && let CollaborationTaskMessageEffectKind::Claimed { turn_id, .. } =
                            &effect.outcome
                        && event.turn_id == Some(*turn_id) =>
                {
                    turn_state
                        .entry(turn_id.value())
                        .or_insert((event.seq.value(), TurnOrigin::AgentTask));
                }
                EventBody::Collaboration(collaboration)
                    if collaboration.event_kind
                        == CollaborationEventKind::QuestionExecutionStarted
                        && let CollaborationAfterState::QuestionExecution { execution } =
                            &collaboration.after_state
                        && let Some(turn_id) = execution.execution_turn_id
                        && event.turn_id == Some(turn_id) =>
                {
                    turn_state
                        .entry(turn_id.value())
                        .or_insert((event.seq.value(), TurnOrigin::QuestionExecution));
                }
                _ => {}
            }
        }
        // The metadata open-turn pointer preserves the auxiliary turn start when compaction has
        // removed its HostOnly start event from the resident tail.
        // metadata 的开放回合指针在压缩移除 HostOnly 起始事件后仍保留辅助回合起点。
        if let Some(start_seq) = self.metadata.open_turn_start_seq {
            if let Some(record) = &self.metadata.context_maintenance
                && record.terminal.is_none()
            {
                if record.started_seq != start_seq {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "context_maintenance.open_anchor",
                    });
                }
                turn_state.insert(
                    record.turn_id.value(),
                    (start_seq.value(), TurnOrigin::ContextMaintenance),
                );
            }
            // Recover agent-task origin even when compaction removed its HostOnly claim anchor.
            // 即使压缩移除了 HostOnly 认领锚点，也恢复代理任务来源。
            for effect in self
                .metadata
                .collaboration
                .state
                .task_message_effects
                .values()
            {
                if effect.event_seq == start_seq
                    && let CollaborationTaskMessageEffectKind::Claimed { turn_id, .. } =
                        &effect.outcome
                {
                    turn_state.insert(turn_id.value(), (start_seq.value(), TurnOrigin::AgentTask));
                }
            }
            for execution in self
                .metadata
                .collaboration
                .state
                .question_executions
                .values()
                .filter(|execution| {
                    execution.state
                        == crate::v1::collaboration::CollaborationQuestionExecutionState::Running
                })
            {
                if let Some(turn_id) = execution.execution_turn_id {
                    turn_state
                        .entry(turn_id.value())
                        .or_insert((start_seq.value(), TurnOrigin::QuestionExecution));
                }
            }
        }
        // Validate closure purpose against every resident opening fact before rebuilding ownership.
        // 重建所有权前，按每个常驻起始事实校验闭合用途。
        for event in &self.events {
            if let EventBody::TurnBoundary(boundary) = &event.event
                && let Some(turn_id) = event.turn_id
                && let Some((_, origin)) = turn_state.get(&turn_id.value())
                && boundary.purpose
                    != match origin {
                        TurnOrigin::QuestionExecution => {
                            crate::v1::event::TurnBoundaryPurpose::QuestionExecution
                        }
                        TurnOrigin::ContextMaintenance => {
                            crate::v1::event::TurnBoundaryPurpose::ContextMaintenance
                        }
                        _ => crate::v1::event::TurnBoundaryPurpose::Business,
                    }
            {
                return Err(DomainError::TurnClosureMismatch {
                    turn_id: turn_id.value(),
                });
            }
        }
        for (turn_id, (start_seq, origin)) in turn_state {
            let closed = turn_ends.contains_key(&turn_id);
            if !closed {
                if active_turn.is_some() {
                    return Err(DomainError::MultipleOpenTurns);
                }
                active_turn = Some(ActiveTurnState {
                    turn_id: TurnId::try_new(turn_id)?,
                    origin,
                    start_seq: Seq::try_new(start_seq)?,
                    closed: false,
                });
            }
        }

        // Rebuild the active API state for the single open group, if any.
        let active_api = match last_open_api {
            Some((api_id, provider_source_placed)) => {
                let api_call_id = ApiCallId::try_new(api_id)?;
                let started = self
                    .events
                    .iter()
                    .find(|event| {
                        matches!(
                            &event.event,
                            EventBody::ApiStarted(value)
                                if value.api_call_id.value() == api_id
                        )
                    })
                    .ok_or_else(|| DomainError::reference("api_started", api_id))?;
                let turn_id = started
                    .turn_id
                    .ok_or_else(|| DomainError::reference("api_group", api_id))?;
                let EventBody::ApiStarted(started_body) = &started.event else {
                    unreachable!("api_started event selected above")
                };
                let reasoning_seq = self.events.iter().find_map(|event| match &event.event {
                    EventBody::AssistantReasoning(reasoning)
                        if reasoning.api_call_id.value() == api_id =>
                    {
                        Some(event.seq)
                    }
                    _ => None,
                });
                let text_seq = self.events.iter().find_map(|event| match &event.event {
                    EventBody::AssistantText(text) if text.api_call_id.value() == api_id => {
                        Some(event.seq)
                    }
                    _ => None,
                });
                let tool_seqs = self
                    .events
                    .iter()
                    .filter_map(|event| match &event.event {
                        EventBody::ToolCall(tool) if tool.api_call_id.value() == api_id => {
                            Some(event.seq)
                        }
                        _ => None,
                    })
                    .collect();
                Some(ActiveApiState {
                    api_call_id,
                    turn_id,
                    purpose: started_body.purpose,
                    started_seq: started.seq,
                    request: started_body.request.clone(),
                    projection: started_body.projection.clone(),
                    reasoning_seq,
                    text_seq,
                    tool_seqs,
                    stats_seq: None,
                    provider_source_placed,
                })
            }
            None => None,
        };
        self.tool_seq_by_id = tool_seq_by_id;
        self.active_turn = active_turn;
        self.active_api = active_api;
        self.id_allocator = IdAllocator::from_frontier(max_local_id + 1, max_seq + 1);
        Ok(())
    }

    /// Validates metadata caches against a fully resident authoritative stream.
    /// 依据完整常驻的权威事件流校验 metadata 缓存。
    pub(super) fn validate_loaded_metadata(&self) -> Result<(), DomainError> {
        validate_active_request_config(&self.metadata.config.active_request)?;
        if let Some(fork_origin) = &self.metadata.fork_origin {
            validate_fork_origin(&self.metadata.session_id, fork_origin)?;
        }
        if let Some(pending) = &self.metadata.config.pending_request {
            validate_timestamp("pending_request.set_at_ms", pending.set_at_ms)?;
            validate_pending_request_config(pending)?;
        }
        validate_knowledge_library_selection_config(
            &self.metadata.config.selection.knowledge_libraries,
        )?;
        validate_provider_tool_selection_config(&self.metadata.config.selection.provider_tools)?;
        if let Some(identity) = &self.metadata.input_state.draft.restored_from_queue {
            validate_user_input(
                self.metadata.input_state.draft.text.clone(),
                &self.metadata.input_state.draft.resource_ids,
            )?;
            let input_id_reused = self
                .metadata
                .input_state
                .pending_in_turn_inputs
                .iter()
                .any(|pending| pending.input_id == identity.input_id)
                || self
                    .metadata
                    .input_state
                    .queued_inputs
                    .iter()
                    .any(|queued| queued.input_id == identity.input_id)
                || self.events.iter().any(|event| {
                    matches!(
                        &event.event,
                        EventBody::UserInput(input) if input.input_id == identity.input_id
                    )
                });
            if input_id_reused {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "draft_input_id_reused",
                });
            }
        }
        if self.resident_start_seq != 1 {
            return Ok(());
        }
        let derived_open_seq = self.active_turn.as_ref().map(|turn| turn.start_seq);
        if self.metadata.open_turn_start_seq != derived_open_seq {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "metadata",
                field: "open_turn_start_seq",
            });
        }
        let goal = &self.metadata.mode.goal;
        if goal.origin_turn_id.is_some() != goal.origin_seq.is_some() {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "goal",
                field: "origin",
            });
        }
        if let (Some(origin_turn_id), Some(origin_seq)) = (goal.origin_turn_id, goal.origin_seq) {
            let origin_matches = self
                .events
                .iter()
                .any(|event| event.seq == origin_seq && event.turn_id == Some(origin_turn_id));
            if !origin_matches {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "goal",
                    field: "origin",
                });
            }
        }
        if goal.enabled
            && goal.state == GoalState::Active
            && goal.origin_seq.is_some()
            && self.active_turn.is_none()
        {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "goal",
                field: "active_turn",
            });
        }
        if let Some(cache) = &goal.budget_cache {
            let covers_turn_stats = self.events.iter().any(|event| {
                event.seq == cache.covered_until_seq
                    && matches!(&event.event, EventBody::TurnStats(_))
            });
            if !covers_turn_stats {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "goal",
                    field: "budget_cache.covered_until_seq",
                });
            }
        }
        Ok(())
    }

    /// Projects persisted streaming semantics and every non-terminal tool to
    /// `interrupted` after a crash reload (plan 6.5.6), then statically closes
    /// the crashed physical API (plan 12.21.1.5).
    /// 崩溃重载后把已持久化的流式语义与每个非终态工具投影为 `interrupted`
    /// (计划 6.5.6)，随后统计闭合崩溃的物理 API(计划 12.21.1.5)。
    ///
    /// This is a crash-terminal projection, not a repair: statuses are mutated
    /// in place on their existing seq, and one terminal `ApiStats` event closes
    /// any API group that still lacks statistics, so the API no longer occupies
    /// `active_api` and the caller can `continue_turn` then start a fresh
    /// physical API. The interrupted group preserves its immutable request
    /// identity while usage stays all-unknown: a crash snapshot never fabricates
    /// official numbers. The tail is marked dirty; the caller must fix the
    /// transition through one normal explicit flush. Idempotent: a second call
    /// projects nothing.
    /// 这是崩溃终态投影，不是修复：状态在其既有 seq 上原位变更，且一条终态
    /// `ApiStats` 事件闭合任何仍缺统计的 API 组，使该 API 不再占用
    /// `active_api`，调用方可以先 `continue_turn` 再启动全新物理 API。中断组保留
    /// 其不可变请求身份，用量保持全未知：崩溃快照绝不伪造官方数字。尾段被标记
    /// dirty；调用方必须通过一次正常显式 flush 固化该转换。幂等：第二次调用不再
    /// 投影任何内容。
    pub fn project_persisted_interruptions(&mut self) -> Result<u32, DomainError> {
        // A restarted active goal requires explicit user resumption, even between physical requests.
        // 重启后的活动目标必须由用户显式恢复，即使退出发生在两个物理请求之间。
        let pause_restarted_goal =
            self.metadata.mode.goal.enabled && self.metadata.mode.goal.state == GoalState::Active;
        if !pause_restarted_goal
            && let Some(active_api) = self.active_api.as_ref()
            && active_api.purpose == ApiPurposeKind::Continue
            && !self.api_transport_was_observed(active_api.api_call_id)
        {
            // A Continue that was durably prepared but never observed by
            // transport remains dispatchable under the same client action
            // and ApiCallId after restart. Projecting a terminal here would
            // make the exactly-once binding impossible to resume.
            // 已持久化准备但尚未被 transport 观察到的 Continue，在重启后仍可
            // 使用同一 client action 与 ApiCallId 派发。若在此投影终态，将使
            // exactly-once 绑定无法恢复。
            return Ok(0);
        }
        // Recover only through the last persisted observation; wall time at reload includes offline time.
        // 仅恢复至最后持久化观察点；重新加载时的墙钟包含离线时间。
        let now_ms = self
            .events
            .iter()
            .map(|event| event.updated_at_ms)
            .max()
            .unwrap_or(self.metadata.updated_at_ms)
            .max(self.metadata.updated_at_ms);
        // Maintenance cannot retain a business Continue path after its process owner disappears.
        // 维护的进程所有者消失后，不能保留业务继续路径。
        // Count its atomic diagnostic/API/turn settlement as one recovered auxiliary execution.
        // 将其原子诊断、API 和回合结算计为一个已恢复辅助执行。
        let mut projected = if self
            .active_turn
            .as_ref()
            .is_some_and(|turn| !turn.closed && turn.origin == TurnOrigin::ContextMaintenance)
        {
            u32::from(self.fail_execution_boundary_and_close_turn(
                "context_maintenance_process_interrupted",
                Some("The process exited before context maintenance completed."),
                now_ms,
            )?)
        } else if self.active_api.is_some() {
            self.interrupt_active_api_state(now_ms, ApiEndReason::ProcessInterrupted, false)?
                .0
        } else {
            0
        };
        // Legacy pre-API failure notes left an open Turn even when the goal was already blocked.
        // 旧 API 前失败提示曾遗留开放回合，即便目标已经受阻。
        let orphaned_pre_api_failure = self.active_turn.as_ref().filter(|turn| !turn.closed)
            .is_some_and(|turn| {
                self.active_api.is_none()
                    && !self.events.iter().any(|event| event.turn_id == Some(turn.turn_id)
                        && matches!(event.event, EventBody::ApiStarted(_)))
                    && self.events.iter().any(|event| event.turn_id == Some(turn.turn_id)
                        && matches!(&event.event, EventBody::DisplayNote(note)
                            if note.notice_kind == DisplayNoticeKind::SystemError && note.api_call_id.is_none()
                                && note.reason_code == "stream_finished_without_v1_api_terminal"))
            });
        if orphaned_pre_api_failure && !pause_restarted_goal {
            self.fail_turn_without_active_api(
                "stream_finished_without_v1_api_terminal",
                None,
                now_ms,
            )?;
            projected += 2;
        }
        if pause_restarted_goal {
            // Pausing automatic Goal work cannot close a distinct user, agent-task or reply owner.
            // 暂停自动 Goal 工作不能关闭独立的用户、代理任务或回流所有者。
            if let Some(turn) = self
                .active_turn
                .as_ref()
                .filter(|turn| !turn.closed && turn.origin == TurnOrigin::GoalContinuation)
            {
                let started_at_ms = self
                    .events
                    .iter()
                    .find(|event| event.seq == turn.start_seq)
                    .ok_or(DomainError::ReferenceViolation {
                        kind: "turn_start_event",
                        value: turn.start_seq.value(),
                    })?
                    .created_at_ms;
                self.close_turn(
                    TurnOutcome::Stopped,
                    now_ms.saturating_sub(started_at_ms).max(0),
                    now_ms,
                )?;
                projected += 2;
            }
            self.pause_goal(
                "The process exited. Goal execution is paused until the user resumes it.",
                now_ms,
            )?;
            projected += 2;
        }
        Ok(projected)
    }
}
