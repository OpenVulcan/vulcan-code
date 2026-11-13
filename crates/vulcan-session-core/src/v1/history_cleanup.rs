//! Local extraction of display history outside the active request window.
//! 在当前请求窗口以外本地提取供阅读的历史。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::document::SessionDocument;
use super::errors::DomainError;
use super::event::{
    ApiCallStatus, ApiPurposeKind, ApiStatsEvent, AssistantSemanticStatus, ConversationEvent,
    EventBody, EventVisibility, GeneratedResourceKind, ToolCallStatus, ToolPayload, TurnOutcome,
    UserInputSource,
};
use super::ids::{ResourceId, Seq};
use super::projection::project_context_generation_items_with_sources;

/// One atomic link from the session metadata to an independent history file.
/// 从会话元数据指向独立历史文件的一条原子链接。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryPublication {
    /// Unique immutable file identity for one published history revision.
    /// 一份已发布历史修订的唯一不可变文件标识。
    pub id: Uuid,
    /// Inclusive internal event ranges already extracted; display records use their own IDs.
    /// 已提取的内部事件闭区间，展示记录使用自己的标识。
    pub ranges: Vec<CleanedEventRange>,
}

/// One internal source coverage interval published with the cleaned history.
/// 与精简历史一同发布的一段内部来源覆盖区间。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanedEventRange {
    /// First event position replaced in the display history.
    /// 在展示历史中被替换的第一个事件位置。
    pub from: Seq,
    /// Last event position replaced in the display history.
    /// 在展示历史中被替换的最后一个事件位置。
    pub through: Seq,
}

impl HistoryPublication {
    /// Checks sorted non-overlapping source coverage below the durable event frontier.
    /// 校验来源覆盖已排序、互不重叠并位于持久事件前沿之前。
    pub fn validate(&self, next_seq: u64) -> Result<(), DomainError> {
        if self.id.is_nil()
            || self.ranges.is_empty()
            || self
                .ranges
                .iter()
                .any(|range| range.from > range.through || range.through.value() >= next_seq)
            || self
                .ranges
                .windows(2)
                .any(|pair| pair[0].through >= pair[1].from)
        {
            return Err(DomainError::InvalidGenerationInput {
                field: "cleaned_history_publication",
            });
        }
        Ok(())
    }
}

/// A checked cleanup candidate containing display history and the retained execution facts.
/// 经过校验的清理候选，包含可读历史与保留的执行事实。
pub struct HistoryCleanupCandidate {
    /// Inclusive start derived from the first remaining eligible turn.
    /// 从最早剩余可清理回合推导的包含式起点。
    pub from: Seq,
    /// Independently addressable display records.
    /// 可独立定位的展示记录。
    pub history: CleanedHistory,
    /// Stream positions and recovery facts required by the existing runtime.
    /// 现有运行时需要的流位置和恢复事实。
    pub events: Vec<ConversationEvent>,
}

/// Display-only records ordered by their original presentation order.
/// 按原展示顺序排列、仅供阅读的历史记录。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanedHistory {
    /// Conversation entries with independent identifiers.
    /// 使用独立标识的问答条目。
    pub turns: Vec<HistoryTurn>,
    /// Resource index entries that can outlive their associated conversation entries.
    /// 可以独立于对应问答条目存在的资源索引。
    pub resources: Vec<HistoryResource>,
}

/// One closed conversation turn extracted from the execution stream.
/// 从执行流提取的一个已结束对话回合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryTurn {
    /// Independent identifier for display and deletion.
    /// 用于展示和删除的独立标识。
    pub id: Uuid,
    /// Original turn closure time in Unix milliseconds.
    /// 原回合结束时间，单位为 Unix 毫秒。
    pub closed_at_ms: i64,
    /// Durable outcome of the original turn.
    /// 原回合的持久终态。
    pub outcome: TurnOutcome,
    /// Original human inputs in order, including supplementary prompts.
    /// 按顺序保留的人工输入原文，包含中途补充。
    pub inputs: Vec<HistoryInput>,
    /// Final assistant text, when the business API completed with an answer.
    /// 业务 API 完成且存在答复时的助手最终正文。
    pub answer: Option<HistoryAnswer>,
}

/// Final answer stored as an independently removable history entry.
/// 作为可独立删除的历史条目保存的最终答复。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryAnswer {
    /// Independent answer identity.
    /// 独立答复标识。
    pub id: Uuid,
    /// Original completed assistant text.
    /// 已完成的助手原文。
    pub text: String,
}

