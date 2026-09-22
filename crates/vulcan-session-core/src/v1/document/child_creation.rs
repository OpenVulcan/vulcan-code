//! Atomic parent reservation and reproducible initial child creation.
//! 原子父预留与可重建的初始子创建。

use super::*;
use crate::v1::child_execution::{
    ChildCreationIntent, ChildCreationOrigin, ChildExecutionPreparation,
};
use crate::v1::collaboration::{
    CollaborationChildTaskReceipt, CollaborationOperationId, CollaborationRunControl,
    ReserveChildTaskInput, SetRunControlInput,
};

mod resumption;

/// Allocates the real initial root and Running control without publishing a file.
/// 分配真实初始根输入及 Running 控制，不发布文件。
/// Inputs are exact creation facts; returns the unpublished document or an explicit domain error.
/// 输入为精确创建事实；返回未发布文档或明确领域错误。
fn initial_child_document(
    session_id: &str,
    project_tag: &str,
    title: &str,
    execution_id: &str,
    prompt: &str,
    created_at_ms: i64,
) -> Result<SessionDocument, DomainError> {
    validate_timestamp("child_creation.created_at_ms", created_at_ms)?;
    validate_exact_external_identity("child_creation.project_tag", project_tag)?;
    // Allocate through production transitions; no numeric turn or generation is guessed.
    // 通过生产转换分配，不猜测数字回合或代次。
    let mut document = SessionDocument::new(session_id, project_tag, title, created_at_ms);
    document.submit_root_input(
        ClientInputSubmission::new(ClientActionId::try_new(execution_id)?, prompt, vec![]),
        created_at_ms,
    )?;
    document.set_collaboration_run_control(SetRunControlInput {
        operation_id: CollaborationOperationId::try_new(format!(
            "root-input-running:{session_id}:{execution_id}",
        ))?,
        state: CollaborationRunControl::Running,
        changed_at_ms: created_at_ms,
    })?;
    Ok(document)
}

impl ChildCreationIntent {
    /// Rebuilds the exact initial document and rejects coordinates the allocator cannot reproduce.
    /// 重建精确初始文档，拒绝分配器无法复现的坐标。
    /// Returns an unpublished document; the host must separately verify the owning parent reservation.
    /// 返回未发布文档；宿主必须另行验证所属父预留。
    pub fn rebuild_initial_document(&self) -> Result<SessionDocument, DomainError> {
        if self.origin != ChildCreationOrigin::Initial {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_creation.initial_origin",
            });
        }
        // Reconstruct only a new child, never a resumed child's existing history.
        // 仅重建新子任务，不重建续跑子任务的既有历史。
        let mut document = initial_child_document(
            self.profile.task_id.as_str(),
            &self.project_tag,
            &self.profile.runtime_identity.display_title,
            &self.profile.business_execution_id,
            &self.profile.invocation_config.prompt,
            self.created_at_ms,
        )?;
        document.capture_child_execution_profile(self.profile.clone(), self.created_at_ms)?;
        Ok(document)
    }
}

impl SessionDocument {
    /// Builds one initial child and its exact parent-owned creation source from prepared settings.
    /// 从已准备设置构建初始子文档及其精确父侧创建来源。
    /// `project_tag` and `created_at_ms` are host facts; neither result has crossed a persistence boundary.
    /// `project_tag` 和 `created_at_ms` 为宿主事实；两个返回结果均未越过持久化边界。
    pub fn prepare_initial_child_creation(
        preparation: ChildExecutionPreparation,
        project_tag: &str,
        created_at_ms: i64,
    ) -> Result<(Self, ChildCreationIntent), DomainError> {
        // The prototype supplies actual canonical coordinates before the parent records its intent.
        // 原型在父侧记录意图之前提供真实规范坐标。
        let mut document = initial_child_document(
            &preparation.task_id.to_string(),
            project_tag,
            &preparation.runtime_identity.display_title,
            &preparation.execution_id,
            &preparation.invocation_config.prompt,
            created_at_ms,
        )?;
        let turn_id = document.require_open_turn()?.turn_id;
        let profile = preparation.bind_child_turn(document.segment_generation(), turn_id)?;
        document.capture_child_execution_profile(profile.clone(), created_at_ms)?;
        let intent = ChildCreationIntent {
            origin: ChildCreationOrigin::Initial,
            profile,
            project_tag: project_tag.to_owned(),
            created_at_ms,
        };
        Ok((document, intent))
    }

