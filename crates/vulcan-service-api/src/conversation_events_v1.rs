//! Strongly typed V1 conversation-event service contracts.
//! 强类型 V1 会话事件服务契约。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vulcan_session_core::v1::config::{ActiveRequestConfig, PendingRequestConfig, SessionConfig};
use vulcan_session_core::v1::event::{AssistantSemanticStatus, EventBody};
use vulcan_session_core::v1::ids::{Seq, TurnId};
use vulcan_session_core::v1::service_dto::{
    ConversationEventSummary, ConversationEventWindowBatch,
};

/// Default number of complete logical turns returned by initial hydration and
/// one historical page.
/// 首次 hydration 与单页历史记录默认返回的完整逻辑回合数量。
pub const CONVERSATION_EVENT_WINDOW_V1_TURN_LIMIT: usize = 30;

/// Requests the authoritative recent event window of one session.
/// 请求一个会话的权威最近事件窗口。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationEventHydrationV1Request {
    /// Cross-system session identity resolved through the V1 SQLite index or an exact embedded-task locator.
    /// 通过 V1 SQLite 索引或精确嵌入式任务定位器解析的跨系统会话身份。
    pub session_id: String,
    /// Exact indexed parent session used only for one-shot embedded TaskSession reads.
    /// 仅用于嵌入式 TaskSession 一次性读取的精确已索引父会话。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
}

/// Requests one adjacent historical page before an already loaded seq.
/// 请求已加载 seq 之前的一页相邻历史记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationEventPageV1Request {
    /// Cross-system session identity owning the frontend window.
    /// 拥有前端窗口的跨系统会话身份。
    pub session_id: String,
    /// Persisted segment generation expected by the caller.
    /// 调用方期望的持久化分片代次。
    pub window_generation: u64,
    /// Exclusive seq cursor equal to the current earliest loaded event.
    /// 等于当前最早已加载事件的排他 seq 游标。
    pub before_seq: u64,
}

/// Requests duplicate-safe reconnect deltas after the last confirmed seq.
/// 请求最后确认 seq 之后可安全去重的重连增量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationEventReconnectV1Request {
    /// Cross-system session identity owning the frontend window.
    /// 拥有前端窗口的跨系统会话身份。
    pub session_id: String,
    /// Persisted segment generation expected by the caller.
    /// 调用方期望的持久化分片代次。
    pub window_generation: u64,
    /// Last seq already confirmed by the frontend, or none for an empty window.
    /// 前端最后确认的 seq；空窗口时为空。
    pub last_confirmed_seq: Option<u64>,
    /// Service-process epoch established by the latest authoritative hydration.
    /// 最近一次权威 hydration 建立的服务进程代次。
    pub service_epoch: String,
    /// Last authoritative memory revision confirmed by the frontend.
    /// 前端最后确认的权威内存修订号。
    pub last_confirmed_memory_revision: u64,
}

/// One current or next-boundary Provider request configuration projected from V1 metadata.
/// 从 V1 metadata 投影的一份当前或下一边界 Provider 请求配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationRequestConfigV1 {
    /// Provider identifier selected for the request.
    /// 为请求选择的 Provider 标识。
    pub provider: Option<String>,
    /// Non-secret connection identifier selected for the request.
    /// 为请求选择的非秘密连接标识。
    pub connection_id: Option<String>,
    /// Model identifier selected for the request.
    /// 为请求选择的模型标识。
    pub model: Option<String>,
    /// Thinking level selected for the request.
    /// 为请求选择的思考级别。
    pub thinking_level: Option<String>,
    /// Optional sampling temperature selected for the request.
    /// 为请求选择的可选采样温度。
    pub temperature: Option<f64>,
    /// Time at which a next-boundary change was requested; current config has none.
    /// 请求下一边界变更的时刻；当前配置为空。
    pub set_at_ms: Option<i64>,
}

impl ConversationRequestConfigV1 {
    /// Projects the request configuration effective for the current physical API boundary.
    /// 投影当前物理 API 边界已生效的请求配置。
    ///
    /// # Parameters
    /// 参数
    /// `config` is the `metadata.config.active_request` value.
    /// `config` 是 `metadata.config.active_request` 值。
    ///
    /// # Returns
    /// 返回值
    /// Returns the service DTO without inventing pending state.
    /// 返回服务 DTO，且不虚构待生效状态。
    fn from_active(config: &ActiveRequestConfig) -> Self {
        Self {
            provider: config.provider.clone(),
            connection_id: config.connection_id.clone(),
            model: config.model.clone(),
            thinking_level: config.thinking_level.clone(),
            temperature: config.temperature,
            set_at_ms: None,
        }
    }

    /// Projects one request change that becomes effective at the next safe boundary.
    /// 投影一份将在下一安全边界生效的请求变更。
    ///
    /// # Parameters
    /// 参数
    /// `config` is the exact `metadata.config.pending_request` value.
    /// `config` 是精确的 `metadata.config.pending_request` 值。
    ///
    /// # Returns
    /// 返回值
    /// Returns the pending request DTO including its request timestamp.
    /// 返回包含请求时刻的待生效请求 DTO。
    fn from_pending(config: &PendingRequestConfig) -> Self {
        Self {
            provider: config.provider.clone(),
            connection_id: config.connection_id.clone(),
            model: config.model.clone(),
            thinking_level: config.thinking_level.clone(),
            temperature: config.temperature,
            set_at_ms: Some(config.set_at_ms),
        }
    }
}

/// Selected-session configuration read only from V1 `metadata.config`.
/// 仅从 V1 `metadata.config` 读取的选中会话配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationSessionConfigV1 {
    /// Request configuration frozen for the current physical API.
    /// 为当前物理 API 锁定的请求配置。
    pub current_request: ConversationRequestConfigV1,
    /// Requested configuration for the next safe boundary, when present.
    /// 下一安全边界待生效的请求配置（如有）。
    pub next_request: Option<ConversationRequestConfigV1>,
    /// Stable compaction-window node selected for the next boundary.
    /// 为下一边界选择的稳定压缩窗口节点。
    pub context_compaction_window_node_id: Option<String>,
}

impl ConversationSessionConfigV1 {
    /// Projects the sole V1 metadata authority used by selected-session UI.
    /// 投影选中会话 UI 使用的唯一 V1 metadata 权威。
    ///
    /// # Parameters
    /// 参数
    /// `config` is the exact session config captured with the event window.
    /// `config` 是与事件窗口一同捕获的精确会话配置。
    ///
    /// # Returns
    /// 返回值
    /// Returns current/next request state and the dynamic compaction selection.
    /// 返回当前/下一请求状态与动态压缩选择。
    pub fn from_session_config(config: &SessionConfig) -> Self {
        Self {
            current_request: ConversationRequestConfigV1::from_active(&config.active_request),
            next_request: config
                .pending_request
                .as_ref()
                .map(ConversationRequestConfigV1::from_pending),
            context_compaction_window_node_id: config
                .context_compaction_window_node_id
                .map(|node_id| node_id.as_str().to_string()),
        }
    }
}

/// One complete service response shared by hydration and historical paging.
/// hydration 与历史分页共享的一份完整服务响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationEventWindowV1Response {
    /// Ownership-bound event batch consumed directly by the WebUI window store.
    /// 由 WebUI 窗口 store 直接消费的带归属事件批次。
    #[serde(flatten)]
    pub batch: ConversationEventWindowBatch,
    /// Exclusive cursor for the immediately older complete-turn page.
    /// 紧邻更早完整回合分页的排他游标。
    pub older_before_seq: Option<Seq>,
    /// Whether an immediately older complete-turn page exists.
    /// 是否存在紧邻更早的完整回合分页。
    pub has_more: bool,
    /// Current open logical turn, when the returned boundary contains one.
    /// 返回边界包含开放逻辑回合时的当前回合 ID。
    pub open_turn_id: Option<TurnId>,
    /// Exact first seq of the current open logical turn.
    /// 当前开放逻辑回合的精确起始 seq。
    pub open_turn_start_seq: Option<Seq>,
    /// Selected-session configuration sourced exclusively from V1 metadata.
    /// 仅从 V1 metadata 获取的选中会话配置。
    pub session_config: ConversationSessionConfigV1,
    /// Revision frontier present only on authoritative hydration responses.
    /// 仅在权威 hydration 响应中存在的修订前沿。
    #[serde(flatten)]
    pub revision_frontier: Option<ConversationEventRevisionFrontierV1>,
}

