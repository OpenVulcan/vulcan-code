//! Rig-core integration scaffold and orchestration-oriented runtime abstractions.
//! Rig-core 接入骨架与面向编排的运行时抽象。

use crate::model::Provider;

/// One independently togglable prompt node reserved for future role composition.
/// 为未来角色编排预留的单个可独立开关提示节点。
#[derive(Debug, Clone)]
pub struct PromptNodeDefinition {
    /// Stable prompt node identifier.
    /// 稳定的提示节点标识。
    pub id: &'static str,
    /// Short prompt node title shown in future configuration views.
    /// 展示在未来配置视图中的提示节点短标题。
    pub title: &'static str,
    /// Prompt node category used during composition.
    /// 在提示词拼装阶段使用的节点类别。
    pub kind: PromptNodeKind,
    /// Whether the prompt node is enabled by default.
    /// 该提示节点是否默认启用。
    pub enabled: bool,
    /// Raw prompt fragment contributed by the node.
    /// 由该节点贡献的原始提示词片段。
    pub content: &'static str,
}

/// Prompt node kinds used to assemble role-specific system instructions.
/// 用于拼装角色专属系统提示词的提示节点类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptNodeKind {
    /// Identity and mission framing.
    /// 身份与任务定位框架。
    Identity,
    /// Hard engineering or safety constraints.
    /// 强制工程约束或安全约束。
    Constraint,
    /// Runtime context and environment hints.
    /// 运行时上下文与环境提示。
    Context,
    /// Output contract and formatting expectations.
    /// 输出契约与格式预期。
    OutputContract,
}

impl std::fmt::Display for PromptNodeKind {
    /// Renders the user-facing prompt node kind label.
    /// 渲染面向用户的提示节点类型标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Identity => "Identity",
            Self::Constraint => "Constraint",
            Self::Context => "Context",
            Self::OutputContract => "Output Contract",
        };

        formatter.write_str(label)
    }
}

