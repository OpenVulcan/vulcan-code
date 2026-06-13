import React from "react";
import {
  closestCenter,
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import {
  GripVertical,
  SlidersHorizontal,
} from "lucide-react";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from "@/components/ui/select";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { useI18n, type LocaleCode } from "@/i18n";
import type {
  AiCapabilitySummary,
  AiCapabilityTargetSummary,
  ContextUsageSnapshot,
} from "@/types";
import {
  COMPOSER_AI_CAPABILITY_OPTIONS,
  orderedAiCapabilityTargets,
  type AiCapabilityKind,
  type ComposerAiCapabilityOption,
  type ComposerSelectOption,
} from "@/components/composer/composer-model";

import { useVerticalSortableItem } from "./composer-sortable";

// ContextUsageOrb renders a compact context budget indicator beside the composer.
// ContextUsageOrb 在输入区旁渲染紧凑的上下文预算指示器。
export function ContextUsageOrb(props: {
  // Context usage snapshot projected by the service.
  // 服务投影出的上下文占用快照。
  snapshot: ContextUsageSnapshot | null;
}) {
  // t translates fallback labels when the runtime has not prepared a budget yet.
  // t 在运行时尚未准备预算时翻译回退标签。
  const { t } = useI18n();
  const percent = Math.max(0, Math.min(100, Math.round(props.snapshot?.percent ?? 0)));
  const severity = props.snapshot?.severity ?? "unknown";
  const title = props.snapshot?.tooltipTitle || t("contextUsage.title");
  const rows = props.snapshot?.tooltipRows.length ? props.snapshot.tooltipRows : [t("contextUsage.waiting")];
  const style = {
    "--context-usage-progress": `${percent * 3.6}deg`,
  } as React.CSSProperties;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          aria-label={title}
          className={cn("context-usage-orb", `severity-${severity}`)}
          style={style}
          type="button"
        >
          <span className="context-usage-orb-label">{percent}%</span>
        </button>
      </TooltipTrigger>
      <TooltipContent align="end" className="context-usage-tooltip" side="top">
        <strong>{title}</strong>
        {rows.map((row, index) => (
          <span key={`${row}-${index}`}>{row}</span>
        ))}
      </TooltipContent>
    </Tooltip>
  );
}

// ComposerCapabilityIcon renders one semantic capability icon with enabled or grayscale treatment.
// ComposerCapabilityIcon 使用启用彩色或未启用灰度样式渲染单个语义能力图标。
function ComposerCapabilityIcon(props: {
  // Whether this capability is enabled for the current prompt.
  // 当前能力是否在本次提示词中启用。
  enabled: boolean;
  // Optional accessible label for the icon wrapper.
  // 图标外层可选无障碍标签。
  label?: string;
  // Capability option that provides the icon component and accent color.
  // 提供图标组件与强调色的能力选项。
  option: ComposerAiCapabilityOption;
  // Rendered icon size in pixels.
  // 图标渲染像素尺寸。
  size: number;
}) {
  const Icon = props.option.Icon;
  return (
    <span
      aria-label={props.label}
      className={cn("composer-capability-colored-icon", props.enabled ? "enabled" : "disabled")}
      style={{ "--capability-color": props.option.color } as React.CSSProperties}
    >
      <Icon size={props.size} />
    </span>
  );
}

