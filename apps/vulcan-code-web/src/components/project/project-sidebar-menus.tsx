import React from "react";
import {
  Archive,
  Copy,
  ExternalLink,
  FileText,
  FolderCog,
  FolderOpen,
  GitBranch,
  Link,
  MailOpen,
  Palette,
  PanelTopOpen,
  Pencil,
  Pin,
  PinOff,
  SquarePen,
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

// CollapsedProjectMenu renders project actions as a hover flyout while the sidebar is collapsed.
// CollapsedProjectMenu 在侧栏折叠时以悬停浮层展示项目操作。
export function CollapsedProjectMenu(props: {
  // Trigger row reused from the expanded project entry.
  // 复用展开项目入口的触发行。
  children: React.ReactNode;
  // Project that owns the collapsed entry.
  // 折叠入口所属项目。
  project: ProjectSummary;
  // Conversations shown below the create command.
  // 显示在新建命令下方的会话列表。
  conversations: ConversationSummary[];
  // Currently selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId: string | null;
  // Opens a new draft conversation for the project.
  // 为项目打开一个新的草稿会话。
  onCreateConversation: (project: ProjectSummary) => void;
  // Selects one existing conversation.
  // 选择一个已有会话。
  onSelectConversation: (conversationId: string) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // anchorRef points at the collapsed folder row used to place the hover flyout.
  // anchorRef 指向折叠后的文件夹行，用于定位悬停浮层。
  const anchorRef = React.useRef<HTMLDivElement | null>(null);
  // closeTimerRef keeps the flyout alive while the pointer moves from folder to menu.
  // closeTimerRef 在指针从文件夹移动到菜单时保持浮层不被立刻关闭。
  const closeTimerRef = React.useRef<number | null>(null);
  // flyoutOpen tracks whether the hover flyout is currently visible.
  // flyoutOpen 跟踪悬停浮层当前是否可见。
  const [flyoutOpen, setFlyoutOpen] = React.useState(false);
  // flyoutPosition stores viewport coordinates so the menu is not clipped by sidebar scrolling.
  // flyoutPosition 保存视口坐标，避免菜单被侧栏滚动容器裁剪。
  const [flyoutPosition, setFlyoutPosition] = React.useState<{ left: number; top: number } | null>(null);

  // clearCloseTimer cancels a pending delayed close when the pointer returns.
  // clearCloseTimer 在指针返回时取消等待中的延迟关闭。
  const clearCloseTimer = React.useCallback(() => {
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);

  // openFlyout positions and shows the collapsed project flyout.
  // openFlyout 定位并显示折叠项目浮层。
  const openFlyout = React.useCallback(() => {
    clearCloseTimer();
    const rect = anchorRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }
    setFlyoutPosition({
      left: rect.right + 8,
      top: rect.top,
    });
    setFlyoutOpen(true);
  }, [clearCloseTimer]);

  // scheduleClose delays closing so pointer travel between folder and menu feels stable.
  // scheduleClose 延迟关闭浮层，让指针在文件夹与菜单间移动时保持稳定。
  const scheduleClose = React.useCallback(() => {
    clearCloseTimer();
    closeTimerRef.current = window.setTimeout(() => {
      setFlyoutOpen(false);
    }, 140);
  }, [clearCloseTimer]);

  React.useEffect(() => clearCloseTimer, [clearCloseTimer]);

  // selectConversation closes the flyout before switching to a conversation.
  // selectConversation 在切换会话前关闭浮层。
  const selectConversation = React.useCallback(
    (conversationId: string) => {
      setFlyoutOpen(false);
      props.onSelectConversation(conversationId);
    },
    [props],
  );

  // createConversation closes the flyout before opening a draft conversation.
  // createConversation 在打开草稿会话前关闭浮层。
  const createConversation = React.useCallback(() => {
    setFlyoutOpen(false);
    props.onCreateConversation(props.project);
  }, [props]);

  return (
    <>
      <div
        className="collapsed-project-hover-root"
        onMouseEnter={openFlyout}
        onMouseLeave={scheduleClose}
        ref={anchorRef}
      >
        {props.children}
      </div>
      {flyoutOpen && flyoutPosition ? (
        <div
          aria-label={t("projectMenu.conversationsAria").replace("{project}", props.project.name)}
          className="collapsed-project-flyout"
          onMouseEnter={openFlyout}
          onMouseLeave={scheduleClose}
          role="menu"
          style={{ left: flyoutPosition.left, top: flyoutPosition.top }}
        >
          <div className="collapsed-project-flyout-title" role="presentation">
            {props.project.name}
          </div>
          <button className="collapsed-project-flyout-item" onClick={createConversation} role="menuitem" type="button">
            <SquarePen size={15} />
            <span>{t("projectMenu.newConversation")}</span>
          </button>
          <div className="collapsed-project-flyout-separator" role="separator" />
          {props.conversations.length > 0 ? (
            props.conversations.map((conversation) => (
              <button
                className={cn(
                  "collapsed-project-flyout-item",
                  props.selectedConversationId === conversation.id && "active",
                )}
                key={conversation.id}
                onClick={() => selectConversation(conversation.id)}
                role="menuitem"
                type="button"
              >
                <span className="collapsed-project-menu-title">{conversation.title}</span>
              </button>
            ))
          ) : (
            <div className="collapsed-project-flyout-item disabled" role="presentation">
              <span>{t("projectMenu.noConversations")}</span>
            </div>
          )}
        </div>
      ) : null}
    </>
  );
}

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
        <ContextMenuItem disabled>
          <PanelTopOpen size={15} />
          <span>{t("projectMenu.createWorktree")}</span>
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
        <ContextMenuItem disabled>
          <MailOpen size={15} />
          <span>{t("conversationMenu.markUnread")}</span>
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
        <ContextMenuItem disabled>
          <Link size={15} />
          <span>{t("conversationMenu.copyDeepLink")}</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled>
          <GitBranch size={15} />
          <span>{t("conversationMenu.forkLocal")}</span>
        </ContextMenuItem>
        <ContextMenuItem disabled>
          <GitBranch size={15} />
          <span>{t("conversationMenu.forkWorktree")}</span>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled>
          <ExternalLink size={15} />
          <span>{t("conversationMenu.openNewWindow")}</span>
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
