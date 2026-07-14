import * as React from "react"
import {
  BrainCircuit,
  ChartNoAxesCombined,
  ChevronDown,
  Hammer,
  Network,
  PanelTopOpen,
  Puzzle,
  Route,
  Undo2,
  Settings,
} from "lucide-react"

import { formatSkillText } from "@/components/features/featureText"
import { vmmSectionIcon, type MemorySubAreaId } from "@/components/features/memory/MemoryArea"
import { type SkillSubAreaId } from "@/components/features/skills/SkillsArea"
import {
  enabledPluginDetailMounts,
  localizedPluginCapability,
  localizedPluginMountLabel,
} from "@/components/features/skills/skill-area-types"
import {
  settingsSubAreaGroups,
  type SettingsSubAreaGroup,
} from "@/components/features/settings/SettingsFeatureContent"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar"
import {
  SidebarBrandMenu,
  SidebarUserMenu,
} from "@/components/sidebar/sidebar-profile-menus"
import { useI18n } from "@/i18n"
import type {
  FeatureAreaSummary,
  MainAreaId,
  SettingsSubAreaId,
  WorkbenchSnapshot,
} from "@/types"

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
  // Selects one top-level application area from the sidebar brand menu.
  // 从侧栏品牌菜单选择一个顶层应用区域。
  onSelectArea: (area: MainAreaId) => void
  // Whether the sidebar brand content should be a native window drag region.
  // 侧栏品牌内容是否作为原生窗口拖动区域。
  titlebarDragRegion?: boolean
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void
  // Workbench snapshot used for sidebar counters.
  // 侧栏计数使用的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { t } = useI18n()
  const feature = primaryFeatureAreas(t).find((area) => area.id === props.activeArea)
  const headerLabel = props.activeArea === "settings" ? t("settings.systemSettings") : feature?.label ?? t("sidebar.console")

  return (
    <Sidebar collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarBrandMenu
          activeArea={props.activeArea}
          currentLabel={headerLabel}
          onSelectArea={props.onSelectArea}
          titlebarDragRegion={props.titlebarDragRegion}
          onTitlebarDoubleClick={props.onTitlebarDoubleClick}
        />
      </SidebarHeader>
      <SidebarContent>
        {props.activeArea === "settings" ? (
          <SettingsSidebarMenu
            activeSettingsSubArea={props.activeSettingsSubArea}
            onBackToWorkspace={() => props.onSelectArea("chat")}
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
      <SidebarFooter>
        <SidebarUserMenu onOpenSettings={() => props.onSelectArea("settings")} />
      </SidebarFooter>
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
  const { locale, t } = useI18n()
  // rows contains static host pages and manifest-generated plugin pages.
  // rows 包含宿主静态页面与清单生成的插件页面。
  const rows = featureSidebarRows(props.activeArea, props.workbench, locale, t)

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
  // Returns from settings to the main workspace.
  // 从设置页返回主工作区。
  onBackToWorkspace: () => void
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
        <SidebarMenuItem>
          <SidebarMenuButton
            className="workspace-sidebar-feature-button settings-back-menu-row"
            onClick={props.onBackToWorkspace}
            tooltip={t("settings.backToWorkspace")}
          >
            <Undo2 size={16} />
            <span className="vcn-sidebar-label workspace-sidebar-feature-label">{t("settings.backToWorkspace")}</span>
          </SidebarMenuButton>
        </SidebarMenuItem>
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
  // isSidebarCollapsed selects hover flyout behavior without replacing the row structure.
  // isSidebarCollapsed 在不替换行结构的情况下选择悬浮菜单行为。
  const isSidebarCollapsed = props.sidebarState === "collapsed"
  // settingsFolderButton keeps one stable category trigger for expanded and collapsed animation.
  // settingsFolderButton 为展开与折叠动画保留同一个稳定的目录触发入口。
  const settingsFolderButton = (
    <SidebarMenuButton
      aria-expanded={isSidebarCollapsed ? undefined : "true"}
      aria-label={isSidebarCollapsed ? props.group.label : undefined}
      className={`project-folder-button settings-folder-button ${isSidebarCollapsed ? "collapsed-project-button settings-collapsed-button" : ""}`}
      onClick={(event) => {
        if (isSidebarCollapsed) {
          event.preventDefault()
          return
        }
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
  )

  return (
    <SidebarMenuItem className="project-tree-item settings-tree-item">
      <div className="project-folder-row settings-folder-row">
        {isSidebarCollapsed ? (
          <CollapsedSettingsGroupMenu
            activeSettingsSubArea={props.activeSettingsSubArea}
            group={props.group}
            onSelectSettingsSubArea={props.onSelectSettingsSubArea}
          >
            {settingsFolderButton}
          </CollapsedSettingsGroupMenu>
        ) : settingsFolderButton}
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
    </SidebarMenuItem>
  )
}

// CollapsedSettingsGroupMenu renders one settings category hover menu while the sidebar is collapsed.
// CollapsedSettingsGroupMenu 在侧栏折叠时渲染单个设置分类的悬停菜单。
function CollapsedSettingsGroupMenu(props: {
  // Trigger row reused from the expanded settings category.
  // 复用展开设置目录入口的触发行。
  children: React.ReactNode
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId
  // Settings category group.
  // 设置分类分组。
  group: SettingsSubAreaGroup
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
        {props.children}
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

// VmmScope identifies the active VMM configuration scope.
// VmmScope 标识当前 VMM 配置作用域。


// areaIconMap maps feature ids to lucide icons.
// areaIconMap 将功能区域标识映射到 lucide 图标。
const areaIconMap: Record<MainAreaId, React.ReactNode> = {
  chat: <PanelTopOpen size={17} />,
  "agent-skills": <Hammer size={17} />,
  skills: <Puzzle size={17} />,
  agents: <Network size={17} />,
  memory: <BrainCircuit size={17} />,
  settings: <Settings size={17} />,
}

// primaryFeatureAreas returns localized top navigation feature metadata.
// primaryFeatureAreas 返回本地化顶部主导航功能元数据。
function primaryFeatureAreas(t: ReturnType<typeof useI18n>["t"]): FeatureAreaSummary[] {
  return [
    { id: "chat", label: t("area.chat"), description: t("area.chat.description"), status: t("feature.status.available") },
    { id: "agent-skills", label: t("area.agentSkills"), description: t("area.agentSkills.description"), status: t("feature.status.available") },
    { id: "skills", label: t("area.skills"), description: t("area.skills.description"), status: t("feature.status.preview") },
    { id: "agents", label: t("area.agents"), description: t("area.agents.description"), status: t("feature.status.preview") },
    { id: "memory", label: t("area.memory"), description: t("area.memory.description"), status: t("feature.status.preview") },
  ]
}

// SettingsSubAreaEntry describes one leaf row in the settings sidebar tree.
// SettingsSubAreaEntry 描述设置侧栏树中的单个叶子行。

// featureSidebarRows builds left sidebar rows for the current non-settings feature.
// featureSidebarRows 为当前非设置功能构建左侧栏条目。
function featureSidebarRows(
  activeArea: MainAreaId,
  workbench: WorkbenchSnapshot,
  locale: string,
  t: ReturnType<typeof useI18n>["t"],
): Array<{ id: string; label: string; detail: string; icon: React.ReactNode }> {
  if (activeArea === "skills") {
    // pluginRows projects every enabled detail mount into one independent navigation row.
    // pluginRows 将每个已启用详情挂载点投影为独立导航行。
    const pluginRows = enabledPluginDetailMounts(workbench.plugins).map((selection) => ({
      id: selection.subAreaId,
      label: localizedPluginMountLabel(selection, locale),
      detail: localizedPluginCapability(selection.plugin, locale),
      icon: pluginDetailMountIcon(selection.mount.icon),
    }))
    return [
      { id: "luaskills-catalog", label: t("skills.navigation.list"), detail: formatSkillText(t("skills.sidebar.luaDetail"), { count: workbench.luaSkills.skills.length }), icon: <Hammer size={16} /> },
      { id: "luaskills-settings", label: t("skills.navigation.luaSettings"), detail: workbench.luaSkills.notice, icon: <Settings size={16} /> },
      { id: "luaskills-plugins", label: t("skills.navigation.plugins"), detail: formatSkillText(t("skills.sidebar.pluginDetail"), { count: workbench.plugins.filter((plugin) => plugin.managementKind === "systemPlugin").length }), icon: <Puzzle size={16} /> },
      ...pluginRows,
    ]
  }
  if (activeArea === "agent-skills") {
    const effectiveRecords = workbench.agentSkills.records.filter((record) => record.effective)
    return [
      { id: "agent-skills-catalog", label: t("area.agentSkills"), detail: formatSkillText(t("skills.sidebar.agentDetail"), { count: effectiveRecords.length }), icon: <Hammer size={16} /> },
    ]
  }
  if (activeArea === "agents") {
    return [
      { id: "agents-catalog", label: t("area.agents"), detail: formatSkillText(t("agents.header.detail"), { count: workbench.subagentCatalog.definitions.length, total: workbench.subagentCatalog.definitions.length, enabled: workbench.subagentCatalog.definitions.filter((definition) => definition.enabled).length, warnings: workbench.subagentCatalog.errors.length }), icon: <Network size={16} /> },
    ]
  }
  if (activeArea === "memory") {
    return [
      { id: "status", label: t("workspaceToolPanel.status"), detail: workbench.vmmMemory.notice, icon: <BrainCircuit size={16} /> },
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

// pluginDetailMountIcon returns the host-owned icon declared by one plugin mount.
// pluginDetailMountIcon 返回单个插件挂载点声明的宿主持有图标。
//
// Parameters: icon is the validated service projection identifier.
// 参数：icon 是已经校验的服务投影标识。
// Returns: the corresponding Lucide icon node.
// 返回值：返回对应的 Lucide 图标节点。
function pluginDetailMountIcon(icon: "plugin" | "route" | "chart"): React.ReactNode {
  if (icon === "route") {
    return <Route size={16} />
  }
  if (icon === "chart") {
    return <ChartNoAxesCombined size={16} />
  }
  return <Puzzle size={16} />
}


// skillSidebarRowToSubArea maps a skills sidebar row id to its content page id.
// skillSidebarRowToSubArea 将技能侧栏行标识映射到对应内容页面标识。
function skillSidebarRowToSubArea(rowId: string): SkillSubAreaId | null {
  if (rowId === "luaskills-catalog") {
    return "catalog"
  }
  if (rowId === "luaskills-settings") {
    return "settings"
  }
  if (rowId === "luaskills-plugins") {
    return "plugins"
  }
  if (rowId.startsWith("plugin:")) {
    return rowId as SkillSubAreaId
  }
  return null
}
export { FeatureSidebar, areaIconMap, primaryFeatureAreas }
