import React from "react";
import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import { arrayMove, SortableContext, sortableKeyboardCoordinates, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { toast } from "@/ui-kit/primitives/toast";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { ConversationSummary } from "@/types";
import { useOptimisticOrder, useSortableRowBinding, type SortableRowBinding } from "./project-sidebar-sortable";

// SortableSessionList lets one run of session rows be reordered by dragging. It owns its own drag context, so a
// drag never leaves the run: pinned rows stay among pinned rows and a project's rows stay in that project.
// SortableSessionList 让一段会话行可以拖动排序。它持有独立的拖动上下文，因此拖动不会离开这一段：置顶行留在置顶行之间，
// 项目的行留在该项目内。
export function SortableSessionList(props: {
  // Rows in their current display order.
  // 按当前显示顺序排列的行。
  sessions: readonly ConversationSummary[];
  // Whether dragging is off, as while a search filters the rows.
  // 是否关闭拖动，例如搜索筛选这些行时。
  disabled: boolean;
  // Grip label; `{name}` receives the session title.
  // 手柄标签；`{name}` 填入会话标题。
  reorderLabel: TranslationKey;
  // Persists the dropped order of these rows; rejects with the service error.
  // 持久化这些行拖放后的顺序；失败时以服务错误拒绝。
  onReorder: (orderedIds: string[]) => Promise<void>;
  // Renders one row joined to the sortable list.
  // 渲染一个加入可排序列表的行。
  renderRow: (conversation: ConversationSummary, sortable: SortableRowBinding) => React.ReactNode;
}) {
  const [ordered, showOrder] = useOptimisticOrder(props.sessions);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const handleDragEnd = (event: DragEndEvent) => {
    if (!event.over || props.disabled) return;
    const ids = ordered.map((conversation) => conversation.id);
    const from = ids.indexOf(String(event.active.id));
    const to = ids.indexOf(String(event.over.id));
    if (from < 0 || to < 0 || from === to) return;
    const next = arrayMove(ids, from, to);
    showOrder(next);
    props.onReorder(next).catch((error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    });
  };
  return (
    <DndContext collisionDetection={closestCenter} onDragEnd={handleDragEnd} sensors={sensors}>
      <SortableContext disabled={props.disabled} items={ordered.map((conversation) => conversation.id)} strategy={verticalListSortingStrategy}>
        {ordered.map((conversation) => (
          <SortableSessionRow
            conversation={conversation}
            disabled={props.disabled}
            key={conversation.id}
            reorderLabel={props.reorderLabel}
            render={props.renderRow}
          />
        ))}
      </SortableContext>
    </DndContext>
  );
}

// SortableSessionRow binds one session row to the surrounding sortable list.
// SortableSessionRow 将一个会话行绑定到外层可排序列表。
function SortableSessionRow(props: {
  conversation: ConversationSummary;
  disabled: boolean;
  reorderLabel: TranslationKey;
  render: (conversation: ConversationSummary, sortable: SortableRowBinding) => React.ReactNode;
}) {
  const { t } = useI18n();
  const sortable = useSortableRowBinding(
    props.conversation.id,
    formatMessage(t(props.reorderLabel), { name: props.conversation.title }),
    props.disabled,
  );
  return <>{props.render(props.conversation, sortable)}</>;
}
