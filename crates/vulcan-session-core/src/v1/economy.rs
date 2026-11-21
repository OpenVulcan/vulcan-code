//! Settled-turn projections and response-tail parsing for economy sessions.
//! 节约会话的已结算回合投影与回答末尾协议解析。

use super::ids::{ApiCallId, CompactionId, Seq, TurnId};
use super::projection::{ProjectedItem, SourcedProjectedItem};
use serde::{Deserialize, Serialize};
use vulcan_ai_contract::economy_window::EconomyWindow;

/// Preferences and receipts are serialized with the owning session revision.
/// 偏好与回执随所属会话的修订一同保存。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EconomyState {
    /// User choice for subsequent root turns.
    /// 后续根回合采用的用户选择。
    pub enabled: bool,
    /// Serializes competing preference updates and invalidates old candidates.
    /// 串行化竞争的设置更新，并使旧候选失效。
    pub revision: u64,
    /// An enable operation cannot adopt a turn already running at the switch.
    /// 启用操作不能接管切换时已经运行的回合。
    pub deferred_turn: Option<TurnId>,
    /// The first eligible turn must establish a historical baseline.
    /// 首个符合条件的回合须建立历史基线。
    pub initialize: bool,
    /// Frozen owner, history baseline and window for one executing turn.
    /// 冻结当前执行回合的归属、历史基线及窗口。
    pub active: Option<EconomyTurn>,
    /// Immutable settlement receipts in publication order.
    /// 按发布顺序保存的结算回执。
    pub settled: Vec<SettledEconomyTurn>,
}

/// Execution snapshot established before preparing a new root request.
/// 准备新的根请求之前创建的执行快照。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EconomyTurn {
    /// Stable business turn identifier.
    /// 稳定的业务回合标识。
    pub turn_id: TurnId,
    /// First durable event belonging to this business turn.
    /// 此业务回合的首个持久事件。
    pub start: Seq,
    /// Selected compaction before the turn's temporary execution compactions.
    /// 本轮临时执行压缩之前选定的压缩节点。
    pub baseline: Option<CompactionId>,
    /// Trigger and batch sizes frozen for this turn.
    /// 本轮固定的触发值与批量大小。
    pub window: EconomyWindow,
}

/// A closed user turn that can be replayed from its original input and final answer.
/// 可从原始输入与最终答复重放的已闭合用户回合。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettledEconomyTurn {
    /// Business owner and the exact historical baseline it inherited.
    /// 业务归属及其继承的精确历史基线。
    pub turn: EconomyTurn,
    /// Source sequence of the completed assistant text.
    /// 最终助手正文的来源序号。
    pub answer: Seq,
    /// Physical generation request and its immutable source coverage at dispatch.
    /// 实际生成最终回答的物理请求及其派发时的不可变来源范围。
    pub api: ApiCallId,
    /// Exact request proof persisted with the API that produced the final answer.
    /// 与最终答复所属请求一并持久化的精确投影证明。
    pub projection: super::event::ProviderProjectionProof,
    /// Selection revision that admitted this response for settlement.
    /// 准许本次响应结算的偏好修订号。
    pub selection_revision: u64,
    /// Last physical compaction observed at settlement.
    /// 结算时最后观察到的物理压缩节点。
    pub physical_tip: Option<CompactionId>,
    /// Assistant-authored summary, absent for simple no-tool exchanges.
    /// 助手生成的摘要，简单无工具问答为空。
    pub summary: Option<String>,
    /// Sequence of the later settlement that evicted this turn.
    /// 将此回合移出窗口的后继结算序号。
    pub retired_at: Option<Seq>,
    /// Preserve referenced user attachments and returned artifacts during batch trimming.
    /// 批量裁剪时保留用户附件与返回产物所引用的回合。
    pub protected: bool,
    /// Stable cause for retaining this turn verbatim, available to local diagnostics.
    /// 本地诊断可读的保留本轮原文原因标识。
    pub retained_reason: Option<String>,
}

/// Private handoff from the successful physical terminal to logical settlement.
/// 从成功物理终态传递到逻辑结算的私有交接信息。
#[derive(Clone, Debug)]
pub struct EconomyCandidate {
    /// Physical response that produced the handoff.
    /// 生成交接信息的物理响应。
    pub api: ApiCallId,
    /// Preference revision frozen at dispatch.
    /// 派发时冻结的偏好修订。
    pub revision: u64,
    /// Optional validated summary text.
    /// 可选的已校验摘要正文。
    pub summary: Option<String>,
}

/// Stable key namespace for model-visible handoffs, including global compaction input.
/// 模型可见交接信息的稳定键前缀，全局压缩输入也使用此键。
pub const SUMMARY_KEY_PREFIX: &str = "economy.turn.";

/// Maximum UTF-8 bytes per handoff; oversized output stays in the ordinary history.
/// 每份交接摘要的 UTF-8 字节上限；超限输出沿用普通历史。
const MAX_SUMMARY_BYTES: usize = 4096;

