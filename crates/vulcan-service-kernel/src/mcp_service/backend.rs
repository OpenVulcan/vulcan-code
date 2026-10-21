//! Kernel implementation of the MCP service backend.
//! MCP 服务后端的内核实现。

use std::sync::Weak;

use async_trait::async_trait;
use serde_json::Value;
use vulcan_mcp_service::{
    McpChatModel, McpMediaFile, McpMediaModel, McpMediaOutcome, McpMediaRun, McpPendingQuestion,
    McpProject, McpQuestionOption, McpQuestionPrompt, McpServiceBackend, McpSessionRow,
    McpSessionStatus, McpSubmission, McpSubmissionAck, McpTranscriptPage,
};
use vulcan_provider_adapters::{ProviderToolCapabilityKind, ProviderToolTargetSelection};
use vulcan_provider_config::provider_config::{
    McpMediaTargetGrant, PersistedMcpServiceConfiguration,
};
use vulcan_service_api::{ConversationSummary, PendingQuestionSnapshot, ServiceEventEmitter};
use vulcan_workbench_model::SessionRunPhase;

use super::grantable_capability;
use super::transcript::project_event;
use crate::conversation_submission::{
    ConversationSubmissionMode, ExistingSessionHydrationPolicy, SubmissionModelSelection,
    service_submit_conversation_prompt,
};
use crate::state::{ServiceBridgeState, SharedServiceBridgeState};
use crate::{
    SubmissionActivation, is_active_session_run_phase,
    pending_question_snapshots_from_agent_runtime,
};

/// Every run phase, so a workbench label maps back to its phase.
/// 全部运行阶段，用于把工作台标签映射回阶段。
const RUN_PHASES: [SessionRunPhase; 10] = [
    SessionRunPhase::Idle,
    SessionRunPhase::Preparing,
    SessionRunPhase::MemoryRecall,
    SessionRunPhase::Requesting,
    SessionRunPhase::Thinking,
    SessionRunPhase::ToolCalling,
    SessionRunPhase::ToolRunning,
    SessionRunPhase::Ready,
    SessionRunPhase::Failed,
    SessionRunPhase::Stopped,
];

/// Directory under the user's Vulcan root that holds media generated for MCP clients.
/// 用户 Vulcan 根目录下保存为 MCP 客户端生成的媒体的目录。
const MCP_MEDIA_DIRECTORY: &str = "mcp-media";

/// MCP backend that reads and drives the live service kernel.
/// 读取并驱动实时服务内核的 MCP 后端。
pub(crate) struct KernelMcpBackend {
    /// Event emitter used by submissions and stops.
    /// 提交与停止使用的事件发射器。
    app: ServiceEventEmitter,
    /// The service state; weak because the state owns this backend's server.
    /// 服务状态；使用弱引用，因为状态持有本后端所在的服务器。
    state: Weak<std::sync::Mutex<ServiceBridgeState>>,
}

impl KernelMcpBackend {
    /// Creates the backend.
    /// 创建后端。
    pub(crate) fn new(
        app: ServiceEventEmitter,
        state: Weak<std::sync::Mutex<ServiceBridgeState>>,
    ) -> Self {
        Self { app, state }
    }

    /// Returns the live service state.
    /// 返回实时服务状态。
    fn state(&self) -> Result<SharedServiceBridgeState, String> {
        self.state
            .upgrade()
            .ok_or_else(|| String::from("The Vulcan Code service is shutting down."))
    }

    /// Runs `read` under the service lock.
    /// 在服务锁内执行 `read`。
    fn with_state<T>(
        &self,
        read: impl FnOnce(&ServiceBridgeState) -> Result<T, String>,
    ) -> Result<T, String> {
        let state = self.state()?;
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        read(&guard)
    }
}

#[async_trait]
impl McpServiceBackend for KernelMcpBackend {
    fn configuration(&self) -> Result<PersistedMcpServiceConfiguration, String> {
        let state = self.state()?;
        Ok(crate::service_state_access::provider_configuration_from_state(&state)?.mcp_service)
    }

    async fn projects(&self) -> Result<Vec<McpProject>, String> {
        self.with_state(|guard| {
            Ok(guard
                .workbench
                .projects
                .iter()
                .map(|project| McpProject {
                    id: project.id.clone(),
                    name: project.name.clone(),
                    path: project.path.clone(),
                })
                .collect())
        })
    }

    async fn sessions(&self) -> Result<Vec<McpSessionRow>, String> {
        self.with_state(|guard| {
            Ok(guard
                .workbench
                .conversations
                .iter()
                .map(session_row)
                .collect())
        })
    }

