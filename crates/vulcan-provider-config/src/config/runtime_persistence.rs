//! Runtime persistence models stored inside the shared provider configuration file.
//! 存储在共享 Provider 配置文件中的运行时持久化模型。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use vulcan_vmm::VmmConfigLayer;

/// Returns the default enabled state used by persisted runtime entries.
/// 返回持久化运行时条目使用的默认启用状态。
fn default_true() -> bool {
    true
}
/// Persisted skill family used by the runtime foundation.
/// 运行时基础架构使用的持久化技能类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedSkillKind {
    /// General built-in skill chain.
    /// 常规内建技能链。
    Regular,
    /// LuaSkills-backed capability entry.
    /// 基于 LuaSkills 的能力条目。
    LuaSkills,
    /// Automation-oriented skill entry.
    /// 面向自动化的技能条目。
    Automation,
    /// Memory-oriented skill entry.
    /// 面向记忆体系的技能条目。
    Memory,
}

/// One persisted skill entry declared inside the shared Vulcan configuration file.
/// 共享 Vulcan 配置文件中声明的单个持久化技能条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSkillEntry {
    /// Human-readable skill name.
    /// 人类可读的技能名称。
    pub name: String,
    /// Skill family used to route the definition into the right registry bucket.
    /// 用于将定义路由到正确注册表桶中的技能类型。
    pub kind: PersistedSkillKind,
    /// Optional skill description shown in configuration surfaces.
    /// 展示在配置界面中的可选技能描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the skill is currently enabled.
    /// 当前技能是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Signals that make the skill applicable.
    /// 使技能生效的信号集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trigger_signals: Vec<String>,
    /// Optional signal emitted by the skill after execution.
    /// 技能执行后可选地发出的信号。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emitted_signal: Option<String>,
    /// Confidence delta contributed by the skill.
    /// 技能贡献的置信度增量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_delta: Option<f32>,
}

/// One persisted tool-policy entry that scopes native, Vulcan Plugin, and MCP tools.
/// 用于限定原生工具、Vulcan 插件工具与 MCP 工具的单个持久化工具策略条目。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedToolPolicyEntry {
    /// Human-readable policy name.
    /// 人类可读的策略名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether every native tool should be exposed to the bound agent.
    /// 是否向绑定代理暴露全部原生工具。
    #[serde(default)]
    pub allow_all_native: bool,
    /// Explicitly allowed native tool identifiers.
    /// 显式允许的原生工具标识集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_tools: Vec<String>,
    /// Whether every enabled Vulcan Plugin tool group should be exposed to the bound agent.
    /// 是否向绑定代理暴露全部已启用 Vulcan 插件工具组。
    #[serde(default)]
    pub allow_all_plugin_tools: bool,
    /// Explicitly allowed Vulcan Plugin tool identifiers.
    /// 显式允许的 Vulcan 插件工具标识集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugin_tools: Vec<String>,
    /// Explicitly allowed MCP server identifiers.
    /// 显式允许的 MCP 服务器标识集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,
    /// Explicitly allowed MCP tool identifiers.
    /// 显式允许的 MCP 工具标识集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_tools: Vec<String>,
}

/// Persisted registration mode for one LuaSkills runtime tool.
/// 单个 LuaSkills 运行时工具的持久化注册模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PersistedLuaSkillToolRegistrationState {
    /// The tool is enabled and registered as an ordinary provider-visible function tool.
    /// 工具已启用，并作为普通 Provider 可见函数工具注册。
    #[default]
    Explicit,
    /// The tool is enabled for host-side dispatch but hidden from provider-visible function schemas.
    /// 工具已启用供宿主侧调度，但不暴露到 Provider 可见函数 Schema。
    Implicit,
    /// The tool is disabled for both direct provider registration and host-side dispatch.
    /// 工具对直接 Provider 注册与宿主侧调度均禁用。
    Disabled,
}

