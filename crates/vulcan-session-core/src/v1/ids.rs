//! V1 session-local identity model with JavaScript-safe bounds.
//! 具有 JavaScript 安全边界的 V1 会话本地身份模型。
//!
//! All session-internal entities (turn, api call, tool call, compaction,
//! pending input, project resource) share one monotonic local allocator and
//! are distinguished by Rust newtypes. External identities (session UUID,
//! provider native ids, terminal ids, subagent session ids) stay raw strings
//! and never become local primary keys.
//! 所有会话内部实体(回合、API 调用、工具调用、压缩、待处理输入、项目资源)
//! 共享一个单调本地分配器，并通过 Rust newtype 区分。外部身份(会话 UUID、
//! Provider 原生 id、终端 id、子代理会话 id)保持原始字符串，永远不作为本地主键。

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::v1::errors::DomainError;

/// Largest integer safely representable by JSON consumers (`2^53 - 1`).
/// JSON 消费者可安全表示的最大整数(`2^53 - 1`)。
pub const JS_SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;

/// Smallest legal value for every session-local id and seq.
/// 每个会话本地 id 与 seq 的最小合法值。
pub const LOCAL_ID_MIN: u64 = 1;

/// Maximum UTF-8 byte length of one external client action id.
/// 一个外部客户端动作 id 的最大 UTF-8 字节长度。
pub const CLIENT_ACTION_ID_MAX_BYTES: usize = 256;

/// Maximum UTF-8 byte length of one external subagent task identity.
/// 一个外部子代理任务身份的最大 UTF-8 字节长度。
pub const SUBAGENT_TASK_ID_MAX_BYTES: usize = 256;

macro_rules! define_local_id {
    ($(#[$doc:meta])* $name:ident, $entity:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(u64);

        impl $name {
            /// Constructs one validated id inside the JSON-safe range.
            /// 在 JSON 安全范围内构造一个经过校验的 id。
            pub fn try_new(value: u64) -> Result<Self, DomainError> {
                if !(LOCAL_ID_MIN..=JS_SAFE_INTEGER_MAX).contains(&value) {
                    return Err(DomainError::invalid_id($entity, value));
                }
                Ok(Self(value))
            }

            /// Returns the raw id value.
            /// 返回原始 id 值。
            pub fn value(self) -> u64 {
                self.0
            }
        }

        impl From<$name> for u64 {
            fn from(id: $name) -> u64 {
                id.value()
            }
        }
    };
}

define_local_id!(
    /// Contiguous stream position of one V1 session event.
    /// 一条 V1 会话事件的连续流位置。
    Seq,
    "seq"
);

/// Stable non-empty Provider stream-item identity used by one tool-call card.
/// 一张工具调用卡使用的稳定非空 Provider 流条目身份。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct RequestStreamId(String);

impl RequestStreamId {
    /// Constructs one exact identity without trimming or fallback normalization.
    /// 在不裁剪、不回退归一化的前提下构造一个精确身份。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty() || value.trim() != value {
            return Err(DomainError::InvalidToolIdentity {
                field: "request_stream_id",
            });
        }
        Ok(Self(value))
    }

    /// Returns the exact Provider stream identity.
    /// 返回精确的 Provider 流身份。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable external idempotency identity of one client submission action.
/// 一次客户端提交动作的稳定外部幂等身份。
///
/// This identity is persisted for RPC retry recognition but never enters the
/// session-local integer reference graph.
/// 该身份用于持久化识别 RPC 重试，但绝不进入会话本地整数引用图。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ClientActionId(String);

impl ClientActionId {
    /// Constructs one exact bounded client action identity.
    /// 构造一个精确且有界的客户端动作身份。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the browser-generated idempotency key and must be non-empty,
    /// free of leading/trailing whitespace, and no longer than 256 UTF-8 bytes.
    /// `value` 是浏览器生成的幂等键，必须非空、没有首尾空白，且不超过 256 个 UTF-8 字节。
    ///
    /// # Returns
    /// 返回值
    /// Returns the validated identity or a structured domain error.
    /// 返回经过校验的身份，或结构化领域错误。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty()
            || value.trim() != value
            || value.len() > CLIENT_ACTION_ID_MAX_BYTES
        {
            return Err(DomainError::InvalidClientActionId {
                max_bytes: CLIENT_ACTION_ID_MAX_BYTES,
            });
        }
        Ok(Self(value))
    }

    /// Returns the exact external idempotency key.
    /// 返回精确的外部幂等键。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ClientActionId {
    /// Deserializes and validates one persisted client action identity.
    /// 反序列化并校验一个持久化客户端动作身份。
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(DeserializerType::Error::custom)
    }
}