// ComposerAiCapabilityControl renders the iced-aligned capability dialog.
// ComposerAiCapabilityControl 渲染对齐 iced 的能力设置对话框。
export function ComposerAiCapabilityControl(props: {
  // Accessible label for the dialog trigger.
  // 对话框触发器的无障碍标签。
  ariaLabel: string;
  // Service-projected capability tree for the composer.
  // 服务投影给输入区的能力树。
  capabilities: AiCapabilitySummary[];
  // Current WebUI locale used for menu labels.
  // 菜单标签使用的当前 WebUI 语言。
  locale: LocaleCode;
  // Runtime-disabled capability keys for the current prompt.
  // 当前提示词运行时禁用的能力键。
  disabledKinds: Set<AiCapabilityKind>;
  // Runtime-disabled target selectors for the current prompt.
  // 当前提示词运行时禁用的目标选择器。
  disabledTargetSelectors: Set<string>;
  // Runtime target order grouped by capability kind.
  // 按能力类型分组的运行时目标顺序。
  targetOrder: Map<AiCapabilityKind, string[]>;
  // Toggles one capability on or off.
  // 开关单个能力。
  onToggle: (kind: AiCapabilityKind, enabled: boolean) => void;
  // Replaces the target route order within one capability.
  // 替换单个能力中的目标路由顺序。
  onTargetReorder: (kind: AiCapabilityKind, targetSelectors: string[]) => void;
  // Toggles one capability target on or off.
  // 开关单个能力目标。
  onTargetToggle: (targetSelector: string, enabled: boolean) => void;
}) {
  // open tracks whether the full capability dialog is visible.
  // open 跟踪完整能力对话框是否可见。
  const [open, setOpen] = React.useState(false);
  // selectedKind stores the capability currently shown in the right pane.
  // selectedKind 保存右侧面板当前展示的能力。
  const [selectedKind, setSelectedKind] = React.useState<AiCapabilityKind>("web_search");
  const capabilityByKind = React.useMemo(() => {
    return new Map(props.capabilities.map((capability) => [capability.kind, capability]));
  }, [props.capabilities]);
  const visibleCapabilities = React.useMemo(() => {
    return COMPOSER_AI_CAPABILITY_OPTIONS.map((option) => ({
      option,
      capability: capabilityByKind.get(option.value),
    }));
  }, [capabilityByKind]);
  const enabledCapabilityEntries = visibleCapabilities.filter(({ option, capability }) => {
    return Boolean(capability?.available) && !props.disabledKinds.has(option.value);
  });
  const enabledCount = enabledCapabilityEntries.length;
  const triggerSubtitle = props.locale === "zh-CN"
    ? `${enabledCount}/${visibleCapabilities.length} 已启用`
    : `${enabledCount}/${visibleCapabilities.length} enabled`;
  const selectedEntry = visibleCapabilities.find(({ option }) => option.value === selectedKind) ?? visibleCapabilities[0];
  const selectedCapability = selectedEntry?.capability;
  const selectedTargets = orderedAiCapabilityTargets(selectedCapability, props.targetOrder);
  const selectedEnabled = Boolean(selectedCapability?.available) && !props.disabledKinds.has(selectedEntry.option.value);
  const targetDragSensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));
  const targetSortableIds = React.useMemo(
    () => selectedTargets.map((target) => target.targetSelector),
    [selectedTargets],
  );
  // handleTargetDragEnd applies the dnd-kit order directly to current-message route overrides.
  // handleTargetDragEnd 将 dnd-kit 顺序直接应用到当前消息级路由覆盖。
  const handleTargetDragEnd = React.useCallback((event: DragEndEvent) => {
    const activeId = String(event.active.id);
    const overId = event.over ? String(event.over.id) : null;
    if (!overId || activeId === overId) {
      return;
    }
    const oldIndex = targetSortableIds.indexOf(activeId);
    const newIndex = targetSortableIds.indexOf(overId);
    if (oldIndex < 0 || newIndex < 0) {
      return;
    }
    props.onTargetReorder(selectedEntry.option.value, arrayMove(targetSortableIds, oldIndex, newIndex));
  }, [props, selectedEntry.option.value, targetSortableIds]);

  React.useEffect(() => {
    if (visibleCapabilities.some(({ option }) => option.value === selectedKind)) {
      return;
    }
    const firstCapability = visibleCapabilities[0]?.option.value;
    if (firstCapability) {
      setSelectedKind(firstCapability);
    }
  }, [selectedKind, visibleCapabilities]);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <button
          aria-label={props.ariaLabel}
          className="composer-capability-trigger composer-select-trigger capability"
          type="button"
        >
          <span className="composer-select-icon"><SlidersHorizontal size={15} /></span>
          <span className="composer-select-copy">
            <span className="composer-select-label">{props.locale === "zh-CN" ? "扩展能力" : "Extended Capability"}</span>
            <span className="composer-capability-icon-row" aria-label={triggerSubtitle}>
              {visibleCapabilities.map(({ option, capability }) => (
                <ComposerCapabilityIcon
                  enabled={Boolean(capability?.available) && !props.disabledKinds.has(option.value)}
                  key={option.value}
                  label={props.locale === "zh-CN" ? option.labelZh : option.labelEn}
                  option={option}
                  size={11}
                />
              ))}
            </span>
          </span>
        </button>
      </DialogTrigger>
      <DialogContent className="composer-capability-dialog">
        <DialogHeader>
          <DialogTitle>{props.locale === "zh-CN" ? "当前消息能力设置" : "Current-message capabilities"}</DialogTitle>
          <DialogDescription className="composer-capability-dialog-note">
            {props.locale === "zh-CN"
              ? "会话进行中动态变更能力会重建上下文，并影响下一次对话的缓存命中。"
              : "Note: Changing capabilities while a session is running rebuilds context and affects the next turn's cache hit rate."}
          </DialogDescription>
        </DialogHeader>
        <div className="composer-capability-dialog-grid">
          <aside className="composer-capability-dialog-left">
            {visibleCapabilities.map(({ option, capability }) => {
              const available = Boolean(capability?.available);
              const enabled = available && !props.disabledKinds.has(option.value);
              const targetCount = capability?.targets.length ?? 0;
              return (
                <div
                  className={cn(
                    "composer-capability-kind-row",
                    selectedKind === option.value && "selected",
                    !available && "unavailable",
                  )}
                  key={option.value}
                  onClick={() => setSelectedKind(option.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      setSelectedKind(option.value);
                    }
                  }}
                  role="button"
                  tabIndex={0}
                >
                  <input
                    checked={enabled}
                    disabled={!available}
                    onChange={(event) => props.onToggle(option.value, event.currentTarget.checked)}
                    onClick={(event) => event.stopPropagation()}
                    type="checkbox"
                  />
                  <span className="composer-capability-kind-select">
                    <ComposerCapabilityIcon
                      enabled={enabled}
                      option={option}
                      size={16}
                    />
                    <span className="composer-capability-kind-copy">
                      <span>{props.locale === "zh-CN" ? option.labelZh : option.labelEn}</span>
                      <small>
                        {available
                          ? props.locale === "zh-CN"
                            ? `${targetCount} 个目标`
                            : `${targetCount} targets`
                          : props.locale === "zh-CN"
                            ? "暂无可用目标"
                            : "No targets"}
                      </small>
                    </span>
                  </span>
                </div>
              );
            })}
          </aside>
          <section className="composer-capability-dialog-right">
            <div className="composer-capability-detail-header">
              <span className="composer-capability-detail-icon">
                <ComposerCapabilityIcon
                  enabled={selectedEnabled}
                  option={selectedEntry.option}
                  size={18}
                />
              </span>
              <span className="composer-capability-detail-copy">
                <strong>{props.locale === "zh-CN" ? selectedEntry.option.labelZh : selectedEntry.option.labelEn}</strong>
                <small>{props.locale === "zh-CN" ? selectedEntry.option.subtitleZh : selectedEntry.option.subtitleEn}</small>
              </span>
            </div>
            {selectedTargets.length > 0 ? (
              <DndContext sensors={targetDragSensors} collisionDetection={closestCenter} onDragEnd={handleTargetDragEnd}>
                <SortableContext items={targetSortableIds} strategy={verticalListSortingStrategy}>
                  <div className="composer-capability-target-table">
                    {selectedTargets.map((target) => {
                      const targetEnabled = selectedEnabled && target.enabled && !props.disabledTargetSelectors.has(target.targetSelector);
                      return (
                        <ComposerCapabilityTargetRow
                          disabled={!target.enabled}
                          enabled={targetEnabled}
                          key={target.targetSelector}
                          selectedEnabled={selectedEnabled}
                          target={target}
                          onToggle={props.onTargetToggle}
                        />
                      );
                    })}
                  </div>
                </SortableContext>
              </DndContext>
            ) : (
              <div className="composer-capability-dialog-empty">
                {props.locale === "zh-CN" ? "当前能力暂无已配置目标。" : "This capability has no configured targets."}
              </div>
            )}
          </section>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ComposerCapabilityTargetRow renders one globally draggable capability target route row.
