//! Low-frequency structured telemetry for physical Provider API requests.
//! 真实 Provider API 请求的低频结构化遥测。

use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionPromptPurpose};
use crate::agent_runtime::provider_transport_request_budget::{
    ProviderRequestBudgetEvidence, ProviderUsageBudgetBaselineHandle,
    build_provider_request_budget_evidence,
};
use crate::context_budget::ContextBudgetSnapshot;
use crate::model::SessionId;
use serde::Serialize;
use serde_json::{Value, to_value};
use time::{OffsetDateTime, UtcOffset};
use uuid::Uuid;
use vulcan_ai_contract::{AiExecutionError, AiTransportKind, DispatchOutcome, RetryAfterHint};
use vulcan_core::types::{ProviderRequestSnapshot, ProviderUsageSnapshot};
use vulcan_workbench_model::SessionProviderCallRecord;

/// Hook point fired after one physical Provider request starts.
/// 一次真实 Provider 请求开始后触发的 Hook 点。
pub const HOOK_PROVIDER_REQUEST_STARTED: &str = "provider.request.started";

/// Hook point fired after one physical Provider request is dispatched.
/// 一次真实 Provider 请求完成网络分派后触发的 Hook 点。
pub const HOOK_PROVIDER_REQUEST_DISPATCHED: &str = "provider.request.dispatched";

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
    /// Exact service entry selected for this immutable execution.
    /// 本次不可变执行选中的精确服务入口。
    service_entry_id: String,
    /// Exact credential connection selected for this immutable execution.
    /// 本次不可变执行选中的精确凭据连接。
    connection_id: String,
    /// Current model display name from the runtime blueprint.
    /// 来自运行时蓝图的当前模型显示名称。
    model_name: String,
    /// Canonical API family that defines whether Provider input already excludes cached tokens.
    /// 定义 Provider 输入是否已经排除缓存 Token 的规范接口族。
    protocol_profile_id: String,
    /// Manifest revision frozen at execution preparation.
    /// 执行准备时冻结的 Manifest 修订号。
    manifest_revision: u64,
    /// Runtime manifest generation frozen at execution preparation.
    /// 执行准备时冻结的运行时 Manifest 代际。
    manifest_generation: u64,
    /// Stable source of the exact model-level protocol binding.
    /// 精确模型级协议绑定的稳定来源。
    binding_source: String,
    /// Exact rule that produced the model binding, absent for provider defaults.
    /// 产生模型绑定的精确规则；供应商默认绑定时为空。
    binding_rule_id: Option<String>,
    /// Ordered request projection profile identities frozen with the route.
    /// 与路由一起冻结的有序请求投影 Profile 身份。
    request_profiles: Vec<vulcan_provider_adapters::ProviderOperationalProfileRevision>,
    /// Ordered response projection profile identities frozen with the route.
    /// 与路由一起冻结的有序响应投影 Profile 身份。
    response_profiles: Vec<vulcan_provider_adapters::ProviderOperationalProfileRevision>,
    /// Content-addressed revision of the exact frozen execution route.
    /// 精确冻结执行路由的内容寻址修订号。
    route_revision: String,
    /// Optional exact provider-internal route identifier.
    /// 可选的精确供应商内部路由标识。
    secondary_route_id: Option<String>,
    /// Optional provider-reported secondary provider display name.
    /// 可选的供应商报告二级供应商显示名称。
    secondary_provider_name: Option<String>,
    /// Optional provider-reported secondary endpoint display name.
    /// 可选的供应商报告二级端点显示名称。
    secondary_endpoint_name: Option<String>,
    /// Stable owner category derived from explicit preparation fields.
    /// 从显式执行准备字段派生的稳定归属类别。
    owner_kind: &'static str,
    /// Stable billing operation derived from the explicit prompt purpose.
    /// 从显式提示用途派生的稳定计费操作。
    operation: &'static str,
    /// Stable active agent identity.
    /// 稳定激活代理身份。
    owner_id: String,
    /// Current active agent display name.
    /// 当前激活代理显示名称。
    owner_name: String,
    /// Optional canonical conversation turn identity.
    /// 可选规范会话回合身份。
    turn_id: Option<String>,
    /// Active context generation frozen for this execution.
    /// 为本次执行冻结的活动上下文代际。
    context_generation_id: Option<String>,
    /// Shared durable Usage baseline used by the next logical Provider request.
    /// 下一次逻辑 Provider 请求使用的共享持久化用量基线。
    provider_usage_budget_baseline: ProviderUsageBudgetBaselineHandle,
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
    pub(crate) fn from_preparation(
        preparation: &ExecutionPreparation,
        provider_usage_budget_baseline: ProviderUsageBudgetBaselineHandle,
    ) -> Self {
        // ownerKind is derived from explicit purpose and subagent context, never from names.
        // ownerKind 从显式用途与子代理上下文派生，绝不根据名称猜测。
        let owner_kind = match preparation.prompt_purpose {
            ExecutionPromptPurpose::ContextCompaction
            | ExecutionPromptPurpose::ContextCompactionCachePreserving => "context_compaction",
            ExecutionPromptPurpose::GitCommitSummary => "git_commit_summary",
            ExecutionPromptPurpose::ExecutionRetry => "main_agent",
            ExecutionPromptPurpose::UserTurn if preparation.subagent_prompt_context.is_some() => {
                "subagent"
            }
            ExecutionPromptPurpose::UserTurn => "main_agent",
        };
        // operation is deliberately independent from owner_kind so aggregation never conflates workload with ownership.
        // operation 刻意独立于 owner_kind，避免聚合时混淆工作负载与归属。
        let operation = match preparation.prompt_purpose {
            ExecutionPromptPurpose::ContextCompaction
            | ExecutionPromptPurpose::ContextCompactionCachePreserving => "compact",
            ExecutionPromptPurpose::GitCommitSummary => "git_commit_summary",
            ExecutionPromptPurpose::ExecutionRetry => "conversation",
            ExecutionPromptPurpose::UserTurn => "conversation",
        };
        // Secondary-route fields come only from the validated immutable route snapshot.
        // 二级路由字段仅来自已校验的不可变路由快照。
        let (secondary_route_id, secondary_provider_name, secondary_endpoint_name) =
            match &preparation.model_blueprint.secondary_route {
                vulcan_provider_adapters::SecondaryRouteSelection::Automatic => (None, None, None),
                vulcan_provider_adapters::SecondaryRouteSelection::PinnedEndpoint {
                    route_id,
                    provider_display_name,
                    endpoint_display_name,
                } => (
                    Some(route_id.clone()),
                    Some(provider_display_name.clone()),
                    Some(endpoint_display_name.clone()),
                ),
            };
        // frozenSelection is the unique authority for manifest generation, binding provenance,
        // and projection-profile revisions; telemetry never re-resolves the active catalog.
        // frozenSelection 是 Manifest 代际、绑定来源与投影 Profile 修订的唯一权威；
        // 遥测绝不重新解析活动目录。
        let frozen_selection = preparation.model_blueprint.frozen_selection.as_ref();
        let protocol_binding =
            frozen_selection.and_then(|selection| selection.protocol_binding.as_ref());
        let binding_source = protocol_binding
            .map(|binding| match binding.source {
                vulcan_provider_adapters::ModelProtocolBindingSource::SystemManifest => {
                    "system_manifest"
                }
                vulcan_provider_adapters::ModelProtocolBindingSource::CustomUser => "custom_user",
                vulcan_provider_adapters::ModelProtocolBindingSource::CustomAutoDefault => {
                    "custom_auto_default"
                }
                vulcan_provider_adapters::ModelProtocolBindingSource::MigratedCustomLegacy => {
                    "migrated_custom_legacy"
                }
            })
            .unwrap_or("connection_default")
            .to_string();
        let binding_rule_id = protocol_binding
            .map(|binding| binding.rule_id.as_str())
            .filter(|rule_id| !rule_id.is_empty())
            .map(str::to_string);
        let request_profiles = frozen_selection
            .map(|selection| {
                selection
                    .route
                    .request_projection_profiles
                    .iter()
                    .map(
                        |profile| vulcan_provider_adapters::ProviderOperationalProfileRevision {
                            profile_id: profile.profile_id.clone(),
                            revision: profile.revision,
                        },
                    )
                    .collect()
            })
            .unwrap_or_default();
        let response_profiles = frozen_selection
            .map(|selection| {
                selection
                    .route
                    .response_projection_profiles
                    .iter()
                    .map(
                        |profile| vulcan_provider_adapters::ProviderOperationalProfileRevision {
                            profile_id: profile.profile_id.clone(),
                            revision: profile.revision,
                        },
                    )
                    .collect()
            })
            .unwrap_or_default();
        Self {
            execution_id: Uuid::new_v4().to_string(),
            session_id: preparation.session_id,
            workspace_id: preparation.workspace_path.clone(),
            provider_name: preparation.model_blueprint.provider_name.clone(),
            service_entry_id: preparation.model_blueprint.service_entry_id.clone(),
            connection_id: preparation.model_blueprint.connection_id.clone(),
            model_name: preparation.model_blueprint.model_name.clone(),
            protocol_profile_id: preparation.model_blueprint.protocol_profile_id.clone(),
            manifest_revision: preparation.model_blueprint.manifest_revision,
            manifest_generation: frozen_selection
                .map(|selection| selection.manifest_generation)
                .unwrap_or(0),
            binding_source,
            binding_rule_id,
            request_profiles,
            response_profiles,
            route_revision: preparation.model_blueprint.route_revision.clone(),
            secondary_route_id,
            secondary_provider_name,
            secondary_endpoint_name,
            owner_kind,
            operation,
            owner_id: preparation.agent_id.clone(),
            owner_name: preparation.agent_name.clone(),
            turn_id: preparation.conversation_hook_turn_id.clone(),
            context_generation_id: preparation.provider_usage_context_generation_id.clone(),
            provider_usage_budget_baseline,
        }
    }

    /// Builds a deterministic telemetry context for sibling-module state-machine tests.
    /// 为同级模块状态机测试构建确定性的遥测上下文。
    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        Self {
            execution_id: String::from("execution-1"),
            session_id: SessionId::nil(),
            workspace_id: String::from("workspace-1"),
            provider_name: String::from("Mock Provider"),
            service_entry_id: String::from("public-api"),
            model_name: String::from("Mock Model"),
            protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID),
            manifest_revision: 1,
            manifest_generation: 2,
            binding_source: String::from("system_manifest"),
            binding_rule_id: Some(String::from("mock-rule")),
            request_profiles: vec![
                vulcan_provider_adapters::ProviderOperationalProfileRevision {
                    profile_id: String::from("request.mock"),
                    revision: 3,
                },
            ],
            response_profiles: vec![
                vulcan_provider_adapters::ProviderOperationalProfileRevision {
                    profile_id: String::from("response.mock"),
                    revision: 4,
                },
            ],
            route_revision: String::from("sha256:test-route"),
            secondary_route_id: None,
            secondary_provider_name: None,
            secondary_endpoint_name: None,
            connection_id: String::from("connection-1"),
            owner_kind: "main_agent",
            operation: "conversation",
            owner_id: String::from("agent-1"),
            owner_name: String::from("Agent One"),
            turn_id: Some(String::from("turn-1")),
            context_generation_id: Some(String::from("context-1")),
            provider_usage_budget_baseline: ProviderUsageBudgetBaselineHandle::new(None),
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
    /// Physical request was dispatched to the frozen route.
    /// 真实请求已分派到冻结路由。
    Dispatched,
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
            Self::Dispatched => HOOK_PROVIDER_REQUEST_DISPATCHED,
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
    /// Exact normalized usage carried by a terminal request when the provider reported any usage.
    /// 当供应商报告了任意用量时，由终态请求携带的精确标准化用量。
    pub completed_usage: Option<ProviderRequestCompletedUsage>,
    /// Shared budget baseline handle excluded from the plugin-facing JSON payload.
    /// 从面向插件的 JSON 载荷中排除的共享预算基线句柄。
    provider_usage_budget_baseline: ProviderUsageBudgetBaselineHandle,
}

