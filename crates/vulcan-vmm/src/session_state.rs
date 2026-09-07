//! Session-owned VMM lease state and immutable prompt anchors.
//! 会话归属的 VMM 租约状态与不可变提示词锚点。

use crate::adapter::{
    render_vmm_context_item_detail, render_vmm_context_item_model, render_vmm_context_item_panel,
};
use crate::config::VmmEffectiveConfig;
use crate::protocol::{ContextItem, PreCheckResponse, SessionMemoryStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_context_injection::{InjectionPanelCard, InjectionPanelItem};

/// Current schema version for independently persisted VMM session memory state.
/// 独立持久化 VMM 会话记忆状态的当前结构版本。
pub const VMM_SESSION_MEMORY_SCHEMA_VERSION: u32 = 2;
/// Maximum physical byte length of one pretty-printed independent VMM state document, including its trailing newline.
/// 单份美化打印独立 VMM 状态文档允许的最大物理字节数，包含末尾换行符。
pub const VMM_SESSION_MEMORY_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Initial cache epoch used before the session has performed its first compaction.
/// 会话首次执行压缩前使用的初始缓存纪元。
const INITIAL_CACHE_EPOCH_ID: &str = "initial";
/// Maximum Provider-visible bytes accepted for one structured memory body.
/// 单条结构化记忆正文允许的最大 Provider 可见字节数。
const VMM_MEMORY_BODY_MAX_BYTES: usize = 256 * 1024;
/// Maximum structured memory items accepted from one precheck response.
/// 单次 precheck 响应允许的结构化记忆条目上限。
const VMM_RECALL_BATCH_MAX_ITEMS: usize = 128;
/// Maximum resident session leases retained until one successful context compaction.
/// 单次上下文压缩成功前允许驻留的会话租约上限。
const VMM_RESIDENT_LEASE_MAX_COUNT: usize = 1024;
/// Maximum immutable Provider-visible anchor bytes retained in one cache epoch.
/// 单个缓存纪元保留的不可变 Provider 可见锚点最大字节数。
const VMM_CACHE_EPOCH_ANCHOR_MAX_BYTES: usize = 3 * 1024 * 1024;

/// One typed recall batch emitted by the VMM precheck hook.
/// VMM precheck hook 产出的一批类型化召回结果。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VmmRecallBatch {
    /// Number of completed logical turns for which recalled items remain active.
    /// 召回项保持活动状态的已完成逻辑回合数。
    pub ttl_turns: u32,
    /// Provider-visible grouping tag configured for this recall.
    /// 为本次召回配置的 Provider 可见分组标签。
    pub injection_tag: String,
    /// Provider-visible grouping description configured for this recall.
    /// 为本次召回配置的 Provider 可见分组说明。
    pub injection_description: String,
    /// Structured context items selected by VMM.
    /// VMM 选出的结构化上下文项。
    pub items: Vec<ContextItem>,
}

impl VmmRecallBatch {
    /// Builds a typed recall batch from one VMM precheck response and effective configuration.
    /// 根据一份 VMM precheck 响应与生效配置构建类型化召回批次。
    ///
    /// # Parameters
    /// 参数
    /// `response` is the authoritative structured response returned by VMM.
    /// `response` 是 VMM 返回的权威结构化响应。
    /// `config` supplies the exact TTL and provider-facing grouping text.
    /// `config` 提供精确 TTL 与面向 Provider 的分组文本。
    ///
    /// # Returns
    /// 返回值
    /// A non-empty batch when VMM requested injection and supplied usable items.
    /// 当 VMM 请求注入且提供可用条目时返回非空批次。
    pub fn from_precheck_response(
        response: &PreCheckResponse,
        config: &VmmEffectiveConfig,
    ) -> Option<Self> {
        if !response.session_memory_status.allows_runtime_memory()
            || !response.should_inject
            || config.implicit_memory_turns == 0
        {
            return None;
        }
        let items = response
            .context_items
            .iter()
            .filter(|item| !render_vmm_context_item_model(item).is_empty())
            .cloned()
            .collect::<Vec<_>>();
        (!items.is_empty()).then(|| Self {
            ttl_turns: config.implicit_memory_turns,
            injection_tag: config.injection_tag.clone(),
            injection_description: config.injection_description.clone(),
            items,
        })
    }

    /// Returns whether this recall batch has no provider-visible memory item.
    /// 返回该召回批次是否没有任何 Provider 可见记忆项。
    pub fn is_empty(&self) -> bool {
        self.items
            .iter()
            .all(|item| render_vmm_context_item_model(item).is_empty())
    }
}

/// Kind of one immutable provider prompt anchor.
/// 一条不可变 Provider 提示词锚点的类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmmPromptAnchorKind {
    /// First provider-visible introduction of one or more recalled memories.
    /// 一条或多条召回记忆首次进入 Provider 可见上下文。
    Memory,
    /// Small append-only notice that invalidates expired historical memory anchors.
    /// 使已过期历史记忆锚点失效的小型追加式通知。
    Revocation,
    /// Active-memory baseline created after a context compaction epoch reset.
    /// 上下文压缩纪元重置后创建的活动记忆基线。
    Baseline,
}

impl VmmPromptAnchorKind {
    /// Returns the stable lowercase label used inside deterministic anchor identifiers.
    /// 返回确定性锚点标识中使用的稳定小写标签。
    fn as_id_label(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Revocation => "revocation",
            Self::Baseline => "baseline",
        }
    }
}

/// One immutable provider-visible prefix bound to one exact canonical user message.
/// 绑定到一条精确规范用户消息的不可变 Provider 可见前缀。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmmPromptAnchor {
    /// Stable anchor identifier retained for diagnostics and lease ownership.
    /// 为诊断与租约归属保留的稳定锚点标识。
    pub anchor_id: String,
    /// Exact cache epoch in which this anchor was created.
    /// 创建该锚点时所属的精确缓存纪元。
    pub epoch_id: String,
    /// Completed logical-turn sequence at which this anchor was bound.
    /// 该锚点被绑定时的已完成逻辑回合序号。
    pub bound_turn_sequence: u64,
    /// Exact canonical user-message identifier receiving the frozen prefix.
    /// 接收冻结前缀的精确规范用户消息标识。
    pub target_message_id: String,
    /// Semantic kind of this anchor.
    /// 该锚点的语义类型。
    pub kind: VmmPromptAnchorKind,
    /// Frozen provider-visible prefix; existing anchors are never rewritten in place.
    /// 冻结后的 Provider 可见前缀；既有锚点绝不原地改写。
    pub prefix: String,
    /// Stable memory keys described or invalidated by this anchor.
    /// 该锚点描述或失效的稳定记忆键。
    pub memory_keys: Vec<String>,
    /// Semantic hashes frozen for every memory key covered by this anchor.
    /// 为该锚点覆盖的每个记忆键冻结的语义哈希。
    pub semantic_content_hashes: BTreeMap<String, String>,
    /// Unix-millis text captured when the anchor was created.
    /// 创建锚点时捕获的 Unix 毫秒文本。
    pub created_at: String,
}

/// Prompt-anchor epoch aligned with one canonical context-compaction baseline.
/// 与一份规范上下文压缩基线对齐的提示词锚点纪元。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmmCacheEpoch {
    /// Stable epoch identifier; the compaction id is used after compaction.
    /// 稳定纪元标识；压缩后使用压缩标识。
    pub epoch_id: String,
    /// Context-compaction identifier covered by this epoch when present.
    /// 存在时表示该纪元覆盖的上下文压缩标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covered_compaction_id: Option<String>,
    /// Immutable prompt anchors still required by the active canonical history.
    /// 当前规范历史仍需使用的不可变提示词锚点。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<VmmPromptAnchor>,
    /// Monotonic revision advanced only when this epoch appends an immutable anchor.
    /// 仅在该纪元追加不可变锚点时推进的单调修订号。
    pub anchor_revision: u64,
    /// Whether active leases must be re-anchored on the next real user message.
    /// 是否必须在下一条真实用户消息上重新锚定活动租约。
    pub needs_active_baseline: bool,
}

impl Default for VmmCacheEpoch {
    /// Builds the initial pre-compaction cache epoch.
    /// 构建压缩前的初始缓存纪元。
    fn default() -> Self {
        Self {
            epoch_id: String::from(INITIAL_CACHE_EPOCH_ID),
            covered_compaction_id: None,
            anchors: Vec::new(),
            anchor_revision: 0,
            needs_active_baseline: false,
        }
    }
}

/// One cache-epoch-resident VMM memory lease with turn-sequence expiry semantics.
/// 一条采用回合序列到期语义并驻留于缓存纪元的 VMM 记忆租约。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmmMemoryLease {
    /// Stable semantic identity used as the map key.
    /// 用作映射键的稳定语义身份。
    pub memory_key: String,
    /// Hash of semantic fields excluding volatile score and display time.
    /// 排除易变评分与展示时间后的语义字段哈希。
    pub semantic_content_hash: String,
    /// Latest structured VMM item used by UI and future baselines.
    /// 供界面与后续基线使用的最新结构化 VMM 条目。
    pub item: ContextItem,
    /// Completed-turn sequence at which the lease was first recalled.
    /// 租约首次被召回时的已完成回合序列。
    pub first_recalled_turn_sequence: u64,
    /// Completed-turn sequence at which the lease was most recently recalled.
    /// 租约最近一次被召回时的已完成回合序列。
    pub last_recalled_turn_sequence: u64,
    /// First completed-turn sequence at which the lease is no longer active.
    /// 租约不再活动的首个已完成回合序列。
    pub expires_after_turn_sequence: u64,
    /// Anchor that most recently introduced the active semantic content.
    /// 最近一次引入当前活动语义内容的锚点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_anchor_id: Option<String>,
    /// Unix-millis text captured when the lease was created.
    /// 创建租约时捕获的 Unix 毫秒文本。
    pub created_at: String,
    /// Unix-millis text captured when the lease was refreshed or replaced.
    /// 刷新或替换租约时捕获的 Unix 毫秒文本。
    pub updated_at: String,
}

impl VmmMemoryLease {
    /// Returns whether this resident lease remains eligible for the next compaction baseline.
    /// 返回该驻留租约是否仍有资格进入下一次压缩基线。
    ///
    /// # Parameters
    /// 参数
    /// `completed_turn_sequence` is the authoritative number of completed logical turns.
    /// `completed_turn_sequence` 是权威的已完成逻辑回合数量。
    ///
    /// # Returns
    /// 返回值
    /// `true` while the absolute expiry sequence remains in the future.
    /// 当绝对到期序号仍位于未来时返回 `true`。
    fn is_active_at(&self, completed_turn_sequence: u64) -> bool {
        self.expires_after_turn_sequence > completed_turn_sequence
    }
}

/// One pending append-only prompt event produced by explicit VMM revocation.
/// 显式撤销 VMM 时产出的一条待追加提示词事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmmPendingPromptEvent {
    /// Stable memory key whose previous historical anchor became inactive.
    /// 先前历史锚点已不再活动的稳定记忆键。
    pub memory_key: String,
    /// Semantic hash of the historical content being invalidated without retaining its body.
    /// 被失效历史内容的语义哈希，不保留其正文。
    pub semantic_content_hash: String,
    /// Unix-millis text captured when the lease expired.
    /// 租约到期时捕获的 Unix 毫秒文本。
    pub expired_at: String,
}

/// Result counters produced by recall, expiry, or epoch transitions.
/// 召回、到期或纪元转换产出的结果计数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VmmLeaseTransitionReport {
    /// Number of newly created active leases.
    /// 新创建的活动租约数量。
    pub inserted_lease_count: usize,
    /// Number of semantically unchanged leases whose TTL was refreshed.
    /// 语义未变化且 TTL 被刷新的租约数量。
    pub refreshed_lease_count: usize,
    /// Number of stable keys whose semantic payload was replaced append-only.
    /// 语义载荷以追加方式替换的稳定键数量。
    pub replaced_lease_count: usize,
    /// Number of leases that expired or were physically reclaimed at a compaction boundary.
    /// 到期或在压缩边界被物理回收的租约数量。
    pub expired_lease_count: usize,
    /// Number of active leases revoked because VMM was explicitly disabled.
    /// 因 VMM 被显式禁用而撤销的活动租约数量。
    pub revoked_lease_count: usize,
    /// Number of immutable anchors appended by the transition.
    /// 本次转换追加的不可变锚点数量。
    pub appended_anchor_count: usize,
    /// Whether any durable VMM state changed.
    /// 是否有任何持久化 VMM 状态发生变化。
    pub changed: bool,
}

/// Provider-context capacity diagnostics derived from active leases and immutable anchors.
/// 根据活动租约与不可变锚点推导的 Provider 上下文容量诊断。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VmmCachePressure {
    /// Bytes of currently active memory bodies rendered from structured lease snapshots.
    /// 根据结构化租约快照渲染的当前活动记忆正文字节数。
    pub active_body_bytes: usize,
    /// Estimated tokens occupied by currently active memory bodies.
    /// 当前活动记忆正文占用的估算 Token 数。
    pub active_body_estimated_tokens: usize,
    /// Bytes held by immutable anchors no longer referenced by any active lease.
    /// 不再被任何活动租约引用的不可变锚点字节数。
    pub reclaimable_anchor_bytes: usize,
    /// Estimated tokens held by immutable anchors reclaimable at the next compaction.
    /// 可在下次压缩回收的不可变锚点估算 Token 数。
    pub reclaimable_anchor_estimated_tokens: usize,
}

/// Independently persisted VMM session lease and prompt-anchor state.
/// 独立持久化的 VMM 会话租约与提示词锚点状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmmSessionMemoryState {
    /// Durable document schema version.
    /// 持久化文档结构版本。
    pub schema_version: u32,
    /// Exact host session identifier owning this state.
    /// 拥有该状态的精确宿主会话标识。
    pub session_id: String,
    /// Exact source of resident recall and lifecycle state; absent until a host publication selects a source.
    /// 驻留召回与生命周期状态的准确来源；在宿主发布选择来源前为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_source: Option<crate::VmmMemoryReadSource>,
    /// Monotonic state revision used by atomic persistence and diagnostics.
    /// 供原子持久化与诊断使用的单调状态版本。
    pub revision: u64,
    /// Authoritative lifecycle status most recently returned by VMM.
    /// VMM 最近一次返回的权威生命周期状态。
    #[serde(default)]
    pub session_memory_status: SessionMemoryStatus,
    /// SHA-256 hashes of every committed Core controller command id.
    /// 每个已提交 Core 控制器命令标识的 SHA-256 哈希。
    pub committed_command_hashes: Vec<String>,
    /// Number of unique terminal logical turns consumed by this VMM state.
    /// 该 VMM 状态已消费的唯一终态逻辑回合数量。
    pub completed_turn_sequence: u64,
    /// Last terminal logical turn id consumed idempotently.
    /// 最近一次以幂等方式消费的终态逻辑回合标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_consumed_turn_id: Option<String>,
    /// Exact currently active logical turn allowed to consume TTL at its successful terminal boundary.
    /// 当前获准在成功终态边界消费 TTL 的精确活动逻辑回合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_turn_id: Option<String>,
    /// Active immutable prompt-anchor epoch.
    /// 当前活动的不可变提示词锚点纪元。
    pub cache_epoch: VmmCacheEpoch,
    /// Whether stable exact-message anchor rendering is active for this cache epoch.
    /// 当前缓存纪元是否启用稳定的精确消息锚点渲染。
    pub stable_anchor_rendering_enabled: bool,
    /// Cache-epoch-resident leases keyed by stable VMM semantic identity, including expired entries awaiting compaction.
    /// 按稳定 VMM 语义身份索引的缓存纪元驻留租约，包含等待压缩清理的过期项。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub active_leases: BTreeMap<String, VmmMemoryLease>,
    /// Explicit revocation notices waiting for the next exact user-message anchor.
    /// 等待绑定到下一条精确用户消息的显式撤销通知。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_prompt_events: Vec<VmmPendingPromptEvent>,
    /// Unix-millis text captured at the latest durable mutation.
    /// 最近一次持久化变更时捕获的 Unix 毫秒文本。
    pub updated_at: String,
}

