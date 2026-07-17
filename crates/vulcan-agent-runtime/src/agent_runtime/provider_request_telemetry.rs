//! Low-frequency structured telemetry for physical Provider API requests.
//! 真实 Provider API 请求的低频结构化遥测。

use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionPromptPurpose};
use crate::model::SessionId;
use crate::provider_config::ProviderApiFamily;
use serde::Serialize;
use serde_json::{Value, to_value};
use time::{OffsetDateTime, UtcOffset};
use uuid::Uuid;
use vulcan_core::types::{ProviderRequestSnapshot, ProviderUsageSnapshot};

/// Hook point fired after one physical Provider request starts.
/// 一次真实 Provider 请求开始后触发的 Hook 点。
pub const HOOK_PROVIDER_REQUEST_STARTED: &str = "provider.request.started";

/// Hook point fired after one physical Provider response exposes its first visible token.
/// 一次真实 Provider 响应暴露首个可见 Token 后触发的 Hook 点。
pub const HOOK_PROVIDER_RESPONSE_FIRST_TOKEN: &str = "provider.response.first_token";

/// Hook point fired after one physical Provider request reaches exactly one terminal state.
/// 一次真实 Provider 请求到达唯一终态后触发的 Hook 点。
pub const HOOK_PROVIDER_REQUEST_COMPLETED: &str = "provider.request.completed";

/// Hook point fired while the transport waits before an automatic retry.
/// 传输层在自动重试前等待时触发的 Hook 点。
pub const HOOK_PROVIDER_REQUEST_RETRYING: &str = "provider.request.retrying";

/// Immutable run metadata captured from the unique execution preparation source.
/// 从唯一执行准备来源捕获的不可变运行元数据。
#[derive(Debug, Clone)]
pub(crate) struct ProviderRequestTelemetryContext {
    /// Host-generated execution identity shared by requests in one stream.
    /// 单次流内请求共享的宿主生成执行身份。
    execution_id: String,
    /// Current session identity.
    /// 当前会话身份。
    session_id: SessionId,
    /// Exact workspace identity captured from execution preparation.
    /// 从执行准备捕获的精确工作区身份。
    workspace_id: String,
    /// Current provider display name from the runtime blueprint.
    /// 来自运行时蓝图的当前 Provider 显示名称。
    provider_name: String,
    /// Current model display name from the runtime blueprint.
    /// 来自运行时蓝图的当前模型显示名称。
    model_name: String,
    /// Canonical API family that defines whether Provider input already excludes cached tokens.
    /// 定义 Provider 输入是否已经排除缓存 Token 的规范接口族。
    api_family: ProviderApiFamily,
    /// Stable owner category derived from explicit preparation fields.
    /// 从显式执行准备字段派生的稳定归属类别。
    owner_kind: &'static str,
    /// Stable active agent identity.
    /// 稳定激活代理身份。
    owner_id: String,
    /// Current active agent display name.
    /// 当前激活代理显示名称。
    owner_name: String,
}

impl ProviderRequestTelemetryContext {
    /// Builds one run context from the validated execution preparation.
    /// 从已校验执行准备构建一个运行上下文。
    ///
    /// # Parameters
    /// 参数
    /// `preparation` is the unique source of model and owner facts for this run.
    /// `preparation` 是本次运行模型与归属事实的唯一来源。
    ///
    /// # Returns
    /// 返回值
    /// Returns an immutable context with a new host execution identity.
    /// 返回带有新宿主执行身份的不可变上下文。
    pub(crate) fn from_preparation(preparation: &ExecutionPreparation) -> Self {
        // ownerKind is derived from explicit purpose and subagent context, never from names.
        // ownerKind 从显式用途与子代理上下文派生，绝不根据名称猜测。
        let owner_kind = match preparation.prompt_purpose {
            ExecutionPromptPurpose::ContextCompaction
            | ExecutionPromptPurpose::ContextCompactionCachePreserving => "context_compaction",
            ExecutionPromptPurpose::GitCommitSummary => "git_commit_summary",
            ExecutionPromptPurpose::UserTurn if preparation.subagent_prompt_context.is_some() => {
                "subagent"
            }
            ExecutionPromptPurpose::UserTurn => "main_agent",
        };
        Self {
            execution_id: Uuid::new_v4().to_string(),
            session_id: preparation.session_id,
            workspace_id: preparation.workspace_path.clone(),
            provider_name: preparation.model_blueprint.provider_name.clone(),
            model_name: preparation.model_blueprint.model_name.clone(),
            api_family: preparation.model_blueprint.api_family(),
            owner_kind,
            owner_id: preparation.agent_id.clone(),
            owner_name: preparation.agent_name.clone(),
        }
    }
}

/// Stable telemetry event kind mapped directly to one plugin Hook point.
/// 直接映射到单个插件 Hook 点的稳定遥测事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRequestTelemetryEventKind {
    /// Physical request started.
    /// 真实请求已开始。
    Started,
    /// First visible token observed.
    /// 已观测到首个可见 Token。
    FirstToken,
    /// Physical request reached a terminal state.
    /// 真实请求已到达终态。
    Completed,
    /// Transport is waiting before retry.
    /// 传输层正在重试前等待。
    Retrying,
}

impl ProviderRequestTelemetryEventKind {
    /// Returns the exact registered Hook point for this event kind.
    /// 返回当前事件类型对应的精确已注册 Hook 点。
    ///
    /// # Returns
    /// 返回值
    /// Returns one stable Hook point key.
    /// 返回一个稳定 Hook 点键。
    pub const fn hook_point(self) -> &'static str {
        match self {
            Self::Started => HOOK_PROVIDER_REQUEST_STARTED,
            Self::FirstToken => HOOK_PROVIDER_RESPONSE_FIRST_TOKEN,
            Self::Completed => HOOK_PROVIDER_REQUEST_COMPLETED,
            Self::Retrying => HOOK_PROVIDER_REQUEST_RETRYING,
        }
    }
}

