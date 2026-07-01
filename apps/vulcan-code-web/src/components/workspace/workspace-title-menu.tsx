import React from "react";
import {
  Archive,
  Copy,
  ExternalLink,
  FileText,
  FolderOpen,
  GitBranch,
  Link,
  MailOpen,
  MoreHorizontal,
  Pencil,
  Pin,
} from "lucide-react";
import { toast } from "sonner";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useI18n } from "@/i18n";
import type { ConversationMenuAction } from "@/components/project/project-sidebar-types";
import type { ConversationSummary } from "@/types";

// WorkspaceTitleMenu renders title-adjacent overflow actions without joining the native drag region.
// WorkspaceTitleMenu 在标题旁渲染溢出操作，并避免加入原生拖动区域。
export function WorkspaceTitleMenu(props: {
  // Title text copied by the generic title action.
  // 通用标题操作复制的标题文本。
  title: string;
  // Conversation owning the full conversation action menu.
  // 拥有完整会话操作菜单的会话。
  conversation?: ConversationSummary;
  // Whether the owning conversation is pinned.
  // 所属会话是否已置顶。
  isPinned?: boolean;
  // Handles one conversation-level action when a conversation exists.
  // 当会话存在时处理单个会话级操作。
  onConversationMenuAction?: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
}) {
  // t resolves stable menu and toast copy for the active locale.
  // t 根据当前语言解析稳定的菜单与提示文案。
  const { t } = useI18n();
  // hasConversationMenu determines whether conversation-only commands can be shown.
  // hasConversationMenu 判断是否可以展示仅会话可用的命令。
  const hasConversationMenu = Boolean(props.conversation && props.onConversationMenuAction);

  // handleCopyTitle copies the visible header title and reports completion.
  // handleCopyTitle 复制当前可见标题并反馈完成状态。
  const handleCopyTitle = React.useCallback(async () => {
    try {
      await navigator.clipboard.writeText(props.title);
      toast.success(t("workbenchConversation.copiedTitle"));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }, [props.title, t]);

  // runConversationAction forwards a selected menu action to the workbench owner.
  // runConversationAction 将选中的菜单动作转发给工作台宿主。
  const runConversationAction = React.useCallback(
    (action: ConversationMenuAction) => {
      if (!props.conversation || !props.onConversationMenuAction) {
        return;
      }
      props.onConversationMenuAction(action, props.conversation);
    },
    [props],
  );

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          aria-label={t("conversationMenu.moreTitleActions")}
          className="workspace-title-menu-trigger"
          title={t("conversationMenu.moreTitleActions")}
          type="button"
        >
          <MoreHorizontal size={15} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="conversation-title-menu" side="bottom" sideOffset={7}>
        <DropdownMenuItem onSelect={() => void handleCopyTitle()}>
          <Copy size={15} />
          <span>{t("conversationMenu.copyTitle")}</span>
        </DropdownMenuItem>
        {hasConversationMenu ? (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => runConversationAction("pin")}>
              <Pin size={15} />
              <span>{props.isPinned ? t("conversationMenu.unpin") : t("conversationMenu.pin")}</span>
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => runConversationAction("rename")}>
              <Pencil size={15} />
              <span>{t("conversationMenu.rename")}</span>
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => runConversationAction("archive")}>
              <Archive size={15} />
              <span>{t("conversationMenu.archive")}</span>
            </DropdownMenuItem>
            <DropdownMenuItem disabled>
              <MailOpen size={15} />
              <span>{t("conversationMenu.markUnread")}</span>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => runConversationAction("open-directory")}>
              <FolderOpen size={15} />
              <span>{t("conversationMenu.openDirectory")}</span>
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => runConversationAction("copy-workdir")}>
              <Copy size={15} />
              <span>{t("conversationMenu.copyWorkspace")}</span>
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => runConversationAction("copy-session-id")}>
              <FileText size={15} />
              <span>{t("conversationMenu.copySessionId")}</span>
            </DropdownMenuItem>
            <DropdownMenuItem disabled>
              <Link size={15} />
              <span>{t("conversationMenu.copyDeepLink")}</span>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled>
              <GitBranch size={15} />
              <span>{t("conversationMenu.forkLocal")}</span>
            </DropdownMenuItem>
            <DropdownMenuItem disabled>
              <GitBranch size={15} />
              <span>{t("conversationMenu.forkWorktree")}</span>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled>
              <ExternalLink size={15} />
              <span>{t("conversationMenu.openNewWindow")}</span>
            </DropdownMenuItem>
          </>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
