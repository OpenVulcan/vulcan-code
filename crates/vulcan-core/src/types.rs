//! Shared protocol and diagnostic types owned by `vulcan-core`.
//! `vulcan-core` 自有的共享协议与诊断类型。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Describes one controller-level failure that should never be hidden behind the provider runtime.
/// 描述一种不应被 Provider 运行时黑盒掩盖的控制器级失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// The controller was asked to act on an invalid or inconsistent state.
    /// 控制器被要求在无效或不一致的状态上继续执行。
    InvalidState {
        /// Human-readable failure detail.
        /// 人类可读的失败细节。
        message: String,
    },
    /// The provider adapter failed while building or sending a request.
    /// Provider 适配器在构建或发送请求时失败。
    ProviderFailure {
        /// Provider identifier attached to the failure.
        /// 与失败绑定的 Provider 标识。
        provider_id: String,
        /// Human-readable failure detail.
        /// 人类可读的失败细节。
        message: String,
    },
    /// The tool executor failed while running a host-side tool invocation.
    /// 工具执行器在运行宿主侧工具调用时失败。
    ToolFailure {
        /// Tool name attached to the failure.
        /// 与失败绑定的工具名称。
        tool_name: String,
        /// Human-readable failure detail.
        /// 人类可读的失败细节。
        message: String,
    },
}

impl Display for CoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::InvalidState { message } => write!(f, "invalid state: {message}"),
            CoreError::ProviderFailure {
                provider_id,
                message,
            } => write!(f, "provider `{provider_id}` failure: {message}"),
            CoreError::ToolFailure { tool_name, message } => {
                write!(f, "tool `{tool_name}` failure: {message}")
            }
        }
    }
}

impl Error for CoreError {}

/// Represents the stable controller settings that shape every execution turn.
/// 表示塑造每一次执行回合的稳定控制器设置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ControllerConfig {
    /// Provider identifier used by the active controller session.
    /// 当前控制器会话使用的 Provider 标识。
    pub provider_id: String,
    /// Model identifier passed to the provider adapter.
    /// 传递给 Provider 适配器的模型标识。
    pub model_id: String,
    /// System prompt or preamble applied to every request.
    /// 作用于每一次请求的系统提示词或前置语。
    pub system_prompt: String,
    /// Structured prompt layers applied to every request when available.
    /// 可用时作用于每一次请求的结构化提示词层。
    #[serde(default, skip_serializing_if = "ProviderPromptLayers::is_empty")]
    pub prompt_layers: ProviderPromptLayers,
    /// Optional sampling temperature applied to the provider request.
    /// 作用于 Provider 请求的可选采样温度。
    pub temperature: Option<f64>,
    /// Maximum number of tool-bearing turns allowed for one execution.
    /// 单次执行允许的最大工具回合数。
    pub max_turns: usize,
    /// Optional provider-specific additional parameters.
    /// 可选的 Provider 专属附加参数。
    pub additional_params: Option<Value>,
    /// Optional provider-facing context-injection overlay rendered outside the canonical transcript.
    /// 在规范转录之外渲染的可选 Provider 可见上下文注入覆盖层。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_injection: Option<ProviderContextInjection>,
}

impl ControllerConfig {
    /// Builds the exact structured prompt layers rendered for one provider request after persistent context injection.
    /// 构建持久上下文注入后单次 Provider 请求实际渲染的精确结构化提示词层。
    ///
    /// # Returns
    /// 返回值
    /// Prompt layers whose runtime-context section contains persistent blocks in their provider-visible order.
    /// 返回运行时上下文区按 Provider 可见顺序包含持久块的提示词层。
    pub fn provider_prompt_layers_with_persistent_injection(&self) -> ProviderPromptLayers {
        let mut layers = if self.prompt_layers.is_empty() {
            ProviderPromptLayers::from_system_prompt(self.system_prompt.clone())
        } else {
            self.prompt_layers.clone()
        };
        let persistent_text = self
            .context_injection
            .as_ref()
            .map(|injection| injection.persistent_blocks.join("\n\n"))
            .unwrap_or_default();
        if persistent_text.trim().is_empty() {
            return layers;
        }
        // runtime_context placement must match the request builder so continuation fingerprints cover exact bytes.
        // runtime_context 的放置方式必须与请求构建器一致，使 continuation 指纹覆盖精确字节。
        let existing_runtime_context = layers.runtime_context.trim();
        layers.runtime_context = if existing_runtime_context.is_empty() {
            persistent_text
        } else {
            format!("{existing_runtime_context}\n\n{persistent_text}")
        };
        layers
    }

    /// Builds the stable fingerprint of provider input layers whose change rewrites previously covered context.
    /// 构建 Provider 输入层的稳定指纹；这些层一旦变化就会改写先前已覆盖的上下文。
    ///
    /// # Returns
    /// 返回值
    /// A versioned SHA-256 fingerprint of the effective system prompt and persistent injection blocks.
    /// 返回有效系统提示词与持久注入块的带版本 SHA-256 指纹。
    pub fn provider_rewrite_fingerprint(&self) -> String {
        // effective_system_prompt is rendered from the same injected structured layers used by transport assembly.
        // effective_system_prompt 根据传输装配使用的同一份注入后结构化层渲染。
        let effective_system_prompt = self
            .provider_prompt_layers_with_persistent_injection()
            .render_or_legacy(self.system_prompt.as_str());
        provider_rewrite_fingerprint_for_system_prompt(effective_system_prompt.as_str())
    }

