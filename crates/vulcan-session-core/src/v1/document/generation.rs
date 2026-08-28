//! Pure domain builders for edited and forked history generations.
//! 编辑与分叉历史 generation 的纯领域构建器。

use crate::v1::compaction_frontier::select_node_for_seq;
use crate::v1::config::{ForkOrigin, GoalConfig, InputState, MetadataDocument, PlanModeConfig};
use crate::v1::context::{CompactionNode, RuntimeInitializationState, RuntimeLock};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ConversationEvent, EventBody, EventVisibility, SystemContextEvent, SystemContextOperation,
    SystemContextScope, SystemRoleHint, UserInputSource,
};
use crate::v1::ids::{JS_SAFE_INTEGER_MAX, ResourceId, Seq};
use crate::v1::projection::derive_active_system_state;

use super::{SessionDocument, validate_timestamp, validate_user_input};

/// Stable Goal state semantic key reset by every rewritten generation.
/// 每个重写 generation 都会重置的稳定 Goal 状态语义键。
const GOAL_STATE_CONTEXT_KEY: &str = "goal_state";

/// Stable Goal continuation semantic key reset by every rewritten generation.
/// 每个重写 generation 都会重置的稳定 Goal 续跑语义键。
const GOAL_CONTINUATION_CONTEXT_KEY: &str = "goal_continuation";

/// Stable Plan-mode semantic key reset by every rewritten generation.
/// 每个重写 generation 都会重置的稳定 Plan 模式语义键。
const PLAN_MODE_CONTEXT_KEY: &str = "plan_mode";

/// Exact replacement payload for one retained root user input.
/// 一条保留的根用户输入的精确替换载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootInputReplacement {
    /// Exact user text without normalization.
    /// 未经归一化的精确用户正文。
    pub text: String,
    /// Ordered project-level resource identities.
    /// 有序的项目级资源身份。
    pub ordered_resource_ids: Vec<ResourceId>,
}

impl RootInputReplacement {
    /// Builds one immutable root-input replacement.
    /// 构建一份不可变的根输入替换载荷。
    ///
    /// # Parameters / 参数
    /// `text` is the exact replacement text and `ordered_resource_ids` preserves attachment order.
    /// `text` 是精确替换正文，`ordered_resource_ids` 保持附件顺序。
    ///
    /// # Returns / 返回值
    /// Returns the strongly typed replacement value.
    /// 返回强类型替换值。
    pub fn new(text: impl Into<String>, ordered_resource_ids: Vec<ResourceId>) -> Self {
        Self {
            text: text.into(),
            ordered_resource_ids,
        }
    }
}

/// Complete domain input for replacing one historical root input in the same session.
/// 在同一会话中替换一条历史根输入的完整领域输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditGenerationInput {
    /// Retained root user-input seq that becomes the open history boundary.
    /// 成为开放历史边界的保留根用户输入 seq。
    pub target_seq: Seq,
    /// Exact replacement payload.
    /// 精确替换载荷。
    pub replacement: RootInputReplacement,
    /// Semantic generation-build time in Unix milliseconds.
    /// generation 构建语义时间（Unix 毫秒）。
    pub now_ms: i64,
}

/// Complete domain input for forking one historical root input into a new session.
/// 把一条历史根输入分叉到新会话的完整领域输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkGenerationInput {
    /// Exact child session identity allocated by the lifecycle owner.
    /// 生命周期 owner 分配的精确子会话身份。
    pub child_session_id: String,
    /// Exact child display title.
    /// 精确子会话显示标题。
    pub title: String,
    /// Retained root user-input seq that becomes the child open history boundary.
    /// 成为子会话开放历史边界的保留根用户输入 seq。
    pub target_seq: Seq,
    /// Child creation and generation-build time in Unix milliseconds.
    /// 子会话创建与 generation 构建时间（Unix 毫秒）。
    pub now_ms: i64,
}

