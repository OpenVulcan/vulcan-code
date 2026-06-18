import React from "react";
import {
  CheckCircle2,
  Circle,
  ListChecks,
  LoaderCircle,
  Maximize2,
  Minimize2,
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { SessionTodoPlanItemSummary, SessionTodoPlanSummary } from "@/types";

// ConversationSessionTodoPanel renders the session TODO plan inside the main conversation surface.
// ConversationSessionTodoPanel 在主会话界面内渲染当前会话的 TODO 计划。
export function ConversationSessionTodoPanel(props: {
  // Current TODO plan summary projected from the selected session runtime state.
  // 从当前选中会话运行时状态投影出的 TODO 计划摘要。
  todoPlan: SessionTodoPlanSummary | null;
  // Whether the TODO panel is currently expanded.
  // TODO 面板当前是否展开。
  expanded: boolean;
  // Updates the TODO panel expanded state.
  // 更新 TODO 面板展开状态。
  onExpandedChange: (expanded: boolean) => void;
}) {
  if (!props.todoPlan || props.todoPlan.plan.length === 0) {
    return null;
  }

  if (!props.expanded) {
    return (
      <button
        aria-label="展开会话 TODO"
        className="workspace-skills-menu-collapsed conversation-session-todo-collapsed"
        onClick={() => props.onExpandedChange(true)}
        title="展开会话 TODO"
        type="button"
      >
        <ListChecks size={15} />
        <span>TODO</span>
        <strong>{props.todoPlan.completedCount}/{props.todoPlan.totalCount}</strong>
        <Maximize2 size={13} />
      </button>
    );
  }

  return (
    <aside className="workspace-skills-menu conversation-session-todo-menu" aria-label="会话 TODO 菜单">
      <div className="workspace-skills-menu-header conversation-session-todo-header">
        <span className="conversation-session-todo-title">
          <ListChecks size={15} />
          TODO
        </span>
        <span className="conversation-session-todo-header-actions">
          <strong>{props.todoPlan.completedCount}/{props.todoPlan.totalCount}</strong>
          <button
            aria-label="折叠会话 TODO"
            className="workspace-skills-menu-icon-button"
            onClick={() => props.onExpandedChange(false)}
            title="折叠会话 TODO"
            type="button"
          >
            <Minimize2 size={13} />
          </button>
        </span>
      </div>
      {props.todoPlan.explanation ? (
        <p className="conversation-session-todo-explanation">{props.todoPlan.explanation}</p>
      ) : null}
      <div className="workspace-skills-menu-list conversation-session-todo-list">
        {props.todoPlan.plan.map((item, index) => (
          <ConversationSessionTodoRow item={item} key={`${index}-${item.status}-${item.step}`} />
        ))}
      </div>
    </aside>
  );
}

// ConversationSessionTodoRow renders one TODO item with its current model-authored status.
// ConversationSessionTodoRow 使用模型写入的当前状态渲染单条 TODO。
function ConversationSessionTodoRow(props: {
  // TODO item rendered by this row.
  // 当前行渲染的 TODO 条目。
  item: SessionTodoPlanItemSummary;
}) {
  return (
    <div className={cn("workspace-skills-menu-row conversation-session-todo-row", `status-${props.item.status}`)}>
      {conversationSessionTodoStatusIcon(props.item.status)}
      <span>{props.item.step}</span>
    </div>
  );
}

// conversationSessionTodoStatusIcon returns the compact icon for one TODO status.
// conversationSessionTodoStatusIcon 返回单个 TODO 状态对应的紧凑图标。
function conversationSessionTodoStatusIcon(status: SessionTodoPlanItemSummary["status"]): React.ReactNode {
  if (status === "completed") {
    return <CheckCircle2 size={15} />;
  }
  if (status === "in_progress") {
    return <LoaderCircle className="spin" size={15} />;
  }
  return <Circle size={15} />;
}
