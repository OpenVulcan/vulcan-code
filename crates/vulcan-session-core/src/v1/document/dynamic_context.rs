//! Atomic synchronization of request-dynamic context events and metadata.
//! 请求动态上下文事件与 metadata 的原子同步。

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::{IdempotentMutation, SessionDocument};
use crate::v1::dynamic_context::{
    DynamicContextInput, DynamicContextOccurrence, DynamicContextOperation, DynamicContextState,
};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    EventBody, EventVisibility, SystemContextEvent, SystemContextOperation, SystemContextScope,
    SystemRoleHint,
};
use crate::v1::ids::{IdAllocator, Seq, TurnId};

use super::persistence::DirtyFile;

/// The fixed role hint for request-dynamic context events.
/// 请求动态上下文事件使用的固定角色提示。
const DYNAMIC_CONTEXT_ROLE: SystemRoleHint = SystemRoleHint::HiddenUserShell;

/// Computes the canonical lowercase SHA-256 fingerprint for one raw body.
/// 计算一份原始正文的规范小写 SHA-256 指纹。
fn fingerprint(raw_content: &str) -> String {
    Sha256::digest(raw_content.as_bytes())
        .iter()
        .flat_map(|byte| {
            [
                char::from_digit(u32::from(byte >> 4), 16)
                    .expect("a four-bit nibble is always hexadecimal"),
                char::from_digit(u32::from(byte & 0x0f), 16)
                    .expect("a four-bit nibble is always hexadecimal"),
            ]
        })
        .collect()
}

/// Validates one producer key used by the dynamic-context metadata map.
/// 校验动态上下文 metadata 映射使用的生产者键。
fn validate_producer_key(producer_key: &str) -> Result<(), DomainError> {
    if producer_key.trim().is_empty() || producer_key.trim() != producer_key {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "dynamic_context",
            field: "producer_key",
        });
    }
    Ok(())
}

/// Requires the stable producer, content-version, and turn markers emitted by runtime expiry facts.
/// 要求 runtime 失效事实正文包含稳定生产者、内容版本和回合标记。
fn validate_occurrence(occurrence: &DynamicContextOccurrence) -> Result<(), DomainError> {
    for value in [
        occurrence.source_id.as_str(),
        occurrence.occurrence_id.as_str(),
        occurrence.tag.as_str(),
        occurrence.content_hash.as_str(),
        occurrence.turn_id.as_str(),
    ] {
        if value.trim().is_empty() || value.trim() != value {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "dynamic_context",
                field: "occurrence",
            });
        }
    }
    Ok(())
}

/// Validates one persisted dynamic-context metadata state.
/// 校验一条持久化动态上下文 metadata 状态。
pub fn validate_dynamic_context_state(
    producer_key: &str,
    state: &DynamicContextState,
) -> Result<(), DomainError> {
    validate_producer_key(producer_key)?;
    if state.raw_content.trim().is_empty() || state.raw_content.trim() != state.raw_content {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "dynamic_context",
            field: "raw_content",
        });
    }
    if state.fingerprint != fingerprint(&state.raw_content)
        || state.fingerprint.len() != 64
        || !state
            .fingerprint
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(DomainError::ImmutableFieldMutation {
            entity: "dynamic_context",
            field: "fingerprint",
        });
    }
    match state.operation {
        DynamicContextOperation::Upsert => {
            if state.ttl_turns == Some(0) {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "dynamic_context",
                    field: "ttl_turns",
                });
            }
            if state.scope == SystemContextScope::Turn && state.ttl_turns != Some(1) {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "dynamic_context",
                    field: "turn_scope_ttl_turns",
                });
            }
        }
        DynamicContextOperation::Remove => {
            if state.ttl_turns.is_some() || state.occurrence.is_none() {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "dynamic_context",
                    field: "remove_state",
                });
            }
        }
    }
    if let Some(occurrence) = state.occurrence.as_ref() {
        validate_occurrence(occurrence)?;
    }
    Ok(())
}

/// Validates the complete persisted dynamic-context map.
/// 校验完整的持久化动态上下文映射。
pub fn validate_dynamic_context_map(
    states: &BTreeMap<String, DynamicContextState>,
) -> Result<(), DomainError> {
    for (producer_key, state) in states {
        validate_dynamic_context_state(producer_key, state)?;
    }
    Ok(())
}

