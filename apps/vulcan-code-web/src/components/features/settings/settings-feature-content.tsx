import { LogsArea } from "@/components/features/settings/settings-diagnostics-area";
// Settings-only styles follow this lazy route instead of inflating the initial workbench stylesheet.
// 仅属于设置的样式随此懒加载路由载入，避免膨胀初始工作台样式表。
import "@/ui-kit/styles/features/settings-pages.css";
import "@/ui-kit/styles/features/settings-forms.css";
import "@/ui-kit/styles/features/settings-preferences.css";
import "@/ui-kit/styles/features/settings-shell.css";
import "@/ui-kit/styles/features/model-services.css";
import "@/ui-kit/styles/features/visual-lab.css";
import { VisualLabSettingsArea } from "@/components/dev/visual-lab-settings-area";
import {
  ArchivedRestoreSettingsArea,
  GitCommitSettingsArea,
  AnswerModeSettingsSection,
  OutputStyleSettingsSection,
} from "@/components/features/settings/misc-settings-areas";
import { AboutLicensesSettingsArea } from "@/components/features/settings/about-licenses-settings-area";
import { ChangelogSettingsArea } from "@/components/features/settings/changelog-settings-area";
import { ModelServicesArea } from "@/components/features/settings/model-services/model-services-area";
import { OpenVulcanAccountArea } from "@/components/features/account/openvulcan-account-area";
import { ContextBasicsSettingsArea } from "@/components/features/settings/context-basics-settings";
import { SettingsPage } from "@/components/features/settings/settings-layout";
import { NotificationSettingsArea } from "@/components/features/settings/notification-settings-area";
import { AppearanceSettingsArea } from "@/components/features/settings/appearance-settings-area";
import { PermissionJudgeSettingsArea } from "@/components/features/settings/permission-judge-settings-area";
import { SubagentModelsSettingsArea } from "@/components/features/subagent-models/subagent-models-settings-area";
import { McpServiceSettingsArea } from "@/components/features/mcp-service/mcp-service-settings-area";
import { ToolCollapseSettingsSection } from "./tool-collapse-settings";
import { ThinkingPopupSettingsSection } from "./thinking-popup-settings";
import { ConversationLinkSettingsSection } from "./conversation-link-settings";
import { SystemPromptSettingsCard } from "./system-prompt-settings";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import {
  normalizeSettingsSubArea,
  settingsSubAreaProviderToolCapability,
} from "@/components/features/settings/settings-navigation";
import type {
  SettingsSubAreaId,
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
  // Workbench snapshot used by settings pages.
  // 设置页面使用的工作台快照。
  workbench: WorkbenchSnapshot;
  // Opens the exact enabled statistics plugin mount, when available.
  // 存在时打开精确的已启用统计插件挂载点。
  onOpenUsageStatistics?: () => void;
}

// READABLE_SETTINGS_PAGES are preference pages that read as one centred column; work surfaces fill the pane.
// READABLE_SETTINGS_PAGES 是以居中单列阅读的偏好类页面；工作界面铺满面板。
const READABLE_SETTINGS_PAGES: ReadonlySet<SettingsSubAreaId> = new Set(["context-basics", "appearance", "notifications", "git-commit", "permission-judge", "subagent-models", "mcp-service", "changelog", "about-licenses"]);

// SettingsFeatureContent frames every settings page; the feature header shows its title and navigation stays in the sidebar.
// SettingsFeatureContent 包裹每个设置页面；功能页头显示其标题，导航保留在侧栏。
export function SettingsFeatureContent(props: SettingsFeatureContentProps) {
  const current = normalizeSettingsSubArea(props.activeSettingsSubArea);
  return (
    // The key replays the page enter transition on navigation and resets page-local state.
    // key 使切换页面时重放进入动效，并重置页面内部状态。
    <SettingsPage key={current} width={READABLE_SETTINGS_PAGES.has(current) ? "readable" : "full"}>
      <SettingsRoutedPage {...props} />
    </SettingsPage>
  );
}

