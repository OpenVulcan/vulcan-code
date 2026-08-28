//! V1 Provider request projection (build-time fixture).
//! V1 Provider 请求投影(构建期 fixture)。
//!
//! `project_provider_items` is the single projection function that turns the
//! seq-ordered main event stream into the Provider-visible message sequence:
//! `ui_only`/`host_only` payloads are filtered after their structural boundary
//! effects are applied, provider-visible events keep their original seq order,
//! and each complete API group expands into one assistant message (reasoning +
//! text + tool calls) followed by its tool results, all derived from the same
//! events. The replay fingerprint hashes only the immutable Provider-visible
//! projection and never the late host fields.
//! `project_provider_items` 是把 seq 有序主事件流转换为 Provider 可见消息
//! 序列的唯一投影函数：先应用 `ui_only`/`host_only` 事件的结构边界作用再过滤
//! 其载荷、保留 Provider 可见事件的原始 seq 顺序、把每个完整 API 组展开为一条
//! assistant 消息(推理 + 正文 + 工具调用)以及紧随其后的工具结果，全部来自同一
//! 组事件。重放指纹只哈希不可变 Provider 可见投影，绝不包含宿主后写字段。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::v1::errors::DomainError;
use crate::v1::event::*;
use crate::v1::ids::{ApiCallId, ResourceId, Seq, ToolCallId, TurnId};

/// System-layer role hint used by the projection.
/// 投影使用的系统层角色提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectedSystemRole {
    /// Map to the developer layer (preferred).
    /// 映射到 developer 层(首选)。
    Developer,
    /// Map to a system layer.
    /// 映射到 system 层。
    System,
    /// Wrap as a hidden user prompt when the protocol lacks system layers.
    /// 协议缺少系统层时套壳为隐藏用户提示。
    HiddenUserShell,
}

/// One provider-visible projected item.
/// 一条 Provider 可见的投影项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectedItem {
    /// Dynamic system semantics still active at projection time.
    /// 投影时刻仍然生效的动态系统语义。
    SystemText {
        /// Role hint for the protocol adapter.
        /// 供协议适配器使用的角色提示。
        role: ProjectedSystemRole,
        /// Stable semantic key.
        /// 稳定语义键。
        context_key: String,
        /// Content body.
        /// 内容正文。
        text: String,
    },
    /// User input with ordered resource references (root, supplementary, or queued).
    /// 带有有序资源引用的用户输入（根、补充或排队）。
    UserInput {
        /// Input text.
        /// 输入正文。
        text: String,
        /// Ordered project resource identities attached to the input.
        /// 附着于输入的有序项目资源身份。
        resource_ids: Vec<ResourceId>,
    },
    /// One complete assistant message of an API group.
    /// 一个 API 组的完整 assistant 消息。
    AssistantText {
        /// Assistant body.
        /// assistant 正文。
        text: String,
        /// Optional displayable reasoning body.
        /// 可选可显示推理正文。
        reasoning: Option<String>,
        /// Tool calls declared by the Provider in Provider order.
        /// 按 Provider 顺序声明的工具调用。
        tool_calls: Vec<ProjectedToolCall>,
        /// Exact Provider-native replay source attached to the group's last sourced event.
        /// 附着在该组最后一条 Provider 来源事件上的精确 Provider 原生重放来源。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider_source: Option<Box<ProviderSource>>,
    },
    /// One tool result produced by the same API group.
    /// 由同一 API 组产生的工具结果。
    ToolResult {
        /// Session-local tool identity used as the authoritative pair key.
        /// 用作权威配对键的会话本地工具身份。
        tool_call_id: ToolCallId,
        /// Provider-native tool call id when available.
        /// 可用的 Provider 原生工具调用 id。
        provider_tool_call_id: Option<String>,
        /// Tool name.
        /// 工具名。
        tool_name: String,
        /// Exact durable terminal status of the tool call.
        /// 工具调用的精确可靠终态。
        status: ToolCallStatus,
        /// Tool output payload.
        /// 工具输出载荷。
        output: ToolPayload,
    },
    /// Subagent return content handed to the parent model.
    /// 交给父模型的子代理回流内容。
    SubagentReturnText {
        /// Returned content body.
        /// 回流内容正文。
        text: String,
    },
}

/// One Provider projection item together with its exact durable source range.
/// 一条 Provider 投影项及其精确可靠来源范围。
#[derive(Debug, Clone, PartialEq)]
pub struct SourcedProjectedItem {
    /// Inclusive first main-event seq that produced the projected item.
    /// 产出该投影项的主事件包含式起始 seq。
    pub source_from_seq: Seq,
    /// Inclusive last main-event seq that produced the projected item.
    /// 产出该投影项的主事件包含式结束 seq。
    pub source_to_seq: Seq,
    /// Provider-visible semantic projected from the source range.
    /// 从来源范围投影出的 Provider 可见语义。
    pub item: ProjectedItem,
}

/// One provider-visible tool call.
/// 一条 Provider 可见的工具调用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectedToolCall {
    /// Session-local tool identity used as the authoritative pair key.
    /// 用作权威配对键的会话本地工具身份。
    pub tool_call_id: ToolCallId,
    /// Provider-native tool call id when available.
    /// 可用的 Provider 原生工具调用 id。
    pub provider_tool_call_id: Option<String>,
    /// Tool name.
    /// 工具名。
    pub tool_name: String,
    /// Fully assembled tool arguments.
    /// 完整组装的工具参数。
    pub input: ToolPayload,
}

/// One still-active dynamic system state restored before the compaction
/// boundary.
/// 压缩边界前恢复的一条仍然生效的动态系统状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSystemState {
    /// Seq of the Upsert event that created the current state generation.
    /// 创建当前状态代际的 Upsert 事件 seq。
    pub source_seq: Seq,
    /// Key of the system semantic.
    /// 系统语义的键。
    pub context_key: String,
    /// Role hint for the protocol adapter.
    /// 供协议适配器使用的角色提示。
    pub role: ProjectedSystemRole,
    /// Active content body.
    /// 生效的内容正文。
    pub content: String,
    /// Scope frozen by the source event.
    /// 来源事件冻结的作用域。
    pub scope: SystemContextScope,
    /// Remaining logical-turn lifetime; `None` means persistent.
    /// 剩余逻辑回合寿命；`None` 表示永久。
    pub remaining_turns: Option<u64>,
    /// Last logical Turn that consumed this state generation.
    /// 最近一次消费当前状态代际的逻辑 Turn。
    pub last_consumed_turn_id: Option<TurnId>,
}

