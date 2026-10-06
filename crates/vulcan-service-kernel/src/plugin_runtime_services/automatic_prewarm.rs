//! Accepted prompt turns own one prewarm trigger; navigation and policy writes never create one.
//! 已接纳提示回合拥有单个预热触发；导航和策略写入绝不创建触发。

use crate::{ServiceBridgeState, SessionId, SharedServiceBridgeState};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, TryLockError};
use vulcan_plugin_runtime::{
    VulcanPluginAutomaticPrewarmOrigin, VulcanPluginAutomaticPrewarmPermit,
    VulcanPluginAutomaticPrewarmTrigger, VulcanPluginPrewarmTargetRequest,
    VulcanPluginRuntimeError, VulcanPluginRuntimeManager,
};

/// One latest trigger per known session preserves once-only admission after its receipts are forgotten.
/// 每个已知会话保留一个最近触发，在回执遗忘后仍保持一次性准入。
pub(crate) struct AutomaticPrewarmTurn {
    /// Exact kernel generation rejects a repeated logical turn after an ABA restart.
    /// 精确内核代际拒绝 ABA 重启后的重复逻辑回合。
    generation: u64,
    /// Exact accepted preparation turn distinguishes successive mailbox turns in one generation.
    /// 精确已接纳准备回合区分同代际连续邮箱回合。
    turn_id: String,
    /// The sole lifecycle owner closes all issued initialization permits when replaced or dropped.
    /// 唯一生命周期所有者在替换或丢弃时关闭全部已签发初始化许可。
    trigger: VulcanPluginAutomaticPrewarmTrigger,
    /// Successive turns share one nonblocking preparation gate rather than queuing duplicate metadata work.
    /// 连续回合共享单个非阻塞准备门，不排队复制元数据工作。
    preparation: Arc<Mutex<()>>,
}

/// Immutable work moves to the blocking executor without retaining the service state lock.
/// 不可变工作移入阻塞执行器，不保留服务状态锁。
struct AcceptedPrewarm {
    /// A unique host trigger remains stable for every configured target in this batch.
    /// 唯一宿主触发在此批次每个配置目标间保持稳定。
    trigger_id: String,
    /// The accepted session remains the original controller of every resulting receipt.
    /// 已接纳会话保持为每个结果回执的原控制器。
    owner: String,
    /// The ordinary prompt workspace resolver supplies the sole execution directory.
    /// 普通提示工作区解析器提供唯一执行目录。
    workspace: PathBuf,
    /// Revocation is live even if this work has not started on the blocking executor.
    /// 即使工作尚未在阻塞执行器启动，撤销仍实时生效。
    permit: VulcanPluginAutomaticPrewarmPermit,
    /// Only one batch may inspect and acknowledge automatic receipts for this session at a time.
    /// 同一时刻仅允许一个批次检查并确认此会话的自动回执。
    preparation: Arc<Mutex<()>>,
}

/// Close original session admission under the existing service lock; keep the once-only marker.
/// 在既有服务锁下关闭原会话入场；保留一次性标记。
pub(crate) fn close(state: &mut ServiceBridgeState, session: SessionId) {
    if let Some(original) = state.automatic_plugin_prewarm.get(&session) {
        original.trigger.close();
    }
}

/// Claim exactly one accepted turn while stop and replacement are excluded; return no work for stale or duplicate entry.
/// 排除停止及替换时认领精确已接纳回合；对陈旧或重复入口返回无工作。
fn claim(
    state: &SharedServiceBridgeState,
    session: SessionId,
    turn_id: &str,
    expected_generation: u64,
) -> Result<Option<AcceptedPrewarm>, String> {
    // One lock captures real ownership, execution workspace and the once-only marker atomically.
    // 单个锁原子捕获真实归属、执行工作区及一次性标记。
    let mut service = state.lock().map_err(|error| error.to_string())?;
    if crate::current_webui_prompt_preparation_generation(&service, session, turn_id)
        != Some(expected_generation)
    {
        return Ok(None);
    }
    if service
        .automatic_plugin_prewarm
        .get(&session)
        .is_some_and(|original| {
            original.generation == expected_generation && original.turn_id == turn_id
        })
    {
        return Ok(None);
    }
    // The same accepted-session slice is used by ordinary prompt preparation, never a browser-supplied path.
    // 使用与普通提示准备相同的已接纳会话切片，绝不使用浏览器提供路径。
    let workspace = crate::conversation_submission::existing_submission_workspace_slice(
        service
            .workspace_snapshot
            .as_ref()
            .ok_or("Workspace is not ready.")?,
        session,
    )?;
    let workspace = PathBuf::from(crate::workspace_path_for_session(&workspace, session));
    let trigger = VulcanPluginAutomaticPrewarmTrigger::default();
    let permit = trigger.permit();
    // Existing session ownership supplies the sole gate; first admission creates it explicitly.
    // 既有会话归属提供唯一门；首次入场明确创建门。
    let preparation = match service.automatic_plugin_prewarm.get(&session) {
        Some(original) => Arc::clone(&original.preparation),
        None => Arc::new(Mutex::new(())),
    };
    // Replacement drops and revokes the previous trigger; its native receipts remain in their original manager.
    // 替换丢弃并撤销先前触发；其原生回执仍保留在原管理器中。
    service.automatic_plugin_prewarm.insert(
        session,
        AutomaticPrewarmTurn {
            generation: expected_generation,
            turn_id: turn_id.into(),
            trigger,
            preparation: Arc::clone(&preparation),
        },
    );
    Ok(Some(AcceptedPrewarm {
        trigger_id: uuid::Uuid::new_v4().to_string(),
        owner: session.to_string(),
        workspace,
        permit,
        preparation,
    }))
}

