// Static mock data: projects, conversations, models, Git state and terminal sessions.
// 静态模拟数据：项目、会话、模型、Git 状态与终端会话。

use crate::model::*;

pub const BASE_URL: &str = "http://127.0.0.1:17921";

pub fn projects() -> Vec<Project> {
    vec![
        Project {
            id: "p-tui".into(),
            name: "vulcan-code-tui".into(),
            path: r"D:\openvulcan\vulcan-code-tui".into(),
            pinned: true,
        },
        Project {
            id: "p-core".into(),
            name: "vulcan-code".into(),
            path: r"D:\openvulcan\vulcan-code".into(),
            pinned: false,
        },
        Project {
            id: "p-lua".into(),
            name: "luaskills".into(),
            path: r"D:\openvulcan\luaskills".into(),
            pinned: false,
        },
    ]
}

pub fn conversations() -> Vec<ConversationSummary> {
    let row = |id: &str, title: &str, project: Option<&str>, label: &str| ConversationSummary {
        id: id.into(),
        title: title.into(),
        project_id: project.map(Into::into),
        run_phase: RunPhase::Idle,
        last_updated_label: label.into(),
        unread: false,
        attention: false,
    };
    let mut list = vec![
        row("c-tui", "TUI 框架與顯示 demo", Some("p-tui"), "now"),
        row("c-git", "Git 側欄在終端中的呈現", Some("p-tui"), "3m"),
        row("c-ask", "Ask 流程與焦點規則", Some("p-core"), "12m"),
        row(
            "c-provider",
            "Provider manifest 簽名檢查",
            Some("p-core"),
            "1h",
        ),
        row("c-skill", "技能配置 Schema 升級", Some("p-lua"), "2d"),
        row("c-scratch", "隨手筆記", None, "5d"),
    ];
    list[3].unread = true;
    list
}

pub fn models() -> Vec<ModelInfo> {
    let model = |id: &str, label: &str, provider: &str, window: u64| ModelInfo {
        id: id.into(),
        label: label.into(),
        provider: provider.into(),
        context_window: window,
    };
    vec![
        model("claude-opus-5-5", "Claude Opus 5.5", "Anthropic", 400_000),
        model(
            "claude-sonnet-5-5",
            "Claude Sonnet 5.5",
            "Anthropic",
            400_000,
        ),
        model("gpt-5.6", "GPT-5.6", "OpenAI", 272_000),
        model(
            "minimax-m3.1-flash-preview",
            "MiniMax M3.1 Flash Preview",
            "MiniMax",
            200_000,
        ),
    ]
}

pub fn git_snapshot() -> GitSnapshot {
    let file = |path: &str, status, staged, additions, deletions| GitFile {
        path: path.into(),
        status,
        staged,
        additions,
        deletions,
    };
    GitSnapshot {
        branch: "feat/tui-shell".into(),
        upstream: Some("origin/feat/tui-shell".into()),
        ahead: 2,
        behind: 0,
        files: vec![
            file(
                "src/ui/transcript.rs",
                GitFileStatus::Modified,
                true,
                84,
                12,
            ),
            file("src/app/mod.rs", GitFileStatus::Modified, true, 40, 8),
            file("src/ui/panel.rs", GitFileStatus::Modified, false, 22, 5),
            file(
                "docs/tui-design.md",
                GitFileStatus::Untracked,
                false,
                120,
                0,
            ),
            file("src/legacy_view.rs", GitFileStatus::Deleted, false, 0, 64),
        ],
        unpushed: vec![
            GitCommit {
                short_sha: "9c1e4a7".into(),
                subject: "feat(ui): add Vulcan theme tokens".into(),
            },
            GitCommit {
                short_sha: "3b72f0d".into(),
                subject: "feat(backend): introduce mock backend handle".into(),
            },
        ],
    }
}