impl<'de> Deserialize<'de> for RequestStreamId {
    /// Deserializes and validates one persisted Provider stream identity.
    /// 反序列化并校验一个持久化 Provider 流身份。
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(DeserializerType::Error::custom)
    }
}

/// Stable external task identity used to deduplicate one subagent return.
/// 用于对子代理回流去重的稳定外部任务身份。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct SubagentTaskId(String);

impl SubagentTaskId {
    /// Constructs one exact bounded task identity without normalization.
    /// 构造一个不做归一化的精确有界任务身份。
    pub fn try_new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty()
            || value.trim() != value
            || value.len() > SUBAGENT_TASK_ID_MAX_BYTES
        {
            return Err(DomainError::InvalidStatistics {
                field: "subagent_task_id",
            });
        }
        Ok(Self(value))
    }

    /// Returns the exact external task identity.
    /// 返回精确的外部任务身份。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for SubagentTaskId {
    /// Deserializes and validates one persisted subagent task identity.
    /// 反序列化并校验一个持久化子代理任务身份。
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_new(value).map_err(DeserializerType::Error::custom)
    }
}

define_local_id!(
    /// Identity of one logical user turn.
    /// 一个逻辑用户回合的身份。
    TurnId,
    "turn_id"
);

define_local_id!(
    /// Identity of one physical Provider API call.
    /// 一次物理 Provider API 调用的身份。
    ApiCallId,
    "api_call_id"
);

define_local_id!(
    /// Identity of one tool invocation inside an API group.
    /// 一个 API 组内一次工具调用的身份。
    ToolCallId,
    "tool_call_id"
);

define_local_id!(
    /// Identity of one context-compaction node.
    /// 一个上下文压缩节点的身份。
    CompactionId,
    "compaction_id"
);

define_local_id!(
    /// Identity of one user input that is pending, queued, or already streamed.
    /// 一条待处理、排队或已流入主事件的用户输入的身份。
    PendingInputId,
    "pending_input_id"
);

define_local_id!(
    /// Project-scoped identity of one reusable resource entity.
    /// 一个可复用资源实体的项目级身份。
    ResourceId,
    "resource_id"
);

/// Monotonic allocator for session-local ids and stream seq values.
/// 会话本地 id 与流 seq 值的单调分配器。
///
/// `seq` advances on its own counter; all other local entities share a second
/// counter. Both counters are cached in `metadata.id_frontier` and must be
/// revalidated against the active tail on load (see the plan section 5.2.16).
/// `seq` 使用独立计数器推进；其余本地实体共享第二个计数器。两个计数器在
/// `metadata.id_frontier` 中缓存，加载时必须与活动尾段重新校验(见计划 5.2.16)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdAllocator {
    /// Next available local entity id.
    /// 下一个可用本地实体 id。
    next_local_id: u64,
    /// Next available stream seq.
    /// 下一个可用流 seq。
    next_seq: u64,
}

impl IdAllocator {
    /// Creates a fresh allocator starting both counters at `1`.
    /// 创建一个两个计数器都从 `1` 开始的全新分配器。
    pub fn new() -> Self {
        Self {
            next_local_id: LOCAL_ID_MIN,
            next_seq: LOCAL_ID_MIN,
        }
    }

    /// Restores an allocator from a persisted frontier cache.
    /// 从持久化的 frontier 缓存恢复一个分配器。
    ///
    /// Callers must still verify the cached values against stream facts and
    /// take the maximum observed id/seq plus one before trusting them.
    /// 调用方仍必须把缓存值与流事实比对，取已观察到的最大 id/seq 加一后再信任。
    pub fn from_frontier(next_local_id: u64, next_seq: u64) -> Self {
        Self {
            next_local_id,
            next_seq,
        }
    }

    /// Returns the current next local id and next seq frontier.
    /// 返回当前 next local id 与 next seq 前沿。
    pub fn frontier(&self) -> (u64, u64) {
        (self.next_local_id, self.next_seq)
    }

    /// Allocates the next session-local entity id (validated JSON-safe).
    /// 分配下一个会话本地实体 id(校验 JSON 安全)。
    pub fn next_local_id(&mut self) -> Result<u64, DomainError> {
        let value = self.next_local_id;
        if !(LOCAL_ID_MIN..=JS_SAFE_INTEGER_MAX).contains(&value) {
            return Err(DomainError::invalid_id("local_id", value));
        }
        self.next_local_id = value + 1;
        Ok(value)
    }

    /// Allocates the next contiguous stream seq.
    /// 分配下一个连续流 seq。
    pub fn next_seq(&mut self) -> Result<Seq, DomainError> {
        let value = self.next_seq;
        if !(LOCAL_ID_MIN..=JS_SAFE_INTEGER_MAX).contains(&value) {
            return Err(DomainError::invalid_id("seq", value));
        }
        self.next_seq = value + 1;
        Ok(Seq(value))
    }