impl fmt::Display for PersistedLuaSkillToolRegistrationState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Explicit => "显式启用",
            Self::Implicit => "隐式启用",
            Self::Disabled => "禁用",
        };
        formatter.write_str(label)
    }
}

impl PersistedLuaSkillToolRegistrationState {
    /// Returns whether this state should register the tool as a provider-visible function tool.
    /// 返回该状态是否应将工具注册为 Provider 可见函数工具。
    pub fn is_explicit(self) -> bool {
        matches!(self, Self::Explicit)
    }
}

/// Host-managed LuaSkills tool registration configuration persisted independently from package lifecycle state.
/// 与技能包生命周期状态独立持久化的宿主管理 LuaSkills 工具注册配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedLuaSkillsConfiguration {
    /// Non-default LuaSkills registration states grouped by stable skill identifier and local tool name.
    /// 按稳定技能标识与本地工具名分组保存的非默认 LuaSkills 注册状态。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tool_registration_by_skill:
        BTreeMap<String, BTreeMap<String, PersistedLuaSkillToolRegistrationState>>,
    /// Legacy hidden LuaSkills local tool names grouped by stable skill identifier.
    /// 按稳定技能标识分组保存的旧版隐藏 LuaSkills 本地工具名称集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub disabled_tools_by_skill: BTreeMap<String, Vec<String>>,
    /// Host-owned Vulcan Plugin identifiers explicitly disabled by the user.
    /// 用户显式停用的宿主自有 Vulcan Plugin 标识集合。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled_vulcan_plugins: BTreeSet<String>,
    /// Host-owned Vulcan Plugin identifiers explicitly enabled over a disabled manifest default.
    /// 在清单默认停用基础上由用户显式启用的宿主自有 Vulcan Plugin 标识集合。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub enabled_vulcan_plugins: BTreeSet<String>,
}

impl PersistedLuaSkillsConfiguration {
    /// Returns whether the current LuaSkills registration configuration is still empty.
    /// 返回当前 LuaSkills 注册配置是否仍为空。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Normalizes the persisted LuaSkills registration map and migrates legacy hidden tools to implicit registration.
    /// 归一化持久化的 LuaSkills 注册映射，并将旧版隐藏工具迁移为隐式注册。
    pub fn normalize(&mut self) -> bool {
        let legacy_implicit_map = self
            .disabled_tools_by_skill
            .iter()
            .filter_map(|(skill_id, local_names)| {
                let stable_skill_id = skill_id.trim();
                if stable_skill_id.is_empty() {
                    return None;
                }

                let normalized_local_names = local_names
                    .iter()
                    .map(|local_name| local_name.trim())
                    .filter(|local_name| !local_name.is_empty())
                    .map(String::from)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();

                if normalized_local_names.is_empty() {
                    None
                } else {
                    Some((stable_skill_id.to_string(), normalized_local_names))
                }
            })
            .collect::<BTreeMap<_, _>>();

        let mut normalized_registration = self
            .tool_registration_by_skill
            .iter()
            .filter_map(|(skill_id, local_states)| {
                let stable_skill_id = skill_id.trim();
                if stable_skill_id.is_empty() {
                    return None;
                }

                let normalized_states = local_states
                    .iter()
                    .filter_map(|(local_name, state)| {
                        let stable_local_name = local_name.trim();
                        if stable_local_name.is_empty()
                            || *state == PersistedLuaSkillToolRegistrationState::Explicit
                        {
                            None
                        } else {
                            Some((stable_local_name.to_string(), *state))
                        }
                    })
                    .collect::<BTreeMap<_, _>>();

                if normalized_states.is_empty() {
                    None
                } else {
                    Some((stable_skill_id.to_string(), normalized_states))
                }
            })
            .collect::<BTreeMap<_, _>>();

        for (skill_id, local_names) in legacy_implicit_map {
            let registration_entry = normalized_registration.entry(skill_id).or_default();
            for local_name in local_names {
                registration_entry
                    .entry(local_name)
                    .or_insert(PersistedLuaSkillToolRegistrationState::Implicit);
            }
        }

        normalized_registration.retain(|_, local_states| !local_states.is_empty());
        let normalized_disabled_vulcan_plugins = self
            .disabled_vulcan_plugins
            .iter()
            .map(|plugin_id| plugin_id.trim())
            .filter(|plugin_id| !plugin_id.is_empty())
            .map(String::from)
            .collect::<BTreeSet<_>>();
        let normalized_enabled_vulcan_plugins = self
            .enabled_vulcan_plugins
            .iter()
            .map(|plugin_id| plugin_id.trim())
            .filter(|plugin_id| !plugin_id.is_empty())
            .map(String::from)
            .collect::<BTreeSet<_>>();
        let normalized_disabled_vulcan_plugins = normalized_disabled_vulcan_plugins
            .difference(&normalized_enabled_vulcan_plugins)
            .cloned()
            .collect::<BTreeSet<_>>();
        let changed = normalized_registration != self.tool_registration_by_skill
            || !self.disabled_tools_by_skill.is_empty()
            || normalized_disabled_vulcan_plugins != self.disabled_vulcan_plugins
            || normalized_enabled_vulcan_plugins != self.enabled_vulcan_plugins;
        self.tool_registration_by_skill = normalized_registration;
        self.disabled_tools_by_skill.clear();
        self.disabled_vulcan_plugins = normalized_disabled_vulcan_plugins;
        self.enabled_vulcan_plugins = normalized_enabled_vulcan_plugins;
        changed
    }