impl ProviderRequestTelemetryEvent {
    /// Builds one detached telemetry event that does not participate in a live request-budget stream.
    /// 构建一条不参与实时请求预算流的分离遥测事件。
    pub fn detached(
        kind: ProviderRequestTelemetryEventKind,
        session_id: SessionId,
        payload: Value,
        completed_usage: Option<ProviderRequestCompletedUsage>,
    ) -> Self {
        Self {
            kind,
            session_id,
            payload,
            completed_usage,
            provider_usage_budget_baseline: ProviderUsageBudgetBaselineHandle::new(None),
        }
    }

    /// Replaces the next-request baseline after the owning ledger boundary is durable.
    /// 在所属账本边界持久化后替换下一请求基线。
    pub fn replace_durable_budget_baseline(
        &self,
        baseline: Option<SessionProviderCallRecord>,
    ) -> Result<(), String> {
        self.provider_usage_budget_baseline.replace(baseline)
    }
}

/// Exact normalized token usage for one terminal physical provider request.
/// 单次终态真实 Provider 请求的精确标准化 Token 用量。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProviderRequestCompletedUsage {
    /// Exact merge kind of the latest provider usage observation.
    /// 最新 Provider 用量观测的精确合并类型。
    pub update_kind: Option<vulcan_ai_contract::UsageUpdateKind>,
    /// Complete provider-reported input token count.
    /// 完整的供应商报告输入 Token 数。
    pub input_tokens: Option<usize>,
    /// Input tokens that were not served from cache.
    /// 未由缓存提供的输入 Token 数。
    pub uncached_input_tokens: Option<usize>,
    /// Input tokens served from cache.
    /// 由缓存提供的输入 Token 数。
    pub cached_input_tokens: Option<usize>,
    /// Input tokens written into a provider prompt cache.
    /// 写入 Provider 提示词缓存的输入 Token 数。
    pub cache_creation_input_tokens: Option<usize>,
    /// Complete output tokens, including reasoning when reported as an output subset.
    /// 完整输出 Token 数；当思考作为输出子集报告时已包含思考。
    pub output_tokens: Option<usize>,
    /// Reasoning tokens reported as an output detail.
    /// 作为输出明细报告的思考 Token 数。
    pub reasoning_tokens: Option<usize>,
    /// Provider-reported total tokens.
    /// Provider 报告的总 Token 数。
    pub total_tokens: Option<usize>,
    /// Provider-reported audio input tokens.
    /// 供应商报告的音频输入 Token。
    pub audio_input_tokens: Option<usize>,
    /// Provider-reported audio output tokens.
    /// 供应商报告的音频输出 Token。
    pub audio_output_tokens: Option<usize>,
    /// Frozen raw usage extraction profile identity.
    /// 冻结的原始用量提取 Profile 标识。
    pub extraction_profile_id: Option<String>,
    /// Frozen raw usage extraction profile revision.
    /// 冻结的原始用量提取 Profile 修订号。
    pub extraction_profile_revision: Option<u64>,
    /// Exact raw source paths keyed by normalized dimension.
    /// 按归一化维度索引的精确原始来源路径。
    pub source_paths: std::collections::BTreeMap<String, String>,
    /// Raw usage validation findings.
    /// 原始用量校验结果。
    pub validation_flags: Vec<String>,
    /// Provider-returned model identity.
    /// 供应商返回的模型标识。
    pub response_model: Option<String>,
    /// Provider-returned response identity.
    /// 供应商返回的响应标识。
    pub response_id: Option<String>,
    /// Provider-reported decimal cost.
    /// 供应商报告的十进制成本。
    pub reported_cost: Option<String>,
    /// Locally estimated decimal cost.
    /// 本地估算的十进制成本。
    pub estimated_cost: Option<String>,
    /// ISO currency for reported or estimated cost.
    /// 报告或估算成本使用的 ISO 币种。
    pub cost_currency: Option<String>,
    /// Immutable pricing revision used by the local estimate.
    /// 本地估算使用的不可变价格修订号。
    pub pricing_revision: Option<String>,
    /// Frozen effective pricing profile used by the local estimate.
    /// 本地估算使用的冻结生效价格 Profile。
    pub pricing_profile: Option<vulcan_ai_contract::PricingProfileSnapshot>,
    /// Provider billing partitions retained separately from normalized totals.
    /// 与规范化合计分开保留的供应商计费分区。
    pub billing_details: Vec<vulcan_ai_contract::UsageBillingDetail>,
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
    /// Shared baseline handle copied from the active started event.
    /// 从活动开始事件复制的共享基线句柄。
    active_budget_baseline: Option<ProviderUsageBudgetBaselineHandle>,
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
                self.active_budget_baseline = Some(event.provider_usage_budget_baseline.clone());
            }
            ProviderRequestTelemetryEventKind::Dispatched => {
                // Cancellation recovery keeps the exact dispatch boundary only for the active request.
                // 取消恢复仅为当前活动请求保留精确分派边界。
                let Some(active_payload) = self.active_started_payload.as_mut() else {
                    return;
                };
                if active_payload.get("request_id") != event.payload.get("request_id") {
                    return;
                }
                if let (Some(active_object), Some(event_object)) =
                    (active_payload.as_object_mut(), event.payload.as_object())
                {
                    for field in ["dispatched_at_ms", "transport", "provider_request_id"] {
                        active_object.insert(
                            field.to_string(),
                            event_object.get(field).cloned().unwrap_or(Value::Null),
                        );
                    }
                }
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
                    self.active_budget_baseline = None;
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
        // budgetBaseline belongs to the same active started event.
        // budgetBaseline 属于同一条活动开始事件。
        let provider_usage_budget_baseline = self.active_budget_baseline.take()?;
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
            "cached_input_tokens",
            "cache_creation_input_tokens",
        ] {
            payload_object.insert(String::from(token_field), Value::Null);
        }
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
            completed_usage: None,
            provider_usage_budget_baseline,
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

    /// Returns the baseline handle shared with Provider request budgeting.
    /// 返回与 Provider 请求预算共享的基线句柄。
    pub(crate) fn budget_baseline_handle(&self) -> Option<ProviderUsageBudgetBaselineHandle> {
        self.context
            .as_ref()
            .map(|context| context.provider_usage_budget_baseline.clone())
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
    /// Returns zero events without context, otherwise one started event plus an invariant failure when needed; unsafe request evidence returns a controlled error.
    /// 缺少上下文时不返回事件；否则返回开始事件，并在需要时附带不变量失败事件；不安全的请求证据返回受控错误。
    pub(crate) fn start_request(
        &mut self,
        snapshot: &ProviderRequestSnapshot,
        context_budget: Option<&ContextBudgetSnapshot>,
    ) -> Result<Vec<ProviderRequestTelemetryEvent>, String> {
        let Some(context) = self.context.clone() else {
            return Ok(Vec::new());
        };
        let budget_evidence = build_provider_request_budget_evidence(
            snapshot,
            context.protocol_profile_id.as_str(),
            context.route_revision.as_str(),
        )?;
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
                None,
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
            transport: None,
            dispatched_at_ms: None,
            headers_at_ms: None,
            http_status: None,
            provider_request_id: None,
            rate_limit_headers: std::collections::BTreeMap::new(),
            semantic_output_observed: false,
            calendar_date: started_at.calendar_date,
            timezone_offset_minutes: started_at.timezone_offset_minutes,
            budget_evidence,
            context_budget: context_budget.cloned(),
        };
        self.next_attempt_index = self.next_attempt_index.saturating_add(1);
        events.push(build_started_event(&context, &active_request));
        self.active_request = Some(active_request);
        Ok(events)
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

    /// Records exact dispatcher-owned transport facts for the active physical request.
    /// 为当前真实请求记录由分派器持有的精确传输事实。
    pub(crate) fn observe_transport(
        &mut self,
        transport: AiTransportKind,
        dispatched_at_ms: i64,
        headers_at_ms: Option<i64>,
        http_status: Option<u16>,
        provider_request_id: Option<String>,
        rate_limit_headers: std::collections::BTreeMap<String, String>,
    ) -> Option<ProviderRequestTelemetryEvent> {
        let context = self.context.as_ref()?;
        let active_request = self.active_request.as_mut()?;
        active_request.transport = Some(transport);
        active_request.dispatched_at_ms = Some(dispatched_at_ms);
        active_request.headers_at_ms = headers_at_ms;
        active_request.http_status = http_status;
        active_request.provider_request_id = provider_request_id;
        active_request.rate_limit_headers = rate_limit_headers;
        if active_request.first_event_at_ms.is_none() {
            active_request.first_event_at_ms = headers_at_ms;
        }
        Some(build_dispatched_event(context, active_request))
    }

    /// Records that the active request exposed replay-relevant semantic output.
    /// 记录活动请求已经暴露与重放相关的语义输出。
    pub(crate) fn observe_semantic_output(&mut self) {
        if let Some(active_request) = self.active_request.as_mut() {
            active_request.semantic_output_observed = true;
            if active_request.first_event_at_ms.is_none() {
                active_request.first_event_at_ms = Some(current_unix_millis());
            }
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
        active_request.semantic_output_observed = true;
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
            None,
        ))
    }

    /// Completes the active request as failed and emits the retry-wait event when applicable.
    /// 将活动请求标记为失败，并在适用时发出重试等待事件。
    ///
    /// # Parameters
    /// 参数
    /// `will_retry` states whether another transport attempt follows, retry fields describe that transition, and `usage` retains reported partial usage.
    /// `will_retry` 表示是否继续传输重试，重试字段描述该转换，`usage` 保留供应商已报告的部分用量。
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
        error: Option<&AiExecutionError>,
        usage: Option<&ProviderUsageSnapshot>,
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
                usage,
                Some(error_category),
                error.and_then(|value| value.upstream_code.as_deref()),
                error,
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
    /// Exact transport observed at dispatch.
    /// 分派时观察到的精确传输。
    transport: Option<AiTransportKind>,
    /// Exact host dispatch timestamp from the transport boundary.
    /// 来自传输边界的精确宿主分派时间戳。
    dispatched_at_ms: Option<i64>,
    /// Exact host response-header timestamp when applicable.
    /// 适用时的精确宿主响应头时间戳。
    headers_at_ms: Option<i64>,
    /// Exact HTTP status observed by the transport.
    /// 传输层观察到的精确 HTTP 状态码。
    http_status: Option<u16>,
    /// Allowlisted upstream request identifier.
    /// 允许列表内的上游请求标识。
    provider_request_id: Option<String>,
    /// Fixed-allowlist rate-limit response headers.
    /// 固定允许列表内的限流响应头。
    rate_limit_headers: std::collections::BTreeMap<String, String>,
    /// Whether replay-relevant semantic output was observed.
    /// 是否已经观察到与重放相关的语义输出。
    semantic_output_observed: bool,
    /// Host-local calendar date at request start.
    /// 请求开始时的宿主本地自然日。
    calendar_date: String,
    /// Host-local UTC offset at request start.
    /// 请求开始时的宿主本地 UTC 偏移。
    timezone_offset_minutes: i32,
    /// Stable non-sensitive evidence for budget baseline alignment.
    /// 用于预算基线对齐的稳定非敏感证据。
    budget_evidence: ProviderRequestBudgetEvidence,
    /// Request-aligned context-budget decision made before dispatch.
    /// 分派前作出的请求对齐上下文预算决策。
    context_budget: Option<ContextBudgetSnapshot>,
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
    /// Optional canonical conversation turn identity.
    /// 可选规范会话回合身份。
    turn_id: Option<&'a str>,
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
    /// Stable billing operation.
    /// 稳定计费操作。
    operation: &'a str,
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
    /// Exact provider service-entry identifier.
    /// 精确供应商服务入口标识。
    service_entry_id: &'a str,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    connection_id: &'a str,
    /// Exact protocol profile identifier.
    /// 精确协议 Profile 标识。
    protocol_profile_id: &'a str,
    /// Manifest revision frozen for the request.
    /// 请求冻结的 Manifest 修订号。
    manifest_revision: u64,
    /// Runtime manifest generation frozen for the request.
    /// 请求冻结的运行时 Manifest 代际。
    manifest_generation: u64,
    /// Stable source of the exact model protocol binding.
    /// 精确模型协议绑定的稳定来源。
    binding_source: &'a str,
    /// Exact binding rule, absent for provider defaults.
    /// 精确绑定规则；供应商默认绑定时为空。
    binding_rule_id: Option<&'a str>,
    /// Ordered request projection profile identities.
    /// 有序请求投影 Profile 身份。
    request_profiles: &'a [vulcan_provider_adapters::ProviderOperationalProfileRevision],
    /// Ordered response projection profile identities.
    /// 有序响应投影 Profile 身份。
    response_profiles: &'a [vulcan_provider_adapters::ProviderOperationalProfileRevision],
    /// Content-addressed revision of the exact frozen route.
    /// 精确冻结路由的内容寻址修订号。
    route_revision: &'a str,
    /// Optional exact provider-internal route identifier.
    /// 可选的精确供应商内部路由标识。
    secondary_route_id: Option<&'a str>,
    /// Optional provider-reported secondary provider display name.
    /// 可选的供应商报告二级供应商显示名称。
    secondary_provider_name: Option<&'a str>,
    /// Optional provider-reported secondary endpoint display name.
    /// 可选的供应商报告二级端点显示名称。
    secondary_endpoint_name: Option<&'a str>,
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
        turn_id: context.turn_id.as_deref(),
        session_id: context.session_id.to_string(),
        project_id: None,
        workspace_id: Some(&context.workspace_id),
        owner_kind: context.owner_kind,
        operation: context.operation,
        owner_id: &context.owner_id,
        owner_name: &context.owner_name,
        provider_key: &request.provider_key,
        provider_name: &context.provider_name,
        service_entry_id: &context.service_entry_id,
        connection_id: &context.connection_id,
        protocol_profile_id: &context.protocol_profile_id,
        manifest_revision: context.manifest_revision,
        manifest_generation: context.manifest_generation,
        binding_source: &context.binding_source,
        binding_rule_id: context.binding_rule_id.as_deref(),
        request_profiles: &context.request_profiles,
        response_profiles: &context.response_profiles,
        route_revision: &context.route_revision,
        secondary_route_id: context.secondary_route_id.as_deref(),
        secondary_provider_name: context.secondary_provider_name.as_deref(),
        secondary_endpoint_name: context.secondary_endpoint_name.as_deref(),
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
    // contextBreakdown carries only non-sensitive request-category weights.
    // contextBreakdown 仅携带非敏感请求分类权重。
    let context_breakdown = request
        .budget_evidence
        .request_text_measurement
        .context_breakdown
        .as_ref();
    ProviderRequestTelemetryEvent {
        kind: ProviderRequestTelemetryEventKind::Started,
        session_id: context.session_id,
        payload: serialize_payload(StartedPayload {
            identity: identity_payload(context, request),
            started_at_ms: request.started_at_ms,
            calendar_date: &request.calendar_date,
            timezone_offset_minutes: request.timezone_offset_minutes,
            request_assembly_fingerprint: &request.budget_evidence.request_assembly_fingerprint,
            context_generation_id: context.context_generation_id.as_deref(),
            model_route_fingerprint: &request.budget_evidence.model_route_fingerprint,
            toolset_fingerprint: &request.budget_evidence.toolset_fingerprint,
            system_prompt_fingerprint: &request.budget_evidence.system_prompt_fingerprint,
            request_character_count: request
                .budget_evidence
                .request_text_measurement
                .character_count,
            request_utf8_byte_count: request
                .budget_evidence
                .request_text_measurement
                .utf8_byte_count,
            request_structural_units: request
                .budget_evidence
                .request_text_measurement
                .structural_units,
            request_history_message_count: request
                .budget_evidence
                .request_text_measurement
                .history_message_count,
            request_prompt_in_history: request
                .budget_evidence
                .request_text_measurement
                .prompt_in_history,
            request_history_fingerprint: request
                .budget_evidence
                .request_text_measurement
                .history_fingerprint
                .as_deref(),
            request_prefix_invariant_fingerprint: request
                .budget_evidence
                .request_text_measurement
                .prefix_invariant_fingerprint
                .as_deref(),
            request_message_units: context_breakdown.map(|value| value.message_units),
            request_system_tool_units: context_breakdown.map(|value| value.system_tool_units),
            request_mcp_tool_units: context_breakdown.map(|value| value.mcp_tool_units),
            request_skill_units: context_breakdown.map(|value| value.skill_units),
            request_system_prompt_units: context_breakdown.map(|value| value.system_prompt_units),
            request_other_units: context_breakdown.map(|value| value.other_units),
            budget_accounting_source: request
                .context_budget
                .as_ref()
                .map(|budget| budget.accounting_source.stable_key()),
            budget_input_tokens: request
                .context_budget
                .as_ref()
                .map(|budget| budget.active_context_tokens),
            budget_total_tokens: request
                .context_budget
                .as_ref()
                .map(|budget| budget.estimated_total_tokens),
            budget_warning_code: request
                .context_budget
                .as_ref()
                .and_then(|budget| budget.accounting_warning_code.as_deref()),
        }),
        completed_usage: None,
        provider_usage_budget_baseline: context.provider_usage_budget_baseline.clone(),
    }
}

