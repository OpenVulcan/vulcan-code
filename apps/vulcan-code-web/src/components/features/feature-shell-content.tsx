import {
  SettingsFeatureContent,
  type SettingsActionProps,
} from "@/components/features/settings/SettingsFeatureContent";
import {
  SkillsArea,
  type SkillActionProps,
  type SkillSubAreaId,
} from "@/components/features/skills/SkillsArea";
import {
  MemoryArea,
  type MemoryActionProps,
  type MemorySubAreaId,
} from "@/components/features/memory/MemoryArea";
import type {
  MainAreaId,
  ProviderSummary,
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
  // Provider currently opened in the edit dialog.
  // 当前在编辑弹窗中打开的模型供应商。
  editingProvider: ProviderSummary | null;
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps;
  // Service-backed VMM memory page actions.
  // 服务端支持的 VMM 记忆页面动作。
  memoryActions: MemoryActionProps;
  // Service-backed settings page actions.
  // 服务端支持的设置页面动作。
  settingsActions: SettingsActionProps;
  // Whether the capability target configuration dialog is open.
  // 能力目标配置弹窗是否打开。
  capabilityTargetsDialogOpen: boolean;
  // Whether the enabled-model configuration dialog is open.
  // 启用模型配置弹窗是否打开。
  modelListDialogOpen: boolean;
  // Updates the provider edit dialog target.
  // 更新模型供应商编辑弹窗目标。
  onEditingProviderChange: (provider: ProviderSummary | null) => void;
  // Updates the capability target configuration dialog open state.
  // 更新能力目标配置弹窗打开状态。
  onCapabilityTargetsDialogOpenChange: (open: boolean) => void;
  // Updates the enabled-model configuration dialog open state.
  // 更新启用模型配置弹窗打开状态。
  onModelListDialogOpenChange: (open: boolean) => void;
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
  if (props.activeArea === "skills") {
    return (
      <div className="feature-content">
        <SkillsArea
          activeTab={props.activeSkillSubArea}
          skillActions={props.skillActions}
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
          memories={props.workbench.memories}
          snapshot={props.workbench.vmmMemory}
        />
      </div>
    );
  }
  if (props.activeArea === "settings") {
    return (
      <div className="feature-content">
        <SettingsFeatureContent
          activeSettingsSubArea={props.activeSettingsSubArea}
          actions={props.settingsActions}
          capabilityTargetsDialogOpen={props.capabilityTargetsDialogOpen}
          editingProvider={props.editingProvider}
          modelListDialogOpen={props.modelListDialogOpen}
          onCapabilityTargetsDialogOpenChange={props.onCapabilityTargetsDialogOpenChange}
          onEditingProviderChange={props.onEditingProviderChange}
          onModelListDialogOpenChange={props.onModelListDialogOpenChange}
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