    /// Returns whether the saved provider continuation can be reused without replaying rewritten input.
    /// 返回能否在不回放已改写输入的前提下复用已保存的 Provider continuation。
    ///
    /// # Parameters
    /// 参数
    /// `continuation` is the provider cursor and its covered rewrite/anchor revisions.
    /// `continuation` 是 Provider 游标及其已覆盖的改写与锚点修订。
    /// `uncovered_message_ids` are the exact local message ids appended after the saved continuation boundary.
    /// `uncovered_message_ids` 是保存在 continuation 边界之后追加的精确本地消息标识集合。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when rewrite-sensitive input matches and every context overlay mutation is proven safe.
    /// 仅当改写敏感输入一致且上下文覆盖层变更均可证明安全时返回 `true`。
    pub fn continuation_reuse_is_proven(
        &self,
        continuation: &ProviderContinuation,
        uncovered_message_ids: &[&str],
    ) -> bool {
        // current_rewrite_fingerprint covers all non-append-only Provider input layers.
        // current_rewrite_fingerprint 覆盖全部非追加式 Provider 输入层。
        let current_rewrite_fingerprint = self.provider_rewrite_fingerprint();
        if continuation.last_provider_message_id.is_some()
            && continuation.context_rewrite_fingerprint.as_deref()
                != Some(current_rewrite_fingerprint.as_str())
        {
            return false;
        }
        match self.context_injection.as_ref() {
            Some(injection) => {
                injection.continuation_reuse_is_proven(continuation, uncovered_message_ids)
            }
            None => {
                continuation.context_cache_epoch_id.is_none()
                    && continuation.context_anchor_revision.is_none()
            }
        }
    }
}

/// Builds the versioned rewrite fingerprint for one exact Provider-visible system prompt.
/// 为一份精确的 Provider 可见系统提示词构建带版本的改写指纹。
///
/// # Parameters
/// 参数
/// `system_prompt` is the fully rendered system input sent to the Provider.
/// `system_prompt` 是发送给 Provider 的完整渲染系统输入。
///
/// # Returns
/// 返回值
/// A lowercase SHA-256 fingerprint suitable for continuation coverage comparison.
/// 返回适用于 continuation 覆盖比较的小写 SHA-256 指纹。
pub fn provider_rewrite_fingerprint_for_system_prompt(system_prompt: &str) -> String {
    // hasher uses a domain/version prefix so future protocol changes cannot collide silently.
    // hasher 使用领域与版本前缀，避免未来协议变化时静默碰撞。
    let mut hasher = Sha256::new();
    hasher.update(b"vulcan-provider-rewrite-fingerprint:v1\0");
    hasher.update(system_prompt.len().to_le_bytes());
    hasher.update(system_prompt.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Returns whether one `usize` field is zero for compact protocol serialization.
/// 返回一个 `usize` 字段是否为零，用于紧凑协议序列化。
///
/// # Parameters
/// 参数
/// `value` is the borrowed numeric field inspected by serde.
/// `value` 是由 serde 检查的借用数值字段。
///
/// # Returns
/// 返回值
/// `true` only for zero.
/// 仅在数值为零时返回 `true`。
fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}

/// Represents provider-facing context-injection text resolved before one controller run starts.
/// 表示单次控制器运行开始前解析完成的 Provider 可见上下文注入文本。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderContextInjection {
    /// Persistent hidden user blocks rendered as clean tag text.
    /// 渲染为干净标签文本的永久隐藏用户块。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub persistent_blocks: Vec<String>,
    /// Transient blocks that should prefix the target user prompt.
    /// 应前置到目标用户提示词的临时块。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub transient_prefix: String,
    /// Immutable prefixes bound to exact canonical user-message identifiers.
    /// 绑定到精确规范用户消息标识的不可变前缀。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchored_prefixes: Vec<ProviderContextAnchor>,
    /// Strongest canonical-history mutation represented by this overlay.
    /// 该覆盖层所代表的最强规范历史变更类型。
    #[serde(default)]
    pub mutation_kind: ProviderContextMutationKind,
    /// Active prompt-cache epoch covered by the immutable anchors.
    /// 不可变锚点所归属的当前提示词缓存纪元。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_epoch_id: Option<String>,
    /// Monotonic immutable-anchor revision within the active cache epoch.
    /// 当前缓存纪元内的单调不可变锚点修订号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_revision: Option<u64>,
    /// Fingerprint of provider input layers that rewrite existing system or historical content.
    /// 会改写既有系统层或历史内容的 Provider 输入层指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewrite_fingerprint: Option<String>,
    /// Bytes occupied by currently active structured VMM memory bodies.
    /// 当前活动结构化 VMM 记忆正文占用的字节数。
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub active_memory_body_bytes: usize,
    /// Estimated tokens occupied by currently active structured VMM memory bodies.
    /// 当前活动结构化 VMM 记忆正文占用的估算 Token 数。
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub active_memory_estimated_tokens: usize,
    /// Bytes held by immutable VMM anchors reclaimable at the next compaction.
    /// 可在下次压缩回收的不可变 VMM 锚点字节数。
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub reclaimable_anchor_bytes: usize,
    /// Estimated tokens held by immutable VMM anchors reclaimable at the next compaction.
    /// 可在下次压缩回收的不可变 VMM 锚点估算 Token 数。
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub reclaimable_anchor_estimated_tokens: usize,
    /// Stable fingerprint for diagnostics and continuation safety checks.
    /// 用于诊断与 continuation 安全检查的稳定指纹。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fingerprint: String,
}

impl ProviderContextInjection {
    /// Returns whether no provider-facing injection content is present.
    /// 返回是否不存在任何 Provider 可见注入内容。
    pub fn is_empty(&self) -> bool {
        self.persistent_blocks.is_empty()
            && self.transient_prefix.trim().is_empty()
            && self.anchored_prefixes.is_empty()
    }

    /// Returns whether transient provider-facing content is present.
    /// 返回是否存在临时 Provider 可见内容。
    pub fn has_transient(&self) -> bool {
        !self.transient_prefix.trim().is_empty()
    }

    /// Returns whether continuation reuse must be disabled because prior provider input changed.
    /// 返回是否因既有 Provider 输入发生变化而必须禁用 continuation 复用。
    pub fn requires_historical_rewrite(&self) -> bool {
        self.has_transient()
            || matches!(
                self.mutation_kind,
                ProviderContextMutationKind::HistoricalRewrite
                    | ProviderContextMutationKind::EpochReset
            )
    }