/// One JSON-safe telemetry event forwarded through the execution stream.
/// 通过执行流转发的一条 JSON 安全遥测事件。
#[derive(Debug, Clone)]
pub struct ProviderRequestTelemetryEvent {
    /// Registered Hook point kind.
    /// 已注册 Hook 点类型。
    pub kind: ProviderRequestTelemetryEventKind,
    /// Session that owns the request.
    /// 拥有该请求的会话。
    pub session_id: SessionId,
    /// Versioned JSON payload without prompts, outputs, headers, or credentials.
    /// 不含提示词、输出、请求头或凭据的版本化 JSON 载荷。
    pub payload: Value,
}

/// Service-edge tracker that closes an in-flight request when its consumer stream is dropped.
/// 服务边界跟踪器，在消费流被丢弃时关闭尚未完成的请求。
#[derive(Debug, Default)]
pub struct ProviderRequestTelemetryCancellationTracker {
    /// Started payload for the sole active physical request observed by this stream consumer.
    /// 当前流消费方观测到的唯一活动真实请求开始载荷。
    active_started_payload: Option<Value>,
    /// Session identity copied from the active started event.
    /// 从活动开始事件复制的会话身份。
    active_session_id: Option<SessionId>,
}

impl ProviderRequestTelemetryCancellationTracker {
    /// Observes one ordered telemetry event without changing its plugin-facing payload.
    /// 观测一条有序遥测事件，不修改其面向插件的载荷。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the exact event already emitted by the Provider telemetry state machine.
    /// `event` 是 Provider 遥测状态机已经发出的精确事件。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value.
    /// 不返回值。
    pub fn observe(&mut self, event: &ProviderRequestTelemetryEvent) {
        match event.kind {
            ProviderRequestTelemetryEventKind::Started => {
                self.active_started_payload = Some(event.payload.clone());
                self.active_session_id = Some(event.session_id);
            }
            ProviderRequestTelemetryEventKind::FirstToken => {
                // activePayload is updated only when the request identity matches exactly.
                // activePayload 仅在请求身份精确匹配时更新。
                let Some(active_payload) = self.active_started_payload.as_mut() else {
                    return;
                };
                if active_payload.get("request_id") != event.payload.get("request_id") {
                    return;
                }
                if let Some(active_object) = active_payload.as_object_mut() {
                    active_object.insert(
                        String::from("first_token_at_ms"),
                        event
                            .payload
                            .get("first_token_at_ms")
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                    active_object.insert(
                        String::from("time_to_first_token_ms"),
                        event
                            .payload
                            .get("time_to_first_token_ms")
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                }
            }
            ProviderRequestTelemetryEventKind::Completed => {
                if self
                    .active_started_payload
                    .as_ref()
                    .and_then(|payload| payload.get("request_id"))
                    == event.payload.get("request_id")
                {
                    self.active_started_payload = None;
                    self.active_session_id = None;
                }
            }
            ProviderRequestTelemetryEventKind::Retrying => {}
        }
    }

    /// Converts the sole active request into one redacted cancelled terminal event.
    /// 将唯一活动请求转换为一条脱敏的取消终态事件。
    ///
    /// # Returns
    /// 返回值
    /// Returns one completed event when a started request remains active, otherwise none.
    /// 当开始请求仍处于活动状态时返回一条完成事件，否则返回空。
    pub fn take_cancelled_event(&mut self) -> Option<ProviderRequestTelemetryEvent> {
        // payload starts from the exact started identity and calendar facts.
        // payload 从精确的开始身份与日历事实构建。
        let mut payload = self.active_started_payload.take()?;
        // sessionId was captured from the same started event.
        // sessionId 从同一开始事件捕获。
        let session_id = self.active_session_id.take()?;
        // completedAt is captured once for every derived duration field.
        // completedAt 为全部派生时长字段捕获一次。
        let completed_at_ms = current_unix_millis();
        // startedAt is required by the versioned started-event schema.
        // startedAt 是版本化开始事件 Schema 的必填字段。
        let started_at_ms = payload.get("started_at_ms")?.as_i64()?;
        // firstTokenAt is the only exact first-event timing available at this low-frequency boundary.
        // firstTokenAt 是当前低频边界可用的唯一精确首事件时间。
        let first_token_at_ms = payload
            .get("first_token_at_ms")
            .cloned()
            .unwrap_or(Value::Null);
        // firstTokenDuration is reused as first-event duration only when the visible token was observed.
        // firstTokenDuration 仅在已观测可见 Token 时复用为首事件耗时。
        let first_token_duration = payload
            .get("time_to_first_token_ms")
            .cloned()
            .unwrap_or(Value::Null);
        // payloadObject is guaranteed by the internal telemetry serializer.
        // payloadObject 由内部遥测序列化器保证为对象。
        let payload_object = payload.as_object_mut()?;
        payload_object.insert(
            String::from("status"),
            Value::String(String::from("cancelled")),
        );
        payload_object.insert(String::from("first_event_at_ms"), first_token_at_ms);
        payload_object
            .entry(String::from("first_token_at_ms"))
            .or_insert(Value::Null);
        payload_object.insert(
            String::from("completed_at_ms"),
            Value::from(completed_at_ms),
        );
        payload_object.insert(String::from("time_to_first_event_ms"), first_token_duration);
        payload_object
            .entry(String::from("time_to_first_token_ms"))
            .or_insert(Value::Null);
        payload_object.insert(
            String::from("duration_ms"),
            Value::from(elapsed_millis(started_at_ms, completed_at_ms)),
        );
        for token_field in [
            "input_tokens",
            "uncached_input_tokens",
            "output_tokens",
            "reasoning_tokens",
            "total_tokens",
        ] {
            payload_object.insert(String::from(token_field), Value::from(0));
        }
        payload_object.insert(String::from("cached_input_tokens"), Value::Null);
        payload_object.insert(String::from("cache_creation_input_tokens"), Value::Null);
        payload_object.insert(String::from("finish_reason"), Value::Null);
        payload_object.insert(
            String::from("error_category"),
            Value::String(String::from("host_cancelled")),
        );
        payload_object.insert(
            String::from("error_code"),
            Value::String(String::from("consumer_stream_dropped")),
        );
        Some(ProviderRequestTelemetryEvent {
            kind: ProviderRequestTelemetryEventKind::Completed,
            session_id,
            payload,
        })
    }
}

/// Mutable state machine that guarantees at most one terminal event per started request.
/// 保证每个已开始请求最多一个终态事件的可变状态机。
#[derive(Debug, Default)]
pub(crate) struct ProviderRequestTelemetryTracker {
    /// Optional immutable run context installed after preparation.
    /// 执行准备后安装的可选不可变运行上下文。
    context: Option<ProviderRequestTelemetryContext>,
    /// Currently active physical request.
    /// 当前活动真实请求。
    active_request: Option<ActiveProviderRequestTelemetry>,
    /// Zero-based physical request index within the execution stream.
    /// 执行流内从零开始的真实请求索引。
    next_attempt_index: u32,
    /// Number of transport retries completed before the next physical request.
    /// 下一次真实请求前已完成的传输重试次数。
    retry_count: u32,
}

impl ProviderRequestTelemetryTracker {
    /// Installs immutable run context before the first request starts.
    /// 在首个请求开始前安装不可变运行上下文。
    ///
    /// # Parameters
    /// 参数
    /// `context` contains facts copied from validated execution preparation.
    /// `context` 包含从已校验执行准备复制的事实。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value.
    /// 不返回值。
    pub(crate) fn install_context(&mut self, context: ProviderRequestTelemetryContext) {
        self.context = Some(context);
    }

    /// Starts one physical request and closes any impossible overlapping request explicitly.
    /// 开始一次真实请求，并显式关闭任何不可能存在的重叠请求。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the exact provider-neutral request dispatched by the controller.
    /// `snapshot` 是控制器分发的精确 Provider 中立请求。
    ///
    /// # Returns
    /// 返回值
    /// Returns zero events without context, otherwise one started event plus an invariant failure when needed.
    /// 缺少上下文时不返回事件；否则返回开始事件，并在需要时附带不变量失败事件。
    pub(crate) fn start_request(
        &mut self,
        snapshot: &ProviderRequestSnapshot,
    ) -> Vec<ProviderRequestTelemetryEvent> {
        let Some(context) = self.context.clone() else {
            return Vec::new();
        };
        // events preserves the terminal-before-next-start ordering for invariant violations.
        // events 在不变量违规时保留先终态、后新开始的顺序。
        let mut events = Vec::new();
        if let Some(active_request) = self.active_request.take() {
            events.push(build_completed_event(
                &context,
                active_request,
                ProviderRequestTerminalStatus::Failed,
                None,
                Some("telemetry_invariant"),
                Some("overlapping_request"),
            ));
        }
        // startedAt captures one timestamp for both state and payload.
        // startedAt 为状态与载荷捕获同一个时间戳。
        let started_at = telemetry_clock_snapshot();
        // activeRequest is the sole mutable owner of terminal emission state.
        // activeRequest 是终态发射状态的唯一可变持有者。
        let active_request = ActiveProviderRequestTelemetry {
            request_id: Uuid::new_v4().to_string(),
            provider_key: snapshot.provider_id.clone(),
            model_key: snapshot.model_id.clone(),
            attempt_index: self.next_attempt_index,
            retry_count: self.retry_count,
            started_at_ms: started_at.timestamp_ms,
            first_event_at_ms: None,
            first_token_at_ms: None,
            calendar_date: started_at.calendar_date,
            timezone_offset_minutes: started_at.timezone_offset_minutes,
        };
        self.next_attempt_index = self.next_attempt_index.saturating_add(1);
        events.push(build_started_event(&context, &active_request));
        self.active_request = Some(active_request);
        events
    }

    /// Records the first Provider event timestamp without emitting a high-frequency Hook.
    /// 记录首个 Provider 事件时间戳，但不发出高频 Hook。
    pub(crate) fn observe_first_event(&mut self) {
        if let Some(active_request) = self.active_request.as_mut()
            && active_request.first_event_at_ms.is_none()
        {
            active_request.first_event_at_ms = Some(current_unix_millis());
        }
    }

    /// Records and emits the first visible token event at most once.
    /// 最多一次记录并发出首个可见 Token 事件。
    ///
    /// # Returns
    /// 返回值
    /// Returns the event only for the first visible token of an active request.
    /// 仅对活动请求的首个可见 Token 返回事件。
    pub(crate) fn observe_first_token(&mut self) -> Option<ProviderRequestTelemetryEvent> {
        let context = self.context.as_ref()?;
        let active_request = self.active_request.as_mut()?;
        if active_request.first_event_at_ms.is_none() {
            active_request.first_event_at_ms = Some(current_unix_millis());
        }
        if active_request.first_token_at_ms.is_some() {
            return None;
        }
        // firstTokenAt is captured once and reused by the payload.
        // firstTokenAt 仅捕获一次并由载荷复用。
        let first_token_at_ms = current_unix_millis();
        active_request.first_token_at_ms = Some(first_token_at_ms);
        Some(build_first_token_event(
            context,
            active_request,
            first_token_at_ms,
        ))
    }

    /// Completes the active request successfully at one controller checkpoint.
    /// 在一个控制器检查点成功完成活动请求。
    ///
    /// # Parameters
    /// 参数
    /// `usage` is the optional usage reported for this successful provider turn.
    /// `usage` 是当前成功 Provider 回合报告的可选用量。
    ///
    /// # Returns
    /// 返回值
    /// Returns exactly one terminal event when a request is active.
    /// 当存在活动请求时恰好返回一个终态事件。
    pub(crate) fn complete_success(
        &mut self,
        usage: Option<&ProviderUsageSnapshot>,
    ) -> Option<ProviderRequestTelemetryEvent> {
        let context = self.context.as_ref()?;
        let active_request = self.active_request.take()?;
        Some(build_completed_event(
            context,
            active_request,
            ProviderRequestTerminalStatus::Succeeded,
            usage,
            None,
            None,
        ))
    }

    /// Completes the active request as failed and emits the retry-wait event when applicable.
    /// 将活动请求标记为失败，并在适用时发出重试等待事件。
    ///
    /// # Parameters
    /// 参数
    /// `will_retry` states whether another transport attempt follows, `retry_number` is its index, and `delay_ms` is the wait.
    /// `will_retry` 表示是否继续传输重试，`retry_number` 是其序号，`delay_ms` 是等待时长。
    ///
    /// # Returns
    /// 返回值
    /// Returns one failure terminal event and optionally one retrying event.
    /// 返回一个失败终态事件，并可选返回一个重试事件。
    pub(crate) fn complete_failure(
        &mut self,
        will_retry: bool,
        retry_number: u32,
        max_retries: u32,
        delay_ms: u64,
        error_category: &str,
    ) -> Vec<ProviderRequestTelemetryEvent> {
        let Some(context) = self.context.as_ref() else {
            return Vec::new();
        };
        // events preserves completed-before-retrying ordering.
        // events 保留先完成、后重试的顺序。
        let mut events = Vec::new();
        if let Some(active_request) = self.active_request.take() {
            // completedRequestId links the retry event to the exact failed request.
            // completedRequestId 将重试事件关联到精确失败请求。
            let completed_request_id = active_request.request_id.clone();
            events.push(build_completed_event(
                context,
                active_request,
                ProviderRequestTerminalStatus::Failed,
                None,
                Some(error_category),
                None,
            ));
            if will_retry {
                events.push(build_retrying_event(
                    context,
                    &completed_request_id,
                    retry_number,
                    max_retries,
                    delay_ms,
                    error_category,
                ));
            }
        }
        if will_retry {
            self.retry_count = retry_number;
        }
        events
    }
}

/// Mutable state for one active physical request.
/// 单次活动真实请求的可变状态。
#[derive(Debug)]
struct ActiveProviderRequestTelemetry {
    /// Host-generated unique request identity.
    /// 宿主生成的唯一请求身份。
    request_id: String,
    /// Stable provider key from the dispatched request snapshot.
    /// 来自已分发请求快照的稳定 Provider 键。
    provider_key: String,
    /// Stable model key from the dispatched request snapshot.
    /// 来自已分发请求快照的稳定模型键。
    model_key: String,
    /// Zero-based physical request sequence.
    /// 从零开始的真实请求序号。
    attempt_index: u32,
    /// Number of transport retries completed before this request.
    /// 当前请求前已完成的传输重试次数。
    retry_count: u32,
    /// Request start timestamp.
    /// 请求开始时间戳。
    started_at_ms: i64,
    /// Optional first Provider event timestamp.
    /// 可选首个 Provider 事件时间戳。
    first_event_at_ms: Option<i64>,
    /// Optional first visible token timestamp.
    /// 可选首个可见 Token 时间戳。
    first_token_at_ms: Option<i64>,
    /// Host-local calendar date at request start.
    /// 请求开始时的宿主本地自然日。
    calendar_date: String,
    /// Host-local UTC offset at request start.
    /// 请求开始时的宿主本地 UTC 偏移。
    timezone_offset_minutes: i32,
}

/// Terminal state stored by the statistics fact table.
/// 由统计事实表保存的终态。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProviderRequestTerminalStatus {
    /// Provider request completed successfully.
    /// Provider 请求成功完成。
    Succeeded,
    /// Provider request failed.
    /// Provider 请求失败。
    Failed,
}

/// Shared fields included in every telemetry payload.
/// 每条遥测载荷包含的共享字段。
#[derive(Serialize)]
struct ProviderTelemetryIdentityPayload<'a> {
    /// Payload schema version.
    /// 载荷 Schema 版本。
    schema_version: u32,
    /// Host-generated request identity.
    /// 宿主生成的请求身份。
    request_id: &'a str,
    /// Host-generated execution identity.
    /// 宿主生成的执行身份。
    execution_id: &'a str,
    /// Session identity.
    /// 会话身份。
    session_id: String,
    /// Optional project identity; currently absent because execution preparation has no project key.
    /// 可选项目身份；当前执行准备不含项目键，因此为空。
    project_id: Option<&'a str>,
    /// Exact workspace identity captured for the execution.
    /// 为当前执行捕获的精确工作区身份。
    workspace_id: Option<&'a str>,
    /// Stable owner category.
    /// 稳定归属类别。
    owner_kind: &'a str,
    /// Stable owner identity.
    /// 稳定归属身份。
    owner_id: &'a str,
    /// Current owner display name.
    /// 当前归属显示名称。
    owner_name: &'a str,
    /// Stable Provider key.
    /// 稳定 Provider 键。
    provider_key: &'a str,
    /// Current Provider display name.
    /// 当前 Provider 显示名称。
    provider_name: &'a str,
    /// Stable model key.
    /// 稳定模型键。
    model_key: &'a str,
    /// Current model display name.
    /// 当前模型显示名称。
    model_name: &'a str,
    /// Zero-based physical request index.
    /// 从零开始的真实请求索引。
    attempt_index: u32,
    /// Completed transport retry count.
    /// 已完成传输重试次数。
    retry_count: u32,
}

