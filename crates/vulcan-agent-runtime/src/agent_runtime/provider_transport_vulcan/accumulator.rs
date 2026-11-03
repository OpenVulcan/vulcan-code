//! Ordered public response accumulation, replay semantics, and usage projection.
//! 有序公共响应累积、重放语义及用量投影。

use super::{provider_failure, public_execution_failure};
use crate::agent_runtime::events::{TransientReasoningStatus, TransientReasoningStatusKind};
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::tool_request_stream::ToolRequestFragment;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use vulcan_ai_contract::{
    AiStreamEvent, ContentItem, ProviderMessagePhaseSnapshot, ProviderReplayEnvelope, RequestUsage,
    ResourceKind, ResourceReference, ToolCall as PublicToolCall, UsageValue,
    is_replayable_codex_responses_item,
};
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ConversationToolCall, CoreError as VulcanCoreError, ProviderContinuation, ProviderTurnBoundary,
    ProviderTurnDelta, ProviderTurnResponse as CoreProviderTurnResponse, ProviderUsageSnapshot,
    TransientText,
};
use vulcan_provider_adapters::{ResponseProjectionProfile, TransientReasoningStatusMode};

/// Resolves the single transient reasoning mode frozen into one response-profile chain.
/// 解析冻结在单条响应 Profile 链中的唯一瞬态推理模式。
///
/// # Parameters
/// 参数
/// `provider_id` identifies controlled failures and `profiles` is the exact frozen chain.
/// `provider_id` 标识受控失败，`profiles` 是精确冻结的 Profile 链。
///
/// # Returns
/// 返回值
/// The configured runtime status kind, or an error when multiple profiles claim the channel.
/// 返回已配置的运行时状态类型；多个 Profile 同时声明该通道时返回错误。
pub(super) fn transient_reasoning_status_kind(
    provider_id: &str,
    profiles: &[ResponseProjectionProfile],
) -> Result<Option<TransientReasoningStatusKind>, VulcanCoreError> {
    // configured iterates only profiles that explicitly claim the transient status channel.
    // configured 只遍历显式声明瞬态状态通道的 Profile。
    let mut configured = profiles
        .iter()
        .filter_map(|profile| profile.transient_reasoning_status);
    // mode is the sole frozen presentation kind accepted for this request.
    // mode 是当前请求唯一接受的冻结展示类型。
    let Some(mode) = configured.next() else {
        return Ok(None);
    };
    if configured.next().is_some() {
        return Err(provider_failure(
            provider_id,
            "Multiple response profiles declared the transient reasoning status channel.",
        ));
    }
    Ok(Some(match mode {
        TransientReasoningStatusMode::Encrypted => TransientReasoningStatusKind::Encrypted,
    }))
}

/// Accumulates public stream events into the controller's completed-turn shape.
/// 将公共流事件累积为控制器的完整回合结构。
#[derive(Debug)]
pub(super) struct PublicTurnAccumulator {
    /// Exact provider definition scope for durable replay.
    /// 可持久化重放使用的精确供应商定义作用域。
    provider_id: String,
    /// Exact credential connection scope for durable replay.
    /// 可持久化重放使用的精确凭据连接作用域。
    connection_id: String,
    /// Exact non-secret resolved request URL for durable replay.
    /// 可持久化重放使用的精确不含秘密的已解析请求 URL。
    route_fingerprint: String,
    /// Exact protocol profile scope for provider-specific replay signatures.
    /// 供应商专属重放签名使用的精确协议 Profile 作用域。
    protocol_profile_id: String,
    /// Exact model scope for durable replay.
    /// 可持久化重放使用的精确模型作用域。
    model_id: String,
    /// Flattened visible assistant output.
    /// 扁平化的可见助手输出。
    text: String,
    /// Flattened reasoning output.
    /// 扁平化的推理输出。
    reasoning: String,
    /// Latest provider response or assistant-message identifier.
    /// 最新供应商响应或助手消息标识。
    provider_message_id: Option<String>,
    /// Complete tool calls.
    /// 完整工具调用。
    tool_calls: Vec<ConversationToolCall>,
    /// Argument text streamed so far, keyed by tool item, for the permission judge.
    /// 按工具条目索引的已流式参数文本，供权限判定使用。
    raw_tool_arguments: BTreeMap<String, String>,
    /// Last public usage update.
    /// 最后一个公共用量更新。
    pub(super) usage: Option<RequestUsage>,
    /// Stream item lookup keyed by stable public item identifiers.
    /// 按稳定公共项目标识索引的流项目查找表。
    replay_item_indices: BTreeMap<String, usize>,
    /// Assistant items retained in original stream order.
    /// 按原始流顺序保留的助手项目。
    replay_items: Vec<ReplayAccumulatorItem>,
    /// Exact provider output items retained in completed-event order.
    /// 按完成事件顺序保留的原始供应商输出项。
    provider_context_items: Vec<Value>,
    /// Typed message phases retained independently from opaque provider context.
    /// 与不透明供应商上下文独立保存的类型化消息阶段。
    message_phases: Vec<ProviderMessagePhaseSnapshot>,
    /// Explicit logical-turn boundary supplied by the provider completion event.
    /// 供应商完成事件提供的显式逻辑回合边界。
    end_turn: Option<bool>,
    /// Whether a successful terminal response event was observed.
    /// 是否已经观察到成功响应终态事件。
    completed: bool,
    /// Replace-in-place reasoning status excluded from every durable response field.
    /// 排除在全部持久化响应字段之外的原地替换式推理状态。
    transient_reasoning_status: Option<TransientReasoningStatus>,
    /// Whether non-empty durable reasoning has superseded every transient fallback.
    /// 非空持久化思考是否已经取代全部瞬态回退状态。
    durable_reasoning_observed: bool,
}