pub fn branches(current: &str, extra: &[String]) -> Vec<GitBranch> {
    let mut names: Vec<(String, bool, &str)> = vec![
        (
            "main".into(),
            false,
            "364cfb3 feat(media): add native media reading",
        ),
        (
            "feat/tui-shell".into(),
            false,
            "9c1e4a7 feat(ui): add Vulcan theme tokens",
        ),
        (
            "feat/git-panel".into(),
            false,
            "a4d09e2 wip: git panel layout",
        ),
        (
            "fix/ask-focus".into(),
            false,
            "77e1c3b fix(ask): keep composer focus",
        ),
        (
            "origin/main".into(),
            true,
            "364cfb3 feat(media): add native media reading",
        ),
        (
            "origin/feat/tui-shell".into(),
            true,
            "d18a2c4 chore: bootstrap crate",
        ),
    ];
    for name in extra {
        if !names.iter().any(|(n, _, _)| n == name) {
            names.insert(0, (name.clone(), false, "HEAD"));
        }
    }
    names
        .into_iter()
        .map(|(name, remote, last)| GitBranch {
            current: name == current,
            name,
            remote,
            last_commit: last.into(),
        })
        .collect()
}

/// Deterministic fake diff so every file in the mock Git panel can be opened.
/// 确定性伪造的 diff，使模拟 Git 面板中的每个文件都能打开。
pub fn fake_diff(file: &GitFile) -> FileDiff {
    const CORPUS: &[&str] = &[
        "use ratatui::prelude::*;",
        "",
        "/// Renders one transcript block into wrapped lines.",
        "pub fn render_block(block: &Block, width: u16, theme: &Theme) -> Vec<Line<'static>> {",
        "    let mut lines = Vec::new();",
        "    match &block.kind {",
        "        BlockKind::User(text) => lines.extend(user_lines(text, width, theme)),",
        "        BlockKind::Tool(tool) => lines.push(tool_row(tool, theme)),",
        "        BlockKind::Reasoning(r) if r.expanded => lines.extend(reasoning_lines(r, width)),",
        "        _ => {}",
        "    }",
        "    lines",
        "}",
        "",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub enum Focus { Sidebar, Transcript, Composer, Panel, Ask }",
        "",
        "impl App {",
        "    pub fn on_tick(&mut self, now: Instant) {",
        "        self.animation_phase = now.duration_since(self.started).as_secs_f32();",
        "    }",
        "}",
    ];
    if file.status == GitFileStatus::Untracked || file.status == GitFileStatus::Added {
        let lines = (0..file.additions.min(40))
            .map(|i| DiffLine {
                kind: DiffLineKind::Add,
                old_no: None,
                new_no: Some(i + 1),
                text: markdown_line(i as usize).into(),
            })
            .collect();
        return FileDiff {
            path: file.path.clone(),
            hunks: vec![DiffHunk {
                header: format!("@@ -0,0 +1,{} @@", file.additions),
                lines,
            }],
        };
    }
    if file.status == GitFileStatus::Deleted {
        let lines = (0..file.deletions.min(40))
            .map(|i| DiffLine {
                kind: DiffLineKind::Delete,
                old_no: Some(i + 1),
                new_no: None,
                text: CORPUS[i as usize % CORPUS.len()].into(),
            })
            .collect();
        return FileDiff {
            path: file.path.clone(),
            hunks: vec![DiffHunk {
                header: format!("@@ -1,{} +0,0 @@", file.deletions),
                lines,
            }],
        };
    }
    let seed = file.path.bytes().map(u32::from).sum::<u32>();
    let mut hunks = Vec::new();
    let mut old_no = 10 + seed % 40;
    let mut new_no = old_no;
    let mut remaining_add = file.additions.min(30);
    let mut remaining_del = file.deletions.min(12);
    for hunk_index in 0..2u32 {
        let start_old = old_no;
        let start_new = new_no;
        let mut lines = Vec::new();
        let mut cursor = (seed + hunk_index * 7) as usize;
        let mut push = |kind: DiffLineKind, old: &mut u32, new: &mut u32| {
            let text = CORPUS[cursor % CORPUS.len()].to_string();
            cursor += 1;
            let (o, n) = match kind {
                DiffLineKind::Context => {
                    *old += 1;
                    *new += 1;
                    (Some(*old - 1), Some(*new - 1))
                }
                DiffLineKind::Add => {
                    *new += 1;
                    (None, Some(*new - 1))
                }
                DiffLineKind::Delete => {
                    *old += 1;
                    (Some(*old - 1), None)
                }
            };
            lines.push(DiffLine {
                kind,
                old_no: o,
                new_no: n,
                text,
            });
        };
        for _ in 0..3 {
            push(DiffLineKind::Context, &mut old_no, &mut new_no);
        }
        let dels = if hunk_index == 0 {
            remaining_del.div_ceil(2)
        } else {
            remaining_del
        };
        for _ in 0..dels {
            push(DiffLineKind::Delete, &mut old_no, &mut new_no);
        }
        remaining_del -= dels;
        let adds = if hunk_index == 0 {
            remaining_add.div_ceil(2)
        } else {
            remaining_add
        };
        for _ in 0..adds {
            push(DiffLineKind::Add, &mut old_no, &mut new_no);
        }
        remaining_add -= adds;
        for _ in 0..3 {
            push(DiffLineKind::Context, &mut old_no, &mut new_no);
        }
        hunks.push(DiffHunk {
            header: format!(
                "@@ -{start_old},{} +{start_new},{} @@ impl App",
                old_no - start_old,
                new_no - start_new
            ),
            lines,
        });
        old_no += 25;
        new_no += 25;
    }
    FileDiff {
        path: file.path.clone(),
        hunks,
    }
}

