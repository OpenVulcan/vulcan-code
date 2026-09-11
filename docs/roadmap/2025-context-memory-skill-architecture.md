# Vulcan Code 上下文管理 × MemoryMesh × Skill Hub × LuaSkills Hub 联动架构规划

> 版本：v1.0  
> 日期：2025-05-20  
> 范围：vulcan-core 下沉 + 跨会话记忆 + 提示词中心 + 技能生态  

---

## 一、架构总览：四模块联动关系

```
+-----------------------------------------------------------------------------+
|                              用户交互层 (GUI)                                 |
|  +--------------+  +--------------+  +--------------+  +------------------+  |
|  |  会话视图     |  |  Skill 市场   |  |  记忆浏览器   |  |  上下文预算仪表盘 |  |
|  +------+-------+  +------+-------+  +------+-------+  +--------+---------+  |
+--------+------------------+-------------------+-------------------+----------+
         |                  |                   |                   |
         v                  v                   v                   v
+-----------------------------------------------------------------------------+
|                           编排协调层 (vulcan-core)                            |
|                                                                             |
|  +---------------------------------------------------------------------+    |
|  |                      Context Manager (新增)                          |    |
|  |  +----------+ +----------+ +----------+ +----------+ +----------+  |    |
|  |  |  Budget  | |  Prune   | | Compact  | | Reactive | |  Cache   |  |    |
|  |  |  预算    | |  轻清理  | | 全量压缩 | | 被动兜底 | |  保护    |  |    |
|  |  +----------+ +----------+ +----------+ +----------+ +----------+  |    |
|  +---------------------------------------------------------------------+    |
|                                                                             |
|  +---------------------------------------------------------------------+    |
|  |                   MemoryMesh Coordinator (增强)                      |    |
|  |  +----------+ +----------+ +----------+ +----------+              |    |
|  |  |  Recall  | |  Extract | |  Inject  | |  Cross-  |              |    |
|  |  |  召回    | |  提取    | |  注入    | |  Project |              |    |
|  |  +----------+ +----------+ +----------+ +----------+              |    |
|  +---------------------------------------------------------------------+    |
|                                                                             |
|  +---------------------------------------------------------------------+    |
|  |                    Prompt Skill Hub (新增)                           |    |
|  |  +----------+ +----------+ +----------+                          |    |
|  |  | Template | | Variable | |  Budget  |                          |    |
|  |  | 模板引擎 | | 替换     | |  感知    |                          |    |
|  |  +----------+ +----------+ +----------+                          |    |
|  +---------------------------------------------------------------------+    |
|                                                                             |
|  +---------------------------------------------------------------------+    |
|  |                   LuaSkills Runtime Bridge (已有增强)                 |    |
|  |  +----------+ +----------+ +----------+ +----------+              |    |
|  |  |  Discover| |  Protect | |  Update  | |  Registry|              |    |
|  |  | 发现     | | 保护     | |  更新    | |  注册表  |              |    |
|  |  +----------+ +----------+ +----------+ +----------+              |    |
|  +---------------------------------------------------------------------+    |
+-----------------------------------------------------------------------------+
         |                  |                   |                   |
         v                  v                   v                   v
+-----------------------------------------------------------------------------+
|                           存储与运行时层                                      |
|  +--------------+  +--------------+  +--------------+  +------------------+  |
|  |  SQLite      |  |  LanceDB     |  |  File System |  |  Lua VM Pool     |  |
|  |  (会话/记录)  |  |  (向量记忆)   |  |  (Skill/Prompt) |  |  (LuaSkills)     |  |
|  +--------------+  +--------------+  +--------------+  +------------------+  |
+-----------------------------------------------------------------------------+
```

**核心设计原则**：
1. **Context Manager 是中枢**：所有模块的上下文消耗必须通过 Budget 系统登记
2. **MemoryMesh 是旁观者也是参与者**：压缩时提取记忆，启动时注入记忆，但不直接修改消息流
3. **Skill Hub 是上下文消费者**：每个激活的 Skill 都会占用 system prompt token，需要被预算管理
4. **LuaSkills 是受保护者**：Skill 工具结果在 Prune 时享受白名单保护

---

## 二、模块一：Context Manager（上下文管理专业处理）

### 2.1 架构下沉：从 GUI 到 Core

**现状问题**：`context_budget.rs` 和 `conversation_protocol.rs` 在 `apps/vulcan-code-gui`，`vulcan-core` 的 `Controller` 无法自主触发压缩。

**目标**：在 `crates/vulcan-core/src/context/` 建立完整的上下文管理体系，GUI 层仅保留 UI 投影和策略配置。

