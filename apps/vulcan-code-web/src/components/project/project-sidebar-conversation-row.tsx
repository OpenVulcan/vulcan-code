import React from "react";
import {
  Archive,
  Bot,
  CircleHelp,
  ClipboardList,
  LoaderCircle,
  MessageSquareText,
  Pin,
  PinOff,
} from "lucide-react";

import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";
import { useI18n, type LocaleCode } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ConversationSummary, ProjectSummary } from "@/types";

import { ConversationContextMenu } from "./project-sidebar-menus";
import type { ConversationMenuAction } from "./project-sidebar-types";
import { formatConversationRelativeTime } from "./project-sidebar-time";

// ConversationTreeRow renders one interactive conversation row with inline quick actions.
// ConversationTreeRow 渲染带行内快捷操作的单个交互式会话行。
export function ConversationTreeRow(props: {
  // Conversation summary shown by the row.
  // 当前行展示的会话摘要。
  conversation: ConversationSummary;
  // Whether the row belongs to the pinned section.
  // 当前行是否属于置顶分区。
  compactPinned: boolean;
  // Whether the conversation is pinned.
  // 当前会话是否已置顶。
  isPinned: boolean;
  // Whether the conversation is currently selected.
  // 当前会话是否被选中。
  isSelected: boolean;
  // Active locale used for compact time labels.
  // 用于紧凑时间标签的当前语言。
  locale: LocaleCode;
  // Current timestamp used for relative time rendering.
  // 用于渲染相对时间的当前时间戳。
  nowMs: number;
  // Handles row-level menu and quick actions.
  // 处理行级菜单和快捷操作。
  onAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Selects this conversation.
  // 选择当前会话。
  onSelectConversation: (conversationId: string) => void;
  // Optional owning project used by pinned conversation rows.
  // 置顶会话行使用的可选所属项目。
  project?: ProjectSummary;
}) {
  const { t } = useI18n();
  // archiveConfirming tracks the inline archive confirmation state.
  // archiveConfirming 跟踪行内归档确认状态。
  const [archiveConfirming, setArchiveConfirming] = React.useState(false);
  const resetArchiveConfirming = React.useCallback(() => setArchiveConfirming(false), []);
  // Dedicated sessions carry a kind mark so they read apart from ordinary conversations.
  // 专职会话带有种类标记，以便与普通对话区分。
  const dedicated = props.conversation.dedicated === true;
  const kindLabel = dedicated ? t("composer.draft.kindDedicated") : null;
  return (
    <SidebarMenuItem key={props.conversation.id} onMouseLeave={resetArchiveConfirming}>
      <ConversationContextMenu
        conversation={props.conversation}
        isPinned={props.isPinned}
        onAction={props.onAction}
      >
        <SidebarMenuButton
          active={props.isSelected}
          className={cn(
            "conversation-tree-button",
            props.compactPinned && "pinned-conversation-button",
            props.compactPinned && "with-pinned-icon",
          )}
          onClick={() => props.onSelectConversation(props.conversation.id)}
          tooltip={[props.conversation.title, kindLabel, props.conversation.status].filter(Boolean).join("\n")}
        >
          {props.compactPinned ? (
            <span className="conversation-tree-project-icon">
              <MessageSquareText size={14} />
            </span>
          ) : null}
          <ConversationRowLeading
            archiveConfirming={archiveConfirming}
            conversation={props.conversation}
            isPinned={props.isPinned}
            isSelected={props.isSelected}
            onAction={props.onAction}
          />
          <span className={cn("vcn-sidebar-label conversation-tree-label", dedicated && "dedicated")}>
            {kindLabel ? (
              <Bot aria-label={kindLabel} className="shrink-0 text-primary" role="img" size={13} />
            ) : null}
            <strong className="min-w-0 flex-1">{props.conversation.title}</strong>
          </span>
          <ConversationRowTrailing
            archiveConfirming={archiveConfirming}
            conversation={props.conversation}
            locale={props.locale}
            nowMs={props.nowMs}
            onAction={props.onAction}
            onRequestArchiveConfirm={() => setArchiveConfirming(true)}
          />
        </SidebarMenuButton>
      </ConversationContextMenu>
    </SidebarMenuItem>
  );
}

