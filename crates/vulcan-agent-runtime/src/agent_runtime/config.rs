//! Runtime configuration mapping shared by the compose execution foundation.
//! compose 执行基础架构共享的运行时配置映射层。

use crate::agent_profile::{AgentProfileCatalog, UserAgentProfile, is_reserved_agent_identifier};
use crate::provider_config::{
    PersistedSkillEntry, PersistedSkillKind, PersistedToolPolicyEntry, ProviderConfigurationFile,
};
use crate::runtime_assets;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use vulcan_session_core::ExecutionRuntimePolicyRequest;

/// Stable identifier of the default worker agent seeded into fresh configurations.
/// 注入到新配置中的默认工作代理稳定标识。
pub const DEFAULT_WORKER_AGENT_ID: &str = "default-worker";

/// Stable identifier of the Lua specialist agent seeded into fresh configurations.
/// 注入到新配置中的 Lua 专家代理稳定标识。
pub const LUA_SPECIALIST_AGENT_ID: &str = "lua-specialist";

/// Stable identifier of the memory specialist agent seeded into fresh configurations.
/// 注入到新配置中的记忆专家代理稳定标识。
pub const MEMORY_SPECIALIST_AGENT_ID: &str = "memory-specialist";

/// Stable identifier of the default coordinator agent seeded into fresh configurations.
/// 注入到新配置中的默认协调代理稳定标识。
pub const WORKSPACE_COORDINATOR_AGENT_ID: &str = "workspace-coordinator";

/// Stable identifier of the first-class planning agent seeded into fresh configurations.
/// 注入到新配置中的一等规划代理稳定标识。
pub const PLAN_AGENT_ID: &str = "plan";

/// Stable identifier of the first-class repository exploration agent seeded into fresh configurations.
/// 注入到新配置中的一等仓库探索代理稳定标识。
pub const EXPLORE_AGENT_ID: &str = "explore";

/// Stable identifier of the default safe tool policy.
/// 默认安全工具策略的稳定标识。
pub const DEFAULT_SAFE_TOOL_POLICY_ID: &str = "default-safe";

/// Stable identifier of the Lua-oriented tool policy.
/// Lua 定向工具策略的稳定标识。
pub const LUA_SAFE_TOOL_POLICY_ID: &str = "lua-safe";

/// Stable identifier of the analysis-oriented read-only tool policy.
/// 分析定向只读工具策略的稳定标识。
pub const ANALYSIS_SAFE_TOOL_POLICY_ID: &str = "analysis-safe";

/// Relative resource directory name that stores default agent system prompts.
/// 存放默认代理系统提示词的相对资源目录名称。
const AGENT_PROMPTS_DIR_NAME: &str = "agent-prompts";

/// Default worker prompt resource file name.
/// 默认工作代理提示词资源文件名。
const DEFAULT_WORKER_PROMPT_FILE_NAME: &str = "default-worker.txt";

/// Default Lua specialist prompt resource file name.
/// 默认 Lua 专家代理提示词资源文件名。
const LUA_SPECIALIST_PROMPT_FILE_NAME: &str = "lua-specialist.txt";

/// Default memory specialist prompt resource file name.
/// 默认记忆专家代理提示词资源文件名。
const MEMORY_SPECIALIST_PROMPT_FILE_NAME: &str = "memory-specialist.txt";

/// Default plan agent prompt resource file name.
/// 默认规划代理提示词资源文件名。
const PLAN_PROMPT_FILE_NAME: &str = "plan.txt";

/// Default explore agent prompt resource file name.
/// 默认探索代理提示词资源文件名。
const EXPLORE_PROMPT_FILE_NAME: &str = "explore.txt";

/// Legacy inline worker prompt kept as one compatibility fallback.
/// 保留为兼容回退的一条旧版内联工作代理提示词。
const LEGACY_DEFAULT_WORKER_PROMPT: &str = "你是 Vulcan Code 的默认工作代理。";

/// Legacy inline Lua prompt kept as one compatibility fallback.
/// 保留为兼容回退的一条旧版内联 Lua 代理提示词。
const LEGACY_LUA_SPECIALIST_PROMPT: &str = "你是 LuaSkills 方向的专家代理。";

/// Legacy inline memory prompt kept as one compatibility fallback.
/// 保留为兼容回退的一条旧版内联记忆代理提示词。
const LEGACY_MEMORY_SPECIALIST_PROMPT: &str = "你是 VulcanMemoryMesh 方向的专家代理。";

/// Legacy inline plan prompt kept as one compatibility fallback.
/// 保留为兼容回退的一条旧版内联规划代理提示词。
const LEGACY_PLAN_PROMPT: &str = "你是 Vulcan Code 的方案代理。";

/// Legacy inline explore prompt kept as one compatibility fallback.
/// 保留为兼容回退的一条旧版内联探索代理提示词。
const LEGACY_EXPLORE_PROMPT: &str = "你是 Vulcan Code 的探索代理。";