/// Rebuilds the exact active system state at one inclusive seq boundary.
/// 在一个包含式 seq 边界重建精确的活动系统状态。
///
/// # Parameters
/// 参数
/// `events` is the contiguous event prefix available to the projection owner;
/// `through_seq` is the inclusive authority boundary.
/// `events` 是投影 owner 可用的连续事件前缀；`through_seq` 是包含式权威边界。
///
/// # Returns
/// 返回值
/// Returns active states ordered by their latest Upsert seq. TTL is consumed
/// exactly once by each persisted logical `TurnBoundary`.
/// 返回按最新 Upsert seq 排序的活动状态；TTL 仅由每个持久化逻辑
/// `TurnBoundary` 精确消费一次。
pub fn derive_active_system_state(
    events: &[ConversationEvent],
    through_seq: Seq,
) -> Result<Vec<ActiveSystemState>, DomainError> {
    let mut states = BTreeMap::new();
    advance_active_system_state(
        &mut states,
        events
            .iter()
            .filter(|event| event.seq().value() <= through_seq.value()),
    )?;
    let mut result: Vec<ActiveSystemState> = states.into_values().collect();
    result.sort_by_key(|state| state.source_seq.value());
    Ok(result)
}

/// Validates one durable system-context event before mutation or projection.
/// 在修改或投影前校验一条持久系统上下文事件。
pub(crate) fn validate_system_context_event(
    context: &SystemContextEvent,
) -> Result<(), DomainError> {
    if context.context_key.trim().is_empty() || context.context_key.trim() != context.context_key {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "system_context",
            field: "context_key",
        });
    }
    if context.content.trim().is_empty() || context.content.trim() != context.content {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "system_context",
            field: "content",
        });
    }
    match (context.operation, context.scope, context.ttl_turns) {
        (SystemContextOperation::Remove, _, None) => Ok(()),
        (SystemContextOperation::Remove, _, Some(_)) => Err(DomainError::ImmutableFieldMutation {
            entity: "system_context",
            field: "remove_ttl_turns",
        }),
        (SystemContextOperation::Upsert, SystemContextScope::Turn, Some(1)) => Ok(()),
        (SystemContextOperation::Upsert, SystemContextScope::Turn, _) => {
            Err(DomainError::ImmutableFieldMutation {
                entity: "system_context",
                field: "turn_scope_ttl_turns",
            })
        }
        (SystemContextOperation::Upsert, _, Some(0)) => Err(DomainError::ImmutableFieldMutation {
            entity: "system_context",
            field: "ttl_turns",
        }),
        (SystemContextOperation::Upsert, _, _) => Ok(()),
    }
}

