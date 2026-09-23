//! Standalone confirmation owner for `RuntimeOnly` prompt compaction.
//! RuntimeOnly 提示词压缩的独立确认所有者。
//!
//! A `RuntimeOnly` compaction runs without any V1 session document, so nothing in the service kernel
//! can confirm the mandatory Provider preparation and durability barriers. This module declares that
//! ownership explicitly and turns the standalone stream consumer into the exact, typed owner of every
//! physical Provider request it publishes.
//! RuntimeOnly 压缩不带任何 V1 会话文档，因此服务内核无法确认强制的 Provider 准备与可靠性屏障。
//! 本模块显式声明该所有权，并让独立流消费方成为其发布的每次物理 Provider 请求的精确强类型所有者。

use crate::agent_runtime::events::{
    ExecutionStreamEvent, PreparedPhysicalApi, ProviderRequestPreparationCandidate,
};
use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::provider_request_telemetry::{
    ProviderRequestTelemetryContext, ProviderRequestTelemetryEvent,
    ProviderRequestTelemetryEventKind,
};
use crate::agent_runtime::provider_v1_bridge::ProviderV1LifecycleFacts;
use crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch;
use crate::model::SessionId;

/// Returns whether one declared compaction ownership must publish standalone Provider facts.
/// 返回某个已声明压缩所有权是否必须发布独立 Provider 事实。
///
/// # Parameters
/// 参数
/// `ownership` is the acknowledgement authority declared by the compaction caller.
/// `ownership` 是压缩调用方声明的确认权威。
///
/// # Returns
/// 返回值
/// `true` only for the standalone `RuntimeOnly` owner; the `SessionV1` owner keeps its V1 facts.
/// 仅对独立 `RuntimeOnly` 所有者返回 `true`；`SessionV1` 所有者保留其 V1 事实。
pub(crate) const fn declares_standalone_provider_ownership(
    ownership: crate::agent_runtime::prompt_compaction::PromptCompactionOwnership,
) -> bool {
    matches!(
        ownership,
        crate::agent_runtime::prompt_compaction::PromptCompactionOwnership::RuntimeOnly
    )
}

/// Declares one compaction launch's Provider ownership before any typed request fact is published.
/// 在发布任何强类型请求事实之前声明单个压缩启动的 Provider 归属。
///
/// # Parameters
/// 参数
/// `launch` is the exact frozen launch payload built for this compaction attempt.
/// `launch` 是为该压缩尝试构建的精确冻结启动载荷。
/// `ownership` is the acknowledgement authority declared by the caller.
/// `ownership` 是调用方声明的确认权威。
///
/// # Returns
/// 返回值
/// The launch carrying the Provider telemetry context that matches `ownership`, or the exact
/// declaration failure.
/// 返回携带与 `ownership` 匹配的 Provider 遥测上下文的启动载荷，或精确的声明失败。
///
/// # Semantics
/// 语义
/// A `SessionV1` launch is returned unchanged because its V1 session owner confirms every barrier.
/// A `RuntimeOnly` launch replaces its telemetry context with the standalone declaration from
/// [`standalone_provider_telemetry_context`].
/// `SessionV1` 启动原样返回，因为其 V1 会话所有者确认每个屏障。`RuntimeOnly` 启动会用
/// [`standalone_provider_telemetry_context`] 的独立声明替换其遥测上下文。
pub(crate) fn declare_launch_provider_ownership(
    launch: RuntimeStreamLaunch,
    ownership: crate::agent_runtime::prompt_compaction::PromptCompactionOwnership,
) -> Result<RuntimeStreamLaunch, String> {
    if !declares_standalone_provider_ownership(ownership) {
        return Ok(launch);
    }
    let mut launch = launch;
    let telemetry_context = standalone_provider_telemetry_context(&launch.preparation)?;
    launch.stream_state = launch
        .stream_state
        .with_provider_request_telemetry_context(telemetry_context);
    Ok(launch)
}