/// Complete storage-independent result of one history generation build.
/// 一次历史 generation 构建的完整、与存储无关的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationBuildOutput {
    /// Complete semantic metadata; the storage owner may only fill `split_list`.
    /// 完整语义 metadata；存储 owner 只允许填充 `split_list`。
    pub metadata: MetadataDocument,
    /// Complete replacement runtime lock.
    /// 完整替换 runtime 锁。
    pub runtime_lock: RuntimeLock,
    /// Complete retained compaction history.
    /// 完整保留的压缩历史。
    pub compactions: Vec<CompactionNode>,
    /// Complete contiguous replacement event stream.
    /// 完整连续的替换事件流。
    pub events: Vec<ConversationEvent>,
    /// First seq whose physical segment must be rewritten for an in-place edit.
    /// 同会话编辑时首个必须重写物理分片的 seq。
    pub history_target_seq: Seq,
}

impl SessionDocument {
    /// Builds one fully validated replacement generation without mutating the source document.
    /// 构建一份完整校验的替换 generation，且不修改源文档。
    ///
    /// # Parameters / 参数
    /// `input` supplies the retained root input, replacement payload, and semantic time.
    /// `input` 提供保留根输入、替换载荷与语义时间。
    ///
    /// # Returns / 返回值
    /// Returns the complete domain output consumed by the storage publisher.
    /// 返回供存储发布器消费的完整领域输出。
    pub fn build_edited_generation(
        &self,
        input: EditGenerationInput,
    ) -> Result<GenerationBuildOutput, DomainError> {
        self.ensure_generation_source_ready()?;
        validate_timestamp("generation.now_ms", input.now_ms)?;
        validate_user_input(
            input.replacement.text.clone(),
            &input.replacement.ordered_resource_ids,
        )?;

        let target_index = self.require_generation_root_target(input.target_seq)?;
        let effective_now_ms = self.metadata.updated_at_ms.max(input.now_ms);
        let mut retained = self.events[..=target_index].to_vec();
        retained[target_index].mutate_in_place(effective_now_ms, |body| {
            let EventBody::UserInput(user_input) = body else {
                unreachable!("generation target was validated as a root user input")
            };
            user_input.text = input.replacement.text;
            user_input.resource_ids = input.replacement.ordered_resource_ids;
        })?;

        let metadata = self.build_generation_metadata(
            self.metadata.session_id.clone(),
            self.metadata.fork_origin.clone(),
            self.metadata.project_tag.clone(),
            self.metadata.title.clone(),
            self.metadata.created_at_ms,
            self.metadata.archived_at_ms,
            input.target_seq,
            effective_now_ms,
        )?;
        self.finish_generation_build(metadata, retained, input.target_seq, effective_now_ms)
    }

    /// Builds one fully validated child generation without mutating the parent document.
    /// 构建一份完整校验的子 generation，且不修改父文档。
    ///
    /// # Parameters / 参数
    /// `input` supplies child identity, retained boundary, and creation time.
    /// `input` 提供子会话身份、保留边界与创建时间。
    ///
    /// # Returns / 返回值
    /// Returns a complete child-domain output whose copied seq and local numeric ids remain exact.
    /// 返回完整子会话领域输出；复制的 seq 与本地数字 id 保持精确不变。
    pub fn build_fork_generation(
        &self,
        input: ForkGenerationInput,
    ) -> Result<GenerationBuildOutput, DomainError> {
        self.ensure_generation_source_ready()?;
        validate_timestamp("generation.now_ms", input.now_ms)?;
        validate_generation_identity("child_session_id", &input.child_session_id)?;
        if input.title.trim().is_empty() || input.title.trim() != input.title {
            return Err(DomainError::InvalidSessionTitle);
        }
        if input.child_session_id == self.metadata.session_id {
            return Err(DomainError::InvalidGenerationInput {
                field: "child_session_id_matches_parent",
            });
        }

        let target_index = self.require_generation_root_target(input.target_seq)?;
        let retained = self.events[..=target_index].to_vec();
        let fork_origin = ForkOrigin {
            source_session_id: self.metadata.session_id.clone(),
            source_segment_generation: self.metadata.segment_generation,
            source_target_seq: input.target_seq,
        };
        let metadata = self.build_generation_metadata(
            input.child_session_id,
            Some(fork_origin),
            self.metadata.project_tag.clone(),
            input.title,
            input.now_ms,
            None,
            input.target_seq,
            input.now_ms,
        )?;
        self.finish_generation_build(metadata, retained, input.target_seq, input.now_ms)
    }

