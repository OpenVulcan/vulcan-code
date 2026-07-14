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

// LuaVmPoolSettings describes one LuaSkills VM pool startup policy.
// LuaVmPoolSettings 描述单个 LuaSkills VM 池启动策略。
export interface LuaVmPoolSettings {
  // Minimum number of warm Lua VMs retained by the pool.
  // 池内保留的最小预热 Lua VM 数量。
  minSize: number;
  // Maximum number of Lua VMs allowed in the pool.
  // 池内允许存在的最大 Lua VM 数量。
  maxSize: number;
  // Idle seconds before one excess Lua VM may be retired.
  // 多余 Lua VM 可被回收前的空闲秒数。
  idleTtlSecs: number;
}

// LuaSkillsToolCacheSettings describes LuaSkills transient tool-cache startup settings.
// LuaSkillsToolCacheSettings 描述 LuaSkills 临时工具缓存启动设置。
export interface LuaSkillsToolCacheSettings {
  // Maximum process-wide cached entry count.
  // 进程级缓存最大条目数。
  maxEntries: number;
  // Default cache entry lifetime in seconds.
  // 缓存条目的默认存活秒数。
  defaultTtlSecs: number;
  // Maximum accepted cache entry lifetime in seconds.
  // 缓存条目允许的最大存活秒数。
  maxTtlSecs: number;
}

// LuaSkillsNetworkSettings describes LuaSkills download and source-routing startup settings.
// LuaSkillsNetworkSettings 描述 LuaSkills 下载与来源路由启动设置。
export interface LuaSkillsNetworkSettings {
  // Whether LuaSkills may download skills and dependencies.
  // LuaSkills 是否允许下载技能与依赖。
  allowDownloads: boolean;
  // Optional GitHub website base URL override.
  // 可选的 GitHub 网站基址覆盖。
  githubBaseUrl?: string | null;
  // Optional GitHub API base URL override.
  // 可选的 GitHub API 基址覆盖。
  githubApiBaseUrl?: string | null;
  // Optional official LuaSkills Hub base URL override.
  // 可选的 LuaSkills 官方 Hub 基址覆盖。
  officialSkillHubBaseUrl?: string | null;
  // Whether trusted system operations may install private URL manifests.
  // 受信系统操作是否允许安装私有 URL 清单。
  enablePrivateUrlSkillInstall: boolean;
  // Allowed URL prefixes for private skill manifests.
  // 私有技能清单允许的 URL 前缀。
  privateSkillSourceAllowlist: string[];
}

// LuaSkillsCapabilitySettings describes LuaSkills host capabilities captured at startup.
// LuaSkillsCapabilitySettings 描述 LuaSkills 启动时捕获的宿主能力。
export interface LuaSkillsCapabilitySettings {
  // Optional default encoding label for managed IO and process APIs.
  // 托管 IO 与进程 API 使用的可选默认编码标签。
  defaultTextEncoding?: string | null;
  // Whether Lua may invoke the host skill-management bridge.
  // Lua 是否可调用宿主技能管理桥接。
  enableSkillManagementBridge: boolean;
  // Whether sessions replace Lua's native global IO table with managed IO.
  // 会话是否使用托管 IO 替换 Lua 原生全局 IO 表。
  enableManagedIoCompat: boolean;
}

// LuaSkillsManagedRuntimeSettings describes managed Python and Node worker/session limits.
// LuaSkillsManagedRuntimeSettings 描述受管 Python 与 Node Worker/会话限制。
export interface LuaSkillsManagedRuntimeSettings {
  // Maximum live workers for one exact environment and package owner.
  // 单个精确环境与包所有者允许的最大活动 Worker 数量。
  workerPoolMaxSizePerEnvironment: number;
  // Idle seconds after which an unused managed worker may be retired.
  // 未使用的受管 Worker 可被回收前的空闲秒数。
  workerIdleTtlSecs: number;
  // Maximum launching or active persistent sessions owned by one engine.
  // 单个引擎允许持有的启动中或活动持久会话最大数量。
  persistentSessionLimitPerEngine: number;
  // Default retained byte limit for each persistent-session output stream.
  // 每个持久会话输出流默认保留的字节上限。
  persistentSessionDefaultBufferLimitBytesPerStream: number;
  // Default invoke timeout in milliseconds; null means unlimited.
  // 默认调用超时毫秒数；null 表示无限制。
  invokeDefaultTimeoutMs?: number | null;
}

