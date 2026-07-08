export interface ComposerAttachmentCapabilitySummary {
  // Stable attachment kind key.
  // 稳定附件类型键。
  kind: string;
  // User-facing attachment kind label.
  // 面向用户显示的附件类型名称。
  label: string;
  // Whether this attachment kind can be submitted.
  // 当前附件类型是否允许提交。
  enabled: boolean;
  // Whether the active provider/model supports this kind natively.
  // 当前 Provider/模型是否原生支持该类型。
  native: boolean;
  // Whether host-side AI tools bridge this kind.
  // 宿主侧 AI 工具是否桥接该类型。
  bridged: boolean;
  // Browser file picker accept string.
  // 浏览器文件选择 accept 字符串。
  accept: string;
}

// ComposerAttachmentSummary describes one staged composer attachment.
// ComposerAttachmentSummary 描述单条已暂存的输入区附件。
export interface ComposerAttachmentSummary {
  // Stable attachment kind key.
  // 稳定附件类型键。
  kind: string;
  // Human-readable file name.
  // 人类可读文件名。
  fileName: string;
  // Managed local path used for provider replay.
  // Provider 回放使用的托管本地路径。
  localPath: string;
  // Optional original source path.
  // 可选原始来源路径。
  sourcePath?: string | null;
  // Optional asset identifier.
  // 可选资源标识。
  assetId?: string | null;
  // Optional byte length.
  // 可选字节长度。
  byteLen?: number | null;
  // Optional preview local path.
  // 可选预览本地路径。
  previewLocalPath?: string | null;
  // MIME type.
  // MIME 类型。
  mimeType: string;
}

// PendingPromptSummary describes one visible queued prompt above the composer.
// PendingPromptSummary 描述输入区上方单条可见排队提示词。
export interface PendingPromptSummary {
  // Stable entry identifier.
  // 稳定条目标识。
  id: string;
  // Full prompt text restored during editing.
  // 编辑时恢复的完整提示词文本。
  prompt: string;
  // Compact queue preview.
  // 紧凑队列预览。
  preview: string;
  // Attachments replayed with the prompt.
  // 随提示词回放的附件。
  attachments: ComposerAttachmentSummary[];
  // Stable queue state key.
  // 稳定队列状态键。
  state: "ready" | "inserted" | string;
  // Unix-millis creation timestamp text.
  // Unix 毫秒格式创建时间文本。
  createdAt: string;
}

// ToolCallSummary describes one tool call block.
// ToolCallSummary 描述单个工具调用块。
export interface ToolCallSummary {
  // Stable tool call identifier.
  // 稳定工具调用标识。
  id: string;
  // Tool display name.
  // 工具显示名称。
  name: string;
  // Tool status.
  // 工具状态。
  status: string;
  // Tool detail text.
  // 工具详情文本。
  detail: string;
}

// LuaSkillToolSummary describes one Lua Skill runtime tool row.
// LuaSkillToolSummary 描述单个 Lua Skill 运行时工具行。
export interface LuaSkillToolSummary {
  // Canonical host-visible tool name.
  // 宿主可见的规范工具名称。
  canonicalName: string;
  // Local tool name inside the skill package.
  // 技能包内部的本地工具名称。
  localName: string;
  // Tool description from the runtime descriptor.
  // 来自运行时描述符的工具说明。
  description: string;
  // Persisted registration state key.
  // 持久化注册状态键。
  registrationState: "explicit" | "implicit" | "disabled";
  // Human-readable registration state label.
  // 人类可读的注册状态标签。
  registrationLabel: string;
  // Short help text for the current registration state.
  // 当前注册状态的简短说明。
  registrationHelp: string;
}

// LuaSkillSummary describes one Lua Skill package.
// LuaSkillSummary 描述单个 Lua Skill 包。
export interface LuaSkillSummary {
  // Stable skill identifier.
  // 稳定技能标识。
  skillId: string;
  // Human-facing display name.
  // 面向用户的显示名称。
  displayName: string;
  // Skill package version.
  // 技能包版本。
  version: string;
  // Runtime root layer name.
  // 运行时根层名称。
  rootName: string;
  // Filesystem directory that contains the skill.
  // 包含该技能的文件系统目录。
  skillDir: string;
  // Whether this skill is currently enabled.
  // 该技能当前是否已启用。
  enabled: boolean;
  // Optional disabled reason.
  // 可选停用原因。
  disabledReason?: string | null;
  // Whether this skill has a managed install record.
  // 该技能是否拥有受管安装记录。
  managed: boolean;
  // Optional install source type label.
  // 可选安装来源类型标签。
  sourceType?: string | null;
  // Optional install source locator.
  // 可选安装来源定位值。
  sourceLocator?: string | null;
  // Number of visible runtime entries.
  // 可见运行时入口数量。
  entryCount: number;
  // Whether this skill belongs to the host-owned ROOT layer.
  // 该技能是否属于宿主自有 ROOT 层。
  systemOwned: boolean;
  // Tool rows exposed by this skill.
  // 该技能暴露的工具行。
  tools: LuaSkillToolSummary[];
}