    /// Returns whether an append-only overlay is provably covered after the saved continuation boundary.
    /// 返回追加式覆盖层是否可证明全部位于已保存 continuation 边界之后。
    ///
    /// # Parameters
    /// 参数
    /// `continuation` is the provider cursor and its last covered cache revision.
    /// `continuation` 是 Provider 游标及其最近覆盖的缓存修订。
    /// `uncovered_message_ids` are exact local message ids after the cursor boundary and must own every uncovered anchor.
    /// `uncovered_message_ids` 是游标边界之后的精确本地消息标识，必须承载全部未覆盖锚点。
    ///
    /// # Returns
    /// 返回值
    /// `true` for no rewrite or a same-epoch monotonic append whose uncovered suffix is bound to the unsent tail; unknown coverage is unsafe.
    /// 无重写，或同纪元单调追加且未覆盖后缀绑定到未发送尾部时返回 `true`；覆盖未知时不安全。
    pub fn continuation_reuse_is_proven(
        &self,
        continuation: &ProviderContinuation,
        uncovered_message_ids: &[&str],
    ) -> bool {
        if self.requires_historical_rewrite() {
            return false;
        }
        if self.anchored_prefixes.is_empty() {
            return self.mutation_kind != ProviderContextMutationKind::AppendOnly
                && continuation.context_cache_epoch_id.is_none()
                && continuation.context_anchor_revision.is_none();
        }
        if self.mutation_kind != ProviderContextMutationKind::AppendOnly {
            return false;
        }
        if continuation.last_provider_message_id.is_none() {
            return true;
        }
        // current_revision must describe this exact ordered anchor vector before suffix coverage can be trusted.
        // current_revision 必须精确描述当前有序锚点向量，之后才能信任后缀覆盖关系。
        let Some(current_revision) = self.anchor_revision else {
            return false;
        };
        if usize::try_from(current_revision).ok() != Some(self.anchored_prefixes.len()) {
            return false;
        }
        // covered_anchor_count is the immutable prefix already represented by the cursor.
        // covered_anchor_count 是游标已经表示的不可变锚点前缀长度。
        let covered_anchor_count = match (
            self.cache_epoch_id.as_deref(),
            continuation.context_cache_epoch_id.as_deref(),
            continuation.context_anchor_revision,
        ) {
            (Some(current_epoch), Some(covered_epoch), Some(covered_revision))
                if current_epoch == covered_epoch && covered_revision <= current_revision =>
            {
                let Ok(covered_anchor_count) = usize::try_from(covered_revision) else {
                    return false;
                };
                covered_anchor_count
            }
            _ => return false,
        };
        // Every uncovered anchor must target a message after the exact local cursor boundary; revision order alone cannot prove placement.
        // 每个未覆盖锚点都必须指向精确本地游标边界之后的消息；仅凭修订顺序无法证明位置。
        self.anchored_prefixes[covered_anchor_count..]
            .iter()
            .all(|anchor| {
                uncovered_message_ids
                    .iter()
                    .any(|message_id| *message_id == anchor.target_message_id)
            })
    }
}

/// One immutable provider prefix bound to an exact canonical user-message identifier.
/// 绑定到精确规范用户消息标识的一条不可变 Provider 前缀。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderContextAnchor {
    /// Stable anchor identifier retained for diagnostics.
    /// 为诊断保留的稳定锚点标识。
    pub anchor_id: String,
    /// Exact canonical user-message identifier receiving this prefix.
    /// 接收该前缀的精确规范用户消息标识。
    pub target_message_id: String,
    /// Frozen provider-visible prefix applied only to the target message.
    /// 仅应用到目标消息的冻结 Provider 可见前缀。
    pub prefix: String,
}

/// Provider-context mutation classification used for continuation safety.
/// 用于 continuation 安全判定的 Provider 上下文变更分类。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderContextMutationKind {
    /// No provider-visible canonical-history mutation occurred.
    /// 未发生 Provider 可见规范历史变更。
    #[default]
    None,
    /// New content was appended without rewriting any previously sent message.
    /// 新内容以追加方式进入，未改写任何既有已发送消息。
    AppendOnly,
    /// A previously sent provider message would be rendered differently.
    /// 某条既有已发送 Provider 消息将以不同内容重新渲染。
    HistoricalRewrite,
    /// Context compaction replaced the provider-visible history baseline.
    /// 上下文压缩替换了 Provider 可见历史基线。
    EpochReset,
}

/// Represents one provider continuation cursor snapshot kept beside the canonical history.
/// 表示与规范历史并存保存的一份 Provider continuation 游标快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderContinuation {
    /// Provider identifier that produced the latest cursor state.
    /// 产出最新游标状态的 Provider 标识。
    pub provider_id: Option<String>,
    /// Model identifier that produced the latest cursor state.
    /// 产出最新游标状态的模型标识。
    pub model_id: Option<String>,
    /// Latest provider-visible assistant message identifier.
    /// 最近一次 Provider 可见的助手消息标识。
    pub last_provider_message_id: Option<String>,
    /// Exact local canonical message identifier represented by the latest provider cursor.
    /// 最近一次 Provider 游标所表示的精确本地规范消息标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_local_message_id: Option<String>,
    /// Prompt-cache epoch covered by the latest provider response.
    /// 最近一次 Provider 响应所覆盖的提示词缓存纪元。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_cache_epoch_id: Option<String>,
    /// Immutable-anchor revision covered by the latest provider response.
    /// 最近一次 Provider 响应所覆盖的不可变锚点修订号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_anchor_revision: Option<u64>,
    /// Rewrite-sensitive context fingerprint covered by the latest provider response.
    /// 最近一次 Provider 响应所覆盖的改写敏感上下文指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_rewrite_fingerprint: Option<String>,
    /// Unix-millis timestamp string recorded when the cursor was refreshed.
    /// 游标刷新时记录的 Unix 毫秒时间戳文本。
    pub updated_at: Option<String>,
}

