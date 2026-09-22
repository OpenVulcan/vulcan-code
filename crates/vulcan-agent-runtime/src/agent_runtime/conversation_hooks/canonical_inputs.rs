//! Exact human-input projection for batches that continue an existing business Hook.
//! 继续已有业务 Hook 的批次所使用的精确人类输入投影。

use super::*;
use std::collections::BTreeSet;

impl ConversationHookRuntime {
    /// Prepares an in-turn batch using each canonical input identity instead of the batch display ID.
    /// 使用每条规范输入身份而非批次展示标识准备回合内批次。
    /// `inputs` are host-validated V1 human inputs in event order; remaining arguments describe shared preparation.
    /// `inputs` 是宿主验证且按事件顺序排列的 V1 人类输入；其余参数描述共享准备。
    /// Returns the existing business Hook's requests; malformed or conflicting batches leave runtime state unchanged.
    /// 返回既有业务 Hook 的请求；畸形或冲突批次保持运行时状态不变。
    pub fn prepare_canonical_in_turn_hook_requests(
        &mut self,
        session_id: SessionId,
        prompt_entry_id: &str,
        project_id: Option<String>,
        workspace_path: String,
        source: PendingSessionPromptSource,
        system_prompt_hook_phase: SystemPromptHookPhase,
        prompt: &str,
        conversation_state: &SessionConversationState,
        vmm_runtime_ready: bool,
        inputs: &[CanonicalInTurnHookInput],
    ) -> Result<ConversationPromptHookRequests, String> {
        if !matches!(
            source,
            PendingSessionPromptSource::UserInterrupt | PendingSessionPromptSource::ExecutionRetry
        ) || (source == PendingSessionPromptSource::UserInterrupt && inputs.is_empty())
            || (source == PendingSessionPromptSource::ExecutionRetry && inputs.len() > 1)
        {
            return Err(String::from(
                "Canonical Hook preparation requires a human interrupt batch or one optional explicit Continue input.",
            ));
        }
        // Stage the owner and all notifications so a late conflict cannot publish a partial batch.
        // 暂存所有者及全部通知，防止后续冲突发布部分批次。
        let mut staged = self.active_turns.get(&session_id).cloned().ok_or_else(|| {
            String::from("Canonical in-turn Hook preparation requires its original business owner.")
        })?;
        let mut notifications = Vec::new();
        let mut actions = BTreeSet::new();
        let mut input_ids = BTreeSet::new();
        let mut previous_seq = None;
        for input in inputs {
            if !actions.insert(&input.client_action_id)
                || !input_ids.insert(input.input_id)
                || previous_seq.is_some_and(|previous| input.input_seq <= previous)
            {
                return Err(String::from(
                    "Canonical in-turn Hook input identities are duplicated or out of order.",
                ));
            }
            previous_seq = Some(input.input_seq);
            // Preserve raw queue observations; new direct inputs use the same pure sanitizer as shared preparation.
            // 保留原始队列观察；新的直接输入使用与共享准备相同的纯清理函数。
            let action = input.client_action_id.as_str();
            let (_, sanitized) = crate::agent_runtime::context_injection_runtime::collect_builtin_manual_injection_ops_and_sanitized_prompt(&input.text);
            let queued = staged.recorded_prompt_entry_ids.contains(action);
            if staged.observed_input_identities.get(action).is_some_and(|observed| {
                observed.0 != input.input_id
                    || (observed.1 == vulcan_session_core::v1::business_hook::BusinessHookInputKind::Queued) != queued
            }) {
                return Err(String::from("Canonical in-turn Hook input changed its durable admission identity."));
            }
            let expected = if queued {
                ConversationTimelineItem::new(
                    "user",
                    "user_followup",
                    input.text.trim().to_owned(),
                    Some(action.to_owned()),
                )
            } else {
                ConversationTimelineItem::new(
                    "user",
                    "user_interrupt",
                    sanitized.trim().to_owned(),
                    Some(action.to_owned()),
                )
            };
            let matching = staged
                .timeline
                .iter()
                .filter(|item| item.source_id.as_deref() == Some(action))
                .collect::<Vec<_>>();
            if let Some(receipt) = staged.canonical_input_receipts.get(action)
                && receipt != &(input.input_id, input.input_seq)
            {
                return Err(String::from(
                    "Canonical in-turn Hook retry changed its original input coordinates.",
                ));
            }
            if matching.is_empty() && !queued {
                staged.timeline.push(expected);
            } else if matching.len() != 1 || matching[0] != &expected {
                return Err(String::from(
                    "Canonical in-turn Hook input conflicts with its observed content.",
                ));
            }
            // Restoring an observation does not dispatch its preparation notification; send once per exact live receipt.
            // 恢复观察不会派发其准备通知；每个精确活动回执只通知一次。
            if !queued && !staged.canonical_input_receipts.contains_key(action) {
                notifications.push((
                    action.to_owned(),
                    input.input_id,
                    input.input_seq,
                    sanitized,
                ));
            }
            staged
                .canonical_input_receipts
                .insert(action.to_owned(), (input.input_id, input.input_seq));
        }
        staged.vmm_runtime_ready = vmm_runtime_ready;
        // Publish the complete batch before any asynchronous Hook observer can see its notification.
        // 在任何异步 Hook 观察器能够看到通知之前发布完整批次。
        let turn_id = staged.turn_id.clone();
        self.active_turns.insert(session_id, staged);
        for (action, input_id, input_seq, content) in notifications {
            self.dispatch_json(
                HOOK_CONVERSATION_USER_EVENT,
                session_id,
                json!({
                    "session_id": session_id.to_string(), "project_id": project_id.clone(),
                    "workspace_path": workspace_path.clone(), "turn_id": turn_id,
                    "event_kind": "user_interrupt", "is_new_turn": false,
                    "client_action_id": action, "input_id": input_id.value(),
                    "input_seq": input_seq.value(), "user_content": content,
                    "vmm_runtime_ready": vmm_runtime_ready,
                }),
                HookScope::Session,
            );
        }
        Ok(self.prepare_prompt_hook_requests_with_timeline_policy(
            session_id,
            prompt_entry_id,
            project_id,
            workspace_path,
            source,
            system_prompt_hook_phase,
            prompt,
            conversation_state,
            vmm_runtime_ready,
            true,
        ))
    }
}
