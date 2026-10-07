// Scripted conversation turns for the mock backend.
// 模拟后端的脚本化会话轮次。
//
// A script is a queue of `(delay, Action)` steps. The world executes each action after its delay,
// producing exactly the event stream the service would: an event upsert opens a block, text appends
// stream into it, and a final upsert settles its status.
// 脚本是 `(延迟, 动作)` 步骤队列。世界在延迟后执行每个动作，产生与服务一致的事件流：事件 upsert
// 打开一个块，文本追加流入其中，最后一次 upsert 固定其状态。

use std::collections::VecDeque;
use std::time::Duration;

use serde_json::{Value, json};

use crate::model::*;

#[derive(Debug, Clone)]
pub enum Action {
    Upsert {
        seq: u64,
        body: EventBody,
    },
    Append {
        seq: u64,
        text: String,
    },
    /// Marks a streaming reasoning/text event completed.
    /// 将流式思考/文本事件标记为完成。
    Settle {
        seq: u64,
    },
    ToolDone {
        seq: u64,
        status: ToolCallStatus,
        output: String,
        changeset: Vec<ChangesetFile>,
    },
    AwaitAnswer {
        tool_seq: u64,
        question: PendingQuestion,
    },
    Todo(Vec<TodoItem>),
    GitTouch(GitFile),
    Terminal {
        id: String,
        title: String,
        command: String,
        line: Option<String>,
        done: Option<i32>,
    },
    Agents(Vec<AgentInfo>),
    Note {
        severity: NoticeSeverity,
        message: String,
    },
    /// Moves the conversation to another run phase.
    /// 将会话切换到另一运行阶段。
    Phase(RunPhase),
    Complete {
        duration_ms: Option<u64>,
    },
}

pub struct Script {
    pub steps: VecDeque<(Duration, Action)>,
    pub next_seq: u64,
    pub turn_id: String,
    pub tool_calls: u32,
}

const CHUNK_DELAY: Duration = Duration::from_millis(28);

impl Script {
    pub fn new(next_seq: u64, turn_id: String) -> Self {
        Script {
            steps: VecDeque::new(),
            next_seq,
            turn_id,
            tool_calls: 0,
        }
    }

    fn seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }

    pub fn push(&mut self, delay_ms: u64, action: Action) {
        self.steps
            .push_back((Duration::from_millis(delay_ms), action));
    }

    pub fn user(&mut self, text: &str, source: UserInputSource) {
        self.user_with(text, source, &[]);
    }

    pub fn user_with(&mut self, text: &str, source: UserInputSource, attachments: &[String]) {
        let seq = self.seq();
        self.push(
            0,
            Action::Upsert {
                seq,
                body: EventBody::UserInput {
                    text: text.into(),
                    input_source: source,
                    attachments: attachments.to_vec(),
                },
            },
        );
    }

    /// Streams reasoning text into a new block, then settles it.
    /// 将思考文本流入新块并在结束时固定。
    pub fn reasoning(&mut self, delay_ms: u64, text: &str) {
        let seq = self.seq();
        self.push(
            delay_ms,
            Action::Upsert {
                seq,
                body: EventBody::AssistantReasoning {
                    text: String::new(),
                    status: AssistantSemanticStatus::Streaming,
                },
            },
        );
        self.stream(seq, text);
        self.push(120, Action::Settle { seq });
    }

    pub fn text(&mut self, delay_ms: u64, text: &str) {
        let seq = self.seq();
        self.push(
            delay_ms,
            Action::Upsert {
                seq,
                body: EventBody::AssistantText {
                    html_answers_allowed: true,
                    text: String::new(),
                    status: AssistantSemanticStatus::Streaming,
                },
            },
        );
        self.stream(seq, text);
        self.push(60, Action::Settle { seq });
    }

    fn stream(&mut self, seq: u64, text: &str) {
        let mut chunk = String::new();
        let mut width = 0;
        for ch in text.chars() {
            chunk.push(ch);
            width += if ch.is_ascii() { 1 } else { 2 };
            if width >= 6 || ch == '\n' {
                self.steps.push_back((
                    CHUNK_DELAY,
                    Action::Append {
                        seq,
                        text: std::mem::take(&mut chunk),
                    },
                ));
                width = 0;
            }
        }
        if !chunk.is_empty() {
            self.steps
                .push_back((CHUNK_DELAY, Action::Append { seq, text: chunk }));
        }
    }

    /// Opens a running tool call and returns its seq; finish it with `tool_done`.
    /// 打开一个运行中的工具调用并返回其 seq；用 `tool_done` 结束。
    pub fn tool_start(&mut self, delay_ms: u64, name: &str, input: Value) -> u64 {
        let seq = self.seq();
        self.tool_calls += 1;
        self.push(
            delay_ms,
            Action::Upsert {
                seq,
                body: EventBody::ToolCall {
                    tool_name: name.into(),
                    input,
                    output: None,
                    changeset: Vec::new(),
                    status: ToolCallStatus::Running,
                    started_at_ms: None,
                    finished_at_ms: None,
                },
            },
        );
        seq
    }

    pub fn tool_done(&mut self, delay_ms: u64, seq: u64, status: ToolCallStatus, output: &str) {
        self.push(
            delay_ms,
            Action::ToolDone {
                seq,
                status,
                output: output.into(),
                changeset: Vec::new(),
            },
        );
    }

    pub fn tool(&mut self, delay_ms: u64, name: &str, input: Value, run_ms: u64, output: &str) {
        let seq = self.tool_start(delay_ms, name, input);
        self.tool_done(run_ms, seq, ToolCallStatus::Completed, output);
    }
}

