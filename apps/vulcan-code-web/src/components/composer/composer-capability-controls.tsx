import React from "react";
import {
  Check,
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
import { useI18n, type LocaleCode, type TranslationKey } from "@/i18n";
import type {
  AiCapabilitySummary,
  AiCapabilityTargetSummary,
  ContextUsageSnapshot,
} from "@/types";
import {
  COMPOSER_AI_CAPABILITY_OPTIONS,
  orderedAiCapabilityTargets,
  type AiCapabilityKind,
  type ComposerAiCapabilityConfigurationSave,
  type ComposerAiCapabilityOption,
  type ComposerSelectOption,
} from "@/components/composer/composer-model";

import { useComposerCapabilityDraft } from "./use-composer-capability-draft";

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

// ComposerAiCapabilityControl renders the WebUI-aligned capability dialog.
// ComposerAiCapabilityControl 渲染对齐 WebUI 的能力设置对话框。
export function ComposerAiCapabilityControl(props: {
  // Stable conversation or new-conversation draft slot that owns dialog state.
  // 拥有弹窗状态的稳定会话或新建会话草稿槽。
  scopeKey: string;
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
  // Session-synchronized exact target binding grouped by capability kind.
  // 按能力类型分组、与会话同步的精确目标绑定。
  targetBindings: Map<AiCapabilityKind, string>;
  // Persists one session-scoped capability configuration draft.
  // 持久化单个会话级能力配置草稿。
  onSaveConfiguration: (request: ComposerAiCapabilityConfigurationSave) => Promise<boolean>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // selectedKind stores the capability currently shown in the right pane.
  // selectedKind 保存右侧面板当前展示的能力。
  const [selectedKind, setSelectedKind] = React.useState<AiCapabilityKind>("web_search");
  // Capability draft controller owns scoped dialog state, save concurrency, and external updates.
  // 能力草稿控制器管理有作用域的弹窗状态、保存并发与外部更新。
  const {
    open,
    confirmCloseOpen,
    draftDisabledKinds,
    draftDisabledTargetSelectors,
    draftTargetBindings,
    activeDisabledKinds,
    hasUnsavedChanges,
    isSaving,
    saveState,
    externalUpdateNotice,
    requestOpenChange: requestDialogOpenChange,
    keepEditing,
    saveDraft,
    resetSavedState: resetSavedButtonState,
    discardDraft,
    useDefaultConfiguration: handleUseDefaultConfiguration,
    setCapabilityEnabled: handleCapabilityToggle,
    setTargetEnabled: handleTargetToggle,
  } = useComposerCapabilityDraft({
    scopeKey: props.scopeKey,
    disabledKinds: props.disabledKinds,
    disabledTargetSelectors: props.disabledTargetSelectors,
    targetBindings: props.targetBindings,
    defaultCapabilities: props.defaultCapabilities,
    onSaveConfiguration: props.onSaveConfiguration,
  });
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
    return Boolean(capability?.available) && !activeDisabledKinds.has(option.value);
  });
  const enabledCount = enabledCapabilityEntries.length;
  const triggerSubtitle = formatComposerTranslation(t, "composer.capability.enabledCount", {
    enabled: String(enabledCount),
    total: String(visibleCapabilities.length),
  });
  const selectedEntry = visibleCapabilities.find(({ option }) => option.value === selectedKind) ?? visibleCapabilities[0];
  const selectedCapability = selectedEntry?.capability;
  const selectedTargets = orderedAiCapabilityTargets(selectedCapability, draftTargetBindings);
  const selectedEnabled = Boolean(selectedCapability?.available) && !draftDisabledKinds.has(selectedEntry.option.value);
  const capabilityTooltipRows = React.useMemo(
    () => composerCapabilityTooltipRows(
      visibleCapabilities,
      activeDisabledKinds,
      props.disabledTargetSelectors,
      props.targetBindings,
      props.locale,
      t,
    ),
    [activeDisabledKinds, props.disabledTargetSelectors, props.locale, props.targetBindings, t, visibleCapabilities],
  );
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
          <strong>{t("composer.capability.tooltipTitle")}</strong>
          {capabilityTooltipRows.length ? (
            capabilityTooltipRows.map((row) => <span key={row}>{row}</span>)
          ) : (
            <span>{t("composer.capability.tooltipEmpty")}</span>
          )}
        </TooltipContent>
      </Tooltip>
      <DialogContent className="composer-capability-dialog">
        <DialogHeader>
          <DialogTitle>{t("composer.capability.dialogTitle")}</DialogTitle>
          <DialogDescription className="composer-capability-dialog-note">
            {t("composer.capability.dialogDescription")}
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
                  ? formatComposerTranslation(t, "composer.capability.source.nativeWithTargets", { count: String(targetCount) })
                  : t("composer.capability.source.native")
                : formatComposerTranslation(t, "composer.capability.source.targets", { count: String(targetCount) });
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
                          : t("composer.capability.source.noTargets")}
                      </small>
                    </span>
                  </span>
                </div>
              );
            })}
          </aside>
          <section className="composer-capability-dialog-right">
            {selectedTargets.length > 0 ? (
              <div className="composer-capability-target-table">
                {selectedTargets.map((target) => {
                  const exactBinding = draftTargetBindings.get(selectedEntry.option.value);
                  const targetEnabled = selectedEnabled
                    && target.enabled
                    && exactBinding === target.targetSelector
                    && !draftDisabledTargetSelectors.has(target.targetSelector);
                  return (
                    <ComposerCapabilityTargetRow
                      capabilityKind={selectedEntry.option.value}
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
            ) : (
              <div className="composer-capability-dialog-empty">
                {selectedCapability?.native
                  ? t("composer.capability.empty.native")
                  : t("composer.capability.empty.noTargets")}
              </div>
            )}
          </section>
        </div>
        <DialogFooter className="composer-capability-dialog-footer">
          <span className="composer-capability-dialog-save-hint">
            {externalUpdateNotice
              ? t("composer.capability.saveHint.externalUpdate")
              : isSaving
              ? t("composer.capability.saveHint.saving")
              : hasUnsavedChanges
                ? t("composer.capability.saveHint.unsaved")
                : t("composer.capability.saveHint.clean")}
          </span>
          <button
            className="skill-action-button secondary composer-capability-default-button"
            disabled={isSaving || props.defaultCapabilities.length === 0}
            onClick={handleUseDefaultConfiguration}
            type="button"
          >
            {t("composer.capability.useDefaults")}
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
              ? t("composer.capability.save.saving")
              : saveState === "saved"
                ? t("composer.capability.save.saved")
                : t("composer.capability.save")}
          </button>
        </DialogFooter>
      </DialogContent>
      <Dialog open={confirmCloseOpen} onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          keepEditing();
        }
      }}>
        <DialogContent className="composer-capability-close-confirm-dialog">
          <DialogHeader>
            <DialogTitle>{t("composer.capability.close.title")}</DialogTitle>
            <DialogDescription>
              {t("composer.capability.close.description")}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <button className="skill-action-button secondary" disabled={isSaving} onClick={discardDraft} type="button">
              {t("composer.capability.close.discard")}
            </button>
            <button className="skill-action-button secondary" disabled={isSaving} onClick={keepEditing} type="button">
              {t("composer.capability.close.keepEditing")}
            </button>
            <button className="skill-action-button" disabled={isSaving} onClick={() => void saveDraft({ closeAfterSave: true })} type="button">
              {isSaving ? <Loader2 className="composer-capability-save-spinner" size={14} /> : <Save size={14} />}
              {t("composer.capability.save")}
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
  // Semantic capability kind owning this exact binding.
  // 拥有该精确绑定的语义能力类型。
  capabilityKind: AiCapabilityKind;
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
  // Selects or clears the exact target binding in the session dialog draft.
  // 选择或清除会话弹窗草稿中的精确目标绑定。
  onToggle: (kind: AiCapabilityKind, targetSelector: string, enabled: boolean) => void;
}) {
  return (
    <div
      className={cn("composer-capability-target-row", props.disabled && "disabled")}
    >
      <input
        checked={props.enabled}
        disabled={props.locked || !props.selectedEnabled || !props.target.enabled}
        onChange={(event) => props.onToggle(
          props.capabilityKind,
          props.target.targetSelector,
          event.currentTarget.checked,
        )}
        type="radio"
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
  targetBindings: Map<AiCapabilityKind, string>,
  locale: LocaleCode,
  t: (key: TranslationKey) => string,
): string[] {
  return entries.flatMap(({ option, capability }) => {
    if (!capability?.available || disabledKinds.has(option.value)) {
      return [];
    }
    const sourceLabels = capability.native ? [t("composer.capability.source.native")] : [];
    const selectedTarget = targetBindings.get(option.value);
    const targetLabels = capability.targets
      .filter((target) => target.targetSelector === selectedTarget)
      .filter((target) => target.enabled && !disabledTargetSelectors.has(target.targetSelector))
      .map(composerCapabilityTargetTooltipLabel);
    const routeLabels = sourceLabels.concat(targetLabels);
    if (!routeLabels.length) {
      return [];
    }
    const visibleLabels = routeLabels.slice(0, 4);
    const suffix = routeLabels.length > visibleLabels.length ? "..." : "";
    const optionLabel = locale === "zh-CN" ? option.labelZh : option.labelEn;
    return [`${optionLabel}${t("global.labelSeparator")}${visibleLabels.join(t("global.listDelimiter"))}${suffix}`];
  });
}

// formatComposerTranslation replaces simple named placeholders in localized composer templates.
// formatComposerTranslation 替换输入区本地化模板中的简单具名占位符。
function formatComposerTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}

// composerCapabilityTargetTooltipLabel returns the shortest useful display label for one capability target.
// composerCapabilityTargetTooltipLabel 返回单个能力目标最短可用展示标签。
function composerCapabilityTargetTooltipLabel(target: AiCapabilityTargetSummary): string {
  return target.targetName || target.targetAlias || target.vendorName || target.vendorId;
}

// ComposerSelectControl renders one WebUI-aligned compact composer selector.
// ComposerSelectControl 渲染一个对齐 WebUI 的紧凑输入区选择器。
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
  // CSS class suffix controlling the WebUI-aligned fixed width.
  // 控制 WebUI 对齐固定宽度的 CSS 类后缀。
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
