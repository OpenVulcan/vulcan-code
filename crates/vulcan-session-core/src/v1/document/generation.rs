//! Pure domain builders for edited and forked history generations.
//! 编辑与分叉历史 generation 的纯领域构建器。

use crate::v1::collaboration::{
    CollaborationMetadata, CollaborationOperationId, CollaborationRunControl, SetRunControlInput,
};
use crate::v1::compaction_frontier::select_node_for_seq;
use crate::v1::config::{
    ForkOrigin, GoalConfig, InputState, MetadataDocument, PlanModeConfig, SegmentInfo,
};
use crate::v1::context::{CompactionNode, RuntimeInitializationState, RuntimeLock};
use crate::v1::dynamic_context::{
    DYNAMIC_CONTEXT_DEVELOPER_KEY, DYNAMIC_CONTEXT_PERSISTENT_KEY, DYNAMIC_CONTEXT_RUNTIME_KEY,
    DYNAMIC_CONTEXT_SYSTEM_KEY, DYNAMIC_CONTEXT_TOOL_GUIDANCE_KEY, DYNAMIC_CONTEXT_TRANSIENT_KEY,
};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ConversationEvent, EventBody, EventVisibility, SystemContextEvent, SystemContextOperation,
    SystemContextScope, SystemRoleHint, UserInputSource,
};
use crate::v1::ids::{ClientActionId, JS_SAFE_INTEGER_MAX, ResourceId, Seq};
use crate::v1::projection::{derive_active_system_state, provider_fingerprint_mismatches};

