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
} from "@/components/features/settings/settings-navigation";
import type {
  SettingsSubAreaId,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";

export type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
export {
  activeSettingsHeaderSummary,
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
  if (props.activeSettingsSubArea === "model-providers") {
    return (
      <ProvidersArea
        onProviderPickerOpenChange={props.onProviderPickerOpenChange}
        pickerOpen={props.providerPickerOpen}
      />
    );
  }
  if (props.activeSettingsSubArea === "model-list") {
    return <ModelListSettingsArea />;
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
      onProviderPickerOpenChange={props.onProviderPickerOpenChange}
      pickerOpen={props.providerPickerOpen}
    />
  );
}
