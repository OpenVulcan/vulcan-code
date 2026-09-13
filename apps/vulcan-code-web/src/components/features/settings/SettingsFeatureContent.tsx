import { LogsArea } from "@/components/features/settings/settings-diagnostics-area";
// Settings-only styles follow this lazy route instead of inflating the initial workbench stylesheet.
// 仅属于设置的样式随此懒加载路由载入，避免膨胀初始工作台样式表。
import "@/styles/settings-pages.css";
import "@/styles/settings-preferences.css";
import "@/styles/visual-lab.css";
import { VisualLabSettingsArea } from "@/components/dev/visual-lab-settings-area";
import { McpSettingsArea } from "@/components/features/settings/mcp-settings-area";
import {
  ArchivedRestoreSettingsArea,
  GitCommitSettingsArea,
  OutputStyleSettingsSection,
} from "@/components/features/settings/misc-settings-areas";
import { AboutLicensesSettingsArea } from "@/components/features/settings/about-licenses-settings-area";
import {
  ModelListSettingsArea,
} from "@/components/features/settings/provider-model-settings";
import {
  CredentialManagementArea,
} from "@/components/features/settings/provider-list-settings";
import { ProviderManagementArea } from "@/components/features/settings/provider-management-settings";
import { ContextBasicsSettingsArea } from "@/components/features/settings/context-basics-settings";
import { ProviderToolSelectionSettingsArea } from "@/components/features/settings/provider-tool-selection-settings";
import { NotificationSettingsArea } from "@/components/features/settings/notification-settings-area";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import {
  activeSettingsHeaderSummary,
  settingsSubAreaProviderToolCapability,
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
  // Startup snapshot used by diagnostics.
  // 诊断页面使用的启动快照。
  startup: StartupSnapshot;
  // Workbench snapshot used by settings pages.
  // 设置页面使用的工作台快照。
  workbench: WorkbenchSnapshot;
  // Opens the exact enabled statistics plugin mount, when available.
  // 存在时打开精确的已启用统计插件挂载点。
  onOpenUsageStatistics?: () => void;
  // Opens the dedicated global diagnostics page from server details.
  // 从服务器详情打开独立的全局运行诊断页面。
  onOpenDiagnostics?: () => void;
}

// SettingsFeatureContent routes settings sub pages to their specialized modules.
// SettingsFeatureContent 将设置子页面路由到专用模块。
export function SettingsFeatureContent(props: SettingsFeatureContentProps) {
  const providerToolCapability = settingsSubAreaProviderToolCapability(
    props.activeSettingsSubArea,
  );
  if (providerToolCapability) {
    return <ProviderToolSelectionSettingsArea capability={providerToolCapability} />;
  }
  if (props.activeSettingsSubArea === "model-providers") {
    return <ProviderManagementArea />;
  }
  if (props.activeSettingsSubArea === "credential-management") {
    return <CredentialManagementArea />;
  }
  if (props.activeSettingsSubArea === "model-list") {
    return <ModelListSettingsArea />;
  }
  if (props.activeSettingsSubArea === "mcp-servers") {
    return <McpSettingsArea actions={props.actions} logs={props.workbench.logs} modules={props.workbench.modules} servers={props.workbench.mcpServers ?? []} startup={props.startup} onOpenDiagnostics={props.onOpenDiagnostics} />;
  }
  if (props.activeSettingsSubArea === "runtime-diagnostics") {
    return (
      <LogsArea
        actions={props.actions}
        providerRequestTraceEnabled={props.workbench.providerRequestTraceEnabled === true}
        startup={props.startup}
        logs={props.workbench.logs}
      />
    );
  }
  if (props.activeSettingsSubArea === "visual-lab") {
    return <VisualLabSettingsArea />;
  }
  if (props.activeSettingsSubArea === "notifications") {
    return <NotificationSettingsArea />;
  }
  if (props.activeSettingsSubArea === "git-commit") {
    return <GitCommitSettingsArea actions={props.actions} workbench={props.workbench} />;
  }
  // The retired composer page resolves to basics while preserving its useful default setting.
  // 已移除的输入组合页跳转到基础设置，并保留有用的默认设置。
  if (props.activeSettingsSubArea === "context-basics" || props.activeSettingsSubArea === "prompt-composer") {
    return (
      <ContextBasicsSettingsArea
        actions={props.actions}
        retrySettings={props.workbench.providerRetry}
        onOpenUsageStatistics={props.onOpenUsageStatistics}
        outputStyleSettings={<OutputStyleSettingsSection actions={props.actions} outputStyleCatalog={props.workbench.outputStyleCatalog} />}
      />
    );
  }
  if (props.activeSettingsSubArea === "about-licenses") {
    return <AboutLicensesSettingsArea appVersion={props.workbench.appVersion} />;
  }
  if (props.activeSettingsSubArea === "archived-restore") {
    return <ArchivedRestoreSettingsArea actions={props.actions} conversations={props.workbench.archivedConversations ?? []} projects={props.workbench.projects} />;
  }
  return (
    <ProviderManagementArea />
  );
}