use super::{
    ANSWER_MODE_CONTEXT_KEY, SHELL_JOB_CONTINUATION_CONTEXT_KEY, SessionDocument,
    validate_timestamp, validate_user_input,
};

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
    /// Optional replacement client action that owns the rewritten prompt intent.
    /// 可选的替换客户端动作，用于持有重写后的提示词意图。
    pub client_action_id: Option<ClientActionId>,
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
            client_action_id: None,
        }
    }

    /// Assigns the durable client action that owns this rewritten prompt.
    /// 指定持有本次重写提示词的可靠客户端动作。
    ///
    /// # Parameters / 参数
    /// `client_action_id` is the exact external idempotency and notification-owner identity.
    /// `client_action_id` 是精确的外部幂等及通知所有者身份。
    ///
    /// # Returns / 返回值
    /// Returns the replacement with its explicit client action installed.
    /// 返回已安装显式客户端动作的替换载荷。
    pub fn with_client_action_id(mut self, client_action_id: ClientActionId) -> Self {
        self.client_action_id = Some(client_action_id);
        self
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

/// Complete domain input for forking one completed Turn into a new session.
/// 把一个已完成回合分叉到新会话的完整领域输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkGenerationInput {
    /// Exact child session identity allocated by the lifecycle owner.
    /// 生命周期 owner 分配的精确子会话身份。
    pub child_session_id: String,
    /// Exact child display title.
    /// 精确子会话显示标题。
    pub title: String,
    /// Terminal TurnStats seq retained as the completed child history boundary.
    /// 作为子会话完整历史边界保留的终态 TurnStats seq。
    pub source_turn_end_seq: Seq,
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
        self.reject_dedicated_change("generation")?;
        self.ensure_generation_source_ready()?;
        // Editing replaces the live generation, so its physical API must already be finished.
        // 编辑会替换原会话代次，因此物理 API 必须已经结束。
        if let Some(active_api) = &self.active_api {
            return Err(DomainError::GenerationWhileApiActive {
                api_call_id: active_api.api_call_id.value(),
            });
        }
        // Replacing source history cannot abandon an accepted question or child obligation.
        // 替换来源历史不能遗弃已接纳问题或子任务义务。
        if self.has_unsettled_collaboration_for_edit_or_delete() {
            return Err(DomainError::InvalidGenerationInput {
                field: "unsettled_collaboration",
            });
        }
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
            // An explicit replacement is a new prompt intent; retained earlier prompts keep their frozen parameters.
            // 显式替换是新提示词意图；此前保留的提示词继续持有各自冻结参数。
            user_input.parameters = crate::v1::UserPromptParameters::for_new_resources(
                &input.replacement.ordered_resource_ids,
            );
            user_input.resource_ids = input.replacement.ordered_resource_ids;
            if let Some(client_action_id) = input.replacement.client_action_id {
                user_input.client_action_id = client_action_id;
            }
        })?;

        let mut metadata = self.build_generation_metadata(
            self.metadata.session_id.clone(),
            self.metadata.fork_origin.clone(),
            self.metadata.project_tag.clone(),
            self.metadata.title.clone(),
            self.metadata.created_at_ms,
            self.metadata.archived_at_ms,
            Some(input.target_seq),
            effective_now_ms,
        )?;
        // Carry the current picker intent into generation finalization, which activates it atomically
        // with the replacement root. Physical dispatch must only consume the admitted active config.
        // 将当前选择器意图带入代次收尾，与替换根输入原子激活。物理分发只消费已准入的活动配置。
        metadata.config.pending_request = self.metadata.config.pending_request.clone();
        // An edit keeps every segment start at or before the target, so retained checkpoints written from a
        // later segment window still verify against the same windows after publication.
        // 编辑保留目标及之前的全部分段起点，因此从更晚分段窗口写入的保留检查点在发布后仍按相同窗口校验。
        let segment_catalog = self
            .segment_catalog
            .iter()
            .filter(|entry| entry.start_seq <= input.target_seq)
            .cloned()
            .collect();
        self.finish_generation_build(
            metadata,
            segment_catalog,
            retained,
            input.target_seq,
            effective_now_ms,
        )
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
        self.reject_dedicated_change("generation")?;
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

        // A validated Turn end bounds the copy even when a later source request is active.
        // 即使来源的后续请求仍在运行，已校验的回合结束点也能限定复制范围。
        let target_index = self.require_generation_turn_end_target(input.source_turn_end_seq)?;
        let mut retained = self.events[..=target_index].to_vec();
        // The child excludes successor work, while the source retains its complete cross-Turn audit.
        // 子会话排除后继工作，来源继续保留完整跨回合审计。
        self.retire_input_handoffs_outside_fork(
            &mut retained,
            input.source_turn_end_seq,
            input.now_ms,
        )?;
        // The child stores its history as one segment from seq 1, so a copied checkpoint written from a later
        // source segment window can never verify there; it is dropped exactly as the child's loader would.
        // 子会话把历史存为从 seq 1 开始的单个分段，从更晚源分段窗口写入的复制检查点在那里永远无法校验；
        // 按子会话加载器同样的方式移除。
        drop_unverifiable_provider_sources(&mut retained)?;
        let fork_origin = ForkOrigin {
            source_session_id: self.metadata.session_id.clone(),
            source_segment_generation: self.metadata.segment_generation,
            source_target_seq: input.source_turn_end_seq,
        };
        let metadata = self.build_generation_metadata(
            input.child_session_id,
            Some(fork_origin),
            self.metadata.project_tag.clone(),
            input.title,
            input.now_ms,
            None,
            None,
            input.now_ms,
        )?;
        self.finish_generation_build(
            metadata,
            Vec::new(),
            retained,
            input.source_turn_end_seq,
            input.now_ms,
        )
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
        Ok(())
    }

    /// Resolves and validates one root input retained as the new open history boundary.
    /// 解析并校验一条作为新开放历史边界保留的根输入。
    fn require_generation_root_target(&self, target_seq: Seq) -> Result<usize, DomainError> {
        self.require_uncleaned_generation_target(target_seq)?;
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

    /// Resolves and validates one terminal TurnStats event retained as a fork boundary.
    /// 解析并校验一条作为分叉边界保留的终态 TurnStats 事件。
    ///
    /// # Parameters / 参数
    /// `turn_end_seq` is the exact inclusive end seq of one completed source Turn.
    /// `turn_end_seq` 是源会话中一个已完成回合的精确包含式结束 seq。
    ///
    /// # Returns / 返回值
    /// Returns the event index only when the seq is the canonical terminal statistics event.
    /// 仅当该 seq 是规范终态统计事件时返回事件索引。
    fn require_generation_turn_end_target(&self, turn_end_seq: Seq) -> Result<usize, DomainError> {
        self.require_uncleaned_generation_target(turn_end_seq)?;
        let index =
            turn_end_seq
                .value()
                .checked_sub(1)
                .ok_or(DomainError::InvalidGenerationInput {
                    field: "source_turn_end_seq",
                })? as usize;
        let event = self.events.get(index).ok_or_else(|| {
            DomainError::reference("generation_source_turn_end_seq", turn_end_seq.value())
        })?;
        if event.seq != turn_end_seq {
            return Err(DomainError::SequenceViolation {
                expected: turn_end_seq.value(),
                actual: event.seq.value(),
            });
        }
        let EventBody::TurnStats(stats) = &event.event else {
            return Err(DomainError::InvalidGenerationInput {
                field: "source_turn_end_event_kind",
            });
        };
        if event.turn_id.is_none() || stats.end_seq != turn_end_seq {
            return Err(DomainError::InvalidGenerationInput {
                field: "source_turn_end_identity",
            });
        }
        Ok(index)
    }

    /// Rejects positions at or before an extracted range because replay history is no longer complete there.
    /// 拒绝已提取区间及其之前的位置，因为这些位置的重放历史已不完整。
    fn require_uncleaned_generation_target(&self, target: Seq) -> Result<(), DomainError> {
        if self
            .metadata
            .cleaned_history
            .as_ref()
            .is_some_and(|history| history.ranges.iter().any(|range| target <= range.through))
        {
            return Err(DomainError::InvalidGenerationInput {
                field: "cleaned_history_target",
            });
        }
        // Once history was cleaned, a regenerated session must retain its current compaction.
        // 清理历史后，重新生成的会话必须保留当前压缩节点。
        if self.metadata.cleaned_history.is_some()
            && self.compactions.iter().any(|node| {
                Some(node.compaction_id) == self.runtime_lock.current_compaction_id
                    && target <= node.api_stats_seq
            })
        {
            return Err(DomainError::InvalidGenerationInput {
                field: "before_active_compaction",
            });
        }
        Ok(())
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
        open_turn_start_seq: Option<Seq>,
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
        // Preserve the user's answer mode, but prepare it against the new generation independently.
        // 保留用户回答模式，但必须针对新代次独立准备。
        metadata.config.answer_mode.prepared = None;
        metadata.input_state = InputState::default();
        // Dynamic request producers are refreshed from the new execution after explicit history invalidation.
        // 动态请求生产者在历史明确失效后由新执行重新生成。
        metadata.dynamic_context.clear();
        // A new event-tree generation cannot inherit the old child's executable profile ownership.
        // 新事件树代际不能继承旧子代理的可执行配置所有权。
        metadata.child_execution_profile = None;
        metadata.context_maintenance = None;
        // A rewritten parent cannot retain executable creation ownership from its old event tree.
        // 改写后的父会话不能保留旧事件树的可执行创建归属。
        metadata.child_creation_intents.clear();
        // A rewritten event tree cannot retain another generation's Hook correlation.
        // 改写后的事件树不能保留另一代次的 Hook 关联。
        metadata.business_hook = crate::v1::business_hook::BusinessHookMetadata::default();
        metadata.segment_generation = next_segment_generation;
        metadata.active_context_generation = next_context_generation;
        metadata.open_turn_start_seq = open_turn_start_seq;
        metadata.split_list.clear();
        Ok(metadata)
    }

    /// Completes mode reset, compaction selection, allocator rebuild, and strict self-validation.
    /// 完成模式重置、压缩选择、分配器重建与严格自校验。
    ///
    /// `segment_catalog` lists the segment starts the published generation keeps, which checkpoint
    /// verification needs; it is empty when the output is stored as one segment from seq 1.
    /// `segment_catalog` 列出发布后代次保留的分段起点，检查点校验需要它；输出存为从 seq 1 开始的单个分段时为空。
    fn finish_generation_build(
        &self,
        mut metadata: MetadataDocument,
        segment_catalog: Vec<SegmentInfo>,
        retained: Vec<ConversationEvent>,
        target_seq: Seq,
        now_ms: i64,
    ) -> Result<GenerationBuildOutput, DomainError> {
        // Copied events remain readable, but only events after this boundary can own new collaboration work.
        // 复制事件保持可读，但只有此边界之后的事件可以拥有新的协作执行权威。
        metadata.collaboration = CollaborationMetadata {
            history_boundary_seq: Some(target_seq),
            ..CollaborationMetadata::new()
        };
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

        let active_mode_keys = active_generation_context_keys(&retained, target_seq)?;
        let mut document = SessionDocument::from_parts(
            metadata.clone(),
            runtime_lock,
            kept_compactions,
            segment_catalog,
            retained,
        )?;
        if !active_mode_keys.is_empty() {
            // mode_reset_turn_id keeps edit resets inside the rewritten open Turn, while a
            // completed-Turn fork records the reset outside every Turn and starts no new work.
            // mode_reset_turn_id 让编辑重置留在重写后的开放回合内；完整回合分叉则在所有回合之外
            // 记录重置，并且不会启动任何新工作。
            let mode_reset_turn_id = document.active_turn.as_ref().map(|turn| turn.turn_id);
            let mut staged_allocator = document.id_allocator;
            let mut staged_events = Vec::with_capacity(active_mode_keys.len());
            for context_key in active_mode_keys {
                document.stage_event(
                    &mut staged_allocator,
                    &mut staged_events,
                    mode_reset_turn_id,
                    EventVisibility::Provider,
                    EventBody::SystemContext(SystemContextEvent {
                        context_key: context_key.to_string(),
                        operation: SystemContextOperation::Remove,
                        scope: SystemContextScope::Session,
                        ttl_turns: None,
                        provider_role_hint: Some(SystemRoleHint::HiddenUserShell),
                        content: generation_reset_content(context_key).to_string(),
                    }),
                    now_ms,
                )?;
            }
            document.id_allocator = staged_allocator;
            document.events.extend(staged_events);
            document.rebuild_derived_state()?;
        }
        // Only edits retain a pending request; forks clear it and have no open root. Reuse the normal
        // user-admission activation so route, account, reasoning and temperature switch together,
        // without a second revision or a post-publication window that could capture a later choice.
        // 只有编辑保留待生效请求；分叉会清空它且没有开放根输入。复用普通用户准入的激活逻辑，
        // 一次切换路由、账号、推理和温度，避免额外修订或发布后误取后续选择的时间窗口。
        if let Some(pending) = document.metadata.config.pending_request.as_ref() {
            // next_active freezes the complete configuration selected at the edit boundary.
            // next_active 冻结编辑边界选择的完整配置。
            let next_active = super::selection::active_request_from_pending(pending);
            // turn_id binds the switch notice to the exact rewritten root Turn.
            // turn_id 将切换提示绑定到精确重写的根回合。
            let turn_id = document.require_open_turn()?.turn_id;
            // staged values keep the pure generation builder transactional on validation failure.
            // 暂存值保证纯代次构建器在校验失败时仍保持事务性。
            let mut staged_metadata = document.metadata.clone();
            // staged_allocator allocates any switch notice after the preserved history prefix.
            // staged_allocator 将切换提示分配在保留历史前缀之后。
            let mut staged_allocator = document.id_allocator;
            // staged_events contains only the optional request-switch notice.
            // staged_events 仅包含可选的请求切换提示。
            let mut staged_events = Vec::new();
            document.stage_bound_request_activation(
                &mut staged_metadata,
                &mut staged_allocator,
                &mut staged_events,
                next_active,
                Some(turn_id),
                now_ms,
            )?;
            document.metadata = staged_metadata;
            document.id_allocator = staged_allocator;
            document.events.extend(staged_events);
        }
        // An edit reopens its root Turn and is dispatched at once, so it is admitted as Running exactly
        // like a submitted root input. The fresh collaboration state above starts naturally idle, and a
        // successful turn only closes while Running: without this, the edited Turn would stay open after
        // its final answer until the next input stopped it. Forks keep no open Turn and stay idle.
        // 编辑会重新打开根回合并立即分发，因此与提交根输入一样以运行中状态准入。上面新建的协作状态
        // 初始为自然空闲，而成功回合只在运行中才会关闭：缺少此步时，编辑后的回合在最终回答后仍保持
        // 开放，直到下一次输入将其停止。分叉没有开放回合，保持空闲。
        if let Some(start_seq) = document
            .active_turn()
            .filter(|turn| !turn.closed)
            .map(|turn| turn.start_seq)
        {
            let client_action_id = document
                .events
                .iter()
                .find_map(|event| match &event.event {
                    EventBody::UserInput(input) if event.seq == start_seq => {
                        Some(input.client_action_id.clone())
                    }
                    _ => None,
                })
                .ok_or(DomainError::InvalidGenerationInput {
                    field: "target_seq",
                })?;
            // Like the reset and request-switch events above, this event joins the generation's single
            // publication revision instead of advancing a second one.
            // 与上面的重置及请求切换事件一样，此事件并入本代次唯一的发布修订，不再推进第二个修订。
            let publication_revision = document.metadata.document_revision;
            document.set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new(format!(
                    "root-input-running:{}:{}",
                    document.metadata.session_id,
                    client_action_id.as_str()
                ))?,
                state: CollaborationRunControl::Running,
                changed_at_ms: now_ms,
            })?;
            document.metadata.document_revision = publication_revision;
        }
        metadata.id_frontier = document.id_frontier();
        document.metadata.id_frontier = metadata.id_frontier;

        let validated = SessionDocument::from_parts(
            document.metadata.clone(),
            document.runtime_lock.clone(),
            document.compactions.clone(),
            document.segment_catalog.clone(),
            document.events.to_vec(),
        )?;
        Ok(GenerationBuildOutput {
            metadata: validated.metadata,
            runtime_lock: validated.runtime_lock,
            compactions: validated.compactions,
            events: validated.events.into_vec(),
            history_target_seq: target_seq,
        })
    }
}