### 2.2 目录结构

```
crates/vulcan-core/src/
├── context/
│   ├── mod.rs                    # 模块入口，导出 public API
│   ├── budget.rs                 # Token 预算计算（从 GUI 迁移并增强）
│   ├── estimate.rs               # Token 估算（文本/JSON/附件/工具）
│   ├── prune.rs                  # 轻量级工具输出清理（新增）
│   ├── compact.rs                # 全量摘要压缩（从 conversation_protocol 迁移）
│   ├── reactive.rs               # 被动兜底：API 返回 overflow 时的恢复
│   ├── tool_budget.rs            # 消息级工具结果预算（新增）
│   ├── cache_guard.rs            # Prompt Cache 保护（Anthropic 场景）
│   └── policy.rs                 # ContextCompactionPolicy 定义
├── types.rs                      # 增强：ConversationSnapshot 加入 context state
└── controller.rs                 # 增强：run_user_turn 中插入预算检查
```

### 2.3 核心数据结构

```rust
// crates/vulcan-core/src/context/policy.rs

/// 上下文压缩策略（vulcan-core 内部使用版本）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCompactionPolicy {
    /// 是否允许自动压缩
    pub auto: bool,
    /// 是否启用轻量级 prune
    pub prune: bool,
    /// 全量压缩后保留的尾部用户轮次数
    pub tail_turns: usize,
    /// 尾部保留的最大 token 预算
    pub preserve_recent_tokens: usize,
    /// 为压缩摘要和下一轮用户输入预留的 token
    pub reserved: usize,
    /// 轻量 prune 的保护区 token（默认 40K）
    pub prune_protect_tokens: usize,
    /// 轻量 prune 的最小释放量（默认 20K）
    pub prune_minimum_tokens: usize,
    /// 单条消息内工具结果聚合预算（字符数，默认 100K）
    pub tool_result_budget_chars: usize,
    /// 单条工具结果持久化阈值（字符数，默认 50K）
    pub tool_result_persist_threshold_chars: usize,
    /// 连续压缩失败熔断阈值
    pub max_consecutive_failures: usize,
    /// 受保护的工具名列表（这些工具的结果永不 prune）
    pub protected_tools: HashSet<String>,
}

impl Default for ContextCompactionPolicy {
    fn default() -> Self {
        Self {
            auto: true,
            prune: true,
            tail_turns: 4,
            preserve_recent_tokens: 8_192,
            reserved: 2_048,
            prune_protect_tokens: 40_000,
            prune_minimum_tokens: 20_000,
            tool_result_budget_chars: 100_000,
            tool_result_persist_threshold_chars: 50_000,
            max_consecutive_failures: 3,
            protected_tools: ["skill", "memorymesh.recall"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}
```

```rust
// crates/vulcan-core/src/context/mod.rs

/// 上下文管理器的完整状态，挂载在 ConversationSnapshot 上
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextManagerState {
    /// 轻量 prune 的累积状态
    pub prune_state: PruneState,
    /// 工具结果预算的跨轮次冻结状态（保护 Prompt Cache）
    pub tool_budget_state: ToolResultBudgetState,
    /// 压缩熔断追踪
    pub compaction_tracking: CompactionTracking,
    /// 最近一次预算快照
    pub last_budget: Option<ContextBudgetSnapshot>,
    /// 会话启动时间（用于 time-based microcompact）
    pub session_started_at: Option<u64>,
}

/// 轻量 prune 状态
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PruneState {
    /// 已被标记为 pruned 的 message id 集合
    pub pruned_ids: HashSet<String>,
    /// 累计已释放的 token 数
    pub tokens_freed: usize,
}

/// 工具结果预算的跨轮次冻结状态
/// 设计意图：一旦决定对某工具结果进行替换/保留，后续轮次永不改变，
/// 以保持 Prompt Cache 前缀的字节级一致性。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolResultBudgetState {
    /// 已见过的 tool_result_id（决策已冻结）
    pub seen_ids: HashSet<String>,
    /// 已替换为预览的 tool_result_id -> 预览文本缓存
    /// 后续轮次直接从 Map 读取，零 I/O，字节级一致
    pub replacements: HashMap<String, String>,
    /// 已持久化到磁盘的 tool_result_id -> 文件路径
    pub persisted_paths: HashMap<String, String>,
}

/// 压缩熔断追踪
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompactionTracking {
    /// 连续失败次数
    pub consecutive_failures: usize,
    /// 最近一次压缩的回合数
    pub last_compact_turn: usize,
    /// 最近一次压缩的 token 数
    pub last_compact_tokens: usize,
}
```

