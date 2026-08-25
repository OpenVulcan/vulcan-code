//! Model-side tool request streaming coordination and progress projection.
//! 模型侧工具请求流协调与进度投影。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use serde_json::json;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use vulcan_tools_manager::{
    ResolvedToolDefinition, ToolRequestParserContext, ToolRequestParserSession, ToolRequestPreview,
    ToolRequestProgressPolicy, ToolRequestSensitivityPolicy, ToolRequestStagingPolicy,
    ToolRequestStagingStatus, ToolRequestStreamingCapability,
};

mod json_lexer;
use crate::agent_runtime::tool_request_argument_assembler::ToolCallDraftAssembler;
use json_lexer::IncrementalJsonLexer;

/// One provider-normalized fragment of a model-side tool request.
/// 单条由 Provider 规范化后的模型侧工具请求碎片。
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum ToolRequestFragment {
    /// Starts one request after the public tool name and stable correlation are known.
    /// 在公开工具名称与稳定关联已知后启动一条请求。
    Detected {
        /// Stable request-stream identifier used before and during host execution.
        /// 在宿主执行前及执行期间使用的稳定请求流标识。
        request_stream_id: String,
        /// Stable tool-call identifier that will correlate the final execution.
        /// 用于关联最终执行的稳定工具调用标识。
        tool_call_id: String,
        /// Exact public tool name resolved from the active tool snapshot.
        /// 从当前工具快照解析出的精确公开工具名称。
        tool_name: String,
        /// Optional argument text carried by the same provider start fragment.
        /// 同一 Provider 启动碎片携带的可选参数文本。
        initial_arguments: String,
        /// Optional provider-shared assembler that avoids a second long-lived cumulative buffer.
        /// 可选的 Provider 共享 assembler，用于避免第二份长期累计缓冲。
        argument_assembler: Option<ToolCallDraftAssembler>,
    },
    /// Appends one raw JSON argument fragment to an active request.
    /// 向一条活动请求追加一段原始 JSON 参数碎片。
    ArgumentsDelta {
        /// Stable request-stream identifier selected by the provider adapter.
        /// Provider 适配器选择的稳定请求流标识。
        request_stream_id: String,
        /// Raw JSON argument text received from the provider.
        /// 从 Provider 收到的原始 JSON 参数文本。
        delta: String,
    },
    /// Marks one request argument stream as complete.
    /// 将一条请求参数流标记为完成。
    Completed {
        /// Stable request-stream identifier selected by the provider adapter.
        /// Provider 适配器选择的稳定请求流标识。
        request_stream_id: String,
        /// Final canonical tool-call identifier when it differs from the initial correlation.
        /// 当最终规范工具调用标识与初始关联不同时携带该标识。
        canonical_tool_call_id: Option<String>,
        /// Optional authoritative complete argument JSON supplied by protocols with a done snapshot.
        /// 由带有完成快照的协议提供的可选权威完整参数 JSON。
        final_arguments: Option<String>,
    },
    /// Rejects one identified request whose final Provider arguments are not executable JSON.
    /// 拒绝一条最终 Provider 参数不是可执行 JSON 的已识别请求。
    Invalid {
        /// Stable request-stream identifier selected by the provider adapter.
        /// Provider 适配器选择的稳定请求流标识。
        request_stream_id: String,
        /// Controlled parse diagnostic that never contains the raw argument payload.
        /// 绝不包含原始参数载荷的受控解析诊断。
        reason: String,
    },
    /// Stops one request before host execution begins.
    /// 在宿主执行开始前停止一条请求。
    Aborted {
        /// Stable request-stream identifier selected by the provider adapter.
        /// Provider 适配器选择的稳定请求流标识。
        request_stream_id: String,
        /// Stable diagnostic reason that does not contain raw arguments.
        /// 不包含原始参数的稳定诊断原因。
        reason: String,
    },
}

impl std::fmt::Debug for ToolRequestFragment {
    /// Renders correlation and byte counts while redacting every raw argument fragment.
    /// 渲染关联身份与字节计数，同时脱敏全部原始参数碎片。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the safe diagnostic representation.
    /// `formatter` 接收安全诊断表示。
    ///
    /// # Returns
    /// 返回值
    /// Standard formatting success or failure.
    /// 返回标准格式化成功或失败。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Detected {
                request_stream_id,
                tool_call_id,
                tool_name,
                initial_arguments,
                argument_assembler,
            } => formatter
                .debug_struct("Detected")
                .field("request_stream_id", request_stream_id)
                .field("tool_call_id", tool_call_id)
                .field("tool_name", tool_name)
                .field(
                    "initial_argument_bytes",
                    &argument_assembler
                        .as_ref()
                        .map_or_else(|| initial_arguments.len(), ToolCallDraftAssembler::len),
                )
                .finish(),
            Self::ArgumentsDelta {
                request_stream_id,
                delta,
            } => formatter
                .debug_struct("ArgumentsDelta")
                .field("request_stream_id", request_stream_id)
                .field("delta_bytes", &delta.len())
                .finish(),
            Self::Completed {
                request_stream_id,
                canonical_tool_call_id,
                final_arguments,
            } => formatter
                .debug_struct("Completed")
                .field("request_stream_id", request_stream_id)
                .field("canonical_tool_call_id", canonical_tool_call_id)
                .field(
                    "final_argument_bytes",
                    &final_arguments.as_ref().map(String::len),
                )
                .finish(),
            Self::Invalid {
                request_stream_id,
                reason,
            } => formatter
                .debug_struct("Invalid")
                .field("request_stream_id", request_stream_id)
                .field("reason", reason)
                .finish(),
            Self::Aborted {
                request_stream_id,
                reason,
            } => formatter
                .debug_struct("Aborted")
                .field("request_stream_id", request_stream_id)
                .field("reason", reason)
                .finish(),
        }
    }
}

/// Mutable argument-progress state owned by one logical tool request.
/// 单次逻辑工具请求持有的可变参数进度状态。
struct ToolRequestDraft {
    /// Stable tool-call identifier used by transcript projection.
    /// 转录投影使用的稳定工具调用标识。
    tool_call_id: String,
    /// Exact public tool name selected by the model.
    /// 模型选择的精确公开工具名称。
    tool_name: String,
    /// Accumulated raw argument JSON used only for final counting and parser routing.
    /// 仅用于最终计数与解析器路由的累计原始参数 JSON。
    arguments_json: String,
    /// Optional provider-shared sole cumulative argument buffer.
    /// 可选的 Provider 共享唯一累计参数缓冲。
    argument_assembler: Option<ToolCallDraftAssembler>,
    /// Monotonic projection revision for stale-update rejection.
    /// 用于拒绝过期更新的单调投影修订号。
    revision: u64,
    /// Argument-byte count included in the latest visible progress event.
    /// 最近一次可见进度事件已包含的参数字节数。
    last_emitted_bytes: usize,
    /// Exact Unicode scalar count received for the current argument document.
    /// 当前参数文档已接收的精确 Unicode 标量数量。
    received_characters: usize,
    /// Instant when the latest visible progress event was emitted.
    /// 最近一次发出可见进度事件的时刻。
    last_emitted_at: Instant,
    /// Strict incremental lexical state for this exact argument stream.
    /// 当前精确参数流使用的严格增量词法状态。
    json_lexer: IncrementalJsonLexer,
    /// Whether the strict lexer already proved one closed JSON root for immediate card finalization.
    /// 严格词法器是否已经证明存在一个已封闭 JSON 根值，以便立即结束卡片接收态。
    arguments_assembled: bool,
    /// First content-free lexer failure that disabled only the speculative incremental preview.
    /// 首个不含正文且只禁用推测性增量预览的词法失败。
    incremental_preview_diagnostic: Option<String>,
    /// Optional isolated tool-owned parser session.
    /// 可选的隔离工具自有解析器会话。
    parser: Option<Box<dyn ToolRequestParserSession>>,
    /// Latest safe parser preview revision projected to the session.
    /// 最近一次投影到会话的安全解析器预览修订号。
    last_preview_revision: u64,
    /// Frozen per-tool progress cadence from the resolved execution snapshot.
    /// 从已解析执行快照冻结的按工具进度节奏。
    progress_policy: ToolRequestProgressPolicy,
    /// Optional exact preview renderer key authorized by the frozen capability.
    /// 冻结能力授权的可选精确预览渲染键。
    preview_kind: Option<String>,
    /// Frozen host staging boundary enforced independently of parser behavior.
    /// 独立于解析器行为强制执行的冻结宿主暂存边界。
    staging_policy: ToolRequestStagingPolicy,
    /// Frozen field whitelist and body ceiling enforced on every parser snapshot.
    /// 对每个解析器快照强制执行的冻结字段白名单与正文上限。
    sensitivity_policy: ToolRequestSensitivityPolicy,
}

impl ToolRequestDraft {
    /// Returns the exact cumulative argument byte count from the active storage route.
    /// 从当前存储路径返回精确累计参数字节数。
    fn arguments_len(&self) -> usize {
        self.argument_assembler
            .as_ref()
            .map_or_else(|| self.arguments_json.len(), ToolCallDraftAssembler::len)
    }

    /// Reads the exact cumulative argument document without creating a persistent duplicate.
    /// 读取精确累计参数文档，且不创建长期副本。
    fn read_arguments<R>(&self, reader: impl FnOnce(&str) -> R) -> R {
        match self.argument_assembler.as_ref() {
            Some(assembler) => assembler.read(reader),
            None => reader(&self.arguments_json),
        }
    }