// LuaSkillsCatalogSummary describes the full Lua Skills catalog.
// LuaSkillsCatalogSummary 描述完整 Lua Skills 目录。
export interface LuaSkillsCatalogSummary {
  // Runtime root used by the embedded Lua Skills host.
  // 嵌入式 Lua Skills 宿主使用的运行时根目录。
  runtimeRoot: string;
  // System Lua library directory used by Lua Skills.
  // Lua Skills 使用的系统 Lua 库目录。
  systemLuaLib: string;
  // Total visible runtime entry count.
  // 可见运行时入口总数。
  entryCount: number;
  // Human-readable notice for the current catalog state.
  // 当前目录状态的人类可读提示。
  notice: string;
  // Skill package rows visible to the host.
  // 宿主当前可见的技能包行。
  skills: LuaSkillSummary[];
}

// AgentSkillSummary describes one prompt-based Agent Skill.
// AgentSkillSummary 描述单个基于提示词的 Agent Skill。
export interface AgentSkillSummary {
  // Stable skill name from frontmatter.
  // 来自 frontmatter 的稳定技能名称。
  name: string;
  // Human-facing display name.
  // 面向用户的显示名称。
  displayName: string;
  // Human-facing short description.
  // 面向用户的简短说明。
  description: string;
  // Source scope label.
  // 来源作用域标签。
  scope: string;
  // Whether this Agent Skill is enabled.
  // 该 Agent Skill 是否已启用。
  enabled: boolean;
  // Filesystem path to the SKILL.md file.
  // 指向 SKILL.md 文件的文件系统路径。
  skillFile: string;
  // Directory that owns this Agent Skill and bundled resources.
  // 拥有该 Agent Skill 与附带资源的目录。
  baseDir: string;
  // Optional resolved icon path.
  // 可选解析图标路径。
  iconPath?: string | null;
  // Markdown body without YAML frontmatter.
  // 去除 YAML frontmatter 后的 Markdown 正文。
  body: string;
}

// AgentSkillLoadIssueSummary describes one non-fatal Agent Skill loading issue.
// AgentSkillLoadIssueSummary 描述单个非致命 Agent Skill 加载问题。
export interface AgentSkillLoadIssueSummary {
  // Path related to the loading issue.
  // 与加载问题相关的路径。
  path: string;
  // Human-readable diagnostic message.
  // 人类可读诊断信息。
  message: string;
}

// AgentSkillsCatalogSummary describes the full Agent Skills catalog.
// AgentSkillsCatalogSummary 描述完整 Agent Skills 目录。
export interface AgentSkillsCatalogSummary {
  // Whether Agent Skills are globally enabled.
  // Agent Skills 是否已全局启用。
  globalEnabled: boolean;
  // System skill root used by host-managed Agent Skills.
  // 宿主管理 Agent Skills 使用的系统技能根目录。
  systemRoot: string;
  // User skill root used by user-installed Agent Skills.
  // 用户安装 Agent Skills 使用的用户技能根目录。
  userRoot: string;
  // Human-readable notice for the current catalog state.
  // 当前目录状态的人类可读提示。
  notice: string;
  // Parsed Agent Skill rows.
  // 已解析的 Agent Skill 行。
  skills: AgentSkillSummary[];
  // Non-fatal loading issues.
  // 非致命加载问题。
  errors: AgentSkillLoadIssueSummary[];
}