impl Default for VmmSessionMemoryState {
    /// Builds one unowned pristine VMM session state.
    /// 构建一份尚未归属会话的纯净 VMM 状态。
    fn default() -> Self {
        Self {
            schema_version: VMM_SESSION_MEMORY_SCHEMA_VERSION,
            session_id: String::new(),
            read_source: None,
            revision: 0,
            session_memory_status: SessionMemoryStatus::Active,
            committed_command_hashes: Vec::new(),
            completed_turn_sequence: 0,
            last_consumed_turn_id: None,
            active_turn_id: None,
            cache_epoch: VmmCacheEpoch::default(),
            stable_anchor_rendering_enabled: true,
            active_leases: BTreeMap::new(),
            pending_prompt_events: Vec::new(),
            updated_at: String::new(),
        }
    }
}

impl VmmSessionMemoryState {
    /// Selects the current host read source and discards foreign or unowned cached memory atomically.
    /// 选择当前宿主读取来源，并原子丢弃其他来源或无来源的缓存记忆。
    /// Takes the exact host session, optional active source and timestamp; returns whether state changed.
    /// 接收准确宿主会话、可选活动来源和时间戳；返回状态是否改变。
    /// Controller receipts and logical-turn lineage survive source switches; old bodies, anchors and status do not.
    /// 来源切换保留控制器回执和逻辑回合沿革，不保留旧正文、锚点和状态。
    pub fn select_read_source(
        &mut self,
        session_id: &str,
        source: Option<&crate::VmmMemoryReadSource>,
        updated_at: &str,
    ) -> Result<bool, String> {
        validate_mutation_timestamp(updated_at)?;
        if let Some(source) = source {
            source.validate().map_err(|error| error.to_string())?;
        }
        // Validate ownership before the no-op path, without mutating the retained state on failure.
        // 在无操作路径之前校验归属，失败时不修改保留状态。
        let mut candidate = self.clone();
        candidate.ensure_owner(session_id, updated_at)?;
        if self.read_source.as_ref() == source
            && (source.is_some()
                || (self.active_leases.is_empty()
                    && self.pending_prompt_events.is_empty()
                    && self.cache_epoch.anchors.is_empty()
                    && self.session_memory_status == SessionMemoryStatus::Active))
        {
            return Ok(false);
        }
        // A monotonic revision makes a fresh source epoch distinct even when returning to an earlier source.
        // 单调版本确保即使切回早先来源，新来源纪元仍然不同。
        let revision = candidate.next_revision()?;
        candidate.read_source = source.cloned();
        candidate.active_leases.clear();
        candidate.pending_prompt_events.clear();
        candidate.cache_epoch = VmmCacheEpoch {
            epoch_id: format!("source-{revision}"),
            ..VmmCacheEpoch::default()
        };
        candidate.session_memory_status = SessionMemoryStatus::Active;
        candidate.revision = revision;
        candidate.updated_at = updated_at.to_owned();
        candidate.validate(session_id)?;
        *self = candidate;
        Ok(true)
    }

    /// Builds one pristine state owned by an exact host session.
    /// 构建一份归属于精确宿主会话的纯净状态。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact persisted session owner.
    /// `session_id` 是精确的持久化会话所有者。
    /// `updated_at` is the initialization timestamp.
    /// `updated_at` 是初始化时间戳。
    ///
    /// # Returns
    /// 返回值
    /// A schema-versioned empty lease state with established ownership, or an explicit invalid-owner/timestamp error.
    /// 返回已建立归属且带结构版本的空租约状态；会话归属或时间戳无效时返回明确错误。
    pub fn new(
        session_id: impl Into<String>,
        updated_at: impl Into<String>,
    ) -> Result<Self, String> {
        let session_id = session_id.into();
        let updated_at = updated_at.into();
        if session_id.trim().is_empty() {
            return Err(String::from("VMM session owner must not be empty."));
        }
        if updated_at.trim().is_empty() {
            return Err(String::from(
                "VMM session memory initialization timestamp must not be empty.",
            ));
        }
        let state = Self {
            session_id: session_id.clone(),
            revision: 1,
            updated_at,
            ..Self::default()
        };
        state.validate(&session_id)?;
        Ok(state)
    }

    /// Returns whether one exact Core controller command has already committed.
    /// 返回某个精确 Core 控制器命令是否已经提交。
    ///
    /// # Parameters
    /// 参数
    /// `command_id` is the stable session-scoped command identity.
    /// `command_id` 是稳定的会话级命令身份。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the durable command ledger contains the command hash.
    /// 当持久化命令账本包含该命令哈希时返回 `true`。
    pub fn has_committed_controller_command(&self, command_id: &str) -> bool {
        let command_hash = controller_command_hash(command_id);
        self.committed_command_hashes
            .iter()
            .any(|existing_hash| existing_hash == &command_hash)
    }

    /// Records one controller command inside the same durable revision as its domain mutation.
    /// 在领域变更的同一持久化 revision 中记录一个控制器命令。
    ///
    /// # Parameters
    /// 参数
    /// `command_id` is the stable non-empty session-scoped command identity.
    /// `command_id` 是稳定且非空的会话级命令身份。
    /// `revision_before_command` is the revision observed before the command mutation began.
    /// `revision_before_command` 是命令变更开始前观察到的 revision。
    /// `updated_at` is the exact command commit timestamp.
    /// `updated_at` 是精确的命令提交时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Success after the receipt is retained and the state owns a new revision when necessary.
    /// 在回执被保留且状态按需拥有新 revision 后返回成功。
    pub fn record_committed_controller_command(
        &mut self,
        command_id: &str,
        revision_before_command: u64,
        updated_at: &str,
    ) -> Result<(), String> {
        if command_id.trim().is_empty() {
            return Err(String::from(
                "Core VMM controller command id must not be empty.",
            ));
        }
        if command_id.len() > 512 {
            return Err(String::from(
                "Core VMM controller command id exceeds the 512 byte limit.",
            ));
        }
        if self.has_committed_controller_command(command_id) {
            return Ok(());
        }
        // command_hash retains permanent membership proof without persisting caller-controlled command text.
        // command_hash 在不持久化调用方控制命令文本的情况下保留永久成员证明。
        self.committed_command_hashes
            .push(controller_command_hash(command_id));
        if self.revision == revision_before_command {
            self.mark_changed(updated_at)?;
        } else {
            self.updated_at = updated_at.to_string();
        }
        Ok(())
    }

    /// Returns whether the state has never acquired session ownership or durable VMM data.
    /// 返回该状态是否从未获得会话归属或持久化 VMM 数据。
    pub fn is_pristine(&self) -> bool {
        self.session_id.is_empty()
            && self.read_source.is_none()
            && self.revision == 0
            && self.session_memory_status == SessionMemoryStatus::Active
            && self.completed_turn_sequence == 0
            && self.last_consumed_turn_id.is_none()
            && self.active_turn_id.is_none()
            && self.active_leases.is_empty()
            && self.pending_prompt_events.is_empty()
            && self.cache_epoch == VmmCacheEpoch::default()
    }

    /// Applies the authoritative VMM lifecycle status and revokes local memory when runtime use is paused.
    /// 应用 VMM 权威生命周期状态，并在运行期记忆暂停时撤销本地记忆。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact host session identifier returned by the owning request path.
    /// `session_id` 是所属请求链路返回的精确宿主会话标识。
    /// `status` is the authoritative lifecycle status returned by VMM.
    /// `status` 是 VMM 返回的权威生命周期状态。
    /// `updated_at` is the durable mutation timestamp.
    /// `updated_at` 是持久化变更时间戳。
    ///
    /// # Returns
    /// 返回值
    /// `true` when durable local state changed, otherwise `false`.
    /// 本地持久化状态发生变化时返回 `true`，否则返回 `false`。
    pub fn apply_authoritative_session_status(
        &mut self,
        session_id: &str,
        status: SessionMemoryStatus,
        updated_at: &str,
    ) -> Result<bool, String> {
        validate_mutation_timestamp(updated_at)?;
        let must_revoke = !status.allows_runtime_memory()
            && (!self.active_leases.is_empty()
                || !self.pending_prompt_events.is_empty()
                || !self.cache_epoch.anchors.is_empty()
                || self.active_turn_id.is_some());
        if self.session_memory_status == status && !must_revoke {
            return Ok(false);
        }
        let previous_state = self.clone();
        self.ensure_owner(session_id, updated_at)?;
        self.session_memory_status = status;
        if !status.allows_runtime_memory() {
            self.active_leases.clear();
            self.pending_prompt_events.clear();
            self.cache_epoch = VmmCacheEpoch::default();
            self.active_turn_id = None;
        }
        if let Err(error) = self.mark_changed(updated_at) {
            *self = previous_state;
            return Err(error);
        }
        Ok(true)
    }

    /// Returns whether an independent `vmm.json` document is required.
    /// 返回是否需要独立的 `vmm.json` 文档。
    pub fn requires_persistence(&self) -> bool {
        !self.is_pristine()
    }

    /// Calculates active-body and reclaimable-anchor capacity pressure without exposing memory text.
    /// 在不暴露记忆正文的情况下计算活动正文与可回收锚点容量压力。
    ///
    /// # Returns
    /// 返回值
    /// Aggregate byte and token estimates suitable for diagnostics and compaction prioritization.
    /// 返回适用于诊断与压缩优先级判断的聚合字节数与 Token 估算。
    pub fn cache_pressure(&self) -> VmmCachePressure {
        // completed_turn_sequence is copied once so every pressure component uses one exact TTL boundary.
        // completed_turn_sequence 仅复制一次，确保所有压力指标使用同一个精确 TTL 边界。
        let completed_turn_sequence = self.completed_turn_sequence;
        // active_anchor_ids identify immutable prefixes still required by at least one active lease.
        // active_anchor_ids 标识仍被至少一条活动租约需要的不可变前缀。
        let active_anchor_ids = self
            .active_leases
            .values()
            .filter(|lease| lease.is_active_at(completed_turn_sequence))
            .filter_map(|lease| lease.active_anchor_id.as_deref())
            .collect::<BTreeSet<_>>();
        // active_body_bytes is derived from current structured snapshots rather than historical anchors.
        // active_body_bytes 根据当前结构化快照计算，而不是读取历史锚点。
        let active_body_bytes = self
            .active_leases
            .values()
            .filter(|lease| lease.is_active_at(completed_turn_sequence))
            .map(|lease| render_vmm_context_item_model(&lease.item).len())
            .sum::<usize>();
        // reclaimable_anchor_bytes counts only whole immutable prefixes with no active lease reference.
        // reclaimable_anchor_bytes 仅统计没有活动租约引用的完整不可变前缀。
        let reclaimable_anchor_bytes = self
            .cache_epoch
            .anchors
            .iter()
            .filter(|anchor| !active_anchor_ids.contains(anchor.anchor_id.as_str()))
            .map(|anchor| anchor.prefix.len())
            .sum::<usize>();
        VmmCachePressure {
            active_body_bytes,
            active_body_estimated_tokens: estimate_vmm_text_tokens_from_bytes(active_body_bytes),
            reclaimable_anchor_bytes,
            reclaimable_anchor_estimated_tokens: estimate_vmm_text_tokens_from_bytes(
                reclaimable_anchor_bytes,
            ),
        }
    }