    /// Resolves one plugin state from explicit overrides and its manifest default.
    /// 根据显式覆盖与清单默认值解析单个插件状态。
    pub fn vulcan_plugin_enabled_or_default(&self, plugin_id: &str, default_enabled: bool) -> bool {
        let stable_plugin_id = plugin_id.trim();
        if stable_plugin_id.is_empty() {
            return false;
        }
        if self.enabled_vulcan_plugins.contains(stable_plugin_id) {
            true
        } else if self.disabled_vulcan_plugins.contains(stable_plugin_id) {
            false
        } else {
            default_enabled
        }
    }

    /// Stores a plugin override relative to the package manifest default.
    /// 保存相对于插件包清单默认值的插件覆盖。
    pub fn set_vulcan_plugin_enabled_with_default(
        &mut self,
        plugin_id: &str,
        enabled: bool,
        default_enabled: bool,
    ) -> bool {
        let stable_plugin_id = plugin_id.trim();
        if stable_plugin_id.is_empty() {
            return false;
        }
        let before_enabled = self.enabled_vulcan_plugins.clone();
        let before_disabled = self.disabled_vulcan_plugins.clone();
        self.enabled_vulcan_plugins.remove(stable_plugin_id);
        self.disabled_vulcan_plugins.remove(stable_plugin_id);
        if enabled != default_enabled {
            if enabled {
                self.enabled_vulcan_plugins
                    .insert(stable_plugin_id.to_string());
            } else {
                self.disabled_vulcan_plugins
                    .insert(stable_plugin_id.to_string());
            }
        }
        before_enabled != self.enabled_vulcan_plugins
            || before_disabled != self.disabled_vulcan_plugins
    }

    /// Returns the effective registration state for one LuaSkills local tool.
    /// 返回单个 LuaSkills 本地工具的有效注册状态。
    pub fn tool_registration_state(
        &self,
        skill_id: &str,
        local_name: &str,
    ) -> PersistedLuaSkillToolRegistrationState {
        let stable_skill_id = skill_id.trim();
        let stable_local_name = local_name.trim();
        if stable_skill_id.is_empty() || stable_local_name.is_empty() {
            return PersistedLuaSkillToolRegistrationState::Explicit;
        }

        if let Some(state) = self
            .tool_registration_by_skill
            .get(stable_skill_id)
            .and_then(|local_states| local_states.get(stable_local_name))
            .copied()
        {
            return state;
        }

        if self
            .disabled_tools_by_skill
            .get(stable_skill_id)
            .is_some_and(|local_names| local_names.iter().any(|name| name == stable_local_name))
        {
            return PersistedLuaSkillToolRegistrationState::Implicit;
        }

        PersistedLuaSkillToolRegistrationState::Explicit
    }