/// Clock sample retaining epoch and host-local calendar facts from one instant.
/// 从同一时刻保留 Epoch 与宿主本地日历事实的时钟样本。
struct TelemetryClockSnapshot {
    /// Unix timestamp in milliseconds.
    /// Unix 毫秒时间戳。
    timestamp_ms: i64,
    /// Host-local ISO calendar date.
    /// 宿主本地 ISO 自然日。
    calendar_date: String,
    /// Host-local UTC offset in minutes.
    /// 宿主本地 UTC 分钟偏移。
    timezone_offset_minutes: i32,
}

/// Builds the shared identity projection for one active request.
/// 为一个活动请求构建共享身份投影。
fn identity_payload<'a>(
    context: &'a ProviderRequestTelemetryContext,
    request: &'a ActiveProviderRequestTelemetry,
) -> ProviderTelemetryIdentityPayload<'a> {
    ProviderTelemetryIdentityPayload {
        schema_version: 1,
        request_id: &request.request_id,
        execution_id: &context.execution_id,
        session_id: context.session_id.to_string(),
        project_id: None,
        workspace_id: Some(&context.workspace_id),
        owner_kind: context.owner_kind,
        owner_id: &context.owner_id,
        owner_name: &context.owner_name,
        provider_key: &request.provider_key,
        provider_name: &context.provider_name,
        model_key: &request.model_key,
        model_name: &context.model_name,
        attempt_index: request.attempt_index,
        retry_count: request.retry_count,
    }
}

