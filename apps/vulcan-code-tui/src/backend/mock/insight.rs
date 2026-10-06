// Mock session insight: injected memory, static profile, lifetime usage, context breakdown and
// goal fixtures, plus the per-turn bookkeeping the service would do.
// 模拟会话洞察：注入记忆、静态画像、生命周期用量、上下文构成与目标的样例数据，以及服务在每轮
// 结束时会做的记账。

use crate::model::*;

/// A usage bucket with plausible cache, reasoning and timing splits.
/// 带有合理缓存、推理与耗时拆分的用量桶。
pub fn usage(requests: u64, tool_calls: u64, input: u64, output: u64) -> UsageBucket {
    let cache_read = input * 62 / 100;
    UsageBucket {
        requests,
        unknown_usage: 0,
        tool_calls,
        input,
        uncached_input: input - cache_read,
        cache_read,
        cache_write: input * 8 / 100,
        output,
        reasoning: output * 35 / 100,
        first_token_ms: requests * 1_380,
        first_token_samples: requests,
        duration_ms: requests * 9_600,
        duration_samples: requests,
        measured_output: output,
        measured_output_ms: requests * 8_900,
    }
}

/// How the used context window splits across its parts.
/// 已用上下文窗口在各组成部分间的拆分。
pub fn breakdown(used: u64) -> Vec<ContextSlice> {
    let system = used * 9 / 100;
    let tools = used * 14 / 100;
    let memory = used * 4 / 100;
    let attachments = used * 3 / 100;
    let conversation = used - system - tools - memory - attachments;
    vec![
        ContextSlice {
            part: ContextPart::SystemPrompt,
            tokens: system,
        },
        ContextSlice {
            part: ContextPart::Tools,
            tokens: tools,
        },
        ContextSlice {
            part: ContextPart::Memory,
            tokens: memory,
        },
        ContextSlice {
            part: ContextPart::Conversation,
            tokens: conversation,
        },
        ContextSlice {
            part: ContextPart::Attachments,
            tokens: attachments,
        },
    ]
}

fn node(
    id: &str,
    date: &str,
    priority: u8,
    level: u8,
    weight: f32,
    source: ProfileSource,
    content: &str,
) -> ProfileNode {
    ProfileNode {
        id: id.into(),
        date: date.into(),
        priority,
        level,
        weight,
        content: content.into(),
        source,
    }
}

/// The static profile every conversation of the demo project sees.
/// 演示项目中每个会话都能看到的静态画像。
fn profile() -> Vec<ProfileSection> {
    vec![
        ProfileSection {
            user: true,
            name: "lingeasy".into(),
            nodes: vec![
                node(
                    "p-1",
                    "2026-09-28",
                    1,
                    1,
                    0.95,
                    ProfileSource::SystemSeed,
                    "偏好繁體中文溝通；程式碼註解採英文一行、中文一行的雙語格式。",
                ),
                node(
                    "p-2",
                    "2026-09-28",
                    1,
                    2,
                    0.90,
                    ProfileSource::TurnExtract,
                    "重視完整復刻與一致性，先確認框架與 mock 流程，再對接真實資料。",
                ),
                node(
                    "p-3",
                    "2026-09-30",
                    2,
                    2,
                    0.72,
                    ProfileSource::ManualInstruction,
                    "測試與建置須限制並行數（jobs = 4），共享資源測試串行執行。",
                ),
            ],
        },
        ProfileSection {
            user: false,
            name: "vulcan-code-tui".into(),
            nodes: vec![
                node(
                    "q-1",
                    "2026-09-29",
                    1,
                    1,
                    0.93,
                    ProfileSource::TurnExtract,
                    "Rust 2024 + ratatui 0.30；mock 後端透過 BackendHandle 與介面解耦，對接時只替換資料來源。",
                ),
                node(
                    "q-2",
                    "2026-09-29",
                    2,
                    2,
                    0.81,
                    ProfileSource::ManualInstruction,
                    "不直接修改上游 vulcan-code；未經要求不提交。",
                ),
                node(
                    "q-3",
                    "2026-10-01",
                    2,
                    3,
                    0.66,
                    ProfileSource::TurnExtract,
                    "TUI 專用文案放在 i18n/overlay 的 tui.* 命名空間，其餘鍵從 WebUI 目錄同步。",
                ),
            ],
        },
    ]
}