    /// Returns whether one LuaSkills local tool should be registered as a provider-visible function tool.
    /// 返回单个 LuaSkills 本地工具是否应注册为 Provider 可见函数工具。
    pub fn is_tool_explicitly_registered(&self, skill_id: &str, local_name: &str) -> bool {
        self.tool_registration_state(skill_id, local_name)
            .is_explicit()
    }

    /// Updates one LuaSkills local tool registration state while persisting only non-default states.
    /// 更新单个 LuaSkills 本地工具的注册状态，并且只持久化非默认状态。
    pub fn set_tool_registration_state(
        &mut self,
        skill_id: &str,
        local_name: &str,
        state: PersistedLuaSkillToolRegistrationState,
    ) -> bool {
        let stable_skill_id = skill_id.trim();
        let stable_local_name = local_name.trim();
        if stable_skill_id.is_empty() || stable_local_name.is_empty() {
            return false;
        }

        let previous = self.tool_registration_state(stable_skill_id, stable_local_name);
        if state == PersistedLuaSkillToolRegistrationState::Explicit {
            if let Some(entry) = self.tool_registration_by_skill.get_mut(stable_skill_id) {
                entry.remove(stable_local_name);
                if entry.is_empty() {
                    self.tool_registration_by_skill.remove(stable_skill_id);
                }
            }
        } else {
            self.tool_registration_by_skill
                .entry(stable_skill_id.to_string())
                .or_default()
                .insert(stable_local_name.to_string(), state);
        }

        if let Some(entry) = self.disabled_tools_by_skill.get_mut(stable_skill_id) {
            entry.retain(|name| name != stable_local_name);
            if entry.is_empty() {
                self.disabled_tools_by_skill.remove(stable_skill_id);
            }
        }

        let changed = previous != state;
        changed | self.normalize()
    }

    /// Updates one legacy LuaSkills hidden-tool entry, mapping hidden to implicit registration for compatibility.
    /// 更新单个旧版 LuaSkills 隐藏工具条目，并为了兼容将隐藏映射为隐式注册。
    pub fn set_tool_disabled(&mut self, skill_id: &str, local_name: &str, disabled: bool) -> bool {
        let state = if disabled {
            PersistedLuaSkillToolRegistrationState::Implicit
        } else {
            PersistedLuaSkillToolRegistrationState::Explicit
        };
        self.set_tool_registration_state(skill_id, local_name, state)
    }
}

/// Persisted on/off state for one prompt skill.
/// 单个提示词技能的持久化开关状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PersistedPromptSkillState {
    /// The prompt skill is visible and loadable.
    /// 提示词技能可见且可加载。
    Enabled,
    /// The prompt skill is hidden and cannot be loaded.
    /// 提示词技能隐藏且不可加载。
    Disabled,
}

impl PersistedPromptSkillState {
    /// Returns whether this persisted state enables one prompt skill.
    /// 返回该持久化状态是否启用某个提示词技能。
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Host-managed prompt skills configuration stored independently from LuaSkills.
/// 独立于 LuaSkills 存储的宿主管理提示词技能配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedPromptSkillsConfiguration {
    /// Default state used when no per-skill override exists.
    /// 不存在单技能覆盖时使用的默认状态。
    #[serde(default = "default_prompt_skill_state")]
    pub default_skill_state: PersistedPromptSkillState,
    /// Global per-skill state overrides.
    /// 全局单技能状态覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skill_overrides: BTreeMap<String, PersistedPromptSkillState>,
}