// SettingsRoutedPage routes settings sub pages to their specialized modules.
// SettingsRoutedPage 将设置子页面路由到专用模块。
function SettingsRoutedPage(props: SettingsFeatureContentProps) {
  // Retired provider, account, model and capability ids open the merged model services page on their tab.
  // 已退役的供应商、账号、模型与能力标识打开合并后的模型服务页对应标签。
  const providerToolCapability = settingsSubAreaProviderToolCapability(
    props.activeSettingsSubArea,
  );
  if (providerToolCapability || props.activeSettingsSubArea === "ai-capabilities") {
    return <ModelServicesArea initialCapability={providerToolCapability} initialTab="functions" />;
  }
  if (props.activeSettingsSubArea === "openvulcan-account") {
    return <OpenVulcanAccountArea />;
  }
  if (props.activeSettingsSubArea === "model-list") {
    return <ModelServicesArea initialView="model" />;
  }
  if (props.activeSettingsSubArea === "model-providers" || props.activeSettingsSubArea === "credential-management") {
    return <ModelServicesArea />;
  }
  if (props.activeSettingsSubArea === "runtime-diagnostics") {
    return (
      <LogsArea
        actions={props.actions}
        providerRequestTraceEnabled={props.workbench.providerRequestTraceEnabled === true}
        providerErrorTraceEnabled={props.workbench.providerErrorTraceEnabled === true}
        logs={props.workbench.logs}
      />
    );
  }
  if (props.activeSettingsSubArea === "visual-lab") {
    return <VisualLabSettingsArea />;
  }
  if (props.activeSettingsSubArea === "appearance") {
    return <AppearanceSettingsArea />;
  }
  if (props.activeSettingsSubArea === "notifications") {
    return <NotificationSettingsArea />;
  }
  if (props.activeSettingsSubArea === "git-commit") {
    return <GitCommitSettingsArea actions={props.actions} workbench={props.workbench} />;
  }
  if (props.activeSettingsSubArea === "subagent-models") {
    return <SubagentModelsSettingsArea providers={props.workbench.providers} />;
  }
  if (props.activeSettingsSubArea === "mcp-service") {
    return <McpServiceSettingsArea projects={props.workbench.projects} />;
  }
  if (props.activeSettingsSubArea === "permission-judge") {
    return <PermissionJudgeSettingsArea projects={props.workbench.projects} />;
  }
  // The retired composer page resolves to basics while preserving its useful default setting.
  // 已移除的输入组合页跳转到基础设置，并保留有用的默认设置。
  if (props.activeSettingsSubArea === "context-basics" || props.activeSettingsSubArea === "prompt-composer") {
    return (
      <ContextBasicsSettingsArea
        actions={props.actions}
        retrySettings={props.workbench.providerRetry}
        commandShell={props.workbench.hostTools?.commandShell}
        onOpenUsageStatistics={props.onOpenUsageStatistics}
        outputStyleSettings={<>
          <OutputStyleSettingsSection actions={props.actions} outputStyleCatalog={props.workbench.outputStyleCatalog} />
          <AnswerModeSettingsSection actions={props.actions} defaultAnswerMode={props.workbench.defaultAnswerMode} />
          <ToolCollapseSettingsSection />
          <ThinkingPopupSettingsSection />
          <ConversationLinkSettingsSection />
        </>}
        systemPromptSettings={<SystemPromptSettingsCard projects={props.workbench.projects} />}
      />
    );
  }
  if (props.activeSettingsSubArea === "changelog") {
    return <ChangelogSettingsArea appVersion={props.workbench.appVersion} />;
  }
  if (props.activeSettingsSubArea === "about-licenses") {
    return <AboutLicensesSettingsArea appVersion={props.workbench.appVersion} />;
  }
  if (props.activeSettingsSubArea === "archived-restore") {
    return <ArchivedRestoreSettingsArea actions={props.actions} conversations={props.workbench.archivedConversations ?? []} projects={props.workbench.projects} />;
  }
  return (
    <ModelServicesArea />
  );
}