/// Compares desired producer values, appends changed facts, and updates metadata atomically.
/// 比较生产者期望值、追加变更事实并原子更新 metadata。
impl SessionDocument {
    /// Synchronizes all request-dynamic producer values at one legal API boundary.
    /// 在一个合法 API 边界同步全部请求动态生产者值。
    ///
    /// `None` values remove a producer by appending a SystemContext removal fact. Existing
    /// event bodies remain immutable; only the metadata accelerator is replaced with the latest
    /// emitted states.
    /// Expiration-prefixed inputs append raw Remove facts and retain their metadata for replay
    /// deduplication, because a later omission must not create a second expiration fact.
    /// 失效前缀输入追加原始 Remove 事实并保留 metadata 用于重放去重，因为后续省略不能再创建第二条失效事实。
    /// `None` 值通过追加 SystemContext 移除事实移除生产者。既有事件正文保持不可变；只替换
    /// metadata 中保存最近发出状态的加速映射。
    pub fn synchronize_dynamic_context(
        &mut self,
        inputs: Vec<DynamicContextInput>,
        now_ms: i64,
    ) -> Result<IdempotentMutation<Vec<Seq>>, DomainError> {
        super::validate_timestamp("dynamic_context.updated_at_ms", now_ms)?;

        let mut desired = BTreeMap::<String, DynamicContextInput>::new();
        for input in inputs {
            validate_producer_key(&input.producer_key)?;
            if input.operation == DynamicContextOperation::Upsert {
                if input.ttl_turns == Some(0) {
                    return Err(DomainError::ImmutableFieldMutation {
                        entity: "dynamic_context",
                        field: "ttl_turns",
                    });
                }
                if input.scope == SystemContextScope::Turn && input.ttl_turns != Some(1) {
                    return Err(DomainError::ImmutableFieldMutation {
                        entity: "dynamic_context",
                        field: "turn_scope_ttl_turns",
                    });
                }
            }
            if let Some(raw_content) = &input.raw_content
                && (raw_content.trim().is_empty() || raw_content.trim() != raw_content)
            {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "dynamic_context",
                    field: "raw_content",
                });
            }
            if let Some(occurrence) = input.occurrence.as_ref() {
                validate_occurrence(occurrence)?;
                if occurrence.source_id != input.producer_key {
                    return Err(DomainError::ReferenceViolation {
                        kind: "dynamic_context.occurrence_source",
                        value: 0,
                    });
                }
            }
            if input.operation == DynamicContextOperation::Remove
                && (input.raw_content.is_none()
                    || input.ttl_turns.is_some()
                    || input.occurrence.is_none())
            {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "dynamic_context",
                    field: "remove_input",
                });
            }
            if desired.insert(input.producer_key.clone(), input).is_some() {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "dynamic_context",
                    field: "duplicate_producer_key",
                });
            }
        }
        validate_dynamic_context_map(&self.metadata.dynamic_context)?;

        let previous = self.metadata.dynamic_context.clone();
        for (producer_key, input) in &desired {
            if input.operation == DynamicContextOperation::Upsert
                && input.raw_content.is_some()
                && input.occurrence.is_some()
                && previous.get(producer_key).is_some_and(|state| {
                    state.operation == DynamicContextOperation::Remove
                        && state.occurrence == input.occurrence
                })
            {
                let source_seq = previous
                    .get(producer_key)
                    .expect("checked previous state")
                    .source_seq;
                return Err(DomainError::ReferenceViolation {
                    kind: "dynamic_context.resurrected_occurrence",
                    value: source_seq.value(),
                });
            }
        }
        let active_turn_id = self
            .active_turn()
            .filter(|turn| !turn.closed)
            .map(|turn| turn.turn_id);
        let mut changed_event_count = 0_u64;
        for (producer_key, input) in &desired {
            let changed = match input.operation {
                DynamicContextOperation::Upsert => {
                    // An absent value is handled by the removal pass, not by Upsert allocation.
                    // 空值由移除阶段处理，不参与 Upsert 分配。
                    let Some(raw_content) = input.raw_content.as_ref() else {
                        continue;
                    };
                    let next_fingerprint = fingerprint(raw_content);
                    !previous.get(producer_key).is_some_and(|state| {
                        state.operation == DynamicContextOperation::Upsert
                            && state.raw_content == *raw_content
                            && state.fingerprint == next_fingerprint
                            && state.scope == input.scope
                            && state.ttl_turns == input.ttl_turns
                            && state.occurrence == input.occurrence
                    })
                }
                DynamicContextOperation::Remove => {
                    !remove_is_idempotent(self, producer_key, input, previous.get(producer_key))?
                }
            };
            if changed {
                changed_event_count = changed_event_count.checked_add(1).ok_or_else(|| {
                    DomainError::invalid_id("dynamic_context_event_count", u64::MAX)
                })?;
            }
        }
        for producer_key in previous.keys() {
            if desired
                .get(producer_key)
                .is_none_or(|input| input.raw_content.is_none())
            {
                if previous
                    .get(producer_key)
                    .is_some_and(|state| state.operation == DynamicContextOperation::Remove)
                {
                    continue;
                }
                changed_event_count = changed_event_count.checked_add(1).ok_or_else(|| {
                    DomainError::invalid_id("dynamic_context_event_count", u64::MAX)
                })?;
            }
        }
        self.ensure_transition_capacity(changed_event_count, 0)?;
        if changed_event_count > 0 {
            self.memory_revision.checked_add(1).ok_or_else(|| {
                DomainError::invalid_id("document_revision", self.memory_revision)
            })?;
        }
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::new();
        let mut next_states = BTreeMap::new();

        for (producer_key, input) in &desired {
            let Some(raw_content) = input.raw_content.as_ref() else {
                continue;
            };
            let next_fingerprint = fingerprint(raw_content);
            if input.operation == DynamicContextOperation::Remove
                && remove_is_idempotent(self, producer_key, input, previous.get(producer_key))?
            {
                if let Some(previous_state) = previous.get(producer_key) {
                    next_states.insert(producer_key.clone(), previous_state.clone());
                }
                continue;
            }
            let operation = match input.operation {
                DynamicContextOperation::Upsert => SystemContextOperation::Upsert,
                DynamicContextOperation::Remove => SystemContextOperation::Remove,
            };
            let unchanged = input.operation == DynamicContextOperation::Upsert
                && previous.get(producer_key).is_some_and(|state| {
                    state.operation == DynamicContextOperation::Upsert
                        && state.raw_content == *raw_content
                        && state.fingerprint == next_fingerprint
                        && state.scope == input.scope
                        && state.ttl_turns == input.ttl_turns
                        && state.occurrence == input.occurrence
                });
            if unchanged {
                next_states.insert(
                    producer_key.clone(),
                    previous
                        .get(producer_key)
                        .expect("unchanged state exists")
                        .clone(),
                );
                continue;
            }
            let event_ttl_turns = match operation {
                SystemContextOperation::Upsert => input.ttl_turns,
                SystemContextOperation::Remove => None,
            };
            let seq = self.stage_dynamic_context_event(
                &mut staged_allocator,
                &mut staged_events,
                event_turn_id(self, active_turn_id, input.scope)?,
                producer_key,
                operation,
                input.scope,
                event_ttl_turns,
                raw_content.clone(),
                now_ms,
            )?;
            next_states.insert(
                producer_key.clone(),
                DynamicContextState {
                    raw_content: raw_content.clone(),
                    fingerprint: next_fingerprint,
                    source_seq: seq,
                    scope: input.scope,
                    ttl_turns: input.ttl_turns,
                    operation: input.operation,
                    occurrence: input.occurrence.clone(),
                },
            );
        }

        for (producer_key, state) in &previous {
            if desired
                .get(producer_key)
                .is_some_and(|input| input.raw_content.is_some())
            {
                continue;
            }
            let Some(input) = desired.get(producer_key) else {
                if state.operation == DynamicContextOperation::Remove {
                    // Typed removals retain their tombstone for replay deduplication.
                    // 类型化移除保留 tombstone 以便重放去重。
                    next_states.insert(producer_key.clone(), state.clone());
                    continue;
                }
                // A producer omitted from the desired set is removed exactly once.
                // 从期望集合省略的生产者只追加一次移除事实。
                let _seq = self.stage_dynamic_context_event(
                    &mut staged_allocator,
                    &mut staged_events,
                    event_turn_id(self, active_turn_id, state.scope)?,
                    producer_key,
                    SystemContextOperation::Remove,
                    state.scope,
                    None,
                    invalidation_content(state.source_seq),
                    now_ms,
                )?;
                continue;
            };
            if input.raw_content.is_none() {
                if state.operation == DynamicContextOperation::Remove {
                    next_states.insert(producer_key.clone(), state.clone());
                    continue;
                }
                let _seq = self.stage_dynamic_context_event(
                    &mut staged_allocator,
                    &mut staged_events,
                    event_turn_id(self, active_turn_id, state.scope)?,
                    producer_key,
                    SystemContextOperation::Remove,
                    state.scope,
                    None,
                    invalidation_content(state.source_seq),
                    now_ms,
                )?;
            }
        }

        if staged_events.is_empty() && next_states == previous {
            return Ok(IdempotentMutation::Idempotent(Vec::new()));
        }

        let changed_event_seqs: Vec<Seq> = staged_events.iter().map(|event| event.seq).collect();
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        self.metadata.dynamic_context = next_states;
        self.bump_revision(
            &[DirtyFile::ActiveTail, DirtyFile::Metadata],
            now_ms,
            &changed_event_seqs,
        );
        Ok(IdempotentMutation::Committed(changed_event_seqs))
    }

    /// Stages one raw dynamic-context event without publishing partial state.
    /// 暂存一条原始动态上下文事件且不发布部分状态。
    #[allow(clippy::too_many_arguments)]
    fn stage_dynamic_context_event(
        &self,
        staged_allocator: &mut IdAllocator,
        staged_events: &mut Vec<crate::v1::event::ConversationEvent>,
        turn_id: Option<crate::v1::ids::TurnId>,
        producer_key: &str,
        operation: SystemContextOperation,
        scope: SystemContextScope,
        ttl_turns: Option<u64>,
        raw_content: String,
        now_ms: i64,
    ) -> Result<Seq, DomainError> {
        self.stage_event(
            staged_allocator,
            staged_events,
            turn_id,
            EventVisibility::Provider,
            EventBody::SystemContext(SystemContextEvent {
                context_key: producer_key.to_string(),
                operation,
                scope,
                ttl_turns,
                provider_role_hint: Some(DYNAMIC_CONTEXT_ROLE),
                content: raw_content,
            }),
            now_ms,
        )
    }
}