    /// Validates source durability, residency, and API quiescence before any generation work.
    /// 在任何 generation 工作前校验源持久化、常驻范围与 API 静止状态。
    fn ensure_generation_source_ready(&self) -> Result<(), DomainError> {
        if self.memory_revision != self.durable_revision {
            return Err(DomainError::GenerationSourceNotDurable {
                memory_revision: self.memory_revision,
                durable_revision: self.durable_revision,
            });
        }
        if self.resident_start_seq != 1 {
            return Err(DomainError::GenerationRequiresFullHistory {
                resident_start_seq: self.resident_start_seq,
            });
        }
        if let Some(active_api) = &self.active_api {
            return Err(DomainError::GenerationWhileApiActive {
                api_call_id: active_api.api_call_id.value(),
            });
        }
        Ok(())
    }

    /// Resolves and validates one root input retained as the new open history boundary.
    /// 解析并校验一条作为新开放历史边界保留的根输入。
    fn require_generation_root_target(&self, target_seq: Seq) -> Result<usize, DomainError> {
        let index =
            target_seq
                .value()
                .checked_sub(1)
                .ok_or(DomainError::InvalidGenerationInput {
                    field: "target_seq",
                })? as usize;
        let event = self
            .events
            .get(index)
            .ok_or_else(|| DomainError::reference("generation_target_seq", target_seq.value()))?;
        if event.seq != target_seq {
            return Err(DomainError::SequenceViolation {
                expected: target_seq.value(),
                actual: event.seq.value(),
            });
        }
        let EventBody::UserInput(user_input) = &event.event else {
            return Err(DomainError::InvalidGenerationInput {
                field: "target_event_kind",
            });
        };
        if user_input.input_source != UserInputSource::Root {
            return Err(DomainError::InvalidGenerationInput {
                field: "target_input_source",
            });
        }
        if event.turn_id.is_none() {
            return Err(DomainError::InvalidGenerationInput {
                field: "target_turn_id",
            });
        }
        Ok(index)
    }

    /// Builds metadata whose semantic fields are complete and whose physical split map is unpublished.
    /// 构建语义字段完整、物理分片映射尚未发布的 metadata。
    #[allow(clippy::too_many_arguments)]
    fn build_generation_metadata(
        &self,
        session_id: String,
        fork_origin: Option<ForkOrigin>,
        project_tag: String,
        title: String,
        created_at_ms: i64,
        archived_at_ms: Option<i64>,
        target_seq: Seq,
        updated_at_ms: i64,
    ) -> Result<MetadataDocument, DomainError> {
        let next_document_revision =
            checked_generation_counter("document_revision", self.metadata.document_revision)?;
        let next_segment_generation =
            checked_generation_counter("segment_generation", self.metadata.segment_generation)?;
        let next_context_generation = checked_generation_counter(
            "active_context_generation",
            self.metadata.active_context_generation,
        )?;
        let mut metadata = self.metadata.clone();
        metadata.session_id = session_id;
        metadata.fork_origin = fork_origin;
        metadata.project_tag = project_tag;
        metadata.created_at_ms = created_at_ms;
        metadata.updated_at_ms = updated_at_ms;
        metadata.document_revision = next_document_revision;
        metadata.archived_at_ms = archived_at_ms;
        metadata.title = title;
        metadata.mode.goal = GoalConfig::default();
        metadata.mode.plan = PlanModeConfig::default();
        metadata.config.pending_request = None;
        metadata.input_state = InputState::default();
        metadata.segment_generation = next_segment_generation;
        metadata.active_context_generation = next_context_generation;
        metadata.open_turn_start_seq = Some(target_seq);
        metadata.split_list.clear();
        Ok(metadata)
    }

