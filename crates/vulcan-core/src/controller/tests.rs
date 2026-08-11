//! Controller-loop tests for the product-owned host execution flow.
//! 面向产品自有宿主执行流的控制器循环测试。

use super::*;
use crate::provider::{VulcanProviderDeltaSink, VulcanStreamingProviderAdapter};
use crate::tool::{AllowAllToolPolicy, ExactToolSetPolicy};
use crate::types::{
    ConversationPayload, ConversationToolCall, ProviderContinuation, ProviderPromptLayers,
    ProviderRequestSnapshot, ProviderRequestToolSchema, ProviderTurnDelta,
};
use async_trait::async_trait;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// One buffered streaming-delta batch paired with the final provider response.
/// 与最终 Provider 响应配对的一批缓冲流式增量。
type PlannedStreamingTurn = (Vec<ProviderTurnDelta>, ProviderTurnResponse);

/// Shared queue of planned streaming turns consumed by the mock streaming adapter.
/// 由模拟流式适配器消费的共享计划流式回合队列。
type PlannedStreamingTurnQueue = Arc<Mutex<VecDeque<PlannedStreamingTurn>>>;

/// Pre-planned mock tool outcome returned for one requested tool.
/// 单个被请求工具返回的预设模拟工具结果。
type MockToolOutcome = Result<(String, Option<Value>, bool), String>;

/// Tool-name indexed outcome table used by the mock tool executor.
/// 模拟工具执行器按工具名称索引的结果表。
type MockToolOutcomeMap = BTreeMap<String, MockToolOutcome>;

/// Captures pre-planned provider turn responses and records the built request snapshots.
/// 持有预先规划的 Provider 回合响应，并记录构建出的请求快照。
#[derive(Debug, Clone)]
struct MockProviderAdapter {
    /// Stable provider identifier exposed by the mock adapter.
    /// 由模拟适配器暴露的稳定 Provider 标识。
    provider_id: String,
    /// Stable model identifier exposed by the mock adapter.
    /// 由模拟适配器暴露的稳定模型标识。
    model_id: String,
    /// Planned normalized provider responses returned sequentially.
    /// 按顺序返回的规范化 Provider 响应计划队列。
    planned_responses: Arc<Mutex<VecDeque<ProviderTurnResponse>>>,
    /// Built request snapshots captured before execution.
    /// 执行前捕获到的请求快照集合。
    built_requests: Arc<Mutex<Vec<ProviderRequestSnapshot>>>,
}