/// Tool policies reserved for future per-role capability filtering.
/// 为未来按角色过滤能力预留的工具策略。
#[derive(Debug, Clone)]
pub enum ToolPolicy {
    /// Uses the active workbench tool surface as-is.
    /// 原样继承当前工作台工具面。
    InheritWorkbench,
    /// Restricts the role to the given tool-group allowlist.
    /// 将角色限制在给定的工具分组白名单内。
    AllowGroups(Vec<&'static str>),
    /// Denies all external tools and keeps the role prompt-only.
    /// 禁止全部外部工具并将角色保持为纯提示词执行。
    DenyAll,
}

/// One Rig feature recommendation captured during the initial integration audit.
/// 在初始接入审计阶段记录的单个 Rig 特性建议项。
#[derive(Debug, Clone)]
pub struct RigFeatureRecommendation {
    /// Stable Cargo feature identifier.
    /// 稳定的 Cargo 特性标识。
    pub name: &'static str,
    /// Whether the feature is explicitly enabled in the current workspace profile.
    /// 该特性是否已在当前工作区配置中显式启用。
    pub enabled_now: bool,
    /// Suggested adoption stage for the feature.
    /// 该特性的建议引入阶段。
    pub stage: &'static str,
    /// Short explanation describing why the feature should or should not be enabled now.
    /// 解释当前是否应该启用该特性的简短说明。
    pub summary: &'static str,
}

/// One role definition that can later become a Rig agent or sub-agent.
/// 后续可演化为 Rig agent 或子代理的单个角色定义。
#[derive(Debug, Clone)]
pub struct RoleDefinition {
    /// Stable role identifier used by workflows and delegation policies.
    /// 被工作流与委派策略使用的稳定角色标识。
    pub id: &'static str,
    /// Short role display name.
    /// 角色的短展示名称。
    pub name: &'static str,
    /// Human-readable role description.
    /// 人类可读的角色说明。
    pub description: &'static str,
    /// Preferred provider kind for the role.
    /// 该角色偏好的供应商类型。
    pub suggested_provider: Provider,
    /// Human-readable model hint used before the real runtime is bound.
    /// 在真实运行时绑定前使用的人类可读模型提示。
    pub suggested_model_hint: &'static str,
    /// Prompt nodes composing the role preamble.
    /// 组成角色前导提示词的节点集合。
    pub prompt_nodes: Vec<PromptNodeDefinition>,
    /// Tool policy reserved for later runtime filtering.
    /// 为后续运行时过滤预留的工具策略。
    pub tool_policy: ToolPolicy,
    /// Delegation targets reserved for future sub-agent orchestration.
    /// 为未来子代理编排预留的委派目标列表。
    pub delegation_targets: Vec<&'static str>,
}

/// One provider descriptor shown in the Rig integration summary card.
/// 展示在 Rig 接入摘要卡片中的单个供应商描述。
#[derive(Debug, Clone)]
pub struct RigProviderDescriptor {
    /// Provider kind represented by the descriptor.
    /// 该描述对应的供应商类型。
    pub kind: Provider,
    /// Environment variables expected by the provider lane.
    /// 该供应商通道期望使用的环境变量。
    pub required_env: Vec<&'static str>,
    /// Main capability surface exposed by the provider.
    /// 该供应商暴露的主能力接口面。
    pub surfaces: &'static str,
    /// Short capability summary shown to the user.
    /// 展示给用户的简短能力摘要。
    pub summary: &'static str,
}

/// Top-level Rig integration scaffold reserved for future multi-role orchestration.
/// 为未来多角色编排预留的顶层 Rig 接入骨架。
#[derive(Debug, Clone)]
pub struct RigIntegrationScaffold {
    /// Feature recommendations captured from the current Rig feature audit.
    /// 从当前 Rig 特性审计中得出的特性建议列表。
    feature_recommendations: Vec<RigFeatureRecommendation>,
    /// Provider descriptors available to the future runtime.
    /// 未来运行时可用的供应商描述列表。
    providers: Vec<RigProviderDescriptor>,
    /// Seeded role definitions representing the initial orchestration slices.
    /// 代表初始编排切面的种子角色定义列表。
    roles: Vec<RoleDefinition>,
}

impl RigIntegrationScaffold {
    /// Builds the initial Rig integration scaffold used by the GUI prototype.
    /// 构建 GUI 原型使用的初始 Rig 接入骨架。
    pub fn seeded() -> Self {
        Self {
            feature_recommendations: vec![
                RigFeatureRecommendation {
                    name: "reqwest",
                    enabled_now: true,
                    stage: "当前必选",
                    summary: "负责基础 HTTP 请求、流式响应与 Provider 网络访问，首版接入必须显式保留。",
                },
                RigFeatureRecommendation {
                    name: "rustls",
                    enabled_now: true,
                    stage: "当前必选",
                    summary: "跨平台 TLS 路径更干净，适合作为桌面工作台的默认传输安全栈。",
                },
                RigFeatureRecommendation {
                    name: "derive",
                    enabled_now: true,
                    stage: "当前建议",
                    summary: "后续接入 Tool 与结构化输入输出时可减少样板代码，建议从一开始保留。",
                },
                RigFeatureRecommendation {
                    name: "rmcp",
                    enabled_now: false,
                    stage: "后续按需",
                    summary: "待 MCP 工具与外部 Skill 服务真正接入后再启用，避免首版依赖过重。",
                },
                RigFeatureRecommendation {
                    name: "reqwest-middleware",
                    enabled_now: false,
                    stage: "后续按需",
                    summary: "统一重试、代理和审计拦截时再启用，当前阶段先保持网络栈更薄。",
                },
                RigFeatureRecommendation {
                    name: "websocket",
                    enabled_now: false,
                    stage: "后续按需",
                    summary: "实时双向会话或流媒体控制真正落地后再引入，首版配置器暂不需要。",
                },
            ],
            providers: vec![
                provider_descriptor(
                    Provider::OpenAi,
                    vec!["OPENAI_API_KEY", "OPENAI_BASE_URL"],
                    "Completion · Embeddings · Image · Audio",
                    "OpenAI 主通道，适合作为标准文本、工具调用与多模态入口。",
                ),
                provider_descriptor(
                    Provider::Anthropic,
                    vec!["ANTHROPIC_API_KEY"],
                    "Completion",
                    "Anthropic 通道，当前主要承担长上下文推理与高质量评审角色。",
                ),
                provider_descriptor(
                    Provider::AzureOpenAi,
                    vec!["AZURE_ENDPOINT", "AZURE_API_VERSION", "AZURE_API_KEY"],
                    "Completion · Embeddings · Image · Audio",
                    "Azure OpenAI 通道，适合企业环境与自有资源编排。",
                ),
                provider_descriptor(
                    Provider::ChatGpt,
                    vec!["CHATGPT_ACCESS_TOKEN"],
                    "Completion",
                    "ChatGPT 官方消费端接口，偏会话入口，不适合首版默认集成路径。",
                ),
                provider_descriptor(
                    Provider::Cohere,
                    vec!["COHERE_API_KEY"],
                    "Completion · Embeddings",
                    "Cohere 通道，适合长文档检索与文本生成协同。",
                ),
                provider_descriptor(
                    Provider::Copilot,
                    vec!["GITHUB_COPILOT_API_KEY", "GITHUB_TOKEN"],
                    "Completion · Embeddings",
                    "Copilot 通道，适合作为 IDE 辅助型代码生成与补全能力源。",
                ),
                provider_descriptor(
                    Provider::DeepSeek,
                    vec!["DEEPSEEK_API_KEY"],
                    "Completion",
                    "DeepSeek 通道，适合作为审查、分析与高性价比推理角色。",
                ),
                provider_descriptor(
                    Provider::Galadriel,
                    vec!["GALADRIEL_API_KEY"],
                    "Completion",
                    "Galadriel 通道，保留为可插拔第三方供应商实例位。",
                ),
                provider_descriptor(
                    Provider::Gemini,
                    vec!["GEMINI_API_KEY"],
                    "Completion · Embeddings · Image",
                    "Gemini 通道，适合多模态工程任务与更大的上下文容量。",
                ),
                provider_descriptor(
                    Provider::Groq,
                    vec!["GROQ_API_KEY"],
                    "Completion",
                    "Groq 通道，适合极低延迟的对话与轻量级编码辅助。",
                ),
                provider_descriptor(
                    Provider::HuggingFace,
                    vec!["HUGGINGFACE_API_KEY"],
                    "Completion",
                    "Hugging Face 推理通道，适合作为模型实验与兼容层入口。",
                ),
                provider_descriptor(
                    Provider::Hyperbolic,
                    vec!["HYPERBOLIC_API_KEY"],
                    "Completion · Image",
                    "Hyperbolic 通道，保留为兼容开放式推理与图像能力的外部供应商入口。",
                ),
                provider_descriptor(
                    Provider::Llamafile,
                    vec!["LLAMAFILE_API_BASE_URL"],
                    "Completion · Embeddings",
                    "Llamafile 本地通道，适合离线原型与自托管编码体验。",
                ),
                provider_descriptor(
                    Provider::MiniMax,
                    vec!["MINIMAX_API_KEY", "MINIMAX_API_BASE"],
                    "Completion",
                    "MiniMax 通道，保留为高上下文商业模型配置入口。",
                ),
                provider_descriptor(
                    Provider::Mira,
                    vec!["MIRA_API_KEY"],
                    "Completion",
                    "Mira 通道，作为新供应商接入样板保留。",
                ),
                provider_descriptor(
                    Provider::Mistral,
                    vec!["MISTRAL_API_KEY"],
                    "Completion · Embeddings",
                    "Mistral 通道，适合作为轻量模型与代码模型的替代来源。",
                ),
                provider_descriptor(
                    Provider::Moonshot,
                    vec!["MOONSHOT_API_KEY", "MOONSHOT_API_BASE"],
                    "Completion",
                    "Moonshot 通道，适合作为长上下文中文工程助手来源。",
                ),
                provider_descriptor(
                    Provider::Ollama,
                    vec!["OLLAMA_API_BASE_URL"],
                    "Completion · Embeddings",
                    "Ollama 本地通道，适合作为离线与私有环境标准入口。",
                ),
                provider_descriptor(
                    Provider::OpenRouter,
                    vec!["OPENROUTER_API_KEY"],
                    "Completion · Embeddings",
                    "OpenRouter 聚合通道，适合作为多供应商统一路由层。",
                ),
                provider_descriptor(
                    Provider::Perplexity,
                    vec!["PERPLEXITY_API_KEY"],
                    "Completion",
                    "Perplexity 通道，偏向研究与检索增强问答。",
                ),
                provider_descriptor(
                    Provider::Together,
                    vec!["TOGETHER_API_KEY"],
                    "Completion · Embeddings",
                    "Together 通道，适合作为开放模型汇聚入口。",
                ),
                provider_descriptor(
                    Provider::VoyageAi,
                    vec!["VOYAGE_API_KEY"],
                    "Embeddings",
                    "VoyageAI 以向量为主，更适合作为检索层而非主对话模型。",
                ),
                provider_descriptor(
                    Provider::Xai,
                    vec!["XAI_API_KEY"],
                    "Completion · Image",
                    "xAI 通道，保留为推理与多模态实验入口。",
                ),
                provider_descriptor(
                    Provider::XiaomiMimo,
                    vec!["XIAOMI_MIMO_API_KEY", "XIAOMI_MIMO_API_BASE"],
                    "Completion",
                    "Xiaomi Mimo 通道，保留为区域性供应商兼容入口。",
                ),
                provider_descriptor(
                    Provider::ZAi,
                    vec!["ZAI_API_KEY", "ZAI_API_BASE"],
                    "Completion",
                    "Z.ai 通道，保留为区域性供应商兼容入口。",
                ),
                provider_descriptor(
                    Provider::OpenAiCompatibleLocal,
                    vec!["OPENAI_API_KEY", "OPENAI_BASE_URL"],
                    "Completion · Embeddings",
                    "自托管 OpenAI 兼容网关入口，适合作为本地与企业代理统一接入点。",
                ),
            ],
            roles: vec![
                RoleDefinition {
                    id: "planner",
                    name: "Planner",
                    description: "负责任务拆解、计划评估与执行前风险预演。",
                    suggested_provider: Provider::Anthropic,
                    suggested_model_hint: "claude-sonnet-4.6",
                    prompt_nodes: vec![
                        PromptNodeDefinition {
                            id: "planner.identity",
                            title: "Planning Identity",
                            kind: PromptNodeKind::Identity,
                            enabled: true,
                            content: "You are the planning specialist for an AI coding workbench.",
                        },
                        PromptNodeDefinition {
                            id: "planner.constraint",
                            title: "Safety Constraint",
                            kind: PromptNodeKind::Constraint,
                            enabled: true,
                            content: "Prefer explicit trade-off analysis before risky changes or branching decisions.",
                        },
                        PromptNodeDefinition {
                            id: "planner.context",
                            title: "Workspace Context",
                            kind: PromptNodeKind::Context,
                            enabled: true,
                            content: "The current project is a Rust + iced desktop AI coding client that will later support configurable roles and tools.",
                        },
                    ],
                    tool_policy: ToolPolicy::DenyAll,
                    delegation_targets: vec!["coder", "reviewer"],
                },
                RoleDefinition {
                    id: "coder",
                    name: "Coder",
                    description: "负责任务实现、补丁生成与工程性修改落地。",
                    suggested_provider: Provider::OpenAi,
                    suggested_model_hint: "gpt-5.5",
                    prompt_nodes: vec![
                        PromptNodeDefinition {
                            id: "coder.identity",
                            title: "Coding Identity",
                            kind: PromptNodeKind::Identity,
                            enabled: true,
                            content: "You are the implementation specialist focused on reliable code changes.",
                        },
                        PromptNodeDefinition {
                            id: "coder.constraint",
                            title: "Quality Constraint",
                            kind: PromptNodeKind::Constraint,
                            enabled: true,
                            content: "Default to maintainable, explicit, and reviewable code instead of short-term shortcuts.",
                        },
                        PromptNodeDefinition {
                            id: "coder.output",
                            title: "Output Contract",
                            kind: PromptNodeKind::OutputContract,
                            enabled: true,
                            content: "Explain major implementation trade-offs briefly after successful changes.",
                        },
                    ],
                    tool_policy: ToolPolicy::InheritWorkbench,
                    delegation_targets: vec!["reviewer"],
                },
                RoleDefinition {
                    id: "reviewer",
                    name: "Reviewer",
                    description: "负责任务回归校验、差异审查与风险提示。",
                    suggested_provider: Provider::DeepSeek,
                    suggested_model_hint: "deepseek-reasoner",
                    prompt_nodes: vec![
                        PromptNodeDefinition {
                            id: "reviewer.identity",
                            title: "Review Identity",
                            kind: PromptNodeKind::Identity,
                            enabled: true,
                            content: "You are the reviewer focused on behavioral regressions, risk, and missing validation.",
                        },
                        PromptNodeDefinition {
                            id: "reviewer.context",
                            title: "Review Context",
                            kind: PromptNodeKind::Context,
                            enabled: true,
                            content: "The workbench should eventually expose multi-role orchestration, provider configuration, and tool gating.",
                        },
                        PromptNodeDefinition {
                            id: "reviewer.output",
                            title: "Review Output Contract",
                            kind: PromptNodeKind::OutputContract,
                            enabled: true,
                            content: "Lead with findings first, then summarize residual risk and validation gaps.",
                        },
                    ],
                    tool_policy: ToolPolicy::AllowGroups(vec!["git", "diff", "diagnostics"]),
                    delegation_targets: vec![],
                },
            ],
        }
    }