/// Stores one in-progress assistant item for deterministic replay assembly.
/// 为确定性重放组装保存一个进行中的助手项目。
#[derive(Debug)]
struct ReplayAccumulatorItem {
    /// Stable public stream item identifier.
    /// 稳定公共流项目标识。
    item_id: String,
    /// Current semantic item state.
    /// 当前语义项目状态。
    state: ReplayAccumulatorItemState,
}

/// Represents the closed set of assistant item states assembled from stream events.
/// 表示从流事件组装出的封闭助手项目状态集合。
#[derive(Debug)]
enum ReplayAccumulatorItemState {
    /// Incrementally assembled visible text.
    /// 增量组装的可见文本。
    Text(String),
    /// Incrementally assembled reasoning and its optional replay signature.
    /// 增量组装的推理及其可选重放签名。
    Reasoning {
        /// Visible reasoning summary or thinking text.
        /// 可见推理摘要或思考文本。
        text: String,
        /// Provider-specific opaque continuation signature.
        /// 供应商专属不透明延续签名。
        signature: Option<String>,
    },
    /// Complete non-incremental public content.
    /// 完整的非增量公共内容。
    Complete(ContentItem),
    /// Tool identity is known but its arguments are still incomplete.
    /// 工具身份已知但参数仍不完整。
    PendingTool {
        /// Provider-visible call identifier required to pair a controlled rejection result.
        /// 用于配对受控拒绝结果的 Provider 可见调用标识。
        call_id: String,
        /// Exact tool name selected by the model before argument parsing completed.
        /// 参数解析完成前模型选定的精确工具名称。
        tool_name: String,
    },
}