// SubagentDefinitionSummary describes one subagent or expert definition.
// SubagentDefinitionSummary 描述单个子代理或专家定义。
export interface SubagentDefinitionSummary {
  // Stable agent id, normally the Markdown file stem.
  // 稳定代理标识，通常为 Markdown 文件 stem。
  agentId: string;
  // Human-facing display name.
  // 面向用户的显示名称。
  name: string;
  // Optional human-facing description.
  // 可选的人类可读说明。
  description?: string | null;
  // Localized display metadata keyed by locale tag.
  // 按语言标签索引的本地化展示元数据。
  locales?: Record<string, SubagentDefinitionLocaleSummary> | null;
  // Optional upstream or license source metadata for the agent page.
  // 智能体页面使用的可选上游或协议来源元数据。
  sourceOrigin?: SubagentOriginSummary | null;
  // Human-facing emoji.
  // 面向用户的图标。
  emoji: string;
  // Human-facing color token or color value.
  // 面向用户的颜色标识或颜色值。
  color: string;
  // Optional short tone or persona hint.
  // 可选简短语气或人格提示。
  vibe?: string | null;
  // Stable source key for this definition row.
  // 当前定义行的稳定来源键。
  sourceKey: string;
  // Optional canonical category id.
  // 可选规范分类标识。
  categoryId?: string | null;
  // Optional package id.
  // 可选包标识。
  packageId?: string | null;
  // Source scope label.
  // 来源作用域标签。
  scope: string;
  // Stable source scope key used by UI logic.
  // UI 逻辑使用的稳定来源作用域键。
  scopeKey: "system" | "user" | "package" | string;
  // Whether this definition is exposed to models and spawn_agent.
  // 该定义是否暴露给模型与 spawn_agent。
  enabled: boolean;
  // Filesystem path to the Markdown definition file.
  // 指向 Markdown 定义文件的文件系统路径。
  definitionFile: string;
  // Optional filesystem path to the sidecar metadata file.
  // 指向 sidecar 元数据文件的可选文件系统路径。
  sidecarFile?: string | null;
  // Optional remote URL that supplied this user definition.
  // 提供该用户定义的可选远程 URL。
  remoteUrl?: string | null;
  // Optional system definition id overridden by this row.
  // 被当前行覆盖的可选系统定义标识。
  overrideOf?: string | null;
  // Whether the user layer can be removed to restore a system definition.
  // 是否可以移除用户层以恢复系统定义。
  canRestoreDefault?: boolean;
  // Optional source hash for audit and cache diagnostics.
  // 用于审计与缓存诊断的可选来源哈希。
  sourceHash?: string | null;
  // Optional source version string.
  // 可选来源版本字符串。
  version?: string | null;
  // Optional last-updated timestamp string.
  // 可选最近更新时间戳字符串。
  updatedAt?: string | null;
  // Unified tool policy mode.
  // 统一工具策略模式。
  toolPolicyMode: CapabilityPolicyMode;
  // Unified tool policy selectors.
  // 统一工具策略选择器。
  toolPolicyTools: string[];
  // Number of explicitly allowed host tools.
  // 显式允许的宿主工具数量。
  allowedToolCount: number;
  // Number of explicitly disabled host tools.
  // 显式禁用的宿主工具数量。
  disabledToolCount: number;
  // Number of enabled prompt skills declared by this definition.
  // 当前定义声明启用的提示词技能数量。
  enabledPromptSkillCount: number;
  // Number of disabled prompt skills declared by this definition.
  // 当前定义声明禁用的提示词技能数量。
  disabledPromptSkillCount: number;
  // Unified prompt skill policy mode.
  // 统一提示词技能策略模式。
  promptSkillPolicyMode: CapabilityPolicyMode;
  // Unified prompt skill policy selectors.
  // 统一提示词技能策略选择器。
  promptSkillPolicySkills: string[];
  // Optional fixed provider/model selection for this agent.
  // 当前智能体可选的固定供应商/模型选择。
  providerModel: SubagentProviderModelSummary;
  // Optional base system prompt.
  // 可选基础系统提示词。
  systemPrompt?: string | null;
  // Optional appended system prompt.
  // 可选追加系统提示词。
  systemPromptAppend?: string | null;
  // Explicitly allowed host tool names.
  // 显式允许的宿主工具名称。
  allowedTools: string[];
  // Explicitly disabled host tool names.
  // 显式禁用的宿主工具名称。
  disabledTools: string[];
  // Explicitly allowed AI capability selectors.
  // 显式允许的 AI 能力选择器。
  allowedAiCapabilities: string[];
  // Explicitly disabled AI capability selectors.
  // 显式禁用的 AI 能力选择器。
  disabledAiCapabilities: string[];
  // LuaSkills-specific exposure override payload.
  // LuaSkills 专属可见性覆盖载荷。
  luaskillsOverrides: unknown;
  // MCP-specific exposure override payload.
  // MCP 专属可见性覆盖载荷。
  mcpServerOverrides: unknown;
  // Explicitly enabled prompt skill names.
  // 显式启用的提示词技能名称。
  enabledPromptSkills: string[];
  // Explicitly disabled prompt skill names.
  // 显式禁用的提示词技能名称。
  disabledPromptSkills: string[];
  // Effective capability diagnostic messages.
  // 最终能力诊断消息。
  capabilityDiagnostics: string[];
}