    /// Creates one short-lived cumulative snapshot for background tokenization.
    /// 为后台 Token 统计创建一份短生命周期累计快照。
    fn arguments_snapshot(&self) -> String {
        match self.argument_assembler.as_ref() {
            Some(assembler) => assembler.snapshot(),
            None => self.arguments_json.clone(),
        }
    }

    /// Applies one provider delta only when the provider does not already own the shared assembler.
    /// 仅当 Provider 尚未持有共享 assembler 时应用一段 Provider 增量。
    fn append_provider_delta(&mut self, delta: &str) {
        self.received_characters = self
            .received_characters
            .saturating_add(delta.chars().count());
        if self.argument_assembler.is_none() {
            self.arguments_json.push_str(delta);
        }
    }

    /// Replaces the active cumulative argument document with an authoritative final snapshot.
    /// 使用权威最终快照替换当前累计参数文档。
    fn replace_arguments(&mut self, arguments_json: String) {
        self.received_characters = arguments_json.chars().count();
        match self.argument_assembler.as_ref() {
            Some(assembler) => assembler.replace(arguments_json),
            None => self.arguments_json = arguments_json,
        }
    }
}

/// Coordinates normalized request fragments into low-frequency runtime events.
/// 将规范化请求碎片协调为低频运行时事件。
pub(crate) struct ToolRequestStreamCoordinator {
    /// Active request drafts keyed by stable request-stream identifier.
    /// 以稳定请求流标识索引的活动请求草稿。
    drafts: HashMap<String, ToolRequestDraft>,
    /// Frozen request-streaming capabilities keyed by exact public tool name.
    /// 按精确公开工具名称索引的冻结请求流能力。
    tool_capabilities: HashMap<String, Option<ToolRequestStreamingCapability>>,
}

impl Default for ToolRequestStreamCoordinator {
    /// Builds an inert coordinator for focused tests without resolved tools.
    /// 为未提供已解析工具的聚焦测试构建惰性协调器。
    fn default() -> Self {
        Self {
            drafts: HashMap::new(),
            tool_capabilities: HashMap::new(),
        }
    }
}

impl ToolRequestStreamCoordinator {
    /// Builds one coordinator from the frozen resolved-tool snapshot.
    /// 根据冻结的已解析工具快照构建协调器。
    ///
    /// # Parameters
    /// 参数
    /// `resolved_tools` is the frozen execution snapshot used for exact name resolution and parser routing.
    /// `resolved_tools` 是用于精确名称解析与解析器路由的冻结执行快照。
    ///
    /// # Returns
    /// 返回值
    /// A fresh coordinator with no active request drafts.
    /// 返回不包含活动请求草稿的新协调器。
    pub(crate) fn new(resolved_tools: &[ResolvedToolDefinition]) -> Self {
        // toolCapabilities uses the exact public ids exposed in provider function declarations.
        // toolCapabilities 使用 Provider 函数声明中暴露的精确公开标识。
        let tool_capabilities = resolved_tools
            .iter()
            .map(|definition| {
                (
                    definition.descriptor.id.clone(),
                    definition.request_streaming.clone(),
                )
            })
            .collect();
        Self {
            drafts: HashMap::new(),
            tool_capabilities,
        }
    }

    /// Applies one normalized fragment and returns visible low-frequency runtime events.
    /// 应用一条规范化碎片并返回可见的低频运行时事件。
    ///
    /// # Parameters
    /// 参数
    /// `fragment` is the exact provider-normalized tool request change.
    /// `fragment` 是精确的 Provider 规范化工具请求变化。
    ///
    /// # Returns
    /// 返回值
    /// Zero or more runtime events that contain counts but never raw argument fragments.
    /// 返回零条或多条仅包含计数、绝不包含原始参数碎片的运行时事件。
    pub(crate) fn apply(&mut self, fragment: ToolRequestFragment) -> Vec<RuntimeEvent> {
        match fragment {
            ToolRequestFragment::Detected {
                request_stream_id,
                tool_call_id,
                tool_name,
                initial_arguments,
                argument_assembler,
            } => self.start_request(
                request_stream_id,
                tool_call_id,
                tool_name,
                initial_arguments,
                argument_assembler,
            ),
            ToolRequestFragment::ArgumentsDelta {
                request_stream_id,
                delta,
            } => self.append_arguments(&request_stream_id, &delta),
            ToolRequestFragment::Completed {
                request_stream_id,
                canonical_tool_call_id,
                final_arguments,
            } => self.complete_request(&request_stream_id, canonical_tool_call_id, final_arguments),
            ToolRequestFragment::Invalid {
                request_stream_id,
                reason,
            } => self.invalidate_request(&request_stream_id, &reason),
            ToolRequestFragment::Aborted {
                request_stream_id,
                reason,
            } => self.abort_request(&request_stream_id, &reason),
        }
    }

