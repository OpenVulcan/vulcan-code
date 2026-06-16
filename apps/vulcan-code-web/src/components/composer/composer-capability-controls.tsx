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
  Check,
  GripVertical,
  Loader2,
  Save,
  SlidersHorizontal,
} from "lucide-react";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
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
  defaultAiCapabilityTargetOrder,
  defaultDisabledAiCapabilityKinds,
  defaultDisabledAiCapabilityTargetSelectors,
  orderedAiCapabilityTargets,
  serializeAiCapabilityTargetOrder,
  type AiCapabilityKind,
  type ComposerAiCapabilityConfigurationSave,
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
        />
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
  // Whether this capability is enabled in the rendered capability state.
  // 当前能力是否在渲染的能力状态中启用。
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
  // Service-projected default capability tree used to reset the session draft.
  // 用于重置会话草稿的服务端默认能力树。
  defaultCapabilities: AiCapabilitySummary[];
  // Current WebUI locale used for menu labels.
  // 菜单标签使用的当前 WebUI 语言。
  locale: LocaleCode;
  // Session-synchronized disabled capability keys.
  // 与会话同步的禁用能力键。
  disabledKinds: Set<AiCapabilityKind>;
  // Session-synchronized disabled target selectors.
  // 与会话同步的禁用目标选择器。
  disabledTargetSelectors: Set<string>;
  // Session-synchronized target order grouped by capability kind.
  // 按能力类型分组、与会话同步的目标顺序。
  targetOrder: Map<AiCapabilityKind, string[]>;
  // Persists one session-scoped capability configuration draft.
  // 持久化单个会话级能力配置草稿。
  onSaveConfiguration: (request: ComposerAiCapabilityConfigurationSave) => Promise<boolean>;
}) {
  // open tracks whether the full capability dialog is visible.
  // open 跟踪完整能力对话框是否可见。
  const [open, setOpen] = React.useState(false);
  // confirmCloseOpen tracks the dirty-close confirmation dialog.
  // confirmCloseOpen 跟踪有未保存变更时的关闭确认弹窗。
  const [confirmCloseOpen, setConfirmCloseOpen] = React.useState(false);
  // selectedKind stores the capability currently shown in the right pane.
  // selectedKind 保存右侧面板当前展示的能力。
  const [selectedKind, setSelectedKind] = React.useState<AiCapabilityKind>("web_search");
  // draftDisabledKinds stores dialog-local capability disabled choices.
  // draftDisabledKinds 保存弹窗本地的能力禁用选择。
  const [draftDisabledKinds, setDraftDisabledKinds] = React.useState<Set<AiCapabilityKind>>(() => new Set());
  // draftDisabledTargetSelectors stores dialog-local target disabled choices.
  // draftDisabledTargetSelectors 保存弹窗本地的目标禁用选择。
  const [draftDisabledTargetSelectors, setDraftDisabledTargetSelectors] = React.useState<Set<string>>(() => new Set());
  // draftTargetOrder stores dialog-local route order choices.
  // draftTargetOrder 保存弹窗本地的路由顺序选择。
  const [draftTargetOrder, setDraftTargetOrder] = React.useState<Map<AiCapabilityKind, string[]>>(() => new Map());
  // baselineSignature stores the persisted snapshot captured when the dialog opens.
  // baselineSignature 保存弹窗打开时捕获的已持久化快照签名。
  const [baselineSignature, setBaselineSignature] = React.useState("");
  // saveState drives the visible save button state.
  // saveState 驱动可见保存按钮状态。
  const [saveState, setSaveState] = React.useState<"idle" | "saving" | "saved">("idle");
  // externalUpdateNotice tracks whether another UI changed this session while the local draft is dirty.
  // externalUpdateNotice 跟踪本地草稿脏时其他 UI 是否修改了当前会话。
  const [externalUpdateNotice, setExternalUpdateNotice] = React.useState(false);
  const capabilityByKind = React.useMemo(() => {
    return new Map(props.capabilities.map((capability) => [capability.kind, capability]));
  }, [props.capabilities]);
  const visibleCapabilities = React.useMemo(() => {
    return COMPOSER_AI_CAPABILITY_OPTIONS.map((option) => ({
      option,
      capability: capabilityByKind.get(option.value),
    }));
  }, [capabilityByKind]);
  const draftSignature = React.useMemo(
    () => composerCapabilityDraftSignature(draftDisabledKinds, draftDisabledTargetSelectors, draftTargetOrder),
    [draftDisabledKinds, draftDisabledTargetSelectors, draftTargetOrder],
  );
  const sessionSignature = React.useMemo(
    () => composerCapabilityDraftSignature(props.disabledKinds, props.disabledTargetSelectors, props.targetOrder),
    [props.disabledKinds, props.disabledTargetSelectors, props.targetOrder],
  );
  const hasUnsavedChanges = open && draftSignature !== baselineSignature;
  const isSaving = saveState === "saving";
  // activeDisabledKinds keeps the closed trigger bound to persisted session state instead of stale drafts.
  // activeDisabledKinds 让关闭状态下的触发器绑定已持久化会话状态，而不是遗留草稿。
  const activeDisabledKinds = open ? draftDisabledKinds : props.disabledKinds;
  const enabledCapabilityEntries = visibleCapabilities.filter(({ option, capability }) => {
    return Boolean(capability?.available) && !activeDisabledKinds.has(option.value);
  });
  const enabledCount = enabledCapabilityEntries.length;
  const triggerSubtitle = props.locale === "zh-CN"
    ? `${enabledCount}/${visibleCapabilities.length} 已启用`
    : `${enabledCount}/${visibleCapabilities.length} enabled`;
  const selectedEntry = visibleCapabilities.find(({ option }) => option.value === selectedKind) ?? visibleCapabilities[0];
  const selectedCapability = selectedEntry?.capability;
  const selectedTargets = orderedAiCapabilityTargets(selectedCapability, draftTargetOrder);
  const selectedEnabled = Boolean(selectedCapability?.available) && !draftDisabledKinds.has(selectedEntry.option.value);
  const targetDragSensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));
  const targetSortableIds = React.useMemo(
    () => selectedTargets.map((target) => target.targetSelector),
    [selectedTargets],
  );
  const capabilityTooltipRows = React.useMemo(
    () => composerCapabilityTooltipRows(
      visibleCapabilities,
      activeDisabledKinds,
      props.disabledTargetSelectors,
      props.targetOrder,
    ),
    [activeDisabledKinds, props.disabledTargetSelectors, props.targetOrder, visibleCapabilities],
  );
  // resetDraftFromProps refreshes the dialog draft from the latest session snapshot.
  // resetDraftFromProps 根据最新会话快照刷新弹窗草稿。
  const resetDraftFromProps = React.useCallback(() => {
    const nextDisabledKinds = cloneAiCapabilityKindSet(props.disabledKinds);
    const nextDisabledTargetSelectors = new Set(props.disabledTargetSelectors);
    const nextTargetOrder = cloneAiCapabilityTargetOrder(props.targetOrder);
    setDraftDisabledKinds(nextDisabledKinds);
    setDraftDisabledTargetSelectors(nextDisabledTargetSelectors);
    setDraftTargetOrder(nextTargetOrder);
    setBaselineSignature(composerCapabilityDraftSignature(nextDisabledKinds, nextDisabledTargetSelectors, nextTargetOrder));
    setSaveState("idle");
    setExternalUpdateNotice(false);
  }, [props.disabledKinds, props.disabledTargetSelectors, props.targetOrder]);
  // requestDialogOpenChange keeps dirty close attempts inside an explicit confirmation flow.
  // requestDialogOpenChange 将带未保存变更的关闭请求收束到显式确认流程。
  const requestDialogOpenChange = React.useCallback((nextOpen: boolean) => {
    if (nextOpen) {
      resetDraftFromProps();
      setConfirmCloseOpen(false);
      setOpen(true);
      return;
    }
    if (isSaving) {
      return;
    }
    if (hasUnsavedChanges) {
      setConfirmCloseOpen(true);
      return;
    }
    setOpen(false);
  }, [hasUnsavedChanges, isSaving, resetDraftFromProps]);
  // saveDraft persists the dialog-local capability draft and optionally completes a pending close request.
  // saveDraft 持久化弹窗本地能力草稿，并可选地完成一次待处理关闭请求。
  const saveDraft = React.useCallback(async (options?: { closeAfterSave?: boolean }) => {
    if (isSaving) {
      return false;
    }
    setSaveState("saving");
    const request: ComposerAiCapabilityConfigurationSave = {
      disabledAiCapabilityKinds: Array.from(draftDisabledKinds),
      disabledAiCapabilityTargetSelectors: Array.from(draftDisabledTargetSelectors),
      aiCapabilityTargetOrderOverrides: serializeAiCapabilityTargetOrder(draftTargetOrder),
    };
    const saved = await props.onSaveConfiguration(request);
    if (!saved) {
      setSaveState("idle");
      return false;
    }
    setBaselineSignature(draftSignature);
    setSaveState("saved");
    setExternalUpdateNotice(false);
    if (options?.closeAfterSave) {
      window.setTimeout(() => {
        setOpen(false);
        setConfirmCloseOpen(false);
        setSaveState("idle");
      }, 220);
      return true;
    }
    setConfirmCloseOpen(false);
    return true;
  }, [draftDisabledKinds, draftDisabledTargetSelectors, draftSignature, draftTargetOrder, isSaving, props]);
  // resetSavedButtonState returns the save button to its normal idle appearance after saved feedback.
  // resetSavedButtonState 在已保存反馈后将保存按钮恢复为普通空闲外观。
  const resetSavedButtonState = React.useCallback(() => {
    if (saveState === "saved") {
      setConfirmCloseOpen(false);
      setSaveState("idle");
    }
  }, [saveState]);
  // discardDraft closes the dialog without applying local draft changes.
  // discardDraft 在不应用本地草稿的情况下关闭弹窗。
  const discardDraft = React.useCallback(() => {
    setConfirmCloseOpen(false);
    setOpen(false);
    setSaveState("idle");
    setExternalUpdateNotice(false);
  }, []);
  // handleUseDefaultConfiguration stages the default capability configuration without persisting it.
  // handleUseDefaultConfiguration 将默认能力配置暂存为草稿但不直接持久化。
  const handleUseDefaultConfiguration = React.useCallback(() => {
    if (isSaving) {
      return;
    }
    setDraftDisabledKinds(defaultDisabledAiCapabilityKinds(props.defaultCapabilities));
    setDraftDisabledTargetSelectors(defaultDisabledAiCapabilityTargetSelectors(props.defaultCapabilities));
    setDraftTargetOrder(defaultAiCapabilityTargetOrder(props.defaultCapabilities));
    setExternalUpdateNotice(false);
    setSaveState("idle");
  }, [isSaving, props.defaultCapabilities]);
  // handleCapabilityToggle updates the dialog-local disabled capability set.
  // handleCapabilityToggle 更新弹窗本地禁用能力集合。
  const handleCapabilityToggle = React.useCallback((kind: AiCapabilityKind, enabled: boolean) => {
    if (isSaving) {
      return;
    }
    setDraftDisabledKinds((current) => {
      const next = new Set(current);
      if (enabled) {
        next.delete(kind);
      } else {
        next.add(kind);
      }
      return next;
    });
    setSaveState("idle");
  }, [isSaving]);
  // handleTargetToggle updates the dialog-local disabled target selector set.
  // handleTargetToggle 更新弹窗本地禁用目标选择器集合。
  const handleTargetToggle = React.useCallback((targetSelector: string, enabled: boolean) => {
    if (isSaving) {
      return;
    }
    setDraftDisabledTargetSelectors((current) => {
      const next = new Set(current);
      if (enabled) {
        next.delete(targetSelector);
      } else {
        next.add(targetSelector);
      }
      return next;
    });
    setSaveState("idle");
  }, [isSaving]);
  // handleTargetReorder replaces one dialog-local route order.
  // handleTargetReorder 替换单个弹窗本地路由顺序。
  const handleTargetReorder = React.useCallback((kind: AiCapabilityKind, targetSelectors: string[]) => {
    if (isSaving) {
      return;
    }
    setDraftTargetOrder((current) => {
      const next = new Map(current);
      const currentOrder = next.get(kind) ?? [];
      if (currentOrder.length === targetSelectors.length && currentOrder.every((selector, index) => selector === targetSelectors[index])) {
        return current;
      }
      next.set(kind, targetSelectors);
      return next;
    });
    setSaveState("idle");
  }, [isSaving]);
  // handleTargetDragEnd applies the dnd-kit order to the dialog-local route draft.
  // handleTargetDragEnd 将 dnd-kit 顺序应用到弹窗本地路由草稿。
  const handleTargetDragEnd = React.useCallback((event: DragEndEvent) => {
    if (isSaving) {
      return;
    }
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
    handleTargetReorder(selectedEntry.option.value, arrayMove(targetSortableIds, oldIndex, newIndex));
  }, [handleTargetReorder, isSaving, selectedEntry.option.value, targetSortableIds]);

  React.useEffect(() => {
    if (visibleCapabilities.some(({ option }) => option.value === selectedKind)) {
      return;
    }
    const firstCapability = visibleCapabilities[0]?.option.value;
    if (firstCapability) {
      setSelectedKind(firstCapability);
    }
  }, [selectedKind, visibleCapabilities]);

  React.useEffect(() => {
    if (!open || isSaving || sessionSignature === baselineSignature) {
      return;
    }
    if (!hasUnsavedChanges) {
      resetDraftFromProps();
      return;
    }
    setExternalUpdateNotice(true);
  }, [baselineSignature, hasUnsavedChanges, isSaving, open, resetDraftFromProps, sessionSignature]);

  return (
    <Dialog open={open} onOpenChange={requestDialogOpenChange}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DialogTrigger asChild>
            <button
              aria-label={props.ariaLabel}
              className="composer-capability-trigger composer-select-trigger capability"
              type="button"
            >
              <span className="composer-select-icon"><SlidersHorizontal size={15} /></span>
              <span className="composer-select-copy">
                <span className="composer-capability-icon-row" aria-label={triggerSubtitle}>
                  {visibleCapabilities.map(({ option, capability }) => (
                    <ComposerCapabilityIcon
                      enabled={Boolean(capability?.available) && !activeDisabledKinds.has(option.value)}
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
        </TooltipTrigger>
        <TooltipContent className="composer-control-tooltip" side="top">
          <strong>点击设置扩展能力</strong>
          {capabilityTooltipRows.length ? (
            capabilityTooltipRows.map((row) => <span key={row}>{row}</span>)
          ) : (
            <span>暂无已启用模块</span>
          )}
        </TooltipContent>
      </Tooltip>
      <DialogContent className="composer-capability-dialog">
        <DialogHeader>
          <DialogTitle>{props.locale === "zh-CN" ? "当前会话能力设置" : "Current session capabilities"}</DialogTitle>
          <DialogDescription className="composer-capability-dialog-note">
            {props.locale === "zh-CN"
              ? "调整排序与勾选后需要点击保存，保存后会写入当前会话的独立配置，并影响下一次对话的缓存命中。"
              : "Changes are saved to this session only after you click Save, and affect the next turn's cache hit rate."}
          </DialogDescription>
        </DialogHeader>
        <div className="composer-capability-dialog-grid">
          <aside className="composer-capability-dialog-left">
            {visibleCapabilities.map(({ option, capability }) => {
              const available = Boolean(capability?.available);
              const enabled = available && !draftDisabledKinds.has(option.value);
              const targetCount = capability?.targets.length ?? 0;
              const sourceLabel = capability?.native
                ? targetCount > 0
                  ? props.locale === "zh-CN"
                    ? `模型原生 + ${targetCount} 个目标`
                    : `Native + ${targetCount} targets`
                  : props.locale === "zh-CN"
                    ? "模型原生"
                    : "Native model"
                : props.locale === "zh-CN"
                  ? `${targetCount} 个目标`
                  : `${targetCount} targets`;
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
                    disabled={isSaving || !available}
                    onChange={(event) => handleCapabilityToggle(option.value, event.currentTarget.checked)}
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
                          ? sourceLabel
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
            {selectedTargets.length > 0 ? (
              <DndContext sensors={targetDragSensors} collisionDetection={closestCenter} onDragEnd={handleTargetDragEnd}>
                <SortableContext items={targetSortableIds} strategy={verticalListSortingStrategy}>
                  <div className="composer-capability-target-table">
                    {selectedTargets.map((target) => {
                      const targetEnabled = selectedEnabled && target.enabled && !draftDisabledTargetSelectors.has(target.targetSelector);
                      return (
                        <ComposerCapabilityTargetRow
                          disabled={!target.enabled}
                          enabled={targetEnabled}
                          key={target.targetSelector}
                          locked={isSaving}
                          selectedEnabled={selectedEnabled}
                          target={target}
                          onToggle={handleTargetToggle}
                        />
                      );
                    })}
                  </div>
                </SortableContext>
              </DndContext>
            ) : (
              <div className="composer-capability-dialog-empty">
                {selectedCapability?.native
                  ? props.locale === "zh-CN"
                    ? "当前能力由所选模型原生处理。"
                    : "This capability is handled natively by the selected model."
                  : props.locale === "zh-CN"
                    ? "当前能力暂无已配置目标。"
                    : "This capability has no configured targets."}
              </div>
            )}
          </section>
        </div>
        <DialogFooter className="composer-capability-dialog-footer">
          <span className="composer-capability-dialog-save-hint">
            {externalUpdateNotice
              ? props.locale === "zh-CN"
                ? "其他窗口已更新此会话配置，当前草稿尚未同步。"
                : "This session was updated elsewhere; the current draft is not synced."
              : isSaving
              ? props.locale === "zh-CN"
                ? "正在保存到当前会话..."
                : "Saving to this session..."
              : hasUnsavedChanges
                ? props.locale === "zh-CN"
                  ? "有未保存变更。"
                  : "Unsaved changes."
                : props.locale === "zh-CN"
                  ? "当前没有未保存变更。"
                  : "No unsaved changes."}
          </span>
          <button
            className="skill-action-button secondary composer-capability-default-button"
            disabled={isSaving || props.defaultCapabilities.length === 0}
            onClick={handleUseDefaultConfiguration}
            type="button"
          >
            {props.locale === "zh-CN" ? "使用默认配置" : "Use defaults"}
          </button>
          <button
            className={cn("skill-action-button composer-capability-save-button", saveState === "saved" && "saved")}
            disabled={isSaving || (!hasUnsavedChanges && saveState !== "saved")}
            onClick={() => {
              if (hasUnsavedChanges) {
                void saveDraft();
              }
            }}
            onMouseLeave={resetSavedButtonState}
            type="button"
          >
            {isSaving ? <Loader2 className="composer-capability-save-spinner" size={14} /> : saveState === "saved" ? <Check size={14} /> : <Save size={14} />}
            {isSaving
              ? props.locale === "zh-CN"
                ? "保存中"
                : "Saving"
              : saveState === "saved"
                ? props.locale === "zh-CN"
                  ? "已保存"
                  : "Saved"
                : props.locale === "zh-CN"
                  ? "保存"
                  : "Save"}
          </button>
        </DialogFooter>
      </DialogContent>
      <Dialog open={confirmCloseOpen} onOpenChange={setConfirmCloseOpen}>
        <DialogContent className="composer-capability-close-confirm-dialog">
          <DialogHeader>
            <DialogTitle>{props.locale === "zh-CN" ? "保存更改？" : "Save changes?"}</DialogTitle>
            <DialogDescription>
              {props.locale === "zh-CN"
                ? "当前会话能力设置有未保存变更。"
                : "This session capability dialog has unsaved changes."}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <button className="skill-action-button secondary" disabled={isSaving} onClick={discardDraft} type="button">
              {props.locale === "zh-CN" ? "不保存" : "Discard"}
            </button>
            <button className="skill-action-button secondary" disabled={isSaving} onClick={() => setConfirmCloseOpen(false)} type="button">
              {props.locale === "zh-CN" ? "继续编辑" : "Keep editing"}
            </button>
            <button className="skill-action-button" disabled={isSaving} onClick={() => void saveDraft({ closeAfterSave: true })} type="button">
              {isSaving ? <Loader2 className="composer-capability-save-spinner" size={14} /> : <Save size={14} />}
              {props.locale === "zh-CN" ? "保存" : "Save"}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Dialog>
  );
}

// ComposerCapabilityTargetRow renders one globally draggable capability target route row.
// ComposerCapabilityTargetRow 渲染单条整行可拖动的能力目标路由。
function ComposerCapabilityTargetRow(props: {
  // Whether the target route is globally disabled.
  // 当前目标路由是否全局不可用。
  disabled: boolean;
  // Whether the target route is enabled in the current session draft.
  // 当前目标路由是否在当前会话草稿中启用。
  enabled: boolean;
  // Whether saving currently locks row interactions.
  // 当前保存流程是否锁定行交互。
  locked: boolean;
  // Whether the owning capability is enabled.
  // 所属能力是否启用。
  selectedEnabled: boolean;
  // Target route summary displayed by the row.
  // 当前行展示的目标路由摘要。
  target: AiCapabilityTargetSummary;
  // Toggles the target route in the session dialog draft.
  // 开关会话弹窗草稿中的目标路由。
  onToggle: (targetSelector: string, enabled: boolean) => void;
}) {
  const sortable = useVerticalSortableItem(props.target.targetSelector, props.locked);
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
        disabled={props.locked || !props.selectedEnabled || !props.target.enabled}
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

// composerCapabilityTooltipRows builds compact enabled module rows for the capability trigger tooltip.
// composerCapabilityTooltipRows 为能力触发器 tooltip 构建紧凑的已启用模块行。
function composerCapabilityTooltipRows(
  entries: Array<{ option: ComposerAiCapabilityOption; capability: AiCapabilitySummary | undefined }>,
  disabledKinds: Set<AiCapabilityKind>,
  disabledTargetSelectors: Set<string>,
  targetOrder: Map<AiCapabilityKind, string[]>,
): string[] {
  return entries.flatMap(({ option, capability }) => {
    if (!capability?.available || disabledKinds.has(option.value)) {
      return [];
    }
    const sourceLabels = capability.native ? ["模型原生"] : [];
    const targetLabels = orderedAiCapabilityTargets(capability, targetOrder)
      .filter((target) => target.enabled && !disabledTargetSelectors.has(target.targetSelector))
      .map(composerCapabilityTargetTooltipLabel);
    const routeLabels = sourceLabels.concat(targetLabels);
    if (!routeLabels.length) {
      return [];
    }
    const visibleLabels = routeLabels.slice(0, 4);
    const suffix = routeLabels.length > visibleLabels.length ? "..." : "";
    return [`${option.labelZh}：${visibleLabels.join("、")}${suffix}`];
  });
}

// composerCapabilityTargetTooltipLabel returns the shortest useful display label for one capability target.
// composerCapabilityTargetTooltipLabel 返回单个能力目标最短可用展示标签。
function composerCapabilityTargetTooltipLabel(target: AiCapabilityTargetSummary): string {
  return target.targetName || target.targetAlias || target.vendorName || target.vendorId;
}

// cloneAiCapabilityKindSet creates a mutable dialog-local copy of a capability set.
// cloneAiCapabilityKindSet 创建一个可变的弹窗本地能力集合副本。
function cloneAiCapabilityKindSet(value: Set<AiCapabilityKind>): Set<AiCapabilityKind> {
  return new Set(value);
}

// cloneAiCapabilityTargetOrder creates a mutable dialog-local copy of target order maps.
// cloneAiCapabilityTargetOrder 创建一个可变的弹窗本地目标顺序映射副本。
function cloneAiCapabilityTargetOrder(value: Map<AiCapabilityKind, string[]>): Map<AiCapabilityKind, string[]> {
  return new Map(Array.from(value.entries()).map(([kind, selectors]) => [kind, [...selectors]]));
}

// composerCapabilityDraftSignature builds a stable comparison key for one dialog draft.
// composerCapabilityDraftSignature 为单个弹窗草稿构建稳定对比键。
function composerCapabilityDraftSignature(
  disabledKinds: Set<AiCapabilityKind>,
  disabledTargetSelectors: Set<string>,
  targetOrder: Map<AiCapabilityKind, string[]>,
): string {
  return JSON.stringify({
    disabledKinds: Array.from(disabledKinds).sort(),
    disabledTargetSelectors: Array.from(disabledTargetSelectors).sort(),
    targetOrder: COMPOSER_AI_CAPABILITY_OPTIONS.map((option) => [
      option.value,
      targetOrder.get(option.value) ?? [],
    ]),
  });
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
  // Whether the closed trigger should suppress subtitle text.
  // 闭合触发器是否隐藏副标题文本。
  hideTriggerSubtitle?: boolean;
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
  // Optional tooltip shown when hovering the closed trigger.
  // 鼠标悬停闭合触发器时显示的可选提示。
  tooltip?: React.ReactNode;
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
      {props.tooltip ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <SelectTrigger
              aria-label={props.ariaLabel}
              className={cn("composer-select-trigger", props.className)}
            >
              {props.icon ? <span className="composer-select-icon">{props.icon}</span> : null}
              <span className={cn("composer-select-copy", !props.icon && "centered")}>
                <span className="composer-select-label">{selectedOption?.label ?? props.fallbackLabel}</span>
                {!props.hideTriggerSubtitle && (props.triggerSubtitle || selectedOption?.subtitle) ? (
                  <span className="composer-select-subtitle">{props.triggerSubtitle ?? selectedOption?.subtitle}</span>
                ) : null}
              </span>
            </SelectTrigger>
          </TooltipTrigger>
          <TooltipContent className="composer-control-tooltip" side="top">{props.tooltip}</TooltipContent>
        </Tooltip>
      ) : (
        <SelectTrigger
          aria-label={props.ariaLabel}
          className={cn("composer-select-trigger", props.className)}
        >
          {props.icon ? <span className="composer-select-icon">{props.icon}</span> : null}
          <span className={cn("composer-select-copy", !props.icon && "centered")}>
            <span className="composer-select-label">{selectedOption?.label ?? props.fallbackLabel}</span>
            {!props.hideTriggerSubtitle && (props.triggerSubtitle || selectedOption?.subtitle) ? (
              <span className="composer-select-subtitle">{props.triggerSubtitle ?? selectedOption?.subtitle}</span>
            ) : null}
          </span>
        </SelectTrigger>
      )}
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