/// Builds one dispatched event from the frozen identity and observed transport boundary.
/// 根据冻结身份与已观测传输边界构建一条 dispatched 事件。
fn build_dispatched_event(
    context: &ProviderRequestTelemetryContext,
    request: &ActiveProviderRequestTelemetry,
) -> ProviderRequestTelemetryEvent {
    ProviderRequestTelemetryEvent {
        kind: ProviderRequestTelemetryEventKind::Dispatched,
        session_id: context.session_id,
        payload: serialize_payload(DispatchedPayload {
            identity: identity_payload(context, request),
            started_at_ms: request.started_at_ms,
            dispatched_at_ms: request
                .dispatched_at_ms
                .expect("dispatched event requires an observed dispatch timestamp"),
            transport: request.transport,
            provider_request_id: request.provider_request_id.as_deref(),
        }),
        completed_usage: None,
        provider_usage_budget_baseline: context.provider_usage_budget_baseline.clone(),
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
        completed_usage: None,
        provider_usage_budget_baseline: context.provider_usage_budget_baseline.clone(),
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
    error: Option<&AiExecutionError>,
) -> ProviderRequestTelemetryEvent {
    // completedAt is captured once for all terminal durations.
    // completedAt 为全部终态时长捕获同一时间戳。
    let completed_at_ms = current_unix_millis();
    // normalizedUsage uses provider facts when available and preserves absent numeric facts as unknown.
    // normalizedUsage 在可用时使用 Provider 事实，并将缺失数字事实保留为未知。
    let normalized_usage = usage.cloned().unwrap_or_default();
    // uncachedInputTokens converts protocol-specific input semantics into the statistics contract once at the telemetry boundary.
    // uncachedInputTokens 在遥测边界一次性把协议专属输入语义转换为统计契约。
    let uncached_input_tokens =
        uncached_input_tokens(context.protocol_profile_id.as_str(), &normalized_usage);
    // outputTokens converts protocol-specific reasoning semantics into one complete output count.
    // outputTokens 将协议专属的思考语义转换为一个完整输出计数。
    let output_tokens =
        complete_output_tokens(context.protocol_profile_id.as_str(), &normalized_usage);
    // outputTokensWithPresence preserves the distinction between an absent output field and a reported zero.
    // outputTokensWithPresence 保留输出字段缺失与明确报告为零之间的区别。
    let output_tokens_with_presence = normalized_usage.output_tokens.map(|_| output_tokens);
    // accountedTokens keeps Provider-reported totals separate from the statistics total used by the plugin.
    // accountedTokens 将 Provider 报告总量与插件使用的统计总量分开保存。
    let (accounted_tokens, accounted_total_source) = accounted_total_tokens(
        &normalized_usage,
        uncached_input_tokens,
        output_tokens_with_presence,
    );
    let completed_usage = usage.is_some().then_some(ProviderRequestCompletedUsage {
        update_kind: normalized_usage.update_kind,
        input_tokens: normalized_usage.input_tokens,
        uncached_input_tokens,
        cached_input_tokens: normalized_usage.cached_input_tokens,
        cache_creation_input_tokens: normalized_usage.cache_creation_input_tokens,
        output_tokens: output_tokens_with_presence,
        reasoning_tokens: normalized_usage.reasoning_tokens,
        total_tokens: normalized_usage.total_tokens,
        audio_input_tokens: normalized_usage.audio_input_tokens,
        audio_output_tokens: normalized_usage.audio_output_tokens,
        extraction_profile_id: normalized_usage.extraction_profile_id.clone(),
        extraction_profile_revision: normalized_usage.extraction_profile_revision,
        source_paths: normalized_usage.source_paths.clone(),
        validation_flags: normalized_usage.validation_flags.clone(),
        response_model: normalized_usage.response_model.clone(),
        response_id: normalized_usage.response_id.clone(),
        reported_cost: normalized_usage.reported_cost.clone(),
        estimated_cost: normalized_usage.estimated_cost.clone(),
        cost_currency: normalized_usage.cost_currency.clone(),
        pricing_revision: normalized_usage.pricing_revision.clone(),
        pricing_profile: normalized_usage.pricing_profile.clone(),
        billing_details: normalized_usage.billing_details.clone(),
    });
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
            input_tokens: normalized_usage.input_tokens,
            uncached_input_tokens,
            output_tokens: output_tokens_with_presence,
            reasoning_tokens: normalized_usage.reasoning_tokens,
            total_tokens: normalized_usage.total_tokens,
            cached_input_tokens: normalized_usage.cached_input_tokens,
            cache_creation_input_tokens: normalized_usage.cache_creation_input_tokens,
            accounted_total_tokens: accounted_tokens,
            accounted_total_source,
            finish_reason: None,
            error_category,
            error_code,
            dispatch_outcome: error
                .map(|value| value.dispatch_outcome)
                .or(request.transport.map(|_| DispatchOutcome::Dispatched)),
            semantic_output_observed: error
                .map(|value| value.semantic_output_observed)
                .unwrap_or(request.semantic_output_observed),
            replay_safe: error.map(provider_error_replay_safe),
            transport: error
                .and_then(|value| value.transport)
                .or(request.transport),
            dispatched_at_ms: request.dispatched_at_ms,
            headers_at_ms: request.headers_at_ms,
            http_status: error
                .and_then(|value| value.http_status)
                .or(request.http_status),
            websocket_close_code: error.and_then(|value| value.websocket_close_code),
            provider_request_id: error
                .and_then(|value| value.request_id.as_deref())
                .or(request.provider_request_id.as_deref()),
            rate_limit_headers: &request.rate_limit_headers,
            retry_after: error.and_then(|value| value.retry_after.as_ref()),
            calendar_date: &request.calendar_date,
            timezone_offset_minutes: request.timezone_offset_minutes,
        }),
        completed_usage,
        provider_usage_budget_baseline: context.provider_usage_budget_baseline.clone(),
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
fn uncached_input_tokens(
    protocol_profile_id: &str,
    usage: &ProviderUsageSnapshot,
) -> Option<usize> {
    if let Some(uncached_input_tokens) = usage.uncached_input_tokens {
        return Some(uncached_input_tokens);
    }
    // inputTokens preserves the Provider-reported input value before statistics normalization.
    // inputTokens 在统计归一化前保留 Provider 报告的输入值。
    let input_tokens = usage.input_tokens?;
    if protocol_profile_id == vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID {
        // Anthropic reports cache writes separately from both uncached input and cache reads.
        // Anthropic 将缓存写入与非缓存输入、缓存读取分别报告。
        input_tokens.checked_add(usage.cache_creation_input_tokens.unwrap_or_default())
    } else {
        input_tokens.checked_sub(usage.cached_input_tokens.unwrap_or_default())
    }
}

