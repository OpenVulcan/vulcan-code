//! Typed request-dynamic context state and synchronization inputs.
//! 请求动态上下文的强类型状态与同步输入。

use serde::{Deserialize, Serialize};

use crate::v1::event::SystemContextScope;
use crate::v1::ids::Seq;

/// The stable key space used by the V1 request dynamic-context bridge.
/// V1 请求动态上下文桥接使用的稳定键空间。
pub const DYNAMIC_CONTEXT_SYSTEM_KEY: &str = "dynamic.system";
/// The stable key for dynamic developer-layer content.
/// 动态 developer 层内容使用的稳定键。
pub const DYNAMIC_CONTEXT_DEVELOPER_KEY: &str = "dynamic.developer";
/// The stable key for dynamic tool guidance content.
/// 动态工具指导内容使用的稳定键。
pub const DYNAMIC_CONTEXT_TOOL_GUIDANCE_KEY: &str = "dynamic.tool_guidance";
/// The stable key for dynamic runtime/user-context content.
/// 动态运行时/用户上下文内容使用的稳定键。
pub const DYNAMIC_CONTEXT_RUNTIME_KEY: &str = "dynamic.runtime_context";
/// The stable key for persistent context-injection content.
/// 持久上下文注入内容使用的稳定键。
pub const DYNAMIC_CONTEXT_PERSISTENT_KEY: &str = "injection.persistent";
/// The stable key for transient context-injection content.
/// 临时上下文注入内容使用的稳定键。
pub const DYNAMIC_CONTEXT_TRANSIENT_KEY: &str = "injection.transient";

/// Selects whether one dynamic producer event creates or invalidates a value.
/// 选择动态生产者事件是创建值还是使值失效。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DynamicContextOperation {
    /// Creates or replaces the raw value for the producer key.
    /// 创建或替换生产者键对应的原始值。
    Upsert,
    /// Invalidates the same producer key after a typed occurrence match.
    /// 在类型化 occurrence 匹配后使同一生产者键失效。
    Remove,
}

/// Identifies one source occurrence without encoding identity in a free-form body.
/// 在不把身份编码到自由文本正文中的前提下标识一个来源 occurrence。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DynamicContextOccurrence {
    /// Stable source identity shared by the active block and its expiry fact.
    /// 活动块及其过期事实共享的稳定来源身份。
    pub source_id: String,
    /// Stable occurrence identity allocated by the source owner.
    /// 来源所有者分配的稳定 occurrence 身份。
    pub occurrence_id: String,
    /// Source-owned instruction or group tag.
    /// 来源拥有的指令或分组标签。
    pub tag: String,
    /// Stable hash of the expired instruction body.
    /// 已过期指令正文的稳定哈希。
    pub content_hash: String,
    /// Source logical turn identity, kept opaque by the domain.
    /// 来源逻辑回合身份由领域保持为不透明值。
    pub turn_id: String,
}

impl DynamicContextOccurrence {
    /// Builds one typed source occurrence from host-provided identity fields.
    /// 根据宿主提供的身份字段构建一个类型化来源 occurrence。
    pub fn new(
        source_id: impl Into<String>,
        occurrence_id: impl Into<String>,
        tag: impl Into<String>,
        content_hash: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            occurrence_id: occurrence_id.into(),
            tag: tag.into(),
            content_hash: content_hash.into(),
            turn_id: turn_id.into(),
        }
    }
}

/// Metadata state for the last emitted value of one dynamic-context producer.
/// 一个动态上下文生产者最近一次发出值的 metadata 状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DynamicContextState {
    /// Exact raw body stored in the corresponding immutable SystemContext event.
    /// 对应不可变 SystemContext 事件中保存的精确原始正文。
    pub raw_content: String,
    /// SHA-256 digest of the exact raw body used for idempotent comparison.
    /// 用于幂等比较的精确原始正文 SHA-256 摘要。
    pub fingerprint: String,
    /// Main-event sequence that emitted the current raw body.
    /// 发出当前原始正文的主事件序号。
    pub source_seq: Seq,
    /// Scope copied into the emitted SystemContext event.
    /// 复制到已发出 SystemContext 事件中的作用域。
    pub scope: SystemContextScope,
    /// Optional TTL copied into the emitted SystemContext event.
    /// 复制到已发出 SystemContext 事件中的可选 TTL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_turns: Option<u64>,
    /// Operation represented by the latest durable event for this producer.
    /// 该生产者最近一条持久事件所表示的操作。
    pub operation: DynamicContextOperation,
    /// Typed source occurrence attached to transient instruction events.
    /// 临时指令事件附带的类型化来源 occurrence。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<DynamicContextOccurrence>,
}

/// One desired producer value at a request preparation boundary.
/// 一次请求准备边界上某个生产者期望的值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicContextInput {
    /// Stable producer key used for metadata comparison and removal.
    /// 用于 metadata 比较与移除的稳定生产者键。
    pub producer_key: String,
    /// Raw body to emit; `None` requests one removal fact for the prior value.
    /// 要发出的原始正文；`None` 请求为旧值追加一条移除事实。
    pub raw_content: Option<String>,
    /// Scope for a newly emitted context event.
    /// 新发出上下文事件的作用域。
    pub scope: SystemContextScope,
    /// Optional TTL for a newly emitted context event.
    /// 新发出上下文事件的可选 TTL。
    pub ttl_turns: Option<u64>,
    /// Whether the raw body is an Upsert or a typed occurrence Remove.
    /// 原始正文表示 Upsert 还是类型化 occurrence Remove。
    pub operation: DynamicContextOperation,
    /// Typed source occurrence required for instruction-level Remove events.
    /// 指令级 Remove 事件要求的类型化来源 occurrence。
    pub occurrence: Option<DynamicContextOccurrence>,
}

impl DynamicContextInput {
    /// Builds one desired dynamic-context producer value.
    /// 构建一个期望的动态上下文生产者值。
    pub fn new(
        producer_key: impl Into<String>,
        raw_content: Option<String>,
        scope: SystemContextScope,
        ttl_turns: Option<u64>,
    ) -> Self {
        Self {
            producer_key: producer_key.into(),
            raw_content,
            scope,
            ttl_turns,
            operation: DynamicContextOperation::Upsert,
            occurrence: None,
        }
    }

    /// Marks this input as a typed occurrence removal while retaining its raw fact body.
    /// 将该输入标记为类型化 occurrence 移除，同时保留其原始事实正文。
    pub fn as_remove(mut self, occurrence: DynamicContextOccurrence) -> Self {
        self.operation = DynamicContextOperation::Remove;
        self.ttl_turns = None;
        self.occurrence = Some(occurrence);
        self
    }

    /// Attaches the source occurrence used to compare an Upsert version.
    /// 附加用于比较 Upsert 版本的来源 occurrence。
    pub fn with_occurrence(mut self, occurrence: DynamicContextOccurrence) -> Self {
        self.occurrence = Some(occurrence);
        self
    }
}
