import React from "react";
import {
  closestCenter,
  DndContext,
  PointerSensor,
  KeyboardSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import {
  CornerDownRight,
  GripVertical,
  Pencil,
  Trash2,
} from "lucide-react";

import { Button } from "@/ui-kit/primitives/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import type { ComposerSlashCommandSummary, PendingPromptSummary } from "@/types";
import { composerSlashCommandDescriptionKey } from "@/components/composer/composer-model";
import { composerListOptionId } from "@/components/composer/composer-panel/use-composer-textarea-keys";

import type { ComposerTraceCallback } from "./composer-control-types";
import { useVerticalSortableItem } from "./composer-sortable";

// PendingPromptQueueStrip renders the service-backed visible send queue above the composer.
// PendingPromptQueueStrip 渲染输入区上方由服务端持久化的可见发送队列。
// Parameters: props supplies durable queued prompts and queue callbacks; returns sortable rows with pointer and keyboard reordering.
// 参数：props 提供持久化排队提示词与队列回调；返回支持指针和键盘排序的行。
export function PendingPromptQueueStrip(props: {
  // Queue entries returned by the selected conversation snapshot.
  // 当前选中会话快照返回的队列条目。
  entries: PendingPromptSummary[];
  // disabled freezes queue mutation while another exact queue transaction is in flight.
  // disabled 在另一项精确队列事务进行期间冻结队列变更。
  disabled?: boolean;
  // Optional trace callback for row-level UI interactions.
  // 行级 UI 交互使用的可选埋点回调。
  onTrace?: ComposerTraceCallback;
  // Restores one entry into the composer draft.
  // 将单条队列恢复到输入区草稿。
  onEdit: (entryId: string) => void;
  // Routes one entry into the shared insertion queue.
  // 将单条队列项路由进共享引导队列。
  onInsert: (entryId: string) => void;
  // Removes one entry from the queue.
  // 从队列中移除单条条目。
  onRemove: (entryId: string) => void;
  // Persists a reordered entry id list.
  // 持久化重排后的条目标识列表。
  onReorder: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // sensors keeps accidental clicks from turning into drag operations.
  // sensors 防止普通点击误触发拖拽。
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  // entryIds is the stable dnd item list derived from the backend queue.
  // entryIds 是从后端队列派生出的稳定拖拽条目列表。
  const entryIds = React.useMemo(() => props.entries.map((entry) => entry.id), [props.entries]);
  // handleDragEnd persists the service-compatible drop target derived from the visual dnd order.
  // handleDragEnd 从可视拖拽顺序推导与服务兼容的落点并持久化。
  const handleDragEnd = React.useCallback((event: DragEndEvent) => {
    if (props.disabled) {
      return;
    }
    const activeId = String(event.active.id);
    const overId = event.over ? String(event.over.id) : null;
    if (!overId || activeId === overId) {
      return;
    }
    const oldIndex = entryIds.indexOf(activeId);
    const newIndex = entryIds.indexOf(overId);
    if (oldIndex < 0 || newIndex < 0) {
      return;
    }
    const nextEntryIds = arrayMove(entryIds, oldIndex, newIndex);
    const movedIndex = nextEntryIds.indexOf(activeId);
    const targetEntryId = movedIndex >= 0 ? nextEntryIds[movedIndex + 1] ?? null : null;
    props.onReorder(nextEntryIds, activeId, targetEntryId);
  }, [entryIds, props]);

  return (
    <div className="vk-queue" aria-label={t("pendingPrompt.queue")}>
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext items={entryIds} strategy={verticalListSortingStrategy}>
          <div className="vk-queue-list">
            {props.entries.map((entry) => (
              <PendingPromptQueueRow
                disabled={props.disabled}
                entry={entry}
                key={entry.id}
                onEdit={props.onEdit}
                onInsert={props.onInsert}
                onRemove={props.onRemove}
                onTrace={props.onTrace}
              />
            ))}
          </div>
        </SortableContext>
      </DndContext>
    </div>
  );
}

// PendingPromptQueueRow renders one sortable queued prompt row.
// PendingPromptQueueRow 渲染单条可排序的排队提示词。
function PendingPromptQueueRow(props: {
  // Queue entry rendered by this row.
  // 当前行渲染的队列条目。
  entry: PendingPromptSummary;
  // disabled blocks sorting and every row action during a competing queue transaction.
  // disabled 在竞争队列事务期间阻止排序与全部行操作。
  disabled?: boolean;
  // Optional trace callback for row-level UI interactions.
  // 行级 UI 交互使用的可选埋点回调。
  onTrace?: ComposerTraceCallback;
  // Restores this entry into the composer draft.
  // 将当前条目恢复到输入区草稿。
  onEdit: (entryId: string) => void;
  // Routes this entry into the shared insertion queue.
  // 将当前条目路由进共享引导队列。
  onInsert: (entryId: string) => void;
  // Removes this entry from the queue.
  // 从队列移除当前条目。
  onRemove: (entryId: string) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const sortable = useVerticalSortableItem(
    props.entry.id,
    Boolean(props.disabled || props.entry.state === "inserted"),
  );
  const attachmentText = props.entry.attachments.length
    ? `  · ${formatPendingPromptTranslation(t, "pendingPrompt.attachmentCount", { count: String(props.entry.attachments.length) })}`
    : "";
  const previewText = `${props.entry.preview || props.entry.prompt || t("pendingPrompt.attachmentMessage")}${attachmentText}`;
  return (
    <div
      className="vk-queue-row"
      data-dragging={sortable.isDragging ? "" : undefined}
      data-inserted={props.entry.state === "inserted" ? "" : undefined}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
    >
      <div
        aria-label={t("pendingPrompt.drag")}
        className="vk-queue-handle"
        {...sortable.attributes}
        {...sortable.listeners}
      >
        <GripVertical size={13} />
      </div>
      <span className="vk-queue-text">{previewText}</span>
      <div
        className="vk-queue-actions"
        onClick={(event) => event.stopPropagation()}
        onPointerDown={(event) => event.stopPropagation()}
      >
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              className="vk-queue-insert"
              data-inserted={props.entry.state === "inserted" ? "" : undefined}
              aria-label={props.entry.state === "inserted" ? t("pendingPrompt.inserted") : t("pendingPrompt.insert")}
              disabled={Boolean(props.disabled || props.entry.state === "inserted")}
              onClick={() => {
                props.onTrace?.("pending_prompt.row.insert", {
                  entryId: props.entry.id,
                  state: props.entry.state,
                });
                props.onInsert(props.entry.id);
              }}
              size="xs"
              type="button"
              variant="ghost"
            >
              <CornerDownRight size={13} />
              <span>{props.entry.state === "inserted" ? t("pendingPrompt.insertedShort") : t("pendingPrompt.insertShort")}</span>
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">{props.entry.state === "inserted" ? t("pendingPrompt.inserted") : t("pendingPrompt.insert")}</TooltipContent>
        </Tooltip>
        {props.entry.state === "inserted" ? null : (
          <>
            <Button
              aria-label={t("pendingPrompt.delete")}
              disabled={props.disabled}
              onClick={() => {
                props.onTrace?.("pending_prompt.row.remove", { entryId: props.entry.id });
                props.onRemove(props.entry.id);
              }}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <Trash2 size={13} />
            </Button>
            <Button
              aria-label={t("pendingPrompt.edit")}
              disabled={props.disabled}
              onClick={() => {
                props.onTrace?.("pending_prompt.row.edit", { entryId: props.entry.id });
                props.onEdit(props.entry.id);
              }}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <Pencil size={13} />
            </Button>
          </>
        )}
      </div>
    </div>
  );
}

// formatPendingPromptTranslation replaces named placeholders in localized queued-prompt labels.
// formatPendingPromptTranslation 替换待发送队列本地化标签中的具名占位符。
function formatPendingPromptTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}

// ComposerSlashCommandPanel renders the WebUI-style slash command candidate list.
// ComposerSlashCommandPanel 渲染对齐 WebUI 的 slash 指令候选列表。
export function ComposerSlashCommandPanel(props: {
  // id names the list for the textarea that drives it.
  // id 为驱动该列表的文本框命名列表。
  id: string;
  // Visible commands after prefix filtering.
  // 前缀过滤后的可见指令。
  commands: ComposerSlashCommandSummary[];
  // Highlighted command row index.
  // 当前高亮的指令行索引。
  highlightedIndex: number;
  // Activates one command row.
  // 激活单个指令行。
  onActivate: (command: ComposerSlashCommandSummary) => void;
  // Updates highlighted command row.
  // 更新高亮指令行。
  onHighlight: (index: number) => void;
}) {
  const { t } = useI18n();
  const listRef = React.useRef<HTMLDivElement | null>(null);
  // The highlighted row stays in view while the arrows move past the panel's visible rows.
  // 方向键移过面板可见行时，高亮行保持在视野内。
  React.useLayoutEffect(() => {
    listRef.current?.children[props.highlightedIndex]?.scrollIntoView?.({ block: "nearest" });
  }, [props.highlightedIndex]);
  return (
    <div className="vk-slash" id={props.id} ref={listRef} role="listbox" aria-label={t("composer.slash.panel")}>
      {props.commands.map((command, index) => {
        const descriptionKey = composerSlashCommandDescriptionKey(command);
        return (
        <button
          aria-selected={index === props.highlightedIndex}
          className="vk-slash-row"
          id={composerListOptionId(props.id, index)}
          key={command.id || command.command}
          onClick={() => props.onActivate(command)}
          onMouseEnter={() => props.onHighlight(index)}
          role="option"
          type="button"
        >
          <span className="vk-slash-cmd">{command.command}</span>
          <span className="vk-slash-desc">{descriptionKey ? t(descriptionKey) : null}</span>
        </button>
        );
      })}
    </div>
  );
}
