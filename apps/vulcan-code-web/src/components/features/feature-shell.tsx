import * as React from "react"
import {
  Bot,
  Boxes,
  BrainCircuit,
  Database,
  FileText,
  FolderTree,
  GitBranch,
  Hammer,
  History,
  ListRestart,
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
  Wrench,
} from "lucide-react"

import { PanelTitle, StatusIcon, StatusPill } from "@/components/shared/status"
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
} from "@/components/ui/sidebar"
import { useI18n } from "@/i18n"
import type {
  AgentSkillSummary,
  CommandSessionSummary,
  FeatureAreaSummary,
  LuaSkillSummary,
  LuaSkillToolSummary,
  LogEntrySummary,
  MainAreaId,
  MemorySummary,
  PluginSummary,
  ProjectSummary,
  ProviderSummary,
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

// SkillSubAreaId identifies the selected skill management category.
// SkillSubAreaId 标识当前选中的技能管理分类。
type SkillSubAreaId = "lua" | "agent"

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
  // Startup snapshot used by diagnostics.
  // 诊断页面使用的启动快照。
  startup: StartupSnapshot
  // Workbench snapshot used by feature pages.
  // 功能页面使用的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  // activeSkillSubArea keeps the left menu and right skill tabs synchronized.
  // activeSkillSubArea 保持左侧菜单与右侧技能页签同步。
  const [activeSkillSubArea, setActiveSkillSubArea] = React.useState<SkillSubAreaId>("lua")

  return (
    <SidebarProvider className="console-shell feature-shell">
      <FeatureSidebar
        activeArea={props.activeArea}
        activeSkillSubArea={activeSkillSubArea}
        activeSettingsSubArea={props.activeSettingsSubArea}
        onSelectSkillSubArea={setActiveSkillSubArea}
        onSelectSettingsSubArea={props.onSelectSettingsSubArea}
        workbench={props.workbench}
      />
      <SidebarInset>
        <div className="feature-main-shell">
          <FeatureHeader feature={props.activeFeature} />
          <FeatureContent
          activeArea={props.activeArea}
          activeSkillSubArea={activeSkillSubArea}
          activeSettingsSubArea={props.activeSettingsSubArea}
          onSelectSkillSubArea={setActiveSkillSubArea}
          memoryActions={props.memoryActions}
          skillActions={props.skillActions}
          startup={props.startup}
          workbench={props.workbench}
          />
        </div>
      </SidebarInset>
    </SidebarProvider>
  )
}

// FeatureHeader renders the inset title bar for a non-console page.
// FeatureHeader 渲染非控制台页面的内容区标题栏。
function FeatureHeader(props: {
  // Feature metadata displayed in the header.
  // 标题栏中展示的功能元数据。
  feature?: FeatureAreaSummary
}) {
  return (
    <header className="console-header">
      <div className="console-header-main">
        <SidebarTrigger />
        <span className="console-header-separator" aria-hidden="true" />
        <h1>{props.feature?.label ?? "工作台"}</h1>
      </div>
      <span className="feature-header-description">{props.feature?.description}</span>
    </header>
  )
}

