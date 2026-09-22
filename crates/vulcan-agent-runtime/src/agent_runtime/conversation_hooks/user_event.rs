//! Deferred user observations released only after the host publishes business Hook ownership.
//! 仅在宿主发布业务 Hook 归属之后释放的延迟用户观察。

use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// One immutable observation shared by cloned preparation objects, with single process-local dispatch.
/// 在克隆准备对象之间共享的一条不可变观察，在进程内只派发一次。
#[derive(Clone)]
pub struct ConversationUserEventRequest {
    /// Dispatcher captured with the original preparation.
    /// 与原准备一起捕获的调度器。
    dispatcher: HookDispatcher,
    /// Exact request payload prepared before publication.
    /// 在发布之前准备的精确请求载荷。
    request: HookDispatchRequest,
    /// Original business Hook correlation required by publication.
    /// 发布要求的原业务 Hook 关联身份。
    turn_id: String,
    /// Shared one-way admission prevents cloned preparations from sending duplicate observations.
    /// 共享单向准入防止克隆准备重复发送观察。
    dispatched: Arc<AtomicBool>,
}

impl ConversationUserEventRequest {
    /// Builds an inert observation from the actual `dispatcher`, `session_id`, payload and Hook identity.
    /// 从实际 `dispatcher`、`session_id`、载荷及 Hook 身份构建尚不执行的观察。
    /// Returns a request without spawning a handler or publishing external effects.
    /// 返回请求，不启动处理器或发布外部效果。
    pub(super) fn new(
        dispatcher: HookDispatcher,
        session_id: SessionId,
        payload: Value,
        turn_id: String,
    ) -> Self {
        Self {
            dispatcher,
            request: prepared_hook_dispatch(
                HOOK_CONVERSATION_USER_EVENT,
                session_id,
                payload,
                HookScope::Session,
            ),
            turn_id,
            dispatched: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Releases the observation after the host has durably published `published_turn_id`.
    /// 在宿主可靠发布 `published_turn_id` 之后释放观察。
    /// Returns true for the first dispatch, false for a clone retry, or an error for foreign ownership.
    /// 首次派发返回 true，克隆重试返回 false，外来归属返回错误。
    pub fn dispatch_after_publication(&self, published_turn_id: &str) -> Result<bool, String> {
        if self.turn_id != published_turn_id {
            return Err(String::from(
                "Deferred user observation has a different published business Hook.",
            ));
        }
        if self
            .dispatched
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(false);
        }
        spawn_hook_dispatch(self.dispatcher.clone(), self.request.clone());
        Ok(true)
    }
}

/// Builds the common immutable request for `point`, `session_id`, `payload` and `scope` without dispatch.
/// 为 `point`、`session_id`、`payload` 及 `scope` 构建通用不可变请求，不执行派发。
/// Returns the same host context used by immediate and deferred observations.
/// 返回即时与延迟观察共用的相同宿主上下文。
pub(super) fn prepared_hook_dispatch(
    point: &'static str,
    session_id: SessionId,
    payload: Value,
    scope: HookScope,
) -> HookDispatchRequest {
    // Keep all host context fields identical across immediate and post-publication dispatch.
    // 保持即时派发与发布后派发的全部宿主上下文字段一致。
    let context = HookContext {
        run_id: format!("{point}:{session_id}"),
        point: point.to_string(),
        scope,
        actor: HookActor::new("vulcan-code-runtime"),
        workspace_id: None,
        project_id: None,
        session_id: Some(session_id.to_string()),
        conversation_id: Some(session_id.to_string()),
        user_id: None,
        capabilities: Default::default(),
        metadata: BTreeMap::new(),
    };
    HookDispatchRequest::new(context, HookPayload { value: payload })
}