/// Checks whether a typed removal already has the exact durable occurrence result.
/// 检查类型化移除是否已经有精确的持久 occurrence 结果。
fn remove_is_idempotent(
    document: &SessionDocument,
    producer_key: &str,
    input: &DynamicContextInput,
    previous: Option<&DynamicContextState>,
) -> Result<bool, DomainError> {
    let occurrence = input
        .occurrence
        .as_ref()
        .ok_or(DomainError::ImmutableFieldMutation {
            entity: "dynamic_context",
            field: "remove_occurrence",
        })?;
    let raw_content = input
        .raw_content
        .as_deref()
        .ok_or(DomainError::ImmutableFieldMutation {
            entity: "dynamic_context",
            field: "remove_raw_content",
        })?;
    if let Some(state) = previous {
        return match state.operation {
            DynamicContextOperation::Remove => {
                if state.occurrence.as_ref() != Some(occurrence) {
                    return Err(DomainError::ReferenceViolation {
                        kind: "dynamic_context.remove_occurrence",
                        value: state.source_seq.value(),
                    });
                }
                if state.raw_content != raw_content {
                    return Err(DomainError::ReferenceViolation {
                        kind: "dynamic_context.remove_body",
                        value: state.source_seq.value(),
                    });
                }
                Ok(true)
            }
            DynamicContextOperation::Upsert => {
                if state.occurrence.as_ref() != Some(occurrence) {
                    return Err(DomainError::ReferenceViolation {
                        kind: "dynamic_context.remove_occurrence",
                        value: state.source_seq.value(),
                    });
                }
                Ok(false)
            }
        };
    }

    let Some((source_seq, operation)) = latest_context_event(document, producer_key) else {
        return Err(DomainError::ReferenceViolation {
            kind: "dynamic_context.remove_source",
            value: 0,
        });
    };
    match operation {
        SystemContextOperation::Remove => Ok(true),
        SystemContextOperation::Upsert => Err(DomainError::ReferenceViolation {
            kind: "dynamic_context.remove_occurrence_metadata",
            value: source_seq.value(),
        }),
    }
}