### 2.4 ConversationSnapshot 增强

```rust
// crates/vulcan-core/src/types.rs

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationSnapshot {
    pub messages: Vec<ConversationMessage>,
    pub records: Vec<ConversationRecord>,
    pub continuation: ProviderContinuation,
    /// 【新增】上下文管理状态
    #[serde(default)]
    pub context_state: ContextManagerState,
}
```

### 2.5 Controller 集成点

在 `VulcanCoreController::run_user_turn` 的 loop 中，每次 `execute_provider_turn` 前插入：

```rust
// crates/vulcan-core/src/controller.rs

loop {
    // ===== 新增：上下文预算检查与自动压缩 =====
    if let Some(action) = self.check_and_apply_compaction()? {
        match action {
            CompactionAction::Pruned(count) => {
                self.push_event(CoreEvent {
                    kind: CoreEventKind::ContextPruned,
                    title: "上下文清理".into(),
                    body: format!("已清除 {} 条旧工具输出以释放空间", count),
                    metadata: None,
                });
            }
            CompactionAction::Compacted { pre_tokens, post_tokens } => {
                self.push_event(CoreEvent {
                    kind: CoreEventKind::ContextCompacted,
                    title: "上下文压缩".into(),
                    body: format!("历史已摘要化：{} -> {} tokens", pre_tokens, post_tokens),
                    metadata: None,
                });
                // 通知 MemoryMesh 提取压缩前的记忆线索
                self.memory_mesh_notify_compaction(pre_tokens, post_tokens);
            }
            CompactionAction::Blocked => {
                return Err(CoreError::InvalidState {
                    message: "上下文已超出模型处理能力，请开启新会话或手动清理".into(),
                });
            }
            _ => {}
        }
    }
    // ==========================================

    let response = self.execute_provider_turn(adapter).await?;
    // ...
}
```

### 2.6 六大子系统详细设计

#### 2.6.1 Budget（预算）

从 `apps/vulcan-code-gui/src/session/context_budget.rs` 迁移核心逻辑到 `vulcan-core/src/context/budget.rs`，保留 `ContextBudgetSnapshot` 和 `ContextBudgetSeverity`。

**增强点**：
- 增加 `estimated_skill_tokens`：当前激活的 Skill 占用的 token
- 增加 `estimated_memory_mesh_tokens`：注入的记忆占用的 token
- `should_compact` 逻辑考虑 `prune` 是否已启用

#### 2.6.2 Prune（轻量级清理）

**设计参考**：OpenCode 的 `prune` + Claude Code 的 `microcompact`。

```rust
// crates/vulcan-core/src/context/prune.rs

/// 轻量级工具输出清理。
/// 成本：纯本地操作，不调用 LLM，O(n) 遍历消息。
/// 触发：每次 Provider 回合前，当预估 token 超过阈值时优先尝试。
pub fn prune_old_tool_outputs(
    messages: &mut [ConversationMessage],
    state: &mut PruneState,
    policy: &ContextCompactionPolicy,
) -> usize {
    let mut total_tool_tokens = 0usize;
    let mut pruned_count = 0usize;
    let mut tokens_freed_this_turn = 0usize;

    // 倒序遍历：最新消息在前
    for message in messages.iter_mut().rev() {
        let ConversationPayload::ToolResult { ref mut text, tool_result_id, .. } = message.payload else {
            continue;
        };

        // 跳过已清理的
        if state.pruned_ids.contains(tool_result_id) {
            continue;
        }

        let tool_tokens = estimate_text_tokens(text);
        total_tool_tokens += tool_tokens;

        // 保护区逻辑：累计未超过保护区的工具结果保留
        if total_tool_tokens <= policy.prune_protect_tokens {
            continue;
        }

        // 白名单检查
        if is_protected_tool(message, &policy.protected_tools) {
            continue;
        }

        // 执行清理
        let original_len = text.len();
        *text = "[Old tool result content cleared]".to_string();
        state.pruned_ids.insert(tool_result_id.clone());
        tokens_freed_this_turn += original_len;
        pruned_count += 1;
    }

    // 只有实际释放量超过最小阈值才生效
    if tokens_freed_this_turn >= policy.prune_minimum_tokens {
        state.tokens_freed += tokens_freed_this_turn;
        pruned_count
    } else {
        // 回滚本次清理
        0
    }
}
```

#### 2.6.3 Compact（全量压缩）

从 `conversation_protocol.rs` 迁移 `compact_conversation_with_summary` 和 `canonical_recent_tail_messages`。