/// Represents the canonical conversation snapshot owned by the controller.
/// 表示由控制器持有的规范会话快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationSnapshot {
    /// Active canonical message window that will be sent to the next provider turn.
    /// 将发送到下一次 Provider 回合的活动规范消息窗口。
    pub messages: Vec<ConversationMessage>,
    /// Durable record stream retained for transcript projection and host metadata.
    /// 为转录投影与宿主元数据保留的持久化记录流。
    #[serde(default)]
    pub records: Vec<ConversationRecord>,
    /// Provider continuation snapshot used to chain later requests.
    /// 用于串联后续请求的 Provider continuation 快照。
    pub continuation: ProviderContinuation,
}

/// Represents one canonical protocol message stored in the active conversation window.
/// 表示存储在活动会话窗口中的一条规范协议消息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationMessage {
    /// Stable message identifier owned by the local workbench.
    /// 由本地工作台持有的稳定消息标识。
    pub id: String,
    /// Structured payload describing the message contents.
    /// 描述消息内容的结构化载荷。
    pub payload: ConversationPayload,
}

/// Represents one durable transcript record stored outside the active message window.
/// 表示存储在活动消息窗口之外的一条持久化转录记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationRecord {
    /// Stable record identifier shared by projection caches and checkpoints.
    /// 被投影缓存与检查点共享的稳定记录标识。
    pub id: String,
    /// Transcript role attached to the record.
    /// 绑定到该记录的转录角色。
    pub role: TranscriptRole,
    /// Durable record kind that describes whether the record is visible or internal.
    /// 描述该记录是可见还是内部节点的持久化记录类型。
    pub record_kind: TranscriptRecordKind,
    /// Whether the record should appear in the visible transcript projection.
    /// 该记录是否应出现在可见转录投影中。
    pub visible: bool,
    /// Optional display title for the transcript card header.
    /// 转录卡片头部使用的可选显示标题。
    pub title: String,
    /// Primary body text shown in the transcript and modal surfaces.
    /// 在转录与弹层中展示的主要正文文本。
    pub body: String,
    /// Optional structured metadata for host-side audit and transcript-only enrichment.
    /// 供宿主审计与仅转录增强使用的可选结构化元数据。
    pub metadata: Option<Value>,
    /// Unix-millis timestamp string captured when the record was created.
    /// 记录创建时捕获的 Unix 毫秒时间戳文本。
    pub created_at: String,
}

/// Represents one canonical assistant tool-call entry.
/// 表示一条规范的助手工具调用项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationToolCall {
    /// Stable tool-call item identifier emitted by the provider.
    /// 由 Provider 发出的稳定工具调用项标识。
    pub id: String,
    /// Optional provider-visible call identifier used by tool-result messages.
    /// 工具结果消息使用的可选 Provider 可见调用标识。
    pub tool_call_id: Option<String>,
    /// Tool function name selected by the assistant.
    /// 助手选择的工具函数名称。
    pub tool_name: String,
    /// Structured JSON arguments selected by the assistant.
    /// 助手选择的结构化 JSON 参数。
    pub arguments: Value,
}

/// Represents one canonical user attachment retained for future request replays.
/// 表示一条为后续请求回放保留的规范用户附件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationAttachmentKind {
    /// Audio attachment backed by one local file path.
    /// 由本地文件路径支撑的音频附件。
    Audio,
    /// Image attachment backed by one local file path.
    /// 由本地文件路径支撑的图片附件。
    Image,
    /// Video attachment backed by one local file path.
    /// 由本地文件路径支撑的视频附件。
    Video,
    /// PDF attachment backed by one local file path.
    /// 由本地文件路径支撑的 PDF 附件。
    Pdf,
}

impl ConversationAttachmentKind {
    /// Returns the stable lowercase modality name attached to the current attachment kind.
    /// 返回绑定到当前附件类型上的稳定小写模态名称。
    pub fn modality_name(&self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Image => "image",
            Self::Video => "video",
            Self::Pdf => "pdf",
        }
    }

    /// Returns the compact Chinese label shown in host-side attachment previews and chips.
    /// 返回展示在宿主侧附件预览与标签中的紧凑中文名称。
    pub fn display_label(&self) -> &'static str {
        match self {
            Self::Audio => "音频",
            Self::Image => "图片",
            Self::Video => "视频",
            Self::Pdf => "PDF",
        }
    }
}

/// Represents one structured user attachment kept beside canonical user text.
/// 表示一条保存在规范用户文本旁边的结构化用户附件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationAttachment {
    /// Stable attachment kind used by capability gates and provider mappers.
    /// 供能力门控与 Provider 映射使用的稳定附件类型。
    pub kind: ConversationAttachmentKind,
    /// Human-readable file name shown in transcript and queue previews.
    /// 展示在转录与队列预览中的人类可读文件名。
    pub file_name: String,
    /// Canonical managed local file path used to re-read the attachment during future provider replays.
    /// 在未来 Provider 回放期间用于重新读取附件的规范托管本地文件路径。
    pub local_path: String,
    /// Optional original source path retained for diagnostics and replay fallback when the managed copy is missing.
    /// 当托管副本缺失时，为诊断与回放回退保留的可选原始来源路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Optional stable managed-asset identifier retained by the host-managed attachment store.
    /// 宿主管理附件存储保留的可选稳定托管资产标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    /// Optional source byte length retained for host-side budget estimation and diagnostics.
    /// 为宿主侧预算估算与诊断保留的可选源字节长度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<u64>,
    /// Optional local preview image path derived from the attachment, used by host-side media cards when the primary file is not directly displayable.
    /// 当主文件本身不可直接展示时，供宿主侧媒体卡片使用的、从附件派生出的可选本地预览图片路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_local_path: Option<String>,
    /// Resolved MIME type attached to the local file.
    /// 绑定到本地文件上的已解析 MIME 类型。
    pub mime_type: String,
}