    /// Validates schema, ownership, stable keys, TTL bounds, and anchor references.
    /// 验证结构版本、归属、稳定键、TTL 边界与锚点引用。
    ///
    /// # Parameters
    /// 参数
    /// `expected_session_id` is the exact directory owner expected by the loader.
    /// `expected_session_id` 是加载器按目录期望的精确所有者。
    ///
    /// # Returns
    /// 返回值
    /// Success only when every persisted invariant is satisfied.
    /// 仅当全部持久化不变量满足时返回成功。
    pub fn validate(&self, expected_session_id: &str) -> Result<(), String> {
        if let Some(source) = &self.read_source {
            source.validate().map_err(|error| error.to_string())?;
        }
        if self.schema_version != VMM_SESSION_MEMORY_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported VMM session memory schema version {}.",
                self.schema_version
            ));
        }
        if expected_session_id.trim().is_empty() || self.session_id != expected_session_id {
            return Err(format!(
                "VMM session memory owner mismatch: expected `{expected_session_id}`, found `{}`.",
                self.session_id
            ));
        }
        if self.revision == 0 {
            return Err(String::from(
                "Owned VMM session memory must have a positive durable revision.",
            ));
        }
        // command_hashes proves the permanent idempotency ledger contains only unique SHA-256 identities.
        // command_hashes 证明永久幂等账本仅包含唯一的 SHA-256 身份。
        let command_hashes = self
            .committed_command_hashes
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if command_hashes.len() != self.committed_command_hashes.len()
            || command_hashes.iter().any(|command_hash| {
                command_hash.len() != 64
                    || !command_hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        {
            return Err(String::from(
                "VMM session memory controller command hashes must be unique lowercase SHA-256 values.",
            ));
        }
        if self.updated_at.trim().is_empty() {
            return Err(String::from(
                "Owned VMM session memory requires a non-empty mutation timestamp.",
            ));
        }
        if (self.completed_turn_sequence == 0) != self.last_consumed_turn_id.is_none() {
            return Err(String::from(
                "VMM completed-turn sequence and last consumed turn id must become initialized together.",
            ));
        }
        if self
            .active_turn_id
            .as_deref()
            .is_some_and(|turn_id| turn_id.trim().is_empty())
        {
            return Err(String::from(
                "VMM active logical turn id must not be empty.",
            ));
        }
        if self.active_turn_id == self.last_consumed_turn_id && self.active_turn_id.is_some() {
            return Err(String::from(
                "VMM active logical turn must not equal the last consumed terminal turn.",
            ));
        }
        if self.cache_epoch.epoch_id.trim().is_empty() {
            return Err(String::from("VMM cache epoch id must not be empty."));
        }
        if !self.stable_anchor_rendering_enabled
            && (!self.cache_epoch.anchors.is_empty() || self.cache_epoch.needs_active_baseline)
        {
            return Err(String::from(
                "Disabled VMM stable-anchor rendering requires an empty anchor epoch with no pending baseline.",
            ));
        }
        // anchors_by_id supports both duplicate detection and exact active-lease ownership validation.
        // anchors_by_id 同时支持重复检测与精确活动租约归属校验。
        let mut anchors_by_id = BTreeMap::new();
        for anchor in &self.cache_epoch.anchors {
            if anchor.anchor_id.trim().is_empty()
                || anchor.epoch_id != self.cache_epoch.epoch_id
                || anchor.target_message_id.trim().is_empty()
                || anchor.prefix.trim().is_empty()
            {
                return Err(String::from(
                    "VMM prompt anchors require the active epoch id plus non-empty ids, target message ids, and prefixes.",
                ));
            }
            if anchors_by_id
                .insert(anchor.anchor_id.as_str(), anchor)
                .is_some()
            {
                return Err(format!(
                    "Duplicate VMM prompt anchor id `{}`.",
                    anchor.anchor_id
                ));
            }
            // Persisted anchor keys must have a one-to-one frozen semantic-hash mapping.
            // 持久化锚点键必须与冻结语义哈希形成一一对应关系。
            let memory_keys = anchor
                .memory_keys
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            if memory_keys.len() != anchor.memory_keys.len()
                || memory_keys
                    .iter()
                    .any(|memory_key| memory_key.trim().is_empty())
                || anchor.semantic_content_hashes.len() != memory_keys.len()
                || memory_keys.iter().any(|memory_key| {
                    anchor
                        .semantic_content_hashes
                        .get(*memory_key)
                        .is_none_or(|hash| hash.trim().is_empty())
                })
            {
                return Err(format!(
                    "VMM prompt anchor `{}` requires one non-empty semantic hash per unique memory key.",
                    anchor.anchor_id
                ));
            }
        }
        if self.cache_epoch.anchor_revision != self.cache_epoch.anchors.len() as u64 {
            return Err(format!(
                "VMM cache epoch anchor revision {} does not match {} immutable anchors.",
                self.cache_epoch.anchor_revision,
                self.cache_epoch.anchors.len()
            ));
        }
        if self.cache_epoch.needs_active_baseline
            && !self
                .active_leases
                .values()
                .any(|lease| lease.is_active_at(self.completed_turn_sequence))
        {
            return Err(String::from(
                "VMM cache epoch cannot require an active baseline when no active lease exists.",
            ));
        }
        for (memory_key, lease) in &self.active_leases {
            if memory_key != &lease.memory_key || memory_key.trim().is_empty() {
                return Err(format!(
                    "VMM lease map key `{memory_key}` does not match its owned memory key `{}`.",
                    lease.memory_key
                ));
            }
            if lease.semantic_content_hash.trim().is_empty() {
                return Err(format!(
                    "VMM lease `{memory_key}` has no semantic content hash."
                ));
            }
            match lease.active_anchor_id.as_deref() {
                Some(anchor_id) => {
                    let Some(anchor) = anchors_by_id.get(anchor_id).copied() else {
                        return Err(format!(
                            "VMM lease `{memory_key}` references missing anchor `{anchor_id}`."
                        ));
                    };
                    if anchor.semantic_content_hashes.get(memory_key)
                        != Some(&lease.semantic_content_hash)
                    {
                        return Err(format!(
                            "VMM lease `{memory_key}` does not match the semantic hash frozen by active anchor `{anchor_id}`."
                        ));
                    }
                }
                None if lease.is_active_at(self.completed_turn_sequence)
                    && self.stable_anchor_rendering_enabled
                    && !self.cache_epoch.needs_active_baseline =>
                {
                    return Err(format!(
                        "VMM lease `{memory_key}` has no active anchor outside a pending baseline transition."
                    ));
                }
                None => {}
            }
        }
        // pending_memory_keys guarantees one body-free revocation identity is retained at most once.
        // pending_memory_keys 保证每个无正文失效身份最多仅保留一次。
        let mut pending_memory_keys = BTreeSet::new();
        for event in &self.pending_prompt_events {
            if event.memory_key.trim().is_empty() || event.semantic_content_hash.trim().is_empty() {
                return Err(String::from(
                    "Pending VMM prompt events require a non-empty memory key and semantic hash.",
                ));
            }
            if !pending_memory_keys.insert(event.memory_key.as_str()) {
                return Err(format!(
                    "Duplicate pending VMM prompt event for memory key `{}`.",
                    event.memory_key
                ));
            }
        }
        self.validate_capacity_limits()
    }

    /// Validates bounded-memory limits for independently persisted leases and immutable anchors.
    /// 验证独立持久化租约与不可变锚点的有界内存限制。
    ///
    /// # Returns
    /// 返回值
    /// Success when every body, collection, and epoch remains within its explicit limit.
    /// 当每段正文、集合与纪元均处于显式限制内时返回成功。
    fn validate_capacity_limits(&self) -> Result<(), String> {
        if self.active_leases.len() > VMM_RESIDENT_LEASE_MAX_COUNT {
            return Err(format!(
                "VMM resident lease count {} exceeds the limit of {VMM_RESIDENT_LEASE_MAX_COUNT}.",
                self.active_leases.len()
            ));
        }
        for (memory_key, lease) in &self.active_leases {
            let body_bytes = render_vmm_context_item_model(&lease.item).len();
            if body_bytes == 0 {
                return Err(format!("VMM memory body `{memory_key}` must not be empty."));
            }
            if body_bytes > VMM_MEMORY_BODY_MAX_BYTES {
                return Err(format!(
                    "VMM memory body `{memory_key}` contains {body_bytes} bytes and exceeds the limit of {VMM_MEMORY_BODY_MAX_BYTES}."
                ));
            }
        }
        let anchor_bytes = self
            .cache_epoch
            .anchors
            .iter()
            .try_fold(0usize, |total, anchor| {
                total.checked_add(anchor.prefix.len()).ok_or_else(|| {
                    String::from("VMM cache epoch anchor byte accounting overflowed.")
                })
            })?;
        if anchor_bytes > VMM_CACHE_EPOCH_ANCHOR_MAX_BYTES {
            return Err(format!(
                "VMM cache epoch contains {anchor_bytes} immutable anchor bytes and exceeds the limit of {VMM_CACHE_EPOCH_ANCHOR_MAX_BYTES}."
            ));
        }
        // encoded_bytes mirrors the exact pretty JSON and trailing-newline contract used by workspace persistence.
        // encoded_bytes 与工作区持久化使用的美化 JSON 和末尾换行符契约完全一致。
        let encoded_bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("Failed to encode VMM session memory state: {error}"))?
            .len()
            .checked_add(1)
            .ok_or_else(|| String::from("VMM state byte accounting overflowed."))?;
        if encoded_bytes > VMM_SESSION_MEMORY_MAX_BYTES {
            return Err(format!(
                "VMM session memory document contains {encoded_bytes} bytes and exceeds the physical limit of {VMM_SESSION_MEMORY_MAX_BYTES}."
            ));
        }
        Ok(())
    }

    /// Applies one typed recall batch to one exact canonical user-message anchor.
    /// 将一批类型化召回应用到一条精确规范用户消息锚点。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact host session owner.
    /// `session_id` 是精确的宿主会话所有者。
    /// `target_message_id` is the canonical user message receiving append-only prefixes.
    /// `target_message_id` 是接收追加式前缀的规范用户消息。
    /// `batch` contains authoritative VMM items and the configured TTL.
    /// `batch` 包含权威 VMM 条目与已配置 TTL。
    /// `updated_at` is the mutation timestamp.
    /// `updated_at` 是本次变更时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Transition counters or a strict invariant error without message-target fallback.
    /// 返回转换计数；若不变量失败则严格报错，绝不回退消息目标。
    pub fn apply_recall_batch(
        &mut self,
        session_id: &str,
        target_message_id: &str,
        batch: &VmmRecallBatch,
        updated_at: &str,
    ) -> Result<VmmLeaseTransitionReport, String> {
        if !self.session_memory_status.allows_runtime_memory() {
            return Err(format!(
                "VMM recall is disabled while session memory status is {:?}.",
                self.session_memory_status
            ));
        }
        if batch.ttl_turns == 0 || batch.is_empty() {
            return Ok(VmmLeaseTransitionReport::default());
        }
        if target_message_id.trim().is_empty() {
            return Err(String::from(
                "VMM recall requires an exact canonical user-message id.",
            ));
        }
        validate_mutation_timestamp(updated_at)?;
        validate_recall_batch_capacity(batch)?;
        // expiry_sequence is checked before any ownership, pending-event, lease, or anchor mutation.
        // expiry_sequence 在任何归属、待处理事件、租约或锚点变更前完成溢出校验。
        let expiry_sequence = self
            .completed_turn_sequence
            .checked_add(u64::from(batch.ttl_turns))
            .ok_or_else(|| String::from("VMM recall expiry sequence overflowed."))?;
        // next_revision proves the durable lineage can advance before any state mutation occurs.
        // next_revision 在任何状态变更发生前证明持久化谱系可以推进。
        self.next_revision()?;
        // previous_state restores the exact durable state if a capacity invariant rejects the candidate.
        // previous_state 在容量不变量拒绝候选时恢复精确的持久化状态。
        let previous_state = self.clone();
        self.ensure_owner(session_id, updated_at)?;
        let mut report = VmmLeaseTransitionReport::default();
        let anchor_revision_before_pending = self.cache_epoch.anchor_revision;
        if self.bind_pending_prompt_events(target_message_id, updated_at) {
            report.appended_anchor_count +=
                usize::from(self.cache_epoch.anchor_revision > anchor_revision_before_pending);
            report.changed = true;
        }
        if self.bind_active_baseline_if_needed(target_message_id, updated_at) {
            report.appended_anchor_count += 1;
            report.changed = true;
        }

        // Semantic identity is immutable: one stable VMM id must always resolve to the same ABS content.
        // 语义身份不可变：同一个稳定 VMM 标识必须始终解析为相同的 ABS 内容。
        for item in &batch.items {
            let normalized_item = normalized_context_item(item);
            if render_vmm_context_item_model(&normalized_item).is_empty() {
                continue;
            }
            let memory_key = stable_memory_key(&normalized_item);
            if let Some(lease) = self.active_leases.get(&memory_key)
                && lease.item.text != normalized_item.text
            {
                *self = previous_state;
                return Err(format!(
                    "VMM identity `{memory_key}` resolved to different ABS content."
                ));
            }
        }

        let mut introduced_items = Vec::new();
        let mut introduced_keys = Vec::new();
        for item in &batch.items {
            let normalized_item = normalized_context_item(item);
            if render_vmm_context_item_model(&normalized_item).is_empty() {
                continue;
            }
            let memory_key = stable_memory_key(&normalized_item);
            let semantic_content_hash = semantic_content_hash(&memory_key, &normalized_item);
            // stable_anchor_rendering_enabled is copied before the mutable lease borrow.
            // stable_anchor_rendering_enabled 在可变借用租约前完成复制。
            let stable_anchor_rendering_enabled = self.stable_anchor_rendering_enabled;
            match self.active_leases.get_mut(&memory_key) {
                Some(lease) => {
                    // requires_anchor is false for normal active or expired recalls because their immutable epoch anchor remains resident.
                    // requires_anchor 对普通活动或过期召回为 false，因为其不可变纪元锚点仍然驻留。
                    let requires_anchor =
                        stable_anchor_rendering_enabled && lease.active_anchor_id.is_none();
                    lease.item = normalized_item.clone();
                    lease.last_recalled_turn_sequence = self.completed_turn_sequence;
                    lease.expires_after_turn_sequence = expiry_sequence;
                    lease.updated_at = updated_at.to_string();
                    if requires_anchor {
                        // A reset epoch has no frozen semantic version, so the new anchor adopts the latest metadata hash.
                        // 重置后的纪元没有冻结语义版本，因此新锚点采用最新元数据哈希。
                        lease.semantic_content_hash = semantic_content_hash;
                        introduced_keys.push(memory_key);
                        introduced_items.push(normalized_item);
                    }
                    report.refreshed_lease_count += 1;
                    report.changed = true;
                }
                None => {
                    self.active_leases.insert(
                        memory_key.clone(),
                        VmmMemoryLease {
                            memory_key: memory_key.clone(),
                            semantic_content_hash,
                            item: normalized_item.clone(),
                            first_recalled_turn_sequence: self.completed_turn_sequence,
                            last_recalled_turn_sequence: self.completed_turn_sequence,
                            expires_after_turn_sequence: expiry_sequence,
                            active_anchor_id: None,
                            created_at: updated_at.to_string(),
                            updated_at: updated_at.to_string(),
                        },
                    );
                    introduced_keys.push(memory_key);
                    introduced_items.push(normalized_item);
                    report.inserted_lease_count += 1;
                    report.changed = true;
                }
            }
        }

        if self.stable_anchor_rendering_enabled && !introduced_items.is_empty() {
            // prefix introduces only identities that have no resident anchor in the current cache epoch.
            // prefix 仅引入当前缓存纪元中不存在驻留锚点的身份。
            let prefix = render_recall_anchor_prefix(batch, &introduced_items);
            let anchor_id = self.append_anchor(
                target_message_id,
                VmmPromptAnchorKind::Memory,
                prefix,
                introduced_keys.clone(),
                updated_at,
            );
            for memory_key in introduced_keys {
                if let Some(lease) = self.active_leases.get_mut(&memory_key) {
                    lease.active_anchor_id = Some(anchor_id.clone());
                }
            }
            report.appended_anchor_count += 1;
        }
        if report.changed {
            self.mark_changed(updated_at)?;
        }
        if let Err(error) = self.validate_capacity_limits() {
            *self = previous_state;
            return Err(error);
        }
        Ok(report)
    }

    /// Binds pending revocations and post-compaction baselines to the next exact user message.
    /// 将待处理失效通知与压缩后基线绑定到下一条精确用户消息。
    ///
    /// # Parameters
    /// 参数
    /// `target_message_id` is the canonical user message receiving pending append-only state.
    /// `target_message_id` 是接收待处理追加式状态的规范用户消息。
    /// `updated_at` is the anchor creation timestamp.
    /// `updated_at` 是锚点创建时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Transition counters; pristine state remains unowned and unchanged.
    /// 返回转换计数；纯净状态保持无归属且不发生变化。
    pub fn bind_pending_state_to_user(
        &mut self,
        target_message_id: &str,
        updated_at: &str,
    ) -> Result<VmmLeaseTransitionReport, String> {
        if self.is_pristine() {
            return Ok(VmmLeaseTransitionReport::default());
        }
        if target_message_id.trim().is_empty() {
            return Err(String::from(
                "VMM pending state requires an exact canonical user-message id.",
            ));
        }
        validate_mutation_timestamp(updated_at)?;
        // previous_state restores pending events and epoch state if the rendered candidate exceeds limits.
        // previous_state 在渲染候选超限时恢复待处理事件与纪元状态。
        let previous_state = self.clone();
        let mut report = VmmLeaseTransitionReport::default();
        let anchor_revision_before_pending = self.cache_epoch.anchor_revision;
        if self.bind_pending_prompt_events(target_message_id, updated_at) {
            report.appended_anchor_count +=
                usize::from(self.cache_epoch.anchor_revision > anchor_revision_before_pending);
            report.changed = true;
        }
        if self.bind_active_baseline_if_needed(target_message_id, updated_at) {
            report.appended_anchor_count += 1;
            report.changed = true;
        }
        if report.changed
            && let Err(error) = self.mark_changed(updated_at)
        {
            *self = previous_state;
            return Err(error);
        }
        if let Err(error) = self.validate_capacity_limits() {
            *self = previous_state;
            return Err(error);
        }
        Ok(report)
    }

    /// Registers the one authoritative logical turn allowed to consume VMM TTL.
    /// 登记唯一获准消费 VMM TTL 的权威逻辑回合。
    ///
    /// # Parameters
    /// 参数
    /// `turn_id` is the exact active conversation-hook turn identifier.
    /// `turn_id` 是精确的活动会话 Hook 回合标识。
    /// `updated_at` is the pre-provider registration timestamp.
    /// `updated_at` 是 Provider 调用前的登记时间戳。
    ///
    /// # Returns
    /// 返回值
    /// A changed report when a new active boundary is registered, or a strict no-op for pristine or already-matching state.
    /// 登记新活动边界时返回变更报告；纯净状态或已匹配状态返回严格无副作用结果。
    pub fn register_active_turn(
        &mut self,
        turn_id: &str,
        updated_at: &str,
    ) -> Result<VmmLeaseTransitionReport, String> {
        if self.is_pristine() {
            return Ok(VmmLeaseTransitionReport::default());
        }
        if turn_id.trim().is_empty() {
            return Err(String::from(
                "VMM active-turn registration requires a non-empty logical turn id.",
            ));
        }
        if self.last_consumed_turn_id.as_deref() == Some(turn_id) {
            return Err(format!(
                "VMM logical turn `{turn_id}` was already consumed and cannot become active again."
            ));
        }
        if self.active_turn_id.as_deref() == Some(turn_id) {
            return Ok(VmmLeaseTransitionReport::default());
        }
        validate_mutation_timestamp(updated_at)?;
        // next_revision rejects an exhausted durable lineage before replacing active-turn ownership.
        // next_revision 在替换活动回合所有权前拒绝已耗尽的持久化谱系。
        self.next_revision()?;
        // Replacing a different active id explicitly abandons an unsuccessful or interrupted turn without consuming TTL.
        // 替换不同的活动标识会显式放弃未成功或被中断的回合，且不消费 TTL。
        self.active_turn_id = Some(turn_id.to_string());
        self.mark_changed(updated_at)?;
        Ok(VmmLeaseTransitionReport {
            changed: true,
            ..VmmLeaseTransitionReport::default()
        })
    }

    /// Abandons one failed active logical turn without consuming TTL or advancing terminal counters.
    /// 放弃一个失败的活动逻辑回合，且不消费 TTL、不推进终态计数。
    ///
    /// # Parameters
    /// 参数
    /// `turn_id` is the exact failed logical-turn identifier currently holding active ownership.
    /// `turn_id` 是当前持有活动所有权的精确失败逻辑回合标识。
    /// `updated_at` is the failure-cleanup timestamp.
    /// `updated_at` 是失败清理时间戳。
    ///
    /// # Returns
    /// 返回值
    /// A changed report after exact ownership is cleared, or a strict ownership error.
    /// 精确所有权清除后返回变更报告，否则返回严格所有权错误。
    pub fn abandon_active_turn(
        &mut self,
        turn_id: &str,
        updated_at: &str,
    ) -> Result<VmmLeaseTransitionReport, String> {
        if self.is_pristine() {
            return Ok(VmmLeaseTransitionReport::default());
        }
        if turn_id.trim().is_empty() {
            return Err(String::from(
                "VMM active-turn abandonment requires a non-empty logical turn id.",
            ));
        }
        validate_mutation_timestamp(updated_at)?;
        if self.active_turn_id.as_deref() != Some(turn_id) {
            return Err(format!(
                "VMM abandoned turn `{turn_id}` does not match the registered active logical turn {:?}.",
                self.active_turn_id
            ));
        }
        // next_revision rejects an exhausted durable lineage before active ownership is cleared.
        // next_revision 在清除活动所有权前拒绝已耗尽的持久化谱系。
        self.next_revision()?;
        self.active_turn_id = None;
        self.mark_changed(updated_at)?;
        Ok(VmmLeaseTransitionReport {
            changed: true,
            ..VmmLeaseTransitionReport::default()
        })
    }

    /// Consumes one unique terminal logical turn while retaining expired leases until compaction.
    /// 消费一个唯一终态逻辑回合，并将过期租约保留到压缩边界。
    ///
    /// # Parameters
    /// 参数
    /// `turn_id` is the authoritative completed logical-turn identifier.
    /// `turn_id` 是权威的已完成逻辑回合标识。
    /// `updated_at` is the terminal consumption timestamp.
    /// `updated_at` 是终态消费时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Idempotent transition counters; the same turn id never decrements twice.
    /// 返回幂等转换计数；同一回合标识绝不会重复递减。
    pub fn consume_turn(
        &mut self,
        turn_id: &str,
        updated_at: &str,
    ) -> Result<VmmLeaseTransitionReport, String> {
        if self.is_pristine() {
            return Ok(VmmLeaseTransitionReport::default());
        }
        if turn_id.trim().is_empty() {
            return Err(String::from(
                "VMM terminal TTL consumption requires a non-empty logical turn id.",
            ));
        }
        if self.last_consumed_turn_id.as_deref() == Some(turn_id) {
            return Ok(VmmLeaseTransitionReport::default());
        }
        validate_mutation_timestamp(updated_at)?;
        if self.active_turn_id.as_deref() != Some(turn_id) {
            return Err(format!(
                "VMM terminal turn `{turn_id}` does not match the registered active logical turn {:?}.",
                self.active_turn_id
            ));
        }
        // next_revision rejects revision exhaustion before terminal counters or leases can change.
        // next_revision 在终态计数或租约发生变化前拒绝修订号耗尽。
        self.next_revision()?;
        self.completed_turn_sequence =
            self.completed_turn_sequence.checked_add(1).ok_or_else(|| {
                String::from("VMM completed-turn sequence overflowed at terminal consumption.")
            })?;
        self.last_consumed_turn_id = Some(turn_id.to_string());
        self.active_turn_id = None;
        // newly_expired_lease_count reports only the exact boundary crossed by this unique terminal turn.
        // newly_expired_lease_count 仅统计本次唯一终态回合跨越的精确到期边界。
        let newly_expired_lease_count = self
            .active_leases
            .values()
            .filter(|lease| lease.expires_after_turn_sequence == self.completed_turn_sequence)
            .count();
        if !self
            .active_leases
            .values()
            .any(|lease| lease.is_active_at(self.completed_turn_sequence))
        {
            // No baseline can be rendered after the final resident lease becomes expired.
            // 最后一条驻留租约过期后不再可能渲染活动基线。
            self.cache_epoch.needs_active_baseline = false;
        }
        self.mark_changed(updated_at)?;
        Ok(VmmLeaseTransitionReport {
            expired_lease_count: newly_expired_lease_count,
            changed: true,
            ..VmmLeaseTransitionReport::default()
        })
    }

    /// Revokes one exact resident lease after an explicit session-scoped user action.
    /// 在用户执行显式会话级操作后撤销一条精确驻留租约。
    ///
    /// # Parameters
    /// 参数
    /// `memory_key` is the exact stable key emitted by the VMM memory panel.
    /// `memory_key` 是 VMM 记忆面板输出的精确稳定键。
    /// `updated_at` is the exact revocation timestamp.
    /// `updated_at` 是本次撤销的精确时间戳。
    ///
    /// # Returns
    /// 返回值
    /// A changed report when the resident existed, or a strict no-op when it was absent.
    /// 驻留项存在时返回发生变更的报告；不存在时严格无副作用。
    pub fn revoke_lease(
        &mut self,
        memory_key: &str,
        updated_at: &str,
    ) -> Result<VmmLeaseTransitionReport, String> {
        if memory_key.trim().is_empty() {
            return Err(String::from(
                "VMM lease revocation requires a non-empty stable memory key.",
            ));
        }
        let Some(lease) = self.active_leases.get(memory_key) else {
            return Ok(VmmLeaseTransitionReport::default());
        };
        validate_mutation_timestamp(updated_at)?;
        self.next_revision()?;
        // revoked_hash is copied before removal so a body-free prompt notice can retain exact identity evidence.
        // revoked_hash 在移除前复制，使无正文提示通知仍可保留精确身份依据。
        let revoked_hash = lease.semantic_content_hash.clone();
        // had_resident_anchor distinguishes a visible historical prefix from an anchorless rendering mode.
        // had_resident_anchor 区分可见历史前缀与无锚点渲染模式。
        let had_resident_anchor = lease.active_anchor_id.is_some();
        self.active_leases.remove(memory_key);
        if had_resident_anchor
            && !self
                .pending_prompt_events
                .iter()
                .any(|event| event.memory_key == memory_key)
        {
            self.pending_prompt_events.push(VmmPendingPromptEvent {
                memory_key: memory_key.to_string(),
                semantic_content_hash: revoked_hash,
                expired_at: updated_at.to_string(),
            });
        }
        if !self
            .active_leases
            .values()
            .any(|lease| lease.is_active_at(self.completed_turn_sequence))
        {
            self.cache_epoch.needs_active_baseline = false;
        }
        self.mark_changed(updated_at)?;
        Ok(VmmLeaseTransitionReport {
            revoked_lease_count: 1,
            changed: true,
            ..VmmLeaseTransitionReport::default()
        })
    }

    /// Revokes every active session lease when VMM is explicitly disabled.
    /// 在 VMM 被显式禁用时撤销当前会话的全部活动租约。
    ///
    /// # Parameters
    /// 参数
    /// `updated_at` is the exact disable transition timestamp.
    /// `updated_at` 是本次禁用转换的精确时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Transition counters with one body-free pending revocation per removed memory key.
    /// 返回转换计数，并为每个已删除记忆键保留一条无正文待失效事件。
    pub fn disable(&mut self, updated_at: &str) -> Result<VmmLeaseTransitionReport, String> {
        if self.active_leases.is_empty() {
            return Ok(VmmLeaseTransitionReport::default());
        }
        validate_mutation_timestamp(updated_at)?;
        // next_revision rejects revision exhaustion before active lease bodies are drained.
        // next_revision 在排空活动租约正文前拒绝修订号耗尽。
        self.next_revision()?;
        // Draining leases guarantees the active panel is empty immediately after disable.
        // 排空租约可保证禁用后活动记忆面板立即为空。
        let removed_leases = std::mem::take(&mut self.active_leases);
        // Existing pending events are keyed by memory identity and must not be duplicated.
        // 既有待处理事件按记忆身份去重，不得重复追加。
        let pending_memory_keys = self
            .pending_prompt_events
            .iter()
            .map(|event| event.memory_key.as_str())
            .collect::<BTreeSet<_>>();
        // Newly revoked items retain only identity and semantic hash, never the memory body.
        // 新撤销条目仅保留身份与语义哈希，绝不保留记忆正文。
        let new_events = removed_leases
            .values()
            .filter(|lease| !pending_memory_keys.contains(lease.memory_key.as_str()))
            .map(|lease| VmmPendingPromptEvent {
                memory_key: lease.memory_key.clone(),
                semantic_content_hash: lease.semantic_content_hash.clone(),
                expired_at: updated_at.to_string(),
            })
            .collect::<Vec<_>>();
        self.pending_prompt_events.extend(new_events);
        // Draining every lease also resolves any previously pending active-baseline request.
        // 排空全部租约也会完成任何先前待处理的活动基线请求。
        self.cache_epoch.needs_active_baseline = false;
        self.mark_changed(updated_at)?;
        Ok(VmmLeaseTransitionReport {
            revoked_lease_count: removed_leases.len(),
            changed: true,
            ..VmmLeaseTransitionReport::default()
        })
    }

    /// Starts a new prompt-cache epoch after canonical context compaction.
    /// 在规范上下文压缩后开始新的提示词缓存纪元。
    ///
    /// # Parameters
    /// 参数
    /// `compaction_id` is the exact completed context-compaction identifier.
    /// `compaction_id` 是精确的已完成上下文压缩标识。
    /// `baseline_target_message_id` is the synthetic continuation user message when available.
    /// `baseline_target_message_id` 是存在时用于承载基线的合成续接用户消息。
    /// `updated_at` is the epoch transition timestamp.
    /// `updated_at` 是纪元转换时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Transition counters after old historical anchors and notices are discarded.
    /// 返回丢弃旧历史锚点与通知后的转换计数。
    pub fn begin_cache_epoch(
        &mut self,
        compaction_id: &str,
        baseline_target_message_id: Option<&str>,
        updated_at: &str,
    ) -> Result<VmmLeaseTransitionReport, String> {
        self.reset_cache_epoch(
            compaction_id,
            Some(compaction_id),
            baseline_target_message_id,
            updated_at,
        )
    }

    /// Synchronizes the complete authoritative host lineage before binding any current user memory.
    /// 在绑定任何当前用户记忆之前，同步完整权威宿主沿革。
    /// Takes the explicit generation/optional-compaction identity and timestamp; returns the atomic transition report.
    /// 接收明确的代次/可选压缩身份及时间戳，返回原子转换报告。
    pub fn synchronize_context_epoch(
        &mut self,
        context: &crate::VmmContextEpoch,
        updated_at: &str,
    ) -> Result<VmmLeaseTransitionReport, String> {
        self.reset_cache_epoch(
            &context.cache_epoch_id(),
            context.compaction_id(),
            None,
            updated_at,
        )
    }

    /// Replaces only cache-epoch state using explicit identities, preserving receipts and logical turn counters.
    /// 使用明确身份仅替换缓存纪元状态，保留回执及逻辑回合计数。
    /// Takes the opaque epoch, optional compaction, optional actual user and timestamp; capacity errors restore the old state.
    /// 接收不透明纪元、可选压缩、可选真实用户及时间戳；容量错误恢复原状态。
    fn reset_cache_epoch(
        &mut self,
        epoch_id: &str,
        compaction_id: Option<&str>,
        baseline_target_message_id: Option<&str>,
        updated_at: &str,
    ) -> Result<VmmLeaseTransitionReport, String> {
        if self.is_pristine() {
            return Ok(VmmLeaseTransitionReport::default());
        }
        if epoch_id.trim().is_empty() || compaction_id.is_some_and(|id| id.trim().is_empty()) {
            return Err("VMM cache epoch requires explicit non-empty identities.".to_owned());
        }
        if baseline_target_message_id.is_some_and(|id| id.trim().is_empty()) {
            return Err("VMM cache epoch received an empty baseline user-message id.".to_owned());
        }
        if self.cache_epoch.epoch_id == epoch_id
            && self.cache_epoch.covered_compaction_id.as_deref() == compaction_id
        {
            return Ok(VmmLeaseTransitionReport::default());
        }
        validate_mutation_timestamp(updated_at)?;
        // Preserve atomicity if the new live baseline cannot fit the existing capacity contract.
        // 新活动基线不能满足既有容量契约时，保持原子性。
        let previous_state = self.clone();
        self.next_revision()?;
        // Expired residents are reclaimed only at the proven context boundary.
        // 仅在已证明的上下文边界回收过期驻留项。
        let expired_keys = self
            .active_leases
            .iter()
            .filter(|(_, lease)| !lease.is_active_at(self.completed_turn_sequence))
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in &expired_keys {
            self.active_leases.remove(key);
        }
        self.cache_epoch = VmmCacheEpoch {
            epoch_id: epoch_id.to_owned(),
            covered_compaction_id: compaction_id.map(str::to_owned),
            anchors: Vec::new(),
            anchor_revision: 0,
            needs_active_baseline: self.stable_anchor_rendering_enabled
                && !self.active_leases.is_empty(),
        };
        self.pending_prompt_events.clear();
        for lease in self.active_leases.values_mut() {
            lease.active_anchor_id = None;
        }
        // The optional immediate target is used only by callers already holding a real canonical user ID.
        // 仅已持有真实规范用户 ID 的调用方使用可选即时目标。
        let mut report = VmmLeaseTransitionReport {
            expired_lease_count: expired_keys.len(),
            changed: true,
            ..Default::default()
        };
        if self.stable_anchor_rendering_enabled
            && let Some(target) = baseline_target_message_id
            && self.bind_active_baseline_if_needed(target, updated_at)
        {
            report.appended_anchor_count = 1;
        }
        self.mark_changed(updated_at)?;
        if let Err(error) = self.validate_capacity_limits() {
            *self = previous_state;
            return Err(error);
        }
        Ok(report)
    }

    /// Synchronizes the stable-anchor rollout mode through an explicit cache-epoch reset.
    /// 通过显式缓存纪元重置同步稳定锚点灰度模式。
    ///
    /// # Parameters
    /// 参数
    /// `enabled` is the exact internal stable-anchor feature decision.
    /// `enabled` 是精确的内部稳定锚点功能决策。
    /// `updated_at` is the rollout transition timestamp.
    /// `updated_at` 是灰度转换时间戳。
    ///
    /// # Returns
    /// 返回值
    /// A changed report after clearing old-mode anchors, or a strict no-op when already aligned.
    /// 清理旧模式锚点后返回变更报告；模式已对齐时严格无副作用。
    pub fn synchronize_stable_anchor_rendering(
        &mut self,
        enabled: bool,
        updated_at: &str,
    ) -> Result<VmmLeaseTransitionReport, String> {
        if self.is_pristine() {
            self.stable_anchor_rendering_enabled = enabled;
            return Ok(VmmLeaseTransitionReport::default());
        }
        if self.stable_anchor_rendering_enabled == enabled {
            return Ok(VmmLeaseTransitionReport::default());
        }
        validate_mutation_timestamp(updated_at)?;
        // previous_state guarantees a rejected rollout transition cannot partially replace the epoch.
        // previous_state 保证被拒绝的灰度转换不会部分替换纪元。
        let previous_state = self.clone();
        let next_revision = self.next_revision()?;
        self.stable_anchor_rendering_enabled = enabled;
        self.cache_epoch = VmmCacheEpoch {
            epoch_id: format!(
                "stable-anchor-{}-{next_revision}",
                if enabled { "enabled" } else { "disabled" }
            ),
            covered_compaction_id: None,
            anchors: Vec::new(),
            anchor_revision: 0,
            needs_active_baseline: enabled
                && self
                    .active_leases
                    .values()
                    .any(|lease| lease.is_active_at(self.completed_turn_sequence)),
        };
        self.pending_prompt_events.clear();
        for lease in self.active_leases.values_mut() {
            lease.active_anchor_id = None;
        }
        self.mark_changed(updated_at)?;
        if let Err(error) = self.validate_capacity_limits() {
            *self = previous_state;
            return Err(error);
        }
        Ok(VmmLeaseTransitionReport {
            changed: true,
            ..VmmLeaseTransitionReport::default()
        })
    }

    /// Resets every lease and anchor after fork, edit, rollback, or history rewrite.
    /// 在分叉、编辑、回滚或历史改写后重置全部租约与锚点。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact owner of the rewritten history.
    /// `session_id` 是改写后历史的精确所有者。
    /// `updated_at` is the reset timestamp.
    /// `updated_at` 是重置时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Success after installing an empty state with a strictly newer revision.
    /// 安装具有严格更新修订号的空状态后返回成功。
    pub fn reset_for_history_rewrite(
        &mut self,
        session_id: &str,
        updated_at: &str,
    ) -> Result<(), String> {
        let next_revision = self.next_revision()?;
        *self = Self::new(session_id, updated_at)?;
        self.revision = next_revision;
        Ok(())
    }

    /// Builds one active-memory panel card containing leases eligible for the next compaction baseline.
    /// 构建一张活动记忆面板卡片，其中包含有资格进入下一次压缩基线的租约。
    ///
    /// # Returns
    /// 返回值
    /// A VMM card when at least one active lease remains.
    /// 至少仍有一条活动租约时返回 VMM 卡片。
    pub fn active_panel_card(&self) -> Option<InjectionPanelCard> {
        let items = self
            .active_leases
            .values()
            .filter(|lease| lease.is_active_at(self.completed_turn_sequence))
            .map(|lease| {
                let remaining_turns = lease
                    .expires_after_turn_sequence
                    .saturating_sub(self.completed_turn_sequence);
                vmm_panel_item(lease, format!("剩余 {remaining_turns} 回合"))
            })
            .collect::<Vec<_>>();
        if items.is_empty() {
            return None;
        }
        Some(InjectionPanelCard {
            tag: String::from("VMM_MEMORY"),
            description: String::from("当前会话仍在 TTL 有效期内的 VMM 召回记忆。"),
            items,
        })
    }

    /// Builds one bottom-panel card for expired leases retained until successful compaction.
    /// 为保留到压缩成功时清理的过期租约构建一张底部面板卡片。
    ///
    /// # Returns
    /// 返回值
    /// An expired VMM card when at least one cache-epoch-resident lease has expired.
    /// 至少存在一条已过期的缓存纪元驻留租约时返回过期 VMM 卡片。
    pub fn expired_panel_card(&self) -> Option<InjectionPanelCard> {
        let items = self
            .active_leases
            .values()
            .filter(|lease| !lease.is_active_at(self.completed_turn_sequence))
            .map(|lease| vmm_panel_item(lease, String::from("已过期")))
            .collect::<Vec<_>>();
        if items.is_empty() {
            return None;
        }
        Some(InjectionPanelCard {
            tag: String::from("VMM_MEMORY_EXPIRED"),
            description: String::from("已过期的 VMM 召回记忆，等待上下文压缩后清理。"),
            items,
        })
    }

    /// Establishes exact session ownership once and rejects cross-session reuse.
    /// 仅建立一次精确会话归属，并拒绝跨会话复用。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact expected owner.
    /// `session_id` 是精确的期望所有者。
    /// `updated_at` is used when pristine state first acquires ownership.
    /// `updated_at` 用于纯净状态首次获得归属时记录时间。
    ///
    /// # Returns
    /// 返回值
    /// Success for matching ownership, otherwise a strict mismatch error.
    /// 归属匹配时成功，否则返回严格的不匹配错误。
    fn ensure_owner(&mut self, session_id: &str, updated_at: &str) -> Result<(), String> {
        if session_id.trim().is_empty() {
            return Err(String::from("VMM session owner must not be empty."));
        }
        if self.session_id.is_empty() && self.is_pristine() {
            self.session_id = session_id.to_string();
            self.updated_at = updated_at.to_string();
            return Ok(());
        }
        if self.session_id != session_id {
            return Err(format!(
                "VMM session memory belongs to `{}` and cannot be reused by `{session_id}`.",
                self.session_id
            ));
        }
        Ok(())
    }

    /// Appends pending expiry notices to one exact new user message and clears the queue.
    /// 将待处理到期通知追加到一条精确新用户消息并清空队列。
    ///
    /// # Parameters
    /// 参数
    /// `target_message_id` is the exact canonical user message receiving the notice.
    /// `target_message_id` 是接收到期通知的精确规范用户消息。
    /// `updated_at` is the anchor creation timestamp.
    /// `updated_at` 是锚点创建时间戳。
    ///
    /// # Returns
    /// 返回值
    /// `true` when one revocation anchor was appended.
    /// 追加了一条失效锚点时返回 `true`。
    fn bind_pending_prompt_events(&mut self, target_message_id: &str, updated_at: &str) -> bool {
        if self.pending_prompt_events.is_empty() {
            return false;
        }
        if !self.stable_anchor_rendering_enabled {
            // Disabled rendering makes historical VMM prefixes absent, so pending revocations have no target.
            // 禁用渲染后历史 VMM 前缀不存在，因此待处理失效声明不再具有目标。
            self.pending_prompt_events.clear();
            return true;
        }
        let memory_keys = self
            .pending_prompt_events
            .iter()
            .map(|event| event.memory_key.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let prefix = format!(
            "<VMM_MEMORY_CHANGES>\nThe following previously recalled memories have expired and must no longer influence the answer: {}\n</VMM_MEMORY_CHANGES>",
            memory_keys.join(", ")
        );
        self.append_anchor(
            target_message_id,
            VmmPromptAnchorKind::Revocation,
            prefix,
            memory_keys,
            updated_at,
        );
        self.pending_prompt_events.clear();
        true
    }

    /// Renders live memory during the anchorless interval after compaction, without mutating canonical history.
    /// 在压缩后尚无锚点的间隔渲染活动记忆，不修改规范历史。
    /// Returns no block outside the pending baseline state or when no unexpired active lease remains.
    /// 不处于等待基线状态或没有未过期活动租约时，不返回文本块。
    pub fn pending_active_baseline_prefix(&self) -> Option<String> {
        if !self.stable_anchor_rendering_enabled || !self.cache_epoch.needs_active_baseline {
            return None;
        }
        // Only still-active leases cross the boundary; historical anchor bodies are never used as a fallback.
        // 只有仍活动的租约跨越边界；历史锚点正文绝不充当备用内容。
        let items = self
            .active_leases
            .values()
            .filter(|lease| lease.is_active_at(self.completed_turn_sequence))
            .map(|lease| lease.item.clone())
            .collect::<Vec<_>>();
        if items.is_empty() {
            return None;
        }
        Some(render_grouped_memory_prefix(
            "VMM_MEMORY_BASELINE",
            "Active VMM memories retained across context compaction.",
            &items,
        ))
    }

    /// Re-anchors every active lease once after compaction when a target message exists.
    /// 压缩后在目标消息存在时一次性重新锚定全部活动租约。
    /// Takes the exact user-message ID and timestamp; returns whether a baseline anchor was appended.
    /// 接收准确用户消息 ID 及时间戳，返回是否追加了基线锚点。
    fn bind_active_baseline_if_needed(
        &mut self,
        target_message_id: &str,
        updated_at: &str,
    ) -> bool {
        if !self.stable_anchor_rendering_enabled {
            self.cache_epoch.needs_active_baseline = false;
            return false;
        }
        if !self.cache_epoch.needs_active_baseline {
            return false;
        }
        self.cache_epoch.needs_active_baseline = false;
        // active_memory_keys exclude expired residents so no stale ABS content crosses the compaction epoch.
        // active_memory_keys 排除过期驻留项，确保陈旧 ABS 内容不会跨越压缩纪元。
        let active_memory_keys = self
            .active_leases
            .iter()
            .filter(|(_, lease)| lease.is_active_at(self.completed_turn_sequence))
            .map(|(memory_key, _)| memory_key.clone())
            .collect::<Vec<_>>();
        if active_memory_keys.is_empty() {
            return false;
        }
        let items = active_memory_keys
            .iter()
            .filter_map(|memory_key| {
                self.active_leases
                    .get(memory_key)
                    .map(|lease| lease.item.clone())
            })
            .collect::<Vec<_>>();
        let prefix = render_grouped_memory_prefix(
            "VMM_MEMORY_BASELINE",
            "Active VMM memories retained across context compaction.",
            &items,
        );
        let anchor_id = self.append_anchor(
            target_message_id,
            VmmPromptAnchorKind::Baseline,
            prefix,
            active_memory_keys.clone(),
            updated_at,
        );
        for memory_key in active_memory_keys {
            if let Some(lease) = self.active_leases.get_mut(&memory_key) {
                lease.active_anchor_id = Some(anchor_id.clone());
            }
        }
        true
    }

    /// Appends one immutable anchor and returns its deterministic identifier.
    /// 追加一条不可变锚点并返回其确定性标识。
    ///
    /// # Parameters
    /// 参数
    /// `target_message_id` is the exact canonical user-message id.
    /// `target_message_id` 是精确的规范用户消息标识。
    /// `kind` is the semantic anchor kind.
    /// `kind` 是锚点语义类型。
    /// `prefix` is the already-rendered frozen provider prefix.
    /// `prefix` 是已渲染并冻结的 Provider 前缀。
    /// `memory_keys` are the stable keys covered by the anchor.
    /// `memory_keys` 是该锚点覆盖的稳定键。
    /// `created_at` is the anchor creation timestamp.
    /// `created_at` 是锚点创建时间戳。
    ///
    /// # Returns
    /// 返回值
    /// The deterministic anchor id based on the next state revision.
    /// 返回基于下一状态版本生成的确定性锚点标识。
    fn append_anchor(
        &mut self,
        target_message_id: &str,
        kind: VmmPromptAnchorKind,
        prefix: String,
        memory_keys: Vec<String>,
        created_at: &str,
    ) -> String {
        // next_anchor_revision derives from the bounded immutable vector and cannot silently saturate.
        // next_anchor_revision 从有界不可变向量派生，绝不会静默饱和。
        let next_anchor_revision = u64::try_from(self.cache_epoch.anchors.len())
            .expect("VMM anchor length must fit in u64")
            .checked_add(1)
            .expect("bounded VMM anchor revision must remain representable");
        // Freeze semantic hashes alongside keys so persisted anchors remain independently auditable.
        // 将语义哈希与记忆键一并冻结，使持久化锚点可被独立审计。
        let semantic_content_hashes = memory_keys
            .iter()
            .filter_map(|memory_key| {
                self.active_leases
                    .get(memory_key)
                    .map(|lease| lease.semantic_content_hash.clone())
                    .or_else(|| {
                        self.pending_prompt_events
                            .iter()
                            .find(|event| event.memory_key == *memory_key)
                            .map(|event| event.semantic_content_hash.clone())
                    })
                    .map(|hash| (memory_key.clone(), hash))
            })
            .collect::<BTreeMap<_, _>>();
        // anchor_id keeps state and epoch-local revisions as separate components, avoiding arithmetic collisions.
        // anchor_id 将状态修订与纪元内修订保持为独立分量，避免算术碰撞。
        let anchor_id = format!(
            "vmm-{}-{}-{}",
            kind.as_id_label(),
            self.revision,
            next_anchor_revision
        );
        self.cache_epoch.anchors.push(VmmPromptAnchor {
            anchor_id: anchor_id.clone(),
            epoch_id: self.cache_epoch.epoch_id.clone(),
            bound_turn_sequence: self.completed_turn_sequence,
            target_message_id: target_message_id.to_string(),
            kind,
            prefix,
            memory_keys,
            semantic_content_hashes,
            created_at: created_at.to_string(),
        });
        self.cache_epoch.anchor_revision = next_anchor_revision;
        anchor_id
    }

    /// Calculates the next strictly monotonic durable revision.
    /// 计算下一个严格单调的持久化修订号。
    ///
    /// # Returns
    /// 返回值
    /// The next positive revision, or an explicit exhaustion error without mutation.
    /// 返回下一个正修订号；若已耗尽则显式报错且不发生变更。
    fn next_revision(&self) -> Result<u64, String> {
        self.revision
            .checked_add(1)
            .map(|revision| revision.max(1))
            .ok_or_else(|| String::from("VMM durable revision exhausted."))
    }

    /// Advances revision and refreshes the durable mutation timestamp.
    /// 推进状态版本并刷新持久化变更时间戳。
    ///
    /// # Parameters
    /// 参数
    /// `updated_at` is the exact timestamp associated with the mutation.
    /// `updated_at` 是与本次变更关联的精确时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Success after a strictly monotonic revision advance, or an explicit exhaustion error.
    /// 严格单调推进修订号后返回成功；若已耗尽则显式报错。
    fn mark_changed(&mut self, updated_at: &str) -> Result<(), String> {
        self.revision = self.next_revision()?;
        self.updated_at = updated_at.to_string();
        Ok(())
    }
}