/// Advances active system state through one seq-ordered event iterator.
/// 通过一个按 seq 排序的事件迭代器推进活动系统状态。
fn advance_active_system_state<'a>(
    states: &mut BTreeMap<String, ActiveSystemState>,
    events: impl Iterator<Item = &'a ConversationEvent>,
) -> Result<(), DomainError> {
    for event in events {
        match &event.event {
            EventBody::SystemContext(context) => {
                validate_system_context_event(context)?;
                match context.operation {
                    SystemContextOperation::Upsert => {
                        states.insert(
                            context.context_key.clone(),
                            ActiveSystemState {
                                source_seq: event.seq(),
                                context_key: context.context_key.clone(),
                                role: role_hint(context.provider_role_hint),
                                content: context.content.clone(),
                                scope: context.scope,
                                remaining_turns: context.ttl_turns,
                                last_consumed_turn_id: None,
                            },
                        );
                    }
                    SystemContextOperation::Remove => {
                        states.remove(&context.context_key);
                    }
                }
            }
            EventBody::TurnBoundary(_) => {
                let turn_id = event.turn_id.ok_or(DomainError::ReferenceViolation {
                    kind: "turn_boundary.turn_id",
                    value: event.seq().value(),
                })?;
                for state in states.values_mut() {
                    let Some(remaining_turns) = state.remaining_turns else {
                        continue;
                    };
                    if state.last_consumed_turn_id == Some(turn_id) {
                        continue;
                    }
                    state.last_consumed_turn_id = Some(turn_id);
                    state.remaining_turns = Some(remaining_turns.saturating_sub(1));
                }
                states.retain(|_, state| state.remaining_turns != Some(0));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Pending accumulation of one in-flight API group.
/// 一个在途 API 组的挂起累积。
#[derive(Debug, Clone, Default)]
struct PendingGroup {
    /// Inclusive first source seq in this API group.
    /// 当前 API 组的包含式首个来源 seq。
    source_from_seq: Option<Seq>,
    /// Inclusive last source seq in this API group.
    /// 当前 API 组的包含式最后来源 seq。
    source_to_seq: Option<Seq>,
    /// Reasoning body accumulated in order.
    /// 按顺序累积的推理正文。
    reasoning: Option<String>,
    /// Assistant text body accumulated in order.
    /// 按顺序累积的 assistant 正文。
    text: String,
    /// Tool calls in Provider order.
    /// 按 Provider 顺序的工具调用。
    tool_calls: Vec<ProjectedToolCall>,
    /// Tool-result items following the assistant message.
    /// 紧随 assistant 消息的工具结果项。
    results: Vec<SourcedProjectedItem>,
    /// Exact Provider-native replay source selected by the owning document.
    /// 由所属文档选定的精确 Provider 原生重放来源。
    provider_source: Option<ProviderSource>,
}

/// Projects the main event stream into the Provider-visible sequence.
/// 把主事件流投影为 Provider 可见序列。
///
/// `retained_from_seq` selects the compaction boundary: events at or after it
/// are included in order; only the still-active backfill system states are
/// restored from before the boundary. Provider-invisible payloads are filtered
/// without discarding structural API-group boundaries such as `ApiStats`.
/// `retained_from_seq` 选择压缩边界：该 seq 及之后的事件按顺序包含；边界前
/// 只恢复仍然生效的回追系统状态。Provider 不可见载荷被过滤，但不会丢弃
/// `ApiStats` 等 API 组结构边界。
pub fn project_provider_items(
    events: &[ConversationEvent],
    retained_from_seq: Option<Seq>,
    backfill_system: &[ActiveSystemState],
) -> Result<Vec<ProjectedItem>, DomainError> {
    Ok(
        project_provider_items_with_sources(events, retained_from_seq, backfill_system)?
            .into_iter()
            .map(|projected| projected.item)
            .collect(),
    )
}

/// Projects the Provider-visible sequence together with exact durable source ranges.
/// 投影 Provider 可见序列及其精确可靠来源范围。
///
/// # Parameters
/// 参数
/// `events`, `retained_from_seq`, and `backfill_system` have the same authority semantics as
/// `project_provider_items`; no caller-provided identity or ordering is inferred.
/// `events`、`retained_from_seq` 与 `backfill_system` 和 `project_provider_items` 具有相同权威语义；
/// 不推断任何调用方提供的身份或顺序。
///
/// # Returns
/// 返回值
/// Returns each projected item with the inclusive source seq range that produced it.
/// 返回每条投影项及产出它的包含式来源 seq 范围。
pub fn project_provider_items_with_sources(
    events: &[ConversationEvent],
    retained_from_seq: Option<Seq>,
    backfill_system: &[ActiveSystemState],
) -> Result<Vec<SourcedProjectedItem>, DomainError> {
    let boundary = retained_from_seq.map(Seq::value).unwrap_or(0);
    let mut active_system = BTreeMap::new();
    for state in backfill_system {
        if active_system
            .insert(state.context_key.clone(), state.clone())
            .is_some()
        {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "active_system_state",
                field: "duplicate_context_key",
            });
        }
    }
    advance_active_system_state(
        &mut active_system,
        events
            .iter()
            .filter(|event| event.seq().value() >= boundary),
    )?;

    let mut prefix_states: Vec<ActiveSystemState> = backfill_system
        .iter()
        .filter(|state| state.remaining_turns.is_none())
        .cloned()
        .chain(
            active_system
                .into_values()
                .filter(|state| state.remaining_turns.is_some()),
        )
        .collect();
    prefix_states.sort_by_key(|state| state.source_seq.value());
    let mut items: Vec<SourcedProjectedItem> = prefix_states
        .iter()
        .map(|state| SourcedProjectedItem {
            source_from_seq: state.source_seq,
            source_to_seq: state.source_seq,
            item: ProjectedItem::SystemText {
                role: state.role,
                context_key: state.context_key.clone(),
                text: state.content.clone(),
            },
        })
        .collect();
    let mut pending: Vec<(ApiCallId, PendingGroup)> = Vec::new();

    for event in events {
        let seq_value = event.seq().value();
        if seq_value < boundary {
            // Events before the compaction boundary are filtered entirely;
            // still-active system states are restored only through the
            // caller-provided `backfill_system` (plan 7.1.3).
            // 压缩边界前的事件被整体过滤；仍然生效的系统状态只通过调用方提供
            // 的 `backfill_system` 恢复(计划 7.1.3)。
            continue;
        }
        // ApiStats is UiOnly payload but an authoritative API-group boundary.
        // Flush it before visibility filtering so a later Provider-visible
        // system/user/subagent fact can never overtake the completed group.
        // ApiStats 的载荷是 UiOnly，但它是权威 API 组边界。必须在 visibility
        // 过滤前 flush，确保后续 Provider 可见的 system/user/subagent 事实绝不
        // 越过已完成组。
        if let EventBody::ApiStats(stats) = &event.event {
            flush_group(&mut items, &mut pending, stats.api_call_id)?;
            continue;
        }
        if !event.is_provider_visible() {
            continue;
        }
        match &event.event {
            EventBody::SystemContext(context) => {
                flush_pending(&mut items, &mut pending)?;
                validate_system_context_event(context)?;
                if context.operation == SystemContextOperation::Upsert
                    && context.ttl_turns.is_some()
                {
                    // TTL-bound semantics are rendered only through the active
                    // prefix calculated for this physical request. Keeping the
                    // durable Upsert in seq still makes every historical request
                    // reproducible without leaking an expired value forward.
                    // TTL 约束语义仅通过本次物理请求计算出的活动前缀渲染。持久
                    // Upsert 仍保留原始 seq，因此既可重建历史请求，也不会把过期值
                    // 泄漏到后续请求。
                    continue;
                }
                // Both entry and explicit exit semantics are durable Provider facts. Remove is
                // not an instruction to erase historical messages; it is the ordered semantic
                // that tells the model this context is no longer active.
                // 进入与显式退出语义都是持久 Provider 事实。Remove 不是删除历史消息的
                // 指令，而是按序告知模型该上下文已不再生效的语义。
                items.push(SourcedProjectedItem {
                    source_from_seq: event.seq,
                    source_to_seq: event.seq,
                    item: ProjectedItem::SystemText {
                        role: role_hint(context.provider_role_hint),
                        context_key: context.context_key.clone(),
                        text: context.content.clone(),
                    },
                });
            }
            EventBody::UserInput(input) => {
                flush_pending(&mut items, &mut pending)?;
                items.push(SourcedProjectedItem {
                    source_from_seq: event.seq,
                    source_to_seq: event.seq,
                    item: ProjectedItem::UserInput {
                        text: input.text.clone(),
                        resource_ids: input.resource_ids.clone(),
                    },
                });
            }
            EventBody::AssistantReasoning(reasoning) => {
                let group = pending_group(&mut pending, reasoning.api_call_id, event.seq);
                group.reasoning = Some(match group.reasoning.take() {
                    Some(current) => current + &reasoning.text,
                    None => reasoning.text.clone(),
                });
                if let Some(provider_source) = &reasoning.provider_source {
                    group.provider_source = Some(provider_source.clone());
                }
            }
            EventBody::AssistantText(text) => {
                let group = pending_group(&mut pending, text.api_call_id, event.seq);
                group.text.push_str(&text.text);
                if let Some(provider_source) = &text.provider_source {
                    group.provider_source = Some(provider_source.clone());
                }
            }
            EventBody::ToolCall(tool) => {
                let group = pending_group(&mut pending, tool.api_call_id, event.seq);
                group.tool_calls.push(ProjectedToolCall {
                    tool_call_id: tool.tool_call_id,
                    provider_tool_call_id: tool.provider_tool_call_id.clone(),
                    tool_name: tool.tool_name.clone(),
                    input: tool.input.clone(),
                });
                if let Some(provider_source) = &tool.provider_source {
                    group.provider_source = Some(provider_source.clone());
                }
                if matches!(
                    tool.status,
                    ToolCallStatus::Completed
                        | ToolCallStatus::Failed
                        | ToolCallStatus::Cancelled
                        | ToolCallStatus::Stopped
                        | ToolCallStatus::Interrupted
                ) {
                    let output =
                        tool.output
                            .clone()
                            .ok_or(DomainError::MissingTerminalToolOutput {
                                tool_call_id: tool.tool_call_id.value(),
                                status: projected_tool_status_name(tool.status),
                            })?;
                    group.results.push(SourcedProjectedItem {
                        source_from_seq: event.seq,
                        source_to_seq: event.seq,
                        item: ProjectedItem::ToolResult {
                            tool_call_id: tool.tool_call_id,
                            provider_tool_call_id: tool.provider_tool_call_id.clone(),
                            tool_name: tool.tool_name.clone(),
                            status: tool.status,
                            output,
                        },
                    });
                }
            }
            EventBody::SubagentReturn(return_event) => {
                flush_pending(&mut items, &mut pending)?;
                items.push(SourcedProjectedItem {
                    source_from_seq: event.seq,
                    source_to_seq: event.seq,
                    item: ProjectedItem::SubagentReturnText {
                        text: return_event.content.clone(),
                    },
                });
            }
            EventBody::ApiStats(_) => {}
            EventBody::ApiStarted(_)
            | EventBody::QueuedInputResolution(_)
            | EventBody::TurnBoundary(_)
            | EventBody::TurnStats(_)
            | EventBody::SubagentReturnStats(_)
            | EventBody::DisplayNote(_)
            | EventBody::RequestConfigChanged(_)
            | EventBody::ExecutionControl(_) => {
                // Provider-invisible facts are filtered; the sequence stays
                // strictly in original seq order.
                // Provider 不可见事实被过滤；序列严格保持原始 seq 顺序。
            }
        }
    }
    flush_pending(&mut items, &mut pending)?;
    Ok(items)
}

/// Returns the stable diagnostic name of one projected tool status.
/// 返回一个被投影工具状态的稳定诊断名。
const fn projected_tool_status_name(status: ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Prepared => "prepared",
        ToolCallStatus::Running => "running",
        ToolCallStatus::WaitingUser => "waiting_user",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed => "failed",
        ToolCallStatus::Cancelled => "cancelled",
        ToolCallStatus::Stopped => "stopped",
        ToolCallStatus::Interrupted => "interrupted",
    }
}

/// Returns a mutable handle to one group's pending accumulation.
/// 返回一个组挂起累积的可变句柄。
fn pending_group(
    pending: &mut Vec<(ApiCallId, PendingGroup)>,
    api_call_id: ApiCallId,
    source_seq: Seq,
) -> &mut PendingGroup {
    let existing = pending.iter().position(|(id, _)| *id == api_call_id);
    let group = match existing {
        Some(index) => &mut pending[index].1,
        None => {
            pending.push((api_call_id, PendingGroup::default()));
            let index = pending.len() - 1;
            &mut pending[index].1
        }
    };
    group.source_from_seq.get_or_insert(source_seq);
    group.source_to_seq = Some(source_seq);
    group
}

/// Flushes the current group if it belongs to `api_call_id`.
/// 当当前组属于 `api_call_id` 时把它 flush 掉。
fn flush_group(
    items: &mut Vec<SourcedProjectedItem>,
    pending: &mut Vec<(ApiCallId, PendingGroup)>,
    api_call_id: ApiCallId,
) -> Result<(), DomainError> {
    if let Some(index) = pending.iter().position(|(id, _)| *id == api_call_id) {
        let (_, group) = pending.remove(index);
        emit_group(items, group)?;
    }
    Ok(())
}

/// Flushes every pending group in first-seen order.
/// 按首次出现顺序 flush 全部挂起组。
fn flush_pending(
    items: &mut Vec<SourcedProjectedItem>,
    pending: &mut Vec<(ApiCallId, PendingGroup)>,
) -> Result<(), DomainError> {
    let groups: Vec<PendingGroup> = pending.drain(..).map(|(_, group)| group).collect();
    for group in groups {
        emit_group(items, group)?;
    }
    Ok(())
}

/// Emits one completed group: assistant message then its tool results.
/// 发射一个完成的组：assistant 消息与随后的工具结果。
fn emit_group(
    items: &mut Vec<SourcedProjectedItem>,
    group: PendingGroup,
) -> Result<(), DomainError> {
    let source_from_seq = group
        .source_from_seq
        .ok_or(DomainError::ImmutableFieldMutation {
            entity: "provider_projection_group",
            field: "source_from_seq",
        })?;
    let source_to_seq = group
        .source_to_seq
        .ok_or(DomainError::ImmutableFieldMutation {
            entity: "provider_projection_group",
            field: "source_to_seq",
        })?;
    let tool_calls = group.tool_calls;
    items.push(SourcedProjectedItem {
        source_from_seq,
        source_to_seq,
        item: ProjectedItem::AssistantText {
            text: group.text,
            reasoning: group.reasoning,
            tool_calls,
            provider_source: group.provider_source.map(Box::new),
        },
    });
    items.extend(group.results);
    Ok(())
}

/// Renders the provider role hint with its stable default.
/// 渲染 Provider 角色提示及其稳定默认值。
fn role_hint(hint: Option<SystemRoleHint>) -> ProjectedSystemRole {
    match hint {
        Some(SystemRoleHint::Developer) => ProjectedSystemRole::Developer,
        Some(SystemRoleHint::System) => ProjectedSystemRole::System,
        Some(SystemRoleHint::HiddenUserShell) => ProjectedSystemRole::HiddenUserShell,
        None => ProjectedSystemRole::Developer,
    }
}

/// Computes the replay fingerprint over the immutable Provider-visible
/// projection of one covered range.
/// 对一段覆盖范围内的不可变 Provider 可见投影计算重放指纹。
///
/// The hash covers the projected items only; `provider_source` objects and
/// host-later tool fields (output, changeset, terminal status, timing) never
/// enter it.
/// 哈希只覆盖投影项；`provider_source` 对象与宿主后写的工具字段(output、
/// changeset、终态、时间)绝不进入指纹。
pub fn provider_visible_fingerprint(
    events: &[ConversationEvent],
    coverage_until_seq: Seq,
) -> Result<String, DomainError> {
    let coverage = coverage_until_seq.value();
    let mut stripped: Vec<ConversationEvent> = events
        .iter()
        .filter(|event| event.seq().value() <= coverage)
        .cloned()
        .collect();
    // Strip provider_source so the hash never includes itself.
    // 剥除 provider_source，使哈希永不包含其自身。
    for event in &mut stripped {
        match &mut event.event {
            EventBody::AssistantReasoning(reasoning) => {
                reasoning.provider_source = None;
            }
            EventBody::AssistantText(text) => {
                text.provider_source = None;
            }
            EventBody::ToolCall(tool) => {
                tool.provider_source = None;
            }
            _ => {}
        }
    }
    let mut items = project_provider_items(&stripped, None, &[])?;
    // Tool results are new Provider inputs after the replay checkpoint and
    // must never enter the checkpoint fingerprint (plan 5.5.10); the tool call
    // itself (provider id/name/args) stays part of the immutable projection.
    // 工具结果是重放检查点之后的新 Provider 输入，绝不能进入检查点指纹
    // (计划 5.5.10)；工具调用本身(Provider id/名/参数)仍属于不可变投影。
    items.retain(|item| !matches!(item, ProjectedItem::ToolResult { .. }));
    let bytes =
        serde_json::to_vec(&items).map_err(|_| DomainError::reference("fingerprint", coverage))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex_encode(&hasher.finalize()))
}

