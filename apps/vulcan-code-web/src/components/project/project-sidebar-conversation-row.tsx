import React from "react";
import {
  Archive,
  Bot,
  CircleHelp,
  ClipboardList,
  MessageSquareText,
  Pin,
  PinOff,
} from "lucide-react";

import { SidebarMenuItem } from "@/ui-kit/shell/sidebar";
import { useI18n, type LocaleCode } from "@/i18n";
import type { ConversationSummary, ProjectSummary } from "@/types";

import { ConversationContextMenu } from "./project-sidebar-menus";
import type { ConversationMenuAction } from "./project-sidebar-types";
import type { SortableRowBinding } from "./project-sidebar-sortable";
import { formatConversationRelativeTime } from "./project-sidebar-time";

// ConversationRowVariant selects the compact tree line or the two-line session-flow entry.
// ConversationRowVariant 选择紧凑树行或两行的会话流条目。
export type ConversationRowVariant = "tree" | "flow";

// ConversationTreeRow renders one conversation: a selectable row plus pin and archive actions beside it.
// The actions are siblings of the row button, so no interactive element is nested inside another.
// ConversationTreeRow 渲染单个会话：可选择的行及其旁的置顶与归档操作。
// 操作与行按钮同级，因此不存在嵌套的交互元素。
export function ConversationTreeRow(props: {
  // Conversation summary shown by the row.
  // 当前行展示的会话摘要。
  conversation: ConversationSummary;
  // Whether the row belongs to the pinned section, which leads with a session mark instead of the state.
  // 当前行是否属于置顶分区；置顶行以会话标记代替状态开头。
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
  // Row layout; the tree variant is the default.
  // 行布局；默认为树行。
  variant?: ConversationRowVariant;
  // Joins a sortable list, as pinned rows do; the grip follows the row actions.
  // 加入可排序列表（置顶行即如此）；手柄位于行操作之后。
  sortable?: SortableRowBinding;
}) {
  const { t } = useI18n();
  // archiveConfirming tracks the inline archive confirmation state.
  // archiveConfirming 跟踪行内归档确认状态。
  const [archiveConfirming, setArchiveConfirming] = React.useState(false);
  const resetArchiveConfirming = React.useCallback(() => setArchiveConfirming(false), []);
  const variant = props.variant ?? "tree";
  const state = conversationRowState(props.conversation, props.isSelected);
  const stateLabel = conversationStateLabel(state, props.conversation, t);
  // Dedicated sessions carry a kind mark so they read apart from ordinary conversations.
  // 专职会话带有种类标记，以便与普通对话区分。
  const dedicated = props.conversation.dedicated === true;
  const kindLabel = dedicated ? t("composer.draft.kindDedicated") : null;
  const time = formatConversationRelativeTime(props.conversation, props.nowMs, props.locale);
  const metaParts = [kindLabel, stateLabel].filter(Boolean);
  return (
    <SidebarMenuItem
      className="vk-tree-item"
      data-confirming={archiveConfirming ? "" : undefined}
      data-dragging={props.sortable?.dragging ? "" : undefined}
      onMouseLeave={resetArchiveConfirming}
      ref={props.sortable?.setNodeRef}
      style={props.sortable?.style}
    >
      <ConversationContextMenu
        conversation={props.conversation}
        isPinned={props.isPinned}
        onAction={props.onAction}
      >
        <button
          aria-current={props.isSelected ? "true" : undefined}
          className="vk-tree-session"
          data-pinned={props.compactPinned ? "" : undefined}
          data-state={state ?? undefined}
          data-variant={variant}
          onClick={() => props.onSelectConversation(props.conversation.id)}
          title={[props.conversation.title, kindLabel, stateLabel].filter(Boolean).join("\n")}
          type="button"
        >
          <span className="vk-tree-lead">
            {props.compactPinned ? <MessageSquareText aria-hidden="true" /> : <ConversationStateMark label={stateLabel} state={state} />}
          </span>
          <span className="vk-tree-title">
            {dedicated ? <Bot aria-label={kindLabel ?? undefined} className="vk-tree-kind" role="img" /> : null}
            <span>{props.conversation.title}</span>
          </span>
          <small className="vk-tree-time">{time}</small>
          {variant === "flow" && metaParts.length > 0 ? <span className="vk-tree-meta">{metaParts.join(" · ")}</span> : null}
        </button>
      </ConversationContextMenu>
      <span className="vk-tree-actions">
        {archiveConfirming ? (
          <button
            aria-label={t("conversationRow.confirmArchive")}
            className="vk-btn"
            data-size="sm"
            data-variant="danger"
            onClick={() => props.onAction("archive", props.conversation)}
            type="button"
          >
            {t("projectDialog.confirm")}
          </button>
        ) : (
          <>
            <button
              aria-label={props.isPinned ? t("conversationMenu.unpin") : t("conversationMenu.pin")}
              className="vk-icon-btn"
              data-size="sm"
              onClick={() => props.onAction("pin", props.conversation)}
              title={props.isPinned ? t("conversationMenu.unpin") : t("conversationMenu.pin")}
              type="button"
            >
              {props.isPinned ? <PinOff /> : <Pin />}
            </button>
            {state === "running" ? null : (
              <button
                aria-label={t("conversationMenu.archive")}
                className="vk-icon-btn"
                data-size="sm"
                onClick={() => setArchiveConfirming(true)}
                title={t("conversationMenu.archive")}
                type="button"
              >
                <Archive />
              </button>
            )}
            {props.sortable?.grip}
          </>
        )}
      </span>
    </SidebarMenuItem>
  );
}