// ComposerCapabilityTargetRow 渲染单条整行可拖动的能力目标路由。
function ComposerCapabilityTargetRow(props: {
  // Whether the target route is globally disabled.
  // 当前目标路由是否全局不可用。
  disabled: boolean;
  // Whether the target route is enabled for this prompt.
  // 当前目标路由是否在本次提示词中启用。
  enabled: boolean;
  // Whether the owning capability is enabled.
  // 所属能力是否启用。
  selectedEnabled: boolean;
  // Target route summary displayed by the row.
  // 当前行展示的目标路由摘要。
  target: AiCapabilityTargetSummary;
  // Toggles the target route for this prompt.
  // 开关本次提示词中的目标路由。
  onToggle: (targetSelector: string, enabled: boolean) => void;
}) {
  const sortable = useVerticalSortableItem(props.target.targetSelector, false);
  return (
    <div
      className={cn("composer-capability-target-row", props.disabled && "disabled", sortable.isDragging && "dragging")}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
      {...sortable.attributes}
      {...sortable.listeners}
    >
      <GripVertical className="composer-capability-target-handle" size={14} />
      <input
        checked={props.enabled}
        disabled={!props.selectedEnabled || !props.target.enabled}
        onChange={(event) => props.onToggle(props.target.targetSelector, event.currentTarget.checked)}
        onPointerDown={(event) => event.stopPropagation()}
        type="checkbox"
      />
      <span className="composer-capability-target-copy">
        <span>{props.target.targetName || props.target.targetAlias}</span>
        <small>{props.target.vendorName} · {props.target.modelId || props.target.targetAlias}</small>
      </span>
    </div>
  );
}