**增强点**：
- 压缩前调用 `strip_images_from_messages`（类似 Claude Code）
- 压缩后重置 `continuation.last_provider_message_id`（已有）
- 压缩后重置 `tool_budget_state` 的 baseline（因为旧消息被替换为 summary）
- 失败时递增 `compaction_tracking.consecutive_failures`

#### 2.6.4 Reactive（被动兜底）

```rust
// crates/vulcan-core/src/context/reactive.rs

/// 当 API 返回上下文溢出错误时的被动恢复。
/// 策略：从消息头部剥离最旧的一个完整 user->assistant->tool round，然后重试。
pub fn reactive_compact_oldest_round(
    messages: &mut Vec<ConversationMessage>,
) -> Option<ReactiveCompactResult> {
    let segments = canonical_tail_segments(messages);
    if segments.len() <= 1 {
        return None; // 无法继续剥离
    }

    // 剥离最旧的 segment
    let oldest = segments.first()?;
    let removed: Vec<_> = messages.drain(oldest.start..oldest.end).collect();
    let removed_tokens: usize = removed.iter().map(estimate_message_tokens).sum();

    Some(ReactiveCompactResult {
        removed_messages: removed.len(),
        removed_tokens,
        remaining_messages: messages.len(),
    })
}
```

#### 2.6.5 Tool Budget（消息级工具结果预算）

```rust
// crates/vulcan-core/src/context/tool_budget.rs

/// 按 API-level user message 聚合控制工具结果总大小。
/// 如果一组并行工具结果的总大小超过预算，选择最大的 fresh 结果进行持久化替换。
pub async fn enforce_tool_result_budget(
    messages: &mut [ConversationMessage],
    state: &mut ToolResultBudgetState,
    policy: &ContextCompactionPolicy,
    persist_fn: &mut dyn FnMut(&str, &str) -> Result<String, String>,
) -> Result<Vec<ToolResultReplacement>, String> {
    let mut replacements = Vec::new();
    let groups = group_tool_results_by_user_message(messages);
    let budget = policy.tool_result_budget_chars;

    for group in groups {
        let total_size: usize = group.iter()
            .map(|m| match &m.payload {
                ConversationPayload::ToolResult { text, .. } => text.len(),
                _ => 0,
            })
            .sum();

        if total_size <= budget {
            for m in &group {
                if let ConversationPayload::ToolResult { tool_result_id, .. } = &m.payload {
                    state.seen_ids.insert(tool_result_id.clone());
                }
            }
            continue;
        }

        // 需要替换：选择最大的 fresh 结果
        let mut candidates: Vec<_> = group.into_iter()
            .filter(|m| {
                if let ConversationPayload::ToolResult { tool_result_id, .. } = &m.payload {
                    !state.seen_ids.contains(tool_result_id)
                } else { false }
            })
            .collect();

        candidates.sort_by_key(|m| match &m.payload {
            ConversationPayload::ToolResult { text, .. } => text.len(),
            _ => 0,
        });
        candidates.reverse();

        let mut freed = 0usize;
        for candidate in candidates {
            if total_size - freed <= budget {
                break;
            }
            if let ConversationPayload::ToolResult { text, tool_result_id, .. } = &mut candidate.payload {
                let path = persist_fn(tool_result_id, text).await?;
                let preview = format!(
                    "<persisted-output>\nOutput too large ({} chars). Full output saved to: {}\n\nPreview (first 2KB):\n{}\n</persisted-output>",
                    text.len(), path, &text[..text.len().min(2000)]
                );

                state.seen_ids.insert(tool_result_id.clone());
                state.replacements.insert(tool_result_id.clone(), preview.clone());
                state.persisted_paths.insert(tool_result_id.clone(), path);

                freed += text.len();
                replacements.push(ToolResultReplacement {
                    message_id: candidate.id.clone(),
                    original_size: text.len(),
                });

                *text = preview;
            }
        }
    }

    Ok(replacements)
}
```

**持久化目录**：`<user-vulcan-root>/code/tool-results/{session_id}/{tool_result_id}.txt`。
其中 `<user-vulcan-root>` 在 release 中为 `~/.vulcan`，在 debug 中为 `~/.vulcan-debug`。

#### 2.6.6 Cache Guard（Prompt Cache 保护）

```rust
// crates/vulcan-core/src/context/cache_guard.rs

/// Anthropic Prompt Caching 的客户端保护。
pub fn split_system_prompt_for_cache(system_prompt: &str) -> Vec<SystemPromptBlock> {
    let parts: Vec<&str> = system_prompt.splitn(2, "\n\n").collect();
    vec![
        SystemPromptBlock {
            text: parts[0].to_string(),
            cache_control: Some(CacheControl::Ephemeral),
        },
        SystemPromptBlock {
            text: parts.get(1).unwrap_or(&"").to_string(),
            cache_control: None,
        },
    ]
}
```

