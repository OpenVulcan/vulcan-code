//! Model-side tool request streaming coordination and progress projection.
//! 模型侧工具请求流协调与进度投影。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use vulcan_context::{ContextTokenEstimator, estimate_mixed_language_text_tokens};
use vulcan_tools_manager::{
    ResolvedToolDefinition, ToolRequestParserContext, ToolRequestParserSession, ToolRequestPreview,
    ToolRequestProgressPolicy, ToolRequestSensitivityPolicy, ToolRequestStagingPolicy,
    ToolRequestStagingStatus, ToolRequestStreamingCapability,
};

mod json_lexer;
use crate::agent_runtime::tool_request_argument_assembler::ToolCallDraftAssembler;
use json_lexer::IncrementalJsonLexer;

/// Largest cumulative request eligible for immediate shared heuristic estimation on the SSE thread.
/// 允许在 SSE 线程上立即使用共享启发式估算的最大累计请求大小。
const MAX_SYNCHRONOUS_TOKEN_ESTIMATE_BYTES: usize = 16 * 1024;

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
    /// Latest monotonic user-visible token estimate.
    /// 最近一次单调用户可见 Token 估算值。
    estimated_tokens: usize,
    /// Precision route that produced the latest visible estimate.
    /// 产生最近可见估算值的精度路径。
    token_precision: String,
    /// Whether one cumulative model-aware calculation is currently in flight.
    /// 当前是否有一项累计模型感知计算正在执行。
    token_task_in_flight: bool,
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

/// One background cumulative token result correlated to an exact argument byte revision.
/// 与精确参数字节修订关联的后台累计 Token 结果。
struct ToolRequestTokenEstimateResult {
    /// Stable request-stream identifier.
    /// 稳定请求流标识。
    request_stream_id: String,
    /// UTF-8 byte count of the cumulative text measured by the worker.
    /// 后台任务测量的累计文本 UTF-8 字节数。
    source_bytes: usize,
    /// Estimated cumulative token count.
    /// 估算得到的累计 Token 数。
    estimated_tokens: usize,
    /// Precision route used by the worker.
    /// 后台任务使用的精度路径。
    token_precision: String,
}

/// One cumulative token-estimation task consumed by the coordinator-owned background worker.
/// 由协调器自有后台工作线程消费的一项累计 Token 估算任务。
struct ToolRequestTokenEstimateTask {
    /// Stable request-stream identifier used to route the result.
    /// 用于路由结果的稳定请求流标识。
    request_stream_id: String,
    /// Argument source snapshotted by the worker instead of the Provider stream thread.
    /// 由 worker 而非 Provider 流线程生成快照的参数来源。
    arguments: ToolRequestTokenEstimateInput,
}

/// Selects shared production storage or the local fallback used by focused tests.
/// 选择生产共享存储，或聚焦测试使用的本地回退。
enum ToolRequestTokenEstimateInput {
    /// Shared assembler whose handle is cheap to enqueue from the Provider stream thread.
    /// 可由 Provider 流线程低成本投递句柄的共享 assembler。
    Shared(ToolCallDraftAssembler),
    /// Owned local snapshot for request paths without a provider-shared assembler.
    /// 未使用 Provider 共享 assembler 的请求路径所用自有本地快照。
    Owned(String),
}

/// Coordinator action selected after correlating one background token result.
/// 关联一项后台 Token 结果后选择的协调器动作。
enum ToolRequestTokenEstimateDisposition {
    /// The current request revision accepted a newer visible token estimate.
    /// 当前请求修订接受了更新的可见 Token 估算。
    Updated {
        /// Exact request stream receiving the newer estimate.
        /// 接收更新估算的精确请求流。
        request_stream_id: String,
        /// Whether the request already emitted its ready boundary.
        /// 请求是否已经发出就绪边界。
        completed: bool,
    },
    /// The source byte revision is stale and the latest cumulative text must be rescheduled.
    /// 来源字节修订已过期，必须重新调度最新累计文本。
    Stale(String),
    /// The request ended before its background result could be correlated.
    /// 请求在后台结果完成关联前已经结束。
    Ignored,
}