    /// Completes mode reset, compaction selection, allocator rebuild, and strict self-validation.
    /// 完成模式重置、压缩选择、分配器重建与严格自校验。
    fn finish_generation_build(
        &self,
        mut metadata: MetadataDocument,
        retained: Vec<ConversationEvent>,
        target_seq: Seq,
        now_ms: i64,
    ) -> Result<GenerationBuildOutput, DomainError> {
        let kept_compactions: Vec<CompactionNode> = self
            .compactions
            .iter()
            .filter(|node| {
                node.api_stats_seq <= target_seq
                    && node.occurred_at_seq <= target_seq
                    && node.retained_from_seq <= target_seq
            })
            .cloned()
            .collect();
        let selected_compaction =
            select_node_for_seq(&kept_compactions, target_seq).map(|node| node.compaction_id);
        let runtime_lock = RuntimeLock {
            initialization_state: RuntimeInitializationState::BaselineRefreshPending,
            current_compaction_id: selected_compaction,
            locked_sources: Vec::new(),
        };

        let active_mode_keys = active_mode_context_keys(&retained, target_seq)?;
        let mut document = SessionDocument::from_parts(
            metadata.clone(),
            runtime_lock.clone(),
            kept_compactions.clone(),
            Vec::new(),
            retained,
        )?;
        if !active_mode_keys.is_empty() {
            let turn_id = document
                .active_turn
                .as_ref()
                .map(|turn| turn.turn_id)
                .ok_or(DomainError::InvalidGenerationInput {
                    field: "target_open_turn",
                })?;
            let mut staged_allocator = document.id_allocator;
            let mut staged_events = Vec::with_capacity(active_mode_keys.len());
            for context_key in active_mode_keys {
                document.stage_event(
                    &mut staged_allocator,
                    &mut staged_events,
                    Some(turn_id),
                    EventVisibility::Provider,
                    EventBody::SystemContext(SystemContextEvent {
                        context_key: context_key.to_string(),
                        operation: SystemContextOperation::Remove,
                        scope: SystemContextScope::Session,
                        ttl_turns: None,
                        provider_role_hint: Some(SystemRoleHint::Developer),
                        content: generation_reset_content(context_key).to_string(),
                    }),
                    now_ms,
                )?;
            }
            document.id_allocator = staged_allocator;
            document.events.extend(staged_events);
            document.rebuild_derived_state()?;
        }
        metadata.id_frontier = document.id_frontier();
        document.metadata.id_frontier = metadata.id_frontier;

        let validated = SessionDocument::from_parts(
            document.metadata.clone(),
            document.runtime_lock.clone(),
            document.compactions.clone(),
            Vec::new(),
            document.events.clone(),
        )?;
        Ok(GenerationBuildOutput {
            metadata: validated.metadata,
            runtime_lock: validated.runtime_lock,
            compactions: validated.compactions,
            events: validated.events,
            history_target_seq: target_seq,
        })
    }
}

/// Returns active Goal/Plan semantic keys that must receive explicit removal facts.
/// 返回必须追加显式移除事实的活动 Goal/Plan 语义键。
fn active_mode_context_keys(
    events: &[ConversationEvent],
    through_seq: Seq,
) -> Result<Vec<&'static str>, DomainError> {
    let active = derive_active_system_state(events, through_seq)?;
    let mut result = Vec::new();
    for context_key in [
        GOAL_STATE_CONTEXT_KEY,
        GOAL_CONTINUATION_CONTEXT_KEY,
        PLAN_MODE_CONTEXT_KEY,
    ] {
        if active.iter().any(|state| state.context_key == context_key) {
            result.push(context_key);
        }
    }
    Ok(result)
}

