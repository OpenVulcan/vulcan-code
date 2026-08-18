//! Provider-neutral streaming events and lifecycle validation.
//! 供应商无关的流式事件与生命周期校验。

use crate::{
    AiContinuationUpdate, AiExecutionError, AiTransportKind, ContentItem, RequestUsage,
    ResourceTransferState, ToolCall, ToolCallProgress, UsageUpdateKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Represents one normalized event emitted by an AI execution.
/// 表示一次 AI 执行产生的归一化事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiStreamEvent {
    /// Exact transport facts became observable for this physical attempt.
    /// 当前真实尝试的精确传输事实已经可观测。
    TransportObserved {
        /// Exact transport selected by the dispatcher.
        /// 分派器选中的精确传输。
        transport: AiTransportKind,
        /// Host timestamp immediately before dispatch began.
        /// 分派开始前的宿主时间戳。
        dispatched_at_ms: i64,
        /// Host timestamp when HTTP response headers became available.
        /// HTTP 响应头可用时的宿主时间戳。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        headers_at_ms: Option<i64>,
        /// HTTP status observed at the transport boundary.
        /// 在传输边界观察到的 HTTP 状态码。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        http_status: Option<u16>,
        /// Allowlisted upstream request identifier.
        /// 允许列表内的上游请求标识。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider_request_id: Option<String>,
        /// Allowlisted rate-limit headers with original header names.
        /// 保留原始响应头名称的允许列表限流响应头。
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        rate_limit_headers: BTreeMap<String, String>,
    },
    /// Response execution has started.
    /// 响应执行已经开始。
    ResponseStarted {
        /// Stable execution identifier.
        /// 稳定执行标识。
        execution_id: String,
    },
    /// Provider continuation metadata became available.
    /// 供应商延续元数据已经可用。
    ResponseMetadataUpdated {
        /// Latest upstream response or assistant-message identifier.
        /// 最新上游响应或助手消息标识。
        continuation: AiContinuationUpdate,
    },
    /// A semantic response item has started.
    /// 一个语义响应项已经开始。
    ResponseItemStarted {
        /// Stable response item identifier.
        /// 稳定响应项标识。
        item_id: String,
        /// Initial item payload.
        /// 初始响应项载荷。
        item: ContentItem,
    },
    /// Text content delta.
    /// 文本内容增量。
    ResponseTextDelta {
        /// Owning response item identifier.
        /// 所属响应项标识。
        item_id: String,
        /// Decoded text delta.
        /// 解码后的文本增量。
        delta: String,
    },
    /// Reasoning content has started.
    /// 推理内容已经开始。
    ReasoningStarted {
        /// Stable reasoning item identifier.
        /// 稳定推理项标识。
        item_id: String,
    },
    /// Reasoning content delta.
    /// 推理内容增量。
    ReasoningDelta {
        /// Owning reasoning item identifier.
        /// 所属推理项标识。
        item_id: String,
        /// Decoded reasoning delta.
        /// 解码后的推理增量。
        delta: String,
    },
    /// A complete readable reasoning summary update that must remain transient.
    /// 必须保持瞬态的完整可读推理摘要更新。
    ReasoningSummaryUpdated {
        /// Latest complete summary assembled or decoded from the provider stream.
        /// 从供应商流组装或解码出的最新完整摘要。
        text: String,
    },
    /// Reasoning content has completed.
    /// 推理内容已经完成。
    ReasoningCompleted {
        /// Stable reasoning item identifier.
        /// 稳定推理项标识。
        item_id: String,
        /// Optional continuation signature.
        /// 可选延续签名。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// Tool identity became available before complete arguments.
    /// 工具身份在完整参数前已经可用。
    ToolCallStarted {
        /// Stable stream item identifier.
        /// 稳定流项目标识。
        item_id: String,
        /// Upstream tool call identifier.
        /// 上游工具调用标识。
        call_id: String,
        /// Registered tool name.
        /// 已注册工具名称。
        tool_name: String,
        /// Stable parallel call index.
        /// 稳定并行调用索引。
        index: u32,
    },
    /// Raw tool argument fragment and cumulative receive counters.
    /// 原始工具参数分片与累计接收计数。
    ToolCallArgumentsDelta {
        /// Stable stream item identifier.
        /// 稳定流项目标识。
        item_id: String,
        /// Raw JSON text fragment.
        /// 原始 JSON 文本分片。
        delta: String,
        /// Current cumulative receive progress.
        /// 当前累计接收进度。
        progress: ToolCallProgress,
    },
    /// Complete and decoded tool call.
    /// 完整且已解码的工具调用。
    ToolCallCompleted {
        /// Stable stream item identifier.
        /// 稳定流项目标识。
        item_id: String,
        /// Complete tool call.
        /// 完整工具调用。
        call: ToolCall,
    },
    /// Tool construction terminated without complete valid JSON.
    /// 工具构造在获得完整有效 JSON 前终止。
    ToolCallAborted {
        /// Stable stream item identifier.
        /// 稳定流项目标识。
        item_id: String,
        /// Controlled protocol diagnostic.
        /// 受控协议诊断信息。
        message: String,
    },
    /// Resource transfer state changed.
    /// 资源传输状态发生变化。
    ResourceProgress {
        /// Stable resource identifier.
        /// 稳定资源标识。
        resource_id: String,
        /// Current transfer state.
        /// 当前传输状态。
        state: ResourceTransferState,
    },
    /// One standard image-generation preview became available.
    /// 一项标准图片生成预览已经可用。
    ImageGenerationPartial {
        /// Stable image-generation response item identifier.
        /// 稳定的图片生成响应项目标识。
        item_id: String,
        /// Zero-based preview sequence index reported by the provider.
        /// 供应商报告的从零开始预览序列索引。
        partial_image_index: u64,
        /// Base64-encoded preview image bytes without a data-URL prefix.
        /// 不含数据 URL 前缀的 Base64 编码预览图片字节。
        partial_image_b64: String,
    },
    /// One response item has completed.
    /// 一个响应项已经完成。
    ResponseItemCompleted {
        /// Stable response item identifier.
        /// 稳定响应项标识。
        item_id: String,
    },
    /// One exact provider output item became available for route-bound replay.
    /// 一项用于精确路由绑定重放的供应商原始输出项已经可用。
    ProviderContextItemCaptured {
        /// Exact completed output item returned by the provider.
        /// 供应商返回的精确已完成输出项。
        item: Value,
    },
    /// Updated request usage.
    /// 更新后的请求用量。
    UsageUpdated {
        /// Normalized usage snapshot.
        /// 归一化用量快照。
        usage: Box<RequestUsage>,
    },
    /// Response completed successfully.
    /// 响应成功完成。
    ResponseCompleted,
    /// Response failed.
    /// 响应失败。
    ResponseFailed {
        /// Normalized error.
        /// 归一化错误。
        error: AiExecutionError,
    },
    /// Response was cancelled.
    /// 响应已取消。
    ResponseCancelled,
}

/// Validates normalized stream event ordering.
/// 校验归一化流事件顺序。
#[derive(Debug, Default)]
pub struct AiStreamStateMachine {
    /// Whether the response start event was observed.
    /// 是否已经观察到响应开始事件。
    started: bool,
    /// Whether a terminal response event was observed.
    /// 是否已经观察到响应终态事件。
    terminal: bool,
    /// Whether the one permitted final usage event was observed.
    /// 是否已经观察到唯一允许的最终用量事件。
    final_usage_observed: bool,
    /// Open response item identifiers.
    /// 尚未完成的响应项标识。
    open_items: BTreeSet<String>,
    /// Tool lifecycle state keyed by stream item identifier.
    /// 按流项目标识索引的工具生命周期状态。
    tools: BTreeMap<String, ToolLifecycle>,
}

/// Tracks one tool lifecycle inside the stream validator.
/// 在流校验器中跟踪一个工具生命周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolLifecycle {
    /// Tool identity is known and arguments are incomplete.
    /// 工具身份已知且参数尚未完整。
    Receiving,
    /// Tool arguments completed successfully.
    /// 工具参数已经成功完成。
    Completed,
    /// Tool construction was aborted.
    /// 工具构造已经中止。
    Aborted,
}