    /// Starts one visible request draft and emits its initial progress snapshot.
    /// 启动一条可见请求草稿并发出初始进度快照。
    fn start_request(
        &mut self,
        request_stream_id: String,
        tool_call_id: String,
        tool_name: String,
        initial_arguments: String,
        argument_assembler: Option<ToolCallDraftAssembler>,
    ) -> Vec<RuntimeEvent> {
        // capability is resolved only by the exact public name frozen for this execution.
        // capability 仅通过当前执行冻结的精确公开名称解析。
        let Some(capability) = self.tool_capabilities.get(&tool_name).cloned() else {
            return Vec::new();
        };
        // Replacing one physical request stream must terminate the old parser session first.
        // 替换同一个物理请求流前必须先终止旧的解析器会话。
        let mut events = Vec::new();
        if let Some(mut previous_draft) = self.drafts.remove(&request_stream_id) {
            abort_draft_parser(&mut previous_draft);
            previous_draft.revision = previous_draft.revision.saturating_add(1);
            events.push(self.runtime_event_for_draft(
                RuntimeEventKind::ToolRequestAborted,
                &request_stream_id,
                &previous_draft,
                "superseded",
                Some("request_stream_superseded"),
                None,
                None,
            ));
        }
        // progressPolicy uses the tool override when present and the host default otherwise.
        // progressPolicy 优先使用工具覆盖，否则使用宿主默认值。
        let progress_policy = capability
            .as_ref()
            .map(|capability| capability.progress_policy)
            .unwrap_or_default();
        // previewPolicy fields are enforced by the coordinator even for host-owned parser implementations.
        // previewPolicy 字段即使面对宿主自有解析器实现也会由协调器强制执行。
        let preview_kind = capability
            .as_ref()
            .and_then(|capability| capability.preview_kind.clone());
        let staging_policy = capability
            .as_ref()
            .map(|capability| capability.staging_policy)
            .unwrap_or_default();
        let sensitivity_policy = capability
            .as_ref()
            .map(|capability| capability.sensitivity_policy.clone())
            .unwrap_or_default();
        // parser is an isolated per-request instance and receives no raw text directly.
        // parser 是按请求隔离的实例，不会直接接收原始文本。
        let parser = capability
            .as_ref()
            .and_then(|capability| capability.parser_factory.as_ref())
            .map(|factory| {
                factory.create(ToolRequestParserContext {
                    request_stream_id: request_stream_id.clone(),
                    tool_name: tool_name.clone(),
                })
            });
        // draft captures the only host-side argument buffer owned by the coordinator.
        // draft 捕获由协调器持有的唯一宿主侧参数缓冲。
        let initial_argument_bytes = argument_assembler
            .as_ref()
            .map_or_else(|| initial_arguments.len(), ToolCallDraftAssembler::len);
        // initialCharacterCount uses the active cumulative source instead of UTF-8 byte length.
        // initialCharacterCount 使用当前累计来源，而不是 UTF-8 字节长度。
        let initial_character_count = argument_assembler.as_ref().map_or_else(
            || initial_arguments.chars().count(),
            |assembler| assembler.read(|arguments| arguments.chars().count()),
        );
        let mut draft = ToolRequestDraft {
            tool_call_id,
            tool_name,
            last_emitted_bytes: initial_argument_bytes,
            received_characters: initial_character_count,
            arguments_json: if argument_assembler.is_some() {
                String::new()
            } else {
                initial_arguments.clone()
            },
            argument_assembler,
            revision: 1,
            last_emitted_at: Instant::now(),
            json_lexer: IncrementalJsonLexer::default(),
            arguments_assembled: false,
            incremental_preview_diagnostic: None,
            parser,
            last_preview_revision: 0,
            progress_policy,
            preview_kind,
            staging_policy,
            sensitivity_policy,
        };
        // startedEvent exposes identity and counts without leaking argument content.
        // startedEvent 在不泄漏参数内容的前提下暴露身份与计数。
        events.push(self.runtime_event_for_draft(
            RuntimeEventKind::ToolRequestStarted,
            &request_stream_id,
            &draft,
            "receiving",
            None,
            None,
            None,
        ));
        // initialPreview consumes same-fragment arguments after the visible card identity is established.
        // initialPreview 在建立可见卡片身份后消费同碎片参数。
        // The shared assembler is authoritative when present, including its already-received prefix.
        // 存在共享 assembler 时它是权威来源，包括其中已经接收的前缀。
        let initial_preview = if let Some(assembler) = draft.argument_assembler.as_ref() {
            let initial_snapshot = assembler.snapshot();
            if initial_snapshot.is_empty() {
                Ok(None)
            } else {
                push_argument_fragment_into_draft(&mut draft, &initial_snapshot)
            }
        } else if initial_arguments.is_empty() {
            Ok(None)
        } else {
            push_argument_fragment_into_draft(&mut draft, &initial_arguments)
        };
        match initial_preview {
            Ok(Some(preview)) => {
                // displayState reflects a lexer-proven closed root even before the Provider response ends.
                // displayState 即使在 Provider 响应结束前，也会反映词法器已证明封闭的根值。
                let display_state = if draft.json_lexer.is_root_complete() {
                    draft.arguments_assembled = true;
                    "arguments_assembled"
                } else {
                    "receiving"
                };
                draft.revision = draft.revision.saturating_add(1);
                events.push(self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestPreviewUpdated,
                    &request_stream_id,
                    &draft,
                    display_state,
                    None,
                    Some(&preview),
                    None,
                ));
            }
            Ok(None) => {
                if draft.json_lexer.is_root_complete() {
                    draft.arguments_assembled = true;
                }
            }
            Err(diagnostic) => {
                // Incremental input is speculative until a Provider-final snapshot arrives.
                // 在 Provider 最终快照到达前，增量输入都只是推测性数据。
                abort_draft_parser(&mut draft);
                draft.incremental_preview_diagnostic = Some(diagnostic);
                draft.revision = draft.revision.saturating_add(1);
                events.push(self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestProgress,
                    &request_stream_id,
                    &draft,
                    "receiving",
                    None,
                    None,
                    draft.incremental_preview_diagnostic.as_deref(),
                ));
            }
        }
        if draft.arguments_assembled {
            draft.revision = draft.revision.saturating_add(1);
            events.push(self.runtime_event_for_draft(
                RuntimeEventKind::ToolRequestProgress,
                &request_stream_id,
                &draft,
                "arguments_assembled",
                None,
                None,
                None,
            ));
        }
        self.drafts.insert(request_stream_id.clone(), draft);
        events
    }

    /// Appends one raw argument fragment and emits throttled progress when needed.
    /// 追加一段原始参数碎片，并在需要时发出节流后的进度。
    fn append_arguments(&mut self, request_stream_id: &str, delta: &str) -> Vec<RuntimeEvent> {
        // now anchors the throttling decision for the current fragment.
        // now 为当前碎片的节流判断提供时间锚点。
        let now = Instant::now();
        // mutation records parser preview changes, safe degradation, and progress cadence.
        // mutation 记录解析器预览变化、安全降级与进度节奏。
        let mutation = {
            // draft is the uniquely correlated active request state.
            // draft 是唯一关联的活动请求状态。
            let Some(draft) = self.drafts.get_mut(request_stream_id) else {
                return Vec::new();
            };
            draft.append_provider_delta(delta);
            // preview is derived only while the speculative incremental document remains valid.
            // preview 仅在推测性增量文档仍合法时派生。
            let (preview, new_preview_diagnostic) =
                if draft.incremental_preview_diagnostic.is_some() {
                    (None, None)
                } else {
                    match push_argument_fragment_into_draft(draft, delta) {
                        Ok(preview) => (preview, None),
                        Err(diagnostic) => {
                            // Parser state cannot survive a rejected speculative prefix, but the draft can.
                            // 解析器状态无法跨越被拒绝的推测性前缀，但草稿本身可以继续保留。
                            abort_draft_parser(draft);
                            draft.incremental_preview_diagnostic = Some(diagnostic.clone());
                            draft.arguments_assembled = false;
                            (None, Some(diagnostic))
                        }
                    }
                };
            // receivedBytes is the authoritative UTF-8 byte count after the append.
            // receivedBytes 是追加后的权威 UTF-8 字节数。
            let received_bytes = draft.arguments_len();
            // byteGrowth is the unprojected byte growth since the latest visible event.
            // byteGrowth 是自最近一次可见事件以来尚未投影的字节增量。
            let byte_growth = received_bytes.saturating_sub(draft.last_emitted_bytes);
            // minimumByteGrowth increases for large inputs to bound token and persistence work.
            // minimumByteGrowth 会针对大输入提高，以限制 Token 与持久化工作。
            let minimum_byte_growth =
                if received_bytes >= draft.progress_policy.large_input_threshold_bytes {
                    draft.progress_policy.large_input_minimum_byte_growth
                } else {
                    draft.progress_policy.minimum_byte_growth
                };
            // firstNonEmptyGrowth guarantees a zero-byte start cannot remain the only visible receiving snapshot.
            // firstNonEmptyGrowth 确保零字节启动快照不会成为唯一可见的接收进度。
            let first_non_empty_growth = draft.last_emitted_bytes == 0 && received_bytes > 0;
            let should_emit_progress = first_non_empty_growth
                || new_preview_diagnostic.is_some()
                || byte_growth >= minimum_byte_growth
                || now.duration_since(draft.last_emitted_at)
                    >= Duration::from_millis(draft.progress_policy.maximum_interval_ms);
            if should_emit_progress {
                draft.last_emitted_bytes = received_bytes;
                draft.last_emitted_at = now;
            }
            // newlyAssembled bypasses cadence throttling exactly once when one valid JSON root closes.
            // newlyAssembled 在一个合法 JSON 根值封闭时恰好一次绕过节奏节流。
            let preview_degraded = draft.incremental_preview_diagnostic.is_some();
            let newly_assembled = !preview_degraded
                && !draft.arguments_assembled
                && draft.json_lexer.is_root_complete();
            if newly_assembled {
                draft.arguments_assembled = true;
                draft.last_emitted_bytes = received_bytes;
                draft.last_emitted_at = now;
            }
            (
                should_emit_progress,
                preview,
                newly_assembled,
                draft.arguments_assembled,
                new_preview_diagnostic,
                preview_degraded,
            )
        };
        let (
            should_emit_progress,
            preview,
            newly_assembled,
            arguments_assembled,
            preview_diagnostic,
            preview_degraded,
        ) = mutation;
        // events contains at most one throttled progress and one semantic preview update.
        // events 最多包含一条节流进度与一条语义预览更新。
        let mut events = Vec::new();
        if should_emit_progress || newly_assembled {
            if let Some(draft) = self.drafts.get_mut(request_stream_id) {
                draft.revision = draft.revision.saturating_add(1);
            }
            if let Some(progress_draft) = self.drafts.get(request_stream_id) {
                events.push(self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestProgress,
                    request_stream_id,
                    progress_draft,
                    if arguments_assembled {
                        "arguments_assembled"
                    } else {
                        "receiving"
                    },
                    None,
                    None,
                    preview_diagnostic.as_deref(),
                ));
            }
        }
        if !preview_degraded && let Some(preview) = preview {
            if let Some(draft) = self.drafts.get_mut(request_stream_id) {
                draft.revision = draft.revision.saturating_add(1);
            }
            if let Some(preview_draft) = self.drafts.get(request_stream_id) {
                events.push(self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestPreviewUpdated,
                    request_stream_id,
                    preview_draft,
                    if arguments_assembled {
                        "arguments_assembled"
                    } else {
                        "receiving"
                    },
                    None,
                    Some(&preview),
                    None,
                ));
            }
        }
        events
    }

    /// Completes one active request and emits its final progress snapshot immediately.
    /// 完成一条活动请求并立即发出最终进度快照。
    fn complete_request(
        &mut self,
        request_stream_id: &str,
        canonical_tool_call_id: Option<String>,
        final_arguments: Option<String>,
    ) -> Vec<RuntimeEvent> {
        // draft is removed so no later provider fragment can revive a completed request.
        // draft 被移除，确保后续 Provider 碎片不能复活已完成请求。
        let Some(mut draft) = self.drafts.remove(request_stream_id) else {
            return Vec::new();
        };
        if let Some(canonical_tool_call_id) = canonical_tool_call_id {
            draft.tool_call_id = canonical_tool_call_id;
        }
        // previewDiagnostic records safe degradation without exposing argument content.
        // previewDiagnostic 记录安全降级，但不暴露参数内容。
        let mut preview_diagnostic = None;
        // incrementalDiagnostic belongs only to the speculative stream and is reconsidered below.
        // incrementalDiagnostic 仅属于推测性参数流，并会在下方根据权威快照重新判断。
        let incremental_diagnostic = draft.incremental_preview_diagnostic.take();
        let authoritative_final_present = final_arguments.is_some();
        if let Some(final_arguments) = final_arguments {
            // finalArgumentsDiffer compares exact bytes before replacing the active cumulative storage.
            // finalArgumentsDiffer 在替换当前累计存储前比较精确字节。
            let final_arguments_differ =
                !draft.read_arguments(|arguments| arguments == final_arguments);
            if final_arguments_differ {
                draft.replace_arguments(final_arguments);
            }
            if final_arguments_differ || incremental_diagnostic.is_some() {
                abort_draft_parser(&mut draft);
                draft.json_lexer = IncrementalJsonLexer::default();
                preview_diagnostic = Some(if final_arguments_differ {
                    "provider_final_arguments_replaced"
                } else {
                    "provider_final_arguments_revalidated"
                });
            }
        }
        if !authoritative_final_present && let Some(diagnostic) = incremental_diagnostic.as_deref()
        {
            abort_draft_parser(&mut draft);
            draft.revision = draft.revision.saturating_add(1);
            return vec![self.runtime_event_for_draft(
                RuntimeEventKind::ToolRequestInvalid,
                request_stream_id,
                &draft,
                "failed",
                Some("invalid_json"),
                None,
                Some(diagnostic),
            )];
        }
        if preview_diagnostic.is_some() {
            let final_arguments_snapshot = draft.arguments_snapshot();
            if let Err(diagnostic) = draft.json_lexer.push(&final_arguments_snapshot) {
                draft.revision = draft.revision.saturating_add(1);
                return vec![self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestInvalid,
                    request_stream_id,
                    &draft,
                    "failed",
                    Some("invalid_json"),
                    None,
                    Some(diagnostic.as_str()),
                )];
            }
        }
        if draft.read_arguments(|arguments| arguments.trim().is_empty()) {
            // Empty protocol arguments have the same authoritative semantics as an empty object.
            // 空协议参数与空对象具有相同的权威语义。
            // receivedCharacters preserves what the Provider actually transmitted before canonical normalization.
            // receivedCharacters 保留规范化前 Provider 实际传输的字符数量。
            let received_characters = draft.received_characters;
            draft.replace_arguments(String::from("{}"));
            draft.received_characters = received_characters;
            if let Err(diagnostic) = draft.json_lexer.push("{}") {
                draft.revision = draft.revision.saturating_add(1);
                return vec![self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestInvalid,
                    request_stream_id,
                    &draft,
                    "failed",
                    Some("invalid_json"),
                    None,
                    Some(diagnostic.as_str()),
                )];
            }
        }
        // lexicalEvents closes a scalar token whose delimiter is the physical stream end.
        // lexicalEvents 会闭合以物理流结尾为分隔符的标量 Token。
        let lexical_events = match draft.json_lexer.finish() {
            Ok(events) => events,
            Err(diagnostic) => {
                abort_draft_parser(&mut draft);
                draft.revision = draft.revision.saturating_add(1);
                return vec![self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestInvalid,
                    request_stream_id,
                    &draft,
                    "failed",
                    Some("invalid_json"),
                    None,
                    Some(diagnostic.as_str()),
                )];
            }
        };
        let _ = deliver_json_events_to_parser(&mut draft, &lexical_events);
        // arguments is the authoritative complete parse used by the existing invocation path.
        // arguments 是现有调用链使用的权威完整解析结果。
        let arguments = match draft
            .read_arguments(|arguments| serde_json::from_str::<serde_json::Value>(arguments))
        {
            Ok(arguments) => arguments,
            Err(error) => {
                abort_draft_parser(&mut draft);
                draft.revision = draft.revision.saturating_add(1);
                // diagnostic retains only serde's structural location and never copies argument content.
                // diagnostic 仅保留 serde 的结构位置，绝不复制参数原文。
                let diagnostic = error.to_string();
                return vec![self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestInvalid,
                    request_stream_id,
                    &draft,
                    "failed",
                    Some("invalid_json"),
                    None,
                    Some(diagnostic.as_str()),
                )];
            }
        };
        // finalPreview must agree with the complete arguments before staging can be considered validated.
        // finalPreview 必须与完整参数一致，暂存才能被视为已校验。
        let (final_preview, parser_diagnostic) = finish_draft_parser(&mut draft, &arguments);
        if parser_diagnostic.is_some() {
            preview_diagnostic = parser_diagnostic;
        }
        let mut events = Vec::new();
        if let Some(preview) = final_preview {
            draft.revision = draft.revision.saturating_add(1);
            events.push(self.runtime_event_for_draft(
                RuntimeEventKind::ToolRequestPreviewUpdated,
                request_stream_id,
                &draft,
                "arguments_assembled",
                None,
                Some(&preview),
                preview_diagnostic,
            ));
        }
        draft.revision = draft.revision.saturating_add(1);
        events.push(self.runtime_event_for_draft(
            RuntimeEventKind::ToolRequestReady,
            request_stream_id,
            &draft,
            "ready",
            None,
            None,
            preview_diagnostic,
        ));
        events
    }

    /// Rejects one identified request as a terminal pre-execution argument failure.
    /// 将一条已识别请求拒绝为执行前参数失败终态。
    ///
    /// # Parameters
    /// 参数
    /// `request_stream_id` selects the exact active draft and `reason` is a controlled diagnostic
    /// without raw argument content.
    /// `request_stream_id` 选择精确活动草稿，`reason` 是不含原始参数内容的受控诊断。
    ///
    /// # Returns
    /// 返回值
    /// One failed terminal request event, or no event when the draft was already closed.
    /// 返回一条失败终态请求事件；草稿已经关闭时不返回事件。
    fn invalidate_request(&mut self, request_stream_id: &str, reason: &str) -> Vec<RuntimeEvent> {
        // draft is removed before projection so delayed fragments cannot revive rejected input.
        // draft 在投影前移除，确保延迟碎片不能复活已拒绝输入。
        let Some(mut draft) = self.drafts.remove(request_stream_id) else {
            return Vec::new();
        };
        abort_draft_parser(&mut draft);
        draft.revision = draft.revision.saturating_add(1);
        vec![self.runtime_event_for_draft(
            RuntimeEventKind::ToolRequestInvalid,
            request_stream_id,
            &draft,
            "failed",
            Some(reason),
            None,
            None,
        )]
    }

    /// Aborts one active request without exposing its accumulated argument content.
    /// 在不暴露累计参数内容的前提下中止一条活动请求。
    fn abort_request(&mut self, request_stream_id: &str, reason: &str) -> Vec<RuntimeEvent> {
        // draft is removed so temporary request state is released immediately.
        // draft 被移除，以便立即释放临时请求状态。
        let Some(mut draft) = self.drafts.remove(request_stream_id) else {
            return Vec::new();
        };
        abort_draft_parser(&mut draft);
        draft.revision = draft.revision.saturating_add(1);
        // displayState distinguishes retry replacement from ordinary cancellation without exposing provider content.
        // displayState 在不暴露 Provider 内容的前提下区分重试替换与普通取消。
        let display_state = if reason.contains("superseded") {
            "superseded"
        } else {
            "stopped"
        };
        vec![self.runtime_event_for_draft(
            RuntimeEventKind::ToolRequestAborted,
            request_stream_id,
            &draft,
            display_state,
            Some(reason),
            None,
            None,
        )]
    }

    /// Builds one safe runtime event from the latest immutable request draft.
    /// 根据最新不可变请求草稿构建一条安全运行时事件。
    fn runtime_event_for_draft(
        &self,
        kind: RuntimeEventKind,
        request_stream_id: &str,
        draft: &ToolRequestDraft,
        display_state: &str,
        reason: Option<&str>,
        preview: Option<&ToolRequestPreview>,
        preview_diagnostic: Option<&str>,
    ) -> RuntimeEvent {
        RuntimeEvent::new(
            kind,
            format!("工具请求 · {} · {}", draft.tool_call_id, draft.tool_name),
            String::new(),
        )
        .with_metadata(Some(json!({
            "request_stream_id": request_stream_id,
            "tool_call_id": draft.tool_call_id,
            "tool_name": draft.tool_name,
            "tool_display_state": display_state,
            "received_bytes": draft.arguments_len(),
            "received_characters": draft.received_characters,
            "request_revision": draft.revision,
            "reason": reason,
            "preview": preview.map(tool_request_preview_json),
            "preview_diagnostic": preview_diagnostic,
        })))
    }
}