/// Removes every checkpoint whose digest does not verify against `events` read from its first event.
/// 移除摘要无法从 `events` 首个事件起校验通过的每个检查点。
fn drop_unverifiable_provider_sources(events: &mut [ConversationEvent]) -> Result<(), DomainError> {
    let mismatched: Vec<bool> = {
        let hosted: Vec<&crate::v1::event::ProviderSource> =
            events.iter().filter_map(hosted_provider_source).collect();
        provider_fingerprint_mismatches(events, &[], &hosted)?
            .into_iter()
            .map(|mismatch| mismatch.is_some())
            .collect()
    };
    let mut mismatched = mismatched.into_iter();
    for event in events.iter_mut() {
        let source = match &mut event.event {
            EventBody::AssistantReasoning(reasoning) => &mut reasoning.provider_source,
            EventBody::AssistantText(text) => &mut text.provider_source,
            EventBody::ToolCall(tool) => &mut tool.provider_source,
            _ => continue,
        };
        if source.is_some() && mismatched.next().unwrap_or(false) {
            *source = None;
        }
    }
    Ok(())
}

/// Returns the replay checkpoint one event hosts, if any.
/// 返回一个事件承载的重放检查点(如有)。
fn hosted_provider_source(event: &ConversationEvent) -> Option<&crate::v1::event::ProviderSource> {
    match &event.event {
        EventBody::AssistantReasoning(reasoning) => reasoning.provider_source.as_ref(),
        EventBody::AssistantText(text) => text.provider_source.as_ref(),
        EventBody::ToolCall(tool) => tool.provider_source.as_ref(),
        _ => None,
    }
}