/// Exact memory and durable revisions captured from one session owner/generation.
/// 从同一个会话 owner/代次捕获的精确内存与可靠修订号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationEventRevisionFrontierV1 {
    /// Stable identity of the current service process.
    /// 当前服务进程的稳定身份。
    pub service_epoch: String,
    /// Latest authoritative memory revision visible to the client.
    /// 客户端可见的最新权威内存修订号。
    pub memory_revision: u64,
    /// Latest whole-document revision confirmed durable.
    /// 已确认整文档可靠发布的最新修订号。
    pub durable_revision: u64,
}

impl ConversationEventRevisionFrontierV1 {
    /// Validates and builds one exact revision frontier.
    /// 校验并构建一个精确修订前沿。
    ///
    /// # Parameters
    /// 参数
    /// `memory_revision` is the current owner revision and `durable_revision` is
    /// the latest revision accepted by the physical writer.
    /// `memory_revision` 是当前 owner 修订号，`durable_revision` 是物理写入器接纳的最新修订号。
    ///
    /// # Returns
    /// 返回值
    /// Returns the frontier when durable state does not lead memory, otherwise
    /// returns one explicit invariant failure.
    /// 当可靠状态未领先内存时返回前沿，否则返回显式不变量错误。
    pub fn try_new(
        service_epoch: impl Into<String>,
        memory_revision: u64,
        durable_revision: u64,
    ) -> Result<Self, String> {
        let service_epoch = service_epoch.into();
        if service_epoch.trim().is_empty() {
            return Err("service epoch must not be empty".to_string());
        }
        if durable_revision > memory_revision {
            return Err(format!(
                "durable revision {durable_revision} is ahead of memory revision {memory_revision}"
            ));
        }
        Ok(Self {
            service_epoch,
            memory_revision,
            durable_revision,
        })
    }
}

impl ConversationEventWindowV1Response {
    /// Builds one authoritative hydration response with its exact revision frontier.
    /// 构建一份携带精确修订前沿的权威 hydration 响应。
    ///
    /// # Parameters
    /// 参数
    /// `batch` is the exact ordered event batch, `older_before_seq` is the
    /// adjacent historical cursor, and the open-Turn fields describe the live
    /// boundary when present.
    /// `batch` 是精确有序事件批次，`older_before_seq` 是相邻历史游标，开放
    /// Turn 字段在存在时描述实时边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns one internally consistent WebUI service response.
    /// 返回一份内部一致的 WebUI 服务响应。
    pub fn hydration(
        batch: ConversationEventWindowBatch,
        older_before_seq: Option<Seq>,
        open_turn_id: Option<TurnId>,
        open_turn_start_seq: Option<Seq>,
        session_config: ConversationSessionConfigV1,
        revision_frontier: ConversationEventRevisionFrontierV1,
    ) -> Self {
        Self {
            batch,
            has_more: older_before_seq.is_some(),
            older_before_seq,
            open_turn_id,
            open_turn_start_seq,
            session_config,
            revision_frontier: Some(revision_frontier),
        }
    }

    /// Builds one historical page that cannot replace the live revision frontier.
    /// 构建一页不能替换实时修订前沿的历史分页。
    ///
    /// # Parameters
    /// 参数
    /// `batch`, `older_before_seq`, and `session_config` describe only the
    /// requested persisted history page.
    /// `batch`、`older_before_seq` 与 `session_config` 只描述请求的持久化历史分页。
    ///
    /// # Returns
    /// 返回值
    /// Returns a page with no revision frontier, explicitly preventing page
    /// loads from resetting hydration/live state.
    /// 返回不含修订前沿的分页，显式防止分页加载重置 hydration/live 状态。
    pub fn historical_page(
        batch: ConversationEventWindowBatch,
        older_before_seq: Option<Seq>,
        session_config: ConversationSessionConfigV1,
    ) -> Self {
        Self {
            batch,
            has_more: older_before_seq.is_some(),
            older_before_seq,
            open_turn_id: None,
            open_turn_start_seq: None,
            session_config,
            revision_frontier: None,
        }
    }
}

/// Durability state of one revisioned conversation-event patch.
/// 一份带修订会话事件补丁的持久化状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationEventPatchDurabilityV1 {
    /// The patch is visible from authoritative memory but is not yet durable.
    /// 该补丁已从权威内存可见，但尚未可靠落盘。
    Volatile,
    /// The revision has completed its whole-file durable publication.
    /// 该修订已经完成整文件可靠发布。
    Durable,
}

/// One revisioned sparse live patch or durable acknowledgement.
/// 一份带修订的稀疏实时补丁或可靠确认。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationEventLiveDeltaV1 {
    /// Ownership-bound events in ascending seq order. Volatile pushes contain
    /// the exact sparse change set and durable acknowledgements are empty.
    /// 按升序排列的带归属事件。volatile 推送包含精确稀疏变化集，可靠确认为空。
    #[serde(flatten)]
    pub batch: ConversationEventWindowBatch,
    /// Stable identity of the service process that emitted this edge.
    /// 发出该边的服务进程稳定身份。
    pub service_epoch: String,
    /// Exact in-memory revision represented or acknowledged by this delta.
    /// 该增量表示或确认的精确内存修订号。
    pub revision: u64,
    /// Exact predecessor of a volatile patch. One mutation is an exact successor edge; a patch
    /// merged from contiguous edges spans `base_memory_revision..revision`.
    /// volatile 补丁的精确前驱修订号。单次修改为精确后继边；由相邻边合并的补丁跨越
    /// `base_memory_revision..revision`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_memory_revision: Option<u64>,
    /// Exact predecessor required only for one durable acknowledgement edge.
    /// 仅可靠确认边要求的精确前驱修订号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_durable_revision: Option<u64>,
    /// Whether the revision is memory-visible or durably published.
    /// 该修订是仅内存可见还是已经可靠发布。
    pub durability: ConversationEventPatchDurabilityV1,
    /// Streaming-text appends in ascending seq order, each for a seq absent from `events`.
    /// Only volatile patches carry them; the event at the base revision must hold exactly the
    /// declared prefix length.
    /// 按 seq 升序排列的流式正文追加，每条对应的 seq 不在 `events` 中。仅 volatile 补丁携带；
    /// 基线修订处的事件必须恰好具有声明的前缀长度。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_appends: Vec<ConversationEventTextAppendV1>,
}

/// One append to the text of a streaming `assistant_text` or `assistant_reasoning` event.
/// 对一条流式 `assistant_text` 或 `assistant_reasoning` 事件正文的一次追加。
///
/// Every other field of the event is unchanged by the append; a change to any of them is
/// published as a complete event instead.
/// 追加不改变事件的任何其他字段；其他字段的变化改为发布完整事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationEventTextAppendV1 {
    /// Event whose text grows.
    /// 正文增长的事件。
    pub seq: Seq,
    /// Event revision after the append.
    /// 追加后的事件修订号。
    pub event_revision: u64,
    /// Last-update time after the append, in Unix milliseconds.
    /// 追加后的最近更新时间(Unix 毫秒)。
    pub updated_at_ms: i64,
    /// Text length before the append, in UTF-16 code units so browsers can check it directly.
    /// 追加前的正文长度，以 UTF-16 码元计，便于浏览器直接校验。
    pub base_text_utf16_len: u64,
    /// Appended text.
    /// 追加的正文。
    pub appended_text: String,
}