impl Default for PersistedPromptSkillsConfiguration {
    /// Builds the default prompt skills configuration.
    /// 构建默认提示词技能配置。
    fn default() -> Self {
        Self {
            default_skill_state: PersistedPromptSkillState::Enabled,
            skill_overrides: BTreeMap::new(),
        }
    }
}

/// Project-local prompt skills configuration that only stores per-skill overrides.
/// 仅存储单技能覆盖的项目级提示词技能配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPromptSkillsConfiguration {
    /// Project-level per-skill state overrides.
    /// 项目级单技能状态覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skill_overrides: BTreeMap<String, PersistedPromptSkillState>,
}

impl ProjectPromptSkillsConfiguration {
    /// Returns whether the current project prompt skills configuration is empty.
    /// 返回当前项目级提示词技能配置是否为空。
    pub fn is_empty(&self) -> bool {
        self.skill_overrides.is_empty()
    }
}

impl PersistedPromptSkillsConfiguration {
    /// Returns whether the current prompt skills configuration is still default.
    /// 返回当前提示词技能配置是否仍为默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Normalizes the persisted prompt skill overrides by trimming names and dropping empty entries.
    /// 通过裁剪名称并移除空条目来归一化持久化提示词技能覆盖配置。
    pub fn normalize(&mut self) -> bool {
        let normalized = self
            .skill_overrides
            .iter()
            .filter_map(|(name, state)| {
                let stable_name = name.trim();
                (!stable_name.is_empty()).then(|| (stable_name.to_string(), *state))
            })
            .collect::<BTreeMap<_, _>>();
        let changed = normalized != self.skill_overrides;
        self.skill_overrides = normalized;
        changed
    }

    /// Updates one global prompt skill state override.
    /// 更新单个全局提示词技能状态覆盖。
    pub fn set_skill_state(
        &mut self,
        name: &str,
        state: Option<PersistedPromptSkillState>,
    ) -> bool {
        let stable_name = name.trim();
        if stable_name.is_empty() {
            return false;
        }

        let changed = match state {
            Some(state) => {
                self.skill_overrides.insert(stable_name.to_string(), state) != Some(state)
            }
            None => self.skill_overrides.remove(stable_name).is_some(),
        };
        changed | self.normalize()
    }
}

/// Returns the default prompt skill state.
/// 返回默认提示词技能状态。
fn default_prompt_skill_state() -> PersistedPromptSkillState {
    PersistedPromptSkillState::Enabled
}

/// Supported MCP transport families persisted inside the shared configuration file.
/// 持久化到共享配置文件中的受支持 MCP 传输类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedMcpTransportKind {
    /// Standard input/output child-process transport.
    /// 标准输入输出子进程传输。
    Stdio,
    /// Streamable HTTP client transport.
    /// Streamable HTTP 客户端传输。
    Http,
    /// Legacy server-sent-events MCP transport.
    /// 旧式基于 Server-Sent Events 的 MCP 传输。
    Sse,
}

impl PersistedMcpTransportKind {
    /// Returns the human-readable transport label used by UI surfaces.
    /// 返回 UI 界面使用的人类可读传输标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
}