/// Coordinates normalized request fragments into low-frequency runtime events.
/// 将规范化请求碎片协调为低频运行时事件。
pub(crate) struct ToolRequestStreamCoordinator {
    /// Active request drafts keyed by stable request-stream identifier.
    /// 以稳定请求流标识索引的活动请求草稿。
    drafts: HashMap<String, ToolRequestDraft>,
    /// Ready requests retained only until their final cumulative token result arrives.
    /// 仅保留到最终累计 Token 结果到达为止的已就绪请求。
    completed_token_drafts: HashMap<String, ToolRequestDraft>,
    /// Frozen request-streaming capabilities keyed by exact public tool name.
    /// 按精确公开工具名称索引的冻结请求流能力。
    tool_capabilities: HashMap<String, Option<ToolRequestStreamingCapability>>,
    /// Task sender feeding the single coordinator-owned background token worker.
    /// 向协调器自有的单一后台 Token 工作线程投递任务的发送器。
    token_task_sender: Sender<ToolRequestTokenEstimateTask>,
    /// Receiver drained by the runtime stream driver without blocking Provider consumption.
    /// 由运行时流驱动排空且不阻塞 Provider 消费的接收器。
    token_result_receiver: Receiver<ToolRequestTokenEstimateResult>,
}

impl Default for ToolRequestStreamCoordinator {
    /// Builds an inert coordinator for focused tests that do not provide runtime token services.
    /// 为未提供运行时 Token 服务的聚焦测试构建惰性协调器。
    fn default() -> Self {
        let (token_task_sender, token_result_receiver) =
            spawn_token_estimate_worker(String::new(), None);
        Self {
            drafts: HashMap::new(),
            completed_token_drafts: HashMap::new(),
            tool_capabilities: HashMap::new(),
            token_task_sender,
            token_result_receiver,
        }
    }
}