/// Builds one request-started telemetry event.
/// 构建一条请求开始遥测事件。
fn build_started_event(
    context: &ProviderRequestTelemetryContext,
    request: &ActiveProviderRequestTelemetry,
) -> ProviderRequestTelemetryEvent {
    ProviderRequestTelemetryEvent {
        kind: ProviderRequestTelemetryEventKind::Started,
        session_id: context.session_id,
        payload: serialize_payload(StartedPayload {
            identity: identity_payload(context, request),
            started_at_ms: request.started_at_ms,
            calendar_date: &request.calendar_date,
            timezone_offset_minutes: request.timezone_offset_minutes,
        }),
    }
}

/// Builds one first-visible-token telemetry event.
/// 构建一条首个可见 Token 遥测事件。
fn build_first_token_event(
    context: &ProviderRequestTelemetryContext,
    request: &ActiveProviderRequestTelemetry,
    first_token_at_ms: i64,
) -> ProviderRequestTelemetryEvent {
    ProviderRequestTelemetryEvent {
        kind: ProviderRequestTelemetryEventKind::FirstToken,
        session_id: context.session_id,
        payload: serialize_payload(FirstTokenPayload {
            identity: identity_payload(context, request),
            started_at_ms: request.started_at_ms,
            first_token_at_ms,
            time_to_first_token_ms: elapsed_millis(request.started_at_ms, first_token_at_ms),
        }),
    }
}