// ConversationRowState is the one state a row signals, in priority order: a question, a plan, running, unread.
// ConversationRowState 是行展示的单一状态，按优先级依次为：问题、计划、运行中、未读。
type ConversationRowState = "ask" | "plan" | "running" | "unread" | null;

// conversationRowState picks the state the row signals; a selected row is never unread.
// conversationRowState 选出行要展示的状态；选中的行不显示未读。
function conversationRowState(conversation: ConversationSummary, isSelected: boolean): ConversationRowState {
  const attention = conversation.attention?.kind ?? null;
  if (attention === "ask") return "ask";
  if (attention === "plan_confirm") return "plan";
  if (conversation.status.toLowerCase() === "running") return "running";
  if ((conversation.unread || attention) && !isSelected) return "unread";
  return null;
}

// conversationStateLabel names the signalled state for tooltips, assistive technology and the flow meta line.
// conversationStateLabel 为提示、辅助技术与会话流元信息行命名所展示的状态。
function conversationStateLabel(
  state: ConversationRowState,
  conversation: ConversationSummary,
  t: ReturnType<typeof useI18n>["t"],
): string | null {
  switch (state) {
    case "ask": return t("conversationRow.askAttention");
    case "plan": return t("conversationRow.planAttention");
    case "running": return t("conversationRow.running");
    case "unread": return conversation.attention?.label ?? t("conversationRow.unread");
    default: return null;
  }
}

// ConversationStateMark draws the signalled state: a live ember dot while running, an info dot when unread,
// and a toned icon when the session waits for an answer or a plan decision.
// ConversationStateMark 绘制所展示的状态：运行中为跳动的余烬点，未读为信息点，
// 等待回答或计划确认时为带色调的图标。
function ConversationStateMark(props: { state: ConversationRowState; label: string | null }) {
  switch (props.state) {
    case "ask": return <CircleHelp aria-label={props.label ?? undefined} data-tone="warn" role="img" />;
    case "plan": return <ClipboardList aria-label={props.label ?? undefined} data-tone="signal" role="img" />;
    case "running": return <span aria-label={props.label ?? undefined} className="vk-dot" data-live="" data-tone="ember" role="img" />;
    case "unread": return <span aria-label={props.label ?? undefined} className="vk-dot" data-tone="info" role="img" />;
    default: return null;
  }
}