/// Represents one structured canonical payload inside the controller message window.
/// 表示控制器消息窗口内部的一条结构化规范载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversationPayload {
    /// Plain user-authored text input.
    /// 纯文本用户输入。
    UserText {
        /// Visible user text content.
        /// 用户可见文本内容。
        text: String,
        /// Structured attachments that should travel with the user-authored text.
        /// 应随用户文本一起传递的结构化附件集合。
        #[serde(default)]
        attachments: Vec<ConversationAttachment>,
    },
    /// Assistant turn that may contain visible text, reasoning, and tool calls.
    /// 可能同时包含可见文本、思考内容与工具调用的助手回合。
    Assistant {
        /// Visible assistant text content.
        /// 助手可见文本内容。
        text: String,
        /// Flattened reasoning text kept for local audit only.
        /// 保留给本地审计使用的扁平化思考文本。
        reasoning: String,
        /// Optional provider-native assistant message identifier.
        /// 可选的 Provider 原生助手消息标识。
        provider_message_id: Option<String>,
        /// Optional provider-native assistant state that must survive future history replays.
        /// 必须在后续历史回放中保留下来的可选 Provider 原生助手状态。
        provider_message_state: Option<Value>,
        /// Optional provider usage snapshot retained for host-side budget calibration.
        /// 为宿主侧预算校准保留的可选 Provider 用量快照。
        #[serde(default)]
        usage: Option<ProviderUsageSnapshot>,
        /// Structured tool-call list emitted by the assistant turn.
        /// 由该助手回合发出的结构化工具调用列表。
        tool_calls: Vec<ConversationToolCall>,
    },
    /// Tool-result payload returned to the model after one assistant tool call.
    /// 在一次助手工具调用后回传给模型的工具结果载荷。
    ToolResult {
        /// Optional provider-visible tool-call identifier linked to the result.
        /// 与该结果关联的可选 Provider 可见工具调用标识。
        tool_call_id: Option<String>,
        /// Flattened human-readable tool-result text.
        /// 已扁平化的人类可读工具结果文本。
        text: String,
    },
    /// Inter-agent message delivered through the session mailbox.
    /// 通过会话 mailbox 投递的代理间消息。
    AgentMessage {
        /// Stable message type shown to the receiving model.
        /// 展示给接收模型的稳定消息类型。
        message_type: String,
        /// Logical task name or agent path associated with the message.
        /// 与该消息关联的逻辑任务名或代理路径。
        task_name: String,
        /// Logical sender name or agent path.
        /// 逻辑发送方名称或代理路径。
        sender: String,
        /// Logical recipient name or agent path.
        /// 逻辑接收方名称或代理路径。
        recipient: String,
        /// Model-visible payload text.
        /// 模型可见的载荷文本。
        payload: String,
        /// Whether this message may wake an idle parent turn.
        /// 该消息是否允许唤醒空闲父级回合。
        trigger_turn: bool,
        /// Host-only metadata retained for UI, audit, and diagnostics.
        /// 供 UI、审计与诊断保留的宿主侧元数据。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<Value>,
    },
}

impl ConversationPayload {
    /// Renders an inter-agent message into a provider-compatible user-message XML envelope.
    /// 将代理间消息渲染为兼容 Provider 的用户消息 XML 信封。
    pub fn render_agent_message_text(
        message_type: &str,
        task_name: &str,
        sender: &str,
        payload: &str,
    ) -> String {
        format!(
            "<agent_message>\n  <message_type>{}</message_type>\n  <task_name>{}</task_name>\n  <sender>{}</sender>\n  <payload>{}</payload>\n</agent_message>",
            xml_escape_text(message_type.trim()),
            xml_escape_text(task_name.trim()),
            xml_escape_text(sender.trim()),
            xml_escape_text(payload)
        )
    }

    /// Returns the full provider-compatible text for one agent message payload.
    /// 返回单条代理消息载荷的完整 Provider 兼容文本。
    pub fn agent_message_model_text(&self) -> Option<String> {
        match self {
            Self::AgentMessage {
                message_type,
                task_name,
                sender,
                payload,
                ..
            } => Some(Self::render_agent_message_text(
                message_type,
                task_name,
                sender,
                payload,
            )),
            _ => None,
        }
    }
}

/// Escapes text inserted into model-visible XML compatibility envelopes.
/// 转义插入模型可见 XML 兼容信封的文本。
fn xml_escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Represents the role that a transcript record should surface in the projection layer.
/// 表示转录记录在投影层应呈现的角色。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptRole {
    /// User-originated content.
    /// 用户发起的内容。
    User,
    /// Assistant-originated content.
    /// 助手发起的内容。
    Assistant,
    /// Tool-originated content.
    /// 工具发起的内容。
    Tool,
    /// System-originated content.
    /// 系统发起的内容。
    System,
    /// Thinking-only content that should stay local.
    /// 仅供本地保留的思考内容。
    Thinking,
    /// Summary or compaction content.
    /// 摘要或压缩内容。
    Summary,
    /// Hidden explore or inspection content.
    /// 隐藏的探索或检查内容。
    Explore,
    /// Any other role that has not been normalized yet.
    /// 任何尚未完成规范化的其他角色。
    Other,
}

/// Represents the durable record category used by the projection and checkpoint layers.
/// 表示投影层与检查点层使用的持久化记录类别。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptRecordKind {
    /// The record should appear in the visible transcript.
    /// 该记录应出现在可见转录中。
    Visible,
    /// The record should remain hidden from normal transcript surfaces.
    /// 该记录应对普通转录界面保持隐藏。
    Hidden,
    /// The record is a checkpoint or compaction anchor.
    /// 该记录是检查点或压缩锚点。
    Checkpoint,
    /// The record exists only as a projection helper.
    /// 该记录仅作为投影辅助节点存在。
    ProjectionOnly,
}

/// Represents one tool invocation request that can be executed by the host tool runtime.
/// 表示可由宿主工具运行时执行的一次工具调用请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocationRequest {
    /// Stable tool-call identifier associated with the request.
    /// 与请求关联的稳定工具调用标识。
    pub tool_call_id: String,
    /// Tool name requested by the assistant.
    /// 助手请求的工具名称。
    pub tool_name: String,
    /// Structured JSON arguments passed to the tool.
    /// 传递给工具的结构化 JSON 参数。
    pub arguments: Value,
}

