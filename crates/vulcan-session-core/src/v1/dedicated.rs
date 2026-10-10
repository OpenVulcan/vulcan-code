//! Dedicated session profile: creation-frozen settings and the memory-lock spans.
//! 专职会话配置：创建时冻结的设置与记忆锁定区间。
//!
//! A dedicated session is a separate session kind chosen only at creation. Its
//! base prompt, tool access, answer mode, output style and knowledge selection
//! never change afterwards, and it can never become an ordinary session. While
//! its memory is locked, every closed turn started inside a locked span is kept
//! for display but excluded from later Provider requests.
//! 专职会话是只能在创建时选择的独立会话种类。其基础提示词、工具开放、回答模式、
//! 输出风格与知识库选择之后不再变化，也永远不能转换为普通会话。记忆锁定期间，
//! 在锁定区间内开始且已闭合的回合保留显示，但不进入之后的 Provider 请求。

use serde::{Deserialize, Serialize};

use super::errors::DomainError;
use super::event::{ConversationEvent, EventBody};
use super::ids::{Seq, TurnId};

/// Maximum base-prompt length in Unicode scalar values; the single definition of this bound.
/// 基础提示词的最大 Unicode 标量长度；该边界的唯一定义。
pub const DEDICATED_BASE_PROMPT_MAX_CHARS: usize = 32_000;

/// Whether a dedicated session exposes the general tool surface.
/// 专职会话是否开放通用工具面。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DedicatedToolAccess {
    /// No general tools; only creation-enabled knowledge tools may remain.
    /// 不开放通用工具；只可保留创建时启用的知识库工具。
    None,
    /// General tools without sub-agent or collaboration tools.
    /// 通用工具，但不含子代理与协作工具。
    Standard,
}

impl DedicatedToolAccess {
    /// Returns the serialized wire name.
    /// 返回序列化传输名称。
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Standard => "standard",
        }
    }
}

/// One memory-lock span; turns started inside it are forgotten once closed.
/// 一个记忆锁定区间；在其中开始的回合闭合后即被遗忘。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DedicatedLockedSpan {
    /// Seq of the lock timeline note that opened the span.
    /// 开启该区间的锁定时间线通知 seq。
    pub locked_at_seq: Seq,
    /// Seq of the unlock timeline note that closed the span; `None` while still locked.
    /// 关闭该区间的解锁时间线通知 seq；仍处于锁定时为 `None`。
    #[serde(deserialize_with = "deserialize_required_nullable_seq")]
    pub unlocked_at_seq: Option<Seq>,
}

impl DedicatedLockedSpan {
    /// Returns whether one seq lies inside this span (the closing note itself is outside).
    /// 返回某 seq 是否位于本区间内（关闭通知本身不在区间内）。
    pub fn contains(&self, seq: Seq) -> bool {
        seq > self.locked_at_seq && self.unlocked_at_seq.is_none_or(|end| seq < end)
    }
}

/// Decode a present nullable seq; a missing field stays a schema error.
/// 解码存在且可为空的 seq；缺失字段仍是模式错误。
fn deserialize_required_nullable_seq<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Seq>, D::Error> {
    Option::deserialize(deserializer)
}

/// Creation-frozen dedicated-session settings plus the realtime memory-lock spans.
/// 创建时冻结的专职会话设置及实时记忆锁定区间。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DedicatedSessionProfile {
    /// General tool access chosen at creation.
    /// 创建时选择的通用工具开放方式。
    pub tool_access: DedicatedToolAccess,
    /// User-authored base prompt; never edited after creation.
    /// 用户编写的基础提示词；创建后不再修改。
    pub base_prompt: String,
    /// Ordered, non-overlapping lock spans; only the last one may still be open.
    /// 有序且互不重叠的锁定区间；只有最后一个可以仍然开放。
    pub locked_spans: Vec<DedicatedLockedSpan>,
}