/// Feeds one raw argument fragment through the strict lexer and optional tool parser.
/// 将一段原始参数碎片送入严格词法器与可选工具解析器。
///
/// # Parameters
/// 参数
/// `draft` owns the exact request lexical and parser state.
/// `draft` 持有精确请求的词法与解析器状态。
/// `fragment` is the ordered raw JSON fragment received from the provider.
/// `fragment` 是从 Provider 接收的有序原始 JSON 碎片。
///
/// # Returns
/// 返回值
/// A newer sanitized preview, no semantic change, or a definitive lexical failure.
/// 返回较新的净化预览、无语义变化，或确定的词法失败。
fn push_argument_fragment_into_draft(
    draft: &mut ToolRequestDraft,
    fragment: &str,
) -> Result<Option<ToolRequestPreview>, String> {
    // lexicalEvents contain decoded semantics but never execute or mutate target resources.
    // lexicalEvents 包含已解码语义，但绝不会执行或修改目标资源。
    let lexical_events = draft.json_lexer.push(fragment)?;
    Ok(deliver_json_events_to_parser(draft, &lexical_events))
}

/// Delivers ordered semantic events to one isolated parser and returns a newer safe snapshot.
/// 将有序语义事件投递给一个隔离解析器并返回较新的安全快照。
fn deliver_json_events_to_parser(
    draft: &mut ToolRequestDraft,
    events: &[vulcan_tools_manager::IncrementalJsonEvent],
) -> Option<ToolRequestPreview> {
    // parser is temporarily moved out so failure can atomically degrade the draft to token-only mode.
    // parser 会被临时移出，以便失败时将草稿原子降级为仅 Token 模式。
    let mut parser = draft.parser.take()?;
    for event in events {
        if parser.push_json_event(event).is_err() {
            parser.abort();
            eprintln!(
                "Tool request preview parser degraded for tool `{}`.",
                draft.tool_name
            );
            return None;
        }
    }
    // preview is constrained again at the coordinator boundary so parser defects cannot widen disclosure.
    // preview 会在协调器边界再次受限，确保解析器缺陷无法扩大信息披露范围。
    let preview = match parser.snapshot() {
        Some(preview) => match sanitize_tool_request_preview(draft, preview) {
            Ok(preview) => Some(preview),
            Err(()) => {
                parser.abort();
                eprintln!(
                    "Tool request preview policy validation failed for tool `{}`.",
                    draft.tool_name
                );
                return None;
            }
        },
        None => None,
    }
    .filter(|preview| preview.revision > draft.last_preview_revision);
    if let Some(preview) = &preview {
        draft.last_preview_revision = preview.revision;
    }
    draft.parser = Some(parser);
    preview
}