/// Insight of a fresh conversation: the shared profile and a breakdown, nothing else yet.
/// 新会话的洞察：共享画像与上下文构成，其余为空。
pub fn baseline(used: u64) -> SessionInsight {
    SessionInsight {
        memory: ContextInjection {
            profile: profile(),
            ..ContextInjection::default()
        },
        breakdown: breakdown(used),
        ..SessionInsight::default()
    }
}

fn injected(
    id: &str,
    turn: &str,
    turns_left: u32,
    score: f32,
    source: &str,
    content: &str,
    dialogue: Option<(&str, &str)>,
) -> InjectedMemory {
    InjectedMemory {
        memory_id: id.into(),
        content: content.into(),
        turn_id: turn.into(),
        turns_left,
        score,
        source: source.into(),
        dialogue: dialogue.map(|(u, a)| (u.into(), a.into())),
    }
}

/// The demo conversation: recalled memory cards, expired injections, lifetime usage and a goal.
/// 演示会话：召回的记忆卡、已过期注入、生命周期用量与目标。
pub fn seeded(used: u64) -> SessionInsight {
    let mut insight = baseline(used);
    insight.memory.dynamic = vec![
        MemoryCard {
            tag: "tui-architecture".into(),
            description: "TUI 架構決策".into(),
            items: vec![
                injected(
                    "m-101",
                    "t-812",
                    3,
                    0.91,
                    "turn_extract",
                    "TUI 以獨立 crate 實作，前期不改動上游 vulcan-code；外掛面板延後到雙向適配階段。",
                    Some((
                        "TUI 需要獨立專案嗎？",
                        "需要，放在 D:\\openvulcan\\vulcan-code-tui，之後再做雙向適配。",
                    )),
                ),
                injected(
                    "m-102",
                    "t-815",
                    2,
                    0.86,
                    "turn_extract",
                    "多語言文案由同步工具從 WebUI 目錄拉取，只保留實際用到的鍵，語系種類與 WebUI 一致。",
                    None,
                ),
            ],
        },
        MemoryCard {
            tag: "ui-feedback".into(),
            description: "介面回饋".into(),
            items: vec![injected(
                "m-301",
                "t-830",
                4,
                0.88,
                "turn_extract",
                "Logo 不使用字元畫；所有區域都要支援滑鼠點擊，思考內容可展開，歷史可探查。",
                Some((
                    "滑鼠點擊區域無效，只能 Tab。",
                    "改為每幀登記命中區域，點擊與鍵盤走同一條動作路徑。",
                )),
            )],
        },
        MemoryCard {
            tag: "rust-style".into(),
            description: "Rust 程式風格".into(),
            items: vec![
                injected(
                    "m-201",
                    "t-640",
                    5,
                    0.78,
                    "system_seed",
                    "面向 AI 的工具名稱、描述與錯誤使用英文；面向使用者的文案可使用中文。",
                    None,
                ),
                injected(
                    "m-202",
                    "t-702",
                    1,
                    0.74,
                    "turn_extract",
                    "同一語意的邊界值只允許一處定義，其餘位置一律派生或引用。",
                    None,
                ),
            ],
        },
    ];
    insight.memory.expired = vec![
        injected(
            "m-090",
            "t-590",
            0,
            0.41,
            "turn_extract",
            "暫時以 Tab 切換焦點作為唯一的互動方式。",
            None,
        ),
        injected(
            "m-091",
            "t-602",
            0,
            0.37,
            "turn_extract",
            "右側面板只保留 Git 變更一個分頁。",
            None,
        ),
    ];
    insight.lifetime = LifetimeUsage {
        main: usage(14, 37, 612_000, 18_400),
        children: Vec::new(),
        history_incomplete: true,
    };
    insight.goal = Some(Goal {
        objective: "完成 TUI 展示 demo：記憶、統計、代理面板與系統頁全部可操作".into(),
        status: GoalStatus::Active,
        elapsed_ms: 38 * 60_000,
        usage: usage(9, 24, 402_000, 11_800),
        budget: Some(1_500_000),
    });
    insight
}

