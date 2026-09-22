//! Owns cancellable QA preparation independently of business runs and Provider streams.
//! 独立于业务运行及 Provider 流持有可取消答疑准备。

use crate::SessionId;
use crate::state::{ActiveStreamTask, ServiceBridgeState, SharedServiceBridgeState};
use uuid::Uuid;
use vulcan_session_core::ActiveTurnKind;
use vulcan_session_core::v1::collaboration::CollaborationExecutionId;

/// Exact process ownership of one auxiliary preparation future.
/// 单个辅助准备 Future 的精确进程所有权。
pub(crate) struct QuestionPreparationTask {
    /// Durable auxiliary execution identity.
    /// 可靠辅助执行身份。
    execution_id: CollaborationExecutionId,
    /// Immutable kernel generation admitted before spawning.
    /// 生成任务前准入的不可变内核代际。
    generation: u64,
    /// Logical preparation identity within that generation.
    /// 该代际内的逻辑准备身份。
    turn_id: String,
    /// Actual cancellable task handle, taken by Stop before terminal settlement.
    /// 实际可取消任务句柄，停止操作在终态结算前取走。
    pub(crate) task: ActiveStreamTask,
}

/// Validates any retained preparation against the current kernel before destructive Stop mutations.
/// 在破坏性停止修改之前，按当前内核验证已保留准备。
/// `state` is already locked and `session_id` selects the target; absence succeeds, mismatches fail.
/// `state` 已加锁，`session_id` 选择目标；不存在时成功，不匹配时失败。
pub(crate) fn validate_question_preparation_task(
    state: &ServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    let Some(preparation) = state.question_preparation_tasks.get(&session_id) else {
        return Ok(());
    };
    let kernel = state
        .session_runtime_kernels
        .get(&session_id)
        .ok_or_else(|| String::from("Question preparation has no kernel owner."))?;
    if kernel.generation() != preparation.generation || !kernel.active_turn().is_some_and(|turn| {
        turn.turn_id == preparation.turn_id
            && matches!(&turn.kind, ActiveTurnKind::QuestionExecution { execution_id } if execution_id == &preparation.execution_id)
    }) {
        return Err(String::from("Question preparation does not match the current kernel execution."));
    }
    Ok(())
}

/// Registers preparation before polling it, then removes only its own entry after all work finishes.
/// 在轮询之前登记准备，全部工作完成后仅移除自己的条目。
/// The target, generation, turn and execution select one admitted owner; `future` performs preparation.
/// 目标、代际、回合及执行选择一个已准入所有者；`future` 执行准备。
/// Returns after registration; errors leave the future unpolled and preserve any existing task.
/// 登记后返回；错误保持 Future 未轮询，并保留已有任务。
pub(crate) fn spawn_question_preparation_task<F>(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    generation: u64,
    turn_id: &str,
    execution_id: &CollaborationExecutionId,
    future: F,
) -> Result<(), String>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    // The start gate prevents external preparation effects before exact registration succeeds.
    // 启动门防止在精确登记成功之前发生准备外部效果。
    let (start_sender, start_receiver) = tokio::sync::oneshot::channel();
    let task_id = Uuid::new_v4();
    {
        let mut service = state.lock().map_err(|error| error.to_string())?;
        if crate::current_webui_prompt_preparation_generation(&service, session_id, turn_id) != Some(generation)
            || !service.session_runtime_kernels.get(&session_id).and_then(|kernel| kernel.active_turn()).is_some_and(|turn|
                matches!(&turn.kind, ActiveTurnKind::QuestionExecution { execution_id: current } if current == execution_id))
        {
            return Err(String::from("Question preparation registration owner is stale."));
        }
        if service.question_preparation_tasks.contains_key(&session_id)
            || service.active_stream_tasks.contains_key(&session_id)
        {
            return Err(String::from(
                "Question preparation already has an active task or stream.",
            ));
        }
        let task_state = state.clone();
        let handle = crate::spawn_service_task(async move {
            if start_receiver.await.is_err() {
                return;
            }
            future.await;
            // A late completion cannot erase replacement preparation or its Provider stream.
            // 迟到完成不能删除替代准备或其 Provider 流。
            match task_state.lock() {
                Ok(mut service) => {
                    if service
                        .question_preparation_tasks
                        .get(&session_id)
                        .is_some_and(|entry| entry.task.task_id() == task_id)
                    {
                        service.question_preparation_tasks.remove(&session_id);
                    }
                }
                Err(error) => {
                    eprintln!("[webui-service] question preparation cleanup lock failed: {error}")
                }
            }
        });
        service.question_preparation_tasks.insert(
            session_id,
            QuestionPreparationTask {
                execution_id: execution_id.clone(),
                generation,
                turn_id: turn_id.to_owned(),
                task: ActiveStreamTask::new(task_id, handle, None),
            },
        );
    }
    if start_sender.send(()).is_err() {
        let mut service = state.lock().map_err(|error| error.to_string())?;
        if service
            .question_preparation_tasks
            .get(&session_id)
            .is_some_and(|entry| entry.task.task_id() == task_id)
        {
            service.question_preparation_tasks.remove(&session_id);
        }
        return Err(String::from("Question preparation start gate closed."));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