/// Returns the frozen Provider telemetry context for one standalone `RuntimeOnly` compaction launch.
/// 返回独立 `RuntimeOnly` 压缩启动使用的冻结 Provider 遥测上下文。
///
/// # Parameters
/// 参数
/// `preparation` is the exact execution preparation already frozen for the compaction attempt.
/// `preparation` 是该压缩尝试已经冻结的精确执行准备。
///
/// # Returns
/// 返回值
/// A context whose typed request template publishes no V1 session-ledger facts, or the exact
/// derivation failure.
/// 返回一个强类型请求模板不发布任何 V1 会话账本事实的上下文，或精确的派生失败。
///
/// # Semantics
/// 语义
/// The standalone owner has no V1 session document, so its Provider requests must declare themselves
/// outside the session ledger. Owner kind and billing operation still derive from the compaction
/// prompt purpose, so physical accounting keeps reporting `context_compaction`/`compact` while no
/// `RequestPrepared` fact is ever published for a request that no standalone consumer could confirm.
/// 独立所有者没有 V1 会话文档，因此其 Provider 请求必须声明位于会话账本之外。归属类别与计费操作仍
/// 由压缩提示用途派生，物理记账继续保持 `context_compaction`/`compact`，同时绝不发布任何独立消费方
/// 无法确认的 `RequestPrepared` 事实。
pub(crate) fn standalone_provider_telemetry_context(
    preparation: &ExecutionPreparation,
) -> Result<ProviderRequestTelemetryContext, String> {
    // The derivation stays the single authority for every other frozen telemetry fact.
    // 派生过程仍是其余全部冻结遥测事实的唯一权威。
    let mut context = ProviderRequestTelemetryContext::from_preparation(preparation, None)?;
    // basePurpose is the only V1 session-ledger projection; clearing it is the explicit standalone
    // declaration and never changes the compaction purpose carried by execution preparation.
    // basePurpose 是唯一的 V1 会话账本投影；清除它就是显式的独立声明，且绝不改变执行准备携带的
    // 压缩用途。
    context.v1_request_template.base_purpose = None;
    Ok(context)
}

/// One physical Provider request admitted by the standalone compaction owner.
/// 独立压缩所有者已准入的一次物理 Provider 请求。
#[derive(Debug, Clone)]
struct StandaloneRequestLedger {
    /// Exact host-generated physical Provider call identity.
    /// 宿主生成的精确物理 Provider 调用身份。
    request_id: String,
    /// Whether the prepared identity was promoted to a started physical request.
    /// 已准备身份是否被提升为已开始的物理请求。
    started: bool,
    /// Whether the transport dispatch boundary was consumed.
    /// 传输分派边界是否已被消费。
    dispatched: bool,
    /// Whether the first visible Provider token was observed.
    /// 是否已观测到首个可见 Provider Token。
    first_token: bool,
    /// Whether exactly one terminal Provider boundary was consumed.
    /// 是否已消费恰好一个终态 Provider 边界。
    terminal: bool,
    /// Number of ordered transport retry notices observed for this physical request.
    /// 当前物理请求观测到的有序传输重试提示数量。
    retries: u32,
}

impl StandaloneRequestLedger {
    /// Creates one admitted ledger row for an exact physical Provider call identity.
    /// 为精确物理 Provider 调用身份创建一条已准入账本记录。
    fn admitted(request_id: String) -> Self {
        Self {
            request_id,
            started: false,
            dispatched: false,
            first_token: false,
            terminal: false,
            retries: 0,
        }
    }
}

/// Explicit typed confirmation owner for one standalone `RuntimeOnly` prompt-compaction stream.
/// 单个独立 `RuntimeOnly` 提示词压缩流的显式强类型确认所有者。
///
/// # Contract
/// 契约
/// The owner reproduces the session compaction state-machine contract (`Started`, `Dispatched`,
/// `FirstToken`, `Completed`, `Retrying`) without a V1 session document: every mandatory barrier is
/// completed exactly once with the correct typed result, and any session-projected telemetry is
/// refused explicitly instead of being silently accepted.
/// 该所有者在不依赖 V1 会话文档的前提下复现会话压缩状态机契约（`Started`、`Dispatched`、
/// `FirstToken`、`Completed`、`Retrying`）：每个强制屏障都使用正确强类型结果恰好完成一次，任何
/// 带会话投影的遥测都被显式拒绝，而不是被静默接受。
#[derive(Debug)]
pub(crate) struct StandalonePromptCompactionOwner {
    /// Session identity owning the standalone compaction stream.
    /// 持有独立压缩流的会话身份。
    session_id: SessionId,
    /// Ordered physical requests admitted on this stream.
    /// 当前流上按顺序准入的物理请求。
    requests: Vec<StandaloneRequestLedger>,
}