// ComposerSelectControl renders one iced-aligned compact composer selector.
// ComposerSelectControl 渲染一个对齐 iced 的紧凑输入区选择器。
export function ComposerSelectControl(props: {
  // Accessible label for the dropdown trigger.
  // 下拉触发器的无障碍标签。
  ariaLabel: string;
  // Localized fallback label shown when the current value is missing.
  // 当前值缺失时显示的本地化回退标签。
  fallbackLabel: string;
  // Optional visual icon rendered in the left accessory slot.
  // 渲染在左侧附属槽位中的可选视觉图标。
  icon?: React.ReactNode;
  // Optional subtitle rendered only inside the closed trigger.
  // 仅在闭合触发器中渲染的可选副标题。
  triggerSubtitle?: string;
  // CSS class suffix controlling the iced-aligned fixed width.
  // 控制 iced 对齐固定宽度的 CSS 类后缀。
  className: string;
  // Available local options for this selector.
  // 当前选择器可用的本地选项。
  options: ComposerSelectOption[];
  // Current selected option value.
  // 当前选中的选项值。
  value: string;
  // Optional signal that opens the selector from an external slash-command route.
  // 可选信号，用于从外部 slash 指令路由打开选择器。
  openSignal?: number;
  // Selection change handler.
  // 选项变更处理函数。
  onValueChange: (value: string) => void;
}) {
  // open controls the Radix select popover so slash commands can reuse it.
  // open 控制 Radix 选择弹窗，使 slash 指令可复用它。
  const [open, setOpen] = React.useState(false);
  const selectedOption = props.options.find((option) => option.value === props.value) ?? props.options[0];

  React.useEffect(() => {
    if (props.openSignal === undefined || props.openSignal <= 0) {
      return;
    }
    setOpen(true);
  }, [props.openSignal]);

  return (
    <Select onOpenChange={setOpen} onValueChange={props.onValueChange} open={open} value={props.value}>
      <SelectTrigger
        aria-label={props.ariaLabel}
        className={cn("composer-select-trigger", props.className)}
      >
        {props.icon ? <span className="composer-select-icon">{props.icon}</span> : null}
        <span className={cn("composer-select-copy", !props.icon && "centered")}>
          <span className="composer-select-label">{selectedOption?.label ?? props.fallbackLabel}</span>
          {props.triggerSubtitle || selectedOption?.subtitle ? (
            <span className="composer-select-subtitle">{props.triggerSubtitle ?? selectedOption?.subtitle}</span>
          ) : null}
        </span>
      </SelectTrigger>
      <SelectContent align="start" className="composer-select-content" position="popper">
        {props.options.map((option) => (
          <SelectItem className="composer-select-item" key={option.value} value={option.value}>
            <span className="composer-select-item-copy">
              <span>{option.label}</span>
              {option.subtitle ? <small>{option.subtitle}</small> : null}
            </span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