/// Returns whether captured Responses items fully cover the provider-neutral assistant replay.
/// 返回捕获的 Responses 项是否完整覆盖供应商无关的助手重放内容。
///
/// # Parameters
/// 参数
/// `content` is the complete normalized assistant output and `provider_context_items` is the
/// ordered provider-owned output-item list observed from completion events.
/// `content` 是完整的规范助手输出；`provider_context_items` 是从完成事件观察到的有序供应商输出项列表。
///
/// # Returns
/// 返回值
/// `true` only when raw replay cannot hide any normalized text, reasoning, tool call, or resource.
/// 仅当原始重放不会隐藏任何规范文本、推理、工具调用或资源时返回 `true`。
fn provider_context_items_cover_replay_content(
    content: &[ContentItem],
    provider_context_items: &[Value],
) -> bool {
    // normalizedCounts describe every semantic category that would otherwise be re-encoded.
    // normalizedCounts 描述原本需要重新编码的每一种语义类别。
    let normalized_message_count = content
        .iter()
        .filter(|item| {
            matches!(
                item,
                ContentItem::Text { .. }
                    | ContentItem::StructuredJson { .. }
                    | ContentItem::Refusal { .. }
            )
        })
        .count();
    let normalized_tool_count = content
        .iter()
        .filter(|item| matches!(item, ContentItem::ToolCall { .. }))
        .count();
    let normalized_resource_count = content
        .iter()
        .filter(|item| matches!(item, ContentItem::Resource { .. }))
        .count();
    let normalized_has_reasoning = content
        .iter()
        .any(|item| matches!(item, ContentItem::Reasoning { .. }));
    if content.iter().any(|item| {
        matches!(
            item,
            ContentItem::ToolResult { .. } | ContentItem::Citation { .. }
        )
    }) {
        return false;
    }

    // providerCounts reflect exact output-item variants accepted by Responses history input.
    // providerCounts 反映 Responses 历史输入接受的精确输出项变体。
    let provider_message_count = provider_context_items
        .iter()
        .filter(|item| {
            matches!(
                item.get("type").and_then(Value::as_str),
                Some("message" | "agent_message")
            )
        })
        .count();
    let provider_tool_count = provider_context_items
        .iter()
        .filter(|item| {
            matches!(
                item.get("type").and_then(Value::as_str),
                Some("function_call" | "custom_tool_call")
            )
        })
        .count();
    let provider_resource_count = provider_context_items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("image_generation_call"))
        .count();
    let provider_has_reasoning = provider_context_items
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("reasoning"));

    provider_message_count >= normalized_message_count
        && provider_tool_count >= normalized_tool_count
        && provider_resource_count >= normalized_resource_count
        && (!normalized_has_reasoning || provider_has_reasoning)
}

