import { LogsArea } from "@/components/features/settings/settings-diagnostics-area";
// Settings-only styles follow this lazy route instead of inflating the initial workbench stylesheet.
// 仅属于设置的样式随此懒加载路由载入，避免膨胀初始工作台样式表。
import "@/styles/settings-pages.css";
import "@/styles/settings-preferences.css";
import "@/styles/settings-shell.css";
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
import { AiCapabilitiesSettingsArea } from "@/components/features/settings/ai-capabilities-settings";
import { SettingsPage } from "@/components/features/settings/settings-layout";
import { NotificationSettingsArea } from "@/components/features/settings/notification-settings-area";
import { ToolCollapseSettingsSection } from "./tool-collapse-settings";
import { ThinkingPopupSettingsSection } from "./thinking-popup-settings";
import { ConversationLinkSettingsSection } from "./conversation-link-settings";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import {
  activeSettingsHeaderSummary,
  normalizeSettingsSubArea,
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

// SettingsFeatureContent frames every settings page; the feature header shows its title and navigation stays in the sidebar.
// SettingsFeatureContent 包裹每个设置页面；功能页头显示其标题，导航保留在侧栏。
export function SettingsFeatureContent(props: SettingsFeatureContentProps) {
  const current = normalizeSettingsSubArea(props.activeSettingsSubArea);
  return (
    // The key replays the page enter transition on navigation and resets page-local state.
    // key 使切换页面时重放进入动效，并重置页面内部状态。
    <SettingsPage key={current}>
      <SettingsRoutedPage {...props} />
    </SettingsPage>
  );
}

// SettingsRoutedPage routes settings sub pages to their specialized modules.
// SettingsRoutedPage 将设置子页面路由到专用模块。
function SettingsRoutedPage(props: SettingsFeatureContentProps) {
  // Retired per-capability ids open the merged page on the tab that owns them.
  // 已退役的单项能力标识打开合并页面中对应的标签页。
  const providerToolCapability = settingsSubAreaProviderToolCapability(
    props.activeSettingsSubArea,
  );
  if (providerToolCapability || props.activeSettingsSubArea === "ai-capabilities") {
    return <AiCapabilitiesSettingsArea initialCapability={providerToolCapability} />;
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
        outputStyleSettings={<>
          <OutputStyleSettingsSection actions={props.actions} outputStyleCatalog={props.workbench.outputStyleCatalog} />
          <ToolCollapseSettingsSection />
          <ThinkingPopupSettingsSection />
          <ConversationLinkSettingsSection />
        </>}
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
