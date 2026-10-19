import * as React from "react"
import { toast } from "@/ui-kit/primitives/toast"
import {
  PackagePlus,
  RefreshCw,
} from "lucide-react"

import { FeatureContent as FeatureShellContent } from "@/components/features/feature-shell-content"
import { Button } from "@/ui-kit/primitives/button"
import { FeatureSidebar, areaIconMap, pluginDetailMountIcon, primaryFeatureAreas } from "@/components/features/feature-shell-sidebar"
import { activeAgentsHeaderSummary } from "@/components/features/agents/agent-area-header"
import type { AgentActionProps } from "@/components/features/agents/agent-area-types"
import { WorkspaceTitleMenu } from "@/components/workspace/workspace-title-menu"
import {
  activeSettingsHeaderSummary,
  settingsSubAreas,
} from "@/components/features/settings/settings-navigation"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import { activeSkillHeaderSummary } from "@/components/features/skills/skill-area-header"
import type { SkillActionProps, SkillHeaderBusyActionId, SkillSubAreaId } from "@/components/features/skills/skill-area-types"
import {
  findEnabledPluginDetailMount,
  isPluginDetailSubArea,
} from "@/components/features/skills/skill-area-types"
import type { AgentSkillActionProps } from "@/components/features/agent-skills/agent-skills-types"
import type { MemoryActionProps } from "@/components/features/memory/use-memory-area-controller"
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/ui-kit/shell/sidebar"
import { useI18n } from "@/i18n"
import type {
  FeatureAreaSummary,
  MainAreaId,
  SettingsSubAreaId,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types"
import type { WorkspacePageDescriptor } from "@/components/workspace/workspace-page-descriptor"

// MemorySubAreaId identifies one service-projected VMM section.
// MemorySubAreaId 标识一个由服务投影的 VMM 分区。
type MemorySubAreaId = string

// LazyLuaSkillInstallDialog keeps the installation workflow outside the initial workbench chunk.
// LazyLuaSkillInstallDialog 将安装流程保留在初始工作台分块之外。
const LazyLuaSkillInstallDialog = React.lazy(() => import("@/components/features/skills/lua-skills-panel").then((module) => ({ default: module.LuaSkillInstallDialog })))

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
  // Selects one top-level application area, such as returning from credentials to settings.
  // 选择一个顶层应用区域，例如从凭据入口跳转到设置。
  onSelectArea: (area: MainAreaId) => void
  // Activity rail rendered beside the feature panel.
  // 渲染在功能面板旁的活动轨道。
  rail: React.ReactNode
  // Opens the exact enabled statistics mount in the embedded workspace plugin area.
  // 在内嵌工作区插件区域打开精确的已启用统计挂载点。
  onOpenUsageStatistics?: () => void
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  // Independent LuaSkills lifecycle actions.
  // 独立 LuaSkills 生命周期动作。
  luaSkillActions: SkillActionProps
  // Independent prompt-based Agent Skills actions.
  // 独立提示词 Agent Skills 动作。
  agentSkillActions: AgentSkillActionProps
  // Service-backed independent agent page actions.
  // 服务端支持的独立智能体页面动作。
  agentActions: AgentActionProps
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
  // Selected conversation route used to scope Core browser sessions.
  // 用于限定 Core 浏览器会话范围的当前会话路由。
  selectedConversationId: string | null
  // App-wide chrome actions rendered in the shared content header.
  // 渲染在共享内容标题栏中的应用级窗口控制区。
  chromeActions?: React.ReactNode
  // Whether shared title bar empty space should be a native drag region.
  // 共享标题栏空白区域是否作为原生拖动区域。
  titlebarDragRegion?: boolean
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void
}) {
  const { locale, t } = useI18n()
  // activeSkillSubArea keeps the left menu and right skill tabs synchronized.
  // activeSkillSubArea 保持左侧菜单与右侧技能页签同步。
  const [activeSkillSubArea, setActiveSkillSubArea] = React.useState<SkillSubAreaId>("catalog")
  React.useEffect(() => {
    if (
      isPluginDetailSubArea(activeSkillSubArea)
      && !findEnabledPluginDetailMount(props.workbench.plugins, activeSkillSubArea)
    ) {
      setActiveSkillSubArea("plugins")
    }
  }, [activeSkillSubArea, props.workbench.plugins])
  // activeMemorySubArea keeps VMM left navigation and content synchronized.
  // activeMemorySubArea 保持 VMM 左侧导航与内容区域同步。
  const [activeMemorySubArea, setActiveMemorySubArea] = React.useState<MemorySubAreaId>("content")
  // luaInstallDialogOpen controls the standalone LuaSkills install dialog.
  // luaInstallDialogOpen 控制独立 LuaSkills 安装弹窗。
  const [luaInstallDialogOpen, setLuaInstallDialogOpen] = React.useState(false)
  // skillHeaderBusyAction stores the title-bar action that should show busy feedback.
  // skillHeaderBusyAction 保存需要在标题栏展示忙碌反馈的动作。
  const [skillHeaderBusyAction, setSkillHeaderBusyAction] = React.useState<SkillHeaderBusyActionId | null>(null)
  // skillHeaderActionRunningRef closes the synchronous repeat-click window before React commits busy state.
  // skillHeaderActionRunningRef 在 React 提交忙碌状态前关闭同步重复点击窗口。
  const skillHeaderActionRunningRef = React.useRef(false)
  // runSkillHeaderAction wraps title-bar actions with disabled and spinner feedback.
  // runSkillHeaderAction 为标题栏动作包裹禁用与旋转反馈。
  //
  // Parameters: actionId selects the busy indicator and action performs the registered service request.
  // 参数：actionId 选择忙碌提示，action 执行已注册的服务请求。
  // Returns: a promise that settles after feedback timing and the synchronous guard are released.
  // 返回值：在反馈时长结束且同步守卫释放后结束的 Promise。
  const runSkillHeaderAction = React.useCallback(async (actionId: SkillHeaderBusyActionId, action: () => Promise<void>) => {
    if (skillHeaderActionRunningRef.current) {
      return
    }
    skillHeaderActionRunningRef.current = true
    setSkillHeaderBusyAction(actionId)
    const startedAt = window.performance.now()
    try {
      await action()
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
    } finally {
      const remainingMs = 450 - (window.performance.now() - startedAt)
      if (remainingMs > 0) {
        await new Promise((resolve) => {
          window.setTimeout(resolve, remainingMs)
        })
      }
      skillHeaderActionRunningRef.current = false
      setSkillHeaderBusyAction(null)
    }
  }, [])
  // skillHeaderAction renders the active skill page actions in the shared title bar.
  // skillHeaderAction 在共享标题栏中渲染当前技能页面操作。
  const skillHeaderAction = props.activeArea === "skills" && activeSkillSubArea === "catalog" ? (
    <div className="feature-header-actions">
      <Button
        aria-label={skillHeaderBusyAction === "lua-refresh" ? t("skills.action.refreshing") : t("skills.action.refresh")}
        title={t("skills.action.refresh")}
        className="feature-header-refresh"
        disabled={skillHeaderBusyAction !== null}
        onClick={() => void runSkillHeaderAction("lua-refresh", props.luaSkillActions.onRefreshLuaSkills)}
        size="sm"
        type="button"
        variant="secondary"
      >
        <RefreshCw className={skillHeaderBusyAction === "lua-refresh" ? "spin" : undefined} size={15} />
        <span>{skillHeaderBusyAction === "lua-refresh" ? t("skills.action.refreshing") : t("skills.action.refresh")}</span>
      </Button>
      <Button disabled={skillHeaderBusyAction !== null} onClick={() => setLuaInstallDialogOpen(true)} size="sm" type="button">
        <PackagePlus size={15} />
        <span>{t("skills.action.installLua")}</span>
      </Button>
    </div>
  ) : null
  // agentsHeaderAction stays empty because the agents page owns a sticky action bar.
  // agentsHeaderAction 保持为空，因为智能体页面自行持有锁顶操作栏。
  const agentsHeaderAction = null
  // settingsHeaderAction stays empty because credential creation belongs to each provider section.
  // settingsHeaderAction 保持为空，因为凭据创建属于各供应商区域。
  const settingsHeaderAction = null
  // skillHeaderSummary stores the active skill page title and compact metrics.
  // skillHeaderSummary 保存当前技能页标题与紧凑统计。
  const skillHeaderSummary = props.activeArea === "skills"
    ? activeSkillHeaderSummary(activeSkillSubArea, props.workbench, locale, t)
    : null
  // agentsHeaderSummary stores the independent Agents page title and compact metrics.
  // agentsHeaderSummary 保存独立智能体页面标题与紧凑统计。
  const agentsHeaderSummary = props.activeArea === "agents"
    ? activeAgentsHeaderSummary(props.workbench, t)
    : null
  // settingsHeaderSummary stores the selected settings section title and description.
  // settingsHeaderSummary 保存当前设置分区标题与说明。
  const settingsHeaderSummary = props.activeArea === "settings"
    ? activeSettingsHeaderSummary(props.activeSettingsSubArea, t)
    : null
  // headerFeature overrides the generic feature title while a skill sub page is active.
  // headerFeature 在技能子页面激活时覆盖通用功能标题。
  const headerFeature = agentsHeaderSummary
    ? {
      ...(props.activeFeature ?? { id: "agents" as MainAreaId, label: t("area.agents"), description: "", status: "" }),
      label: agentsHeaderSummary.title,
      description: agentsHeaderSummary.detail,
    }
    : skillHeaderSummary
    ? {
      ...(props.activeFeature ?? { id: "skills" as MainAreaId, label: t("area.skills"), description: "", status: "" }),
      label: skillHeaderSummary.title,
      description: skillHeaderSummary.detail,
    }
    : settingsHeaderSummary
      ? {
        ...(props.activeFeature ?? { id: "settings" as MainAreaId, label: t("area.settings"), description: "", status: "" }),
        // The header carries the page title and description; settings pages have no in-page heading.
        // 标题栏承载页面标题与说明；设置页面内不再有独立标题。
        label: settingsHeaderSummary.title,
        description: settingsHeaderSummary.detail,
      }
    : props.activeFeature
  // pageDescriptor registers feature-page chrome through the shared workspace contract.
  // pageDescriptor 通过共享工作区契约注册功能页外壳信息。
  const pageDescriptor: WorkspacePageDescriptor = {
    content: (
      <FeatureShellContent
        activeArea={props.activeArea}
        activeMemorySubArea={activeMemorySubArea}
        activeSkillSubArea={activeSkillSubArea}
        activeSettingsSubArea={props.activeSettingsSubArea}
        onOpenUsageStatistics={props.onOpenUsageStatistics}
        onOpenDiagnostics={() => {
          props.onSelectSettingsSubArea("runtime-diagnostics")
          props.onSelectArea("settings")
        }}
        memoryActions={props.memoryActions}
        onOpenCredentialManagement={() => {
          props.onSelectSettingsSubArea("credential-management")
          props.onSelectArea("settings")
        }}
        onSelectMemorySubArea={setActiveMemorySubArea}
        settingsActions={props.settingsActions}
        agentActions={props.agentActions}
          luaSkillActions={props.luaSkillActions}
          agentSkillActions={props.agentSkillActions}
        startup={props.startup}
        workbench={props.workbench}
      />
    ),
    detail: headerFeature?.description,
    headerActions: agentsHeaderAction ?? skillHeaderAction ?? settingsHeaderAction,
    title: headerFeature?.label ?? t("sidebar.console"),
  }

  return (
    <SidebarProvider
      className="feature-shell"
      collapsible={false}
    >
      <FeatureSidebar
        activeArea={props.activeArea}
        activeMemorySubArea={activeMemorySubArea}
        activeSkillSubArea={activeSkillSubArea}
        activeSettingsSubArea={props.activeSettingsSubArea}
        onSelectMemorySubArea={setActiveMemorySubArea}
        onSelectSkillSubArea={setActiveSkillSubArea}
        onSelectSettingsSubArea={props.onSelectSettingsSubArea}
        rail={props.rail}
        titlebarDragRegion={props.titlebarDragRegion}
        onTitlebarDoubleClick={props.onTitlebarDoubleClick}
        workbench={props.workbench}
      />
      <SidebarInset>
        <div className="feature-main-shell">
          <FeatureHeader
            chromeActions={props.chromeActions}
            descriptor={pageDescriptor}
            titlebarDragRegion={props.titlebarDragRegion}
            onTitlebarDoubleClick={props.onTitlebarDoubleClick}
          />
          {pageDescriptor.content}
        </div>
        {luaInstallDialogOpen ? (
          <React.Suspense fallback={null}>
            <LazyLuaSkillInstallDialog
              actions={props.luaSkillActions}
              open={luaInstallDialogOpen}
              onOpenChange={setLuaInstallDialogOpen}
            />
          </React.Suspense>
        ) : null}
      </SidebarInset>
    </SidebarProvider>
  )
}