// LuaSkillsRuntimeSettings describes the complete Vulcan-owned engine startup document.
// LuaSkillsRuntimeSettings 描述完整的 Vulcan 自有引擎启动设置文档。
export interface LuaSkillsRuntimeSettings {
  // Settings document schema version.
  // 设置文档 schema 版本。
  schemaVersion: number;
  // Reusable VM pool used by ordinary Skill calls.
  // 普通 Skill 调用使用的可复用 VM 池。
  vmPool: LuaVmPoolSettings;
  // Dedicated isolated VM pool used by runLua.
  // runLua 使用的独立隔离 VM 池。
  runLuaPool: LuaVmPoolSettings;
  // Shared transient tool-cache policy.
  // 共享临时工具缓存策略。
  toolCache: LuaSkillsToolCacheSettings;
  // Network and source-routing policy.
  // 网络与来源路由策略。
  network: LuaSkillsNetworkSettings;
  // Host capabilities and default text encoding.
  // 宿主能力与默认文本编码。
  capabilities: LuaSkillsCapabilitySettings;
  // Managed Python and Node worker and persistent-session policy.
  // 受管 Python、Node Worker 与持久会话策略。
  managedRuntime: LuaSkillsManagedRuntimeSettings;
  // AI-visible LuaSkills tools controlled by the Vulcan host.
  // 由 Vulcan 宿主控制的 AI 可见 LuaSkills 工具。
  aiTools: LuaSkillsAiToolSettings;
}

// LuaSkillsAiToolSettings controls host tool exposure without disabling LuaSkills internals.
// LuaSkillsAiToolSettings 控制宿主工具曝光，但不会关闭 LuaSkills 内部能力。
export interface LuaSkillsAiToolSettings {
  // Whether isolated one-shot Lua execution is visible to AI providers.
  // 隔离的一次性 Lua 执行是否对 AI Provider 可见。
  enableRunLua: boolean;
  // Whether persistent Lua runtime sessions are visible to AI providers.
  // 持久 Lua 运行时会话是否对 AI Provider 可见。
  enableRunLuaSession: boolean;
}

// LuaSkillsManagedRuntimeBootstrapStatus identifies one completed bootstrap outcome.
// LuaSkillsManagedRuntimeBootstrapStatus 标识一次已完成的初始化结果。
export type LuaSkillsManagedRuntimeBootstrapStatus = "ready" | "installed" | "unsupported" | "unavailable";

// LuaSkillsManagedRuntimeTarget identifies one independently installable runtime group.
// LuaSkillsManagedRuntimeTarget 标识一个可独立安装的运行时组。
export type LuaSkillsManagedRuntimeTarget = "python" | "node";

// LuaSkillsManagedRuntimeBootstrapSummary describes the latest native managed runtime attempt.
// LuaSkillsManagedRuntimeBootstrapSummary 描述最近一次原生受管运行时尝试。
export interface LuaSkillsManagedRuntimeBootstrapSummary {
  // Stable completed-attempt status.
  // 稳定的已完成尝试状态。
  status: LuaSkillsManagedRuntimeBootstrapStatus;
  // Native platform key when platform detection succeeded.
  // 平台检测成功时的原生平台键。
  platform?: string | null;
  // Whether the native installer attempted to update the runtime layout.
  // 原生安装器是否尝试更新运行时目录布局。
  installAttempted: boolean;
  // Bounded backend diagnostic for the attempt.
  // 当前尝试的有界后端诊断信息。
  diagnostic: string;
}