impl StandalonePromptCompactionOwner {
    /// Creates the confirmation owner for one standalone compaction stream.
    /// 为单个独立压缩流创建确认所有者。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact session identity carried by the mirrored Provider telemetry.
    /// `session_id` 是镜像 Provider 遥测携带的精确会话身份。
    ///
    /// # Returns
    /// 返回值
    /// A confirmation owner that has admitted no physical request yet.
    /// 返回尚未准入任何物理请求的确认所有者。
    pub(crate) fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            requests: Vec::new(),
        }
    }

    /// Returns the exact number of physical Provider requests this owner admitted.
    /// 返回该所有者已准入的物理 Provider 请求精确数量。
    pub(crate) fn admitted_request_count(&self) -> usize {
        self.requests.len()
    }

    /// Returns the exact number of admitted physical Provider requests that reached a terminal.
    /// 返回已准入且到达终态的物理 Provider 请求精确数量。
    pub(crate) fn terminal_request_count(&self) -> usize {
        self.requests
            .iter()
            .filter(|record| record.terminal)
            .count()
    }

    /// Consumes one mirrored execution event and completes every mandatory Provider barrier exactly once.
    /// 消费一条镜像执行事件，并恰好一次完成全部强制 Provider 屏障。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the exact event already yielded by the standalone compaction stream.
    /// `event` 是独立压缩流已经产出的精确事件。
    ///
    /// # Returns
    /// 返回值
    /// Success only after the barrier (when any) has been completed with the owner's typed result.
    /// 仅当屏障（若存在）已经使用所有者的强类型结果完成时返回成功。
    pub(crate) fn observe(&mut self, event: ExecutionStreamEvent) -> Result<(), String> {
        match event {
            ExecutionStreamEvent::ProviderRequestPreparationBoundary {
                event,
                candidate,
                completion,
            } => {
                let result = self.admit(&event, candidate);
                // The transport waits for exactly one typed permit, including on refusal.
                // 传输层等待恰好一个强类型许可，拒绝时同样如此。
                let response = result.as_ref().map(|_| ()).map_err(Clone::clone);
                let _ = completion.complete(result);
                response
            }
            ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, completion } => {
                let result = self.acknowledge(&event);
                let _ = completion.complete(result.clone());
                result
            }
            ExecutionStreamEvent::ProviderRequestTelemetry(event) => self.record(&event),
            _ => Ok(()),
        }
    }

    /// Confirms a native remote compaction terminal for the exact standalone physical request.
    /// 确认精确独立物理请求的原生远端压缩终态。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the completed telemetry paired with the remote compaction barrier.
    /// `event` 是与远端压缩屏障配对的完成遥测。
    ///
    /// # Returns
    /// 返回值
    /// Success only for an admitted, standalone-owned request that has not terminated before.
    /// 仅当请求已准入、归属独立所有者且尚未终结时返回成功。
    pub(crate) fn confirm_remote_terminal(
        &mut self,
        event: &ProviderRequestTelemetryEvent,
    ) -> Result<(), String> {
        if event.kind != ProviderRequestTelemetryEventKind::Completed {
            return Err(String::from(
                "Standalone remote compaction requires completed Provider telemetry.",
            ));
        }
        self.record(event)
    }

    /// Verifies the standalone run admitted a physical request and consumed its terminal boundary.
    /// 校验独立运行已准入物理请求并消费了其终态边界。
    ///
    /// # Returns
    /// 返回值
    /// Success only for an admitted stream with at least one consumed terminal boundary.
    /// 仅当流已准入且至少消费一个终态边界时返回成功。
    pub(crate) fn verify_terminal_accounting(&self) -> Result<(), String> {
        if self.admitted_request_count() == 0 {
            return Err(String::from(
                "Standalone prompt compaction completed without admitting any physical Provider request.",
            ));
        }
        if self.terminal_request_count() == 0 {
            return Err(String::from(
                "Standalone prompt compaction completed without consuming a terminal Provider boundary.",
            ));
        }
        // Attempts abandoned before a terminal are legitimate only when another attempt completed the
        // operation; requiring every admitted attempt to terminate would reject a valid pruned retry.
        // 仅当另一次尝试完成了整项操作时，未达终态即被放弃的尝试才是合法的；要求每次已准入尝试都必须
        // 到达终态会错误拒绝合法的剪枝重试。
        Ok(())
    }

    /// Admits one prepared candidate as an explicitly standalone physical Provider request.
    /// 将一个已准备候选准入为显式的独立物理 Provider 请求。
    ///
    /// # Parameters
    /// 参数
    /// `event` carries the exact typed preparation fact; `candidate` is the pre-gate request.
    /// `event` 携带精确强类型准备事实；`candidate` 是闸门前请求。
    ///
    /// # Returns
    /// 返回值
    /// The excluded non-session permit released to the transport, or an explicit refusal.
    /// 返回放行给传输层的非会话排除许可，或显式拒绝。
    fn admit(
        &mut self,
        event: &ProviderRequestTelemetryEvent,
        candidate: ProviderRequestPreparationCandidate,
    ) -> Result<PreparedPhysicalApi, String> {
        self.require_standalone_facts(event)?;
        let request_id = event.provider_call_id()?.to_string();
        if self
            .requests
            .iter()
            .any(|record| record.request_id == request_id)
        {
            return Err(format!(
                "Standalone prompt compaction prepared the already admitted Provider request `{request_id}`."
            ));
        }
        // The standalone owner is the only permit authority on this path: it releases the candidate
        // exactly as built because no session media owner or V1 ledger exists to rebuild it.
        // 独立所有者是此路径上唯一的许可权威：由于不存在会话媒体所有者或 V1 账本可以重建候选，
        // 它按原样放行候选。
        let permit = candidate.into_excluded_non_session()?;
        self.requests
            .push(StandaloneRequestLedger::admitted(request_id));
        Ok(permit)
    }

    /// Acknowledges one mandatory durable boundary after validating its typed standalone ownership.
    /// 在验证强类型独立归属后确认一个强制可靠边界。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the exact started, dispatched, or terminal telemetry carried by the boundary.
    /// `event` 是边界携带的精确开始、分派或终态遥测。
    ///
    /// # Returns
    /// 返回值
    /// Success only for a standalone-owned physical boundary of this stream.
    /// 仅当属于当前流的独立物理边界时返回成功。
    fn acknowledge(&mut self, event: &ProviderRequestTelemetryEvent) -> Result<(), String> {
        match (event.kind, event.v1_lifecycle.as_ref()) {
            (
                ProviderRequestTelemetryEventKind::Started,
                Some(ProviderV1LifecycleFacts::RequestPrepared(_)),
            ) => Err(String::from(
                "Standalone prompt compaction received an unexpected session Provider start durability boundary; the standalone owner never admits V1 session requests.",
            )),
            (
                ProviderRequestTelemetryEventKind::FirstToken
                | ProviderRequestTelemetryEventKind::Retrying,
                _,
            ) => Err(format!(
                "Standalone prompt compaction cannot acknowledge the non-durable Provider boundary `{}`.",
                telemetry_kind_label(event.kind)
            )),
            _ => self.record(event),
        }
    }

    /// Records one ordered Provider lifecycle fact without performing durable persistence.
    /// 记录一条有序 Provider 生命周期事实，但不执行可靠持久化。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the exact telemetry event already published by the standalone owner's tracker.
    /// `event` 是独立所有者跟踪器已经发布的精确遥测事件。
    ///
    /// # Returns
    /// 返回值
    /// Success only for a monotonic transition of one standalone-owned physical request.
    /// 仅当属于同一独立物理请求的单调转换时返回成功。
    fn record(&mut self, event: &ProviderRequestTelemetryEvent) -> Result<(), String> {
        self.require_standalone_facts(event)?;
        let request_id = event.provider_call_id()?.to_string();
        let Some(record) = self
            .requests
            .iter_mut()
            .find(|record| record.request_id == request_id)
        else {
            // The non-permit streaming path publishes Started telemetry without any preparation
            // boundary; adopting it is safe only because its lifecycle was just proven standalone.
            // 无许可的流式路径会在没有准备边界的情况下发布 Started 遥测；仅因为刚刚证明其生命周期
            // 属于独立路径，接受它才是安全的。
            if event.kind == ProviderRequestTelemetryEventKind::Started {
                let mut record = StandaloneRequestLedger::admitted(request_id);
                record.started = true;
                self.requests.push(record);
                return Ok(());
            }
            return Err(format!(
                "Standalone prompt compaction observed Provider telemetry for the unadmitted request `{request_id}`."
            ));
        };
        match event.kind {
            ProviderRequestTelemetryEventKind::Started => {
                if record.started {
                    return Err(format!(
                        "Standalone prompt compaction started the Provider request `{request_id}` twice."
                    ));
                }
                record.started = true;
            }
            ProviderRequestTelemetryEventKind::Dispatched => {
                if !record.started {
                    return Err(format!(
                        "Standalone prompt compaction dispatched the Provider request `{request_id}` before it started."
                    ));
                }
                if record.dispatched {
                    return Err(format!(
                        "Standalone prompt compaction dispatched the Provider request `{request_id}` twice."
                    ));
                }
                record.dispatched = true;
            }
            ProviderRequestTelemetryEventKind::FirstToken => {
                if !record.started {
                    return Err(format!(
                        "Standalone prompt compaction observed a first token for the unstarted Provider request `{request_id}`."
                    ));
                }
                record.first_token = true;
            }
            ProviderRequestTelemetryEventKind::Completed => {
                if !record.started {
                    return Err(format!(
                        "Standalone prompt compaction completed the Provider request `{request_id}` before it started."
                    ));
                }
                if record.terminal {
                    return Err(format!(
                        "Standalone prompt compaction completed the Provider request `{request_id}` twice."
                    ));
                }
                record.terminal = true;
            }
            ProviderRequestTelemetryEventKind::Retrying => {
                record.retries = record.retries.saturating_add(1);
            }
        }
        Ok(())
    }

    /// Requires the exact typed fact that only the standalone owner may confirm.
    /// 要求只有独立所有权可以确认的精确强类型事实。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the telemetry event about to be confirmed or recorded.
    /// `event` 是即将被确认或记录的遥测事件。
    ///
    /// # Returns
    /// 返回值
    /// Success only for an explicitly excluded non-session lifecycle.
    /// 仅当生命周期为显式排除的非会话时才返回成功。
    fn require_standalone_facts(
        &self,
        event: &ProviderRequestTelemetryEvent,
    ) -> Result<(), String> {
        match event.v1_lifecycle.as_ref() {
            Some(ProviderV1LifecycleFacts::ExcludedNonSession) => Ok(()),
            // First-token telemetry deliberately carries no V1 lifecycle fact.
            // 首 Token 遥测刻意不携带任何 V1 生命周期事实。
            None if event.kind == ProviderRequestTelemetryEventKind::FirstToken => Ok(()),
            Some(ProviderV1LifecycleFacts::RequestPrepared(facts)) => Err(format!(
                "Standalone prompt compaction cannot confirm the Provider request for session `{session}` because it is declared as `request_prepared` with session purpose `{purpose:?}`. Declare the standalone owner outside the V1 session ledger before dispatch.",
                session = self.session_id,
                purpose = facts.purpose
            )),
            Some(other) => Err(format!(
                "Standalone prompt compaction cannot confirm the Provider request for session `{session}` because it is declared as `{lifecycle}`. Declare the standalone owner outside the V1 session ledger before dispatch.",
                session = self.session_id,
                lifecycle = lifecycle_label(other)
            )),
            None => Err(format!(
                "Standalone prompt compaction received `{kind}` Provider telemetry for session `{session}` without typed lifecycle facts.",
                kind = telemetry_kind_label(event.kind),
                session = self.session_id
            )),
        }
    }
}

/// Returns one stable diagnostic label for a typed V1 lifecycle fact.
/// 返回强类型 V1 生命周期事实的稳定诊断标签。
fn lifecycle_label(facts: &ProviderV1LifecycleFacts) -> &'static str {
    match facts {
        ProviderV1LifecycleFacts::ExcludedNonSession => "excluded_non_session",
        ProviderV1LifecycleFacts::RequestPrepared(_) => "request_prepared",
        ProviderV1LifecycleFacts::TransportObserved(_) => "transport_observed",
        ProviderV1LifecycleFacts::ApiTerminal(_) => "api_terminal",
    }
}

/// Returns one stable diagnostic label for a Provider telemetry event kind.
/// 返回 Provider 遥测事件类型的稳定诊断标签。
fn telemetry_kind_label(kind: ProviderRequestTelemetryEventKind) -> &'static str {
    match kind {
        ProviderRequestTelemetryEventKind::Started => "started",
        ProviderRequestTelemetryEventKind::Dispatched => "dispatched",
        ProviderRequestTelemetryEventKind::FirstToken => "first_token",
        ProviderRequestTelemetryEventKind::Completed => "completed",
        ProviderRequestTelemetryEventKind::Retrying => "retrying",
    }
}