/// Builds the scripted reply to one submitted prompt.
/// 为一次提交的提示构建脚本化回复。
pub fn build_turn(
    script: &mut Script,
    prompt: &str,
    source: UserInputSource,
    mode: ExecutionMode,
    attachments: &[String],
) {
    let lower = prompt.to_lowercase();
    script.user_with(prompt, source, attachments);
    // Memory recall (PreCheck) runs before every answer, as in the service.
    // 与服务一致，每次回答前先进行记忆召回（PreCheck）。
    script.push(0, Action::Phase(RunPhase::MemoryRecall));
    script.push(900, Action::Phase(RunPhase::Running));

    if mode == ExecutionMode::Plan {
        plan_turn(script, prompt);
        return;
    }

    script.reasoning(
        350,
        "使用者想看到 TUI 的主要環節能被完整模擬。先確認目前的版面：側欄、對話區、右側面板與輸入框。\n\
         接著需要示範工具呼叫的生命週期（running → completed），以及 Git 變更如何反映到右側面板。\n\
         Keep the event shapes aligned with conversation_events_v1 so wiring later only swaps the backend.",
    );
    script.text(
        150,
        "我先檢查目前的版面實作，然後執行一次編譯檢查，最後更新思考框的淡出邏輯。",
    );
    script.tool(
        200,
        "read_file",
        json!({ "path": "src/ui/mod.rs", "offset": 1, "limit": 120 }),
        650,
        "   1 // Root layout: header, sidebar, transcript, panel, composer, status bar.\n   2 use ratatui::prelude::*;\n   …\n 118 pub fn draw(frame: &mut Frame, app: &mut App) {",
    );
    script.tool(
        120,
        "grep",
        json!({ "pattern": "tr!\\(", "path": "src", "output_mode": "count" }),
        480,
        "src/ui/panel.rs: 31\nsrc/ui/transcript.rs: 18\nsrc/ui/overlays.rs: 44\nsrc/app/mod.rs: 9",
    );

    if lower.contains("ask") || prompt.contains('問') || prompt.contains("確認") {
        let seq = script.tool_start(
            200,
            "ask",
            json!({ "questions": [{ "header": "Thinking box", "multiple": false }] }),
        );
        script.push(
            300,
            Action::AwaitAnswer {
                tool_seq: seq,
                question: PendingQuestion {
                    id: format!("{}-question", script.turn_id),
                    questions: vec![
                        Question {
                            header: "思考框位置".into(),
                            question: "動態思考框在終端中要放在哪裡？".into(),
                            options: vec![
                                QuestionOption {
                                    label: "輸入框上方".into(),
                                    description: "浮動於對話區底部，跟隨串流內容".into(),
                                },
                                QuestionOption {
                                    label: "右側".into(),
                                    description: "固定在對話區右側，不遮擋輸出".into(),
                                },
                            ],
                            multiple: false,
                        },
                        Question {
                            header: "完成後".into(),
                            question: "思考完成後保留哪些資訊？（可複選）".into(),
                            options: vec![
                                QuestionOption {
                                    label: "折疊的思考過程".into(),
                                    description: "在對話中保留可展開的區塊".into(),
                                },
                                QuestionOption {
                                    label: "耗時".into(),
                                    description: "顯示思考花費的時間".into(),
                                },
                                QuestionOption {
                                    label: "Token 數".into(),
                                    description: "顯示推理使用的 token".into(),
                                },
                            ],
                            multiple: true,
                        },
                    ],
                },
            },
        );
    }

    let failing = lower.contains("fail") || prompt.contains('錯');
    let terminal_id = format!("cmd-{}", script.turn_id);
    let seq = script.tool_start(
        150,
        "shell_command",
        json!({ "command": "cargo check --all-targets", "timeout_ms": 600000 }),
    );
    let lines: &[&str] = if failing {
        &[
            "    Checking vulcan-code-tui v0.1.0",
            "error[E0308]: mismatched types",
            "  --> src/ui/thinking.rs:42:18",
            "error: could not compile `vulcan-code-tui` due to 1 previous error",
        ]
    } else {
        &[
            "    Checking ratatui v0.30.2",
            "    Checking ratatui-textarea v0.9.2",
            "    Checking vulcan-code-tui v0.1.0",
            "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.08s",
        ]
    };
    for (index, line) in lines.iter().enumerate() {
        script.push(
            if index == 0 { 200 } else { 420 },
            Action::Terminal {
                id: terminal_id.clone(),
                title: "cargo check".into(),
                command: "cargo check --all-targets".into(),
                line: Some((*line).into()),
                done: None,
            },
        );
    }
    let exit = if failing { 101 } else { 0 };
    script.push(
        200,
        Action::Terminal {
            id: terminal_id,
            title: "cargo check".into(),
            command: "cargo check --all-targets".into(),
            line: None,
            done: Some(exit),
        },
    );
    script.tool_done(
        50,
        seq,
        if failing {
            ToolCallStatus::Failed
        } else {
            ToolCallStatus::Completed
        },
        &lines.join("\n"),
    );
    if failing {
        script.push(
            150,
            Action::Note {
                severity: NoticeSeverity::Warning,
                message: "shell_command exited with code 101; the model will retry with a fix."
                    .into(),
            },
        );
    }

    if lower.contains("agent") || prompt.contains("代理") {
        agent_steps(script);
    }

    let patch = script.tool_start(
        250,
        "apply_patch",
        json!({ "path": "src/ui/thinking.rs", "summary": "fade out the thinking overlay after completion" }),
    );
    script.push(
        700,
        Action::ToolDone {
            seq: patch,
            status: ToolCallStatus::Completed,
            output: "Updated src/ui/thinking.rs".into(),
            changeset: vec![ChangesetFile {
                path: "src/ui/thinking.rs".into(),
                additions: 46,
                deletions: 3,
            }],
        },
    );
    script.push(
        0,
        Action::GitTouch(GitFile {
            path: "src/ui/thinking.rs".into(),
            status: GitFileStatus::Modified,
            staged: false,
            additions: 46,
            deletions: 3,
        }),
    );
    script.tool(
        150,
        "todolist",
        json!({ "action": "update" }),
        200,
        "2 items updated",
    );
    script.push(
        0,
        Action::Todo(vec![
            TodoItem {
                content: "確認 TUI 框架結構".into(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "模擬思考、輸出與工具呼叫".into(),
                status: TodoStatus::Completed,
            },
            TodoItem {
                content: "Git 側欄：diff、提交、推送、分支".into(),
                status: TodoStatus::InProgress,
            },
            TodoItem {
                content: "對接 core service RPC 與事件流".into(),
                status: TodoStatus::Pending,
            },
        ]),
    );

    script.reasoning(
        300,
        "Check passed; summarize what changed and what remains for wiring.",
    );
    script.text(
        120,
        &format!(
            "## 完成\n\n已依照「{}」完成一輪模擬：\n\n\
             - 思考框在串流時浮動顯示，結束後淡出並折疊成 **思考過程** 區塊\n\
             - 工具呼叫以 `running → completed` 呈現，連續呼叫會合併為一組\n\
             - `apply_patch` 的變更已同步到右側 **Changes** 面板\n\n\
             ```rust\nfn fade(elapsed: Duration) -> f32 {{\n    1.0 - (elapsed.as_secs_f32() / 0.9).min(1.0)\n}}\n```\n\n\
             > 下一步：確認框架結構後，以服務後端取代 mock backend。",
            truncate(prompt, 24)
        ),
    );
    script.push(80, Action::Complete { duration_ms: None });
}

fn plan_turn(script: &mut Script, prompt: &str) {
    script.reasoning(
        300,
        "Plan mode: do not modify files. Read the relevant modules and produce a step-by-step plan.",
    );
    script.tool(
        150,
        "read_file",
        json!({ "path": "src/app/mod.rs" }),
        500,
        "pub struct App { … }",
    );
    script.text(
        120,
        &format!(
            "### 計畫：{}\n\n1. 定義 `ServiceBackend`，實作與 mock 相同的 `BackendHandle`\n\
             2. 把 `/events` WebSocket 的 `conversationEventDeltaV1` 映射為 `EventUpsert` / `TextAppend`\n\
             3. 將 Git RPC 映射到 `Command::Git*`\n4. 保持視圖層不變，只替換資料來源\n",
            truncate(prompt, 24)
        ),
    );
    script.push(80, Action::Complete { duration_ms: None });
}

/// One scripted subagent: identity, the activity it has shown so far and its result once done.
/// 一个脚本化子代理：身份、目前显示过的活动，以及完成后的结果。
struct AgentPlan {
    id: &'static str,
    name: &'static str,
    profile: &'static str,
    task: &'static str,
    prompt: &'static str,
    activity: &'static [&'static str],
    result: &'static str,
}

