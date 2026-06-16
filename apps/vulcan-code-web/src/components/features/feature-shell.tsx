import * as React from "react"
import {
  PackagePlus,
  RefreshCw,
} from "lucide-react"

import { FeatureContent as FeatureShellContent } from "@/components/features/feature-shell-content"
import { FeatureSidebar, areaIconMap, primaryFeatureAreas } from "@/components/features/feature-shell-sidebar"
import {
  activeSettingsHeaderSummary,
  localizedLiteral,
  settingsSubAreaCapabilityKind,
  settingsSubAreas,
  type SettingsActionProps,
} from "@/components/features/settings/SettingsFeatureContent"
import {
  LuaSkillInstallDialog,
  activeSkillHeaderSummary,
  type SkillActionProps,
  type SkillHeaderBusyActionId,
  type SkillSubAreaId,
} from "@/components/features/skills/SkillsArea"
import {
  type MemoryActionProps,
  type MemorySubAreaId,
} from "@/components/features/memory/MemoryArea"
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar"
import { useI18n } from "@/i18n"
import type {
  FeatureAreaSummary,
  MainAreaId,
  ProviderSummary,
  SettingsSubAreaId,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types"

// ProviderConfigurationSaveRequest describes one provider form submission.
// ProviderConfigurationSaveRequest 描述单次 Provider 表单提交。

// localizedLiteral chooses a short local component label by the active WebUI locale.
// localizedLiteral 根据当前 WebUI 语言选择组件内短文案。
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
  // Selects one top-level application area from the sidebar brand menu.
  // 从侧栏品牌菜单选择一个顶层应用区域。
  onSelectArea: (area: MainAreaId) => void
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
        onSelectArea={props.onSelectArea}
        workbench={props.workbench}
      />
      <SidebarInset>
        <div className="feature-main-shell">
          <FeatureHeader
            action={skillHeaderAction ?? settingsHeaderAction}
            feature={headerFeature}
          />
          <FeatureShellContent
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

export { FeatureShell, areaIconMap, primaryFeatureAreas, settingsSubAreas }
