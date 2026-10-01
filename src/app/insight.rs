// Session insight helpers shared by the panel view and its input: row lists of the Memory and
// Agents tabs, localized labels, and the detail sheets and confirmations their rows open.
// 会话洞察辅助：面板视图与输入共用的记忆页、代理页行列表、本地化标签，以及这些行打开的
// 详情面板与确认框。

use super::action::{PromptDialog, PromptThen, UiAction, confirm};
use super::overlay::DetailSheet;
use super::{App, PanelTab};
use crate::backend::Command;
use crate::model::*;
use crate::tr;

/// One selectable row of the Memory tab.
/// 记忆页的一个可选行。
pub enum MemoryEntry<'a> {
    Dynamic(&'a MemoryCard, &'a InjectedMemory),
    Expired(&'a InjectedMemory),
    Profile(&'a ProfileSection, &'a ProfileNode),
}

/// Rows of the Memory tab in display order.
/// 按显示顺序排列的记忆页行。
pub fn memory_entries(insight: &SessionInsight, static_view: bool) -> Vec<MemoryEntry<'_>> {
    let memory = &insight.memory;
    if static_view {
        return memory
            .profile
            .iter()
            .flat_map(|s| s.nodes.iter().map(move |n| MemoryEntry::Profile(s, n)))
            .collect();
    }
    let mut entries: Vec<MemoryEntry> = memory
        .dynamic
        .iter()
        .flat_map(|c| c.items.iter().map(move |i| MemoryEntry::Dynamic(c, i)))
        .collect();
    entries.extend(memory.expired.iter().map(MemoryEntry::Expired));
    entries
}

/// Selectable rows of a panel tab.
/// 面板标签页的可选行数。
pub fn item_count(app: &App, tab: PanelTab) -> usize {
    let Some(session) = app.session() else {
        return 0;
    };
    match tab {
        PanelTab::Memory => memory_entries(&session.insight, app.panel.memory_static).len(),
        PanelTab::Agents => session.agents.len(),
        _ => 0,
    }
}

pub fn agent_state_label(state: AgentState) -> String {
    match state {
        AgentState::Running => tr!("conversation.subagent.state.running"),
        AgentState::Standby => tr!("conversation.subagent.state.completed"),
        AgentState::Stopped => tr!("conversation.subagent.state.stopped"),
        AgentState::Failed => tr!("conversation.subagent.state.failed"),
        AgentState::Closed => tr!("conversation.subagent.state.closed"),
    }
}

pub fn profile_source_label(source: ProfileSource) -> String {
    match source {
        ProfileSource::SystemSeed => tr!("memory.profile.sourceSystemSeed"),
        ProfileSource::TurnExtract => tr!("memory.profile.sourceTurnExtract"),
        ProfileSource::ManualInstruction => tr!("memory.profile.sourceManualInstruction"),
    }
}

pub fn profile_section_label(section: &ProfileSection) -> String {
    let kind = if section.user {
        tr!("memory.profile.user")
    } else {
        tr!("memory.profile.project")
    };
    format!("{kind} · {}", section.name)
}

pub fn context_part_label(part: ContextPart) -> String {
    match part {
        ContextPart::SystemPrompt => tr!("tui.context.systemPrompt"),
        ContextPart::Tools => tr!("tui.context.tools"),
        ContextPart::Memory => tr!("contextPanel.memory"),
        ContextPart::Conversation => tr!("tui.context.conversation"),
        ContextPart::Attachments => tr!("tui.context.attachments"),
    }
}

pub fn goal_status_label(status: GoalStatus) -> String {
    match status {
        GoalStatus::Active => tr!("composer.goal.status.active"),
        GoalStatus::Paused => tr!("composer.goal.status.paused"),
        GoalStatus::Blocked => tr!("composer.goal.status.blocked"),
        GoalStatus::UsageLimited => tr!("composer.goal.status.usageLimited"),
        GoalStatus::BudgetLimited => tr!("composer.goal.status.budgetLimited"),
        GoalStatus::Completed => tr!("composer.goal.status.completed"),
    }
}

/// `3 turns left` for live injections, the expired label otherwise.
/// 有效注入显示剩余轮次，否则显示已过期。
pub fn ttl_label(item: &InjectedMemory) -> String {
    if item.turns_left == 0 {
        tr!("workspaceMemory.expiredRegionTitle")
    } else {
        tr!("tui.memory.turnsLeft", count = item.turns_left)
    }
}

/// Actions of the goal bar in the order the WebUI shows them.
/// 目标栏的操作，顺序与 WebUI 一致。
pub fn goal_actions(conversation_id: &str, goal: &Goal) -> Vec<(String, bool, UiAction)> {
    let send = |op: GoalOp| {
        UiAction::Send(Command::Goal {
            conversation_id: conversation_id.to_string(),
            op,
        })
    };
    let mut actions = Vec::new();
    if goal.status == GoalStatus::Active {
        actions.push((
            tr!("composer.goal.action.pause"),
            false,
            send(GoalOp::Pause),
        ));
    } else if goal.status.resumable() {
        actions.push((
            tr!("composer.goal.action.resume"),
            false,
            send(GoalOp::Resume),
        ));
    }
    actions.push((
        tr!("composer.goal.action.edit"),
        false,
        edit_goal(conversation_id, Some(goal)),
    ));
    actions.push((tr!("composer.goal.action.clear"), true, send(GoalOp::Clear)));
    actions
}