/// Finalizes one parser against authoritative complete arguments.
/// 根据权威完整参数完成一个解析器。
///
/// # Parameters
/// 参数
/// `draft` owns the optional parser and latest projected preview revision.
/// `draft` 持有可选解析器与最近投影的预览修订号。
/// `arguments` is the complete JSON value accepted by the existing invocation parser.
/// `arguments` 是现有调用解析器接受的完整 JSON 值。
///
/// # Returns
/// 返回值
/// A newer final preview and an optional stable consistency diagnostic key.
/// 返回较新的最终预览与可选稳定一致性诊断键。
fn finish_draft_parser<'a>(
    draft: &mut ToolRequestDraft,
    arguments: &serde_json::Value,
) -> (Option<ToolRequestPreview>, Option<&'a str>) {
    // parser is consumed because a completed request cannot receive further semantic events.
    // parser 会被消费，因为已完成请求不能再接收语义事件。
    let Some(mut parser) = draft.parser.take() else {
        return (None, None);
    };
    match parser.finish(arguments) {
        Ok(preview) => {
            let preview = match preview {
                Some(preview) => match sanitize_tool_request_preview(draft, preview) {
                    Ok(preview) => Some(preview),
                    Err(()) => {
                        parser.abort();
                        eprintln!(
                            "Tool request preview policy validation failed for tool `{}`.",
                            draft.tool_name
                        );
                        return (None, Some("parser_policy_violation"));
                    }
                },
                None => None,
            }
            .filter(|preview| preview.revision > draft.last_preview_revision);
            if let Some(preview) = &preview {
                draft.last_preview_revision = preview.revision;
            }
            (preview, None)
        }
        Err(_) => {
            parser.abort();
            eprintln!(
                "Tool request preview consistency validation failed for tool `{}`.",
                draft.tool_name
            );
            (None, Some("parser_consistency_mismatch"))
        }
    }
}

/// Enforces one frozen request-stream capability against a parser-produced preview.
/// 根据冻结的请求流能力强制校验解析器产出的预览。
///
/// # Parameters
/// 参数
/// `draft` owns the exact capability policy frozen for the current tool request.
/// `draft` 持有当前工具请求冻结的精确能力策略。
/// `preview` is the candidate host-parser snapshot to constrain.
/// `preview` 是待约束的候选宿主解析器快照。
///
/// # Returns
/// 返回值
/// A bounded whitelisted preview, or failure when renderer or staging claims violate policy.
/// 返回有界白名单预览；渲染器或暂存声明违反策略时返回失败。
fn sanitize_tool_request_preview(
    draft: &ToolRequestDraft,
    mut preview: ToolRequestPreview,
) -> Result<ToolRequestPreview, ()> {
    if let Some(expected_kind) = draft.preview_kind.as_deref()
        && preview.kind != expected_kind
    {
        return Err(());
    }
    // allowedFields is a small host declaration and is the sole key admission source.
    // allowedFields 是小型宿主声明，也是字段键获准进入预览的唯一来源。
    let field_is_allowed = |field: &str| {
        draft
            .sensitivity_policy
            .allowed_preview_fields
            .iter()
            .any(|allowed| allowed == field)
    };
    preview
        .title_fields
        .retain(|field, _| field_is_allowed(field));
    preview
        .progress_fields
        .retain(|field, _| field_is_allowed(field));
    if !field_is_allowed("body_preview") {
        preview.body_preview = None;
    } else if let Some(body_preview) = preview.body_preview.as_mut() {
        truncate_utf8_to_bytes(
            body_preview,
            draft.sensitivity_policy.maximum_body_preview_bytes,
        );
        if body_preview.is_empty() {
            preview.body_preview = None;
        }
    }
    if !field_is_allowed("affected_resources") {
        preview.affected_resources.clear();
    }
    if !field_is_allowed("structured_lines") {
        preview.structured_lines.clear();
    } else {
        let maximum_lines = draft.sensitivity_policy.maximum_structured_preview_lines;
        let maximum_bytes = draft.sensitivity_policy.maximum_structured_preview_bytes;
        let mut retained = Vec::new();
        let mut retained_bytes = 0usize;
        let mut truncated = false;
        for mut line in preview.structured_lines.drain(..) {
            if retained.len() >= maximum_lines || retained_bytes >= maximum_bytes {
                truncated = true;
                break;
            }
            let remaining = maximum_bytes.saturating_sub(retained_bytes);
            let original_len = line.content.len();
            truncate_utf8_to_bytes(&mut line.content, remaining);
            if line.content.len() < original_len {
                truncated = true;
            }
            retained_bytes = retained_bytes.saturating_add(line.content.len());
            retained.push(line);
        }
        preview.structured_lines = retained;
        if truncated
            && !preview
                .redaction_flags
                .iter()
                .any(|flag| flag == "structured_preview_truncated")
        {
            preview
                .redaction_flags
                .push(String::from("structured_preview_truncated"));
        }
    }
    if draft.staging_policy == ToolRequestStagingPolicy::Forbidden
        && preview.staging_status != ToolRequestStagingStatus::NotApplicable
    {
        return Err(());
    }
    Ok(preview)
}

/// Truncates one UTF-8 string in place without splitting a Unicode scalar.
/// 在不拆分 Unicode 标量的前提下就地截断 UTF-8 字符串。
///
/// # Parameters
/// 参数
/// `value` is the parser-provided bounded-text candidate.
/// `value` 是解析器提供的有界文本候选值。
/// `maximum_bytes` is the capability-declared inclusive byte ceiling.
/// `maximum_bytes` 是能力声明的包含式字节上限。
///
/// # Returns
/// 返回值
/// This function mutates the string and returns no value.
/// 此函数修改字符串且不返回值。
fn truncate_utf8_to_bytes(value: &mut String, maximum_bytes: usize) {
    if value.len() <= maximum_bytes {
        return;
    }
    let boundary = value
        .char_indices()
        .map(|(index, character)| index + character.len_utf8())
        .take_while(|end| *end <= maximum_bytes)
        .last()
        .unwrap_or_default();
    value.truncate(boundary);
}

/// Aborts and removes one parser session without exposing parser-owned content.
/// 中止并移除一个解析器会话，且不暴露解析器持有的内容。
fn abort_draft_parser(draft: &mut ToolRequestDraft) {
    if let Some(mut parser) = draft.parser.take() {
        parser.abort();
    }
}

/// Converts one already-sanitized parser preview into the stable runtime metadata shape.
/// 将一份已净化解析器预览转换为稳定运行时元数据形态。
fn tool_request_preview_json(preview: &ToolRequestPreview) -> serde_json::Value {
    // stagingStatus is a stable renderer key and never contains a filesystem path.
    // stagingStatus 是稳定渲染键，绝不包含文件系统路径。
    let staging_status = match preview.staging_status {
        vulcan_tools_manager::ToolRequestStagingStatus::NotApplicable => "not_applicable",
        vulcan_tools_manager::ToolRequestStagingStatus::Active => "active",
        vulcan_tools_manager::ToolRequestStagingStatus::Degraded => "degraded",
        vulcan_tools_manager::ToolRequestStagingStatus::Validated => "validated",
        vulcan_tools_manager::ToolRequestStagingStatus::Discarded => "discarded",
    };
    json!({
        "kind": preview.kind,
        "revision": preview.revision,
        "title_fields": preview.title_fields,
        "progress_fields": preview.progress_fields,
        "body_preview": preview.body_preview,
        "structured_lines": preview
            .structured_lines
            .iter()
            .map(|line| {
                json!({
                    "operation_index": line.operation_index,
                    "resource_index": line.resource_index,
                    "side": line.side,
                    "line_index": line.line_index,
                    "content": line.content,
                })
            })
            .collect::<Vec<_>>(),
        "affected_resources": preview.affected_resources,
        "redaction_flags": preview.redaction_flags,
        "staging_status": staging_status,
    })
}

#[cfg(test)]
mod tests {
    //! Tool-request coordination and background token-estimation tests.
    //! 工具请求协调与后台 Token 估算测试。

    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use vulcan_tools_manager::ToolRequestParserFactory;

    /// Builds an inert coordinator that recognizes one exact test tool name.
    /// 构建一个能够识别唯一精确测试工具名称的惰性协调器。
    fn test_coordinator() -> ToolRequestStreamCoordinator {
        // coordinator uses the real serialized worker while omitting tool-owned parsers.
        // coordinator 使用真实串行工作线程，同时省略工具自有解析器。
        let mut coordinator = ToolRequestStreamCoordinator::default();
        coordinator
            .tool_capabilities
            .insert(String::from("test_tool"), None);
        coordinator
    }

    /// Parser fixture that emits a safe preview only at authoritative completion.
    /// 仅在权威完成边界发出安全预览的解析器夹具。
    #[derive(Default)]
    struct FinalPreviewOnlyParser;

    impl ToolRequestParserSession for FinalPreviewOnlyParser {
        /// Accepts semantic fragments without exposing an intermediate preview.
        /// 接受语义碎片，但不暴露中间预览。
        fn push_json_event(
            &mut self,
            _event: &vulcan_tools_manager::IncrementalJsonEvent,
        ) -> Result<(), String> {
            Ok(())
        }

