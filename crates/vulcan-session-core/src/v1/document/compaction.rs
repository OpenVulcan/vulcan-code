//! Context compaction commits and validation of the document's locked generation.
//! 上下文压缩提交及文档锁定代际校验。

use super::persistence::DirtyFile;
use super::{IdempotentMutation, SessionDocument, ensure_api_request_field, group_event_api_id};
use crate::v1::compaction_frontier::{assert_node_order, trim_unpublished_candidates};
use crate::v1::context::{
    CompactionContent, CompactionNode, LockedBaselineProjection, RemoteResponsesCheckpoint,
    RuntimeInitializationState,
};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ApiCallStatus, ApiPurposeKind, AssistantSemanticStatus, EventBody, ProviderProjectionProof,
};
use crate::v1::ids::{ApiCallId, CompactionId, JS_SAFE_INTEGER_MAX, Seq};

/// Checks `checkpoint` against the API's frozen `request`; returns a field-specific error on drift.
/// 针对 API 冻结的`request`检查`checkpoint`；漂移时返回字段级错误。
/// The host separately verifies the endpoint against its immutable prepared transport route.
/// 宿主另行针对不可变的已准备传输路由核对端点。
fn validate_remote_checkpoint_route(
    checkpoint: &RemoteResponsesCheckpoint,
    request: &crate::v1::event::ApiRequestSnapshot,
) -> Result<(), DomainError> {
    checkpoint.validate()?;
    ensure_api_request_field(
        "compaction.provider",
        &request.provider,
        &checkpoint.provider_id,
    )?;
    ensure_api_request_field("compaction.model", &request.model, &checkpoint.model_id)?;
    ensure_api_request_field(
        "compaction.connection",
        &request.connection_id,
        &Some(checkpoint.connection_id.clone()),
    )?;
    ensure_api_request_field(
        "compaction.profile",
        &request.profile,
        &Some(checkpoint.protocol_profile_id.clone()),
    )?;
    Ok(())
}

/// Strongly typed atomic input for publishing one context-compaction generation.
/// 发布一个上下文压缩代际的强类型原子输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionCommitInput {
    /// Physical compaction API identity.
    /// 物理压缩 API 身份。
    pub api_call_id: ApiCallId,
    /// Summary semantic seq, or the terminal API seq for a remote checkpoint.
    /// 摘要语义 seq，或远端检查点的 API 终态 seq。
    pub occurred_at_seq: Seq,
    /// Inclusive first event retained by the new Provider window.
    /// 新 Provider 窗口包含的首个保留事件。
    pub retained_from_seq: Seq,
    /// Exact local summary or canonical opaque remote output.
    /// 精确本地摘要或规范不透明远端输出。
    pub content: CompactionContent,
    /// Exact eight-kind locked baseline segments refreshed for the new generation.
    /// 为新代际刷新的精确八种锁定基线片段。
    pub locked_baseline: LockedBaselineProjection,
}

impl CompactionCommitInput {
    /// Builds one complete compaction commit input without classifying any source text.
    /// 构建一份完整压缩提交输入，且不对任何来源正文做分类猜测。
    pub fn new(
        api_call_id: ApiCallId,
        occurred_at_seq: Seq,
        retained_from_seq: Seq,
        summary: impl Into<String>,
        locked_baseline: LockedBaselineProjection,
    ) -> Self {
        Self {
            api_call_id,
            occurred_at_seq,
            retained_from_seq,
            content: CompactionContent::Summary {
                text: summary.into(),
            },
            locked_baseline,
        }
    }

    /// Builds a remote commit from `api_call_id`, its durable `api_stats_seq`, the retained boundary,
    /// exact provider `checkpoint`, and the admitted `locked_baseline`; validation occurs on commit.
    /// 根据`api_call_id`、其可靠`api_stats_seq`、保留边界、精确供应商`checkpoint`及准入
    /// `locked_baseline`构建远端提交；提交时执行校验。
    pub fn remote_responses(
        api_call_id: ApiCallId,
        api_stats_seq: Seq,
        retained_from_seq: Seq,
        checkpoint: RemoteResponsesCheckpoint,
        locked_baseline: LockedBaselineProjection,
    ) -> Self {
        Self {
            api_call_id,
            occurred_at_seq: api_stats_seq,
            retained_from_seq,
            content: CompactionContent::RemoteResponses { checkpoint },
            locked_baseline,
        }
    }
}

