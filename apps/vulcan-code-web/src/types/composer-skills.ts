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