impl AiStreamStateMachine {
    /// Applies one event and rejects invalid lifecycle transitions.
    /// 应用一个事件并拒绝无效生命周期转换。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the next normalized event.
    /// `event` 是下一个归一化事件。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when the transition is valid.
    /// 转换有效时返回 `Ok(())`。
    pub fn apply(&mut self, event: &AiStreamEvent) -> Result<(), String> {
        if self.final_usage_observed && matches!(event, AiStreamEvent::UsageUpdated { .. }) {
            return Err(String::from(
                "A usage event was emitted after the authoritative final usage snapshot.",
            ));
        }
        if self.terminal {
            if matches!(
                event,
                AiStreamEvent::UsageUpdated { usage }
                    if matches!(usage.update_kind, UsageUpdateKind::Final)
            ) && !self.final_usage_observed
            {
                self.final_usage_observed = true;
                return Ok(());
            }
            return Err(String::from(
                "A stream event was emitted after the response reached a terminal state.",
            ));
        }
        match event {
            AiStreamEvent::ResponseStarted { .. } => {
                if self.started {
                    return Err(String::from("The response start event was emitted twice."));
                }
                self.started = true;
            }
            AiStreamEvent::TransportObserved { .. }
            | AiStreamEvent::ResponseMetadataUpdated { .. } => {}
            event if !self.started => {
                return Err(format!(
                    "Stream event `{}` was emitted before the response start event.",
                    event.kind()
                ));
            }
            AiStreamEvent::ResponseItemStarted { item_id, .. }
            | AiStreamEvent::ReasoningStarted { item_id } => {
                if !self.open_items.insert(item_id.clone()) {
                    return Err(format!("Response item `{item_id}` was started twice."));
                }
            }
            AiStreamEvent::ResponseTextDelta { item_id, .. }
            | AiStreamEvent::ReasoningDelta { item_id, .. } => {
                self.require_open_item(item_id)?;
            }
            AiStreamEvent::ReasoningCompleted { item_id, .. }
            | AiStreamEvent::ResponseItemCompleted { item_id } => {
                if !self.open_items.remove(item_id) {
                    return Err(format!(
                        "Response item `{item_id}` completed without being open."
                    ));
                }
            }
            AiStreamEvent::ToolCallStarted { item_id, .. } => {
                if self
                    .tools
                    .insert(item_id.clone(), ToolLifecycle::Receiving)
                    .is_some()
                {
                    return Err(format!("Tool call item `{item_id}` was started twice."));
                }
            }
            AiStreamEvent::ToolCallArgumentsDelta {
                item_id, progress, ..
            } => {
                if self.tools.get(item_id) != Some(&ToolLifecycle::Receiving) {
                    return Err(format!(
                        "Tool call item `{item_id}` received arguments outside its receive state."
                    ));
                }
                if progress.complete {
                    return Err(format!(
                        "Tool call item `{item_id}` marked a delta as complete before completion."
                    ));
                }
            }
            AiStreamEvent::ToolCallCompleted { item_id, .. } => {
                self.finish_tool(item_id, ToolLifecycle::Completed)?;
            }
            AiStreamEvent::ToolCallAborted { item_id, .. } => {
                self.finish_tool(item_id, ToolLifecycle::Aborted)?;
            }
            AiStreamEvent::ResponseCompleted
            | AiStreamEvent::ResponseFailed { .. }
            | AiStreamEvent::ResponseCancelled => {
                if self
                    .tools
                    .values()
                    .any(|state| *state == ToolLifecycle::Receiving)
                {
                    return Err(String::from(
                        "The response terminated while a tool call was still receiving data.",
                    ));
                }
                if !self.open_items.is_empty() {
                    return Err(String::from(
                        "The response terminated while a response item was still open.",
                    ));
                }
                self.terminal = true;
            }
            AiStreamEvent::UsageUpdated { usage }
                if matches!(usage.update_kind, UsageUpdateKind::Final) =>
            {
                if self.final_usage_observed {
                    return Err(String::from(
                        "The final usage event was emitted more than once.",
                    ));
                }
                self.final_usage_observed = true;
            }
            AiStreamEvent::ResourceProgress { .. }
            | AiStreamEvent::ImageGenerationPartial { .. }
            | AiStreamEvent::ProviderContextItemCaptured { .. }
            | AiStreamEvent::ReasoningSummaryUpdated { .. }
            | AiStreamEvent::UsageUpdated { .. } => {}
        }
        Ok(())
    }