/// Runtime-facing agent family used by the compose foundation.
/// compose 基础架构使用的运行时代理类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAgentKind {
    /// General worker agent.
    /// 常规工作代理。
    Worker,
    /// First-class planning agent.
    /// 一等规划代理。
    Planner,
    /// First-class exploration agent.
    /// 一等探索代理。
    Explorer,
    /// Specialist agent.
    /// 专家代理。
    Specialist,
    /// Coordinator agent.
    /// 协调代理。
    Coordinator,
}

impl RuntimeAgentKind {
    /// Returns a compact human-readable label.
    /// 返回紧凑的人类可读标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Worker => "工作代理",
            Self::Planner => "规划代理",
            Self::Explorer => "探索代理",
            Self::Specialist => "专家代理",
            Self::Coordinator => "协调代理",
        }
    }
}

/// Runtime-facing skill family used by the compose foundation.
/// compose 基础架构使用的运行时技能类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSkillKind {
    /// General non-Lua skill.
    /// 常规非 Lua 技能。
    Regular,
    /// LuaSkills-backed skill.
    /// 由 LuaSkills 驱动的技能。
    LuaSkills,
    /// Automation-oriented skill.
    /// 面向自动化的技能。
    Automation,
    /// Memory-oriented skill.
    /// 面向记忆体系的技能。
    Memory,
}

impl From<PersistedSkillKind> for RuntimeSkillKind {
    /// Converts one persisted skill kind into its runtime counterpart.
    /// 将一个持久化技能类型转换为对应的运行时技能类型。
    fn from(value: PersistedSkillKind) -> Self {
        match value {
            PersistedSkillKind::Regular => Self::Regular,
            PersistedSkillKind::LuaSkills => Self::LuaSkills,
            PersistedSkillKind::Automation => Self::Automation,
            PersistedSkillKind::Memory => Self::Memory,
        }
    }
}

/// Runtime-facing routing rule used by coordinator agents.
/// 协调代理使用的运行时路由规则。
#[derive(Debug, Clone)]
pub struct RuntimeAgentRoute {
    /// Signal name that activates the route.
    /// 激活该路由的信号名称。
    pub signal: String,
    /// Target delegate agent identifier.
    /// 目标委派代理标识。
    pub delegate: String,
}

/// Runtime-facing tool-policy definition.
/// 面向运行时的工具策略定义。
#[derive(Debug, Clone)]
pub struct RuntimeToolPolicyDefinition {
    /// Stable policy identifier.
    /// 稳定策略标识。
    pub id: String,
    /// Human-readable policy name.
    /// 人类可读的策略名称。
    pub name: String,
    /// Whether every native tool should be visible.
    /// 是否让全部原生工具都可见。
    pub allow_all_native: bool,
    /// Explicit native tool whitelist.
    /// 显式原生工具白名单。
    pub native_tools: Vec<String>,
    /// Whether every enabled Vulcan Plugin tool group should be visible.
    /// 是否让全部已启用 Vulcan 插件工具组都可见。
    pub allow_all_plugin_tools: bool,
    /// Explicit Vulcan Plugin tool whitelist.
    /// 显式 Vulcan 插件工具白名单。
    pub plugin_tools: Vec<String>,
    /// Explicit MCP server whitelist.
    /// 显式 MCP 服务器白名单。
    pub mcp_servers: Vec<String>,
    /// Explicit MCP tool whitelist.
    /// 显式 MCP 工具白名单。
    pub mcp_tools: Vec<String>,
}

/// Runtime-facing skill definition.
/// 面向运行时的技能定义。
#[derive(Debug, Clone)]
pub struct RuntimeSkillDefinition {
    /// Stable skill identifier.
    /// 稳定技能标识。
    pub id: String,
    /// Human-readable skill name.
    /// 人类可读的技能名称。
    pub name: String,
    /// Skill family.
    /// 技能类型。
    pub kind: RuntimeSkillKind,
    /// Optional skill description.
    /// 可选技能描述。
    pub description: String,
    /// Whether the skill is enabled.
    /// 当前技能是否启用。
    pub enabled: bool,
    /// Signals that activate the skill.
    /// 激活该技能的信号集合。
    pub trigger_signals: Vec<String>,
    /// Optional emitted signal.
    /// 可选的输出信号。
    pub emitted_signal: Option<String>,
    /// Confidence delta contributed by the skill.
    /// 技能贡献的置信度增量。
    pub confidence_delta: f32,
}