---

## 三、模块二：MemoryMesh（跨会话跨项目记忆体系）

### 3.1 现状分析

**已有基础设施**：
- `memory-specialist` Agent + `memory.recall` / `memory.ready` Signal
- `vulcan-workmem` LuaSkill
- SQLite + LanceDB 存储（进程内引擎）
- 项目表 `projects`、会话表 `sessions` 已存在

**缺失**：
- Rust 侧没有统一的 MemoryMesh Coordinator
- 压缩时未提取记忆线索
- 跨项目记忆检索未实现
- 记忆注入与上下文预算未联动

### 3.2 架构设计

```
+-------------------------------------------------------------+
|                    MemoryMesh Coordinator                    |
|  (crates/vulcan-core/src/memory_mesh/coordinator.rs)        |
+-------------------------------------------------------------+
|  +----------+  +----------+  +---------------------+        |
|  | Extractor|  | Injector |  | CrossProjectSearch  |        |
|  | 压缩时提取 |  | 启动时注入 |  | 跨项目语义搜索       |        |
|  +----------+  +----------+  +---------------------+        |
|  +----------+  +----------+  +---------------------+        |
|  |  Recall  |  |Consolidate|  |    BudgetAware      |        |
|  | 即时召回  |  | 定期整合   |  |    预算感知注入      |        |
|  +----------+  +----------+  +---------------------+        |
+-------------------------------------------------------------+
          |            |                  |
          v            v                  v
+-------------------------------------------------------------+
|                      Storage Layer                           |
|  +--------------+  +--------------+  +------------------+   |
|  |  SQLite      |  |  LanceDB     |  |  Session State   |   |
|  |  (结构化记忆) |  |  (向量索引)   |  |  (运行时状态)     |   |
|  +--------------+  +--------------+  +------------------+   |
+-------------------------------------------------------------+
```

### 3.3 数据模型

```rust
// crates/vulcan-core/src/memory_mesh/types.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub kind: MemoryKind,
    pub project_id: Option<String>,
    pub session_id: String,
    pub source_message_id: Option<String>,
    pub source_tool: Option<String>,
    pub created_at: u64,
    pub last_accessed_at: u64,
    pub access_count: usize,
    pub tags: Vec<String>,
    pub related_files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryKind {
    Fact,
    Preference,
    Decision,
    Issue,
    CodeStructure,
    Architecture,
    NextStep,
}

#[derive(Debug, Clone)]
pub struct MemoryRecallRequest {
    pub query: String,
    pub project_id: Option<String>,
    pub session_id: String,
    pub cross_project: bool,
    pub top_k: usize,
    pub min_relevance: f32,
}
```

### 3.4 SQLite Schema 扩展

```sql
CREATE TABLE memories (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    kind TEXT NOT NULL,
    project_id TEXT,
    session_id TEXT NOT NULL,
    source_message_id TEXT,
    source_tool TEXT,
    created_at INTEGER NOT NULL,
    last_accessed_at INTEGER NOT NULL,
    access_count INTEGER DEFAULT 0,
    tags TEXT,
    related_files TEXT,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE SET NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE TABLE memory_links (
    from_memory_id TEXT NOT NULL,
    to_memory_id TEXT NOT NULL,
    relation TEXT NOT NULL,
    strength REAL NOT NULL,
    PRIMARY KEY (from_memory_id, to_memory_id),
    FOREIGN KEY (from_memory_id) REFERENCES memories(id) ON DELETE CASCADE,
    FOREIGN KEY (to_memory_id) REFERENCES memories(id) ON DELETE CASCADE
);

CREATE TABLE memory_embeddings (
    memory_id TEXT PRIMARY KEY,
    lance_table_version INTEGER,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
);

CREATE TABLE project_memory_configs (
    project_id TEXT PRIMARY KEY,
    auto_extract_enabled INTEGER DEFAULT 1,
    auto_inject_enabled INTEGER DEFAULT 1,
    max_injected_memories INTEGER DEFAULT 5,
    max_injected_tokens INTEGER DEFAULT 2048,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);
```

### 3.5 核心流程

#### 流程 1：压缩时记忆提取