const EXPLORER: AgentPlan = AgentPlan {
    id: "agent-1",
    name: "explorer",
    profile: "Explore",
    task: "Survey the service event kinds the TUI must map (conversationEventDeltaV1, workspaceGitChanged, projectsChanged).",
    prompt: "You are a read-only exploration agent. List every `/events` WebSocket event kind the service emits, \
             with the payload fields the TUI reducer needs. Do not modify files.",
    activity: &[
        "read_file services/events/mod.rs",
        "grep \"EventKind::\" services/",
        "read_file apps/vulcan-code-web/src/service/events.ts",
        "summarize 14 event kinds",
    ],
    result: "Found 14 event kinds. The TUI needs conversationEventDeltaV1 (events + text_appends), \
             conversationSummaryChanged, workspaceGitChanged and projectsChanged; the rest are WebUI-only.",
};

const REVIEWER: AgentPlan = AgentPlan {
    id: "agent-2",
    name: "reviewer",
    profile: "Review",
    task: "Review Git panel key bindings for conflicts with the composer and global shortcuts.",
    prompt: "You are a code review agent. Check src/app/input for key bindings of the Git panel that \
             collide with global shortcuts or text input, and report each conflict with a fix.",
    activity: &[
        "read_file src/app/input/regions.rs",
        "read_file src/app/input/mod.rs",
        "compare 23 bindings",
    ],
    result: "No hard conflicts. `p` (push) only fires with the panel focused; suggest showing Alt+P in the \
             button bar so it is discoverable.",
};

