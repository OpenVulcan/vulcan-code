import React from "react";
import type { ConversationTimelineItem } from "@/components/conversation/conversation-model";
import { conversationTimelineItemKey } from "@/components/conversation/conversation-timeline";
import { ESTIMATED_TURN_HEIGHT_PX, useConversationTurnWindow } from "./conversation-turn-window";
import type {
  ConversationTimelineProjectionStore,
  ConversationTurnProjection,
} from "@/components/conversation/conversation-timeline-projection-store";

// ConversationTurnSeatProps describes one stable rendered Turn boundary.
// ConversationTurnSeatProps 描述单个稳定渲染的 Turn 边界。
export interface ConversationTurnSeatProps {
  // Stable Turn key inside the surface.
  // 界面内部稳定的 Turn 键。
  rowKey: string;
  // Exact row store that owns this Turn snapshot.
  // 拥有当前 Turn 快照的精确行仓库。
  store: ConversationTimelineProjectionStore;
  // Exact parent projection used only before a newly mounted row enters the committed Store.
  // 仅在新挂载行进入已提交 Store 前使用的精确父级投影。
  initialRow: ConversationTurnProjection;
  // Renders one timeline item; live clocks belong to the leaves that display them, never to the Seat.
  // 渲染单条时间线条目；实时计时属于显示它的叶子组件，绝不属于 Seat。
  renderItem: (item: ConversationTimelineItem) => React.ReactNode;
}

// ConversationTurnSeat keeps one stable Turn outside sibling render updates.
// ConversationTurnSeat 让单个稳定 Turn 不受兄弟行渲染更新影响。
// Parameters: props contains the exact Turn identity, row store, and stable renderer.
// 参数：props 包含精确 Turn 身份、行仓库与稳定渲染器。
// Returns: the ordered timeline children for the mounted turn body.
// 返回值：已挂载回合正文的有序时间线子项。
export const ConversationTurnSeat = React.memo(function ConversationTurnSeat(
  props: ConversationTurnSeatProps,
) {
  // subscribe binds React to this exact row key rather than the complete timeline projection.
  // subscribe 将 React 绑定到当前精确行键，而不是完整时间线投影。
  const subscribe = React.useCallback(
    (listener: () => void) => props.store.subscribe(props.rowKey, listener),
    [props.rowKey, props.store],
  );
  // getSnapshot reads the immutable row reference retained by the projection store.
  // getSnapshot 读取投影仓库保留的不可变行引用。
  const getSnapshot = React.useCallback(
    () => props.store.getRowSnapshotOr(props.rowKey, props.initialRow),
    [props.initialRow, props.rowKey, props.store],
  );
  // row is updated only when this exact Turn projection changes.
  // row 仅在当前精确 Turn 投影变化时更新。
  const row = React.useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  return (
    <>
      {row.items.map((item) => (
        <React.Fragment key={conversationTimelineItemKey(item)}>
          {props.renderItem(item)}
        </React.Fragment>
      ))}
    </>
  );
}, conversationTurnSeatPropsEqual);

// conversationTurnSeatPropsEqual isolates committed row notifications from parent projection allocation.
// conversationTurnSeatPropsEqual 将已提交行通知与父级投影对象分配隔离。
// Parameters: previous and next are consecutive Seat props for the same React key.
// 参数：previous 与 next 是相同 React key 对应的连续 Seat 属性。
// Returns: true when render-relevant props are unchanged; initialRow identity is deliberately ignored after mount.
// 返回值：渲染相关属性未变化时返回 true；挂载后会刻意忽略 initialRow 身份变化。
function conversationTurnSeatPropsEqual(
  previous: ConversationTurnSeatProps,
  next: ConversationTurnSeatProps,
): boolean {
  return previous.rowKey === next.rowKey
    && previous.store === next.store
    && previous.renderItem === next.renderItem;
}

