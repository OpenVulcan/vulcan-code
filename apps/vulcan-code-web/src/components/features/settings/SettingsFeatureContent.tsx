import {
  AiCapabilitySettingsArea,
} from "@/components/features/settings/ai-capability-target-settings";
import {
  AiCapabilityVendorsArea,
} from "@/components/features/settings/ai-capability-vendor-settings";
import { LogsArea } from "@/components/features/settings/settings-diagnostics-area";
import { McpSettingsArea } from "@/components/features/settings/mcp-settings-area";
import {
  AboutLicensesSettingsArea,
  ArchivedRestoreSettingsArea,
  GitCommitSettingsArea,
  PromptComposerSettingsArea,
} from "@/components/features/settings/misc-settings-areas";
import {
  ModelListSettingsArea,
} from "@/components/features/settings/provider-model-settings";
import {
  ProvidersArea,
} from "@/components/features/settings/provider-list-settings";
import { ProviderRetrySettingsArea } from "@/components/features/settings/provider-retry-settings";
import { NotificationSettingsArea } from "@/components/features/settings/notification-settings-area";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import {
  activeSettingsHeaderSummary,
  settingsSubAreaCapabilityKind,
} from "@/components/features/settings/settings-navigation";
import { useI18n } from "@/i18n";
import type {
  ProviderSummary,
  SettingsSubAreaId,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";

export type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
export {
  activeSettingsHeaderSummary,
  settingsSubAreaCapabilityKind,
  settingsSubAreaGroups,
  settingsSubAreas,
  type SettingsHeaderSummary,
  type SettingsSubAreaEntry,
  type SettingsSubAreaGroup,
} from "@/components/features/settings/settings-navigation";

// localizedLiteral chooses a display string by the active WebUI locale.
// localizedLiteral 根据当前 WebUI 语言选择展示文本。
export function localizedLiteral(locale: string, zh: string, en: string): string {
  return locale === "zh-CN" ? zh : en;
}

// SettingsFeatureContentProps describes routed settings content inputs.
// SettingsFeatureContentProps 描述设置内容路由输入。
export interface SettingsFeatureContentProps {
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId;
  // Service-backed settings page actions.
  // 服务端支持的设置页面动作。
  actions: SettingsActionProps;
  // Whether the capability target configuration dialog is open.
  // 能力目标配置弹窗是否打开。
  capabilityTargetsDialogOpen: boolean;
  // Provider currently opened in the edit dialog.
  // 当前在编辑弹窗中打开的模型供应商。
  editingProvider: ProviderSummary | null;
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
  // Startup snapshot used by diagnostics.
  // 诊断页面使用的启动快照。
  startup: StartupSnapshot;
  // Workbench snapshot used by settings pages.
  // 设置页面使用的工作台快照。
  workbench: WorkbenchSnapshot;
}

// SettingsFeatureContent routes settings sub pages to their specialized modules.
// SettingsFeatureContent 将设置子页面路由到专用模块。
export function SettingsFeatureContent(props: SettingsFeatureContentProps) {
  const { t } = useI18n();
  if (props.activeSettingsSubArea === "model-providers") {
    return (
      <ProvidersArea
        actions={props.actions}
        editingProvider={props.editingProvider}
        onEditingProviderChange={props.onEditingProviderChange}
        onProviderPickerOpenChange={props.onProviderPickerOpenChange}
        pickerOpen={props.providerPickerOpen}
        providers={props.workbench.providers}
      />
    );
  }
  if (props.activeSettingsSubArea === "model-list") {
    return <ModelListSettingsArea actions={props.actions} dialogOpen={props.modelListDialogOpen} onDialogOpenChange={props.onModelListDialogOpenChange} providers={props.workbench.providers} />;
  }
  if (props.activeSettingsSubArea === "ai-vendors") {
    return <AiCapabilityVendorsArea actions={props.actions} capabilities={props.workbench.aiCapabilities ?? []} catalog={props.workbench.aiCapabilityVendorCatalog ?? []} providers={props.workbench.providers} vendors={props.workbench.aiCapabilityVendors ?? []} />;
  }
  const activeCapabilityKind = settingsSubAreaCapabilityKind(props.activeSettingsSubArea);
  if (activeCapabilityKind) {
    const header = activeSettingsHeaderSummary(props.activeSettingsSubArea, t);
    return (
      <AiCapabilitySettingsArea
        actions={props.actions}
        activeArea={props.activeSettingsSubArea}
        capabilityKind={activeCapabilityKind}
        capabilities={props.workbench.aiCapabilities ?? []}
        configDialogOpen={props.capabilityTargetsDialogOpen}
        headerDescription={header.detail}
        headerTitle={header.title}
        onConfigDialogOpenChange={props.onCapabilityTargetsDialogOpenChange}
        vendors={props.workbench.aiCapabilityVendors ?? []}
      />
    );
  }
  if (props.activeSettingsSubArea === "mcp-servers") {
    return <McpSettingsArea actions={props.actions} logs={props.workbench.logs} modules={props.workbench.modules} servers={props.workbench.mcpServers ?? []} startup={props.startup} />;
  }
  if (props.activeSettingsSubArea === "runtime-diagnostics") {
    return <LogsArea startup={props.startup} logs={props.workbench.logs} />;
  }
  if (props.activeSettingsSubArea === "notifications") {
    return <NotificationSettingsArea />;
  }
  if (props.activeSettingsSubArea === "git-commit") {
    return <GitCommitSettingsArea actions={props.actions} workbench={props.workbench} />;
  }
  if (props.activeSettingsSubArea === "provider-retry") {
    return <ProviderRetrySettingsArea actions={props.actions} settings={props.workbench.providerRetry} />;
  }
  if (props.activeSettingsSubArea === "prompt-composer") {
    return (
      <PromptComposerSettingsArea
        attachmentCapabilities={props.workbench.composerAttachmentCapabilities ?? []}
        reasoningSelection={props.workbench.reasoningSelection ?? null}
        slashCommands={props.workbench.composerSlashCommands ?? []}
      />
    );
  }
  if (props.activeSettingsSubArea === "about-licenses") {
    return <AboutLicensesSettingsArea appVersion={props.workbench.appVersion} />;
  }
  if (props.activeSettingsSubArea === "archived-restore") {
    return <ArchivedRestoreSettingsArea conversations={props.workbench.archivedConversations ?? []} projects={props.workbench.projects} />;
  }
  return (
    <ProvidersArea
      actions={props.actions}
      editingProvider={props.editingProvider}
      onEditingProviderChange={props.onEditingProviderChange}
      onProviderPickerOpenChange={props.onProviderPickerOpenChange}
      pickerOpen={props.providerPickerOpen}
      providers={props.workbench.providers}
    />
  );
}
