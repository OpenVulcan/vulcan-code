import React from "react";
import {
  AlertTriangle,
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
  SelectValue,
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
import type {
  ComposerProviderToolSettingsSnapshot,
  ComposerSelectOption,
} from "@/components/composer/composer-model";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import {
  loadPublicAiSessionProviderToolSelections,
  loadPublicAiSettings,
  savePublicAiSessionProviderToolSelections,
} from "@/service/settings-actions-service";
import type {
  ContextUsageBreakdownItem,
  ContextUsageSnapshot,
  PublicAiProviderToolCapabilityKind,
  PublicAiProviderToolTargetSnapshot,
  PublicAiProviderToolTargetSelection,
  PublicAiSessionProviderToolConflict,
  PublicAiSettingsSnapshot,
} from "@/types";

// ContextUsageOrb renders a compact context budget indicator beside the composer.
// ContextUsageOrb 在输入区旁渲染紧凑的上下文预算指示器。
export function ContextUsageOrb(props: {
  // Context usage snapshot projected by the service.
  // 服务投影出的上下文占用快照。
  snapshot: ContextUsageSnapshot | null;
}) {
  if (!props.snapshot) {
    return null;
  }
  const snapshot = props.snapshot;
  const breakdown = contextUsageBreakdown(snapshot);
  const usageBasisPoints = contextUsageBasisPoints(snapshot);
  const percent = usageBasisPoints / 100;
  const title = snapshot.tooltipTitle || "上下文容量";
  const detailVisible = breakdown.length > 0;
  const thresholdState = percent >= 100
    ? "full"
    : percent >= 95
      ? "critical"
      : percent >= 90
        ? "warning"
        : "normal";
  const style = {
    "--context-usage-progress": `${percent * 3.6}deg`,
    "--context-usage-percent": `${percent}%`,
  } as React.CSSProperties;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          aria-label={title}
          className={cn("context-usage-orb", `severity-${snapshot.severity}`)}
          data-threshold-state={thresholdState}
          style={style}
          type="button"
        />
      </TooltipTrigger>
      <TooltipContent
        align="end"
        className="context-usage-tooltip"
        showArrow={false}
        side="top"
        sideOffset={8}
      >
        <div className="context-usage-card-header">
          <strong>{title}</strong>
          <span>{formatContextUsageTotal(snapshot)}</span>
        </div>
        <ContextUsageProgressBar
          breakdown={breakdown}
          usageBasisPoints={usageBasisPoints}
        />
        {detailVisible ? (
          <div className="context-usage-breakdown">
            {breakdown.map((item) => (
              <div className="context-usage-breakdown-row" key={item.kind}>
                <span className="context-usage-breakdown-label">
                  <i className={`context-usage-dot kind-${item.kind}`} />
                  {item.label}
                </span>
                <span>{formatBasisPoints(item.basisPoints)}</span>
              </div>
            ))}
          </div>
        ) : null}
        <div className="context-usage-cache-row">
          <span>平均缓存命中率</span>
          <strong>
            {snapshot.cacheHitBasisPoints == null
              ? "暂无数据"
              : formatBasisPoints(snapshot.cacheHitBasisPoints)}
          </strong>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

// ContextUsageProgressBar renders used, unused, category, and threshold states in one track.
// ContextUsageProgressBar 在同一轨道中渲染已用、未用、分类及阈值状态。
function ContextUsageProgressBar(props: {
  // Normalized category rows for the active snapshot.
  // 当前快照的标准化分类行。
  breakdown: ContextUsageBreakdownItem[];
  // Normalized context usage in basis points.
  // 标准化后的上下文占用万分比。
  usageBasisPoints: number;
}) {
  const percent = props.usageBasisPoints / 100;
  return (
    <div
      aria-label={`上下文已使用 ${percent.toFixed(1)}%`}
      className="context-usage-progress-track"
      role="progressbar"
      aria-valuemax={100}
      aria-valuemin={0}
      aria-valuenow={percent}
    >
      <div className="context-usage-threshold-zone threshold-90" />
      <div className="context-usage-threshold-zone threshold-95" />
      <div className="context-usage-progress-fill" style={{ width: `${percent}%` }}>
        {props.breakdown.length > 0
          ? props.breakdown.map((item) => (
              <span
                className={`context-usage-segment kind-${item.kind}`}
                key={item.kind}
                style={{ width: `${item.basisPoints / 100}%` }}
              />
            ))
          : null}
      </div>
      <i className="context-usage-threshold-marker marker-90" />
      <i className="context-usage-threshold-marker marker-95" />
      <i className="context-usage-threshold-marker marker-100" />
    </div>
  );
}

// contextUsageBasisPoints normalizes both current and legacy service snapshots.
// contextUsageBasisPoints 标准化当前版与旧版服务快照中的上下文占用比例。
function contextUsageBasisPoints(snapshot: ContextUsageSnapshot): number {
  const value = typeof snapshot.usageBasisPoints === "number"
    && Number.isFinite(snapshot.usageBasisPoints)
    ? snapshot.usageBasisPoints
    : snapshot.percent * 100;
  return Math.max(0, Math.min(10_000, Math.round(value)));
}

// contextUsageBreakdown returns an empty legacy breakdown when the service omits the field.
// contextUsageBreakdown 在服务未提供字段时返回空的旧版分类明细。
function contextUsageBreakdown(snapshot: ContextUsageSnapshot): ContextUsageBreakdownItem[] {
  return Array.isArray(snapshot.breakdown) ? snapshot.breakdown : [];
}

// formatContextUsageTotal renders the compact current-window summary.
// formatContextUsageTotal 渲染紧凑的当前窗口摘要。
function formatContextUsageTotal(snapshot: ContextUsageSnapshot): string {
  const usageBasisPoints = contextUsageBasisPoints(snapshot);
  if (typeof snapshot.usedTokens !== "number" || !Number.isFinite(snapshot.usedTokens)) {
    return formatBasisPoints(usageBasisPoints);
  }
  const used = formatContextTokenCount(snapshot.usedTokens);
  if (typeof snapshot.maxTokens !== "number" || !Number.isFinite(snapshot.maxTokens)) {
    return `${used} Token`;
  }
  return `${used}/${formatContextTokenCount(snapshot.maxTokens)} (${formatBasisPoints(usageBasisPoints)})`;
}

// formatContextTokenCount formats large token counts with Chinese ten-thousand and hundred-million units.
// formatContextTokenCount 使用中文万与亿单位格式化较大的 Token 数量。
function formatContextTokenCount(value: number): string {
  if (value >= 100_000_000) {
    return `${formatCompactDecimal(value / 100_000_000)}亿`;
  }
  if (value >= 10_000) {
    return `${formatCompactDecimal(value / 10_000)}万`;
  }
  return Math.max(0, Math.round(value)).toLocaleString("zh-CN");
}

// formatCompactDecimal keeps one meaningful decimal without a redundant trailing zero.
// formatCompactDecimal 保留一位有效小数并移除多余的尾随零。
function formatCompactDecimal(value: number): string {
  return value.toFixed(1).replace(/\.0$/u, "");
}

// formatBasisPoints formats integer basis points as a one-decimal percentage.
// formatBasisPoints 将整数万分比格式化为一位小数百分比。
function formatBasisPoints(value: number): string {
  return `${(Math.max(0, Math.min(10_000, value)) / 100).toFixed(1)}%`;
}

// ComposerAiCapabilityControl owns one materialized session snapshot with separate switches, models, and accounts.
// ComposerAiCapabilityControl 持有一份将开关、模型与账号分离的已物化会话快照。
export function ComposerAiCapabilityControl(props: {
  // Current persisted conversation, or null for a new conversation draft.
  // 当前持久化会话；新会话草稿时为空。
  conversationId: string | null;
  // Accessible trigger label.
  // 触发器无障碍标签。
  ariaLabel: string;
  // Optional complete draft snapshot captured before the conversation exists.
  // 会话创建前捕获的可选完整草稿快照。
  draftSettings?: ComposerProviderToolSettingsSnapshot;
  // Saves one complete draft snapshot without calling a session RPC.
  // 在不调用会话 RPC 的情况下保存完整草稿快照。
  onDraftSettingsChange?: (settings: ComposerProviderToolSettingsSnapshot) => void;
  // Reports whether unresolved session choices must block sending.
  // 报告未解决的会话选择是否必须阻止发送。
  onConflictChange?: (blocked: boolean) => void;
}) {
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null);
  const [selectedCapability, setSelectedCapability] =
    React.useState<PublicAiProviderToolCapabilityKind>("web_search");
  const [draft, setDraft] = React.useState<ComposerProviderToolSettingsSnapshot>({
    enabledCapabilities: [],
    selections: {},
  });
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
      .then(([nextSnapshot, sessionSettings]) => {
        if (!active) {
          return;
        }
        const nextDraft = sessionSettings
          ? {
              enabledCapabilities: sessionSettings.enabledCapabilities,
              selections: sessionSettings.selections,
            }
          : props.draftSettings ?? {
              enabledCapabilities: nextSnapshot.providerToolEnabledCapabilities,
              selections: nextSnapshot.providerToolSelections,
            };
        const capabilities = orderedProviderToolCapabilities(nextSnapshot);
        setSnapshot(nextSnapshot);
        setDraft(nextDraft);
        setBaselineSignature(providerToolSettingsSignature(nextDraft));
        if (!props.conversationId && !props.draftSettings) {
          props.onDraftSettingsChange?.(nextDraft);
        }
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
  }, [open, props.conversationId, props.draftSettings, props.onDraftSettingsChange]);

  const capabilities = snapshot ? orderedProviderToolCapabilities(snapshot) : [];
  const capabilityTargets = snapshot?.providerToolTargets.filter(
    (target) => target.target.contract.capability === selectedCapability,
  ) ?? [];
  const selectedModels = uniqueProviderToolModels(capabilityTargets);
  const selectedSelection = draft.selections[selectedCapability];
  const selectedModel = selectedModels.find(
    (target) => target.target.target_id === selectedSelection?.target_id,
  ) ?? null;
  const selectedAccounts = selectedModel
    ? capabilityTargets.filter((target) => target.target.target_id === selectedModel.target.target_id)
    : [];
  const selectedAccount = selectedAccounts.find(
    (target) => target.connectionId === selectedSelection?.connection_id,
  ) ?? null;
  const localConflicts = snapshot ? providerToolSettingsConflicts(snapshot, draft) : [];
  const conflicts = localConflicts;
  const enabledCount = draft.enabledCapabilities.length;
  const hasUnsavedChanges =
    providerToolSettingsSignature(draft) !== baselineSignature;
  React.useEffect(() => {
    props.onConflictChange?.(conflicts.length > 0);
  }, [conflicts.length, props.onConflictChange]);
  const save = React.useCallback(async () => {
    if (!props.conversationId) {
      props.onDraftSettingsChange?.(draft);
      setBaselineSignature(providerToolSettingsSignature(draft));
      setOpen(false);
      return;
    }
    setSaving(true);
    try {
      const next = await savePublicAiSessionProviderToolSelections({
        conversationId: props.conversationId,
        enabledCapabilities: draft.enabledCapabilities,
        selections: draft.selections,
      });
      const nextDraft = {
        enabledCapabilities: next.enabledCapabilities,
        selections: next.selections,
      };
      setDraft(nextDraft);
      setBaselineSignature(providerToolSettingsSignature(nextDraft));
      setError(null);
      setOpen(false);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setSaving(false);
    }
  }, [
    draft,
    props.conversationId,
    props.onDraftSettingsChange,
  ]);

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
              <span className="composer-select-icon">
                {conflicts.length ? <AlertTriangle size={15} /> : <SlidersHorizontal size={15} />}
              </span>
              <span className="composer-select-copy">
                <span className="composer-capability-icon-row">
                  {capabilities.map((capability) => (
                    <span
                      className={cn(
                        "composer-capability-colored-icon",
                        draft.enabledCapabilities.includes(capability) ? "enabled" : "disabled",
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
            {conflicts.length ? t("composer.capability.conflict.tooltip") : `${enabledCount} / ${capabilities.length}`}
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
        {conflicts.length ? (
          <div className="composer-capability-conflict-banner">
            <AlertTriangle aria-hidden="true" size={16} />
            <span>{t("composer.capability.conflict.banner")}</span>
          </div>
        ) : null}
        <div className="composer-capability-dialog-grid">
          <aside className="composer-capability-dialog-left">
            {capabilities.map((capability) => {
              const enabled = draft.enabledCapabilities.includes(capability);
              const targets = snapshot?.providerToolTargets.filter(
                (target) => target.target.contract.capability === capability,
              ) ?? [];
              const targetCount = uniqueProviderToolModels(targets).length;
              const conflict = conflicts.find((item) => item.capability === capability);
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
                        const enabledCapabilities = new Set(current.enabledCapabilities);
                        if (!checked) {
                          enabledCapabilities.delete(capability);
                          return { ...current, enabledCapabilities: Array.from(enabledCapabilities).sort() };
                        }
                        enabledCapabilities.add(capability);
                        const first = targets[0];
                        if (first) {
                          return {
                            enabledCapabilities: Array.from(enabledCapabilities).sort(),
                            selections: current.selections[capability] ? current.selections : {
                              ...current.selections,
                              [capability]: selectionForTarget(first),
                            },
                          };
                        }
                        return { ...current, enabledCapabilities: Array.from(enabledCapabilities).sort() };
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
                      {conflict ? (
                        <small className="settings-error-text">
                          {t(conflict.kind === "missing_selection"
                            ? "composer.capability.conflict.missingSelection"
                            : "composer.capability.conflict.targetUnavailable")}
                        </small>
                      ) : null}
                    </span>
                  </span>
                </div>
              );
            })}
          </aside>
          <section className="composer-capability-dialog-right">
            {selectedModels.length ? (
              <div className="composer-capability-route-selectors">
                <label>
                  <span>{t("settings.providerTools.model")}</span>
                  <Select
                    disabled={saving}
                    onValueChange={(targetId) => {
                      const compatible = capabilityTargets.filter(
                        (target) => target.target.target_id === targetId,
                      );
                      const target = compatible.find(
                        (candidate) => candidate.connectionId === selectedAccount?.connectionId,
                      ) ?? compatible[0];
                      if (target) {
                        setDraft((current) => ({
                          ...current,
                          selections: {
                            ...current.selections,
                            [selectedCapability]: selectionForTarget(target),
                          },
                        }));
                      }
                    }}
                    value={selectedModel?.target.target_id ?? ""}
                  >
                    <SelectTrigger><SelectValue placeholder={t("settings.providerTools.selectModel")} /></SelectTrigger>
                    <SelectContent>
                      {selectedModels.map((target) => (
                        <SelectItem key={target.target.target_id} value={target.target.target_id}>
                          {target.target.display_name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </label>
                <label>
                  <span>{t("settings.providerTools.account")}</span>
                  <Select
                    disabled={saving || !selectedModel}
                    onValueChange={(connectionId) => {
                      const target = selectedAccounts.find(
                        (candidate) => candidate.connectionId === connectionId,
                      );
                      if (target) {
                        setDraft((current) => ({
                          ...current,
                          selections: {
                            ...current.selections,
                            [selectedCapability]: selectionForTarget(target),
                          },
                        }));
                      }
                    }}
                    value={selectedAccount?.connectionId ?? ""}
                  >
                    <SelectTrigger><SelectValue placeholder={t("settings.providerTools.selectAccount")} /></SelectTrigger>
                    <SelectContent>
                      {selectedAccounts.map((target) => (
                        <SelectItem key={target.connectionId} value={target.connectionId}>
                          {target.connectionDisplayName}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </label>
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
            {hasUnsavedChanges
              ? t("composer.capability.saveHint.unsaved")
              : t("composer.capability.saveHint.clean")}
          </span>
          <button
            className="skill-action-button"
            disabled={saving || !hasUnsavedChanges || conflicts.length > 0}
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

// providerToolSettingsSignature serializes one complete snapshot deterministically for dirty checks.
// providerToolSettingsSignature 以确定性方式序列化完整快照用于脏检查。
function providerToolSettingsSignature(
  settings: ComposerProviderToolSettingsSnapshot,
): string {
  return JSON.stringify(
    [
      [...settings.enabledCapabilities].sort(),
      Object.entries(settings.selections)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([capability, selection]) => [
        capability,
        selection?.connection_id ?? "",
        selection?.target_id ?? "",
      ]),
    ],
  );
}

// orderProviderToolCapabilities returns unique capabilities with explicit web-search priority.
// orderProviderToolCapabilities 返回去重后且明确将网页搜索置顶的能力列表。
export function orderProviderToolCapabilities(
  capabilities: PublicAiProviderToolCapabilityKind[],
): PublicAiProviderToolCapabilityKind[] {
  const priority = (capability: PublicAiProviderToolCapabilityKind): number => {
    if (capability === "web_search") {
      return 0;
    }
    if (capability === "web_extract") {
      return 1;
    }
    return 2;
  };
  return Array.from(new Set(capabilities)).sort((left, right) => {
    const priorityDifference = priority(left) - priority(right);
    return priorityDifference === 0 ? left.localeCompare(right) : priorityDifference;
  });
}

function orderedProviderToolCapabilities(
  snapshot: PublicAiSettingsSnapshot,
): PublicAiProviderToolCapabilityKind[] {
  return orderProviderToolCapabilities(
    snapshot.providerToolTargets.map((target) => target.target.contract.capability),
  );
}

// providerToolSettingsConflicts reports stale choices without silently replacing them.
// providerToolSettingsConflicts 报告过期选择且不静默替换。
export function providerToolSettingsConflicts(
  snapshot: PublicAiSettingsSnapshot,
  settings: ComposerProviderToolSettingsSnapshot,
): PublicAiSessionProviderToolConflict[] {
  const current = new Map(snapshot.providerToolTargets
    .map((target) => [
      `${target.connectionId.length}:${target.connectionId}:${target.target.target_id.length}:${target.target.target_id}`,
      target.target.contract.capability,
    ]));
  const capabilities = new Set<PublicAiProviderToolCapabilityKind>(settings.enabledCapabilities);
  const conflicts: PublicAiSessionProviderToolConflict[] = [];
  for (const capability of capabilities) {
    const selection = settings.selections[capability];
    if (!selection) {
      conflicts.push({
        capability,
        kind: "missing_selection",
        message: `Capability ${capability} has no selected model and account.`,
      });
      continue;
    }
    const key = `${selection.connection_id.length}:${selection.connection_id}:${selection.target_id.length}:${selection.target_id}`;
    if (current.get(key) !== capability) {
      conflicts.push({
        capability,
        kind: "target_unavailable",
        message: `The saved model or account for ${capability} is no longer available.`,
      });
    }
  }
  return conflicts;
}

// uniqueProviderToolModels collapses account-expanded targets into one row per model.
// uniqueProviderToolModels 将按账号展开的目标折叠为每个模型一行。
function uniqueProviderToolModels(
  targets: PublicAiProviderToolTargetSnapshot[],
): PublicAiProviderToolTargetSnapshot[] {
  const models = new Map<string, PublicAiProviderToolTargetSnapshot>();
  for (const target of targets) {
    if (!models.has(target.target.target_id)) models.set(target.target.target_id, target);
  }
  return Array.from(models.values()).sort((left, right) => (
    left.target.display_name.localeCompare(right.target.display_name)
    || left.target.target_id.localeCompare(right.target.target_id)
  ));
}

// selectionForTarget projects one browser target into the exact persisted selection.
// selectionForTarget 将一个浏览器目标投影为精确持久化选择。
function selectionForTarget(
  target: PublicAiProviderToolTargetSnapshot,
): PublicAiProviderToolTargetSelection {
  return {
    connection_id: target.connectionId,
    target_id: target.target.target_id,
  };
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
  if (capability === "video_generate" || capability === "video_edit" || capability === "video_extend") {
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
  if (capability === "video_generate" || capability === "video_edit" || capability === "video_extend") {
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
    media_analyze: "settings.aiUnderstandImage",
    image_generate: "settings.providerTools.imageGenerate",
    image_edit: "settings.providerTools.imageEdit",
    video_generate: "settings.providerTools.videoGenerate",
    video_edit: "settings.providerTools.videoEdit",
    video_extend: "settings.providerTools.videoExtend",
    speech_synthesize: "settings.providerTools.speechSynthesize",
    speech_transcribe: "settings.providerTools.speechTranscribe",
    music_generate: "settings.providerTools.musicGenerate",
    music_cover: "settings.providerTools.musicCover",
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
      <SelectContent className="composer-select-content">
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