    /// Reserves an initial or resumed child execution and its immutable source in one failure-atomic revision.
    /// 在一个失败原子的修订中预留初始或续跑子执行及其不可变来源。
    /// `input` is host-authenticated; `intent` comes from the actual unpublished execution prototype.
    /// `input` 由宿主认证；`intent` 来自真实未发布执行原型。
    /// Exact retries return the original reservation; conflicts leave the parent unchanged.
    /// 精确重试返回原预留；冲突保持父文档不变。
    pub fn reserve_child_creation(
        &mut self,
        input: ReserveChildTaskInput,
        intent: ChildCreationIntent,
    ) -> Result<IdempotentMutation<CollaborationChildTaskReceipt>, DomainError> {
        // Initial reconstruction or an explicit predecessor revision validates the declared publication mode.
        // 初始重建或明确前序修订验证声明的发布模式。
        intent.validate_creation_source()?;
        let execution_id = input.execution_id.clone();
        if let Some(existing) = self.metadata.child_creation_intents.get(&execution_id) {
            if existing != &intent {
                return Err(DomainError::CollaborationStateViolation {
                    field: "child_creation.immutable_intent",
                });
            }
            self.validate_child_creation_intents()?;
            return self.reserve_child_task(input);
        }
        // An old reservation cannot retroactively acquire a different creation authority.
        // 已有预留不能事后获得另一份创建权威。
        if self
            .collaboration_state()
            .child_tasks
            .contains_key(&execution_id)
            || matches!(
                self.collaboration_run_control(),
                CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents
            )
        {
            return Err(DomainError::CollaborationStateViolation {
                field: "child_creation.reservation_unavailable",
            });
        }
        self.apply_atomic_transition(move |staged| {
            let receipt = staged.reserve_child_task(input)?;
            staged
                .metadata
                .child_creation_intents
                .insert(execution_id, intent);
            staged.validate_child_creation_intents()?;
            // Reservation already marks Metadata dirty at this composite revision.
            // 预留已经在此复合修订中将 Metadata 标脏。
            Ok(receipt)
        })
    }

    /// Verifies persisted creation sources belong to exact retained reservations in this generation.
    /// 验证持久创建来源属于此代次精确保留的预留。
    /// Returns an error for orphan, foreign, rewritten or non-reproducible creation records.
    /// 孤立、外来、改写或无法重建的创建记录返回错误。
    pub(super) fn validate_child_creation_intents(&self) -> Result<(), DomainError> {
        for (execution_id, intent) in &self.metadata.child_creation_intents {
            let task = self
                .collaboration_state()
                .child_tasks
                .get(execution_id)
                .ok_or(DomainError::CollaborationStateViolation {
                    field: "child_creation.orphan_intent",
                })?;
            let profile = &intent.profile;
            if profile.business_execution_id != execution_id.as_str()
                || profile.parent_session_id != self.metadata.session_id
                || task.parent_segment_generation != self.segment_generation()
                || task.task_id != profile.task_id
                || task.child.session_id != profile.task_id.as_str()
                || task.child.root_session_id != profile.root_session_id
                || task.child.project_id != intent.project_tag
                || intent.project_tag != self.metadata.project_tag
                || task.child.segment_generation != profile.segment_generation
                || task
                    .child
                    .turn_id
                    .is_some_and(|turn| turn != profile.business_turn_id)
            {
                return Err(DomainError::CollaborationStateViolation {
                    field: "child_creation.reservation_binding",
                });
            }
            intent.validate_creation_source()?;
        }
        Ok(())
    }
}