/// Builds one memory-panel item from a resident lease and an explicit TTL status label.
/// 根据驻留租约与显式 TTL 状态标签构建一条记忆面板记录。
///
/// # Parameters
/// 参数
/// `lease` is the exact resident VMM lease projected into the panel.
/// `lease` 是需要投影到面板的精确 VMM 驻留租约。
/// `ttl_label` is the activity or expiry text displayed by the WebUI.
/// `ttl_label` 是 WebUI 展示的活动或过期文本。
///
/// # Returns
/// 返回值
/// A structured panel item preserving VMM identity and lookup metadata.
/// 返回保留 VMM 身份与回查元数据的结构化面板项。
fn vmm_panel_item(lease: &VmmMemoryLease, ttl_label: String) -> InjectionPanelItem {
    InjectionPanelItem {
        content: render_vmm_context_item_panel(&lease.item),
        detail: render_vmm_context_item_detail(&lease.item),
        dedupe_key: Some(lease.memory_key.clone()),
        memory_id: (lease.item.memory_id > 0).then_some(lease.item.memory_id),
        turn_id: (lease.item.turn_id > 0).then_some(lease.item.turn_id),
        score_label: Some(format_score(lease.item.score)),
        has_dialogue: lease.item.has_dialogue,
        created_datetime: non_empty_string(&lease.item.created_datetime),
        updated_at: non_empty_string(&lease.updated_at),
        turn_label: Some(if lease.item.has_dialogue && lease.item.turn_id > 0 {
            format!("TURN_ID:{}", lease.item.turn_id)
        } else {
            String::from("NOTURN")
        }),
        ttl_label,
        source_display: Some(String::from("VMM")),
    }
}

