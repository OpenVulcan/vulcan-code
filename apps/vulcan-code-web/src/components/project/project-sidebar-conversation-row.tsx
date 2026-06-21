import React from "react";
import {
  Archive,
  MessageSquareText,
  Pin,
  PinOff,
} from "lucide-react";

import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";
import { ProgressRing } from "@/components/ui/progress-ring";
import type { LocaleCode } from "@/i18n";
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
  // archiveConfirming tracks the inline archive confirmation state.
  // archiveConfirming 跟踪行内归档确认状态。
  const [archiveConfirming, setArchiveConfirming] = React.useState(false);
  const resetArchiveConfirming = React.useCallback(() => setArchiveConfirming(false), []);
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
          tooltip={`${props.conversation.title}\n${props.conversation.status}`}
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
          <span className="vcn-sidebar-label conversation-tree-label">
            <strong>{props.conversation.title}</strong>
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
  const isRunning = props.conversation.status.toLowerCase() === "running";
  if (props.archiveConfirming) {
    return <span className="conversation-tree-leading" />;
  }
  return (
    <span className={cn("conversation-tree-leading", isRunning && "running")}>
      <span className={cn("conversation-tree-leading-status", isRunning && "running")}>
        {isRunning ? (
          <span className="conversation-tree-status-mark" aria-label="会话正在运行">
            <ProgressRing aria-label="会话正在运行" className="conversation-tree-running-progress" size={13} value={null} />
          </span>
        ) : props.conversation.unread && !props.isSelected ? (
          <span className="conversation-tree-status-mark">
            <span className="conversation-tree-unread-dot" aria-label="未读" />
          </span>
        ) : null}
      </span>
      <span className={cn("conversation-tree-leading-actions", isRunning && "running")} aria-hidden="false">
        <span
          aria-label={props.isPinned ? "取消置顶对话" : "置顶对话"}
          className="conversation-tree-quick-action"
          onClick={(event) => {
            event.stopPropagation();
            props.onAction("pin", props.conversation);
          }}
          onMouseDown={(event) => event.preventDefault()}
          role="button"
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
          aria-label="确认归档对话"
          className="conversation-tree-confirm-action"
          onClick={(event) => {
            event.stopPropagation();
            props.onAction("archive", props.conversation);
          }}
          role="button"
        >
          确认
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
          aria-label="归档对话"
          className="conversation-tree-quick-action"
          onClick={(event) => {
            event.stopPropagation();
            props.onRequestArchiveConfirm();
          }}
          onMouseDown={(event) => event.preventDefault()}
          role="button"
        >
          <Archive size={13} />
        </span>
      </span>
    </span>
  );
}
