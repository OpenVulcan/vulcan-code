import React from "react";
import { composerSlashCommandsForSession } from "@/components/composer/composer-model";
import type { ComposerSettingsRequest, ComposerSettingsSection } from "@/components/composer/composer-settings-request";
import { useDedicatedComposer } from "@/components/composer/dedicated/dedicated-session-controls";
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
}

// useComposerSlashSession hides frozen or unused commands in dedicated sessions and routes settings commands.
// useComposerSlashSession 在专职会话中隐藏已冻结或不使用的指令，并路由设置类指令。
export function useComposerSlashSession(
  catalog: ComposerSlashCommandSummary[],
  openGoalDialog: () => Promise<void> | void,
): ComposerSlashSession {
  const dedicatedToolAccess = useDedicatedComposer()?.toolAccess ?? null;
  const commands = React.useMemo(
    () => composerSlashCommandsForSession(catalog, dedicatedToolAccess),
    [catalog, dedicatedToolAccess],
  );
  const [settingsRequest, setSettingsRequest] = React.useState<ComposerSettingsRequest | null>(null);
  const openSettings = React.useCallback((section: ComposerSettingsSection | null) => {
    setSettingsRequest((current) => ({ section, nonce: (current?.nonce ?? 0) + 1 }));
  }, []);
  const openGoal = React.useCallback(() => void openGoalDialog(), [openGoalDialog]);
  return { commands, settingsRequest, openSettings, openGoal };
}
