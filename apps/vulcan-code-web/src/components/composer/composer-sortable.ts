import type React from "react";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

// useVerticalSortableItem centralizes vertical sortable item behavior for composer controls.
// useVerticalSortableItem 集中输入区控件中的纵向排序条目行为。
export function useVerticalSortableItem(id: string, disabled = false) {
  // sortable owns the dnd-kit state and drag listeners for one vertical item.
  // sortable 持有单个纵向条目的 dnd-kit 状态与拖拽监听。
  const sortable = useSortable({ id, disabled });
  // sortableTransform keeps movement vertical-only and prevents size-based text scaling.
  // sortableTransform 将移动限制为仅纵向，并阻止基于尺寸差异的文字缩放。
  const sortableTransform = sortable.transform ? { ...sortable.transform, x: 0, scaleX: 1, scaleY: 1 } : null;
  // sortableStyle is shared by pending prompts and capability target rows.
  // sortableStyle 由待发送提示词与能力目标行共同使用。
  const sortableStyle: React.CSSProperties = {
    transform: CSS.Transform.toString(sortableTransform),
    transition: sortable.transition,
  };
  return {
    ...sortable,
    sortableStyle,
  };
}
