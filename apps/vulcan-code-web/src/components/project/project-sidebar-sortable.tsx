import * as React from "react";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { GripVertical } from "lucide-react";

// useVerticalSortableItem centralizes the sidebar-proven vertical sortable item behavior.
// useVerticalSortableItem 集中左侧侧栏已验证的纵向排序条目行为。
export function useVerticalSortableItem(id: string, disabled = false) {
  // sortable owns the dnd-kit state and drag listeners for one vertical item.
  // sortable 持有单个纵向条目的 dnd-kit 状态与拖拽监听。
  const sortable = useSortable({ id, disabled });
  // sortableTransform keeps movement vertical-only and prevents size-based text scaling.
  // sortableTransform 将移动限制为仅纵向，并阻止基于尺寸差异的文字缩放。
  const sortableTransform = sortable.transform ? { ...sortable.transform, x: 0, scaleX: 1, scaleY: 1 } : null;
  // sortableStyle is shared by project rows and pending prompt rows.
  // sortableStyle 由项目行和待发送提示词行共同使用。
  const sortableStyle: React.CSSProperties = {
    transform: CSS.Transform.toString(sortableTransform),
    transition: sortable.transition,
  };
  return {
    ...sortable,
    sortableStyle,
  };
}

// OPTIMISTIC_ORDER_TIMEOUT_MS bounds how long a dropped order is shown before the authoritative order returns,
// so a rejected reorder cannot leave the list in an order the service never accepted.
// OPTIMISTIC_ORDER_TIMEOUT_MS 限定拖放后的顺序在权威顺序回来之前最多显示多久，使被拒绝的排序不会让列表停留在服务从未接受的顺序。
const OPTIMISTIC_ORDER_TIMEOUT_MS = 5000;

// useOptimisticOrder shows a dropped order at once while its persistence is in flight. Without it the list
// renders the old order for the round trip after dnd-kit releases the row, so the row snaps back and then
// jumps into place. The pending order clears once the authoritative items arrive in that order, when the item
// set changes, or after OPTIMISTIC_ORDER_TIMEOUT_MS.
// useOptimisticOrder 在持久化进行期间立即显示拖放后的顺序。否则 dnd-kit 释放该行后，列表会在往返期间渲染旧顺序，
// 该行先弹回再跳到位。权威条目按该顺序到达、条目集合变化或超过 OPTIMISTIC_ORDER_TIMEOUT_MS 后，待定顺序即清除。
export function useOptimisticOrder<T extends { id: string }>(items: readonly T[]): [readonly T[], (ids: string[]) => void] {
  const [pending, setPending] = React.useState<string[] | null>(null);
  const signature = items.map((item) => item.id).join("\n");
  React.useEffect(() => {
    if (!pending) return;
    const ids = signature.split("\n");
    const sameSet = ids.length === pending.length && pending.every((id) => ids.includes(id));
    if (!sameSet || ids.every((id, index) => id === pending[index])) {
      setPending(null);
      return;
    }
    const timer = window.setTimeout(() => setPending(null), OPTIMISTIC_ORDER_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
  }, [pending, signature]);
  const ordered = React.useMemo(() => {
    if (!pending) return items;
    const rank = new Map(pending.map((id, index) => [id, index]));
    return [...items].sort((left, right) => (rank.get(left.id) ?? Infinity) - (rank.get(right.id) ?? Infinity));
  }, [items, pending]);
  return [ordered, setPending];
}

// SortableRowBinding lets a row that does not own its sorting join a sortable list: the row applies the node ref,
// style and dragging mark to its item and renders the grip among its actions.
// SortableRowBinding 让不自行持有排序的行加入可排序列表：行把节点引用、样式与拖动标记应用到自身条目，并在操作中渲染手柄。
export interface SortableRowBinding {
  // Registers the row element with dnd-kit.
  // 向 dnd-kit 注册行元素。
  setNodeRef: (node: HTMLElement | null) => void;
  // Vertical-only transform applied while sorting.
  // 排序时应用的仅纵向变换。
  style: React.CSSProperties;
  // Whether this row is being dragged.
  // 此行是否正被拖动。
  dragging: boolean;
  // Drag handle rendered among the row actions.
  // 渲染在行操作中的拖动手柄。
  grip: React.ReactNode;
}

// useSortableRowBinding builds the sortable binding of one row with a labelled grip.
// useSortableRowBinding 为一行构建带标签手柄的排序绑定。
export function useSortableRowBinding(id: string, label: string, disabled = false): SortableRowBinding {
  const sortable = useVerticalSortableItem(id, disabled);
  return {
    setNodeRef: sortable.setNodeRef,
    style: sortable.sortableStyle,
    dragging: sortable.isDragging,
    grip: (
      <button
        ref={sortable.setActivatorNodeRef}
        {...sortable.attributes}
        {...sortable.listeners}
        aria-label={label}
        className="vk-icon-btn vk-tree-grip"
        data-size="sm"
        title={label}
        type="button"
      >
        <GripVertical aria-hidden="true" />
      </button>
    ),
  };
}