impl ConversationEventTextAppendV1 {
    /// Describes how `next` extends `previous`, when it is a pure streaming-text append.
    /// 当 `next` 是对 `previous` 的纯流式正文追加时，描述该追加。
    ///
    /// # Parameters
    /// 参数
    /// `previous` is the event as last published and `next` the same seq after later mutations.
    /// `previous` 是上次发布的事件，`next` 是同一 seq 之后修改的结果。
    ///
    /// # Returns
    /// 返回值
    /// Returns the append when both are streaming text of the same kind, `next` text strictly
    /// extends `previous` text, and every other field except the revision and update time is
    /// equal; otherwise `None`, meaning `next` must be published as a complete event.
    /// 两者为同类流式正文、`next` 正文严格延长 `previous` 正文，且除修订号与更新时间外其他字段
    /// 均相同时返回追加；否则返回 `None`，表示 `next` 必须作为完整事件发布。
    pub fn between(
        previous: &ConversationEventSummary,
        next: &ConversationEventSummary,
    ) -> Option<Self> {
        if previous.seq != next.seq
            || previous.turn_id != next.turn_id
            || previous.created_at_ms != next.created_at_ms
            || previous.visibility != next.visibility
        {
            return None;
        }
        let previous_text = streaming_text(&previous.event)?;
        let next_text = streaming_text(&next.event)?;
        let appended_text = next_text.strip_prefix(previous_text)?;
        if appended_text.is_empty() {
            return None;
        }
        let mut expected = previous.event.clone();
        *streaming_text_mut(&mut expected)? = next_text.to_owned();
        if expected != next.event {
            return None;
        }
        Some(Self {
            seq: next.seq,
            event_revision: next.event_revision,
            updated_at_ms: next.updated_at_ms,
            base_text_utf16_len: utf16_len(previous_text),
            appended_text: appended_text.to_owned(),
        })
    }

    /// Reports whether a later append may extend this event, i.e. it is streaming text.
    /// 判断后续追加能否延长该事件，即它是否为流式正文。
    pub fn can_extend(event: &ConversationEventSummary) -> bool {
        streaming_text(&event.event).is_some()
    }

    /// Applies this append to the event it extends.
    /// 把该追加应用到其延长的事件上。
    ///
    /// # Returns
    /// 返回值
    /// Returns the extended event, or an error when the event is not streaming text of the
    /// declared base length.
    /// 返回延长后的事件；若事件不是具有声明基线长度的流式正文则返回错误。
    pub fn apply(
        &self,
        event: &ConversationEventSummary,
    ) -> Result<ConversationEventSummary, String> {
        if event.seq != self.seq {
            return Err(format!(
                "text append for seq {} cannot apply to seq {}",
                self.seq.value(),
                event.seq.value()
            ));
        }
        let mut extended = event.clone();
        let text = streaming_text_mut(&mut extended.event).ok_or_else(|| {
            format!(
                "text append for seq {} targets an event that is not streaming text",
                self.seq.value()
            )
        })?;
        let actual = utf16_len(text);
        if actual != self.base_text_utf16_len {
            return Err(format!(
                "text append for seq {} expects base length {} but the event has {actual}",
                self.seq.value(),
                self.base_text_utf16_len
            ));
        }
        text.push_str(&self.appended_text);
        extended.event_revision = self.event_revision;
        extended.updated_at_ms = self.updated_at_ms;
        Ok(extended)
    }

    /// Concatenates a later append of the same seq onto this one.
    /// 把同一 seq 的后续追加拼接到本追加之后。
    fn then(&self, next: &Self) -> Result<Self, String> {
        let expected_base = self.base_text_utf16_len + utf16_len(&self.appended_text);
        if next.seq != self.seq || next.base_text_utf16_len != expected_base {
            return Err(format!(
                "text append for seq {} with base {} does not continue seq {} ending at {expected_base}",
                next.seq.value(),
                next.base_text_utf16_len,
                self.seq.value()
            ));
        }
        let mut appended_text = self.appended_text.clone();
        appended_text.push_str(&next.appended_text);
        Ok(Self {
            seq: self.seq,
            event_revision: next.event_revision,
            updated_at_ms: next.updated_at_ms,
            base_text_utf16_len: self.base_text_utf16_len,
            appended_text,
        })
    }
}

/// Returns the text of a streaming assistant text or reasoning event.
/// 返回流式 assistant 正文或思考事件的正文。
fn streaming_text(body: &EventBody) -> Option<&str> {
    match body {
        EventBody::AssistantText(text) if text.status == AssistantSemanticStatus::Streaming => {
            Some(&text.text)
        }
        EventBody::AssistantReasoning(reasoning)
            if reasoning.status == AssistantSemanticStatus::Streaming =>
        {
            Some(&reasoning.text)
        }
        _ => None,
    }
}

/// Returns the mutable text of a streaming assistant text or reasoning event.
/// 返回流式 assistant 正文或思考事件的可变正文。
fn streaming_text_mut(body: &mut EventBody) -> Option<&mut String> {
    match body {
        EventBody::AssistantText(text) if text.status == AssistantSemanticStatus::Streaming => {
            Some(&mut text.text)
        }
        EventBody::AssistantReasoning(reasoning)
            if reasoning.status == AssistantSemanticStatus::Streaming =>
        {
            Some(&mut reasoning.text)
        }
        _ => None,
    }
}

/// Counts UTF-16 code units, the unit browsers use for string length.
/// 统计 UTF-16 码元数量，即浏览器字符串长度的单位。
fn utf16_len(text: &str) -> u64 {
    text.encode_utf16().count() as u64
}

impl ConversationEventLiveDeltaV1 {
    /// Builds one volatile sparse event patch.
    /// 构建一份 volatile 稀疏事件补丁。
    ///
    /// # Parameters
    /// 参数
    /// `batch` contains only events changed by `revision`; `base_memory_revision`
    /// is the exact owner revision observed before that same mutation.
    /// `batch` 只包含 `revision` 改变的事件；`base_memory_revision` 是同一次
    /// 修改前观察到的精确 owner 修订号。
    ///
    /// # Returns
    /// 返回值
    /// Returns the transport DTO only for one exact successor mutation.
    /// 仅为一次精确后继修改返回传输 DTO。
    pub fn volatile(
        batch: ConversationEventWindowBatch,
        service_epoch: impl Into<String>,
        base_memory_revision: u64,
        revision: u64,
    ) -> Result<Self, String> {
        let service_epoch = service_epoch.into();
        if service_epoch.trim().is_empty() {
            return Err("volatile service epoch must not be empty".to_string());
        }
        let expected_revision = base_memory_revision.checked_add(1).ok_or_else(|| {
            format!("volatile base memory revision {base_memory_revision} has no valid successor")
        })?;
        if revision != expected_revision {
            return Err(format!(
                "volatile revision {revision} must be the exact successor {expected_revision} of base memory revision {base_memory_revision}"
            ));
        }
        Ok(Self {
            batch,
            service_epoch,
            revision,
            base_memory_revision: Some(base_memory_revision),
            base_durable_revision: None,
            durability: ConversationEventPatchDurabilityV1::Volatile,
            text_appends: Vec::new(),
        })
    }

    /// Builds one empty durable acknowledgement for an already emitted revision.
    /// 为一个已经发出事件补丁的修订构建空 durable 确认。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` and `window_generation` bind the acknowledgement to one
    /// event owner; `revision` is the exact persisted candidate revision.
    /// `session_id` 与 `window_generation` 把确认绑定到一个事件归属；`revision`
    /// 是精确持久化候选修订号。
    ///
    /// # Returns
    /// 返回值
    /// Returns an event-free acknowledgement that cannot overwrite a newer
    /// volatile event snapshot.
    /// 返回不含事件的确认，因此不会覆盖更新的 volatile 事件快照。
    pub fn durable_ack(
        session_id: impl Into<String>,
        window_generation: u64,
        service_epoch: impl Into<String>,
        base_durable_revision: u64,
        revision: u64,
    ) -> Result<Self, String> {
        let service_epoch = service_epoch.into();
        if service_epoch.trim().is_empty() {
            return Err("durable acknowledgement service epoch must not be empty".to_string());
        }
        if revision <= base_durable_revision {
            return Err(format!(
                "durable acknowledgement revision {revision} must advance base durable revision {base_durable_revision}"
            ));
        }
        Ok(Self {
            batch: ConversationEventWindowBatch {
                session_id: session_id.into(),
                window_generation,
                events: Vec::new(),
            },
            service_epoch,
            revision,
            base_memory_revision: None,
            base_durable_revision: Some(base_durable_revision),
            durability: ConversationEventPatchDurabilityV1::Durable,
            text_appends: Vec::new(),
        })
    }