        /// Returns no preview before the authoritative completion boundary.
        /// 在权威完成边界之前不返回预览。
        fn snapshot(&self) -> Option<ToolRequestPreview> {
            None
        }

        /// Returns one final safe preview for the fully parsed arguments.
        /// 为完整解析后的参数返回一份最终安全预览。
        fn finish(
            &mut self,
            _arguments: &serde_json::Value,
        ) -> Result<Option<ToolRequestPreview>, String> {
            Ok(Some(ToolRequestPreview {
                revision: 1,
                ..ToolRequestPreview::default()
            }))
        }

        /// Releases no resources because this fixture owns no staging state.
        /// 不释放任何资源，因为该夹具不持有暂存状态。
        fn abort(&mut self) {}
    }

    /// Parser fixture that records whether an invalid initial fragment releases its session.
    /// 验证无效初始碎片是否释放解析器会话的测试解析器。
    struct AbortTrackingParser {
        /// Shared counter observed by the test after the parser is aborted.
        /// 测试在解析器中止后读取的共享计数器。
        abort_count: Arc<AtomicUsize>,
    }

    impl ToolRequestParserSession for AbortTrackingParser {
        /// Accepts semantic events because the test fails before a valid event is required.
        /// 接受语义事件，因为测试会在需要有效事件前失败。
        fn push_json_event(
            &mut self,
            _event: &vulcan_tools_manager::IncrementalJsonEvent,
        ) -> Result<(), String> {
            Ok(())
        }

        /// Emits no preview because the test only observes lifecycle cleanup.
        /// 不生成预览，因为测试只观察生命周期清理。
        fn snapshot(&self) -> Option<ToolRequestPreview> {
            None
        }

        /// Emits no final preview because the request is intentionally invalid.
        /// 不生成最终预览，因为该请求被故意设置为无效。
        fn finish(
            &mut self,
            _arguments: &serde_json::Value,
        ) -> Result<Option<ToolRequestPreview>, String> {
            Ok(None)
        }

        /// Records the mandatory cleanup call.
        /// 记录必须发生的清理调用。
        fn abort(&mut self) {
            self.abort_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Factory that creates an abort-tracking parser for one request.
    /// 为单个请求创建可跟踪中止行为解析器的工厂。
    struct AbortTrackingParserFactory {
        /// Shared counter passed to each isolated parser session.
        /// 传递给每个隔离解析器会话的共享计数器。
        abort_count: Arc<AtomicUsize>,
    }

    impl ToolRequestParserFactory for AbortTrackingParserFactory {
        /// Creates a parser that records the coordinator cleanup boundary.
        /// 创建记录协调器清理边界的解析器。
        fn create(&self, _context: ToolRequestParserContext) -> Box<dyn ToolRequestParserSession> {
            Box::new(AbortTrackingParser {
                abort_count: Arc::clone(&self.abort_count),
            })
        }
    }

    /// Verifies immediate progress reports Unicode scalar counts without tokenization.
    /// 验证即时进度在不分词的情况下报告 Unicode 标量数量。
    #[test]
    fn immediate_progress_reports_unicode_scalar_count() {
        let mut coordinator = test_coordinator();
        let started = coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-token"),
            tool_call_id: String::from("call-token"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("{\"text\":\"hello 世界\"}"),
            argument_assembler: None,
        });
        // receivedCharacters counts visible Unicode scalars instead of UTF-8 storage bytes.
        // receivedCharacters 统计可见 Unicode 标量，而不是 UTF-8 存储字节。
        assert_eq!(
            started[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("received_characters"))
                .and_then(serde_json::Value::as_u64),
            Some("{\"text\":\"hello 世界\"}".chars().count() as u64)
        );
    }

    /// Verifies a closed JSON root leaves the receiving state before the Provider response terminates.
    /// 验证已封闭 JSON 根值会在 Provider 响应终止前离开接收状态。
    #[test]
    fn closed_argument_root_emits_immediate_assembled_state() {
        let mut coordinator = test_coordinator();
        let started = coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-assembled"),
            tool_call_id: String::from("call-assembled"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("{\"text\":"),
            argument_assembler: None,
        });
        assert!(started.iter().all(|event| {
            event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_display_state"))
                .and_then(serde_json::Value::as_str)
                != Some("arguments_assembled")
        }));

        let assembled = coordinator.apply(ToolRequestFragment::ArgumentsDelta {
            request_stream_id: String::from("stream-assembled"),
            delta: String::from("\"done\"}"),
        });
        assert!(assembled.iter().any(|event| {
            event.kind == RuntimeEventKind::ToolRequestProgress
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_display_state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("arguments_assembled")
        }));
        assert!(
            coordinator
                .drafts
                .get("stream-assembled")
                .is_some_and(|draft| draft.arguments_assembled)
        );
    }

    /// Verifies the first non-empty delta bypasses byte and time throttles after an empty start.
    /// 验证空启动后的首个非空增量会绕过字节与时间节流。
    #[test]
    fn first_non_empty_argument_delta_bypasses_progress_throttle() {
        // coordinator starts from an empty visible snapshot under the real default throttle policy.
        // coordinator 在真实默认节流策略下从空的可见快照开始。
        let mut coordinator = test_coordinator();
        coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-first-growth"),
            tool_call_id: String::from("call-first-growth"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::new(),
            argument_assembler: None,
        });

        // progress receives one incomplete byte that is below every ordinary byte threshold.
        // progress 接收一个低于所有常规字节阈值的未完成字节。
        let progress = coordinator.apply(ToolRequestFragment::ArgumentsDelta {
            request_stream_id: String::from("stream-first-growth"),
            delta: String::from("{"),
        });