/// Converts Provider-specific reasoning semantics into a complete output count.
/// 将 Provider 专属的思考语义转换为完整输出计数。
///
/// `api_family` identifies whether reasoning is already included in the reported output.
/// `api_family` 标识报告的输出中是否已经包含思考。
/// `usage` contains the final normalized Provider usage snapshot.
/// `usage` 包含最终规范化 Provider 用量快照。
/// Returns complete output without counting a Provider-reported reasoning subset twice.
/// 返回完整输出，并避免重复计算 Provider 报告的思考子集。
fn complete_output_tokens(_protocol_profile_id: &str, usage: &ProviderUsageSnapshot) -> usize {
    // All enabled public profiles normalize reasoning into the reported output count.
    // 所有已启用公共档案都会把推理归一化到已报告输出计数中。
    usage.output_tokens.unwrap_or_default()
}

/// Derive the statistics total while preserving the Provider-reported total and its quality.
/// 在保留 Provider 报告总量的同时推导统计总量及其质量。
fn accounted_total_tokens(
    usage: &ProviderUsageSnapshot,
    uncached_input_tokens: Option<usize>,
    output_tokens: Option<usize>,
) -> (Option<usize>, &'static str) {
    // derivedTotal is available only when the normalized input and complete output are both known.
    // derivedTotal 仅在规范化输入和完整输出都已知时可用。
    let derived_total =
        uncached_input_tokens
            .zip(output_tokens)
            .and_then(|(uncached_input, output)| {
                uncached_input
                    .checked_add(usage.cached_input_tokens.unwrap_or_default())?
                    .checked_add(output)
            });

    match (usage.total_tokens, derived_total) {
        (Some(reported), Some(derived)) if reported == derived => (Some(reported), "reported"),
        // A zero total beside known positive components is the legacy bridge representation of a missing total.
        // 已知分项为正而总量为零时，视为旧桥接层对缺失总量的兼容表示。
        (Some(0), Some(derived)) if derived > 0 => (Some(derived), "inconsistent"),
        (Some(reported), Some(_)) => (Some(reported), "inconsistent"),
        (Some(reported), None) => (Some(reported), "reported"),
        (None, Some(derived)) => (Some(derived), "derived"),
        (None, None) => (None, "unknown"),
    }
}