/// Returns active mode and request-context keys that must receive explicit removal facts.
/// 返回必须追加显式移除事实的活动模式及请求上下文语义键。
fn active_generation_context_keys(
    events: &[ConversationEvent],
    through_seq: Seq,
) -> Result<Vec<&'static str>, DomainError> {
    // Active versions come from the retained prefix, never from a newer metadata snapshot.
    // 活动版本来自保留历史前缀，不能从时间更晚的 metadata 快照推断。
    let active = derive_active_system_state(events, through_seq)?;
    let mut result = Vec::new();
    for context_key in [
        GOAL_STATE_CONTEXT_KEY,
        GOAL_CONTINUATION_CONTEXT_KEY,
        SHELL_JOB_CONTINUATION_CONTEXT_KEY,
        PLAN_MODE_CONTEXT_KEY,
        DYNAMIC_CONTEXT_SYSTEM_KEY,
        DYNAMIC_CONTEXT_DEVELOPER_KEY,
        DYNAMIC_CONTEXT_TOOL_GUIDANCE_KEY,
        DYNAMIC_CONTEXT_RUNTIME_KEY,
        DYNAMIC_CONTEXT_PERSISTENT_KEY,
        DYNAMIC_CONTEXT_TRANSIENT_KEY,
        ANSWER_MODE_CONTEXT_KEY,
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
        SHELL_JOB_CONTINUATION_CONTEXT_KEY => {
            "The background command continuation was reset because the conversation history was replaced."
        }
        PLAN_MODE_CONTEXT_KEY => {
            "Plan mode was reset because the conversation history was replaced."
        }
        ANSWER_MODE_CONTEXT_KEY => {
            "The previous answer mode context expired with this history generation. Follow the separately supplied answer mode for new responses."
        }
        DYNAMIC_CONTEXT_SYSTEM_KEY
        | DYNAMIC_CONTEXT_DEVELOPER_KEY
        | DYNAMIC_CONTEXT_TOOL_GUIDANCE_KEY
        | DYNAMIC_CONTEXT_RUNTIME_KEY
        | DYNAMIC_CONTEXT_PERSISTENT_KEY
        | DYNAMIC_CONTEXT_TRANSIENT_KEY => {
            "This context version expired because the conversation history was replaced. New execution context will be supplied separately."
        }
        _ => unreachable!("only frozen generation context keys are passed to the reset builder"),
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
    use std::collections::BTreeSet;

    use super::*;
    use crate::v1::collaboration::{
        AddCollaborationGrantInput, CollaborationDeliveryId, CollaborationExecutionPurpose,
        CollaborationGrantAction, CollaborationGrantAuthority, CollaborationOperationId,
        CollaborationParticipant, CollaborationQuestionId, CreateQuestionInput,
        ReserveChildTaskInput,
    };
    use crate::v1::config::{ActiveRequestConfig, AnswerMode, PendingRequestConfig};
    use crate::v1::document::{ApiCompletion, ClientInputSubmission};
    use crate::v1::dynamic_context::DynamicContextInput;
    use crate::v1::event::{
        ApiCallStatus, ApiEndReason, ApiPurposeKind, ApiRequestSnapshot, ApiTimings,
        AssistantSemanticStatus, ContinuationEligibility,
    };
    use crate::v1::ids::{ClientActionId, SubagentTaskId};
    use crate::v1::stats::UsageSnapshot;

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

    /// Builds the six active request dynamic producers used by generation fixtures.
    /// 构建 generation 夹具使用的六个活动请求动态生产者。
    fn active_dynamic_context_inputs() -> Vec<DynamicContextInput> {
        vec![
            DynamicContextInput::new(
                DYNAMIC_CONTEXT_SYSTEM_KEY,
                Some("system-prefix".to_string()),
                SystemContextScope::Session,
                None,
            ),
            DynamicContextInput::new(
                DYNAMIC_CONTEXT_DEVELOPER_KEY,
                Some("developer-prefix".to_string()),
                SystemContextScope::Session,
                None,
            ),
            DynamicContextInput::new(
                DYNAMIC_CONTEXT_TOOL_GUIDANCE_KEY,
                Some("tool-prefix".to_string()),
                SystemContextScope::Session,
                None,
            ),
            DynamicContextInput::new(
                DYNAMIC_CONTEXT_RUNTIME_KEY,
                Some("runtime-prefix".to_string()),
                SystemContextScope::Session,
                None,
            ),
            DynamicContextInput::new(
                DYNAMIC_CONTEXT_PERSISTENT_KEY,
                Some("persistent-prefix".to_string()),
                SystemContextScope::Session,
                None,
            ),
            DynamicContextInput::new(
                DYNAMIC_CONTEXT_TRANSIENT_KEY,
                Some("transient-prefix".to_string()),
                SystemContextScope::Session,
                None,
            ),
        ]
    }

    /// Builds one durable source with two root turns, an active Plan context, and six dynamic keys before the second root.
    /// 构建一个包含两个根回合、活动 Plan 上下文及六个动态键的可靠源文档。
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
        document
            .synchronize_dynamic_context(active_dynamic_context_inputs(), T0 + 4)
            .unwrap();
        document.metadata.mode.plan.enabled = true;
        document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("root-2").unwrap(),
                    "second",
                    Vec::new(),
                ),
                T0 + 5,
            )
            .unwrap();
        document
            .close_turn(crate::v1::event::TurnOutcome::Completed, 1, T0 + 6)
            .unwrap();
        make_durable(&mut document);
        document
    }

    /// Builds a durable source with real pending question, child, and grant state.
    /// 构建一个带有真实待处理问题、子任务和授权状态的可靠源文档。
    fn durable_fork_source_with_pending_collaboration() -> SessionDocument {
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
        let parent_turn_id = document
            .active_turn()
            .expect("first root must open a parent turn")
            .turn_id;
        let child_task_id = SubagentTaskId::try_new("child-task").unwrap();
        let child = CollaborationParticipant::new(
            "child-session",
            "source-session",
            "project",
            1,
            None,
            Some(child_task_id.clone()),
        )
        .unwrap();
        document
            .reserve_child_task(ReserveChildTaskInput {
                source_tool_name: String::from("spawn_agent"),
                execution_id: crate::v1::collaboration::CollaborationExecutionId::try_new(
                    "child-execution",
                )
                .unwrap(),
                operation_id: CollaborationOperationId::try_new("child-operation").unwrap(),
                task_id: child_task_id,
                parent_turn_id,
                child,
                reserved_at_ms: T0 + 2,
            })
            .unwrap();

        let source = CollaborationParticipant::new(
            "source-session",
            "source-session",
            "project",
            1,
            Some(parent_turn_id),
            None,
        )
        .unwrap();
        let target = CollaborationParticipant::new(
            "target-session",
            "target-session",
            "project",
            1,
            None,
            None,
        )
        .unwrap();
        document
            .create_question(CreateQuestionInput {
                operation_id: CollaborationOperationId::try_new("question-operation").unwrap(),
                question_id: CollaborationQuestionId::try_new("question-id").unwrap(),
                delivery_id: CollaborationDeliveryId::try_new("question-delivery").unwrap(),
                source,
                target,
                body: "pending question".to_string(),
                execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
                created_at_ms: T0 + 3,
            })
            .unwrap();

        let mut actions = BTreeSet::new();
        actions.insert(CollaborationGrantAction::Ask);
        document
            .add_collaboration_grant(AddCollaborationGrantInput {
                operation_id: CollaborationOperationId::try_new("grant-operation").unwrap(),
                grant_id: crate::v1::collaboration::CollaborationGrantId::try_new("grant-id")
                    .unwrap(),
                authority: CollaborationGrantAuthority::LocalUser,
                source_session_id: "source-session".to_string(),
                target_session_id: "target-session".to_string(),
                actions,
                created_at_ms: T0 + 4,
            })
            .unwrap();
        document
            .close_turn(crate::v1::event::TurnOutcome::Stopped, 1, T0 + 5)
            .unwrap();
        document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("root-2").unwrap(),
                    "second",
                    Vec::new(),
                ),
                T0 + 6,
            )
            .unwrap();
        document
            .synchronize_dynamic_context(active_dynamic_context_inputs(), T0 + 7)
            .unwrap();
        document
            .close_turn(crate::v1::event::TurnOutcome::Completed, 1, T0 + 8)
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
        source.metadata.config.active_request = ActiveRequestConfig {
            provider: Some(String::from("minimax")),
            connection_id: Some(String::from("minimax-account")),
            model: Some(String::from("MiniMax-M2.7-highspeed")),
            thinking_level: None,
            temperature: None,
        };
        source.metadata.config.pending_request = Some(PendingRequestConfig {
            provider: Some(String::from("minimax")),
            connection_id: Some(String::from("minimax-account")),
            model: Some(String::from("MiniMax-M2.7-highspeed")),
            thinking_level: Some(String::from("adaptive")),
            temperature: None,
            set_at_ms: T0 + 6,
        });
        let mut post_boundary_inputs = active_dynamic_context_inputs();
        post_boundary_inputs.push(DynamicContextInput::new(
            "dynamic.post_boundary",
            Some("post-boundary".to_string()),
            SystemContextScope::Session,
            None,
        ));
        source
            .synchronize_dynamic_context(post_boundary_inputs, T0 + 7)
            .unwrap();
        make_durable(&mut source);
        let before = source.clone();
        let target_seq = source
            .events()
            .iter()
            .find_map(|event| match &event.event {
                EventBody::UserInput(input) if input.text == "second" => Some(event.seq),
                _ => None,
            })
            .expect("second root input must exist");
        let old_event_revision = source.events()[target_seq.value() as usize - 1].event_revision;
        // replacement_client_action_id records the client that initiated the edit operation.
        // replacement_client_action_id 记录发起编辑操作的客户端。
        let replacement_client_action_id = ClientActionId::try_new(
            "vcn1:4c751b3e-b8de-4ae1-866a-46fa4a093404:8da7f088-04da-444b-918b-a52001dfc3d1",
        )
        .unwrap();
        let output = source
            .build_edited_generation(EditGenerationInput {
                target_seq,
                replacement: RootInputReplacement::new("replacement", Vec::new())
                    .with_client_action_id(replacement_client_action_id.clone()),
                now_ms: T0 + 10,
            })
            .unwrap();

        assert_eq!(source, before);
        assert_eq!(output.metadata.fork_origin, Some(fork_origin));
        assert_eq!(output.history_target_seq, target_seq);
        assert!(!output.metadata.mode.plan.enabled);
        assert_eq!(output.metadata.input_state, InputState::default());
        assert!(output.metadata.config.pending_request.is_none());
        assert_eq!(
            output
                .metadata
                .config
                .active_request
                .thinking_level
                .as_deref(),
            Some("adaptive")
        );
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
        assert_eq!(&replaced.client_action_id, &replacement_client_action_id);
        assert!(output.events.iter().any(|event| matches!(
            &event.event,
            EventBody::SystemContext(SystemContextEvent {
                context_key,
                operation: SystemContextOperation::Remove,
                ..
            }) if context_key == PLAN_MODE_CONTEXT_KEY
        )));
        assert!(output.metadata.dynamic_context.is_empty());
        assert!(!output.events.iter().any(|event| matches!(
            &event.event,
            EventBody::SystemContext(SystemContextEvent { context_key, .. })
                if context_key == "dynamic.post_boundary"
        )));
        let removed_dynamic_keys = output
            .events
            .iter()
            .filter_map(|event| match &event.event {
                EventBody::SystemContext(SystemContextEvent {
                    context_key,
                    operation: SystemContextOperation::Remove,
                    ..
                }) if context_key.starts_with("dynamic.")
                    || context_key.starts_with("injection.") =>
                {
                    Some(context_key.as_str())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            removed_dynamic_keys,
            BTreeSet::from([
                DYNAMIC_CONTEXT_SYSTEM_KEY,
                DYNAMIC_CONTEXT_DEVELOPER_KEY,
                DYNAMIC_CONTEXT_TOOL_GUIDANCE_KEY,
                DYNAMIC_CONTEXT_RUNTIME_KEY,
                DYNAMIC_CONTEXT_PERSISTENT_KEY,
                DYNAMIC_CONTEXT_TRANSIENT_KEY,
            ])
        );
        assert_eq!(
            output.metadata.id_frontier.next_seq,
            output.events.len() as u64 + 1
        );
        // Strict reload already exposes the admitted route, without a dispatch-time promotion.
        // 严格重载已暴露准入路由，无需在分发时提升。
        let dispatch_document = SessionDocument::from_parts(
            output.metadata.clone(),
            output.runtime_lock.clone(),
            output.compactions.clone(),
            Vec::new(),
            output.events.clone(),
        )
        .expect("edited generation should reconstruct for first dispatch");
        assert_eq!(
            dispatch_document.active_turn().unwrap().start_seq,
            target_seq
        );
        assert!(dispatch_document.metadata.config.pending_request.is_none());
        assert_eq!(
            dispatch_document
                .metadata
                .config
                .active_request
                .thinking_level
                .as_deref(),
            Some("adaptive")
        );

        // A rewritten generation without a pending record still establishes its first exact request
        // instead of comparing current model normalization against the previous generation.
        // 不含待生效记录的重写代际仍会建立首次精确请求，而不是把当前模型规范化结果与上一代际比较。
        let mut no_pending_metadata = output.metadata.clone();
        no_pending_metadata.config.pending_request = None;
        let mut rebased_document = SessionDocument::from_parts(
            no_pending_metadata,
            output.runtime_lock.clone(),
            output.compactions.clone(),
            Vec::new(),
            output.events.clone(),
        )
        .expect("edited generation without pending selection should reconstruct");
        let mut adaptive_request =
            crate::v1::event::ApiRequestSnapshot::new("minimax", "MiniMax-M2.7-highspeed");
        adaptive_request.connection_id = Some(String::from("minimax-account"));
        adaptive_request.thinking_level = Some(String::from("adaptive"));
        rebased_document
            .start_api(
                crate::v1::event::ApiPurposeKind::MainGeneration,
                adaptive_request,
                T0 + 11,
            )
            .expect("first edited-generation request should rebase active selection");
        assert_eq!(
            rebased_document
                .metadata
                .config
                .active_request
                .thinking_level
                .as_deref(),
            Some("adaptive")
        );
    }

    /// Editing admits the current model, account and parameters while preserving earlier history.
    /// 编辑会准入当前模型、账号和参数，并保留此前历史。
    /// No parameters; verifies the first API and isolation from later picker changes; returns nothing.
    /// 无参数；验证首次 API 及与后续选择的隔离，不返回值。
    #[test]
    fn edit_generation_admits_selected_model_before_first_api() {
        // Cover provider, model and account-only switches, including Provider-default reasoning.
        // 覆盖供应商、模型和仅账号切换，同时覆盖供应商默认推理。
        for (provider, model, thinking_level) in [
            ("deepseek", "deepseek-v4-flash", Some("high")),
            ("deepseek", "deepseek-v4-pro", Some("off")),
            ("opencode-go", "deepseek-v4-flash", None),
        ] {
            // source represents a completed conversation with an expired old-provider account.
            // source 表示旧供应商账号已过期的已完成会话。
            let mut source = durable_edit_source();
            source.metadata.config.active_request = ActiveRequestConfig {
                provider: Some("opencode-go".into()),
                connection_id: Some("expired-account".into()),
                model: Some("deepseek-v4-flash".into()),
                thinking_level: Some("low".into()),
                temperature: Some(0.2),
            };
            // selected captures every current session picker setting, even when the model ID matches.
            // selected 捕获当前会话选择器的全部设置，即使模型 ID 相同。
            let selected = ActiveRequestConfig {
                provider: Some(provider.into()),
                connection_id: Some("current-account".into()),
                model: Some(model.into()),
                thinking_level: thinking_level.map(str::to_owned),
                temperature: Some(0.7),
            };
            source.metadata.config.pending_request = Some(PendingRequestConfig {
                provider: selected.provider.clone(),
                connection_id: selected.connection_id.clone(),
                model: selected.model.clone(),
                thinking_level: selected.thinking_level.clone(),
                temperature: selected.temperature,
                set_at_ms: T0 + 6,
            });
            // target_seq identifies the second root, leaving earlier events untouched.
            // target_seq 标识第二个根输入，此前事件保持不变。
            let target_seq = source
                .events()
                .iter()
                .find_map(|event| match &event.event {
                    EventBody::UserInput(input) if input.text == "second" => Some(event.seq),
                    _ => None,
                })
                .unwrap();
            // before proves that the pure builder does not mutate its source.
            // before 用于证明纯构建器不修改源文档。
            let before = source.clone();
            // output atomically publishes the new text and admitted request configuration.
            // output 原子发布新正文和准入的请求配置。
            let output = source
                .build_edited_generation(EditGenerationInput {
                    target_seq,
                    replacement: RootInputReplacement::new("new user prompt", Vec::new()),
                    now_ms: T0 + 10,
                })
                .unwrap();
            assert_eq!(source, before);
            assert_eq!(
                output.metadata.document_revision,
                before.metadata.document_revision + 1
            );
            assert_eq!(output.metadata.config.active_request, selected);
            assert!(output.metadata.config.pending_request.is_none());
            assert_eq!(
                &output.events[..target_seq.value() as usize - 1],
                &before.events()[..target_seq.value() as usize - 1]
            );
            assert!(
                matches!(&output.events[target_seq.value() as usize - 1].event,
                EventBody::UserInput(input) if input.text == "new user prompt")
            );
            assert!(output.events.iter().any(|event| matches!(&event.event,
                EventBody::RequestConfigChanged(change) if change.previous == before.metadata.config.active_request && change.current == selected)));
            // dispatch reloads the published facts, without a separate pending promotion.
            // dispatch 重载已发布事实，不单独执行待生效配置提升。
            let mut dispatch = SessionDocument::from_parts(
                output.metadata,
                output.runtime_lock,
                output.compactions,
                Vec::new(),
                output.events,
            )
            .unwrap();
            // later is selected after edit admission and must wait for another user boundary.
            // later 在编辑准入后选择，必须等待另一个用户边界。
            let later = PendingRequestConfig {
                provider: Some("another-provider".into()),
                connection_id: Some("later-account".into()),
                model: Some("later-model".into()),
                thinking_level: None,
                temperature: None,
                set_at_ms: T0 + 11,
            };
            dispatch
                .set_pending_request(later.clone(), T0 + 11)
                .unwrap();
            // request follows the service contract by consuming only the admitted active fields.
            // request 按服务契约仅消费已准入的活动字段。
            let mut request = crate::v1::event::ApiRequestSnapshot::new(
                selected.provider.clone().unwrap(),
                selected.model.clone().unwrap(),
            );
            request.connection_id = selected.connection_id.clone();
            request.thinking_level = selected.thinking_level.clone();
            request.temperature = selected.temperature;
            dispatch
                .start_api(
                    crate::v1::event::ApiPurposeKind::MainGeneration,
                    request,
                    T0 + 12,
                )
                .unwrap();
            assert_eq!(dispatch.metadata.config.active_request, selected);
            assert_eq!(dispatch.metadata.config.pending_request, Some(later));
        }
    }

    /// Edited generation duration starts at the replacement revision rather than original history time.
    /// 编辑 generation 的耗时从替换修订时间开始，而不是原始历史时间。
    #[test]
    fn edited_generation_duration_starts_at_replacement_revision() {
        let source = durable_edit_source();
        let target_seq = source
            .events()
            .iter()
            .find_map(|event| match &event.event {
                EventBody::UserInput(input) if input.text == "second" => Some(event.seq),
                _ => None,
            })
            .expect("second root input must exist");
        let replacement_at_ms = T0 + 10;
        let output = source
            .build_edited_generation(EditGenerationInput {
                target_seq,
                replacement: RootInputReplacement::new("replacement", Vec::new()),
                now_ms: replacement_at_ms,
            })
            .expect("edited generation must build");
        let retained_root = &output.events[target_seq.value() as usize - 1];
        assert_eq!(retained_root.created_at_ms, T0 + 5);
        assert_eq!(retained_root.updated_at_ms, replacement_at_ms);

        let mut edited = SessionDocument::from_parts(
            output.metadata,
            output.runtime_lock,
            output.compactions,
            Vec::new(),
            output.events,
        )
        .expect("edited generation must reconstruct");
        // The reopened root is admitted as Running like a submitted input; only a Running turn closes
        // successfully, so a naturally idle edit would keep its answered Turn open.
        // 重新打开的根回合与提交的输入一样以运行中准入；只有运行中的回合才会成功关闭，
        // 自然空闲的编辑会让已回答的回合一直保持开放。
        assert_eq!(
            edited.collaboration_run_control(),
            CollaborationRunControl::Running
        );
        let turn_id = edited
            .active_turn()
            .expect("edited root must own an open Turn")
            .turn_id;
        let request = ApiRequestSnapshot::new("openai", "gpt-5");
        let api_call_id = edited
            .start_api(
                ApiPurposeKind::MainGeneration,
                request.clone(),
                replacement_at_ms + 1,
            )
            .expect("edited generation API must start");
        edited
            .append_assistant_text(
                api_call_id,
                "answer",
                AssistantSemanticStatus::Completed,
                replacement_at_ms + 2,
            )
            .expect("edited generation answer must append");
        edited
            .complete_api_group(
                api_call_id,
                ApiCompletion {
                    request,
                    usage: UsageSnapshot::default(),
                    status: ApiCallStatus::Completed,
                    end_reason: Some(ApiEndReason::CompletedSuccessfully),
                    error: None,
                    continuation_eligibility: ContinuationEligibility::Forbidden,
                    timing: ApiTimings::default(),
                    provider_source: None,
                    completed_at_ms: replacement_at_ms + 3,
                },
            )
            .expect("edited generation API must complete");
        edited
            .complete_turn_and_select_successor(turn_id, replacement_at_ms + 5)
            .expect("edited generation Turn must complete");

        let duration_ms = edited
            .events()
            .iter()
            .rev()
            .find_map(|event| match &event.event {
                EventBody::TurnStats(stats) => stats.duration_ms,
                _ => None,
            })
            .expect("edited generation must persist Turn duration");
        assert_eq!(duration_ms, 5);
    }

    /// A checkpoint written from a later segment window survives an edit and is dropped by a fork.
    /// 从更晚分段窗口写入的检查点在编辑后保留，在分叉时被移除。
    #[test]
    fn generations_verify_checkpoints_from_their_segment_start() {
        let mut source = SessionDocument::new("source-session", "project", "source", T0);
        source
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("root-1").unwrap(),
                    "first",
                    Vec::new(),
                ),
                T0 + 1,
            )
            .unwrap();
        let rollover = source.events().last().unwrap().seq.value() + 1;
        let segment = |segment_number: u64, start: u64| SegmentInfo {
            segment_number,
            file_name: format!("s{segment_number:07}.jsonl"),
            start_seq: Seq::try_new(start).unwrap(),
            end_seq: None,
            event_count: 0,
            byte_len: 0,
            next_event_local_id: 1,
        };
        source.segment_catalog = vec![segment(1, 1), segment(2, rollover)];
        let turn_id = source.active_turn().unwrap().turn_id;
        let request = ApiRequestSnapshot::new("openai", "gpt-5");
        let api_call_id = source
            .start_api(ApiPurposeKind::MainGeneration, request.clone(), T0 + 2)
            .unwrap();
        source
            .append_assistant_text(
                api_call_id,
                "answer",
                AssistantSemanticStatus::Completed,
                T0 + 3,
            )
            .unwrap();
        source
            .complete_api_group(
                api_call_id,
                ApiCompletion {
                    request,
                    usage: UsageSnapshot::default(),
                    status: ApiCallStatus::Completed,
                    end_reason: Some(ApiEndReason::CompletedSuccessfully),
                    error: None,
                    continuation_eligibility: ContinuationEligibility::Forbidden,
                    timing: ApiTimings::default(),
                    provider_source: Some(crate::v1::event::ProviderSourceInput {
                        provider: "openai".to_string(),
                        profile: None,
                        model: "gpt-5".to_string(),
                        connection_id: None,
                        message_id: Some("m1".to_string()),
                        response_id: None,
                        replay: None,
                    }),
                    completed_at_ms: T0 + 4,
                },
            )
            .unwrap();
        source
            .complete_turn_and_select_successor(turn_id, T0 + 5)
            .unwrap();
        let first_turn_end = source.events().last().unwrap().seq;
        source
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("root-2").unwrap(),
                    "second",
                    Vec::new(),
                ),
                T0 + 6,
            )
            .unwrap();
        let second_root = source.events().last().unwrap().seq;
        source
            .close_turn(crate::v1::event::TurnOutcome::Completed, 1, T0 + 7)
            .unwrap();
        make_durable(&mut source);
        let source_of = |events: &[ConversationEvent]| {
            events
                .iter()
                .find_map(hosted_provider_source)
                .map(|source| (source.covered_until_seq, source.fingerprint_sha256.clone()))
        };
        let (coverage, digest) = source_of(source.events()).expect("checkpoint written");
        // The digest is only reproducible from the later segment start.
        // 摘要只能从较晚的分段起点复现。
        assert_ne!(
            digest,
            crate::v1::projection::provider_visible_fingerprint(source.events(), coverage).unwrap()
        );

        let edited = source
            .build_edited_generation(EditGenerationInput {
                target_seq: second_root,
                replacement: RootInputReplacement::new("replacement", Vec::new()),
                now_ms: T0 + 10,
            })
            .expect("edit keeps the segment start its checkpoint was written from");
        assert_eq!(source_of(&edited.events), Some((coverage, digest)));

        let forked = source
            .build_fork_generation(ForkGenerationInput {
                child_session_id: "child-session".to_string(),
                title: "child".to_string(),
                source_turn_end_seq: first_turn_end,
                now_ms: T0 + 10,
            })
            .expect("fork drops a checkpoint its single segment cannot verify");
        assert_eq!(source_of(&forked.events), None);
    }

    /// Edit generation rejects unsettled collaboration without mutating the source document.
    /// 编辑 generation 拒绝未结算协作，并且不修改源文档。
    #[test]
    fn edit_generation_rejects_unsettled_collaboration_without_mutation() {
        let source = durable_fork_source_with_pending_collaboration();
        assert_eq!(source.collaboration_state().questions.len(), 1);
        assert_eq!(source.collaboration_state().child_tasks.len(), 1);
        assert_eq!(source.collaboration_state().grants.len(), 1);
        assert_eq!(source.collaboration_state().operations.len(), 3);
        let target_seq = source
            .events()
            .iter()
            .find_map(|event| match &event.event {
                EventBody::UserInput(input) if input.text == "second" => Some(event.seq),
                _ => None,
            })
            .expect("second root input must exist");
        let before = source.clone();

        let result = source.build_edited_generation(EditGenerationInput {
            target_seq,
            replacement: RootInputReplacement::new("replacement", Vec::new()),
            now_ms: T0 + 20,
        });

        assert!(matches!(
            result,
            Err(DomainError::InvalidGenerationInput {
                field: "unsettled_collaboration",
            })
        ));
        assert_eq!(source, before);
    }

    /// Fork generation preserves copied fact identities while assigning exact child metadata.
    /// 分叉 generation 保留复制事实身份，同时写入精确子会话 metadata。
    #[test]
    fn fork_generation_preserves_prefix_identity_and_resets_transient_state() {
        let mut source = durable_fork_source_with_pending_collaboration();
        assert_eq!(source.collaboration_state().questions.len(), 1);
        assert_eq!(source.collaboration_state().child_tasks.len(), 1);
        assert_eq!(source.collaboration_state().grants.len(), 1);
        assert_eq!(source.collaboration_state().operations.len(), 3);
        let source_turn_end_seq = source.events().last().unwrap().seq;
        let mut post_boundary_inputs = active_dynamic_context_inputs();
        post_boundary_inputs.push(DynamicContextInput::new(
            "dynamic.post_boundary",
            Some("post-boundary".to_string()),
            SystemContextScope::Session,
            None,
        ));
        source
            .synchronize_dynamic_context(post_boundary_inputs, T0 + 9)
            .unwrap();
        make_durable(&mut source);
        let output = source
            .build_fork_generation(ForkGenerationInput {
                child_session_id: "child-session".to_string(),
                title: "child".to_string(),
                source_turn_end_seq,
                now_ms: T0 + 20,
            })
            .unwrap();

        assert_eq!(output.metadata.session_id, "child-session");
        assert_eq!(
            output.metadata.fork_origin,
            Some(ForkOrigin {
                source_session_id: source.metadata.session_id.clone(),
                source_segment_generation: source.metadata.segment_generation,
                source_target_seq: source_turn_end_seq,
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
            output.events[..source_turn_end_seq.value() as usize],
            source.events()[..source_turn_end_seq.value() as usize]
        );
        assert_eq!(output.history_target_seq, source_turn_end_seq);
        assert_eq!(
            output.metadata.collaboration.history_boundary_seq,
            Some(source_turn_end_seq)
        );
        assert!(
            output
                .events
                .iter()
                .any(|event| { matches!(&event.event, EventBody::Collaboration(_)) })
        );
        assert!(output.metadata.dynamic_context.is_empty());
        assert!(!output.events.iter().any(|event| matches!(
            &event.event,
            EventBody::SystemContext(SystemContextEvent { context_key, .. })
                if context_key == "dynamic.post_boundary"
        )));
        assert!(output.metadata.collaboration.state.questions.is_empty());
        // A fork keeps no open Turn, so it is not admitted as Running.
        // 分叉没有开放回合，因此不以运行中准入。
        assert_eq!(
            output.metadata.collaboration.state.run_control,
            CollaborationRunControl::NaturallyIdle
        );
        assert!(
            output
                .metadata
                .collaboration
                .state
                .outbound_deliveries
                .is_empty()
        );
        assert!(
            output
                .metadata
                .collaboration
                .state
                .inbound_deliveries
                .is_empty()
        );
        assert!(output.metadata.collaboration.state.child_tasks.is_empty());
        assert!(
            output
                .metadata
                .collaboration
                .state
                .question_executions
                .is_empty()
        );
        assert!(output.metadata.collaboration.state.grants.is_empty());
        assert!(output.metadata.collaboration.state.operations.is_empty());
        assert_eq!(output.metadata.open_turn_start_seq, None);
        let restored = SessionDocument::from_parts(
            output.metadata.clone(),
            output.runtime_lock.clone(),
            output.compactions.clone(),
            Vec::new(),
            output.events.clone(),
        )
        .unwrap();
        assert!(restored.active_turn().is_none());
        assert!(
            restored
                .events()
                .iter()
                .any(|event| matches!(&event.event, EventBody::Collaboration(_)))
        );
        assert!(restored.collaboration_state().questions.is_empty());
        assert!(restored.collaboration_state().child_tasks.is_empty());
        assert!(restored.collaboration_state().grants.is_empty());
        assert!(restored.collaboration_state().operations.is_empty());
        assert!(!output.metadata.mode.goal.enabled);
        assert!(!output.metadata.mode.plan.enabled);
        assert_eq!(output.metadata.input_state, InputState::default());
        assert_eq!(
            output.runtime_lock.initialization_state,
            RuntimeInitializationState::BaselineRefreshPending
        );
        assert!(output.runtime_lock.locked_sources.is_empty());
    }

    /// A fork inherits the current desired preference but invalidates the original prepared context.
    /// 分叉继承当前期望偏好，但使原始准备上下文失效。
    #[test]
    fn answer_mode_fork_inherits_preference_and_reprepares_context() {
        let mut source = SessionDocument::new("mode-parent", "p", "parent", T0);
        source
            .submit_root_input_fixture("hello", Vec::new(), T0 + 1)
            .unwrap();
        source
            .set_answer_mode(AnswerMode::Interactive, 0, T0 + 2)
            .unwrap();
        source
            .synchronize_context_with_answer_mode(
                Vec::new(),
                "components on",
                "components off",
                T0 + 3,
            )
            .unwrap();
        source
            .close_turn(crate::v1::event::TurnOutcome::Completed, 1, T0 + 4)
            .unwrap();
        make_durable(&mut source);
        let boundary = source
            .events()
            .iter()
            .find(|event| matches!(event.event, EventBody::TurnStats(_)))
            .unwrap()
            .seq;
        let output = source
            .build_fork_generation(ForkGenerationInput {
                child_session_id: "mode-child".into(),
                title: "child".into(),
                source_turn_end_seq: boundary,
                now_ms: T0 + 5,
            })
            .unwrap();
        assert_eq!(
            output.metadata.config.answer_mode.mode,
            AnswerMode::Interactive
        );
        assert_eq!(
            output.metadata.config.answer_mode.revision,
            source.metadata.config.answer_mode.revision
        );
        assert_eq!(output.metadata.config.answer_mode.prepared, None);
        assert!(output.events.iter().any(|event| matches!(&event.event,
            EventBody::SystemContext(context) if context.context_key == ANSWER_MODE_CONTEXT_KEY && context.operation == SystemContextOperation::Remove
        )));
        let mut restored = SessionDocument::from_parts(
            output.metadata,
            output.runtime_lock,
            output.compactions,
            Vec::new(),
            output.events,
        )
        .unwrap();
        assert_eq!(
            restored
                .synchronize_context_with_answer_mode(
                    Vec::new(),
                    "components on",
                    "components off",
                    T0 + 6
                )
                .unwrap()
                .into_value()
                .len(),
            1
        );
        assert_eq!(
            restored.metadata.config.answer_mode.prepared.unwrap().mode,
            AnswerMode::Interactive
        );
    }

    /// Fork generation rejects a root-input anchor because only a completed Turn end is valid.
    /// 分叉 generation 拒绝根输入锚点，因为只有已完成回合的结束点才有效。
    #[test]
    fn fork_generation_rejects_root_input_anchor() {
        let source = durable_edit_source();
        let root_input_seq = source
            .events()
            .iter()
            .find_map(|event| match &event.event {
                EventBody::UserInput(_) => Some(event.seq),
                _ => None,
            })
            .expect("source root input must exist");

        assert!(matches!(
            source.build_fork_generation(ForkGenerationInput {
                child_session_id: "child-session".to_string(),
                title: "child".to_string(),
                source_turn_end_seq: root_input_seq,
                now_ms: T0 + 20,
            }),
            Err(DomainError::InvalidGenerationInput {
                field: "source_turn_end_event_kind",
            })
        ));
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