/// Runtime-facing agent definition.
/// 面向运行时的代理定义。
#[derive(Debug, Clone)]
pub struct RuntimeAgentDefinition {
    /// Stable agent identifier.
    /// 稳定代理标识。
    pub id: String,
    /// Human-readable agent name.
    /// 人类可读的代理名称。
    pub name: String,
    /// Agent family.
    /// 代理类型。
    pub kind: RuntimeAgentKind,
    /// Optional agent description.
    /// 可选代理描述。
    pub description: String,
    /// Optional agent charter or system prompt.
    /// 可选代理章程或系统提示词。
    pub system_prompt: String,
    /// Skill identifiers bound to the agent.
    /// 绑定到该代理上的技能标识集合。
    pub skill_ids: Vec<String>,
    /// Direct native tool attachments.
    /// 直接附着的原生工具集合。
    pub native_tools: Vec<String>,
    /// Direct Vulcan Plugin tool attachments.
    /// 直接附着的 Vulcan 插件工具集合。
    pub plugin_tools: Vec<String>,
    /// Direct MCP tool attachments.
    /// 直接附着的 MCP 工具集合。
    pub mcp_tools: Vec<String>,
    /// Delegate agent identifiers.
    /// 委派代理标识集合。
    pub delegates: Vec<String>,
    /// Optional shared tool-policy identifier.
    /// 可选共享工具策略标识。
    pub tool_policy: Option<String>,
    /// Coordinator routing rules.
    /// 协调代理路由规则集合。
    pub routes: Vec<RuntimeAgentRoute>,
    /// Optional fallback delegate.
    /// 可选回退委派目标。
    pub fallback_delegate: Option<String>,
    /// Optional model override.
    /// 可选模型覆盖。
    pub model: Option<String>,
    /// Optional temperature override.
    /// 可选温度参数覆盖。
    pub temperature: Option<f32>,
    /// Optional max-turn override.
    /// 可选最大轮次数覆盖。
    pub max_turns: Option<u16>,
    /// Optional task-session runtime feature overrides.
    /// 可选的子代理运行时能力覆盖。
    pub runtime_policy: ExecutionRuntimePolicyRequest,
}

/// Runtime-facing aggregate configuration consumed by the compose foundation.
/// 供 compose 基础架构消费的运行时聚合配置。
#[derive(Debug, Clone)]
pub struct AgentRuntimeConfig {
    /// Currently selected global model in `provider/model` form.
    /// 当前以 `provider/model` 形式保存的全局模型。
    pub active_model: Option<String>,
    /// Active agent identifier selected for execution.
    /// 选中用于执行的激活代理标识。
    pub active_agent_id: String,
    /// Declared agent definitions.
    /// 已声明的代理定义集合。
    pub agents: Vec<RuntimeAgentDefinition>,
    /// Declared skill definitions.
    /// 已声明的技能定义集合。
    pub skills: Vec<RuntimeSkillDefinition>,
    /// Declared tool-policy definitions keyed by policy id.
    /// 以策略标识为键声明的工具策略定义集合。
    pub tool_policies: BTreeMap<String, RuntimeToolPolicyDefinition>,
    /// Configured MCP server identifiers.
    /// 已配置的 MCP 服务器标识集合。
    pub configured_mcp_server_ids: Vec<String>,
    /// Enabled MCP server identifiers.
    /// 已启用的 MCP 服务器标识集合。
    pub enabled_mcp_server_ids: Vec<String>,
}

impl AgentRuntimeConfig {
    /// Builds the runtime-facing aggregate configuration from the persisted root config.
    /// 根据持久化根配置构建面向运行时的聚合配置。
    pub fn from_sources(
        config: &ProviderConfigurationFile,
        agent_profile_catalog: &AgentProfileCatalog,
    ) -> Self {
        let mut agents = vec![
            default_worker_agent(),
            default_plan_agent(),
            default_explore_agent(),
            default_lua_agent(),
            default_memory_agent(),
            default_coordinator_agent(),
        ];
        agents.extend(
            agent_profile_catalog
                .agents
                .values()
                .filter(|profile| !is_reserved_agent_identifier(&profile.id))
                .map(runtime_agent_from_user_profile),
        );

        let skills = config
            .skills
            .iter()
            .map(|(id, entry)| RuntimeSkillDefinition {
                id: id.clone(),
                name: entry.name.clone(),
                kind: entry.kind.into(),
                description: entry.description.clone().unwrap_or_default(),
                enabled: entry.enabled,
                trigger_signals: entry.trigger_signals.clone(),
                emitted_signal: entry.emitted_signal.clone(),
                confidence_delta: entry.confidence_delta.unwrap_or(0.0),
            })
            .collect::<Vec<_>>();

        let tool_policies = config
            .tool_policies
            .iter()
            .map(|(id, entry)| (id.clone(), runtime_tool_policy_from_persisted(id, entry)))
            .collect::<BTreeMap<_, _>>();
        let tool_policies = merge_runtime_default_tool_policies(tool_policies);

        let configured_mcp_server_ids = config.mcp.servers.keys().cloned().collect::<Vec<_>>();
        let enabled_mcp_server_ids = config
            .mcp
            .servers
            .iter()
            .filter(|(_, entry)| entry.enabled)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();

        Self {
            active_model: None,
            active_agent_id: DEFAULT_WORKER_AGENT_ID.to_string(),
            agents,
            skills,
            tool_policies,
            configured_mcp_server_ids,
            enabled_mcp_server_ids,
        }
    }
}

/// Ensures the shared configuration file contains the compose foundation defaults.
/// 确保共享配置文件中包含 compose 基础架构所需的默认配置。
pub fn ensure_runtime_defaults(config: &mut ProviderConfigurationFile) -> bool {
    let mut changed = false;

    changed |= ensure_default_skill(
        config,
        "regular.workspace-triage",
        default_workspace_skill(),
    );
    changed |= ensure_default_skill(config, "luaskills.bridge", default_lua_skill());
    changed |= ensure_default_skill(config, "automation.scheduler", default_automation_skill());
    changed |= ensure_default_skill(config, "memorymesh.recall", default_memory_skill());

    changed
}