/// Prompt that sets or replaces the conversation goal.
/// 设置或替换会话目标的输入框。
pub fn edit_goal(conversation_id: &str, goal: Option<&Goal>) -> UiAction {
    let title = if goal.is_some() {
        tr!("composer.goalDialog.editTitle")
    } else {
        tr!("composer.goal")
    };
    UiAction::Prompt(Box::new(PromptDialog::new(
        title,
        tr!("composer.goalDialog.objectivePlaceholder"),
        goal.map(|g| g.objective.clone()).unwrap_or_default(),
        PromptThen::SetGoal(conversation_id.to_string()),
        false,
    )))
}

/// Confirmation before stopping one subagent.
/// 停止单个子代理前的确认。
pub fn stop_agent(conversation_id: &str, agent: &AgentInfo) -> UiAction {
    confirm(
        tr!("conversation.subagent.stopConfirmTitle"),
        tr!(
            "conversation.subagent.stopConfirmDescription",
            task = agent.task.clone()
        ),
        tr!("conversation.subagent.stopConfirm"),
        UiAction::Send(Command::AgentControl {
            conversation_id: conversation_id.to_string(),
            agent_id: agent.id.clone(),
            op: AgentOp::Stop,
        }),
    )
}

/// Confirmation before ending one subagent.
/// 结束单个子代理前的确认。
pub fn close_agent(conversation_id: &str, agent: &AgentInfo) -> UiAction {
    confirm(
        tr!("conversation.subagent.closeConfirmTitle"),
        tr!(
            "conversation.subagent.closeConfirmDescription",
            task = agent.task.clone()
        ),
        tr!("conversation.subagent.closeConfirm"),
        UiAction::Send(Command::AgentControl {
            conversation_id: conversation_id.to_string(),
            agent_id: agent.id.clone(),
            op: AgentOp::Close,
        }),
    )
}

/// Prompt for a message to a subagent.
/// 向子代理发送消息的输入框。
pub fn message_agent(conversation_id: &str, agent: &AgentInfo, steering: bool) -> UiAction {
    let title = if steering {
        tr!("conversation.subagent.queue.steering")
    } else {
        tr!("conversation.subagent.queue.followup")
    };
    UiAction::Prompt(Box::new(PromptDialog::new(
        title,
        tr!("conversation.subagent.detail.message"),
        String::new(),
        PromptThen::AgentMessage {
            conversation_id: conversation_id.to_string(),
            agent_id: agent.id.clone(),
            steering,
        },
        false,
    )))
}

/// Buttons shown under an agent: messages while it can receive them, stop while running, end
/// until closed.
/// 代理下方的按钮：可接收消息时可发送消息，运行中可停止，未关闭前可结束。
pub fn agent_actions(conversation_id: &str, agent: &AgentInfo) -> Vec<(String, bool, UiAction)> {
    let mut actions = Vec::new();
    if agent.state != AgentState::Closed {
        actions.push((
            tr!("conversation.subagent.detail.message"),
            false,
            message_agent(conversation_id, agent, agent.state == AgentState::Running),
        ));
    }
    if agent.state == AgentState::Running {
        actions.push((
            tr!("conversation.subagent.stopTask"),
            true,
            stop_agent(conversation_id, agent),
        ));
    }
    if agent.state != AgentState::Closed {
        actions.push((
            tr!("conversation.subagent.closeTask"),
            true,
            close_agent(conversation_id, agent),
        ));
    }
    actions
}

/// Detail sheet of a panel row; `None` when the row does not exist.
/// 面板行的详情面板；行不存在时为 `None`。
pub fn sheet_for(app: &App, tab: PanelTab, index: usize) -> Option<DetailSheet> {
    let conversation_id = app.selected.clone()?;
    let session = app.session()?;
    match tab {
        PanelTab::Memory => {
            let entries = memory_entries(&session.insight, app.panel.memory_static);
            Some(memory_sheet(&conversation_id, entries.get(index)?))
        }
        PanelTab::Agents => Some(agent_sheet(&conversation_id, session.agents.get(index)?)),
        _ => None,
    }
}