    async fn session_status(&self, session_id: &str) -> Result<Option<McpSessionStatus>, String> {
        let snapshot = self.with_state(|guard| {
            let Some(conversation) = guard
                .workbench
                .conversations
                .iter()
                .find(|conversation| conversation.id == session_id)
            else {
                return Ok(None);
            };
            let questions =
                pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref())?
                    .into_iter()
                    .filter(|question| {
                        question.session_id == session_id
                            || question.owner_conversation_id.as_deref() == Some(session_id)
                    })
                    .map(pending_question)
                    .collect::<Vec<_>>();
            let queued_prompts = guard
                .workbench
                .pending_prompts_by_conversation
                .iter()
                .find(|entry| entry.conversation_id == session_id)
                .map_or(0, |entry| entry.pending_prompts.len());
            Ok(Some((session_row(conversation), questions, queued_prompts)))
        })?;
        let Some((row, questions, queued_prompts)) = snapshot else {
            return Ok(None);
        };
        let state = self.state()?;
        let frontier = crate::conversation_events_v1::read_session_events_after(
            &state,
            session_id,
            u64::MAX,
            0,
        )
        .await?;
        Ok(Some(McpSessionStatus {
            row,
            questions,
            queued_prompts,
            next_seq: frontier.next_seq,
        }))
    }

    async fn read_transcript(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: usize,
        include_tool_output: bool,
    ) -> Result<McpTranscriptPage, String> {
        let state = self.state()?;
        let tail = crate::conversation_events_v1::read_session_events_after(
            &state, session_id, after_seq, limit,
        )
        .await?;
        Ok(McpTranscriptPage {
            last_scanned_seq: tail.events.last().map(|event| event.seq.value()),
            entries: tail
                .events
                .iter()
                .filter_map(|event| project_event(event, include_tool_output))
                .collect(),
            next_seq: tail.next_seq,
        })
    }

    async fn submit(&self, submission: McpSubmission) -> Result<McpSubmissionAck, String> {
        let state = self.state()?;
        let ack = service_submit_conversation_prompt(
            self.app.clone(),
            state,
            submission.submission_id,
            submission.project_id,
            submission.session_id,
            None,
            submission.prompt,
            None,
            // Plan mode is only ever entered here; leaving it is the user's decision in the app.
            // 这里只会进入计划模式；退出计划模式由用户在应用内决定。
            submission.plan_mode.then(|| String::from("plan")),
            submission.reasoning_effort,
            submission.model.map(|model| SubmissionModelSelection {
                provider_id: model.provider_id,
                model_id: model.model_id,
                connection_id: model.connection_id,
            }),
            None,
            None,
            None,
            ExistingSessionHydrationPolicy::HydrateIfEmpty,
            ConversationSubmissionMode::UserPrompt,
            SubmissionActivation::Background,
        )
        .await?;
        Ok(McpSubmissionAck {
            session_id: ack.conversation_id,
            queued: ack.queued,
        })
    }

    async fn stop(&self, session_id: &str) -> Result<(), String> {
        let state = self.state()?;
        crate::service_stop_conversation_run(self.app.clone(), state, session_id.to_string(), None)
            .await
            .map(|_| ())
    }

    async fn answer_question(
        &self,
        request_id: &str,
        answers: Option<Vec<Vec<String>>>,
    ) -> Result<bool, String> {
        let state = self.state()?;
        match answers {
            Some(answers) => crate::conversation_actions::service_reply_pending_question(
                state,
                request_id.to_string(),
                answers,
            ),
            None => crate::conversation_actions::service_reject_pending_question(
                state,
                request_id.to_string(),
            ),
        }
    }

    async fn chat_models(&self) -> Result<Vec<McpChatModel>, String> {
        self.with_state(|guard| {
            Ok(guard
                .workbench
                .providers
                .iter()
                .filter_map(|provider| {
                    let connection_id = provider.connection_id.clone()?;
                    Some(provider.models.iter().map(move |model| {
                        McpChatModel {
                            provider_id: provider
                                .source_provider_id
                                .clone()
                                .unwrap_or_else(|| provider.id.clone()),
                            model_id: model.id.clone(),
                            connection_id: connection_id.clone(),
                            model_name: model.name.clone(),
                            provider_name: provider
                                .provider_display_name
                                .clone()
                                .unwrap_or_else(|| provider.name.clone()),
                            account_name: provider
                                .account_name
                                .clone()
                                .unwrap_or_else(|| provider.name.clone()),
                            preferred_account: model.preferred_connection,
                        }
                    }))
                })
                .flatten()
                .collect())
        })
    }

    async fn media_models(
        &self,
        grants: &[McpMediaTargetGrant],
    ) -> Result<Vec<McpMediaModel>, String> {
        let state = self.state()?;
        let (runtime, configuration) = crate::public_ai_runtime_and_configuration(&state)?;
        runtime
            .available_provider_tool_targets(&configuration)?
            .iter()
            .filter(|target| {
                grants.iter().any(|grant| {
                    grant.connection_id == target.connection_id
                        && grant.target_id == target.target.target_id
                })
            })
            .filter_map(|target| {
                grantable_capability(target).map(|capability| (target, capability))
            })
            .map(|(target, capability)| {
                Ok(McpMediaModel {
                    model: vulcan_agent_runtime::model_extension_public_model_id(target),
                    capability,
                    display_name: target.target.display_name.clone(),
                    connection_id: target.connection_id.clone(),
                    target_id: target.target.target_id.clone(),
                    parameters_markdown: vulcan_agent_runtime::model_extension_parameters_markdown(
                        target,
                    )?,
                })
            })
            .collect()
    }

    async fn run_media(&self, run: McpMediaRun) -> Result<McpMediaOutcome, String> {
        let state = self.state()?;
        let (runtime, configuration) = crate::public_ai_runtime_and_configuration(&state)?;
        let capability: ProviderToolCapabilityKind =
            serde_json::from_value(Value::String(run.capability.clone()))
                .map_err(|_| format!("Unknown media capability `{}`.", run.capability))?;
        // The grant was checked by the server; the target must still be live in this generation.
        // 授权已由服务器检查；目标还必须在当前代际中仍然可用。
        let available = runtime.available_provider_tool_targets(&configuration)?;
        let still_available = available.iter().any(|target| {
            target.connection_id == run.connection_id
                && target.target.target_id == run.target_id
                && grantable_capability(target).as_deref() == Some(run.capability.as_str())
        });
        if !still_available {
            return Err(String::from(
                "This media model is no longer available. Call list_media_models again.",
            ));
        }
        let result = runtime
            .execute_provider_tool_target_for_owner_in_workspace(
                &configuration,
                capability,
                &ProviderToolTargetSelection {
                    connection_id: run.connection_id,
                    target_id: run.target_id,
                },
                run.arguments,
                Some(run.owner_id),
                None,
            )
            .await?;
        let directory = vulcan_runtime_assets::resolve_user_vulcan_root()
            .ok_or_else(|| String::from("The user home directory could not be resolved."))?
            .join(MCP_MEDIA_DIRECTORY)
            .join(&run.client_id)
            .join(&run.job_id);
        let saved =
            vulcan_agent_runtime::save_provider_tool_result_to_directory(result, &directory)
                .await?;
        Ok(McpMediaOutcome {
            files: saved
                .files
                .into_iter()
                .map(|file| McpMediaFile {
                    path: file.path,
                    kind: file.kind,
                    mime_type: file.mime_type,
                    byte_len: file.byte_len,
                })
                .collect(),
            text: saved.text,
        })
    }

    fn cancel_media(&self, owner_id: &str) -> Result<(), String> {
        let state = self.state()?;
        let (runtime, _) = crate::public_ai_runtime_and_configuration(&state)?;
        runtime
            .request_provider_task_cancellation_for_owner(owner_id)
            .map(|_| ())
    }
}