/// Represents one tool execution result returned by the host runtime.
/// 表示由宿主运行时返回的一次工具执行结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocationResult {
    /// Stable tool-call identifier associated with the result.
    /// 与结果关联的稳定工具调用标识。
    pub tool_call_id: String,
    /// Tool name that produced the result.
    /// 产出该结果的工具名称。
    pub tool_name: String,
    /// Text that should be fed back to the model.
    /// 应当回传给模型的文本内容。
    pub model_output: String,
    /// Optional host-side structured metadata reserved for UI and audit consumers.
    /// 供 UI 与审计消费者保留的可选宿主侧结构化元数据。
    pub host_result: Option<Value>,
    /// Whether the tool invocation should be treated as an error by the controller.
    /// 控制器是否应将该工具调用视为错误。
    pub is_error: bool,
}

/// Represents one normalized provider usage snapshot retained for budget calibration and diagnostics.
/// 表示一份被保留用于预算校准与诊断的规范化 Provider 用量快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderUsageSnapshot {
    /// Optional provider-reported input token count.
    /// Provider 报告的可选输入 Token 数量。
    #[serde(default)]
    pub input_tokens: Option<usize>,
    /// Optional provider-reported output token count.
    /// Provider 报告的可选输出 Token 数量。
    #[serde(default)]
    pub output_tokens: Option<usize>,
    /// Optional provider-reported reasoning token count.
    /// Provider 报告的可选推理 Token 数量。
    #[serde(default)]
    pub reasoning_tokens: Option<usize>,
    /// Optional provider-reported total token count.
    /// Provider 报告的可选总 Token 数量。
    #[serde(default)]
    pub total_tokens: Option<usize>,
    /// Optional provider-reported cache-hit input token count.
    /// Provider 报告的可选缓存命中输入 Token 数量。
    #[serde(default)]
    pub cached_input_tokens: Option<usize>,
    /// Optional provider-reported cache-creation input token count.
    /// Provider 报告的可选缓存写入输入 Token 数量。
    #[serde(default)]
    pub cache_creation_input_tokens: Option<usize>,
}

/// Represents structured provider prompt layers before transport-specific rendering.
/// 表示传输专属渲染前的结构化 Provider 提示词层。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPromptLayers {
    /// System-level instructions.
    /// 系统级指令。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub system: String,
    /// Developer-level instructions.
    /// 开发者级指令。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub developer: String,
    /// Tool guidance generated outside provider-native tool schemas.
    /// 在 Provider 原生工具 schema 之外生成的工具指导。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool_guidance: String,
    /// Runtime/user-context instructions that should accompany the current request.
    /// 应随当前请求携带的运行时或用户上下文指令。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub runtime_context: String,
}

impl ProviderPromptLayers {
    /// Build layers from one legacy system prompt.
    /// 从一份旧版系统提示词构建层信息。
    pub fn from_system_prompt(system_prompt: impl Into<String>) -> Self {
        Self {
            system: system_prompt.into(),
            ..Self::default()
        }
    }

    /// Returns whether all prompt layers are empty.
    /// 返回所有提示词层是否均为空。
    pub fn is_empty(&self) -> bool {
        self.system.trim().is_empty()
            && self.developer.trim().is_empty()
            && self.tool_guidance.trim().is_empty()
            && self.runtime_context.trim().is_empty()
    }

    /// Render layers into one stable fallback system prompt.
    /// 将各层渲染为一份稳定的回退系统提示词。
    pub fn render_fallback_system_prompt(&self) -> String {
        let mut sections = Vec::new();
        push_prompt_layer_section(&mut sections, None, &self.system);
        push_prompt_layer_section(
            &mut sections,
            Some("Developer Instructions"),
            &self.developer,
        );
        push_prompt_layer_section(&mut sections, Some("Tool Guidance"), &self.tool_guidance);
        push_prompt_layer_section(
            &mut sections,
            Some("Runtime Context"),
            &self.runtime_context,
        );
        sections.join("\n\n")
    }

    /// Return the fallback rendering, or the supplied legacy prompt when layers are empty.
    /// 返回回退渲染；当层为空时返回给定的旧版提示词。
    pub fn render_or_legacy(&self, legacy_system_prompt: &str) -> String {
        if self.is_empty() {
            legacy_system_prompt.trim().to_string()
        } else {
            self.render_fallback_system_prompt()
        }
    }
}

/// Push one optional titled prompt layer into a fallback render buffer.
/// 将一个可选标题的提示词层推入回退渲染缓冲区。
fn push_prompt_layer_section(sections: &mut Vec<String>, title: Option<&str>, body: &str) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    match title {
        Some(title) => sections.push(format!("## {title}\n{body}")),
        None => sections.push(body.to_string()),
    }
}

impl ProviderUsageSnapshot {
    /// Returns whether the snapshot contains at least one concrete token count.
    /// 返回该快照是否至少包含一个具体 Token 数量。
    pub const fn has_any_tokens(&self) -> bool {
        self.input_tokens.is_some()
            || self.output_tokens.is_some()
            || self.reasoning_tokens.is_some()
            || self.total_tokens.is_some()
            || self.cached_input_tokens.is_some()
            || self.cache_creation_input_tokens.is_some()
    }

    /// Adds another provider usage snapshot into this snapshot field by field.
    /// 将另一份 Provider 用量快照按字段累加到当前快照中。
    ///
    /// # Parameters
    /// 参数
    /// `other` is the successful provider call usage to add.
    /// `other` 是需要累加的成功 Provider 调用用量。
    pub fn accumulate_from(&mut self, other: &Self) {
        accumulate_optional_token_count(&mut self.input_tokens, other.input_tokens);
        accumulate_optional_token_count(&mut self.output_tokens, other.output_tokens);
        accumulate_optional_token_count(&mut self.reasoning_tokens, other.reasoning_tokens);
        accumulate_optional_token_count(&mut self.total_tokens, other.total_tokens);
        accumulate_optional_token_count(&mut self.cached_input_tokens, other.cached_input_tokens);
        accumulate_optional_token_count(
            &mut self.cache_creation_input_tokens,
            other.cache_creation_input_tokens,
        );
    }
}