/// Converts one persisted tool policy into its runtime representation.
/// 将单条持久化工具策略转换为对应的运行时表示。
fn runtime_tool_policy_from_persisted(
    id: &str,
    entry: &PersistedToolPolicyEntry,
) -> RuntimeToolPolicyDefinition {
    RuntimeToolPolicyDefinition {
        id: id.to_string(),
        name: entry.name.clone().unwrap_or_else(|| id.to_string()),
        allow_all_native: entry.allow_all_native,
        native_tools: entry.native_tools.clone(),
        allow_all_plugin_tools: entry.allow_all_plugin_tools,
        plugin_tools: entry.plugin_tools.clone(),
        mcp_servers: entry.mcp_servers.clone(),
        mcp_tools: entry.mcp_tools.clone(),
    }
}

/// Merges runtime-only reserved tool-policy defaults without mutating the persisted configuration file.
/// 合并仅存在于运行时的保留工具策略默认值，同时不改写持久化配置文件。
fn merge_runtime_default_tool_policies(
    mut persisted: BTreeMap<String, RuntimeToolPolicyDefinition>,
) -> BTreeMap<String, RuntimeToolPolicyDefinition> {
    for definition in [
        default_safe_runtime_tool_policy(),
        default_analysis_runtime_tool_policy(),
        default_lua_runtime_tool_policy(),
    ] {
        persisted.entry(definition.id.clone()).or_insert(definition);
    }
    persisted
}

/// Inserts a default skill definition only when the given identifier is absent.
/// 仅当给定标识不存在时插入默认技能定义。
fn ensure_default_skill(
    config: &mut ProviderConfigurationFile,
    id: &str,
    entry: PersistedSkillEntry,
) -> bool {
    if config.skills.contains_key(id) {
        false
    } else {
        config.skills.insert(id.to_string(), entry);
        true
    }
}

/// Builds the runtime-only default safe tool policy used when the persisted configuration omits the reserved entry.
/// 构建当持久化配置缺失保留条目时使用的仅运行时默认安全工具策略。
fn default_safe_runtime_tool_policy() -> RuntimeToolPolicyDefinition {
    RuntimeToolPolicyDefinition {
        id: String::from(DEFAULT_SAFE_TOOL_POLICY_ID),
        name: String::from("默认安全策略"),
        allow_all_native: true,
        native_tools: Vec::new(),
        allow_all_plugin_tools: true,
        plugin_tools: Vec::new(),
        mcp_servers: Vec::new(),
        mcp_tools: Vec::new(),
    }
}

/// Builds the runtime-only analysis tool policy used when the persisted configuration omits the reserved entry.
/// 构建当持久化配置缺失保留条目时使用的仅运行时分析工具策略。
fn default_analysis_runtime_tool_policy() -> RuntimeToolPolicyDefinition {
    RuntimeToolPolicyDefinition {
        id: String::from(ANALYSIS_SAFE_TOOL_POLICY_ID),
        name: String::from("分析只读策略"),
        allow_all_native: false,
        native_tools: vec![
            String::from("file"),
            String::from("shell_command"),
            String::from("terminal_session"),
            String::from("vmm_memory_search"),
            String::from("vmm_turn_details"),
        ],
        allow_all_plugin_tools: false,
        plugin_tools: Vec::new(),
        mcp_servers: Vec::new(),
        mcp_tools: Vec::new(),
    }
}

/// Builds the runtime-only Lua tool policy used when the persisted configuration omits the reserved entry.
/// 构建当持久化配置缺失保留条目时使用的仅运行时 Lua 工具策略。
fn default_lua_runtime_tool_policy() -> RuntimeToolPolicyDefinition {
    RuntimeToolPolicyDefinition {
        id: String::from(LUA_SAFE_TOOL_POLICY_ID),
        name: String::from("Lua 定向策略"),
        allow_all_native: false,
        native_tools: vec![
            String::from("shell_command"),
            String::from("terminal_session"),
            String::from("vmm_memory_search"),
            String::from("vmm_turn_details"),
        ],
        allow_all_plugin_tools: false,
        plugin_tools: Vec::new(),
        mcp_servers: Vec::new(),
        mcp_tools: Vec::new(),
    }
}

/// Builds the default regular workspace skill definition.
/// 构建默认的常规工作区技能定义。
fn default_workspace_skill() -> PersistedSkillEntry {
    PersistedSkillEntry {
        name: String::from("工作区梳理"),
        kind: PersistedSkillKind::Regular,
        description: Some(String::from("负责工作区扫描、任务拆解与执行入口预判。")),
        enabled: true,
        trigger_signals: vec![String::from("workspace.scan")],
        emitted_signal: Some(String::from("workspace.ready")),
        confidence_delta: Some(0.05),
    }
}