/// One human prompt extracted without runtime submission metadata.
/// 从历史提取的一条人工提示词，省略运行时提交元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryInput {
    /// Independent identifier for deleting this prompt.
    /// 用于删除该提示词的独立标识。
    pub id: Uuid,
    /// Original input time in Unix milliseconds.
    /// 原输入时间，单位为 Unix 毫秒。
    pub created_at_ms: i64,
    /// Whether the input was added during the same turn.
    /// 该输入是否在同回合中途追加。
    pub supplementary: bool,
    /// User text preserved verbatim.
    /// 原样保留的用户正文。
    pub text: String,
    /// Ordered project resource links attached by the user.
    /// 用户附加的有序项目资源引用。
    pub attachments: Vec<ResourceId>,
}

/// A catalog reference to an existing image or video produced by the session.
/// 指向会话已经产出的图片或视频的目录引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryResource {
    /// Independent identifier for deleting this index entry.
    /// 用于删除该索引条目的独立标识。
    pub id: Uuid,
    /// Independent history turn identifier, when the tool had an extracted turn.
    /// 工具所属回合已提取时使用的独立历史回合标识。
    pub history_turn_id: Option<Uuid>,
    /// Project resource identifier used to open the original asset.
    /// 用于打开原素材的项目资源标识。
    pub resource_id: ResourceId,
    /// Original preview reference, when the producer supplied one.
    /// 产出方提供时的原预览引用。
    pub preview_resource_id: Option<ResourceId>,
    /// Persisted display category of the original asset.
    /// 原素材的持久展示类别。
    pub kind: GeneratedResourceKind,
    /// Original tool event time in Unix milliseconds.
    /// 原工具事件时间，单位为 Unix 毫秒。
    pub created_at_ms: i64,
    /// Exact documented generation prompt, when available.
    /// 存在时保留协议定义的原生成提示词。
    pub prompt: Option<String>,
}

impl CleanedHistory {
    /// Checks unique independent IDs and verifies every non-detached resource owner.
    /// 检查独立标识的唯一性并验证每个未分离资源的归属。
    pub fn validate(&self) -> Result<(), DomainError> {
        // Distinguish turn owners from all selectable records.
        // 区分回合所有者和全部可选条目。
        let turns: BTreeSet<_> = self.turns.iter().map(|turn| turn.id).collect();
        // Track ID uniqueness across all entry categories.
        // 跨全部条目类别追踪标识唯一性。
        let mut ids = BTreeSet::new();
        for id in self
            .turns
            .iter()
            .flat_map(|turn| {
                std::iter::once(turn.id)
                    .chain(turn.inputs.iter().map(|input| input.id))
                    .chain(turn.answer.iter().map(|answer| answer.id))
            })
            .chain(self.resources.iter().map(|resource| resource.id))
        {
            if id.is_nil() || !ids.insert(id) {
                return Err(DomainError::InvalidGenerationInput {
                    field: "cleaned_history_entry_id",
                });
            }
        }
        if self.resources.iter().any(|resource| {
            resource
                .history_turn_id
                .is_some_and(|id| !turns.contains(&id))
                || !matches!(
                    resource.kind,
                    GeneratedResourceKind::Image | GeneratedResourceKind::Video
                )
        }) {
            return Err(DomainError::InvalidGenerationInput {
                field: "cleaned_history_resource",
            });
        }
        Ok(())
    }

    /// Deletes selected turn, prompt and resource entries by their independent IDs.
    /// 按独立标识删除选定的回合、提示词和素材索引。
    ///
    /// Returns the number removed. Detached assets remain listed without a turn link.
    /// 返回删除条数。删除归属回合后的素材仍留在索引，移除回合链接。
    pub fn delete(&mut self, ids: &BTreeSet<Uuid>) -> usize {
        // Count all independently selectable entries before applying the mutation.
        // 变更前统计所有可独立选择的条目。
        let before = self.entry_count();
        self.turns.retain(|turn| !ids.contains(&turn.id));
        for turn in &mut self.turns {
            turn.inputs.retain(|input| !ids.contains(&input.id));
            if turn
                .answer
                .as_ref()
                .is_some_and(|answer| ids.contains(&answer.id))
            {
                turn.answer = None;
            }
        }
        self.resources
            .retain(|resource| !ids.contains(&resource.id));
        for resource in &mut self.resources {
            if resource.history_turn_id.is_some_and(|id| ids.contains(&id)) {
                resource.history_turn_id = None;
            }
        }
        before - self.entry_count()
    }

