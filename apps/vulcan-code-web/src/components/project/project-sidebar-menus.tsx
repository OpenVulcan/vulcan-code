import React from "react";
import {
  Archive,
  Copy,
  FileText,
  FolderCog,
  FolderOpen,
  Palette,
  Pencil,
  Pin,
  PinOff,
  SquarePen,
  TerminalSquare,
  Trash2,
} from "lucide-react";

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ConversationSummary, ProjectSummary } from "@/types";
import type { ConversationMenuAction, ProjectMenuAction } from "./project-sidebar-types";

// ProjectContextMenu renders project-level right-click commands.
// ProjectContextMenu 渲染项目级右键命令。
export function ProjectContextMenu(props: {
  // Project that receives the selected action.
  // 接收所选操作的项目。
  project: ProjectSummary;
  // Trigger element wrapped by the context menu.
  // 由右键菜单包裹的触发元素。
  children: React.ReactNode;
  // Handles one project menu action.
  // 处理单个项目菜单动作。
  onAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{props.children}</ContextMenuTrigger>
      <ContextMenuContent className="project-context-menu">
        <ContextMenuItem onSelect={() => props.onAction("toggle-pin", props.project)}>
          <PinOff size={15} />
          <span>{props.project.pinned ? t("projectMenu.unpin") : t("projectMenu.pin")}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("open-directory", props.project)}>
          <FolderOpen size={15} />
          <span>{t("projectMenu.openDirectory")}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("open-terminal", props.project)}>
          <TerminalSquare size={15} />
          <span>{t("projectMenu.openTerminal")}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("rename", props.project)}>
          <Pencil size={15} />
          <span>{t("projectMenu.renameProject")}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("set-icon", props.project)}>
          <Palette size={15} />
          <span>{t("projectMenu.setIcon")}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("archive-conversations", props.project)}>
          <Archive size={15} />
          <span>{t("projectMenu.archiveConversations")}</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => props.onAction("remove-project", props.project)} variant="destructive">
          <Trash2 size={15} />
          <span>{t("projectMenu.removeProject")}</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => props.onAction("settings", props.project)}>
          <FolderCog size={15} />
          <span>{t("projectMenu.projectSettings")}</span>
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

// ConversationContextMenu renders conversation-level right-click commands.
// ConversationContextMenu 渲染会话级右键命令。
export function ConversationContextMenu(props: {
  // Conversation that receives the selected action.
  // 接收所选操作的会话。
  conversation: ConversationSummary;
  // Trigger element wrapped by the context menu.
  // 由右键菜单包裹的触发元素。
  children: React.ReactNode;
  // Whether the conversation is currently pinned.
  // 当前会话是否置顶。
  isPinned: boolean;
  // Handles one conversation menu action.
  // 处理单个会话菜单动作。
  onAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // isRunning keeps every archive entrypoint aligned with the backend lifecycle boundary.
  // isRunning 让所有归档入口与后端生命周期边界保持一致。
  const isRunning = props.conversation.status.toLowerCase() === "running";
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{props.children}</ContextMenuTrigger>
      <ContextMenuContent className="conversation-context-menu">
        <ContextMenuItem onSelect={() => props.onAction("pin", props.conversation)}>
          <Pin size={15} />
          <span>{props.isPinned ? t("conversationMenu.unpin") : t("conversationMenu.pin")}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("rename", props.conversation)}>
          <Pencil size={15} />
          <span>{t("conversationMenu.rename")}</span>
        </ContextMenuItem>
        <ContextMenuItem
          disabled={isRunning}
          onSelect={() => props.onAction("archive", props.conversation)}
        >
          <Archive size={15} />
          <span>{t("conversationMenu.archive")}</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => props.onAction("open-directory", props.conversation)}>
          <FolderOpen size={15} />
          <span>{t("conversationMenu.openDirectory")}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("copy-title", props.conversation)}>
          <Copy size={15} />
          <span>{t("conversationMenu.copyTitle")}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("copy-workdir", props.conversation)}>
          <Copy size={15} />
          <span>{t("conversationMenu.copyWorkspace")}</span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => props.onAction("copy-session-id", props.conversation)}>
          <FileText size={15} />
          <span>{t("conversationMenu.copySessionId")}</span>
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