/// Builds the default LuaSkills bridge definition.
/// 构建默认的 LuaSkills 桥接定义。
fn default_lua_skill() -> PersistedSkillEntry {
    PersistedSkillEntry {
        name: String::from("LuaSkills 桥接"),
        kind: PersistedSkillKind::LuaSkills,
        description: Some(String::from(
            "负责 LuaSkills 技能路由、工具装配与执行桥接。",
        )),
        enabled: true,
        trigger_signals: vec![String::from("lua.request")],
        emitted_signal: Some(String::from("lua.ready")),
        confidence_delta: Some(0.04),
    }
}

/// Builds the default automation skill definition.
/// 构建默认的自动化技能定义。
fn default_automation_skill() -> PersistedSkillEntry {
    PersistedSkillEntry {
        name: String::from("自动化调度"),
        kind: PersistedSkillKind::Automation,
        description: Some(String::from("负责自动化唤醒、调度信号与流程编排前置判断。")),
        enabled: true,
        trigger_signals: vec![String::from("automation.tick")],
        emitted_signal: Some(String::from("automation.ready")),
        confidence_delta: Some(0.03),
    }
}

/// Builds the default memory-oriented skill definition.
/// 构建默认的面向记忆体系的技能定义。
fn default_memory_skill() -> PersistedSkillEntry {
    PersistedSkillEntry {
        name: String::from("记忆召回"),
        kind: PersistedSkillKind::Memory,
        description: Some(String::from("负责记忆网格召回、历史痕迹补全与上下文复用。")),
        enabled: true,
        trigger_signals: vec![String::from("memory.recall")],
        emitted_signal: Some(String::from("memory.ready")),
        confidence_delta: Some(0.02),
    }
}

/// Builds the default worker agent runtime definition.
/// 构建默认工作代理运行时定义。
fn default_worker_agent() -> RuntimeAgentDefinition {
    RuntimeAgentDefinition {
        id: String::from(DEFAULT_WORKER_AGENT_ID),
        name: String::from("默认工作代理"),
        kind: RuntimeAgentKind::Worker,
        description: String::from("负责常规 AI Coding 执行、问题拆解与基础工具协作。"),
        system_prompt: load_default_agent_prompt(
            DEFAULT_WORKER_PROMPT_FILE_NAME,
            LEGACY_DEFAULT_WORKER_PROMPT,
        ),
        skill_ids: vec![String::from("regular.workspace-triage")],
        native_tools: Vec::new(),
        plugin_tools: Vec::new(),
        mcp_tools: Vec::new(),
        delegates: vec![
            String::from(LUA_SPECIALIST_AGENT_ID),
            String::from(MEMORY_SPECIALIST_AGENT_ID),
        ],
        tool_policy: Some(String::from(DEFAULT_SAFE_TOOL_POLICY_ID)),
        routes: Vec::new(),
        fallback_delegate: None,
        model: None,
        temperature: None,
        max_turns: None,
        runtime_policy: ExecutionRuntimePolicyRequest::default(),
    }
}

/// Builds the default plan agent runtime definition.
/// 构建默认规划代理运行时定义。
fn default_plan_agent() -> RuntimeAgentDefinition {
    RuntimeAgentDefinition {
        id: String::from(PLAN_AGENT_ID),
        name: String::from("方案代理"),
        kind: RuntimeAgentKind::Planner,
        description: String::from("负责在编码前完成目标拆解、风险识别与执行路径规划。"),
        system_prompt: load_default_agent_prompt(PLAN_PROMPT_FILE_NAME, LEGACY_PLAN_PROMPT),
        skill_ids: Vec::new(),
        native_tools: Vec::new(),
        plugin_tools: Vec::new(),
        mcp_tools: Vec::new(),
        delegates: Vec::new(),
        tool_policy: Some(String::from(ANALYSIS_SAFE_TOOL_POLICY_ID)),
        routes: Vec::new(),
        fallback_delegate: None,
        model: None,
        temperature: None,
        max_turns: None,
        runtime_policy: ExecutionRuntimePolicyRequest::default(),
    }
}

/// Builds the default explore agent runtime definition.
/// 构建默认探索代理运行时定义。
fn default_explore_agent() -> RuntimeAgentDefinition {
    RuntimeAgentDefinition {
        id: String::from(EXPLORE_AGENT_ID),
        name: String::from("探索代理"),
        kind: RuntimeAgentKind::Explorer,
        description: String::from("负责工作区定位、代码脉络梳理与证据化上下文收集。"),
        system_prompt: load_default_agent_prompt(EXPLORE_PROMPT_FILE_NAME, LEGACY_EXPLORE_PROMPT),
        skill_ids: Vec::new(),
        native_tools: Vec::new(),
        plugin_tools: Vec::new(),
        mcp_tools: Vec::new(),
        delegates: Vec::new(),
        tool_policy: Some(String::from(ANALYSIS_SAFE_TOOL_POLICY_ID)),
        routes: Vec::new(),
        fallback_delegate: None,
        model: None,
        temperature: None,
        max_turns: None,
        runtime_policy: ExecutionRuntimePolicyRequest::default(),
    }
}