/// The agent after `step` of its activity; the last step returns the result.
/// 代理执行到活动第 `step` 步时的状态；最后一步返回结果。
fn agent_at(plan: &AgentPlan, step: usize) -> AgentInfo {
    let done = step >= plan.activity.len();
    let seen = step.min(plan.activity.len());
    let step = step as u64;
    AgentInfo {
        id: plan.id.into(),
        name: plan.name.into(),
        profile: plan.profile.into(),
        task: plan.task.into(),
        state: if done {
            AgentState::Standby
        } else {
            AgentState::Running
        },
        prompt: plan.prompt.into(),
        pending: Vec::new(),
        activity: plan.activity[..seen]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        result: done.then(|| plan.result.into()),
        usage: super::insight::usage(step + 1, step * 2, 18_000 * (step + 1), 900 * (step + 1)),
    }
}

/// Subagents of the demo conversation: one finished, one still working with a queued message.
/// 演示会话的子代理：一个已完成，一个仍在工作且有排队消息。
pub fn seed_agents() -> Vec<AgentInfo> {
    let mut reviewer = agent_at(&REVIEWER, 2);
    reviewer.pending.push(AgentMessage {
        steering: true,
        text: "Also check the Alt+digit panel shortcuts.".into(),
    });
    vec![agent_at(&EXPLORER, EXPLORER.activity.len()), reviewer]
}