fn markdown_line(index: usize) -> &'static str {
    const DOC: &[&str] = &[
        "# Vulcan Code TUI design",
        "",
        "## Layout",
        "",
        "- Header: brand, project, conversation, model and mode.",
        "- Sidebar: projects and conversations with run phase markers.",
        "- Transcript: user input, assistant markdown, thinking and tool calls.",
        "- Right panel: Changes / Progress / Terminal / Agents.",
        "",
        "## Data flow",
        "",
        "Commands go to the backend handle; events come back as facts.",
    ];
    DOC[index % DOC.len()]
}

pub fn command_sessions() -> Vec<CommandSession> {
    vec![
        CommandSession {
            id: "cmd-watch".into(),
            title: "cargo watch".into(),
            command: "cargo watch -x \"check --all-targets\"".into(),
            running: true,
            exit_code: None,
            screen: vec![
                "[Running 'cargo check --all-targets']".into(),
                "    Checking vulcan-code-tui v0.1.0".into(),
                "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.41s".into(),
                "[Finished running. Exit status: 0]".into(),
            ],
        },
        CommandSession {
            id: "cmd-web".into(),
            title: "pnpm dev".into(),
            command: "pnpm --filter vulcan-code-web dev".into(),
            running: false,
            exit_code: Some(0),
            screen: vec![
                "  VITE v7.1.3  ready in 812 ms".into(),
                "  ➜  Local:   http://localhost:5173/".into(),
                "  ➜  press h + enter to show help".into(),
            ],
        },
    ]
}

pub fn initial_todo() -> Vec<TodoItem> {
    vec![
        TodoItem {
            content: "確認 TUI 框架結構".into(),
            status: TodoStatus::Completed,
        },
        TodoItem {
            content: "模擬思考、輸出與工具呼叫".into(),
            status: TodoStatus::InProgress,
        },
        TodoItem {
            content: "Git 側欄：diff、提交、推送、分支".into(),
            status: TodoStatus::Pending,
        },
        TodoItem {
            content: "對接 core service RPC 與事件流".into(),
            status: TodoStatus::Pending,
        },
    ]
}