/// Validates one durable mutation timestamp before any state field can change.
/// 在任何状态字段发生变化前验证持久化变更时间戳。
///
/// # Parameters
/// 参数
/// `updated_at` is the exact timestamp that will identify the durable mutation.
/// `updated_at` 是用于标识持久化变更的精确时间戳。
///
/// # Returns
/// 返回值
/// Success for non-empty timestamp text, otherwise an explicit invariant error.
/// 时间戳文本非空时返回成功，否则返回明确的不变量错误。
fn validate_mutation_timestamp(updated_at: &str) -> Result<(), String> {
    if updated_at.trim().is_empty() {
        return Err(String::from(
            "VMM durable mutation timestamp must not be empty.",
        ));
    }
    Ok(())
}

/// Validates one typed recall batch before it can mutate session ownership or lease state.
/// 在类型化召回批次改变会话归属或租约状态前完成校验。
///
/// # Parameters
/// 参数
/// `batch` is the authoritative structured recall candidate.
/// `batch` 是权威的结构化召回候选批次。
///
/// # Returns
/// 返回值
/// Success only when the item count and every rendered body remain bounded.
/// 仅当条目数量与每段渲染正文均保持有界时返回成功。
fn validate_recall_batch_capacity(batch: &VmmRecallBatch) -> Result<(), String> {
    if batch.items.len() > VMM_RECALL_BATCH_MAX_ITEMS {
        return Err(format!(
            "VMM recall batch contains {} items and exceeds the limit of {VMM_RECALL_BATCH_MAX_ITEMS}.",
            batch.items.len()
        ));
    }
    // memory_keys rejects ambiguous duplicate identities before any lease or anchor mutation occurs.
    // memory_keys 在任何租约或锚点修改前拒绝含糊的重复身份。
    let mut memory_keys = BTreeSet::new();
    for item in &batch.items {
        // normalized_item matches the exact representation used by stable identity and semantic hashing.
        // normalized_item 与稳定身份和语义哈希使用的精确表示保持一致。
        let normalized_item = normalized_context_item(item);
        // body_bytes measures the Provider-visible normalized memory row.
        // body_bytes 测量 Provider 可见的规范化记忆行。
        let body_bytes = render_vmm_context_item_model(&normalized_item).len();
        if body_bytes == 0 {
            continue;
        }
        if body_bytes > VMM_MEMORY_BODY_MAX_BYTES {
            return Err(format!(
                "VMM recall item contains {body_bytes} bytes and exceeds the per-memory limit of {VMM_MEMORY_BODY_MAX_BYTES}."
            ));
        }
        // memory_key is the authoritative identity that must occur at most once per recall response.
        // memory_key 是每次召回响应中最多出现一次的权威身份。
        let memory_key = stable_memory_key(&normalized_item);
        if !memory_keys.insert(memory_key.clone()) {
            return Err(format!(
                "VMM recall batch contains duplicate memory key `{memory_key}`."
            ));
        }
    }
    Ok(())
}