fn agent_steps(script: &mut Script) {
    let seq = script.tool_start(
        200,
        "subagent",
        json!({ "action": "create", "agents": ["explorer", "reviewer"] }),
    );
    // Both agents advance one activity per step; the reviewer has fewer and finishes first.
    // 两个代理每步推进一项活动；reviewer 的活动较少，先完成。
    let steps = EXPLORER.activity.len().max(REVIEWER.activity.len());
    for step in 0..=steps {
        script.push(
            if step == 0 { 300 } else { 900 },
            Action::Agents(vec![agent_at(&EXPLORER, step), agent_at(&REVIEWER, step)]),
        );
    }
    script.tool_done(
        100,
        seq,
        ToolCallStatus::Completed,
        "2 agents returned results",
    );
}

/// Background script for a conversation the user is not looking at.
/// 用户未查看的会话的后台脚本。
pub fn background_turn(script: &mut Script) {
    script.user("整理 Git 側欄在窄終端下的呈現方式", UserInputSource::Root);
    script.reasoning(
        400,
        "Narrow terminals cannot fit the right panel. Fold it into a drawer toggled by Ctrl+\\.",
    );
    script.tool(
        200,
        "read_file",
        json!({ "path": "src/ui/panel.rs" }),
        4000,
        "…",
    );
    script.tool(
        200,
        "shell_command",
        json!({ "command": "git status --short" }),
        6000,
        " M src/ui/panel.rs",
    );
    script.text(200, "窄終端下右側面板改為抽屜，Ctrl+\\ 開關。");
    script.push(100, Action::Complete { duration_ms: None });
}

/// A turn that stops at an `ask`, used to seed the conversation that needs attention.
/// 停在 `ask` 的轮次，用于预置需要处理的会话。
pub fn ask_seed_turn(script: &mut Script) {
    script.user("Ask 面板在 TUI 中要不要搶焦點？", UserInputSource::Root);
    script.reasoning(
        0,
        "The contract says Ask is non-modal. Confirm the focus rule with the user.",
    );
    let seq = script.tool_start(0, "ask", json!({ "questions": 1 }));
    script.push(
        0,
        Action::AwaitAnswer {
            tool_seq: seq,
            question: PendingQuestion {
                id: "seed-question".into(),
                questions: vec![Question {
                    header: "焦點".into(),
                    question: "提問出現時，焦點規則為何？".into(),
                    options: vec![
                        QuestionOption {
                            label: "輸入框為空時才移動焦點".into(),
                            description: "不打斷正在輸入的內容".into(),
                        },
                        QuestionOption {
                            label: "永不移動焦點".into(),
                            description: "以 Tab 切換到提問面板".into(),
                        },
                    ],
                    multiple: false,
                }],
            },
        },
    );
}

pub fn truncate(text: &str, max_chars: usize) -> String {
    let flat = text.replace('\n', " ");
    if flat.chars().count() <= max_chars {
        flat
    } else {
        let mut out: String = flat.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}