/// Encodes one digest as lowercase hex.
/// 把一个摘要编码为小写十六进制。
fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

/// Convenience projection entry for documents.
/// 文档投影便捷入口。
pub fn project_document(
    document: &crate::v1::document::SessionDocument,
    retained_from_seq: Option<Seq>,
    backfill_system: &[ActiveSystemState],
) -> Result<Vec<ProjectedItem>, DomainError> {
    project_provider_items(document.events(), retained_from_seq, backfill_system)
}

#[cfg(test)]
mod tests_projection {
    use super::*;
    use crate::v1::document::{SessionDocument, ToolCompletion};
    use crate::v1::stats::UsageSnapshot;

    fn build_document() -> SessionDocument {
        let mut document = SessionDocument::new("sess", "project", "title", 1_700_000_000_000);
        document
            .submit_root_input_fixture("hello", Vec::new(), 1_700_000_000_000)
            .unwrap();
        let api = document
            .start_api(
                ApiPurposeKind::MainGeneration,
                crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
                0,
            )
            .unwrap();
        document
            .append_reasoning(
                api,
                "think",
                AssistantSemanticStatus::Completed,
                1_700_000_000_000,
            )
            .unwrap();
        document
            .append_assistant_text(
                api,
                "hi",
                AssistantSemanticStatus::Completed,
                1_700_000_000_000,
            )
            .unwrap();
        let tool = document
            .start_tool(
                api,
                "shell",
                ToolPayload::try_new(serde_json::json!({"cmd": "ls"})).unwrap(),
                Some("pt-1".to_string()),
                1_700_000_000_000,
            )
            .unwrap();
        document
            .finish_tool(
                api,
                tool,
                crate::v1::document::ToolCompletion {
                    output: Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
                    changeset: None,
                    status: ToolCallStatus::Completed,
                    finished_at_ms: 1_700_000_000_000,
                    external_refs: Vec::new(),
                    generated_resources: Vec::new(),
                },
            )
            .unwrap();
        document
            .complete_api_group(
                api,
                crate::v1::document::ApiCompletion {
                    request: ApiRequestSnapshot::new("openai", "gpt-5"),
                    usage: UsageSnapshot::default(),
                    status: ApiCallStatus::Completed,
                    end_reason: Some(ApiEndReason::CompletedSuccessfully),
                    error: None,
                    continuation_eligibility: ContinuationEligibility::Forbidden,
                    timing: ApiTimings::default(),
                    provider_source: None,
                    completed_at_ms: 1_700_000_000_000,
                },
            )
            .unwrap();
        document
            .close_turn(TurnOutcome::Completed, 1, 1_700_000_000_000)
            .unwrap();
        document
    }