// FeatureSidebar renders the left navigation for one non-console feature area.
// FeatureSidebar 渲染单个非控制台功能区的左侧导航。
function FeatureSidebar(props: {
  // Active top-level area.
  // 当前顶层区域。
  activeArea: MainAreaId
  // Selected skill sub page.
  // 当前选中的技能子页面。
  activeSkillSubArea: SkillSubAreaId
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
  // Updates the selected skill sub page.
  // 更新选中的技能子页面。
  onSelectSkillSubArea: (area: SkillSubAreaId) => void
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
            activeSkillSubArea={props.activeSkillSubArea}
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
  // Selected skill sub page.
  // 当前选中的技能子页面。
  activeSkillSubArea: SkillSubAreaId
  // Updates the selected skill sub page.
  // 更新选中的技能子页面。
  onSelectSkillSubArea: (area: SkillSubAreaId) => void
  // Workbench snapshot used for row counters.
  // 行计数使用的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { t } = useI18n()
  const rows = featureSidebarRows(props.activeArea, props.workbench)

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
                tooltip={row.detail}
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
  return (
    <SidebarGroup>
      <SidebarGroupLabel><Settings size={15} /><span>{t("sidebar.settingsCategories")}</span></SidebarGroupLabel>
      <SidebarMenu>
        {settingsSubAreas(t).map((area) => (
          <SidebarMenuItem key={area.id}>
            <SidebarMenuButton
              active={props.activeSettingsSubArea === area.id}
              className="feature-sidebar-row"
              onClick={() => props.onSelectSettingsSubArea(area.id)}
              tooltip={area.description}
            >
              {area.icon}
              <span className="vcn-sidebar-label">
                <strong>{area.label}</strong>
                <small>{area.description}</small>
              </span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        ))}
      </SidebarMenu>
    </SidebarGroup>
  )
}

// FeatureContent renders the selected non-console feature area.
// FeatureContent 渲染当前选中的非控制台功能区域。
function FeatureContent(props: {
  // Active top-level feature area.
  // 当前顶层功能区域。
  activeArea: MainAreaId
  // Selected skill sub page.
  // 当前选中的技能子页面。
  activeSkillSubArea: SkillSubAreaId
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
  // Updates the selected skill sub page.
  // 更新选中的技能子页面。
  onSelectSkillSubArea: (area: SkillSubAreaId) => void
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps
  // Service-backed VMM memory page actions.
  // 服务端支持的 VMM 记忆页面动作。
  memoryActions: MemoryActionProps
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
          onActiveTabChange={props.onSelectSkillSubArea}
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
  // Startup snapshot used by diagnostics.
  // 诊断页面使用的启动快照。
  startup: StartupSnapshot
  // Workbench snapshot used by settings pages.
  // 设置页面使用的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  if (props.activeSettingsSubArea === "projects") {
    return <ProjectsArea projects={props.workbench.projects} modules={props.workbench.modules} />
  }
  if (props.activeSettingsSubArea === "providers") {
    return <ProvidersArea providers={props.workbench.providers} modules={props.workbench.modules} />
  }
  if (props.activeSettingsSubArea === "tools") {
    return <ToolsArea toolCalls={props.workbench.toolCalls} commandSessions={props.workbench.commandSessions} />
  }
  if (props.activeSettingsSubArea === "diagnostics") {
    return <LogsArea startup={props.startup} logs={props.workbench.logs} />
  }
  return <SettingsArea settings={props.workbench.settings} />
}

// SkillsArea renders LuaSkills and tool capability status.
// SkillsArea 渲染 LuaSkills 与工具能力状态。
function SkillsArea(props: {
  // Selected skill tab.
  // 当前选中的技能页签。
  activeTab: SkillSubAreaId
  // Updates the selected skill tab.
  // 更新当前选中的技能页签。
  onActiveTabChange: (area: SkillSubAreaId) => void
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps
  // Workbench snapshot containing Lua Skills and Agent Skills catalogs.
  // 包含 Lua Skills 与 Agent Skills 目录的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  return (
    <section className="panel full-height skill-management-panel">
      <div className="skill-page-header">
        <PanelTitle icon={<Hammer size={17} />} title="技能" />
        <div className="skill-tabs" role="tablist" aria-label="技能分类">
          <button className={props.activeTab === "lua" ? "active" : ""} onClick={() => props.onActiveTabChange("lua")} type="button">
            <Hammer size={15} />
            <span>Lua Skills</span>
          </button>
          <button className={props.activeTab === "agent" ? "active" : ""} onClick={() => props.onActiveTabChange("agent")} type="button">
            <Bot size={15} />
            <span>Agent Skills</span>
          </button>
        </div>
      </div>
      {props.activeTab === "lua" ? (
        <LuaSkillsPanel
          actions={props.skillActions}
          skills={props.workbench.luaSkills.skills}
          entryCount={props.workbench.luaSkills.entryCount}
          notice={props.workbench.luaSkills.notice}
          runtimeRoot={props.workbench.luaSkills.runtimeRoot}
          systemLuaLib={props.workbench.luaSkills.systemLuaLib}
        />
      ) : (
        <AgentSkillsPanel
          actions={props.skillActions}
          errors={props.workbench.agentSkills.errors}
          globalEnabled={props.workbench.agentSkills.globalEnabled}
          notice={props.workbench.agentSkills.notice}
          skills={props.workbench.agentSkills.skills}
          systemRoot={props.workbench.agentSkills.systemRoot}
          userRoot={props.workbench.agentSkills.userRoot}
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
  // Total runtime entry count.
  // 运行时入口总数。
  entryCount: number
  // Catalog notice text.
  // 目录提示文本。
  notice: string
  // Runtime root path.
  // 运行时根路径。
  runtimeRoot: string
  // System Lua library path.
  // 系统 Lua 库路径。
  systemLuaLib: string
}) {
  const [sourceType, setSourceType] = React.useState<"github" | "url">("github")
  const [installSkillId, setInstallSkillId] = React.useState("")
  const [installSource, setInstallSource] = React.useState("")
  const [busyAction, setBusyAction] = React.useState<string | null>(null)
  const enabledCount = props.skills.filter((skill) => skill.enabled).length
  const managedCount = props.skills.filter((skill) => skill.managed).length

  const runAction = React.useCallback(async (label: string, action: () => Promise<void>) => {
    setBusyAction(label)
    try {
      await action()
    } finally {
      setBusyAction(null)
    }
  }, [])

  return (
    <div className="skill-page-body">
      <div className="skill-summary-grid">
        <article className="skill-summary-card">
          <strong>LuaSkills 目录</strong>
          <span>{props.notice}</span>
          <small>可见技能 {props.skills.length} · 已启用 {enabledCount} · 受管 {managedCount} · 入口 {props.entryCount}</small>
        </article>
        <article className="skill-summary-card">
          <strong>运行时路径</strong>
          <span>{props.runtimeRoot || "等待运行时目录"}</span>
          <small>{props.systemLuaLib || "等待 system_lua_lib"}</small>
        </article>
      </div>
      <div className="skill-toolbar">
        <button className="skill-action-button secondary" disabled={busyAction !== null} onClick={() => runAction("refresh", props.actions.onRefreshLuaSkills)} type="button">
          <RefreshCw size={15} />
          <span>刷新</span>
        </button>
        <select value={sourceType} onChange={(event) => setSourceType(event.target.value as "github" | "url")}>
          <option value="github">GitHub</option>
          <option value="url">URL</option>
        </select>
        <input placeholder="可选技能标识，例如 vulcan-codekit" value={installSkillId} onChange={(event) => setInstallSkillId(event.target.value)} />
        <input placeholder={sourceType === "github" ? "LuaSkills/vulcan-codekit" : "https://example.com/skill-source.yaml"} value={installSource} onChange={(event) => setInstallSource(event.target.value)} />
        <button className="skill-action-button" disabled={busyAction !== null || installSource.trim().length === 0} onClick={() => runAction("install", () => props.actions.onInstallLuaSkill(installSkillId, installSource, sourceType))} type="button">
          <PackagePlus size={15} />
          <span>安装</span>
        </button>
      </div>
      <div className="skill-card-list">
        {props.skills.length === 0 ? (
          <article className="skill-empty-card">
            <StatusPill status="pending" label="pending" />
            <strong>等待 LuaSkills 目录</strong>
            <span>启动完成或刷新后会显示真实技能包。</span>
          </article>
        ) : props.skills.map((skill) => (
          <LuaSkillCard
            actions={props.actions}
            busy={busyAction !== null}
            key={skill.skillId}
            onRunAction={runAction}
            skill={skill}
          />
        ))}
      </div>
    </div>
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
  // Runs a labeled async action with page-level busy state.
  // 使用页面级忙碌状态运行带标签的异步动作。
  onRunAction: (label: string, action: () => Promise<void>) => Promise<void>
  // Lua Skill row rendered by this card.
  // 当前卡片渲染的 Lua Skill 行。
  skill: LuaSkillSummary
}) {
  const [toolsOpen, setToolsOpen] = React.useState(false)
  const source = props.skill.sourceLocator ? `${props.skill.sourceType ?? "source"} · ${props.skill.sourceLocator}` : "未记录来源"

  return (
    <article className="skill-card">
      <header className="skill-card-header">
        <div>
          <strong>{props.skill.displayName} <span>v{props.skill.version}</span></strong>
          <small>ID: {props.skill.skillId}</small>
        </div>
        <StatusPill status={props.skill.enabled ? "ready" : "warning"} label={props.skill.enabled ? "已启用" : "已停用"} />
      </header>
      <div className="skill-meta-grid">
        <span>{props.skill.rootName} · {props.skill.managed ? "受管" : "非受管"} · 入口 {props.skill.entryCount}</span>
        <span>{source}</span>
        <span>{props.skill.skillDir}</span>
        {props.skill.disabledReason ? <span>停用原因：{props.skill.disabledReason}</span> : null}
      </div>
      <div className="skill-actions-row">
        <button className="skill-action-button" disabled={props.busy} onClick={() => props.onRunAction("toggle", () => props.actions.onSetLuaSkillEnabled(props.skill.skillId, !props.skill.enabled))} type="button">
          <Power size={15} />
          <span>{props.skill.enabled ? "停用" : "启用"}</span>
        </button>
        {props.skill.managed ? (
          <button className="skill-action-button secondary" disabled={props.busy} onClick={() => props.onRunAction("update", () => props.actions.onUpdateLuaSkill(props.skill.skillId))} type="button">
            <RotateCw size={15} />
            <span>更新</span>
          </button>
        ) : null}
        <button className="skill-action-button secondary" disabled={props.busy || props.skill.systemOwned} onClick={() => props.onRunAction("uninstall", () => props.actions.onUninstallLuaSkill(props.skill.skillId))} type="button">
          <Trash2 size={15} />
          <span>{props.skill.systemOwned ? "ROOT 技能" : "卸载"}</span>
        </button>
        <button className="skill-action-button secondary" onClick={() => setToolsOpen((open) => !open)} type="button">
          <FileText size={15} />
          <span>{toolsOpen ? "收起工具" : `工具 ${props.skill.tools.length}`}</span>
        </button>
      </div>
      {toolsOpen ? (
        <div className="skill-tool-list">
          {props.skill.tools.length === 0 ? (
            <p>当前技能尚未解析到可管理的运行时工具。</p>
          ) : props.skill.tools.map((tool) => (
            <LuaSkillToolRow
              actions={props.actions}
              busy={props.busy}
              key={`${props.skill.skillId}-${tool.localName}`}
              skillId={props.skill.skillId}
              tool={tool}
            />
          ))}
        </div>
      ) : null}
    </article>
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
  const handleRegistrationChange = React.useCallback(async (event: React.ChangeEvent<HTMLSelectElement>) => {
    const previousState = selectedState
    const nextState = event.target.value as LuaSkillToolSummary["registrationState"]
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
  }, [props.actions, props.skillId, props.tool.localName, selectedState])

  return (
    <article className="skill-tool-row">
      <div>
        <strong>{props.tool.localName}</strong>
        <span>{props.tool.canonicalName}</span>
        <p>{props.tool.description || props.tool.registrationHelp}</p>
      </div>
      <div className="skill-tool-control">
        <select
          disabled={props.busy}
          value={selectedState}
          onChange={handleRegistrationChange}
        >
          <option value="explicit">显式启用</option>
          <option value="implicit">隐式启用</option>
          <option value="disabled">禁用</option>
        </select>
        {syncing ? <small className="skill-sync-status">同步中</small> : null}
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
  // Whether Agent Skills are globally enabled.
  // Agent Skills 是否全局启用。
  globalEnabled: boolean
  // Catalog notice text.
  // 目录提示文本。
  notice: string
  // Agent Skill rows.
  // Agent Skill 行。
  skills: AgentSkillSummary[]
  // System skill root path.
  // 系统技能根路径。
  systemRoot: string
  // User skill root path.
  // 用户技能根路径。
  userRoot: string
}) {
  const [busyAction, setBusyAction] = React.useState<string | null>(null)
  const runAction = React.useCallback(async (label: string, action: () => Promise<void>) => {
    setBusyAction(label)
    try {
      await action()
    } finally {
      setBusyAction(null)
    }
  }, [])

  return (
    <div className="skill-page-body">
      <div className="skill-summary-grid">
        <article className="skill-summary-card">
          <strong>Agent Skills</strong>
          <span>{props.notice}</span>
          <small>全局状态：{props.globalEnabled ? "已启用" : "已关闭"} · 技能 {props.skills.length}</small>
        </article>
        <article className="skill-summary-card">
          <strong>技能根目录</strong>
          <span>{props.systemRoot}</span>
          <small>{props.userRoot}</small>
        </article>
      </div>
      <div className="skill-toolbar">
        <button className="skill-action-button secondary" disabled={busyAction !== null} onClick={() => runAction("refresh", props.actions.onRefreshAgentSkills)} type="button">
          <RefreshCw size={15} />
          <span>刷新</span>
        </button>
        <button className="skill-action-button secondary" disabled={busyAction !== null} onClick={() => runAction("sync", props.actions.onInstallSystemAgentSkills)} type="button">
          <PackagePlus size={15} />
          <span>同步系统技能</span>
        </button>
        <button className="skill-action-button" disabled={busyAction !== null} onClick={() => runAction("global", () => props.actions.onSetAgentSkillsEnabled(!props.globalEnabled))} type="button">
          <Power size={15} />
          <span>{props.globalEnabled ? "关闭全局技能" : "启用全局技能"}</span>
        </button>
      </div>
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
      <div className="skill-card-list">
        {props.skills.length === 0 ? (
          <article className="skill-empty-card">
            <StatusPill status="pending" label="pending" />
            <strong>提示词技能目录尚未载入</strong>
            <span>点击刷新或同步系统技能后会扫描系统目录与用户目录下的 SKILL.md。</span>
          </article>
        ) : props.skills.map((skill) => (
          <AgentSkillCard
            actions={props.actions}
            busy={busyAction !== null}
            key={skill.name}
            onRunAction={runAction}
            skill={skill}
          />
        ))}
      </div>
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
  // Runs a labeled async action with page-level busy state.
  // 使用页面级忙碌状态运行带标签的异步动作。
  onRunAction: (label: string, action: () => Promise<void>) => Promise<void>
  // Agent Skill row rendered by this card.
  // 当前卡片渲染的 Agent Skill 行。
  skill: AgentSkillSummary
}) {
  const [detailsOpen, setDetailsOpen] = React.useState(false)
  const fallbackIcon = props.skill.displayName.trim().charAt(0).toUpperCase() || "*"

  return (
    <article className="skill-card">
      <header className="skill-card-header">
        <div className="agent-skill-title">
          <span className="agent-skill-icon">{fallbackIcon}</span>
          <div>
            <strong>{props.skill.displayName}</strong>
            <small>ID: {props.skill.name} · {props.skill.scope}</small>
          </div>
        </div>
        <StatusPill status={props.skill.enabled ? "ready" : "warning"} label={props.skill.enabled ? "已启用" : "已停用"} />
      </header>
      <p>{props.skill.description}</p>
      <div className="skill-meta-grid">
        <span>{props.skill.skillFile}</span>
        <span>{props.skill.baseDir}</span>
      </div>
      <div className="skill-actions-row">
        <button className="skill-action-button" disabled={props.busy} onClick={() => props.onRunAction("agent-toggle", () => props.actions.onSetAgentSkillEnabled(props.skill.name, !props.skill.enabled))} type="button">
          <Power size={15} />
          <span>{props.skill.enabled ? "停用" : "启用"}</span>
        </button>
        <button className="skill-action-button secondary" onClick={() => setDetailsOpen((open) => !open)} type="button">
          <FileText size={15} />
          <span>{detailsOpen ? "收起" : "详情"}</span>
        </button>
      </div>
      {detailsOpen ? <pre className="skill-body-preview">{props.skill.body}</pre> : null}
    </article>
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
  // Provider summaries shown in settings.
  // 设置中展示的 Provider 摘要。
  providers: ProviderSummary[]
  // Runtime module summaries shown beside providers.
  // Provider 旁展示的运行时模块摘要。
  modules: RuntimeModuleSnapshot[]
}) {
  return (
    <div className="area-grid two-columns">
      <section className="panel">
        <PanelTitle icon={<SlidersHorizontal size={17} />} title="模型与 Provider" />
        <div className="card-grid">
          {props.providers.map((provider) => (
            <article className="data-card" key={provider.id}>
              <StatusPill status="ready" label={provider.status} />
              <strong>{provider.name}</strong>
              <span>已启用模型：{provider.enabledModels}</span>
            </article>
          ))}
        </div>
      </section>
      <RuntimeModulesPanel modules={props.modules.filter((module) => module.id === "providers" || module.id === "core")} />
    </div>
  )
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
  const firstSection = props.snapshot.sections[0]?.id ?? "user_manager";
  const [activeSection, setActiveSection] = React.useState(firstSection);
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
    activeSection,
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

  return (
    <section className="panel full-height vmm-memory-page">
      <PanelTitle icon={<BrainCircuit size={17} />} title="VMM 记忆" />
      <div className="vmm-memory-shell">
        <aside className="vmm-memory-rail">
          <div className="vmm-memory-status">
            <StatusPill status={props.snapshot.effective.diagnostic === "ready" ? "ready" : "warning"} label={vmmDiagnosticLabel(props.snapshot.effective.diagnostic)} />
            <strong>{props.snapshot.effective.enabled ? "记忆已启用" : "记忆已关闭"}</strong>
            <span>{props.snapshot.notice}</span>
          </div>
          <div className="vmm-section-list">
            {props.snapshot.sections.map((section) => (
              <button
                className={`vmm-section-button${activeSection === section.id ? " active" : ""}`}
                key={section.id}
                onClick={() => setActiveSection(section.id)}
                type="button"
              >
                <small>{section.key}</small>
                <strong>{section.label}</strong>
                <span>{section.description}</span>
              </button>
            ))}
          </div>
        </aside>
        <main className="vmm-memory-main">
          <VmmEffectiveOverview memories={props.memories} snapshot={props.snapshot} />
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

// settingsSubAreas returns localized settings sidebar entries.
// settingsSubAreas 返回本地化设置侧栏入口。
function settingsSubAreas(t: ReturnType<typeof useI18n>["t"]): Array<{ id: SettingsSubAreaId; label: string; icon: React.ReactNode; description: string }> {
  return [
    { id: "general", label: t("settings.general"), icon: <Settings size={16} />, description: t("settings.general.description") },
    { id: "projects", label: t("settings.projects"), icon: <Boxes size={16} />, description: t("settings.projects.description") },
    { id: "providers", label: t("settings.providers"), icon: <SlidersHorizontal size={16} />, description: t("settings.providers.description") },
    { id: "tools", label: t("settings.tools"), icon: <Hammer size={16} />, description: t("settings.tools.description") },
    { id: "diagnostics", label: t("settings.diagnostics"), icon: <ScrollText size={16} />, description: t("settings.diagnostics.description") },
  ]
}

// featureSidebarRows builds left sidebar rows for the current non-settings feature.
// featureSidebarRows 为当前非设置功能构建左侧栏条目。
function featureSidebarRows(activeArea: MainAreaId, workbench: WorkbenchSnapshot): Array<{ id: string; label: string; detail: string; icon: React.ReactNode }> {
  if (activeArea === "skills") {
    return [
      { id: "skills-lua", label: "Lua Skills", detail: `${workbench.luaSkills.skills.length} 个技能包`, icon: <Hammer size={16} /> },
      { id: "skills-agent", label: "Agent Skills", detail: `${workbench.agentSkills.skills.length} 个提示词技能`, icon: <Bot size={16} /> },
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
      { id: "memory-vmm", label: "VMM 记忆", detail: `${workbench.memories.length} 个入口`, icon: <BrainCircuit size={16} /> },
      { id: "memory-audit", label: "冲突审计", detail: "等待真实节点", icon: <History size={16} /> },
    ]
  }
  return []
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