        // progressMetadata proves the first non-empty fragment reached visible projection immediately.
        // progressMetadata 证明首个非空片段立即进入可见投影。
        let progress_metadata = progress
            .iter()
            .find(|event| event.kind == RuntimeEventKind::ToolRequestProgress)
            .and_then(|event| event.metadata.as_ref())
            .expect("the first non-empty delta should emit progress immediately");
        assert_eq!(
            progress_metadata
                .get("received_bytes")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            progress_metadata
                .get("tool_display_state")
                .and_then(serde_json::Value::as_str),
            Some("receiving")
        );
    }

    /// Verifies interleaved tool streams close and advance independently before the Provider response ends.
    /// 验证交错工具流会在 Provider 响应结束前分别封闭并独立推进。
    #[test]
    fn interleaved_tool_streams_emit_independent_assembled_and_ready_boundaries() {
        // coordinator owns four simultaneously active request identities from the same Provider response.
        // coordinator 持有同一 Provider 响应中的四个并行活动请求身份。
        let mut coordinator = test_coordinator();
        for index in 1..=4 {
            coordinator.apply(ToolRequestFragment::Detected {
                request_stream_id: format!("stream-{index}"),
                tool_call_id: format!("call-{index}"),
                tool_name: String::from("test_tool"),
                initial_arguments: String::from("{\"index\":"),
                argument_assembler: None,
            });
        }

        // firstAssembled closes only stream 1 while streams 2 through 4 remain lexically incomplete.
        // firstAssembled 仅封闭流 1，而流 2 至流 4 仍保持词法未完成。
        let first_assembled = coordinator.apply(ToolRequestFragment::ArgumentsDelta {
            request_stream_id: String::from("stream-1"),
            delta: String::from("1}"),
        });
        assert!(first_assembled.iter().any(|event| {
            event.kind == RuntimeEventKind::ToolRequestProgress
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("request_stream_id"))
                    .and_then(serde_json::Value::as_str)
                    == Some("stream-1")
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_display_state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("arguments_assembled")
        }));
        assert!(
            coordinator
                .drafts
                .get("stream-1")
                .is_some_and(|draft| draft.arguments_assembled)
        );
        for request_stream_id in ["stream-2", "stream-3", "stream-4"] {
            assert!(
                coordinator
                    .drafts
                    .get(request_stream_id)
                    .is_some_and(|draft| !draft.arguments_assembled)
            );
        }

        // firstReady proves a Provider completion removes and advances only the named request.
        // firstReady 证明 Provider 完成事件只移除并推进被点名的请求。
        let first_ready = coordinator.apply(ToolRequestFragment::Completed {
            request_stream_id: String::from("stream-1"),
            canonical_tool_call_id: Some(String::from("call-1")),
            final_arguments: Some(String::from("{\"index\":1}")),
        });
        assert!(first_ready.iter().any(|event| {
            event.kind == RuntimeEventKind::ToolRequestReady
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("request_stream_id"))
                    .and_then(serde_json::Value::as_str)
                    == Some("stream-1")
        }));
        assert!(!coordinator.drafts.contains_key("stream-1"));
        assert!(coordinator.drafts.contains_key("stream-2"));
        assert!(coordinator.drafts.contains_key("stream-3"));
        assert!(coordinator.drafts.contains_key("stream-4"));

        // secondAssembled demonstrates the next sibling can close without waiting for the remaining pair.
        // secondAssembled 证明下一个同级请求无需等待其余两个请求即可封闭。
        let second_assembled = coordinator.apply(ToolRequestFragment::ArgumentsDelta {
            request_stream_id: String::from("stream-2"),
            delta: String::from("2}"),
        });
        assert!(second_assembled.iter().any(|event| {
            event.kind == RuntimeEventKind::ToolRequestProgress
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("request_stream_id"))
                    .and_then(serde_json::Value::as_str)
                    == Some("stream-2")
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_display_state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("arguments_assembled")
        }));
        assert!(
            coordinator
                .drafts
                .get("stream-2")
                .is_some_and(|draft| draft.arguments_assembled)
        );
        assert!(
            coordinator
                .drafts
                .get("stream-3")
                .is_some_and(|draft| !draft.arguments_assembled)
        );
        assert!(
            coordinator
                .drafts
                .get("stream-4")
                .is_some_and(|draft| !draft.arguments_assembled)
        );
    }

    /// Verifies a completion-only preview cannot regress an assembled card to receiving.
    /// 验证仅在完成时产生的预览不会把已组装卡片回退为接收中。
    #[test]
    fn final_preview_preserves_assembled_display_state() {
        // coordinator receives one lexically incomplete request before its closing fragment.
        // coordinator 先接收一条词法未完成请求，再接收其封闭碎片。
        let mut coordinator = test_coordinator();
        coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-final-preview"),
            tool_call_id: String::from("call-final-preview"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("{\"index\":"),
            argument_assembler: None,
        });
        let assembled = coordinator.apply(ToolRequestFragment::ArgumentsDelta {
            request_stream_id: String::from("stream-final-preview"),
            delta: String::from("1}"),
        });
        assert!(assembled.iter().any(|event| {
            event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_display_state"))
                .and_then(serde_json::Value::as_str)
                == Some("arguments_assembled")
        }));

        // parser simulates a tool that publishes its first safe semantic preview during finish.
        // parser 模拟首次在完成阶段发布安全语义预览的工具。
        coordinator
            .drafts
            .get_mut("stream-final-preview")
            .expect("assembled request should remain active before completion")
            .parser = Some(Box::<FinalPreviewOnlyParser>::default());
        let completed = coordinator.apply(ToolRequestFragment::Completed {
            request_stream_id: String::from("stream-final-preview"),
            canonical_tool_call_id: Some(String::from("call-final-preview")),
            final_arguments: Some(String::from("{\"index\":1}")),
        });

        // finalPreviewState must remain monotonic until the subsequent ready event.
        // finalPreviewState 必须保持单调，直至后续 ready 事件。
        let final_preview_state = completed
            .iter()
            .find(|event| event.kind == RuntimeEventKind::ToolRequestPreviewUpdated)
            .and_then(|event| event.metadata.as_ref())
            .and_then(|metadata| metadata.get("tool_display_state"))
            .and_then(serde_json::Value::as_str);
        assert_eq!(final_preview_state, Some("arguments_assembled"));
        assert!(
            completed
                .iter()
                .any(|event| event.kind == RuntimeEventKind::ToolRequestReady)
        );
    }

    /// Verifies a large provider request and coordinator draft retain one shared cumulative allocation.
    /// 验证大型 Provider 请求与协调器草稿只保留一个共享累计分配。
    #[test]
    fn shared_assembler_avoids_second_long_lived_large_argument_buffer() {
        // largeArguments models the 10 MiB performance boundary required by the implementation plan.
        // largeArguments 模拟实施计划要求覆盖的 10 MiB 性能边界。
        let large_arguments = format!("{{\"content\":\"{}\"}}", "a".repeat(10 * 1024 * 1024));
        let provider_assembler = ToolCallDraftAssembler::new(large_arguments);
        let initial_arguments = provider_assembler.snapshot();
        let mut coordinator = test_coordinator();

        coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-large-shared"),
            tool_call_id: String::from("call-large-shared"),
            tool_name: String::from("test_tool"),
            initial_arguments,
            argument_assembler: Some(provider_assembler.clone()),
        });

        let draft = coordinator
            .drafts
            .get("stream-large-shared")
            .expect("large shared request should remain active");
        assert!(draft.arguments_json.is_empty());
        let retained_assembler = draft
            .argument_assembler
            .as_ref()
            .expect("coordinator should retain the provider-shared assembler");
        assert!(retained_assembler.shares_storage_with(&provider_assembler));
        assert_eq!(retained_assembler.len(), provider_assembler.len());
    }

    /// Verifies an assembler-owned prefix is lexed even when the start fragment is empty.
    /// 验证启动碎片为空时仍会词法解析 assembler 持有的前缀。
    #[test]
    fn shared_assembler_prefix_is_lexed_before_following_delta() {
        let provider_assembler = ToolCallDraftAssembler::new(String::from("{\"value\":"));
        let mut coordinator = test_coordinator();
        let started = coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-assembler-prefix"),
            tool_call_id: String::from("call-assembler-prefix"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::new(),
            argument_assembler: Some(provider_assembler.clone()),
        });
        assert!(
            started
                .iter()
                .all(|event| event.kind != RuntimeEventKind::ToolRequestInvalid)
        );

        provider_assembler.replace(String::from("{\"value\":\"done\"}"));
        let completed = coordinator.apply(ToolRequestFragment::ArgumentsDelta {
            request_stream_id: String::from("stream-assembler-prefix"),
            delta: String::from("\"done\"}"),
        });
        assert!(completed.iter().any(|event| {
            event.kind == RuntimeEventKind::ToolRequestProgress
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_display_state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("arguments_assembled")
        }));
        assert!(coordinator.drafts.contains_key("stream-assembler-prefix"));
    }

    /// Verifies an invalid initial fragment aborts only its speculative parser session.
    /// 验证无效初始碎片只会中止其推测性解析器会话。
    #[test]
    fn invalid_initial_fragment_degrades_preview_without_discarding_draft() {
        let abort_count = Arc::new(AtomicUsize::new(0));
        let mut coordinator = ToolRequestStreamCoordinator::default();
        coordinator.tool_capabilities.insert(
            String::from("abort_tool"),
            Some(ToolRequestStreamingCapability {
                parser_factory: Some(Arc::new(AbortTrackingParserFactory {
                    abort_count: Arc::clone(&abort_count),
                })),
                preview_kind: None,
                staging_policy: ToolRequestStagingPolicy::Forbidden,
                sensitivity_policy: ToolRequestSensitivityPolicy::default(),
                progress_policy: ToolRequestProgressPolicy::default(),
            }),
        );

        let events = coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-invalid-initial"),
            tool_call_id: String::from("call-invalid-initial"),
            tool_name: String::from("abort_tool"),
            initial_arguments: String::from("}"),
            argument_assembler: None,
        });
        assert!(
            events
                .iter()
                .all(|event| event.kind != RuntimeEventKind::ToolRequestInvalid)
        );
        assert_eq!(abort_count.load(Ordering::SeqCst), 1);
        assert!(coordinator.drafts.contains_key("stream-invalid-initial"));
    }

    /// Verifies an invalid delta degrades preview until authoritative final arguments repair it.
    /// 验证非法增量只会降级预览，直至权威最终参数将其修正。
    #[test]
    fn invalid_delta_is_repaired_by_authoritative_final_arguments() {
        // coordinator starts one string value whose next fragment must provide valid JSON escaping.
        // coordinator 启动一个字符串值，其后续碎片必须提供合法 JSON 转义。
        let mut coordinator = test_coordinator();
        coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-invalid-delta"),
            tool_call_id: String::from("call-invalid-delta"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from(r#"{"path":""#),
            argument_assembler: None,
        });

        // degradedEvents must retain only the lexer explanation after an unsupported escape.
        // degradedEvents 必须在遇到不受支持的转义后仅保留词法说明。
        let degraded_events = coordinator.apply(ToolRequestFragment::ArgumentsDelta {
            request_stream_id: String::from("stream-invalid-delta"),
            delta: String::from(r#"\q"#),
        });
        // degradedEvent is non-terminal so a later Provider-final snapshot can repair the draft.
        // degradedEvent 是非终态事件，使后续 Provider 最终快照可以修正草稿。
        let degraded_event = degraded_events
            .iter()
            .find(|event| event.kind == RuntimeEventKind::ToolRequestProgress)
            .expect("invalid delta should emit one degraded progress event");
        assert_eq!(
            degraded_event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("preview_diagnostic"))
                .and_then(serde_json::Value::as_str),
            Some("Invalid JSON escape `\\q` in tool request arguments.")
        );
        // metadataText proves the diagnostic does not copy the surrounding argument field.
        // metadataText 证明诊断没有复制周边参数字段。
        let metadata_text = serde_json::to_string(&degraded_event.metadata)
            .expect("invalid event metadata should serialize");
        assert!(!metadata_text.contains("path"));
        assert!(
            degraded_events
                .iter()
                .all(|event| event.kind != RuntimeEventKind::ToolRequestInvalid)
        );
        assert!(coordinator.drafts.contains_key("stream-invalid-delta"));

        // completedEvents prove valid Provider-final arguments replace the rejected incremental form.
        // completedEvents 证明合法 Provider 最终参数会替换被拒绝的增量形态。
        let completed_events = coordinator.apply(ToolRequestFragment::Completed {
            request_stream_id: String::from("stream-invalid-delta"),
            canonical_tool_call_id: Some(String::from("call-invalid-delta")),
            final_arguments: Some(String::from(r#"{"path":"fixed"}"#)),
        });
        assert!(
            completed_events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::ToolRequestReady)
        );
        assert!(
            completed_events
                .iter()
                .all(|event| event.kind != RuntimeEventKind::ToolRequestInvalid)
        );
        assert!(coordinator.drafts.is_empty());
    }

    /// Builds one minimal request draft for direct preview-policy enforcement tests.
    /// 为直接测试预览策略强制执行构建一份最小请求草稿。
    fn preview_policy_draft() -> ToolRequestDraft {
        // policy allows one title, one body, and no affected-resource disclosure or staging.
        // policy 允许一个标题与一段正文，但不允许受影响资源披露或暂存。
        ToolRequestDraft {
            tool_call_id: String::from("call-policy"),
            tool_name: String::from("test_tool"),
            arguments_json: String::new(),
            argument_assembler: None,
            revision: 1,
            last_emitted_bytes: 0,
            received_characters: 0,
            last_emitted_at: Instant::now(),
            json_lexer: IncrementalJsonLexer::default(),
            arguments_assembled: false,
            incremental_preview_diagnostic: None,
            parser: None,
            last_preview_revision: 0,
            progress_policy: ToolRequestProgressPolicy::default(),
            preview_kind: Some(String::from("safe_preview")),
            staging_policy: ToolRequestStagingPolicy::Forbidden,
            sensitivity_policy: ToolRequestSensitivityPolicy {
                allowed_preview_fields: vec![String::from("name"), String::from("body_preview")],
                redacted_json_paths: Vec::new(),
                maximum_body_preview_bytes: 5,
                maximum_structured_preview_lines: 0,
                maximum_structured_preview_bytes: 0,
            },
        }
    }

    /// Verifies coordinator policy removes undeclared fields and truncates UTF-8 body text safely.
    /// 验证协调器策略会移除未声明字段并安全截断 UTF-8 正文。
    #[test]
    fn preview_policy_filters_fields_and_bounds_utf8_body() {
        // preview intentionally contains one unapproved title, resource path, and oversized body.
        // preview 刻意包含一个未获准标题、资源路径与超长正文。
        let mut preview = ToolRequestPreview {
            kind: String::from("safe_preview"),
            revision: 1,
            body_preview: Some(String::from("世界abc")),
            affected_resources: vec![String::from("secret/path")],
            ..ToolRequestPreview::default()
        };
        preview
            .title_fields
            .insert(String::from("name"), json!("visible"));
        preview
            .title_fields
            .insert(String::from("secret"), json!("hidden"));

        let sanitized = sanitize_tool_request_preview(&preview_policy_draft(), preview)
            .expect("allowed preview should sanitize");
        assert_eq!(sanitized.title_fields.len(), 1);
        assert_eq!(sanitized.title_fields["name"], json!("visible"));
        assert_eq!(sanitized.body_preview.as_deref(), Some("世"));
        assert!(sanitized.affected_resources.is_empty());
    }

    /// Verifies forbidden staging and mismatched renderer claims are rejected rather than normalized silently.
    /// 验证被禁止的暂存与不匹配渲染器声明会被拒绝，而不是静默规范化。
    #[test]
    fn preview_policy_rejects_staging_and_renderer_violations() {
        // activeStaging violates the exact forbidden policy frozen on the request draft.
        // activeStaging 违反请求草稿上冻结的明确禁止暂存策略。
        let active_staging = ToolRequestPreview {
            kind: String::from("safe_preview"),
            revision: 1,
            staging_status: ToolRequestStagingStatus::Active,
            ..ToolRequestPreview::default()
        };
        assert!(sanitize_tool_request_preview(&preview_policy_draft(), active_staging).is_err());
        let wrong_renderer = ToolRequestPreview {
            kind: String::from("other_preview"),
            revision: 1,
            ..ToolRequestPreview::default()
        };
        assert!(sanitize_tool_request_preview(&preview_policy_draft(), wrong_renderer).is_err());
    }

    /// Verifies generic request events expose only counts and never copy raw argument content into metadata.
    /// 验证通用请求事件只暴露计数，绝不会把原始参数内容复制进元数据。
    #[test]
    fn generic_progress_metadata_never_contains_raw_arguments() {
        // secretMarker is intentionally distinctive so any accidental event disclosure is detectable.
        // secretMarker 刻意使用显著内容，以便检测任何意外事件披露。
        let secret_marker = "DO_NOT_PROJECT_RAW_ARGUMENT_9f26";
        let mut coordinator = test_coordinator();
        let events = coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-safe"),
            tool_call_id: String::from("call-safe"),
            tool_name: String::from("test_tool"),
            initial_arguments: format!("{{\"secret\":\"{secret_marker}\"}}"),
            argument_assembler: None,
        });
        assert!(!events.is_empty());
        for event in events {
            assert!(!event.title.contains(secret_marker));
            assert!(!event.detail.contains(secret_marker));
            let metadata = serde_json::to_string(&event.metadata)
                .expect("safe request metadata should serialize");
            assert!(!metadata.contains(secret_marker));
        }
        let debug_text = format!(
            "{:?}",
            ToolRequestFragment::ArgumentsDelta {
                request_stream_id: String::from("stream-safe"),
                delta: secret_marker.to_string(),
            }
        );
        assert!(!debug_text.contains(secret_marker));
        assert!(debug_text.contains("delta_bytes"));
    }

    /// Verifies an unknown Provider function name cannot start a generic request card.
    /// 验证未知 Provider 函数名称不能启动通用请求卡片。
    #[test]
    fn unknown_tool_name_is_not_projected() {
        // empty coordinator catalog is the exact frozen fact used for name rejection.
        // 空协调器目录是拒绝名称时使用的精确冻结事实。
        let mut coordinator = ToolRequestStreamCoordinator::default();
        let events = coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-unknown"),
            tool_call_id: String::from("call-unknown"),
            tool_name: String::from("unknown_tool"),
            initial_arguments: String::from("{}"),
            argument_assembler: None,
        });
        assert!(events.is_empty());
        assert!(coordinator.drafts.is_empty());
    }

    /// Verifies Provider-final invalid JSON closes the request as failed instead of stopped or running.
    /// 验证 Provider 最终非法 JSON 会将请求关闭为失败，而不是停止或运行中。
    #[test]
    fn provider_invalid_arguments_use_failed_terminal_request_state() {
        // coordinator owns one identified request whose raw payload remains private.
        // coordinator 持有一条已识别请求，其原始载荷保持私有。
        let mut coordinator = test_coordinator();
        coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-provider-invalid"),
            tool_call_id: String::from("call-provider-invalid"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("{\"value\":"),
            argument_assembler: None,
        });

        // invalidEvent is the visible pre-execution failure produced from the controlled decoder reason.
        // invalidEvent 是根据受控 decoder 原因生成的可见执行前失败。
        let events = coordinator.apply(ToolRequestFragment::Invalid {
            request_stream_id: String::from("stream-provider-invalid"),
            reason: String::from("tool arguments did not contain complete JSON"),
        });
        let invalid_event = events
            .iter()
            .find(|event| event.kind == RuntimeEventKind::ToolRequestInvalid)
            .expect("invalid Provider arguments should emit one terminal failure");
        assert_eq!(
            invalid_event
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_display_state"))
                .and_then(serde_json::Value::as_str),
            Some("failed")
        );
        assert!(coordinator.drafts.is_empty());
    }

    /// Verifies retry cleanup marks the old physical request as superseded rather than generically stopped.
    /// 验证重试清理会把旧物理请求标记为已替换，而不是笼统的已停止。
    #[test]
    fn retry_abort_uses_superseded_display_state() {
        // coordinator starts one request and then receives the stable central retry reason.
        // coordinator 启动一条请求，随后接收稳定的中央重试原因。
        let mut coordinator = test_coordinator();
        coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-retry"),
            tool_call_id: String::from("call-retry"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("{}"),
            argument_assembler: None,
        });
        let events = coordinator.apply(ToolRequestFragment::Aborted {
            request_stream_id: String::from("stream-retry"),
            reason: String::from("provider_retry_superseded"),
        });
        let aborted = events
            .iter()
            .find(|event| event.kind == RuntimeEventKind::ToolRequestAborted)
            .expect("retry cleanup should emit one abort boundary");
        assert_eq!(
            aborted
                .metadata
                .as_ref()
                .and_then(|metadata| metadata["tool_display_state"].as_str()),
            Some("superseded")
        );
        assert!(coordinator.drafts.is_empty());
    }

    /// Verifies a duplicate physical request id supersedes its old card and retains only the new draft.
    /// 验证重复物理请求标识会替换旧卡片，并且只保留新草稿。
    #[test]
    fn duplicate_request_stream_supersedes_previous_draft() {
        let mut coordinator = test_coordinator();
        coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-duplicate"),
            tool_call_id: String::from("call-old"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("{}"),
            argument_assembler: None,
        });
        let events = coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-duplicate"),
            tool_call_id: String::from("call-new"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("{}"),
            argument_assembler: None,
        });
        assert!(events.iter().any(|event| {
            event.kind == RuntimeEventKind::ToolRequestAborted
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_call_id"))
                    .and_then(serde_json::Value::as_str)
                    == Some("call-old")
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_display_state"))
                    .and_then(serde_json::Value::as_str)
                    == Some("superseded")
        }));
        assert!(events.iter().any(|event| {
            event.kind == RuntimeEventKind::ToolRequestStarted
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_call_id"))
                    .and_then(serde_json::Value::as_str)
                    == Some("call-new")
        }));
        assert_eq!(coordinator.drafts.len(), 1);
        assert_eq!(
            coordinator
                .drafts
                .get("stream-duplicate")
                .map(|draft| draft.tool_call_id.as_str()),
            Some("call-new")
        );
    }
}
