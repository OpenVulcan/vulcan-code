// Mock system areas: settings, providers, memory, knowledge, skills, agents and the code index.
// 模拟系统区域：设置、供应商、记忆、知识库、技能、代理与代码索引。
//
// Credentials are masked at the source: the mock never holds a secret, so nothing can leak into
// the UI. Long operations (service start/stop, reindex, search) run on timers so the pages show the
// same transitions and progress the service reports.
// 凭据在源头即已打码：模拟后端从不持有密钥，界面无从泄露。长操作（服务启停、重建索引、搜索）
// 由定时器驱动，使页面呈现与服务一致的状态过渡与进度。

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::{Timer, World};
use crate::backend::{BackendEvent, Notice};
use crate::model::*;

/// Delayed system work scheduled on the mock world's timer list.
/// 挂在模拟世界定时器列表上的延迟系统任务。
pub enum SystemTimer {
    ServiceSettled(ServiceStatus),
    MemorySearch(String),
    KnowledgeSearch(String),
    CredentialTested { provider: String, id: String },
    McpRestarted(String),
    RouteTested(String),
    JobTick(JobTarget),
}

#[derive(Clone)]
pub enum JobTarget {
    Knowledge(String),
    Index(String),
    Memory(String),
    Lua(String),
}

const JOB_STEP_MS: u64 = 350;

/// Keeps only a masked tail of a secret; the secret itself is dropped by the caller.
/// 只保留密钥打码后的尾部；密钥本身由调用方丢弃。
fn mask(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    let tail: String = chars[chars.len().saturating_sub(4)..].iter().collect();
    format!("••••••••{tail}")
}

/// Setting keys whose values are secrets and are stored masked.
/// 值为密钥、以打码形式保存的设置键。
pub const SECRET_SETTINGS: [&str; 2] =
    ["memory.advanced.apiKey", "memory.advanced.managementToken"];

