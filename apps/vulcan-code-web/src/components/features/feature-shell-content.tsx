import React, { type ReactNode } from "react";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import type { SkillActionProps, SkillSubAreaId } from "@/components/features/skills/skill-area-types";
import type { AgentActionProps } from "@/components/features/agents/agent-area-types";
import type { AgentSkillActionProps } from "@/components/features/agent-skills/agent-skills-types";
import type { MemoryActionProps } from "@/components/features/memory/use-memory-area-controller";
import { Spinner } from "@/ui-kit/primitives/spinner";
import { Button } from "@/ui-kit/primitives/button";
import { PageNotice } from "@/components/page";
import { RenderErrorBoundary } from "@/app/render-error-boundary";
import { useI18n } from "@/i18n";
import type {
  MainAreaId,
  SettingsSubAreaId,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";

// MemorySubAreaId identifies one service-projected VMM section.
// MemorySubAreaId 标识一个由服务投影的 VMM 分区。
type MemorySubAreaId = string;

// LazySettingsFeatureContent isolates settings and Visual Lab from the console-first workbench chunk.
// LazySettingsFeatureContent 将设置与视觉实验室隔离在控制台优先的工作台分块之外。
const LazySettingsFeatureContent = React.lazy(() => import("@/components/features/settings/settings-feature-content").then((module) => ({ default: module.SettingsFeatureContent })));
// LazySkillsArea isolates LuaSkills management from the console-first workbench chunk.
// LazySkillsArea 将 LuaSkills 管理隔离在控制台优先的工作台分块之外。
const LazySkillsArea = React.lazy(() => import("@/components/features/skills/skills-area").then((module) => ({ default: module.SkillsArea })));
// LazyAgentsArea isolates agent management from the console-first workbench chunk.
// LazyAgentsArea 将智能体管理隔离在控制台优先的工作台分块之外。
const LazyAgentsArea = React.lazy(() => import("@/components/features/agents/agents-area").then((module) => ({ default: module.AgentsArea })));
// LazyAgentSkillsArea isolates prompt skill management from the console-first workbench chunk.
// LazyAgentSkillsArea 将提示词技能管理隔离在控制台优先的工作台分块之外。
const LazyAgentSkillsArea = React.lazy(() => import("@/components/features/agent-skills/agent-skills-area").then((module) => ({ default: module.AgentSkillsArea })));
// LazyMemoryArea isolates VMM management from the console-first workbench chunk.
// LazyMemoryArea 将 VMM 管理隔离在控制台优先的工作台分块之外。
const LazyMemoryArea = React.lazy(() => import("@/components/features/memory/memory-area").then((module) => ({ default: module.MemoryArea })));
// FeatureLoadBoundary provides one stable non-layout-shifting fallback for deferred feature chunks.
// FeatureLoadBoundary 为延迟功能分块提供一个稳定且不改变布局的加载回退。
export function FeatureLoadBoundary(props: {
  // children is the selected lazy feature subtree.
  // children 是所选延迟加载功能子树。
  children: ReactNode;
  // regionKey clears failed navigation while preserving healthy page state.
  // regionKey 清除已失败的导航，同时保留正常页面状态。
  regionKey: string;
}) {
  // t keeps recovery text aligned with the current application language.
  // t 使恢复文案与当前应用语言一致。
  const { t } = useI18n();
  return (
    <RenderErrorBoundary resetKey={props.regionKey} fallback={(error) => (
      <PageNotice
        className="vk-region-fault"
        tone="error"
        title={t("features.shell.thisPageCouldNot")}
        details={error.message}
        detailsLabel={t("features.shell.errorDetails")}
        actions={<Button variant="outline" onClick={() => window.location.reload()}>{t("features.shell.reloadApplication")}</Button>}
      >
        {t("features.shell.otherPagesRemainAvailable")}
      </PageNotice>
    )}>
    <React.Suspense fallback={<div className="vk-region-wait"><Spinner /></div>}>
      {props.children}
    </React.Suspense>
    </RenderErrorBoundary>
  );
}

// FeatureContent renders the selected non-console feature area.
// FeatureContent 渲染当前选中的非控制台功能区域。
export function FeatureContent(props: {
  // Active top-level feature area.
  // 当前顶层功能区域。
  activeArea: MainAreaId;
  // Selected VMM memory sub page.
  // 当前选中的 VMM 记忆子页面。
  activeMemorySubArea: MemorySubAreaId;
  // Selected skill sub page.
  // 当前选中的技能子页面。
  activeSkillSubArea: SkillSubAreaId;
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId;
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  // Independent LuaSkills lifecycle actions.
  // 独立 LuaSkills 生命周期动作。
  luaSkillActions: SkillActionProps;
  // Independent prompt-based Agent Skills actions.
  // 独立提示词 Agent Skills 动作。
  agentSkillActions: AgentSkillActionProps;
  // Service-backed independent agent page actions.
  // 服务端支持的独立智能体页面动作。
  agentActions: AgentActionProps;
  // Service-backed VMM memory page actions.
  // 服务端支持的 VMM 记忆页面动作。
  memoryActions: MemoryActionProps;
  // Updates the selected VMM memory sub page.
  // 更新选中的 VMM 记忆子页面。
  onSelectMemorySubArea: (area: MemorySubAreaId) => void;
  // Opens credential management for memory inference prerequisites.
  // 为记忆推理前置条件打开凭据管理。
  onOpenCredentialManagement: () => void;
  // Opens the available statistics plugin through the owning workspace navigation.
  // 通过所属工作区导航打开可用统计插件。
  onOpenUsageStatistics?: () => void;
  // Opens the global diagnostics route from the MCP server page under plugin management.
  // 从插件管理下的 MCP 服务器页面打开全局诊断路由。
  onOpenDiagnostics?: () => void;
  // Service-backed settings page actions.
  // 服务端支持的设置页面动作。
  settingsActions: SettingsActionProps;
  // Startup snapshot used by settings diagnostics.
  // 设置诊断页面使用的启动快照。
  startup: StartupSnapshot;
  // Workbench snapshot used by feature pages.
  // 功能页面使用的工作台快照。
  workbench: WorkbenchSnapshot;
}) {
  if (props.activeArea === "agent-skills") {
    return (
      <div className="feature-content">
        <FeatureLoadBoundary regionKey={props.activeArea}><LazyAgentSkillsArea actions={props.agentSkillActions} workbench={props.workbench} /></FeatureLoadBoundary>
      </div>
    );
  }
  if (props.activeArea === "skills") {
    return (
      <div className="feature-content">
        <FeatureLoadBoundary regionKey={`${props.activeArea}:${props.activeSkillSubArea}`}><LazySkillsArea
          activeTab={props.activeSkillSubArea}
          onOpenDiagnostics={props.onOpenDiagnostics}
          skillActions={props.luaSkillActions}
          workbench={props.workbench}
        /></FeatureLoadBoundary>
      </div>
    );
  }
  if (props.activeArea === "agents") {
    return (
      <div className="feature-content">
        <FeatureLoadBoundary regionKey={props.activeArea}><LazyAgentsArea
          actions={props.agentActions}
          workbench={props.workbench}
        /></FeatureLoadBoundary>
      </div>
    );
  }
  if (props.activeArea === "memory") {
    return (
      <div className="feature-content">
        <FeatureLoadBoundary regionKey={`${props.activeArea}:${props.activeMemorySubArea}`}><LazyMemoryArea
          activeSection={props.activeMemorySubArea}
          actions={props.memoryActions}
          onOpenCredentialManagement={props.onOpenCredentialManagement}
          onSelectSection={props.onSelectMemorySubArea}
          snapshot={props.workbench.vmmMemory}
        /></FeatureLoadBoundary>
      </div>
    );
  }
  if (props.activeArea === "settings") {
    return (
      <div className="feature-content settings-feature-content">
        <FeatureLoadBoundary regionKey={`${props.activeArea}:${props.activeSettingsSubArea}`}><LazySettingsFeatureContent
          activeSettingsSubArea={props.activeSettingsSubArea}
          onOpenUsageStatistics={props.onOpenUsageStatistics}
          actions={props.settingsActions}
          startup={props.startup}
          workbench={props.workbench}
        /></FeatureLoadBoundary>
      </div>
    );
  }

  return null;
}