    /// Counts the independently addressable entries in this view.
    /// 统计视图中可独立定位的条目数量。
    fn entry_count(&self) -> usize {
        self.turns.len()
            + self.resources.len()
            + self
                .turns
                .iter()
                .map(|turn| turn.inputs.len() + usize::from(turn.answer.is_some()))
                .sum::<usize>()
    }
}

/// Computes the first protected sequence from the committed node and stored replay windows.
/// 根据已提交节点与持久重放窗口计算保护区起点。
///
/// The input is a complete, durably loaded session. No active node means there is no removable prefix.
/// 输入为完整可靠加载的会话。没有生效节点时不存在可清理的前缀。
pub fn protected_start(document: &SessionDocument) -> Result<Option<Seq>, DomainError> {
    // The runtime pointer is the authority for the effective compaction.
    // 运行时指针是生效压缩的权威来源。
    let Some(compaction_id) = document.runtime_lock.current_compaction_id else {
        return Ok(None);
    };
    // The referenced node supplies the original retained request window.
    // 被引用的节点提供原始请求保留窗口。
    let node = document
        .compactions
        .iter()
        .find(|node| node.compaction_id == compaction_id)
        .ok_or_else(|| {
            DomainError::reference("history_cleanup.compaction", compaction_id.value())
        })?;
    // A full stream is required to find every turn and replay dependency at the boundary.
    // 完整事件流用于找到边界上全部回合与重放依赖。
    let events = document.events();
    if events.first().is_none_or(|event| event.seq.value() != 1) {
        return Err(DomainError::GenerationRequiresFullHistory {
            resident_start_seq: events.first().map_or(1, |event| event.seq.value()),
        });
    }
    // A retained turn may start before the actual provider projection boundary.
    // 保留回合可能早于实际供应商投影边界开始。
    let mut cut = complete_turn_start(events, node.retained_from_seq)?;
    // A still-active goal may rely on an older root prompt independently of the Provider window.
    // 仍生效的目标可能独立于供应商窗口依赖更早的根提示。
    if let Some(origin) = document
        .metadata
        .mode
        .goal
        .origin_seq
        .filter(|_| document.metadata.mode.goal.enabled)
    {
        cut = cut.min(complete_turn_start(events, origin)?);
    }
    // Only checkpoints carried by the original runtime tail can be used by its request.
    // 只有原运行时尾部承载的检查点能用于该运行时的请求。
    let runtime_tail =
        &events[events.partition_point(|event| event.seq < node.retained_from_seq)..];
    for event in runtime_tail {
        // Only these event kinds can carry an immutable Provider checkpoint.
        // 只有以下事件类型可以承载不可变供应商检查点。
        let source = match &event.event {
            EventBody::AssistantText(body) => body.provider_source.as_ref(),
            EventBody::AssistantReasoning(body) => body.provider_source.as_ref(),
            EventBody::ToolCall(body) => body.provider_source.as_ref(),
            _ => None,
        };
        if let Some(source) = source {
            // The digest covers its original segment window through the checkpoint coverage.
            // 摘要从原分片窗口开始覆盖到检查点覆盖位置。
            let window = verified_replay_window(document, source)?;
            cut = cut.min(window);
        }
    }
    complete_turn_start(events, cut).map(Some)
}

/// Finds the newest original segment window that reproduces an immutable replay digest.
/// 找到能够复现不可变重放摘要的最新原始分段窗口。
fn verified_replay_window(
    document: &SessionDocument,
    source: &super::event::ProviderSource,
) -> Result<Seq, DomainError> {
    // Use exactly the existing verifier's possible starts, including the full-stream start.
    // 使用现有校验器的精确候选起点，包括完整流起点。
    let first = Seq::try_new(1)?;
    for start in document
        .segment_catalog()
        .iter()
        .rev()
        .map(|entry| entry.start_seq)
        .chain(std::iter::once(first))
        .filter(|start| *start <= source.covered_until_seq)
    {
        // A persisted digest is the proof of the actual source window.
        // 持久摘要是实际来源窗口的证明。
        let events = document.events();
        // Preserve precisely the successful source window without dropping to an unverified format.
        // 精确保留通过验证的来源窗口，不降级到未经验证的格式。
        let index = events.partition_point(|event| event.seq < start);
        if super::projection::provider_visible_fingerprint(
            &events[index..],
            source.covered_until_seq,
        )? == source.fingerprint_sha256
        {
            return Ok(start);
        }
    }
    Err(DomainError::reference(
        "history_cleanup.replay_window",
        source.covered_until_seq.value(),
    ))
}