fn memory_sheet(conversation_id: &str, entry: &MemoryEntry) -> DetailSheet {
    match entry {
        MemoryEntry::Dynamic(card, item) => dynamic_sheet(item, Some(card))
            .action(
                tr!("workspaceMemory.suppressForSession"),
                false,
                UiAction::Send(Command::SuppressMemory {
                    conversation_id: conversation_id.to_string(),
                    memory_id: item.memory_id.clone(),
                }),
            )
            .action(
                tr!("workspaceMemory.permanentDeleteInvalid"),
                true,
                delete_memory(conversation_id, item),
            ),
        MemoryEntry::Expired(item) => dynamic_sheet(item, None).action(
            tr!("workspaceMemory.permanentDeleteInvalid"),
            true,
            delete_memory(conversation_id, item),
        ),
        MemoryEntry::Profile(section, node) => DetailSheet::new(profile_section_label(section))
            .field(
                tr!("memory.profile.priority"),
                format!("P{}", node.priority),
            )
            .field(tr!("memory.profile.level"), format!("L{}", node.level))
            .field(tr!("tui.memory.weight"), format!("{:.2}", node.weight))
            .field(
                tr!("memory.profile.source"),
                profile_source_label(node.source),
            )
            .field(tr!("memory.profile.updatedAt"), node.date.clone())
            .section(tr!("tui.memory.content"), node.content.clone())
            .action(
                tr!("memory.profile.instruction"),
                false,
                UiAction::Prompt(Box::new(PromptDialog::new(
                    tr!("memory.profile.instruction"),
                    tr!("memory.profile.instructionPlaceholder"),
                    String::new(),
                    PromptThen::MemoryCorrect,
                    false,
                ))),
            ),
    }
}

/// Confirmation before permanently deleting an invalid memory.
/// 永久删除无效记忆前的确认。
fn delete_memory(conversation_id: &str, item: &InjectedMemory) -> UiAction {
    confirm(
        tr!("workspaceMemory.permanentDeleteInvalid"),
        item.content.clone(),
        tr!("agentSkills.delete"),
        UiAction::Send(Command::DeleteMemory {
            conversation_id: conversation_id.to_string(),
            memory_id: item.memory_id.clone(),
        }),
    )
}

fn dynamic_sheet(item: &InjectedMemory, card: Option<&MemoryCard>) -> DetailSheet {
    let mut sheet = DetailSheet::new(tr!(
        "workspaceMemory.handleMemoryWithId",
        memoryId = item.memory_id.clone()
    ));
    if let Some(card) = card {
        sheet = sheet.field(
            tr!("tui.memory.tag"),
            format!("{} · {}", card.tag, card.description),
        );
    }
    sheet = sheet
        .field(tr!("tui.memory.ttl"), ttl_label(item))
        .field(tr!("tui.memory.score"), format!("{:.2}", item.score))
        .field(tr!("memory.profile.source"), item.source.clone())
        .field(
            tr!("memory.profile.sourceTurns"),
            tr!("workspaceMemory.turnId", turnId = item.turn_id.clone()),
        )
        .section(tr!("tui.memory.content"), item.content.clone());
    if let Some((user, assistant)) = &item.dialogue {
        sheet = sheet.section(
            tr!(
                "workspaceMemory.viewDetailsWithTurn",
                turnId = item.turn_id.clone()
            ),
            format!(
                "{}\n{}",
                tr!("workspaceMemory.user", content = user.clone()),
                tr!("workspaceMemory.assistant", content = assistant.clone())
            ),
        );
    }
    if item.turns_left == 0 {
        sheet = sheet.section(
            tr!("workspaceMemory.expiredRegionTitle"),
            tr!("workspaceMemory.expiredRegionDescription"),
        );
    }
    sheet
}

fn agent_sheet(conversation_id: &str, agent: &AgentInfo) -> DetailSheet {
    let usage = &agent.usage;
    let mut sheet = DetailSheet::new(format!(
        "{} · {}",
        tr!("conversation.subagent.schedule"),
        agent.name
    ))
    .field(
        tr!("conversation.subagent.detail.agent"),
        format!("{} ({})", agent.name, agent.profile),
    )
    .field(
        tr!("conversation.subagent.detail.status"),
        agent_state_label(agent.state),
    )
    .field(
        tr!("conversation.usage.requests"),
        usage.requests.to_string(),
    )
    .field(
        tr!("conversation.usage.toolCalls"),
        usage.tool_calls.to_string(),
    )
    .field(
        tr!("conversation.turnStats.totalTokens"),
        usage.total().to_string(),
    )
    .section(tr!("tui.agents.task"), agent.task.clone())
    .section(
        tr!("conversation.subagent.detail.effectivePrompt"),
        agent.prompt.clone(),
    );
    let queued: Vec<String> = agent
        .pending
        .iter()
        .map(|m| {
            let kind = if m.steering {
                tr!("conversation.subagent.queue.steering")
            } else {
                tr!("conversation.subagent.queue.followup")
            };
            format!("[{kind}] {}", m.text)
        })
        .collect();
    sheet = sheet.section(
        tr!("conversation.subagent.detail.queued"),
        if queued.is_empty() {
            tr!("conversation.subagent.detail.noPendingWork")
        } else {
            queued.join("\n")
        },
    );
    if !agent.activity.is_empty() {
        sheet = sheet.section(tr!("tui.agents.activity"), agent.activity.join("\n"));
    }
    sheet = sheet.section(
        tr!("conversation.subagent.detail.result"),
        agent
            .result
            .clone()
            .unwrap_or_else(|| tr!("conversation.subagent.detail.noResult")),
    );
    for (label, danger, action) in agent_actions(conversation_id, agent) {
        sheet = sheet.action(label, danger, action);
    }
    sheet
}