/// Builds one terminal telemetry event and consumes its active request state.
/// 构建一条终态遥测事件并消费其活动请求状态。
fn build_completed_event(
    context: &ProviderRequestTelemetryContext,
    request: ActiveProviderRequestTelemetry,
    status: ProviderRequestTerminalStatus,
    usage: Option<&ProviderUsageSnapshot>,
    error_category: Option<&str>,
    error_code: Option<&str>,
) -> ProviderRequestTelemetryEvent {
    // completedAt is captured once for all terminal durations.
    // completedAt 为全部终态时长捕获同一时间戳。
    let completed_at_ms = current_unix_millis();
    // normalizedUsage uses provider facts when available and zeros only for absent numeric facts.
    // normalizedUsage 在可用时使用 Provider 事实，仅对缺失数字事实使用零值。
    let normalized_usage = usage.cloned().unwrap_or_default();
    // uncachedInputTokens converts protocol-specific input semantics into the statistics contract once at the telemetry boundary.
    // uncachedInputTokens 在遥测边界一次性把协议专属输入语义转换为统计契约。
    let uncached_input_tokens = uncached_input_tokens(context.api_family, &normalized_usage);
    ProviderRequestTelemetryEvent {
        kind: ProviderRequestTelemetryEventKind::Completed,
        session_id: context.session_id,
        payload: serialize_payload(CompletedPayload {
            identity: identity_payload(context, &request),
            status,
            started_at_ms: request.started_at_ms,
            first_event_at_ms: request.first_event_at_ms,
            first_token_at_ms: request.first_token_at_ms,
            completed_at_ms,
            time_to_first_event_ms: request
                .first_event_at_ms
                .map(|value| elapsed_millis(request.started_at_ms, value)),
            time_to_first_token_ms: request
                .first_token_at_ms
                .map(|value| elapsed_millis(request.started_at_ms, value)),
            duration_ms: elapsed_millis(request.started_at_ms, completed_at_ms),
            input_tokens: normalized_usage.input_tokens.unwrap_or(0),
            uncached_input_tokens,
            output_tokens: normalized_usage.output_tokens.unwrap_or(0),
            reasoning_tokens: normalized_usage.reasoning_tokens.unwrap_or(0),
            total_tokens: normalized_usage.total_tokens.unwrap_or(0),
            cached_input_tokens: normalized_usage.cached_input_tokens,
            cache_creation_input_tokens: normalized_usage.cache_creation_input_tokens,
            finish_reason: None,
            error_category,
            error_code,
            calendar_date: &request.calendar_date,
            timezone_offset_minutes: request.timezone_offset_minutes,
        }),
    }
}

