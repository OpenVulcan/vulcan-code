import * as React from "react"
import {
  Bot,
  BrainCircuit,
  ChevronDown,
  Hammer,
  PanelTopOpen,
  Undo2,
  Settings,
} from "lucide-react"

import { formatSkillText } from "@/components/features/featureText"
import { vmmSectionIcon, type MemorySubAreaId } from "@/components/features/memory/MemoryArea"
import { type SkillSubAreaId } from "@/components/features/skills/SkillsArea"
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
  // Workbench snapshot used for sidebar counters.
  // 侧栏计数使用的工作台快照。
  workbench: WorkbenchSnapshot
}) {
  const { t } = useI18n()
  const feature = primaryFeatureAreas(t).find((area) => area.id === props.activeArea)
  const headerLabel = props.activeArea === "settings" ? "系统设置" : feature?.label ?? "工作台"

  return (
    <Sidebar collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarBrandMenu
          activeArea={props.activeArea}
          currentLabel={headerLabel}
          onSelectArea={props.onSelectArea}
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
            tooltip="返回工作台"
          >
            <Undo2 size={16} />
            <span className="vcn-sidebar-label workspace-sidebar-feature-label">返回工作台</span>
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
  skills: <Hammer size={17} />,
  memory: <BrainCircuit size={17} />,
  settings: <Settings size={17} />,
}

// primaryFeatureAreas returns localized top navigation feature metadata.
// primaryFeatureAreas 返回本地化顶部主导航功能元数据。
function primaryFeatureAreas(t: ReturnType<typeof useI18n>["t"]): FeatureAreaSummary[] {
  return [
    { id: "chat", label: t("area.chat"), description: t("area.chat.description"), status: t("feature.status.available") },
    { id: "skills", label: t("area.skills"), description: t("area.skills.description"), status: t("feature.status.preview") },
    { id: "memory", label: t("area.memory"), description: t("area.memory.description"), status: t("feature.status.preview") },
  ]
}

// SettingsSubAreaEntry describes one leaf row in the settings sidebar tree.
// SettingsSubAreaEntry 描述设置侧栏树中的单个叶子行。

// featureSidebarRows builds left sidebar rows for the current non-settings feature.
// featureSidebarRows 为当前非设置功能构建左侧栏条目。
function featureSidebarRows(activeArea: MainAreaId, workbench: WorkbenchSnapshot, t: ReturnType<typeof useI18n>["t"]): Array<{ id: string; label: string; detail: string; icon: React.ReactNode }> {
  if (activeArea === "skills") {
    return [
      { id: "skills-lua", label: "Lua Skills", detail: formatSkillText(t("skills.sidebar.luaDetail"), { count: workbench.luaSkills.skills.length }), icon: <Hammer size={16} /> },
      { id: "skills-agent", label: "Agent Skills", detail: formatSkillText(t("skills.sidebar.agentDetail"), { count: workbench.agentSkills.skills.length }), icon: <Bot size={16} /> },
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
export { FeatureSidebar, areaIconMap, primaryFeatureAreas }