// ConversationRowLeading renders the left-side conversation state and pin action.
// ConversationRowLeading 渲染左侧会话状态与置顶操作。
function ConversationRowLeading(props: {
  // Whether inline archive confirmation is active.
  // 行内归档确认是否激活。
  archiveConfirming: boolean;
  // Conversation summary used to decide the row status indicator.
  // 用于决定行状态指示器的会话摘要。
  conversation: ConversationSummary;
  // Whether the conversation is pinned.
  // 当前会话是否置顶。
  isPinned: boolean;
  // Whether this conversation is currently selected.
  // 当前会话是否正在被选中。
  isSelected: boolean;
  // Handles row-level pin actions.
  // 处理行级置顶操作。
  onAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
}) {
  const { t } = useI18n();
  const isRunning = props.conversation.status.toLowerCase() === "running";
  const attentionKind = props.conversation.attention?.kind ?? null;
  const attentionLabel = attentionKind === "ask"
    ? t("conversationRow.askAttention")
    : attentionKind === "plan_confirm"
      ? t("conversationRow.planAttention")
      : attentionKind
        ? props.conversation.attention?.label ?? t("conversationRow.unread")
        : null;
  if (props.archiveConfirming) {
    return <span className="conversation-tree-leading" />;
  }
  return (
    <span className={cn("conversation-tree-leading", isRunning && "running")}>
      <span className={cn("conversation-tree-leading-status", isRunning && "running")}>
        {attentionKind === "ask" ? (
          <span className="conversation-tree-status-mark" title={attentionLabel ?? undefined}>
            <CircleHelp aria-label={attentionLabel ?? t("conversationRow.askAttention")} className="conversation-tree-attention-icon ask" size={14} />
          </span>
        ) : attentionKind === "plan_confirm" ? (
          <span className="conversation-tree-status-mark" title={attentionLabel ?? undefined}>
            <ClipboardList aria-label={attentionLabel ?? t("conversationRow.planAttention")} className="conversation-tree-attention-icon plan" size={14} />
          </span>
        ) : isRunning ? (
          <span className="conversation-tree-status-mark" aria-label={t("conversationRow.running")}>
            <LoaderCircle aria-label={t("conversationRow.running")} className="conversation-tree-running-spinner spin" size={14} />
          </span>
        ) : props.conversation.unread && !props.isSelected ? (
          <span className="conversation-tree-status-mark">
            <span className="conversation-tree-unread-dot" aria-label={t("conversationRow.unread")} />
          </span>
        ) : null}
      </span>
      <span className={cn("conversation-tree-leading-actions", isRunning && "running")} aria-hidden="false">
        <span
          aria-label={props.isPinned ? t("conversationMenu.unpin") : t("conversationMenu.pin")}
          className="conversation-tree-quick-action focus-visible:outline-2 focus-visible:outline-ring"
          onClick={(event) => {
            event.stopPropagation();
            props.onAction("pin", props.conversation);
          }}
          onMouseDown={(event) => event.preventDefault()}
          role="button"
          tabIndex={0}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") { event.preventDefault(); event.stopPropagation(); event.currentTarget.click(); }
          }}
        >
          {props.isPinned ? <PinOff size={13} /> : <Pin size={13} />}
        </span>
      </span>
    </span>
  );
}

// ConversationRowTrailing renders the right-side time, archive action, and archive confirmation.
// ConversationRowTrailing 渲染右侧时间、归档操作与归档确认。
function ConversationRowTrailing(props: {
  // Whether inline archive confirmation is active.
  // 行内归档确认是否激活。
  archiveConfirming: boolean;
  // Conversation summary used to render the right-side row state.
  // 用于渲染右侧行状态的会话摘要。
  conversation: ConversationSummary;
  // Active locale used for compact time labels.
  // 用于紧凑时间标签的当前语言。
  locale: LocaleCode;
  // Current timestamp used for relative time rendering.
  // 用于渲染相对时间的当前时间戳。
  nowMs: number;
  // Handles row-level archive actions.
  // 处理行级归档操作。
  onAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Enters inline archive confirmation mode.
  // 进入行内归档确认模式。
  onRequestArchiveConfirm: () => void;
}) {
  const { t } = useI18n();
  const isRunning = props.conversation.status.toLowerCase() === "running";
  if (isRunning) {
    return <span className="conversation-tree-trailing running" />;
  }
  if (props.archiveConfirming) {
    return (
      <span
        className="conversation-tree-trailing confirming"
        onClick={(event) => event.stopPropagation()}
        onMouseDown={(event) => event.preventDefault()}
      >
        <span
          aria-label={t("conversationRow.confirmArchive")}
          className="conversation-tree-confirm-action focus-visible:outline-2 focus-visible:outline-ring"
          onClick={(event) => {
            event.stopPropagation();
            props.onAction("archive", props.conversation);
          }}
          role="button"
          tabIndex={0}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") { event.preventDefault(); event.stopPropagation(); event.currentTarget.click(); }
          }}
        >
          {t("projectDialog.confirm")}
        </span>
      </span>
    );
  }
  return (
    <span className="conversation-tree-trailing">
      <span className="conversation-tree-status">
        {formatConversationRelativeTime(props.conversation, props.nowMs, props.locale)}
      </span>
      <span className="conversation-tree-quick-actions" aria-hidden="false">
        <span
          aria-label={t("conversationMenu.archive")}
          className="conversation-tree-quick-action focus-visible:outline-2 focus-visible:outline-ring"
          onClick={(event) => {
            event.stopPropagation();
            props.onRequestArchiveConfirm();
          }}
          onMouseDown={(event) => event.preventDefault()}
          role="button"
          tabIndex={0}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") { event.preventDefault(); event.stopPropagation(); event.currentTarget.click(); }
          }}
        >
          <Archive size={13} />
        </span>
      </span>
    </span>
  );
}