// CapabilityPolicyMode describes how a selector list affects visible capabilities.
// CapabilityPolicyMode 描述选择器列表如何影响可见能力。
export type CapabilityPolicyMode = "all" | "allow_only" | "deny_only";

// SubagentDefinitionLocaleSummary describes localized agent display metadata.
// SubagentDefinitionLocaleSummary 描述智能体本地化展示元数据。
export interface SubagentDefinitionLocaleSummary {
  // Optional localized agent name.
  // 可选本地化智能体名称。
  name?: string | null;
  // Optional localized agent description.
  // 可选本地化智能体说明。
  description?: string | null;
}

// SubagentOriginSummary describes upstream or license source metadata.
// SubagentOriginSummary 描述上游或协议来源元数据。
export interface SubagentOriginSummary {
  // Source kind such as github, system, or other.
  // 来源类型，例如 github、system 或 other。
  kind?: string | null;
  // Optional human-facing source label.
  // 可选的人类可读来源标签。
  label?: string | null;
  // Optional icon key used by the WebUI.
  // WebUI 使用的可选图标键。
  icon?: string | null;
  // Optional source URL.
  // 可选来源链接。
  url?: string | null;
}

// SubagentProviderModelSummary describes one optional fixed provider/model selection.
// SubagentProviderModelSummary 描述单个可选固定供应商/模型选择。
export interface SubagentProviderModelSummary {
  // Optional provider identifier.
  // 可选供应商标识。
  providerId?: string | null;
  // Optional model identifier.
  // 可选模型标识。
  modelId?: string | null;
  // Effective display mode for this agent's model selection.
  // 当前智能体模型选择的有效展示模式。
  mode?: string;
  // Stable provider/model selector when a fixed model is configured.
  // 配置固定模型时使用的稳定 provider/model 选择器。
  selector?: string | null;
  // Whether the fixed model is currently selectable.
  // 固定模型当前是否可被选择。
  available?: boolean | null;
  // Human-readable reason when the fixed model is unavailable.
  // 固定模型不可用时的人类可读原因。
  unavailableReason?: string | null;
}

// SubagentDefinitionEnabledUpdate describes one lightweight enabled-state update.
// SubagentDefinitionEnabledUpdate 描述单个轻量启停状态更新。
export interface SubagentDefinitionEnabledUpdate {
  // Stable agent id that was updated.
  // 已更新的稳定智能体标识。
  agentId: string;
  // Updated enabled state.
  // 更新后的启停状态。
  enabled: boolean;
  // Updated catalog notice.
  // 更新后的目录提示。
  notice: string;
}

