//! Reconstructs owned terminal results from exact durable native child executions.
//! 从精确可靠原生子执行重建所属终态结果。

use super::*;
use vulcan_session_core::v1::collaboration::{
    CollaborationChildTaskRecord, CollaborationChildTaskState,
};

/// Repairs closed source executions while leaving unfinished work open.
/// 修复已闭合来源执行，同时保持未完成任务开放。
/// `records` are discovery hints; canonical parents and children are revalidated under lifecycle gates.
/// `records` 是发现线索；在生命周期锁内重新校验规范父子文档。
/// Returns the number of newly published terminal results; never starts a provider or fabricates text.
/// 返回新发布终态结果数量；绝不启动供应商或制造正文。
pub(super) async fn recover_terminal_returns(
    registry: &Arc<SessionRuntimeRegistry>,
    parent: Arc<SessionHandle>,
    child: Arc<SessionHandle>,
    records: Vec<CollaborationChildTaskRecord>,
) -> Result<usize, String> {
    lifecycle_write::with_canonical_handles(
        registry,
        vec![parent.clone(), child.clone()],
        move || {
            // A physical child can retain several historical executions, each with separate source proof.
            // 一个物理子会话可保留多次历史执行，每次都有独立来源凭据。
            let mut repaired = 0;
            for original in records {
                // Discovery never authorizes publication; read the current durable canonical reservation.
                // 发现绝不授权发布；读取当前可靠规范预留。
                let current = parent
                    .read_full_durable(|document| {
                        if document.segment_generation() != original.parent_segment_generation {
                            return Err(String::from("native result parent generation changed"));
                        }
                        document
                            .collaboration_state()
                            .child_tasks
                            .get(&original.execution_id)
                            .cloned()
                            .ok_or_else(|| String::from("native result reservation is missing"))
                    })
                    .map_err(|error| error.to_string())??;
                if current.child != original.child
                    || current.task_id != original.task_id
                    || current.parent_turn_id != original.parent_turn_id
                    || current.reservation_operation_id != original.reservation_operation_id
                    || current.source_tool_name != original.source_tool_name
                {
                    return Err(String::from(
                        "native result reservation changed after discovery",
                    ));
                }
                if current.state == CollaborationChildTaskState::Terminal {
                    continue;
                }
                // Resolve the original root action and business terminal from full durable source history.
                // 从完整可靠来源历史解析原根动作与业务终态。
                let request = child
                    .read_full_durable(|document| {
                        if document.segment_generation() != current.child.segment_generation
                            || document.metadata.session_id != current.child.session_id
                        {
                            return Err(String::from("native result child scope changed"));
                        }
                        document
                            .recover_completed_child_return_request(
                                &current.execution_id,
                                current.task_id.clone(),
                            )
                            .map_err(|error| error.to_string())
                    })
                    .map_err(|error| error.to_string())??;
                let Some(request) = request else {
                    // A persisted host reason supplies failure versus stop; ordinary diagnostics never classify it.
                    // 持久宿主原因提供失败与停止分类；普通诊断绝不负责分类。
                    let non_success = child
                        .read_full_durable(|document| {
                            document
                                .recover_child_task_non_success_input(current.execution_id.clone())
                        })
                        .map_err(|error| error.to_string())?
                        .map_err(|error| error.to_string())?;
                    let Some(input) = non_success else {
                        continue;
                    };
                    // Commit the result and parent-owned terminal in the same existing durable transition.
                    // 在同一次既有可靠转换中提交结果与父侧所属终态。
                    let observed_at_ms = now_ms()?;
                    let receipt = parent
                        .mutate_durable_if_changed(
                            SessionMutationAdmission::ControlPlane,
                            SessionFlushReason::SubagentReturned,
                            |document| {
                                document
                                    .record_child_task_non_success(input, observed_at_ms)
                                    .map(session_change)
                            },
                        )
                        .map_err(|error| error.to_string())?;
                    repaired += usize::from(receipt.changed);
                    continue;
                };
                // Coverage belongs to this generation, even if an older generation reused the same sequence numbers.
                // 覆盖归属于此代次，即使旧代次复用了相同序号。
                let frontier = parent
                    .read_full_durable(|document| {
                        document.subagent_return_frontier_before(
                            &current.child.session_id,
                            current.child.segment_generation,
                            request.source_assistant_text_seq,
                        )
                    })
                    .map_err(|error| error.to_string())?;
                // Use the same opaque builder and atomic parent transition as live completion.
                // 使用与实时完成相同的不透明构造器及原子父转换。
                let input = child
                    .read_full_durable(|document| {
                        document.build_subagent_return_input_since(request, frontier)
                    })
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?;
                // Timestamp only the parent observation; original source timestamps remain immutable.
                // 只记录父侧观测时间；原来源时间保持不可变。
                let observed_at_ms = now_ms()?;
                let receipt = parent.mutate_durable_if_changed(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::SubagentReturned,
                    |document| {
                        document
                            .complete_child_task_with_return(
                                current.execution_id,
                                input,
                                observed_at_ms,
                            )
                            .map(session_change)
                    },
                );
                repaired += usize::from(receipt.map_err(|error| error.to_string())?.changed);
            }
            Ok(repaired)
        },
    )
    .await
}