/// Converts Provider-specific input-token semantics into a non-cached input count for statistics consumers.
/// 将 Provider 专属输入 Token 语义转换为供统计消费方使用的非缓存输入量。
///
/// `api_family` identifies the exact wire-protocol usage contract.
/// `api_family` 标识精确的线协议用量契约。
/// `usage` contains the final normalized Provider usage snapshot.
/// `usage` 包含最终规范化 Provider 用量快照。
/// Returns the non-cached input-token count without changing the Provider-reported fields.
/// 返回非缓存输入 Token 数量，且不修改 Provider 原始报告字段。
fn uncached_input_tokens(api_family: ProviderApiFamily, usage: &ProviderUsageSnapshot) -> usize {
    // inputTokens preserves the Provider-reported input value before statistics normalization.
    // inputTokens 在统计归一化前保留 Provider 报告的输入值。
    let input_tokens = usage.input_tokens.unwrap_or_default();
    match api_family {
        ProviderApiFamily::OpenAiResponses
        | ProviderApiFamily::OpenAiChatCompatible
        | ProviderApiFamily::GoogleGeminiGenerateContent => {
            input_tokens.saturating_sub(usage.cached_input_tokens.unwrap_or_default())
        }
        ProviderApiFamily::AnthropicMessages
        | ProviderApiFamily::CohereChat
        | ProviderApiFamily::BedrockConverse => input_tokens,
    }
}

/// Builds one retry-wait telemetry event linked to the failed physical request.
/// 构建一条关联失败真实请求的重试等待遥测事件。
fn build_retrying_event(
    context: &ProviderRequestTelemetryContext,
    request_id: &str,
    retry_number: u32,
    max_retries: u32,
    delay_ms: u64,
    error_category: &str,
) -> ProviderRequestTelemetryEvent {
    ProviderRequestTelemetryEvent {
        kind: ProviderRequestTelemetryEventKind::Retrying,
        session_id: context.session_id,
        payload: serialize_payload(RetryingPayload {
            schema_version: 1,
            request_id,
            execution_id: &context.execution_id,
            session_id: context.session_id.to_string(),
            retry_number,
            max_retries,
            delay_ms,
            error_category,
            retrying_at_ms: current_unix_millis(),
        }),
    }
}

/// Started-event payload.
/// 请求开始事件载荷。
#[derive(Serialize)]
struct StartedPayload<'a> {
    /// Shared identity fields flattened into the payload.
    /// 平铺到载荷中的共享身份字段。
    #[serde(flatten)]
    identity: ProviderTelemetryIdentityPayload<'a>,
    /// Request start timestamp.
    /// 请求开始时间戳。
    started_at_ms: i64,
    /// Host-local calendar date.
    /// 宿主本地自然日。
    calendar_date: &'a str,
    /// Host-local UTC offset in minutes.
    /// 宿主本地 UTC 分钟偏移。
    timezone_offset_minutes: i32,
}

/// First-token event payload.
/// 首 Token 事件载荷。
#[derive(Serialize)]
struct FirstTokenPayload<'a> {
    /// Shared identity fields flattened into the payload.
    /// 平铺到载荷中的共享身份字段。
    #[serde(flatten)]
    identity: ProviderTelemetryIdentityPayload<'a>,
    /// Request start timestamp.
    /// 请求开始时间戳。
    started_at_ms: i64,
    /// First visible token timestamp.
    /// 首个可见 Token 时间戳。
    first_token_at_ms: i64,
    /// Milliseconds from start to first visible token.
    /// 从开始到首个可见 Token 的毫秒数。
    time_to_first_token_ms: u64,
}

/// Completed-event payload.
/// 请求完成事件载荷。
#[derive(Serialize)]
struct CompletedPayload<'a> {
    /// Shared identity fields flattened into the payload.
    /// 平铺到载荷中的共享身份字段。
    #[serde(flatten)]
    identity: ProviderTelemetryIdentityPayload<'a>,
    /// Exact terminal status.
    /// 精确终态。
    status: ProviderRequestTerminalStatus,
    /// Request start timestamp.
    /// 请求开始时间戳。
    started_at_ms: i64,
    /// Optional first Provider event timestamp.
    /// 可选首个 Provider 事件时间戳。
    first_event_at_ms: Option<i64>,
    /// Optional first visible token timestamp.
    /// 可选首个可见 Token 时间戳。
    first_token_at_ms: Option<i64>,
    /// Terminal timestamp.
    /// 终态时间戳。
    completed_at_ms: i64,
    /// Optional first-event latency.
    /// 可选首事件延迟。
    time_to_first_event_ms: Option<u64>,
    /// Optional first-token latency.
    /// 可选首 Token 延迟。
    time_to_first_token_ms: Option<u64>,
    /// Total request duration.
    /// 请求总时长。
    duration_ms: u64,
    /// Provider-reported input tokens or zero when absent.
    /// Provider 报告的输入 Token，缺失时为零。
    input_tokens: usize,
    /// Protocol-normalized non-cached input tokens.
    /// 按协议规范化后的非缓存输入 Token。
    uncached_input_tokens: usize,
    /// Provider-reported output tokens or zero when absent.
    /// Provider 报告的输出 Token，缺失时为零。
    output_tokens: usize,
    /// Provider-reported reasoning tokens or zero when absent.
    /// Provider 报告的推理 Token，缺失时为零。
    reasoning_tokens: usize,
    /// Provider-reported total tokens or zero when absent.
    /// Provider 报告的总 Token，缺失时为零。
    total_tokens: usize,
    /// Optional provider-reported cache-hit input tokens.
    /// 可选 Provider 报告缓存命中输入 Token。
    cached_input_tokens: Option<usize>,
    /// Optional provider-reported cache-creation input tokens.
    /// 可选 Provider 报告缓存写入输入 Token。
    cache_creation_input_tokens: Option<usize>,
    /// Optional finish reason; currently absent because the normalized controller usage has no such field.
    /// 可选结束原因；当前规范控制器用量不含此字段，因此为空。
    finish_reason: Option<&'a str>,
    /// Optional stable error category.
    /// 可选稳定错误类别。
    error_category: Option<&'a str>,
    /// Optional stable error code.
    /// 可选稳定错误码。
    error_code: Option<&'a str>,
    /// Host-local calendar date at request start.
    /// 请求开始时的宿主本地自然日。
    calendar_date: &'a str,
    /// Host-local UTC offset at request start.
    /// 请求开始时的宿主本地 UTC 偏移。
    timezone_offset_minutes: i32,
}

