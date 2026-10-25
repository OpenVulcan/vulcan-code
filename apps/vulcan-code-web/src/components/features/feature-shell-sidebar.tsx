import * as React from "react"
import {
  Bot,
  Globe2,
  ChartNoAxesCombined,
  Hammer,
  PanelTopOpen,
  Puzzle,
  Route,
  Settings,
} from "lucide-react"

import { MemoryBrainIcon } from "@/components/shell/memory-brain-icon"
import {
  normalizeSettingsSubArea,
  settingsSubAreaGroups,
  type SettingsSubAreaGroup,
} from "@/components/features/settings/settings-navigation"
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuItem,
  useSidebarNavigation,
} from "@/ui-kit/shell/sidebar"
import { SidebarBrandMenu } from "@/components/sidebar/sidebar-profile-menus"
import { useI18n } from "@/i18n"
import type {
  FeatureAreaSummary,
  MainAreaId,
  SettingsSubAreaId,
} from "@/types"

// FeatureSidebar renders the left navigation of the settings page.
// FeatureSidebar 渲染设置页面的左侧导航。
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
  // Activity rail rendered beside the panel; it stays visible when the panel collapses.
  // 渲染在面板旁的活动轨道；面板折叠时轨道仍保持可见。
  rail: React.ReactNode
  // Whether the sidebar brand content should be a native window drag region.
  // 侧栏品牌内容是否作为原生窗口拖动区域。
  titlebarDragRegion?: boolean
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void
}) {
  const { t } = useI18n()
  // navigate preserves each destination action and closes phone navigation.
  // navigate 保留各导航动作并关闭手机导航。
  const navigate = useSidebarNavigation();
  const feature = primaryFeatureAreas(t).find((area) => area.id === props.activeArea)
  const headerLabel = props.activeArea === "settings" ? t("settings.systemSettings") : feature?.label ?? t("sidebar.console")

  return (
    <Sidebar aria-label={headerLabel} collapsible="offcanvas" rail={props.rail} variant="inset">
      <SidebarHeader>
        <SidebarBrandMenu
          currentLabel={headerLabel}
          titlebarDragRegion={props.titlebarDragRegion}
          onTitlebarDoubleClick={props.onTitlebarDoubleClick}
        />
      </SidebarHeader>
      <SidebarContent>
        {props.activeArea === "settings" ? (
          <SettingsSidebarMenu
            activeSettingsSubArea={props.activeSettingsSubArea}
            onSelectSettingsSubArea={navigate(props.onSelectSettingsSubArea)}
          />
        ) : null}
      </SidebarContent>
    </Sidebar>
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
  const groups = settingsSubAreaGroups(t)
  return (
    <SidebarGroup className="vk-settings-nav">
      <SidebarMenu>
        {groups.map((group) => (
          <SettingsSidebarGroupRow
            activeSettingsSubArea={props.activeSettingsSubArea}
            group={group}
            key={group.id}
            onSelectSettingsSubArea={props.onSelectSettingsSubArea}
          />
        ))}
      </SidebarMenu>
    </SidebarGroup>
  )
}

// SettingsSidebarGroupRow renders one settings category as a quiet label over its pages. Settings has few pages,
// so every page stays in view: there is nothing to collapse, and each row carries its own icon.
// SettingsSidebarGroupRow 将单个设置分类渲染为一个低调标签及其下的页面。设置页面不多，因此所有页面始终可见：
// 无需折叠，每一行都带有自身图标。
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
}) {
  const labelId = `settings-nav-${props.group.id}`
  const active = normalizeSettingsSubArea(props.activeSettingsSubArea)
  return (
    <SidebarMenuItem className="vk-nav-section" role="group" aria-labelledby={labelId}>
      <div className="vk-nav-group vk-label" id={labelId}>{props.group.label}</div>
      {props.group.items.map((area) => (
        <button
          aria-current={active === area.id ? "page" : undefined}
          className="vk-nav-item"
          key={area.id}
          onClick={() => props.onSelectSettingsSubArea(area.id)}
          title={area.description}
          type="button"
        >
          {area.icon}
          <span>{area.label}</span>
        </button>
      ))}
    </SidebarMenuItem>
  )
}

// areaIconMap maps feature ids to lucide icons.
// areaIconMap 将功能区域标识映射到 lucide 图标。
const areaIconMap: Record<MainAreaId, React.ReactNode> = {
  chat: <PanelTopOpen size={17} />,
  // Retain the legacy icon key for old deep links while browser navigation is plugin-owned.
  // 为旧深链接保留图标键，但浏览器导航已经归属插件。
  browser: <Globe2 size={17} />,
  "agent-skills": <Hammer size={17} />,
  skills: <Puzzle size={17} />,
  agents: <Bot size={17} />,
  memory: <MemoryBrainIcon size={17} />,
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

export { FeatureSidebar, areaIconMap, pluginDetailMountIcon, primaryFeatureAreas }
