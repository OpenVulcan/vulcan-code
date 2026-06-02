import * as React from "react"
import {
  Boxes,
  BrainCircuit,
  Database,
  GitBranch,
  Hammer,
  History,
  PanelTopOpen,
  PlugZap,
  ScrollText,
  Settings,
  SlidersHorizontal,
  TerminalSquare,
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
  CommandSessionSummary,
  FeatureAreaSummary,
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
  WorkbenchSnapshot,
} from "@/types"

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
  // Startup snapshot used by diagnostics.
  // 诊断页面使用的启动快照。
  startup: StartupSnapshot
  // Workbench snapshot used by feature pages.
  // 功能页面使用的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  return (
    <SidebarProvider className="console-shell feature-shell">
      <FeatureSidebar
        activeArea={props.activeArea}
        activeSettingsSubArea={props.activeSettingsSubArea}
        onSelectSettingsSubArea={props.onSelectSettingsSubArea}
        workbench={props.workbench}
      />
      <SidebarInset>
        <FeatureHeader feature={props.activeFeature} />
        <FeatureContent
          activeArea={props.activeArea}
          activeSettingsSubArea={props.activeSettingsSubArea}
          startup={props.startup}
          workbench={props.workbench}
        />
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
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
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
          <FeatureOverviewMenu activeArea={props.activeArea} workbench={props.workbench} />
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
            <div className="vcn-sidebar-static-item feature-sidebar-row">
              {row.icon}
              <span className="vcn-sidebar-label">
                <strong>{row.label}</strong>
                <small>{row.detail}</small>
              </span>
            </div>
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
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
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
          commandSessions={props.workbench.commandSessions}
          plugins={props.workbench.plugins}
          toolCalls={props.workbench.toolCalls}
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
        <MemoryArea memories={props.workbench.memories} />
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
  // Installed plugin summaries used to identify LuaSkills.
  // 用于识别 LuaSkills 的已安装插件摘要。
  plugins: PluginSummary[]
  // Tool call summaries shown as recent capability activity.
  // 作为近期能力活动展示的工具调用摘要。
  toolCalls: ToolCallSummary[]
  // Command sessions shown as long-running skill activity.
  // 作为长生命周期技能活动展示的命令会话。
  commandSessions: CommandSessionSummary[]
}) {
  const skillPlugins = props.plugins.filter((plugin) => plugin.kind === "LuaSkill")

  return (
    <div className="area-grid two-columns">
      <section className="panel">
        <PanelTitle icon={<Hammer size={17} />} title="技能" />
        <div className="card-grid">
          {skillPlugins.map((plugin) => (
            <article className="data-card" key={plugin.id}>
              <StatusPill status={plugin.status === "已启用" || plugin.status === "预览" ? "ready" : "warning"} label={plugin.status} />
              <strong>{plugin.name}</strong>
              <span>{plugin.kind}</span>
              <p>{plugin.capability}</p>
            </article>
          ))}
          {skillPlugins.length === 0 ? (
            <article className="data-card">
              <StatusPill status="pending" label="pending" />
              <strong>等待 LuaSkills 目录</strong>
              <span>Service 接入后会显示真实技能包</span>
            </article>
          ) : null}
        </div>
      </section>
      <div className="stack compact-stack">
        <ToolPanel toolCalls={props.toolCalls} />
        <CommandSessionsPanel commandSessions={props.commandSessions} />
      </div>
    </div>
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

// MemoryArea renders VMM memory entry points.
// MemoryArea 渲染 VMM 记忆入口。
function MemoryArea(props: {
  // Memory summaries shown in the memory page.
  // 记忆页展示的记忆摘要。
  memories: MemorySummary[]
}) {
  return (
    <section className="panel full-height">
      <PanelTitle icon={<BrainCircuit size={17} />} title="VMM 记忆" />
      <div className="card-grid">
        {props.memories.map((memory) => (
          <article className="data-card" key={memory.id}>
            <StatusPill status="warning" label={memory.category} />
            <strong>{memory.title}</strong>
            <span>{memory.state}</span>
          </article>
        ))}
      </div>
    </section>
  )
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
      { id: "skills-luaskills", label: "LuaSkills", detail: `${workbench.plugins.filter((plugin) => plugin.kind === "LuaSkill").length} 个技能包`, icon: <Hammer size={16} /> },
      { id: "skills-tools", label: "工具能力", detail: `${workbench.toolCalls.length} 条工具记录`, icon: <Wrench size={16} /> },
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

export { FeatureShell, areaIconMap, primaryFeatureAreas, settingsSubAreas }
