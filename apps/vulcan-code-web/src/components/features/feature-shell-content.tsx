import React, { type ReactNode } from "react";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import type { SkillActionProps, SkillSubAreaId } from "@/components/features/skills/skill-area-types";
import type { AgentActionProps } from "@/components/features/agents/agent-area-types";
import type { AgentSkillActionProps } from "@/components/features/agent-skills/agent-skills-types";
import type { MemoryActionProps } from "@/components/features/memory/use-memory-area-controller";
import { Spinner } from "@/components/ui/spinner";
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
const LazySettingsFeatureContent = React.lazy(() => import("@/components/features/settings/SettingsFeatureContent").then((module) => ({ default: module.SettingsFeatureContent })));
// LazySkillsArea isolates LuaSkills management from the console-first workbench chunk.
// LazySkillsArea 将 LuaSkills 管理隔离在控制台优先的工作台分块之外。
const LazySkillsArea = React.lazy(() => import("@/components/features/skills/SkillsArea").then((module) => ({ default: module.SkillsArea })));
// LazyAgentsArea isolates agent management from the console-first workbench chunk.
// LazyAgentsArea 将智能体管理隔离在控制台优先的工作台分块之外。
const LazyAgentsArea = React.lazy(() => import("@/components/features/agents/AgentsArea").then((module) => ({ default: module.AgentsArea })));
// LazyAgentSkillsArea isolates prompt skill management from the console-first workbench chunk.
// LazyAgentSkillsArea 将提示词技能管理隔离在控制台优先的工作台分块之外。
const LazyAgentSkillsArea = React.lazy(() => import("@/components/features/agent-skills/AgentSkillsArea").then((module) => ({ default: module.AgentSkillsArea })));
// LazyMemoryArea isolates VMM management from the console-first workbench chunk.
// LazyMemoryArea 将 VMM 管理隔离在控制台优先的工作台分块之外。
const LazyMemoryArea = React.lazy(() => import("@/components/features/memory/MemoryArea").then((module) => ({ default: module.MemoryArea })));
// LazyKnowledgeBaseArea isolates knowledge management from the console-first workbench chunk.
// LazyKnowledgeBaseArea 将知识库管理隔离在控制台优先的工作台分块之外。
const LazyKnowledgeBaseArea = React.lazy(() => import("@/components/features/knowledge/KnowledgeBaseArea").then((module) => ({ default: module.KnowledgeBaseArea })));

// FeatureLoadBoundary provides one stable non-layout-shifting fallback for deferred feature chunks.
// FeatureLoadBoundary 为延迟功能分块提供一个稳定且不改变布局的加载回退。
function FeatureLoadBoundary(props: { children: ReactNode }) {
  return (
    <React.Suspense fallback={<div className="flex min-h-32 items-center justify-center"><Spinner /></div>}>
      {props.children}
    </React.Suspense>
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
  // Service-backed settings page actions.
  // 服务端支持的设置页面动作。
  settingsActions: SettingsActionProps;
  // Updates the add-provider picker open state.
  // 更新添加模型供应商选择弹窗的打开状态。
  onProviderPickerOpenChange: (open: boolean) => void;
  // Whether the add-provider picker is open.
  // 添加模型供应商选择弹窗是否打开。
  providerPickerOpen: boolean;
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
        <FeatureLoadBoundary><LazyAgentSkillsArea actions={props.agentSkillActions} workbench={props.workbench} /></FeatureLoadBoundary>
      </div>
    );
  }
  if (props.activeArea === "skills") {
    return (
      <div className="feature-content">
        <FeatureLoadBoundary><LazySkillsArea
          activeTab={props.activeSkillSubArea}
          skillActions={props.luaSkillActions}
          workbench={props.workbench}
        /></FeatureLoadBoundary>
      </div>
    );
  }
  if (props.activeArea === "agents") {
    return (
      <div className="feature-content">
        <FeatureLoadBoundary><LazyAgentsArea
          actions={props.agentActions}
          workbench={props.workbench}
        /></FeatureLoadBoundary>
      </div>
    );
  }
  if (props.activeArea === "memory") {
    return (
      <div className="feature-content">
        <FeatureLoadBoundary><LazyMemoryArea
          activeSection={props.activeMemorySubArea}
          actions={props.memoryActions}
          onOpenCredentialManagement={props.onOpenCredentialManagement}
          onSelectSection={props.onSelectMemorySubArea}
          snapshot={props.workbench.vmmMemory}
        /></FeatureLoadBoundary>
      </div>
    );
  }
  if (props.activeArea === "knowledge") {
    return (
      <div className="feature-content">
        <FeatureLoadBoundary><LazyKnowledgeBaseArea /></FeatureLoadBoundary>
      </div>
    );
  }
  if (props.activeArea === "settings") {
    return (
      <div className="feature-content">
        <FeatureLoadBoundary><LazySettingsFeatureContent
          activeSettingsSubArea={props.activeSettingsSubArea}
          actions={props.settingsActions}
          onProviderPickerOpenChange={props.onProviderPickerOpenChange}
          providerPickerOpen={props.providerPickerOpen}
          startup={props.startup}
          workbench={props.workbench}
        /></FeatureLoadBoundary>
      </div>
    );
  }

  return null;
}