impl DedicatedSessionProfile {
    /// Builds a validated profile with no lock spans yet.
    /// 构建尚无锁定区间的已校验配置。
    pub fn new(
        tool_access: DedicatedToolAccess,
        base_prompt: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let profile = Self {
            tool_access,
            base_prompt: base_prompt.into(),
            locked_spans: Vec::new(),
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Returns whether the memory is currently locked.
    /// 返回记忆当前是否处于锁定状态。
    pub fn is_locked(&self) -> bool {
        self.locked_spans
            .last()
            .is_some_and(|span| span.unlocked_at_seq.is_none())
    }

    /// Returns the 1-based ordinal of the current or next lock.
    /// 返回当前或下一次锁定的序号（从 1 开始）。
    pub fn lock_count(&self) -> usize {
        self.locked_spans.len()
    }

    /// Returns whether one turn starting at `start_seq` is excluded from memory.
    /// 返回在 `start_seq` 开始的回合是否被排除在记忆之外。
    pub fn excludes_turn_start(&self, start_seq: Seq) -> bool {
        self.locked_spans
            .iter()
            .any(|span| span.contains(start_seq))
    }

    /// Validates the prompt bound and the span ordering invariants.
    /// 校验提示词边界与区间顺序不变量。
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.base_prompt.trim().is_empty()
            || self.base_prompt.chars().count() > DEDICATED_BASE_PROMPT_MAX_CHARS
        {
            return Err(DomainError::InvalidDedicatedProfile {
                field: "base_prompt",
            });
        }
        let mut previous_end: Option<Seq> = None;
        for (index, span) in self.locked_spans.iter().enumerate() {
            let is_last = index + 1 == self.locked_spans.len();
            if previous_end.is_some_and(|end| span.locked_at_seq <= end) {
                return Err(DomainError::InvalidDedicatedProfile {
                    field: "locked_spans.order",
                });
            }
            match span.unlocked_at_seq {
                Some(end) if end <= span.locked_at_seq => {
                    return Err(DomainError::InvalidDedicatedProfile {
                        field: "locked_spans.range",
                    });
                }
                Some(end) => previous_end = Some(end),
                None if !is_last => {
                    return Err(DomainError::InvalidDedicatedProfile {
                        field: "locked_spans.open",
                    });
                }
                None => {}
            }
        }
        Ok(())
    }
}

/// Collects the closed turns whose start lies inside a lock span, given the still-open turn.
/// 根据仍开放的回合，收集起点位于锁定区间内且已闭合的回合。
fn forgotten_turns(
    profile: &DedicatedSessionProfile,
    events: &[ConversationEvent],
    open_turn: Option<TurnId>,
) -> std::collections::BTreeSet<TurnId> {
    let mut seen = std::collections::BTreeSet::new();
    let mut forgotten = std::collections::BTreeSet::new();
    for event in events {
        let Some(turn_id) = event.turn_id else {
            continue;
        };
        // The first event of a turn in seq order is its start.
        // 按 seq 顺序，回合的第一条事件即其起点。
        if seen.insert(turn_id)
            && Some(turn_id) != open_turn
            && profile.excludes_turn_start(event.seq)
        {
            forgotten.insert(turn_id);
        }
    }
    forgotten
}

/// Returns the events a dedicated session may project to the Provider.
/// 返回专职会话可以投影给 Provider 的事件。
///
/// Events owned by forgotten turns are dropped as whole turns so API-group
/// structure stays intact. System-context semantics are always kept: they are
/// session-level instructions whose idempotent synchronization would otherwise
/// never re-emit them. The still-open turn is always kept in full.
/// 被遗忘回合所拥有的事件按整回合丢弃，以保持 API 组结构完整。系统上下文语义
/// 始终保留：它们是会话级指令，其幂等同步不会再次发出。仍开放的回合始终完整保留。
pub fn dedicated_provider_events(
    profile: &DedicatedSessionProfile,
    events: &[ConversationEvent],
    open_turn: Option<TurnId>,
) -> Vec<ConversationEvent> {
    let forgotten = forgotten_turns(profile, events, open_turn);
    events
        .iter()
        .filter(|event| {
            matches!(event.event, EventBody::SystemContext(_))
                || event
                    .turn_id
                    .is_none_or(|turn_id| !forgotten.contains(&turn_id))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one seq from a literal known to be valid.
    /// 用已知合法的字面量构建 seq。
    fn seq(value: u64) -> Seq {
        Seq::try_new(value).unwrap()
    }

    /// Builds a profile with explicit spans.
    /// 构建带显式区间的配置。
    fn profile_with(spans: Vec<DedicatedLockedSpan>) -> DedicatedSessionProfile {
        DedicatedSessionProfile {
            tool_access: DedicatedToolAccess::None,
            base_prompt: "translate".to_string(),
            locked_spans: spans,
        }
    }

    /// Span containment excludes both lock notes and includes the seqs between them.
    /// 区间包含关系排除两端通知，包含其间的 seq。
    #[test]
    fn span_contains_only_seqs_between_notes() {
        let span = DedicatedLockedSpan {
            locked_at_seq: seq(5),
            unlocked_at_seq: Some(seq(9)),
        };
        assert!(!span.contains(seq(5)));
        assert!(span.contains(seq(6)));
        assert!(span.contains(seq(8)));
        assert!(!span.contains(seq(9)));
        let open = DedicatedLockedSpan {
            locked_at_seq: seq(5),
            unlocked_at_seq: None,
        };
        assert!(open.contains(seq(1_000)));
    }

    /// Validation rejects an empty prompt, overlapping spans, and an open span before the last.
    /// 校验拒绝空提示词、重叠区间以及非末尾的开放区间。
    #[test]
    fn validation_rejects_malformed_profiles() {
        assert!(DedicatedSessionProfile::new(DedicatedToolAccess::None, "  ").is_err());
        let too_long = "x".repeat(DEDICATED_BASE_PROMPT_MAX_CHARS + 1);
        assert!(DedicatedSessionProfile::new(DedicatedToolAccess::None, too_long).is_err());
        let open_then_closed = profile_with(vec![
            DedicatedLockedSpan {
                locked_at_seq: seq(2),
                unlocked_at_seq: None,
            },
            DedicatedLockedSpan {
                locked_at_seq: seq(4),
                unlocked_at_seq: Some(seq(6)),
            },
        ]);
        assert!(open_then_closed.validate().is_err());
        let overlapping = profile_with(vec![
            DedicatedLockedSpan {
                locked_at_seq: seq(2),
                unlocked_at_seq: Some(seq(6)),
            },
            DedicatedLockedSpan {
                locked_at_seq: seq(6),
                unlocked_at_seq: None,
            },
        ]);
        assert!(overlapping.validate().is_err());
        let valid = profile_with(vec![
            DedicatedLockedSpan {
                locked_at_seq: seq(2),
                unlocked_at_seq: Some(seq(6)),
            },
            DedicatedLockedSpan {
                locked_at_seq: seq(9),
                unlocked_at_seq: None,
            },
        ]);
        assert!(valid.validate().is_ok());
        assert!(valid.is_locked());
        assert_eq!(valid.lock_count(), 2);
    }

    /// The profile wire form requires every field, including a null open span end.
    /// 配置传输形态要求所有字段，包括开放区间为 null 的终点。
    #[test]
    fn profile_wire_form_requires_every_field() {
        let profile = profile_with(vec![DedicatedLockedSpan {
            locked_at_seq: seq(3),
            unlocked_at_seq: None,
        }]);
        let json = serde_json::to_value(&profile).unwrap();
        assert_eq!(json["tool_access"], "none");
        assert!(json["locked_spans"][0]["unlocked_at_seq"].is_null());
        let decoded: DedicatedSessionProfile = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(decoded, profile);
        let mut missing = json;
        missing["locked_spans"][0]
            .as_object_mut()
            .unwrap()
            .remove("unlocked_at_seq");
        assert!(serde_json::from_value::<DedicatedSessionProfile>(missing).is_err());
    }
}