/// Adds one optional token count into an optional accumulated token count.
/// 将一个可选 Token 数累加到可选累计 Token 数中。
///
/// # Parameters
/// 参数
/// `target` receives the accumulated token count when `value` exists.
/// `target` 在 `value` 存在时接收累计后的 Token 数。
/// `value` is the token count reported by one provider call.
/// `value` 是单次 Provider 调用报告的 Token 数。
fn accumulate_optional_token_count(target: &mut Option<usize>, value: Option<usize>) {
    let Some(value) = value else {
        return;
    };
    *target = Some(target.unwrap_or(0).saturating_add(value));
}

/// Represents one normalized provider turn response consumed by the host-owned controller loop.
/// 表示一份由宿主自有控制器循环消费的规范化 Provider 单回合响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTurnResponse {
    /// Visible assistant text returned by the provider.
    /// Provider 返回的助手可见文本。
    pub assistant_text: String,
    /// Flattened assistant reasoning retained for local audit only.
    /// 仅保留给本地审计使用的扁平化助手思考内容。
    pub assistant_reasoning: String,
    /// Optional provider-native assistant message identifier.
    /// 可选的 Provider 原生助手消息标识。
    pub provider_message_id: Option<String>,
    /// Optional provider-native assistant state that must survive canonical history replay.
    /// 必须在规范历史回放中保留下来的可选 Provider 原生助手状态。
    pub provider_message_state: Option<Value>,
    /// Optional provider usage snapshot retained for host-side budget calibration.
    /// 为宿主侧预算校准保留的可选 Provider 用量快照。
    #[serde(default)]
    pub usage: Option<ProviderUsageSnapshot>,
    /// Structured tool calls returned by the provider for the current turn.
    /// Provider 在当前回合返回的结构化工具调用集合。
    #[serde(default)]
    pub tool_calls: Vec<ConversationToolCall>,
    /// Optional continuation cursor snapshot refreshed by the provider.
    /// 由 Provider 刷新的可选 continuation 游标快照。
    pub continuation: Option<ProviderContinuation>,
}

/// Represents one streaming delta emitted while the provider is still building the normalized turn response.
/// 表示 Provider 在构建规范化回合响应期间持续发出的单条流式增量。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderTurnDelta {
    /// One reasoning delta emitted by the provider stream.
    /// Provider 流发出的一条思考增量。
    Thinking {
        /// Raw visible reasoning delta text.
        /// 原始可见思考增量文本。
        delta: String,
    },
    /// One assistant output delta emitted by the provider stream.
    /// Provider 流发出的一条助手输出增量。
    Output {
        /// Raw visible assistant output delta text.
        /// 原始可见助手输出增量文本。
        delta: String,
    },
}

/// Represents one summary of a completed host-owned controller loop.
/// 表示一份已完成的宿主自有控制器循环摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerRunResult {
    /// Total provider turns executed during the loop.
    /// 循环期间实际执行的 Provider 回合总数。
    pub provider_turns_executed: usize,
    /// Total tool calls processed during the loop, including skipped calls.
    /// 循环期间处理过的工具调用总数，包含被策略跳过的调用。
    pub tool_calls_processed: usize,
    /// Final assistant message identifier produced by the loop when one exists.
    /// 循环结束时产出的最终助手消息标识（如果存在）。
    pub final_assistant_message_id: Option<String>,
}

/// Represents one request snapshot captured before a provider call is sent.
/// 表示在发送 Provider 调用前捕获的一份请求快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequestSnapshot {
    /// Provider identifier used for the request.
    /// 用于该请求的 Provider 标识。
    pub provider_id: String,
    /// Model identifier used for the request.
    /// 用于该请求的模型标识。
    pub model_id: String,
    /// System prompt attached to the request.
    /// 附加到请求中的系统提示词。
    pub system_prompt: String,
    /// Structured prompt layers attached to the request when available.
    /// 可用时附加到请求中的结构化提示词层。
    #[serde(default, skip_serializing_if = "ProviderPromptLayers::is_empty")]
    pub prompt_layers: ProviderPromptLayers,
    /// Latest outgoing prompt message body attached to the request.
    /// 附加到请求中的最新出站提示消息正文。
    pub prompt: String,
    /// Canonical history forwarded into the request.
    /// 传入请求的规范历史。
    pub history: Vec<ConversationMessage>,
    /// Temperature attached to the request.
    /// 附加到请求中的温度参数。
    pub temperature: Option<f64>,
    /// Maximum number of tool turns allowed for this request.
    /// 该请求允许的最大工具回合数。
    pub max_turns: usize,
    /// Tool schemas exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的工具 schema 集合。
    pub tool_schemas: Vec<ProviderRequestToolSchema>,
    /// Optional provider-specific additional parameters.
    /// 可选的 Provider 专属附加参数。
    pub additional_params: Option<Value>,
    /// Optional serialized request headers that will be sent to the provider.
    /// 将要发送给 Provider 的可选序列化请求头。
    pub request_headers: Option<Value>,
    /// Continuation cursor snapshot that will participate in request chaining.
    /// 将参与请求串联的 continuation 游标快照。
    pub continuation: ProviderContinuation,
}

/// Represents one tool schema exposed to the provider inside a request snapshot.
/// 表示请求快照中暴露给 Provider 的单个工具 schema。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequestToolSchema {
    /// Stable tool identifier exposed to the provider.
    /// 暴露给 Provider 的稳定工具标识。
    pub id: String,
    /// Human-readable tool name exposed to the provider.
    /// 暴露给 Provider 的人类可读工具名称。
    pub name: String,
    /// Human-readable tool description exposed to the provider.
    /// 暴露给 Provider 的人类可读工具描述。
    pub description: String,
    /// Stable tool source label used for diagnostics and UI inspection.
    /// 用于诊断与界面检查的稳定工具来源标签。
    pub source_label: String,
    /// JSON Schema describing the tool arguments sent to the provider.
    /// 描述发送给 Provider 的工具参数 JSON Schema。
    pub parameters_schema: Value,
}