    /// Advances both counters beyond one retained frontier (fork/edit).
    /// 在保留前沿之后推进两个计数器(分叉/编辑)。
    ///
    /// The next allocated seq continues from `after_seq + 1` and the next
    /// local id from `after_local_id + 1`.
    /// 下一个分配的 seq 从 `after_seq + 1` 继续，下一个本地 id 从
    /// `after_local_id + 1` 继续。
    pub fn advance_beyond(
        &mut self,
        after_seq: u64,
        after_local_id: u64,
    ) -> Result<(), DomainError> {
        let next_seq = after_seq
            .checked_add(1)
            .ok_or_else(|| DomainError::invalid_id("seq", after_seq))?;
        let next_local_id = after_local_id
            .checked_add(1)
            .ok_or_else(|| DomainError::invalid_id("local_id", after_local_id))?;
        if next_seq > JS_SAFE_INTEGER_MAX || next_local_id > JS_SAFE_INTEGER_MAX {
            return Err(DomainError::invalid_id("seq", next_seq.max(next_local_id)));
        }
        self.next_seq = next_seq;
        self.next_local_id = next_local_id;
        Ok(())
    }
}

impl Default for IdAllocator {
    /// Default allocator starts both counters at `1`.
    /// 默认分配器让两个计数器都从 `1` 开始。
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seq and every local id accept the legal range and reject zero and
    /// values beyond the JavaScript-safe maximum.
    /// seq 与每个本地 id 接受合法范围，拒绝零与超过 JavaScript 安全上限的值。
    #[test]
    fn id_range_validation() {
        for (make, entity) in id_constructors() {
            assert!(make(0).is_err(), "{entity}: zero must be rejected");
            assert!(make(1).is_ok(), "{entity}: one must be accepted");
            assert!(
                make(JS_SAFE_INTEGER_MAX).is_ok(),
                "{entity}: js-safe max must be accepted"
            );
            assert!(
                make(JS_SAFE_INTEGER_MAX + 1).is_err(),
                "{entity}: beyond js-safe max must be rejected"
            );
        }
    }

    /// Collects one checked constructor per id kind for shared validation.
    /// 为共享校验收集每种 id 种类的一个受检构造函数。
    fn id_constructors() -> Vec<(fn(u64) -> Result<u64, ()>, &'static str)> {
        vec![
            (|v| Seq::try_new(v).map(|_| v).map_err(|_| ()), "seq"),
            (|v| TurnId::try_new(v).map(|_| v).map_err(|_| ()), "turn_id"),
            (
                |v| ApiCallId::try_new(v).map(|_| v).map_err(|_| ()),
                "api_call_id",
            ),
            (
                |v| ToolCallId::try_new(v).map(|_| v).map_err(|_| ()),
                "tool_call_id",
            ),
            (
                |v| CompactionId::try_new(v).map(|_| v).map_err(|_| ()),
                "compaction_id",
            ),
            (
                |v| PendingInputId::try_new(v).map(|_| v).map_err(|_| ()),
                "pending_input_id",
            ),
            (
                |v| ResourceId::try_new(v).map(|_| v).map_err(|_| ()),
                "resource_id",
            ),
        ]
    }

    /// The allocator produces strictly contiguous seq values starting at 1.
    /// 分配器产生从 1 开始的严格连续 seq 值。
    #[test]
    fn allocator_seq_is_contiguous() {
        let mut allocator = IdAllocator::new();
        let mut previous = 0u64;
        for _ in 0..10_000 {
            let seq = allocator.next_seq().unwrap();
            assert_eq!(u64::from(seq), previous + 1);
            previous = u64::from(seq);
        }
    }

    /// The local id allocator advances independently of seq.
    /// 本地 id 分配器独立于 seq 推进。
    #[test]
    fn allocator_local_ids_advance_independently() {
        let mut allocator = IdAllocator::new();
        let _ = allocator.next_seq().unwrap();
        assert_eq!(allocator.next_local_id().unwrap(), 1);
        assert_eq!(allocator.next_local_id().unwrap(), 2);
        assert_eq!(allocator.next_seq().unwrap().value(), 2);
    }

    /// The frontier cache round-trips through serialized values.
    /// frontier 缓存通过序列化值往返一致。
    #[test]
    fn frontier_round_trip() {
        let mut allocator = IdAllocator::new();
        let _ = allocator.next_seq().unwrap();
        let _ = allocator.next_local_id().unwrap();
        let (local_id, seq) = allocator.frontier();
        let restored = IdAllocator::from_frontier(local_id, seq);
        assert_eq!(restored.frontier(), (local_id, seq));
    }
}