```rust
pub async fn extract_memories_from_compaction(
    &self,
    summary_text: &str,
    pre_compact_messages: &[ConversationMessage],
    session_id: &str,
    project_id: Option<&str>,
) -> Result<Vec<MemoryEntry>, String> {
    let facts = self.parse_summary_for_facts(summary_text);
    let preferences = self.extract_preferences(pre_compact_messages);
    let decisions = self.extract_decisions(pre_compact_messages);
    let issues = self.extract_issues(pre_compact_messages);

    let mut entries = Vec::new();
    for fact in facts {
        entries.push(MemoryEntry {
            id: uuid(),
            content: fact,
            kind: MemoryKind::Fact,
            project_id: project_id.map(|s| s.to_string()),
            session_id: session_id.to_string(),
            created_at: now(),
            last_accessed_at: now(),
            access_count: 0,
            tags: vec![],
            related_files: vec![],
            ..Default::default()
        });
    }

    self.store_memories_with_embeddings(&entries).await?;
    Ok(entries)
}
```

#### 流程 2：会话启动时记忆注入

```rust
pub async fn inject_relevant_memories(
    &self,
    session_id: &str,
    project_id: Option<&str>,
    current_task: &str,
    budget: &mut ContextBudgetSnapshot,
) -> Result<Vec<String>, String> {
    let request = MemoryRecallRequest {
        query: current_task.to_string(),
        project_id: project_id.map(|s| s.to_string()),
        session_id: session_id.to_string(),
        cross_project: true,
        top_k: 10,
        min_relevance: 0.7,
    };

    let recall_result = self.recall_memories(request).await?;

    let max_tokens = budget.effective_input_limit
        .map(|l| l.saturating_sub(budget.estimated_input_tokens))
        .unwrap_or(2048)
        .min(2048);

    let selected = self.select_memories_within_budget(&recall_result.memories, max_tokens);

    let injected_tokens: usize = selected.iter()
        .map(|m| estimate_text_tokens(&m.memory.content))
        .sum();
    budget.estimated_history_tokens += injected_tokens;
    budget.estimated_input_tokens += injected_tokens;

    Ok(selected.into_iter().map(|m| format_memory_for_prompt(&m.memory)).collect())
}
```

### 3.6 与现有 `vulcan-workmem` LuaSkill 的协作

| 层级 | 职责 | 实现 |
|------|------|------|
| Rust Coordinator | 生命周期钩子、预算管理、格式化、注入策略 | 新增 |
| LuaSkill (`vulcan-workmem`) | 向量嵌入、语义搜索、LanceDB 操作 | 已有 |
| SQLite | 结构化记忆存储、关系链接 | 已有 + 扩展 |

---

## 四、模块三：Prompt Skill Hub（常规提示词中心）

### 4.1 定位

区别于**工具类 Skill**，Prompt Skill Hub 管理的是**认知类 Skill**——即通过增强 system prompt 来改变 Agent 行为模式的提示词模板。

示例：
- "代码审查专家" —— 注入代码审查的思维方式和检查清单
- "测试驱动开发" —— 注入 TDD 的工作流程
- "安全审计模式" —— 注入安全审计的关注点
- "中文技术写作" —— 注入中文文档写作的规范

### 4.2 数据结构

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub template: String,
    pub variables: Vec<PromptSkillVariable>,
    pub estimated_tokens: usize,
    pub tags: Vec<String>,
    pub conflicts_with: Vec<String>,
    pub requires: Vec<String>,
    pub priority: i32,
    pub source: PromptSkillSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptSkillSource {
    BuiltIn,
    UserInstalled,
    ProjectLocal,
}
```

### 4.3 存储布局

```
<user-vulcan-root>/code/
├── prompt-skills/
│   ├── built-in/
│   │   ├── code-review/skill.yaml + template.md
│   │   ├── tdd/skill.yaml + template.md
│   │   └── security-audit/skill.yaml + template.md
│   └── user/
│       └── my-custom-style/skill.yaml + template.md
├── prompt-skill-hub.yaml