// FeatureHeader renders the inset title bar for a non-console page.
// FeatureHeader 渲染非控制台页面的内容区标题栏。
function FeatureHeader(props: {
  // App-wide chrome actions rendered at the far right of the feature header.
  // 渲染在功能标题栏最右侧的应用级窗口控制区。
  chromeActions?: React.ReactNode
  // Shared page descriptor used to render title and actions.
  // 用于渲染标题与操作区的共享页面描述。
  descriptor: WorkspacePageDescriptor
  // Whether shared title bar empty space should be a native drag region.
  // 共享标题栏空白区域是否作为原生拖动区域。
  titlebarDragRegion?: boolean
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void
}) {
  const { t } = useI18n()
  // headerTitle stores the visible feature page title for display and copying.
  // headerTitle 保存可见功能页标题，用于展示与复制。
  const headerTitle = props.descriptor.title ?? t("sidebar.console")
  return (
    <header className="console-header">
      <div className="console-header-main">
        <SidebarTrigger />
        <span className="console-header-separator" aria-hidden="true" />
        <div
          className="workspace-custom-header-title"
        >
          <div
            className="workspace-header-title-text"
            data-tauri-drag-region={props.titlebarDragRegion ? "" : undefined}
            onDoubleClick={props.titlebarDragRegion ? undefined : props.onTitlebarDoubleClick}
          >
            <h1>{headerTitle}</h1>
            {props.descriptor.detail ? <small>{props.descriptor.detail}</small> : null}
          </div>
          <WorkspaceTitleMenu title={headerTitle} />
        </div>
        <span
          aria-hidden="true"
          className="console-header-drag-fill"
          data-tauri-drag-region={props.titlebarDragRegion ? "" : undefined}
          onDoubleClick={props.titlebarDragRegion ? undefined : props.onTitlebarDoubleClick}
        />
      </div>
      <div className="console-header-actions">
        {props.descriptor.headerActions}
        {props.descriptor.headerRightActions}
        {props.chromeActions}
      </div>
    </header>
  )
}

export { FeatureShell, areaIconMap, pluginDetailMountIcon, primaryFeatureAreas, settingsSubAreas }