// SubagentDefinitionSaveRequest describes one editable agent definition form submission.
// SubagentDefinitionSaveRequest 描述单次可编辑智能体定义表单提交。
export interface SubagentDefinitionSaveRequest {
  // Stable agent id, normally the Markdown file stem.
  // 稳定智能体标识，通常为 Markdown 文件 stem。
  agentId: string;
  // Optional original agent id when saving a moved definition.
  // 保存已转移定义时使用的可选原始智能体标识。
  originalAgentId?: string | null;
  // Optional user package id assigned to this definition.
  // 分配给当前定义的可选用户包标识。
  packageId?: string | null;
  // Human-facing display name.
  // 面向用户的显示名称。
  name: string;
  // Optional human-facing description.
  // 可选的人类可读说明。
  description?: string | null;
  // Localized display metadata keyed by locale tag.
  // 按语言标签索引的本地化展示元数据。
  locales?: Record<string, SubagentDefinitionLocaleSummary> | null;
  // Optional human-facing emoji override.
  // 可选的人类可读图标覆盖。
  emoji?: string | null;
  // Optional human-facing color token or value override.
  // 可选的人类可读颜色标识或颜色值覆盖。
  color?: string | null;
  // Optional short tone or persona hint override.
  // 可选简短语气或人格提示覆盖。
  vibe?: string | null;
  // Whether this definition is exposed to models and spawn_agent.
  // 该定义是否暴露给模型与 spawn_agent。
  enabled: boolean;
  // Optional base system prompt.
  // 可选基础系统提示词。
  systemPrompt?: string | null;
  // Optional appended system prompt.
  // 可选追加系统提示词。
  systemPromptAppend?: string | null;
  // Explicitly allowed host tool names.
  // 显式允许的宿主工具名称。
  allowedTools: string[];
  // Explicitly disabled host tool names.
  // 显式禁用的宿主工具名称。
  disabledTools: string[];
  // Unified tool policy mode.
  // 统一工具策略模式。
  toolPolicyMode?: CapabilityPolicyMode | null;
  // Unified tool policy selectors.
  // 统一工具策略选择器。
  toolPolicyTools?: string[];
  // Explicitly allowed AI capability selectors.
  // 显式允许的 AI 能力选择器。
  allowedAiCapabilities?: string[];
  // Explicitly disabled AI capability selectors.
  // 显式禁用的 AI 能力选择器。
  disabledAiCapabilities?: string[];
  // LuaSkills-specific exposure override payload.
  // LuaSkills 专属可见性覆盖载荷。
  luaskillsOverrides?: unknown;
  // MCP-specific exposure override payload.
  // MCP 专属可见性覆盖载荷。
  mcpServerOverrides?: unknown;
  // Explicitly enabled prompt skill names.
  // 显式启用的提示词技能名称。
  enabledPromptSkills: string[];
  // Explicitly disabled prompt skill names.
  // 显式禁用的提示词技能名称。
  disabledPromptSkills: string[];
  // Unified prompt skill policy mode.
  // 统一提示词技能策略模式。
  promptSkillPolicyMode?: CapabilityPolicyMode | null;
  // Unified prompt skill policy selectors.
  // 统一提示词技能策略选择器。
  promptSkillPolicySkills?: string[];
  // Optional fixed provider/model selection for this agent.
  // 当前智能体可选的固定供应商/模型选择。
  providerModel?: SubagentProviderModelSummary;
}

// SubagentCategorySummary describes one parsed agent catalog category.
// SubagentCategorySummary 描述单个已解析智能体目录分类。
export interface SubagentCategorySummary {
  // Stable category id.
  // 稳定分类标识。
  categoryId: string;
  // Human-facing category name.
  // 面向用户的分类名称。
  name: string;
  // Optional category description.
  // 可选分类说明。
  description?: string | null;
  // Localized display metadata keyed by locale tag.
  // 按语言标签索引的本地化展示元数据。
  locales?: Record<string, SubagentCategoryLocaleSummary> | null;
  // Human-facing category emoji.
  // 面向用户的分类图标。
  emoji: string;
  // Human-facing category color token or value.
  // 面向用户的分类颜色标识或颜色值。
  color: string;
  // Optional category order.
  // 可选分类排序值。
  order?: number | null;
}

// SubagentCategoryLocaleSummary describes localized category display metadata.
// SubagentCategoryLocaleSummary 描述分类本地化展示元数据。
export interface SubagentCategoryLocaleSummary {
  // Optional localized category name.
  // 可选本地化分类名称。
  name?: string | null;
  // Optional localized category description.
  // 可选本地化分类说明。
  description?: string | null;
}

// SubagentCatalogIssueSummary describes one subagent catalog loading issue.
// SubagentCatalogIssueSummary 描述单个子代理目录加载问题。
export interface SubagentCatalogIssueSummary {
  // Path related to the loading issue.
  // 与加载问题相关的路径。
  path: string;
  // Human-readable diagnostic message.
  // 人类可读诊断信息。
  message: string;
}

// SubagentCatalogSummary describes the full subagent and expert catalog.
// SubagentCatalogSummary 描述完整子代理与专家目录。
export interface SubagentCatalogSummary {
  // System subagent definition root shipped with the application.
  // 随应用发布的系统子代理定义根目录。
  systemRoot: string;
  // User-owned subagent definition and override root.
  // 用户拥有的子代理定义与覆盖根目录。
  userRoot: string;
  // Human-readable notice for the current catalog state.
  // 当前目录状态的人类可读提示。
  notice: string;
  // Public tool ids that agents cannot execute directly.
  // 智能体不能直接执行的公开工具标识。
  childAgentForbiddenToolIds: string[];
  // Merged subagent and expert definition rows.
  // 合并后的子代理与专家定义行。
  definitions: SubagentDefinitionSummary[];
  // Parsed category rows.
  // 已解析的分类行。
  categories: SubagentCategorySummary[];
  // Non-fatal loading issues.
  // 非致命加载问题。
  errors: SubagentCatalogIssueSummary[];
}