{project_root}/
├── .vulcan/
│   └── prompt-skills/project-conventions/skill.yaml + template.md
```

### 4.4 与 Context Budget 的集成

```rust
pub fn build_request_context_budget_snapshot(
    active_prompt_skills: &[ActivatedPromptSkill],
) -> ContextBudgetSnapshot {
    let estimated_skill_tokens = active_prompt_skills
        .iter()
        .map(|s| s.actual_tokens)
        .sum::<usize>();

    let estimated_system_tokens = estimate_text_tokens(&system_prompt)
        + estimated_skill_tokens;

    // ...
}
```

### 4.5 System Prompt 构建流程增强

当前 `load_default_agent_prompt()` 的流程：

```
agent-prompts/{agent_id}.txt -> system prompt
```

增强后的流程：

```
1. 加载基础 agent prompt
2. 加载 provider-specific system prompt
3. 从 Prompt Skill Hub 加载激活的 Prompt Skills
4. 按优先级排序，执行变量替换
5. 组合为最终 system prompt（2-part cache 结构）
6. 登记到 Context Budget
```

---

## 五、模块四：LuaSkills Hub（技能生态中心）

### 5.1 定位

已有 `luaskills_runtime.rs` 和 `output/skills/` 目录，但缺少一个**统一发现、管理、推荐**的 Hub 层。

### 5.2 核心增强点

#### 5.2.1 Skill 注册表（Registry）

```rust
pub struct LuaSkillsRegistry {
    pub system_skills: HashMap<String, SkillMeta>,
    pub user_skills: HashMap<String, SkillMeta>,
    pub project_skills: HashMap<String, SkillMeta>,
    pub marketplace_cache: Vec<MarketplaceSkillEntry>,
}
```

#### 5.2.2 上下文保护（与 Context Manager 联动）

```rust
impl ContextCompactionPolicy {
    pub fn with_luaskills_protection(mut self, registry: &LuaSkillsRegistry) -> Self {
        for (_, meta) in registry.active_skills() {
            for entry in &meta.entries {
                self.protected_tools.insert(entry.name.clone());
            }
        }
        self
    }
}
```

#### 5.2.3 智能推荐

```rust
pub async fn recommend_skills_for_task(
    &self,
    task_description: &str,
    active_skills: &[String],
) -> Vec<SkillRecommendation> {
    let keyword_matches = self.match_by_keywords(task_description);
    let historical_matches = self.match_by_history(task_description).await;
    let stack_matches = self.match_by_tech_stack().await;
    // 合并排序...
}
```

#### 5.2.4 市场索引格式

```yaml
skills:
  - id: vulcan-codekit
    name: Vulcan CodeKit
    description: 代码分析与导航工具集
    author: vulcan-team
    source: github:vulcan-ai/vulcan-codekit
    latest_version: 1.2.3
    tags: [code, analysis, navigation]
```

---

## 六、四模块联动数据流

```
[会话创建]
    |
    v
[MemoryMesh] ----> 召回跨会话/跨项目记忆
[Coordinator]      (注入到 system prompt 或首条 user message)
    |
    v
[Prompt Skill] ---> 加载激活的 Prompt Skills
[Hub]              (合并到 system prompt)
    |
    v
[Context Budget] -> 计算总预算：system + history + skills + memory + tools
[Snapshot]         (如果超过 85% -> 标记 CompactSoon)
    |
    v
[用户输入] ---> [Controller 回合循环]
                    |
                    v
            [Context Manager] -> 检查预算
            (每回合前)           Healthy -> 继续
                                CompactSoon -> 先 Prune，再 Compact
                                Overflow -> Reactive Compact
                    |
                    v
            [LLM 请求]
                    |
                    v
            [工具调用]
                    |
                    v
            [Tool Budget] ----> 检查工具结果总大小
            [Enforcement]        超限 -> 持久化到磁盘 + 预览替换
                    |
                    v
            [工具结果回传]
                    |
                    v
            [下一轮循环...]
                    |
                    v
            [用户输入新任务]
                    |
                    v
            [MemoryMesh] -----> 检测到任务切换，触发 memory.recall
            [Coordinator]       (通过 Signal 路由到 memory-specialist)
                    |
                    v
            [上下文压缩时]
                    |
                    v
            [MemoryMesh] -----> 从 summary 中提取记忆，写入 SQLite + LanceDB
            [Extractor]
                    |
                    v
            [会话结束/归档]