/// Determines replay safety only from the structured failure facts used by the retry layer.
/// 仅依据重试层使用的结构化失败事实判断重放安全性。
fn provider_error_replay_safe(error: &AiExecutionError) -> bool {
    !error.semantic_output_observed
        && !error.side_effect_observed
        && error.dispatch_outcome != DispatchOutcome::Unknown
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
        completed_usage: None,
        provider_usage_budget_baseline: context.provider_usage_budget_baseline.clone(),
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
    /// Request assembly digest without request content.
    /// 不含请求正文的请求装配摘要。
    request_assembly_fingerprint: &'a str,
    /// Active context generation.
    /// 活动上下文代际。
    context_generation_id: Option<&'a str>,
    /// Frozen route digest.
    /// 冻结路由摘要。
    model_route_fingerprint: &'a str,
    /// Frozen toolset digest.
    /// 冻结工具集摘要。
    toolset_fingerprint: &'a str,
    /// Frozen system-prompt digest.
    /// 冻结系统提示摘要。
    system_prompt_fingerprint: &'a str,
    /// Unicode scalar count of the canonical request.
    /// 规范请求的 Unicode 标量数量。
    request_character_count: usize,
    /// UTF-8 byte count of the canonical request.
    /// 规范请求的 UTF-8 字节数量。
    request_utf8_byte_count: usize,
    /// Structural estimation units of the canonical request.
    /// 规范请求的结构估算单位。
    request_structural_units: usize,
    /// Canonical history message count.
    /// 规范历史消息数量。
    request_history_message_count: usize,
    /// Whether canonical history already contains the latest prompt.
    /// 规范历史是否已经包含最新提示。
    request_prompt_in_history: bool,
    /// Canonical history digest.
    /// 规范历史摘要。
    request_history_fingerprint: Option<&'a str>,
    /// Digest of request components that must remain stable for history-prefix reuse.
    /// 规范历史前缀复用时必须保持稳定的请求组件摘要。
    request_prefix_invariant_fingerprint: Option<&'a str>,
    /// Character-level weight of canonical messages.
    /// 规范消息的字符级权重。
    #[serde(skip_serializing_if = "Option::is_none")]
    request_message_units: Option<usize>,
    /// Character-level weight of host and bundled tools.
    /// 宿主及随包工具的字符级权重。
    #[serde(skip_serializing_if = "Option::is_none")]
    request_system_tool_units: Option<usize>,
    /// Character-level weight of MCP tools.
    /// MCP 工具的字符级权重。
    #[serde(skip_serializing_if = "Option::is_none")]
    request_mcp_tool_units: Option<usize>,
    /// Character-level weight of LuaSkills tools.
    /// LuaSkills 工具的字符级权重。
    #[serde(skip_serializing_if = "Option::is_none")]
    request_skill_units: Option<usize>,
    /// Character-level weight of system instructions.
    /// 系统指令的字符级权重。
    #[serde(skip_serializing_if = "Option::is_none")]
    request_system_prompt_units: Option<usize>,
    /// Character-level weight of remaining request context.
    /// 其余请求上下文的字符级权重。
    #[serde(skip_serializing_if = "Option::is_none")]
    request_other_units: Option<usize>,
    /// Stable budget-accounting source.
    /// 稳定预算记账来源。
    budget_accounting_source: Option<&'a str>,
    /// Input tokens used for admission.
    /// 准入使用的输入 Token。
    budget_input_tokens: Option<usize>,
    /// Total tokens including response reserves.
    /// 包含响应预留的总 Token。
    budget_total_tokens: Option<usize>,
    /// Stable warning for non-exact accounting.
    /// 非精确记账的稳定警告。
    budget_warning_code: Option<&'a str>,
}

