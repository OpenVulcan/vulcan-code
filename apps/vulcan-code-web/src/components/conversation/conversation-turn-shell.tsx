import type { ReactNode } from "react";
import { ESTIMATED_TURN_HEIGHT_PX, type ConversationTurnWindow } from "./conversation-turn-window";

// ConversationTurnShell keeps the row identity and height available while its body is outside the viewport.
// ConversationTurnShell 在正文移出视口时保留行标识和高度。
// Parameters: rowKey identifies the turn, turnWindow supplies visibility and measured heights, and children render the body.
// 参数：rowKey 标识回合，turnWindow 提供可见范围和实测高度，children 渲染正文。
// Returns: the shared transcript row boundary, with its body mounted only when requested.
// 返回值：共享的会话行外壳，仅在视口需要时挂载正文。
export function ConversationTurnShell({ rowKey, turnWindow, children }: {
  // Stable row identity for restoration and intersection tracking.
  // 用于恢复位置和追踪视口相交的稳定行标识。
  rowKey: string;
  // Shared mounted keys and measured placeholder heights.
  // 共享的挂载键和实测占位高度。
  turnWindow: ConversationTurnWindow;
  // Source-specific body rendered within the shared shell.
  // 渲染在共享外壳内的来源正文。
  children: ReactNode;
}) {
  // A mounted body uses natural height so the scroll controller sees its actual geometry.
  // 已挂载的正文使用自然高度，使滚动控制器获得真实几何。
  const mounted = turnWindow.mounted === null || turnWindow.mounted.has(rowKey);
  return (
    <section
      className="vk-turn"
      data-conversation-row-key={rowKey}
      data-conversation-turn-shell={rowKey}
      data-conversation-turn-placeholder={mounted ? undefined : ""}
      style={mounted ? undefined : { height: turnWindow.heights.get(rowKey) ?? ESTIMATED_TURN_HEIGHT_PX }}
    >
      {mounted ? children : null}
    </section>
  );
}