/// Finds the latest durable SystemContext operation for one exact producer key.
/// 查找某个精确生产者键最近的持久 SystemContext 操作。
fn latest_context_event(
    document: &SessionDocument,
    producer_key: &str,
) -> Option<(Seq, SystemContextOperation)> {
    document.events().iter().rev().find_map(|event| {
        let EventBody::SystemContext(context) = &event.event else {
            return None;
        };
        (context.context_key == producer_key).then_some((event.seq(), context.operation))
    })
}

/// Resolves the durable owner turn for one Turn-scoped context event.
/// 为一条 Turn 范围上下文事件解析可靠的所属回合。
fn event_turn_id(
    document: &SessionDocument,
    active_turn_id: Option<TurnId>,
    scope: SystemContextScope,
) -> Result<Option<TurnId>, DomainError> {
    if scope == SystemContextScope::Turn {
        return active_turn_id
            .or_else(|| {
                document
                    .active_turn()
                    .filter(|turn| !turn.closed)
                    .map(|turn| turn.turn_id)
            })
            .map(Some)
            .ok_or(DomainError::NoActiveTurn);
    }
    Ok(None)
}

/// Describes which prior source version a remove event invalidates.
/// 描述移除事件使哪一个旧来源版本失效。
fn invalidation_content(source_seq: Seq) -> String {
    format!("expired_source_seq={}", source_seq.value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::dynamic_context::{
        DYNAMIC_CONTEXT_SYSTEM_KEY, DYNAMIC_CONTEXT_TRANSIENT_KEY, DynamicContextOccurrence,
    };
    use crate::v1::event::EventBody;

    /// Verifies changed values append raw immutable events and unchanged values are idempotent.
    /// 验证变更值追加原始不可变事件，未变更值保持幂等。
    #[test]
    fn synchronization_appends_once_per_changed_producer() {
        let mut document = SessionDocument::new("session", "project", "title", 1);
        let inputs = vec![
            DynamicContextInput::new(
                DYNAMIC_CONTEXT_SYSTEM_KEY,
                Some(String::from("system body")),
                SystemContextScope::Session,
                None,
            ),
            DynamicContextInput::new(
                DYNAMIC_CONTEXT_TRANSIENT_KEY,
                Some(String::from("transient body")),
                SystemContextScope::Session,
                None,
            ),
        ];
        let first = document
            .synchronize_dynamic_context(inputs.clone(), 2)
            .expect("first synchronization succeeds");
        assert!(first.changed());
        assert_eq!(first.into_value().len(), 2);
        assert_eq!(document.events().len(), 2);
        assert_eq!(document.metadata.dynamic_context.len(), 2);
        assert!(matches!(
            &document.events()[0].event,
            EventBody::SystemContext(SystemContextEvent {
                operation: SystemContextOperation::Upsert,
                provider_role_hint: Some(SystemRoleHint::HiddenUserShell),
                ..
            })
        ));

        let second = document
            .synchronize_dynamic_context(inputs, 3)
            .expect("identical synchronization succeeds");
        assert!(!second.changed());
        assert!(second.into_value().is_empty());
        assert_eq!(document.events().len(), 2);
    }

    /// Verifies removal appends an invalidation fact while preserving the old event body.
    /// 验证移除追加失效事实，同时保留旧事件正文不变。
    #[test]
    fn removal_appends_invalidation_without_rewriting_history() {
        let mut document = SessionDocument::new("session", "project", "title", 1);
        document
            .synchronize_dynamic_context(
                vec![DynamicContextInput::new(
                    DYNAMIC_CONTEXT_SYSTEM_KEY,
                    Some(String::from("old body")),
                    SystemContextScope::Session,
                    None,
                )],
                2,
            )
            .expect("initial value succeeds");
        let original = document.events()[0].clone();
        document
            .synchronize_dynamic_context(Vec::new(), 3)
            .expect("removal succeeds");
        assert_eq!(document.events()[0], original);
        assert_eq!(document.events().len(), 2);
        assert!(matches!(
            &document.events()[1].event,
            EventBody::SystemContext(SystemContextEvent {
                operation: SystemContextOperation::Remove,
                content,
                ..
            }) if content == "expired_source_seq=1"
        ));
        assert!(document.metadata.dynamic_context.is_empty());
    }

    /// Appends one instruction-level expiration once and keeps its raw version body unchanged.
    /// 只追加一次指令级失效事实，并保持其原始版本正文不变。
    #[test]
    fn expiration_event_is_idempotent_and_not_replayed_when_omitted() {
        let mut document = SessionDocument::new("session", "project", "title", 1);
        document
            .submit_root_input_fixture("prompt", Vec::new(), 1)
            .expect("open turn");
        let key = String::from("context-injection:transient:profile:hash-1");
        let occurrence = DynamicContextOccurrence::new(
            key.clone(),
            "occurrence-1",
            "profile",
            "hash-1",
            "turn-1",
        );
        document
            .synchronize_dynamic_context(
                vec![
                    DynamicContextInput::new(
                        key.clone(),
                        Some(String::from("transient body")),
                        SystemContextScope::Turn,
                        Some(1),
                    )
                    .with_occurrence(occurrence.clone()),
                ],
                2,
            )
            .expect("source upsert succeeds");
        let body = String::from("typed expiry fact");
        let input = DynamicContextInput::new(
            key.clone(),
            Some(body.clone()),
            SystemContextScope::Turn,
            None,
        )
        .as_remove(occurrence);

        let first = document
            .synchronize_dynamic_context(vec![input.clone()], 3)
            .expect("expiration event succeeds");
        assert_eq!(first.into_value().len(), 1);
        assert!(matches!(
            document.events().last().map(|event| &event.event),
            Some(EventBody::SystemContext(SystemContextEvent {
                context_key,
                operation: SystemContextOperation::Remove,
                content,
                ttl_turns: None,
                ..
            })) if context_key == &key && content == &body
        ));

        let event_count = document.events().len();
        let second = document
            .synchronize_dynamic_context(vec![input], 4)
            .expect("identical expiration is idempotent");
        assert!(!second.changed());
        assert_eq!(document.events().len(), event_count);
        assert!(document.metadata.dynamic_context.contains_key(&key));

        let before_resurrection = document.clone();
        let error = document
            .synchronize_dynamic_context(
                vec![
                    DynamicContextInput::new(
                        key,
                        Some(String::from("stale transient body")),
                        SystemContextScope::Turn,
                        Some(1),
                    )
                    .with_occurrence(DynamicContextOccurrence::new(
                        "context-injection:transient:profile:hash-1",
                        "occurrence-1",
                        "profile",
                        "hash-1",
                        "turn-1",
                    )),
                ],
                5,
            )
            .expect_err("expired occurrence cannot be resurrected");
        assert!(matches!(
            error,
            DomainError::ReferenceViolation {
                kind: "dynamic_context.resurrected_occurrence",
                ..
            }
        ));
        assert_eq!(document, before_resurrection);
    }

    /// Rejects an expiration body that cannot identify its original instruction version.
    /// 拒绝无法识别原始指令版本的失效正文。
    #[test]
    fn expiration_requires_typed_version_identity() {
        let mut document = SessionDocument::new("session", "project", "title", 1);
        let mut input = DynamicContextInput::new(
            String::from("context-injection:transient:profile:hash-1"),
            Some(String::from("Context injection expired after TTL.")),
            SystemContextScope::Turn,
            None,
        );
        input.operation = DynamicContextOperation::Remove;
        let error = document
            .synchronize_dynamic_context(vec![input], 2)
            .expect_err("an untyped removal must fail");
        assert!(matches!(
            error,
            DomainError::ImmutableFieldMutation {
                entity: "dynamic_context",
                field: "remove_input"
            }
        ));
        assert!(document.events().is_empty());
    }

    /// Avoids a generic transient removal when an instruction-level expiration explains it.
    /// 当指令级失效已经说明 transient 聚合失效时，不再追加泛化移除事实。
    #[test]
    fn explicit_expiration_keeps_generic_transient_removal() {
        let mut document = SessionDocument::new("session", "project", "title", 1);
        document
            .submit_root_input_fixture("prompt", Vec::new(), 1)
            .expect("open turn");
        document
            .synchronize_dynamic_context(
                vec![DynamicContextInput::new(
                    DYNAMIC_CONTEXT_TRANSIENT_KEY,
                    Some(String::from("transient body")),
                    SystemContextScope::Turn,
                    Some(1),
                )],
                2,
            )
            .expect("transient value succeeds");
        let key = String::from("context-injection:transient:profile:hash-1");
        let occurrence = DynamicContextOccurrence::new(
            key.clone(),
            "occurrence-1",
            "profile",
            "hash-1",
            "turn-1",
        );
        let body = String::from("typed expiry fact");
        document
            .synchronize_dynamic_context(
                vec![
                    DynamicContextInput::new(
                        DYNAMIC_CONTEXT_TRANSIENT_KEY,
                        None,
                        SystemContextScope::Turn,
                        Some(1),
                    ),
                    DynamicContextInput::new(
                        key.clone(),
                        Some(String::from("transient item body")),
                        SystemContextScope::Turn,
                        Some(1),
                    )
                    .with_occurrence(occurrence.clone()),
                ],
                2,
            )
            .expect("transient value succeeds");
        document
            .synchronize_dynamic_context(
                vec![
                    DynamicContextInput::new(
                        DYNAMIC_CONTEXT_TRANSIENT_KEY,
                        None,
                        SystemContextScope::Turn,
                        Some(1),
                    ),
                    DynamicContextInput::new(
                        key.clone(),
                        Some(body.clone()),
                        SystemContextScope::Turn,
                        None,
                    )
                    .as_remove(occurrence),
                ],
                3,
            )
            .expect("explicit expiration succeeds");

        let system_contexts = document
            .events()
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::SystemContext(context) => Some(context),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            system_contexts
                .iter()
                .filter(|context| context.operation == SystemContextOperation::Remove)
                .count(),
            2
        );
        assert!(system_contexts.iter().any(|context| {
            context.context_key == key
                && context.operation == SystemContextOperation::Remove
                && context.content == body
        }));
        assert!(system_contexts.iter().any(|context| {
            context.context_key == DYNAMIC_CONTEXT_TRANSIENT_KEY
                && context.operation == SystemContextOperation::Remove
        }));
    }

    /// Verifies malformed metadata fingerprints are rejected before synchronization.
    /// 验证错误 metadata 指纹会在同步前被拒绝。
    #[test]
    fn malformed_state_is_rejected() {
        let mut document = SessionDocument::new("session", "project", "title", 1);
        document.metadata.dynamic_context.insert(
            String::from(DYNAMIC_CONTEXT_SYSTEM_KEY),
            DynamicContextState {
                raw_content: String::from("body"),
                fingerprint: String::from("bad"),
                source_seq: Seq::try_new(1).expect("sequence"),
                scope: SystemContextScope::Session,
                ttl_turns: None,
                operation: DynamicContextOperation::Upsert,
                occurrence: None,
            },
        );
        let error = document
            .synchronize_dynamic_context(Vec::new(), 2)
            .expect_err("malformed state must fail");
        assert!(matches!(
            error,
            DomainError::ImmutableFieldMutation {
                entity: "dynamic_context",
                field: "fingerprint"
            }
        ));
    }

    /// Rejects a Turn-scoped value when no open logical Turn can own its event.
    /// 没有开放逻辑回合可归属事件时拒绝 Turn 范围值。
    #[test]
    fn turn_scope_requires_active_turn() {
        let mut document = SessionDocument::new("session", "project", "title", 1);
        let error = document
            .synchronize_dynamic_context(
                vec![DynamicContextInput::new(
                    DYNAMIC_CONTEXT_SYSTEM_KEY,
                    Some(String::from("turn body")),
                    SystemContextScope::Turn,
                    Some(1),
                )],
                2,
            )
            .expect_err("Turn scope must have an active owner");
        assert!(matches!(error, DomainError::NoActiveTurn));
        assert!(document.events().is_empty());
        assert!(document.metadata.dynamic_context.is_empty());
    }

    /// Preserves the document when revision capacity rejects a staged change.
    /// 修订容量拒绝暂存变更时保持文档完全不变。
    #[test]
    fn revision_capacity_failure_is_atomic() {
        let mut document = SessionDocument::new("session", "project", "title", 1);
        document.memory_revision = u64::MAX;
        let before = document.clone();
        let error = document
            .synchronize_dynamic_context(
                vec![DynamicContextInput::new(
                    DYNAMIC_CONTEXT_SYSTEM_KEY,
                    Some(String::from("body")),
                    SystemContextScope::Session,
                    None,
                )],
                2,
            )
            .expect_err("revision overflow must be rejected");
        assert!(matches!(
            error,
            DomainError::InvalidId {
                entity: "document_revision",
                value: u64::MAX,
                ..
            }
        ));
        assert_eq!(document, before);
    }
}