/// Normalizes provider-visible text while retaining authoritative structured identity fields.
/// 规范化 Provider 可见文本，同时保留权威结构化身份字段。
///
/// # Parameters
/// 参数
/// `item` is the authoritative item returned by VMM.
/// `item` 是 VMM 返回的权威条目。
///
/// # Returns
/// 返回值
/// A cloned item whose free-form text fields have stable single-line representation.
/// 返回自由文本字段具有稳定单行表示的克隆条目。
fn normalized_context_item(item: &ContextItem) -> ContextItem {
    ContextItem {
        text: collapse_text(&item.text),
        score: if item.score.is_finite() {
            item.score
        } else {
            0.0
        },
        turn_id: item.turn_id,
        has_dialogue: item.has_dialogue,
        created_datetime: collapse_text(&item.created_datetime),
        memory_id: item.memory_id,
    }
}

/// Builds the stable semantic identity for one structured VMM context item.
/// 为一条结构化 VMM 上下文项构建稳定语义身份。
///
/// # Parameters
/// 参数
/// `item` is the normalized VMM context item.
/// `item` 是规范化后的 VMM 上下文项。
///
/// # Returns
/// 返回值
/// Memory id, dialogue turn id, or deterministic content-hash identity in that order.
/// 依次返回记忆标识、对话回合标识或确定性内容哈希身份。
fn stable_memory_key(item: &ContextItem) -> String {
    if item.memory_id > 0 {
        return format!("memory:{}", item.memory_id);
    }
    if item.has_dialogue && item.turn_id > 0 {
        return format!("turn:{}", item.turn_id);
    }
    format!(
        "content:{}",
        hex_sha256(collapse_text(&item.text).as_bytes())
    )
}

/// Hashes semantic VMM content while excluding volatile score and display timestamp fields.
/// 对 VMM 语义内容进行哈希，同时排除易变评分与展示时间字段。
///
/// # Parameters
/// 参数
/// `memory_key` is the stable semantic identity.
/// `memory_key` 是稳定语义身份。
/// `item` is the normalized structured context item.
/// `item` 是规范化后的结构化上下文项。
///
/// # Returns
/// 返回值
/// A lowercase SHA-256 digest used to distinguish refresh from replacement.
/// 返回用于区分刷新与替换的小写 SHA-256 摘要。
fn semantic_content_hash(memory_key: &str, item: &ContextItem) -> String {
    let payload = format!(
        "v1\0{memory_key}\0{}\0{}\0{}\0{}",
        collapse_text(&item.text),
        item.memory_id,
        item.turn_id,
        item.has_dialogue
    );
    hex_sha256(payload.as_bytes())
}

/// Renders one recall batch into a frozen provider-visible grouping block.
/// 将一批召回结果渲染为冻结的 Provider 可见分组块。
///
/// # Parameters
/// 参数
/// `batch` supplies the configured tag and description.
/// `batch` 提供已配置的标签与说明。
/// `items` are only newly introduced or semantically replaced items.
/// `items` 仅包含新引入或语义被替换的条目。
///
/// # Returns
/// 返回值
/// One immutable XML-like provider prefix.
/// 返回一段不可变类 XML Provider 前缀。
fn render_recall_anchor_prefix(batch: &VmmRecallBatch, items: &[ContextItem]) -> String {
    render_grouped_memory_prefix(&batch.injection_tag, &batch.injection_description, items)
}

/// Renders structured memory items under one exact XML-like tag and description.
/// 在一个精确类 XML 标签与说明下渲染结构化记忆条目。
///
/// # Parameters
/// 参数
/// `tag` is the provider-visible outer grouping tag.
/// `tag` 是 Provider 可见外层分组标签。
/// `description` is rendered once before memory rows.
/// `description` 在记忆行之前仅渲染一次。
/// `items` are rendered in the caller-provided deterministic order.
/// `items` 按调用方提供的确定性顺序渲染。
///
/// # Returns
/// 返回值
/// One frozen provider-visible block.
/// 返回一段冻结的 Provider 可见文本块。
fn render_grouped_memory_prefix(tag: &str, description: &str, items: &[ContextItem]) -> String {
    let rendered_items = items
        .iter()
        .map(render_vmm_context_item_model)
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let description = description.trim();
    if description.is_empty() {
        format!("<{tag}>\n{rendered_items}\n</{tag}>")
    } else {
        format!("<{tag}>\n{description}\n{rendered_items}\n</{tag}>")
    }
}

