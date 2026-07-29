import React from "react";
import {
  FileSearch,
  Image,
  Music,
  Search,
  SlidersHorizontal,
  Video,
  Volume2,
} from "lucide-react";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from "@/components/ui/select";
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
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { ComposerSelectOption } from "@/components/composer/composer-model";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import {
  loadPublicAiSessionProviderToolSelections,
  loadPublicAiSettings,
  savePublicAiSessionProviderToolSelections,
} from "@/service/settings-actions-service";
import type {
  ContextUsageSnapshot,
  PublicAiProviderToolCapabilityKind,
  PublicAiProviderToolTargetSelection,
  PublicAiSettingsSnapshot,
} from "@/types";

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
  const rows = props.snapshot?.tooltipRows.length
    ? props.snapshot.tooltipRows
    : [t("contextUsage.waiting")];
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

// ComposerAiCapabilityControl restores the established session capability dialog with one target per capability.
// ComposerAiCapabilityControl 恢复既有会话能力弹窗，并确保每项能力只选择一个目标。
export function ComposerAiCapabilityControl(props: {
  // Current persisted conversation, or null for a new conversation draft.
  // 当前持久化会话；新会话草稿时为空。
  conversationId: string | null;
  // Accessible trigger label.
  // 触发器无障碍标签。
  ariaLabel: string;
}) {
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [selectedCapability, setSelectedCapability] =
    React.useState<PublicAiProviderToolCapabilityKind>("web_search");
  const [draft, setDraft] = React.useState<Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection>
  >>({});
  const [baselineSignature, setBaselineSignature] = React.useState("{}");
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let active = true;
    void Promise.all([
      loadPublicAiSettings(),
      props.conversationId
        ? loadPublicAiSessionProviderToolSelections(props.conversationId)
        : Promise.resolve(null),
    ])
      .then(([nextSnapshot, sessionSelections]) => {
        if (!active) {
          return;
        }
        const nextDraft = filterEnabledCurrentSelections(
          nextSnapshot,
          sessionSelections ?? nextSnapshot.providerToolSelections,
        );
        const capabilities = orderedProviderToolCapabilities(nextSnapshot);
        setSnapshot(nextSnapshot);
        setDraft(nextDraft);
        setBaselineSignature(providerToolSelectionsSignature(nextDraft));
        setSelectedCapability((current) => (
          capabilities.includes(current) ? current : capabilities[0] ?? "web_search"
        ));
        setError(null);
      })
      .catch((reason) => {
        if (active) {
          setError(errorMessage(reason));
        }
      });
    return () => {
      active = false;
    };
  }, [open, props.conversationId]);

  const capabilities = snapshot ? orderedProviderToolCapabilities(snapshot) : [];
  const selectedTargets = snapshot?.providerToolTargets.filter(
    (target) => target.enabled
      && target.target.contract.capability === selectedCapability,
  ) ?? [];
  const enabledCount = Object.keys(draft).length;
  const hasUnsavedChanges =
    providerToolSelectionsSignature(draft) !== baselineSignature;
  const save = React.useCallback(async () => {
    if (!props.conversationId) {
      return;
    }
    setSaving(true);
    try {
      const next = await savePublicAiSessionProviderToolSelections({
        conversationId: props.conversationId,
        selections: draft,
      });
      setDraft(next);
      setBaselineSignature(providerToolSelectionsSignature(next));
      setError(null);
      setOpen(false);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setSaving(false);
    }
  }, [draft, props.conversationId]);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
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
                <span className="composer-capability-icon-row">
                  {capabilities.map((capability) => (
                    <span
                      className={cn(
                        "composer-capability-colored-icon",
                        draft[capability] ? "enabled" : "disabled",
                      )}
                      key={capability}
                      style={{
                        "--capability-color": providerToolCapabilityColor(capability),
                      } as React.CSSProperties}
                    >
                      {providerToolCapabilityIcon(capability, 11)}
                    </span>
                  ))}
                </span>
              </span>
            </button>
          </DialogTrigger>
        </TooltipTrigger>
        <TooltipContent className="composer-control-tooltip" side="top">
          <strong>{t("composer.capability.tooltipTitle")}</strong>
          <span>
            {enabledCount} / {capabilities.length}
          </span>
        </TooltipContent>
      </Tooltip>
      <DialogContent className="composer-capability-dialog">
        <DialogHeader>
          <DialogTitle>{t("composer.capability.dialogTitle")}</DialogTitle>
          <DialogDescription className="composer-capability-dialog-note">
            {t("composer.capability.dialogDescription")}
          </DialogDescription>
        </DialogHeader>
        {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
        <div className="composer-capability-dialog-grid">
          <aside className="composer-capability-dialog-left">
            {capabilities.map((capability) => {
              const enabled = Boolean(draft[capability]);
              const targetCount = snapshot?.providerToolTargets.filter(
                (target) => target.enabled
                  && target.target.contract.capability === capability,
              ).length ?? 0;
              return (
                <div
                  className={cn(
                    "composer-capability-kind-row",
                    selectedCapability === capability && "selected",
                  )}
                  key={capability}
                  onClick={() => setSelectedCapability(capability)}
                  role="button"
                  tabIndex={0}
                >
                  <input
                    checked={enabled}
                    disabled={saving || targetCount === 0}
                    onChange={(event) => {
                      event.stopPropagation();
                      const checked = event.currentTarget.checked;
                      setDraft((current) => {
                        const next = { ...current };
                        if (!checked) {
                          delete next[capability];
                          return next;
                        }
                        const first = snapshot?.providerToolTargets.find(
                          (target) => target.enabled
                            && target.target.contract.capability === capability,
                        );
                        if (first) {
                          next[capability] = {
                            connection_id: first.connectionId,
                            target_id: first.target.target_id,
                          };
                        }
                        return next;
                      });
                    }}
                    onClick={(event) => event.stopPropagation()}
                    type="checkbox"
                  />
                  <span className="composer-capability-kind-select">
                    <span className={cn(
                      "composer-capability-colored-icon",
                      enabled ? "enabled" : "disabled",
                    )} style={{
                      "--capability-color": providerToolCapabilityColor(capability),
                    } as React.CSSProperties}>
                      {providerToolCapabilityIcon(capability, 16)}
                    </span>
                    <span className="composer-capability-kind-copy">
                      <span>{providerToolCapabilityLabel(capability, t)}</span>
                      <small>{targetCount} {t("settings.providerTools.targets")}</small>
                    </span>
                  </span>
                </div>
              );
            })}
          </aside>
          <section className="composer-capability-dialog-right">
            {selectedTargets.length ? (
              <div className="composer-capability-target-table">
                {selectedTargets.map((target) => {
                  const selection = draft[selectedCapability];
                  const checked = selection?.connection_id === target.connectionId
                    && selection.target_id === target.target.target_id;
                  return (
                    <label
                      className="composer-capability-target-row"
                      key={`${target.connectionId}/${target.target.target_id}`}
                    >
                      <input
                        checked={checked}
                        disabled={saving}
                        name={`composer-capability-${selectedCapability}`}
                        onChange={() => setDraft((current) => ({
                          ...current,
                          [selectedCapability]: {
                            connection_id: target.connectionId,
                            target_id: target.target.target_id,
                          },
                        }))}
                        type="radio"
                      />
                      <span className="composer-capability-target-copy">
                        <span>{target.target.display_name}</span>
                        <small>{target.connectionDisplayName}</small>
                      </span>
                    </label>
                  );
                })}
              </div>
            ) : (
              <div className="composer-capability-dialog-empty">
                {t("composer.capability.empty.noTargets")}
              </div>
            )}
          </section>
        </div>
        <DialogFooter className="composer-capability-dialog-footer">
          <span className="composer-capability-dialog-save-hint">
            {props.conversationId
              ? hasUnsavedChanges
                ? t("composer.capability.saveHint.unsaved")
                : t("composer.capability.saveHint.clean")
              : t("composer.capability.sessionRequired")}
          </span>
          <button
            className="skill-action-button"
            disabled={saving || !props.conversationId || !hasUnsavedChanges}
            onClick={() => void save()}
            type="button"
          >
            {saving
              ? t("composer.capability.save.saving")
              : t("composer.capability.save")}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// providerToolSelectionsSignature serializes a selection map deterministically for dirty checks.
// providerToolSelectionsSignature 以确定性方式序列化选择映射，用于脏检查。
function providerToolSelectionsSignature(
  selections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection>
  >,
): string {
  return JSON.stringify(
    Object.entries(selections)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([capability, selection]) => [
        capability,
        selection?.connection_id ?? "",
        selection?.target_id ?? "",
      ]),
  );
}