/// Dispatched-event payload.
/// 请求已分派事件载荷。
#[derive(Serialize)]
struct DispatchedPayload<'a> {
    /// Shared identity fields flattened into the payload.
    /// 平铺到载荷中的共享身份字段。
    #[serde(flatten)]
    identity: ProviderTelemetryIdentityPayload<'a>,
    /// Request start timestamp.
    /// 请求开始时间戳。
    started_at_ms: i64,
    /// Exact transport dispatch timestamp.
    /// 精确传输分派时间戳。
    dispatched_at_ms: i64,
    /// Exact observed transport.
    /// 精确观测到的传输类型。
    transport: Option<AiTransportKind>,
    /// Provider request identity observed with response headers when available.
    /// 若响应头已提供，则记录观测到的 Provider 请求身份。
    provider_request_id: Option<&'a str>,
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
    /// Provider-reported input tokens, or unknown when absent.
    /// Provider 报告的输入 Token，缺失时保持未知。
    input_tokens: Option<usize>,
    /// Protocol-normalized non-cached input tokens.
    /// 按协议规范化后的非缓存输入 Token。
    uncached_input_tokens: Option<usize>,
    /// Provider-reported output tokens, or unknown when absent.
    /// Provider 报告的输出 Token，缺失时保持未知。
    output_tokens: Option<usize>,
    /// Provider-reported reasoning tokens, or unknown when absent.
    /// Provider 报告的推理 Token，缺失时保持未知。
    reasoning_tokens: Option<usize>,
    /// Provider-reported total tokens, or unknown when absent.
    /// Provider 报告的总 Token，缺失时保持未知。
    total_tokens: Option<usize>,
    /// Statistics total derived from the normalized token breakdown when needed.
    /// 必要时根据规范化 Token 分项推导的统计总量。
    accounted_total_tokens: Option<usize>,
    /// Source and quality of the statistics total.
    /// 统计总量的来源与质量。
    accounted_total_source: &'a str,
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
    /// Exact dispatch outcome when a structured failure supplied it.
    /// 结构化失败提供时的精确分派结果。
    dispatch_outcome: Option<DispatchOutcome>,
    /// Whether replay-relevant semantic output was observed.
    /// 是否已经观察到与重放相关的语义输出。
    semantic_output_observed: bool,
    /// Whether automatic replay is proven safe at this terminal boundary.
    /// 当前终态边界是否已证明可安全自动重放。
    replay_safe: Option<bool>,
    /// Exact upstream transport observed for this attempt.
    /// 当前尝试观察到的精确上游传输。
    transport: Option<AiTransportKind>,
    /// Host timestamp immediately before transport dispatch.
    /// 传输分派前的宿主时间戳。
    dispatched_at_ms: Option<i64>,
    /// Host timestamp when HTTP response headers became available.
    /// HTTP 响应头可用时的宿主时间戳。
    headers_at_ms: Option<i64>,
    /// Optional upstream HTTP status.
    /// 可选上游 HTTP 状态码。
    http_status: Option<u16>,
    /// Exact WebSocket close code supplied by the peer.
    /// 对端提供的精确 WebSocket 关闭码。
    websocket_close_code: Option<u16>,
    /// Optional upstream request identifier from the structured failure.
    /// 来自结构化失败的可选上游请求标识。
    provider_request_id: Option<&'a str>,
    /// Fixed-allowlist rate-limit response headers.
    /// 固定允许列表内的限流响应头。
    rate_limit_headers: &'a std::collections::BTreeMap<String, String>,
    /// Optional normalized provider retry delay with exact provenance.
    /// 可选的带精确来源归一化供应商重试延迟。
    retry_after: Option<&'a RetryAfterHint>,
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
    /// Effective finite retry ceiling.
    /// 有效有限重试上限。
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
    use vulcan_core::types::{
        ConversationMessage, ConversationPayload, ProviderContinuation, ProviderPromptLayers,
    };

    /// Builds one exact tracker context without constructing unrelated execution preparation fields.
    /// 在不构造无关执行准备字段的前提下构建一个精确跟踪上下文。
    fn test_tracker() -> ProviderRequestTelemetryTracker {
        // tracker owns one deterministic test context while request ids remain host-generated.
        // tracker 持有一个确定性测试上下文，请求 ID 仍由宿主生成。
        let mut tracker = ProviderRequestTelemetryTracker::default();
        tracker.install_context(ProviderRequestTelemetryContext {
            execution_id: String::from("execution-1"),
            session_id: SessionId::nil(),
            turn_id: None,
            workspace_id: String::from("workspace-1"),
            provider_name: String::from("Provider Name"),
            service_entry_id: String::from("public-api"),
            model_name: String::from("Model Name"),
            protocol_profile_id: String::from(vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID),
            manifest_revision: 17,
            manifest_generation: 23,
            binding_source: String::from("system_manifest"),
            binding_rule_id: Some(String::from("claude-exact")),
            request_profiles: vec![
                vulcan_provider_adapters::ProviderOperationalProfileRevision {
                    profile_id: String::from("request.anthropic"),
                    revision: 5,
                },
            ],
            response_profiles: vec![
                vulcan_provider_adapters::ProviderOperationalProfileRevision {
                    profile_id: String::from("response.anthropic"),
                    revision: 6,
                },
            ],
            route_revision: String::from("sha256:test-route"),
            secondary_route_id: Some(String::from("route-1")),
            secondary_provider_name: Some(String::from("DeepInfra")),
            secondary_endpoint_name: Some(String::from("DeepInfra endpoint")),
            connection_id: String::from("connection-1"),
            owner_kind: "main_agent",
            operation: "conversation",
            owner_id: String::from("agent-1"),
            owner_name: String::from("Agent One"),
            context_generation_id: None,
            provider_usage_budget_baseline: ProviderUsageBudgetBaselineHandle::new(None),
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
            prompt_in_history: false,
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
        let started = tracker
            .start_request(&test_request_snapshot(), None)
            .expect("request evidence should build");
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].kind, ProviderRequestTelemetryEventKind::Started);
        assert_eq!(started[0].payload["provider_key"], "provider-key");
        assert_eq!(started[0].payload["service_entry_id"], "public-api");
        assert_eq!(started[0].payload["connection_id"], "connection-1");
        assert_eq!(
            started[0].payload["protocol_profile_id"],
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
        );
        assert_eq!(started[0].payload["manifest_revision"], 17);
        assert_eq!(started[0].payload["manifest_generation"], 23);
        assert_eq!(started[0].payload["binding_source"], "system_manifest");
        assert_eq!(started[0].payload["binding_rule_id"], "claude-exact");
        assert_eq!(
            started[0].payload["request_profiles"][0]["profile_id"],
            "request.anthropic"
        );
        assert_eq!(started[0].payload["request_profiles"][0]["revision"], 5);
        assert_eq!(
            started[0].payload["response_profiles"][0]["profile_id"],
            "response.anthropic"
        );
        assert_eq!(started[0].payload["response_profiles"][0]["revision"], 6);
        assert_eq!(started[0].payload["route_revision"], "sha256:test-route");
        assert_eq!(started[0].payload["secondary_route_id"], "route-1");
        assert_eq!(started[0].payload["secondary_provider_name"], "DeepInfra");
        assert_eq!(
            started[0].payload["secondary_endpoint_name"],
            "DeepInfra endpoint"
        );
        assert_eq!(started[0].payload["model_key"], "model-key");
        assert_eq!(started[0].payload["request_prompt_in_history"], false);
        let dispatched = tracker
            .observe_transport(
                AiTransportKind::Http,
                100,
                Some(101),
                Some(200),
                Some(String::from("upstream-request")),
                std::collections::BTreeMap::new(),
            )
            .expect("active request should emit dispatched telemetry");
        assert_eq!(
            dispatched.kind,
            ProviderRequestTelemetryEventKind::Dispatched
        );
        assert_eq!(dispatched.payload["dispatched_at_ms"], 100);
        assert_eq!(dispatched.payload["transport"], "http");
        assert_eq!(
            dispatched.payload["provider_request_id"],
            "upstream-request"
        );
        assert!(tracker.observe_first_token().is_some());
        assert!(tracker.observe_first_token().is_none());
        let completed = tracker
            .complete_success(Some(&ProviderUsageSnapshot {
                input_tokens: Some(10),
                uncached_input_tokens: Some(10),
                output_tokens: Some(5),
                reasoning_tokens: Some(2),
                total_tokens: Some(18),
                cached_input_tokens: Some(3),
                cache_creation_input_tokens: None,
                ..ProviderUsageSnapshot::default()
            }))
            .expect("active request should complete");
        assert_eq!(completed.kind, ProviderRequestTelemetryEventKind::Completed);
        assert_eq!(completed.payload["status"], "succeeded");
        assert_eq!(completed.payload["total_tokens"], 18);
        assert_eq!(completed.payload["uncached_input_tokens"], 10);
        assert_eq!(completed.payload["accounted_total_tokens"], 18);
        assert_eq!(completed.payload["accounted_total_source"], "reported");
        assert_eq!(
            completed.completed_usage,
            Some(ProviderRequestCompletedUsage {
                input_tokens: Some(10),
                uncached_input_tokens: Some(10),
                cached_input_tokens: Some(3),
                cache_creation_input_tokens: None,
                output_tokens: Some(5),
                reasoning_tokens: Some(2),
                total_tokens: Some(18),
                ..ProviderRequestCompletedUsage::default()
            })
        );
        assert!(tracker.complete_success(None).is_none());
    }

    /// Verifies Started telemetry preserves the canonical prompt-ownership fact used by Provider anchor deltas.
    /// 验证 Started 遥测会保留 Provider 锚点增量所依赖的规范提示词归属事实。
    #[test]
    fn started_telemetry_preserves_prompt_in_history() {
        let mut tracker = test_tracker();
        let mut snapshot = test_request_snapshot();
        snapshot.prompt = String::from("continue");
        snapshot.prompt_in_history = true;
        snapshot.history.push(ConversationMessage {
            id: String::from("user-1"),
            payload: ConversationPayload::UserText {
                text: String::from("continue"),
                attachments: Vec::new(),
            },
        });

        let started = tracker
            .start_request(&snapshot, None)
            .expect("request evidence should build");

        assert_eq!(started.len(), 1);
        assert_eq!(started[0].payload["request_prompt_in_history"], true);
        assert_eq!(started[0].payload["request_history_message_count"], 1);
        assert!(started[0].payload["request_history_fingerprint"].is_string());
        assert!(started[0].payload["request_prefix_invariant_fingerprint"].is_string());
    }

    /// Verifies every physical request receives a unique trace while retaining one frozen route.
    /// 验证每次物理请求获得唯一追踪身份，同时保留同一份冻结路由。
    #[test]
    fn physical_requests_receive_unique_trace_ids_with_exact_frozen_route_identity() {
        // firstTracker owns the first independent physical request lifecycle.
        // firstTracker 持有第一条独立物理请求生命周期。
        let mut first_tracker = test_tracker();
        // secondTracker owns the second independent physical request lifecycle.
        // secondTracker 持有第二条独立物理请求生命周期。
        let mut second_tracker = test_tracker();
        // firstStarted and secondStarted retain the emitted immutable start facts.
        // firstStarted 与 secondStarted 保留发出的不可变启动事实。
        let first_started = first_tracker
            .start_request(&test_request_snapshot(), None)
            .expect("first request evidence should build");
        let second_started = second_tracker
            .start_request(&test_request_snapshot(), None)
            .expect("second request evidence should build");

        assert_ne!(
            first_started[0].payload["request_id"], second_started[0].payload["request_id"],
            "physical requests must never share one trace identity"
        );
        for events in [&first_started, &second_started] {
            assert_eq!(events[0].payload["manifest_revision"], 17);
            assert_eq!(events[0].payload["manifest_generation"], 23);
            assert_eq!(events[0].payload["binding_source"], "system_manifest");
            assert_eq!(events[0].payload["binding_rule_id"], "claude-exact");
            assert_eq!(events[0].payload["route_revision"], "sha256:test-route");
            assert_eq!(events[0].payload["request_profiles"][0]["revision"], 5);
            assert_eq!(events[0].payload["response_profiles"][0]["revision"], 6);
        }
    }

    /// Verifies absent provider accounting remains unknown instead of becoming fabricated zero usage.
    /// 验证缺失的 Provider 计量保持未知，而不会被伪造成零用量。
    #[test]
    fn missing_usage_serializes_as_unknown_values() {
        // tracker owns one physical request whose provider returned no accounting fields.
        // tracker 持有一次 Provider 未返回任何计量字段的物理请求。
        let mut tracker = test_tracker();
        tracker
            .start_request(&test_request_snapshot(), None)
            .expect("request evidence should build");
        let completed = tracker
            .complete_success(Some(&ProviderUsageSnapshot::default()))
            .expect("active request should complete");

        assert!(completed.payload["uncached_input_tokens"].is_null());
        assert!(completed.payload["cached_input_tokens"].is_null());
        assert!(completed.payload["output_tokens"].is_null());
        assert!(completed.payload["reasoning_tokens"].is_null());
        assert!(completed.payload["total_tokens"].is_null());
        assert!(completed.payload["accounted_total_tokens"].is_null());
        assert_eq!(completed.payload["accounted_total_source"], "unknown");
        assert_eq!(
            completed.completed_usage,
            Some(ProviderRequestCompletedUsage {
                uncached_input_tokens: None,
                cached_input_tokens: None,
                cache_creation_input_tokens: None,
                output_tokens: None,
                reasoning_tokens: None,
                total_tokens: None,
                ..ProviderRequestCompletedUsage::default()
            })
        );
    }

    /// Verifies cache reads are excluded while Anthropic cache writes remain new input.
    /// 验证缓存读取被排除，而 Anthropic 缓存写入仍属于新增输入。
    #[test]
    fn normalizes_uncached_input_by_provider_api_family() {
        // anthropicUsage models Kimi's Anthropic-compatible separated cache accounting.
        // anthropicUsage 模拟 Kimi Anthropic 兼容接口的缓存分离计量。
        let anthropic_usage = ProviderUsageSnapshot {
            input_tokens: Some(2_509),
            uncached_input_tokens: None,
            output_tokens: Some(330),
            reasoning_tokens: None,
            total_tokens: Some(66_870),
            cached_input_tokens: Some(64_000),
            cache_creation_input_tokens: Some(31),
            ..ProviderUsageSnapshot::default()
        };
        // openaiUsage models an OpenAI-compatible prompt total that includes cached tokens.
        // openaiUsage 模拟包含缓存 Token 的 OpenAI 兼容提示词总量。
        let openai_usage = ProviderUsageSnapshot {
            input_tokens: Some(66_509),
            ..anthropic_usage.clone()
        };

        assert_eq!(
            uncached_input_tokens(
                vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID,
                &anthropic_usage,
            ),
            Some(2_540)
        );
        assert_eq!(
            uncached_input_tokens(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID, &openai_usage,),
            Some(2_509)
        );
    }

    /// Verifies inconsistent cache totals remain unknown instead of becoming a fabricated zero.
    /// 验证不一致的缓存总量保持未知，而不会被伪造为零。
    #[test]
    fn inconsistent_cache_totals_do_not_fabricate_uncached_zero() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(10),
            cached_input_tokens: Some(11),
            ..ProviderUsageSnapshot::default()
        };

        assert_eq!(
            uncached_input_tokens(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID, &usage),
            None
        );
    }

    /// Verifies missing or bridged-zero Provider totals use the normalized token breakdown.
    /// 验证缺失或被桥接为零的 Provider 总量使用规范化 Token 分项计算。
    #[test]
    fn derives_accounted_total_from_normalized_breakdown() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(2_509),
            uncached_input_tokens: Some(2_540),
            output_tokens: Some(330),
            cached_input_tokens: Some(64_000),
            cache_creation_input_tokens: Some(31),
            ..ProviderUsageSnapshot::default()
        };

        assert_eq!(
            accounted_total_tokens(&usage, Some(2_540), Some(330)),
            (Some(66_870), "derived")
        );

        let bridged_zero_usage = ProviderUsageSnapshot {
            total_tokens: Some(0),
            ..usage
        };
        assert_eq!(
            accounted_total_tokens(&bridged_zero_usage, Some(2_540), Some(330)),
            (Some(66_870), "inconsistent")
        );
    }

    /// Verifies enabled public profiles retain output counts that already include reasoning details.
    /// 验证已启用公共协议保留已经包含推理明细的输出计数。
    #[test]
    fn normalizes_complete_output_by_provider_api_family() {
        // usage models one reported output count with a separately reported reasoning detail subset.
        // usage 模拟一个带单独推理明细子集的已报告输出计数。
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(1_000),
            uncached_input_tokens: None,
            output_tokens: Some(200),
            reasoning_tokens: Some(50),
            total_tokens: Some(1_250),
            cached_input_tokens: None,
            cache_creation_input_tokens: None,
            ..ProviderUsageSnapshot::default()
        };

        assert_eq!(
            complete_output_tokens(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID, &usage),
            200
        );
        assert_eq!(
            complete_output_tokens(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID, &usage),
            200
        );
    }

    /// Verifies retry transitions close the failed request before publishing retry metadata.
    /// 验证重试转换会在发布重试元数据前关闭失败请求。
    #[test]
    fn retry_transition_orders_failure_before_retry_event() {
        // tracker owns the lifecycle under test.
        // tracker 持有待测生命周期。
        let mut tracker = test_tracker();
        tracker
            .start_request(&test_request_snapshot(), None)
            .expect("request evidence should build");
        let events = tracker.complete_failure(
            true,
            1,
            3,
            250,
            "retryable_provider_transport_error",
            None,
            None,
        );
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, ProviderRequestTelemetryEventKind::Completed);
        assert_eq!(events[0].payload["status"], "failed");
        assert_eq!(events[1].kind, ProviderRequestTelemetryEventKind::Retrying);
        assert_eq!(events[1].payload["retry_number"], 1);
        let next_started = tracker
            .start_request(&test_request_snapshot(), None)
            .expect("retry request evidence should build");
        assert_eq!(next_started[0].payload["retry_count"], 1);
    }

    /// Verifies a failed physical request retains usage reported before its terminal error.
    /// 验证失败的真实请求会保留终态错误前已报告的用量。
    #[test]
    fn failed_request_retains_reported_partial_usage() {
        let mut tracker = test_tracker();
        tracker
            .start_request(&test_request_snapshot(), None)
            .expect("request evidence should build");
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(13),
            output_tokens: Some(2),
            total_tokens: Some(15),
            ..ProviderUsageSnapshot::default()
        };

        let events = tracker.complete_failure(
            false,
            0,
            0,
            0,
            "provider_transport_error",
            None,
            Some(&usage),
        );

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["status"], "failed");
        assert_eq!(events[0].payload["input_tokens"], 13);
        assert_eq!(events[0].payload["output_tokens"], 2);
        assert_eq!(events[0].payload["total_tokens"], 15);
        assert_eq!(
            events[0]
                .completed_usage
                .as_ref()
                .and_then(|value| value.total_tokens),
            Some(15)
        );
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
        let started = tracker
            .start_request(&test_request_snapshot(), None)
            .expect("request evidence should build")
            .remove(0);
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
        assert!(cancelled.payload["total_tokens"].is_null());
        assert!(cancelled.payload["first_token_at_ms"].is_number());
        assert_eq!(cancelled.payload.get("prompt"), None);
        assert!(cancellation.take_cancelled_event().is_none());
    }
}