pub fn snapshot() -> SystemSnapshot {
    let mut settings = BTreeMap::new();
    let mut set = |key: &str, value: SettingValue| {
        settings.insert(key.to_string(), value);
    };
    set("context.compressionThreshold", SettingValue::Number(80));
    set("context.maxOutputTokens", SettingValue::Number(32_000));
    set("context.autoCompact", SettingValue::Bool(true));
    set(
        "context.systemPromptExtra",
        SettingValue::Text(String::new()),
    );
    set("notifications.enabled", SettingValue::Bool(true));
    set("notifications.completed", SettingValue::Bool(true));
    set("notifications.failed", SettingValue::Bool(true));
    set("notifications.showTitle", SettingValue::Bool(false));
    set("gitCommit.autoSummary", SettingValue::Bool(true));
    set(
        "gitCommit.summaryModel",
        SettingValue::Choice(String::new()),
    );
    set(
        "gitCommit.summaryExtraPrompt",
        SettingValue::Text("Use Conventional Commits.".into()),
    );
    set(
        "diagnostics.providerRequestTrace",
        SettingValue::Bool(false),
    );
    memory_settings(&mut set);
    lua_settings(&mut set);
    engine_settings(&mut set);

    let credential = |id: &str, label: &str, masked: &str, is_default: bool| Credential {
        id: id.into(),
        label: label.into(),
        masked: masked.into(),
        is_default,
        last_test: None,
    };
    let provider =
        |id: &str, name: &str, category: &str, protocols: &[&str], credentials| Provider {
            id: id.into(),
            name: name.into(),
            category: category.into(),
            protocols: protocols.iter().map(|p| p.to_string()).collect(),
            endpoints: protocols.len() as u32,
            credentials,
        };
    let providers = vec![
        provider(
            "anthropic",
            "Anthropic",
            "official",
            &["anthropic-messages"],
            vec![
                credential("cred-a1", "Work", "sk-ant-••••••••3f9a", true),
                credential("cred-a2", "Personal", "sk-ant-••••••••c21e", false),
            ],
        ),
        provider(
            "openai",
            "OpenAI",
            "official",
            &["openai-responses", "openai-chat"],
            vec![credential(
                "cred-o1",
                "Default",
                "sk-proj-••••••••7d0b",
                true,
            )],
        ),
        provider(
            "minimax",
            "MiniMax",
            "official",
            &["openai-chat"],
            vec![credential("cred-m1", "Default", "eyJ••••••••Qx8", true)],
        ),
        provider(
            "openrouter",
            "OpenRouter",
            "relay",
            &["openai-chat"],
            Vec::new(),
        ),
    ];

    let model =
        |id: &str, label: &str, provider: &str, window: u64, conversation, internal| ModelEntry {
            id: id.into(),
            label: label.into(),
            provider: provider.into(),
            context_window: window,
            conversation_enabled: conversation,
            internal_enabled: internal,
        };
    let models = vec![
        model(
            "claude-opus-5-5",
            "Claude Opus 5.5",
            "Anthropic",
            400_000,
            true,
            true,
        ),
        model(
            "claude-sonnet-5-5",
            "Claude Sonnet 5.5",
            "Anthropic",
            400_000,
            true,
            true,
        ),
        model(
            "claude-haiku-4-5",
            "Claude Haiku 4.5",
            "Anthropic",
            200_000,
            false,
            true,
        ),
        model("gpt-5.6", "GPT-5.6", "OpenAI", 272_000, true, true),
        model(
            "gpt-5.6-mini",
            "GPT-5.6 mini",
            "OpenAI",
            272_000,
            false,
            true,
        ),
        model(
            "minimax-m3.1-flash-preview",
            "MiniMax M3.1 Flash Preview",
            "MiniMax",
            200_000,
            true,
            false,
        ),
    ];

    let capability = |key: &str, model: Option<&str>, candidates: &[&str]| Capability {
        key: key.into(),
        enabled: model.is_some(),
        model: model.map(Into::into),
        candidates: candidates.iter().map(|c| c.to_string()).collect(),
    };
    let text_models = ["claude-sonnet-5-5", "gpt-5.6", "gpt-5.6-mini"];
    let vision_models = ["claude-opus-5-5", "claude-sonnet-5-5", "gpt-5.6"];
    let capabilities = vec![
        capability("aiWebSearch", Some("gpt-5.6"), &text_models),
        capability("aiWebExtract", Some("gpt-5.6-mini"), &text_models),
        capability(
            "aiUnderstandImage",
            Some("claude-sonnet-5-5"),
            &vision_models,
        ),
        capability("aiUnderstandVideo", None, &["gpt-5.6"]),
        capability("aiUnderstandAudio", None, &["gpt-5.6"]),
        capability("aiImageGeneration", Some("gpt-image-2"), &["gpt-image-2"]),
        capability("aiVideoGeneration", None, &["sora-2"]),
        capability("aiSpeechSynthesis", None, &["gpt-5.6-mini-tts"]),
        capability("aiMusicGeneration", None, &["music-2.0"]),
        capability("aiMusicCover", None, &["music-2.0"]),
    ];

    let mcp = |id: &str, name: &str, transport: &str, status, tools, enabled| McpServer {
        id: id.into(),
        name: name.into(),
        transport: transport.into(),
        status,
        tools,
        enabled,
    };
    let mcp_servers = vec![
        mcp(
            "mcp-fs",
            "filesystem",
            "stdio",
            ServiceStatus::Running,
            11,
            true,
        ),
        mcp("mcp-gh", "github", "http", ServiceStatus::Running, 26, true),
        mcp(
            "mcp-pw",
            "playwright",
            "stdio",
            ServiceStatus::Failed,
            0,
            true,
        ),
        mcp(
            "mcp-pg",
            "postgres",
            "stdio",
            ServiceStatus::Stopped,
            0,
            false,
        ),
    ];

    let archived = vec![ArchivedConversation {
        id: "c-old-release".into(),
        title: "0.9 發版檢查清單".into(),
        project: "vulcan-code".into(),
        archived_label: "2026-09-12".into(),
    }];

    let check = |label: &str, severity, detail: &str| StartupCheck {
        label: label.into(),
        severity,
        detail: detail.into(),
    };
    let startup = vec![
        check("Service kernel", Severity::Info, "ready in 412 ms"),
        check(
            "Provider manifest",
            Severity::Info,
            "signature verified · 2026.09.27",
        ),
        check("LuaSkills runtime", Severity::Info, "14 skills loaded"),
        check("Memory service (VMM)", Severity::Info, "running · embedded"),
        check(
            "Code semantic index",
            Severity::Warning,
            "luaskills pending rebuild",
        ),
        check(
            "MCP · playwright",
            Severity::Error,
            "spawn failed: executable not found",
        ),
    ];
    let logs = [
        "09:41:02 INFO  service kernel listening on 127.0.0.1:17921",
        "09:41:02 INFO  provider manifest verified (7 providers)",
        "09:41:03 INFO  vmm: embedded deployment ready",
        "09:41:03 WARN  context-engine: luaskills index is stale (42 pending)",
        "09:41:04 ERROR mcp[playwright]: spawn failed: executable not found",
        "09:42:17 INFO  conversation c-tui: turn t1 completed in 42.0s",
    ]
    .iter()
    .map(|l| l.to_string())
    .collect();

    let item = |id: &str, content: &str, project: &str, source: &str| MemoryItem {
        id: id.into(),
        content: content.into(),
        project: project.into(),
        source: source.into(),
        score: None,
    };
    let memory = MemoryState {
        status: ServiceStatus::Running,
        deployment: "embedded · local".into(),
        health: Severity::Info,
        pending_writebacks: 2,
        completed_turns: 148,
        last_profile_update: "2026-09-29 18:42".into(),
        items: vec![
            item(
                "m1",
                "使用者偏好以繁體中文回報，程式註解採英中雙語。",
                "global",
                "profile",
            ),
            item(
                "m2",
                "vulcan-code 的 Cargo 並發上限為 4，測試需限制執行緒。",
                "vulcan-code",
                "turn",
            ),
            item(
                "m3",
                "TUI 專案位於 D:\\openvulcan\\vulcan-code-tui，獨立 crate。",
                "vulcan-code-tui",
                "turn",
            ),
            item(
                "m4",
                "不得直接編輯 output/，資源變更後執行 make.ps1 sync。",
                "vulcan-code",
                "turn",
            ),
            item(
                "m5",
                "Ask 面板為會話內非模態面板，不得劫持鍵盤。",
                "vulcan-code",
                "turn",
            ),
            item("m6", "提交前需要使用者明確同意。", "global", "profile"),
            item(
                "m7",
                "LuaSkills 版本以根 Cargo.toml 為準。",
                "luaskills",
                "turn",
            ),
        ],
        search: None,
        searching: false,
        profile: vec![
            "語言：繁體中文（台灣），程式與 AI 文案用英文".into(),
            "角色：Vulcan Code 核心維護者".into(),
            "偏好：先確認架構再實作；不自動提交".into(),
            "工具：Rust、PowerShell、Windows 11".into(),
        ],
        projects: vec![
            memory_project("mp-core", "vulcan-code", 64, true, false),
            memory_project("mp-tui", "vulcan-code-tui", 21, false, false),
            memory_project("mp-lua", "luaskills", 17, false, false),
            memory_project("mp-global", "global", 9, false, true),
        ],
        resources: memory_resources(),
        diagnostics: [
            ("runtime", "running · pid 18244"),
            ("backend", "native"),
            ("grpc", "127.0.0.1:17933"),
            ("controller", "vldb 0.8 · lancedb 0.21"),
            ("data", "~/.vulcan/code/vmm"),
            ("log", "~/.vulcan/code/logs/vmm.log"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect(),
        jobs: Vec::new(),
    };

    let knowledge = knowledge_bases();

    let skill = |id: &str, name: &str, source: &str, description: &str, enabled| Skill {
        id: id.into(),
        name: name.into(),
        source: source.into(),
        description: description.into(),
        enabled,
        overridden: false,
        plugin: None,
        issue: None,
    };
    let skills = vec![
        skill(
            "sk-review",
            "code-review",
            "system",
            "Review a diff against project rules.",
            true,
        ),
        Skill {
            overridden: true,
            ..skill(
                "sk-commit",
                "git-commit",
                "system",
                "Draft Conventional Commit messages.",
                true,
            )
        },
        skill(
            "sk-docs",
            "docs-writer",
            "system",
            "Write bilingual design documents.",
            true,
        ),
        skill(
            "sk-i18n",
            "i18n-sync",
            "user",
            "Sync locale catalogs from the WebUI.",
            true,
        ),
        Skill {
            issue: Some("SKILL.md: frontmatter is missing `description`".into()),
            ..skill("sk-bench", "bench-runner", "user", "", false)
        },
        Skill {
            plugin: Some("playwright-tools".into()),
            ..skill(
                "sk-browser",
                "browser-automation",
                "plugin",
                "Drive a real browser through Playwright MCP.",
                true,
            )
        },
    ];

    let agents = agent_profiles();

    let project =
        |id: &str, name: &str, status: &str, counts: [u32; 4], watching, pending| IndexedProject {
            project_id: id.into(),
            name: name.into(),
            status: status.into(),
            files: counts[0],
            chunks: counts[1],
            vectors: counts[1] * 2,
            symbols: counts[2],
            watching,
            pending,
            checkpoint: format!("ck-{:04}", counts[3]),
            last_update: String::new(),
            fragments: counts[3] % 47,
            job: None,
        };
    let index = vec![
        IndexedProject {
            last_update: "2026-10-01 09:12".into(),
            ..project(
                "p-tui",
                "vulcan-code-tui",
                "ready",
                [41, 612, 1_286, 192],
                true,
                0,
            )
        },
        IndexedProject {
            last_update: "2026-10-01 08:57".into(),
            ..project(
                "p-core",
                "vulcan-code",
                "ready",
                [2_904, 38_410, 71_455, 1_877],
                true,
                3,
            )
        },
        IndexedProject {
            last_update: "2026-09-22 14:03".into(),
            ..project(
                "p-lua",
                "luaskills",
                "rebuildRequired",
                [388, 4_120, 9_120, 310],
                false,
                42,
            )
        },
    ];

    SystemSnapshot {
        settings,
        providers,
        models,
        capabilities,
        mcp_servers,
        archived,
        startup,
        logs,
        memory,
        knowledge,
        knowledge_search: None,
        skills,
        agents,
        index,
        lua_skills: lua_skills(),
        plugins: plugins(),
        managed_runtimes: vec![
            check("Python 3.13", Severity::Info, "ready · workers 0 / 4"),
            check("Node.js 24", Severity::Warning, "installed during this run"),
        ],
        embedding_models: vec![
            (
                "text-embedding-3-large".into(),
                "OpenAI text-embedding-3-large".into(),
                vec![256, 1_024, 3_072],
            ),
            (
                "voyage-code-3".into(),
                "Voyage code-3".into(),
                vec![256, 512, 1_024, 2_048],
            ),
            (
                "qwen3-embedding-8b".into(),
                "Qwen3 Embedding 8B".into(),
                vec![1_024, 4_096],
            ),
        ],
        rerank_models: vec![
            ("rerank-2.5".into(), "Voyage rerank-2.5".into()),
            ("qwen3-reranker-4b".into(), "Qwen3 Reranker 4B".into()),
        ],
        route_tests: BTreeMap::new(),
        version: format!("{} (mock)", env!("CARGO_PKG_VERSION")),
    }
}

fn memory_settings(set: &mut impl FnMut(&str, SettingValue)) {
    use SettingValue::{Bool, Choice, Number, Text};
    set("memory.rules.enable", Bool(true));
    set("memory.rules.recall", Bool(true));
    set("memory.rules.organize", Bool(true));
    set("memory.rules.aiTools", Bool(false));
    set("memory.rules.recallTurns", Number(3));
    set("memory.rules.profileRefresh", Number(20));
    set("memory.rules.profileLimit", Number(40));
    set("memory.rules.scope", Choice(String::new()));
    set("memory.policy.precheckHistory", Number(4));
    set("memory.policy.precheckBudget", Number(6_000));
    set("memory.policy.recallTopK", Number(8));
    set("memory.policy.reviewLimit", Number(24));
    set("memory.policy.postactionHistory", Number(6));
    set("memory.policy.postactionBudget", Number(12_000));
    set("memory.policy.extractedLevel", Choice("working".into()));
    set("memory.ranking.hybrid", Bool(true));
    set("memory.ranking.mmr", Bool(true));
    set("memory.ranking.decay", Bool(true));
    set("memory.ranking.rerank", Number(20));
    set("memory.inference.precheckL1", Choice("gpt-5.6-mini".into()));
    set(
        "memory.inference.precheckL2",
        Choice("claude-sonnet-5-5".into()),
    );
    set(
        "memory.inference.postactionL1",
        Choice("gpt-5.6-mini".into()),
    );
    set(
        "memory.inference.postactionL2",
        Choice("claude-sonnet-5-5".into()),
    );
    set(
        "memory.inference.embedding",
        Choice("qwen3-embedding-8b".into()),
    );
    set("memory.inference.rerankEnabled", Bool(false));
    set("memory.inference.rerank", Choice(String::new()));
    set("memory.resources.promptBundle", Choice("zh-TW".into()));
    set("memory.resources.piiLocale", Choice("TW".into()));
    set("memory.resources.noiseLocale", Choice("TW".into()));
    set("memory.resources.noiseEnabled", Bool(true));
    set("memory.resources.noiseSemanticEnabled", Bool(false));
    set("memory.resources.noiseThreshold", Number(72));
    set("memory.advanced.deployment", Choice("native".into()));
    set("memory.advanced.endpoint", Text(String::new()));
    set("memory.advanced.apiKey", Text(String::new()));
    set("memory.advanced.autoStart", Bool(true));
    set("memory.advanced.postactionTimeout", Number(180));
    set("memory.advanced.failurePassThreshold", Number(3));
    set("memory.advanced.maxConnectionsPerHost", Number(4));
}

fn lua_settings(set: &mut impl FnMut(&str, SettingValue)) {
    use SettingValue::{Bool, Choice, Number, Text};
    set("lua.runlua", Bool(true));
    set("lua.runluaSession", Bool(true));
    set("lua.mainPool.min", Number(2));
    set("lua.mainPool.max", Number(8));
    set("lua.mainPool.idleTtl", Number(300));
    set("lua.runluaPool.min", Number(1));
    set("lua.runluaPool.max", Number(4));
    set("lua.runluaPool.idleTtl", Number(120));
    set("lua.cache.maxEntries", Number(512));
    set("lua.cache.defaultTtl", Number(300));
    set("lua.cache.maxTtl", Number(3_600));
    set("lua.allowDownloads", Bool(true));
    set("lua.githubBaseUrl", Text("https://github.com".into()));
    set(
        "lua.githubApiBaseUrl",
        Text("https://api.github.com".into()),
    );
    set(
        "lua.skillHubBaseUrl",
        Text("https://hub.luaskills.dev".into()),
    );
    set("lua.privateUrlInstall", Bool(false));
    set("lua.defaultEncoding", Choice(String::new()));
    set("lua.skillManagementBridge", Bool(true));
    set("lua.managedIoCompat", Bool(false));
    set("lua.managed.workerPoolMax", Number(4));
    set("lua.managed.invokeTimeout", Number(30_000));
}

fn engine_settings(set: &mut impl FnMut(&str, SettingValue)) {
    use SettingValue::{Bool, Choice, Number, Text};
    set("engine.embeddingModel", Choice("voyage-code-3".into()));
    set("engine.dimensions", Choice("1024".into()));
    set("engine.updateMode", Choice("automatic".into()));
    set("engine.minInterval", Number(30));
    set("engine.maxFileBytes", Number(1_048_576));
    set("engine.maxResults", Number(20));
    set("engine.contextBudget", Number(24_000));
    set("engine.embeddingConcurrency", Number(4));
    set("engine.queryConcurrency", Number(2));
    set("engine.queryRewrite", Bool(true));
    set("engine.rerank", Bool(false));
    set("engine.ignore", Text("target/, output/, *.min.js".into()));
}

fn memory_project(
    id: &str,
    name: &str,
    items: u32,
    default: bool,
    protected: bool,
) -> MemoryProject {
    MemoryProject {
        id: id.into(),
        name: name.into(),
        items,
        default,
        protected,
    }
}

fn memory_resources() -> Vec<MemoryResource> {
    let resource =
        |id: &str, file: &str, category: &str, purpose: &str, overridden| MemoryResource {
            id: id.into(),
            file: file.into(),
            category: category.into(),
            purpose: purpose.into(),
            overridden,
        };
    vec![
        resource(
            "r-pre1",
            "prompts/precheck_l1.md",
            "prompt",
            "precheckL1",
            false,
        ),
        resource(
            "r-pre2",
            "prompts/precheck_l2.md",
            "prompt",
            "precheckL2",
            false,
        ),
        resource(
            "r-post1",
            "prompts/postaction_l1.md",
            "prompt",
            "postactionL1",
            true,
        ),
        resource(
            "r-post2",
            "prompts/postaction_l2.md",
            "prompt",
            "postactionL2",
            false,
        ),
        resource(
            "r-profile",
            "prompts/profile_instruction.md",
            "prompt",
            "profileInstruction",
            false,
        ),
        resource("r-pii-common", "pii/common.json", "pii", "", false),
        resource("r-pii-tw", "pii/TW.json", "pii", "", true),
        resource("r-noise-common", "noise/common.json", "noise", "", false),
        resource("r-noise-tw", "noise/TW.json", "noise", "", false),
    ]
}

fn healthy(chunks: u32, revision: u32) -> IndexHealth {
    IndexHealth {
        authoritative: chunks,
        indexed: chunks,
        missing: 0,
        orphaned: 0,
        content_revision: (revision, revision),
        dictionary_revision: (3, 3),
        tokenizer: "jieba · bm25 v2".into(),
    }
}

fn knowledge_bases() -> Vec<KnowledgeBase> {
    let source = |id: &str, path: &str, documents, chunks, last: &str| KnowledgeSource {
        id: id.into(),
        path: path.into(),
        documents,
        chunks,
        last_scanned: last.into(),
    };
    let base = |id: &str, name: &str, sources: Vec<KnowledgeSource>, enabled, mode: &str| {
        let documents = sources.iter().map(|s| s.documents).sum();
        let chunks = sources.iter().map(|s| s.chunks).sum();
        KnowledgeBase {
            id: id.into(),
            name: name.into(),
            documents,
            chunks,
            enabled,
            session_default: false,
            mode: mode.into(),
            status: "ready".into(),
            sources,
            health: healthy(chunks, 12),
            job: None,
        }
    };
    let contracts = base(
        "kb-contracts",
        "Vulcan 契約文件",
        vec![
            source(
                "ks-1",
                "D:\\openvulcan\\vulcan-code\\docs\\contracts",
                31,
                1_010,
                "2026-09-30 21:04",
            ),
            source(
                "ks-2",
                "D:\\openvulcan\\vulcan-code\\docs\\plans",
                7,
                194,
                "2026-09-28 10:15",
            ),
        ],
        true,
        "display",
    );
    let mut ratatui = base(
        "kb-ratatui",
        "ratatui 0.30 docs",
        vec![source(
            "ks-3",
            "F:\\docs\\ratatui-0.30",
            212,
            6_830,
            "2026-09-20 08:51",
        )],
        true,
        "hybrid",
    );
    ratatui.status = "dirty".into();
    ratatui.health = IndexHealth {
        indexed: 6_816,
        missing: 14,
        orphaned: 2,
        content_revision: (4, 5),
        ..healthy(6_830, 5)
    };
    let notes = base(
        "kb-notes",
        "會議記錄",
        vec![source(
            "ks-4",
            "D:\\notes\\meetings",
            17,
            240,
            "2026-09-18 17:30",
        )],
        false,
        "raw",
    );
    vec![
        KnowledgeBase {
            session_default: true,
            ..contracts
        },
        ratatui,
        notes,
    ]
}

fn agent_profiles() -> Vec<AgentProfile> {
    let agent = |id: &str, name: &str, description: &str, scope: &str, model: &str| AgentProfile {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        model: model.into(),
        enabled: true,
        scope: scope.into(),
        plugin: None,
        overridden: false,
        expose: true,
        allowed_tools: 0,
        denied_tools: 0,
        skills: Vec::new(),
        file: format!("builtin://agents/{name}.md"),
    };
    vec![
        AgentProfile {
            allowed_tools: 6,
            ..agent(
                "ag-explore",
                "explorer",
                "Read-only codebase search.",
                "system",
                "",
            )
        },
        AgentProfile {
            overridden: true,
            allowed_tools: 9,
            denied_tools: 2,
            skills: vec!["code-review".into()],
            file: "~/.vulcan/code/agents/reviewer.md".into(),
            ..agent(
                "ag-review",
                "reviewer",
                "Find defects in a change set.",
                "system",
                "claude-opus-5-5",
            )
        },
        AgentProfile {
            denied_tools: 3,
            file: "~/.vulcan/code/agents/test-runner.md".into(),
            ..agent(
                "ag-test",
                "test-runner",
                "Run targeted tests and report.",
                "user",
                "gpt-5.6-mini",
            )
        },
        AgentProfile {
            enabled: false,
            expose: false,
            skills: vec!["docs-writer".into(), "i18n-sync".into()],
            file: "~/.vulcan/code/agents/doc-writer.md".into(),
            ..agent(
                "ag-docs",
                "doc-writer",
                "Keep docs in sync with code.",
                "user",
                "",
            )
        },
        AgentProfile {
            plugin: Some("playwright-tools".into()),
            allowed_tools: 22,
            skills: vec!["browser-automation".into()],
            file: "plugin://playwright-tools/agents/browser-operator.md".into(),
            ..agent(
                "ag-browser",
                "browser-operator",
                "Operate a real browser for end-to-end checks.",
                "plugin",
                "",
            )
        },
    ]
}

fn lua_skills() -> Vec<LuaSkill> {
    let skill =
        |id: &str, name: &str, version: &str, source: &str, layer: &str, description: &str| {
            LuaSkill {
                id: id.into(),
                name: name.into(),
                version: version.into(),
                source: source.into(),
                layer: layer.into(),
                description: description.into(),
                enabled: true,
                tools: Vec::new(),
                update: None,
                job: None,
            }
        };
    let tools = |list: &[(&str, ToolMode)]| {
        list.iter()
            .map(|(name, mode)| (name.to_string(), *mode))
            .collect::<Vec<_>>()
    };
    vec![
        LuaSkill {
            tools: tools(&[
                ("code_search", ToolMode::Explicit),
                ("apply_patch", ToolMode::Explicit),
                ("symbol_outline", ToolMode::Implicit),
            ]),
            update: Some("1.5.0".into()),
            ..skill(
                "ls-codekit",
                "vulcan-codekit",
                "1.4.2",
                "LuaSkills/vulcan-codekit",
                "system",
                "Code navigation, patch and search helpers.",
            )
        },
        LuaSkill {
            tools: tools(&[
                ("docx_read", ToolMode::Implicit),
                ("xlsx_write", ToolMode::Implicit),
            ]),
            ..skill(
                "ls-office",
                "office-docs",
                "0.9.0",
                "LuaSkills/office-docs",
                "user",
                "Read and write docx, xlsx and pptx files.",
            )
        },
        LuaSkill {
            enabled: false,
            tools: tools(&[("fetch_readable", ToolMode::Disabled)]),
            ..skill(
                "ls-web",
                "web-fetch-plus",
                "2.1.0",
                "https://skills.example.com/web-fetch-plus.yaml",
                "user",
                "Fetch pages with readability extraction.",
            )
        },
    ]
}

fn plugins() -> Vec<Plugin> {
    let plugin = |id: &str, name: &str, kind: &str, description: &str| Plugin {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        description: description.into(),
        enabled: true,
        status: ServiceStatus::Running,
        tools: 0,
        hooks: 0,
        sessions: 0,
        skills: 0,
        agents: 0,
        permissions: Vec::new(),
        mounts: Vec::new(),
        diagnostics: Vec::new(),
    };
    let strings = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    vec![
        Plugin {
            tools: 14,
            hooks: 2,
            sessions: 1,
            permissions: strings(&["network", "process"]),
            mounts: strings(&["Browser workspace"]),
            diagnostics: strings(&["playwright runtime ready", "1 live page projected"]),
            ..plugin(
                "pl-browser",
                "browser",
                "builtin",
                "Core browser sessions and live page projection.",
            )
        },
        Plugin {
            tools: 22,
            hooks: 1,
            skills: 1,
            agents: 1,
            permissions: strings(&["process", "filesystem:read"]),
            mounts: strings(&["Route inspector"]),
            diagnostics: strings(&["mcp: 22 tools registered"]),
            ..plugin(
                "pl-playwright",
                "playwright-tools",
                "system",
                "Playwright MCP bridge with a browser operator agent.",
            )
        },
        Plugin {
            enabled: false,
            status: ServiceStatus::Stopped,
            tools: 2,
            hooks: 3,
            permissions: strings(&["conversation:read"]),
            mounts: strings(&["Cost chart"]),
            ..plugin(
                "pl-usage",
                "usage-meter",
                "user",
                "Per-provider cost dashboard.",
            )
        },
    ]
}

fn job(id: &str, label: &str) -> Job {
    Job {
        id: id.into(),
        label: label.into(),
        progress: Some(0),
        done: false,
        failed: false,
        detail: String::new(),
    }
}

impl World {
    pub(super) fn emit_system(&self) {
        self.emit(BackendEvent::System(Box::new(self.system.clone())));
    }

    fn later(&mut self, ms: u64, timer: SystemTimer) {
        self.timers.push((
            Instant::now() + Duration::from_millis(ms),
            Timer::System(timer),
        ));
    }

    pub(super) fn handle_system(&mut self, command: SystemCommand) {
        match command {
            SystemCommand::SetSetting { key, value } => {
                let value = if SECRET_SETTINGS.contains(&key.as_str()) {
                    SettingValue::Text(mask(value.as_str()))
                } else {
                    value
                };
                self.system.settings.insert(key, value);
            }
            SystemCommand::SetField {
                kind,
                id,
                field,
                value,
            } => self.set_field(kind, &id, &field, value),
            SystemCommand::TestRoute(key) => {
                self.system
                    .route_tests
                    .insert(key.clone(), (Severity::Warning, "…".into()));
                self.later(900, SystemTimer::RouteTested(key));
            }
            SystemCommand::SaveSettings(values) => {
                self.system.settings.extend(values);
                self.emit(BackendEvent::Notice(Notice::SettingsSaved));
            }
            SystemCommand::SetUiLocale(_) => {
                // The service persists `uiLocale`; nothing else in the snapshot changes.
                // 服务持久化 `uiLocale`；快照中其他内容不变。
                return;
            }
            SystemCommand::SetEnabled { kind, id, enabled } => self.set_enabled(kind, &id, enabled),
            SystemCommand::SetRoute { kind, id, model } => match kind {
                EntityKind::Capability => {
                    if let Some(c) = self.system.capabilities.iter_mut().find(|c| c.key == id) {
                        c.enabled = model.is_some();
                        c.model = model;
                    }
                }
                EntityKind::Agent => {
                    if let Some(a) = self.system.agents.iter_mut().find(|a| a.id == id) {
                        a.model = model.unwrap_or_default();
                    }
                }
                _ => {}
            },
            SystemCommand::Credential { provider, op } => self.credential(&provider, op),
            SystemCommand::RestoreArchived(id) => self.restore_archived(&id),
            SystemCommand::DeleteArchived(id) => {
                self.system.archived.retain(|a| a.id != id);
                self.archived_rows.remove(&id);
                self.emit(BackendEvent::Notice(Notice::Deleted));
            }
            SystemCommand::RestartMcp(id) => {
                if let Some(s) = self.system.mcp_servers.iter_mut().find(|s| s.id == id) {
                    s.status = ServiceStatus::Starting;
                }
                self.later(1_200, SystemTimer::McpRestarted(id));
            }
            SystemCommand::MemoryService(op) => {
                let memory = &mut self.system.memory;
                let (now, then) = match op {
                    ServiceOp::Start => (ServiceStatus::Starting, ServiceStatus::Running),
                    ServiceOp::Stop => (ServiceStatus::Stopping, ServiceStatus::Stopped),
                    ServiceOp::Restart => (ServiceStatus::Stopping, ServiceStatus::Running),
                };
                memory.status = now;
                self.later(1_400, SystemTimer::ServiceSettled(then));
            }
            SystemCommand::MemorySearch(query) => {
                if query.trim().is_empty() {
                    self.system.memory.search = None;
                } else {
                    self.system.memory.searching = true;
                    self.later(600, SystemTimer::MemorySearch(query));
                }
            }
            SystemCommand::MemoryDelete(id) => {
                let memory = &mut self.system.memory;
                memory.items.retain(|i| i.id != id);
                if let Some((_, hits)) = &mut memory.search {
                    hits.retain(|i| i.id != id);
                }
                self.emit(BackendEvent::Notice(Notice::Deleted));
            }
            SystemCommand::MemoryCorrectProfile(text) => {
                self.system.memory.profile.push(text);
                self.system.memory.last_profile_update = "now".into();
                self.emit(BackendEvent::Notice(Notice::MemoryUpdated));
            }
            SystemCommand::MemoryMaintenance(task) => {
                let (id, label) = match task {
                    MaintenanceTask::Organize => ("mj-organize", "organize"),
                    MaintenanceTask::RefreshProfile => ("mj-profile", "profile"),
                    MaintenanceTask::Cleanup => ("mj-cleanup", "cleanup"),
                };
                let memory = &mut self.system.memory;
                memory.jobs.retain(|j| j.id != id);
                memory.jobs.insert(0, job(id, label));
                self.emit(BackendEvent::Notice(Notice::JobStarted(label.into())));
                self.later(
                    JOB_STEP_MS,
                    SystemTimer::JobTick(JobTarget::Memory(id.into())),
                );
            }
            SystemCommand::MemoryProject(op) => self.memory_project(op),
            SystemCommand::MemoryResourceRestore(id) => {
                if let Some(r) = self.system.memory.resources.iter_mut().find(|r| r.id == id) {
                    r.overridden = false;
                    let file = r.file.clone();
                    self.emit(BackendEvent::Notice(Notice::Done(file)));
                }
            }
            SystemCommand::KnowledgeRepair(id) | SystemCommand::KnowledgeReindex(id) => {
                if let Some(base) = self.system.knowledge.iter_mut().find(|b| b.id == id) {
                    base.job = Some(job(&format!("kj-{id}"), &base.name));
                    base.status = "rebuilding".into();
                    let name = base.name.clone();
                    self.emit(BackendEvent::Notice(Notice::JobStarted(name)));
                    self.later(JOB_STEP_MS, SystemTimer::JobTick(JobTarget::Knowledge(id)));
                }
            }
            SystemCommand::KnowledgeCreate(name) => {
                let id = format!("kb-{}", self.system.knowledge.len() + 1);
                self.system.knowledge.push(KnowledgeBase {
                    id,
                    name: name.clone(),
                    documents: 0,
                    chunks: 0,
                    enabled: true,
                    session_default: false,
                    mode: "raw".into(),
                    status: "empty".into(),
                    sources: Vec::new(),
                    health: healthy(0, 0),
                    job: None,
                });
                self.emit(BackendEvent::Notice(Notice::Done(name)));
            }
            SystemCommand::KnowledgeDelete(id) => {
                self.system.knowledge.retain(|b| b.id != id);
                self.emit(BackendEvent::Notice(Notice::Deleted));
            }
            SystemCommand::KnowledgeSource { library, op } => self.knowledge_source(&library, op),
            SystemCommand::IndexCompact(project_id) => {
                if let Some(entry) = self
                    .system
                    .index
                    .iter_mut()
                    .find(|p| p.project_id == project_id)
                {
                    entry.fragments = 0;
                    let name = entry.name.clone();
                    self.emit(BackendEvent::Notice(Notice::Done(name)));
                }
            }
            SystemCommand::Skill(op) => self.skill_op(op),
            SystemCommand::Agent(op) => self.agent_op(op),
            SystemCommand::LuaSkill(op) => self.lua_op(op),
            SystemCommand::Plugin(op) => self.plugin_op(op),
            SystemCommand::KnowledgeSearch(query) => {
                if query.trim().is_empty() {
                    self.system.knowledge_search = None;
                } else {
                    self.later(500, SystemTimer::KnowledgeSearch(query));
                }
            }
            SystemCommand::IndexRebuild(project_id) => {
                if let Some(entry) = self
                    .system
                    .index
                    .iter_mut()
                    .find(|p| p.project_id == project_id)
                {
                    entry.job = Some(job(&format!("ij-{project_id}"), &entry.name));
                    entry.status = "indexing".into();
                    let name = entry.name.clone();
                    self.emit(BackendEvent::Notice(Notice::JobStarted(name)));
                    self.later(
                        JOB_STEP_MS,
                        SystemTimer::JobTick(JobTarget::Index(project_id)),
                    );
                }
            }
            SystemCommand::SendTestNotification => {
                self.emit(BackendEvent::Notice(Notice::NotificationSent));
                return;
            }
            SystemCommand::CleanupLogs => {
                self.system.logs.clear();
                self.emit(BackendEvent::Notice(Notice::LogsCleaned));
            }
        }
        self.emit_system();
    }

    fn set_enabled(&mut self, kind: EntityKind, id: &str, enabled: bool) {
        let system = &mut self.system;
        match kind {
            EntityKind::ConversationModel => {
                if let Some(m) = system.models.iter_mut().find(|m| m.id == id) {
                    m.conversation_enabled = enabled;
                }
            }
            EntityKind::InternalModel => {
                if let Some(m) = system.models.iter_mut().find(|m| m.id == id) {
                    m.internal_enabled = enabled;
                }
            }
            EntityKind::Capability => {
                if let Some(c) = system.capabilities.iter_mut().find(|c| c.key == id) {
                    c.enabled = enabled;
                    if enabled && c.model.is_none() {
                        c.model = c.candidates.first().cloned();
                    }
                }
            }
            EntityKind::Mcp => {
                if let Some(s) = system.mcp_servers.iter_mut().find(|s| s.id == id) {
                    s.enabled = enabled;
                    s.status = if enabled {
                        ServiceStatus::Running
                    } else {
                        ServiceStatus::Stopped
                    };
                    s.tools = if enabled { s.tools.max(4) } else { 0 };
                }
            }
            EntityKind::Skill => {
                if let Some(s) = system.skills.iter_mut().find(|s| s.id == id) {
                    s.enabled = enabled;
                }
            }
            EntityKind::Agent => {
                if let Some(a) = system.agents.iter_mut().find(|a| a.id == id) {
                    a.enabled = enabled;
                }
            }
            EntityKind::Knowledge => {
                if let Some(b) = system.knowledge.iter_mut().find(|b| b.id == id) {
                    b.enabled = enabled;
                }
            }
            EntityKind::IndexWatch => {
                if let Some(p) = system.index.iter_mut().find(|p| p.project_id == id) {
                    p.watching = enabled;
                }
            }
            EntityKind::LuaSkill => {
                if let Some(s) = system.lua_skills.iter_mut().find(|s| s.id == id) {
                    s.enabled = enabled;
                }
            }
            EntityKind::Plugin => {
                if let Some(p) = system.plugins.iter_mut().find(|p| p.id == id) {
                    p.enabled = enabled;
                    p.status = if enabled {
                        ServiceStatus::Running
                    } else {
                        ServiceStatus::Stopped
                    };
                    if !enabled {
                        p.sessions = 0;
                    }
                }
            }
        }
    }

    fn set_field(&mut self, kind: EntityKind, id: &str, field: &str, value: SettingValue) {
        let system = &mut self.system;
        match kind {
            EntityKind::Knowledge => {
                let Some(base) = system.knowledge.iter_mut().find(|b| b.id == id) else {
                    return;
                };
                match field {
                    "mode" => {
                        base.mode = value.as_str().to_string();
                        // A richer layer must be generated before it can serve searches.
                        // 更丰富的检索层需先生成才能服务搜索。
                        if base.mode != "raw" && base.status == "ready" {
                            base.status = "dirty".into();
                        }
                    }
                    "sessionDefault" => base.session_default = value.as_bool(),
                    _ => {}
                }
            }
            EntityKind::LuaSkill => {
                let Some(tool) = field.strip_prefix("tool:") else {
                    return;
                };
                if let Some(skill) = system.lua_skills.iter_mut().find(|s| s.id == id)
                    && let Some(entry) = skill.tools.iter_mut().find(|(name, _)| name == tool)
                    && let Some(mode) = ToolMode::from_id(value.as_str())
                {
                    entry.1 = mode;
                }
            }
            EntityKind::Agent => {
                if field == "expose"
                    && let Some(agent) = system.agents.iter_mut().find(|a| a.id == id)
                {
                    agent.expose = value.as_bool();
                }
            }
            _ => {}
        }
    }

    fn memory_project(&mut self, op: MemoryProjectOp) {
        let projects = &mut self.system.memory.projects;
        match op {
            MemoryProjectOp::Create(name) => {
                let id = format!("mp-{}", projects.len() + 1);
                projects.push(memory_project(&id, &name, 0, false, false));
                self.emit(BackendEvent::Notice(Notice::Done(name)));
            }
            MemoryProjectOp::SetDefault(id) => {
                for p in projects.iter_mut() {
                    p.default = p.id == id;
                }
            }
            MemoryProjectOp::Migrate { from, to } => {
                let Some(target) = projects.iter().position(|p| p.name == to && p.id != from)
                else {
                    self.emit(BackendEvent::Toast {
                        severity: Severity::Error,
                        message: format!("Migration target \"{to}\" does not exist."),
                    });
                    return;
                };
                let Some(source) = projects.iter().position(|p| p.id == from) else {
                    return;
                };
                let moved = std::mem::take(&mut projects[source].items);
                projects[target].items += moved;
                let name = projects[target].name.clone();
                self.emit(BackendEvent::Notice(Notice::Done(name)));
            }
            MemoryProjectOp::Delete(id) => {
                projects.retain(|p| p.id != id || p.protected);
                self.emit(BackendEvent::Notice(Notice::Deleted));
            }
        }
    }

    fn knowledge_source(&mut self, library: &str, op: SourceOp) {
        let Some(base) = self.system.knowledge.iter_mut().find(|b| b.id == library) else {
            return;
        };
        let reindex = match op {
            SourceOp::Add(path) => {
                base.sources.push(KnowledgeSource {
                    id: format!("ks-{library}-{}", base.sources.len() + 1),
                    path,
                    documents: 0,
                    chunks: 0,
                    last_scanned: "—".into(),
                });
                true
            }
            SourceOp::Sync(_) => true,
            SourceOp::Remove(id) => {
                base.sources.retain(|s| s.id != id);
                base.documents = base.sources.iter().map(|s| s.documents).sum();
                base.chunks = base.sources.iter().map(|s| s.chunks).sum();
                base.health = healthy(base.chunks, base.health.content_revision.1 + 1);
                self.emit(BackendEvent::Notice(Notice::Deleted));
                false
            }
        };
        if reindex {
            self.handle_system(SystemCommand::KnowledgeReindex(library.to_string()));
        }
    }

    fn skill_op(&mut self, op: CatalogOp) {
        let skills = &mut self.system.skills;
        match op {
            CatalogOp::Restore(id) => {
                if let Some(s) = skills.iter_mut().find(|s| s.id == id) {
                    s.overridden = false;
                    let name = s.name.clone();
                    self.emit(BackendEvent::Notice(Notice::Done(name)));
                }
            }
            CatalogOp::Delete(id) => {
                skills.retain(|s| s.id != id || s.source != "user");
                self.emit(BackendEvent::Notice(Notice::Deleted));
            }
            CatalogOp::Install(_) | CatalogOp::Update(_) => {}
        }
    }

    fn agent_op(&mut self, op: CatalogOp) {
        let agents = &mut self.system.agents;
        match op {
            CatalogOp::Restore(id) => {
                if let Some(a) = agents.iter_mut().find(|a| a.id == id) {
                    a.overridden = false;
                    a.file = format!("builtin://agents/{}.md", a.name);
                    let name = a.name.clone();
                    self.emit(BackendEvent::Notice(Notice::Done(name)));
                }
            }
            CatalogOp::Delete(id) => {
                agents.retain(|a| a.id != id || a.scope != "user");
                self.emit(BackendEvent::Notice(Notice::Deleted));
            }
            CatalogOp::Install(_) | CatalogOp::Update(_) => {}
        }
    }

    fn lua_op(&mut self, op: CatalogOp) {
        let skills = &mut self.system.lua_skills;
        let started = match op {
            CatalogOp::Install(source) => {
                let name = source
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or(&source)
                    .trim_end_matches(".yaml")
                    .to_string();
                let id = format!("ls-{}", skills.len() + 1);
                skills.push(LuaSkill {
                    id: id.clone(),
                    name: name.clone(),
                    version: "—".into(),
                    source,
                    layer: "user".into(),
                    description: String::new(),
                    enabled: false,
                    tools: Vec::new(),
                    update: None,
                    job: Some(job(&format!("lj-{id}"), &name)),
                });
                Some((id, name))
            }
            CatalogOp::Update(id) => skills.iter_mut().find(|s| s.id == id).map(|s| {
                s.job = Some(job(&format!("lj-{id}"), &s.name));
                (id.clone(), s.name.clone())
            }),
            CatalogOp::Delete(id) => {
                skills.retain(|s| s.id != id);
                self.emit(BackendEvent::Notice(Notice::Deleted));
                None
            }
            CatalogOp::Restore(_) => None,
        };
        if let Some((id, name)) = started {
            self.emit(BackendEvent::Notice(Notice::JobStarted(name)));
            self.later(JOB_STEP_MS, SystemTimer::JobTick(JobTarget::Lua(id)));
        }
    }

    fn plugin_op(&mut self, op: CatalogOp) {
        let plugins = &mut self.system.plugins;
        match op {
            CatalogOp::Install(path) => {
                let name = path
                    .trim_end_matches(['/', '\\'])
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(&path)
                    .to_string();
                plugins.push(Plugin {
                    id: format!("pl-{}", plugins.len() + 1),
                    name: name.clone(),
                    kind: "user".into(),
                    description: path,
                    enabled: false,
                    status: ServiceStatus::Stopped,
                    tools: 0,
                    hooks: 0,
                    sessions: 0,
                    skills: 0,
                    agents: 0,
                    permissions: Vec::new(),
                    mounts: Vec::new(),
                    diagnostics: Vec::new(),
                });
                self.emit(BackendEvent::Notice(Notice::Done(name)));
            }
            CatalogOp::Delete(id) => {
                if plugins.iter().any(|p| p.id == id && p.enabled) {
                    self.emit(BackendEvent::Toast {
                        severity: Severity::Warning,
                        message: "Disable the plugin before uninstalling it.".into(),
                    });
                    return;
                }
                plugins.retain(|p| p.id != id || p.kind != "user");
                self.emit(BackendEvent::Notice(Notice::Deleted));
            }
            CatalogOp::Update(_) | CatalogOp::Restore(_) => {}
        }
    }

    fn credential(&mut self, provider: &str, op: CredentialOp) {
        let Some(entry) = self.system.providers.iter_mut().find(|p| p.id == provider) else {
            return;
        };
        match op {
            CredentialOp::Add { label, secret } => {
                // Keep only a masked tail; the secret itself is dropped here.
                // 只保留打码后的尾部；密钥本身在此丢弃。
                let masked = mask(&secret);
                let id = format!("cred-{}-{}", provider, entry.credentials.len() + 1);
                let is_default = entry.credentials.is_empty();
                entry.credentials.push(Credential {
                    id,
                    label: if label.trim().is_empty() {
                        format!("Key {}", entry.credentials.len() + 1)
                    } else {
                        label
                    },
                    masked,
                    is_default,
                    last_test: None,
                });
                self.emit(BackendEvent::Notice(Notice::CredentialAdded));
            }
            CredentialOp::Test(id) => {
                if let Some(c) = entry.credentials.iter_mut().find(|c| c.id == id) {
                    c.last_test = Some((Severity::Warning, "…".into()));
                }
                let provider = provider.to_string();
                self.later(900, SystemTimer::CredentialTested { provider, id });
            }
            CredentialOp::MakeDefault(id) => {
                for c in &mut entry.credentials {
                    c.is_default = c.id == id;
                }
            }
            CredentialOp::Delete(id) => {
                entry.credentials.retain(|c| c.id != id);
                if !entry.credentials.iter().any(|c| c.is_default)
                    && let Some(first) = entry.credentials.first_mut()
                {
                    first.is_default = true;
                }
                self.emit(BackendEvent::Notice(Notice::Deleted));
            }
        }
    }

    fn restore_archived(&mut self, id: &str) {
        let Some(index) = self.system.archived.iter().position(|a| a.id == id) else {
            return;
        };
        let archived = self.system.archived.remove(index);
        let row = self
            .archived_rows
            .remove(id)
            .unwrap_or_else(|| ConversationSummary {
                id: archived.id.clone(),
                title: archived.title.clone(),
                project_id: self
                    .projects
                    .iter()
                    .find(|p| p.name == archived.project)
                    .map(|p| p.id.clone()),
                run_phase: RunPhase::Idle,
                last_updated_label: archived.archived_label.clone(),
                unread: false,
                attention: false,
                pinned: false,
            });
        self.ensure_log(&row.id);
        self.conversations.insert(0, row.clone());
        self.emit(BackendEvent::ConversationSummary(row));
        self.emit(BackendEvent::Notice(Notice::Restored(archived.title)));
    }

    pub(super) fn fire_system(&mut self, timer: SystemTimer) {
        match timer {
            SystemTimer::ServiceSettled(status) => {
                let memory = &mut self.system.memory;
                memory.status = status;
                memory.health = if status == ServiceStatus::Running {
                    Severity::Info
                } else {
                    Severity::Warning
                };
            }
            SystemTimer::MemorySearch(query) => {
                let memory = &mut self.system.memory;
                memory.searching = false;
                let needle = query.to_lowercase();
                let hits = memory
                    .items
                    .iter()
                    .filter(|i| {
                        i.content.to_lowercase().contains(&needle)
                            || i.project.to_lowercase().contains(&needle)
                    })
                    .enumerate()
                    .map(|(rank, i)| MemoryItem {
                        score: Some(0.92 - rank as f32 * 0.07),
                        ..i.clone()
                    })
                    .collect();
                memory.search = Some((query, hits));
            }
            SystemTimer::KnowledgeSearch(query) => {
                let hit = |title: &str, snippet: &str, score| SearchHit {
                    title: title.into(),
                    snippet: snippet.into(),
                    score,
                };
                self.system.knowledge_search = Some((
                    query.clone(),
                    vec![
                        hit(
                            "tooling-and-terminal-runtime.md",
                            &format!("… `{query}` · timeout_ms omitted means wait forever …"),
                            0.88,
                        ),
                        hit(
                            "ratatui · Buffer::cell_mut",
                            &format!("… returns the cell at a position for `{query}` …"),
                            0.74,
                        ),
                        hit(
                            "會議記錄 2026-09-18",
                            &format!("… 討論 {query} 的呈現方式 …"),
                            0.61,
                        ),
                    ],
                ));
            }
            SystemTimer::CredentialTested { provider, id } => {
                let ok = !id.ends_with('2');
                if let Some(entry) = self.system.providers.iter_mut().find(|p| p.id == provider)
                    && let Some(c) = entry.credentials.iter_mut().find(|c| c.id == id)
                {
                    c.last_test = Some(if ok {
                        (Severity::Info, "200 OK · 312 ms".into())
                    } else {
                        (Severity::Error, "401 invalid x-api-key".into())
                    });
                }
                self.emit(BackendEvent::Notice(Notice::CredentialTested { ok }));
            }
            SystemTimer::McpRestarted(id) => {
                if let Some(s) = self.system.mcp_servers.iter_mut().find(|s| s.id == id) {
                    // playwright keeps failing so the error path stays visible in the demo.
                    // playwright 持续失败，使 demo 始终能展示错误路径。
                    if s.name == "playwright" {
                        s.status = ServiceStatus::Failed;
                    } else {
                        s.status = ServiceStatus::Running;
                        s.enabled = true;
                        s.tools = s.tools.max(4);
                    }
                }
            }
            SystemTimer::RouteTested(key) => {
                let model = if key == "engine.embedding" {
                    "engine.embeddingModel"
                } else {
                    key.as_str()
                };
                let configured = self
                    .system
                    .setting(model)
                    .is_some_and(|v| !v.as_str().is_empty());
                let result = if configured {
                    (Severity::Info, "200 OK · 286 ms".to_string())
                } else {
                    (Severity::Error, "No model route is configured.".to_string())
                };
                self.system.route_tests.insert(key, result);
            }
            SystemTimer::JobTick(target) => {
                let finished = self.tick_job(&target);
                match finished {
                    Some(label) => self.emit(BackendEvent::Notice(Notice::JobFinished(label))),
                    None => self.later(JOB_STEP_MS, SystemTimer::JobTick(target)),
                }
            }
        }
        self.emit_system();
    }

    /// Advances one job by a step; returns its label once it completes.
    /// 将任务推进一步；完成时返回其标签。
    fn tick_job(&mut self, target: &JobTarget) -> Option<String> {
        fn step(job: &mut Job) -> bool {
            let next = job.progress.unwrap_or(0).saturating_add(9).min(100);
            job.progress = Some(next);
            job.detail = format!("{next}%");
            if next >= 100 {
                job.done = true;
            }
            job.done
        }
        let system = &mut self.system;
        match target {
            JobTarget::Knowledge(id) => {
                let base = system.knowledge.iter_mut().find(|b| &b.id == id)?;
                let job = base.job.as_mut()?;
                if !step(job) {
                    return None;
                }
                let label = job.label.clone();
                base.job = None;
                base.documents = base.sources.iter().map(|s| s.documents.max(3)).sum();
                for source in &mut base.sources {
                    source.documents = source.documents.max(3);
                    source.chunks = source.chunks.max(48);
                    source.last_scanned = "now".into();
                }
                base.chunks = base.sources.iter().map(|s| s.chunks).sum();
                base.health = healthy(base.chunks, base.health.content_revision.1);
                base.status = if base.chunks == 0 { "empty" } else { "ready" }.into();
                Some(label)
            }
            JobTarget::Index(id) => {
                let entry = system.index.iter_mut().find(|p| &p.project_id == id)?;
                let job = entry.job.as_mut()?;
                entry.pending = entry.pending.saturating_sub(entry.pending / 3 + 1);
                if !step(job) {
                    return None;
                }
                entry.pending = 0;
                entry.symbols += 128;
                entry.status = "ready".into();
                entry.last_update = "now".into();
                let label = job.label.clone();
                entry.job = None;
                Some(label)
            }
            JobTarget::Lua(id) => {
                let skill = system.lua_skills.iter_mut().find(|s| &s.id == id)?;
                let job = skill.job.as_mut()?;
                if !step(job) {
                    return None;
                }
                let label = job.label.clone();
                skill.job = None;
                if let Some(version) = skill.update.take() {
                    skill.version = version;
                } else if skill.version == "—" {
                    skill.version = "1.0.0".into();
                    skill.tools = vec![(
                        format!("{}_run", skill.name.replace('-', "_")),
                        ToolMode::Implicit,
                    )];
                }
                Some(label)
            }
            JobTarget::Memory(id) => {
                let job = system.memory.jobs.iter_mut().find(|j| &j.id == id)?;
                if !step(job) {
                    return None;
                }
                let label = job.label.clone();
                if id == "mj-cleanup" {
                    system.memory.pending_writebacks = 0;
                }
                if id == "mj-profile" {
                    system.memory.last_profile_update = "now".into();
                }
                Some(label)
            }
        }
    }
}
