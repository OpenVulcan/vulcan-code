import * as React from "react"

import { FeatureContent as FeatureShellContent } from "@/components/features/feature-shell-content"
import { FeatureSidebar, areaIconMap, pluginDetailMountIcon, primaryFeatureAreas } from "@/components/features/feature-shell-sidebar"
import { WorkspaceTitleMenu } from "@/components/workspace/workspace-title-menu"
import { StageHead } from "@/components/shell/stage-head"
import {
  activeSettingsHeaderSummary,
  settingsSubAreas,
} from "@/components/features/settings/settings-navigation"
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types"
import {
  SidebarInset,
  SidebarProvider,
} from "@/ui-kit/shell/sidebar"
import { useI18n } from "@/i18n"
import type {
  FeatureAreaSummary,
  MainAreaId,
  SettingsSubAreaId,
  WorkbenchSnapshot,
} from "@/types"
import type { WorkspacePageDescriptor } from "@/components/workspace/workspace-page-descriptor"

// FeatureShell renders the settings page with the same sidebar and inset structure as the console.
// Plugins, Agents, Agent Skills and memory open inside the workbench (WorkspaceShell) instead.
// FeatureShell 使用与控制台一致的侧栏和内容框结构渲染设置页面。
// 插件、智能体、Agent Skills 与记忆改在工作台（WorkspaceShell）内打开。
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
  // Activity rail rendered beside the feature panel.
  // 渲染在功能面板旁的活动轨道。
  rail: React.ReactNode
  // Opens the exact enabled statistics mount in the embedded workspace plugin area.
  // 在内嵌工作区插件区域打开精确的已启用统计挂载点。
  onOpenUsageStatistics?: () => void
  // Service-backed settings page actions.
  // 服务端支持的设置页面动作。
  settingsActions: SettingsActionProps
  // Local service generation; settings pages that follow service state read it again when it changes.
  // 本地服务代次；跟随服务状态的设置页面在其变化时重新读取。
  serviceInstanceEpoch: number
  // Workbench snapshot used by feature pages.
  // 功能页面使用的工作台快照。
  workbench: WorkbenchSnapshot
  // App-wide chrome actions rendered in the shared content header.
  // 渲染在共享内容标题栏中的应用级窗口控制区。
  chromeActions?: React.ReactNode
  // Status bar pinned to the bottom edge of the page.
  // 固定在页面底部边缘的状态栏。
  footer?: React.ReactNode
  // Whether shared title bar empty space should be a native drag region.
  // 共享标题栏空白区域是否作为原生拖动区域。
  titlebarDragRegion?: boolean
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void
}) {
  const { t } = useI18n()
  // settingsHeaderSummary stores the selected settings section title and description.
  // settingsHeaderSummary 保存当前设置分区标题与说明。
  const settingsHeaderSummary = props.activeArea === "settings"
    ? activeSettingsHeaderSummary(props.activeSettingsSubArea, t)
    : null
  // headerFeature carries the page title and description; settings pages have no in-page heading.
  // headerFeature 承载页面标题与说明；设置页面内不再有独立标题。
  const headerFeature = settingsHeaderSummary
    ? {
      ...(props.activeFeature ?? { id: "settings" as MainAreaId, label: t("area.settings"), description: "", status: "" }),
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
        activeSettingsSubArea={props.activeSettingsSubArea}
        onOpenUsageStatistics={props.onOpenUsageStatistics}
        settingsActions={props.settingsActions}
        serviceInstanceEpoch={props.serviceInstanceEpoch}
        workbench={props.workbench}
      />
    ),
    detail: headerFeature?.description,
    title: headerFeature?.label ?? t("sidebar.console"),
  }

  return (
    <SidebarProvider
      className="feature-shell"
      collapsible={false}
    >
      <FeatureSidebar
        activeArea={props.activeArea}
        activeSettingsSubArea={props.activeSettingsSubArea}
        onSelectSettingsSubArea={props.onSelectSettingsSubArea}
        rail={props.rail}
        titlebarDragRegion={props.titlebarDragRegion}
        onTitlebarDoubleClick={props.onTitlebarDoubleClick}
      />
      <SidebarInset>
        <div className="vk-stage-page">
          <FeatureHeader
            chromeActions={props.chromeActions}
            descriptor={pageDescriptor}
            titlebarDragRegion={props.titlebarDragRegion}
            onTitlebarDoubleClick={props.onTitlebarDoubleClick}
          />
          {pageDescriptor.content}
        </div>
        {props.footer ? <div className="vk-status-frame">{props.footer}</div> : null}
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
    <StageHead
      actions={props.descriptor.headerActions || props.descriptor.headerRightActions ? (
        <>
          {props.descriptor.headerActions}
          {props.descriptor.headerRightActions}
        </>
      ) : null}
      chrome={props.chromeActions}
      detail={props.descriptor.detail}
      onTitlebarDoubleClick={props.onTitlebarDoubleClick}
      title={headerTitle}
      titleMenu={<WorkspaceTitleMenu title={headerTitle} />}
      titlebarDragRegion={props.titlebarDragRegion}
    />
  )
}

export { FeatureShell, areaIconMap, pluginDetailMountIcon, primaryFeatureAreas, settingsSubAreas }