/// Projects one workbench conversation into an MCP session row.
/// 将一个工作台会话投影为 MCP 会话行。
fn session_row(conversation: &ConversationSummary) -> McpSessionRow {
    let running = RUN_PHASES
        .iter()
        .find(|phase| phase.to_string() == conversation.run_phase)
        .is_some_and(|phase| is_active_session_run_phase(*phase));
    McpSessionRow {
        id: conversation.id.clone(),
        title: conversation.title.clone(),
        project_id: conversation.project_id.clone(),
        run_phase: conversation.run_phase.clone(),
        running,
        attention: conversation
            .attention
            .as_ref()
            .map(|attention| attention.kind.clone()),
        updated_at: conversation.last_updated_at.clone(),
        unread: conversation.unread,
        dedicated: conversation.dedicated,
    }
}

/// Projects one live pending question.
/// 投影一个实时待回答问题。
fn pending_question(question: PendingQuestionSnapshot) -> McpPendingQuestion {
    McpPendingQuestion {
        request_id: question.id,
        approval_tool: question.approval.map(|approval| approval.tool_name),
        prompts: question
            .questions
            .into_iter()
            .map(|prompt| McpQuestionPrompt {
                header: prompt.header,
                question: prompt.question,
                options: prompt
                    .options
                    .into_iter()
                    .map(|option| McpQuestionOption {
                        label: option.label,
                        description: option.description,
                    })
                    .collect(),
                multiple: prompt.multiple,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_phase_labels_round_trip() {
        for phase in RUN_PHASES {
            // A new phase fails to compile here until RUN_PHASES lists it.
            // 新增阶段在此处编译失败，直到 RUN_PHASES 列入它。
            match phase {
                SessionRunPhase::Idle
                | SessionRunPhase::Preparing
                | SessionRunPhase::MemoryRecall
                | SessionRunPhase::Requesting
                | SessionRunPhase::Thinking
                | SessionRunPhase::ToolCalling
                | SessionRunPhase::ToolRunning
                | SessionRunPhase::Ready
                | SessionRunPhase::Failed
                | SessionRunPhase::Stopped => {}
            }
            let label = phase.to_string();
            assert_eq!(
                RUN_PHASES
                    .iter()
                    .filter(|candidate| candidate.to_string() == label)
                    .count(),
                1,
                "run phase label `{label}` must be unique"
            );
        }
    }
}