/// Builds the default Lua specialist agent runtime definition.
/// 构建默认 Lua 专家代理运行时定义。
fn default_lua_agent() -> RuntimeAgentDefinition {
    RuntimeAgentDefinition {
        id: String::from(LUA_SPECIALIST_AGENT_ID),
        name: String::from("LuaSkills 专家代理"),
        kind: RuntimeAgentKind::Specialist,
        description: String::from("负责 LuaSkills、Lua 工具链与宿主运行时桥接。"),
        system_prompt: load_default_agent_prompt(
            LUA_SPECIALIST_PROMPT_FILE_NAME,
            LEGACY_LUA_SPECIALIST_PROMPT,
        ),
        skill_ids: vec![String::from("luaskills.bridge")],
        native_tools: Vec::new(),
        plugin_tools: Vec::new(),
        mcp_tools: Vec::new(),
        delegates: Vec::new(),
        tool_policy: Some(String::from(LUA_SAFE_TOOL_POLICY_ID)),
        routes: Vec::new(),
        fallback_delegate: None,
        model: None,
        temperature: None,
        max_turns: None,
        runtime_policy: ExecutionRuntimePolicyRequest::default(),
    }
}

/// Builds the default memory specialist agent runtime definition.
/// 构建默认记忆专家代理运行时定义。
fn default_memory_agent() -> RuntimeAgentDefinition {
    RuntimeAgentDefinition {
        id: String::from(MEMORY_SPECIALIST_AGENT_ID),
        name: String::from("记忆专家代理"),
        kind: RuntimeAgentKind::Specialist,
        description: String::from("负责记忆网格、历史上下文与长期信息编织。"),
        system_prompt: load_default_agent_prompt(
            MEMORY_SPECIALIST_PROMPT_FILE_NAME,
            LEGACY_MEMORY_SPECIALIST_PROMPT,
        ),
        skill_ids: vec![String::from("memorymesh.recall")],
        native_tools: Vec::new(),
        plugin_tools: Vec::new(),
        mcp_tools: Vec::new(),
        delegates: Vec::new(),
        tool_policy: Some(String::from(DEFAULT_SAFE_TOOL_POLICY_ID)),
        routes: Vec::new(),
        fallback_delegate: None,
        model: None,
        temperature: None,
        max_turns: None,
        runtime_policy: ExecutionRuntimePolicyRequest::default(),
    }
}

/// Builds the system-default coordinator runtime definition used when the user does not select one custom agent.
/// 构建当用户未选择自定义 Agent 时使用的系统默认协调运行时定义。
fn default_coordinator_agent() -> RuntimeAgentDefinition {
    RuntimeAgentDefinition {
        id: String::from(WORKSPACE_COORDINATOR_AGENT_ID),
        name: String::from("系统默认（按模型）"),
        kind: RuntimeAgentKind::Coordinator,
        description: String::from("负责在常规代理、LuaSkills 代理与记忆代理之间进行确定性调度。"),
        system_prompt: String::new(),
        skill_ids: Vec::new(),
        native_tools: Vec::new(),
        plugin_tools: Vec::new(),
        mcp_tools: Vec::new(),
        delegates: vec![
            String::from(DEFAULT_WORKER_AGENT_ID),
            String::from(LUA_SPECIALIST_AGENT_ID),
            String::from(MEMORY_SPECIALIST_AGENT_ID),
        ],
        tool_policy: Some(String::from(DEFAULT_SAFE_TOOL_POLICY_ID)),
        routes: vec![
            RuntimeAgentRoute {
                signal: String::from("lua.request"),
                delegate: String::from(LUA_SPECIALIST_AGENT_ID),
            },
            RuntimeAgentRoute {
                signal: String::from("memory.recall"),
                delegate: String::from(MEMORY_SPECIALIST_AGENT_ID),
            },
        ],
        fallback_delegate: Some(String::from(DEFAULT_WORKER_AGENT_ID)),
        model: None,
        temperature: None,
        max_turns: None,
        runtime_policy: ExecutionRuntimePolicyRequest::default(),
    }
}

/// Builds one runtime agent definition from a user-authored profile by overlaying it on the system-default coordinator semantics.
/// 通过叠加系统默认协调代理语义，从用户自定义配置构建一条运行时代理定义。
fn runtime_agent_from_user_profile(profile: &UserAgentProfile) -> RuntimeAgentDefinition {
    let mut definition = default_coordinator_agent();
    definition.id = profile.id.clone();
    definition.name = profile.name.clone();
    definition.description = if profile.description.trim().is_empty() {
        String::from("基于用户自定义提示词的主代理。")
    } else {
        profile.description.clone()
    };
    definition.system_prompt = profile.prompt.clone();
    definition.model = profile.model.clone();
    definition.temperature = profile.temperature.or(definition.temperature);
    definition.max_turns = profile.max_turns.or(definition.max_turns);
    definition.skill_ids = profile.skill_ids.clone();
    definition.native_tools = profile.native_tools.clone();
    definition.plugin_tools = profile.plugin_tools.clone();
    definition.mcp_tools = profile.mcp_tools.clone();
    if profile.tool_policy.is_some() {
        definition.tool_policy = profile.tool_policy.clone();
    }
    definition.runtime_policy = profile.runtime_policy;
    definition
}