    /// Returns feature recommendations captured for the current Rig integration.
    /// 返回为当前 Rig 接入记录的特性建议列表。
    pub fn feature_recommendations(&self) -> &[RigFeatureRecommendation] {
        &self.feature_recommendations
    }

    /// Returns the provider descriptors captured for the current Rig integration.
    /// 返回为当前 Rig 接入记录的供应商描述列表。
    pub fn providers(&self) -> &[RigProviderDescriptor] {
        &self.providers
    }

    /// Returns the seeded role definitions reserved for future orchestration growth.
    /// 返回为未来编排扩展预留的种子角色定义列表。
    pub fn roles(&self) -> &[RoleDefinition] {
        &self.roles
    }

    /// Returns the total number of provider kinds captured by the Rig catalog.
    /// 返回 Rig 目录中记录的供应商类型总数。
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Returns the total number of seeded orchestration roles.
    /// 返回已种子化的编排角色总数。
    pub fn role_count(&self) -> usize {
        self.roles.len()
    }

    /// Returns the total number of enabled prompt nodes across all roles.
    /// 返回所有角色中已启用提示节点的总数量。
    pub fn enabled_prompt_node_count(&self) -> usize {
        self.roles
            .iter()
            .flat_map(|role| role.prompt_nodes.iter())
            .filter(|node| node.enabled)
            .count()
    }

    /// Returns one role by its stable identifier.
    /// 通过稳定标识返回单个角色定义。
    pub fn role(&self, role_id: &str) -> Option<&RoleDefinition> {
        self.roles.iter().find(|role| role.id == role_id)
    }

    /// Composes the enabled prompt-node content into one preamble string.
    /// 将已启用提示节点内容拼装为单个前导提示词字符串。
    pub fn compose_preamble(&self, role_id: &str) -> Option<String> {
        let role = self.role(role_id)?;

        Some(
            role.prompt_nodes
                .iter()
                .filter(|node| node.enabled)
                .map(|node| node.content)
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
    }
}

/// Builds one provider descriptor from the shared catalog metadata.
/// 使用共享目录元数据构建单个供应商描述。
fn provider_descriptor(
    kind: Provider,
    required_env: Vec<&'static str>,
    surfaces: &'static str,
    summary: &'static str,
) -> RigProviderDescriptor {
    RigProviderDescriptor {
        kind,
        required_env,
        surfaces,
        summary,
    }
}