impl fmt::Display for PersistedMcpTransportKind {
    /// Renders the transport kind into one compact UI label.
    /// 将传输类型渲染为紧凑 UI 标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// One persisted MCP server entry stored in the shared configuration file.
/// 存储在共享配置文件中的单个持久化 MCP 服务器条目。
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedMcpOAuthConfig {
    /// Optional pre-registered OAuth client identifier.
    /// 可选的预注册 OAuth 客户端标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Optional pre-registered OAuth client secret.
    /// 可选的预注册 OAuth 客户端密钥。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Optional OAuth scopes requested during authorization.
    /// 授权过程中请求的可选 OAuth scopes 集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Optional redirect URI used by the OAuth authorization-code flow.
    /// OAuth 授权码流程使用的可选回调 URI。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
}

/// One persisted MCP server entry stored in the shared configuration file.
/// 存储在共享配置文件中的单个持久化 MCP 服务器条目。
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedMcpServerEntry {
    /// Human-readable server name.
    /// 人类可读的服务器名称。
    pub name: String,
    /// Whether the server is enabled for runtime registration.
    /// 该服务器是否对运行时注册启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Transport family used to start the server.
    /// 用于启动该服务器的传输类型。
    pub transport: PersistedMcpTransportKind,
    /// Command used to launch the server process.
    /// 用于启动服务器进程的命令。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    /// Argument vector passed to the server process.
    /// 传递给服务器进程的参数列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Optional remote MCP endpoint URL used by HTTP transports.
    /// HTTP 传输使用的可选远端 MCP 端点 URL。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Optional bearer token injected into HTTP MCP requests.
    /// 注入到 HTTP MCP 请求中的可选 Bearer Token。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
    /// Optional HTTP headers injected into remote MCP requests.
    /// 注入到远端 MCP 请求中的可选 HTTP 请求头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// Optional HTTP headers whose values should be resolved from current process environment variables.
    /// 值需要从当前进程环境变量解析的可选 HTTP 请求头映射。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers_from_env: BTreeMap<String, String>,
    /// Optional OAuth configuration used by host-driven remote authorization flows.
    /// 宿主驱动远端授权流程使用的可选 OAuth 配置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<PersistedMcpOAuthConfig>,
    /// Optional working directory used for the server process.
    /// 服务器进程使用的可选工作目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Optional environment variables injected into the server process.
    /// 注入到服务器进程中的可选环境变量集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Optional environment-variable names that should be passed through from the current host process when they are present.
    /// 当前宿主进程中存在时应被透传的可选环境变量名称集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_passthrough: Vec<String>,
}

impl fmt::Debug for PersistedMcpServerEntry {
    /// Formats the server topology while redacting the token, OAuth secret, header and environment values.
    /// 格式化服务器拓扑，同时脱敏 Token、OAuth 密钥、请求头与环境变量值。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives only key names and presence flags for credential-bearing values.
    /// `formatter` 对承载凭据的值只接收键名与存在性标记。
    ///
    /// # Returns
    /// 返回值
    /// Returns the formatter result without any secret value.
    /// 返回不含任何秘密值的格式化结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistedMcpServerEntry")
            .field("name", &self.name)
            .field("enabled", &self.enabled)
            .field("transport", &self.transport)
            .field("command", &self.command)
            .field("argument_count", &self.args.len())
            .field("url", &self.url)
            .field("has_bearer_token", &self.bearer_token.is_some())
            .field("header_keys", &self.headers.keys().collect::<Vec<_>>())
            .field("headers_from_env", &self.headers_from_env)
            .field(
                "oauth",
                &self.oauth.as_ref().map(|oauth| {
                    (
                        oauth.client_id.as_deref(),
                        oauth.client_secret.is_some(),
                        &oauth.scopes,
                        oauth.redirect_uri.as_deref(),
                    )
                }),
            )
            .field("cwd", &self.cwd)
            .field("environment_keys", &self.env.keys().collect::<Vec<_>>())
            .field("env_passthrough", &self.env_passthrough)
            .finish()
    }
}

/// MCP configuration block persisted inside the shared Vulcan configuration file.
/// 持久化在共享 Vulcan 配置文件中的 MCP 配置块。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedMcpConfiguration {
    /// MCP server definitions keyed by stable identifier.
    /// 以稳定标识为键的 MCP 服务器定义集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub servers: BTreeMap<String, PersistedMcpServerEntry>,
}

