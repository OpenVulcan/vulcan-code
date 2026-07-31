import {
  SettingsFeatureContent,
  type SettingsActionProps,
} from "@/components/features/settings/SettingsFeatureContent";
import {
  SkillsArea,
  type SkillActionProps,
  type SkillSubAreaId,
} from "@/components/features/skills/SkillsArea";
import { AgentsArea, type AgentActionProps } from "@/components/features/agents/AgentsArea";
import { AgentSkillsArea } from "@/components/features/agent-skills/AgentSkillsArea";
import { KnowledgeBaseArea } from "@/components/features/knowledge/KnowledgeBaseArea";
import type { AgentSkillActionProps } from "@/components/features/agent-skills/agent-skills-types";
import {
  MemoryArea,
  type MemoryActionProps,
  type MemorySubAreaId,
} from "@/components/features/memory/MemoryArea";
import type {
  MainAreaId,
  SettingsSubAreaId,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";

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
      <AgentSkillsArea actions={props.agentSkillActions} workbench={props.workbench} />
      </div>
    );
  }
  if (props.activeArea === "skills") {
    return (
      <div className="feature-content">
        <SkillsArea
          activeTab={props.activeSkillSubArea}
        skillActions={props.luaSkillActions}
          workbench={props.workbench}
        />
      </div>
    );
  }
  if (props.activeArea === "agents") {
    return (
      <div className="feature-content">
        <AgentsArea
          actions={props.agentActions}
          workbench={props.workbench}
        />
      </div>
    );
  }
  if (props.activeArea === "memory") {
    return (
      <div className="feature-content">
        <MemoryArea
          activeSection={props.activeMemorySubArea}
          actions={props.memoryActions}
          onOpenCredentialManagement={props.onOpenCredentialManagement}
          onSelectSection={props.onSelectMemorySubArea}
          snapshot={props.workbench.vmmMemory}
        />
      </div>
    );
  }
  if (props.activeArea === "knowledge") {
    return (
      <div className="feature-content">
        <KnowledgeBaseArea />
      </div>
    );
  }
  if (props.activeArea === "settings") {
    return (
      <div className="feature-content">
        <SettingsFeatureContent
          activeSettingsSubArea={props.activeSettingsSubArea}
          actions={props.settingsActions}
          onProviderPickerOpenChange={props.onProviderPickerOpenChange}
          providerPickerOpen={props.providerPickerOpen}
          startup={props.startup}
          workbench={props.workbench}
        />
      </div>
    );
  }

  return null;
}