/// Collapses free-form VMM text into the established stable single-line representation.
/// 将自由格式 VMM 文本压缩为既有稳定单行表示。
///
/// # Parameters
/// 参数
/// `value` is the free-form source text.
/// `value` 是自由格式源文本。
///
/// # Returns
/// 返回值
/// Trimmed non-empty lines joined by one ASCII space.
/// 返回由单个 ASCII 空格连接的已裁剪非空行。
fn collapse_text(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Computes the permanent ledger hash for one controller command identity.
/// 为一个控制器命令身份计算永久账本哈希。
///
/// # Parameters
/// 参数
/// `command_id` is the validated session-scoped command identity.
/// `command_id` 是已校验的会话级命令身份。
///
/// # Returns
/// 返回值
/// A domain-separated lowercase SHA-256 digest.
/// 返回经过域隔离的小写 SHA-256 摘要。
fn controller_command_hash(command_id: &str) -> String {
    let payload = format!("vmm-controller-command-v1\0{command_id}");
    hex_sha256(payload.as_bytes())
}

/// Computes a lowercase SHA-256 digest for deterministic identities.
/// 为确定性身份计算小写 SHA-256 摘要。
///
/// # Parameters
/// 参数
/// `bytes` is the exact canonical byte sequence to hash.
/// `bytes` 是需要哈希的精确规范字节序列。
///
/// # Returns
/// 返回值
/// A 64-character lowercase hexadecimal digest.
/// 返回 64 字符小写十六进制摘要。
fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Formats a finite score for UI display with the provider-context precision.
/// 使用 Provider 上下文精度格式化有限评分以供界面展示。
///
/// # Parameters
/// 参数
/// `score` is the latest VMM relevance score.
/// `score` 是最新 VMM 相关性评分。
///
/// # Returns
/// 返回值
/// A three-decimal finite score label.
/// 返回三位小数的有限评分标签。
fn format_score(score: f64) -> String {
    if score.is_finite() {
        format!("{score:.3}")
    } else {
        String::from("0.000")
    }
}

/// Converts one non-empty borrowed string into an owned optional value.
/// 将一个非空借用字符串转换为拥有所有权的可选值。
///
/// # Parameters
/// 参数
/// `value` is the string inspected without semantic fallback.
/// `value` 是在不做语义回退的情况下检查的字符串。
///
/// # Returns
/// 返回值
/// A trimmed owned value when non-empty.
/// 非空时返回裁剪后的拥有所有权值。
fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Estimates mixed-language token pressure conservatively from a UTF-8 byte count.
/// 根据 UTF-8 字节数保守估算混合语言 Token 压力。
///
/// # Parameters
/// 参数
/// `byte_count` is the aggregate Provider-visible UTF-8 byte count.
/// `byte_count` 是 Provider 可见 UTF-8 聚合字节数。
///
/// # Returns
/// 返回值
/// A ceiling estimate using three bytes per token to avoid undercounting common CJK text.
/// 返回按每 Token 三字节计算的向上取整估算，以避免低估常见中日韩文本。
fn estimate_vmm_text_tokens_from_bytes(byte_count: usize) -> usize {
    byte_count.saturating_add(2) / 3
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies source switches remove all memory bodies without erasing command receipts or terminal lineage.
    /// 验证来源切换移除全部记忆正文，而不清除命令回执或终态沿革。
    /// Uses exact native owners and reused numeric IDs; returns after round-trip and same-source assertions.
    /// 使用准确原生归属及复用数字 ID；完成切回和同来源断言后返回。
    #[test]
    fn source_switch_preserves_receipts_but_never_reuses_old_bodies() {
        // The linked-scope change deliberately keeps the same write owner.
        // 连带范围变化刻意保持相同写入归属。
        let first = crate::VmmMemoryReadSource::new(
            crate::VmmMemorySource::native(7).expect("owner"),
            vec![7],
        )
        .expect("source");
        // A distinct visibility set must still invalidate cached context.
        // 不同可见范围仍必须使缓存上下文失效。
        let linked =
            crate::VmmMemoryReadSource::new(first.owner.clone(), vec![7, 8]).expect("linked");
        // Create a real lease, terminal receipt and active next turn through public state operations.
        // 通过公共状态操作创建真实租约、终态回执及下一活动回合。
        let mut state = VmmSessionMemoryState::new("session-a", "1").expect("state");
        state
            .select_read_source("session-a", Some(&first), "1")
            .expect("select");
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(3, vec![memory_item(11, "old-source-body", 1.0)]),
                "2",
            )
            .expect("recall");
        state.register_active_turn("turn-1", "2").expect("active");
        state.consume_turn("turn-1", "3").expect("terminal");
        state
            .register_active_turn("turn-2", "4")
            .expect("next active");
        state
            .record_committed_controller_command("receipt-1", state.revision, "4")
            .expect("receipt");
        // Disable leaves immutable old anchors and body-free events, both of which source selection must discard.
        // 禁用会留下旧不可变锚点及无正文事件，来源选择必须同时丢弃二者。
        state.disable("5").expect("disable");
        assert!(!state.cache_epoch.anchors.is_empty());
        assert!(!state.pending_prompt_events.is_empty());
        // Same-source selection must be an exact no-op, not a cache refresh or history rewrite.
        // 同来源选择必须准确无操作，而非缓存刷新或历史重写。
        let before = state.clone();
        assert!(
            !state
                .select_read_source("session-a", Some(&first), "6")
                .expect("same source")
        );
        assert_eq!(state, before);
        assert!(
            state
                .select_read_source("session-a", Some(&linked), "7")
                .expect("scope change")
        );
        assert!(state.active_leases.is_empty());
        assert!(state.cache_epoch.anchors.is_empty());
        assert!(state.pending_prompt_events.is_empty());
        assert_ne!(state.cache_epoch.epoch_id, before.cache_epoch.epoch_id);
        assert_eq!(
            state.committed_command_hashes,
            before.committed_command_hashes
        );
        assert_eq!(
            state.completed_turn_sequence,
            before.completed_turn_sequence
        );
        assert_eq!(state.last_consumed_turn_id, before.last_consumed_turn_id);
        assert_eq!(state.active_turn_id, before.active_turn_id);
        assert!(state.has_committed_controller_command("receipt-1"));
        state
            .apply_recall_batch(
                "session-a",
                "user-2",
                &recall_batch(3, vec![memory_item(11, "new-source-body", 1.0)]),
                "8",
            )
            .expect("same ID new source");
        assert!(
            state
                .select_read_source("session-a", Some(&first), "9")
                .expect("switch back")
        );
        assert!(state.active_leases.is_empty());
        assert!(state.cache_epoch.anchors.is_empty());
        state.validate("session-a").expect("valid switched state");
        assert_eq!(
            serde_json::from_str::<VmmSessionMemoryState>(
                &serde_json::to_string(&state).expect("encode")
            )
            .expect("decode"),
            state
        );
    }

    /// Verifies missing or inactive sources cannot retain unowned content and invalid selections are atomic.
    /// 验证缺失或停用来源不能保留无归属内容，且无效来源选择保持原子性。
    /// Returns after validating unknown-cache cleanup, exact ownership, invalid scope and revision exhaustion.
    /// 验证未知缓存清理、准确归属、无效范围及版本耗尽后返回。
    #[test]
    fn source_selection_clears_unowned_cache_and_rejects_invalid_changes_atomically() {
        // An unselected cache represents content with no source proof and must never be relabeled.
        // 未选择来源的缓存表示没有来源证明的内容，绝不能重新贴上来源标签。
        let mut state = VmmSessionMemoryState::new("session-a", "1").expect("state");
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(3, vec![memory_item(11, "unowned-body", 1.0)]),
                "2",
            )
            .expect("recall");
        assert!(
            state
                .select_read_source("session-a", None, "3")
                .expect("clear unowned")
        );
        assert!(state.active_leases.is_empty());
        assert!(state.cache_epoch.anchors.is_empty());
        // An inactive empty cache stays unchanged on repeated publication.
        // 停用的空缓存在重复发布时保持不变。
        let before = state.clone();
        assert!(
            !state
                .select_read_source("session-a", None, "4")
                .expect("empty no-op")
        );
        assert_eq!(state, before);
        // Invalid inputs must leave every byte of the retained state unchanged.
        // 无效输入必须保持保留状态的每个字节不变。
        let invalid = crate::VmmMemoryReadSource {
            owner: crate::VmmMemorySource::native(7).expect("owner"),
            visible_project_ids: vec![8],
        };
        assert!(
            state
                .select_read_source("session-a", Some(&invalid), "5")
                .is_err()
        );
        assert!(
            state
                .select_read_source("other-session", None, "5")
                .is_err()
        );
        assert!(state.select_read_source("session-a", None, "").is_err());
        assert_eq!(state, before);
        // Exhaustion is tested against a valid changed source so the no-op branch cannot conceal it.
        // 使用有效变化来源测试耗尽，避免无操作分支掩盖错误。
        let valid = crate::VmmMemoryReadSource::new(invalid.owner, vec![7]).expect("source");
        state.revision = u64::MAX;
        // Keep an exact snapshot after deliberate exhaustion.
        // 刻意耗尽之后保留准确快照。
        let exhausted = state.clone();
        assert!(
            state
                .select_read_source("session-a", Some(&valid), "6")
                .is_err()
        );
        assert_eq!(state, exhausted);
    }

    /// Builds one deterministic memory-backed VMM item for state-machine tests.
    /// 为状态机测试构建一条确定性的长期记忆 VMM 条目。
    fn memory_item(memory_id: u64, text: &str, score: f64) -> ContextItem {
        ContextItem {
            text: text.to_string(),
            score,
            turn_id: 17,
            has_dialogue: true,
            created_datetime: String::from("2026-07-20"),
            memory_id,
        }
    }

    /// Builds one deterministic typed recall batch for state-machine tests.
    /// 为状态机测试构建一批确定性的类型化召回结果。
    fn recall_batch(ttl_turns: u32, items: Vec<ContextItem>) -> VmmRecallBatch {
        VmmRecallBatch {
            ttl_turns,
            injection_tag: String::from("VMM_MEMORY"),
            injection_description: String::from("Relevant memories."),
            items,
        }
    }

    /// Verifies an explicitly owned constructor produces an immediately valid durable document.
    /// 验证显式归属构造器会生成可立即通过校验的持久化文档。
    #[test]
    fn owned_constructor_is_immediately_valid() {
        let state =
            VmmSessionMemoryState::new("session-a", "1").expect("valid owned VMM session state");

        assert_eq!(state.revision, 1);
        state
            .validate("session-a")
            .expect("owned constructor should produce valid durable state");
    }

    /// Verifies permanent command hashes preserve idempotency beyond the removed rolling window.
    /// 验证永久命令哈希会在已删除的滚动窗口范围之外继续保持幂等性。
    #[test]
    fn controller_command_ledger_never_evicts_older_receipts() {
        // state owns the durable ledger exercised by this test.
        // state 持有本测试所检查的持久化账本。
        let mut state =
            VmmSessionMemoryState::new("session-a", "1").expect("valid owned VMM session state");
        // command_count deliberately exceeds the former 256-entry eviction boundary.
        // command_count 故意超过原先 256 条的淘汰边界。
        let command_count = 1_024_u64;

        for command_index in 0..command_count {
            let revision_before_command = state.revision;
            state
                .record_committed_controller_command(
                    &format!("provider_boundary:turn-{command_index}"),
                    revision_before_command,
                    &format!("{}", command_index + 2),
                )
                .expect("record permanent controller command receipt");
        }

        assert_eq!(state.committed_command_hashes.len(), command_count as usize);
        assert!(state.has_committed_controller_command("provider_boundary:turn-0"));
        assert!(state.has_committed_controller_command("provider_boundary:turn-1023"));
        assert!(
            state
                .committed_command_hashes
                .iter()
                .all(|command_hash| command_hash.len() == 64)
        );
        state
            .validate("session-a")
            .expect("permanent command ledger should remain valid");
    }

    /// Verifies the owned constructor rejects empty identity metadata at the creation boundary.
    /// 验证归属构造器会在创建边界拒绝空身份元数据。
    #[test]
    fn owned_constructor_rejects_empty_owner_or_timestamp() {
        let empty_owner = VmmSessionMemoryState::new(" ", "1")
            .expect_err("empty session ownership must be rejected");
        let empty_timestamp = VmmSessionMemoryState::new("session-a", " ")
            .expect_err("empty initialization timestamp must be rejected");

        assert!(empty_owner.contains("owner must not be empty"));
        assert!(empty_timestamp.contains("timestamp must not be empty"));
    }

    /// Verifies an empty mutation timestamp cannot partially change an already-owned VMM state.
    /// 验证空变更时间戳无法部分修改已经归属的 VMM 状态。
    #[test]
    fn empty_mutation_timestamp_rejects_recall_atomically() {
        let mut state =
            VmmSessionMemoryState::new("session-a", "1").expect("valid owned VMM session state");
        // state_before freezes every field that must survive the rejected recall unchanged.
        // state_before 冻结被拒绝召回后必须保持不变的全部字段。
        let state_before = state.clone();

        let error = state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(3, vec![memory_item(7, "rejected", 0.8)]),
                " ",
            )
            .expect_err("empty mutation timestamp must reject recall");

        assert!(error.contains("mutation timestamp must not be empty"));
        assert_eq!(state, state_before);
    }

    /// Verifies a zero-TTL recall is a strict no-op that does not acquire session ownership.
    /// 验证 TTL 为零的召回是严格无副作用操作，且不会取得会话归属。
    #[test]
    fn zero_ttl_recall_does_not_create_lease_anchor_or_state() {
        let mut state = VmmSessionMemoryState::default();

        let report = state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(0, vec![memory_item(7, "ignored", 0.8)]),
                "1",
            )
            .expect("zero-TTL recall should be ignored");

        assert_eq!(report, VmmLeaseTransitionReport::default());
        assert!(state.is_pristine());
        assert!(state.active_leases.is_empty());
        assert!(state.cache_epoch.anchors.is_empty());
    }

    /// Verifies an exhausted durable revision rejects recall without changing any state field.
    /// 验证耗尽的持久化修订号会拒绝召回，且不改变任何状态字段。
    #[test]
    fn exhausted_revision_rejects_recall_atomically() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(3, vec![memory_item(7, "existing", 0.8)]),
                "1",
            )
            .expect("initial recall should succeed");
        state.revision = u64::MAX;
        // state_before proves the rejected transition preserves leases, anchors, ownership, and timestamps.
        // state_before 证明被拒绝的转换会保留租约、锚点、归属和时间戳。
        let state_before = state.clone();

        let error = state
            .apply_recall_batch(
                "session-a",
                "user-2",
                &recall_batch(3, vec![memory_item(8, "new", 0.7)]),
                "2",
            )
            .expect_err("an exhausted durable revision must reject recall");

        assert!(error.contains("revision exhausted"));
        assert_eq!(state, state_before);
    }

    /// Verifies expiry arithmetic overflow is rejected before session state mutation.
    /// 验证到期序号算术溢出会在会话状态发生变更前被拒绝。
    #[test]
    fn expiry_overflow_rejects_recall_atomically() {
        let mut state =
            VmmSessionMemoryState::new("session-a", "1").expect("valid owned VMM session state");
        state.completed_turn_sequence = u64::MAX - 1;
        state.last_consumed_turn_id = Some(String::from("turn-before-limit"));
        // state_before proves overflow cannot acquire leases or append prompt anchors.
        // state_before 证明溢出不会取得租约或追加提示词锚点。
        let state_before = state.clone();

        let error = state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(2, vec![memory_item(7, "overflow", 0.8)]),
                "2",
            )
            .expect_err("expiry overflow must reject recall");

        assert!(error.contains("expiry sequence overflowed"));
        assert_eq!(state, state_before);
    }

    /// Verifies one recall batch preserves VMM item order inside its frozen anchor.
    /// 验证单个召回批次会在冻结锚点内保持 VMM 条目顺序。
    #[test]
    fn recall_batch_preserves_deterministic_item_order() {
        let mut state = VmmSessionMemoryState::default();

        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(
                    2,
                    vec![
                        memory_item(7, "first-memory", 0.8),
                        memory_item(8, "second-memory", 0.7),
                    ],
                ),
                "1",
            )
            .expect("ordered recall should succeed");

        let anchor = &state.cache_epoch.anchors[0];
        assert!(
            anchor
                .prefix
                .find("first-memory")
                .expect("first memory should be rendered")
                < anchor
                    .prefix
                    .find("second-memory")
                    .expect("second memory should be rendered")
        );
        assert_eq!(anchor.epoch_id, "initial");
        assert_eq!(anchor.bound_turn_sequence, 0);
        assert_eq!(anchor.semantic_content_hashes.len(), 2);
        state
            .validate("session-a")
            .expect("frozen anchor metadata should validate");
    }

    /// Verifies unchanged recall refreshes TTL without mutating the historical anchor.
    /// 验证未变化召回会刷新 TTL 且不会改写历史锚点。
    #[test]
    fn unchanged_recall_refreshes_lease_without_new_anchor() {
        let mut state = VmmSessionMemoryState::default();
        let batch = recall_batch(3, vec![memory_item(7, "remember this", 0.8)]);
        state
            .apply_recall_batch("session-a", "user-1", &batch, "1")
            .expect("first recall should succeed");
        state
            .register_active_turn("turn-1", "1")
            .expect("first logical turn should register");
        let frozen_anchor = state.cache_epoch.anchors[0].clone();
        state
            .consume_turn("turn-1", "2")
            .expect("terminal turn should be consumed");
        let refreshed = recall_batch(3, vec![memory_item(7, "remember this", 0.9)]);

        let report = state
            .apply_recall_batch("session-a", "user-2", &refreshed, "3")
            .expect("unchanged semantic recall should refresh");

        assert_eq!(report.refreshed_lease_count, 1);
        assert_eq!(report.appended_anchor_count, 0);
        assert_eq!(state.cache_epoch.anchors, vec![frozen_anchor]);
        assert_eq!(state.active_leases["memory:7"].item.score, 0.9);
        assert_eq!(
            state.active_leases["memory:7"].expires_after_turn_sequence,
            4
        );
    }

    /// Verifies one stable VMM identity cannot resolve to different ABS content.
    /// 验证同一个稳定 VMM 身份不能解析为不同的 ABS 内容。
    #[test]
    fn semantic_identity_mismatch_is_rejected_atomically() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(2, vec![memory_item(7, "old", 0.8)]),
                "1",
            )
            .expect("first recall should succeed");
        // state_before proves the rejected identity conflict cannot partially refresh TTL or anchors.
        // state_before 用于证明被拒绝的身份冲突不能部分刷新 TTL 或锚点。
        let state_before = state.clone();

        let error = state
            .apply_recall_batch(
                "session-a",
                "user-2",
                &recall_batch(2, vec![memory_item(7, "new", 0.7)]),
                "2",
            )
            .expect_err("one VMM identity must not replace its ABS content");

        assert!(error.contains("different ABS content"));
        assert_eq!(state, state_before);
    }

    /// Verifies TTL expiry retains one lease and a repeated recall refreshes it without a new anchor.
    /// 验证 TTL 到期会保留租约，且重复召回只刷新租约而不新增锚点。
    #[test]
    fn terminal_ttl_expiry_retains_lease_until_refresh_or_compaction() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(1, vec![memory_item(7, "short lived", 0.8)]),
                "1",
            )
            .expect("recall should succeed");
        state
            .register_active_turn("turn-1", "1")
            .expect("expiring logical turn should register");

        let report = state
            .consume_turn("turn-1", "2")
            .expect("terminal turn should expire lease");

        assert_eq!(report.expired_lease_count, 1);
        assert!(state.active_leases.contains_key("memory:7"));
        assert!(state.active_panel_card().is_none());
        assert!(state.pending_prompt_events.is_empty());
        assert_eq!(
            state.expired_panel_card().map(|card| card.items.len()),
            Some(1)
        );
        let anchor_count = state.cache_epoch.anchors.len();

        let refresh = state
            .apply_recall_batch(
                "session-a",
                "user-2",
                &recall_batch(3, vec![memory_item(7, "short lived", 0.9)]),
                "3",
            )
            .expect("expired resident recall should refresh without reinjection");

        assert_eq!(refresh.refreshed_lease_count, 1);
        assert_eq!(refresh.appended_anchor_count, 0);
        assert_eq!(state.cache_epoch.anchors.len(), anchor_count);
        assert!(state.active_panel_card().is_some());
        assert!(state.expired_panel_card().is_none());
    }

    /// Verifies failed-turn abandonment clears only active ownership and preserves TTL state.
    /// 验证失败回合放弃只清除活动所有权，并保留 TTL 状态。
    #[test]
    fn failed_turn_abandonment_preserves_terminal_sequence_and_lease_ttl() {
        // state owns one live lease and one active logical turn that will fail before Provider execution.
        // state 持有一条活动租约与一个将在 Provider 执行前失败的活动逻辑回合。
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(2, vec![memory_item(7, "keep after failure", 0.8)]),
                "1",
            )
            .expect("recall should succeed");
        state
            .register_active_turn("turn-failed", "2")
            .expect("failed logical turn should register");
        // completed_turn_sequence_before proves abandonment cannot consume one terminal TTL boundary.
        // completed_turn_sequence_before 证明放弃操作不能消费终态 TTL 边界。
        let completed_turn_sequence_before = state.completed_turn_sequence;
        // lease_expiry_before proves abandonment leaves the existing lease lifetime unchanged.
        // lease_expiry_before 证明放弃操作保持已有租约生命周期不变。
        let lease_expiry_before = state.active_leases["memory:7"].expires_after_turn_sequence;

        let report = state
            .abandon_active_turn("turn-failed", "3")
            .expect("exact failed turn should be abandoned");

        assert!(report.changed);
        assert_eq!(report.expired_lease_count, 0);
        assert!(state.active_turn_id.is_none());
        assert_eq!(
            state.completed_turn_sequence,
            completed_turn_sequence_before
        );
        assert!(state.last_consumed_turn_id.is_none());
        assert_eq!(
            state.active_leases["memory:7"].expires_after_turn_sequence,
            lease_expiry_before
        );
        // state_before_mismatch proves a different failure identity cannot clear or mutate ownership.
        // state_before_mismatch 证明不同的失败身份不能清除或修改所有权。
        state
            .register_active_turn("turn-current", "4")
            .expect("new exact turn should register");
        let state_before_mismatch = state.clone();
        assert!(
            state
                .abandon_active_turn("turn-other", "5")
                .expect_err("mismatched turn must be rejected")
                .contains("does not match")
        );
        assert_eq!(state, state_before_mismatch);
    }

    /// Verifies an expired resident survives independent JSON persistence until compaction reclaims it.
    /// 验证过期驻留项在压缩回收前可通过独立 JSON 持久化往返保存。
    #[test]
    fn expired_resident_round_trips_and_remains_valid() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(1, vec![memory_item(7, "persisted-expired", 0.8)]),
                "1",
            )
            .expect("recall should succeed");
        state
            .register_active_turn("turn-1", "1")
            .expect("logical turn should register");
        state
            .consume_turn("turn-1", "2")
            .expect("terminal turn should retain the expired resident");
        state
            .validate("session-a")
            .expect("expired resident state should validate before persistence");

        let encoded =
            serde_json::to_string_pretty(&state).expect("expired resident state should serialize");
        let restored = serde_json::from_str::<VmmSessionMemoryState>(&encoded)
            .expect("expired resident state should deserialize");

        assert_eq!(restored, state);
        restored
            .validate("session-a")
            .expect("restored expired resident state should remain valid");
        assert!(restored.active_panel_card().is_none());
        assert!(restored.expired_panel_card().is_some());
    }

    /// Verifies explicit session suppression removes an expired resident and queues one body-free revocation.
    /// 验证显式会话抑制会删除过期驻留项，并排队一条无正文撤销通知。
    #[test]
    fn explicit_lease_revocation_removes_expired_resident() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(1, vec![memory_item(7, "suppressed-body", 0.8)]),
                "1",
            )
            .expect("recall should succeed");
        state
            .register_active_turn("turn-1", "1")
            .expect("logical turn should register");
        state
            .consume_turn("turn-1", "2")
            .expect("terminal turn should retain the expired resident");

        let report = state
            .revoke_lease("memory:7", "3")
            .expect("explicit suppression should revoke the resident");

        assert_eq!(report.revoked_lease_count, 1);
        assert!(state.active_leases.is_empty());
        assert!(state.expired_panel_card().is_none());
        assert_eq!(state.pending_prompt_events.len(), 1);
        state
            .bind_pending_state_to_user("user-2", "4")
            .expect("explicit revocation should bind to the next exact user message");
        let revocation = state
            .cache_epoch
            .anchors
            .last()
            .expect("explicit revocation anchor should exist");
        assert_eq!(revocation.kind, VmmPromptAnchorKind::Revocation);
        assert!(!revocation.prefix.contains("suppressed-body"));
    }

    /// Verifies repeated terminal reports with the same id consume TTL exactly once.
    /// 验证同一标识的重复终态报告只会消费一次 TTL。
    #[test]
    fn terminal_turn_consumption_is_idempotent() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(2, vec![memory_item(7, "two turns", 0.8)]),
                "1",
            )
            .expect("recall should succeed");
        state
            .register_active_turn("turn-1", "1")
            .expect("first logical turn should register");

        state
            .consume_turn("turn-1", "2")
            .expect("first terminal report should succeed");
        let duplicate = state
            .consume_turn("turn-1", "3")
            .expect("duplicate terminal report should be ignored");

        assert!(!duplicate.changed);
        assert_eq!(state.completed_turn_sequence, 1);
        assert!(state.active_leases.contains_key("memory:7"));
    }

    /// Verifies a consumed or out-of-order terminal id cannot advance TTL after another turn became authoritative.
    /// 验证另一个回合成为权威边界后，已消费或乱序终态标识无法推进 TTL。
    #[test]
    fn terminal_turn_consumption_rejects_out_of_order_ids() {
        // state begins with one lease spanning several successful logical turns.
        // state 初始包含一条跨越多个成功逻辑回合的租约。
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(5, vec![memory_item(7, "bounded", 0.8)]),
                "1",
            )
            .expect("recall should succeed");
        state
            .register_active_turn("turn-a", "1")
            .expect("first logical turn should register");
        state
            .consume_turn("turn-a", "2")
            .expect("first logical turn should consume once");
        state
            .register_active_turn("turn-b", "3")
            .expect("second logical turn should register");
        state
            .consume_turn("turn-b", "4")
            .expect("second logical turn should consume once");
        // sequence_before proves the rejected late report cannot mutate durable TTL state.
        // sequence_before 用于证明被拒绝的迟到报告无法修改持久化 TTL 状态。
        let sequence_before = state.completed_turn_sequence;
        // revision_before proves rejection is fully atomic rather than partially recorded.
        // revision_before 用于证明拒绝过程完全原子，不会留下部分记录。
        let revision_before = state.revision;

        let error = state
            .consume_turn("turn-a", "5")
            .expect_err("an older terminal id must be rejected");

        assert!(error.contains("does not match"));
        assert_eq!(state.completed_turn_sequence, sequence_before);
        assert_eq!(state.revision, revision_before);
    }

    /// Verifies disabling VMM immediately clears active leases and queues body-free revocations.
    /// 验证禁用 VMM 会立即清空活动租约并排队无正文失效事件。
    #[test]
    fn disable_clears_active_leases_and_queues_body_free_revocations() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(3, vec![memory_item(7, "secret-memory-body", 0.8)]),
                "1",
            )
            .expect("recall should succeed");

        let report = state.disable("2").expect("disable VMM state");

        assert_eq!(report.revoked_lease_count, 1);
        assert!(state.active_leases.is_empty());
        assert!(state.active_panel_card().is_none());
        assert_eq!(state.pending_prompt_events.len(), 1);
        state
            .bind_pending_state_to_user("user-2", "3")
            .expect("revocation should bind to the next exact user message");
        let revocation = state
            .cache_epoch
            .anchors
            .last()
            .expect("revocation anchor should exist");
        assert_eq!(revocation.kind, VmmPromptAnchorKind::Revocation);
        assert!(!revocation.prefix.contains("secret-memory-body"));
        assert_eq!(revocation.semantic_content_hashes.len(), 1);
    }

    /// Verifies compaction clears historical anchors and emits only the active baseline.
    /// 验证压缩会清理历史锚点，并仅产出活动记忆基线。
    #[test]
    fn compaction_starts_new_epoch_with_active_baseline() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(3, vec![memory_item(7, "active", 0.8)]),
                "1",
            )
            .expect("recall should succeed");

        let report = state
            .begin_cache_epoch("compact-1", Some("continuation-1"), "2")
            .expect("compaction epoch should succeed");

        assert_eq!(report.appended_anchor_count, 1);
        assert_eq!(state.cache_epoch.epoch_id, "compact-1");
        assert_eq!(state.cache_epoch.anchors.len(), 1);
        assert_eq!(
            state.cache_epoch.anchors[0].kind,
            VmmPromptAnchorKind::Baseline
        );
        assert_eq!(
            state.cache_epoch.anchors[0].target_message_id,
            "continuation-1"
        );
    }

    /// Verifies compaction removes expired residents and baselines only still-active ABS content.
    /// 验证压缩会删除过期驻留项，并且只为仍活动的 ABS 内容创建基线。
    #[test]
    fn compaction_removes_expired_residents_and_baselines_only_active_items() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(1, vec![memory_item(7, "expired", 0.8)]),
                "1",
            )
            .expect("expiring recall should succeed");
        state
            .register_active_turn("turn-1", "1")
            .expect("expiring logical turn should register");
        state
            .consume_turn("turn-1", "2")
            .expect("terminal turn should retain the expired resident");
        state
            .apply_recall_batch(
                "session-a",
                "user-2",
                &recall_batch(3, vec![memory_item(8, "active", 0.9)]),
                "3",
            )
            .expect("active recall should succeed");

        let report = state
            .begin_cache_epoch("compact-mixed", Some("continuation-1"), "4")
            .expect("mixed compaction epoch should succeed");

        assert_eq!(report.expired_lease_count, 1);
        assert_eq!(report.appended_anchor_count, 1);
        assert!(!state.active_leases.contains_key("memory:7"));
        assert!(state.active_leases.contains_key("memory:8"));
        assert!(state.expired_panel_card().is_none());
        assert_eq!(state.cache_epoch.anchors.len(), 1);
        assert!(state.cache_epoch.anchors[0].prefix.contains("active"));
        assert!(!state.cache_epoch.anchors[0].prefix.contains("expired"));
    }

    /// Verifies replaying the same completed compaction id is a strict no-op.
    /// 验证重复应用同一个已完成压缩标识时严格无副作用。
    #[test]
    fn repeated_compaction_id_is_idempotent() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(3, vec![memory_item(7, "active", 0.8)]),
                "1",
            )
            .expect("recall should succeed");
        state
            .begin_cache_epoch("compact-1", Some("continuation-1"), "2")
            .expect("first compaction should succeed");
        let state_after_first_compaction = state.clone();

        let duplicate = state
            .begin_cache_epoch("compact-1", Some("continuation-1"), "3")
            .expect("duplicate compaction should be ignored");

        assert!(!duplicate.changed);
        assert_eq!(state, state_after_first_compaction);
    }

    /// Verifies expired immutable history is reported as reclaimable and removed by compaction.
    /// 验证到期后的不可变历史会被报告为可回收，并由压缩实际清除。
    #[test]
    fn cache_pressure_distinguishes_active_bodies_from_reclaimable_anchors() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(1, vec![memory_item(7, "short-lived", 0.8)]),
                "1",
            )
            .expect("recall should succeed");
        state
            .register_active_turn("turn-1", "1")
            .expect("expiring logical turn should register");
        let active_pressure = state.cache_pressure();
        assert!(active_pressure.active_body_bytes > 0);
        assert_eq!(active_pressure.reclaimable_anchor_bytes, 0);

        state
            .consume_turn("turn-1", "2")
            .expect("terminal turn should expire the lease");
        let expired_pressure = state.cache_pressure();
        assert_eq!(expired_pressure.active_body_bytes, 0);
        assert!(expired_pressure.reclaimable_anchor_bytes > 0);
        assert!(state.active_leases.contains_key("memory:7"));
        assert!(state.expired_panel_card().is_some());

        let report = state
            .begin_cache_epoch("compact-1", None, "3")
            .expect("compaction should reclaim immutable history");
        assert_eq!(report.expired_lease_count, 1);
        assert!(state.active_leases.is_empty());
        assert!(state.expired_panel_card().is_none());
        assert_eq!(state.cache_pressure(), VmmCachePressure::default());
    }

    /// Verifies stable-anchor rollout changes reset epochs while preserving active lease state.
    /// 验证稳定锚点灰度变更会重置纪元，同时保留活动租约状态。
    #[test]
    fn stable_anchor_feature_transition_resets_epoch_and_rebaselines_on_enable() {
        let mut state = VmmSessionMemoryState::default();
        let batch = recall_batch(3, vec![memory_item(7, "active", 0.8)]);
        state
            .apply_recall_batch("session-a", "user-1", &batch, "1")
            .expect("initial anchored recall should succeed");
        let initial_epoch = state.cache_epoch.epoch_id.clone();

        let disabled = state
            .synchronize_stable_anchor_rendering(false, "2")
            .expect("disabling anchors should reset the epoch");
        assert!(disabled.changed);
        assert_ne!(state.cache_epoch.epoch_id, initial_epoch);
        assert!(state.cache_epoch.anchors.is_empty());
        assert!(state.active_leases.contains_key("memory:7"));
        state
            .apply_recall_batch("session-a", "user-2", &batch, "3")
            .expect("disabled-mode refresh should retain the lease");
        assert!(state.cache_epoch.anchors.is_empty());

        state
            .synchronize_stable_anchor_rendering(true, "4")
            .expect("re-enabling anchors should create a fresh epoch");
        assert!(state.cache_epoch.needs_active_baseline);
        state
            .bind_pending_state_to_user("user-3", "5")
            .expect("the next exact message should receive the active baseline");
        assert_eq!(state.cache_epoch.anchors.len(), 1);
        assert_eq!(
            state.cache_epoch.anchors[0].kind,
            VmmPromptAnchorKind::Baseline
        );
        state
            .validate("session-a")
            .expect("re-enabled anchor state should validate");
    }

    /// Verifies compaction while stable anchors are disabled does not leave an impossible pending baseline.
    /// 验证稳定锚点关闭时执行压缩不会遗留不可能完成的待处理基线。
    #[test]
    fn disabled_stable_anchor_compaction_keeps_a_valid_anchorless_epoch() {
        // state starts with one active lease so the disabled compaction path exercises baseline eligibility.
        // state 先建立一条活动租约，使禁用模式压缩路径覆盖基线资格判断。
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(3, vec![memory_item(7, "active", 0.8)]),
                "1",
            )
            .expect("initial anchored recall should succeed");
        state
            .synchronize_stable_anchor_rendering(false, "2")
            .expect("stable anchors should disable");

        let report = state
            .begin_cache_epoch("compact-disabled", Some("continuation-1"), "3")
            .expect("disabled-mode compaction should preserve a valid epoch");

        assert!(report.changed);
        assert_eq!(report.appended_anchor_count, 0);
        assert!(state.cache_epoch.anchors.is_empty());
        assert!(!state.cache_epoch.needs_active_baseline);
        assert!(state.active_leases.contains_key("memory:7"));
        state
            .validate("session-a")
            .expect("disabled-mode compacted state should remain persistable");
    }

    /// Verifies an oversized recall body is rejected before acquiring session ownership.
    /// 验证超大召回正文会在取得会话归属前被拒绝。
    #[test]
    fn oversized_recall_body_is_rejected_without_mutating_state() {
        let mut state = VmmSessionMemoryState::default();
        let oversized_text = "x".repeat(VMM_MEMORY_BODY_MAX_BYTES + 1);

        let error = state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(2, vec![memory_item(7, &oversized_text, 0.8)]),
                "1",
            )
            .expect_err("oversized recall should fail");

        assert!(error.contains("exceeds the per-memory limit"));
        assert!(state.is_pristine());
    }

    /// Verifies aggregate lease and anchor duplication cannot create a state that exceeds the physical VMM document limit.
    /// 验证租约与锚点的聚合重复内容无法创建超过 VMM 物理文档上限的状态。
    #[test]
    fn oversized_serialized_state_is_rejected_atomically() {
        let mut state = VmmSessionMemoryState::default();
        // large_text remains below the per-memory limit while nine duplicated snapshots exceed the complete file budget.
        // large_text 保持低于单条记忆上限，但九份重复快照会超过完整文件预算。
        let large_text = "x".repeat(240 * 1024);
        let items = (1..=9_u64)
            .map(|memory_id| memory_item(memory_id, &large_text, 0.8))
            .collect::<Vec<_>>();

        let error = state
            .apply_recall_batch("session-a", "user-1", &recall_batch(2, items), "1")
            .expect_err("physically oversized VMM state must fail before commit");

        assert!(error.contains("exceeds the physical limit"));
        assert!(state.is_pristine());
    }

    /// Verifies one precheck cannot introduce an unbounded number of structured memories.
    /// 验证单次 precheck 无法引入无界数量的结构化记忆。
    #[test]
    fn oversized_recall_batch_is_rejected_without_mutating_state() {
        let mut state = VmmSessionMemoryState::default();
        let items = (0..=VMM_RECALL_BATCH_MAX_ITEMS)
            .map(|index| memory_item(index as u64 + 1, "bounded", 0.8))
            .collect::<Vec<_>>();

        let error = state
            .apply_recall_batch("session-a", "user-1", &recall_batch(2, items), "1")
            .expect_err("oversized recall batch should fail");

        assert!(error.contains("exceeds the limit"));
        assert!(state.is_pristine());
    }

    /// Verifies one recall response cannot assign the same stable memory identity twice.
    /// 验证一次召回响应不能重复分配同一个稳定记忆身份。
    #[test]
    fn duplicate_recall_memory_key_is_rejected_before_state_mutation() {
        // state remains pristine because duplicate identity validation precedes ownership acquisition.
        // state 保持纯净，因为重复身份校验先于归属获取。
        let mut state = VmmSessionMemoryState::default();
        // batch deliberately supplies conflicting semantic bodies for the same authoritative memory id.
        // batch 刻意为同一个权威记忆标识提供冲突语义正文。
        let batch = recall_batch(
            2,
            vec![
                memory_item(7, "first-version", 0.8),
                memory_item(7, "second-version", 0.7),
            ],
        );

        let error = state
            .apply_recall_batch("session-a", "user-1", &batch, "1")
            .expect_err("duplicate memory identity must be rejected");

        assert!(error.contains("duplicate memory key `memory:7`"));
        assert!(state.is_pristine());
    }

    /// Verifies persisted active leases must point to an anchor carrying their exact current semantic hash.
    /// 验证持久化活动租约必须指向携带其精确当前语义哈希的锚点。
    #[test]
    fn active_lease_anchor_semantic_mismatch_is_rejected() {
        // state starts valid before one frozen anchor hash is deliberately corrupted.
        // state 在一条冻结锚点哈希被刻意破坏前保持有效。
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(2, vec![memory_item(7, "memory", 0.8)]),
                "1",
            )
            .expect("recall should create a valid anchored lease");
        state.cache_epoch.anchors[0]
            .semantic_content_hashes
            .insert(String::from("memory:7"), String::from("wrong-hash"));

        let error = state
            .validate("session-a")
            .expect_err("active anchor hash mismatch must be rejected");

        assert!(error.contains("does not match the semantic hash frozen"));
    }

    /// Verifies authoritative recycle status revokes local prompt state and blocks later recall until restart.
    /// 验证权威回收状态会撤销本地提示词状态，并在重新开始前阻止后续召回。
    #[test]
    fn authoritative_recycle_status_revokes_and_blocks_local_memory() {
        let mut state = VmmSessionMemoryState::default();
        state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &recall_batch(2, vec![memory_item(7, "memory", 0.8)]),
                "1",
            )
            .expect("recall should create local state");

        let changed = state
            .apply_authoritative_session_status("session-a", SessionMemoryStatus::Recycled, "2")
            .expect("recycle status should apply");

        assert!(changed);
        assert_eq!(state.session_memory_status, SessionMemoryStatus::Recycled);
        assert!(state.active_leases.is_empty());
        assert!(state.cache_epoch.anchors.is_empty());
        assert!(
            state
                .apply_recall_batch(
                    "session-a",
                    "user-2",
                    &recall_batch(2, vec![memory_item(8, "blocked", 0.8)]),
                    "3",
                )
                .is_err()
        );

        state
            .apply_authoritative_session_status("session-a", SessionMemoryStatus::Active, "4")
            .expect("explicit restart status should apply");
        assert!(
            state
                .apply_recall_batch(
                    "session-a",
                    "user-3",
                    &recall_batch(2, vec![memory_item(8, "resumed", 0.8)]),
                    "5",
                )
                .is_ok()
        );
    }
}