impl PublicTurnAccumulator {
    /// Creates one accumulator bound to an immutable execution route.
    /// 创建一个绑定到不可变执行路由的累积器。
    ///
    /// # Parameters
    /// 参数
    /// The identifiers are the provider definition, credential connection, resolved request URL,
    /// protocol profile, and model selected for this physical request.
    /// 各标识分别是本次物理请求选定的供应商定义、凭据连接、已解析请求 URL、协议 Profile 与模型。
    ///
    /// # Returns
    /// 返回值
    /// A new empty turn accumulator.
    /// 返回一个新的空回合累积器。
    pub(super) fn new(
        provider_id: &str,
        connection_id: &str,
        route_fingerprint: &str,
        protocol_profile_id: &str,
        model_id: &str,
        transient_reasoning_kind: Option<TransientReasoningStatusKind>,
    ) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            connection_id: connection_id.to_string(),
            route_fingerprint: route_fingerprint.to_string(),
            protocol_profile_id: protocol_profile_id.to_string(),
            model_id: model_id.to_string(),
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: None,
            tool_calls: Vec::new(),
            raw_tool_arguments: BTreeMap::new(),
            usage: None,
            replay_item_indices: BTreeMap::new(),
            replay_items: Vec::new(),
            provider_context_items: Vec::new(),
            message_phases: Vec::new(),
            end_turn: None,
            completed: false,
            durable_reasoning_observed: false,
            transient_reasoning_status: transient_reasoning_kind.map(|kind| {
                TransientReasoningStatus {
                    kind,
                    reasoning_tokens: None,
                }
            }),
        }
    }

    /// Emits the latest transient reasoning status without touching semantic state.
    /// 发送最新瞬态推理状态，且不触碰任何语义状态。
    ///
    /// # Parameters
    /// 参数
    /// `live_sender` is the request-local WebUI delta channel.
    /// `live_sender` 是当前请求局部的 WebUI 增量通道。
    fn emit_transient_reasoning_status(&self, live_sender: &ProviderLiveDeltaSender) {
        let _ = live_sender.send(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(
            self.transient_reasoning_status.clone(),
        )));
    }

    /// Clears the transient reasoning collection without recording a transcript delta.
    /// 清除瞬态推理状态集合，且不记录转录增量。
    ///
    /// # Parameters
    /// 参数
    /// `live_sender` is the request-local WebUI delta channel.
    /// `live_sender` 是当前请求局部的 WebUI 增量通道。
    fn clear_transient_reasoning_status(&mut self, live_sender: &ProviderLiveDeltaSender) {
        if self.transient_reasoning_status.take().is_some() {
            let _ = live_sender.send(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(
                None,
            )));
        }
    }

    /// Registers one stream item exactly once while preserving arrival order.
    /// 精确注册一次流项目并保留到达顺序。
    ///
    /// # Parameters
    /// 参数
    /// `item_id` is the stable public identifier and `state` is its initial semantic state.
    /// `item_id` 是稳定公共标识，`state` 是其初始语义状态。
    ///
    /// # Returns
    /// 返回值
    /// Success when the identifier is new; duplicate identifiers are controlled failures.
    /// 标识为新值时成功；重复标识会产生受控失败。
    fn register_replay_item(
        &mut self,
        item_id: String,
        state: ReplayAccumulatorItemState,
    ) -> Result<(), VulcanCoreError> {
        if self.replay_item_indices.contains_key(item_id.as_str()) {
            return Err(provider_failure(
                self.provider_id.as_str(),
                format!("Public stream item `{item_id}` started more than once.").as_str(),
            ));
        }
        let index = self.replay_items.len();
        self.replay_item_indices.insert(item_id.clone(), index);
        self.replay_items
            .push(ReplayAccumulatorItem { item_id, state });
        Ok(())
    }

    /// Resolves the mutable semantic state for one previously started item.
    /// 解析一个已启动项目的可变语义状态。
    ///
    /// # Parameters
    /// 参数
    /// `item_id` is the stable identifier from a delta or completion event.
    /// `item_id` 是增量或完成事件携带的稳定标识。
    ///
    /// # Returns
    /// 返回值
    /// The exact registered state, or a controlled ordering failure.
    /// 返回精确注册状态；事件顺序无效时返回受控失败。
    fn replay_item_state_mut(
        &mut self,
        item_id: &str,
    ) -> Result<&mut ReplayAccumulatorItemState, VulcanCoreError> {
        let index = self
            .replay_item_indices
            .get(item_id)
            .copied()
            .ok_or_else(|| {
                provider_failure(
                    self.provider_id.as_str(),
                    format!("Public stream item `{item_id}` was updated before it started.")
                        .as_str(),
                )
            })?;
        Ok(&mut self.replay_items[index].state)
    }

    /// Materializes all completed assistant items in original stream order.
    /// 按原始流顺序实体化所有已完成助手项目。
    ///
    /// # Returns
    /// 返回值
    /// Ordered public content, or a controlled failure when a tool remains incomplete.
    /// 返回有序公共内容；工具仍不完整时返回受控失败。
    fn replay_content(&self) -> Result<Vec<ContentItem>, VulcanCoreError> {
        self.replay_items
            .iter()
            .map(|item| match &item.state {
                ReplayAccumulatorItemState::Text(text) => {
                    Ok(ContentItem::Text { text: text.clone() })
                }
                ReplayAccumulatorItemState::Reasoning { text, signature } => {
                    Ok(ContentItem::Reasoning {
                        text: text.clone(),
                        signature: signature.clone(),
                    })
                }
                ReplayAccumulatorItemState::Complete(content) => Ok(content.clone()),
                ReplayAccumulatorItemState::PendingTool { .. } => Err(provider_failure(
                    self.provider_id.as_str(),
                    format!(
                        "Public tool stream item `{}` remained incomplete at response completion.",
                        item.item_id
                    )
                    .as_str(),
                )),
            })
            .collect()
    }

    /// Closes one started tool item as rejected so Core answers it without executing the tool.
    /// 将一个已开始的工具条目封闭为已拒绝，使 Core 不执行工具即可答复。
    /// `item_id` is the started item, `message` the model-visible reason, `live_sender` the live channel.
    /// `item_id` 为已开始的条目，`message` 为模型可见原因，`live_sender` 为实时通道。
    /// Returns a controlled failure when the item is not a pending tool.
    /// 条目不是待完成工具时返回受控失败。
    fn reject_tool_call(
        &mut self,
        item_id: String,
        message: String,
        live_sender: &ProviderLiveDeltaSender,
    ) -> Result<(), VulcanCoreError> {
        // rejectedIdentity is retained from ToolCallStarted because a model-visible tool result requires the original call id and name.
        // rejectedIdentity 来自 ToolCallStarted，因为模型可见工具结果必须使用原始调用标识与名称。
        let (call_id, tool_name) = {
            let replay_state = self.replay_item_state_mut(item_id.as_str())?;
            let ReplayAccumulatorItemState::PendingTool { call_id, tool_name } = replay_state
            else {
                return Err(provider_failure(
                    self.provider_id.as_str(),
                    "Public tool abort targeted a non-pending tool item.",
                ));
            };
            (call_id.clone(), tool_name.clone())
        };
        // rejectedCall closes canonical replay with valid JSON while the separate parse diagnostic forbids host execution.
        // rejectedCall 使用有效 JSON 封闭规范重放，同时由独立解析诊断禁止宿主执行。
        let rejected_call = PublicToolCall {
            id: call_id.clone(),
            name: tool_name.clone(),
            arguments: Value::Null,
            signature: None,
        };
        *self.replay_item_state_mut(item_id.as_str())? =
            ReplayAccumulatorItemState::Complete(ContentItem::ToolCall {
                call: rejected_call,
            });
        let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
            ToolRequestFragment::Invalid {
                request_stream_id: item_id.clone(),
                reason: message.clone(),
            },
        )));
        // Invalid calls never run, so they are never judged.
        // 无效调用从不执行，因此从不判定。
        self.raw_tool_arguments.remove(item_id.as_str());
        self.tool_calls.push(ConversationToolCall {
            // Invalid calls retain the same item identity so failure replaces the original draft too.
            // 无效调用同样保留条目标识，使失败结果也能替换原草稿。
            id: item_id,
            tool_call_id: Some(call_id),
            tool_name,
            arguments: Value::Null,
            argument_parse_error: Some(message),
            raw_arguments: Default::default(),
        });
        Ok(())
    }

    /// Applies one public stream event.
    /// 应用一个公共流事件。
    pub(super) fn apply_event<S>(
        &mut self,
        provider_id: &str,
        live_sender: &ProviderLiveDeltaSender,
        sink: &mut S,
        event: AiStreamEvent,
    ) -> Result<(), VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        match event {
            AiStreamEvent::TransportObserved {
                transport: _,
                dispatched_at_ms: _,
                headers_at_ms: _,
                http_status: _,
                provider_request_id: _,
                rate_limit_headers: _,
            } => {
                return Err(provider_failure(
                    provider_id,
                    "TransportObserved must pass through the dispatch persistence barrier.",
                ));
            }
            AiStreamEvent::ResponseMetadataUpdated { continuation } => {
                if let Some(existing) = self.provider_message_id.as_deref()
                    && existing != continuation.provider_message_id
                {
                    return Err(provider_failure(
                        provider_id,
                        "Provider response identity changed during one physical request.",
                    ));
                }
                self.provider_message_id = Some(continuation.provider_message_id);
            }
            AiStreamEvent::ResponseTextDelta { item_id, delta } => {
                let ReplayAccumulatorItemState::Text(text) =
                    self.replay_item_state_mut(item_id.as_str())?
                else {
                    return Err(provider_failure(
                        provider_id,
                        "Public text delta targeted a non-text item.",
                    ));
                };
                text.push_str(delta.as_str());
                self.text.push_str(delta.as_str());
                // liveDelta preserves the same decoded text for the workbench stream while Core keeps its canonical copy.
                // liveDelta 在 Core 保留规范副本的同时，将同一段已解码文本投影到工作台流。
                let live_delta = delta.clone();
                sink.push_delta(ProviderTurnDelta::Output { delta });
                let _ = live_sender.send(Ok(ProviderStreamDelta::Output(live_delta)));
            }
            AiStreamEvent::ResponseItemStarted { item_id, item } => {
                let replay_state = match &item {
                    ContentItem::Text { text } => ReplayAccumulatorItemState::Text(text.clone()),
                    _ => ReplayAccumulatorItemState::Complete(item.clone()),
                };
                self.register_replay_item(item_id, replay_state)?;
                if let ContentItem::Resource {
                    resource:
                        ResourceReference {
                            kind: ResourceKind::Image,
                            local_path,
                            url,
                            ..
                        },
                } = item
                {
                    let location = local_path.or(url).ok_or_else(|| {
                        provider_failure(
                            provider_id,
                            "Provider-generated image has no local path or user-safe URL.",
                        )
                    })?;
                    let markdown = format!(
                        "{}![Generated image](<{}>)",
                        if self.text.ends_with('\n') || self.text.is_empty() {
                            ""
                        } else {
                            "\n\n"
                        },
                        location
                    );
                    self.text.push_str(markdown.as_str());
                    // liveMarkdown keeps generated-resource output visible before the terminal checkpoint.
                    // liveMarkdown 让生成资源输出在终态检查点到达前即可见。
                    let live_markdown = markdown.clone();
                    sink.push_delta(ProviderTurnDelta::Output { delta: markdown });
                    let _ = live_sender.send(Ok(ProviderStreamDelta::Output(live_markdown)));
                }
            }
            AiStreamEvent::ReasoningStarted { item_id } => {
                self.register_replay_item(
                    item_id,
                    ReplayAccumulatorItemState::Reasoning {
                        text: String::new(),
                        signature: None,
                    },
                )?;
            }
            AiStreamEvent::ReasoningDelta { item_id, delta } => {
                let ReplayAccumulatorItemState::Reasoning { text, .. } =
                    self.replay_item_state_mut(item_id.as_str())?
                else {
                    return Err(provider_failure(
                        provider_id,
                        "Public reasoning delta targeted a non-reasoning item.",
                    ));
                };
                text.push_str(delta.as_str());
                self.reasoning.push_str(delta.as_str());
                if !delta.is_empty() && !self.durable_reasoning_observed {
                    // Full readable reasoning is authoritative and removes every transient fallback.
                    // 完整可读思考具有权威性，并会移除全部瞬态回退状态。
                    self.durable_reasoning_observed = true;
                    self.clear_transient_reasoning_status(live_sender);
                }
                // liveDelta projects provider reasoning without removing it from Core's canonical response.
                // liveDelta 投影 Provider 推理内容，同时不从 Core 规范响应中移除该内容。
                let live_delta = delta.clone();
                sink.push_delta(ProviderTurnDelta::Thinking { delta });
                let _ = live_sender.send(Ok(ProviderStreamDelta::Thinking(live_delta)));
            }
            AiStreamEvent::ReasoningSummaryDelta { delta } => {
                // Full readable reasoning outranks the summary, so a summary after it is redundant.
                // 完整可读思考优先于摘要，因此在其之后出现的摘要是冗余的。
                if delta.is_empty() || self.durable_reasoning_observed {
                    return Ok(());
                }
                self.clear_transient_reasoning_status(live_sender);
                // The summary is shown and recorded as thinking but never registers a replay item:
                // replaying a summary as the model's own reasoning would break its context.
                // 摘要作为思考展示与记录，但绝不登记重放项：把摘要当作模型自身思考重放会打断其上下文。
                self.reasoning.push_str(delta.as_str());
                let live_delta = delta.clone();
                sink.push_delta(ProviderTurnDelta::Thinking { delta });
                let _ = live_sender.send(Ok(ProviderStreamDelta::SummaryThinking(live_delta)));
            }
            AiStreamEvent::ReasoningCompleted { item_id, signature } => {
                let ReplayAccumulatorItemState::Reasoning {
                    signature: current_signature,
                    ..
                } = self.replay_item_state_mut(item_id.as_str())?
                else {
                    return Err(provider_failure(
                        provider_id,
                        "Public reasoning completion targeted a non-reasoning item.",
                    ));
                };
                *current_signature = signature;
            }
            AiStreamEvent::ToolCallStarted {
                item_id,
                call_id,
                tool_name,
                ..
            } => {
                self.register_replay_item(
                    item_id.clone(),
                    ReplayAccumulatorItemState::PendingTool {
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                    },
                )?;
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::Detected {
                        request_stream_id: item_id,
                        tool_call_id: call_id,
                        tool_name,
                        initial_arguments: String::new(),
                        argument_assembler: None,
                    },
                )));
            }
            AiStreamEvent::ToolCallArgumentsDelta { item_id, delta, .. } => {
                self.raw_tool_arguments
                    .entry(item_id.clone())
                    .or_default()
                    .push_str(&delta);
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::ArgumentsDelta {
                        request_stream_id: item_id,
                        delta,
                    },
                )));
            }
            // Every tool input schema is an object, so any other JSON value is unusable arguments.
            // 所有工具输入 Schema 都是对象，因此其他 JSON 值都是不可用参数。
            AiStreamEvent::ToolCallCompleted { item_id, call } if !call.arguments.is_object() => {
                self.reject_tool_call(
                    item_id,
                    String::from("Provider tool arguments must be a JSON object."),
                    live_sender,
                )?;
            }
            AiStreamEvent::ToolCallCompleted { item_id, call } => {
                let replay_state = self.replay_item_state_mut(item_id.as_str())?;
                if !matches!(replay_state, ReplayAccumulatorItemState::PendingTool { .. }) {
                    return Err(provider_failure(
                        provider_id,
                        "Public tool completion targeted a non-tool item.",
                    ));
                }
                *replay_state = ReplayAccumulatorItemState::Complete(ContentItem::ToolCall {
                    call: call.clone(),
                });
                let raw_arguments = self.raw_tool_arguments.remove(item_id.as_str());
                let final_arguments = serde_json::to_string(&call.arguments).map_err(|error| {
                    provider_failure(
                        provider_id,
                        format!("Public tool arguments could not be serialized: {error}").as_str(),
                    )
                })?;
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::Completed {
                        request_stream_id: item_id.clone(),
                        canonical_tool_call_id: Some(call.id.clone()),
                        final_arguments: Some(final_arguments),
                    },
                )));
                self.tool_calls.push(ConversationToolCall {
                    // Preserve the streamed item identity through Core preparation and the durable tool event.
                    // 将流式条目标识一直保留至 Core 准备和持久工具事件。
                    id: item_id,
                    tool_call_id: Some(call.id),
                    tool_name: call.name,
                    arguments: call.arguments,
                    argument_parse_error: None,
                    raw_arguments: raw_arguments.map(TransientText::new).unwrap_or_default(),
                });
            }
            AiStreamEvent::ToolCallAborted { item_id, message } => {
                self.reject_tool_call(item_id, message, live_sender)?;
            }
            AiStreamEvent::UsageUpdated { usage } => {
                if let Some(current) = self.usage.as_mut() {
                    current.merge_update(*usage);
                } else {
                    self.usage = Some(*usage);
                }
                if let Some(reasoning_tokens) = self
                    .usage
                    .as_ref()
                    .and_then(|usage| known_usage_u64(&usage.reasoning_tokens))
                    && let Some(status) = self.transient_reasoning_status.as_mut()
                    && status.reasoning_tokens != Some(reasoning_tokens)
                {
                    status.reasoning_tokens = Some(reasoning_tokens);
                    self.emit_transient_reasoning_status(live_sender);
                }
            }
            AiStreamEvent::ResponseCompleted => {
                self.clear_transient_reasoning_status(live_sender);
                self.completed = true;
            }
            AiStreamEvent::ResponseTurnBoundary { end_turn } => {
                if self.end_turn.replace(end_turn).is_some() {
                    return Err(provider_failure(
                        provider_id,
                        "Provider emitted its logical-turn boundary more than once.",
                    ));
                }
            }
            AiStreamEvent::ResponseMessagePhaseUpdated { item_id, phase } => {
                self.message_phases
                    .push(ProviderMessagePhaseSnapshot { item_id, phase });
            }
            AiStreamEvent::ResponseFailed { error } => {
                self.clear_transient_reasoning_status(live_sender);
                return Err(public_execution_failure(provider_id, error));
            }
            AiStreamEvent::ResponseCancelled => {
                self.clear_transient_reasoning_status(live_sender);
                return Err(provider_failure(
                    provider_id,
                    "Provider execution was cancelled.",
                ));
            }
            AiStreamEvent::ProviderContextItemCaptured { item } => {
                if !is_replayable_codex_responses_item(&item) {
                    return Err(provider_failure(
                        provider_id,
                        "Provider context capture contained a non-replayable Responses item.",
                    ));
                }
                self.provider_context_items.push(item);
            }
            AiStreamEvent::ResponseStarted { .. } => {
                if self.transient_reasoning_status.is_some() {
                    self.emit_transient_reasoning_status(live_sender);
                }
            }
            AiStreamEvent::ResourceProgress { .. }
            | AiStreamEvent::ImageGenerationPartial { .. }
            | AiStreamEvent::ResponseItemCompleted { .. } => {}
        }
        Ok(())
    }

    /// Reports whether response completion still owns an unfinished tool request.
    /// 报告响应完成时是否仍持有未完成的工具请求。
    ///
    /// # Returns
    /// 返回值
    /// `true` when final response projection would fail at the tool request coordinator boundary.
    /// 当最终响应投影会在工具请求协调边界失败时返回 `true`。
    pub(super) fn has_pending_tool_request(&self) -> bool {
        self.replay_items
            .iter()
            .any(|item| matches!(item.state, ReplayAccumulatorItemState::PendingTool { .. }))
    }

    /// Finishes the controller response after a public terminal event.
    /// 在公共终态事件后完成控制器响应。
    pub(super) fn finish(self) -> Result<CoreProviderTurnResponse, VulcanCoreError> {
        if !self.completed {
            return Err(provider_failure(
                self.provider_id.as_str(),
                "Provider stream ended without a successful terminal response event.",
            ));
        }
        let replay_content = self.replay_content()?;
        if replay_content.is_empty() {
            return Err(provider_failure(
                self.provider_id.as_str(),
                "Provider returned a successful terminal response without durable semantic output.",
            ));
        }
        // providerContextItems are enabled only when they can replace the entire normalized output.
        // providerContextItems 仅在能够替代全部规范输出时启用。
        let provider_context_items = if provider_context_items_cover_replay_content(
            replay_content.as_slice(),
            self.provider_context_items.as_slice(),
        ) {
            self.provider_context_items
        } else {
            Vec::new()
        };
        let replay = ProviderReplayEnvelope::new(
            self.provider_id.clone(),
            self.connection_id.clone(),
            self.route_fingerprint.clone(),
            self.protocol_profile_id.clone(),
            self.model_id.clone(),
            replay_content,
            provider_context_items,
        )
        .with_message_phases(self.message_phases);
        // vulcanAiState keeps public typed metadata separate from opaque route-bound replay items.
        // vulcanAiState 将公共类型化元数据与不透明的路由绑定重放项目分开保存。
        let mut vulcan_ai_state = serde_json::Map::new();
        vulcan_ai_state.insert(String::from("replay"), json!(replay));
        if let Some(end_turn) = self.end_turn {
            vulcan_ai_state.insert(
                String::from("turn_boundary"),
                json!(ProviderTurnBoundary { end_turn }),
            );
        }
        let provider_message_state = Some(json!({
            "vulcan_ai": Value::Object(vulcan_ai_state),
        }));
        Ok(CoreProviderTurnResponse {
            assistant_text: self.text,
            assistant_reasoning: self.reasoning,
            provider_message_id: self.provider_message_id.clone(),
            provider_message_state,
            usage: self.usage.as_ref().map(public_usage_to_core),
            tool_calls: self.tool_calls,
            continuation: Some(ProviderContinuation {
                provider_id: Some(self.provider_id),
                protocol_profile_id: Some(self.protocol_profile_id),
                model_id: Some(self.model_id),
                last_provider_message_id: self.provider_message_id,
                ..ProviderContinuation::default()
            }),
        })
    }
}