// ConversationTurnSeatListProps describes the order-only parent boundary around stable Turn Seats.
// ConversationTurnSeatListProps 描述稳定 Turn Seat 外部仅负责顺序的父级边界。
export interface ConversationTurnSeatListProps {
  // Stable conversation or child-surface key used to remount rows when ownership changes; null means no owner.
  // 界面所有权变化时用于重新挂载行的稳定会话或子界面键；null 表示没有所有者。
  cacheScopeKey: string | null;
  // Stable ordered Turn keys for the current visible transcript.
  // 当前可见转录的稳定有序 Turn 键。
  rowKeys: readonly string[];
  // Current parent projections used only to mount keys not already subscribed.
  // 仅用于挂载尚未订阅键的当前父级投影。
  rows: ReadonlyMap<string, ConversationTurnProjection>;
  // Exact row Store shared by every Seat.
  // 每个 Seat 共享的精确行 Store。
  store: ConversationTimelineProjectionStore;
  // Shared presentation renderer whose identity changes only for presentation-wide inputs.
  // 仅在展示级输入变化时改变身份的共享展示渲染器。
  renderItem: (item: ConversationTimelineItem) => React.ReactNode;
  // The exact scrollport enables body windowing on interactive surfaces.
  // 精确滚动视口为交互界面启用正文视口渲染。
  scrollRef?: React.RefObject<HTMLElement | null>;
}

// ConversationTurnSeatList prevents content-only row changes from reconciling every historical Seat.
// ConversationTurnSeatList 防止纯行内容变化协调全部历史 Seat。
// Parameters: props contains stable order, exact first-mount projections, Store, and shared renderer.
// 参数：props 包含稳定顺序、精确首次挂载投影、Store 与共享渲染器。
// Returns: stable row shells with mounted bodies only where needed.
// 返回值：稳定的回合外壳，正文仅按需挂载。
export const ConversationTurnSeatList = React.memo(function ConversationTurnSeatList(
  props: ConversationTurnSeatListProps,
) {
  // The window keeps shell geometry stable when a turn's body is evicted.
  // 正文卸载时，视口窗口保留回合外壳的几何尺寸。
  const { mounted, heights } = useConversationTurnWindow(props.scrollRef, props.rowKeys);
  return (
    <>
      {props.rowKeys.map((rowKey) => {
        // row is required only when this ordered key mounts a new Seat before the Store layout commit.
        // row 仅在该有序键于 Store 布局提交前挂载新 Seat 时需要。
        const row = props.rows.get(rowKey);
        if (!row) {
          throw new Error(`Conversation timeline order has no projection: ${rowKey}`);
        }
        // Mounted turns use natural height so streaming and folds still flow through the scroll owner.
        // 已挂载回合使用自然高度，流式输出与折叠继续由滚动所有者处理。
        const visible = mounted === null || mounted.has(rowKey);
        return (
          <section
            className="vk-turn"
            data-conversation-row-key={rowKey}
            data-conversation-turn-shell={rowKey}
            data-conversation-turn-placeholder={visible ? undefined : ""}
            key={props.cacheScopeKey ? `${props.cacheScopeKey}:${rowKey}` : rowKey}
            style={visible ? undefined : { height: heights.get(rowKey) ?? ESTIMATED_TURN_HEIGHT_PX }}
          >
            {visible ? <ConversationTurnSeat
              initialRow={row}
              renderItem={props.renderItem}
              rowKey={rowKey}
              store={props.store}
            /> : null}
          </section>
        );
      })}
    </>
  );
}, conversationTurnSeatListPropsEqual);

// conversationTurnSeatListPropsEqual skips sibling reconciliation while row order and shared presentation stay stable.
// conversationTurnSeatListPropsEqual 在行顺序与共享展示保持稳定时跳过兄弟行协调。
// Parameters: previous and next are consecutive order-boundary props.
// 参数：previous 与 next 是连续的顺序边界属性。
// Returns: true when row Store notifications can fully handle the change without rebuilding the Seat list.
// 返回值：行 Store 通知可完整处理变化且无需重建 Seat 列表时返回 true。
function conversationTurnSeatListPropsEqual(
  previous: ConversationTurnSeatListProps,
  next: ConversationTurnSeatListProps,
): boolean {
  return previous.cacheScopeKey === next.cacheScopeKey
    && previous.rowKeys === next.rowKeys
    && previous.store === next.store
    && previous.renderItem === next.renderItem
    && previous.scrollRef === next.scrollRef;
}