/// Retrying-event payload.
/// 重试事件载荷。
#[derive(Serialize)]
struct RetryingPayload<'a> {
    /// Payload schema version.
    /// 载荷 Schema 版本。
    schema_version: u32,
    /// Failed physical request identity.
    /// 失败真实请求身份。
    request_id: &'a str,
    /// Host-generated execution identity.
    /// 宿主生成的执行身份。
    execution_id: &'a str,
    /// Session identity.
    /// 会话身份。
    session_id: String,
    /// One-based retry number.
    /// 从一开始的重试序号。
    retry_number: u32,
    /// Configured retry ceiling, or zero for cancellation-bound rate-limit retries.
    /// 已配置的重试上限；仅由取消终止的限流重试为零。
    max_retries: u32,
    /// Retry wait duration.
    /// 重试等待时长。
    delay_ms: u64,
    /// Stable redacted failure category.
    /// 稳定脱敏失败类别。
    error_category: &'a str,
    /// Retry decision timestamp.
    /// 重试决策时间戳。
    retrying_at_ms: i64,
}

/// Serializes an internal telemetry payload that only contains JSON-compatible fields.
/// 序列化一份仅含 JSON 兼容字段的内部遥测载荷。
fn serialize_payload(payload: impl Serialize) -> Value {
    to_value(payload).expect("provider telemetry payload should always be JSON serializable")
}

/// Captures one epoch and local-calendar clock sample.
/// 捕获一份 Epoch 与本地日历时钟样本。
fn telemetry_clock_snapshot() -> TelemetryClockSnapshot {
    // nowUtc is the single source instant for epoch and local projection.
    // nowUtc 是 Epoch 与本地投影使用的唯一来源时刻。
    let now_utc = OffsetDateTime::now_utc();
    // localOffset uses UTC only when the operating system cannot resolve its configured offset.
    // localOffset 仅在操作系统无法解析已配置偏移时使用 UTC。
    let local_offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    // localNow projects the same instant into the resolved host offset.
    // localNow 将同一时刻投影到已解析宿主偏移。
    let local_now = now_utc.to_offset(local_offset);
    TelemetryClockSnapshot {
        timestamp_ms: unix_millis(now_utc),
        calendar_date: local_now.date().to_string(),
        timezone_offset_minutes: local_offset.whole_minutes() as i32,
    }
}

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
fn current_unix_millis() -> i64 {
    unix_millis(OffsetDateTime::now_utc())
}

/// Converts one UTC timestamp to a saturating signed millisecond count.
/// 将一个 UTC 时间戳转换为饱和有符号毫秒数。
fn unix_millis(value: OffsetDateTime) -> i64 {
    // nanos is retained as i128 until the final bounded conversion.
    // nanos 在最终有界转换前保留为 i128。
    let nanos = value.unix_timestamp_nanos();
    i64::try_from(nanos / 1_000_000).unwrap_or_else(|_| {
        if nanos.is_negative() {
            i64::MIN
        } else {
            i64::MAX
        }
    })
}