impl PersistedMcpConfiguration {
    /// Returns whether the MCP configuration block is effectively empty.
    /// 返回 MCP 配置块是否实际上为空。
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

/// Prompt-composer personality mode persisted in the shared Vulcan file.
/// 持久化在共享 Vulcan 文件中的提示词组合人格模式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedPromptPersonalitySelection {
    /// Use the built-in default personality.
    /// 使用内置默认人格。
    Default,
    /// Use one named personality from built-in or user resources.
    /// 使用来自内置或用户资源的具名人格。
    Named(String),
    /// Disable personality injection.
    /// 禁用人格注入。
    Disabled,
}

impl Default for PersistedPromptPersonalitySelection {
    /// Returns the user-friendly built-in default personality selection.
    /// 返回用户友好的内置默认人格选择。
    fn default() -> Self {
        Self::Default
    }
}

/// Persisted output-style selection used as the default for new conversations.
/// 用作新会话默认值的持久化输出风格选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedPromptOutputStyleSelection {
    /// Disable generic output-style instructions.
    /// 禁用通用输出风格指令。
    Unrestricted,
    /// Concise general-purpose output style.
    /// 简洁通用输出风格。
    Balanced,
    /// Structured technical-report output style.
    /// 分层技术报告输出风格。
    Structured,
    /// Strict engineering-audit output style.
    /// 严格工程审计输出风格。
    Rigorous,
}

impl Default for PersistedPromptOutputStyleSelection {
    /// Returns the concise general-purpose default.
    /// 返回简洁通用默认值。
    fn default() -> Self {
        Self::Balanced
    }
}

impl PersistedPromptOutputStyleSelection {
    /// Parses one stable catalog identifier.
    /// 解析一个稳定目录标识。
    pub fn from_id(value: &str) -> Option<Self> {
        match value.trim() {
            "unrestricted" => Some(Self::Unrestricted),
            "balanced" => Some(Self::Balanced),
            "structured" => Some(Self::Structured),
            "rigorous" => Some(Self::Rigorous),
            _ => None,
        }
    }

    /// Returns the stable catalog identifier.
    /// 返回稳定目录标识。
    pub const fn id(self) -> &'static str {
        match self {
            Self::Unrestricted => "unrestricted",
            Self::Balanced => "balanced",
            Self::Structured => "structured",
            Self::Rigorous => "rigorous",
        }
    }

    /// Returns whether this selection is the persisted default.
    /// 返回该选择是否为持久化默认值。
    pub const fn is_default(&self) -> bool {
        matches!(self, Self::Balanced)
    }

    /// Returns the compact Chinese selector label.
    /// 返回紧凑中文选择器名称。
    pub const fn label_zh(self) -> &'static str {
        match self {
            Self::Unrestricted => "不限定",
            Self::Balanced => "清晰均衡",
            Self::Structured => "分层报告",
            Self::Rigorous => "工程严谨",
        }
    }
}

/// Persisted answer mode copied into each new conversation.
/// 复制到每个新会话的持久化回答模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedAnswerModeSelection {
    /// Ordinary Markdown answers.
    /// 普通 Markdown 回答。
    #[default]
    Standard,
    /// Markdown plus native answer components.
    /// Markdown 加原生回答组件。
    Interactive,
}

impl PersistedAnswerModeSelection {
    /// Returns whether this selection is the persisted default.
    /// 返回该选择是否为持久化默认值。
    pub const fn is_default(&self) -> bool {
        matches!(self, Self::Standard)
    }
}

/// Prompt-composer configuration persisted in the shared Vulcan file.
/// 持久化在共享 Vulcan 文件中的提示词组合配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedPromptConfiguration {
    /// Personality selection used by the main runtime prompt composer.
    /// 主运行时提示词组合器使用的人格选择。
    #[serde(
        default,
        skip_serializing_if = "PersistedPromptPersonalitySelection::is_default"
    )]
    pub personality: PersistedPromptPersonalitySelection,
    /// Default output style applied when a new conversation is materialized.
    /// 新会话物化时使用的默认输出风格。
    #[serde(
        default,
        skip_serializing_if = "PersistedPromptOutputStyleSelection::is_default"
    )]
    pub output_style: PersistedPromptOutputStyleSelection,
    /// Default answer mode applied when a new conversation is materialized.
    /// 新会话物化时使用的默认回答模式。
    #[serde(
        default,
        skip_serializing_if = "PersistedAnswerModeSelection::is_default"
    )]
    pub answer_mode: PersistedAnswerModeSelection,
}