/// Closes one finished turn: goal progress, TTL countdown and the new breakdown. The turn's usage
/// is already in the lifetime (booked per request by the caller).
/// 结束一轮：目标进度、TTL 倒数与新的上下文构成。该轮用量已由调用方按请求记入生命周期。
pub fn finish_turn(insight: &mut SessionInsight, turn: &UsageBucket, duration_ms: u64, used: u64) {
    if let Some(goal) = &mut insight.goal
        && goal.status == GoalStatus::Active
    {
        goal.usage.add(turn);
        goal.elapsed_ms += duration_ms;
        if goal.budget.is_some_and(|b| goal.usage.total() >= b) {
            goal.status = GoalStatus::BudgetLimited;
        }
    }
    let mut expired = Vec::new();
    for card in &mut insight.memory.dynamic {
        for item in &mut card.items {
            item.turns_left = item.turns_left.saturating_sub(1);
        }
        let (gone, live): (Vec<_>, Vec<_>) = card.items.drain(..).partition(|i| i.turns_left == 0);
        card.items = live;
        expired.extend(gone);
    }
    insight.memory.dynamic.retain(|c| !c.items.is_empty());
    insight.memory.expired.extend(expired);
    insight.breakdown = breakdown(used);
}

/// Keeps one lifetime row per subagent, updated from the latest agent list.
/// 每个子代理保留一行生命周期用量，并按最新代理列表更新。
pub fn sync_children(insight: &mut SessionInsight, agents: &[AgentInfo]) {
    for agent in agents {
        let children = &mut insight.lifetime.children;
        match children.iter_mut().find(|(name, _)| *name == agent.name) {
            Some((_, bucket)) => *bucket = agent.usage,
            None => children.push((agent.name.clone(), agent.usage)),
        }
    }
}

/// Removes one memory from the dynamic and expired injections; true when it existed.
/// 从动态与已过期注入中移除一条记忆；存在时返回真。
pub fn remove_memory(insight: &mut SessionInsight, memory_id: &str, include_expired: bool) -> bool {
    let memory = &mut insight.memory;
    let before = memory.dynamic_count();
    for card in &mut memory.dynamic {
        card.items.retain(|i| i.memory_id != memory_id);
    }
    memory.dynamic.retain(|c| !c.items.is_empty());
    if include_expired {
        memory.expired.retain(|i| i.memory_id != memory_id);
    }
    memory.dynamic_count() != before
}

/// A goal change of the demo, one per goal command.
/// demo 的目标变更，每个目标命令对应一种。
pub enum GoalChange {
    /// `updateSessionGoalStatus`
    Status(GoalStatus),
    /// `clearSessionGoal`
    Clear,
    /// `saveSessionGoal` / `resumeSessionGoal`
    Set {
        objective: String,
        budget: Option<u64>,
    },
}

/// Applies a goal change; returns the new status for the notice, or `None` when unchanged.
/// 执行目标变更；返回用于提示的新状态，未变化时返回 `None`。
pub fn apply_goal(insight: &mut SessionInsight, change: GoalChange) -> Option<Option<GoalStatus>> {
    match change {
        GoalChange::Status(status) => {
            let goal = insight.goal.as_mut()?;
            (goal.status != status).then(|| {
                goal.status = status;
                Some(status)
            })
        }
        GoalChange::Clear => insight.goal.take().map(|_| None),
        GoalChange::Set { objective, budget } => {
            let objective = objective.trim().to_string();
            if objective.is_empty() {
                return None;
            }
            match &mut insight.goal {
                Some(goal) => {
                    goal.objective = objective;
                    goal.budget = budget.or(goal.budget);
                    goal.status = GoalStatus::Active;
                }
                None => {
                    insight.goal = Some(Goal {
                        objective,
                        status: GoalStatus::Active,
                        elapsed_ms: 0,
                        usage: UsageBucket::default(),
                        budget,
                    })
                }
            }
            Some(Some(GoalStatus::Active))
        }
    }
}