    /// Returns the canonical summaries without introducing a second shape.
    /// 返回 canonical 摘要，且不引入第二种形态。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact ordered summary slice carried by the batch.
    /// 返回该批次携带的精确有序摘要切片。
    pub fn events(&self) -> &[ConversationEventSummary] {
        &self.batch.events
    }

    /// Builds one volatile patch that carries streaming text as appends.
    /// 构建一份以追加形式携带流式正文的 volatile 补丁。
    ///
    /// # Parameters
    /// 参数
    /// `batch` holds the complete changed events and `text_appends` the appended ones; together
    /// they are the exact change set of the successor edge `base_memory_revision -> revision`.
    /// `batch` 持有完整的变更事件，`text_appends` 持有追加的事件；两者合起来是后继边
    /// `base_memory_revision -> revision` 的精确变更集。
    ///
    /// # Returns
    /// 返回值
    /// Returns the patch, or an error when the edge is not an exact successor or the appends are
    /// not in ascending seq order disjoint from `batch`.
    /// 返回补丁；若不是精确后继边，或追加未按 seq 升序且与 `batch` 不相交，则返回错误。
    pub fn volatile_with_text_appends(
        batch: ConversationEventWindowBatch,
        text_appends: Vec<ConversationEventTextAppendV1>,
        service_epoch: impl Into<String>,
        base_memory_revision: u64,
        revision: u64,
    ) -> Result<Self, String> {
        let mut delta = Self::volatile(batch, service_epoch, base_memory_revision, revision)?;
        for pair in text_appends.windows(2) {
            if pair[0].seq >= pair[1].seq {
                return Err(format!(
                    "text appends must be in strictly ascending seq order: {} then {}",
                    pair[0].seq.value(),
                    pair[1].seq.value()
                ));
            }
        }
        if let Some(append) = text_appends.iter().find(|append| {
            delta
                .batch
                .events
                .iter()
                .any(|event| event.seq == append.seq)
        }) {
            return Err(format!(
                "seq {} cannot be both a complete event and a text append",
                append.seq.value()
            ));
        }
        delta.text_appends = text_appends;
        Ok(delta)
    }

    /// Merges the volatile patch that directly follows this one into a single patch.
    /// 把紧随本补丁的 volatile 补丁合并为一份补丁。
    ///
    /// # Parameters
    /// 参数
    /// `next` must be volatile, belong to the same session, window generation and service epoch,
    /// and start at this patch's revision.
    /// `next` 必须是 volatile，属于相同会话、窗口代次与服务代次，并且从本补丁的修订号开始。
    ///
    /// # Returns
    /// 返回值
    /// Returns a patch spanning both edges whose application equals applying both in order, or
    /// `None` when the two cannot be merged and must be sent separately.
    /// 返回跨越两条边的补丁，其应用效果等于依次应用两者；无法合并、必须分别发送时返回 `None`。
    pub fn merge_volatile(&self, next: &Self) -> Option<Self> {
        let base_memory_revision = self.base_memory_revision?;
        if self.durability != ConversationEventPatchDurabilityV1::Volatile
            || next.durability != ConversationEventPatchDurabilityV1::Volatile
            || next.base_memory_revision != Some(self.revision)
            || next.batch.session_id != self.batch.session_id
            || next.batch.window_generation != self.batch.window_generation
            || next.service_epoch != self.service_epoch
        {
            return None;
        }
        let mut events: BTreeMap<Seq, ConversationEventSummary> = self
            .batch
            .events
            .iter()
            .map(|event| (event.seq, event.clone()))
            .collect();
        let mut appends: BTreeMap<Seq, ConversationEventTextAppendV1> = self
            .text_appends
            .iter()
            .map(|append| (append.seq, append.clone()))
            .collect();
        for event in &next.batch.events {
            appends.remove(&event.seq);
            events.insert(event.seq, event.clone());
        }
        for append in &next.text_appends {
            if let Some(event) = events.get(&append.seq) {
                let extended = append.apply(event).ok()?;
                events.insert(append.seq, extended);
            } else if let Some(previous) = appends.get(&append.seq) {
                let joined = previous.then(append).ok()?;
                appends.insert(append.seq, joined);
            } else {
                appends.insert(append.seq, append.clone());
            }
        }
        Some(Self {
            batch: ConversationEventWindowBatch {
                session_id: self.batch.session_id.clone(),
                window_generation: self.batch.window_generation,
                events: events.into_values().collect(),
            },
            service_epoch: self.service_epoch.clone(),
            revision: next.revision,
            base_memory_revision: Some(base_memory_revision),
            base_durable_revision: None,
            durability: ConversationEventPatchDurabilityV1::Volatile,
            text_appends: appends.into_values().collect(),
        })
    }
}

/// Why a reconnect request cannot be safely applied as an incremental recovery.
/// 重连请求无法安全应用为增量恢复的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationEventRehydrateReasonV1 {
    /// The service process changed after the client's last hydration.
    /// 客户端最近一次 hydration 后服务进程已经变化。
    ServiceEpochChanged,
    /// The authoritative event generation no longer matches the client owner.
    /// 权威事件代次不再匹配客户端 owner。
    WindowGenerationChanged,
    /// The client memory revision is ahead of the authoritative service owner.
    /// 客户端内存修订号领先权威服务 owner。
    MemoryRevisionAhead,
    /// The client seq cursor is ahead of the authoritative event stream.
    /// 客户端 seq 游标领先权威事件流。
    CursorAhead,
}

/// Disposition of one authoritative reconnect response.
/// 一份权威重连响应的处置方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "reason", rename_all = "snake_case")]
pub enum ConversationEventReconnectDispositionV1 {
    /// Apply the included complete recovery range to the existing owner.
    /// 把所含完整恢复范围应用到既有 owner。
    Recovery,
    /// Replace the client window through a fresh authoritative hydration.
    /// 通过新的权威 hydration 替换客户端窗口。
    RehydrateRequired(ConversationEventRehydrateReasonV1),
}

/// One authoritative reconnect result independent from sparse live-push DTOs.
/// 一份独立于稀疏实时推送 DTO 的权威重连结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationEventReconnectV1Response {
    /// Complete ownership-bound recovery range, empty when rehydration is required.
    /// 完整的带归属恢复范围；要求重新 hydration 时为空。
    #[serde(flatten)]
    pub batch: ConversationEventWindowBatch,
    /// Stable identity of the current service process.
    /// 当前服务进程的稳定身份。
    pub service_epoch: String,
    /// Client memory frontier used as the requested recovery base.
    /// 作为请求恢复前驱的客户端内存前沿。
    pub base_memory_revision: u64,
    /// Current authoritative resident memory revision.
    /// 当前权威驻留内存修订号。
    pub memory_revision: u64,
    /// Current authoritative durable revision.
    /// 当前权威可靠修订号。
    pub durable_revision: u64,
    /// Whether the range is applicable or a full hydration is required.
    /// 恢复范围可应用，还是需要完整 hydration。
    pub disposition: ConversationEventReconnectDispositionV1,
}

impl ConversationEventReconnectV1Response {
    /// Builds one applicable authoritative recovery range.
    /// 构建一段可应用的权威恢复范围。
    pub fn recovery(
        batch: ConversationEventWindowBatch,
        service_epoch: impl Into<String>,
        base_memory_revision: u64,
        memory_revision: u64,
        durable_revision: u64,
    ) -> Result<Self, String> {
        let service_epoch = service_epoch.into();
        validate_reconnect_frontiers(&service_epoch, memory_revision, durable_revision)?;
        if memory_revision < base_memory_revision {
            return Err(format!(
                "reconnect memory revision {memory_revision} precedes base memory revision {base_memory_revision}"
            ));
        }
        Ok(Self {
            batch,
            service_epoch,
            base_memory_revision,
            memory_revision,
            durable_revision,
            disposition: ConversationEventReconnectDispositionV1::Recovery,
        })
    }