/// Expands a stream boundary to preserve the complete turn it intersects.
/// 将流边界扩展到相交回合的起点，以保留完整回合。
///
/// Returns the boundary itself when its event is outside any turn.
/// 边界事件不属于任何回合时返回边界本身。
fn complete_turn_start(events: &[ConversationEvent], boundary: Seq) -> Result<Seq, DomainError> {
    // Locate the boundary event by its validated stream position.
    // 按已校验的流位置定位边界事件。
    let index = events
        .binary_search_by_key(&boundary, |event| event.seq)
        .map_err(|_| DomainError::reference("history_cleanup.boundary", boundary.value()))?;
    // Turn membership is explicit in the event header.
    // 回合归属由事件头明确给出。
    let Some(turn_id) = events[index].turn_id else {
        return Ok(boundary);
    };
    events[..=index]
        .iter()
        .find(|event| event.turn_id == Some(turn_id))
        .map(|event| event.seq)
        .ok_or_else(|| DomainError::reference("history_cleanup.turn", turn_id.value()))
}

/// Iterates complete, uncleaned turn closures before the supplied protection boundary.
/// 遍历给定保护区边界之前完整且尚未清理的回合闭合记录。
///
/// The document provides the full stream and the published coverage. Returns closures in stream order.
/// 文档提供完整事件流与已发布覆盖范围，返回值按事件流顺序排列。
pub fn eligible_turns(
    document: &SessionDocument,
    boundary: Option<Seq>,
) -> impl Iterator<Item = (&ConversationEvent, &super::event::TurnStatsEvent)> {
    document.events().iter().filter_map(move |event| {
        // A closed turn is eligible only when every source event remains outside published coverage.
        // 已结束回合的全部来源事件都未被已发布范围覆盖时才可清理。
        let EventBody::TurnStats(stats) = &event.event else {
            return None;
        };
        (boundary.is_some_and(|cut| stats.end_seq < cut)
            && !is_already_cleaned(document, stats.start_seq, stats.end_seq))
        .then_some((event, stats))
    })
}

/// Returns whether published coverage intersects the supplied inclusive source interval.
/// 返回已发布覆盖范围是否与给定的包含式来源区间相交。
fn is_already_cleaned(document: &SessionDocument, from: Seq, through: Seq) -> bool {
    document
        .metadata
        .cleaned_history
        .as_ref()
        .is_some_and(|history| {
            history
                .ranges
                .iter()
                .any(|range| range.from <= through && from <= range.through)
        })
}