impl MockProviderAdapter {
    /// Creates one mock provider adapter from the planned turn-response sequence.
    /// 根据计划中的回合响应序列创建一个模拟 Provider 适配器。
    fn new(planned_responses: Vec<ProviderTurnResponse>) -> Self {
        Self {
            provider_id: String::from("mock-openai"),
            model_id: String::from("mock-model"),
            planned_responses: Arc::new(Mutex::new(VecDeque::from(planned_responses))),
            built_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Returns the built request snapshots captured by the adapter so far.
    /// 返回适配器截至当前捕获到的请求快照集合。
    fn built_requests(&self) -> Vec<ProviderRequestSnapshot> {
        self.built_requests
            .lock()
            .expect("lock built requests")
            .clone()
    }
}

/// Captures pre-planned streaming provider responses and emits buffered turn deltas before returning the final normalized response.
/// 持有预先规划的流式 Provider 响应，并在返回最终规范化响应前发出缓冲回合增量。
#[derive(Debug, Clone)]
struct MockStreamingProviderAdapter {
    /// Stable provider identifier exposed by the mock streaming adapter.
    /// 由模拟流式适配器暴露的稳定 Provider 标识。
    provider_id: String,
    /// Stable model identifier exposed by the mock streaming adapter.
    /// 由模拟流式适配器暴露的稳定模型标识。
    model_id: String,
    /// Planned normalized provider responses paired with the deltas that should be emitted before each final response.
    /// 与每次最终响应之前应发出的增量配对保存的规范化 Provider 响应计划队列。
    planned_turns: PlannedStreamingTurnQueue,
    /// Built request snapshots captured before execution.
    /// 执行前捕获到的请求快照集合。
    built_requests: Arc<Mutex<Vec<ProviderRequestSnapshot>>>,
}

impl MockStreamingProviderAdapter {
    /// Creates one mock streaming provider adapter from the planned turn sequence.
    /// 根据计划中的回合序列创建一个模拟流式 Provider 适配器。
    fn new(planned_turns: Vec<PlannedStreamingTurn>) -> Self {
        Self {
            provider_id: String::from("mock-openai-streaming"),
            model_id: String::from("mock-model"),
            planned_turns: Arc::new(Mutex::new(VecDeque::from(planned_turns))),
            built_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Returns the built request snapshots captured by the streaming adapter so far.
    /// 返回流式适配器截至当前捕获到的请求快照集合。
    fn built_requests(&self) -> Vec<ProviderRequestSnapshot> {
        self.built_requests
            .lock()
            .expect("lock streaming built requests")
            .clone()
    }
}

#[async_trait]
impl VulcanProviderAdapter for MockProviderAdapter {
    type Request = ProviderRequestSnapshot;
    type Response = ProviderTurnResponse;

    /// Returns the stable provider identifier owned by the mock adapter.
    /// 返回由模拟适配器持有的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier owned by the mock adapter.
    /// 返回由模拟适配器持有的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one request snapshot directly from the controller snapshot for later assertions.
    /// 直接根据控制器快照构建一份请求快照，供后续断言使用。
    async fn build_request(
        &self,
        snapshot: &ConversationSnapshot,
        config: &ControllerConfig,
    ) -> Result<Self::Request, CoreError> {
        let request = ProviderRequestSnapshot {
            provider_id: config.provider_id.clone(),
            model_id: config.model_id.clone(),
            system_prompt: config.system_prompt.clone(),
            prompt_layers: config.prompt_layers.clone(),
            prompt: String::from("mock-user-prompt"),
            prompt_in_history: false,
            history: snapshot.messages.clone(),
            temperature: config.temperature,
            max_turns: config.max_turns,
            tool_schemas: vec![ProviderRequestToolSchema {
                id: String::from("mock-tool"),
                name: String::from("mock-tool"),
                description: String::from("mock tool"),
                source_kind: String::from("native"),
                source_label: String::from("test"),
                parameters_schema: json!({"type":"object"}),
            }],
            additional_params: config.additional_params.clone(),
            request_headers: None,
            continuation: snapshot.continuation.clone(),
        };
        self.built_requests
            .lock()
            .expect("lock built requests")
            .push(request.clone());
        Ok(request)
    }

    /// Returns the next pre-planned turn response or fails when the queue has been exhausted.
    /// 返回下一个预先规划的回合响应，或在队列耗尽时失败。
    async fn execute_request(&self, _request: Self::Request) -> Result<Self::Response, CoreError> {
        self.planned_responses
            .lock()
            .expect("lock planned responses")
            .pop_front()
            .ok_or_else(|| CoreError::InvalidState {
                message: String::from("模拟 Provider 响应队列已耗尽。"),
            })
    }
}

#[async_trait]
impl VulcanProviderAdapter for MockStreamingProviderAdapter {
    type Request = ProviderRequestSnapshot;
    type Response = ProviderTurnResponse;

    /// Returns the stable provider identifier owned by the mock streaming adapter.
    /// 返回由模拟流式适配器持有的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier owned by the mock streaming adapter.
    /// 返回由模拟流式适配器持有的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one request snapshot directly from the controller snapshot for later assertions.
    /// 直接根据控制器快照构建一份请求快照，供后续断言使用。
    async fn build_request(
        &self,
        snapshot: &ConversationSnapshot,
        config: &ControllerConfig,
    ) -> Result<Self::Request, CoreError> {
        let request = ProviderRequestSnapshot {
            provider_id: config.provider_id.clone(),
            model_id: config.model_id.clone(),
            system_prompt: config.system_prompt.clone(),
            prompt_layers: config.prompt_layers.clone(),
            prompt: String::from("mock-streaming-user-prompt"),
            prompt_in_history: false,
            history: snapshot.messages.clone(),
            temperature: config.temperature,
            max_turns: config.max_turns,
            tool_schemas: vec![ProviderRequestToolSchema {
                id: String::from("mock-tool"),
                name: String::from("mock-tool"),
                description: String::from("mock tool"),
                source_kind: String::from("native"),
                source_label: String::from("test"),
                parameters_schema: json!({"type":"object"}),
            }],
            additional_params: config.additional_params.clone(),
            request_headers: None,
            continuation: snapshot.continuation.clone(),
        };
        self.built_requests
            .lock()
            .expect("lock built requests")
            .push(request.clone());
        Ok(request)
    }

    /// Mock streaming adapters are expected to use `execute_request_streaming` instead of the one-shot request path.
    /// 模拟流式适配器应当使用 `execute_request_streaming`，而不是一次性请求路径。
    async fn execute_request(&self, _request: Self::Request) -> Result<Self::Response, CoreError> {
        Err(CoreError::InvalidState {
            message: String::from("模拟流式适配器不支持一次性 execute_request 调用。"),
        })
    }
}

#[async_trait]
impl VulcanStreamingProviderAdapter for MockStreamingProviderAdapter {
    /// Emits the pre-planned delta sequence into the provided sink and then returns the next final turn response.
    /// 将预先规划的增量序列写入给定 sink，并在随后返回下一条最终回合响应。
    async fn execute_request_streaming<S>(
        &self,
        _request: Self::Request,
        sink: &mut S,
    ) -> Result<Self::Response, CoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        let (deltas, response) = self
            .planned_turns
            .lock()
            .expect("lock planned streaming turns")
            .pop_front()
            .ok_or_else(|| CoreError::InvalidState {
                message: String::from("模拟流式 Provider 响应队列已耗尽。"),
            })?;
        for delta in deltas {
            sink.push_delta(delta);
        }
        Ok(response)
    }
}

/// Executes pre-planned tool outcomes and records each incoming tool request.
/// 执行预先规划的工具结果，并记录每一次收到的工具请求。
#[derive(Debug, Clone, Default)]
struct MockToolExecutor {
    /// Pre-planned tool outcomes keyed by tool name.
    /// 按工具名称索引的预先规划工具结果。
    outcomes_by_tool: MockToolOutcomeMap,
    /// Recorded tool invocation requests received by the executor.
    /// 被执行器收到并记录下来的工具调用请求集合。
    executed_requests: Arc<Mutex<Vec<ToolInvocationRequest>>>,
}

impl MockToolExecutor {
    /// Creates one empty mock tool executor.
    /// 创建一个空白的模拟工具执行器。
    fn new() -> Self {
        Self::default()
    }

    /// Registers one pre-planned tool outcome for a specific tool name.
    /// 为指定工具名称注册一条预先规划的工具结果。
    fn with_outcome(
        mut self,
        tool_name: &str,
        model_output: &str,
        host_result: Option<Value>,
        is_error: bool,
    ) -> Self {
        self.outcomes_by_tool.insert(
            tool_name.to_string(),
            Ok((model_output.to_string(), host_result, is_error)),
        );
        self
    }

    /// Registers one pre-planned tool failure for a specific tool name.
    /// 为指定工具名称注册一条预先规划的工具失败。
    fn with_failure(mut self, tool_name: &str, message: &str) -> Self {
        self.outcomes_by_tool
            .insert(tool_name.to_string(), Err(message.to_string()));
        self
    }

    /// Returns the recorded tool invocation requests captured by the executor so far.
    /// 返回执行器截至当前捕获到的工具调用请求集合。
    fn executed_requests(&self) -> Vec<ToolInvocationRequest> {
        self.executed_requests
            .lock()
            .expect("lock executed requests")
            .clone()
    }
}

#[async_trait]
impl VulcanToolExecutor for MockToolExecutor {
    /// Executes one tool request by looking up the pre-planned result for the requested tool.
    /// 通过查找被请求工具的预设结果来执行一次工具请求。
    async fn execute_tool(
        &self,
        request: &ToolInvocationRequest,
    ) -> Result<ToolInvocationResult, CoreError> {
        self.executed_requests
            .lock()
            .expect("lock executed requests")
            .push(request.clone());
        let (model_output, host_result, is_error) = self
            .outcomes_by_tool
            .get(&request.tool_name)
            .cloned()
            .ok_or_else(|| CoreError::InvalidState {
                message: format!("未找到工具 `{}` 的模拟结果。", request.tool_name),
            })?
            .map_err(|message| CoreError::ToolFailure {
                tool_name: request.tool_name.clone(),
                message,
            })?;
        Ok(ToolInvocationResult {
            tool_call_id: request.tool_call_id.clone(),
            tool_name: request.tool_name.clone(),
            model_output,
            host_result,
            is_error,
        })
    }
}

/// Records overlapping tool execution so batch-level concurrency is directly observable.
/// 记录重叠的工具执行，使批量工具并发可以被直接观察。
#[derive(Debug, Clone, Default)]
struct ConcurrentToolExecutor {
    /// Number of tool calls currently inside the mock executor.
    /// 当前进入模拟执行器的工具调用数量。
    current_running: Arc<AtomicUsize>,
    /// Maximum number of simultaneously running tool calls observed by the mock executor.
    /// 模拟执行器观测到的最大同时运行工具调用数量。
    max_running: Arc<AtomicUsize>,
}

#[async_trait]
impl VulcanToolExecutor for ConcurrentToolExecutor {
    /// Executes one delayed tool request while tracking concurrent in-flight calls.
    /// 执行一次带延迟的工具请求，同时跟踪并发中的调用数量。
    async fn execute_tool(
        &self,
        request: &ToolInvocationRequest,
    ) -> Result<ToolInvocationResult, CoreError> {
        let current = self.current_running.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_running.fetch_max(current, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        self.current_running.fetch_sub(1, Ordering::SeqCst);
        Ok(ToolInvocationResult {
            tool_call_id: request.tool_call_id.clone(),
            tool_name: request.tool_name.clone(),
            model_output: format!("{} done", request.tool_name),
            host_result: None,
            is_error: false,
        })
    }
}

/// Builds one baseline controller configuration for loop tests.
/// 为循环测试构建一份基线控制器配置。
fn build_test_config(max_turns: usize) -> ControllerConfig {
    ControllerConfig {
        provider_id: String::from("mock-openai"),
        model_id: String::from("mock-model"),
        system_prompt: String::from("system"),
        prompt_layers: ProviderPromptLayers::from_system_prompt("system"),
        temperature: Some(0.1),
        max_turns,
        additional_params: Some(json!({"reasoning_effort":"medium"})),
        context_injection: None,
    }
}

/// Verifies inter-agent injections stay provider-visible but transcript-hidden.
/// 验证代理间注入对 Provider 可见，但在转录显示中保持隐藏。
#[test]
fn append_agent_message_keeps_transcript_record_hidden() {
    let mut controller =
        VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
            .expect("valid core conversation snapshot");

    controller.append_agent_message(AgentMessageAppendRequest {
        message_type: String::from("FINAL_ANSWER"),
        task_name: String::from("code-review"),
        sender: String::from("code-review"),
        recipient: String::from("parent"),
        payload: String::from("review complete"),
        trigger_turn: true,
        metadata: Some(json!({ "source": "test" })),
    });

    let snapshot = controller.snapshot();
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(snapshot.records.len(), 1);
    assert!(matches!(
        &snapshot.messages[0].payload,
        ConversationPayload::AgentMessage {
            task_name,
            payload,
            trigger_turn,
            ..
        } if task_name == "code-review" && payload == "review complete" && *trigger_turn
    ));
    assert!(!snapshot.records[0].visible);
    assert!(snapshot.records[0].body.contains("<agent_message>"));
    assert!(snapshot.records[0].body.contains("review complete"));
}

/// Verifies a preseeded typed agent message is not duplicated as a plain user turn.
/// 验证已预播种的类型化智能体消息不会被重复追加为普通用户回合。
#[test]
fn preseeded_agent_message_prompt_reuses_existing_tail() {
    let mut controller =
        VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
            .expect("valid core conversation snapshot");

    controller.append_agent_message(AgentMessageAppendRequest {
        message_type: String::from("MESSAGE"),
        task_name: String::from("worker-a"),
        sender: String::from("parent:root"),
        recipient: String::from("worker-a"),
        payload: String::from("continue with this context"),
        trigger_turn: true,
        metadata: None,
    });

    let prompt = ConversationPayload::render_agent_message_text(
        "MESSAGE",
        "worker-a",
        "parent:root",
        "continue with this context",
    );

    assert!(!controller.should_append_user_turn(&prompt));
}

/// Verifies display-only records do not enter provider-visible history.
/// 验证仅显示记录不会进入 Provider 可见历史。
#[test]
fn append_display_record_keeps_provider_history_unchanged() {
    let mut controller =
        VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
            .expect("valid core conversation snapshot");

    let record_id = controller
        .append_display_record(DisplayRecordAppendRequest {
            record_id: Some(String::from("display-agent-message-1")),
            role: TranscriptRole::User,
            title: String::from("智能体消息"),
            body: String::from("请用中文重新输出结论。"),
            metadata: Some(json!({ "display_note": true, "kind": "subagent_message" })),
        })
        .expect("valid display record identity")
        .expect("display record should be appended");

    let snapshot = controller.snapshot();
    assert_eq!(record_id, "display-agent-message-1");
    assert!(snapshot.messages.is_empty());
    assert_eq!(snapshot.records.len(), 1);
    assert!(matches!(
        snapshot.records[0].record_kind,
        TranscriptRecordKind::ProjectionOnly
    ));
    assert!(snapshot.records[0].visible);
    assert_eq!(snapshot.records[0].body, "请用中文重新输出结论。");
}

/// Verifies controller construction rejects duplicate canonical identities before ownership transfer.
/// 验证控制器构造会在接管所有权前拒绝重复的规范身份。
#[test]
fn controller_construction_rejects_duplicate_snapshot_identities() {
    // duplicate_message is reused verbatim to prove the message namespace rejects the later owner.
    // duplicate_message 被逐字复用，用于证明消息命名空间会拒绝后出现的所有者。
    let duplicate_message = ConversationMessage {
        id: String::from("duplicate-message"),
        payload: ConversationPayload::UserText {
            text: String::from("hello"),
            attachments: Vec::new(),
        },
    };
    // snapshot contains one exact duplicate in the canonical message window.
    // snapshot 在规范消息窗口中包含一个精确重复项。
    let snapshot = ConversationSnapshot {
        messages: vec![duplicate_message.clone(), duplicate_message],
        ..ConversationSnapshot::default()
    };

    let error = VulcanCoreController::new(build_test_config(4), snapshot)
        .expect_err("duplicate snapshot must be rejected");

    assert!(matches!(error, CoreError::InvalidState { .. }));
    assert!(
        error
            .to_string()
            .contains("duplicates id \"duplicate-message\"")
    );
}

/// Verifies snapshot replacement is atomic when the replacement record namespace is invalid.
/// 验证替换记录命名空间无效时快照替换保持原子性。
#[test]
fn snapshot_replacement_is_atomic_on_record_identity_failure() {
    let mut controller =
        VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
            .expect("valid core conversation snapshot");
    controller
        .append_display_record(DisplayRecordAppendRequest {
            record_id: Some(String::from("retained-record")),
            role: TranscriptRole::Assistant,
            title: String::from("Retained"),
            body: String::from("retained body"),
            metadata: None,
        })
        .expect("valid retained record identity")
        .expect("nonblank record should append");
    // original_snapshot captures the complete authoritative state before replacement.
    // original_snapshot 捕获替换前的完整权威状态。
    let original_snapshot =
        serde_json::to_value(controller.snapshot()).expect("original snapshot should serialize");
    // duplicate_record is reused to make the replacement record namespace invalid.
    // duplicate_record 被复用以使替换记录命名空间无效。
    let duplicate_record = controller.snapshot().records[0].clone();
    let replacement = ConversationSnapshot {
        records: vec![duplicate_record.clone(), duplicate_record],
        ..ConversationSnapshot::default()
    };

    controller
        .replace_snapshot(replacement)
        .expect_err("duplicate replacement must fail");

    assert_eq!(
        serde_json::to_value(controller.snapshot())
            .expect("retained snapshot should serialize after failure"),
        original_snapshot
    );
}

/// Verifies caller-supplied display identities reject duplicates without appending a partial row.
/// 验证调用方提供的显示身份会拒绝重复项且不追加局部记录。
#[test]
fn display_record_append_is_atomic_on_duplicate_identity() {
    let mut controller =
        VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
            .expect("valid core conversation snapshot");
    // first_request installs the only valid owner of the supplied durable identity.
    // first_request 安装调用方提供持久化身份的唯一有效所有者。
    let first_request = DisplayRecordAppendRequest {
        record_id: Some(String::from("display-record")),
        role: TranscriptRole::Assistant,
        title: String::from("First"),
        body: String::from("first body"),
        metadata: None,
    };
    controller
        .append_display_record(first_request)
        .expect("valid first display identity")
        .expect("nonblank record should append");
    // original_snapshot proves the rejected duplicate does not change any canonical field.
    // original_snapshot 证明被拒绝的重复项不会修改任何规范字段。
    let original_snapshot =
        serde_json::to_value(controller.snapshot()).expect("original snapshot should serialize");

    let error = controller
        .append_display_record(DisplayRecordAppendRequest {
            record_id: Some(String::from("display-record")),
            role: TranscriptRole::User,
            title: String::from("Duplicate"),
            body: String::from("duplicate body"),
            metadata: None,
        })
        .expect_err("duplicate display identity must fail");

    assert!(
        error
            .to_string()
            .contains("duplicates id \"display-record\"")
    );
    assert_eq!(
        serde_json::to_value(controller.snapshot())
            .expect("snapshot should serialize after duplicate rejection"),
        original_snapshot
    );
}

mod interventions;
mod loop_flow;
mod streaming;
mod tool_batches;