// LuaSkillsManagedRuntimeComponentSummary describes one independently repairable runtime.
// LuaSkillsManagedRuntimeComponentSummary 描述一个可独立修复的运行时。
export interface LuaSkillsManagedRuntimeComponentSummary {
  // Exact packaged runtime version.
  // 随包提供的精确运行时版本。
  version: string;
  // Stable validation status for this runtime.
  // 当前运行时的稳定校验状态。
  status: LuaSkillsManagedRuntimeBootstrapStatus;
  // Exact backend validation failure retained for diagnostics.
  // 为诊断保留的精确后端校验失败信息。
  diagnostic: string;
}

// LuaSkillsManagedRuntimeProgressSummary describes one live native installation update.
// LuaSkillsManagedRuntimeProgressSummary 描述一次实时原生安装更新。
export interface LuaSkillsManagedRuntimeProgressSummary {
  phase: string;
  component?: string | null;
  message: string;
  completedBytes?: number | null;
  totalBytes?: number | null;
  overallPercent?: number | null;
}

// LuaSkillsCatalogSummary describes the full Lua Skills catalog.
// LuaSkillsCatalogSummary 描述完整 Lua Skills 目录。
export interface LuaSkillsCatalogSummary {
  // Runtime root used by the embedded Lua Skills host.
  // 嵌入式 Lua Skills 宿主使用的运行时根目录。
  runtimeRoot: string;
  // Host-owned shared root containing managed Python and Node distributions.
  // 宿主持有的共享根目录，包含受管 Python 与 Node 发行包。
  managedRuntimeDistributionRoot: string;
  // LuaSkills-owned writable root containing reusable managed environments.
  // LuaSkills 持有的可写根目录，包含可复用受管环境。
  managedRuntimeEnvironmentRoot: string;
  // Latest managed Python and Node bootstrap state.
  // 最近一次受管 Python 与 Node 初始化状态。
  managedRuntimeBootstrap: LuaSkillsManagedRuntimeBootstrapSummary;
  // Independently repairable Python runtime state.
  // 可独立修复的 Python 运行时状态。
  managedPythonRuntime?: LuaSkillsManagedRuntimeComponentSummary | null;
  // Independently repairable Node.js runtime state.
  // 可独立修复的 Node.js 运行时状态。
  managedNodeRuntime?: LuaSkillsManagedRuntimeComponentSummary | null;
  // Current native installation progress while bootstrap or retry is active.
  // 初始化或重试进行时的当前原生安装进度。
  managedRuntimeProgress?: LuaSkillsManagedRuntimeProgressSummary | null;
  // System Lua library directory used by Lua Skills.
  // Lua Skills 使用的系统 Lua 库目录。
  systemLuaLib: string;
  // Exact Vulcan-owned runtime settings file.
  // 精确的 Vulcan 自有运行时设置文件。
  runtimeSettingsFile: string;
  // Complete Vulcan-owned startup settings saved for LuaSkills.
  // 为 LuaSkills 保存的完整 Vulcan 自有启动设置。
  runtimeSettings: LuaSkillsRuntimeSettings;
  // Whether saved startup settings require an application restart.
  // 已保存启动设置是否需要重启应用。
  restartRequired: boolean;
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

// SystemPluginPermissionPreview is the host-validated authorization surface for one local package.
// SystemPluginPermissionPreview 是宿主校验后的单个本地插件包授权界面数据。
export interface SystemPluginPermissionPreview {
  // Stable plugin identity.
  // 稳定插件身份。
  pluginId: string;
  // Exact package version.
  // 精确插件包版本。
  version: string;
  // Host-assigned package origin.
  // 宿主分配的插件包来源。
  origin: "user_installed";
  // Complete capability set that must be approved exactly.
  // 必须完整批准的能力集合。
  capabilities: string[];
  // Exact host Hook points affected by the plugin.
  // 插件影响的精确宿主 Hook 点。
  hookPoints: string[];
  // Exact host UI mount points affected by the plugin.
  // 插件影响的精确宿主界面挂载点。
  uiMountPoints: string[];
  // Declared native process identifiers.
  // 声明的原生进程标识。
  binaryIds: string[];
  // Declared Node.js process identifiers.
  // 声明的 Node.js 进程标识。
  nodeEntryIds: string[];
  // Declared Python process identifiers.
  // 声明的 Python 进程标识。
  pythonEntryIds: string[];
}

// PluginUiInvalidatedEvent describes one revision-only plugin UI data invalidation.
// PluginUiInvalidatedEvent 描述一次仅含修订号的插件界面数据失效。
export interface PluginUiInvalidatedEvent {
  // Exact plugin identity owning the topic.
  // 拥有主题的精确插件身份。
  pluginId: string;
  // Plugin-local invalidation topic.
  // 插件内失效主题。
  topic: string;
  // Monotonic plugin data revision.
  // 单调递增的插件数据修订号。
  revision: number;
}

// PluginSummary describes one plugin or LuaSkills package projected by the service.
// PluginSummary 描述服务端投影的单个插件或 LuaSkills 包。
export interface PluginSummary {
  // Stable plugin identifier.
  // 稳定插件标识。
  id: string;
  // Optional package version for manifest-driven plugins.
  // 清单驱动插件的可选包版本。
  version?: string | null;
  // Human-facing plugin name.
  // 面向用户的插件名称。
  name: string;
  // Plugin kind such as LuaSkill or native Rust.
  // 插件类型，例如 LuaSkill 或原生 Rust。
  kind: string;
  // Current plugin lifecycle status.
  // 当前插件生命周期状态。
  status: string;
  // Stable machine-readable runtime lifecycle state.
  // 稳定的机器可读运行时生命周期状态。
  runtimeState: string;
  // Human-readable plugin capability summary.
  // 人类可读的插件能力摘要。
  capability: string;
  // Localized names keyed by locale identifier.
  // 以语言标识为键的本地化名称。
  nameByLocale: Record<string, string>;
  // Localized capability summaries keyed by locale identifier.
  // 以语言标识为键的本地化能力摘要。
  capabilityByLocale: Record<string, string>;
  // Whether the plugin is currently active in the runtime catalog.
  // 插件当前是否在运行时目录中处于活动状态。
  enabled: boolean;
  // Desired persisted enabled state independent from transient lifecycle state.
  // 与临时生命周期状态独立的持久化期望启用状态。
  desiredEnabled: boolean;
  // Whether required plugin assets are available in the active runtime.
  // 活动运行时中是否存在所需插件资产。
  available: boolean;
  // Number of provider-visible tools declared by the plugin package.
  // 插件包声明的 Provider 可见工具数量。
  toolCount: number;
  // Provider-visible tool metadata used by agent policy editors.
  // 智能体策略编辑器使用的 Provider 可见工具元数据。
  tools: Array<{
    // Stable provider-visible tool identifier.
    // 稳定的 Provider 可见工具标识。
    id: string;
    // Atomic plugin-local tool group identifier.
    // 插件内部原子工具组标识。
    group: string;
    // English model-facing tool description.
    // 面向模型的英文工具说明。
    description: string;
  }>;
  // Number of host hook handlers declared by the plugin package.
  // 插件包声明的宿主 hook 处理器数量。
  hookCount: number;
  // Number of currently retained managed sessions.
  // 当前保留的受管会话数量。
  activeSessionCount: number;
  // Package, dependency, lifecycle, and cleanup diagnostics.
  // 插件包、依赖、生命周期与清理诊断。
  diagnostics: string[];
  // Stable lifecycle manager kind used by the action router.
  // 动作路由使用的稳定生命周期管理类型。
  managementKind: "luaSkill" | "systemPlugin";
  // Optional manifest-owned extension surfaces for real Vulcan plugins.
  // 真实 Vulcan 插件可用的可选清单自有扩展面。
  extension?: {
    // Stable package origin.
    // 稳定的包来源。
    origin: "bundledSystem" | "userInstalled";
    // Exact host capabilities declared by the manifest.
    // 清单声明的精确宿主能力。
    capabilities: string[];
    // Declared host hooks shown during authorization and management.
    // 在授权与管理期间展示的已声明宿主 Hook。
    hooks: Array<{ id: string; point: string }>;
    // Host-callable plugin business actions.
    // 宿主可调用的插件业务 Action。
    actions: string[];
    // Dynamic host UI mounts.
    // 动态宿主界面挂载项。
    uiMounts: Array<{
      // Stable plugin-local mount identifier.
      // 稳定的插件内挂载标识。
      id: string;
      // Stable host-owned mount point.
      // 稳定的宿主自有挂载点。
      point: "skillsPluginDetail" | "workspaceMain" | "conversationRightPanel";
      // Default host navigation label declared by the mount.
      // 挂载点声明的默认宿主导航名称。
      label: string;
      // Localized navigation labels keyed by locale identifier.
      // 以语言标识为键的本地化导航名称。
      labelByLocale: Record<string, string>;
      // Stable host-owned navigation icon identifier.
      // 稳定的宿主导航图标标识。
      icon: "plugin" | "route" | "chart";
      // Named ESM export containing the component factory.
      // 包含组件工厂的 ESM 命名导出。
      exportName: string;
    }>;
  } | null;
}

// PluginUiModuleSource carries one bounded uncompressed plugin ESM module.
// PluginUiModuleSource 携带一个有大小边界的未压缩插件 ESM 模块。
export interface PluginUiModuleSource {
  // Exact owning plugin identifier.
  // 精确的所属插件标识。
  pluginId: string;
  // Exact plugin-local mount identifier.
  // 精确的插件内挂载标识。
  mountId: string;
  // Named ESM export containing the component factory.
  // 包含组件工厂的 ESM 命名导出。
  exportName: string;
  // Untouched UTF-8 ESM source returned by the authenticated service.
  // 由已认证服务返回的原样 UTF-8 ESM 源码。
  source: string;
  // Plugin-owned UI messages grouped by locale and stable message key.
  // 按语言与稳定消息键组织的插件自有界面文案。
  messagesByLocale: Record<string, Record<string, string>>;
}

// PluginTextAssetSource carries one bounded manifest-declared UTF-8 plugin asset.
// PluginTextAssetSource 携带一个有大小边界且由清单声明的 UTF-8 插件资源。
export interface PluginTextAssetSource {
  // Exact owning plugin identifier.
  // 精确的所属插件标识。
  pluginId: string;
  // Exact package-relative path declared by the plugin manifest.
  // 插件清单声明的精确包内相对路径。
  path: string;
  // Untouched UTF-8 text returned by the authenticated service.
  // 由已认证服务返回的原样 UTF-8 文本。
  content: string;
}

// LuaSkillsCatalogUpdate carries one revisioned full LuaSkills domain projection.
// LuaSkillsCatalogUpdate 携带单次带版本的完整 LuaSkills 领域投影。
export interface LuaSkillsCatalogUpdate {
  // Workbench revision committed together with this domain projection.
  // 与当前领域投影一并提交的工作台版本。
  revision: number;
  // Complete LuaSkills catalog after the committed mutation.
  // 已提交变更后的完整 LuaSkills 目录。
  luaSkills: LuaSkillsCatalogSummary;
  // Plugin rows derived from the same committed LuaSkills catalog.
  // 从同一份已提交 LuaSkills 目录派生的插件行。
  plugins: PluginSummary[];
}

// AgentSkillRecordSummary describes one source-aware prompt Agent Skill record.
// AgentSkillRecordSummary 描述单个来源感知提示词 Agent Skill 记录。
export interface AgentSkillRecordSummary {
  // Stable internal key composed from scope and standard name.
  // 由作用域与标准名称组成的稳定内部键。
  skillKey: string;
  // Standard canonical skill name from frontmatter.
  // 来自 frontmatter 的标准规范技能名称。
  name: string;
  // Human-facing display name.
  // 面向用户的显示名称。
  displayName: string;
  // Human-facing short description.
  // 面向用户的简短说明。
  description: string;
  // Localized display metadata keyed by locale tag.
  // 按语言标签索引的本地化展示元数据。
  locales: Record<string, AgentSkillLocaleSummary>;
  // Stable source scope key.
  // 稳定来源作用域键。
  scopeKey: "system" | "user";
  // Whether this source record is effective after same-name resolution.
  // 同名解析后当前来源记录是否生效。
  effective: boolean;
  // Optional system record key overridden by this user record.
  // 当前用户记录覆盖的可选系统记录键。
  overrideOf?: string | null;
  // Whether this effective Agent Skill is enabled.
  // 当前有效 Agent Skill 是否已启用。
  enabled: boolean;
  // Stable primary logical category identifier.
  // 稳定主逻辑分类标识。
  categoryId: string;
  // Stable searchable tags.
  // 稳定可搜索标签。
  tags: string[];
  // Complete standard frontmatter metadata.
  // 完整标准 frontmatter 元数据。
  standard: AgentSkillStandardMetadataSummary;
  // Auditable source provenance.
  // 可审计来源信息。
  origin: AgentSkillOriginSummary;
  // Standard optional resource counts.
  // 标准可选资源数量。
  resources: AgentSkillResourceSummary;
  // Filesystem path to the SKILL.md file.
  // 指向 SKILL.md 文件的文件系统路径。
  skillFile: string;
  // Directory that owns this Agent Skill and bundled resources.
  // 拥有该 Agent Skill 与附带资源的目录。
  baseDir: string;
  // Optional emoji rendered by the agent-style marker.
  // 由智能体同款标记渲染的可选表情图标。
  emoji?: string | null;
  // Optional hexadecimal marker color.
  // 可选十六进制标记颜色。
  color?: string | null;
  // Markdown body without YAML frontmatter.
  // 去除 YAML frontmatter 后的 Markdown 正文。
  body: string;
}

// AgentSkillLocaleSummary describes localized display metadata for one Agent Skill.
// AgentSkillLocaleSummary 描述单个 Agent Skill 的本地化展示元数据。
export interface AgentSkillLocaleSummary {
  // Optional localized display name.
  // 可选本地化显示名称。
  displayName?: string | null;
  // Optional localized compact description.
  // 可选本地化紧凑说明。
  shortDescription?: string | null;
}

// AgentSkillStandardMetadataSummary describes standard Agent Skills frontmatter.
// AgentSkillStandardMetadataSummary 描述标准 Agent Skills frontmatter。
export interface AgentSkillStandardMetadataSummary {
  // Model-facing trigger description.
  // 面向模型的触发说明。
  description: string;
  // Optional license identifier or bundled file reference.
  // 可选协议标识或包内文件引用。
  license?: string | null;
  // Optional environment compatibility statement.
  // 可选环境兼容性说明。
  compatibility?: string | null;
  // Standard string extension metadata.
  // 标准字符串扩展元数据。
  metadata: Record<string, string>;
}

// AgentSkillOriginSummary describes auditable Agent Skill provenance.
// AgentSkillOriginSummary 描述可审计 Agent Skill 来源。
export interface AgentSkillOriginSummary {
  // Stable origin kind.
  // 稳定来源类型。
  kind: string;
  // Optional source label.
  // 可选来源标签。
  label?: string | null;
  // Optional source URL.
  // 可选来源链接。
  url?: string | null;
  // Optional source version.
  // 可选来源版本。
  version?: string | null;
  // Optional immutable revision.
  // 可选不可变修订号。
  revision?: string | null;
  // Whether the source lifecycle is host-managed.
  // 来源生命周期是否由宿主管理。
  managed: boolean;
  // Optional integrity digest.
  // 可选完整性摘要。
  integrity?: string | null;
  // Optional installation timestamp.
  // 可选安装时间戳。
  installedAt?: string | null;
  // Optional last synchronization timestamp.
  // 可选最近同步时间戳。
  updatedAt?: string | null;
}

// AgentSkillResourceSummary describes standard optional resource counts.
// AgentSkillResourceSummary 描述标准可选资源数量。
export interface AgentSkillResourceSummary {
  // Number of script files.
  // 脚本文件数量。
  scripts: number;
  // Number of reference files.
  // 引用文件数量。
  references: number;
  // Number of asset files.
  // 资产文件数量。
  assets: number;
}

// AgentSkillCategorySummary describes one logical Agent Skills category.
// AgentSkillCategorySummary 描述单个逻辑 Agent Skills 分类。
export interface AgentSkillCategorySummary {
  // Stable category identifier.
  // 稳定分类标识。
  categoryId: string;
  // Default category name.
  // 默认分类名称。
  name: string;
  // Optional default category description.
  // 可选默认分类说明。
  description?: string | null;
  // Category emoji.
  // 分类图标。
  emoji: string;
  // Category color.
  // 分类颜色。
  color: string;
  // Stable display order.
  // 稳定展示顺序。
  order: number;
  // Whether the category ships with the host.
  // 当前分类是否由宿主内置。
  builtin: boolean;
  // Localized category metadata keyed by locale tag.
  // 按语言标签索引的本地化分类元数据。
  locales: Record<string, AgentSkillCategoryLocaleSummary>;
}

// AgentSkillInterfaceSaveRequest contains complete user-authored display metadata.
// AgentSkillInterfaceSaveRequest 包含完整用户创作展示元数据。
export interface AgentSkillInterfaceSaveRequest {
  // Optional default human-facing name independent from the model trigger name.
  // 可选默认用户显示名称，独立于模型触发名称。
  displayName: string | null;
  // Optional default compact description for management surfaces.
  // 可选默认管理界面简短说明。
  shortDescription: string | null;
  // Optional emoji rendered by the skill marker.
  // 由技能标记渲染的可选表情图标。
  emoji: string | null;
  // Optional hexadecimal marker color.
  // 可选十六进制标记颜色。
  color: string | null;
  // Authored localized display metadata keyed by BCP 47 locale.
  // 按 BCP 47 语言标签索引的已创作本地化展示元数据。
  locales: Record<string, AgentSkillLocaleSummary>;
}

// AgentSkillSaveRequest is the strict complete package editor payload.
// AgentSkillSaveRequest 是严格完整技能包编辑器载荷。
export interface AgentSkillSaveRequest {
  // Exact existing user package identity for edit or rename operations.
  // 编辑或重命名操作对应的精确现有用户技能包身份。
  originalName: string | null;
  // Exact source-aware record key whose optional resources should be copied.
  // 需要复制可选资源的精确来源记录键。
  sourceSkillKey: string | null;
  // Canonical Agent Skills package name written to frontmatter and directory identity.
  // 写入 frontmatter 与目录身份的 Agent Skills 规范名称。
  name: string;
  // Model-visible trigger description from standard frontmatter.
  // 来自标准 frontmatter 的模型可见触发说明。
  description: string;
  // Optional standard license declaration.
  // 可选标准许可证声明。
  license: string | null;
  // Optional standard environment compatibility declaration.
  // 可选标准环境兼容性声明。
  compatibility: string | null;
  // Standard string-valued extension metadata.
  // 标准字符串值扩展元数据。
  metadata: Record<string, string>;
  // Model-facing SKILL.md instruction body without frontmatter.
  // 不含 frontmatter 的面向模型 SKILL.md 指令正文。
  body: string;
  // Complete user-authored display metadata.
  // 完整用户创作展示元数据。
  interface: AgentSkillInterfaceSaveRequest;
  // Exact logical catalog category identifier.
  // 精确逻辑目录分类标识。
  categoryId: string;
  // Stable normalized catalog search tags.
  // 稳定规范化目录搜索标签。
  tags: string[];
  // Desired per-skill enabled state committed with the package save.
  // 与技能包保存一并提交的目标单技能启用状态。
  enabled: boolean;
}

// AgentSkillCategorySaveRequest is one complete user-defined category payload.
// AgentSkillCategorySaveRequest 是一个完整用户定义分类载荷。
export interface AgentSkillCategorySaveRequest {
  // Stable canonical category identity.
  // 稳定规范分类身份。
  categoryId: string;
  // Default human-facing category name.
  // 默认用户显示分类名称。
  name: string;
  // Optional default category description.
  // 可选默认分类说明。
  description: string | null;
  // Required category emoji.
  // 必填分类图标。
  emoji: string;
  // Required hexadecimal category color.
  // 必填十六进制分类颜色。
  color: string;
  // Stable integer ordering key.
  // 稳定整数排序键。
  order: number;
  // Authored localized category metadata keyed by BCP 47 locale.
  // 按 BCP 47 语言标签索引的已创作本地化分类元数据。
  locales: Record<string, AgentSkillCategoryLocaleSummary>;
}

// AgentSkillCategoryLocaleSummary describes localized category metadata.
// AgentSkillCategoryLocaleSummary 描述本地化分类元数据。
export interface AgentSkillCategoryLocaleSummary {
  // Localized category name.
  // 本地化分类名称。
  name: string;
  // Optional localized category description.
  // 可选本地化分类说明。
  description?: string | null;
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
  // System skill root used by host-managed Agent Skills.
  // 宿主管理 Agent Skills 使用的系统技能根目录。
  systemRoot: string;
  // User skill root used by user-installed Agent Skills.
  // 用户安装 Agent Skills 使用的用户技能根目录。
  userRoot: string;
  // Human-readable notice for the current catalog state.
  // 当前目录状态的人类可读提示。
  notice: string;
  // Complete source-aware Agent Skill records.
  // 完整且包含来源关系的 Agent Skill 记录。
  records: AgentSkillRecordSummary[];
  // Logical category rows synchronized with current records.
  // 与当前记录同步的逻辑分类行。
  categories: AgentSkillCategorySummary[];
  // Non-fatal loading issues.
  // 非致命加载问题。
  errors: AgentSkillLoadIssueSummary[];
}

// AgentSkillsCatalogUpdate carries one revisioned full Agent Skills domain projection.
// AgentSkillsCatalogUpdate 携带单次带版本的完整 Agent Skills 领域投影。
export interface AgentSkillsCatalogUpdate {
  // Workbench revision committed together with this domain projection.
  // 与当前领域投影一并提交的工作台版本。
  revision: number;
  // Complete Agent Skills catalog after the committed mutation.
  // 已提交变更后的完整 Agent Skills 目录。
  agentSkills: AgentSkillsCatalogSummary;
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

// SubagentCatalogUpdate carries one revisioned full agent-catalog domain projection.
// SubagentCatalogUpdate 携带单次带版本的完整智能体目录领域投影。
export interface SubagentCatalogUpdate {
  // Workbench revision committed together with this domain projection.
  // 与当前领域投影一并提交的工作台版本。
  revision: number;
  // Complete agent catalog after the committed mutation.
  // 已提交变更后的完整智能体目录。
  subagentCatalog: SubagentCatalogSummary;
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