/// Extracts complete turns in the verified source prefix and skips published history.
/// 从已校验的来源前缀提取完整回合，并跳过已发布历史。
///
/// Both limits are inclusive event positions from the selected source window. The output has no event positions.
/// 两个边界是所选来源窗口内的包含式事件位置，输出不携带事件位置。
fn extract_history(
    document: &SessionDocument,
    from: Seq,
    through: Seq,
) -> Result<CleanedHistory, DomainError> {
    // Slice the complete validated source stream once.
    // 对已验证的完整来源事件流只截取一次。
    let events = document.events();
    // Each range endpoint must correspond to a complete turn boundary.
    // 范围两端必须分别对应完整回合边界。
    let range = &events[events.partition_point(|event| event.seq < from)
        ..events.partition_point(|event| event.seq <= through)];
    if !range
        .iter()
        .any(|event| matches!(&event.event, EventBody::TurnStats(stats) if stats.start_seq == from))
        || !range.last().is_some_and(
            |event| matches!(&event.event, EventBody::TurnStats(stats) if stats.end_seq == through),
        )
    {
        return Err(DomainError::InvalidGenerationInput {
            field: "history_cleanup.complete_turn_range",
        });
    }
    // The temporary map groups source events without embedding original IDs in the output.
    // 临时映射对来源事件分组，输出不携带原有标识。
    let mut by_turn = BTreeMap::<_, Vec<_>>::new();
    for event in range {
        if let Some(turn_id) = event.turn_id {
            by_turn.entry(turn_id).or_default().push(event);
        }
    }
    // Output order follows closure events, independent of source ID allocation.
    // 输出顺序跟随闭合事件，与原有标识分配无关。
    let mut result = CleanedHistory::default();
    for event in range {
        if let EventBody::TurnStats(stats) = &event.event
            && let Some(turn_id) = event.turn_id
        {
            // Never copy the fragment of a turn that overlaps the chosen range.
            // 不复制与选择范围部分重叠的回合。
            if stats.start_seq < from || stats.end_seq > through {
                return Err(DomainError::InvalidGenerationInput {
                    field: "history_cleanup.partial_turn",
                });
            }
            // Legacy middle-range publications retain their independent IDs and deletion state.
            // 旧版清理过的中段保留其独立标识和删除状态。
            if is_already_cleaned(document, stats.start_seq, stats.end_seq) {
                continue;
            }
            // The grouping map was populated from this exact source range.
            // 分组映射来自同一份精确来源范围。
            let group = &by_turn[&turn_id];
            // New identifiers are assigned only during extraction and are independent of source positions.
            // 新标识只在提取时分配，与来源事件位置无关。
            let id = Uuid::new_v4();
            result.turns.push(HistoryTurn {
                id,
                closed_at_ms: event.created_at_ms,
                outcome: stats.outcome,
                inputs: group
                    .iter()
                    .filter_map(|event| match &event.event {
                        EventBody::UserInput(input)
                            if input.input_source != UserInputSource::AgentMessage =>
                        {
                            Some(HistoryInput {
                                id: Uuid::new_v4(),
                                created_at_ms: event.created_at_ms,
                                supplementary: input.input_source == UserInputSource::PendingInTurn,
                                text: input.text.clone(),
                                attachments: input.resource_ids.clone(),
                            })
                        }
                        _ => None,
                    })
                    .collect(),
                answer: final_answer(group).map(|text| HistoryAnswer {
                    id: Uuid::new_v4(),
                    text,
                }),
            });
            for event in group {
                // Use typed project resources emitted by the host, including screenshots.
                // 使用宿主产出的强类型项目资源，包括截图。
                let EventBody::ToolCall(tool) = &event.event else {
                    continue;
                };
                result
                    .resources
                    .extend(extract_tool_resources(tool, id, event.created_at_ms));
            }
        }
    }
    Ok(result)
}

/// Extracts only completed image/video references and their documented generation prompt.
/// 仅提取已完成的图片／视频引用及其协议定义的生成提示词。
fn extract_tool_resources(
    tool: &super::event::ToolCallEvent,
    turn_id: Uuid,
    created_at_ms: i64,
) -> impl Iterator<Item = HistoryResource> + '_ {
    // Media generation has a documented string prompt; other artifact-producing tools may omit it.
    // 媒体生成具有协议定义的字符串提示词，其他产出素材的工具可以缺省。
    let prompt = match tool.tool_name.as_str() {
        "image_generate" | "video_generate" => tool.input.string_argument("prompt"),
        _ => None,
    };
    tool.generated_resources
        .iter()
        .filter(move |resource| {
            tool.status == ToolCallStatus::Completed
                && matches!(
                    resource.kind,
                    GeneratedResourceKind::Image | GeneratedResourceKind::Video
                )
        })
        .map(move |resource| HistoryResource {
            id: Uuid::new_v4(),
            history_turn_id: Some(turn_id),
            resource_id: resource.resource_id,
            preview_resource_id: resource.preview_resource_id,
            kind: resource.kind,
            created_at_ms,
            prompt: prompt.map(str::to_owned),
        })
}