/// Validates exact final locked segments without deriving kind, target, identity, or order.
/// 校验精确的最终锁定片段，且不推导其种类、目标层、身份或顺序。
/// Validates one exact request projection against the current frozen document frontier.
/// 针对当前冻结文档前沿校验一份精确请求投影。
pub(super) fn validate_provider_projection_proof(
    document: &SessionDocument,
    proof: &ProviderProjectionProof,
) -> Result<(), DomainError> {
    validate_provider_projection_proof_shape(proof)?;
    if proof.segment_generation != document.metadata.segment_generation {
        return Err(DomainError::InvalidProviderProjection {
            field: "segment_generation",
        });
    }
    if proof.document_revision != document.memory_revision {
        return Err(DomainError::InvalidProviderProjection {
            field: "document_revision",
        });
    }
    if proof.through_seq != document.last_seq()? {
        return Err(DomainError::InvalidProviderProjection {
            field: "through_seq",
        });
    }
    let expected_retained = match document.runtime_lock.current_compaction_id {
        Some(compaction_id) => document
            .compactions
            .iter()
            .find(|node| node.compaction_id == compaction_id)
            .map(|node| node.retained_from_seq)
            .ok_or_else(|| DomainError::reference("current_compaction", compaction_id.value()))?,
        None => Seq::try_new(1)?,
    };
    if proof.retained_from_seq != expected_retained {
        return Err(DomainError::InvalidProviderProjection {
            field: "retained_from_seq",
        });
    }
    Ok(())
}

/// Validates the storage-independent shape of one persisted projection proof.
/// 校验一份持久化投影证明与存储代次无关的结构。
pub(super) fn validate_provider_projection_proof_shape(
    proof: &ProviderProjectionProof,
) -> Result<(), DomainError> {
    if proof.segment_generation == 0 || proof.segment_generation > JS_SAFE_INTEGER_MAX {
        return Err(DomainError::InvalidProviderProjection {
            field: "segment_generation",
        });
    }
    if proof.document_revision == 0 || proof.document_revision > JS_SAFE_INTEGER_MAX {
        return Err(DomainError::InvalidProviderProjection {
            field: "document_revision",
        });
    }
    if proof.retained_from_seq.value() > proof.through_seq.value() {
        return Err(DomainError::InvalidProviderProjection {
            field: "retained_from_seq",
        });
    }
    if proof.fingerprint_sha256.len() != 64
        || !proof
            .fingerprint_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(DomainError::InvalidProviderProjection {
            field: "fingerprint_sha256",
        });
    }
    Ok(())
}