    fn kinds(items: &[ProjectedItem]) -> Vec<&'static str> {
        items
            .iter()
            .map(|item| match item {
                ProjectedItem::UserInput { .. } => "user",
                ProjectedItem::AssistantText { .. } => "assistant",
                ProjectedItem::ToolResult { .. } => "tool_result",
                ProjectedItem::SystemText { .. } => "system",
                ProjectedItem::SubagentReturnText { .. } => "subagent",
            })
            .collect()
    }

    /// Completes one tool-free physical API inside the currently open logical Turn.
    /// 在当前开放逻辑 Turn 内完成一次无工具物理 API。
    fn complete_text_api(document: &mut SessionDocument, text: &str, now_ms: i64) -> ApiCallId {
        let api_call_id = document
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                now_ms,
            )
            .expect("physical API must start");
        document
            .append_assistant_text(
                api_call_id,
                text,
                AssistantSemanticStatus::Completed,
                now_ms,
            )
            .expect("assistant text must persist");
        document
            .complete_api_group(
                api_call_id,
                crate::v1::document::ApiCompletion {
                    request: ApiRequestSnapshot::new("openai", "gpt-5"),
                    usage: UsageSnapshot::default(),
                    status: ApiCallStatus::Completed,
                    end_reason: Some(ApiEndReason::CompletedSuccessfully),
                    error: None,
                    continuation_eligibility: ContinuationEligibility::Forbidden,
                    timing: ApiTimings::default(),
                    provider_source: None,
                    completed_at_ms: now_ms,
                },
            )
            .expect("physical API must complete");
        api_call_id
    }

    /// Provider-invisible events are filtered; the visible sequence keeps the
    /// assistant message followed by its tool results.
    /// Provider 不可见事件被过滤；可见序列保持 assistant 消息后随工具结果。
    #[test]
    fn projection_filters_invisible_and_keeps_order() {
        let document = build_document();
        let items = project_document(&document, None, &[]).unwrap();
        assert_eq!(kinds(&items), vec!["user", "assistant", "tool_result"]);
        match &items[1] {
            ProjectedItem::AssistantText {
                text,
                reasoning,
                tool_calls,
                provider_source,
            } => {
                assert_eq!(text, "hi");
                assert_eq!(reasoning.as_deref(), Some("think"));
                assert_eq!(tool_calls.len(), 1);
                assert!(provider_source.is_none());
            }
            _ => unreachable!(),
        }
    }

    /// Provider projection preserves the exact replay source selected by the document.
    /// Provider 投影保留由文档选定的精确重放来源。
    #[test]
    fn projection_preserves_provider_source_on_assistant_group() {
        let mut document = SessionDocument::new("source", "project", "title", 1);
        document
            .submit_root_input_fixture("hello", Vec::new(), 1)
            .expect("root input must persist");
        let api_call_id = document
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                2,
            )
            .expect("API must start");
        document
            .append_assistant_text(api_call_id, "answer", AssistantSemanticStatus::Completed, 3)
            .expect("assistant text must persist");
        document
            .complete_api_group(
                api_call_id,
                crate::v1::document::ApiCompletion {
                    request: ApiRequestSnapshot::new("openai", "gpt-5"),
                    usage: UsageSnapshot::default(),
                    status: ApiCallStatus::Completed,
                    end_reason: Some(ApiEndReason::CompletedSuccessfully),
                    error: None,
                    continuation_eligibility: ContinuationEligibility::Forbidden,
                    timing: ApiTimings::default(),
                    provider_source: Some(ProviderSourceInput {
                        provider: String::from("openai"),
                        profile: None,
                        model: String::from("gpt-5"),
                        connection_id: None,
                        message_id: Some(String::from("response-1")),
                        response_id: None,
                        replay: None,
                    }),
                    completed_at_ms: 4,
                },
            )
            .expect("API must complete");

        let items = project_document(&document, None, &[]).expect("projection must succeed");
        let provider_source = items.iter().find_map(|item| match item {
            ProjectedItem::AssistantText {
                provider_source, ..
            } => provider_source.as_ref(),
            _ => None,
        });
        let provider_source = provider_source.expect("assistant group must retain provider source");
        assert_eq!(provider_source.provider, "openai");
        assert_eq!(provider_source.model, "gpt-5");
        assert_eq!(provider_source.message_id.as_deref(), Some("response-1"));
        assert_eq!(provider_source.covered_until_seq.value(), 3);
        assert_eq!(provider_source.fingerprint_sha256.len(), 64);
    }

    /// UiOnly ApiStats flushes its API group before a later Provider-visible
    /// system context; the exact serialized projection is the ordering golden.
    /// UiOnly ApiStats 在后续 Provider 可见系统上下文前 flush 其 API 组；精确
    /// 序列化投影就是顺序 golden。
    #[test]
    fn api_stats_boundary_precedes_following_system_context_golden()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut document = build_document();
        document.append_system_context(
            None,
            "goal_continuation",
            SystemContextOperation::Upsert,
            SystemContextScope::Session,
            None,
            None,
            "next",
            1_700_000_000_001,
        )?;

        let items = project_document(&document, None, &[])?;
        let actual = serde_json::to_string(&items)?;
        assert_eq!(
            actual,
            r#"[{"kind":"user_input","text":"hello","resource_ids":[]},{"kind":"assistant_text","text":"hi","reasoning":"think","tool_calls":[{"tool_call_id":4,"provider_tool_call_id":"pt-1","tool_name":"shell","input":{"cmd":"ls"}}]},{"kind":"tool_result","tool_call_id":4,"provider_tool_call_id":"pt-1","tool_name":"shell","status":"completed","output":{"ok":true}},{"kind":"system_text","role":"developer","context_key":"goal_continuation","text":"next"}]"#
        );
        Ok(())
    }

    /// A tool event expands into a tool call and its result from one event.
    /// 一个工具事件展开为来自同一事件的工具调用与其结果。
    #[test]
    fn tool_event_expands_into_call_and_result() {
        let document = build_document();
        let expected_tool_call_id = document
            .events()
            .iter()
            .find_map(|event| match &event.event {
                EventBody::ToolCall(tool) => Some(tool.tool_call_id),
                _ => None,
            })
            .expect("fixture must contain one tool");
        let items = project_document(&document, None, &[]).unwrap();
        match &items[1] {
            ProjectedItem::AssistantText { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].tool_call_id, expected_tool_call_id);
            }
            _ => unreachable!(),
        }
        match &items[2] {
            ProjectedItem::ToolResult {
                tool_call_id,
                provider_tool_call_id,
                tool_name,
                status,
                ..
            } => {
                assert_eq!(*tool_call_id, expected_tool_call_id);
                assert_eq!(provider_tool_call_id.as_deref(), Some("pt-1"));
                assert_eq!(tool_name, "shell");
                assert_eq!(*status, ToolCallStatus::Completed);
            }
            _ => unreachable!(),
        }
    }

    /// Local tool identity pairs call and result even when the Provider supplied no native id.
    /// 即使 Provider 未提供原生 id，本地工具身份仍会配对调用与结果。
    #[test]
    fn local_tool_identity_pairs_projection_without_provider_id() {
        let mut document = SessionDocument::new("s", "p", "t", 1);
        document
            .submit_root_input_fixture("run", Vec::new(), 1)
            .unwrap();
        let api = document
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                2,
            )
            .unwrap();
        let tool_call_id = document
            .start_tool(
                api,
                "shell",
                ToolPayload::try_new(serde_json::json!({"cmd": "pwd"})).unwrap(),
                None,
                3,
            )
            .unwrap();
        document
            .finish_tool(
                api,
                tool_call_id,
                ToolCompletion {
                    output: Some(
                        ToolPayload::try_new(serde_json::json!({"exit_code": 0})).unwrap(),
                    ),
                    changeset: None,
                    status: ToolCallStatus::Completed,
                    finished_at_ms: 4,
                    external_refs: Vec::new(),
                    generated_resources: Vec::new(),
                },
            )
            .unwrap();
        document
            .complete_api_group(
                api,
                crate::v1::document::ApiCompletion {
                    request: ApiRequestSnapshot::new("openai", "gpt-5"),
                    usage: UsageSnapshot::default(),
                    status: ApiCallStatus::Completed,
                    end_reason: Some(ApiEndReason::CompletedSuccessfully),
                    error: None,
                    continuation_eligibility: ContinuationEligibility::Forbidden,
                    timing: ApiTimings::default(),
                    provider_source: None,
                    completed_at_ms: 5,
                },
            )
            .unwrap();

        let items = project_document(&document, None, &[]).unwrap();
        let projected_call = items.iter().find_map(|item| match item {
            ProjectedItem::AssistantText { tool_calls, .. } => tool_calls.first(),
            _ => None,
        });
        let projected_result = items.iter().find(|item| {
            matches!(
                item,
                ProjectedItem::ToolResult {
                    tool_call_id: value,
                    provider_tool_call_id: None,
                    ..
                } if *value == tool_call_id
            )
        });
        assert_eq!(
            projected_call.map(|call| (call.tool_call_id, call.provider_tool_call_id.as_ref())),
            Some((tool_call_id, None))
        );
        let projected_result = projected_result.expect("local result pair must be projected");

        let json = serde_json::to_string(projected_result).unwrap();
        let decoded: ProjectedItem = serde_json::from_str(&json).unwrap();
        assert_eq!(&decoded, projected_result);
        let mut missing_local_id = serde_json::to_value(projected_result).unwrap();
        missing_local_id
            .as_object_mut()
            .expect("projected item must serialize as an object")
            .remove("tool_call_id");
        assert!(serde_json::from_value::<ProjectedItem>(missing_local_id).is_err());
    }

    /// The fingerprint is a stable sha-256 digest over the immutable
    /// Provider-visible projection: host-later tool fields never move it,
    /// provider-produced tool arguments do.
    /// 指纹是对不可变 Provider 可见投影的稳定 sha-256 摘要：宿主后写的工具
    /// 字段绝不改变它，Provider 产生的工具参数会改变它。
    #[test]
    fn fingerprint_is_stable_and_excludes_host_fields() {
        let document = build_document();
        let coverage = document.events().last().unwrap().seq();
        let first = provider_visible_fingerprint(document.events(), coverage).unwrap();
        let again = provider_visible_fingerprint(document.events(), coverage).unwrap();
        assert_eq!(first, again);
        assert_eq!(first.len(), 64, "sha-256 hex digest length");

        // Host-later tool output/changeset/status/timing must not move the
        // fingerprint.
        // 宿主后写的工具 output/changeset/状态/时间绝不改变指纹。
        let mut host_later = document.events().to_vec();
        for event in &mut host_later {
            if let EventBody::ToolCall(tool) = &mut event.event {
                tool.output =
                    Some(ToolPayload::try_new(serde_json::json!({"host": true})).unwrap());
                tool.changeset = Some(vulcan_change_set::PersistentChangeSet {
                    mode: vulcan_change_set::ChangeSetMode::Applied,
                    summary: None,
                    files: Vec::new(),
                    diagnostics: Vec::new(),
                });
                tool.status = ToolCallStatus::Interrupted;
                tool.finished_at_ms = Some(99);
            }
        }
        let after_host_later = provider_visible_fingerprint(&host_later, coverage).unwrap();
        assert_eq!(after_host_later, first);

        // Provider-produced tool arguments DO move the fingerprint.
        // Provider 产生的工具参数确实会改变指纹。
        let mut changed_args = document.events().to_vec();
        for event in &mut changed_args {
            if let EventBody::ToolCall(tool) = &mut event.event {
                tool.input = ToolPayload::try_new(serde_json::json!({"cmd": "pwd"})).unwrap();
            }
        }
        let after_args = provider_visible_fingerprint(&changed_args, coverage).unwrap();
        assert_ne!(after_args, first);
    }

    /// Root attachment-only input preserves resource order and participates in the fingerprint.
    /// 仅附件根输入会保留资源顺序并参与指纹计算。
    #[test]
    fn root_attachment_only_input_projects_ordered_resources_and_fingerprint() {
        let mut document = SessionDocument::new("sess", "project", "title", 1);
        let first_resource = ResourceId::try_new(20).unwrap();
        let second_resource = ResourceId::try_new(10).unwrap();
        document
            .submit_root_input_fixture("", vec![first_resource, second_resource], 1)
            .unwrap();

        let items = project_document(&document, None, &[]).unwrap();
        assert!(matches!(
            &items[0],
            ProjectedItem::UserInput { text, resource_ids }
                if text.is_empty() && resource_ids == &vec![first_resource, second_resource]
        ));
        let coverage = document.events().last().unwrap().seq();
        let original = provider_visible_fingerprint(document.events(), coverage).unwrap();
        let mut reordered = document.events().to_vec();
        let EventBody::UserInput(input) = &mut reordered[0].event else {
            unreachable!("root event must be user input")
        };
        input.resource_ids.swap(0, 1);
        let changed = provider_visible_fingerprint(&reordered, coverage).unwrap();
        assert_ne!(original, changed);
    }

    /// Supplementary and queued inputs use the same resource-preserving projection contract.
    /// 过程内补充输入与排队输入使用同一资源保真投影契约。
    #[test]
    fn supplementary_and_queued_inputs_project_ordered_resources() {
        let supplementary_resource = ResourceId::try_new(30).unwrap();
        let mut in_turn = SessionDocument::new("in-turn", "project", "title", 1);
        in_turn
            .submit_root_input_fixture("root", Vec::new(), 1)
            .unwrap();
        in_turn
            .add_pending_in_turn_input_fixture("", vec![supplementary_resource], 2)
            .unwrap();
        in_turn
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                3,
            )
            .unwrap();
        in_turn.interrupt_active_api(4).unwrap();
        let promoted = in_turn
            .events()
            .iter()
            .find_map(|event| match &event.event {
                EventBody::UserInput(input)
                    if input.input_source == UserInputSource::PendingInTurn =>
                {
                    Some(event.seq)
                }
                _ => None,
            })
            .expect("supplementary input must be promoted by interruption");
        let supplementary_items = project_document(&in_turn, None, &[]).unwrap();
        assert!(matches!(
            &supplementary_items[1],
            ProjectedItem::UserInput { text, resource_ids }
                if text.is_empty() && resource_ids == &vec![supplementary_resource]
        ));
        let retained_items = project_document(&in_turn, Some(promoted), &[]).unwrap();
        assert_eq!(retained_items.len(), 1);
        assert!(matches!(
            &retained_items[0],
            ProjectedItem::UserInput { resource_ids, .. }
                if resource_ids == &vec![supplementary_resource]
        ));

        let queued_resource = ResourceId::try_new(40).unwrap();
        let mut queued = SessionDocument::new("queued", "project", "title", 1_700_000_000_000);
        let queued_turn = queued
            .submit_root_input_fixture("root", Vec::new(), 1_700_000_000_000)
            .unwrap();
        queued
            .enqueue_input_fixture("", vec![queued_resource], 1_700_000_000_001)
            .unwrap();
        complete_text_api(&mut queued, "done", 1_700_000_000_002);
        let transition = queued
            .complete_turn_and_select_successor(queued_turn, 1_700_000_000_003)
            .unwrap();
        assert!(matches!(
            transition.successor,
            crate::v1::document::TurnSuccessor::QueuedUser { .. }
        ));
        let queued_items = project_document(&queued, None, &[]).unwrap();
        assert!(matches!(
            queued_items.last().unwrap(),
            ProjectedItem::UserInput { text, resource_ids }
                if text.is_empty() && resource_ids == &vec![queued_resource]
        ));
    }

    /// Compaction backfill restores still-active system states exactly once.
    /// 压缩回追恰好一次恢复仍然生效的系统状态。
    #[test]
    fn backfill_restores_active_system_state() {
        let mut document = build_document();
        // The goal state is declared before the boundary and still active.
        // goal 状态在边界之前声明且仍然生效。
        document
            .append_system_context(
                None,
                "goal_state",
                SystemContextOperation::Upsert,
                SystemContextScope::Session,
                None,
                None,
                "continue",
                1_700_000_000_001,
            )
            .unwrap();
        let last = document.events().last().unwrap().seq();
        let boundary = Seq::try_new(last.value() + 1).unwrap();
        let items = project_document(
            &document,
            Some(boundary),
            &[ActiveSystemState {
                source_seq: last,
                context_key: "goal_state".to_string(),
                role: ProjectedSystemRole::Developer,
                content: "continue".to_string(),
                scope: SystemContextScope::Session,
                remaining_turns: None,
                last_consumed_turn_id: None,
            }],
        )
        .unwrap();
        let goal_entries = items
            .iter()
            .filter(|item| matches!(item, ProjectedItem::SystemText { context_key, .. } if context_key == "goal_state"))
            .count();
        assert_eq!(
            goal_entries, 1,
            "backfill must project the active state exactly once"
        );
        // Pre-boundary events are filtered: without backfill nothing appears.
        // 边界前事件被过滤：没有回追时什么都不出现。
        let empty_items = project_document(&document, Some(boundary), &[]).unwrap();
        assert_eq!(empty_items.len(), 0);
    }

    /// TTL consumes once per logical Turn, survives compaction backfill, and expires deterministically.
    /// TTL 每个逻辑 Turn 仅消费一次，能够跨压缩回追，并确定性过期。
    #[test]
    fn ttl_system_context_tracks_logical_turns_and_compaction_boundaries() {
        let mut document = SessionDocument::new("ttl", "project", "title", 1);
        let ttl_seq = document
            .append_system_context(
                None,
                "vmm:memory-1",
                SystemContextOperation::Upsert,
                SystemContextScope::Session,
                Some(2),
                Some(SystemRoleHint::Developer),
                "remember this",
                1,
            )
            .expect("TTL context must persist");
        let first_turn = document
            .submit_root_input_fixture("first", Vec::new(), 2)
            .expect("first Turn");
        complete_text_api(&mut document, "checkpoint", 3);
        complete_text_api(&mut document, "final", 4);
        let before_close = project_document(&document, None, &[]).expect("open-Turn projection");
        assert_eq!(
            before_close
                .iter()
                .filter(|item| matches!(item, ProjectedItem::SystemText { text, .. } if text == "remember this"))
                .count(),
            1,
            "multiple physical APIs in one Turn never consume TTL twice"
        );
        document
            .close_turn(TurnOutcome::Completed, 2, 5)
            .expect("first Turn closes");

        let first_closed_state = derive_active_system_state(
            document.events(),
            document.events().last().expect("turn stats").seq(),
        )
        .expect("state rebuild");
        assert_eq!(first_closed_state[0].remaining_turns, Some(1));
        assert_eq!(
            first_closed_state[0].last_consumed_turn_id,
            Some(first_turn)
        );
        let backfill = derive_active_system_state(document.events(), ttl_seq)
            .expect("pre-boundary state rebuild");
        let retained_from_seq = Seq::try_new(ttl_seq.value() + 1).expect("next seq");
        let retained_items = project_document(&document, Some(retained_from_seq), &backfill)
            .expect("retained projection");
        assert!(retained_items.iter().any(
            |item| matches!(item, ProjectedItem::SystemText { text, .. } if text == "remember this")
        ));

        let second_turn = document
            .submit_root_input_fixture("second", Vec::new(), 6)
            .expect("second Turn");
        complete_text_api(&mut document, "done", 7);
        document
            .close_turn(TurnOutcome::Completed, 1, 8)
            .expect("second Turn closes");
        assert_ne!(first_turn, second_turn);
        assert!(
            derive_active_system_state(
                document.events(),
                document.events().last().expect("turn stats").seq(),
            )
            .expect("state rebuild")
            .is_empty()
        );
        let expired_items = project_document(&document, Some(retained_from_seq), &backfill)
            .expect("restart-equivalent retained projection");
        assert!(!expired_items.iter().any(
            |item| matches!(item, ProjectedItem::SystemText { text, .. } if text == "remember this")
        ));
    }

    /// Upsert resets TTL and an explicit Remove terminates the refreshed generation immediately.
    /// Upsert 会重置 TTL，显式 Remove 会立即终止刷新后的代际。
    #[test]
    fn ttl_upsert_resets_and_remove_terminates_the_same_key() {
        let mut document = SessionDocument::new("ttl-reset", "project", "title", 1);
        document
            .append_system_context(
                None,
                "vmm:memory-1",
                SystemContextOperation::Upsert,
                SystemContextScope::Turn,
                Some(1),
                None,
                "first value",
                1,
            )
            .expect("Turn TTL must persist");
        document
            .submit_root_input_fixture("first", Vec::new(), 2)
            .expect("first Turn");
        complete_text_api(&mut document, "done", 3);
        document
            .close_turn(TurnOutcome::Completed, 1, 4)
            .expect("first Turn closes");
        assert!(
            derive_active_system_state(
                document.events(),
                document.events().last().expect("turn stats").seq(),
            )
            .expect("expired state")
            .is_empty()
        );

        document
            .append_system_context(
                None,
                "vmm:memory-1",
                SystemContextOperation::Upsert,
                SystemContextScope::Session,
                Some(2),
                None,
                "refreshed value",
                5,
            )
            .expect("refreshed TTL must persist");
        assert!(project_document(&document, None, &[])
            .expect("active refreshed projection")
            .iter()
            .any(|item| matches!(item, ProjectedItem::SystemText { text, .. } if text == "refreshed value")));
        document
            .append_system_context(
                None,
                "vmm:memory-1",
                SystemContextOperation::Remove,
                SystemContextScope::Session,
                None,
                None,
                "The temporary memory is no longer active.",
                6,
            )
            .expect("explicit removal must persist");
        let removed = project_document(&document, None, &[]).expect("removed projection");
        assert!(!removed.iter().any(
            |item| matches!(item, ProjectedItem::SystemText { text, .. } if text == "refreshed value")
        ));
        assert!(removed.iter().any(
            |item| matches!(item, ProjectedItem::SystemText { text, .. } if text == "The temporary memory is no longer active.")
        ));
    }
}