impl Default for PersistedPromptConfiguration {
    /// Builds the default prompt-composer configuration.
    /// 构建默认提示词组合配置。
    fn default() -> Self {
        Self {
            personality: PersistedPromptPersonalitySelection::Default,
            output_style: PersistedPromptOutputStyleSelection::Balanced,
            answer_mode: PersistedAnswerModeSelection::Standard,
        }
    }
}

#[cfg(test)]
mod vulcan_plugin_policy_tests {
    use super::*;

    /// Verifies the dedicated Vulcan Plugin policy dimension survives serialization round trips.
    /// 验证专用 Vulcan 插件策略维度可在序列化往返后保持不变。
    #[test]
    fn vulcan_plugin_policy_fields_round_trip_without_native_tool_fallback() {
        let policy = PersistedToolPolicyEntry {
            name: Some(String::from("Plugin policy")),
            allow_all_native: false,
            native_tools: vec![String::from("shell_command")],
            allow_all_plugin_tools: true,
            plugin_tools: vec![String::from("plugin_tool_a")],
            mcp_servers: Vec::new(),
            mcp_tools: Vec::new(),
        };
        let encoded = serde_json::to_string(&policy).expect("encode policy");
        let decoded: PersistedToolPolicyEntry =
            serde_json::from_str(&encoded).expect("decode policy");
        assert!(decoded.allow_all_plugin_tools);
        assert_eq!(decoded.plugin_tools, vec![String::from("plugin_tool_a")]);
        assert_eq!(decoded.native_tools, vec![String::from("shell_command")]);
    }

    /// Verifies a minimal policy document defaults an omitted plugin dimension to closed.
    /// 验证最小策略文档会将省略的插件维度默认设为关闭。
    #[test]
    fn minimal_policy_document_defaults_omitted_plugin_dimension_to_closed() {
        let decoded: PersistedToolPolicyEntry =
            serde_json::from_str(r#"{"allowAllNative":true,"nativeTools":["shell_command"]}"#)
                .expect("decode legacy policy");
        assert!(!decoded.allow_all_plugin_tools);
        assert!(decoded.plugin_tools.is_empty());
    }
}

impl PersistedPromptPersonalitySelection {
    /// Returns whether the personality selection is still the default value.
    /// 返回人格选择是否仍为默认值。
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default)
    }

    /// Returns the compact settings label for this personality selection.
    /// 返回该人格选择在设置中使用的紧凑标签。
    pub fn label(&self) -> String {
        match self {
            Self::Default => String::from("亲和（默认）"),
            Self::Named(id) => format!("自定义：{id}"),
            Self::Disabled => String::from("禁用人格"),
        }
    }
}

impl PersistedPromptConfiguration {
    /// Returns whether the prompt configuration still equals built-in defaults.
    /// 返回提示词配置是否仍等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Project-local Vulcan Code configuration saved under one workspace.
/// 保存到单个工作区下的项目级 Vulcan Code 配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVulcanCodeConfigurationFile {
    /// Optional project-local prompt skills overrides.
    /// 可选的项目级提示词技能覆盖配置。
    #[serde(
        default,
        skip_serializing_if = "ProjectPromptSkillsConfiguration::is_empty"
    )]
    pub prompt_skills: ProjectPromptSkillsConfiguration,
    /// Optional project-local VMM memory integration overrides.
    /// 可选的项目级 VMM 记忆集成覆盖配置。
    #[serde(default, skip_serializing_if = "VmmConfigLayer::is_empty")]
    pub vmm: VmmConfigLayer,
}
