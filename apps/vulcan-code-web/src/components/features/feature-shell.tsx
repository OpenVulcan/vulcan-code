import * as React from "react"
import {
  closestCenter,
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core"
import {
  arrayMove,
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable"
import { CSS } from "@dnd-kit/utilities"
import {
  Archive,
  Bot,
  Boxes,
  BrainCircuit,
  ChevronDown,
  Database,
  FileText,
  FolderTree,
  GitBranch,
  GripVertical,
  Hammer,
  History,
  Image,
  ListRestart,
  Music,
  PanelTopOpen,
  PackagePlus,
  PlugZap,
  Power,
  RefreshCw,
  RotateCw,
  ScrollText,
  Search,
  Save,
  Settings,
  SlidersHorizontal,
  TerminalSquare,
  Trash2,
  User,
  Video,
  Volume2,
  Wrench,
} from "lucide-react"

import { PanelTitle, StatusIcon, StatusPill } from "@/components/shared/status"
import { Badge } from "@/components/ui/badge"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/sidebar"
import { useI18n } from "@/i18n"
import { localMediaSourceUrl } from "@/resources/localMedia"
import type {
  AiCapabilitySummary,
  AiCapabilityTargetSummary,
  AiCapabilityVendorCatalogSummary,
  AiCapabilityVendorSummary,
  AiCapabilityVendorTargetChoiceSummary,
  AgentSkillSummary,
  CommandSessionSummary,
  ComposerAttachmentCapabilitySummary,
  ComposerSlashCommandSummary,
  ConversationSummary,
  FeatureAreaSummary,
  LuaSkillSummary,
  LuaSkillToolSummary,
  LogEntrySummary,
  MainAreaId,
  MemorySummary,
  PluginSummary,
  ProjectSummary,
  ProviderModelSummary,
  ProviderSummary,
  McpServerSummary,
  RuntimeModuleSnapshot,
  SettingsSubAreaId,
  SettingSectionSummary,
  StartupSnapshot,
  ToolCallSummary,
  VmmConfigDraftSummary,
  VmmConfigSaveRequest,
  VmmMemoryActionRequest,
  VmmMemoryActionResult,
  VmmMemorySnapshot,
  WorkbenchSnapshot,
} from "@/types"

// SkillActionProps groups service-backed actions used by the skill page.
// SkillActionProps 分组技能页面使用的服务端动作。
interface SkillActionProps {
  // Refreshes the Lua Skills catalog.
  // 刷新 Lua Skills 目录。
  onRefreshLuaSkills: () => Promise<void>
  // Installs one Lua Skill package.
  // 安装单个 Lua Skill 包。
  onInstallLuaSkill: (skillId: string, source: string, sourceType: string) => Promise<void>
  // Updates one managed Lua Skill package.
  // 更新单个受管 Lua Skill 包。
  onUpdateLuaSkill: (skillId: string) => Promise<void>
  // Toggles one Lua Skill package lifecycle state.
  // 切换单个 Lua Skill 包生命周期状态。
  onSetLuaSkillEnabled: (skillId: string, enabled: boolean) => Promise<void>
  // Uninstalls one ordinary Lua Skill package.
  // 卸载单个普通 Lua Skill 包。
  onUninstallLuaSkill: (skillId: string) => Promise<void>
  // Persists one Lua Skill tool registration state.
  // 持久化单个 Lua Skill 工具注册状态。
  onSetLuaSkillToolRegistrationState: (skillId: string, localName: string, registrationState: string) => Promise<void>
  // Refreshes prompt-based Agent Skills.
  // 刷新基于提示词的 Agent Skills。
  onRefreshAgentSkills: () => Promise<void>
  // Syncs bundled system Agent Skills.
  // 同步内置系统 Agent Skills。
  onInstallSystemAgentSkills: () => Promise<void>
  // Toggles the global Agent Skills switch.
  // 切换 Agent Skills 全局开关。
  onSetAgentSkillsEnabled: (enabled: boolean) => Promise<void>
  // Toggles one Agent Skill global state.
  // 切换单个 Agent Skill 全局状态。
  onSetAgentSkillEnabled: (name: string, enabled: boolean) => Promise<void>
}

// MemoryActionProps groups service-backed actions used by the VMM memory page.
// MemoryActionProps 分组 VMM 记忆页面使用的服务端动作。
interface MemoryActionProps {
  // Runs one VMM memory management action.
  // 执行单个 VMM 记忆管理动作。
  onRunVmmAction: (request: VmmMemoryActionRequest) => Promise<VmmMemoryActionResult>
  // Saves one VMM memory configuration draft.
  // 保存单个 VMM 记忆配置草稿。
  onSaveVmmConfig: (request: VmmConfigSaveRequest) => Promise<void>
}

// ProviderConfigurationSaveRequest describes one provider form submission.
// ProviderConfigurationSaveRequest 描述单次 Provider 表单提交。
interface ProviderConfigurationSaveRequest {
  // Stable provider identifier.
  // 稳定 Provider 标识。
  providerId: string
  // Field values keyed by provider field key.
  // 按 Provider 字段键保存的字段值。
  fields: Record<string, string>
}

// ProviderEnabledModelsSaveRequest describes one provider model enablement submission.
// ProviderEnabledModelsSaveRequest 描述单次 Provider 模型启用提交。
interface ProviderEnabledModelsSaveRequest {
  // Stable provider identifier.
  // 稳定 Provider 标识。
  providerId: string
  // Enabled model identifiers.
  // 启用模型标识集合。
  enabledModelIds: string[]
}

// ProviderModelOverrideSaveRequest describes one model capability override submission.
// ProviderModelOverrideSaveRequest 描述单个模型能力覆盖提交。
interface ProviderModelOverrideSaveRequest {
  // Stable provider identifier.
  // 稳定 Provider 标识。
  providerId: string
  // Stable model identifier.
  // 稳定模型标识。
  modelId: string
  // Optional reasoning capability override.
  // 可选推理能力覆盖值。
  reasoning: boolean | null
  // Optional attachment marker override.
  // 可选附件标记覆盖值。
  attachment: boolean | null
  // Optional context-window limit override.
  // 可选上下文窗口上限覆盖值。
  contextLimit: number | null
  // Optional input-token limit override.
  // 可选输入 Token 上限覆盖值。
  inputLimit: number | null
  // Optional output-token limit override.
  // 可选输出 Token 上限覆盖值。
  outputLimit: number | null
  // Optional input-modality override list.
  // 可选输入模态覆盖列表。
  inputModalities: string[] | null
  // Optional output-modality override list.
  // 可选输出模态覆盖列表。
  outputModalities: string[] | null
  // Optional reasoning profile JSON override.
  // 可选推理档位 JSON 覆盖值。
  reasoningProfileJson: string | null
}

// AiCapabilityVendorSaveRequest describes one shared AI capability vendor submission.
// AiCapabilityVendorSaveRequest 描述单次共享 AI 能力供应商提交。
interface AiCapabilityVendorSaveRequest {
  // Stable vendor identifier.
  // 稳定供应商标识。
  vendorId: string
  // Whether the vendor is enabled.
  // 供应商是否启用。
  enabled: boolean
  // Shared API key or environment placeholder.
  // 共享 API Key 或环境变量占位符。
  apiKey: string
  // Shared base URL override.
  // 共享 Base URL 覆盖值。
  baseUrl: string
  // Shared region override.
  // 共享区域覆盖值。
  region: string
  // Optional connection mode.
  // 可选连接模式。
  connectionMode?: string | null
}

// AiCapabilityTargetEnabledRequest describes one target enabled-state submission.
// AiCapabilityTargetEnabledRequest 描述单次目标启用状态提交。
interface AiCapabilityTargetEnabledRequest {
  // Stable capability kind.
  // 稳定能力类型。
  capabilityKind: string
  // Stable vendor identifier.
  // 稳定供应商标识。
  vendorId: string
  // Stable target alias.
  // 稳定目标别名。
  targetAlias: string
  // Next enabled state.
  // 下一启用状态。
  enabled: boolean
}

// AiCapabilityTargetsSaveRequest describes one ordered capability target-list submission.
// AiCapabilityTargetsSaveRequest 描述单次有序能力目标列表提交。
interface AiCapabilityTargetsSaveRequest {
  // Stable semantic capability kind.
  // 稳定语义能力类型。
  capabilityKind: string
  // Ordered target bindings to keep for this capability.
  // 当前能力需要保留的有序目标绑定集合。
  targets: AiCapabilityTargetSaveInput[]
}

// AiCapabilityTargetSaveInput describes one saved capability target binding.
// AiCapabilityTargetSaveInput 描述单个待保存能力目标绑定。
interface AiCapabilityTargetSaveInput {
  // Stable vendor identifier.
  // 稳定供应商标识。
  vendorId: string
  // Stable target alias under the vendor.
  // 供应商下的稳定目标别名。
  targetAlias: string
  // Provider request model id.
  // Provider 请求模型标识。
  modelId: string
  // Whether the binding is enabled.
  // 该绑定是否启用。
  enabled: boolean
}

// McpServerSaveRequest describes one MCP server form submission.
// McpServerSaveRequest 描述单次 MCP 服务器表单提交。
interface McpServerSaveRequest {
  // Existing server id, or null when creating one.
  // 已有服务器标识，创建时为 null。
  serverId?: string | null
  // Server display name.
  // 服务器显示名称。
  name: string
  // Whether the server is enabled.
  // 服务器是否启用。
  enabled: boolean
  // Transport key.
  // 传输键。
  transport: string
  // Stdio command.
  // Stdio 命令。
  command: string
  // Stdio arguments.
  // Stdio 参数。
  args: string[]
  // Remote endpoint URL.
  // 远端端点 URL。
  url: string
  // Remote bearer token.
  // 远端 Bearer Token。
  bearerToken: string
  // Explicit request headers.
  // 显式请求头。
  headers: Array<{ key: string; value: string }>
  // Headers resolved from environment variables.
  // 从环境变量解析的请求头。
  headersFromEnv: Array<{ key: string; value: string }>
  // OAuth client id.
  // OAuth 客户端标识。
  oauthClientId: string
  // OAuth client secret.
  // OAuth 客户端密钥。
  oauthClientSecret: string
  // OAuth scopes.
  // OAuth scope 列表。
  oauthScopes: string[]
  // OAuth redirect URI.
  // OAuth 回调 URI。
  oauthRedirectUri: string
  // Stdio working directory.
  // Stdio 工作目录。
  cwd: string
  // Stdio environment variables.
  // Stdio 环境变量。
  env: Array<{ key: string; value: string }>
  // Stdio environment passthrough names.
  // Stdio 环境变量透传名称。
  envPassthrough: string[]
}

// SettingsActionProps groups service-backed actions used by the settings page.
// SettingsActionProps 分组设置页面使用的服务端动作。
interface SettingsActionProps {
  // Saves one provider configuration.
  // 保存单个 Provider 配置。
  onSaveProvider: (request: ProviderConfigurationSaveRequest) => Promise<void>
  // Removes one provider configuration.
  // 移除单个 Provider 配置。
  onRemoveProvider: (providerId: string) => Promise<void>
  // Saves enabled model identifiers for one provider.
  // 保存单个 Provider 的启用模型标识集合。
  onSaveProviderEnabledModels: (request: ProviderEnabledModelsSaveRequest) => Promise<void>
  // Saves one provider model capability override.
  // 保存单个 Provider 模型能力覆盖。
  onSaveProviderModelOverride: (request: ProviderModelOverrideSaveRequest) => Promise<void>
  // Saves one shared AI capability vendor.
  // 保存单个共享 AI 能力供应商。
  onSaveAiCapabilityVendor: (request: AiCapabilityVendorSaveRequest) => Promise<void>
  // Removes one shared AI capability vendor.
  // 移除单个共享 AI 能力供应商。
  onRemoveAiCapabilityVendor: (vendorId: string) => Promise<void>
  // Toggles one AI capability target.
  // 切换单个 AI 能力目标。
  onSetAiCapabilityTargetEnabled: (request: AiCapabilityTargetEnabledRequest) => Promise<void>
  // Saves one ordered AI capability target list.
  // 保存单个有序 AI 能力目标列表。
  onSaveAiCapabilityTargets: (request: AiCapabilityTargetsSaveRequest) => Promise<void>
  // Saves one MCP server configuration.
  // 保存单个 MCP 服务器配置。
  onSaveMcpServer: (request: McpServerSaveRequest) => Promise<void>
  // Removes one MCP server configuration.
  // 移除单个 MCP 服务器配置。
  onRemoveMcpServer: (serverId: string) => Promise<void>
}

// SkillSubAreaId identifies the selected skill management category.
// SkillSubAreaId 标识当前选中的技能管理分类。
type SkillSubAreaId = "lua" | "agent"

// SkillHeaderBusyActionId identifies one title-bar skill action currently showing feedback.
// SkillHeaderBusyActionId 标识当前正在标题栏展示反馈的技能动作。
type SkillHeaderBusyActionId = "lua-refresh" | "agent-refresh" | "agent-sync"

// MemorySubAreaId identifies the selected VMM memory management page.
// MemorySubAreaId 标识当前选中的 VMM 记忆管理页面。
type MemorySubAreaId = string

// formatSkillText replaces simple named placeholders in one localized skill string.
// formatSkillText 替换技能本地化字符串中的简单命名占位符。
function formatSkillText(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (match, key) => String(values[key] ?? match))
}

// localizedAgentSkillScope maps backend scope labels into the active UI language.
// localizedAgentSkillScope 将后端作用域标签映射到当前界面语言。
function localizedAgentSkillScope(scope: string, t: ReturnType<typeof useI18n>["t"]): string {
  const normalized = scope.trim().toLowerCase()
  if (normalized === "系统" || normalized === "system") {
    return t("skills.label.system")
  }
  if (normalized === "用户" || normalized === "user") {
    return t("skills.label.user")
  }
  return scope
}

// localizedLiteral chooses a short local component label by the active WebUI locale.
// localizedLiteral 根据当前 WebUI 语言选择组件内短文案。
function localizedLiteral(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en
}

// SkillHeaderSummary describes the active skill page header copy.
// SkillHeaderSummary 描述当前技能页标题文案。
interface SkillHeaderSummary {
  // Header title shown on the first line.
  // 第一行展示的标题。
  title: string
  // Compact metrics shown on the second line.
  // 第二行展示的紧凑统计。
  detail: string
}

// SettingsHeaderSummary describes the active settings page header copy.
// SettingsHeaderSummary 描述当前设置页标题文案。
interface SettingsHeaderSummary {
  // Header title shown on the first line.
  // 第一行展示的标题。
  title: string
  // Compact description shown on the second line.
  // 第二行展示的紧凑说明。
  detail: string
}

// FeatureShell renders non-console pages with the same sidebar and inset structure as the console.
// FeatureShell 使用与控制台一致的侧栏和内容框结构渲染非控制台页面。
function FeatureShell(props: {
  // Active top-level area.
  // 当前顶层区域。
  activeArea: MainAreaId
  // Active top-level feature metadata.
  // 当前顶层功能元数据。
  activeFeature?: FeatureAreaSummary
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
  // Updates the selected settings sub page.
  // 更新设置二级页面选择。
  onSelectSettingsSubArea: (area: SettingsSubAreaId) => void
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps
  // Service-backed VMM memory page actions.
  // 服务端支持的 VMM 记忆页面动作。
  memoryActions: MemoryActionProps
  // Service-backed settings page actions.
  // 服务端支持的设置页面动作。
  settingsActions: SettingsActionProps
  // Startup snapshot used by diagnostics.
  // 诊断页面使用的启动快照。
  startup: StartupSnapshot
  // Workbench snapshot used by feature pages.
  // 功能页面使用的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { locale, t } = useI18n()
  // activeSkillSubArea keeps the left menu and right skill tabs synchronized.
  // activeSkillSubArea 保持左侧菜单与右侧技能页签同步。
  const [activeSkillSubArea, setActiveSkillSubArea] = React.useState<SkillSubAreaId>("lua")
  // activeMemorySubArea keeps VMM left navigation and content synchronized.
  // activeMemorySubArea 保持 VMM 左侧导航与内容区域同步。
  const [activeMemorySubArea, setActiveMemorySubArea] = React.useState<MemorySubAreaId>("status")
  // luaInstallDialogOpen controls the standalone LuaSkills install dialog.
  // luaInstallDialogOpen 控制独立 LuaSkills 安装弹窗。
  const [luaInstallDialogOpen, setLuaInstallDialogOpen] = React.useState(false)
  // skillHeaderBusyAction stores the title-bar action that should show busy feedback.
  // skillHeaderBusyAction 保存需要在标题栏展示忙碌反馈的动作。
  const [skillHeaderBusyAction, setSkillHeaderBusyAction] = React.useState<SkillHeaderBusyActionId | null>(null)
  // providerPickerOpen controls the model-provider catalog dialog from the shared header.
  // providerPickerOpen 从共享标题栏控制模型供应商目录弹窗。
  const [providerPickerOpen, setProviderPickerOpen] = React.useState(false)
  // editingProvider stores the provider currently being configured.
  // editingProvider 保存当前正在配置的模型供应商。
  const [editingProvider, setEditingProvider] = React.useState<ProviderSummary | null>(null)
  // modelListDialogOpen controls the enabled-model configuration dialog from the shared header.
  // modelListDialogOpen 从共享标题栏控制启用模型配置弹窗。
  const [modelListDialogOpen, setModelListDialogOpen] = React.useState(false)
  // capabilityTargetsDialogOpen controls the capability target dialog from the shared header.
  // capabilityTargetsDialogOpen 从共享标题栏控制能力目标配置弹窗。
  const [capabilityTargetsDialogOpen, setCapabilityTargetsDialogOpen] = React.useState(false)
  React.useEffect(() => {
    setCapabilityTargetsDialogOpen(false)
  }, [props.activeSettingsSubArea])
  // runSkillHeaderAction wraps title-bar actions with disabled and spinner feedback.
  // runSkillHeaderAction 为标题栏动作包裹禁用与旋转反馈。
  const runSkillHeaderAction = React.useCallback(async (actionId: SkillHeaderBusyActionId, action: () => Promise<void>) => {
    setSkillHeaderBusyAction(actionId)
    const startedAt = window.performance.now()
    try {
      await action()
    } finally {
      const remainingMs = 450 - (window.performance.now() - startedAt)
      if (remainingMs > 0) {
        await new Promise((resolve) => {
          window.setTimeout(resolve, remainingMs)
        })
      }
      setSkillHeaderBusyAction(null)
    }
  }, [])
  // skillHeaderAction renders the active skill page actions in the shared title bar.
  // skillHeaderAction 在共享标题栏中渲染当前技能页面操作。
  const skillHeaderAction = props.activeArea === "skills" && activeSkillSubArea === "lua" ? (
    <div className="feature-header-actions">
      <button
        className="skill-action-button secondary"
        disabled={skillHeaderBusyAction !== null}
        onClick={() => void runSkillHeaderAction("lua-refresh", props.skillActions.onRefreshLuaSkills)}
        type="button"
      >
        <RefreshCw className={skillHeaderBusyAction === "lua-refresh" ? "spin" : undefined} size={15} />
        <span>{skillHeaderBusyAction === "lua-refresh" ? t("skills.action.refreshing") : t("skills.action.refresh")}</span>
      </button>
      <button className="skill-action-button" onClick={() => setLuaInstallDialogOpen(true)} type="button">
        <PackagePlus size={15} />
        <span>{t("skills.action.installLua")}</span>
      </button>
    </div>
  ) : props.activeArea === "skills" && activeSkillSubArea === "agent" ? (
    <div className="feature-header-actions">
      <button
        className="skill-action-button secondary"
        disabled={skillHeaderBusyAction !== null}
        onClick={() => void runSkillHeaderAction("agent-refresh", props.skillActions.onRefreshAgentSkills)}
        type="button"
      >
        <RefreshCw className={skillHeaderBusyAction === "agent-refresh" ? "spin" : undefined} size={15} />
        <span>{skillHeaderBusyAction === "agent-refresh" ? t("skills.action.refreshing") : t("skills.action.refresh")}</span>
      </button>
      <button
        className="skill-action-button secondary"
        disabled={skillHeaderBusyAction !== null}
        onClick={() => void runSkillHeaderAction("agent-sync", props.skillActions.onInstallSystemAgentSkills)}
        type="button"
      >
        <PackagePlus className={skillHeaderBusyAction === "agent-sync" ? "spin" : undefined} size={15} />
        <span>{skillHeaderBusyAction === "agent-sync" ? t("skills.action.syncing") : t("skills.action.syncSystem")}</span>
      </button>
    </div>
  ) : null
  // settingsHeaderAction renders actions for the active settings page in the shared title bar.
  // settingsHeaderAction 在共享标题栏中渲染当前设置页操作。
  const settingsHeaderAction = props.activeArea === "settings" && props.activeSettingsSubArea === "model-providers" ? (
    <div className="feature-header-actions">
      <button className="skill-action-button" onClick={() => setProviderPickerOpen(true)} type="button">
        <span>{localizedLiteral(locale, "添加 Provider", "Add provider")}</span>
      </button>
    </div>
  ) : props.activeArea === "settings" && props.activeSettingsSubArea === "model-list" ? (
    <div className="feature-header-actions">
      <button className="skill-action-button" onClick={() => setModelListDialogOpen(true)} type="button">
        <span>{localizedLiteral(locale, "调整模型列表", "Configure model list")}</span>
      </button>
    </div>
  ) : props.activeArea === "settings" && settingsSubAreaCapabilityKind(props.activeSettingsSubArea) ? (
    <div className="feature-header-actions">
      <button className="skill-action-button" onClick={() => setCapabilityTargetsDialogOpen(true)} type="button">
        <span>{localizedLiteral(locale, "配置接口", "Configure targets")}</span>
      </button>
    </div>
  ) : null
  // skillHeaderSummary stores the active skill page title and compact metrics.
  // skillHeaderSummary 保存当前技能页标题与紧凑统计。
  const skillHeaderSummary = props.activeArea === "skills"
    ? activeSkillHeaderSummary(activeSkillSubArea, props.workbench, t)
    : null
  // settingsHeaderSummary stores the selected settings section title and description.
  // settingsHeaderSummary 保存当前设置分区标题与说明。
  const settingsHeaderSummary = props.activeArea === "settings"
    ? activeSettingsHeaderSummary(props.activeSettingsSubArea, t)
    : null
  // headerFeature overrides the generic feature title while a skill sub page is active.
  // headerFeature 在技能子页面激活时覆盖通用功能标题。
  const headerFeature = skillHeaderSummary
    ? {
      ...(props.activeFeature ?? { id: "skills" as MainAreaId, label: t("area.skills"), description: "", status: "" }),
      label: skillHeaderSummary.title,
      description: skillHeaderSummary.detail,
    }
    : settingsHeaderSummary
      ? {
        ...(props.activeFeature ?? { id: "settings" as MainAreaId, label: t("area.settings"), description: "", status: "" }),
        label: settingsHeaderSummary.title,
        description: settingsHeaderSummary.detail,
      }
    : props.activeFeature

  return (
    <SidebarProvider className="console-shell feature-shell">
      <FeatureSidebar
        activeArea={props.activeArea}
        activeMemorySubArea={activeMemorySubArea}
        activeSkillSubArea={activeSkillSubArea}
        activeSettingsSubArea={props.activeSettingsSubArea}
        onSelectMemorySubArea={setActiveMemorySubArea}
        onSelectSkillSubArea={setActiveSkillSubArea}
        onSelectSettingsSubArea={props.onSelectSettingsSubArea}
        workbench={props.workbench}
      />
      <SidebarInset>
        <div className="feature-main-shell">
          <FeatureHeader
            action={skillHeaderAction ?? settingsHeaderAction}
            feature={headerFeature}
          />
          <FeatureContent
            activeArea={props.activeArea}
            activeMemorySubArea={activeMemorySubArea}
            capabilityTargetsDialogOpen={capabilityTargetsDialogOpen}
            activeSkillSubArea={activeSkillSubArea}
            activeSettingsSubArea={props.activeSettingsSubArea}
            editingProvider={editingProvider}
            memoryActions={props.memoryActions}
            modelListDialogOpen={modelListDialogOpen}
            onEditingProviderChange={setEditingProvider}
            onCapabilityTargetsDialogOpenChange={setCapabilityTargetsDialogOpen}
            onModelListDialogOpenChange={setModelListDialogOpen}
            onProviderPickerOpenChange={setProviderPickerOpen}
            providerPickerOpen={providerPickerOpen}
            settingsActions={props.settingsActions}
            skillActions={props.skillActions}
            startup={props.startup}
            workbench={props.workbench}
          />
        </div>
        <LuaSkillInstallDialog
          actions={props.skillActions}
          open={luaInstallDialogOpen}
          onOpenChange={setLuaInstallDialogOpen}
        />
      </SidebarInset>
    </SidebarProvider>
  )
}

// FeatureHeader renders the inset title bar for a non-console page.
// FeatureHeader 渲染非控制台页面的内容区标题栏。
function FeatureHeader(props: {
  // Optional right-side action rendered in the feature header.
  // 渲染在功能标题栏右侧的可选操作。
  action?: React.ReactNode
  // Feature metadata displayed in the header.
  // 标题栏中展示的功能元数据。
  feature?: FeatureAreaSummary
}) {
  return (
    <header className="console-header">
      <div className="console-header-main">
        <SidebarTrigger />
        <span className="console-header-separator" aria-hidden="true" />
        <div className="feature-header-title-stack">
          <h1>{props.feature?.label ?? "工作台"}</h1>
          {props.feature?.description ? <small>{props.feature.description}</small> : null}
        </div>
      </div>
      {props.action}
    </header>
  )
}

// FeatureSidebar renders the left navigation for one non-console feature area.
// FeatureSidebar 渲染单个非控制台功能区的左侧导航。
function FeatureSidebar(props: {
  // Active top-level area.
  // 当前顶层区域。
  activeArea: MainAreaId
  // Selected VMM memory sub page.
  // 当前选中的 VMM 记忆子页面。
  activeMemorySubArea: MemorySubAreaId
  // Selected skill sub page.
  // 当前选中的技能子页面。
  activeSkillSubArea: SkillSubAreaId
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
  // Updates the selected skill sub page.
  // 更新选中的技能子页面。
  onSelectSkillSubArea: (area: SkillSubAreaId) => void
  // Updates the selected VMM memory sub page.
  // 更新选中的 VMM 记忆子页面。
  onSelectMemorySubArea: (area: MemorySubAreaId) => void
  // Updates the selected settings sub page.
  // 更新设置二级页面选择。
  onSelectSettingsSubArea: (area: SettingsSubAreaId) => void
  // Workbench snapshot used for sidebar counters.
  // 侧栏计数使用的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { t } = useI18n()
  const feature = primaryFeatureAreas(t).find((area) => area.id === props.activeArea)

  return (
    <Sidebar collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <div aria-current="page" className="vcn-sidebar-static-item">
              {areaIconMap[props.activeArea]}
              <span className="vcn-sidebar-label strong">{feature?.label ?? "工作台"}</span>
            </div>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        {props.activeArea === "settings" ? (
          <SettingsSidebarMenu
            activeSettingsSubArea={props.activeSettingsSubArea}
            onSelectSettingsSubArea={props.onSelectSettingsSubArea}
          />
        ) : (
          <FeatureOverviewMenu
            activeArea={props.activeArea}
            activeMemorySubArea={props.activeMemorySubArea}
            activeSkillSubArea={props.activeSkillSubArea}
            onSelectMemorySubArea={props.onSelectMemorySubArea}
            onSelectSkillSubArea={props.onSelectSkillSubArea}
            workbench={props.workbench}
          />
        )}
      </SidebarContent>
    </Sidebar>
  )
}

// FeatureOverviewMenu renders compact side navigation for non-settings feature pages.
// FeatureOverviewMenu 为非设置功能页渲染紧凑侧栏导航。
function FeatureOverviewMenu(props: {
  // Active feature area used to choose overview rows.
  // 用于选择概览行的当前功能区域。
  activeArea: MainAreaId
  // Selected VMM memory sub page.
  // 当前选中的 VMM 记忆子页面。
  activeMemorySubArea: MemorySubAreaId
  // Selected skill sub page.
  // 当前选中的技能子页面。
  activeSkillSubArea: SkillSubAreaId
  // Updates the selected skill sub page.
  // 更新选中的技能子页面。
  onSelectSkillSubArea: (area: SkillSubAreaId) => void
  // Updates the selected VMM memory sub page.
  // 更新选中的 VMM 记忆子页面。
  onSelectMemorySubArea: (area: MemorySubAreaId) => void
  // Workbench snapshot used for row counters.
  // 行计数使用的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { t } = useI18n()
  const rows = featureSidebarRows(props.activeArea, props.workbench, t)

  return (
    <SidebarGroup>
      <SidebarGroupLabel><PanelTopOpen size={15} /><span>{t("sidebar.overview")}</span></SidebarGroupLabel>
      <SidebarMenu>
        {rows.map((row) => (
          <SidebarMenuItem key={row.id}>
            {props.activeArea === "skills" ? (
              <SidebarMenuButton
                active={skillSidebarRowToSubArea(row.id) === props.activeSkillSubArea}
                className="feature-sidebar-row"
                onClick={() => {
                  const subArea = skillSidebarRowToSubArea(row.id)
                  if (subArea) {
                    props.onSelectSkillSubArea(subArea)
                  }
                }}
                tooltip={row.label}
              >
                {row.icon}
                <span className="vcn-sidebar-label">
                  <strong>{row.label}</strong>
                  <small>{row.detail}</small>
                </span>
              </SidebarMenuButton>
            ) : props.activeArea === "memory" ? (
              <SidebarMenuButton
                active={row.id === props.activeMemorySubArea}
                className="feature-sidebar-row"
                onClick={() => props.onSelectMemorySubArea(row.id)}
                tooltip={row.label}
              >
                {row.icon}
                <span className="vcn-sidebar-label">
                  <strong>{row.label}</strong>
                  <small>{row.detail}</small>
                </span>
              </SidebarMenuButton>
            ) : (
              <div className="vcn-sidebar-static-item feature-sidebar-row">
                {row.icon}
                <span className="vcn-sidebar-label">
                  <strong>{row.label}</strong>
                  <small>{row.detail}</small>
                </span>
              </div>
            )}
          </SidebarMenuItem>
        ))}
      </SidebarMenu>
    </SidebarGroup>
  )
}

// SettingsSidebarMenu renders the settings secondary navigation.
// SettingsSidebarMenu 渲染设置二级导航。
function SettingsSidebarMenu(props: {
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
  // Updates the selected settings sub page.
  // 更新设置二级页面选择。
  onSelectSettingsSubArea: (area: SettingsSubAreaId) => void
}) {
  const { t } = useI18n()
  const { state: sidebarState } = useSidebar()
  const groups = settingsSubAreaGroups(t)
  return (
    <SidebarGroup className="settings-tree-section">
      <SidebarMenu className="settings-tree-list">
        {groups.map((group) => (
          <SettingsSidebarGroupRow
            activeSettingsSubArea={props.activeSettingsSubArea}
            group={group}
            key={group.id}
            onSelectSettingsSubArea={props.onSelectSettingsSubArea}
            sidebarState={sidebarState}
          />
        ))}
      </SidebarMenu>
    </SidebarGroup>
  )
}

// SettingsSidebarGroupRow renders one settings category with always-expanded children.
// SettingsSidebarGroupRow 渲染单个设置分类及其默认展开的子菜单。
function SettingsSidebarGroupRow(props: {
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
  // Settings category group.
  // 设置分类分组。
  group: SettingsSubAreaGroup
  // Updates the selected settings sub page.
  // 更新设置二级页面选择。
  onSelectSettingsSubArea: (area: SettingsSubAreaId) => void
  // Current sidebar collapsed or expanded state.
  // 当前侧栏展开或折叠状态。
  sidebarState: ReturnType<typeof useSidebar>["state"]
}) {
  const groupActive = props.group.items.some((item) => item.id === props.activeSettingsSubArea)
  return (
    <SidebarMenuItem className="project-tree-item settings-tree-item">
      {props.sidebarState === "collapsed" ? (
        <CollapsedSettingsGroupMenu
          activeSettingsSubArea={props.activeSettingsSubArea}
          group={props.group}
          groupActive={groupActive}
          onSelectSettingsSubArea={props.onSelectSettingsSubArea}
        />
      ) : (
        <>
          <div className="project-folder-row settings-folder-row">
            <SidebarMenuButton
              aria-expanded="true"
              className={`project-folder-button settings-folder-button ${groupActive ? "project-folder-active" : ""}`}
              onClick={() => {
                const firstItem = props.group.items[0]
                if (firstItem) {
                  props.onSelectSettingsSubArea(firstItem.id)
                }
              }}
              tooltip={props.group.label}
            >
              {props.group.icon}
              <span className="vcn-sidebar-label project-folder-label">
                <strong>{props.group.label}</strong>
              </span>
              <span className="project-folder-arrow" aria-hidden="true">
                <ChevronDown size={14} />
              </span>
            </SidebarMenuButton>
          </div>
          <SidebarMenu className="project-conversation-list settings-subarea-list">
            {props.group.items.map((area) => (
              <SidebarMenuItem key={area.id}>
                <SidebarMenuButton
                  active={props.activeSettingsSubArea === area.id}
                  className="conversation-tree-button settings-subarea-button"
                  onClick={() => props.onSelectSettingsSubArea(area.id)}
                  tooltip={area.label}
                >
                  <span className="vcn-sidebar-label conversation-tree-label">
                    <strong>{area.label}</strong>
                  </span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </>
      )}
    </SidebarMenuItem>
  )
}

// CollapsedSettingsGroupMenu renders one settings category hover menu while the sidebar is collapsed.
// CollapsedSettingsGroupMenu 在侧栏折叠时渲染单个设置分类的悬停菜单。
function CollapsedSettingsGroupMenu(props: {
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
  // Settings category group.
  // 设置分类分组。
  group: SettingsSubAreaGroup
  // Whether this group contains the active page.
  // 当前分组是否包含活动页面。
  groupActive: boolean
  // Updates the selected settings sub page.
  // 更新设置二级页面选择。
  onSelectSettingsSubArea: (area: SettingsSubAreaId) => void
}) {
  const anchorRef = React.useRef<HTMLDivElement | null>(null)
  const closeTimerRef = React.useRef<number | null>(null)
  const [flyoutOpen, setFlyoutOpen] = React.useState(false)
  const [flyoutPosition, setFlyoutPosition] = React.useState<{ left: number; top: number } | null>(null)
  const clearCloseTimer = React.useCallback(() => {
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current)
      closeTimerRef.current = null
    }
  }, [])
  const openFlyout = React.useCallback(() => {
    clearCloseTimer()
    const rect = anchorRef.current?.getBoundingClientRect()
    if (!rect) {
      return
    }
    setFlyoutPosition({ left: rect.right + 8, top: rect.top })
    setFlyoutOpen(true)
  }, [clearCloseTimer])
  const scheduleClose = React.useCallback(() => {
    clearCloseTimer()
    closeTimerRef.current = window.setTimeout(() => setFlyoutOpen(false), 140)
  }, [clearCloseTimer])
  React.useEffect(() => clearCloseTimer, [clearCloseTimer])
  const selectArea = React.useCallback((areaId: SettingsSubAreaId) => {
    setFlyoutOpen(false)
    props.onSelectSettingsSubArea(areaId)
  }, [props])
  return (
    <>
      <div
        className="collapsed-project-hover-root settings-collapsed-hover-root"
        onMouseEnter={openFlyout}
        onMouseLeave={scheduleClose}
        ref={anchorRef}
      >
        <SidebarMenuButton
          aria-label={props.group.label}
          className={`project-folder-button collapsed-project-button settings-collapsed-button ${props.groupActive ? "project-folder-active" : ""}`}
          onClick={(event) => event.preventDefault()}
        >
          {props.group.icon}
          <span className="vcn-sidebar-label project-folder-label">
            <strong>{props.group.label}</strong>
          </span>
        </SidebarMenuButton>
      </div>
      {flyoutOpen && flyoutPosition ? (
        <div
          aria-label={props.group.label}
          className="collapsed-project-flyout settings-collapsed-flyout"
          onMouseEnter={openFlyout}
          onMouseLeave={scheduleClose}
          role="menu"
          style={{ left: flyoutPosition.left, top: flyoutPosition.top }}
        >
          <div className="collapsed-project-flyout-title" role="presentation">
            {props.group.label}
          </div>
          {props.group.items.map((area) => (
            <button
              className={`collapsed-project-flyout-item ${props.activeSettingsSubArea === area.id ? "active" : ""}`}
              key={area.id}
              onClick={() => selectArea(area.id)}
              role="menuitem"
              type="button"
            >
              {area.icon}
              <span className="collapsed-project-menu-title">{area.label}</span>
            </button>
          ))}
        </div>
      ) : null}
    </>
  )
}

// FeatureContent renders the selected non-console feature area.
// FeatureContent 渲染当前选中的非控制台功能区域。
function FeatureContent(props: {
  // Active top-level feature area.
  // 当前顶层功能区域。
  activeArea: MainAreaId
  // Selected VMM memory sub page.
  // 当前选中的 VMM 记忆子页面。
  activeMemorySubArea: MemorySubAreaId
  // Selected skill sub page.
  // 当前选中的技能子页面。
  activeSkillSubArea: SkillSubAreaId
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
  // Provider currently opened in the edit dialog.
  // 当前在编辑弹窗中打开的模型供应商。
  editingProvider: ProviderSummary | null
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps
  // Service-backed VMM memory page actions.
  // 服务端支持的 VMM 记忆页面动作。
  memoryActions: MemoryActionProps
  // Service-backed settings page actions.
  // 服务端支持的设置页面动作。
  settingsActions: SettingsActionProps
  // Whether the capability target configuration dialog is open.
  // 能力目标配置弹窗是否打开。
  capabilityTargetsDialogOpen: boolean
  // Whether the enabled-model configuration dialog is open.
  // 启用模型配置弹窗是否打开。
  modelListDialogOpen: boolean
  // Updates the provider edit dialog target.
  // 更新模型供应商编辑弹窗目标。
  onEditingProviderChange: (provider: ProviderSummary | null) => void
  // Updates the capability target configuration dialog open state.
  // 更新能力目标配置弹窗打开状态。
  onCapabilityTargetsDialogOpenChange: (open: boolean) => void
  // Updates the enabled-model configuration dialog open state.
  // 更新启用模型配置弹窗打开状态。
  onModelListDialogOpenChange: (open: boolean) => void
  // Updates the add-provider picker open state.
  // 更新添加模型供应商选择弹窗的打开状态。
  onProviderPickerOpenChange: (open: boolean) => void
  // Whether the add-provider picker is open.
  // 添加模型供应商选择弹窗是否打开。
  providerPickerOpen: boolean
  // Startup snapshot used by settings diagnostics.
  // 设置诊断页面使用的启动快照。
  startup: StartupSnapshot
  // Workbench snapshot used by feature pages.
  // 功能页面使用的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  if (props.activeArea === "skills") {
    return (
      <div className="feature-content">
        <SkillsArea
          activeTab={props.activeSkillSubArea}
          skillActions={props.skillActions}
          workbench={props.workbench}
        />
      </div>
    )
  }
  if (props.activeArea === "automation") {
    return (
      <div className="feature-content">
        <AutomationArea commandSessions={props.workbench.commandSessions} modules={props.workbench.modules} />
      </div>
    )
  }
  if (props.activeArea === "plugins") {
    return (
      <div className="feature-content">
        <PluginsArea plugins={props.workbench.plugins} />
      </div>
    )
  }
  if (props.activeArea === "memory") {
    return (
      <div className="feature-content">
        <MemoryArea
          activeSection={props.activeMemorySubArea}
          actions={props.memoryActions}
          memories={props.workbench.memories}
          snapshot={props.workbench.vmmMemory}
        />
      </div>
    )
  }
  if (props.activeArea === "settings") {
    return (
      <div className="feature-content">
        <SettingsFeatureContent
          activeSettingsSubArea={props.activeSettingsSubArea}
          actions={props.settingsActions}
          capabilityTargetsDialogOpen={props.capabilityTargetsDialogOpen}
          editingProvider={props.editingProvider}
          modelListDialogOpen={props.modelListDialogOpen}
          onCapabilityTargetsDialogOpenChange={props.onCapabilityTargetsDialogOpenChange}
          onEditingProviderChange={props.onEditingProviderChange}
          onModelListDialogOpenChange={props.onModelListDialogOpenChange}
          onProviderPickerOpenChange={props.onProviderPickerOpenChange}
          providerPickerOpen={props.providerPickerOpen}
          startup={props.startup}
          workbench={props.workbench}
        />
      </div>
    )
  }

  return null
}

// SettingsFeatureContent renders legacy pages that now live under settings.
// SettingsFeatureContent 渲染已经迁入设置的旧功能页面。
function SettingsFeatureContent(props: {
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
  // Service-backed settings page actions.
  // 服务端支持的设置页面动作。
  actions: SettingsActionProps
  // Whether the capability target configuration dialog is open.
  // 能力目标配置弹窗是否打开。
  capabilityTargetsDialogOpen: boolean
  // Provider currently opened in the edit dialog.
  // 当前在编辑弹窗中打开的模型供应商。
  editingProvider: ProviderSummary | null
  // Whether the enabled-model configuration dialog is open.
  // 启用模型配置弹窗是否打开。
  modelListDialogOpen: boolean
  // Updates the provider edit dialog target.
  // 更新模型供应商编辑弹窗目标。
  onEditingProviderChange: (provider: ProviderSummary | null) => void
  // Updates the capability target configuration dialog open state.
  // 更新能力目标配置弹窗打开状态。
  onCapabilityTargetsDialogOpenChange: (open: boolean) => void
  // Updates the enabled-model configuration dialog open state.
  // 更新启用模型配置弹窗打开状态。
  onModelListDialogOpenChange: (open: boolean) => void
  // Updates the add-provider picker open state.
  // 更新添加模型供应商选择弹窗的打开状态。
  onProviderPickerOpenChange: (open: boolean) => void
  // Whether the add-provider picker is open.
  // 添加模型供应商选择弹窗是否打开。
  providerPickerOpen: boolean
  // Startup snapshot used by diagnostics.
  // 诊断页面使用的启动快照。
  startup: StartupSnapshot
  // Workbench snapshot used by settings pages.
  // 设置页面使用的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  if (props.activeSettingsSubArea === "model-providers") {
    return (
      <ProvidersArea
        actions={props.actions}
        editingProvider={props.editingProvider}
        onEditingProviderChange={props.onEditingProviderChange}
        onProviderPickerOpenChange={props.onProviderPickerOpenChange}
        pickerOpen={props.providerPickerOpen}
        providers={props.workbench.providers}
      />
    )
  }
  if (props.activeSettingsSubArea === "model-list") {
    return <ModelListSettingsArea actions={props.actions} dialogOpen={props.modelListDialogOpen} onDialogOpenChange={props.onModelListDialogOpenChange} providers={props.workbench.providers} />
  }
  if (props.activeSettingsSubArea === "ai-vendors") {
    return <AiCapabilityVendorsArea actions={props.actions} capabilities={props.workbench.aiCapabilities ?? []} catalog={props.workbench.aiCapabilityVendorCatalog ?? []} providers={props.workbench.providers} vendors={props.workbench.aiCapabilityVendors ?? []} />
  }
  if (settingsSubAreaCapabilityKind(props.activeSettingsSubArea)) {
    return (
      <AiCapabilitySettingsArea
        actions={props.actions}
        activeArea={props.activeSettingsSubArea}
        capabilities={props.workbench.aiCapabilities ?? []}
        configDialogOpen={props.capabilityTargetsDialogOpen}
        onConfigDialogOpenChange={props.onCapabilityTargetsDialogOpenChange}
        vendors={props.workbench.aiCapabilityVendors ?? []}
      />
    )
  }
  if (props.activeSettingsSubArea === "mcp-servers") {
    return <McpSettingsArea actions={props.actions} logs={props.workbench.logs} modules={props.workbench.modules} servers={props.workbench.mcpServers ?? []} startup={props.startup} />
  }
  if (props.activeSettingsSubArea === "runtime-diagnostics") {
    return <LogsArea startup={props.startup} logs={props.workbench.logs} />
  }
  if (props.activeSettingsSubArea === "prompt-composer") {
    return (
      <PromptComposerSettingsArea
        attachmentCapabilities={props.workbench.composerAttachmentCapabilities ?? []}
        reasoningSelection={props.workbench.reasoningSelection ?? null}
        slashCommands={props.workbench.composerSlashCommands ?? []}
      />
    )
  }
  if (props.activeSettingsSubArea === "about-licenses") {
    return <AboutLicensesSettingsArea appVersion={props.workbench.appVersion} />
  }
  if (props.activeSettingsSubArea === "archived-restore") {
    return <ArchivedRestoreSettingsArea conversations={props.workbench.archivedConversations ?? []} projects={props.workbench.projects} />
  }
  return (
    <ProvidersArea
      actions={props.actions}
      editingProvider={props.editingProvider}
      onEditingProviderChange={props.onEditingProviderChange}
      onProviderPickerOpenChange={props.onProviderPickerOpenChange}
      pickerOpen={props.providerPickerOpen}
      providers={props.workbench.providers}
    />
  )
}

// SkillsArea renders LuaSkills and tool capability status.
// SkillsArea 渲染 LuaSkills 与工具能力状态。
// activeSettingsHeaderSummary returns the title and description for the current settings page.
// activeSettingsHeaderSummary 返回当前设置页的标题与说明。
function activeSettingsHeaderSummary(activeArea: SettingsSubAreaId, t: ReturnType<typeof useI18n>["t"]): SettingsHeaderSummary {
  const area = settingsSubAreas(t).find((entry) => entry.id === activeArea)
  return {
    title: area?.label ?? t("area.settings"),
    detail: area?.description ?? t("area.settings.description"),
  }
}

// activeSkillHeaderSummary returns the title and metrics for the current skill sub page.
// activeSkillHeaderSummary 返回当前技能子页面的标题与统计。
function activeSkillHeaderSummary(activeTab: SkillSubAreaId, workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): SkillHeaderSummary {
  if (activeTab === "agent") {
    const enabledCount = workbench.agentSkills.skills.filter((skill) => skill.enabled).length
    return {
      title: "Agent Skills",
      detail: formatSkillText(t("skills.header.agentDetail"), {
        total: workbench.agentSkills.skills.length,
        enabled: enabledCount,
        warnings: workbench.agentSkills.errors.length,
      }),
    }
  }
  const visibleToolCount = workbench.luaSkills.skills.reduce((count, skill) => count + skill.entryCount, 0)
  const implicitToolCount = workbench.luaSkills.skills.reduce((count, skill) => (
    count + skill.tools.filter((tool) => tool.registrationState === "implicit").length
  ), 0)
  return {
    title: "Lua Skills",
    detail: formatSkillText(t("skills.header.luaDetail"), {
      total: workbench.luaSkills.skills.length,
      visible: visibleToolCount,
      implicit: implicitToolCount,
    }),
  }
}

function SkillsArea(props: {
  // Selected skill tab.
  // 当前选中的技能页签。
  activeTab: SkillSubAreaId
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps
  // Workbench snapshot containing Lua Skills and Agent Skills catalogs.
  // 包含 Lua Skills 与 Agent Skills 目录的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  return (
    <section className="skill-management-surface">
      {props.activeTab === "lua" ? (
        <LuaSkillsPanel
          actions={props.skillActions}
          skills={props.workbench.luaSkills.skills}
        />
      ) : (
        <AgentSkillsPanel
          actions={props.skillActions}
          errors={props.workbench.agentSkills.errors}
          skills={props.workbench.agentSkills.skills}
        />
      )}
    </section>
  )
}

// LuaSkillsPanel renders package lifecycle controls and runtime tool registration controls.
// LuaSkillsPanel 渲染包生命周期控件与运行时工具注册控件。
function LuaSkillsPanel(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps
  // Lua Skill rows.
  // Lua Skill 行。
  skills: LuaSkillSummary[]
}) {
  const { t } = useI18n()
  const [busyAction, setBusyAction] = React.useState<string | null>(null)
  // detailSkill stores the Lua Skill selected for the details dialog.
  // detailSkill 保存当前打开详情弹窗的 Lua Skill。
  const [detailSkill, setDetailSkill] = React.useState<LuaSkillSummary | null>(null)
  // toolsSkill stores the Lua Skill selected for the tools dialog.
  // toolsSkill 保存当前打开工具弹窗的 Lua Skill。
  const [toolsSkill, setToolsSkill] = React.useState<LuaSkillSummary | null>(null)

  const runAction = React.useCallback(async (label: string, action: () => Promise<void>) => {
    setBusyAction(label)
    try {
      await action()
    } finally {
      setBusyAction(null)
    }
  }, [])

  return (
    <div className="skill-page-body lua-skill-page-body">
      <div className="skill-card-list">
        {props.skills.length === 0 ? (
          <article className="skill-empty-card">
            <StatusPill status="pending" label="pending" />
            <strong>{t("skills.lua.emptyTitle")}</strong>
            <span>{t("skills.lua.emptyDetail")}</span>
          </article>
        ) : props.skills.map((skill) => (
          <LuaSkillCard
            actions={props.actions}
            busy={busyAction !== null}
            key={skill.skillId}
            onOpenDetail={setDetailSkill}
            onOpenTools={setToolsSkill}
            onRunAction={runAction}
            skill={skill}
          />
        ))}
      </div>
      <LuaSkillDetailDialog
        open={detailSkill !== null}
        skill={detailSkill}
        onOpenChange={(open) => {
          if (!open) {
            setDetailSkill(null)
          }
        }}
      />
      <LuaSkillToolsDialog
        actions={props.actions}
        busy={busyAction !== null}
        open={toolsSkill !== null}
        skill={toolsSkill}
        onOpenChange={(open) => {
          if (!open) {
            setToolsSkill(null)
          }
        }}
      />
    </div>
  )
}

// LuaSkillInstallDialog renders the standalone LuaSkills install form.
// LuaSkillInstallDialog 渲染独立的 LuaSkills 安装表单。
function LuaSkillInstallDialog(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps
  // Whether the install dialog is open.
  // 安装弹窗当前是否打开。
  open: boolean
  // Updates the install dialog open state.
  // 更新安装弹窗打开状态。
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useI18n()
  // sourceType stores the selected install source kind.
  // sourceType 保存当前选择的安装来源类型。
  const [sourceType, setSourceType] = React.useState<"github" | "url">("github")
  // source stores the install source locator.
  // source 保存安装来源定位值。
  const [source, setSource] = React.useState("")
  // installing tracks the active install request.
  // installing 跟踪当前安装请求。
  const [installing, setInstalling] = React.useState(false)

  React.useEffect(() => {
    if (props.open) {
      setSourceType("github")
      setSource("")
      setInstalling(false)
    }
  }, [props.open])

  // submitInstall validates and runs one LuaSkills install request.
  // submitInstall 校验并执行单次 LuaSkills 安装请求。
  const submitInstall = React.useCallback(async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (source.trim().length === 0) {
      return
    }
    setInstalling(true)
    try {
      await props.actions.onInstallLuaSkill("", source, sourceType)
      props.onOpenChange(false)
    } finally {
      setInstalling(false)
    }
  }, [props.actions, props.onOpenChange, source, sourceType])

  return (
    <Dialog open={props.open} onOpenChange={(open) => !installing && props.onOpenChange(open)}>
      <DialogContent className="lua-skill-install-dialog">
        <DialogHeader>
          <DialogTitle>{t("skills.lua.installTitle")}</DialogTitle>
          <DialogDescription>{t("skills.lua.installDescription")}</DialogDescription>
        </DialogHeader>
        <form className="lua-skill-install-form" onSubmit={submitInstall}>
          <label className="lua-skill-install-field">
            <span>{t("skills.lua.sourceType")}</span>
            <select disabled={installing} value={sourceType} onChange={(event) => setSourceType(event.target.value as "github" | "url")}>
              <option value="github">GitHub</option>
              <option value="url">URL</option>
            </select>
          </label>
          <label className="lua-skill-install-field">
            <span>{t("skills.lua.installSource")}</span>
            <input
              autoFocus
              disabled={installing}
              placeholder={sourceType === "github" ? t("skills.lua.placeholderGithub") : t("skills.lua.placeholderUrl")}
              value={source}
              onChange={(event) => setSource(event.target.value)}
            />
          </label>
          <DialogFooter>
            <button className="skill-action-button secondary" disabled={installing} onClick={() => props.onOpenChange(false)} type="button">{t("skills.action.cancel")}</button>
            <button className="skill-action-button" disabled={installing || source.trim().length === 0} type="submit">
              <PackagePlus size={15} />
              <span>{installing ? t("skills.action.installing") : t("skills.action.install")}</span>
            </button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

// LuaSkillCard renders one Lua Skill package and its tool rows.
// LuaSkillCard 渲染单个 Lua Skill 包及其工具行。
function LuaSkillCard(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps
  // Whether another skill action is running.
  // 是否已有其他技能动作正在运行。
  busy: boolean
  // Opens the details dialog for this Lua Skill.
  // 打开当前 Lua Skill 的详情弹窗。
  onOpenDetail: (skill: LuaSkillSummary) => void
  // Opens the tools dialog for this Lua Skill.
  // 打开当前 Lua Skill 的工具弹窗。
  onOpenTools: (skill: LuaSkillSummary) => void
  // Runs a labeled async action with page-level busy state.
  // 使用页面级忙碌状态运行带标签的异步动作。
  onRunAction: (label: string, action: () => Promise<void>) => Promise<void>
  // Lua Skill row rendered by this card.
  // 当前卡片渲染的 Lua Skill 行。
  skill: LuaSkillSummary
}) {
  const { t } = useI18n()
  const source = props.skill.sourceLocator ?? t("skills.source.unknown")
  const sourceTypeLabel = (props.skill.sourceType ?? "source").toUpperCase()
  const explicitToolCount = props.skill.tools.filter((tool) => tool.registrationState === "explicit").length
  const implicitToolCount = props.skill.tools.filter((tool) => tool.registrationState === "implicit").length
  const disabledToolCount = props.skill.tools.filter((tool) => tool.registrationState === "disabled").length

  return (
    <article className="skill-card lua-skill-card">
      <header className="skill-card-header">
        <div className="skill-card-title">
          <div className="skill-card-title-line">
            <strong>{props.skill.displayName}</strong>
            <div className="skill-card-title-meta">
              <Badge variant="secondary">v{props.skill.version}</Badge>
            </div>
          </div>
            <div className="skill-card-badges">
              {props.skill.enabled ? (
              <Badge className="skill-card-enabled-badge">{t("skills.status.enabled")}</Badge>
            ) : (
              <Badge variant="destructive">{t("skills.status.stopped")}</Badge>
            )}
            <Badge variant="outline">{sourceTypeLabel}</Badge>
          </div>
        </div>
      </header>
      <div className="skill-card-summary">
        <span>{source}</span>
      </div>
      <div className="skill-card-metrics">
        <span>{props.skill.rootName === "ROOT" ? t("skills.label.system") : props.skill.rootName}</span>
        {props.skill.enabled ? (
          <>
            <span>{t("skills.label.explicit")} {explicitToolCount}</span>
            <span>{t("skills.label.implicit")} {implicitToolCount}</span>
            {disabledToolCount > 0 ? <span>{t("skills.label.closed")} {disabledToolCount}</span> : null}
          </>
        ) : null}
      </div>
      <div className="skill-actions-row">
        <button className="skill-action-button secondary" onClick={() => props.onOpenDetail(props.skill)} type="button">
          <FileText size={15} />
          <span>{t("skills.action.details")}</span>
        </button>
        <button className="skill-action-button secondary" onClick={() => props.onOpenTools(props.skill)} type="button">
          <Wrench size={15} />
          <span>{t("skills.action.tools")} {props.skill.tools.length}</span>
        </button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button className="skill-action-button secondary" disabled={props.busy} type="button">
              <span>{t("skills.action.action")}</span>
              <ChevronDown size={14} />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="lua-skill-action-menu">
            <DropdownMenuItem onSelect={() => void props.onRunAction("toggle", () => props.actions.onSetLuaSkillEnabled(props.skill.skillId, !props.skill.enabled))}>
              <Power size={15} />
              <span>{props.skill.enabled ? t("skills.action.disable") : t("skills.action.enable")}</span>
            </DropdownMenuItem>
            {props.skill.managed ? (
              <DropdownMenuItem onSelect={() => void props.onRunAction("update", () => props.actions.onUpdateLuaSkill(props.skill.skillId))}>
                <RotateCw size={15} />
                <span>{t("skills.action.update")}</span>
              </DropdownMenuItem>
            ) : null}
            {!props.skill.systemOwned ? (
              <DropdownMenuItem variant="destructive" onSelect={() => void props.onRunAction("uninstall", () => props.actions.onUninstallLuaSkill(props.skill.skillId))}>
                <Trash2 size={15} />
                <span>{t("skills.action.delete")}</span>
              </DropdownMenuItem>
            ) : null}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </article>
  )
}

// LuaSkillDetailDialog renders package metadata for one Lua Skill.
// LuaSkillDetailDialog 渲染单个 Lua Skill 的包元数据。
function LuaSkillDetailDialog(props: {
  // Whether the details dialog is open.
  // 详情弹窗当前是否打开。
  open: boolean
  // Lua Skill selected for details.
  // 当前查看详情的 Lua Skill。
  skill: LuaSkillSummary | null
  // Updates the details dialog open state.
  // 更新详情弹窗打开状态。
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useI18n()
  const skill = props.skill
  const source = skill?.sourceLocator ? `${skill.sourceType ?? "source"} · ${skill.sourceLocator}` : t("skills.source.unknown")
  return (
    <Dialog open={props.open && skill !== null} onOpenChange={props.onOpenChange}>
      <DialogContent className="lua-skill-detail-dialog">
        <DialogHeader>
          <DialogTitle>{skill?.displayName ?? t("skills.lua.detailsTitle")}</DialogTitle>
          <DialogDescription>{skill ? `${skill.skillId} · v${skill.version}` : t("skills.lua.detailsDescription")}</DialogDescription>
        </DialogHeader>
        {skill ? (
          <div className="lua-skill-detail-grid">
            <div>
              <span>{t("skills.label.currentStatus")}</span>
              <strong>{skill.enabled ? t("skills.status.enabled") : t("skills.status.stopped")}</strong>
            </div>
            <div>
              <span>{t("skills.label.installLayer")}</span>
              <strong>{skill.rootName === "ROOT" ? t("skills.label.system") : skill.rootName}</strong>
            </div>
            <div>
              <span>{t("skills.label.installManagement")}</span>
              <strong>{skill.managed ? t("skills.label.managedInstall") : t("skills.label.unmanagedInstall")}</strong>
            </div>
            <div>
              <span>{t("skills.label.runtimeEntries")}</span>
              <strong>{skill.entryCount}</strong>
            </div>
            <div className="wide">
              <span>{t("skills.label.source")}</span>
              <strong>{source}</strong>
            </div>
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}

// LuaSkillToolsDialog renders runtime tool registration controls in a dialog.
// LuaSkillToolsDialog 在弹窗中渲染运行时工具注册控件。
function LuaSkillToolsDialog(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps
  // Whether another skill action is running.
  // 是否已有其他技能动作正在运行。
  busy: boolean
  // Whether the tools dialog is open.
  // 工具弹窗当前是否打开。
  open: boolean
  // Lua Skill selected for tool management.
  // 当前管理工具的 Lua Skill。
  skill: LuaSkillSummary | null
  // Updates the tools dialog open state.
  // 更新工具弹窗打开状态。
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useI18n()
  const skill = props.skill
  return (
    <Dialog open={props.open && skill !== null} onOpenChange={props.onOpenChange}>
      <DialogContent className="lua-skill-tools-dialog">
        <DialogHeader>
          <DialogTitle>{skill ? `${skill.displayName} ${t("skills.action.tools")}` : t("skills.lua.toolsTitle")}</DialogTitle>
          <DialogDescription>{skill ? `${skill.skillId} · ${skill.tools.length} ${t("skills.label.runtimeTools")}` : t("skills.lua.toolsDescription")}</DialogDescription>
        </DialogHeader>
        <div className="skill-tool-list dialog-tool-list">
          {skill && skill.tools.length === 0 ? (
            <p>{t("skills.lua.toolsEmpty")}</p>
          ) : skill?.tools.map((tool) => (
            <LuaSkillToolRow
              actions={props.actions}
              busy={props.busy}
              key={`${skill.skillId}-${tool.localName}`}
              skillId={skill.skillId}
              tool={tool}
            />
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}

// LuaSkillToolRow renders one runtime tool registration selector.
// LuaSkillToolRow 渲染单个运行时工具注册选择器。
function LuaSkillToolRow(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps
  // Whether another skill action is running.
  // 是否已有其他技能动作正在运行。
  busy: boolean
  // Parent Lua Skill identifier.
  // 父级 Lua Skill 标识。
  skillId: string
  // Runtime tool row.
  // 运行时工具行。
  tool: LuaSkillToolSummary
}) {
  const { t } = useI18n()
  // selectedState mirrors the select value immediately before backend synchronization completes.
  // selectedState 在后端同步完成前立即镜像选择框取值。
  const [selectedState, setSelectedState] = React.useState(props.tool.registrationState)
  // syncing tracks the background persistence request for this single tool row.
  // syncing 跟踪当前工具行的后台持久化请求。
  const [syncing, setSyncing] = React.useState(false)

  React.useEffect(() => {
    setSelectedState(props.tool.registrationState)
  }, [props.tool.registrationState])

  // handleRegistrationChange applies an optimistic local switch and reverts on persistence failure.
  // handleRegistrationChange 先执行乐观本地切换，并在持久化失败时回滚。
  const handleRegistrationChange = React.useCallback(async (nextState: LuaSkillToolSummary["registrationState"]) => {
    const previousState = selectedState
    if (nextState === previousState || syncing || props.busy) {
      return
    }
    setSelectedState(nextState)
    setSyncing(true)
    try {
      await props.actions.onSetLuaSkillToolRegistrationState(props.skillId, props.tool.localName, nextState)
    } catch (error) {
      setSelectedState(previousState)
      console.warn("sync LuaSkill tool registration state failed", error)
    } finally {
      setSyncing(false)
    }
  }, [props.actions, props.busy, props.skillId, props.tool.localName, selectedState, syncing])

  return (
    <article className="skill-tool-row">
      <div className="skill-tool-row-header">
        <div>
          <strong>{props.tool.localName}</strong>
          <span>{props.tool.canonicalName}</span>
        </div>
        <div className="skill-tool-control">
          <div className="skill-tool-segmented" role="group" aria-label={formatSkillText(t("skills.lua.toolModeAria"), { name: props.tool.localName })}>
            <button
              className={selectedState === "explicit" ? "active" : ""}
              disabled={props.busy || syncing}
              onClick={() => handleRegistrationChange("explicit")}
              type="button"
            >
              {t("skills.lua.toolExplicit")}
            </button>
            <button
              className={selectedState === "implicit" ? "active" : ""}
              disabled={props.busy || syncing}
              onClick={() => handleRegistrationChange("implicit")}
              type="button"
            >
              {t("skills.lua.toolImplicit")}
            </button>
            <button
              className={selectedState === "disabled" ? "active" : ""}
              disabled={props.busy || syncing}
              onClick={() => handleRegistrationChange("disabled")}
              type="button"
            >
              {t("skills.lua.toolDisabled")}
            </button>
          </div>
          {syncing ? <small className="skill-sync-status">{t("skills.lua.syncing")}</small> : null}
        </div>
      </div>
      <div className="skill-tool-description">
        <p>{props.tool.description || props.tool.registrationHelp}</p>
      </div>
    </article>
  )
}

// AgentSkillsPanel renders prompt-based skill controls and detail previews.
// AgentSkillsPanel 渲染基于提示词的技能控件与详情预览。
function AgentSkillsPanel(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps
  // Non-fatal loading errors.
  // 非致命加载错误。
  errors: Array<{ path: string; message: string }>
  // Agent Skill rows.
  // Agent Skill 行。
  skills: AgentSkillSummary[]
}) {
  const { t } = useI18n()
  const [busyAction, setBusyAction] = React.useState<string | null>(null)
  // detailSkill stores the Agent Skill selected for the details dialog.
  // detailSkill 保存当前打开详情弹窗的 Agent Skill。
  const [detailSkill, setDetailSkill] = React.useState<AgentSkillSummary | null>(null)
  const runAction = React.useCallback(async (label: string, action: () => Promise<void>) => {
    setBusyAction(label)
    try {
      await action()
    } finally {
      setBusyAction(null)
    }
  }, [])

  return (
    <div className="skill-page-body agent-skill-page-body">
      {props.errors.length > 0 ? (
        <div className="skill-warning-list">
          {props.errors.map((error) => (
            <article className="skill-warning-row" key={`${error.path}-${error.message}`}>
              <strong>{error.path}</strong>
              <span>{error.message}</span>
            </article>
          ))}
        </div>
      ) : null}
      <div className="skill-card-list agent-skill-card-list">
        {props.skills.length === 0 ? (
          <article className="skill-empty-card">
            <StatusPill status="pending" label="pending" />
            <strong>{t("skills.agent.emptyTitle")}</strong>
            <span>{t("skills.agent.emptyDetail")}</span>
          </article>
        ) : props.skills.map((skill) => (
          <AgentSkillCard
            actions={props.actions}
            busy={busyAction !== null}
            key={skill.name}
            onOpenDetail={setDetailSkill}
            onRunAction={runAction}
            skill={skill}
          />
        ))}
      </div>
      <AgentSkillDetailDialog
        open={detailSkill !== null}
        skill={detailSkill}
        onOpenChange={(open) => {
          if (!open) {
            setDetailSkill(null)
          }
        }}
      />
    </div>
  )
}

// AgentSkillCard renders one prompt skill and optional body details.
// AgentSkillCard 渲染单个提示词技能及可选正文详情。
function AgentSkillCard(props: {
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  actions: SkillActionProps
  // Whether another skill action is running.
  // 是否已有其他技能动作正在运行。
  busy: boolean
  // Opens the details dialog for this Agent Skill.
  // 打开当前 Agent Skill 的详情弹窗。
  onOpenDetail: (skill: AgentSkillSummary) => void
  // Runs a labeled async action with page-level busy state.
  // 使用页面级忙碌状态运行带标签的异步动作。
  onRunAction: (label: string, action: () => Promise<void>) => Promise<void>
  // Agent Skill row rendered by this card.
  // 当前卡片渲染的 Agent Skill 行。
  skill: AgentSkillSummary
}) {
  const { t } = useI18n()
  const fallbackIcon = props.skill.displayName.trim().charAt(0).toUpperCase() || "*"
  const scopeLabel = localizedAgentSkillScope(props.skill.scope, t)

  return (
    <article className="skill-card agent-skill-card">
      <header className="skill-card-header">
        <div className="agent-skill-title">
          <AgentSkillIcon fallback={fallbackIcon} iconPath={props.skill.iconPath} />
          <div>
            <strong>{props.skill.displayName}</strong>
            <div className="skill-card-badges">
              {props.skill.enabled ? (
                <Badge className="skill-card-enabled-badge">{t("skills.status.enabled")}</Badge>
              ) : (
                <Badge variant="destructive">{t("skills.status.stopped")}</Badge>
              )}
              <Badge variant="outline">{scopeLabel}</Badge>
            </div>
          </div>
        </div>
      </header>
      <p>{props.skill.description}</p>
      <div className="skill-actions-row">
        <button className="skill-action-button" disabled={props.busy} onClick={() => props.onRunAction("agent-toggle", () => props.actions.onSetAgentSkillEnabled(props.skill.name, !props.skill.enabled))} type="button">
          <Power size={15} />
          <span>{props.skill.enabled ? t("skills.action.disable") : t("skills.action.enable")}</span>
        </button>
        <button className="skill-action-button secondary" onClick={() => props.onOpenDetail(props.skill)} type="button">
          <FileText size={15} />
          <span>{t("skills.action.details")}</span>
        </button>
      </div>
    </article>
  )
}

// AgentSkillIcon renders the resolved Agent Skill icon with a text fallback.
// AgentSkillIcon 渲染解析后的 Agent Skill 图标，并提供文本兜底。
function AgentSkillIcon(props: {
  // Fallback text used when the icon cannot be loaded.
  // 图标无法加载时使用的兜底文本。
  fallback: string
  // Optional local icon path resolved by the service.
  // 服务端解析出的可选本地图标路径。
  iconPath?: string | null
}) {
  // failed tracks image loading failure so the fallback stays visible.
  // failed 跟踪图片加载失败以持续展示兜底。
  const [failed, setFailed] = React.useState(false)
  React.useEffect(() => {
    setFailed(false)
  }, [props.iconPath])

  if (!props.iconPath || failed) {
    return <span className="agent-skill-icon fallback">{props.fallback}</span>
  }
  return (
    <span className="agent-skill-icon">
      <img alt="" src={localMediaSourceUrl(props.iconPath)} onError={() => setFailed(true)} />
    </span>
  )
}

// AgentSkillDetailDialog renders Agent Skill metadata and Markdown body in a dialog.
// AgentSkillDetailDialog 在弹窗中渲染 Agent Skill 元数据与 Markdown 正文。
function AgentSkillDetailDialog(props: {
  // Whether the details dialog is open.
  // 详情弹窗当前是否打开。
  open: boolean
  // Agent Skill selected for details.
  // 当前查看详情的 Agent Skill。
  skill: AgentSkillSummary | null
  // Updates the details dialog open state.
  // 更新详情弹窗打开状态。
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useI18n()
  const skill = props.skill
  const scopeLabel = skill ? localizedAgentSkillScope(skill.scope, t) : ""
  return (
    <Dialog open={props.open && skill !== null} onOpenChange={props.onOpenChange}>
      <DialogContent className="agent-skill-detail-dialog">
        <DialogHeader>
          <DialogTitle>{skill?.displayName ?? t("skills.agent.detailsTitle")}</DialogTitle>
          <DialogDescription>{skill ? `${scopeLabel} · ${skill.enabled ? t("skills.status.enabled") : t("skills.status.stopped")}` : t("skills.agent.detailsDescription")}</DialogDescription>
        </DialogHeader>
        {skill ? (
          <div className="agent-skill-detail-body">
            <div className="lua-skill-detail-grid">
              <div>
                <span>{t("skills.label.currentStatus")}</span>
                <strong>{skill.enabled ? t("skills.status.enabled") : t("skills.status.stopped")}</strong>
              </div>
              <div>
                <span>{t("skills.label.scope")}</span>
                <strong>{scopeLabel}</strong>
              </div>
              <div className="wide">
                <span>{t("skills.label.skillFile")}</span>
                <strong>{skill.skillFile}</strong>
              </div>
              <div className="wide">
                <span>{t("skills.label.skillDir")}</span>
                <strong>{skill.baseDir}</strong>
              </div>
              {skill.iconPath ? (
                <div className="wide">
                  <span>{t("skills.label.iconPath")}</span>
                  <strong>{skill.iconPath}</strong>
                </div>
              ) : null}
            </div>
            <pre className="skill-body-preview">{skill.body}</pre>
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}

// AutomationArea renders workflow and background execution placeholders.
// AutomationArea 渲染工作流与后台执行的预览入口。
function AutomationArea(props: {
  // Command sessions used to preview background execution.
  // 用于预览后台执行状态。
  commandSessions: CommandSessionSummary[]
  // Runtime modules used to show automation dependency readiness.
  // 用于展示自动化依赖就绪情况的运行时模块。
  modules: RuntimeModuleSnapshot[]
}) {
  const automationCards = [
    { id: "workflow", title: "工作流", status: "规划中", detail: "脚本化任务、插件流程与 WebUI 编排入口" },
    { id: "queue", title: "后台队列", status: "预览", detail: `${props.commandSessions.length} 条命令会话可作为后续队列数据源` },
    { id: "schedule", title: "计划任务", status: "待接入", detail: "后续承接定时、监控与远程触发任务" },
  ]

  return (
    <div className="area-grid two-columns">
      <section className="panel">
        <PanelTitle icon={<GitBranch size={17} />} title="自动" />
        <div className="card-grid">
          {automationCards.map((card) => (
            <article className="data-card" key={card.id}>
              <StatusPill status={card.status === "预览" ? "ready" : "warning"} label={card.status} />
              <strong>{card.title}</strong>
              <span>{card.detail}</span>
            </article>
          ))}
        </div>
      </section>
      <RuntimeModulesPanel modules={props.modules.filter((module) => module.id === "core" || module.id === "mcp" || module.id === "luaskills")} />
    </div>
  )
}

// ProjectsArea renders project and workspace status.
// ProjectsArea 渲染项目与工作区状态。
function ProjectsArea(props: {
  // Project summaries shown in settings.
  // 设置中展示的项目摘要。
  projects: ProjectSummary[]
  // Runtime module summaries shown beside projects.
  // 项目旁展示的运行时模块摘要。
  modules: RuntimeModuleSnapshot[]
}) {
  return (
    <div className="area-grid two-columns">
      <section className="panel">
        <PanelTitle icon={<Boxes size={17} />} title="项目管理" />
        <div className="card-list">
          {props.projects.map((project) => (
            <article className="data-card" key={project.id}>
              <strong>{project.name}</strong>
              <span>{project.path}</span>
            </article>
          ))}
        </div>
      </section>
      <RuntimeModulesPanel modules={props.modules} />
    </div>
  )
}

// ProvidersArea renders model provider status.
// ProvidersArea 渲染模型 Provider 状态。
function ProvidersArea(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Provider currently opened in the edit dialog.
  // 当前在编辑弹窗中打开的模型供应商。
  editingProvider: ProviderSummary | null
  // Updates the provider edit dialog target.
  // 更新模型供应商编辑弹窗目标。
  onEditingProviderChange: (provider: ProviderSummary | null) => void
  // Updates the add-provider picker open state.
  // 更新添加模型供应商选择弹窗的打开状态。
  onProviderPickerOpenChange: (open: boolean) => void
  // Whether the add-provider picker is open.
  // 添加模型供应商选择弹窗是否打开。
  pickerOpen: boolean
  // Provider summaries shown in settings.
  // 设置中展示的 Provider 摘要。
  providers: ProviderSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const configuredProviders = props.providers.filter((provider) => provider.configured)
  return (
    <div className="settings-direct-content">
      <div className="card-grid">
        {configuredProviders.length === 0 ? (
          <article className="settings-card">
            <strong>{text("暂无模型供应商", "No model providers")}</strong>
            <p>{text("点击添加后从目录中筛选供应商，再填写密钥等信息。", "Click Add, filter the catalog, then fill credentials and settings.")}</p>
          </article>
        ) : configuredProviders.map((provider) => (
          <article className="data-card" key={provider.id}>
            <StatusPill status={provider.configured ? "ready" : "pending"} label={provider.status} />
            <strong>{provider.name}</strong>
            <span>{provider.id}</span>
            <span>{text("已启用模型", "Enabled models")}：{provider.enabledModels}</span>
            <div className="settings-card-actions">
              <button className="skill-action-button secondary" onClick={() => props.onEditingProviderChange(provider)} type="button">{text("编辑", "Edit")}</button>
              <button className="skill-action-button secondary danger" onClick={() => void props.actions.onRemoveProvider(provider.id)} type="button">{text("删除", "Delete")}</button>
            </div>
          </article>
        ))}
      </div>
      <ProviderPickerDialog
        onOpenChange={props.onProviderPickerOpenChange}
        onSelect={(provider) => {
          props.onProviderPickerOpenChange(false)
          props.onEditingProviderChange(provider)
        }}
        open={props.pickerOpen}
        providers={props.providers.filter((provider) => !provider.configured)}
      />
      <ProviderEditDialog
        actions={props.actions}
        onOpenChange={(open) => {
          if (!open) {
            props.onEditingProviderChange(null)
          }
        }}
        open={props.editingProvider !== null}
        provider={props.editingProvider}
      />
    </div>
  )
}

// ModelCapabilityTarget identifies one provider/model pair opened for capability editing.
// ModelCapabilityTarget 标识一个正在编辑能力的供应商与模型组合。
interface ModelCapabilityTarget {
  // Provider that owns the model.
  // 拥有该模型的供应商。
  provider: ProviderSummary
  // Model whose capability metadata is being edited.
  // 正在编辑能力元数据的模型。
  model: ProviderModelSummary
}

// ModelListSettingsArea renders enabled models with an iced-style provider/model split.
// ModelListSettingsArea 使用 iced 风格供应商/模型分栏渲染已启用模型。
function ModelListSettingsArea(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Whether the enabled-model configuration dialog is open.
  // 启用模型配置弹窗是否打开。
  dialogOpen: boolean
  // Updates the enabled-model configuration dialog open state.
  // 更新启用模型配置弹窗打开状态。
  onDialogOpenChange: (open: boolean) => void
  // Provider summaries containing enabled model rows.
  // 包含已启用模型行的 Provider 摘要。
  providers: ProviderSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const configuredProviders = React.useMemo(() => props.providers.filter((provider) => provider.configured), [props.providers])
  const [selectedProviderId, setSelectedProviderId] = React.useState<string | null>(null)
  const [capabilityTarget, setCapabilityTarget] = React.useState<ModelCapabilityTarget | null>(null)
  React.useEffect(() => {
    if (configuredProviders.length === 0) {
      setSelectedProviderId(null)
      return
    }
    if (!selectedProviderId || !configuredProviders.some((provider) => provider.id === selectedProviderId)) {
      setSelectedProviderId(configuredProviders[0]?.id ?? null)
    }
  }, [configuredProviders, selectedProviderId])
  const enabledModelCount = props.providers.reduce((count, provider) => count + (provider.models?.length ?? 0), 0)
  const selectedProvider = configuredProviders.find((provider) => provider.id === selectedProviderId) ?? configuredProviders[0] ?? null
  return (
    <div className="settings-direct-content model-list-surface">
      <div className="settings-split-layout">
        <aside className="settings-provider-nav" aria-label={text("供应商", "Providers")}>
          <div className="settings-split-column-title">{text("供应商", "Providers")}</div>
          {configuredProviders.map((provider) => (
            <button
              className={`settings-provider-nav-row ${selectedProvider?.id === provider.id ? "active" : ""}`}
              key={provider.id}
              onClick={() => setSelectedProviderId(provider.id)}
              type="button"
            >
              <FileText size={14} />
              <span>{provider.name}</span>
              <small>[{provider.models?.length ?? 0}]</small>
            </button>
          ))}
        </aside>
        <section className="settings-model-list-pane" aria-label={text("已启用模型", "Enabled models")}>
          <div className="settings-model-pane-header">
            <div>
              <div className="settings-split-column-title">{text("已启用模型", "Enabled models")}</div>
              <small>{selectedProvider ? selectedProvider.name : text("未选择供应商", "No provider selected")} · {text("总启用", "Total enabled")} {enabledModelCount}</small>
            </div>
            {selectedProvider ? <span className="settings-muted-line">{selectedProvider.name}</span> : null}
          </div>
          <div className="settings-model-row-list">
            {!selectedProvider ? (
              <p className="settings-muted-line">{text("当前没有已配置供应商。", "No configured provider is available.")}</p>
            ) : (selectedProvider.models ?? []).length === 0 ? (
              <p className="settings-muted-line">{text("该供应商当前没有启用模型。", "This provider has no enabled model.")}</p>
            ) : selectedProvider.models?.map((model) => (
              <article className="settings-model-row" key={`${selectedProvider.id}-${model.id}`}>
                <div>
                  <strong>{model.name}</strong>
                  <span>{formatProviderModelMeta(model, text)}</span>
                </div>
                <button className="skill-action-button secondary" onClick={() => setCapabilityTarget({ provider: selectedProvider, model })} type="button">{text("编辑能力", "Edit capabilities")}</button>
              </article>
            ))}
          </div>
        </section>
      </div>
      <ProviderModelsDialog
        actions={props.actions}
        initialProviderId={selectedProvider?.id ?? null}
        onEditCapability={setCapabilityTarget}
        onOpenChange={props.onDialogOpenChange}
        open={props.dialogOpen}
        providers={configuredProviders}
      />
      <ModelCapabilityDialog
        actions={props.actions}
        onOpenChange={(open) => {
          if (!open) {
            setCapabilityTarget(null)
          }
        }}
        open={capabilityTarget !== null}
        target={capabilityTarget}
      />
    </div>
  )
}

// ProviderEditDialog renders the provider add/edit form.
// ProviderEditDialog 渲染 Provider 新增/编辑表单。
function ProviderEditDialog(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Provider being edited.
  // 当前编辑的 Provider。
  provider: ProviderSummary | null
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [fields, setFields] = React.useState<Record<string, string>>({})
  React.useEffect(() => {
    const nextFields: Record<string, string> = {}
    props.provider?.fields?.forEach((field) => {
      nextFields[field.key] = field.value
    })
    setFields(nextFields)
  }, [props.provider])
  const save = React.useCallback(async () => {
    if (!props.provider) {
      return
    }
    await props.actions.onSaveProvider({ providerId: props.provider.id, fields })
    props.onOpenChange(false)
  }, [fields, props])
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog">
        <DialogHeader>
          <DialogTitle>{props.provider?.configured ? text("编辑 Provider", "Edit provider") : text("添加 Provider", "Add provider")}</DialogTitle>
          <DialogDescription>{props.provider?.name}</DialogDescription>
        </DialogHeader>
        <div className="settings-form-grid">
          {(props.provider?.fields ?? []).map((field) => (
            <label className="settings-form-field" key={field.key}>
              <span>{field.label}</span>
              <input
                className="settings-input"
                onChange={(event) => setFields((current) => ({ ...current, [field.key]: event.target.value }))}
                placeholder={field.placeholder}
                type={field.secret ? "password" : "text"}
                value={fields[field.key] ?? ""}
              />
            </label>
          ))}
        </div>
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">{text("取消", "Cancel")}</button>
          <button className="skill-action-button" onClick={() => void save()} type="button">{text("保存", "Save")}</button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ProviderPickerDialog renders the add-provider catalog picker.
// ProviderPickerDialog 渲染添加 Provider 的目录选择器。
function ProviderPickerDialog(props: {
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Provider catalog choices.
  // Provider 目录选项。
  providers: ProviderSummary[]
  // Handles one selected provider.
  // 处理单个已选择 Provider。
  onSelect: (provider: ProviderSummary) => void
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [filter, setFilter] = React.useState("")
  React.useEffect(() => {
    if (!props.open) {
      setFilter("")
    }
  }, [props.open])
  const query = filter.trim().toLowerCase()
  const visibleProviders = props.providers.filter((provider) => {
    if (!query) {
      return true
    }
    return provider.id.toLowerCase().includes(query) || provider.name.toLowerCase().includes(query)
  })
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog wide">
        <DialogHeader>
          <DialogTitle>{text("添加模型供应商", "Add model provider")}</DialogTitle>
          <DialogDescription>{text("输入关键字过滤供应商，选择后填写密钥等配置。", "Filter providers, then select one to fill credentials and settings.")}</DialogDescription>
        </DialogHeader>
        <input
          className="settings-input"
          onChange={(event) => setFilter(event.target.value)}
          placeholder={text("搜索供应商", "Search providers")}
          value={filter}
        />
        <div className="settings-modal-list">
          {visibleProviders.length === 0 ? (
            <p className="settings-muted-line">{text("当前没有可新增的供应商，或者过滤后没有匹配结果。", "No provider can be added, or no result matches the filter.")}</p>
          ) : visibleProviders.map((provider) => (
            <button className="settings-picker-row" key={provider.id} onClick={() => props.onSelect(provider)} type="button">
              <strong>{provider.name}</strong>
              <span>{provider.id} · {(provider.catalogModels ?? []).length} models</span>
            </button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}

// ProviderModelsDialog renders the iced-style enabled-model picker across providers.
// ProviderModelsDialog 渲染跨供应商的 iced 风格启用模型选择器。
function ProviderModelsDialog(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Initial provider id selected when the dialog opens.
  // 弹窗打开时初始选中的供应商标识。
  initialProviderId: string | null
  // Opens the capability editor for one provider/model pair.
  // 打开单个供应商/模型组合的能力编辑器。
  onEditCapability: (target: ModelCapabilityTarget) => void
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Configured providers available in the left column.
  // 左侧可用的已配置供应商。
  providers: ProviderSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [draftEnabledIds, setDraftEnabledIds] = React.useState<Record<string, Set<string>>>({})
  const [filter, setFilter] = React.useState("")
  const [selectedProviderId, setSelectedProviderId] = React.useState<string | null>(null)
  React.useEffect(() => {
    if (!props.open) {
      return
    }
    const nextDrafts = Object.fromEntries(props.providers.map((provider) => [
      provider.id,
      new Set((provider.catalogModels ?? []).filter((model) => model.enabled).map((model) => model.id)),
    ]))
    setDraftEnabledIds(nextDrafts)
    setSelectedProviderId(props.initialProviderId && props.providers.some((provider) => provider.id === props.initialProviderId)
      ? props.initialProviderId
      : props.providers[0]?.id ?? null)
    setFilter("")
  }, [props.initialProviderId, props.open, props.providers])
  const selectedProvider = props.providers.find((provider) => provider.id === selectedProviderId) ?? props.providers[0] ?? null
  const selectedEnabledIds = selectedProvider ? draftEnabledIds[selectedProvider.id] ?? new Set<string>() : new Set<string>()
  const visibleModels = (selectedProvider?.catalogModels ?? []).filter((model) => {
    const query = filter.trim().toLowerCase()
    if (!query) {
      return true
    }
    return model.id.toLowerCase().includes(query) || model.name.toLowerCase().includes(query) || (model.family ?? "").toLowerCase().includes(query)
  })
  const toggle = React.useCallback((providerId: string, modelId: string, enabled: boolean) => {
    setDraftEnabledIds((current) => {
      const next = new Set(current[providerId] ?? [])
      if (enabled) {
        next.add(modelId)
      } else {
        next.delete(modelId)
      }
      return { ...current, [providerId]: next }
    })
  }, [])
  const save = React.useCallback(async () => {
    for (const provider of props.providers) {
      const nextIds = Array.from(draftEnabledIds[provider.id] ?? [])
      if (sameStringSet(nextIds, (provider.catalogModels ?? []).filter((model) => model.enabled).map((model) => model.id))) {
        continue
      }
      await props.actions.onSaveProviderEnabledModels({
        providerId: provider.id,
        enabledModelIds: nextIds,
      })
    }
    props.onOpenChange(false)
  }, [draftEnabledIds, props])
  const selectCurrentFiltered = React.useCallback((enabled: boolean) => {
    if (!selectedProvider) {
      return
    }
    setDraftEnabledIds((current) => {
      const next = new Set(current[selectedProvider.id] ?? [])
      visibleModels.forEach((model) => {
        if (enabled) {
          next.add(model.id)
        } else {
          next.delete(model.id)
        }
      })
      return { ...current, [selectedProvider.id]: next }
    })
  }, [selectedProvider, visibleModels])
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog model-list-dialog">
        <DialogHeader>
          <DialogTitle>{text("配置启用模型", "Configure enabled models")}</DialogTitle>
          <DialogDescription>{text("左侧切换供应商，右侧编辑完整模型目录。全选、取消等操作都会写入草稿，只有点击保存才会真正落盘。", "Switch providers on the left and edit the full model catalog on the right. Changes are drafted until Save.")}</DialogDescription>
        </DialogHeader>
        <div className="settings-split-layout modal">
          <aside className="settings-provider-nav" aria-label={text("供应商", "Providers")}>
            <div className="settings-split-column-title">{text("供应商", "Providers")}</div>
            {props.providers.map((provider) => (
              <button
                className={`settings-provider-nav-row ${selectedProvider?.id === provider.id ? "active" : ""}`}
                key={provider.id}
                onClick={() => setSelectedProviderId(provider.id)}
                type="button"
              >
                <FileText size={14} />
                <span>{provider.name}</span>
                <small>[{draftEnabledIds[provider.id]?.size ?? 0}]</small>
              </button>
            ))}
          </aside>
          <section className="settings-model-list-pane" aria-label={text("模型目录", "Model catalog")}>
            <div className="settings-model-pane-header">
              <div>
                <div className="settings-split-column-title">{text("模型目录", "Model catalog")}</div>
                <small>{selectedProvider?.name ?? text("未选择供应商", "No provider selected")}</small>
              </div>
              <input
                className="settings-input settings-model-filter"
                onChange={(event) => setFilter(event.target.value)}
                placeholder={text("输入模型名称或 ID 过滤", "Filter by model name or id")}
                value={filter}
              />
            </div>
            <div className="settings-card-actions">
              <button className="skill-action-button secondary" onClick={() => selectCurrentFiltered(true)} type="button">{text("全选当前结果", "Select filtered")}</button>
              <button className="skill-action-button secondary" onClick={() => selectCurrentFiltered(false)} type="button">{text("取消当前结果", "Clear filtered")}</button>
            </div>
            <div className="settings-model-row-list modal">
              {visibleModels.map((model) => (
                <label className="settings-model-row selectable" key={model.id}>
                  <input
                    checked={selectedEnabledIds.has(model.id)}
                    onChange={(event) => selectedProvider ? toggle(selectedProvider.id, model.id, event.target.checked) : undefined}
                    type="checkbox"
                  />
                  <span>
                    <strong>{model.name}</strong>
                    <small>{formatProviderModelMeta(model, text)}</small>
                  </span>
                  {selectedProvider ? (
                    <button
                      className="skill-action-button secondary"
                      onClick={(event) => {
                        event.preventDefault()
                        props.onEditCapability({ provider: selectedProvider, model })
                      }}
                      type="button"
                    >
                      {text("编辑能力", "Edit capabilities")}
                    </button>
                  ) : null}
                </label>
              ))}
            </div>
          </section>
        </div>
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">{text("取消", "Cancel")}</button>
          <button className="skill-action-button" onClick={() => void save()} type="button">{text("保存", "Save")}</button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ModelCapabilityDialog renders the model capability override editor.
// ModelCapabilityDialog 渲染模型能力覆盖编辑器。
function ModelCapabilityDialog(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Provider and model being edited.
  // 正在编辑的供应商与模型。
  target: ModelCapabilityTarget | null
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [reasoning, setReasoning] = React.useState(false)
  const [attachment, setAttachment] = React.useState(false)
  const [inputModalities, setInputModalities] = React.useState<Set<string>>(() => new Set(["text"]))
  const [outputModalities, setOutputModalities] = React.useState<Set<string>>(() => new Set(["text"]))
  const [contextLimit, setContextLimit] = React.useState("")
  const [inputLimit, setInputLimit] = React.useState("")
  const [outputLimit, setOutputLimit] = React.useState("")
  const [reasoningProfileJson, setReasoningProfileJson] = React.useState("")
  React.useEffect(() => {
    const model = props.target?.model
    setReasoning(model?.reasoning ?? false)
    setAttachment(model?.attachment ?? false)
    setInputModalities(new Set((model?.inputModalities?.length ? model.inputModalities : ["text"]).map((item) => item.toLowerCase())))
    setOutputModalities(new Set((model?.outputModalities?.length ? model.outputModalities : ["text"]).map((item) => item.toLowerCase())))
    setContextLimit(model?.contextLimit ? String(model.contextLimit) : "")
    setInputLimit(model?.inputLimit ? String(model.inputLimit) : "")
    setOutputLimit(model?.outputLimit ? String(model.outputLimit) : "")
    setReasoningProfileJson(model?.reasoningProfileJson ?? "")
  }, [props.target])
  const toggleModality = React.useCallback((kind: "input" | "output", value: string, checked: boolean) => {
    const setter = kind === "input" ? setInputModalities : setOutputModalities
    setter((current) => {
      const next = new Set(current)
      if (checked) {
        next.add(value)
      } else {
        next.delete(value)
      }
      return next
    })
  }, [])
  const save = React.useCallback(async () => {
    if (!props.target) {
      return
    }
    await props.actions.onSaveProviderModelOverride({
      providerId: props.target.provider.id,
      modelId: props.target.model.id,
      reasoning,
      attachment,
      contextLimit: nullablePositiveInteger(contextLimit),
      inputLimit: nullablePositiveInteger(inputLimit),
      outputLimit: nullablePositiveInteger(outputLimit),
      inputModalities: Array.from(inputModalities),
      outputModalities: Array.from(outputModalities),
      reasoningProfileJson: reasoningProfileJson.trim() || null,
    })
    props.onOpenChange(false)
  }, [attachment, contextLimit, inputLimit, inputModalities, outputLimit, outputModalities, props, reasoning, reasoningProfileJson])
  const model = props.target?.model
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog wide">
        <DialogHeader>
          <DialogTitle>{text("编辑模型能力", "Edit model capabilities")}</DialogTitle>
          <DialogDescription>{props.target ? `${props.target.provider.name} · ${model?.name}` : ""}</DialogDescription>
        </DialogHeader>
        <div className="model-capability-form">
          <div className="settings-card-actions">
            <label className="settings-check-row inline"><input checked={reasoning} onChange={(event) => setReasoning(event.target.checked)} type="checkbox" />{text("声明支持思考 / 推理", "Supports reasoning")}</label>
            <label className="settings-check-row inline"><input checked={attachment} onChange={(event) => setAttachment(event.target.checked)} type="checkbox" />{text("声明附件标记", "Supports attachments")}</label>
          </div>
          <fieldset className="settings-modality-box">
            <legend>{text("输入能力", "Input capability")}</legend>
            {["text", "image", "audio", "video", "pdf"].map((item) => (
              <label className="settings-check-row inline" key={item}>
                <input checked={inputModalities.has(item)} onChange={(event) => toggleModality("input", item, event.target.checked)} type="checkbox" />
                {localizedModalityLabel("input", item, text)}
              </label>
            ))}
          </fieldset>
          <fieldset className="settings-modality-box">
            <legend>{text("输出能力", "Output capability")}</legend>
            {["text", "image", "audio", "video"].map((item) => (
              <label className="settings-check-row inline" key={item}>
                <input checked={outputModalities.has(item)} onChange={(event) => toggleModality("output", item, event.target.checked)} type="checkbox" />
                {localizedModalityLabel("output", item, text)}
              </label>
            ))}
          </fieldset>
          <div className="settings-form-grid three">
            <label className="settings-form-field"><span>{text("上下文长度", "Context limit")}</span><input className="settings-input" onChange={(event) => setContextLimit(event.target.value)} value={contextLimit} /></label>
            <label className="settings-form-field"><span>{text("输入长度", "Input limit")}</span><input className="settings-input" onChange={(event) => setInputLimit(event.target.value)} value={inputLimit} /></label>
            <label className="settings-form-field"><span>{text("输出长度", "Output limit")}</span><input className="settings-input" onChange={(event) => setOutputLimit(event.target.value)} value={outputLimit} /></label>
          </div>
          <label className="settings-form-field">
            <span>{text("思考模式附加配置（可选 JSON）", "Reasoning profile override (optional JSON)")}</span>
            <textarea className="settings-input textarea large" onChange={(event) => setReasoningProfileJson(event.target.value)} value={reasoningProfileJson} />
          </label>
          <p className="settings-muted-line">{text("修改只会写入该模型的用户覆盖配置，保存后会刷新模型能力结果。", "Changes are saved as user overrides for this model only and refresh the resolved capability result.")}</p>
        </div>
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">{text("取消", "Cancel")}</button>
          <button className="skill-action-button" onClick={() => void save()} type="button">{text("保存", "Save")}</button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// sameStringSet compares two string lists as unordered sets.
// sameStringSet 将两个字符串列表作为无序集合比较。
function sameStringSet(left: string[], right: string[]): boolean {
  if (left.length !== right.length) {
    return false
  }
  const rightSet = new Set(right)
  return left.every((item) => rightSet.has(item))
}

// nullablePositiveInteger parses a positive integer field while treating blanks as null.
// nullablePositiveInteger 解析正整数输入，并将空值视为 null。
function nullablePositiveInteger(value: string): number | null {
  const trimmed = value.trim()
  if (!trimmed) {
    return null
  }
  const parsed = Number.parseInt(trimmed, 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null
}

// formatProviderModelMeta renders the compact capability line for a model row.
// formatProviderModelMeta 渲染模型行的紧凑能力信息。
function formatProviderModelMeta(model: ProviderModelSummary, text: (zh: string, en: string) => string): string {
  const modalities = model.inputModalities?.length ? model.inputModalities.join("+") : "text"
  const limits = [
    model.contextLimit ? `${text("上下文", "context")} ${model.contextLimit}` : null,
    model.inputLimit ? `${text("输入", "input")} ${model.inputLimit}` : null,
    model.outputLimit ? `${text("输出", "output")} ${model.outputLimit}` : null,
  ].filter(Boolean)
  return [model.id, model.family, modalities, ...limits].filter(Boolean).join(" · ")
}

// localizedModalityLabel returns the compact label for one model modality direction.
// localizedModalityLabel 返回单个模型模态方向的紧凑标签。
function localizedModalityLabel(direction: "input" | "output", value: string, text: (zh: string, en: string) => string): string {
  const suffix = direction === "input" ? text("输入", "input") : text("输出", "output")
  switch (value) {
    case "text":
      return text(`文本${suffix}`, `Text ${suffix}`)
    case "image":
      return text(`图片${suffix}`, `Image ${suffix}`)
    case "audio":
      return text(`音频${suffix}`, `Audio ${suffix}`)
    case "video":
      return text(`视频${suffix}`, `Video ${suffix}`)
    case "pdf":
      return text(`PDF / 文件${suffix}`, `PDF / file ${suffix}`)
    default:
      return value
  }
}

// AiCapabilityVendorsArea renders shared capability vendors and route targets.
// AiCapabilityVendorsArea 渲染共享能力供应商与路由目标。
function AiCapabilityVendorsArea(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Semantic AI capability summaries.
  // 语义 AI 能力摘要。
  capabilities: AiCapabilitySummary[]
  // Shared AI capability vendor summaries.
  // 共享 AI 能力供应商摘要。
  vendors: AiCapabilityVendorSummary[]
  // Supported AI capability vendor catalog choices.
  // 受支持 AI 能力供应商目录选项。
  catalog: AiCapabilityVendorCatalogSummary[]
  // Provider summaries used as model-provider context.
  // 作为模型供应商上下文的 Provider 摘要。
  providers: ProviderSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [editingVendor, setEditingVendor] = React.useState<AiCapabilityVendorSummary | null>(null)
  const [pickerOpen, setPickerOpen] = React.useState(false)
  const targets = props.capabilities.flatMap((capability) => capability.targets.map((target) => ({ capability, target })))
  const configuredVendorIds = new Set(props.vendors.map((vendor) => vendor.vendorId.toLowerCase()))
  const availableVendors = props.catalog.filter((vendor) => !configuredVendorIds.has(vendor.vendorId.toLowerCase()))
  return (
    <div className="area-grid two-columns">
      <section className="panel settings-detail-panel">
        <div className="settings-panel-title-row">
          <PanelTitle icon={<FileText size={17} />} title={text("能力供应商", "Capability vendors")} />
          <button className="skill-action-button" onClick={() => setPickerOpen(true)} type="button">{text("添加供应商", "Add vendor")}</button>
        </div>
        <div className="settings-grid compact">
          {props.vendors.length === 0 ? (
            <article className="settings-card"><strong>{text("暂无能力供应商", "No capability vendors")}</strong><p>{text("当前没有已配置的 AI 能力路由目标。", "No configured AI capability route targets.")}</p></article>
          ) : props.vendors.map((vendor) => (
            <article className="settings-card" key={vendor.vendorId}>
              <StatusPill status={vendor.enabled ? "ready" : "pending"} label={vendor.enabled ? text("启用", "Enabled") : text("停用", "Disabled")} />
              <strong>{vendor.name}</strong>
              <p>{vendor.vendorId} · {text(`${vendor.targetCount} 个能力目标`, `${vendor.targetCount} targets`)}</p>
              <div className="settings-card-actions">
                <button className="skill-action-button secondary" onClick={() => setEditingVendor(vendor)} type="button">{text("编辑", "Edit")}</button>
                <button className="skill-action-button secondary danger" onClick={() => void props.actions.onRemoveAiCapabilityVendor(vendor.vendorId)} type="button">{text("删除", "Delete")}</button>
              </div>
            </article>
          ))}
        </div>
        <AiCapabilityVendorPickerDialog
          catalog={availableVendors}
          onOpenChange={setPickerOpen}
          onSelect={(vendor) => {
            setPickerOpen(false)
            setEditingVendor({
              id: vendor.vendorId,
              vendorId: vendor.vendorId,
              name: vendor.name,
              enabled: true,
              apiKey: "",
              baseUrl: "",
              region: "",
              connectionMode: "local",
              targetCount: 0,
              targetChoices: [],
            })
          }}
          open={pickerOpen}
        />
        <AiCapabilityVendorDialog
          actions={props.actions}
          onOpenChange={(open) => {
            if (!open) {
              setEditingVendor(null)
            }
          }}
          open={editingVendor !== null}
          vendor={editingVendor}
        />
      </section>
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<SlidersHorizontal size={17} />} title={text("路由目标", "Route targets")} />
        <div className="settings-target-list">
          {targets.length === 0 ? (
            <p className="settings-muted-line">{text("没有可展示的能力路由目标。", "No capability route targets to display.")}</p>
          ) : targets.map(({ capability, target }) => (
            <article className="settings-target-row" key={`${capability.kind}-${target.targetSelector}`}>
              <div>
                <strong>{target.targetName || target.targetAlias}</strong>
                <span>{capability.label} · {target.vendorName} · {target.modelId}</span>
              </div>
              <button
                className={`skill-action-button ${target.enabled ? "" : "secondary"}`}
                onClick={() => void props.actions.onSetAiCapabilityTargetEnabled({
                  capabilityKind: capability.kind,
                  vendorId: target.vendorId,
                  targetAlias: target.targetAlias,
                  enabled: !target.enabled,
                })}
                type="button"
              >
                {target.enabled ? text("停用", "Disable") : text("启用", "Enable")}
              </button>
            </article>
          ))}
        </div>
      </section>
      <RuntimeModulesPanel modules={props.providers.map((provider) => ({
        id: provider.id,
        label: provider.name,
        status: provider.active ? "ready" as const : "pending" as const,
        message: provider.enabledModels,
      }))} />
    </div>
  )
}

// AiCapabilityVendorDialog renders the shared AI capability vendor editor.
// AiCapabilityVendorDialog 渲染共享 AI 能力供应商编辑器。
function AiCapabilityVendorDialog(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Vendor being edited.
  // 当前编辑的供应商。
  vendor: AiCapabilityVendorSummary | null
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [vendorId, setVendorId] = React.useState("")
  const [enabled, setEnabled] = React.useState(true)
  const [apiKey, setApiKey] = React.useState("")
  const [baseUrl, setBaseUrl] = React.useState("")
  const [region, setRegion] = React.useState("")
  const [connectionMode, setConnectionMode] = React.useState("local")
  React.useEffect(() => {
    setVendorId(props.vendor?.vendorId ?? "")
    setEnabled(props.vendor?.enabled ?? true)
    setApiKey(props.vendor?.apiKey ?? "")
    setBaseUrl(props.vendor?.baseUrl ?? "")
    setRegion(props.vendor?.region ?? "")
    setConnectionMode(props.vendor?.connectionMode ?? "local")
  }, [props.vendor])
  const save = React.useCallback(async () => {
    await props.actions.onSaveAiCapabilityVendor({
      vendorId,
      enabled,
      apiKey,
      baseUrl,
      region,
      connectionMode,
    })
    props.onOpenChange(false)
  }, [apiKey, baseUrl, connectionMode, enabled, props, region, vendorId])
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog">
        <DialogHeader>
          <DialogTitle>{props.vendor ? text("编辑能力供应商", "Edit capability vendor") : text("添加能力供应商", "Add capability vendor")}</DialogTitle>
          <DialogDescription>{text("供应商 ID 例如 minimax、tavily、ollama。", "Vendor id examples: minimax, tavily, ollama.")}</DialogDescription>
        </DialogHeader>
        <div className="settings-form-grid">
          <label className="settings-form-field">
            <span>{text("供应商 ID", "Vendor ID")}</span>
            <input className="settings-input" disabled onChange={(event) => setVendorId(event.target.value)} value={vendorId} />
          </label>
          <label className="settings-check-row inline">
            <input checked={enabled} onChange={(event) => setEnabled(event.target.checked)} type="checkbox" />
            <span>{text("启用供应商", "Enable vendor")}</span>
          </label>
          <label className="settings-form-field">
            <span>API Key</span>
            <input className="settings-input" onChange={(event) => setApiKey(event.target.value)} type="password" value={apiKey} />
          </label>
          <label className="settings-form-field">
            <span>Base URL</span>
            <input className="settings-input" onChange={(event) => setBaseUrl(event.target.value)} value={baseUrl} />
          </label>
          <label className="settings-form-field">
            <span>{text("区域", "Region")}</span>
            <input className="settings-input" onChange={(event) => setRegion(event.target.value)} value={region} />
          </label>
          <label className="settings-form-field">
            <span>{text("连接模式", "Connection mode")}</span>
            <select className="settings-input" onChange={(event) => setConnectionMode(event.target.value)} value={connectionMode}>
              <option value="local">{text("本地", "Local")}</option>
              <option value="cloud">Ollama Cloud</option>
            </select>
          </label>
        </div>
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">{text("取消", "Cancel")}</button>
          <button className="skill-action-button" onClick={() => void save()} type="button">{text("保存", "Save")}</button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// AiCapabilityVendorPickerDialog renders the add-vendor catalog picker.
// AiCapabilityVendorPickerDialog 渲染添加能力供应商的目录选择器。
function AiCapabilityVendorPickerDialog(props: {
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Supported vendor catalog choices.
  // 支持的供应商目录选项。
  catalog: AiCapabilityVendorCatalogSummary[]
  // Handles one selected vendor.
  // 处理单个已选择供应商。
  onSelect: (vendor: AiCapabilityVendorCatalogSummary) => void
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [filter, setFilter] = React.useState("")
  React.useEffect(() => {
    if (!props.open) {
      setFilter("")
    }
  }, [props.open])
  const query = filter.trim().toLowerCase()
  const visibleVendors = props.catalog.filter((vendor) => {
    if (!query) {
      return true
    }
    return vendor.vendorId.toLowerCase().includes(query)
      || vendor.name.toLowerCase().includes(query)
      || vendor.capabilities.some((capability) => capability.toLowerCase().includes(query))
  })
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog wide">
        <DialogHeader>
          <DialogTitle>{text("添加能力供应商", "Add capability vendor")}</DialogTitle>
          <DialogDescription>{text("输入关键字过滤供应商或能力，选择后进入配置弹窗。", "Filter vendors or capabilities, then select one to configure.")}</DialogDescription>
        </DialogHeader>
        <input
          className="settings-input"
          onChange={(event) => setFilter(event.target.value)}
          placeholder={text("搜索供应商或能力", "Search vendors or capabilities")}
          value={filter}
        />
        <div className="settings-modal-list">
          {visibleVendors.length === 0 ? (
            <p className="settings-muted-line">{text("当前没有可新增的供应商，或者过滤后没有匹配结果。", "No vendor can be added, or no result matches the filter.")}</p>
          ) : visibleVendors.map((vendor) => (
            <button className="settings-picker-row" key={vendor.vendorId} onClick={() => props.onSelect(vendor)} type="button">
              <strong>{vendor.name}</strong>
              <span>{vendor.vendorId} · {vendor.capabilities.length ? vendor.capabilities.join("、") : text("暂无能力", "No capabilities")}</span>
            </button>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  )
}

// AiCapabilitySettingsArea renders one semantic AI capability page.
// AiCapabilitySettingsArea 渲染单个语义 AI 能力页面。
function AiCapabilitySettingsArea(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Active settings sub area.
  // 当前设置子页面。
  activeArea: SettingsSubAreaId
  // Semantic AI capability summaries.
  // 语义 AI 能力摘要。
  capabilities: AiCapabilitySummary[]
  // Whether the target configuration dialog is open.
  // 目标配置弹窗是否打开。
  configDialogOpen: boolean
  // Updates the target configuration dialog open state.
  // 更新目标配置弹窗打开状态。
  onConfigDialogOpenChange: (open: boolean) => void
  // Configured AI capability vendors with target choices.
  // 带目标选项的已配置 AI 能力供应商。
  vendors: AiCapabilityVendorSummary[]
}) {
  const { locale, t } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  // displayedTargets keeps drag sorting optimistic while the service snapshot catches up.
  // displayedTargets 在服务快照追上前保持拖拽排序的乐观展示顺序。
  const [displayedTargets, setDisplayedTargets] = React.useState<AiCapabilityTargetSummary[]>([])
  // pendingRemoveSelector stores the route waiting for a second remove click.
  // pendingRemoveSelector 保存等待第二次点击确认移除的路由。
  const [pendingRemoveSelector, setPendingRemoveSelector] = React.useState<string | null>(null)
  const capabilityKind = settingsSubAreaCapabilityKind(props.activeArea)
  const capability = props.capabilities.find((entry) => entry.kind === capabilityKind)
  const header = settingsSubAreas(t).find((entry) => entry.id === props.activeArea)
  const targetDragSensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }))
  const snapshotTargets = capability?.targets ?? []
  const snapshotTargetSignature = React.useMemo(() => aiCapabilityTargetSummarySignature(snapshotTargets), [snapshotTargets])
  React.useEffect(() => {
    setDisplayedTargets(snapshotTargets)
    setPendingRemoveSelector(null)
  }, [capability?.kind, snapshotTargetSignature])
  const saveTargets = React.useCallback(async (targets: AiCapabilityTargetSaveInput[]) => {
    if (!capability) {
      return
    }
    await props.actions.onSaveAiCapabilityTargets({
      capabilityKind: capability.kind,
      targets,
    })
  }, [capability, props.actions])
  const targetInputs = React.useMemo(() => (
    displayedTargets.map((target) => aiCapabilityTargetInputFromSummary(target))
  ), [displayedTargets])
  const targetSortableIds = React.useMemo(() => (
    targetInputs.map((target) => aiCapabilityTargetInputSelector(target))
  ), [targetInputs])
  const handleTargetDragEnd = React.useCallback((event: DragEndEvent) => {
    const activeId = String(event.active.id)
    const overId = event.over ? String(event.over.id) : null
    if (!overId || activeId === overId) {
      return
    }
    const oldIndex = targetSortableIds.indexOf(activeId)
    const newIndex = targetSortableIds.indexOf(overId)
    if (oldIndex < 0 || newIndex < 0) {
      return
    }
    const nextDisplayedTargets = arrayMove(displayedTargets, oldIndex, newIndex)
    const nextTargetInputs = arrayMove(targetInputs, oldIndex, newIndex)
    setDisplayedTargets(nextDisplayedTargets)
    void saveTargets(nextTargetInputs).catch(() => setDisplayedTargets(snapshotTargets))
  }, [displayedTargets, saveTargets, snapshotTargets, targetInputs, targetSortableIds])
  if (!capability) {
    return (
      <div className="settings-direct-content">
        <PanelTitle icon={header?.icon ?? <SlidersHorizontal size={17} />} title={header?.label ?? "AI 能力"} />
        <article className="settings-card">
          <StatusPill status="warning" label="reserved" />
          <strong>{header?.label ?? props.activeArea}</strong>
          <p>{header?.description ?? text("该能力入口已预留，当前 snapshot 没有可用目标。", "This capability entry is reserved and has no available target in the current snapshot.")}</p>
        </article>
      </div>
    )
  }
  return (
    <div className="settings-direct-content settings-capability-page">
      {displayedTargets.length === 0 ? (
        <div className="composer-capability-dialog-empty settings-capability-empty">
          {text("该能力还没有配置目标。", "This capability has no configured targets.")}
        </div>
      ) : (
        <DndContext sensors={targetDragSensors} collisionDetection={closestCenter} onDragEnd={handleTargetDragEnd}>
          <SortableContext items={targetSortableIds} strategy={verticalListSortingStrategy}>
            <div className="settings-capability-route-list">
              {displayedTargets.map((target, index) => (
                <SortableAiCapabilityTargetRow
                  confirmRemove={pendingRemoveSelector === target.targetSelector}
                  index={index}
                  key={target.targetSelector}
                  onCancelRemove={() => setPendingRemoveSelector(null)}
                  onRemove={() => {
                    const nextTargets = targetInputs.filter((item) => !(item.vendorId === target.vendorId && item.targetAlias === target.targetAlias))
                    setPendingRemoveSelector(null)
                    setDisplayedTargets((current) => current.filter((item) => item.targetSelector !== target.targetSelector))
                    void saveTargets(nextTargets).catch(() => setDisplayedTargets(snapshotTargets))
                  }}
                  onRequestRemove={() => setPendingRemoveSelector(target.targetSelector)}
                  onToggle={(enabled) => {
                    setPendingRemoveSelector(null)
                    const nextTargets = targetInputs.map((item) => (
                      item.vendorId === target.vendorId && item.targetAlias === target.targetAlias
                        ? { ...item, enabled }
                        : item
                    ))
                    setDisplayedTargets((current) => current.map((item) => (
                      item.targetSelector === target.targetSelector ? { ...item, enabled } : item
                    )))
                    void saveTargets(nextTargets).catch(() => setDisplayedTargets(snapshotTargets))
                  }}
                  target={target}
                  text={text}
                />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      )}
      <AiCapabilityTargetsDialog
        capability={capability}
        onOpenChange={props.onConfigDialogOpenChange}
        onSave={saveTargets}
        open={props.configDialogOpen}
        vendors={props.vendors}
      />
    </div>
  )
}

// SortableAiCapabilityTargetRow renders one settings capability route with dnd-kit sorting.
// SortableAiCapabilityTargetRow 使用 dnd-kit 排序渲染单个设置能力路由。
function SortableAiCapabilityTargetRow(props: {
  // Whether the remove action is waiting for confirmation.
  // 移除操作是否正在等待确认。
  confirmRemove: boolean
  // Zero-based row index shown to the user.
  // 展示给用户的零基行序号。
  index: number
  // Target route summary displayed by the row.
  // 当前行展示的目标路由摘要。
  target: AiCapabilityTargetSummary
  // Localized literal helper.
  // 本地化字面量辅助函数。
  text: (zh: string, en: string) => string
  // Toggles whether this target is enabled.
  // 切换当前目标是否启用。
  onToggle: (enabled: boolean) => void
  // Removes this target from the capability route list.
  // 从能力路由列表移除当前目标。
  onRemove: () => void
  // Enters remove-confirmation state.
  // 进入移除确认状态。
  onRequestRemove: () => void
  // Cancels remove-confirmation state.
  // 取消移除确认状态。
  onCancelRemove: () => void
}) {
  const sortable = useVerticalSortableItem(props.target.targetSelector, false)
  return (
    <article
      className={`settings-capability-route-row ${sortable.isDragging ? "dragging" : ""}`}
      onMouseLeave={() => {
        if (props.confirmRemove) {
          props.onCancelRemove()
        }
      }}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
      {...sortable.attributes}
      {...sortable.listeners}
    >
      <GripVertical className="settings-drag-handle" size={14} />
      <input
        checked={props.target.enabled}
        onChange={(event) => props.onToggle(event.target.checked)}
        onPointerDown={(event) => event.stopPropagation()}
        type="checkbox"
      />
      <span className="settings-route-index">{String(props.index + 1).padStart(2, "0")}</span>
      <span className="settings-route-separator">·</span>
      <div className="settings-route-main">
        <strong>{props.target.vendorName} / {props.target.targetName || props.target.targetAlias}</strong>
        <small>{props.target.modelId} · {props.target.targetSelector}</small>
      </div>
      <button
        className={`skill-action-button secondary danger ${props.confirmRemove ? "confirm" : ""}`}
        onClick={(event) => {
          event.stopPropagation()
          if (props.confirmRemove) {
            props.onRemove()
            return
          }
          props.onRequestRemove()
        }}
        onPointerDown={(event) => event.stopPropagation()}
        type="button"
      >
        {props.confirmRemove ? props.text("确认移除", "Confirm remove") : props.text("移除", "Remove")}
      </button>
    </article>
  )
}

// AiCapabilityTargetsDialog renders the iced-style target configuration modal.
// AiCapabilityTargetsDialog 渲染 iced 风格能力目标配置弹窗。
function AiCapabilityTargetsDialog(props: {
  // Capability being configured.
  // 正在配置的能力。
  capability: AiCapabilitySummary
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Saves the ordered target list.
  // 保存有序目标列表。
  onSave: (targets: AiCapabilityTargetSaveInput[]) => Promise<void>
  // Configured vendors with catalog target choices.
  // 带目录目标选项的已配置供应商。
  vendors: AiCapabilityVendorSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [selectedVendorId, setSelectedVendorId] = React.useState<string | null>(null)
  const [drafts, setDrafts] = React.useState<AiCapabilityTargetDraft[]>([])
  React.useEffect(() => {
    if (!props.open) {
      return
    }
    setDrafts(props.capability.targets.map((target) => ({
      vendorId: target.vendorId,
      vendorName: target.vendorName,
      targetAlias: target.targetAlias,
      targetName: target.targetName || target.targetAlias,
      modelId: target.modelId,
      enabled: target.enabled,
      supportsModelOverride: true,
      modelOptions: [],
    })))
    setSelectedVendorId(props.vendors[0]?.vendorId ?? null)
  }, [props.capability.targets, props.open, props.vendors])
  const selectedVendor = props.vendors.find((vendor) => vendor.vendorId === selectedVendorId) ?? props.vendors[0] ?? null
  const choices = (selectedVendor?.targetChoices ?? []).filter((choice) => choice.capabilityKind === props.capability.kind)
  const selectedCount = drafts.filter((draft) => draft.enabled).length
  const toggleChoice = React.useCallback((vendor: AiCapabilityVendorSummary, choice: AiCapabilityVendorTargetChoiceSummary, checked: boolean) => {
    setDrafts((current) => {
      const existingIndex = current.findIndex((draft) => draft.vendorId === vendor.vendorId && draft.targetAlias === choice.targetAlias)
      if (!checked) {
        return existingIndex >= 0 ? current.filter((_, index) => index !== existingIndex) : current
      }
      const nextDraft: AiCapabilityTargetDraft = {
        vendorId: vendor.vendorId,
        vendorName: vendor.name,
        targetAlias: choice.targetAlias,
        targetName: choice.label,
        modelId: choice.defaultModelId || choice.targetAlias,
        enabled: true,
        supportsModelOverride: choice.supportsModelOverride,
        modelOptions: choice.modelOptions,
      }
      if (existingIndex < 0) {
        return [...current, nextDraft]
      }
      return current.map((draft, index) => index === existingIndex ? { ...draft, enabled: true } : draft)
    })
  }, [])
  const updateDraftModel = React.useCallback((vendorId: string, targetAlias: string, modelId: string) => {
    setDrafts((current) => current.map((draft) => (
      draft.vendorId === vendorId && draft.targetAlias === targetAlias ? { ...draft, modelId } : draft
    )))
  }, [])
  const save = React.useCallback(async () => {
    await props.onSave(drafts.map((draft) => ({
      vendorId: draft.vendorId,
      targetAlias: draft.targetAlias,
      modelId: draft.modelId,
      enabled: draft.enabled,
    })))
    props.onOpenChange(false)
  }, [drafts, props])
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog model-list-dialog">
        <DialogHeader>
          <DialogTitle>{text(`配置接口 · ${props.capability.label}`, `Configure targets · ${props.capability.label}`)}</DialogTitle>
          <DialogDescription>{text("左侧选择供应商，右侧勾选要启用的接口。保存后会写回当前能力页的接口、顺序与配置。", "Select a vendor on the left and choose targets on the right. Save persists targets, order, and model config.")}</DialogDescription>
        </DialogHeader>
        <div className="settings-split-layout modal">
          <aside className="settings-provider-nav" aria-label={text("供应商", "Providers")}>
            <div className="settings-split-column-title">{text("供应商", "Providers")}</div>
            {props.vendors.map((vendor) => {
              const vendorChoices = (vendor.targetChoices ?? []).filter((choice) => choice.capabilityKind === props.capability.kind)
              return (
                <button
                  className={`settings-provider-nav-row ${selectedVendor?.vendorId === vendor.vendorId ? "active" : ""}`}
                  key={vendor.vendorId}
                  onClick={() => setSelectedVendorId(vendor.vendorId)}
                  type="button"
                >
                  <FileText size={14} />
                  <span>{vendor.name}</span>
                  <small>[{vendorChoices.length}]</small>
                </button>
              )
            })}
          </aside>
          <section className="settings-model-list-pane">
            <div className="settings-model-pane-header">
              <div>
                <div className="settings-split-column-title">{text("接口列表", "Target list")}</div>
                <small>{selectedVendor?.name ?? text("未选择供应商", "No provider selected")}</small>
              </div>
              <span className="settings-muted-line">{text("已勾选", "Selected")} {selectedCount}</span>
            </div>
            <div className="settings-model-row-list modal">
              {!selectedVendor ? (
                <p className="settings-muted-line">{text("当前没有能力供应商。", "No capability vendor is configured.")}</p>
              ) : choices.length === 0 ? (
                <p className="settings-muted-line">{text("该供应商没有当前能力的接口。", "This vendor has no target for the current capability.")}</p>
              ) : choices.map((choice) => {
                const draft = drafts.find((item) => item.vendorId === selectedVendor.vendorId && item.targetAlias === choice.targetAlias)
                return (
                  <label className="settings-model-row selectable capability-target-choice" key={`${selectedVendor.vendorId}-${choice.targetAlias}`}>
                    <input
                      checked={Boolean(draft)}
                      onChange={(event) => toggleChoice(selectedVendor, choice, event.target.checked)}
                      type="checkbox"
                    />
                    <span>
                      <strong>{choice.label}</strong>
                      <small>{selectedVendor.vendorId}/{choice.targetAlias}</small>
                    </span>
                    {draft ? (
                      choice.modelOptions.length > 0 ? (
                        <select className="settings-input compact" onChange={(event) => updateDraftModel(selectedVendor.vendorId, choice.targetAlias, event.target.value)} value={draft.modelId}>
                          {choice.modelOptions.map((option) => <option key={option} value={option}>{option}</option>)}
                        </select>
                      ) : choice.supportsModelOverride ? (
                        <input className="settings-input compact" onChange={(event) => updateDraftModel(selectedVendor.vendorId, choice.targetAlias, event.target.value)} value={draft.modelId} />
                      ) : (
                        <span className="settings-muted-line">{draft.modelId}</span>
                      )
                    ) : null}
                  </label>
                )
              })}
            </div>
          </section>
        </div>
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">{text("取消", "Cancel")}</button>
          <button className="skill-action-button" onClick={() => void save()} type="button">{text("保存", "Save")}</button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// AiCapabilityTargetDraft stores one dialog-local target binding draft.
// AiCapabilityTargetDraft 保存弹窗内单个目标绑定草稿。
interface AiCapabilityTargetDraft {
  // Stable vendor identifier.
  // 稳定供应商标识。
  vendorId: string
  // User-facing vendor name.
  // 面向用户显示的供应商名称。
  vendorName: string
  // Stable target alias.
  // 稳定目标别名。
  targetAlias: string
  // User-facing target name.
  // 面向用户显示的目标名称。
  targetName: string
  // Provider request model id.
  // Provider 请求模型标识。
  modelId: string
  // Whether the target is enabled.
  // 目标是否启用。
  enabled: boolean
  // Whether the request model can be edited.
  // 请求模型是否可编辑。
  supportsModelOverride: boolean
  // Optional finite model choices.
  // 可选有限模型选项。
  modelOptions: string[]
}

// aiCapabilityTargetInputFromSummary converts one rendered summary into a save input.
// aiCapabilityTargetInputFromSummary 将渲染摘要转换为保存输入。
function aiCapabilityTargetInputFromSummary(target: AiCapabilityTargetSummary): AiCapabilityTargetSaveInput {
  return {
    vendorId: target.vendorId,
    targetAlias: target.targetAlias,
    modelId: target.modelId,
    enabled: target.enabled,
  }
}

// aiCapabilityTargetInputSelector builds the same stable selector used by rendered summaries.
// aiCapabilityTargetInputSelector 构建与渲染摘要一致的稳定选择器。
function aiCapabilityTargetInputSelector(input: AiCapabilityTargetSaveInput): string {
  return `${input.vendorId}/${input.targetAlias}`
}

// aiCapabilityTargetSummarySignature tracks meaningful snapshot changes without depending on array identity.
// aiCapabilityTargetSummarySignature 跟踪快照的关键变化，避免依赖数组引用身份。
function aiCapabilityTargetSummarySignature(targets: AiCapabilityTargetSummary[]): string {
  return targets.map((target) => [
    target.targetSelector,
    target.vendorId,
    target.targetAlias,
    target.modelId,
    target.enabled ? "1" : "0",
  ].join(":")).join("|")
}

// useVerticalSortableItem centralizes vertical-only dnd-kit row behavior for settings lists.
// useVerticalSortableItem 集中设置列表纵向 dnd-kit 行为并限制横向漂移。
function useVerticalSortableItem(id: string, disabled = false) {
  const sortable = useSortable({ id, disabled })
  const sortableTransform = sortable.transform ? { ...sortable.transform, x: 0, scaleX: 1, scaleY: 1 } : null
  const sortableStyle: React.CSSProperties = {
    transform: CSS.Transform.toString(sortableTransform),
    transition: sortable.transition,
  }
  return {
    ...sortable,
    sortableStyle,
  }
}

// McpSettingsArea renders MCP runtime status using startup and diagnostic snapshots.
// McpSettingsArea 使用启动与诊断快照渲染 MCP 运行态。
function McpSettingsArea(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Diagnostic log entries.
  // 诊断日志条目。
  logs: LogEntrySummary[]
  // Runtime modules used to locate MCP status.
  // 用于定位 MCP 状态的运行时模块。
  modules: RuntimeModuleSnapshot[]
  // Persisted MCP servers.
  // 持久化 MCP 服务器。
  servers: McpServerSummary[]
  // Startup snapshot used to locate MCP startup steps.
  // 用于定位 MCP 启动步骤的启动快照。
  startup: StartupSnapshot
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [editingServer, setEditingServer] = React.useState<McpServerSummary | null>(null)
  const [creatingServer, setCreatingServer] = React.useState(false)
  const mcpModules = props.modules.filter((module) => module.id.toLowerCase().includes("mcp"))
  const mcpSteps = props.startup.steps.filter((step) => step.id.toLowerCase().includes("mcp") || step.label.toLowerCase().includes("mcp"))
  const mcpLogs = props.logs.filter((log) => log.source.toLowerCase().includes("mcp") || log.message.toLowerCase().includes("mcp"))
  return (
    <div className="area-grid two-columns">
      <section className="panel settings-detail-panel">
        <div className="settings-panel-title-row">
          <PanelTitle icon={<FolderTree size={17} />} title={text("MCP 服务器", "MCP servers")} />
          <button className="skill-action-button" onClick={() => setCreatingServer(true)} type="button">{text("添加 MCP", "Add MCP")}</button>
        </div>
        <div className="settings-provider-list">
          {props.servers.length === 0 ? (
            <p className="settings-muted-line">{text("当前没有 MCP 服务器配置。", "No MCP servers are configured.")}</p>
          ) : props.servers.map((server) => (
            <article className="settings-provider-row" key={server.id}>
              <header>
                <div>
                  <strong>{server.name}</strong>
                  <span>{server.id} · {server.transport}</span>
                </div>
                <div className="chip-row">
                  <StatusPill status={server.enabled ? "ready" : "pending"} label={server.enabled ? text("启用", "Enabled") : text("停用", "Disabled")} />
                  <button className="skill-action-button secondary" onClick={() => setEditingServer(server)} type="button">{text("编辑", "Edit")}</button>
                  <button className="skill-action-button secondary danger" onClick={() => void props.actions.onRemoveMcpServer(server.id)} type="button">{text("删除", "Delete")}</button>
                </div>
              </header>
              <span className="settings-muted-line">{server.transport === "stdio" ? server.command : server.url}</span>
            </article>
          ))}
        </div>
        <McpServerDialog
          actions={props.actions}
          onOpenChange={(open) => {
            if (!open) {
              setCreatingServer(false)
              setEditingServer(null)
            }
          }}
          open={creatingServer || editingServer !== null}
          server={editingServer}
        />
      </section>
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<History size={17} />} title={text("MCP 状态", "MCP status")} />
        <RuntimeModulesPanel modules={mcpModules.length ? mcpModules : props.modules.filter((module) => module.id === "core")} />
        <div className="step-list">
          {mcpSteps.map((step) => (
            <div className="step-item" key={step.id}>
              <StatusIcon status={step.status} />
              <div>
                <strong>{step.label}</strong>
                <span>{step.message ?? step.id}</span>
              </div>
            </div>
          ))}
        </div>
        <div className="log-list">
          {mcpLogs.length === 0 ? (
            <p className="settings-muted-line">{text("当前没有 MCP 相关诊断日志。", "No MCP diagnostic logs are available.")}</p>
          ) : mcpLogs.map((log) => (
            <article className="log-row" key={log.id}>
              <StatusPill status={log.level === "info" ? "ready" : "warning"} label={log.level} />
              <strong>{log.source}</strong>
              <span>{log.message}</span>
            </article>
          ))}
        </div>
      </section>
    </div>
  )
}

// McpServerDialog renders the MCP server add/edit form.
// McpServerDialog 渲染 MCP 服务器新增/编辑表单。
function McpServerDialog(props: {
  // Service-backed settings actions.
  // 服务端支持的设置动作。
  actions: SettingsActionProps
  // Whether the dialog is open.
  // 弹窗是否打开。
  open: boolean
  // Updates the dialog open state.
  // 更新弹窗打开状态。
  onOpenChange: (open: boolean) => void
  // Server being edited.
  // 当前编辑的服务器。
  server: McpServerSummary | null
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const [name, setName] = React.useState("")
  const [enabled, setEnabled] = React.useState(true)
  const [transport, setTransport] = React.useState("stdio")
  const [command, setCommand] = React.useState("")
  const [argsText, setArgsText] = React.useState("")
  const [url, setUrl] = React.useState("")
  const [bearerToken, setBearerToken] = React.useState("")
  const [headersText, setHeadersText] = React.useState("")
  const [headersFromEnvText, setHeadersFromEnvText] = React.useState("")
  const [oauthClientId, setOauthClientId] = React.useState("")
  const [oauthClientSecret, setOauthClientSecret] = React.useState("")
  const [oauthScopesText, setOauthScopesText] = React.useState("")
  const [oauthRedirectUri, setOauthRedirectUri] = React.useState("")
  const [cwd, setCwd] = React.useState("")
  const [envText, setEnvText] = React.useState("")
  const [envPassthroughText, setEnvPassthroughText] = React.useState("")
  React.useEffect(() => {
    const server = props.server
    setName(server?.name ?? "")
    setEnabled(server?.enabled ?? true)
    setTransport(server?.transport ?? "stdio")
    setCommand(server?.command ?? "")
    setArgsText((server?.args ?? []).join("\n"))
    setUrl(server?.url ?? "")
    setBearerToken(server?.bearerToken ?? "")
    setHeadersText(formatKeyValueLines(server?.headers ?? []))
    setHeadersFromEnvText(formatKeyValueLines(server?.headersFromEnv ?? []))
    setOauthClientId(server?.oauthClientId ?? "")
    setOauthClientSecret(server?.oauthClientSecret ?? "")
    setOauthScopesText((server?.oauthScopes ?? []).join("\n"))
    setOauthRedirectUri(server?.oauthRedirectUri ?? "")
    setCwd(server?.cwd ?? "")
    setEnvText(formatKeyValueLines(server?.env ?? []))
    setEnvPassthroughText((server?.envPassthrough ?? []).join("\n"))
  }, [props.server])
  const save = React.useCallback(async () => {
    await props.actions.onSaveMcpServer({
      serverId: props.server?.id ?? null,
      name,
      enabled,
      transport,
      command,
      args: parseLines(argsText),
      url,
      bearerToken,
      headers: parseKeyValueLines(headersText),
      headersFromEnv: parseKeyValueLines(headersFromEnvText),
      oauthClientId,
      oauthClientSecret,
      oauthScopes: parseLines(oauthScopesText),
      oauthRedirectUri,
      cwd,
      env: parseKeyValueLines(envText),
      envPassthrough: parseLines(envPassthroughText),
    })
    props.onOpenChange(false)
  }, [argsText, bearerToken, command, cwd, enabled, envPassthroughText, envText, headersFromEnvText, headersText, name, oauthClientId, oauthClientSecret, oauthRedirectUri, oauthScopesText, props, transport, url])
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="settings-dialog wide">
        <DialogHeader>
          <DialogTitle>{props.server ? text("编辑 MCP 服务器", "Edit MCP server") : text("添加 MCP 服务器", "Add MCP server")}</DialogTitle>
          <DialogDescription>{text("多值字段每行一个；键值字段使用 KEY=VALUE。", "Use one line per value; key/value fields use KEY=VALUE.")}</DialogDescription>
        </DialogHeader>
        <div className="settings-form-grid two">
          <label className="settings-form-field">
            <span>{text("名称", "Name")}</span>
            <input className="settings-input" onChange={(event) => setName(event.target.value)} value={name} />
          </label>
          <label className="settings-form-field">
            <span>{text("传输", "Transport")}</span>
            <select className="settings-input" onChange={(event) => setTransport(event.target.value)} value={transport}>
              <option value="stdio">stdio</option>
              <option value="http">http</option>
              <option value="sse">sse</option>
            </select>
          </label>
          <label className="settings-check-row inline">
            <input checked={enabled} onChange={(event) => setEnabled(event.target.checked)} type="checkbox" />
            <span>{text("启用服务器", "Enable server")}</span>
          </label>
          {transport === "stdio" ? (
            <>
              <label className="settings-form-field">
                <span>{text("命令", "Command")}</span>
                <input className="settings-input" onChange={(event) => setCommand(event.target.value)} value={command} />
              </label>
              <label className="settings-form-field">
                <span>{text("参数", "Arguments")}</span>
                <textarea className="settings-input textarea" onChange={(event) => setArgsText(event.target.value)} value={argsText} />
              </label>
              <label className="settings-form-field">
                <span>CWD</span>
                <input className="settings-input" onChange={(event) => setCwd(event.target.value)} value={cwd} />
              </label>
              <label className="settings-form-field">
                <span>Env</span>
                <textarea className="settings-input textarea" onChange={(event) => setEnvText(event.target.value)} value={envText} />
              </label>
              <label className="settings-form-field">
                <span>{text("环境透传", "Env passthrough")}</span>
                <textarea className="settings-input textarea" onChange={(event) => setEnvPassthroughText(event.target.value)} value={envPassthroughText} />
              </label>
            </>
          ) : (
            <>
              <label className="settings-form-field">
                <span>URL</span>
                <input className="settings-input" onChange={(event) => setUrl(event.target.value)} value={url} />
              </label>
              <label className="settings-form-field">
                <span>Bearer Token</span>
                <input className="settings-input" onChange={(event) => setBearerToken(event.target.value)} type="password" value={bearerToken} />
              </label>
              <label className="settings-form-field">
                <span>Headers</span>
                <textarea className="settings-input textarea" onChange={(event) => setHeadersText(event.target.value)} value={headersText} />
              </label>
              <label className="settings-form-field">
                <span>Headers from env</span>
                <textarea className="settings-input textarea" onChange={(event) => setHeadersFromEnvText(event.target.value)} value={headersFromEnvText} />
              </label>
              <label className="settings-form-field">
                <span>OAuth Client ID</span>
                <input className="settings-input" onChange={(event) => setOauthClientId(event.target.value)} value={oauthClientId} />
              </label>
              <label className="settings-form-field">
                <span>OAuth Client Secret</span>
                <input className="settings-input" onChange={(event) => setOauthClientSecret(event.target.value)} type="password" value={oauthClientSecret} />
              </label>
              <label className="settings-form-field">
                <span>OAuth Scopes</span>
                <textarea className="settings-input textarea" onChange={(event) => setOauthScopesText(event.target.value)} value={oauthScopesText} />
              </label>
              <label className="settings-form-field">
                <span>OAuth Redirect URI</span>
                <input className="settings-input" onChange={(event) => setOauthRedirectUri(event.target.value)} value={oauthRedirectUri} />
              </label>
            </>
          )}
        </div>
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">{text("取消", "Cancel")}</button>
          <button className="skill-action-button" onClick={() => void save()} type="button">{text("保存", "Save")}</button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// parseLines converts a multiline text area into a trimmed string list.
// parseLines 将多行文本框转换为裁剪后的字符串列表。
function parseLines(value: string): string[] {
  return value.split(/\r?\n/g).map((line) => line.trim()).filter(Boolean)
}

// parseKeyValueLines converts KEY=VALUE lines into key/value rows.
// parseKeyValueLines 将 KEY=VALUE 行转换为键值行。
function parseKeyValueLines(value: string): Array<{ key: string; value: string }> {
  return parseLines(value).map((line) => {
    const separatorIndex = line.indexOf("=")
    if (separatorIndex < 0) {
      return { key: line, value: "" }
    }
    return {
      key: line.slice(0, separatorIndex).trim(),
      value: line.slice(separatorIndex + 1).trim(),
    }
  })
}

// formatKeyValueLines converts key/value rows into KEY=VALUE lines.
// formatKeyValueLines 将键值行转换为 KEY=VALUE 行。
function formatKeyValueLines(values: Array<{ key: string; value: string }>): string {
  return values.map((entry) => `${entry.key}=${entry.value}`).join("\n")
}

// PromptComposerSettingsArea renders composer command and capability settings status.
// PromptComposerSettingsArea 渲染输入区指令与能力设置状态。
function PromptComposerSettingsArea(props: {
  // Composer attachment capability gates.
  // 输入区附件能力门控。
  attachmentCapabilities: ComposerAttachmentCapabilitySummary[]
  // Backend-resolved reasoning selector view.
  // 后端解析出的推理选择器视图。
  reasoningSelection: WorkbenchSnapshot["reasoningSelection"] | null
  // Composer slash commands.
  // 输入区 slash 指令。
  slashCommands: ComposerSlashCommandSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  return (
    <div className="area-grid two-columns">
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<FileText size={17} />} title={text("Slash 指令", "Slash commands")} />
        <div className="settings-target-list">
          {props.slashCommands.map((command) => (
            <article className="settings-target-row" key={command.id}>
              <div>
                <strong>{command.command}</strong>
                <span>{command.descriptionZh || command.descriptionEn}</span>
              </div>
              <Badge variant="outline">{command.id}</Badge>
            </article>
          ))}
        </div>
      </section>
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<PackagePlus size={17} />} title={text("附件能力", "Attachment capability")} />
        <div className="settings-grid compact">
          {props.attachmentCapabilities.map((capability) => (
            <article className="settings-card" key={capability.kind}>
              <StatusPill status={capability.enabled ? "ready" : "warning"} label={capability.enabled ? "enabled" : "disabled"} />
              <strong>{capability.label}</strong>
              <p>{capability.native ? text("模型原生支持", "Native model support") : capability.bridged ? text("通过宿主工具桥接", "Bridged by host tools") : text("当前不可用", "Currently unavailable")}</p>
            </article>
          ))}
        </div>
      </section>
      <section className="panel settings-detail-panel">
        <PanelTitle icon={<SlidersHorizontal size={17} />} title={text("推理选择", "Reasoning selection")} />
        <div className="settings-target-list">
          {(props.reasoningSelection?.options ?? []).map((option) => (
            <article className="settings-target-row" key={option.value}>
              <div>
                <strong>{option.label}</strong>
                <span>{option.value}</span>
              </div>
              {option.value === props.reasoningSelection?.selectedEffort ? <Badge className="skill-card-enabled-badge">Selected</Badge> : <Badge variant="outline">Available</Badge>}
            </article>
          ))}
        </div>
      </section>
    </div>
  )
}

// AboutLicensesSettingsArea renders application version and license entry information.
// AboutLicensesSettingsArea 渲染应用版本与许可证入口信息。
function AboutLicensesSettingsArea(props: {
  // Current application version.
  // 当前应用版本。
  appVersion: string
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  return (
    <section className="panel full-height settings-detail-panel">
      <PanelTitle icon={<ScrollText size={17} />} title={text("关于与许可证", "About and licenses")} />
      <div className="settings-grid">
        <article className="settings-card">
          <StatusPill status="ready" label="version" />
          <strong>Vulcan Code</strong>
          <p>{text(`当前版本：${props.appVersion}`, `Current version: ${props.appVersion}`)}</p>
        </article>
        <article className="settings-card">
          <StatusPill status="pending" label="licenses" />
          <strong>{text("许可证目录", "License directory")}</strong>
          <p>{text("iced 版本包含许可证目录打开与加载状态；WebUI 当前展示入口状态，后续接入目录命令后可直接打开。", "The iced version exposes license loading and directory opening. WebUI shows the entry state until the directory command is connected.")}</p>
        </article>
      </div>
    </section>
  )
}

// ArchivedRestoreSettingsArea renders restorable archived conversations from service snapshots.
// ArchivedRestoreSettingsArea 根据服务快照渲染可恢复的归档会话。
function ArchivedRestoreSettingsArea(props: {
  // Archived conversation summaries.
  // 归档会话摘要。
  conversations: ConversationSummary[]
  // Active project summaries.
  // 活动项目摘要。
  projects: ProjectSummary[]
}) {
  const { locale } = useI18n()
  const text = React.useCallback((zh: string, en: string) => localizedLiteral(locale, zh, en), [locale])
  const archivedConversations = props.conversations
  return (
    <section className="panel full-height settings-detail-panel">
      <PanelTitle icon={<Archive size={17} />} title={text("归档恢复", "Archived restore")} />
      <div className="settings-summary-strip">
        <StatusPill status={archivedConversations.length > 0 ? "warning" : "ready"} label={`${archivedConversations.length}`} />
        <span>{text(`当前 WebUI snapshot 暴露 ${props.projects.length} 个活动项目与 ${archivedConversations.length} 个可恢复归档会话。`, `The current WebUI snapshot exposes ${props.projects.length} active projects and ${archivedConversations.length} restorable archived conversations.`)}</span>
      </div>
      <div className="settings-target-list">
        {archivedConversations.length === 0 ? (
          <p className="settings-muted-line">{text("当前快照中没有可恢复的归档会话。", "No restorable archived conversations are exposed in the current snapshot.")}</p>
        ) : archivedConversations.map((conversation) => (
          <article className="settings-target-row" key={conversation.id}>
            <div>
              <strong>{conversation.title}</strong>
              <span>{conversation.projectId ?? "standalone"} · {conversation.lastUpdatedLabel ?? ""}</span>
            </div>
            <StatusPill status="warning" label={conversation.status} />
          </article>
        ))}
      </div>
    </section>
  )
}

// settingsSubAreaCapabilityKind maps a settings page to the backend AI capability kind.
// settingsSubAreaCapabilityKind 将设置页面映射到后端 AI 能力类型。
function settingsSubAreaCapabilityKind(area: SettingsSubAreaId): string | null {
  const mapping: Partial<Record<SettingsSubAreaId, string>> = {
    "ai-web-search": "web_search",
    "ai-understand-image": "understand_image",
    "ai-understand-video": "understand_video",
    "ai-understand-audio": "understand_audio",
    "ai-image-generation": "image_generation",
    "ai-video-generation": "video_generation",
    "ai-speech-synthesis": "speech_synthesis",
    "ai-music-generation": "music_generation",
    "ai-music-cover": "music_cover",
  }
  return mapping[area] ?? null
}

// ToolsArea renders tool calls and command sessions.
// ToolsArea 渲染工具调用与命令会话。
function ToolsArea(props: {
  // Tool call summaries shown in settings.
  // 设置中展示的工具调用摘要。
  toolCalls: ToolCallSummary[]
  // Command session summaries shown in settings.
  // 设置中展示的命令会话摘要。
  commandSessions: CommandSessionSummary[]
}) {
  return (
    <div className="area-grid two-columns">
      <ToolPanel toolCalls={props.toolCalls} />
      <CommandSessionsPanel commandSessions={props.commandSessions} />
    </div>
  )
}

// PluginsArea renders plugin control center.
// PluginsArea 渲染插件控制中心。
function PluginsArea(props: {
  // Plugin summaries shown in the plugin center.
  // 插件中心展示的插件摘要。
  plugins: PluginSummary[]
}) {
  return (
    <section className="panel full-height">
      <PanelTitle icon={<Wrench size={17} />} title="插件中心" />
      <div className="card-grid">
        {props.plugins.map((plugin) => (
          <article className="data-card" key={plugin.id}>
            <StatusPill status={plugin.status === "已启用" ? "ready" : "warning"} label={plugin.status} />
            <strong>{plugin.name}</strong>
            <span>{plugin.kind}</span>
            <p>{plugin.capability}</p>
          </article>
        ))}
      </div>
    </section>
  )
}

// VmmScope identifies the active VMM configuration scope.
// VmmScope 标识当前 VMM 配置作用域。
type VmmScope = "global" | "project"

// MemoryArea renders the VMM memory management workbench.
// MemoryArea 渲染 VMM 记忆管理工作台。
function MemoryArea(props: {
  // Active VMM section selected from the shared feature sidebar.
  // 从共享功能侧栏选中的当前 VMM 分区。
  activeSection: MemorySubAreaId
  // Service-backed VMM actions.
  // 服务端支持的 VMM 动作。
  actions: MemoryActionProps
  // Memory summaries shown in the memory page.
  // 记忆页展示的记忆摘要。
  memories: MemorySummary[]
  // Detailed VMM snapshot.
  // 详细 VMM 快照。
  snapshot: VmmMemorySnapshot
}) {
  const [scope, setScope] = React.useState<VmmScope>("global");
  const [projectId, setProjectId] = React.useState(props.snapshot.selectedProjectId ?? props.snapshot.projectScopes[0]?.projectId ?? "");
  const [globalDraft, setGlobalDraft] = React.useState(props.snapshot.globalDraft);
  const [projectDraft, setProjectDraft] = React.useState(props.snapshot.projectDraft);
  const [result, setResult] = React.useState<VmmMemoryActionResult | null>(null);
  const [notice, setNotice] = React.useState(props.snapshot.notice);
  const [busyAction, setBusyAction] = React.useState<string | null>(null);
  const [userRef, setUserRef] = React.useState(props.snapshot.effective.userId?.toString() ?? "");
  const [userConfirmationCode, setUserConfirmationCode] = React.useState("");
  const [projectPath, setProjectPath] = React.useState("");
  const [projectDeleteConfirmed, setProjectDeleteConfirmed] = React.useState(false);
  const [migrateSource, setMigrateSource] = React.useState("");
  const [migrateTarget, setMigrateTarget] = React.useState("");
  const [migrateConfirmed, setMigrateConfirmed] = React.useState(false);
  const [profileTarget, setProfileTarget] = React.useState("user");
  const [profileInstruction, setProfileInstruction] = React.useState("");
  const [searchQueries, setSearchQueries] = React.useState("");
  const [searchTopK, setSearchTopK] = React.useState("5");
  const [turnIds, setTurnIds] = React.useState("");
  const [writeAbstract, setWriteAbstract] = React.useState("");
  const [writeDetails, setWriteDetails] = React.useState("");
  const [writeScopeLevel, setWriteScopeLevel] = React.useState("2");
  const [writeCategory, setWriteCategory] = React.useState("1");
  const [writePriority, setWritePriority] = React.useState("2");
  const [writeMemoryLevel, setWriteMemoryLevel] = React.useState("2");

  React.useEffect(() => {
    setGlobalDraft(props.snapshot.globalDraft);
    setProjectDraft(props.snapshot.projectDraft);
    setProjectId(props.snapshot.selectedProjectId ?? props.snapshot.projectScopes[0]?.projectId ?? "");
    setNotice(props.snapshot.notice);
  }, [props.snapshot]);

  const activeDraft = scope === "global" ? globalDraft : projectDraft;
  const setActiveDraft = (patch: Partial<VmmConfigDraftSummary>) => {
    if (scope === "global") {
      setGlobalDraft((draft) => ({ ...draft, ...patch }));
    } else {
      setProjectDraft((draft) => ({ ...draft, ...patch }));
    }
  };

  const runAction = async (action: string, payload: Record<string, unknown> = {}) => {
    setBusyAction(action);
    setNotice(`正在执行 VMM 动作：${action}`);
    try {
      const nextResult = await props.actions.onRunVmmAction({
        action,
        scope,
        projectId: scope === "project" ? projectId : null,
        payload,
      });
      setResult(nextResult);
      setNotice(nextResult.summary);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setNotice(`VMM 动作失败：${message}`);
    } finally {
      setBusyAction(null);
    }
  };

  const saveDraft = async (targetScope: VmmScope = scope) => {
    const draft = targetScope === "global" ? globalDraft : projectDraft;
    setBusyAction(`save-${targetScope}`);
    setNotice(targetScope === "global" ? "正在保存全局 VMM 配置。" : "正在保存项目 VMM 配置。");
    try {
      await props.actions.onSaveVmmConfig({
        scope: targetScope,
        projectId: targetScope === "project" ? projectId : null,
        draft,
      });
      setNotice(targetScope === "global" ? "已保存全局 VMM 配置。" : "已保存项目 VMM 配置。");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setNotice(`保存 VMM 配置失败：${message}`);
    } finally {
      setBusyAction(null);
    }
  };

  const activeSectionDetail = renderVmmSectionDetail({
    activeDraft,
    activeSection: props.activeSection,
    busyAction,
    globalDraft,
    migrateConfirmed,
    migrateSource,
    migrateTarget,
    notice,
    profileInstruction,
    profileTarget,
    projectDeleteConfirmed,
    projectDraft,
    projectId,
    projectPath,
    result,
    scope,
    searchQueries,
    searchTopK,
    setActiveDraft,
    setGlobalDraft,
    setMigrateConfirmed,
    setMigrateSource,
    setMigrateTarget,
    setProfileInstruction,
    setProfileTarget,
    setProjectDeleteConfirmed,
    setProjectId,
    setProjectPath,
    setScope,
    setSearchQueries,
    setSearchTopK,
    setTurnIds,
    setUserConfirmationCode,
    setUserRef,
    setWriteAbstract,
    setWriteCategory,
    setWriteDetails,
    setWriteMemoryLevel,
    setWritePriority,
    setWriteScopeLevel,
    snapshot: props.snapshot,
    turnIds,
    userConfirmationCode,
    userRef,
    writeAbstract,
    writeCategory,
    writeDetails,
    writeMemoryLevel,
    writePriority,
    writeScopeLevel,
    onRunAction: runAction,
    onSaveDraft: saveDraft,
  });

  if (props.activeSection === "status") {
    return (
      <section className="vmm-memory-page">
        <div className="vmm-memory-shell status-only">
          <main className="vmm-memory-main vmm-status-page">
            <VmmEffectiveOverview memories={props.memories} snapshot={props.snapshot} />
            <div className="vmm-status-grid">
              <VmmRuntimePanel snapshot={props.snapshot} />
              <VmmResultPanel result={result} />
            </div>
          </main>
        </div>
      </section>
    )
  }

  return (
    <section className="vmm-memory-page">
      <div className="vmm-memory-shell">
        <main className="vmm-memory-main">
          <div className="vmm-scope-bar">
            <label>
              <span>作用域</span>
              <select value={scope} onChange={(event) => setScope(event.target.value as VmmScope)}>
                <option value="global">全局配置</option>
                <option value="project">项目配置</option>
              </select>
            </label>
            <label>
              <span>项目</span>
              <select disabled={scope !== "project"} value={projectId} onChange={(event) => setProjectId(event.target.value)}>
                {props.snapshot.projectScopes.map((project) => (
                  <option key={project.projectId} value={project.projectId}>{project.name}</option>
                ))}
              </select>
            </label>
            <button className="vmm-icon-button" onClick={() => runAction("healthz")} type="button">
              <RefreshCw size={15} />测试连接
            </button>
            <button className="vmm-icon-button" onClick={() => runAction("load_users_projects")} type="button">
              <ListRestart size={15} />刷新用户/项目
            </button>
          </div>
          {activeSectionDetail}
        </main>
        <aside className="vmm-memory-result">
          <VmmRuntimePanel snapshot={props.snapshot} />
          <VmmResultPanel result={result} />
        </aside>
      </div>
    </section>
  )
}

// VmmSectionRenderProps groups state and callbacks used by VMM section renderers.
// VmmSectionRenderProps 分组 VMM 分区渲染器使用的状态与回调。
interface VmmSectionRenderProps {
  activeDraft: VmmConfigDraftSummary
  activeSection: string
  busyAction: string | null
  globalDraft: VmmConfigDraftSummary
  migrateConfirmed: boolean
  migrateSource: string
  migrateTarget: string
  notice: string
  profileInstruction: string
  profileTarget: string
  projectDeleteConfirmed: boolean
  projectDraft: VmmConfigDraftSummary
  projectId: string
  projectPath: string
  result: VmmMemoryActionResult | null
  scope: VmmScope
  searchQueries: string
  searchTopK: string
  snapshot: VmmMemorySnapshot
  turnIds: string
  userConfirmationCode: string
  userRef: string
  writeAbstract: string
  writeCategory: string
  writeDetails: string
  writeMemoryLevel: string
  writePriority: string
  writeScopeLevel: string
  setActiveDraft: (patch: Partial<VmmConfigDraftSummary>) => void
  setGlobalDraft: React.Dispatch<React.SetStateAction<VmmConfigDraftSummary>>
  setMigrateConfirmed: (value: boolean) => void
  setMigrateSource: (value: string) => void
  setMigrateTarget: (value: string) => void
  setProfileInstruction: (value: string) => void
  setProfileTarget: (value: string) => void
  setProjectDeleteConfirmed: (value: boolean) => void
  setProjectId: (value: string) => void
  setProjectPath: (value: string) => void
  setScope: (value: VmmScope) => void
  setSearchQueries: (value: string) => void
  setSearchTopK: (value: string) => void
  setTurnIds: (value: string) => void
  setUserConfirmationCode: (value: string) => void
  setUserRef: (value: string) => void
  setWriteAbstract: (value: string) => void
  setWriteCategory: (value: string) => void
  setWriteDetails: (value: string) => void
  setWriteMemoryLevel: (value: string) => void
  setWritePriority: (value: string) => void
  setWriteScopeLevel: (value: string) => void
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<void>
  onSaveDraft: (scope?: VmmScope) => Promise<void>
}

// renderVmmSectionDetail renders the active VMM management section.
// renderVmmSectionDetail 渲染当前激活的 VMM 管理分区。
function renderVmmSectionDetail(props: VmmSectionRenderProps) {
  if (props.activeSection === "project_manager") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<FolderTree size={16} />} title="项目管理" subtitle="项目路径预检、创建确认、删除确认和迁移确认。" />
        <div className="vmm-form-grid">
          <VmmTextField label="项目路径" placeholder="Team/Space/Project" value={props.projectPath} onChange={props.setProjectPath} />
          <label className="vmm-check-row"><input checked={props.projectDeleteConfirmed} onChange={(event) => props.setProjectDeleteConfirmed(event.target.checked)} type="checkbox" />确认删除</label>
          <VmmTextField label="迁移源项目" placeholder="OldTeam/OldSpace/OldProject" value={props.migrateSource} onChange={props.setMigrateSource} />
          <VmmTextField label="迁移目标项目" placeholder="NewTeam/NewSpace/NewProject" value={props.migrateTarget} onChange={props.setMigrateTarget} />
          <label className="vmm-check-row"><input checked={props.migrateConfirmed} onChange={(event) => props.setMigrateConfirmed(event.target.checked)} type="checkbox" />确认迁移</label>
        </div>
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === "ensure_project"} icon={<Search size={15} />} label="预检项目" onClick={() => props.onRunAction("ensure_project", { projectPath: props.projectPath, confirmCreate: false })} />
          <VmmActionButton busy={props.busyAction === "ensure_project"} icon={<PackagePlus size={15} />} label="确认创建" onClick={() => props.onRunAction("ensure_project", { projectPath: props.projectPath, confirmCreate: true })} />
          <VmmActionButton danger busy={props.busyAction === "delete_project"} icon={<Trash2 size={15} />} label="删除项目" onClick={() => props.onRunAction("delete_project", { projectPath: props.projectPath, confirmDelete: props.projectDeleteConfirmed })} />
          <VmmActionButton danger busy={props.busyAction === "migrate_project"} icon={<GitBranch size={15} />} label="执行迁移" onClick={() => props.onRunAction("migrate_project", { sourceProjectPath: props.migrateSource, targetProjectPath: props.migrateTarget, confirmMigrate: props.migrateConfirmed })} />
        </div>
        <VmmProjectRows result={props.result} />
      </section>
    );
  }
  if (props.activeSection === "profile_center") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<BrainCircuit size={16} />} title="画像中心" subtitle="读取 active profile nodes，并向当前目标提交手工画像指令。" />
        <div className="vmm-form-grid">
          <label className="vmm-field">
            <span>画像目标</span>
            <select value={props.profileTarget} onChange={(event) => props.setProfileTarget(event.target.value)}>
              <option value="user">用户画像</option>
              <option value="project">项目画像</option>
              <option value="team">团队画像</option>
              <option value="space">空间画像</option>
            </select>
          </label>
          <VmmTextField label="手工画像指令" placeholder="例如：降低某条偏好权重" value={props.profileInstruction} onChange={props.setProfileInstruction} />
        </div>
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === "load_profile_nodes"} icon={<RefreshCw size={15} />} label="刷新节点" onClick={() => props.onRunAction("load_profile_nodes", { target: props.profileTarget })} />
          <VmmActionButton busy={props.busyAction === "apply_profile_instruction"} icon={<Hammer size={15} />} label="应用指令" onClick={() => props.onRunAction("apply_profile_instruction", { target: props.profileTarget, instruction: props.profileInstruction })} />
        </div>
        <VmmProfileRows result={props.result} />
      </section>
    );
  }
  if (props.activeSection === "profile_bundle") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<ScrollText size={16} />} title="画像 Bundle 测试" subtitle="读取当前 user/project 的 FULL bundle，检查画像组合结果。" />
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === "load_profile_bundle"} icon={<RefreshCw size={15} />} label="读取完整画像" onClick={() => props.onRunAction("load_profile_bundle")} />
        </div>
        <VmmBundlePreview result={props.result} />
      </section>
    );
  }
  if (props.activeSection === "tools_debug") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<TerminalSquare size={16} />} title="工具调试" subtitle="模拟 VMM AI 专属工具：检索记忆、展开 turn、主动写入记忆。" />
        <div className="vmm-form-grid">
          <VmmTextField label="检索查询" placeholder="多条用 | 分隔" value={props.searchQueries} onChange={props.setSearchQueries} />
          <VmmTextField label="Top K" placeholder="5" value={props.searchTopK} onChange={props.setSearchTopK} />
          <VmmTextField label="Turn IDs" placeholder="用逗号或 | 分隔" value={props.turnIds} onChange={props.setTurnIds} />
          <VmmTextField label="主动写入摘要" placeholder="记忆摘要" value={props.writeAbstract} onChange={props.setWriteAbstract} />
          <VmmTextField label="主动写入详情" placeholder="完整内容" value={props.writeDetails} onChange={props.setWriteDetails} />
          <VmmTextField label="Scope" placeholder="2" value={props.writeScopeLevel} onChange={props.setWriteScopeLevel} />
          <VmmTextField label="Category" placeholder="1" value={props.writeCategory} onChange={props.setWriteCategory} />
          <VmmTextField label="Priority" placeholder="2" value={props.writePriority} onChange={props.setWritePriority} />
          <VmmTextField label="Level" placeholder="2" value={props.writeMemoryLevel} onChange={props.setWriteMemoryLevel} />
        </div>
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === "search_memory_events"} icon={<Search size={15} />} label="SearchMemoryEvents" onClick={() => props.onRunAction("search_memory_events", { queries: props.searchQueries, topK: props.searchTopK })} />
          <VmmActionButton busy={props.busyAction === "get_turn_details"} icon={<FileText size={15} />} label="GetTurnDetails" onClick={() => props.onRunAction("get_turn_details", { turnIds: props.turnIds })} />
          <VmmActionButton busy={props.busyAction === "write_memories"} icon={<Database size={15} />} label="WriteMemories" onClick={() => props.onRunAction("write_memories", { abstractText: props.writeAbstract, details: props.writeDetails, scopeLevel: props.writeScopeLevel, category: props.writeCategory, priority: props.writePriority, memoryLevel: props.writeMemoryLevel })} />
        </div>
        <VmmToolResult result={props.result} />
      </section>
    );
  }
  if (props.activeSection === "memory_settings") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<SlidersHorizontal size={16} />} title="记忆设置" subtitle="配置记忆开关、AI 工具、临时 TTL、画像刷新和注入说明。" />
        <VmmSettingsForm draft={props.activeDraft} mode="memory" onChange={props.setActiveDraft} />
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === `save-${props.scope}`} icon={<Save size={15} />} label={props.scope === "global" ? "保存到全局" : "保存到项目"} onClick={() => props.onSaveDraft()} />
        </div>
      </section>
    );
  }
  if (props.activeSection === "grpc_transport") {
    return (
      <section className="vmm-section-panel">
        <VmmPanelHeading icon={<PlugZap size={16} />} title="gRPC 传输" subtitle="连接地址、API key、超时、最大接收字节数与 keepalive 策略。" />
        <VmmSettingsForm draft={props.globalDraft} mode="transport" onChange={(patch) => props.setGlobalDraft((draft) => ({ ...draft, ...patch }))} />
        <div className="vmm-action-row">
          <VmmActionButton busy={props.busyAction === "save-global"} icon={<Save size={15} />} label="保存到全局" onClick={() => props.onSaveDraft("global")} />
        </div>
      </section>
    );
  }
  return (
    <section className="vmm-section-panel">
      <VmmPanelHeading icon={<User size={16} />} title="用户管理" subtitle="长期用户列表、解析用户引用、创建确认和删除确认。" />
      <div className="vmm-form-grid">
        <VmmTextField label="用户引用" placeholder="ID 或用户名" value={props.userRef} onChange={props.setUserRef} />
        <VmmTextField label="删除确认码" placeholder="首次删除后返回" value={props.userConfirmationCode} onChange={props.setUserConfirmationCode} />
      </div>
      <div className="vmm-action-row">
        <VmmActionButton busy={props.busyAction === "resolve_user"} icon={<Search size={15} />} label="解析用户" onClick={() => props.onRunAction("resolve_user", { userRef: props.userRef, confirmCreate: false })} />
        <VmmActionButton busy={props.busyAction === "resolve_user"} icon={<PackagePlus size={15} />} label="确认创建" onClick={() => props.onRunAction("resolve_user", { userRef: props.userRef, confirmCreate: true })} />
        <VmmActionButton danger busy={props.busyAction === "delete_user"} icon={<Trash2 size={15} />} label="删除用户" onClick={() => props.onRunAction("delete_user", { userRef: props.userRef, confirmationCode: props.userConfirmationCode })} />
      </div>
      <VmmUserRows result={props.result} />
    </section>
  );
}

// VmmEffectiveOverview renders compact effective-config cards.
// VmmEffectiveOverview 渲染紧凑的最终配置卡片。
function VmmEffectiveOverview(props: {
  // Legacy summary entries.
  // 旧摘要条目。
  memories: MemorySummary[]
  // Detailed VMM snapshot.
  // 详细 VMM 快照。
  snapshot: VmmMemorySnapshot
}) {
  const effective = props.snapshot.effective;
  return (
    <div className="vmm-overview-grid">
      <article className="vmm-overview-card">
        <StatusPill status={effective.diagnostic === "ready" ? "ready" : "warning"} label={vmmDiagnosticLabel(effective.diagnostic)} />
        <strong>有效配置</strong>
        <span>gRPC：{effective.grpcTarget || "未配置"}</span>
        <span>user/project：{effective.userId ?? "-"} / {effective.projectId ?? "-"}</span>
      </article>
      <article className="vmm-overview-card">
        <StatusPill status={effective.precheckDiagnostic === "ready" ? "ready" : "warning"} label={vmmDiagnosticLabel(effective.precheckDiagnostic)} />
        <strong>召回策略</strong>
        <span>TTL：{effective.implicitMemoryTurns} 回合</span>
        <span>前置召回：{effective.precheckEnabled ? "启用" : "关闭"} · AI 工具：{effective.aiToolsEnabled ? "启用" : "关闭"}</span>
      </article>
      <article className="vmm-overview-card">
        <StatusPill status="ready" label="runtime" />
        <strong>运行态</strong>
        <span>outbox：{props.snapshot.runtime.outboxCount} · 画像节点：{props.snapshot.runtime.profileSyncCount}</span>
        <span>已完成回合：{props.snapshot.runtime.completedTurns}</span>
      </article>
      <article className="vmm-overview-card">
        <StatusPill status="warning" label="summary" />
        <strong>摘要入口</strong>
        <span>{props.memories.map((memory) => `${memory.title} ${memory.state}`).join("；") || "暂无摘要"}</span>
      </article>
    </div>
  );
}

// VmmRuntimePanel renders selected-session runtime details.
// VmmRuntimePanel 渲染当前选中会话运行态详情。
function VmmRuntimePanel(props: {
  // Detailed VMM snapshot.
  // 详细 VMM 快照。
  snapshot: VmmMemorySnapshot
}) {
  return (
    <section className="vmm-side-card">
      <VmmPanelHeading icon={<History size={15} />} title="运行态" subtitle="当前会话的 VMM hook 与画像缓存。" />
      <dl className="vmm-stat-list">
        <div><dt>会话</dt><dd>{props.snapshot.selectedConversationId ?? "未选择"}</dd></div>
        <div><dt>outbox</dt><dd>{props.snapshot.runtime.outboxCount}</dd></div>
        <div><dt>画像节点</dt><dd>{props.snapshot.runtime.profileSyncCount}</dd></div>
        <div><dt>更新时间</dt><dd>{props.snapshot.runtime.updatedAt ?? "暂无"}</dd></div>
      </dl>
      <pre className="vmm-preview-text">{props.snapshot.runtime.profilePreview || "暂无画像基点。"}</pre>
    </section>
  );
}

// VmmResultPanel renders the latest VMM action JSON previews.
// VmmResultPanel 渲染最近一次 VMM 动作 JSON 预览。
function VmmResultPanel(props: {
  // Latest action result.
  // 最近动作结果。
  result: VmmMemoryActionResult | null
}) {
  return (
    <section className="vmm-side-card">
      <VmmPanelHeading icon={<FileJson2Icon />} title="动作结果" subtitle={props.result?.summary ?? "尚未执行 VMM 动作。"} />
      {props.result ? (
        <>
          <strong className="vmm-result-kind">{props.result.kind}</strong>
          <VmmJsonPreview title="请求" value={props.result.request} />
          <VmmJsonPreview title="响应" value={props.result.response} />
        </>
      ) : (
        <span className="vmm-muted">点击左侧动作后会显示请求与响应。</span>
      )}
    </section>
  );
}

// FileJson2Icon renders the file-json icon without widening imports further.
// FileJson2Icon 渲染 file-json 图标并避免继续扩大导入列表。
function FileJson2Icon() {
  return <FileText size={15} />;
}

// VmmPanelHeading renders a small VMM panel heading.
// VmmPanelHeading 渲染小型 VMM 面板标题。
function VmmPanelHeading(props: {
  // Heading icon.
  // 标题图标。
  icon: React.ReactNode
  // Heading title.
  // 标题文本。
  title: string
  // Heading subtitle.
  // 副标题文本。
  subtitle: string
}) {
  return (
    <header className="vmm-panel-heading">
      {props.icon}
      <div>
        <strong>{props.title}</strong>
        <span>{props.subtitle}</span>
      </div>
    </header>
  );
}

// VmmTextField renders one compact VMM text field.
// VmmTextField 渲染单个紧凑 VMM 文本字段。
function VmmTextField(props: {
  label: string
  placeholder: string
  value: string
  onChange: (value: string) => void
}) {
  return (
    <label className="vmm-field">
      <span>{props.label}</span>
      <input placeholder={props.placeholder} value={props.value} onChange={(event) => props.onChange(event.target.value)} />
    </label>
  );
}

// VmmActionButton renders one VMM action button.
// VmmActionButton 渲染单个 VMM 动作按钮。
function VmmActionButton(props: {
  busy: boolean
  danger?: boolean
  icon: React.ReactNode
  label: string
  onClick: () => void
}) {
  return (
    <button className={`vmm-icon-button${props.danger ? " danger" : ""}`} disabled={props.busy} onClick={props.onClick} type="button">
      {props.icon}{props.busy ? "执行中" : props.label}
    </button>
  );
}

// VmmSettingsForm renders editable VMM settings.
// VmmSettingsForm 渲染可编辑 VMM 设置。
function VmmSettingsForm(props: {
  draft: VmmConfigDraftSummary
  mode: "memory" | "transport"
  onChange: (patch: Partial<VmmConfigDraftSummary>) => void
}) {
  if (props.mode === "transport") {
    return (
      <div className="vmm-form-grid">
        <VmmTextField label="gRPC 地址" placeholder="127.0.0.1:50051" value={props.draft.grpcTarget} onChange={(grpcTarget) => props.onChange({ grpcTarget })} />
        <VmmTextField label="API Key" placeholder="可选" value={props.draft.grpcApiKey} onChange={(grpcApiKey) => props.onChange({ grpcApiKey })} />
        <VmmTextField label="请求超时" placeholder="30000" value={props.draft.receiveTimeoutMs} onChange={(receiveTimeoutMs) => props.onChange({ receiveTimeoutMs })} />
        <VmmTextField label="握手超时" placeholder="8000" value={props.draft.handshakeTimeoutMs} onChange={(handshakeTimeoutMs) => props.onChange({ handshakeTimeoutMs })} />
        <VmmTextField label="最大接收字节" placeholder="可选" value={props.draft.maxReceiveMessageBytes} onChange={(maxReceiveMessageBytes) => props.onChange({ maxReceiveMessageBytes })} />
        <VmmTextField label="keepalive 间隔" placeholder="30000" value={props.draft.grpcKeepaliveTimeMs} onChange={(grpcKeepaliveTimeMs) => props.onChange({ grpcKeepaliveTimeMs })} />
        <VmmTextField label="keepalive 超时" placeholder="10000" value={props.draft.grpcKeepaliveTimeoutMs} onChange={(grpcKeepaliveTimeoutMs) => props.onChange({ grpcKeepaliveTimeoutMs })} />
        <label className="vmm-check-row"><input checked={props.draft.grpcKeepalivePermitWithoutCalls} onChange={(event) => props.onChange({ grpcKeepalivePermitWithoutCalls: event.target.checked })} type="checkbox" />空闲 keepalive</label>
      </div>
    );
  }
  return (
    <div className="vmm-form-grid">
      <label className="vmm-check-row"><input checked={props.draft.enabled} onChange={(event) => props.onChange({ enabled: event.target.checked })} type="checkbox" />启用记忆连接</label>
      <label className="vmm-check-row"><input checked={props.draft.aiToolsEnabled} onChange={(event) => props.onChange({ aiToolsEnabled: event.target.checked })} type="checkbox" />暴露 AI 专属工具</label>
      <label className="vmm-check-row"><input checked={props.draft.precheckEnabled} onChange={(event) => props.onChange({ precheckEnabled: event.target.checked })} type="checkbox" />自动前置召回</label>
      <VmmTextField label="用户 ID" placeholder="数字" value={props.draft.userId} onChange={(userId) => props.onChange({ userId })} />
      <VmmTextField label="项目 ID" placeholder="数字" value={props.draft.projectId} onChange={(projectId) => props.onChange({ projectId })} />
      <VmmTextField label="临时记忆 TTL" placeholder="5" value={props.draft.implicitMemoryTurns} onChange={(implicitMemoryTurns) => props.onChange({ implicitMemoryTurns })} />
      <VmmTextField label="画像刷新间隔" placeholder="3" value={props.draft.profileRefreshTurns} onChange={(profileRefreshTurns) => props.onChange({ profileRefreshTurns })} />
      <VmmTextField label="画像节点上限" placeholder="24" value={props.draft.profileNodeLimit} onChange={(profileNodeLimit) => props.onChange({ profileNodeLimit })} />
      <VmmTextField label="注入标签" placeholder="vmm_inject" value={props.draft.injectionTag} onChange={(injectionTag) => props.onChange({ injectionTag })} />
      <VmmTextField label="注入说明" placeholder="VMM recalled memory" value={props.draft.injectionDescription} onChange={(injectionDescription) => props.onChange({ injectionDescription })} />
    </div>
  );
}

// VmmUserRows renders user rows from a VMM result.
// VmmUserRows 从 VMM 结果渲染用户行。
function VmmUserRows(props: { result: VmmMemoryActionResult | null }) {
  const users = vmmResultArray(props.result, "users");
  if (users.length === 0) {
    return <p className="vmm-muted">执行“刷新用户/项目”后会显示用户列表。</p>;
  }
  return <div className="vmm-row-list">{users.map((user, index) => <VmmDataRow key={index} title={String(user.userName ?? user.user_name ?? "未命名用户")} detail={`ID：${String(user.userId ?? user.user_id ?? "-")}`} />)}</div>;
}

// VmmProjectRows renders project rows from a VMM result.
// VmmProjectRows 从 VMM 结果渲染项目行。
function VmmProjectRows(props: { result: VmmMemoryActionResult | null }) {
  const projects = vmmResultArray(props.result, "projects");
  if (projects.length === 0) {
    return <p className="vmm-muted">执行“刷新用户/项目”后会显示项目列表。</p>;
  }
  return <div className="vmm-row-list">{projects.map((project, index) => <VmmDataRow key={index} title={String(project.displayPath ?? project.display_path ?? "未命名项目")} detail={`ID：${String(project.projectId ?? project.project_id ?? "-")}`} />)}</div>;
}

// VmmProfileRows renders active profile nodes from a VMM result.
// VmmProfileRows 从 VMM 结果渲染 active 画像节点。
function VmmProfileRows(props: { result: VmmMemoryActionResult | null }) {
  const nodes = vmmResultArray(props.result, "nodes");
  if (nodes.length === 0) {
    return <p className="vmm-muted">刷新画像节点后会在这里显示 active profile nodes。</p>;
  }
  return <div className="vmm-row-list">{nodes.map((node, index) => <VmmDataRow key={index} title={`#${String(node.profileNodeId ?? node.profile_node_id ?? index + 1)}`} detail={String(node.content ?? "")} meta={`${String(node.priority ?? "-")} · ${String(node.level ?? "-")} · bind=${String(node.bindId ?? node.bind_id ?? "-")}`} />)}</div>;
}

// VmmBundlePreview renders profile bundle text from a VMM result.
// VmmBundlePreview 从 VMM 结果渲染画像 bundle 文本。
function VmmBundlePreview(props: { result: VmmMemoryActionResult | null }) {
  const response = vmmObject(props.result?.response);
  const parts = [
    ["完整画像", response.combinedText ?? response.combined_text],
    ["团队画像", response.teamProfile ?? response.team_profile],
    ["空间画像", response.spaceProfile ?? response.space_profile],
    ["项目画像", response.projectProfile ?? response.project_profile],
    ["用户画像", response.userProfile ?? response.user_profile],
  ]
    .filter((entry): entry is [string, string] => typeof entry[1] === "string" && entry[1].trim().length > 0);
  if (parts.length === 0) {
    return <p className="vmm-muted">读取完整画像后会显示 bundle 内容。</p>;
  }
  return <div className="vmm-bundle-preview">{parts.map(([label, value]) => <section key={label}><strong>{label}</strong><pre>{String(value)}</pre></section>)}</div>;
}

// VmmToolResult renders compact tool-debug results.
// VmmToolResult 渲染紧凑的工具调试结果。
function VmmToolResult(props: { result: VmmMemoryActionResult | null }) {
  if (!props.result) {
    return <p className="vmm-muted">工具调试结果会按动作显示。</p>;
  }
  if (props.result.kind === "search_memory_events") {
    const groups = vmmResultArray(props.result, "results");
    return <div className="vmm-row-list">{groups.flatMap((group, groupIndex) => vmmArray(group.hits).map((hit, hitIndex) => <VmmDataRow key={`${groupIndex}-${hitIndex}`} title={String(group.query ?? "query")} detail={String(hit.abstractText ?? hit.abstract_text ?? "")} meta={`memory=${String(hit.memoryId ?? hit.memory_id ?? "-")} · turn=${String(hit.sourceTurnId ?? hit.source_turn_id ?? "-")}`} />))}</div>;
  }
  if (props.result.kind === "get_turn_details") {
    const turns = vmmResultArray(props.result, "turns");
    return <div className="vmm-row-list">{turns.map((turn, index) => <VmmDataRow key={index} title={`turn=${String(turn.turnId ?? turn.turn_id ?? "-")}`} detail={String(turn.userQuestion ?? turn.user_question ?? "")} meta={String(turn.assistantAnswer ?? turn.assistant_answer ?? "")} />)}</div>;
  }
  if (props.result.kind === "write_memories") {
    const items = vmmResultArray(props.result, "items");
    return <div className="vmm-row-list">{items.map((item, index) => <VmmDataRow key={index} title={`memory=${String(item.memoryId ?? item.memory_id ?? "-")}`} detail={`deduped=${String(item.deduped ?? false)}`} />)}</div>;
  }
  return <VmmJsonPreview title="结果" value={props.result.response} />;
}

// VmmDataRow renders one compact result row.
// VmmDataRow 渲染单条紧凑结果行。
function VmmDataRow(props: { title: string; detail: string; meta?: string }) {
  return (
    <article className="vmm-data-row">
      <strong>{props.title}</strong>
      <span>{props.detail}</span>
      {props.meta ? <small>{props.meta}</small> : null}
    </article>
  );
}

// VmmJsonPreview renders one JSON preview block.
// VmmJsonPreview 渲染单个 JSON 预览块。
function VmmJsonPreview(props: { title: string; value: unknown }) {
  return (
    <div className="vmm-json-preview">
      <strong>{props.title}</strong>
      <pre>{JSON.stringify(props.value, null, 2)}</pre>
    </div>
  );
}

// vmmDiagnosticLabel returns a user-facing diagnostic label.
// vmmDiagnosticLabel 返回面向用户的诊断标签。
function vmmDiagnosticLabel(value: string): string {
  const labels: Record<string, string> = {
    ready: "ready",
    disabled_by_config: "已关闭",
    missing_grpc_target: "缺少 gRPC",
    grpc_offline: "gRPC 离线",
    missing_user_id: "缺少用户",
    missing_project_id: "缺少项目",
  };
  return labels[value] ?? value;
}

// vmmResultArray reads an array from the latest VMM result response.
// vmmResultArray 从最近 VMM 结果响应中读取数组。
function vmmResultArray(result: VmmMemoryActionResult | null, key: string): Record<string, unknown>[] {
  return vmmArray(vmmObject(result?.response)[key]);
}

// vmmArray normalizes unknown values into object arrays.
// vmmArray 将未知值规范化为对象数组。
function vmmArray(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.filter((item): item is Record<string, unknown> => typeof item === "object" && item !== null) : [];
}

// vmmObject normalizes unknown values into an object.
// vmmObject 将未知值规范化为对象。
function vmmObject(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null ? value as Record<string, unknown> : {};
}

// SettingsArea renders settings sections.
// SettingsArea 渲染设置分组。
function SettingsArea(props: {
  // Settings sections shown in general settings.
  // 通用设置中展示的设置分组。
  settings: SettingSectionSummary[]
}) {
  return (
    <section className="panel full-height">
      <PanelTitle icon={<Settings size={17} />} title="设置" />
      <div className="settings-grid">
        {props.settings.map((section) => (
          <article className="settings-card" key={section.id}>
            <strong>{section.label}</strong>
            <p>{section.description}</p>
            <div className="chip-row">
              {section.items.map((item) => (
                <span className="chip" key={item}>{item}</span>
              ))}
            </div>
          </article>
        ))}
      </div>
    </section>
  )
}

// LogsArea renders startup and diagnostic logs.
// LogsArea 渲染启动与诊断日志。
function LogsArea(props: {
  // Startup snapshot shown in diagnostics.
  // 诊断中展示的启动快照。
  startup: StartupSnapshot
  // Diagnostic log entries shown beside startup.
  // 启动状态旁展示的诊断日志。
  logs: LogEntrySummary[]
}) {
  return (
    <div className="area-grid two-columns">
      <StartupPanel startup={props.startup} />
      <section className="panel">
        <PanelTitle icon={<History size={17} />} title="诊断日志" />
        <div className="log-list">
          {props.logs.map((log) => (
            <article className="log-row" key={log.id}>
              <StatusPill status={log.level === "info" ? "ready" : "warning"} label={log.level} />
              <strong>{log.source}</strong>
              <span>{log.message}</span>
            </article>
          ))}
        </div>
      </section>
    </div>
  )
}

// StartupPanel renders service startup progress.
// StartupPanel 渲染服务启动进度。
function StartupPanel(props: {
  // Startup snapshot displayed as step rows.
  // 以步骤行展示的启动快照。
  startup: StartupSnapshot
}) {
  return (
    <section className="panel startup-panel">
      <PanelTitle icon={<PlugZap size={17} />} title="启动状态" />
      <div className="step-list">
        {props.startup.steps.map((step) => (
          <div className="step-item" key={step.id}>
            <StatusIcon status={step.status} />
            <div>
              <strong>{step.label}</strong>
              <span>{step.message ?? "等待状态更新"}</span>
            </div>
          </div>
        ))}
      </div>
      <div className="log-preview">
        {props.startup.recentLogs.map((line) => (
          <code key={line}>{line}</code>
        ))}
      </div>
    </section>
  )
}

// RuntimeModulesPanel renders module status cards.
// RuntimeModulesPanel 渲染模块状态卡片。
function RuntimeModulesPanel(props: {
  // Runtime modules shown as cards.
  // 以卡片展示的运行时模块。
  modules: RuntimeModuleSnapshot[]
}) {
  return (
    <section className="panel modules-panel">
      <PanelTitle icon={<Database size={17} />} title="运行时模块" />
      <div className="module-grid">
        {props.modules.map((module) => (
          <article className="module-card" key={module.id}>
            <StatusPill status={module.status} label={module.status} />
            <strong>{module.label}</strong>
            <span>{module.message}</span>
          </article>
        ))}
      </div>
    </section>
  )
}

// ToolPanel renders tool call blocks.
// ToolPanel 渲染工具调用块。
function ToolPanel(props: {
  // Tool call summaries shown as cards.
  // 以卡片展示的工具调用摘要。
  toolCalls: ToolCallSummary[]
}) {
  return (
    <section className="panel tool-panel">
      <PanelTitle icon={<Hammer size={17} />} title="工具调用" />
      <div className="tool-list">
        {props.toolCalls.map((toolCall) => (
          <article className="tool-card" key={toolCall.id}>
            <div>
              <TerminalSquare size={16} />
              <strong>{toolCall.name}</strong>
            </div>
            <StatusPill status={toolCall.status === "ready" ? "ready" : "warning"} label={toolCall.status} />
            <p>{toolCall.detail}</p>
          </article>
        ))}
      </div>
    </section>
  )
}

// CommandSessionsPanel renders command session summaries.
// CommandSessionsPanel 渲染命令会话摘要。
function CommandSessionsPanel(props: {
  // Command session summaries shown as cards.
  // 以卡片展示的命令会话摘要。
  commandSessions: CommandSessionSummary[]
}) {
  return (
    <section className="panel">
      <PanelTitle icon={<TerminalSquare size={17} />} title="命令会话" />
      <div className="tool-list">
        {props.commandSessions.map((session) => (
          <article className="tool-card" key={session.id}>
            <div>
              <TerminalSquare size={16} />
              <strong>{session.command}</strong>
            </div>
            <StatusPill status={session.status === "passed" ? "ready" : "warning"} label={session.status} />
            <p>{session.preview}</p>
          </article>
        ))}
      </div>
    </section>
  )
}

// areaIconMap maps feature ids to lucide icons.
// areaIconMap 将功能区域标识映射到 lucide 图标。
const areaIconMap: Record<MainAreaId, React.ReactNode> = {
  chat: <PanelTopOpen size={17} />,
  skills: <Hammer size={17} />,
  automation: <GitBranch size={17} />,
  plugins: <Wrench size={17} />,
  memory: <BrainCircuit size={17} />,
  settings: <Settings size={17} />,
}

// primaryFeatureAreas returns localized top navigation feature metadata.
// primaryFeatureAreas 返回本地化顶部主导航功能元数据。
function primaryFeatureAreas(t: ReturnType<typeof useI18n>["t"]): FeatureAreaSummary[] {
  return [
    { id: "chat", label: t("area.chat"), description: t("area.chat.description"), status: t("feature.status.available") },
    { id: "skills", label: t("area.skills"), description: t("area.skills.description"), status: t("feature.status.preview") },
    { id: "automation", label: t("area.automation"), description: t("area.automation.description"), status: t("feature.status.planned") },
    { id: "plugins", label: t("area.plugins"), description: t("area.plugins.description"), status: t("feature.status.preview") },
    { id: "memory", label: t("area.memory"), description: t("area.memory.description"), status: t("feature.status.preview") },
  ]
}

// SettingsSubAreaEntry describes one leaf row in the settings sidebar tree.
// SettingsSubAreaEntry 描述设置侧栏树中的单个叶子行。
interface SettingsSubAreaEntry {
  // Stable settings page identifier.
  // 稳定设置页面标识。
  id: SettingsSubAreaId
  // Localized settings page label.
  // 本地化设置页面标签。
  label: string
  // Icon shown before the page label.
  // 显示在页面标签前的图标。
  icon: React.ReactNode
  // Localized settings page description.
  // 本地化设置页面说明。
  description: string
}

// SettingsSubAreaGroup describes one parent group in the settings sidebar tree.
// SettingsSubAreaGroup 描述设置侧栏树中的单个父级分组。
interface SettingsSubAreaGroup {
  // Stable group identifier.
  // 稳定分组标识。
  id: string
  // Localized group label.
  // 本地化分组标签。
  label: string
  // Group icon.
  // 分组图标。
  icon: React.ReactNode
  // Leaf settings pages in the group.
  // 分组下的叶子设置页面。
  items: SettingsSubAreaEntry[]
}

// settingsSubAreas returns localized settings sidebar leaf entries.
// settingsSubAreas 返回本地化设置侧栏叶子入口。
function settingsSubAreas(t: ReturnType<typeof useI18n>["t"]): SettingsSubAreaEntry[] {
  return settingsSubAreaGroups(t).flatMap((group) => group.items)
}

// settingsSubAreaGroups returns the settings tree aligned with the iced settings navigation.
// settingsSubAreaGroups 返回与 iced 设置导航对齐的设置树。
function settingsSubAreaGroups(t: ReturnType<typeof useI18n>["t"]): SettingsSubAreaGroup[] {
  return [
    {
      id: "models",
      label: t("settings.group.models"),
      icon: <Bot size={16} />,
      items: [
        { id: "model-providers", label: t("settings.modelProviders"), icon: <FileText size={16} />, description: t("settings.modelProviders.description") },
        { id: "model-list", label: t("settings.modelList"), icon: <FileText size={16} />, description: t("settings.modelList.description") },
      ],
    },
    {
      id: "ai-capabilities",
      label: t("settings.group.aiCapabilities"),
      icon: <BrainCircuit size={16} />,
      items: [
        { id: "ai-vendors", label: t("settings.aiVendors"), icon: <FileText size={16} />, description: t("settings.aiVendors.description") },
        { id: "ai-web-search", label: t("settings.aiWebSearch"), icon: <Search size={16} />, description: t("settings.aiWebSearch.description") },
        { id: "ai-understand-image", label: t("settings.aiUnderstandImage"), icon: <Image size={16} />, description: t("settings.aiUnderstandImage.description") },
        { id: "ai-understand-video", label: t("settings.aiUnderstandVideo"), icon: <Video size={16} />, description: t("settings.aiUnderstandVideo.description") },
        { id: "ai-understand-audio", label: t("settings.aiUnderstandAudio"), icon: <Volume2 size={16} />, description: t("settings.aiUnderstandAudio.description") },
        { id: "ai-image-generation", label: t("settings.aiImageGeneration"), icon: <Image size={16} />, description: t("settings.aiImageGeneration.description") },
        { id: "ai-video-generation", label: t("settings.aiVideoGeneration"), icon: <Video size={16} />, description: t("settings.aiVideoGeneration.description") },
        { id: "ai-speech-synthesis", label: t("settings.aiSpeechSynthesis"), icon: <Volume2 size={16} />, description: t("settings.aiSpeechSynthesis.description") },
        { id: "ai-music-generation", label: t("settings.aiMusicGeneration"), icon: <Music size={16} />, description: t("settings.aiMusicGeneration.description") },
        { id: "ai-music-cover", label: t("settings.aiMusicCover"), icon: <RotateCw size={16} />, description: t("settings.aiMusicCover.description") },
      ],
    },
    {
      id: "system",
      label: t("area.settings"),
      icon: <Settings size={16} />,
      items: [
        { id: "mcp-servers", label: t("settings.mcpServers"), icon: <FolderTree size={16} />, description: t("settings.mcpServers.description") },
        { id: "runtime-diagnostics", label: t("settings.runtimeDiagnostics"), icon: <RefreshCw size={16} />, description: t("settings.runtimeDiagnostics.description") },
        { id: "prompt-composer", label: t("settings.promptComposer"), icon: <FileText size={16} />, description: t("settings.promptComposer.description") },
        { id: "about-licenses", label: t("settings.aboutLicenses"), icon: <ScrollText size={16} />, description: t("settings.aboutLicenses.description") },
        { id: "archived-restore", label: t("settings.archivedRestore"), icon: <Archive size={16} />, description: t("settings.archivedRestore.description") },
      ],
    },
  ]
}

// featureSidebarRows builds left sidebar rows for the current non-settings feature.
// featureSidebarRows 为当前非设置功能构建左侧栏条目。
function featureSidebarRows(activeArea: MainAreaId, workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): Array<{ id: string; label: string; detail: string; icon: React.ReactNode }> {
  if (activeArea === "skills") {
    return [
      { id: "skills-lua", label: "Lua Skills", detail: formatSkillText(t("skills.sidebar.luaDetail"), { count: workbench.luaSkills.skills.length }), icon: <Hammer size={16} /> },
      { id: "skills-agent", label: "Agent Skills", detail: formatSkillText(t("skills.sidebar.agentDetail"), { count: workbench.agentSkills.skills.length }), icon: <Bot size={16} /> },
    ]
  }
  if (activeArea === "automation") {
    return [
      { id: "automation-workflow", label: "工作流", detail: "等待运行时接入", icon: <GitBranch size={16} /> },
      { id: "automation-sessions", label: "后台任务", detail: `${workbench.commandSessions.length} 条命令会话`, icon: <TerminalSquare size={16} /> },
    ]
  }
  if (activeArea === "plugins") {
    return [
      { id: "plugins-installed", label: "已安装插件", detail: `${workbench.plugins.length} 个插件`, icon: <PlugZap size={16} /> },
      { id: "plugins-webui", label: "WebUI 面板", detail: "高级插件控制入口", icon: <PanelTopOpen size={16} /> },
    ]
  }
  if (activeArea === "memory") {
    return [
      { id: "status", label: "状态", detail: workbench.vmmMemory.notice, icon: <BrainCircuit size={16} /> },
      ...workbench.vmmMemory.sections.map((section) => ({
        id: section.id,
        label: section.label,
        detail: section.description,
        icon: vmmSectionIcon(section.id),
      })),
    ]
  }
  return []
}

// vmmSectionIcon maps VMM section ids to sidebar icons.
// vmmSectionIcon 将 VMM 分区标识映射到侧栏图标。
function vmmSectionIcon(sectionId: string): React.ReactNode {
  if (sectionId === "user_manager") {
    return <User size={16} />
  }
  if (sectionId === "project_manager") {
    return <FolderTree size={16} />
  }
  if (sectionId === "profile_center") {
    return <BrainCircuit size={16} />
  }
  if (sectionId === "profile_bundle") {
    return <ScrollText size={16} />
  }
  if (sectionId === "tools_debug") {
    return <TerminalSquare size={16} />
  }
  if (sectionId === "memory_settings") {
    return <SlidersHorizontal size={16} />
  }
  if (sectionId === "grpc_transport") {
    return <PlugZap size={16} />
  }
  return <BrainCircuit size={16} />
}

// skillSidebarRowToSubArea maps a skills sidebar row id to its content page id.
// skillSidebarRowToSubArea 将技能侧栏行标识映射到对应内容页面标识。
function skillSidebarRowToSubArea(rowId: string): SkillSubAreaId | null {
  if (rowId === "skills-lua") {
    return "lua"
  }
  if (rowId === "skills-agent") {
    return "agent"
  }
  return null
}

export { FeatureShell, areaIconMap, primaryFeatureAreas, settingsSubAreas }