/// Represents one controller event emitted for the WebUI, logs, and persistence adapters.
/// 表示供 WebUI、日志与持久化适配器消费的一条控制器事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreEvent {
    /// Event kind describing the semantic phase.
    /// 描述语义阶段的事件类型。
    pub kind: CoreEventKind,
    /// Human-readable event title.
    /// 人类可读的事件标题。
    pub title: String,
    /// Human-readable event body.
    /// 人类可读的事件正文。
    pub body: String,
    /// Optional structured metadata reserved for host-side consumers.
    /// 供宿主侧消费者保留的可选结构化元数据。
    pub metadata: Option<Value>,
}

/// Represents the semantic kind of a controller event.
/// 表示控制器事件的语义类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CoreEventKind {
    /// Assistant text emitted by the provider stream.
    /// Provider 流式输出的助手文本。
    Output,
    /// Assistant reasoning emitted by the provider stream.
    /// Provider 流式输出的助手思考内容。
    Thinking,
    /// A tool call has started.
    /// 一次工具调用已开始。
    ToolStarted,
    /// A tool call has finished.
    /// 一次工具调用已完成。
    ToolFinished,
    /// A final response or final history snapshot has been produced.
    /// 已产生最终回复或最终历史快照。
    Final,
    /// The controller encountered an error.
    /// 控制器遇到错误。
    Error,
}

#[cfg(test)]
mod continuation_proof_tests {
    use super::*;

    /// Builds one scoped provider cursor whose rewrite fingerprint matches the supplied configuration.
    /// 构建一条作用域明确且改写指纹与给定配置匹配的 Provider 游标。
    fn matching_continuation(config: &ControllerConfig) -> ProviderContinuation {
        ProviderContinuation {
            last_provider_message_id: Some(String::from("response-1")),
            last_local_message_id: Some(String::from("assistant-1")),
            context_rewrite_fingerprint: Some(config.provider_rewrite_fingerprint()),
            ..ProviderContinuation::default()
        }
    }

    /// Verifies persistent context fingerprints cover the exact titled runtime-context bytes rendered by transports.
    /// 验证持久上下文指纹覆盖传输层实际渲染的带标题运行时上下文字节。
    #[test]
    fn persistent_fingerprint_matches_exact_structured_prompt_rendering() {
        let config = ControllerConfig {
            system_prompt: String::from("system"),
            prompt_layers: ProviderPromptLayers::from_system_prompt(String::from("system")),
            context_injection: Some(ProviderContextInjection {
                persistent_blocks: vec![String::from("persistent")],
                ..ProviderContextInjection::default()
            }),
            ..ControllerConfig::default()
        };
        let rendered = config
            .provider_prompt_layers_with_persistent_injection()
            .render_or_legacy(config.system_prompt.as_str());

        assert_eq!(rendered, "system\n\n## Runtime Context\npersistent");
        assert_eq!(
            config.provider_rewrite_fingerprint(),
            provider_rewrite_fingerprint_for_system_prompt(&rendered)
        );
    }

    /// Verifies plain system text cannot collide with structurally injected persistent runtime context.
    /// 验证普通系统文本不会与结构化注入的持久运行时上下文发生指纹碰撞。
    #[test]
    fn persistent_runtime_context_does_not_collide_with_plain_system_suffix() {
        let injected = ControllerConfig {
            system_prompt: String::from("system"),
            prompt_layers: ProviderPromptLayers::from_system_prompt(String::from("system")),
            context_injection: Some(ProviderContextInjection {
                persistent_blocks: vec![String::from("persistent")],
                ..ProviderContextInjection::default()
            }),
            ..ControllerConfig::default()
        };
        let plain = ControllerConfig {
            system_prompt: String::from("system\n\npersistent"),
            prompt_layers: ProviderPromptLayers::from_system_prompt(String::from(
                "system\n\npersistent",
            )),
            ..ControllerConfig::default()
        };

        assert_ne!(
            injected.provider_rewrite_fingerprint(),
            plain.provider_rewrite_fingerprint()
        );
    }

    /// Verifies a cursor that covered VMM anchors cannot survive their disappearance from current input.
    /// 验证覆盖过 VMM 锚点的游标无法在当前输入中的锚点消失后继续复用。
    #[test]
    fn disappeared_anchor_epoch_forces_full_replay() {
        let config = ControllerConfig::default();
        let mut continuation = matching_continuation(&config);
        continuation.context_cache_epoch_id = Some(String::from("epoch-1"));
        continuation.context_anchor_revision = Some(1);

        assert!(!config.continuation_reuse_is_proven(&continuation, &[]));
    }

    /// Verifies anchored content cannot bypass suffix proof through an inconsistent mutation classification.
    /// 验证锚定内容无法通过不一致的变更分类绕过后缀证明。
    #[test]
    fn anchor_without_append_only_classification_is_rejected() {
        let mut config = ControllerConfig {
            context_injection: Some(ProviderContextInjection {
                anchored_prefixes: vec![ProviderContextAnchor {
                    anchor_id: String::from("anchor-1"),
                    target_message_id: String::from("user-2"),
                    prefix: String::from("memory"),
                }],
                mutation_kind: ProviderContextMutationKind::None,
                cache_epoch_id: Some(String::from("initial")),
                anchor_revision: Some(1),
                ..ProviderContextInjection::default()
            }),
            ..ControllerConfig::default()
        };
        let continuation = matching_continuation(&config);

        assert!(!config.continuation_reuse_is_proven(&continuation, &["user-2"]));

        config
            .context_injection
            .as_mut()
            .expect("context injection should exist")
            .mutation_kind = ProviderContextMutationKind::AppendOnly;
        // continuation must explicitly prove zero-anchor coverage in the current initial epoch.
        // continuation 必须显式证明当前初始纪元中的零锚点覆盖状态。
        let mut continuation = matching_continuation(&config);
        continuation.context_cache_epoch_id = Some(String::from("initial"));
        continuation.context_anchor_revision = Some(0);
        assert!(config.continuation_reuse_is_proven(&continuation, &["user-2"]));
    }
}
