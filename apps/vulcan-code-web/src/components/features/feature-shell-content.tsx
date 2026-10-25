import React, { type ReactNode } from "react";
import type { SettingsActionProps } from "@/components/features/settings/settings-action-types";
import { Spinner } from "@/ui-kit/primitives/spinner";
import { Button } from "@/ui-kit/primitives/button";
import { PageNotice } from "@/components/page";
import { RenderErrorBoundary } from "@/app/render-error-boundary";
import { useI18n } from "@/i18n";
import type {
  MainAreaId,
  SettingsSubAreaId,
  WorkbenchSnapshot,
} from "@/types";

// LazySettingsFeatureContent isolates settings and Visual Lab from the console-first workbench chunk.
// LazySettingsFeatureContent 将设置与视觉实验室隔离在控制台优先的工作台分块之外。
const LazySettingsFeatureContent = React.lazy(() => import("@/components/features/settings/settings-feature-content").then((module) => ({ default: module.SettingsFeatureContent })));
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

// FeatureContent renders the settings page content.
// FeatureContent 渲染设置页面内容。
export function FeatureContent(props: {
  // Active top-level feature area.
  // 当前顶层功能区域。
  activeArea: MainAreaId;
  // Selected settings sub page.
  // 当前设置二级页面。
  activeSettingsSubArea: SettingsSubAreaId;
  // Opens the available statistics plugin through the owning workspace navigation.
  // 通过所属工作区导航打开可用统计插件。
  onOpenUsageStatistics?: () => void;
  // Service-backed settings page actions.
  // 服务端支持的设置页面动作。
  settingsActions: SettingsActionProps;
  // Workbench snapshot used by feature pages.
  // 功能页面使用的工作台快照。
  workbench: WorkbenchSnapshot;
}) {
  if (props.activeArea === "settings") {
    return (
      <div className="feature-content settings-feature-content">
        <FeatureLoadBoundary regionKey={`${props.activeArea}:${props.activeSettingsSubArea}`}><LazySettingsFeatureContent
          activeSettingsSubArea={props.activeSettingsSubArea}
          onOpenUsageStatistics={props.onOpenUsageStatistics}
          actions={props.settingsActions}
          workbench={props.workbench}
        /></FeatureLoadBoundary>
      </div>
    );
  }

  return null;
}