// orderedProviderToolCapabilities returns stable capabilities that have at least one current target.
// orderedProviderToolCapabilities 返回至少含一个当前目标的稳定能力列表。
function orderedProviderToolCapabilities(
  snapshot: PublicAiSettingsSnapshot,
): PublicAiProviderToolCapabilityKind[] {
  return Array.from(new Set(
    snapshot.providerToolTargets
      .filter((target) => target.enabled)
      .map((target) => target.target.contract.capability),
  )).sort();
}

// filterEnabledCurrentSelections drops historical, removed, and globally disabled targets.
// filterEnabledCurrentSelections 过滤历史、已移除及全局禁用的目标。
function filterEnabledCurrentSelections(
  snapshot: PublicAiSettingsSnapshot,
  selections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection>
  >,
): Partial<Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection>> {
  const current = new Map(snapshot.providerToolTargets
    .filter((target) => target.enabled)
    .map((target) => [
      `${target.connectionId.length}:${target.connectionId}:${target.target.target_id.length}:${target.target.target_id}`,
      target.target.contract.capability,
    ]));
  return Object.fromEntries(Object.entries(selections).filter(([capability, selection]) => {
    if (!selection) {
      return false;
    }
    const key = `${selection.connection_id.length}:${selection.connection_id}:${selection.target_id.length}:${selection.target_id}`;
    return current.get(key) === capability;
  }));
}