/// Opening delimiter reserved on a separate line at the end of the answer.
/// 保留给答复末尾独立行的开始标记。
const OPEN: &str = "<SUMMARY_INFO>";
/// Matching terminal delimiter.
/// 配对的结束标记。
const CLOSE: &str = "</SUMMARY_INFO>";

/// Finds an unfenced standalone opening delimiter or its partial streaming prefix.
/// 查找代码围栏外独立的开始标记或流式输出中的标记前缀。
fn tail_start(text: &str) -> Option<usize> {
    let mut fence: Option<(char, usize)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let raw = line.trim_end_matches(['\n', '\r']);
        let trimmed = raw.trim_start();
        if let Some(marker @ ('`' | '~')) = trimmed.chars().next() {
            let length = trimmed.chars().take_while(|c| *c == marker).count();
            if length >= 3 {
                match fence {
                    None => fence = Some((marker, length)),
                    Some((old, size))
                        if old == marker
                            && length >= size
                            && trimmed[length..].trim().is_empty() =>
                    {
                        fence = None
                    }
                    _ => {}
                }
            }
        } else if fence.is_none()
            && !raw.is_empty()
            && (raw.starts_with(OPEN) || (!line.ends_with('\n') && OPEN.starts_with(raw)))
        {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// Returns text safe to stream while a potential protocol tail is pending.
/// 返回候选协议尾段待定时可流式展示的正文。
pub fn streaming_body(text: &str) -> &str {
    &text[..tail_start(text).unwrap_or(text.len())]
}

/// Separates one valid terminal summary, preserving exact visible answer bytes.
/// 拆分唯一有效的末尾摘要，保留可见答复的精确字节。
pub fn split_summary(text: &str) -> Option<(&str, &str)> {
    let start = tail_start(text)?;
    let summary = text[start..]
        .trim_end()
        .strip_prefix(OPEN)?
        .strip_suffix(CLOSE)?
        .trim();
    (!text[..start].trim().is_empty()
        && !summary.is_empty()
        && summary.len() <= MAX_SUMMARY_BYTES
        && !summary.contains(OPEN)
        && !summary.contains(CLOSE))
    .then_some((&text[..start], summary))
}

impl EconomyState {
    /// Rejects malformed receipt ordering and inconsistent state before replaying persisted history.
    /// 在重放持久历史前拒绝次序损坏的回执和不一致的模式状态。
    pub fn validate(&self) -> Result<(), super::DomainError> {
        let valid = self.revision <= super::ids::JS_SAFE_INTEGER_MAX
            && (self.enabled || (!self.initialize && self.active.is_none()))
            && self.active.as_ref().is_none_or(|turn| {
                turn.window.validate().is_ok()
                    && self
                        .settled
                        .last()
                        .is_none_or(|last| last.answer < turn.start)
            })
            && self
                .settled
                .windows(2)
                .all(|pair| pair[0].answer < pair[1].turn.start)
            && self.settled.iter().all(|r| {
                r.turn.window.validate().is_ok()
                    && r.turn.start <= r.projection.through_seq
                    && r.projection.through_seq < r.answer
                    && r.projection.retained_from_seq <= r.projection.through_seq
                    && r.projection.fingerprint_sha256.len() == 64
                    && r.projection
                        .fingerprint_sha256
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit())
                    && r.selection_revision <= self.revision
                    && r.retired_at.is_none_or(|at| at > r.answer)
                    && r.summary
                        .as_ref()
                        .is_none_or(|s| !s.trim().is_empty() && s.len() <= MAX_SUMMARY_BYTES)
                    && (r.retained_reason.is_none() || r.summary.is_none())
            });
        if !valid {
            return Err(super::DomainError::InvalidSelectionConfiguration {
                field: "economy.receipts",
            });
        }
        Ok(())
    }

    /// Maps the physical tip to the baseline restored by its latest settlement.
    /// 把物理末端映射到最近一次结算恢复的基线。
    pub fn visible_compaction(&self, physical: Option<CompactionId>) -> Option<CompactionId> {
        match self.settled.last() {
            Some(receipt) if receipt.physical_tip == physical => receipt.turn.baseline,
            _ => physical,
        }
    }

    /// Returns the last publication that could shift an already anchored provider message.
    /// 返回最近一次可能移动已锚定供应商消息位置的发布序号。
    pub fn position_revision(&self) -> Option<u64> {
        self.settled
            .iter()
            .flat_map(|receipt| {
                let changed =
                    receipt.summary.is_some() || receipt.physical_tip != receipt.turn.baseline;
                [changed.then_some(receipt.answer), receipt.retired_at]
                    .into_iter()
                    .flatten()
            })
            .max()
            .map(Seq::value)
    }

    /// Rewinds receipts and batch evictions when editing or forking at a durable sequence.
    /// 在某个持久序号编辑或分叉时回退已结算回执和批量裁剪。
    pub fn rewind(&mut self, target: Seq) {
        self.active = None;
        self.deferred_turn = None;
        self.settled.retain(|r| r.answer < target);
        if self.enabled && self.settled.is_empty() {
            self.initialize = true;
        }
        for receipt in &mut self.settled {
            if receipt.retired_at.is_some_and(|seq| seq >= target) {
                receipt.retired_at = None;
            }
        }
    }

    /// Projects closed turns in request order; the active turn uses its normal execution history.
    /// 依请求顺序投影已闭合回合，活动回合沿用正常执行历史。
    pub fn project(
        &self,
        items: &mut Vec<SourcedProjectedItem>,
        baseline_summary: Option<Seq>,
        events: &[super::event::ConversationEvent],
    ) {
        if self.settled.is_empty() {
            return;
        }
        let mut projected = Vec::with_capacity(items.len());
        for mut entry in items.drain(..) {
            let index = self
                .settled
                .partition_point(|receipt| receipt.answer < entry.source_from_seq);
            let Some(receipt) = self
                .settled
                .get(index)
                .filter(|receipt| receipt.turn.start <= entry.source_from_seq)
            else {
                projected.push(entry);
                continue;
            };
            // Interleaved work keeps its own owner. Only the current task's temporary maintenance is replaced.
            // 交错任务保留独立归属，仅替换当前任务执行期间的临时压缩。
            let source = events
                .binary_search_by_key(&entry.source_from_seq, |e| e.seq)
                .ok()
                .map(|i| &events[i]);
            if source.is_none_or(|e| matches!(e.event, super::event::EventBody::SystemContext(_)))
                || baseline_summary == Some(entry.source_to_seq)
                || matches!(entry.item, ProjectedItem::SystemText { .. })
            {
                projected.push(entry);
                continue;
            }
            if let Some(event) = source
                && event.turn_id != Some(receipt.turn.turn_id)
            {
                let maintenance_api = match &event.event {
                    super::event::EventBody::AssistantReasoning(reasoning) => {
                        Some(reasoning.api_call_id)
                    }
                    super::event::EventBody::AssistantText(text) => Some(text.api_call_id),
                    _ => None,
                };
                if !maintenance_api.is_some_and(|api| events.iter().any(|e| matches!(&e.event,
                    super::event::EventBody::ApiStarted(a) if a.api_call_id == api && a.purpose == super::event::ApiPurposeKind::Compaction))) {
                    projected.push(entry);
                }
                continue;
            }
            if receipt.retired_at.is_some() {
                continue;
            }
            if receipt.retained_reason.is_some() {
                projected.push(entry);
                continue;
            }
            match &mut entry.item {
                ProjectedItem::UserInput { resource_ids, .. } if !resource_ids.is_empty() || !source.is_some_and(|e| matches!(&e.event, super::event::EventBody::UserInput(u) if u.input_source == super::event::UserInputSource::AgentMessage)) => projected.push(entry),
                ProjectedItem::AssistantText {
                    reasoning,
                    tool_calls,
                    provider_source,
                    ..
                } if entry.source_to_seq >= receipt.answer && tool_calls.is_empty() => {
                    *reasoning = None;
                    *provider_source = None;
                    if let Some(summary) = &receipt.summary {
                        let body = summary
                            .replace('&', "&amp;")
                            .replace('<', "&lt;")
                            .replace('>', "&gt;");
                        projected.push(SourcedProjectedItem {
                            source_from_seq: receipt.answer,
                            source_to_seq: receipt.answer,
                            user_prompt_parameters: None,
                            item: ProjectedItem::SystemText {
                                role: super::projection::ProjectedSystemRole::HiddenUserShell,
                                context_key: format!("{SUMMARY_KEY_PREFIX}{}", receipt.turn.turn_id.value()),
                                text: format!("<vulcan_turn_summary turn_id=\"{}\">\n{body}\n</vulcan_turn_summary>", receipt.turn.turn_id.value()),
                            },
                        });
                    }
                    projected.push(entry);
                }
                ProjectedItem::AssistantText {
                    reasoning,
                    provider_source,
                    tool_calls,
                    ..
                } if tool_calls.iter().all(|t| t.tool_name == "ask") && !tool_calls.is_empty() => {
                    *reasoning = None;
                    *provider_source = None;
                    projected.push(entry);
                }
                ProjectedItem::ToolResult { tool_name, .. } if tool_name == "ask" => {
                    projected.push(entry)
                }
                _ => {}
            }
        }
        *items = projected;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chunk boundary inside a delimiter never exposes private protocol text.
    /// 流式分块落在标记内部时不会暴露私有协议正文。
    #[test]
    fn summary_suffix_and_code_examples() {
        let input = "Answer\n<SUMMARY_INFO>handoff</SUMMARY_INFO>";
        for end in 7..=input.len() {
            assert_eq!(streaming_body(&input[..end]), "Answer\n");
        }
        assert_eq!(split_summary(input), Some(("Answer\n", "handoff")));
        assert!(split_summary("```xml\n<SUMMARY_INFO>example</SUMMARY_INFO>\n```").is_none());
        assert!(split_summary("A\n<SUMMARY_INFO>partial").is_none());
        assert!(
            split_summary("A\n<SUMMARY_INFO>one</SUMMARY_INFO>\n<SUMMARY_INFO>two</SUMMARY_INFO>")
                .is_none()
        );
    }
}