// Keep all mutations on the single authoritative document instance.
// 所有修改仍作用于唯一权威文档实例。
impl SessionDocument {
    /// Records one compaction node and advances the runtime pointer.
    /// 记录一个压缩节点并推进 runtime 指针。
    ///
    /// Node appends first; the runtime pointer is published separately by the
    /// writer so a crash between the two files keeps the old generation valid.
    /// 节点先追加；runtime 指针由写入器单独发布，使两文件之间的崩溃保持旧
    /// 代次仍然有效。
    pub fn record_compaction(
        &mut self,
        input: CompactionCommitInput,
        now_ms: i64,
    ) -> Result<IdempotentMutation<CompactionId>, DomainError> {
        if self.runtime_lock.initialization_state != RuntimeInitializationState::Initialized {
            return Err(DomainError::RuntimeBaselineUninitialized);
        }
        let CompactionCommitInput {
            api_call_id,
            occurred_at_seq,
            retained_from_seq,
            content,
            locked_baseline,
        } = input;
        let locked_sources = locked_baseline.into_locked_sources();
        let started = self
            .events
            .iter()
            .find_map(|event| match &event.event {
                EventBody::ApiStarted(started) if started.api_call_id == api_call_id => {
                    Some(started)
                }
                _ => None,
            })
            .ok_or_else(|| DomainError::reference("compaction_api_started", api_call_id.value()))?;
        ensure_api_request_field(
            "compaction.purpose",
            &ApiPurposeKind::Compaction,
            &started.purpose,
        )?;
        let (api_stats_seq, api_stats) = self
            .events
            .iter()
            .find_map(|event| match &event.event {
                EventBody::ApiStats(stats) if stats.api_call_id == api_call_id => {
                    Some((event.seq, stats))
                }
                _ => None,
            })
            .ok_or_else(|| DomainError::reference("compaction_api_stats", api_call_id.value()))?;
        ensure_api_request_field(
            "compaction.status",
            &ApiCallStatus::Completed,
            &api_stats.status,
        )?;
        let last_provider_event = self
            .events
            .iter()
            .filter(|event| {
                group_event_api_id(event) == Some(api_call_id.value())
                    && matches!(
                        &event.event,
                        EventBody::AssistantReasoning(_)
                            | EventBody::AssistantText(_)
                            | EventBody::ToolCall(_)
                    )
            })
            .max_by_key(|event| event.seq.value());
        match &content {
            CompactionContent::Summary { text: summary } => {
                let last_provider_event = last_provider_event.ok_or_else(|| {
                    DomainError::reference("compaction_summary", api_call_id.value())
                })?;
                if last_provider_event.seq != occurred_at_seq {
                    return Err(DomainError::SequenceViolation {
                        expected: last_provider_event.seq.value(),
                        actual: occurred_at_seq.value(),
                    });
                }
                let persisted_summary = match &last_provider_event.event {
                    EventBody::AssistantText(text)
                        if text.status == AssistantSemanticStatus::Completed =>
                    {
                        &text.text
                    }
                    EventBody::AssistantReasoning(reasoning)
                        if reasoning.status == AssistantSemanticStatus::Completed =>
                    {
                        &reasoning.text
                    }
                    _ => {
                        return Err(DomainError::ImmutableFieldMutation {
                            entity: "compaction",
                            field: "summary_source",
                        });
                    }
                };
                if persisted_summary != summary {
                    return Err(DomainError::ImmutableFieldMutation {
                        entity: "compaction",
                        field: "summary",
                    });
                }
                if occurred_at_seq >= api_stats_seq {
                    return Err(DomainError::SequenceViolation {
                        expected: occurred_at_seq.value(),
                        actual: api_stats_seq.value(),
                    });
                }
            }
            CompactionContent::RemoteResponses { checkpoint } => {
                validate_remote_checkpoint_route(checkpoint, &started.request)?;
                if occurred_at_seq != api_stats_seq || last_provider_event.is_some() {
                    return Err(DomainError::ImmutableFieldMutation {
                        entity: "compaction",
                        field: "remote_source",
                    });
                }
            }
        }
        if retained_from_seq > occurred_at_seq {
            return Err(DomainError::SequenceViolation {
                expected: occurred_at_seq.value(),
                actual: retained_from_seq.value(),
            });
        }
        if let Some(existing) = self
            .compactions
            .iter()
            .find(|node| node.api_call_id == api_call_id)
        {
            if existing.api_stats_seq != api_stats_seq
                || existing.occurred_at_seq != occurred_at_seq
                || existing.retained_from_seq != retained_from_seq
                || existing.content != content
            {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "existing_node",
                });
            }
            if self.runtime_lock.current_compaction_id != Some(existing.compaction_id) {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "current_compaction_id",
                });
            }
            if self.runtime_lock.locked_sources != locked_sources {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "locked_baseline",
                });
            }
            return Ok(IdempotentMutation::Idempotent(existing.compaction_id));
        }
        // The next formal node write first drops unpublished candidates left
        // over from a failed runtime write, so an in-process failure can never
        // cause the next compaction to append on top of them (plan 6.2/5.8.3,
        // audit P0#7). Only nodes at or before the durably published pointer
        // survive; the loader ignores but never deletes on-disk candidates.
        // 下一次正式节点写入先把 runtime 写失败残留的未发布候选裁剪掉，同进程
        // 失败绝不可能使下一次压缩基于它们继续追加(计划 6.2/5.8.3、审计
        // P0#7)。只有已持久化指针之前(含)的节点保留；加载器只忽略、绝不删除
        // 磁盘候选。
        let mut staged_allocator = self.id_allocator;
        let compaction_id = CompactionId::try_new(staged_allocator.next_local_id()?)?;
        let mut staged_compactions =
            trim_unpublished_candidates(&self.compactions, self.published_compaction_id);
        staged_compactions.push(CompactionNode {
            compaction_id,
            api_call_id,
            api_stats_seq,
            occurred_at_seq,
            retained_from_seq,
            content,
            created_at_ms: now_ms,
        });
        // A delayed result cannot publish an older generation after a newer node has become durable.
        // 迟到结果不能在较新节点已可靠发布后再发布更旧代次。
        if !assert_node_order(&staged_compactions) {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "compactions",
                field: "node_order",
            });
        }
        let mut staged_runtime_lock = self.runtime_lock.clone();
        staged_runtime_lock.current_compaction_id = Some(compaction_id);
        staged_runtime_lock.locked_sources = locked_sources;
        self.id_allocator = staged_allocator;
        self.compactions = staged_compactions;
        self.runtime_lock = staged_runtime_lock;
        self.bump_revision(&[DirtyFile::Compactions, DirtyFile::Runtime], now_ms, &[]);
        Ok(IdempotentMutation::Committed(compaction_id))
    }

    /// Replaces the locked sources of the current context generation.
    /// 替换当前上下文代次的锁定来源。
    pub fn initialize_runtime_baseline(
        &mut self,
        baseline: LockedBaselineProjection,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        let sources = baseline.into_locked_sources();
        match self.runtime_lock.initialization_state {
            RuntimeInitializationState::FreshUninitialized
            | RuntimeInitializationState::BaselineRefreshPending => {
                if !self.runtime_lock.locked_sources.is_empty() {
                    return Err(DomainError::InvalidRuntimeInitializationState {
                        field: "locked_sources",
                    });
                }
                if self.runtime_lock.initialization_state
                    == RuntimeInitializationState::FreshUninitialized
                    && self.runtime_lock.current_compaction_id.is_some()
                {
                    return Err(DomainError::InvalidRuntimeInitializationState {
                        field: "current_compaction_id",
                    });
                }
                self.runtime_lock.initialization_state = RuntimeInitializationState::Initialized;
                self.runtime_lock.locked_sources = sources;
                self.bump_revision(&[DirtyFile::Runtime], now_ms, &[]);
                Ok(true)
            }
            RuntimeInitializationState::Initialized
                if self.runtime_lock.locked_sources == sources =>
            {
                Ok(false)
            }
            RuntimeInitializationState::Initialized => {
                Err(DomainError::RuntimeBaselineAlreadyInitialized)
            }
        }
    }

    /// Validates that persisted Provider API facts never exist under a fresh runtime baseline.
    /// 校验新鲜运行时基线下绝不存在已持久化的 Provider API 事实。
    pub(super) fn validate_runtime_initialization_state(&self) -> Result<(), DomainError> {
        match self.runtime_lock.initialization_state {
            RuntimeInitializationState::Initialized => Ok(()),
            RuntimeInitializationState::BaselineRefreshPending => {
                if self.runtime_lock.locked_sources.is_empty() {
                    Ok(())
                } else {
                    Err(DomainError::InvalidRuntimeInitializationState {
                        field: "locked_sources",
                    })
                }
            }
            RuntimeInitializationState::FreshUninitialized => {
                if !self.runtime_lock.locked_sources.is_empty() {
                    return Err(DomainError::InvalidRuntimeInitializationState {
                        field: "locked_sources",
                    });
                }
                if self.runtime_lock.current_compaction_id.is_some() {
                    return Err(DomainError::InvalidRuntimeInitializationState {
                        field: "current_compaction_id",
                    });
                }
                if self
                    .events
                    .iter()
                    .any(|event| matches!(event.event, EventBody::ApiStarted(_)))
                {
                    return Err(DomainError::RuntimeBaselineUninitialized);
                }
                Ok(())
            }
        }
    }

    /// Validates the compaction document and its runtime publication pointer.
    /// 校验压缩文档及其 runtime 发布指针。
    pub(super) fn validate_compaction_state(&self) -> Result<(), DomainError> {
        if !assert_node_order(&self.compactions) {
            return Err(DomainError::ImmutableFieldMutation {
                entity: "compactions",
                field: "node_order",
            });
        }
        if let Some(pointer) = self.runtime_lock.current_compaction_id
            && !self
                .compactions
                .iter()
                .any(|node| node.compaction_id == pointer)
        {
            return Err(DomainError::reference(
                "runtime_compaction_id",
                pointer.value(),
            ));
        }
        for node in &self.compactions {
            // Both payload kinds remain validated even when their API events are in sealed storage.
            // 即使 API 事件已经进入封存存储，两种载荷仍需验证。
            let invalid_occurrence = match &node.content {
                CompactionContent::Summary { .. } => node.occurred_at_seq >= node.api_stats_seq,
                CompactionContent::RemoteResponses { checkpoint } => {
                    checkpoint.validate()?;
                    node.occurred_at_seq != node.api_stats_seq
                }
            };
            if node.retained_from_seq > node.occurred_at_seq || invalid_occurrence {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "seq_boundaries",
                });
            }
            if node.occurred_at_seq.value() < self.resident_start_seq {
                continue;
            }
            let started = self.events.iter().find_map(|event| match &event.event {
                EventBody::ApiStarted(started) if started.api_call_id == node.api_call_id => {
                    Some(started)
                }
                _ => None,
            });
            let Some(started) = started else {
                return Err(DomainError::reference(
                    "compaction_api_started",
                    node.api_call_id.value(),
                ));
            };
            if started.purpose != ApiPurposeKind::Compaction {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "api_purpose",
                });
            }
            let stats = self.events.iter().find_map(|event| match &event.event {
                EventBody::ApiStats(stats)
                    if event.seq == node.api_stats_seq && stats.api_call_id == node.api_call_id =>
                {
                    Some(stats)
                }
                _ => None,
            });
            let Some(stats) = stats else {
                return Err(DomainError::reference(
                    "compaction_api_stats",
                    node.api_stats_seq.value(),
                ));
            };
            if stats.status != ApiCallStatus::Completed {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "api_status",
                });
            }
            let CompactionContent::Summary { text: summary } = &node.content else {
                if let CompactionContent::RemoteResponses { checkpoint } = &node.content {
                    validate_remote_checkpoint_route(checkpoint, &started.request)?;
                }
                if self.events.iter().any(|event| {
                    group_event_api_id(event) == Some(node.api_call_id.value())
                        && matches!(
                            &event.event,
                            EventBody::AssistantReasoning(_)
                                | EventBody::AssistantText(_)
                                | EventBody::ToolCall(_)
                        )
                }) {
                    return Err(DomainError::ImmutableFieldMutation {
                        entity: "compaction",
                        field: "remote_source",
                    });
                }
                continue;
            };
            let summary_matches = self.events.iter().any(|event| {
                if event.seq != node.occurred_at_seq {
                    return false;
                }
                match &event.event {
                    EventBody::AssistantText(text) => {
                        text.api_call_id == node.api_call_id
                            && text.status == AssistantSemanticStatus::Completed
                            && &text.text == summary
                    }
                    EventBody::AssistantReasoning(reasoning) => {
                        reasoning.api_call_id == node.api_call_id
                            && reasoning.status == AssistantSemanticStatus::Completed
                            && &reasoning.text == summary
                    }
                    _ => false,
                }
            });
            if !summary_matches {
                return Err(DomainError::ImmutableFieldMutation {
                    entity: "compaction",
                    field: "summary_source",
                });
            }
        }
        Ok(())
    }

    /// Selects the persisted context-compaction node for the next safe request
    /// boundary without changing physical segment rollover thresholds.
    /// 为下一个安全请求边界选择持久化上下文压缩节点，不改变物理分片滚段阈值。
    pub fn set_context_compaction_window_node_id(
        &mut self,
        node_id: Option<vulcan_ai_contract::ContextCompactionWindowNodeId>,
        now_ms: i64,
    ) -> bool {
        if self.metadata.config.context_compaction_window_node_id == node_id {
            return false;
        }
        self.metadata.config.context_compaction_window_node_id = node_id;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        true
    }
}