// providerToolCapabilityColor restores the established capability color tokens.
// providerToolCapabilityColor 恢复既有能力颜色令牌。
function providerToolCapabilityColor(capability: PublicAiProviderToolCapabilityKind): string {
  if (capability === "web_search" || capability === "web_extract") {
    return "#0ea5e9";
  }
  if (capability === "media_analyze") {
    return "#6366f1";
  }
  if (capability === "image_generate" || capability === "image_edit") {
    return "#ec4899";
  }
  if (capability === "video_generate") {
    return "#f97316";
  }
  if (capability === "speech_synthesize" || capability === "speech_transcribe") {
    return "#14b8a6";
  }
  if (capability === "music_cover") {
    return "#84cc16";
  }
  return "#a855f7";
}

// providerToolCapabilityIcon returns the established capability-family icon.
// providerToolCapabilityIcon 返回既有能力族图标。
function providerToolCapabilityIcon(
  capability: PublicAiProviderToolCapabilityKind,
  size: number,
): React.ReactNode {
  if (capability === "web_search") {
    return <Search size={size} />;
  }
  if (capability === "web_extract") {
    return <FileSearch size={size} />;
  }
  if (capability === "media_analyze" || capability === "image_generate" || capability === "image_edit") {
    return <Image size={size} />;
  }
  if (capability === "video_generate") {
    return <Video size={size} />;
  }
  if (capability === "speech_synthesize" || capability === "speech_transcribe") {
    return <Volume2 size={size} />;
  }
  return <Music size={size} />;
}

// providerToolCapabilityLabel localizes one provider tool capability.
// providerToolCapabilityLabel 本地化一项供应商工具能力。
function providerToolCapabilityLabel(
  capability: PublicAiProviderToolCapabilityKind,
  t: ReturnType<typeof useI18n>["t"],
): string {
  const keys: Partial<Record<
    PublicAiProviderToolCapabilityKind,
    Parameters<typeof t>[0]
  >> = {
    web_search: "settings.providerTools.webSearch",
    web_extract: "settings.providerTools.webExtract",
    media_analyze: "settings.providerTools.mediaAnalyze",
    image_generate: "settings.providerTools.imageGenerate",
    video_generate: "settings.providerTools.videoGenerate",
    speech_synthesize: "settings.providerTools.speechSynthesize",
    music_generate: "settings.providerTools.musicGenerate",
  };
  const key = keys[capability];
  return key ? t(key) : capability;
}

// errorMessage converts a rejected value into a visible diagnostic.
// errorMessage 将被拒绝的值转换为可见诊断。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
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
  // Whether interaction with this selector is disabled.
  // 是否禁用此选择器的交互。
  disabled?: boolean;
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
  const selectedOption =
    props.options.find((option) => option.value === props.value) ?? props.options[0];

  React.useEffect(() => {
    if (props.disabled || props.openSignal === undefined || props.openSignal <= 0) {
      return;
    }
    setOpen(true);
  }, [props.disabled, props.openSignal]);

  const trigger = (
    <SelectTrigger
      aria-label={props.ariaLabel}
      className={cn("composer-select-trigger", props.className)}
    >
      {props.icon ? <span className="composer-select-icon">{props.icon}</span> : null}
      <span className={cn("composer-select-copy", !props.icon && "centered")}>
        <span className="composer-select-label">
          {selectedOption?.label ?? props.fallbackLabel}
        </span>
        {!props.hideTriggerSubtitle &&
        (props.triggerSubtitle || selectedOption?.subtitle) ? (
          <span className="composer-select-subtitle">
            {props.triggerSubtitle ?? selectedOption?.subtitle}
          </span>
        ) : null}
      </span>
    </SelectTrigger>
  );

  return (
    <Select
      disabled={props.disabled}
      onOpenChange={setOpen}
      onValueChange={props.onValueChange}
      open={props.disabled ? false : open}
      value={props.value}
    >
      {props.tooltip ? (
        <Tooltip>
          <TooltipTrigger asChild>{trigger}</TooltipTrigger>
          <TooltipContent className="composer-control-tooltip" side="top">
            {props.tooltip}
          </TooltipContent>
        </Tooltip>
      ) : (
        trigger
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
