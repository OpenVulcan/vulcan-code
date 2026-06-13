import * as React from "react"
import { useSortable } from "@dnd-kit/sortable"
import { CSS } from "@dnd-kit/utilities"
import { GripVertical } from "lucide-react"

import type { AiCapabilityTargetSummary } from "@/types"

// SortableAiCapabilityTargetRow renders one settings capability route with dnd-kit sorting.
// SortableAiCapabilityTargetRow 使用 dnd-kit 排序渲染单个设置能力路由。
export function SortableAiCapabilityTargetRow(props: {
  // Whether the remove action is waiting for confirmation.
  // 移除操作是否正在等待确认。
  confirmRemove: boolean
  // Zero-based row index shown to the user.
  // 展示给用户的零基行序号。
  index: number
  // Target route summary displayed by the row.
  // 当前行展示的目标路由摘要。
  target: AiCapabilityTargetSummary
  // Localized literal helper.
  // 本地化字面量辅助函数。
  text: (zh: string, en: string) => string
  // Toggles whether this target is enabled.
  // 切换当前目标是否启用。
  onToggle: (enabled: boolean) => void
  // Removes this target from the capability route list.
  // 从能力路由列表移除当前目标。
  onRemove: () => void
  // Enters remove-confirmation state.
  // 进入移除确认状态。
  onRequestRemove: () => void
  // Cancels remove-confirmation state.
  // 取消移除确认状态。
  onCancelRemove: () => void
}) {
  const sortable = useVerticalSortableItem(props.target.targetSelector, false)
  return (
    <article
      className={`settings-capability-route-row ${sortable.isDragging ? "dragging" : ""}`}
      onMouseLeave={() => {
        if (props.confirmRemove) {
          props.onCancelRemove()
        }
      }}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
      {...sortable.attributes}
      {...sortable.listeners}
    >
      <GripVertical className="settings-drag-handle" size={14} />
      <input
        checked={props.target.enabled}
        onChange={(event) => props.onToggle(event.target.checked)}
        onPointerDown={(event) => event.stopPropagation()}
        type="checkbox"
      />
      <span className="settings-route-index">{String(props.index + 1).padStart(2, "0")}</span>
      <span className="settings-route-separator">·</span>
      <div className="settings-route-main">
        <strong>{props.target.vendorName} / {props.target.targetName || props.target.targetAlias}</strong>
        <small>{props.target.modelId} · {props.target.targetSelector}</small>
      </div>
      <button
        className={`skill-action-button secondary danger ${props.confirmRemove ? "confirm" : ""}`}
        onClick={(event) => {
          event.stopPropagation()
          if (props.confirmRemove) {
            props.onRemove()
            return
          }
          props.onRequestRemove()
        }}
        onPointerDown={(event) => event.stopPropagation()}
        type="button"
      >
        {props.confirmRemove ? props.text("确认移除", "Confirm remove") : props.text("移除", "Remove")}
      </button>
    </article>
  )
}

// useVerticalSortableItem centralizes vertical-only dnd-kit row behavior for settings lists.
// useVerticalSortableItem 集中设置列表纵向 dnd-kit 行为并限制横向漂移。
function useVerticalSortableItem(id: string, disabled = false) {
  const sortable = useSortable({ id, disabled })
  const sortableTransform = sortable.transform ? { ...sortable.transform, x: 0, scaleX: 1, scaleY: 1 } : null
  const sortableStyle: React.CSSProperties = {
    transform: CSS.Transform.toString(sortableTransform),
    transition: sortable.transition,
  }
  return {
    ...sortable,
    sortableStyle,
  }
}
