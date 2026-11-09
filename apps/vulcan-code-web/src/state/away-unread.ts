import React from "react";

import type { ConversationSummary } from "@/types";

// The service treats the selected conversation as read, so a run that settles there while the window is
// unattended leaves no unread mark. This client-only store holds those marks until the user interacts again.
// 服务端把选中会话视为已读，因此窗口无人关注时在其中结束的运行不会留下未读标记。
// 此纯客户端存储保存这些标记，直到用户再次操作。

// SelectedRunObservation is the last observed run state of the selected conversation.
// SelectedRunObservation 是最近一次观察到的选中会话运行状态。
export interface SelectedRunObservation {
  conversationId: string | null;
  running: boolean;
}

// INITIAL_SELECTED_RUN_OBSERVATION has no selection, so the first observation never reports a settle.
// INITIAL_SELECTED_RUN_OBSERVATION 没有选中会话，因此首次观察绝不会报告结束。
export const INITIAL_SELECTED_RUN_OBSERVATION: SelectedRunObservation = { conversationId: null, running: false };

// conversationIsRunning reads the run state the sidebar row signals.
// conversationIsRunning 读取侧栏行所展示的运行状态。
export function conversationIsRunning(conversation: ConversationSummary): boolean {
  return conversation.status.toLowerCase() === "running";
}

// observeSelectedRun compares the selected conversation with the previous observation; `settledWhileAway` names the
// conversation whose run finished while it stayed selected and the window was unattended.
// observeSelectedRun 将选中会话与上次观察比较；`settledWhileAway` 指明保持选中且窗口无人关注期间运行结束的会话。
export function observeSelectedRun(
  previous: SelectedRunObservation,
  selectedConversationId: string | null,
  conversations: readonly ConversationSummary[],
  attended: boolean,
): { next: SelectedRunObservation; settledWhileAway: string | null } {
  const selected = selectedConversationId === null
    ? undefined
    : conversations.find((conversation) => conversation.id === selectedConversationId);
  const next: SelectedRunObservation = {
    conversationId: selected ? selected.id : null,
    running: selected ? conversationIsRunning(selected) : false,
  };
  const settled = next.conversationId !== null
    && previous.conversationId === next.conversationId
    && previous.running
    && !next.running;
  return { next, settledWhileAway: settled && !attended ? next.conversationId : null };
}

// awayUnreadIds holds conversations that settled while the window was unattended.
// awayUnreadIds 保存窗口无人关注期间结束的会话。
let awayUnreadIds: ReadonlySet<string> = new Set();
// listeners are notified after the mark set changes.
// listeners 在标记集合变化后收到通知。
const listeners = new Set<() => void>();

// publish replaces the mark set and notifies subscribers.
// publish 替换标记集合并通知订阅者。
function publish(next: ReadonlySet<string>): void {
  awayUnreadIds = next;
  listeners.forEach((listener) => listener());
}

// markAwayUnread records one conversation that settled while the window was unattended.
// markAwayUnread 记录一个在窗口无人关注期间结束的会话。
export function markAwayUnread(conversationId: string): void {
  if (awayUnreadIds.has(conversationId)) return;
  publish(new Set([...awayUnreadIds, conversationId]));
}

// clearAwayUnread drops every mark once the user interacts with the window again.
// clearAwayUnread 在用户再次操作窗口时清除全部标记。
export function clearAwayUnread(): void {
  if (awayUnreadIds.size === 0) return;
  publish(new Set());
}

// hasAwayUnread reports whether any mark is pending.
// hasAwayUnread 返回是否存在待清除的标记。
export function hasAwayUnread(): boolean {
  return awayUnreadIds.size > 0;
}

// isAwayUnread reports whether one conversation carries a mark.
// isAwayUnread 返回单个会话是否带有标记。
export function isAwayUnread(conversationId: string): boolean {
  return awayUnreadIds.has(conversationId);
}

// subscribe registers one listener for mark changes.
// subscribe 注册一个标记变化监听器。
function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

// useAwayUnread binds one conversation's mark for rendering.
// useAwayUnread 绑定单个会话的标记以供渲染。
export function useAwayUnread(conversationId: string): boolean {
  const read = React.useCallback(() => awayUnreadIds.has(conversationId), [conversationId]);
  return React.useSyncExternalStore(subscribe, read, read);
}
