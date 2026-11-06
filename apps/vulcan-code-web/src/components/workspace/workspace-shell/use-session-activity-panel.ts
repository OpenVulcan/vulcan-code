// Session activity wiring of the workspace shell: the running background command count of the selected session and
// the opener of the activity tab in the right context panel.
// 工作区外壳的会话活动接线：当前选中会话中运行中的后台命令数，以及在右侧上下文面板中打开活动页签的函数。
import * as React from "react";
import { useSessionBackgroundJobCount } from "@/components/workspace/session-activity/use-session-background-job-count";
import {
  openWorkspaceActivityContextTab,
  type WorkspaceContextTabStateEntry,
} from "@/components/workspace/workspace-shell/context-tabs";
import type { TranslationKey } from "@/i18n";
import type { SessionActivityScope } from "@/types";

export interface SessionActivityPanelWiring {
  // Running background commands of the selected session; 0 without one.
  // 当前选中会话中运行中的后台命令数；没有会话时为 0。
  backgroundJobCount: number
  // Shows `page` of the activity tab and opens the right context panel.
  // 在活动页签中显示 `page` 并打开右侧上下文面板。
  openActivityPanel: (page: SessionActivityScope) => void
}

/** Returns the activity count and the stable activity tab opener for the workspace shell.
 * 返回工作区外壳所需的活动计数与稳定的活动页签打开函数。 */
export function useSessionActivityPanel(
  // Selected chat session, or null when none is shown.
  // 当前选中的对话会话；未显示会话时为 null。
  sessionId: string | null,
  // Setter of the context tab state owned by the workspace shell.
  // 工作区外壳持有的上下文页签状态设置函数。
  setContextTabStateEntry: React.Dispatch<React.SetStateAction<WorkspaceContextTabStateEntry>>,
  // Opens the right context panel.
  // 打开右侧上下文面板。
  openRightContextPanel: () => void,
  // Translation helper for default tab titles.
  // 默认页签标题的翻译辅助函数。
  t: (key: TranslationKey) => string,
): SessionActivityPanelWiring {
  const backgroundJobCount = useSessionBackgroundJobCount(sessionId);
  const openActivityPanel = React.useCallback((page: SessionActivityScope) => {
    setContextTabStateEntry((current) => ({
      ...current,
      state: openWorkspaceActivityContextTab(current.state, page, t),
    }));
    openRightContextPanel();
  }, [openRightContextPanel, setContextTabStateEntry, t]);
  return { backgroundJobCount, openActivityPanel };
}