/// Spawn one bounded batch for the accepted turn; return its observer task or none for stale or repeated entry.
/// 为已接纳回合启动单个有界批次；返回观测任务，陈旧或重复入口返回空值。
/// The caller may detach the task; stopping the turn still closes its original permission.
/// 调用方可以分离任务；停止回合仍关闭其原权限。
pub(crate) fn spawn_for_turn(
    state: &SharedServiceBridgeState,
    manager: VulcanPluginRuntimeManager,
    session: SessionId,
    turn_id: &str,
    expected_generation: u64,
) -> Result<Option<tokio::task::JoinHandle<()>>, String> {
    // Missing execution infrastructure must not consume the turn's once-only marker.
    // 执行基础设施缺失不得消费回合的一次性标记。
    let executor = tokio::runtime::Handle::try_current().map_err(|error| error.to_string())?;
    let Some(accepted) = claim(state, session, turn_id, expected_generation)? else {
        return Ok(None);
    };
    Ok(Some(executor.spawn(async move {
        // Observe blocking completion independently of the original UI or prompt waiter.
        // 独立于原界面或提示等待者观测阻塞完成。
        match tokio::task::spawn_blocking(move || run(&manager, accepted)).await {
            Ok(Ok(failures)) => {
                for failure in failures {
                    tracing::warn!("[plugin-prewarm] session={session}: {failure}");
                }
            }
            Ok(Err(error)) => tracing::warn!("[plugin-prewarm] session={session}: {error}"),
            Err(error) => {
                tracing::warn!("[plugin-prewarm] session={session} worker failed: {error}")
            }
        }
    })))
}

/// Resolve one committed policy and current declarations, then prepare and start each exact target once.
/// 解析单个已提交策略及当前声明，随后将每个精确目标准备并启动一次。
/// Return per-rule failures without failing ordinary prompt execution or replaying any target.
/// 返回逐规则失败，不使普通提示执行失败，也不重放目标。
fn run(
    manager: &VulcanPluginRuntimeManager,
    accepted: AcceptedPrewarm,
) -> Result<Vec<String>, VulcanPluginRuntimeError> {
    // A stopped older batch must never acknowledge a newer turn's successful receipts.
    // 已停止旧批次绝不能确认新回合的成功回执。
    let _preparation = match accepted.preparation.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => return Ok(vec!["Automatic prewarm preparation is already active for this session; this turn will not retry.".into()]),
        Err(TryLockError::Poisoned(_)) => return Err(VulcanPluginRuntimeError::new("Automatic prewarm preparation gate is poisoned.")),
    };
    if accepted.permit.is_closed() {
        return Ok(Vec::new());
    }
    // A service without a native plugin parent cannot own automatic execution.
    // 没有原生插件父级的服务不能拥有自动执行。
    if manager.formal_runtime_status()?.is_none() {
        return Ok(Vec::new());
    }
    let policy = manager.prewarm_policy_status()?;
    let mut failures = Vec::new();
    // Successful previous automatic receipts are acknowledged by their host owner, preventing per-turn history growth.
    // 先前成功自动回执由其宿主所有者确认，防止历史随回合增长。
    for call in manager.formal_calls(&accepted.owner)? {
        if call.retired
            && call.error.is_none()
            && call.prewarm.as_ref().is_some_and(|target| {
                target
                    .automatic
                    .as_ref()
                    .is_some_and(|origin| origin.trigger_id != accepted.trigger_id)
                    && target.satisfied.is_some()
            })
            && let Err(error) = manager.forget_formal_call(&call.invocation_id, &accepted.owner)
        {
            failures.push(format!(
                "Previous automatic receipt {} acknowledgement failed: {error}",
                call.invocation_id
            ));
        }
    }
    let catalog = manager.snapshot();
    for rule in policy.rules {
        // Stopped queued batches cannot initialize; the runtime repeats this check atomically at actual enqueue.
        // 已停止排队批次不能初始化；运行时在实际入队时原子重复检查。
        if accepted.permit.is_closed() {
            break;
        }
        let Some(plugin) = catalog.plugins.get(&rule.plugin_id) else {
            failures.push(format!(
                "Configured automatic prewarm plugin {} is unavailable.",
                rule.plugin_id
            ));
            continue;
        };
        // Ordinary preparation validates enabled state, exact generation, reuse policy and authorized capabilities.
        // 普通准备校验启用状态、精确代次、复用策略及授权能力。
        let prepared = manager.prepare_automatic_tool_prewarm(
            VulcanPluginPrewarmTargetRequest {
                runtime_id: policy.runtime_id.clone(),
                plugin_id: rule.plugin_id.clone(),
                tool_id: rule.tool_id.clone(),
                plugin_generation: plugin.generation,
                owner_session_id: accepted.owner.clone(),
                workspace_root: accepted.workspace.clone(),
                target_idle: rule.target_idle,
            },
            VulcanPluginAutomaticPrewarmOrigin {
                trigger_id: accepted.trigger_id.clone(),
                policy_revision: policy.revision.clone(),
            },
            accepted.permit.clone(),
        );
        match prepared {
            Ok(receipt) => {
                if let Err(error) =
                    manager.start_reusable_tool_prewarm(&receipt.invocation_id, &accepted.owner)
                {
                    failures.push(format!(
                        "Automatic prewarm {} retained receipt {} could not start: {error}",
                        rule.tool_id, receipt.invocation_id
                    ));
                }
            }
            Err(error) => failures.push(format!(
                "Automatic prewarm {} admission failed: {error}",
                rule.tool_id
            )),
        }
    }
    Ok(failures)
}