/// Returns a non-negative elapsed duration between two epoch timestamps.
/// 返回两个 Epoch 时间戳之间的非负经过时长。
fn elapsed_millis(started_at_ms: i64, completed_at_ms: i64) -> u64 {
    completed_at_ms.saturating_sub(started_at_ms).max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_core::types::{ProviderContinuation, ProviderPromptLayers};

    /// Builds one exact tracker context without constructing unrelated execution preparation fields.
    /// 在不构造无关执行准备字段的前提下构建一个精确跟踪上下文。
    fn test_tracker() -> ProviderRequestTelemetryTracker {
        // tracker owns one deterministic test context while request ids remain host-generated.
        // tracker 持有一个确定性测试上下文，请求 ID 仍由宿主生成。
        let mut tracker = ProviderRequestTelemetryTracker::default();
        tracker.install_context(ProviderRequestTelemetryContext {
            execution_id: String::from("execution-1"),
            session_id: SessionId::nil(),
            workspace_id: String::from("workspace-1"),
            provider_name: String::from("Provider Name"),
            model_name: String::from("Model Name"),
            api_family: ProviderApiFamily::AnthropicMessages,
            owner_kind: "main_agent",
            owner_id: String::from("agent-1"),
            owner_name: String::from("Agent One"),
        });
        tracker
    }

    /// Builds one minimal request snapshot using the real protocol shape.
    /// 使用真实协议形态构建一份最小请求快照。
    fn test_request_snapshot() -> ProviderRequestSnapshot {
        ProviderRequestSnapshot {
            provider_id: String::from("provider-key"),
            model_id: String::from("model-key"),
            system_prompt: String::new(),
            prompt_layers: ProviderPromptLayers::default(),
            prompt: String::new(),
            history: Vec::new(),
            temperature: None,
            max_turns: 1,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        }
    }

    /// Verifies one started request emits at most one first-token and one terminal event.
    /// 验证一个已开始请求最多发出一个首 Token 事件与一个终态事件。
    #[test]
    fn request_lifecycle_emits_exactly_one_terminal_event() {
        // tracker owns the lifecycle under test.
        // tracker 持有待测生命周期。
        let mut tracker = test_tracker();
        let started = tracker.start_request(&test_request_snapshot());
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].kind, ProviderRequestTelemetryEventKind::Started);
        assert_eq!(started[0].payload["provider_key"], "provider-key");
        assert_eq!(started[0].payload["model_key"], "model-key");
        assert!(tracker.observe_first_token().is_some());
        assert!(tracker.observe_first_token().is_none());
        let completed = tracker
            .complete_success(Some(&ProviderUsageSnapshot {
                input_tokens: Some(10),
                output_tokens: Some(5),
                reasoning_tokens: Some(2),
                total_tokens: Some(17),
                cached_input_tokens: Some(3),
                cache_creation_input_tokens: None,
            }))
            .expect("active request should complete");
        assert_eq!(completed.kind, ProviderRequestTelemetryEventKind::Completed);
        assert_eq!(completed.payload["status"], "succeeded");
        assert_eq!(completed.payload["total_tokens"], 17);
        assert_eq!(completed.payload["uncached_input_tokens"], 10);
        assert!(tracker.complete_success(None).is_none());
    }

    /// Verifies Anthropic already-separated input remains non-cached while OpenAI total prompt input subtracts cache hits.
    /// 验证 Anthropic 已分离输入保持为非缓存输入，而 OpenAI 提示词总输入会扣除缓存命中。
    #[test]
    fn normalizes_uncached_input_by_provider_api_family() {
        // anthropicUsage models Kimi's Anthropic-compatible separated cache accounting.
        // anthropicUsage 模拟 Kimi Anthropic 兼容接口的缓存分离计量。
        let anthropic_usage = ProviderUsageSnapshot {
            input_tokens: Some(2_509),
            output_tokens: Some(330),
            reasoning_tokens: None,
            total_tokens: Some(66_839),
            cached_input_tokens: Some(64_000),
            cache_creation_input_tokens: Some(0),
        };
        // openaiUsage models an OpenAI-compatible prompt total that includes cached tokens.
        // openaiUsage 模拟包含缓存 Token 的 OpenAI 兼容提示词总量。
        let openai_usage = ProviderUsageSnapshot {
            input_tokens: Some(66_509),
            ..anthropic_usage.clone()
        };

        assert_eq!(
            uncached_input_tokens(ProviderApiFamily::AnthropicMessages, &anthropic_usage),
            2_509
        );
        assert_eq!(
            uncached_input_tokens(ProviderApiFamily::OpenAiChatCompatible, &openai_usage),
            2_509
        );
    }

    /// Verifies retry transitions close the failed request before publishing retry metadata.
    /// 验证重试转换会在发布重试元数据前关闭失败请求。
    #[test]
    fn retry_transition_orders_failure_before_retry_event() {
        // tracker owns the lifecycle under test.
        // tracker 持有待测生命周期。
        let mut tracker = test_tracker();
        tracker.start_request(&test_request_snapshot());
        let events =
            tracker.complete_failure(true, 1, 3, 250, "retryable_provider_transport_error");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, ProviderRequestTelemetryEventKind::Completed);
        assert_eq!(events[0].payload["status"], "failed");
        assert_eq!(events[1].kind, ProviderRequestTelemetryEventKind::Retrying);
        assert_eq!(events[1].payload["retry_number"], 1);
        let next_started = tracker.start_request(&test_request_snapshot());
        assert_eq!(next_started[0].payload["retry_count"], 1);
    }

    /// Verifies a dropped consumer converts the active request into exactly one cancelled terminal event.
    /// 验证被丢弃的消费方会把活动请求转换为恰好一条取消终态事件。
    #[test]
    fn cancellation_tracker_closes_active_request_once() {
        // tracker emits the normal started and first-token events under test.
        // tracker 发出待测的正常开始与首 Token 事件。
        let mut tracker = test_tracker();
        // cancellation owns the service-edge state derived exclusively from emitted events.
        // cancellation 持有完全从已发出事件派生的服务边界状态。
        let mut cancellation = ProviderRequestTelemetryCancellationTracker::default();
        // started is the sole request identity source for the cancellation result.
        // started 是取消结果唯一的请求身份来源。
        let started = tracker.start_request(&test_request_snapshot()).remove(0);
        cancellation.observe(&started);
        // firstToken preserves exact first-token timing when cancellation happens later.
        // firstToken 在随后发生取消时保留精确首 Token 时间。
        let first_token = tracker
            .observe_first_token()
            .expect("emit first token telemetry");
        cancellation.observe(&first_token);
        // cancelled is emitted once and retains no prompt, output, header, or credential fields.
        // cancelled 仅发出一次，且不保留提示词、输出、请求头或凭据字段。
        let cancelled = cancellation
            .take_cancelled_event()
            .expect("emit cancelled terminal event");
        assert_eq!(cancelled.kind, ProviderRequestTelemetryEventKind::Completed);
        assert_eq!(cancelled.payload["status"], "cancelled");
        assert_eq!(cancelled.payload["total_tokens"], 0);
        assert!(cancelled.payload["first_token_at_ms"].is_number());
        assert_eq!(cancelled.payload.get("prompt"), None);
        assert!(cancellation.take_cancelled_event().is_none());
    }
}