/// Converts public usage into the controller's temporary accounting shape.
/// 将公共用量转换为控制器的临时统计结构。
pub(in crate::agent_runtime) fn public_usage_to_core(
    usage: &RequestUsage,
) -> ProviderUsageSnapshot {
    ProviderUsageSnapshot {
        update_kind: Some(usage.update_kind),
        input_tokens: known_usage(&usage.input_tokens),
        uncached_input_tokens: known_usage(&usage.uncached_input_tokens),
        output_tokens: known_usage(&usage.output_tokens),
        reasoning_tokens: known_usage(&usage.reasoning_tokens),
        total_tokens: known_usage(&usage.total_tokens),
        cached_input_tokens: known_usage(&usage.cache_read_tokens),
        cache_creation_input_tokens: known_usage(&usage.cache_write_tokens),
        audio_input_tokens: known_usage(&usage.audio_input_tokens),
        audio_output_tokens: known_usage(&usage.audio_output_tokens),
        extraction_profile_id: usage.extraction_profile_id.clone(),
        extraction_profile_revision: usage.extraction_profile_revision,
        source_paths: usage.source_paths.clone(),
        validation_flags: usage.validation_flags.clone(),
        response_model: usage.response_model.clone(),
        response_id: usage.response_id.clone(),
        billing_details: usage.billing_details.clone(),
    }
}

/// Converts one known public token count into the controller's platform size.
/// 将一个已知公共 Token 数转换为控制器平台大小。
fn known_usage(value: &UsageValue) -> Option<usize> {
    match value {
        UsageValue::Unknown => None,
        UsageValue::Known { value, .. } => usize::try_from(*value).ok(),
    }
}

/// Returns one known public usage value without platform-size truncation.
/// 返回一个已知公共用量值，且不发生平台尺寸截断。
///
/// # Parameters
/// 参数
/// `value` is one normalized usage dimension.
/// `value` 是一个规范化用量维度。
///
/// # Returns
/// 返回值
/// The exact count when known, otherwise `None`.
/// 已知时返回精确计数，否则返回 `None`。
fn known_usage_u64(value: &UsageValue) -> Option<u64> {
    match value {
        UsageValue::Unknown => None,
        UsageValue::Known { value, .. } => Some(*value),
    }
}

#[cfg(test)]
mod tests;