/// Loads one bundled default agent prompt and falls back to one short inline prompt when missing.
/// 加载一份内置默认代理提示词，并在缺失时回退到一条简短的内联提示词。
fn load_default_agent_prompt(file_name: &str, fallback_prompt: &str) -> String {
    let packaged_prompt_path = runtime_assets::resolve_app_resources_directory()
        .join(AGENT_PROMPTS_DIR_NAME)
        .join(file_name);
    if let Some(prompt) = read_non_empty_prompt_file(&packaged_prompt_path) {
        return prompt;
    }

    let source_prompt_path = runtime_assets::source_resource_directory()
        .join(AGENT_PROMPTS_DIR_NAME)
        .join(file_name);
    if source_prompt_path != packaged_prompt_path
        && let Some(prompt) = read_non_empty_prompt_file(&source_prompt_path)
    {
        return prompt;
    }

    fallback_prompt.to_string()
}

/// Reads one prompt text file and trims outer blank lines while preserving the inner content.
/// 读取单个提示词文本文件，并裁剪外围空白行同时保留内部正文。
fn read_non_empty_prompt_file(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        ANALYSIS_SAFE_TOOL_POLICY_ID, AgentRuntimeConfig, DEFAULT_SAFE_TOOL_POLICY_ID,
        DEFAULT_WORKER_AGENT_ID, EXPLORE_AGENT_ID, LUA_SAFE_TOOL_POLICY_ID, PLAN_AGENT_ID,
        ensure_runtime_defaults,
    };
    use crate::agent_profile::{AgentProfileCatalog, UserAgentProfile};
    use crate::provider_config::{PersistedToolPolicyEntry, ProviderConfigurationFile};
    use vulcan_session_core::ExecutionRuntimePolicyRequest;

    /// Verifies that the runtime seed always exposes the first-class plan and explore agents.
    /// 验证运行时种子始终会暴露一等的 plan 与 explore 代理。
    #[test]
    fn runtime_config_seeds_first_class_analysis_agents() {
        let runtime_config = AgentRuntimeConfig::from_sources(
            &ProviderConfigurationFile::default(),
            &AgentProfileCatalog::default(),
        );

        assert!(
            runtime_config
                .agents
                .iter()
                .any(|agent| agent.id == PLAN_AGENT_ID)
        );
        assert!(
            runtime_config
                .agents
                .iter()
                .any(|agent| agent.id == EXPLORE_AGENT_ID)
        );
    }

    /// Verifies built-in agents preserve provider sampling defaults instead of forcing temperature.
    /// 验证内置代理保留供应商采样默认值，而不是强制指定温度。
    #[test]
    fn runtime_config_builtin_agents_do_not_force_temperature() {
        let runtime_config = AgentRuntimeConfig::from_sources(
            &ProviderConfigurationFile::default(),
            &AgentProfileCatalog::default(),
        );

        assert!(
            runtime_config
                .agents
                .iter()
                .all(|agent| agent.temperature.is_none())
        );
    }

    /// Verifies requests without an explicit Agent use the stable built-in worker fallback.
    /// 验证未显式指定 Agent 的请求会使用稳定的内建 worker 回退项。
    #[test]
    fn runtime_config_defaults_to_builtin_worker_agent() {
        // runtime_config is built without any request-level Agent override.
        // runtime_config 在没有任何请求级 Agent 覆盖的情况下构建。
        let runtime_config = AgentRuntimeConfig::from_sources(
            &ProviderConfigurationFile::default(),
            &AgentProfileCatalog::default(),
        );

        assert_eq!(runtime_config.active_agent_id, DEFAULT_WORKER_AGENT_ID);
    }

    /// Verifies user-authored profiles project advanced tool and runtime-policy fields into runtime agents.
    /// 验证用户自定义配置会将高级工具与运行时策略字段投影到运行时代理。
    #[test]
    fn runtime_config_projects_user_profile_advanced_overrides() {
        let mut catalog = AgentProfileCatalog::default();
        catalog.agents.insert(
            String::from("agent-review"),
            UserAgentProfile {
                id: String::from("agent-review"),
                name: String::from("Review Agent"),
                description: String::from("Reviews code"),
                prompt: String::from("Review carefully."),
                model: Some(String::from("deepseek/deepseek-chat")),
                temperature: Some(0.3),
                max_turns: Some(8),
                skill_ids: vec![String::from("regular.review")],
                native_tools: vec![String::from("shell_command")],
                plugin_tools: vec![String::from("plugin_tool")],
                mcp_tools: vec![String::from("server.tool")],
                tool_policy: Some(String::from("analysis-safe")),
                runtime_policy: ExecutionRuntimePolicyRequest {
                    hooks_enabled: Some(true),
                    vmm_enabled: None,
                    memory_recall_enabled: Some(true),
                    memory_writeback_enabled: None,
                    database_projection_enabled: Some(true),
                },
            },
        );

        let runtime_config =
            AgentRuntimeConfig::from_sources(&ProviderConfigurationFile::default(), &catalog);
        let agent = runtime_config
            .agents
            .iter()
            .find(|agent| agent.id == "agent-review")
            .expect("user agent should project");

        assert_eq!(agent.skill_ids, vec![String::from("regular.review")]);
        assert_eq!(agent.native_tools, vec![String::from("shell_command")]);
        assert_eq!(agent.mcp_tools, vec![String::from("server.tool")]);
        assert_eq!(agent.tool_policy.as_deref(), Some("analysis-safe"));
        assert_eq!(agent.runtime_policy.hooks_enabled, Some(true));
        assert_eq!(agent.runtime_policy.memory_recall_enabled, Some(true));
        assert_eq!(agent.runtime_policy.database_projection_enabled, Some(true));
    }

    /// Verifies that runtime projection injects reserved tool-policy defaults in memory when the persisted configuration omits them.
    /// 验证当持久化配置缺失保留工具策略时，运行时投影会在内存中注入这些默认策略。
    #[test]
    fn runtime_config_injects_reserved_tool_policy_defaults_in_memory() {
        let runtime_config = AgentRuntimeConfig::from_sources(
            &ProviderConfigurationFile::default(),
            &AgentProfileCatalog::default(),
        );

        let default_safe = runtime_config
            .tool_policies
            .get(DEFAULT_SAFE_TOOL_POLICY_ID)
            .expect("default-safe runtime policy should exist");
        assert!(default_safe.allow_all_native);

        let analysis_safe = runtime_config
            .tool_policies
            .get(ANALYSIS_SAFE_TOOL_POLICY_ID)
            .expect("analysis-safe runtime policy should exist");
        assert_eq!(
            analysis_safe.native_tools,
            vec![
                String::from("file"),
                String::from("shell_command"),
                String::from("terminal_session"),
                String::from("vmm_memory_search"),
                String::from("vmm_turn_details"),
            ]
        );

        let lua_safe = runtime_config
            .tool_policies
            .get(LUA_SAFE_TOOL_POLICY_ID)
            .expect("lua-safe runtime policy should exist");
        assert_eq!(
            lua_safe.native_tools,
            vec![
                String::from("shell_command"),
                String::from("terminal_session"),
                String::from("vmm_memory_search"),
                String::from("vmm_turn_details"),
            ]
        );
    }

    /// Verifies that persisted reserved tool policies override runtime-only defaults instead of being merged or rewritten.
    /// 验证持久化的保留工具策略会覆盖仅运行时默认值，而不会被合并或改写。
    #[test]
    fn runtime_config_prefers_persisted_reserved_tool_policy_entries() {
        let mut config = ProviderConfigurationFile::default();
        config.tool_policies.insert(
            String::from(DEFAULT_SAFE_TOOL_POLICY_ID),
            PersistedToolPolicyEntry {
                name: Some(String::from("用户默认策略")),
                allow_all_native: false,
                native_tools: vec![String::from("shell_command")],
                allow_all_plugin_tools: false,
                plugin_tools: Vec::new(),
                mcp_servers: Vec::new(),
                mcp_tools: Vec::new(),
            },
        );

        let runtime_config =
            AgentRuntimeConfig::from_sources(&config, &AgentProfileCatalog::default());
        let default_safe = runtime_config
            .tool_policies
            .get(DEFAULT_SAFE_TOOL_POLICY_ID)
            .expect("persisted default-safe runtime policy should exist");

        assert_eq!(default_safe.name, "用户默认策略");
        assert!(!default_safe.allow_all_native);
        assert_eq!(
            default_safe.native_tools,
            vec![String::from("shell_command")]
        );
    }

    /// Verifies runtime default seeding does not auto-create tool policies because policy content must stay under explicit user control.
    /// 验证运行时默认注入不会自动创建工具策略，因为策略内容必须保持在用户显式控制之下。
    #[test]
    fn runtime_defaults_do_not_seed_tool_policies() {
        let mut config = ProviderConfigurationFile::default();

        assert!(ensure_runtime_defaults(&mut config));
        assert!(config.tool_policies.is_empty());
    }

    /// Verifies runtime default seeding leaves existing tool-policy entries untouched so user-authored policy state is never silently rewritten.
    /// 验证运行时默认注入会保持现有工具策略条目不变，避免静默改写用户自定义策略状态。
    #[test]
    fn runtime_defaults_leave_existing_tool_policies_untouched() {
        let mut config = ProviderConfigurationFile::default();
        config.tool_policies.insert(
            String::from("custom-policy"),
            PersistedToolPolicyEntry {
                name: Some(String::from("自定义策略")),
                allow_all_native: false,
                native_tools: vec![String::from("shell_command")],
                allow_all_plugin_tools: false,
                plugin_tools: Vec::new(),
                mcp_servers: vec![String::from("server-a")],
                mcp_tools: vec![String::from("tool-a")],
            },
        );

        assert!(ensure_runtime_defaults(&mut config));

        let policy = config
            .tool_policies
            .get("custom-policy")
            .expect("custom tool policy should remain");
        assert_eq!(policy.name.as_deref(), Some("自定义策略"));
        assert!(!policy.allow_all_native);
        assert_eq!(policy.native_tools, vec![String::from("shell_command")]);
        assert_eq!(policy.mcp_servers, vec![String::from("server-a")]);
        assert_eq!(policy.mcp_tools, vec![String::from("tool-a")]);
    }
}