    /// Returns whether the response has reached a terminal state.
    /// 返回响应是否已经到达终态。
    ///
    /// # Returns
    /// 返回值
    /// `true` after a completed, failed, or cancelled event.
    /// 在完成、失败或取消事件后返回 `true`。
    pub fn is_terminal(&self) -> bool {
        self.terminal
    }

    /// Requires one response item to be open.
    /// 要求一个响应项处于打开状态。
    fn require_open_item(&self, item_id: &str) -> Result<(), String> {
        self.open_items
            .contains(item_id)
            .then_some(())
            .ok_or_else(|| format!("Response item `{item_id}` is not open."))
    }

    /// Finishes one receiving tool call.
    /// 完成一个正在接收的工具调用。
    fn finish_tool(&mut self, item_id: &str, state: ToolLifecycle) -> Result<(), String> {
        let current = self
            .tools
            .get_mut(item_id)
            .ok_or_else(|| format!("Tool call item `{item_id}` was not started."))?;
        if *current != ToolLifecycle::Receiving {
            return Err(format!("Tool call item `{item_id}` already terminated."));
        }
        *current = state;
        Ok(())
    }
}

impl AiStreamEvent {
    /// Returns a stable diagnostic event kind.
    /// 返回稳定的诊断事件类别。
    fn kind(&self) -> &'static str {
        match self {
            Self::TransportObserved { .. } => "transport_observed",
            Self::ResponseStarted { .. } => "response_started",
            Self::ResponseMetadataUpdated { .. } => "response_metadata_updated",
            Self::ResponseItemStarted { .. } => "response_item_started",
            Self::ResponseTextDelta { .. } => "response_text_delta",
            Self::ReasoningStarted { .. } => "reasoning_started",
            Self::ReasoningDelta { .. } => "reasoning_delta",
            Self::ReasoningSummaryUpdated { .. } => "reasoning_summary_updated",
            Self::ReasoningCompleted { .. } => "reasoning_completed",
            Self::ToolCallStarted { .. } => "tool_call_started",
            Self::ToolCallArgumentsDelta { .. } => "tool_call_arguments_delta",
            Self::ToolCallCompleted { .. } => "tool_call_completed",
            Self::ToolCallAborted { .. } => "tool_call_aborted",
            Self::ResourceProgress { .. } => "resource_progress",
            Self::ImageGenerationPartial { .. } => "image_generation_partial",
            Self::ResponseItemCompleted { .. } => "response_item_completed",
            Self::ProviderContextItemCaptured { .. } => "provider_context_item_captured",
            Self::UsageUpdated { .. } => "usage_updated",
            Self::ResponseCompleted => "response_completed",
            Self::ResponseFailed { .. } => "response_failed",
            Self::ResponseCancelled => "response_cancelled",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verifies a tool identity can be displayed before complete JSON arguments.
    /// 验证工具身份可以在 JSON 参数完整前显示。
    #[test]
    fn tool_can_receive_multiple_argument_fragments() {
        let mut state = AiStreamStateMachine::default();
        state
            .apply(&AiStreamEvent::ResponseStarted {
                execution_id: String::from("execution"),
            })
            .expect("start should be valid");
        state
            .apply(&AiStreamEvent::ToolCallStarted {
                item_id: String::from("item"),
                call_id: String::from("call"),
                tool_name: String::from("file_write"),
                index: 0,
            })
            .expect("tool start should be valid");
        for (delta, bytes, characters) in [("{\"text\":\"", 9, 9), ("你好", 15, 11)] {
            state
                .apply(&AiStreamEvent::ToolCallArgumentsDelta {
                    item_id: String::from("item"),
                    delta: delta.to_string(),
                    progress: ToolCallProgress {
                        call_id: Some(String::from("call")),
                        tool_name: Some(String::from("file_write")),
                        bytes_received: bytes,
                        characters_received: characters,
                        complete: false,
                    },
                })
                .expect("incomplete JSON must remain receive progress");
        }
        state
            .apply(&AiStreamEvent::ToolCallCompleted {
                item_id: String::from("item"),
                call: ToolCall {
                    id: String::from("call"),
                    name: String::from("file_write"),
                    arguments: json!({"text": "你好"}),
                    signature: None,
                },
            })
            .expect("complete tool should be valid");
        state
            .apply(&AiStreamEvent::ResponseCompleted)
            .expect("terminal event should be valid");
        assert!(state.is_terminal());
    }

    /// Verifies a response cannot finish while tool JSON remains incomplete.
    /// 验证工具 JSON 尚未完整时响应不能结束。
    #[test]
    fn terminal_event_rejects_incomplete_tool() {
        let mut state = AiStreamStateMachine::default();
        state
            .apply(&AiStreamEvent::ResponseStarted {
                execution_id: String::from("execution"),
            })
            .expect("start should be valid");
        state
            .apply(&AiStreamEvent::ToolCallStarted {
                item_id: String::from("item"),
                call_id: String::from("call"),
                tool_name: String::from("edit"),
                index: 0,
            })
            .expect("tool start should be valid");
        assert!(state.apply(&AiStreamEvent::ResponseCompleted).is_err());
    }

    /// Verifies only an explicitly final usage snapshot may arrive after a terminal event.
    /// 验证终态事件后仅允许显式标记为最终快照的用量事件到达。
    #[test]
    fn terminal_event_accepts_only_explicit_final_usage() {
        let mut state = AiStreamStateMachine::default();
        state
            .apply(&AiStreamEvent::ResponseStarted {
                execution_id: String::from("execution"),
            })
            .expect("start should be valid");
        state
            .apply(&AiStreamEvent::ResponseCompleted)
            .expect("terminal event should be valid");

        let snapshot = AiStreamEvent::UsageUpdated {
            usage: Box::new(RequestUsage::default()),
        };
        assert!(state.apply(&snapshot).is_err());

        let final_usage = AiStreamEvent::UsageUpdated {
            usage: Box::new(RequestUsage {
                update_kind: UsageUpdateKind::Final,
                ..RequestUsage::default()
            }),
        };
        state
            .apply(&final_usage)
            .expect("explicit final usage should be accepted after terminal");
        assert!(state.apply(&final_usage).is_err());
    }

    /// Verifies a final usage event emitted before completion cannot be repeated after completion.
    /// 验证在完成事件前发出的最终用量不能在完成后重复。
    #[test]
    fn pre_terminal_final_usage_cannot_be_repeated_after_completion() {
        let mut state = AiStreamStateMachine::default();
        state
            .apply(&AiStreamEvent::ResponseStarted {
                execution_id: String::from("execution"),
            })
            .expect("start should be valid");
        let final_usage = AiStreamEvent::UsageUpdated {
            usage: Box::new(RequestUsage {
                update_kind: UsageUpdateKind::Final,
                ..RequestUsage::default()
            }),
        };
        state
            .apply(&final_usage)
            .expect("first final usage should be valid");
        state
            .apply(&AiStreamEvent::ResponseCompleted)
            .expect("terminal event should be valid");

        assert!(state.apply(&final_usage).is_err());
    }

    /// Verifies no later snapshot or delta may overwrite an authoritative final usage event.
    /// 验证后续快照或增量都不能覆盖权威最终用量事件。
    #[test]
    fn final_usage_rejects_later_non_final_usage_before_completion() {
        let mut state = AiStreamStateMachine::default();
        state
            .apply(&AiStreamEvent::ResponseStarted {
                execution_id: String::from("execution"),
            })
            .expect("start should be valid");
        state
            .apply(&AiStreamEvent::UsageUpdated {
                usage: Box::new(RequestUsage {
                    update_kind: UsageUpdateKind::Final,
                    ..RequestUsage::default()
                }),
            })
            .expect("first final usage should be valid");

        assert!(
            state
                .apply(&AiStreamEvent::UsageUpdated {
                    usage: Box::new(RequestUsage {
                        update_kind: UsageUpdateKind::Delta,
                        ..RequestUsage::default()
                    }),
                })
                .is_err()
        );
    }
}