    /// Builds one explicit full-hydration requirement without a pseudo recovery patch.
    /// 构建一条显式完整 hydration 要求，不伪造恢复补丁。
    pub fn rehydrate_required(
        session_id: impl Into<String>,
        window_generation: u64,
        service_epoch: impl Into<String>,
        base_memory_revision: u64,
        memory_revision: u64,
        durable_revision: u64,
        reason: ConversationEventRehydrateReasonV1,
    ) -> Result<Self, String> {
        let service_epoch = service_epoch.into();
        validate_reconnect_frontiers(&service_epoch, memory_revision, durable_revision)?;
        Ok(Self {
            batch: ConversationEventWindowBatch {
                session_id: session_id.into(),
                window_generation,
                events: Vec::new(),
            },
            service_epoch,
            base_memory_revision,
            memory_revision,
            durable_revision,
            disposition: ConversationEventReconnectDispositionV1::RehydrateRequired(reason),
        })
    }
}

/// Validates frontiers shared by reconnect recovery and rehydrate responses.
/// 校验重连恢复与重新 hydration 响应共享的前沿。
fn validate_reconnect_frontiers(
    service_epoch: &str,
    memory_revision: u64,
    durable_revision: u64,
) -> Result<(), String> {
    if service_epoch.trim().is_empty() {
        return Err("reconnect service epoch must not be empty".to_string());
    }
    if durable_revision > memory_revision {
        return Err(format!(
            "reconnect durable revision {durable_revision} is ahead of memory revision {memory_revision}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_ai_contract::ContextCompactionWindowNodeId;
    use vulcan_session_core::v1::config::QueuedInputIdentity;
    use vulcan_session_core::v1::document::{
        ApiCompletion, ClientInputSubmission, PrepareAndStartApiInput, SessionDocument,
        ToolCompletion, ToolPreparation,
    };
    use vulcan_session_core::v1::event::{
        ApiCallStatus, ApiEndReason, ApiPurposeKind, ApiRequestSnapshot, ApiTimings,
        ContinuationEligibility, GeneratedResourceKind, GeneratedResourceRef,
        ProviderProjectionProof, ToolCallStatus, ToolPayload,
    };
    use vulcan_session_core::v1::stats::UsageSnapshot;
    use vulcan_session_core::v1::{
        ClientActionId, LockedBaselineProjection, ProviderRequestBudgetEvidence,
        ProviderRequestContextBreakdown, ProviderRequestTextMeasurement, RequestStreamId,
        ResourceId,
    };

    /// Hydration requests preserve the indexed parent only for exact embedded TaskSession snapshots.
    /// hydration 请求仅为精确嵌入式 TaskSession 快照保留已索引父会话。
    #[test]
    fn embedded_task_hydration_request_uses_exact_camel_case_parent_identity() {
        let request = ConversationEventHydrationV1Request {
            session_id: String::from("child-session"),
            parent_session_id: Some(String::from("parent-session")),
        };

        assert_eq!(
            serde_json::to_value(request).expect("serialize embedded task hydration request"),
            serde_json::json!({
                "sessionId": "child-session",
                "parentSessionId": "parent-session",
            })
        );
    }

    /// Builds deterministic valid Provider budget evidence for the service golden fixture.
    /// 为服务 golden 夹具构建确定且有效的 Provider 预算证据。
    fn provider_request_budget_evidence() -> ProviderRequestBudgetEvidence {
        ProviderRequestBudgetEvidence {
            request_assembly_fingerprint: format!("sha256:{}", "0".repeat(64)),
            model_route_fingerprint: format!("sha256:{}", "1".repeat(64)),
            toolset_fingerprint: format!("sha256:{}", "2".repeat(64)),
            system_prompt_fingerprint: format!("sha256:{}", "3".repeat(64)),
            request_text_measurement: ProviderRequestTextMeasurement {
                character_count: 0,
                utf8_byte_count: 0,
                structural_units: 0,
                history_message_count: 0,
                prompt_in_history: false,
                history_fingerprint: format!("sha256:{}", "4".repeat(64)),
                prefix_invariant_fingerprint: format!("sha256:{}", "5".repeat(64)),
                context_breakdown: ProviderRequestContextBreakdown::default(),
            },
        }
    }

    /// Builds the exact V1 hydration response shared by Rust and TypeScript golden tests.
    /// 构建 Rust 与 TypeScript golden 测试共享的精确 V1 hydration 响应。
    fn golden_window_response() -> ConversationEventWindowV1Response {
        let mut document = SessionDocument::new("session-golden", "project", "title", 1);
        document.metadata.config.active_request = ActiveRequestConfig {
            provider: Some("openai".to_string()),
            connection_id: Some("conn-1".to_string()),
            model: Some("gpt-5".to_string()),
            thinking_level: Some("high".to_string()),
            temperature: Some(0.3),
        };
        let turn_id = document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("golden-root-input").unwrap(),
                    "hello",
                    Vec::new(),
                ),
                1,
            )
            .unwrap()
            .into_value();
        // Create and remove one queued input through the real Core lifecycle for the shared DTO.
        // 通过真实 Core 生命周期创建并移除一条排队输入，供共享 DTO golden 使用。
        let queued = document
            .enqueue_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("golden-queued-input").unwrap(),
                    "queued then removed",
                    Vec::new(),
                ),
                2,
            )
            .unwrap()
            .into_value();
        document
            .remove_queued_input(
                QueuedInputIdentity {
                    client_action_id: queued.client_action_id,
                    input_id: queued.input_id,
                },
                3,
            )
            .unwrap();
        document
            .set_pending_request(
                PendingRequestConfig {
                    provider: Some("anthropic".to_string()),
                    connection_id: Some("conn-2".to_string()),
                    model: Some("claude-4".to_string()),
                    thinking_level: Some("max".to_string()),
                    temperature: Some(0.2),
                    set_at_ms: 4,
                },
                4,
            )
            .unwrap();
        document.promote_pending_request(Some(turn_id), 5).unwrap();
        let request = ApiRequestSnapshot {
            provider: "anthropic".to_string(),
            profile: None,
            model: "claude-4".to_string(),
            connection_id: Some("conn-2".to_string()),
            thinking_level: Some("max".to_string()),
            temperature: Some(0.2),
        };
        let api_call_id = document
            .prepare_and_start_api(PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: request.clone(),
                projection: ProviderProjectionProof {
                    segment_generation: document.metadata.segment_generation,
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).unwrap(),
                    through_seq: document.events().last().unwrap().seq(),
                    fingerprint_sha256: "0".repeat(64),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                request_budget_evidence: provider_request_budget_evidence(),
                now_ms: 6,
            })
            .unwrap()
            .into_value()
            .api_call_id;
        document
            .mark_api_transport_observed(api_call_id, 7)
            .unwrap();
        let tool_call_id = document
            .prepare_tool_batch(
                api_call_id,
                vec![ToolPreparation {
                    request_stream_id: RequestStreamId::try_new("golden-generated-resource")
                        .unwrap(),
                    tool_name: "image_generate".to_string(),
                    input: ToolPayload::try_new(serde_json::json!({"prompt": "golden image"}))
                        .unwrap(),
                    provider_tool_call_id: None,
                }],
                8,
            )
            .unwrap()
            .into_value()
            .tools[0]
            .tool_call_id;
        document
            .start_prepared_tool(api_call_id, tool_call_id, 9)
            .unwrap();
        document
            .finish_tool(
                api_call_id,
                tool_call_id,
                ToolCompletion {
                    output: Some(ToolPayload::try_new(serde_json::json!({"ok": true})).unwrap()),
                    changeset: None,
                    status: ToolCallStatus::Completed,
                    finished_at_ms: 10,
                    external_refs: Vec::new(),
                    generated_resources: vec![GeneratedResourceRef {
                        resource_id: ResourceId::try_new(501).unwrap(),
                        kind: GeneratedResourceKind::Image,
                        preview_resource_id: Some(ResourceId::try_new(502).unwrap()),
                    }],
                },
            )
            .unwrap();
        document
            .complete_api_group(
                api_call_id,
                ApiCompletion {
                    request,
                    usage: UsageSnapshot::default(),
                    status: ApiCallStatus::Interrupted,
                    end_reason: Some(ApiEndReason::ProcessInterrupted),
                    error: None,
                    continuation_eligibility: ContinuationEligibility::Allowed,
                    timing: ApiTimings::default(),
                    provider_source: None,
                    completed_at_ms: 11,
                },
            )
            .unwrap();
        ConversationEventWindowV1Response::hydration(
            ConversationEventWindowBatch::from_events("session-golden", 7, document.events()),
            None,
            document.active_turn().map(|turn| turn.turn_id),
            document.active_turn().map(|turn| turn.start_seq),
            ConversationSessionConfigV1::from_session_config(&document.metadata.config),
            ConversationEventRevisionFrontierV1::try_new(
                "test-service-epoch",
                document.memory_revision(),
                document.durable_revision(),
            )
            .expect("valid hydration revision frontier"),
        )
    }

    /// V1 metadata is the sole selected-session source for current/next model state.
    /// V1 metadata 是选中会话当前/下一模型状态的唯一来源。
    #[test]
    fn session_config_projection_preserves_current_next_and_compaction_state() {
        let config = SessionConfig {
            active_request: ActiveRequestConfig {
                provider: Some("openai".to_string()),
                connection_id: Some("account-current".to_string()),
                model: Some("gpt-current".to_string()),
                thinking_level: Some("high".to_string()),
                temperature: Some(0.2),
            },
            pending_request: Some(PendingRequestConfig {
                provider: Some("openai".to_string()),
                connection_id: Some("account-next".to_string()),
                model: Some("gpt-next".to_string()),
                thinking_level: Some("xhigh".to_string()),
                temperature: Some(0.4),
                set_at_ms: 99,
            }),
            context_compaction_window_node_id: Some(
                ContextCompactionWindowNodeId::CompressionWindow4,
            ),
            ..SessionConfig::default()
        };

        let projected = ConversationSessionConfigV1::from_session_config(&config);

        assert_eq!(
            projected.current_request.model.as_deref(),
            Some("gpt-current")
        );
        assert_eq!(
            projected.current_request.thinking_level.as_deref(),
            Some("high")
        );
        assert_eq!(
            projected
                .next_request
                .as_ref()
                .and_then(|request| request.model.as_deref()),
            Some("gpt-next")
        );
        assert_eq!(
            projected
                .next_request
                .as_ref()
                .and_then(|request| request.thinking_level.as_deref()),
            Some("xhigh")
        );
        assert_eq!(
            projected.context_compaction_window_node_id.as_deref(),
            Some(ContextCompactionWindowNodeId::CompressionWindow4.as_str())
        );
    }

    /// Verifies the shared Rust service contract matches the checked-in JSON
    /// fixture consumed by the TypeScript controller test.
    /// 验证共享 Rust 服务契约与 TypeScript 控制器测试消费的已检入 JSON fixture 一致。
    #[test]
    fn rust_window_response_matches_typescript_golden_fixture() {
        // response is the canonical Rust projection serialized for the shared fixture comparison.
        // response 是为共享夹具比较而序列化的规范 Rust 投影。
        let response = golden_window_response();
        // actual must serialize successfully because every field belongs to the public API contract.
        // actual 必须成功序列化，因为每个字段都属于公共 API 契约。
        let actual = serde_json::to_string_pretty(&response)
            .expect("golden conversation event window should serialize");
        // expected normalizes checkout-specific CRLF without weakening byte-for-byte JSON content comparison.
        // expected 归一化检出环境特有的 CRLF，同时不弱化逐字节 JSON 内容比较。
        let expected = include_str!("../tests/fixtures/conversation_event_window_v1.golden.json")
            .replace("\r\n", "\n");
        assert_eq!(actual.trim(), expected.trim());
    }

    /// Missing continuation eligibility must fail the strict V1 service contract.
    /// 缺失 Continue 资格字段必须使严格 V1 服务契约失败。
    #[test]
    fn api_stats_missing_continuation_eligibility_is_rejected() {
        let mut value = serde_json::to_value(golden_window_response()).unwrap();
        let events = value
            .get_mut("events")
            .and_then(serde_json::Value::as_array_mut)
            .expect("golden hydration must contain events");
        let api_stats = events
            .iter_mut()
            .find_map(|summary| {
                let event = summary.get_mut("event")?;
                (event.get("kind")?.as_str()? == "api_stats").then_some(event)
            })
            .expect("golden hydration must contain api_stats");
        api_stats
            .as_object_mut()
            .expect("api_stats must be an object")
            .remove("continuation_eligibility");

        assert!(serde_json::from_value::<ConversationEventWindowV1Response>(value).is_err());
    }

    /// Unknown continuation fields must fail instead of being silently discarded.
    /// 未知 Continue 字段必须失败，而不能被静默丢弃。
    #[test]
    fn api_stats_unknown_continuation_field_is_rejected() {
        let mut value = serde_json::to_value(golden_window_response()).unwrap();
        let events = value
            .get_mut("events")
            .and_then(serde_json::Value::as_array_mut)
            .expect("golden hydration must contain events");
        let api_stats = events
            .iter_mut()
            .find_map(|summary| {
                let event = summary.get_mut("event")?;
                (event.get("kind")?.as_str()? == "api_stats").then_some(event)
            })
            .expect("golden hydration must contain api_stats");
        api_stats
            .as_object_mut()
            .expect("api_stats must be an object")
            .insert(
                "continuation_policy".to_string(),
                serde_json::json!("allowed"),
            );

        assert!(serde_json::from_value::<ConversationEventWindowV1Response>(value).is_err());
    }

    /// Generated-resource identity fields are mandatory in the shared service DTO.
    /// 生成资源身份字段在共享服务 DTO 中必须存在。
    #[test]
    fn generated_resource_missing_kind_is_rejected() {
        let mut value = serde_json::to_value(golden_window_response()).unwrap();
        let generated_resource = golden_generated_resource_mut(&mut value);
        generated_resource
            .as_object_mut()
            .expect("generated resource must be an object")
            .remove("kind");

        assert!(serde_json::from_value::<ConversationEventWindowV1Response>(value).is_err());
    }

    /// Generated-resource payloads reject local-path compatibility fields.
    /// 生成资源载荷拒绝本地路径兼容字段。
    #[test]
    fn generated_resource_unknown_local_path_is_rejected() {
        let mut value = serde_json::to_value(golden_window_response()).unwrap();
        let generated_resource = golden_generated_resource_mut(&mut value);
        generated_resource
            .as_object_mut()
            .expect("generated resource must be an object")
            .insert(
                "local_path".to_string(),
                serde_json::json!("D:/forbidden/generated.png"),
            );

        assert!(serde_json::from_value::<ConversationEventWindowV1Response>(value).is_err());
    }

    /// Returns the single generated-resource object from the shared golden response.
    /// 从共享 golden 响应中返回唯一的生成资源对象。
    fn golden_generated_resource_mut(value: &mut serde_json::Value) -> &mut serde_json::Value {
        value
            .get_mut("events")
            .and_then(serde_json::Value::as_array_mut)
            .and_then(|events| {
                events.iter_mut().find_map(|summary| {
                    let event = summary.get_mut("event")?;
                    (event.get("kind")?.as_str()? == "tool_call")
                        .then_some(event.get_mut("generated_resources")?)
                })
            })
            .and_then(serde_json::Value::as_array_mut)
            .and_then(|resources| resources.first_mut())
            .expect("golden hydration must contain one generated resource")
    }

    /// Queue-resolution identity fields are mandatory in the shared service DTO.
    /// 队列处置身份字段在共享服务 DTO 中必须存在。
    #[test]
    fn queued_input_resolution_missing_identity_is_rejected() {
        let mut value = serde_json::to_value(golden_window_response()).unwrap();
        let events = value
            .get_mut("events")
            .and_then(serde_json::Value::as_array_mut)
            .expect("golden hydration must contain events");
        let resolution = events
            .iter_mut()
            .find_map(|summary| {
                let event = summary.get_mut("event")?;
                (event.get("kind")?.as_str()? == "queued_input_resolution").then_some(event)
            })
            .expect("golden hydration must contain queued_input_resolution");
        resolution
            .as_object_mut()
            .expect("queued_input_resolution must be an object")
            .remove("client_action_id");

        assert!(serde_json::from_value::<ConversationEventWindowV1Response>(value).is_err());
    }

    /// Queue-resolution payloads reject unknown compatibility fields.
    /// 队列处置载荷拒绝未知兼容字段。
    #[test]
    fn queued_input_resolution_unknown_field_is_rejected() {
        let mut value = serde_json::to_value(golden_window_response()).unwrap();
        let events = value
            .get_mut("events")
            .and_then(serde_json::Value::as_array_mut)
            .expect("golden hydration must contain events");
        let resolution = events
            .iter_mut()
            .find_map(|summary| {
                let event = summary.get_mut("event")?;
                (event.get("kind")?.as_str()? == "queued_input_resolution").then_some(event)
            })
            .expect("golden hydration must contain queued_input_resolution");
        resolution
            .as_object_mut()
            .expect("queued_input_resolution must be an object")
            .insert("legacy_queue_id".to_string(), serde_json::json!("legacy"));

        assert!(serde_json::from_value::<ConversationEventWindowV1Response>(value).is_err());
    }

    /// Historical pages omit revision fields so they cannot reset live state.
    /// 历史分页省略修订字段，因此不能重置实时状态。
    #[test]
    fn historical_page_omits_live_revision_frontier() {
        let response = ConversationEventWindowV1Response::historical_page(
            ConversationEventWindowBatch {
                session_id: "session-page".to_string(),
                window_generation: 3,
                events: Vec::new(),
            },
            None,
            ConversationSessionConfigV1::from_session_config(&SessionConfig::default()),
        );
        let value = serde_json::to_value(response).expect("serialize historical page");
        assert!(value.get("memory_revision").is_none());
        assert!(value.get("durable_revision").is_none());
    }

    /// Sparse volatile patches accept only one exact revision successor.
    /// 稀疏 volatile 补丁只接受一个精确修订后继。
    #[test]
    fn volatile_patch_requires_exact_successor_revision() {
        let batch = ConversationEventWindowBatch {
            session_id: "session-live".to_string(),
            window_generation: 3,
            events: Vec::new(),
        };

        let accepted =
            ConversationEventLiveDeltaV1::volatile(batch.clone(), "test-service-epoch", 3, 4)
                .expect("exact successor edge");
        assert_eq!(accepted.base_memory_revision, Some(3));
        assert_eq!(accepted.revision, 4);
        assert!(
            ConversationEventLiveDeltaV1::volatile(batch.clone(), "test-service-epoch", 3, 3,)
                .is_err()
        );
        assert!(ConversationEventLiveDeltaV1::volatile(batch, "test-service-epoch", 3, 5).is_err());
    }

    /// The maximum revision cannot fabricate a wrapped volatile successor.
    /// 最大修订号不能伪造回绕的 volatile 后继。
    #[test]
    fn volatile_patch_rejects_revision_successor_overflow() {
        let batch = ConversationEventWindowBatch {
            session_id: "session-live-overflow".to_string(),
            window_generation: 1,
            events: Vec::new(),
        };

        assert!(
            ConversationEventLiveDeltaV1::volatile(batch, "test-service-epoch", u64::MAX, 0,)
                .is_err()
        );
    }

    /// Durable acknowledgements carry an exact durable predecessor and no events.
    /// reliable 确认携带精确可靠前驱且不含事件。
    #[test]
    fn durable_acknowledgement_carries_aggregated_revision_edge() {
        let acknowledgement = ConversationEventLiveDeltaV1::durable_ack(
            "session-live",
            3,
            "test-service-epoch",
            2,
            5,
        )
        .expect("aggregated durable edge");

        assert_eq!(acknowledgement.base_memory_revision, None);
        assert_eq!(acknowledgement.base_durable_revision, Some(2));
        assert_eq!(acknowledgement.revision, 5);
        assert!(acknowledgement.batch.events.is_empty());
        assert!(
            ConversationEventLiveDeltaV1::durable_ack(
                "session-live",
                3,
                "test-service-epoch",
                5,
                5,
            )
            .is_err()
        );
    }

    /// Reconnect recovery is a distinct multi-revision DTO, while restart drift requests hydration.
    /// 重连恢复使用独立的跨修订 DTO，服务重启漂移则要求 hydration。
    #[test]
    fn reconnect_response_distinguishes_recovery_from_rehydrate() {
        let batch = ConversationEventWindowBatch {
            session_id: "session-reconnect".to_string(),
            window_generation: 4,
            events: Vec::new(),
        };
        let recovery =
            ConversationEventReconnectV1Response::recovery(batch, "service-epoch-current", 3, 7, 6)
                .expect("multi-revision reconnect recovery");
        assert_eq!(
            recovery.disposition,
            ConversationEventReconnectDispositionV1::Recovery
        );
        assert_eq!(recovery.base_memory_revision, 3);
        assert_eq!(recovery.memory_revision, 7);
        assert_eq!(recovery.durable_revision, 6);

        let rehydrate = ConversationEventReconnectV1Response::rehydrate_required(
            "session-reconnect",
            1,
            "service-epoch-new",
            9,
            6,
            6,
            ConversationEventRehydrateReasonV1::ServiceEpochChanged,
        )
        .expect("explicit rehydrate requirement");
        assert_eq!(
            rehydrate.disposition,
            ConversationEventReconnectDispositionV1::RehydrateRequired(
                ConversationEventRehydrateReasonV1::ServiceEpochChanged
            )
        );
        assert!(rehydrate.batch.events.is_empty());
    }

    /// streaming_text_event builds one assistant text or reasoning summary for live-patch tests.
    /// streaming_text_event 为实时补丁测试构建一条 assistant 正文或思考摘要。
    fn streaming_text_event(
        seq: u64,
        reasoning: bool,
        text: &str,
        status: AssistantSemanticStatus,
        event_revision: u64,
    ) -> ConversationEventSummary {
        use vulcan_session_core::v1::event::{
            AssistantReasoningEvent, AssistantTextEvent, EventVisibility,
        };
        use vulcan_session_core::v1::ids::ApiCallId;
        let api_call_id = ApiCallId::try_new(1).expect("api call id");
        let event = if reasoning {
            EventBody::AssistantReasoning(AssistantReasoningEvent {
                api_call_id,
                text: text.to_string(),
                status,
                provider_source: None,
            })
        } else {
            EventBody::AssistantText(AssistantTextEvent {
                api_call_id,
                text: text.to_string(),
                status,
                provider_source: None,
            })
        };
        ConversationEventSummary {
            seq: Seq::try_new(seq).expect("seq"),
            turn_id: None,
            created_at_ms: 100,
            updated_at_ms: 100 + event_revision as i64,
            event_revision,
            visibility: EventVisibility::Provider,
            event,
        }
    }

    /// live_batch wraps events in the owner used by the live-patch tests.
    /// live_batch 用实时补丁测试使用的归属包装事件。
    fn live_batch(events: Vec<ConversationEventSummary>) -> ConversationEventWindowBatch {
        ConversationEventWindowBatch {
            session_id: "session-append".to_string(),
            window_generation: 2,
            events,
        }
    }

    /// apply_patch applies one volatile patch to a seq-keyed window, as the WebUI store does.
    /// apply_patch 像 WebUI store 一样把一份 volatile 补丁应用到按 seq 索引的窗口。
    fn apply_patch(
        window: &mut BTreeMap<Seq, ConversationEventSummary>,
        patch: &ConversationEventLiveDeltaV1,
    ) {
        for event in &patch.batch.events {
            window.insert(event.seq, event.clone());
        }
        for append in &patch.text_appends {
            let extended = append
                .apply(window.get(&append.seq).expect("appended event is loaded"))
                .expect("append applies to its base");
            window.insert(append.seq, extended);
        }
    }

    /// A pure streaming extension becomes an append measured in UTF-16 code units.
    /// 纯流式延长成为以 UTF-16 码元计量的追加。
    #[test]
    fn text_append_describes_pure_streaming_extensions_only() {
        let streaming = AssistantSemanticStatus::Streaming;
        let before = streaming_text_event(5, false, "你好😀", streaming, 3);
        let after = streaming_text_event(5, false, "你好😀，世界", streaming, 4);
        let append = ConversationEventTextAppendV1::between(&before, &after).expect("pure append");
        assert_eq!(append.base_text_utf16_len, 4);
        assert_eq!(append.appended_text, "，世界");
        assert_eq!(append.event_revision, 4);
        assert_eq!(append.apply(&before).expect("apply append"), after);

        let completed = streaming_text_event(
            5,
            false,
            "你好😀，世界",
            AssistantSemanticStatus::Completed,
            4,
        );
        assert!(ConversationEventTextAppendV1::between(&before, &completed).is_none());
        let rewritten = streaming_text_event(5, false, "再见", streaming, 4);
        assert!(ConversationEventTextAppendV1::between(&before, &rewritten).is_none());
        assert!(ConversationEventTextAppendV1::between(&before, &before).is_none());
        let reasoning = streaming_text_event(5, true, "你好😀，世界", streaming, 4);
        assert!(ConversationEventTextAppendV1::between(&before, &reasoning).is_none());

        let shorter = streaming_text_event(5, false, "你好", streaming, 3);
        assert!(append.apply(&shorter).is_err());
    }

    /// Appends serialize with an explicit shape and are omitted when absent.
    /// 追加以明确形状序列化，缺省时省略。
    #[test]
    fn text_appends_serialize_only_when_present() {
        let streaming = AssistantSemanticStatus::Streaming;
        let plain = ConversationEventLiveDeltaV1::volatile(live_batch(Vec::new()), "epoch", 1, 2)
            .expect("plain volatile patch");
        assert!(
            serde_json::to_value(&plain)
                .expect("serialize plain patch")
                .get("text_appends")
                .is_none()
        );

        let append = ConversationEventTextAppendV1::between(
            &streaming_text_event(7, true, "a", streaming, 1),
            &streaming_text_event(7, true, "ab", streaming, 2),
        )
        .expect("append");
        let patch = ConversationEventLiveDeltaV1::volatile_with_text_appends(
            live_batch(Vec::new()),
            vec![append],
            "epoch",
            1,
            2,
        )
        .expect("append patch");
        let value = serde_json::to_value(&patch).expect("serialize append patch");
        assert_eq!(
            value["text_appends"],
            serde_json::json!([{
                "seq": 7,
                "event_revision": 2,
                "updated_at_ms": 102,
                "base_text_utf16_len": 1,
                "appended_text": "b",
            }])
        );
        let decoded: ConversationEventLiveDeltaV1 =
            serde_json::from_value(value).expect("decode append patch");
        assert_eq!(decoded, patch);
    }

    /// Append patches reject unordered appends and seqs that are also complete events.
    /// 追加补丁拒绝乱序追加以及同时作为完整事件的 seq。
    #[test]
    fn append_patch_rejects_ambiguous_change_sets() {
        let streaming = AssistantSemanticStatus::Streaming;
        let append_for = |seq: u64| {
            ConversationEventTextAppendV1::between(
                &streaming_text_event(seq, false, "a", streaming, 1),
                &streaming_text_event(seq, false, "ab", streaming, 2),
            )
            .expect("append")
        };
        assert!(
            ConversationEventLiveDeltaV1::volatile_with_text_appends(
                live_batch(Vec::new()),
                vec![append_for(9), append_for(8)],
                "epoch",
                1,
                2,
            )
            .is_err()
        );
        assert!(
            ConversationEventLiveDeltaV1::volatile_with_text_appends(
                live_batch(vec![streaming_text_event(8, false, "ab", streaming, 2)]),
                vec![append_for(8)],
                "epoch",
                1,
                2,
            )
            .is_err()
        );
    }

    /// Merging contiguous volatile patches equals applying them one after another.
    /// 合并相邻 volatile 补丁等价于依次应用它们。
    #[test]
    fn merged_volatile_patches_apply_like_their_sequence() {
        let streaming = AssistantSemanticStatus::Streaming;
        let completed = AssistantSemanticStatus::Completed;
        let reasoning_1 = streaming_text_event(3, true, "想", streaming, 1);
        let reasoning_2 = streaming_text_event(3, true, "想一想", streaming, 2);
        let reasoning_3 = streaming_text_event(3, true, "想一想。", completed, 3);
        let answer_1 = streaming_text_event(4, false, "答", streaming, 1);
        let answer_2 = streaming_text_event(4, false, "答案😀", streaming, 2);
        let answer_3 = streaming_text_event(4, false, "答案😀是", streaming, 3);

        let append = |before: &ConversationEventSummary, after: &ConversationEventSummary| {
            ConversationEventTextAppendV1::between(before, after).expect("append")
        };
        let patches = vec![
            ConversationEventLiveDeltaV1::volatile_with_text_appends(
                live_batch(Vec::new()),
                vec![append(&reasoning_1, &reasoning_2)],
                "epoch",
                10,
                11,
            )
            .expect("patch 11"),
            ConversationEventLiveDeltaV1::volatile(
                live_batch(vec![reasoning_3.clone(), answer_1.clone()]),
                "epoch",
                11,
                12,
            )
            .expect("patch 12"),
            ConversationEventLiveDeltaV1::volatile_with_text_appends(
                live_batch(Vec::new()),
                vec![append(&answer_1, &answer_2)],
                "epoch",
                12,
                13,
            )
            .expect("patch 13"),
            ConversationEventLiveDeltaV1::volatile_with_text_appends(
                live_batch(Vec::new()),
                vec![append(&answer_2, &answer_3)],
                "epoch",
                13,
                14,
            )
            .expect("patch 14"),
        ];
        let base: BTreeMap<Seq, ConversationEventSummary> =
            [(reasoning_1.seq, reasoning_1.clone())]
                .into_iter()
                .collect();

        let mut sequential = base.clone();
        for patch in &patches {
            apply_patch(&mut sequential, patch);
        }
        // Every suffix of merges must be equivalent, including append-onto-append chains.
        // 每一段合并后缀都必须等价，包括追加接追加的链。
        for start in 0..patches.len() {
            let mut prefix = base.clone();
            for patch in &patches[..start] {
                apply_patch(&mut prefix, patch);
            }
            let merged =
                patches[start + 1..]
                    .iter()
                    .fold(patches[start].clone(), |merged, next| {
                        merged
                            .merge_volatile(next)
                            .expect("contiguous patches merge")
                    });
            assert_eq!(merged.revision, 14);
            assert_eq!(merged.base_memory_revision, Some(10 + start as u64));
            apply_patch(&mut prefix, &merged);
            assert_eq!(prefix, sequential, "merge from patch {start} diverged");
        }
        assert_eq!(sequential.get(&answer_3.seq), Some(&answer_3));
        assert_eq!(sequential.get(&reasoning_3.seq), Some(&reasoning_3));
    }

    /// Only contiguous volatile patches of the same owner and service epoch merge.
    /// 只有同一归属与服务代次的相邻 volatile 补丁可以合并。
    #[test]
    fn merge_rejects_non_contiguous_or_foreign_patches() {
        let first = ConversationEventLiveDeltaV1::volatile(live_batch(Vec::new()), "epoch", 1, 2)
            .expect("first");
        let gap = ConversationEventLiveDeltaV1::volatile(live_batch(Vec::new()), "epoch", 3, 4)
            .expect("gap");
        assert!(first.merge_volatile(&gap).is_none());
        let other_epoch =
            ConversationEventLiveDeltaV1::volatile(live_batch(Vec::new()), "other", 2, 3)
                .expect("other epoch");
        assert!(first.merge_volatile(&other_epoch).is_none());
        let mut other_generation = live_batch(Vec::new());
        other_generation.window_generation = 3;
        let other_generation =
            ConversationEventLiveDeltaV1::volatile(other_generation, "epoch", 2, 3)
                .expect("other generation");
        assert!(first.merge_volatile(&other_generation).is_none());
        let durable = ConversationEventLiveDeltaV1::durable_ack("session-append", 2, "epoch", 1, 2)
            .expect("durable");
        assert!(first.merge_volatile(&durable).is_none());
        assert!(durable.merge_volatile(&first).is_none());
    }
}