/// Returns the stable Provider-visible explanation for one generation mode reset.
/// 返回一次 generation 模式重置的稳定 Provider 可见说明。
fn generation_reset_content(context_key: &str) -> &'static str {
    match context_key {
        GOAL_STATE_CONTEXT_KEY => {
            "The active goal was reset because the conversation history was replaced."
        }
        GOAL_CONTINUATION_CONTEXT_KEY => {
            "Goal continuation was reset because the conversation history was replaced."
        }
        PLAN_MODE_CONTEXT_KEY => {
            "Plan mode was reset because the conversation history was replaced."
        }
        _ => unreachable!("only frozen mode context keys are passed to the reset builder"),
    }
}

/// Validates one exact non-normalized generation identity.
/// 校验一个不做归一化的精确 generation 身份。
fn validate_generation_identity(field: &'static str, value: &str) -> Result<(), DomainError> {
    if value.trim().is_empty() || value.trim() != value {
        return Err(DomainError::InvalidGenerationInput { field });
    }
    Ok(())
}

/// Advances one JS-safe generation counter exactly once.
/// 将一个 JS 安全 generation 计数器精确推进一次。
fn checked_generation_counter(field: &'static str, value: u64) -> Result<u64, DomainError> {
    let next = value
        .checked_add(1)
        .ok_or(DomainError::InvalidGenerationInput { field })?;
    if next > JS_SAFE_INTEGER_MAX {
        return Err(DomainError::InvalidGenerationInput { field });
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::document::ClientInputSubmission;
    use crate::v1::ids::ClientActionId;

    /// Fixed semantic time used by generation tests.
    /// generation 测试使用的固定语义时间。
    const T0: i64 = 1_700_000_000_000;

    /// Marks the current document revision durable through the real flush-candidate protocol.
    /// 通过真实 flush-candidate 协议把当前文档修订标记为已持久化。
    fn make_durable(document: &mut SessionDocument) {
        let candidate = document
            .prepare_flush_candidates()
            .expect("flush candidate preparation succeeds")
            .expect("dirty fixture produces one candidate");
        document
            .advance_durable(candidate.revision)
            .expect("fixture revision becomes durable");
    }

    /// Builds one durable source containing two root turns and an active Plan context before the second root.
    /// 构建一个包含两个根回合且第二根输入前已有活动 Plan 上下文的可靠源文档。
    fn durable_edit_source() -> SessionDocument {
        let mut document = SessionDocument::new("source-session", "project", "source", T0);
        document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("root-1").unwrap(),
                    "first",
                    Vec::new(),
                ),
                T0 + 1,
            )
            .unwrap();
        document
            .close_turn(crate::v1::event::TurnOutcome::Stopped, 1, T0 + 2)
            .unwrap();
        document
            .append_system_context(
                None,
                PLAN_MODE_CONTEXT_KEY,
                SystemContextOperation::Upsert,
                SystemContextScope::Session,
                None,
                Some(SystemRoleHint::Developer),
                "Plan mode is active.",
                T0 + 3,
            )
            .unwrap();
        document.metadata.mode.plan.enabled = true;
        document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("root-2").unwrap(),
                    "second",
                    Vec::new(),
                ),
                T0 + 4,
            )
            .unwrap();
        make_durable(&mut document);
        document
    }

    /// Edit generation replaces only the retained root and emits an explicit mode reset.
    /// 编辑 generation 只替换保留根输入并追加显式模式重置。
    #[test]
    fn edit_generation_is_pure_and_resets_mode_state() {
        let mut source = durable_edit_source();
        let fork_origin = ForkOrigin {
            source_session_id: "ancestor-session".to_string(),
            source_segment_generation: 3,
            source_target_seq: Seq::try_new(1).unwrap(),
        };
        source.metadata.fork_origin = Some(fork_origin.clone());
        let before = source.clone();
        let target_seq = source.events().last().unwrap().seq;
        let old_event_revision = source.events().last().unwrap().event_revision;
        let output = source
            .build_edited_generation(EditGenerationInput {
                target_seq,
                replacement: RootInputReplacement::new("replacement", Vec::new()),
                now_ms: T0 + 10,
            })
            .unwrap();

        assert_eq!(source, before);
        assert_eq!(output.metadata.fork_origin, Some(fork_origin));
        assert_eq!(output.history_target_seq, target_seq);
        assert!(!output.metadata.mode.plan.enabled);
        assert_eq!(output.metadata.input_state, InputState::default());
        assert!(output.metadata.config.pending_request.is_none());
        assert_eq!(output.metadata.open_turn_start_seq, Some(target_seq));
        assert_eq!(
            output.runtime_lock.initialization_state,
            RuntimeInitializationState::BaselineRefreshPending
        );
        assert!(output.runtime_lock.locked_sources.is_empty());
        assert_eq!(
            output.events[target_seq.value() as usize - 1].event_revision,
            old_event_revision + 1
        );
        let EventBody::UserInput(replaced) = &output.events[target_seq.value() as usize - 1].event
        else {
            panic!("target remains a user input")
        };
        assert_eq!(replaced.text, "replacement");
        assert!(matches!(
            output.events.last().map(|event| &event.event),
            Some(EventBody::SystemContext(SystemContextEvent {
                context_key,
                operation: SystemContextOperation::Remove,
                ..
            })) if context_key == PLAN_MODE_CONTEXT_KEY
        ));
        assert_eq!(
            output.metadata.id_frontier.next_seq,
            output.events.len() as u64 + 1
        );
    }

    /// Fork generation preserves copied fact identities while assigning exact child metadata.
    /// 分叉 generation 保留复制事实身份，同时写入精确子会话 metadata。
    #[test]
    fn fork_generation_preserves_prefix_identity_and_resets_transient_state() {
        let source = durable_edit_source();
        let target_seq = source.events().last().unwrap().seq;
        let output = source
            .build_fork_generation(ForkGenerationInput {
                child_session_id: "child-session".to_string(),
                title: "child".to_string(),
                target_seq,
                now_ms: T0 + 20,
            })
            .unwrap();

        assert_eq!(output.metadata.session_id, "child-session");
        assert_eq!(
            output.metadata.fork_origin,
            Some(ForkOrigin {
                source_session_id: source.metadata.session_id.clone(),
                source_segment_generation: source.metadata.segment_generation,
                source_target_seq: target_seq,
            })
        );
        assert_eq!(output.metadata.created_at_ms, T0 + 20);
        assert_eq!(output.metadata.archived_at_ms, None);
        assert_eq!(
            output.metadata.segment_generation,
            source.metadata.segment_generation + 1
        );
        assert_eq!(
            output.metadata.document_revision,
            source.metadata.document_revision + 1
        );
        assert_eq!(
            output.events[..target_seq.value() as usize],
            source.events()[..target_seq.value() as usize]
        );
        assert!(!output.metadata.mode.goal.enabled);
        assert!(!output.metadata.mode.plan.enabled);
        assert_eq!(output.metadata.input_state, InputState::default());
        assert_eq!(
            output.runtime_lock.initialization_state,
            RuntimeInitializationState::BaselineRefreshPending
        );
        assert!(output.runtime_lock.locked_sources.is_empty());
    }

    /// Generation building rejects dirty sources and leaves them byte-for-byte unchanged.
    /// generation 构建拒绝脏源并保持源文档逐字段不变。
    #[test]
    fn generation_rejects_non_durable_source_without_mutation() {
        let mut source = durable_edit_source();
        source.rename_session("dirty", T0 + 30).unwrap();
        let before = source.clone();
        let target_seq = source.events().last().unwrap().seq;
        let result = source.build_edited_generation(EditGenerationInput {
            target_seq,
            replacement: RootInputReplacement::new("replacement", Vec::new()),
            now_ms: T0 + 31,
        });

        assert!(matches!(
            result,
            Err(DomainError::GenerationSourceNotDurable { .. })
        ));
        assert_eq!(source, before);
    }
}