```

---

## 七、实施路线图

### Phase 1：基础设施下沉（2 周）

| 任务 | 文件 | 优先级 |
|------|------|--------|
| 创建 `context/` 模块骨架 | `crates/vulcan-core/src/context/mod.rs` | P0 |
| 迁移 `ContextBudgetSnapshot` 到 Core | `context/budget.rs` | P0 |
| 迁移 `compact_conversation_with_summary` 到 Core | `context/compact.rs` | P0 |
| 新增 `PruneState` + `prune_old_tool_outputs` | `context/prune.rs` | P0 |
| 新增 `ToolResultBudgetState` | `context/tool_budget.rs` | P1 |
| 增强 `ConversationSnapshot` | `types.rs` | P0 |
| Controller 集成预算检查 | `controller.rs` | P0 |
| 占位文本缩短 | `conversation_protocol.rs` | P0 |

**验收标准**：
- Controller 能在每次 LLM 请求前自动检测上下文预算
- 超过 85% 时自动触发 Prune，超过阈值时触发 Compact
- GUI 层不再直接操作压缩逻辑

### Phase 2：MemoryMesh 硬化（2 周）

| 任务 | 文件 | 优先级 |
|------|------|--------|
| SQLite Schema 扩展 | migration | P0 |
| `MemoryMeshCoordinator` 骨架 | `memory_mesh/coordinator.rs` | P0 |
| `MemoryStorage` SQLite 实现 | `memory_mesh/storage.rs` | P0 |
| 压缩时记忆提取 | `memory_mesh/extractor.rs` | P0 |
| 会话启动时记忆注入 | `memory_mesh/injector.rs` | P0 |
| LanceDB 向量集成 | `memory_mesh/vector_store.rs` | P1 |
| 跨项目搜索 | `memory_mesh/cross_project.rs` | P1 |

**验收标准**：
- 压缩后能自动提取 3-5 条关键记忆写入存储
- 新会话启动时自动注入相关记忆（不超过 2048 tokens）

### Phase 3：Prompt Skill Hub（1.5 周）

| 任务 | 文件 | 优先级 |
|------|------|--------|
| `prompt_skill_hub/` 模块骨架 | `prompt_skill_hub/mod.rs` | P0 |
| Prompt Skill 注册表 | `prompt_skill_hub/registry.rs` | P0 |
| 极简模板引擎 | `prompt_skill_hub/template.rs` | P0 |
| 目录布局初始化 | `<user-vulcan-root>/code/prompt-skills/` | P0 |
| 5 个内置 Prompt Skill | `built-in/` | P1 |
| System Prompt 构建流程增强 | `agent_runtime/config.rs` | P0 |
| 与 Budget 集成 | `context/budget.rs` | P0 |

### Phase 4：LuaSkills Hub 增强（1.5 周）

| 任务 | 文件 | 优先级 |
|------|------|--------|
| `LuaSkillsRegistry` 统一索引 | `luaskills_hub/registry.rs` | P0 |
| 上下文保护自动同步 | `context/policy.rs` | P0 |
| Skill 智能推荐 | `luaskills_hub/recommendation.rs` | P1 |
| 自动更新检查 | `luaskills_hub/auto_update.rs` | P1 |
| 市场索引格式定义 | `marketplace-index.yaml` | P1 |

### Phase 5：Prompt Cache 优化（1 周，可并行）

| 任务 | 文件 | 优先级 |
|------|------|--------|
| System Prompt 2-part 拆分 | `providers/anthropic/request_builder.rs` | P0 |
| `cache_control` 注入 | `providers/anthropic/` | P0 |
| Tool 描述分段 | `providers/anthropic/` | P1 |

---

## 八、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 迁移 `conversation_protocol.rs` 到 Core 引入回归 | 高 | 保留原有测试，新增集成测试，渐进式迁移 |
| MemoryMesh 向量搜索延迟 | 中 | 异步召回，首回合先注入文本记忆，向量搜索后台完成 |
| Prompt Skill 过多导致 system prompt 膨胀 | 中 | 严格的 `max_skill_tokens` 限制 + 优先级截断 |
| LuaSkills 保护列表过大影响 Prune 效果 | 低 | 只保护当前会话实际使用过的 Skill 工具 |
| 向下兼容（旧会话恢复） | 高 | `ContextManagerState` 使用 `#[serde(default)]`，旧数据无感升级 |

---

## 九、与 OpenCode / Claude Code 的差异化策略

| 维度 | OpenCode | Claude Code | vulcan-code（目标） |
|------|----------|-------------|---------------------|
| 上下文压缩 | 2 层，Prune + Compact | 6 层，极致工程化 | **Prune + Compact + Reactive，Rust 高性能** |
| 记忆 | WorkMem（task-scoped） | Session Memory（后台 Agent） | **MemoryMesh（跨项目 + Signal 编排 + 向量）** |
| 提示词扩展 | Plugin hooks | 无（封闭） | **Prompt Skill Hub（模板化 + 预算感知）** |
| 工具生态 | Native + MCP + Plugin | 内置 + MCP | **Native + LuaSkills + MCP + AI Fallback（四层 + Hub）** |
| 运行环境 | CLI | CLI | **原生桌面（iced）+ Rust 性能** |

**一句话定位**：
> vulcan-code 不做"比 Claude Code 更重的上下文管理"，而是做"**在桌面端原生体验中，将上下文管理、跨项目记忆、提示词增强、技能生态四者无缝融合**"的差异化方案。