/// Builds a local cleanup candidate and checks it with the ordinary Core reloader.
/// 构建本地清理候选，并使用普通 Core 重载逻辑校验。
///
/// The cutoff selects the last old turn. Returns independent history and a verified execution stream.
/// 截止位置选定最后一轮旧回合，返回独立历史与通过校验的执行流。
pub fn build_cleanup_candidate(
    document: &SessionDocument,
    through: Seq,
) -> Result<HistoryCleanupCandidate, DomainError> {
    // Derive both limits from the same authoritative list used to preview the operation.
    // 两端都根据用于操作预览的同一份权威列表确定。
    let boundary = protected_start(document)?;
    // The requested endpoint must be a remaining eligible turn.
    // 用户指定的终点必须是尚未清理且可清理的回合。
    let mut eligible = eligible_turns(document, boundary).peekable();
    // Always start from the oldest remaining eligible turn.
    // 始终从最早的剩余可清理回合开始。
    let from = eligible.peek().map(|(_, stats)| stats.start_seq).ok_or(
        DomainError::InvalidGenerationInput {
            field: "history_cleanup.no_eligible_turns",
        },
    )?;
    if !eligible.any(|(_, stats)| stats.end_seq == through) {
        return Err(DomainError::InvalidGenerationInput {
            field: "history_cleanup.protected_range",
        });
    }
    // Extract before stripping source payloads so every retained display fact is exact.
    // 在移除来源载荷之前提取，确保每条保留的展示事实都精确。
    let history = extract_history(document, from, through)?;
    // Preserve every compaction summary carrier as long as the compaction catalog references it.
    // 压缩目录仍引用时，保留每个摘要承载事件。
    let summaries: BTreeSet<_> = document
        .compactions
        .iter()
        .map(|node| node.occurred_at_seq)
        .collect();
    // Keep the existing event identities for allocator, collaboration and edit/fork verification.
    // 保留既有事件身份供分配器、协作及编辑分叉校验。
    let mut events = document.events().to_vec();
    // Older replay envelopes can depend on the selected events without being eligible for the active request.
    // 旧重放信封可能依赖所选事件，但不具备当前请求资格。
    // Only checkpoints at or after the real retained boundary can enter a rebuilt request.
    // 只有实际保留边界及其之后的检查点能够进入重建请求。
    let node = document
        .compactions
        .iter()
        .find(|node| Some(node.compaction_id) == document.runtime_lock.current_compaction_id)
        .ok_or(DomainError::InvalidGenerationInput {
            field: "history_cleanup.requires_compaction",
        })?;
    // A queued resolution must continue to match the exact input receipt during cold load.
    // 冷加载时，排队处置必须继续匹配精确的输入回执。
    let resolved_inputs: BTreeSet<_> = events
        .iter()
        .filter_map(|event| match &event.event {
            EventBody::QueuedInputResolution(resolution) => Some(resolution.input_id),
            _ => None,
        })
        .collect();
    // Display payloads stop at the earliest protected turn, including original replay windows.
    // 展示载荷在最早的受保护回合前停止，其中包括原始重放窗口。
    let protected_from = boundary.ok_or(DomainError::InvalidGenerationInput {
        field: "history_cleanup.requires_compaction",
    })?;
    for event in events
        .iter_mut()
        .take_while(|event| event.seq < protected_from)
    {
        // Only the user-selected range loses visible payloads.
        // 只有用户选择的范围移除可见载荷。
        let selected = (from..=through).contains(&event.seq)
            && !is_already_cleaned(document, event.seq, event.seq);
        match &mut event.event {
            EventBody::UserInput(input) if selected => {
                // Client-action, media bindings and turn ownership still participate in recovery.
                // 客户端动作、媒体绑定和回合归属仍参与恢复。
                if !input.text.is_empty() && !resolved_inputs.contains(&input.input_id) {
                    input.text = String::from("[Cleaned history]");
                }
                event.visibility = EventVisibility::HostOnly;
            }
            EventBody::AssistantReasoning(body) => {
                if selected && !summaries.contains(&event.seq) {
                    body.text.clear();
                    event.visibility = EventVisibility::HostOnly;
                }
            }
            EventBody::AssistantText(body) => {
                if selected && !summaries.contains(&event.seq) {
                    body.text.clear();
                    body.code_snapshots.clear();
                    event.visibility = EventVisibility::HostOnly;
                }
            }
            EventBody::ToolCall(tool) => {
                // Collaboration wait receipts are read by recovery, so their typed payload is retained.
                // 恢复会读取协作等待回执，因此保留其强类型载荷。
                if selected && tool.tool_name != "wait_question" {
                    tool.input = ToolPayload::empty();
                    tool.output = tool.output.as_ref().map(|_| ToolPayload::empty());
                    tool.changeset = None;
                    tool.generated_resources.clear();
                    tool.external_refs.clear();
                    event.visibility = EventVisibility::HostOnly;
                }
            }
            EventBody::DisplayNote(_) | EventBody::MemoryRecall(_) if selected => {
                event.visibility = EventVisibility::HostOnly;
            }
            _ => {}
        }
    }
    // Obsolete envelopes before the retained request are distinct from the original facts they authenticate.
    // 保留请求之前的过期信封与它们所验证的原始事件事实分别处理。
    let retained_turn_start = complete_turn_start(&events, node.retained_from_seq)?;
    retire_invalid_inactive_sources(document, &mut events, from, through, retained_turn_start)?;
    // The latest compaction node is guaranteed by the admission and remains unchanged.
    // 准入保证最新压缩节点存在且保持原样。
    // Compare the same projection that the cold runtime uses at the committed frontier.
    // 对比冷启动运行时在已提交前沿使用的同一投影。
    let before = project_context_generation_items_with_sources(
        document.events(),
        node.retained_from_seq,
        Some(node.api_stats_seq),
    )?;
    // Compare complete sourced items, including replay envelopes and media parameters.
    // 比较完整带来源条目，包含重放信封及媒体参数。
    let after = project_context_generation_items_with_sources(
        &events,
        node.retained_from_seq,
        Some(node.api_stats_seq),
    )?;
    if before != after {
        return Err(DomainError::InvalidGenerationInput {
            field: "history_cleanup.request_projection_changed",
        });
    }
    SessionDocument::from_parts(
        document.metadata.clone(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        events.clone(),
    )?;
    Ok(HistoryCleanupCandidate {
        from,
        history,
        events,
    })
}

/// Retires invalid inactive replay envelopes after a display payload is removed.
/// 在展示载荷移除后退役失效的非活动重放信封。
fn retire_invalid_inactive_sources(
    document: &SessionDocument,
    events: &mut [ConversationEvent],
    from: Seq,
    through: Seq,
    retained_turn_start: Seq,
) -> Result<(), DomainError> {
    // Record only inactive envelopes that the current request cannot project.
    // 只记录当前请求不可能投影的非活动信封。
    let inactive: Vec<_> = events
        .iter()
        .enumerate()
        .take_while(|(_, event)| event.seq < retained_turn_start)
        .filter_map(|(index, event)| {
            // The event variant determines whether it can host replay evidence.
            // 事件类型决定是否可以承载重放证据。
            let source = match &event.event {
                EventBody::AssistantReasoning(body) => body.provider_source.as_ref(),
                EventBody::AssistantText(body) => body.provider_source.as_ref(),
                EventBody::ToolCall(body) => body.provider_source.as_ref(),
                _ => None,
            };
            source
                .filter(|source| {
                    source.covered_until_seq >= from || event.seq <= through && event.seq >= from
                })
                .map(|source| (index, source, (from..=through).contains(&event.seq)))
        })
        .collect();
    // Reuse the ordinary batch digest validator to preserve proofs that still verify.
    // 复用普通批量摘要校验器，保留仍然有效的证明。
    let sources: Vec<_> = inactive.iter().map(|(_, source, _)| *source).collect();
    // Original segment starts remain authoritative.
    // 原分段起点仍是权威来源。
    let starts: Vec<_> = document
        .segment_catalog()
        .iter()
        .map(|entry| entry.start_seq)
        .collect();
    // Compute all mismatch decisions before modifying an envelope.
    // 在修改信封前完成所有不匹配判断。
    let invalid = super::projection::provider_fingerprint_mismatches(events, &starts, &sources)?;
    // Proof envelopes are not hashed by another proof, so retiring these cannot invalidate another digest.
    // 其他证明不会哈希证明信封，因此退役这些信封不会使其他摘要失效。
    let indices: Vec<_> = inactive
        .into_iter()
        .zip(invalid)
        .filter_map(|((index, _, selected), mismatch)| {
            (selected || mismatch.is_some()).then_some(index)
        })
        .collect();
    for index in indices {
        match &mut events[index].event {
            EventBody::AssistantReasoning(body) => body.provider_source = None,
            EventBody::AssistantText(body) => body.provider_source = None,
            EventBody::ToolCall(body) => body.provider_source = None,
            _ => unreachable!("only replay hosts were selected"),
        }
    }
    Ok(())
}

/// Returns the completed final business answer from this turn's exact API group.
/// 从本回合的精确 API 组取出已完成的业务最终答复。
fn final_answer(events: &[&ConversationEvent]) -> Option<String> {
    // Compaction and auxiliary question executions cannot supply the business answer.
    // 压缩和辅助答疑执行不能作为业务最终答复。
    let ApiStatsEvent {
        api_call_id,
        status,
        ..
    } = events.iter().rev().find_map(|event| match &event.event {
        EventBody::ApiStats(stats)
            if !matches!(
                stats.purpose,
                ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution
            ) =>
        {
            Some(stats)
        }
        _ => None,
    })?;
    if *status != ApiCallStatus::Completed
        || events.iter().any(|event| matches!(&event.event, EventBody::ToolCall(tool) if tool.api_call_id == *api_call_id))
    {
        return None;
    }
    // The stream already orders completed text fragments exactly.
    // 事件流已经按顺序记录已完成的正文片段。
    let mut parts = events.iter().filter_map(|event| match &event.event {
        EventBody::AssistantText(text)
            if text.api_call_id == *api_call_id
                && text.status == AssistantSemanticStatus::Completed =>
        {
            Some(text.text.as_str())
        }
        _ => None,
    });
    // Require a recorded text fragment even if the response text was empty.
    // 即使答复正文为空也要求存在已记录的正文片段。
    let mut answer = parts.next()?.to_owned();
    answer.extend(parts);
    Some(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resource extractor keeps image/video IDs, preview and prompt while dropping raw payloads.
    /// 资源提取保留图片／视频标识、预览和提示词，同时省略原始载荷。
    #[test]
    fn extracts_completed_media_without_copying_tool_payload()
    -> Result<(), Box<dyn std::error::Error>> {
        // Build the exact persisted tool shape with extra arguments and several resource kinds.
        // 构建带额外参数与多种资源类型的精确持久工具结构。
        let mut tool: super::super::event::ToolCallEvent = serde_json::from_value(
            serde_json::json!({
                "api_call_id": 1, "tool_call_id": 2, "request_stream_id": "media-test",
                "tool_name": "image_generate", "input": {"prompt": "Draw a tree", "quality": "extra"},
                "output": {"provider_payload": "drop this"}, "status": "completed",
                "started_at_ms": 1, "finished_at_ms": 2,
                "generated_resources": [
                    {"kind": "image", "resource_id": 3, "preview_resource_id": 4},
                    {"kind": "file", "resource_id": 5}
                ]
            }),
        )?;
        // Link extracted resources to an independent display owner.
        // 将提取的资源关联到独立展示所有者。
        let owner = Uuid::new_v4();
        // Materialize the same extractor used during cleanup.
        // 物化清理中使用的同一提取器。
        let references: Vec<_> = extract_tool_resources(&tool, owner, 2).collect();
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].history_turn_id, Some(owner));
        assert_eq!(references[0].resource_id, ResourceId::try_new(3)?);
        assert_eq!(
            references[0].preview_resource_id,
            Some(ResourceId::try_new(4)?)
        );
        assert_eq!(references[0].prompt.as_deref(), Some("Draw a tree"));
        // The display encoding must omit the tool argument and output envelopes.
        // 展示编码必须省略工具参数和输出信封。
        let encoded = serde_json::to_value(&references[0])?;
        assert!(encoded.get("input").is_none() && encoded.get("output").is_none());
        tool.status = ToolCallStatus::Failed;
        assert_eq!(extract_tool_resources(&tool, owner, 2).count(), 0);
        Ok(())
    }

    /// A resource survives deletion of its owning turn and can then be selected on its own.
    /// 删除所属回合后，资源仍存在且可以继续单独选中删除。
    #[test]
    fn deletes_independent_entries_without_removing_unselected_resources() -> Result<(), DomainError>
    {
        // Independent IDs intentionally share no relation to the runtime sequence.
        // 独立标识与运行时序号无关。
        let turn = Uuid::new_v4();
        // The answer can be selected separately from its turn.
        // 答复可以独立于回合选择。
        let answer = Uuid::new_v4();
        // Resource selection addresses the index record, not the underlying catalog ID.
        // 资源选择定位索引条目，而不是底层目录标识。
        let resource = Uuid::new_v4();
        // Build a minimal display-only document with one detachable asset.
        // 构建带一个可分离素材的最小展示文档。
        let mut history = CleanedHistory {
            turns: vec![HistoryTurn {
                id: turn,
                closed_at_ms: 1,
                outcome: TurnOutcome::Completed,
                inputs: Vec::new(),
                answer: Some(HistoryAnswer {
                    id: answer,
                    text: String::from("Final"),
                }),
            }],
            resources: vec![HistoryResource {
                id: resource,
                history_turn_id: Some(turn),
                resource_id: ResourceId::try_new(1)?,
                preview_resource_id: None,
                kind: GeneratedResourceKind::Image,
                created_at_ms: 1,
                prompt: Some(String::from("Draw a tree")),
            }],
        };
        assert_eq!(history.delete(&BTreeSet::from([answer])), 1);
        assert_eq!(history.delete(&BTreeSet::from([answer, turn])), 1);
        assert!(history.turns.is_empty());
        assert_eq!(history.resources[0].history_turn_id, None);
        history.validate()?;
        assert_eq!(history.delete(&BTreeSet::from([resource])), 1);
        assert_eq!(history, CleanedHistory::default());
        Ok(())
    }
}
