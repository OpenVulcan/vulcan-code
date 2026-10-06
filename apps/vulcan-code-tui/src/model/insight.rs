// Per-conversation insight: memory injected into the context, lifetime usage, context breakdown
// and the conversation goal.
// 每个会话的洞察数据：注入上下文的记忆、生命周期用量、上下文构成与会话目标。
//
// Shapes follow the WebUI right panel (`contextPanel.dynamic` / `contextPanel.static`), the
// conversation usage popover (`conversation.usage.*`) and the goal bar (`composer.goal.*`).
// 形状对应 WebUI 右侧面板（动态 / 静态上下文）、会话用量浮层与目标栏。

/// Token and timing counters of one usage scope (main session, one subagent, one goal).
/// 单个用量范围（主会话、单个子代理、单个目标）的 token 与耗时计数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageBucket {
    pub requests: u64,
    /// Requests whose provider did not report usage; token totals are then lower bounds.
    /// 提供方未报告用量的请求数；此时 token 合计只是下限。
    pub unknown_usage: u64,
    pub tool_calls: u64,
    pub input: u64,
    pub uncached_input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
    pub reasoning: u64,
    pub first_token_ms: u64,
    pub first_token_samples: u64,
    pub duration_ms: u64,
    pub duration_samples: u64,
    /// Output tokens of the requests that also reported timing, for the speed average.
    /// 同时报告了耗时的请求的输出 token，用于计算平均速度。
    pub measured_output: u64,
    pub measured_output_ms: u64,
}

impl UsageBucket {
    pub fn total(&self) -> u64 {
        self.input + self.output
    }

    /// True when some requests did not report usage, so totals are only lower bounds.
    /// 部分请求未报告用量时为真，此时合计只是下限。
    pub fn partial(&self) -> bool {
        self.unknown_usage > 0
    }

    /// Cache hit rate over input tokens; `None` without complete input usage.
    /// 输入 token 的缓存命中率；输入用量不完整时为 `None`。
    pub fn cache_hit_rate(&self) -> Option<f32> {
        (self.input > 0 && !self.partial()).then(|| self.cache_read as f32 / self.input as f32)
    }

    pub fn average_first_token_ms(&self) -> Option<u64> {
        (self.first_token_samples > 0).then(|| self.first_token_ms / self.first_token_samples)
    }

    pub fn average_duration_ms(&self) -> Option<u64> {
        (self.duration_samples > 0).then(|| self.duration_ms / self.duration_samples)
    }

    /// Output tokens per second over the measured requests.
    /// 已测量请求的每秒输出 token 数。
    pub fn average_speed(&self) -> Option<f32> {
        (self.measured_output_ms > 0)
            .then(|| self.measured_output as f32 * 1000.0 / self.measured_output_ms as f32)
    }

    pub fn add(&mut self, other: &UsageBucket) {
        self.requests += other.requests;
        self.unknown_usage += other.unknown_usage;
        self.tool_calls += other.tool_calls;
        self.input += other.input;
        self.uncached_input += other.uncached_input;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        self.output += other.output;
        self.reasoning += other.reasoning;
        self.first_token_ms += other.first_token_ms;
        self.first_token_samples += other.first_token_samples;
        self.duration_ms += other.duration_ms;
        self.duration_samples += other.duration_samples;
        self.measured_output += other.measured_output;
        self.measured_output_ms += other.measured_output_ms;
    }
}

/// Lifetime usage of a conversation: the main session plus every subagent it created.
/// 会话的生命周期用量：主会话加上它创建的每个子代理。
#[derive(Debug, Clone, Default)]
pub struct LifetimeUsage {
    pub main: UsageBucket,
    /// `(agent name, usage)` per subagent.
    /// 每个子代理的 `(代理名, 用量)`。
    pub children: Vec<(String, UsageBucket)>,
    /// Some history predates usage recording.
    /// 部分历史早于用量记录。
    pub history_incomplete: bool,
}

impl LifetimeUsage {
    pub fn children_total(&self) -> UsageBucket {
        let mut total = UsageBucket::default();
        for (_, bucket) in &self.children {
            total.add(bucket);
        }
        total
    }
}

/// What occupies the context window.
/// 上下文窗口的组成部分。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextPart {
    SystemPrompt,
    Tools,
    Memory,
    Conversation,
    Attachments,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextSlice {
    pub part: ContextPart,
    pub tokens: u64,
}

/// One memory item injected into this conversation's context.
/// 注入本会话上下文的一条记忆。
#[derive(Debug, Clone, PartialEq)]
pub struct InjectedMemory {
    pub memory_id: String,
    pub content: String,
    /// Turn that produced the memory.
    /// 产生该记忆的轮次。
    pub turn_id: String,
    /// Turns left before the dynamic injection expires; 0 once expired.
    /// 动态注入到期前剩余的轮次；到期后为 0。
    pub turns_left: u32,
    pub score: f32,
    pub source: String,
    /// The source turn's exchange `(user, assistant)`, when recorded.
    /// 来源轮次的对话 `(用户, 助手)`（如有记录）。
    pub dialogue: Option<(String, String)>,
}

/// A group of dynamic memories sharing one recall tag.
/// 共享同一召回标签的一组动态记忆。
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryCard {
    pub tag: String,
    pub description: String,
    pub items: Vec<InjectedMemory>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSource {
    SystemSeed,
    TurnExtract,
    ManualInstruction,
}

/// One node of the static profile (user or project).
/// 静态画像（用户或项目）的一个节点。
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileNode {
    pub id: String,
    pub date: String,
    pub priority: u8,
    pub level: u8,
    pub weight: f32,
    pub content: String,
    pub source: ProfileSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileSection {
    /// User profile when true, project profile otherwise.
    /// 为真时是用户画像，否则是项目画像。
    pub user: bool,
    pub name: String,
    pub nodes: Vec<ProfileNode>,
}

/// Memory context of a conversation: dynamic TTL injections and the static profile.
/// 会话的记忆上下文：动态 TTL 注入与静态画像。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContextInjection {
    pub dynamic: Vec<MemoryCard>,
    /// Injections past their TTL, removed at the next context compaction.
    /// 已超过 TTL 的注入，将在下次上下文压缩时移除。
    pub expired: Vec<InjectedMemory>,
    pub profile: Vec<ProfileSection>,
}

impl ContextInjection {
    pub fn dynamic_count(&self) -> usize {
        self.dynamic.iter().map(|c| c.items.len()).sum::<usize>() + self.expired.len()
    }

    pub fn static_count(&self) -> usize {
        self.profile.iter().map(|s| s.nodes.len()).sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalStatus {
    Active,
    Paused,
    Blocked,
    UsageLimited,
    BudgetLimited,
    Completed,
}

impl GoalStatus {
    /// A goal that is not pursuing and not finished can be resumed.
    /// 未在推进且未完成的目标可以恢复。
    pub fn resumable(self) -> bool {
        !matches!(self, GoalStatus::Active | GoalStatus::Completed)
    }
}

/// The conversation goal the agent keeps pursuing across turns.
/// 代理跨轮次持续追求的会话目标。
#[derive(Debug, Clone, PartialEq)]
pub struct Goal {
    pub objective: String,
    pub status: GoalStatus,
    pub elapsed_ms: u64,
    pub usage: UsageBucket,
    pub budget: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionInsight {
    pub memory: ContextInjection,
    pub lifetime: LifetimeUsage,
    pub breakdown: Vec<ContextSlice>,
    pub goal: Option<Goal>,
}
