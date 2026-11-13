import React from "react";
import { composerSlashCommandsForSession, normalizedComposerSlashCommandId, type ComposerSlashCommandResult } from "@/components/composer/composer-model";
import type { ComposerSettingsRequest, ComposerSettingsSection } from "@/components/composer/composer-settings-request";
import { useDedicatedComposer } from "@/components/composer/dedicated/dedicated-session-controls";
import { ConversationHistoryDialogContext } from "@/state/conversation-history-dialog";
import type { ComposerSlashCommandSummary } from "@/types";

// ComposerSlashSession is the per-session slash catalog plus the settings-menu request it can raise.
// ComposerSlashSession 是按会话筛选的快速指令目录及其可发起的设置菜单请求。
export interface ComposerSlashSession {
  // commands lists only the commands that apply to the current session kind.
  // commands 只列出适用于当前会话种类的指令。
  commands: ComposerSlashCommandSummary[];
  // settingsRequest asks the footer settings menu to open, optionally at one submenu.
  // settingsRequest 请求底栏设置菜单打开，可选地直接展开某个子菜单。
  settingsRequest: ComposerSettingsRequest | null;
  // openSettings raises a fresh settings request for the given section.
  // openSettings 为指定分区发起一次新的设置请求。
  openSettings: (section: ComposerSettingsSection | null) => void;
  // openGoal opens the goal dialog with a stable identity for slash activation.
  // openGoal 以稳定引用打开目标对话框，供快速指令激活使用。
  openGoal: () => void;
  // Routes the history command locally and delegates other actions to the workbench.
  // 在本地路由历史指令，其余动作交给工作台。
  routeCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
}

// useComposerSlashSession hides frozen or unused commands in dedicated sessions and routes settings commands.
// useComposerSlashSession 在专职会话中隐藏已冻结或不使用的指令，并路由设置类指令。
// Takes the catalog, goal opener, composer session and workbench router. Returns the filtered commands and handlers.
// 接收目录、目标打开函数、输入区会话及工作台路由，返回筛选后的指令和处理函数。
export function useComposerSlashSession(
  catalog: ComposerSlashCommandSummary[],
  openGoalDialog: () => Promise<void> | void,
  conversationId: string | null,
  onRouteCommand: ComposerSlashSession["routeCommand"],
): ComposerSlashSession {
  const dedicatedToolAccess = useDedicatedComposer()?.toolAccess ?? null;
  // Only the composer matching the open conversation surface can open its history.
  // 只有匹配当前会话界面的输入区能打开它的历史。
  const historyDialog = React.useContext(ConversationHistoryDialogContext);
  // A draft or another surface cannot offer the history command.
  // 草稿或其他界面不提供历史指令。
  const canOpenHistory = historyDialog !== null && historyDialog.sessionId === conversationId;
  const commands = React.useMemo(
    () => composerSlashCommandsForSession(catalog, dedicatedToolAccess)
      .filter((command) => canOpenHistory || normalizedComposerSlashCommandId(command) !== "clean-history"),
    [canOpenHistory, catalog, dedicatedToolAccess],
  );
  // Resolve the command against the same owner used for the catalog.
  // 使用与指令目录相同的所有者来解析指令。
  const routeCommand = React.useCallback<ComposerSlashSession["routeCommand"]>((command) => {
    if (historyDialog && canOpenHistory && normalizedComposerSlashCommandId(command) === "clean-history") {
      historyDialog.setOpen(true);
      return { handled: true };
    }
    return onRouteCommand(command);
  }, [canOpenHistory, historyDialog, onRouteCommand]);
  const [settingsRequest, setSettingsRequest] = React.useState<ComposerSettingsRequest | null>(null);
  const openSettings = React.useCallback((section: ComposerSettingsSection | null) => {
    setSettingsRequest((current) => ({ section, nonce: (current?.nonce ?? 0) + 1 }));
  }, []);
  const openGoal = React.useCallback(() => void openGoalDialog(), [openGoalDialog]);
  return { commands, settingsRequest, openSettings, openGoal, routeCommand };
}