impl ToolRequestStreamCoordinator {
    /// Builds one coordinator from the exact model and shared token runtime selected for the run.
    /// 根据本次运行选择的精确模型与共享 Token 运行时构建协调器。
    ///
    /// # Parameters
    /// 参数
    /// `model_id` identifies the tokenizer route for argument progress.
    /// `model_id` 标识参数进度使用的 Tokenizer 路由。
    /// `token_budget_runtime` owns the warmed model-aware token counter.
    /// `token_budget_runtime` 持有已预热的模型感知 Token 统计器。
    /// `resolved_tools` is the frozen execution snapshot used for exact name resolution and parser routing.
    /// `resolved_tools` 是用于精确名称解析与解析器路由的冻结执行快照。
    ///
    /// # Returns
    /// 返回值
    /// A fresh coordinator with no active request drafts.
    /// 返回不包含活动请求草稿的新协调器。
    pub(crate) fn new(
        model_id: impl Into<String>,
        token_budget_runtime: Arc<TokenBudgetRuntime>,
        resolved_tools: &[ResolvedToolDefinition],
    ) -> Self {
        let model_id = model_id.into();
        let (token_task_sender, token_result_receiver) =
            spawn_token_estimate_worker(model_id.clone(), Some(Arc::clone(&token_budget_runtime)));
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
            completed_token_drafts: HashMap::new(),
            tool_capabilities,
            token_task_sender,
            token_result_receiver,
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
        let mut events = self.drain_token_progress_events();
        events.extend(match fragment {
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
            ToolRequestFragment::Aborted {
                request_stream_id,
                reason,
            } => self.abort_request(&request_stream_id, &reason),
        });
        events
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
        let initial_token_estimate = argument_assembler.as_ref().map_or_else(
            || immediate_cumulative_token_estimate(&initial_arguments),
            |assembler| assembler.read(immediate_cumulative_token_estimate),
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
            parser,
            last_preview_revision: 0,
            progress_policy,
            preview_kind,
            staging_policy,
            sensitivity_policy,
            estimated_tokens: initial_token_estimate,
            token_precision: String::from("heuristic"),
            token_task_in_flight: false,
        };
        // startedEvent exposes identity and counts without leaking argument content.
        // startedEvent 在不泄漏参数内容的前提下暴露身份与计数。
        let mut events = vec![self.runtime_event_for_draft(
            RuntimeEventKind::ToolRequestStarted,
            &request_stream_id,
            &draft,
            "receiving",
            None,
            None,
            None,
        )];
        // initialPreview consumes same-fragment arguments after the visible card identity is established.
        // initialPreview 在建立可见卡片身份后消费同碎片参数。
        let initial_preview = if initial_arguments.is_empty() {
            Ok(None)
        } else {
            push_argument_fragment_into_draft(&mut draft, &initial_arguments)
        };
        match initial_preview {
            Ok(Some(preview)) => {
                draft.revision = draft.revision.saturating_add(1);
                events.push(self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestPreviewUpdated,
                    &request_stream_id,
                    &draft,
                    "receiving",
                    None,
                    Some(&preview),
                    None,
                ));
            }
            Ok(None) => {}
            Err(()) => {
                draft.revision = draft.revision.saturating_add(1);
                events.push(self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestInvalid,
                    &request_stream_id,
                    &draft,
                    "invalid",
                    Some("invalid_json"),
                    None,
                    None,
                ));
                return events;
            }
        }
        self.drafts.insert(request_stream_id.clone(), draft);
        self.schedule_token_estimate(&request_stream_id);
        events
    }

    /// Appends one raw argument fragment and emits throttled progress when needed.
    /// 追加一段原始参数碎片，并在需要时发出节流后的进度。
    fn append_arguments(&mut self, request_stream_id: &str, delta: &str) -> Vec<RuntimeEvent> {
        // now anchors the throttling decision for the current fragment.
        // now 为当前碎片的节流判断提供时间锚点。
        let now = Instant::now();
        // mutationResult records lexical validity, parser preview changes, and progress cadence.
        // mutationResult 记录词法有效性、解析器预览变化与进度节奏。
        let mutation_result = {
            // draft is the uniquely correlated active request state.
            // draft 是唯一关联的活动请求状态。
            let Some(draft) = self.drafts.get_mut(request_stream_id) else {
                return Vec::new();
            };
            draft.append_provider_delta(delta);
            if let Some(cumulative_estimate) =
                draft.read_arguments(immediate_cumulative_token_estimate_optional)
            {
                draft.estimated_tokens = cumulative_estimate;
                draft.token_precision = String::from("heuristic");
            }
            // preview is derived only from complete semantic JSON events.
            // preview 仅从完整语义 JSON 事件派生。
            let preview = push_argument_fragment_into_draft(draft, delta);
            if preview.is_err() {
                None
            } else {
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
                let should_emit_progress = byte_growth >= minimum_byte_growth
                    || now.duration_since(draft.last_emitted_at)
                        >= Duration::from_millis(draft.progress_policy.maximum_interval_ms);
                if should_emit_progress {
                    draft.last_emitted_bytes = received_bytes;
                    draft.last_emitted_at = now;
                }
                Some((should_emit_progress, preview.unwrap_or_default()))
            }
        };
        let Some((should_emit_progress, preview)) = mutation_result else {
            return self.invalidate_request(request_stream_id, "invalid_json");
        };
        // events contains at most one throttled progress and one semantic preview update.
        // events 最多包含一条节流进度与一条语义预览更新。
        let mut events = Vec::new();
        if should_emit_progress {
            if let Some(draft) = self.drafts.get_mut(request_stream_id) {
                draft.revision = draft.revision.saturating_add(1);
            }
            if let Some(progress_draft) = self.drafts.get(request_stream_id) {
                events.push(self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestProgress,
                    request_stream_id,
                    progress_draft,
                    "receiving",
                    None,
                    None,
                    None,
                ));
            }
            self.schedule_token_estimate(request_stream_id);
        }
        if let Some(preview) = preview {
            if let Some(draft) = self.drafts.get_mut(request_stream_id) {
                draft.revision = draft.revision.saturating_add(1);
            }
            if let Some(preview_draft) = self.drafts.get(request_stream_id) {
                events.push(self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestPreviewUpdated,
                    request_stream_id,
                    preview_draft,
                    "receiving",
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
        if let Some(final_arguments) = final_arguments
            && !draft.read_arguments(|arguments| arguments == final_arguments)
        {
            abort_draft_parser(&mut draft);
            draft.replace_arguments(final_arguments);
            draft.json_lexer = IncrementalJsonLexer::default();
            preview_diagnostic = Some("provider_final_arguments_replaced");
            let final_arguments_snapshot = draft.arguments_snapshot();
            let final_arguments_valid = draft.json_lexer.push(&final_arguments_snapshot).is_ok();
            if !final_arguments_valid {
                draft.revision = draft.revision.saturating_add(1);
                return vec![self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestInvalid,
                    request_stream_id,
                    &draft,
                    "invalid",
                    Some("invalid_json"),
                    None,
                    preview_diagnostic,
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
            if draft.json_lexer.push("{}").is_err() {
                draft.revision = draft.revision.saturating_add(1);
                return vec![self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestInvalid,
                    request_stream_id,
                    &draft,
                    "invalid",
                    Some("invalid_json"),
                    None,
                    preview_diagnostic,
                )];
            }
        }
        // lexicalEvents closes a scalar token whose delimiter is the physical stream end.
        // lexicalEvents 会闭合以物理流结尾为分隔符的标量 Token。
        let lexical_events = match draft.json_lexer.finish() {
            Ok(events) => events,
            Err(_) => {
                abort_draft_parser(&mut draft);
                draft.revision = draft.revision.saturating_add(1);
                return vec![self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestInvalid,
                    request_stream_id,
                    &draft,
                    "invalid",
                    Some("invalid_json"),
                    None,
                    None,
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
            Err(_) => {
                abort_draft_parser(&mut draft);
                draft.revision = draft.revision.saturating_add(1);
                return vec![self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestInvalid,
                    request_stream_id,
                    &draft,
                    "invalid",
                    Some("invalid_json"),
                    None,
                    None,
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
                "receiving",
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
        // completedTokenDrafts keeps no parser or staging resource and exists only for the final background estimate.
        // completedTokenDrafts 不保留解析器或暂存资源，仅用于最终后台估算。
        self.completed_token_drafts
            .insert(request_stream_id.to_string(), draft);
        self.schedule_token_estimate(request_stream_id);
        events
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

    /// Marks one active request invalid and releases parser-owned resources immediately.
    /// 将一条活动请求标记为无效并立即释放解析器持有的资源。
    fn invalidate_request(&mut self, request_stream_id: &str, reason: &str) -> Vec<RuntimeEvent> {
        // draft is removed before projection so no later fragment can revive invalid state.
        // draft 在投影前被移除，确保后续碎片不能复活无效状态。
        let Some(mut draft) = self.drafts.remove(request_stream_id) else {
            return Vec::new();
        };
        abort_draft_parser(&mut draft);
        draft.revision = draft.revision.saturating_add(1);
        vec![self.runtime_event_for_draft(
            RuntimeEventKind::ToolRequestInvalid,
            request_stream_id,
            &draft,
            "invalid",
            Some(reason),
            None,
            None,
        )]
    }

    /// Starts at most one background cumulative token calculation for an active or just-ready request.
    /// 为一条活动或刚就绪请求启动至多一个后台累计 Token 计算。
    ///
    /// # Parameters
    /// 参数
    /// `request_stream_id` identifies the exact cumulative argument buffer to snapshot.
    /// `request_stream_id` 标识需要快照的精确累计参数缓冲区。
    ///
    /// # Returns
    /// 返回值
    /// This function schedules work when eligible and returns no value.
    /// 此函数在符合条件时调度任务，不返回值。
    fn schedule_token_estimate(&mut self, request_stream_id: &str) {
        let draft = if let Some(draft) = self.drafts.get_mut(request_stream_id) {
            draft
        } else if let Some(draft) = self.completed_token_drafts.get_mut(request_stream_id) {
            draft
        } else {
            return;
        };
        if draft.token_task_in_flight {
            return;
        }
        draft.token_task_in_flight = true;
        let request_stream_id = request_stream_id.to_string();
        let arguments = match draft.argument_assembler.as_ref() {
            Some(assembler) => ToolRequestTokenEstimateInput::Shared(assembler.clone()),
            None => ToolRequestTokenEstimateInput::Owned(draft.arguments_json.clone()),
        };
        let task = ToolRequestTokenEstimateTask {
            request_stream_id: request_stream_id.clone(),
            arguments,
        };
        if self.token_task_sender.send(task).is_err() {
            if let Some(draft) = self.drafts.get_mut(request_stream_id.as_str()) {
                draft.token_task_in_flight = false;
            } else if let Some(draft) = self
                .completed_token_drafts
                .get_mut(request_stream_id.as_str())
            {
                draft.token_task_in_flight = false;
            }
        }
    }

    /// Drains completed background token calculations and rejects results for stale byte revisions.
    /// 排空已完成的后台 Token 计算，并拒绝过期字节修订的结果。
    ///
    /// # Returns
    /// 返回值
    /// Monotonic progress events ready for immediate session projection.
    /// 返回可立即投影到会话的单调进度事件。
    pub(crate) fn drain_token_progress_events(&mut self) -> Vec<RuntimeEvent> {
        let mut updated_requests = Vec::new();
        let mut stale_request_ids = Vec::new();
        while let Ok(result) = self.token_result_receiver.try_recv() {
            match self.apply_token_estimate_result(result) {
                ToolRequestTokenEstimateDisposition::Updated {
                    request_stream_id,
                    completed,
                } => {
                    updated_requests.push((request_stream_id, completed));
                }
                ToolRequestTokenEstimateDisposition::Stale(request_stream_id) => {
                    stale_request_ids.push(request_stream_id);
                }
                ToolRequestTokenEstimateDisposition::Ignored => {}
            }
        }
        let mut events = Vec::new();
        for (request_stream_id, completed) in updated_requests {
            let draft = if completed {
                self.completed_token_drafts.get(&request_stream_id)
            } else {
                self.drafts.get(&request_stream_id)
            };
            if let Some(draft) = draft {
                events.push(self.runtime_event_for_draft(
                    RuntimeEventKind::ToolRequestProgress,
                    &request_stream_id,
                    draft,
                    if completed { "ready" } else { "receiving" },
                    None,
                    None,
                    None,
                ));
            }
            if completed {
                self.completed_token_drafts.remove(&request_stream_id);
            }
        }
        stale_request_ids.sort_unstable();
        stale_request_ids.dedup();
        for request_stream_id in stale_request_ids {
            self.schedule_token_estimate(&request_stream_id);
        }
        events
    }

    /// Returns whether a ready request is still waiting for its final background token result.
    /// 返回是否仍有已就绪请求正在等待最终后台 Token 结果。
    ///
    /// # Returns
    /// 返回值
    /// `true` while at least one ready request must remain available for final token projection.
    /// 至少一条已就绪请求仍需保留以投影最终 Token 时返回 `true`。
    pub(crate) fn has_pending_ready_token_progress(&self) -> bool {
        !self.completed_token_drafts.is_empty()
    }

    /// Applies one correlated token result without draining or scheduling additional channel work.
    /// 应用一项已关联 Token 结果，但不排空或调度额外通道任务。
    ///
    /// # Parameters
    /// 参数
    /// `result` carries one cumulative estimate and its exact source byte revision.
    /// `result` 携带一项累计估算及其精确来源字节修订。
    ///
    /// # Returns
    /// 返回值
    /// The deterministic follow-up action selected from current request state.
    /// 返回根据当前请求状态选择的确定性后续动作。
    fn apply_token_estimate_result(
        &mut self,
        result: ToolRequestTokenEstimateResult,
    ) -> ToolRequestTokenEstimateDisposition {
        let completed = !self.drafts.contains_key(&result.request_stream_id);
        let draft = if completed {
            self.completed_token_drafts
                .get_mut(&result.request_stream_id)
        } else {
            self.drafts.get_mut(&result.request_stream_id)
        };
        let Some(draft) = draft else {
            return ToolRequestTokenEstimateDisposition::Ignored;
        };
        draft.token_task_in_flight = false;
        if draft.arguments_len() != result.source_bytes {
            return ToolRequestTokenEstimateDisposition::Stale(result.request_stream_id);
        }
        draft.estimated_tokens = result.estimated_tokens;
        draft.token_precision = result.token_precision;
        draft.revision = draft.revision.saturating_add(1);
        ToolRequestTokenEstimateDisposition::Updated {
            request_stream_id: result.request_stream_id,
            completed,
        }
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
            "estimated_tokens": draft.estimated_tokens,
            "token_precision": draft.token_precision,
            "request_revision": draft.revision,
            "reason": reason,
            "preview": preview.map(tool_request_preview_json),
            "preview_diagnostic": preview_diagnostic,
        })))
    }
}

/// Starts one serialized background token worker for a request-stream coordinator.
/// 为一个请求流协调器启动一条串行后台 Token 工作线程。
///
/// # Parameters
/// 参数
/// `model_id` selects the warmed tokenizer route used by every queued snapshot.
/// `model_id` 选择所有排队快照共用的已预热 Tokenizer 路由。
/// `token_budget_runtime` optionally supplies model-aware token counting.
/// `token_budget_runtime` 可选提供模型感知 Token 统计。
///
/// # Returns
/// 返回值
/// A non-blocking task sender and the result receiver drained by the stream driver.
/// 返回非阻塞任务发送器以及由流驱动排空的结果接收器。
fn spawn_token_estimate_worker(
    model_id: String,
    token_budget_runtime: Option<Arc<TokenBudgetRuntime>>,
) -> (
    Sender<ToolRequestTokenEstimateTask>,
    Receiver<ToolRequestTokenEstimateResult>,
) {
    let (task_sender, task_receiver) = channel::<ToolRequestTokenEstimateTask>();
    let (result_sender, result_receiver) = channel::<ToolRequestTokenEstimateResult>();
    // worker owns the receivers and shared estimator, so Provider streaming only clones and enqueues snapshots.
    // worker 持有接收器与共享估算器，因此 Provider 流线程只负责克隆并投递快照。
    let _ = std::thread::Builder::new()
        .name(String::from("tool-token-estimate"))
        .spawn(move || {
            while let Ok(task) = task_receiver.recv() {
                // argumentsJson is cloned from shared storage only after work reaches this background thread.
                // argumentsJson 只会在任务进入该后台线程后从共享存储克隆。
                let arguments_json = match task.arguments {
                    ToolRequestTokenEstimateInput::Shared(assembler) => assembler.snapshot(),
                    ToolRequestTokenEstimateInput::Owned(arguments_json) => arguments_json,
                };
                let source_bytes = arguments_json.len();
                let model_estimate = token_budget_runtime
                    .as_ref()
                    .and_then(|runtime| runtime.estimate_text_tokens(&model_id, &arguments_json));
                let estimated_tokens = model_estimate
                    .unwrap_or_else(|| estimate_mixed_language_text_tokens(&arguments_json));
                let token_precision = if model_estimate.is_some() {
                    String::from("model")
                } else {
                    String::from("heuristic")
                };
                if result_sender
                    .send(ToolRequestTokenEstimateResult {
                        request_stream_id: task.request_stream_id,
                        source_bytes,
                        estimated_tokens,
                        token_precision,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
    (task_sender, result_receiver)
}

/// Returns an immediate shared heuristic estimate for one bounded cumulative request prefix.
/// 为一段有界累计请求前缀返回即时共享启发式估算。
///
/// # Parameters
/// 参数
/// `arguments_json` is the exact cumulative argument text available at the request boundary.
/// `arguments_json` 是请求边界处可用的精确累计参数文本。
///
/// # Returns
/// 返回值
/// A shared heuristic estimate, or zero when the cumulative text is delegated entirely to the background worker.
/// 返回共享启发式估算；当累计文本完全交给后台任务时返回零。
fn immediate_cumulative_token_estimate(arguments_json: &str) -> usize {
    immediate_cumulative_token_estimate_optional(arguments_json).unwrap_or_default()
}

/// Returns an immediate shared heuristic estimate only for cumulative text below the SSE-thread cost ceiling.
/// 仅为低于 SSE 线程成本上限的累计文本返回即时共享启发式估算。
///
/// # Parameters
/// 参数
/// `arguments_json` is the complete ordered argument prefix received so far.
/// `arguments_json` 是目前为止收到的完整有序参数前缀。
///
/// # Returns
/// 返回值
/// The cumulative shared heuristic estimate when bounded, otherwise `None` for background-only processing.
/// 受控时返回累计共享启发式估算，否则返回 `None` 交由纯后台处理。
fn immediate_cumulative_token_estimate_optional(arguments_json: &str) -> Option<usize> {
    (arguments_json.len() <= MAX_SYNCHRONOUS_TOKEN_ESTIMATE_BYTES)
        .then(|| estimate_mixed_language_text_tokens(arguments_json))
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
) -> Result<Option<ToolRequestPreview>, ()> {
    // lexicalEvents contain decoded semantics but never execute or mutate target resources.
    // lexicalEvents 包含已解码语义，但绝不会执行或修改目标资源。
    let lexical_events = draft.json_lexer.push(fragment).map_err(|_| ())?;
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

    /// Verifies background token completion produces a later monotonic progress revision.
    /// 验证后台 Token 计算完成后会产生更晚的单调进度修订。
    #[test]
    fn background_token_estimate_emits_dynamic_progress() {
        // coordinator starts one recognized request and schedules its cumulative snapshot.
        // coordinator 启动一条已识别请求并调度其累计快照。
        let mut coordinator = test_coordinator();
        let started = coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-token"),
            tool_call_id: String::from("call-token"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("{\"text\":\"hello 世界\"}"),
            argument_assembler: None,
        });
        let started_revision = started[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("request_revision"))
            .and_then(serde_json::Value::as_u64)
            .expect("started revision should exist");
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
        // deadline bounds asynchronous test waiting without coupling to one scheduler tick.
        // deadline 限制异步测试等待，同时不绑定某一次调度时机。
        let deadline = Instant::now() + Duration::from_secs(1);
        let progress = loop {
            let events = coordinator.drain_token_progress_events();
            if let Some(event) = events.into_iter().next() {
                break event;
            }
            assert!(
                Instant::now() < deadline,
                "token worker should finish promptly"
            );
            std::thread::yield_now();
        };
        assert_eq!(progress.kind, RuntimeEventKind::ToolRequestProgress);
        let metadata = progress.metadata.expect("progress metadata should exist");
        assert!(
            metadata["request_revision"]
                .as_u64()
                .is_some_and(|revision| revision > started_revision)
        );
        assert!(
            metadata["estimated_tokens"]
                .as_u64()
                .is_some_and(|value| value > 0)
        );
    }

    /// Verifies immediate heuristic progress is invariant to provider fragment boundaries.
    /// 验证即时启发式进度不受 Provider 碎片边界影响。
    #[test]
    fn immediate_token_estimate_uses_cumulative_argument_prefix() {
        // The two fragments each round to one token separately, while their four-byte union is one token.
        // 两段碎片分别计算都会向上取整为一个 Token，而四字节合并文本只应为一个 Token。
        let mut coordinator = test_coordinator();
        coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-fragment-invariant"),
            tool_call_id: String::from("call-fragment-invariant"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("\""),
            argument_assembler: None,
        });
        coordinator.apply(ToolRequestFragment::ArgumentsDelta {
            request_stream_id: String::from("stream-fragment-invariant"),
            delta: String::from("ab\""),
        });

        let draft = coordinator
            .drafts
            .get("stream-fragment-invariant")
            .expect("active request should retain its cumulative argument prefix");
        assert_eq!(draft.arguments_json, "\"ab\"");
        assert_eq!(
            draft.estimated_tokens,
            estimate_mixed_language_text_tokens(&draft.arguments_json)
        );
        assert_eq!(draft.estimated_tokens, 1);
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

    /// Verifies a model-aware result replaces a higher heuristic value for the same byte revision.
    /// 验证模型感知结果会在同一字节修订上替换更高的启发式值。
    #[test]
    fn model_token_result_overrides_higher_heuristic_estimate() {
        // coordinator owns one exact byte revision whose heuristic is intentionally inflated.
        // coordinator 持有一个精确字节修订，其启发式值被刻意抬高。
        let mut coordinator = test_coordinator();
        coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-model-override"),
            tool_call_id: String::from("call-model-override"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("{}"),
            argument_assembler: None,
        });
        let draft = coordinator
            .drafts
            .get_mut("stream-model-override")
            .expect("active request should exist");
        draft.estimated_tokens = 100;
        let source_bytes = draft.arguments_json.len();

        let disposition = coordinator.apply_token_estimate_result(ToolRequestTokenEstimateResult {
            request_stream_id: String::from("stream-model-override"),
            source_bytes,
            estimated_tokens: 1,
            token_precision: String::from("model"),
        });

        assert!(matches!(
            disposition,
            ToolRequestTokenEstimateDisposition::Updated {
                request_stream_id,
                completed: false,
            } if request_stream_id == "stream-model-override"
        ));
        let draft = coordinator
            .drafts
            .get("stream-model-override")
            .expect("active request should remain available");
        assert_eq!(draft.estimated_tokens, 1);
        assert_eq!(draft.token_precision, "model");
    }

    /// Verifies a token result for an older byte snapshot is rejected and the latest text is rescheduled.
    /// 验证旧字节快照的 Token 结果会被拒绝，并重新调度最新文本。
    #[test]
    fn stale_background_token_result_is_rejected_and_rescheduled() {
        // coordinator owns one active request whose arguments advance after the stale source revision.
        // coordinator 持有一条活动请求，其参数会在旧来源修订之后继续推进。
        let mut coordinator = test_coordinator();
        coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-stale"),
            tool_call_id: String::from("call-stale"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("{\"text\":\"a"),
            argument_assembler: None,
        });
        coordinator.apply(ToolRequestFragment::ArgumentsDelta {
            request_stream_id: String::from("stream-stale"),
            delta: String::from("b\"}"),
        });
        let disposition = coordinator.apply_token_estimate_result(ToolRequestTokenEstimateResult {
            request_stream_id: String::from("stream-stale"),
            source_bytes: 12,
            estimated_tokens: 999,
            token_precision: String::from("model"),
        });
        assert!(matches!(
            disposition,
            ToolRequestTokenEstimateDisposition::Stale(request_stream_id)
                if request_stream_id == "stream-stale"
        ));
        coordinator.schedule_token_estimate("stream-stale");
        let draft = coordinator
            .drafts
            .get("stream-stale")
            .expect("active request should remain available");
        assert_ne!(draft.arguments_json.len(), 12);
        assert_ne!(draft.estimated_tokens, 999);
        assert!(draft.token_task_in_flight);
    }

    /// Verifies a final background estimate can update the same request after its ready boundary.
    /// 验证最终后台估算可以在请求就绪边界之后继续更新同一请求。
    #[test]
    fn final_background_token_estimate_updates_ready_request() {
        // coordinator completes with an authoritative replacement whose latest token result arrives afterward.
        // coordinator 使用权威替换参数完成请求，而其最新 Token 结果随后到达。
        let mut coordinator = test_coordinator();
        coordinator.apply(ToolRequestFragment::Detected {
            request_stream_id: String::from("stream-ready-token"),
            tool_call_id: String::from("call-ready-token"),
            tool_name: String::from("test_tool"),
            initial_arguments: String::from("{\"text\":\"short\"}"),
            argument_assembler: None,
        });
        let ready_events = coordinator.apply(ToolRequestFragment::Completed {
            request_stream_id: String::from("stream-ready-token"),
            canonical_tool_call_id: None,
            final_arguments: Some(String::from("{\"text\":\"authoritative replacement\"}")),
        });
        assert!(
            ready_events
                .iter()
                .any(|event| event.kind == RuntimeEventKind::ToolRequestReady)
        );
        let source_bytes = coordinator
            .completed_token_drafts
            .get("stream-ready-token")
            .expect("ready request should await its final token result")
            .arguments_json
            .len();
        let (result_sender, result_receiver) = channel();
        coordinator.token_result_receiver = result_receiver;
        result_sender
            .send(ToolRequestTokenEstimateResult {
                request_stream_id: String::from("stream-ready-token"),
                source_bytes,
                estimated_tokens: 99,
                token_precision: String::from("model"),
            })
            .expect("test final result should enqueue");

        let progress = coordinator.drain_token_progress_events();
        assert!(matches!(
            progress.as_slice(),
            [event] if event.kind == RuntimeEventKind::ToolRequestProgress
                && event.metadata.as_ref().and_then(|metadata| metadata["tool_display_state"].as_str()) == Some("ready")
                && event.metadata.as_ref().and_then(|metadata| metadata["estimated_tokens"].as_u64()) == Some(99)
        ));
        assert!(
            !coordinator
                .completed_token_drafts
                .contains_key("stream-ready-token")
        );
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
            parser: None,
            last_preview_revision: 0,
            progress_policy: ToolRequestProgressPolicy::default(),
            preview_kind: Some(String::from("safe_preview")),
            staging_policy: ToolRequestStagingPolicy::Forbidden,
            sensitivity_policy: ToolRequestSensitivityPolicy {
                allowed_preview_fields: vec![String::from("name"), String::from("body_preview")],
                redacted_json_paths: Vec::new(),
                maximum_body_preview_bytes: 5,
            },
            estimated_tokens: 0,
            token_precision: String::from("heuristic"),
            token_task_in_flight: false,
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
}
