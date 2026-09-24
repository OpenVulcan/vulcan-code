import React from "react";
import { SessionLifetimeUsagePanel } from "@/components/conversation/session-lifetime-usage";
import { Button } from "@/components/ui/button";
import { Combobox } from "@/components/ui/combobox";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { InlineStreamPlaceholder, StreamPlaceholder } from "@/components/ui/stream-placeholder";
import {
  AlertTriangle,
  LoaderCircle,
  LockKeyhole,
  RotateCw,
  SlidersHorizontal,
  ChevronDown,
} from "lucide-react";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  SegmentedControl,
  SegmentedControlItem,
} from "@/components/ui/choice-controls";
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
import { ProviderToolCapabilityIcon } from "@/components/shared/provider-tool-capability-icon";
import { ProviderToolExtensionModelCard } from "@/components/shared/provider-tool-extension-model-card";
import type {
  ComposerProviderToolSettingsSnapshot,
  ComposerSelectOption,
} from "@/components/composer/composer-model";
import { useI18n } from "@/i18n";
import { formatTokenCount } from "@/lib/usage-format";
import { cn } from "@/lib/utils";
import {
  isDirectProviderToolCapability,
  isModelExtensionCapability,
  mediaUnderstandingCapabilityToggleDisabled,
  modelCapabilitiesSupportMediaUnderstanding,
} from "@/lib/provider-tool-capabilities";
import {
  mediaReplacementReasoningEfforts,
  mediaReplacementReasoningOverrides,
  PUBLIC_AI_REASONING_EFFORT_LABEL_KEYS,
} from "@/lib/reasoning-effort-labels";
import {
  loadPublicAiSessionProviderToolSelections,
  rebuildPublicAiSessionProviderToolSelections,
  loadPublicAiSettings,
  savePublicAiSessionProviderToolSelections,
} from "@/service/settings-actions-service";
import type {
  ContextUsageBreakdownItem,
  ContextCompactionWindowNodeId,
  ContextUsageSnapshot,
  PublicAiContextCompactionWindowNode,
  PublicAiMediaUnderstandingModelCandidate,
  PublicAiMediaUnderstandingBackendMode,
  PublicAiMediaUnderstandingBackendSelection,
  PublicAiProviderToolCapabilityKind,
  PublicAiReasoningEffort,
  PublicAiProviderToolTargetSnapshot,
  PublicAiProviderToolTargetSelection,
  PublicAiSessionProviderToolConflict,
  PublicAiSettingsSnapshot,
  SessionProviderUsageSummary,
} from "@/types";

// ContextUsageOrb renders a compact context budget indicator beside the composer.
// ContextUsageOrb 在输入区旁渲染紧凑的上下文预算指示器。
// Parameters: props supplies measured usage, context nodes and the node mutation callback; returns a compact indicator and keyboard-accessible policy popover.
// 参数：props 提供实测用量、上下文节点及节点变更回调；返回紧凑指示器与可键盘访问的策略浮层。
export function ContextUsageOrb(props: {
  // Context usage snapshot projected by the service.
  // 服务投影出的上下文占用快照。
  snapshot: ContextUsageSnapshot | null;
  // Session-level Provider accounting shown inside the context-usage card.
  // 会话级 Provider 统计在上下文占用卡片内展示。
  sessionProviderUsage?: SessionProviderUsageSummary | null;
  // Fixed backend-owned compression-window nodes.
  // 后端持有的固定压缩窗口节点。
  contextCompactionWindowNodes: PublicAiContextCompactionWindowNode[];
  // Stable node id selected by the current session.
  // 当前会话选择的稳定节点标识。
  selectedNodeId: ContextCompactionWindowNodeId | null;
  // Unchanged hard context limit declared by the active model.
  // 当前模型声明且保持不变的硬上下文窗口。
  modelContextLimit: number | null;
  // Persists or clears one session-only compression-window lock.
  // 持久化或清除一个仅限当前会话的压缩窗口锁定。
  onNodeChange: (nodeId: ContextCompactionWindowNodeId | null) => Promise<boolean>;
}) {
  // t resolves stable copy for the active WebUI language.
  // t 为当前 WebUI 语言解析稳定文案。
  const { locale, t } = useI18n();
  // nodeMutation prevents concurrent node writes while retaining the last authoritative selection.
  // nodeMutation 防止并发节点写入，同时保留最后的权威选择。
  const nodeMutation = React.useRef(false);
  const [nodeSaving, setNodeSaving] = React.useState(false);
  const [nodeError, setNodeError] = React.useState<string | null>(null);
  // contentRef receives initial focus so opening the metrics does not immediately show a missing-value tooltip.
  // contentRef 接收初始焦点，避免打开指标时立即弹出缺失值提示并遮挡数值。
  const contentRef = React.useRef<HTMLDivElement>(null);
  // Both click and delayed hover control the same interactive statistics popover.
  // 点击和延迟悬停控制同一个可交互统计浮层。
  const [statisticsOpen, setStatisticsOpen] = React.useState(false);
  // Pending hover opens are cancelled when the pointer leaves or the component unmounts.
  // 指针离开或组件卸载时取消待执行的悬停打开。
  const hoverTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  // Hover must not steal keyboard focus from the conversation composer.
  // 悬停不得抢走会话输入区的键盘焦点。
  const openedByHover = React.useRef(false);
  // Cancels a scheduled hover open; no input or return value is needed.
  // 取消已安排的悬停打开；无需输入或返回值。
  const cancelHoverOpen = React.useCallback(() => {
    if (hoverTimer.current !== null) clearTimeout(hoverTimer.current);
    hoverTimer.current = null;
  }, []);
  React.useEffect(() => cancelHoverOpen, [cancelHoverOpen]);
  if (!props.snapshot) {
    return null;
  }
  const snapshot = props.snapshot;
  const breakdown = contextUsageBreakdown(snapshot);
  const usageBasisPoints = contextUsageBasisPoints(snapshot);
  const percent = usageBasisPoints / 100;
  const title = snapshot.tooltipTitle || t("contextUsage.title");
  const detailVisible = breakdown.length > 0;
  // providerUsage keeps session-level accounting out of repeated process headers.
  // providerUsage 将会话级统计从重复的过程标题中移出。
  const providerUsage = props.sessionProviderUsage;
  // selectedNode resolves the persisted id against the current backend projection.
  // selectedNode 根据当前后端投影解析持久化标识。
  const selectedNode = props.contextCompactionWindowNodes.find((node) => node.id === props.selectedNodeId) ?? null;
  // availableNodes applies the strict node-window-smaller-than-model contract.
  // availableNodes 应用节点窗口必须严格小于模型窗口的契约。
  const modelContextLimit = props.modelContextLimit;
  const availableNodes = availableContextCompactionWindowNodes(
    props.contextCompactionWindowNodes,
    modelContextLimit,
  );
  // defaultContextWindowLabel exposes the active model's original hard window on the restore-default item.
  // defaultContextWindowLabel 在恢复默认选项上直接展示当前模型的原始硬窗口。
  const defaultContextWindowLabel = modelContextLimit == null
    ? t("composer.contextWindow.default")
    : formatContextWindowNodeTokens(modelContextLimit);
  // selectedNodeIsInvalid keeps a changed global node visible until the user restores Default.
  // selectedNodeIsInvalid 在全局节点变化失效后仍保持可见，直到用户恢复默认。
  const selectedNodeIsInvalid = selectedNode != null
    && !availableNodes.some((node) => node.id === selectedNode.id);
  // contextUsageAriaLabel exposes the same session-level summary through the capacity control.
  // contextUsageAriaLabel 通过上下文容量控件公开同一份会话级统计摘要。
  const contextUsageAriaLabel = [
    title,
    selectedNode ? formatContextWindowNodeTokens(selectedNode.tokens) : null,
  ].filter((value): value is string => Boolean(value)).join(" · ");
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
    <Popover open={statisticsOpen} onOpenChange={(open) => { cancelHoverOpen(); setStatisticsOpen(open); }}>
      <PopoverTrigger asChild>
        <button
          aria-label={contextUsageAriaLabel}
          className={cn("context-usage-control", selectedNode && "is-locked")}
          type="button"
          onPointerEnter={(event) => {
            if (event.pointerType !== "mouse" || statisticsOpen) return;
            cancelHoverOpen();
            hoverTimer.current = setTimeout(() => {
              hoverTimer.current = null;
              openedByHover.current = true;
              setStatisticsOpen(true);
            }, 500);
          }}
          onPointerLeave={cancelHoverOpen}
          onPointerDown={() => { cancelHoverOpen(); openedByHover.current = false; }}
        >
          <span
            aria-hidden="true"
            className={cn("context-usage-orb", `severity-${snapshot.severity}`)}
            data-threshold-state={thresholdState}
            style={style}
          />
          {selectedNode ? (
            <span className="context-usage-lock-label">
              <LockKeyhole aria-hidden="true" size={11} strokeWidth={2} />
              {formatContextWindowNodeTokens(selectedNode.tokens)}
            </span>
          ) : null}
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        aria-label={title}
        className="context-usage-tooltip context-usage-popover"
        collisionPadding={12}
        ref={contentRef}
        onOpenAutoFocus={(event) => { event.preventDefault(); if (!openedByHover.current) contentRef.current?.focus(); }}
        onCloseAutoFocus={(event) => { if (openedByHover.current) event.preventDefault(); openedByHover.current = false; }}
        side="top"
        sideOffset={8}
      >
        <div className="context-usage-card-header">
          <strong>{title}</strong>
          <span>{formatContextUsageTotal(snapshot)}</span>
        </div>
        <ContextUsageProgressBar breakdown={breakdown} usageBasisPoints={usageBasisPoints} />
        {detailVisible || snapshot.cacheHitBasisPoints != null ? (
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
            {snapshot.cacheHitBasisPoints != null ? <div className="context-usage-cache-row">
              <span className="context-usage-breakdown-label">
                <RotateCw className="context-usage-dot" aria-hidden="true" />
                {t("composer.contextUsage.cacheHitRate")}
              </span>
              <strong>{formatBasisPoints(snapshot.cacheHitBasisPoints)}</strong>
            </div> : null}
          </div>
        ) : null}
        <SessionLifetimeUsagePanel usage={providerUsage?.lifetime} error={providerUsage?.lifetimeError} />
        <div className="context-usage-window-section">
          <div className="context-usage-window-title">
            {t("composer.contextWindow.title")}
            <span>{selectedNode ? (locale === "zh-CN" ? "本会话已锁定" : "Locked for this session") : (locale === "zh-CN" ? "跟随模型" : "Follows the model")}</span>
          </div>
          <SegmentedControl
            aria-label={t("composer.contextWindow.title")}
            className="context-usage-window-switch"
            disabled={nodeSaving}
            onValueChange={(value) => {
              const nextNodeId = contextWindowNodeIdFromSwitchValue(value, props.contextCompactionWindowNodes);
              if (nextNodeId !== undefined && !nodeMutation.current) {
                nodeMutation.current = true;
                setNodeSaving(true);
                setNodeError(null);
                void props.onNodeChange(nextNodeId).then((saved) => {
                  if (!saved) setNodeError("");
                }).catch((reason: unknown) => {
                  setNodeError(errorMessage(reason));
                }).finally(() => {
                  nodeMutation.current = false;
                  setNodeSaving(false);
                });
              }
            }}
            value={props.selectedNodeId ?? "default"}
          >
            {availableNodes.map((node) => (
              <SegmentedControlItem key={node.id} value={node.id}>
                {formatContextWindowNodeTokens(node.tokens)}
              </SegmentedControlItem>
            ))}
            {selectedNodeIsInvalid ? (
              <SegmentedControlItem disabled value={selectedNode.id}
                aria-label={`${formatContextWindowNodeTokens(selectedNode.tokens)} · ${t("composer.contextWindow.invalid")}`}>
                <AlertTriangle aria-hidden="true" size={13} />
                {formatContextWindowNodeTokens(selectedNode.tokens)}
              </SegmentedControlItem>
            ) : null}
            <SegmentedControlItem
              aria-label={`${t("composer.contextWindow.default")} · ${defaultContextWindowLabel}`}
              value="default"
            >
              {defaultContextWindowLabel}
            </SegmentedControlItem>
          </SegmentedControl>
          {selectedNodeIsInvalid ? <p className="text-xs text-muted-foreground">{formatContextWindowNodeTokens(selectedNode.tokens)} · {t("composer.contextWindow.invalid")}</p> : null}
          {nodeError !== null ? <div className="text-xs">
            <p role="alert" className="text-destructive">{t("composer.contextWindow.saveFailed")}</p>
            {nodeError ? <details className="mt-1">
              <summary className="cursor-pointer text-muted-foreground">{t("composer.capability.errorDetails")}</summary>
              <pre tabIndex={0} className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap break-all text-xs">{nodeError}</pre>
            </details> : null}
          </div> : null}
        </div>
      </PopoverContent>
    </Popover>
  );
}

// formatCompactProviderCount formats request and token counts with up to two decimals and K/M/B/T units.
// formatCompactProviderCount 使用最多两位小数以及 K/M/B/T 单位格式化请求次数与 Token 数量。
// Parameters: value is the non-negative count to format, and unknownLabel is shown when the count is unavailable.
// 参数：value 是待格式化的非负数量，unknownLabel 是数量不可用时显示的文本。
// Returns: a compact count string without a redundant trailing decimal zero.
// 返回值：去除多余尾随零的小数紧凑数量文本。
export function formatCompactProviderCount(value: number | null | undefined, unknownLabel: string): string {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return unknownLabel;
  }
  // normalizedValue prevents negative accounting values from leaking into the display.
  // normalizedValue 防止负数统计值泄漏到界面展示中。
  const normalizedValue = Math.max(0, value);
  // unitIndex selects the largest supported compact unit without probing unknown formats.
  // unitIndex 选择最大的已支持紧凑单位，不探测未知格式。
  let unitIndex = 0;
  // scaledValue is the count after applying the selected unit divisor.
  // scaledValue 是应用所选单位除数后的数量。
  let scaledValue = normalizedValue;
  // units lists the only compact units accepted by the WebUI display contract.
  // units 列出 WebUI 展示契约唯一接受的紧凑单位。
  const units = ["", "K", "M", "B", "T"];
  while (scaledValue >= 1_000 && unitIndex < units.length - 1) {
    scaledValue /= 1_000;
    unitIndex += 1;
  }
  // formattedValue keeps small values exact and larger values within two decimals.
  // formattedValue 保持小数值精确，并将较大数值限制在两位小数内。
  const formattedValue = unitIndex === 0
    ? String(scaledValue)
    : scaledValue.toFixed(2).replace(/\.?(0+)$/u, "");
  return `${formattedValue}${units[unitIndex]}`;
}

// contextWindowNodeIdFromSwitchValue validates one switch value against the authoritative node list.
// contextWindowNodeIdFromSwitchValue 根据权威节点列表校验一个切换值。
function contextWindowNodeIdFromSwitchValue(
  value: string,
  nodes: PublicAiContextCompactionWindowNode[],
): ContextCompactionWindowNodeId | null | undefined {
  if (value === "default") {
    return null;
  }
  return nodes.find((node) => node.id === value)?.id;
}

// formatContextWindowNodeTokens renders node and model windows as integer K/M labels.
// formatContextWindowNodeTokens 把节点与模型窗口渲染为不带小数的 K/M 标签。
export function formatContextWindowNodeTokens(tokens: number): string {
  if (tokens >= 1_000_000 && tokens % 1_000_000 === 0) {
    return `${tokens / 1_000_000}M`;
  }
  return `${Math.round(tokens / 1_000)}K`;
}

// availableContextCompactionWindowNodes returns fixed nodes strictly below one model hard window.
// availableContextCompactionWindowNodes 返回严格小于模型硬窗口的固定节点。
export function availableContextCompactionWindowNodes(
  nodes: PublicAiContextCompactionWindowNode[],
  modelContextLimit: number | null,
): PublicAiContextCompactionWindowNode[] {
  if (modelContextLimit == null) {
    return [];
  }
  return nodes.filter((node) => node.tokens < modelContextLimit);
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
  const { t } = useI18n();
  const percent = props.usageBasisPoints / 100;
  return (
    <div
      aria-label={t("composer.contextUsage.usedPercent").replace("{percent}", percent.toFixed(1))}
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
  const used = formatTokenCount(snapshot.usedTokens);
  if (typeof snapshot.maxTokens !== "number" || !Number.isFinite(snapshot.maxTokens)) {
    return `${used} Token`;
  }
  return `${used}/${formatTokenCount(snapshot.maxTokens)} (${formatBasisPoints(usageBasisPoints)})`;
}

// formatBasisPoints formats integer basis points as a one-decimal percentage.
// formatBasisPoints 将整数万分比格式化为一位小数百分比。
function formatBasisPoints(value: number): string {
  return `${(Math.max(0, Math.min(10_000, value)) / 100).toFixed(1)}%`;
}

// ComposerAiCapabilityControl owns one materialized session snapshot with separate switches, models, and accounts.
// ComposerAiCapabilityControl 持有一份将开关、模型与账号分离的已物化会话快照。
// Parameters: props identifies the conversation and capability configuration; returns the session editor with exact targets, local mode state and guarded saves.
// 参数：props 标识会话与能力配置；返回包含精确目标、本地模式状态和受保护保存的会话编辑器。
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
    extensionTargetSets: {},
    mediaUnderstandingSelections: {},
  });
  const [baselineSignature, setBaselineSignature] = React.useState("{}");
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  // loadState binds availability to its owning conversation; stale settings must never be saved to another session.
  // loadState 将可用性绑定到所属会话；旧设置绝不能保存到另一个会话。
  const [loadState, setLoadState] = React.useState<{
    conversationId: string | null;
    status: "loading" | "ready" | "failed";
  }>({ conversationId: props.conversationId, status: "loading" });
  // reloadVersion retries the same authoritative reads without replacing session settings with empty defaults.
  // reloadVersion 重试相同的权威读取，不用空默认值替换会话设置。
  const [reloadVersion, setReloadVersion] = React.useState(0);
  // saveGeneration rejects late save responses after the settings owner or load generation changes.
  // saveGeneration 拒绝设置归属或加载代次变化后迟到的保存响应。
  const saveGeneration = React.useRef(0);
  // mutationPending closes the same-frame duplicate window before React renders saving state.
  // mutationPending 在 React 渲染保存状态前关闭同帧重复操作窗口。
  const mutationPending = React.useRef(false);
  // Recovery actions remove their own button; retain keyboard focus on the next recovery or current category.
  // 恢复操作会移除自身按钮；将键盘焦点保留在下一个恢复操作或当前分类。
  const recoveryFocusPending = React.useRef(false);
  const extensionRecoveryList = React.useRef<HTMLDivElement>(null);
  const selectedCategoryButton = React.useRef<HTMLButtonElement>(null);
  // Retry focus remains within the recovered controls after the loading branch removes the clicked button.
  // 加载分支移除重试按钮后，将焦点保留在恢复后的控件中。
  const retryFocusPending = React.useRef(false);
  const retryButton = React.useRef<HTMLButtonElement>(null);
  React.useLayoutEffect(() => {
    if (!recoveryFocusPending.current) return;
    recoveryFocusPending.current = false;
    const nextRecovery = extensionRecoveryList.current?.querySelector<HTMLButtonElement>('[data-extension-recovery]');
    (nextRecovery ?? selectedCategoryButton.current)?.focus();
  }, [draft]);
  // mediaEditModes owns empty UI modes without fabricating a complete persisted route.
  // mediaEditModes 持有空的界面模式，不伪造完整持久化路由。
  const [mediaEditModes, setMediaEditModes] = React.useState<Partial<Record<PublicAiMediaUnderstandingCapabilityKind, PublicAiMediaUnderstandingBackendMode>>>({});
  // loadStatus invalidates the previous owner synchronously, before the next effect can start.
  // loadStatus 在下一次 effect 启动前同步使上一个会话的状态失效。
  const loadStatus = loadState.conversationId === props.conversationId ? loadState.status : "loading";
  // settingsReady is required both by the UI and the save callback to prevent destructive empty writes after read errors.
  // settingsReady 同时约束界面与保存回调，防止读取失败后用空配置覆盖原设置。
  const settingsReady = loadStatus === "ready";
  React.useLayoutEffect(() => {
    if (!retryFocusPending.current || !open || loadStatus === "loading") return;
    retryFocusPending.current = false;
    (loadStatus === "failed" ? retryButton.current : selectedCategoryButton.current)?.focus();
  }, [open, loadStatus]);

  React.useEffect(() => {
    let active = true;
    saveGeneration.current += 1;
    mutationPending.current = false;
    setMediaEditModes({});
    setSaving(false);
    setLoadState({ conversationId: props.conversationId, status: "loading" });
    setError(null);
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
              extensionTargetSets: sessionSettings.extensionTargetSets,
              mediaUnderstandingSelections: sessionSettings.mediaUnderstandingSelections,
            }
          : props.draftSettings
            ? normalizedComposerProviderToolSettings(props.draftSettings)
            : {
              enabledCapabilities: nextSnapshot.providerToolEnabledCapabilities,
              selections: nextSnapshot.providerToolSelections,
              extensionTargetSets: nextSnapshot.providerExtensionTargetSets,
              mediaUnderstandingSelections: nextSnapshot.mediaUnderstandingSelections ?? {},
            };
        const capabilities = composerProviderToolCapabilities(nextSnapshot, nextDraft);
        // Prefer a configured family initially; fixed empty media categories remain selectable when none is configured.
        // 初次优先展示已有配置的能力族；无配置时仍可选中固定空媒体分类。
        const initialCapability = orderedProviderToolCapabilities(nextSnapshot)[0] ?? capabilities[0];
        setSnapshot(nextSnapshot);
        setDraft(nextDraft);
        setBaselineSignature(providerToolSettingsSignature(nextDraft));
        if (!props.conversationId && !props.draftSettings) {
          props.onDraftSettingsChange?.(nextDraft);
        }
        setSelectedCapability((current) => (
          capabilities.includes(current) ? current : initialCapability
        ));
        setError(null);
        setLoadState({ conversationId: props.conversationId, status: "ready" });
      })
      .catch((reason) => {
        if (active) {
          setError(errorMessage(reason));
          setLoadState({ conversationId: props.conversationId, status: "failed" });
        }
      });
    return () => {
      active = false;
      saveGeneration.current += 1;
    };
  }, [open, props.conversationId, props.draftSettings, props.onDraftSettingsChange, reloadVersion]);

  const capabilities = settingsReady && snapshot
    ? composerProviderToolCapabilities(snapshot, draft)
    : [];
  const capabilityTargets = snapshot?.providerToolTargets.filter(
    (target) => target.target.contract.capability === selectedCapability
      && (!isModelExtensionCapability(selectedCapability)
        || target.manifestSourceLayer !== "custom"),
  ) ?? [];
  const selectedModels = uniqueProviderToolModels(capabilityTargets);
  // selectedExtensionTargets is the complete native model set for the selected capability.
  // selectedExtensionTargets 是当前选中能力的完整原生模型集合。
  const selectedExtensionTargets = draft.extensionTargetSets[selectedCapability] ?? [];
  // unavailableExtensionTargets preserves exact saved identities that cannot appear in a current model card.
  // unavailableExtensionTargets 保留无法出现在当前模型卡片中的准确已存身份。
  const unavailableExtensionTargets = selectedExtensionTargets.filter((selection) => !capabilityTargets.some((target) => (
    target.connectionId === selection.connection_id && target.target.target_id === selection.target_id
  )));
  // updateMediaSelection changes one explicit route while requiring manual activation after an unavailable route.
  // updateMediaSelection 修改一条显式路由；原路由不可用时，要求用户重新手动开启。
  // Parameters: capability identifies the media facade; selection is the complete chosen route. Returns nothing.
  // 参数：capability 标识媒体能力；selection 是完整选定路由。无返回值。
  const updateMediaSelection = (
    capability: PublicAiProviderToolCapabilityKind,
    selection: PublicAiMediaUnderstandingBackendSelection,
  ): void => {
    if (!snapshot || !isMediaUnderstandingCapability(capability) || mutationPending.current) return;
    setDraft((current) => {
      // wasAvailable uses the same route and mode checks as the displayed switch, never its stale stored flag alone.
      // wasAvailable 使用与显示开关一致的路由和模式校验，绝不单独采用过期的已存标记。
      const wasAvailable = (mediaEditModes[capability] ?? current.mediaUnderstandingSelections[capability]?.mode)
        === current.mediaUnderstandingSelections[capability]?.mode
        && !providerToolSettingsConflicts(snapshot, { ...current, enabledCapabilities: [capability] })
          .some((conflict) => conflict.capability === capability);
      return {
        ...current,
        enabledCapabilities: wasAvailable ? current.enabledCapabilities
          : current.enabledCapabilities.filter((enabled) => enabled !== capability),
        mediaUnderstandingSelections: { ...current.mediaUnderstandingSelections, [capability]: selection },
      };
    });
  };
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
  // selectedMediaTargets contains every exact provider-tool route for the selected media kind.
  // selectedMediaTargets 包含当前媒体类型的全部精确供应商工具路由。
  const selectedMediaTargets = isMediaUnderstandingCapability(selectedCapability)
    ? capabilityTargets
    : [];
  const selectedMediaProviderModels = uniqueProviderToolModels(selectedMediaTargets);
  // selectedMediaCandidates is independent from system defaults and global enable switches.
  // selectedMediaCandidates 与系统默认值及全局启用开关完全独立。
  const selectedMediaCandidates = snapshot && isMediaUnderstandingCapability(selectedCapability)
    ? (snapshot.mediaUnderstandingModelCandidates ?? []).filter((candidate) => (
      modelCapabilitiesSupportMediaUnderstanding(candidate.capabilities, selectedCapability)
    ))
    : [];
  const selectedMediaModelChoices = uniqueMediaUnderstandingModels(selectedMediaCandidates);
  // selectedMediaSelection is the complete route owned by this session.
  // selectedMediaSelection 是当前会话持有的完整路由。
  const selectedMediaSelection = isMediaUnderstandingCapability(selectedCapability)
    ? draft.mediaUnderstandingSelections[selectedCapability]
    : undefined;
  // selectedMediaMode remains readable even when the authoritative selection is absent.
  // selectedMediaMode 在权威选择缺失时仍保留可读的编辑模式。
  const selectedMediaMode = isMediaUnderstandingCapability(selectedCapability)
    ? mediaEditModes[selectedCapability] ?? selectedMediaSelection?.mode ?? "provider_tool"
    : "provider_tool";
  const selectedMediaProviderTarget = selectedMediaSelection?.mode === "provider_tool"
    ? selectedMediaTargets.find((target) => (
      target.connectionId === selectedMediaSelection.selection.connection_id
      && target.target.target_id === selectedMediaSelection.selection.target_id
    ))
    : undefined;
  const selectedMediaProviderModelKey = selectedMediaProviderTarget
    ? providerToolModelKey(selectedMediaProviderTarget)
    : "";
  const selectedMediaProviderAccounts = selectedMediaTargets.filter(
    (target) => providerToolModelKey(target) === selectedMediaProviderModelKey,
  );
  const selectedMediaModelCandidate = selectedMediaSelection?.mode === "model_replacement"
    ? selectedMediaCandidates.find((candidate) => (
      candidate.selection.provider_id === selectedMediaSelection.selection.provider_id
      && candidate.selection.model_id === selectedMediaSelection.selection.model_id
      && candidate.selection.connection_id === selectedMediaSelection.selection.connection_id
    ))
    : undefined;
  const selectedMediaModelKey = selectedMediaModelCandidate
    ? mediaUnderstandingModelKey(selectedMediaModelCandidate)
    : "";
  const selectedMediaModelAccounts = selectedMediaCandidates.filter(
    (candidate) => mediaUnderstandingModelKey(candidate) === selectedMediaModelKey,
  );
  const selectedMediaReasoningEfforts = selectedMediaModelCandidate
    ? mediaReplacementReasoningOverrides(selectedMediaModelCandidate.capabilities)
    : [];
  const localConflicts = snapshot ? providerToolSettingsConflicts(snapshot, draft) : [];
  const conflicts = localConflicts;
  // enabledCount combines direct enabled capabilities and non-empty extension sets.
  // enabledCount 合并直连已启用能力与非空扩展集合。
  const enabledCount = draft.enabledCapabilities.length + Object.values(draft.extensionTargetSets)
    .filter((selections) => selections && selections.length > 0).length;
  const hasUnsavedChanges = settingsReady
    && providerToolSettingsSignature(draft) !== baselineSignature;
  React.useEffect(() => {
    props.onConflictChange?.(!settingsReady || conflicts.length > 0);
  }, [settingsReady, conflicts.length, props.onConflictChange]);
  const save = React.useCallback(async () => {
    if (!settingsReady || mutationPending.current || conflicts.length > 0) {
      return;
    }
    if (!props.conversationId) {
      props.onDraftSettingsChange?.(draft);
      setBaselineSignature(providerToolSettingsSignature(draft));
      setOpen(false);
      return;
    }
    mutationPending.current = true;
    setSaving(true);
    // generation pins this response to the exact settings view that initiated the save.
    // generation 将响应固定到发起保存的精确设置视图。
    const generation = ++saveGeneration.current;
    try {
      const next = await savePublicAiSessionProviderToolSelections({
        conversationId: props.conversationId,
        enabledCapabilities: draft.enabledCapabilities,
        selections: draft.selections,
        extensionTargetSets: draft.extensionTargetSets,
        mediaUnderstandingSelections: draft.mediaUnderstandingSelections,
      });
      if (generation !== saveGeneration.current) {
        return;
      }
      const nextDraft = {
        enabledCapabilities: next.enabledCapabilities,
        selections: next.selections,
        extensionTargetSets: next.extensionTargetSets,
        mediaUnderstandingSelections: next.mediaUnderstandingSelections,
      };
      setDraft(nextDraft);
      setBaselineSignature(providerToolSettingsSignature(nextDraft));
      setError(null);
      setOpen(false);
    } catch (reason) {
      if (generation === saveGeneration.current) {
        setError(errorMessage(reason));
      }
    } finally {
      if (generation === saveGeneration.current) {
        mutationPending.current = false;
        setSaving(false);
      }
    }
  }, [
    draft,
    settingsReady,
    conflicts.length,
    saving,
    props.conversationId,
    props.onDraftSettingsChange,
  ]);

  // Rebuild only the selected conversation's addon after an explicit click; ignore stale responses.
  // 仅在显式点击后重建当前会话附加项；忽略过期响应。
  // Returns after the write, then reloads authoritative settings without closing the recovery view.
  // 写入完成后返回，并重新加载权威设置，不关闭恢复视图。
  const rebuild = React.useCallback(async () => {
    if (!props.conversationId || mutationPending.current) {
      return;
    }
    const generation = ++saveGeneration.current;
    mutationPending.current = true;
    setSaving(true);
    setError(null);
    try {
      await rebuildPublicAiSessionProviderToolSelections(props.conversationId);
      if (generation === saveGeneration.current) {
        setReloadVersion((current) => current + 1);
      }
    } catch (reason) {
      if (generation === saveGeneration.current) {
        setError(errorMessage(reason));
      }
    } finally {
      if (generation === saveGeneration.current) {
        mutationPending.current = false;
        setSaving(false);
      }
    }
  }, [props.conversationId, saving]);

  return (
    <Dialog open={open} onOpenChange={(nextOpen) => {
      if (!mutationPending.current) setOpen(nextOpen);
    }}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DialogTrigger asChild>
            <button
              aria-label={props.ariaLabel}
              className="composer-capability-trigger composer-select-trigger capability"
              type="button"
            >
              <span className="composer-select-icon" data-tone={loadStatus !== "loading" && (loadStatus === "failed" || conflicts.length) ? "warning" : undefined}>
                {loadStatus === "loading" ? <LoaderCircle size={15} className="animate-spin" />
                  : loadStatus === "failed" || conflicts.length ? <AlertTriangle size={15} />
                    : <SlidersHorizontal size={15} />}
              </span>
              <span className="composer-select-copy">
                <span className="composer-capability-icon-row">
                  {capabilities.map((capability) => (
                    <span
                      className={cn(
                        "composer-capability-colored-icon",
                        composerCapabilityIsEnabled(draft, capability) ? "enabled" : "disabled",
                      )}
                      key={capability}
                      style={{
                        "--capability-color": providerToolCapabilityColor(capability),
                      } as React.CSSProperties}
                    >
                      <ProviderToolCapabilityIcon capability={capability} size={11} />
                    </span>
                  ))}
                </span>
              </span>
              <ChevronDown aria-hidden="true" className="composer-select-chevron" />
            </button>
          </DialogTrigger>
        </TooltipTrigger>
        <TooltipContent className="composer-control-tooltip" side="top">
          <strong>{t("composer.capability.tooltipTitle")}</strong>
          <span>
            {!settingsReady ? t(loadStatus === "failed" ? "composer.capability.loadFailed" : "composer.capability.loading")
              : conflicts.length ? t("composer.capability.conflict.tooltip") : `${enabledCount} / ${capabilities.length}`}
          </span>
        </TooltipContent>
      </Tooltip>
      <DialogContent className="composer-capability-dialog" scrollMode="fixed-footer">
        <DialogHeader>
          <DialogTitle>{t("composer.capability.dialogTitle")}</DialogTitle>
        </DialogHeader>
        <div className="composer-capability-dialog-body flex min-h-0 flex-col overflow-hidden">
          <DialogDescription className="mt-3 shrink-0 text-xs text-muted-foreground">
            {t("composer.capability.dialogDescription")}
          </DialogDescription>
        {loadState.conversationId === props.conversationId && error
          ? <div className="my-2 min-h-0 shrink-0 rounded-md border border-destructive/30 p-3">
              <p className="text-sm text-destructive" role="alert">
                {t(loadStatus === "failed" ? "composer.capability.loadFailed" : "composer.capability.operationFailed")}
              </p>
              <details className="mt-1 text-xs text-muted-foreground">
                <summary className="cursor-pointer">{t("composer.capability.errorDetails")}</summary>
                <pre className="mt-2 max-h-24 overflow-auto whitespace-pre-wrap break-all">{error}</pre>
              </details>
            </div> : null}
        {loadStatus === "loading" ? (
          <div className="composer-capability-dialog-grid composer-capability-dialog-placeholder flex-1">
            <StreamPlaceholder className="composer-capability-dialog-left" label={t("composer.capability.loading")} shape="rows" count={6} />
            <div className="composer-capability-dialog-right grid content-start gap-4">
              <StreamPlaceholder decorative shape="fields" count={1} />
              <StreamPlaceholder decorative shape="rows" count={4} />
            </div>
          </div>
        ) : null}
        {loadStatus === "failed" ? (
          <div>
            {props.conversationId ? <p className="settings-muted-line">{t("composer.capability.rebuildHint")}</p> : null}
            <div className="flex gap-2">
              <button className="skill-action-button" type="button" ref={retryButton} disabled={saving} onClick={() => {
                retryFocusPending.current = true;
                setReloadVersion((current) => current + 1);
              }}>
                {t("composer.capability.retry")}
              </button>
              {props.conversationId ? <button className="skill-action-button" type="button" disabled={saving} onClick={() => void rebuild()}>
                {t(saving ? "composer.capability.rebuilding" : "composer.capability.rebuild")}
              </button> : null}
            </div>
          </div>
        ) : null}
        {settingsReady ? <div className="composer-capability-dialog-grid vcn-stream-reveal flex-1">
          <aside className="composer-capability-dialog-left">
            {capabilities.map((capability) => {
              // directMode controls whether the capability uses one selected route and a switch.
              // directMode 控制当前能力是否使用单路由与开关。
              const directMode = isDirectProviderToolCapability(capability);
              // extensionSelectionCount reports selected native models for one extension family.
              // extensionSelectionCount 报告一个扩展能力族已选原生模型数量。
              const extensionSelectionCount = draft.extensionTargetSets[capability]?.length ?? 0;
              // enabled represents direct activation or a non-empty native extension set.
              // enabled 表示直连启用或非空原生扩展集合。
              const storedEnabled = directMode
                ? draft.enabledCapabilities.includes(capability)
                : extensionSelectionCount > 0;
              // mediaAvailable reuses exact route conflicts, including reasoning support, independently from the stored switch.
              // mediaAvailable 独立于已存开关复用精确路由冲突校验，包括思考档位支持。
              const mediaAvailable = isMediaUnderstandingCapability(capability) && snapshot
                ? (mediaEditModes[capability] ?? draft.mediaUnderstandingSelections[capability]?.mode ?? "provider_tool") === draft.mediaUnderstandingSelections[capability]?.mode
                  && !providerToolSettingsConflicts(snapshot, { ...draft, enabledCapabilities: [capability] })
                    .some((item) => item.capability === capability)
                : true;
              const enabled = storedEnabled && mediaAvailable;
              const targets = snapshot?.providerToolTargets.filter(
                (target) => target.target.contract.capability === capability
                  && (!isModelExtensionCapability(capability)
                    || target.manifestSourceLayer !== "custom"),
              ) ?? [];
              const mediaCandidates = snapshot && isMediaUnderstandingCapability(capability)
                ? (snapshot.mediaUnderstandingModelCandidates ?? []).filter((candidate) => (
                  modelCapabilitiesSupportMediaUnderstanding(candidate.capabilities, capability)
                ))
                : [];
              // targetCount reports every independently selectable session model across both backend families.
              // targetCount 报告两种后端类型中全部可由会话独立选择的模型。
              const targetCount = isMediaUnderstandingCapability(capability)
                ? uniqueProviderToolModels(targets).length
                  + uniqueMediaUnderstandingModels(mediaCandidates).length
                : uniqueProviderToolModels(targets).length;
              const conflict = conflicts.find((item) => item.capability === capability);
              return (
                <div
                  className={cn(
                    "composer-capability-kind-row",
                    selectedCapability === capability && "selected",
                    conflict && "conflict",
                  )}
                  aria-invalid={conflict ? true : undefined}
                  key={capability}
                >
                  {directMode ? (
                    <input
                      aria-label={`${providerToolCapabilityLabel(capability, t)} · ${t("settings.providerTools.enabled")}`}
                      checked={enabled}
                      disabled={isMediaUnderstandingCapability(capability)
                        ? mediaUnderstandingCapabilityToggleDisabled(saving, mediaAvailable, enabled)
                        : providerToolCapabilitySwitchDisabled(saving, targetCount, enabled)}
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
                          if (isMediaUnderstandingCapability(capability)) {
                            if (!mediaAvailable) return current;
                            return {
                              ...current,
                              enabledCapabilities: Array.from(enabledCapabilities).sort(),
                            };
                          }
                          const first = targets[0];
                          if (first) {
                            return {
                              ...current,
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
                  ) : (
                    <span className="composer-capability-extension-count">
                      {extensionSelectionCount}
                    </span>
                  )}
                  <Button className="composer-capability-kind-select h-auto min-w-0 flex-1 justify-start whitespace-normal text-left" type="button" variant="ghost"
                    ref={selectedCapability === capability ? selectedCategoryButton : undefined}
                    aria-pressed={selectedCapability === capability} disabled={saving}
                    onClick={() => setSelectedCapability(capability)}>
                    <span className={cn(
                      "composer-capability-colored-icon",
                      enabled ? "enabled" : "disabled",
                    )} style={{
                      "--capability-color": providerToolCapabilityColor(capability),
                    } as React.CSSProperties}>
                      <ProviderToolCapabilityIcon capability={capability} size={16} />
                    </span>
                    <span className="composer-capability-kind-copy">
                      <span className="composer-capability-kind-title">
                        {providerToolCapabilityLabel(capability, t)}
                        {conflict ? <AlertTriangle aria-hidden="true" size={13} /> : null}
                      </span>
                      <small>{targetCount} {t("settings.providerTools.targets")}</small>
                      {conflict ? (
                        <small className="composer-capability-node-warning" role="alert">
                          {t(conflict.kind === "missing_selection"
                            ? "composer.capability.conflict.missingSelection"
                            : "composer.capability.conflict.targetUnavailable")}
                        </small>
                      ) : null}
                    </span>
                  </Button>
                </div>
              );
            })}
          </aside>
          <section className="composer-capability-dialog-right">
            {isMediaUnderstandingCapability(selectedCapability)
              && draft.enabledCapabilities.includes(selectedCapability)
              && conflicts.some((conflict) => conflict.capability === selectedCapability) ? (
              <div className="mb-3 flex flex-wrap items-center justify-between gap-2 rounded-md border p-3">
                <span className="text-sm text-muted-foreground">{t("composer.capability.unavailableEnabled")}</span>
                <Button aria-label={t("composer.capability.disableUnavailable")} variant="outline" size="sm" disabled={saving}
                  onClick={() => {
                    if (mutationPending.current) return;
                    recoveryFocusPending.current = true;
                    setDraft((current) => ({ ...current,
                      enabledCapabilities: current.enabledCapabilities.filter((capability) => capability !== selectedCapability),
                    }));
                  }}>
                  {t("composer.capability.disableUnavailable")}
                </Button>
              </div>
            ) : null}
            {isMediaUnderstandingCapability(selectedCapability) ? (
                <div className="composer-capability-route-selectors">
                  <label>
                    <span>{t("composer.capability.mediaBackendMode")}</span>
                    <SegmentedControl
                      aria-label={t("composer.capability.mediaBackendMode")}
                      disabled={saving}
                      value={selectedMediaMode}
                      onValueChange={(value) => {
                        const mode = value as PublicAiMediaUnderstandingBackendMode;
                        if (mutationPending.current) return;
                        setMediaEditModes((current) => ({ ...current, [selectedCapability]: mode }));
                        const selection = mode === "provider_tool"
                          ? selectedMediaTargets[0]
                            ? providerMediaUnderstandingSelection(selectedMediaTargets[0])
                            : undefined
                          : selectedMediaCandidates[0]
                            ? modelReplacementSelection(selectedMediaCandidates[0])
                            : undefined;
                        if (selection) {
                          updateMediaSelection(selectedCapability, selection);
                        } else {
                          setDraft((current) => ({ ...current,
                            enabledCapabilities: current.enabledCapabilities.filter((capability) => capability !== selectedCapability),
                          }));
                        }
                      }}
                    >
                          <SegmentedControlItem value="provider_tool" aria-label={t("composer.capability.mode.provider")}>
                            {t("composer.capability.mode.provider")}
                          </SegmentedControlItem>
                          <SegmentedControlItem value="model_replacement" aria-label={t("composer.capability.mode.model")}>
                            {t("composer.capability.mode.model")}
                          </SegmentedControlItem>
                    </SegmentedControl>
                  </label>
                  {selectedMediaMode === "provider_tool" ? (
                    <>
                      <label>
                        <span>{t("settings.providerTools.model")}</span>
                        <Combobox
                          ariaLabel={t("settings.providerTools.model")}
                          placeholder={t("settings.providerTools.selectModel")}
                          searchPlaceholder={t("settings.providerTools.selectModel")}
                          emptyText={t("composer.capability.mediaBackendMissing")}
                          options={selectedMediaProviderModels.map((target) => ({
                            value: providerToolModelKey(target), label: target.target.display_name,
                            keywords: [target.target.provider_id, target.connectionDisplayName, target.target.upstream_target_id],
                          }))}
                          disabled={saving || selectedMediaProviderModels.length === 0}
                          value={selectedMediaProviderModelKey}
                          onValueChange={(modelKey) => {
                            const target = selectedMediaTargets.find((candidate) => (
                              providerToolModelKey(candidate) === modelKey
                              && candidate.connectionId === selectedMediaProviderTarget?.connectionId
                            )) ?? selectedMediaTargets.find((candidate) => (
                              providerToolModelKey(candidate) === modelKey
                            ));
                            if (target) {
                              updateMediaSelection(selectedCapability, providerMediaUnderstandingSelection(target));
                            }
                          }}
                        />
                      </label>
                      <label>
                        <span>{t("settings.providerTools.account")}</span>
                        <Select
                          disabled={saving || selectedMediaProviderAccounts.length === 0}
                          value={selectedMediaProviderTarget?.connectionId ?? ""}
                          onValueChange={(connectionId) => {
                            const target = selectedMediaProviderAccounts.find(
                              (candidate) => candidate.connectionId === connectionId,
                            );
                            if (target) {
                              updateMediaSelection(selectedCapability, providerMediaUnderstandingSelection(target));
                            }
                          }}
                        >
                          <SelectTrigger><SelectValue placeholder={t("settings.providerTools.selectAccount")} /></SelectTrigger>
                          <SelectContent>
                            {selectedMediaProviderAccounts.map((target) => (
                              <SelectItem key={target.connectionId} value={target.connectionId}>
                                {target.connectionDisplayName}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      </label>
                    </>
                  ) : selectedMediaMode === "model_replacement" ? (
                    <>
                      <label>
                        <span>{t("settings.providerTools.model")}</span>
                        <Combobox
                          ariaLabel={t("settings.providerTools.model")}
                          placeholder={t("settings.providerTools.selectModel")}
                          searchPlaceholder={t("settings.providerTools.selectModel")}
                          emptyText={t("composer.capability.mediaBackendMissing")}
                          options={selectedMediaModelChoices.map((candidate) => ({
                            value: mediaUnderstandingModelKey(candidate), label: `${candidate.providerDisplayName} · ${candidate.modelDisplayName}`,
                            keywords: [candidate.selection.provider_id, candidate.selection.model_id, candidate.connectionDisplayName],
                          }))}
                          disabled={saving || selectedMediaModelChoices.length === 0}
                          value={selectedMediaModelKey}
                          onValueChange={(modelKey) => {
                            const candidate = selectedMediaCandidates.find(
                              (item) => mediaUnderstandingModelKey(item) === modelKey,
                            );
                            if (candidate) {
                              updateMediaSelection(selectedCapability, modelReplacementSelection(candidate));
                            }
                          }}
                        />
                      </label>
                      <label>
                        <span>{t("settings.providerTools.account")}</span>
                        <Select
                          disabled={saving || selectedMediaModelAccounts.length === 0}
                          value={selectedMediaModelCandidate?.selection.connection_id ?? ""}
                          onValueChange={(connectionId) => {
                            const candidate = selectedMediaModelAccounts.find(
                              (item) => item.selection.connection_id === connectionId,
                            );
                            if (candidate) {
                              updateMediaSelection(selectedCapability, modelReplacementSelection(
                                candidate,
                                supportedMediaReplacementReasoningEffort(
                                  candidate,
                                  selectedMediaSelection?.mode === "model_replacement" ? selectedMediaSelection.reasoning_effort : undefined,
                                ),
                              ));
                            }
                          }}
                        >
                          <SelectTrigger>
                            <SelectValue placeholder={t("settings.providerTools.selectAccount")} />
                          </SelectTrigger>
                          <SelectContent>
                            {selectedMediaModelAccounts.map((candidate) => (
                              <SelectItem
                                key={candidate.selection.connection_id}
                                value={candidate.selection.connection_id}
                              >
                                {candidate.connectionDisplayName}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      </label>
                      {selectedMediaReasoningEfforts.length > 0 ? (
                        <label>
                          <span>{t("settings.providerTools.reasoningEffort")}</span>
                          <Select
                            disabled={saving || !selectedMediaModelCandidate}
                            value={(selectedMediaSelection?.mode === "model_replacement" ? selectedMediaSelection.reasoning_effort : undefined) ?? "__default__"}
                            onValueChange={(value) => {
                              if (!selectedMediaModelCandidate) return;
                              const effort = value === "__default__"
                                ? undefined
                                : value as PublicAiReasoningEffort;
                              updateMediaSelection(selectedCapability, modelReplacementSelection(selectedMediaModelCandidate, effort));
                            }}
                          >
                            <SelectTrigger><SelectValue /></SelectTrigger>
                            <SelectContent>
                              <SelectItem value="__default__">
                                {t("settings.providerTools.modelDefaultReasoning")}
                              </SelectItem>
                              {selectedMediaReasoningEfforts.map((effort) => (
                                <SelectItem key={effort} value={effort}>
                                  {t(PUBLIC_AI_REASONING_EFFORT_LABEL_KEYS[effort])}
                                </SelectItem>
                              ))}
                            </SelectContent>
                          </Select>
                        </label>
                      ) : null}
                    </>
                  ) : (
                    <div className="composer-capability-dialog-empty">
                      {t("settings.providerTools.noSelection")}
                    </div>
                  )}
                  <p className="composer-capability-media-note settings-muted-line">
                    {t("composer.capability.mediaBackendNote")}
                  </p>
                  {(selectedMediaMode === "provider_tool" ? selectedMediaTargets.length : selectedMediaCandidates.length) === 0
                    ? <p className="settings-muted-line" role="status">{t("composer.capability.mediaBackendMissing")}</p> : null}
                </div>
            ) : isModelExtensionCapability(selectedCapability) ? (
              <div className="composer-capability-extension-list" ref={extensionRecoveryList}>
                <p className="settings-muted-line">
                  {t("composer.capability.extensionHint")}
                </p>
                {unavailableExtensionTargets.map((selection) => (
                  <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-start gap-x-3 gap-y-1 rounded-md border p-3"
                    key={JSON.stringify([selection.connection_id, selection.target_id])}>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-sm font-medium" title={selection.target_id}>{selection.target_id}</p>
                      <span className="text-xs text-muted-foreground">{t("settings.providerTools.unavailable")}</span>
                    </div>
                    <Button variant="outline" size="sm" disabled={saving} data-extension-recovery
                      aria-label={`${t("composer.capability.removeUnavailable")} · ${selection.target_id}`}
                      onClick={() => {
                        if (mutationPending.current) return;
                        recoveryFocusPending.current = true;
                        setDraft((current) => ({ ...current, extensionTargetSets: {
                          ...current.extensionTargetSets,
                          [selectedCapability]: (current.extensionTargetSets[selectedCapability] ?? []).filter((saved) =>
                            saved.connection_id !== selection.connection_id || saved.target_id !== selection.target_id),
                        } }));
                      }}>
                      {t("composer.capability.removeUnavailable")}
                    </Button>
                    <details className="col-span-2 min-w-0 text-xs text-muted-foreground">
                      <summary className="cursor-pointer">{t("composer.capability.unavailableDetails")}</summary>
                      <dl className="mt-2 space-y-1 break-all">
                        <dt>{t("settings.providerTools.model")}</dt><dd>{selection.target_id}</dd>
                        <dt>{t("settings.providerTools.account")}</dt><dd>{selection.connection_id}</dd>
                      </dl>
                    </details>
                  </div>
                ))}
                {selectedModels.length === 0 && unavailableExtensionTargets.length === 0
                  ? <p role="status" className="settings-muted-line">{t("composer.capability.empty.noTargets")}</p> : null}
                {selectedModels.map((model) => {
                  // modelTargets contains every configured account for one extension model.
                  // modelTargets 包含一个扩展模型的全部已配置账号。
                  const modelTargets = capabilityTargets.filter((target) => (
                    target.target.provider_id === model.target.provider_id
                    && target.target.upstream_target_id === model.target.upstream_target_id
                  ));
                  // selectedExtension resolves the saved target for this model.
                  // selectedExtension 解析当前模型已保存的目标。
                  const selectedExtension = selectedExtensionTargets.find((selection) => (
                    modelTargets.some((target) => (
                      target.connectionId === selection.connection_id
                      && target.target.target_id === selection.target_id
                    ))
                  ));
                  // selectedExtensionTarget resolves the active account or the first candidate.
                  // selectedExtensionTarget 解析活动账号或第一个候选账号。
                  const selectedExtensionTarget = modelTargets.find((target) => (
                    target.connectionId === selectedExtension?.connection_id
                    && target.target.target_id === selectedExtension.target_id
                  )) ?? modelTargets[0];
                  // profile contains reviewed model behavior and constraints.
                  // profile 包含已审核的模型行为与约束。
                  const profile = model.target.contract.extension_profile;
                  return (
                    <ProviderToolExtensionModelCard
                      accountControl={(
                        <Select
                          disabled={saving || !selectedExtension}
                          onValueChange={(connectionId) => {
                            const replacement = modelTargets.find((target) => target.connectionId === connectionId);
                            if (!replacement) return;
                            const retained = selectedExtensionTargets.filter((selection) => !modelTargets.some((target) => (
                              target.connectionId === selection.connection_id
                              && target.target.target_id === selection.target_id
                            )));
                            retained.push(selectionForTarget(replacement));
                            setDraft((current) => ({
                              ...current,
                              extensionTargetSets: {
                                ...current.extensionTargetSets,
                                [selectedCapability]: retained,
                              },
                            }));
                          }}
                          value={selectedExtensionTarget?.connectionId ?? ""}
                        >
                          <SelectTrigger><SelectValue /></SelectTrigger>
                          <SelectContent>
                            {modelTargets.map((target) => (
                              <SelectItem key={target.connectionId} value={target.connectionId}>
                                {target.connectionDisplayName}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      )}
                      characteristics={null}
                      checked={Boolean(selectedExtension)}
                      checkboxDisabled={saving || !profile}
                      constraints={t("composer.capability.extensionConstraints").replace(
                        "{value}",
                        profile?.constraints?.length
                          ? profile.constraints.join(t("global.listDelimiter"))
                          : "—",
                      )}
                      key={`${model.target.provider_id}/${model.target.upstream_target_id}`}
                      limitations={t("composer.capability.extensionLimitations").replace(
                        "{value}",
                        profile?.limitations?.length
                          ? profile.limitations.join(t("global.listDelimiter"))
                          : "—",
                      )}
                      modelId={model.target.upstream_target_id}
                      onCheckedChange={(checked) => {
                        const retained = selectedExtensionTargets.filter((selection) => !modelTargets.some((target) => (
                          target.connectionId === selection.connection_id
                          && target.target.target_id === selection.target_id
                        )));
                        if (checked && selectedExtensionTarget) {
                          retained.push(selectionForTarget(selectedExtensionTarget));
                        }
                        setDraft((current) => ({
                          ...current,
                          extensionTargetSets: {
                            ...current.extensionTargetSets,
                            [selectedCapability]: retained,
                          },
                        }));
                      }}
                      providerId={model.target.provider_id}
                      summary={profile?.summary ?? t("composer.capability.extensionProfileMissing")}
                      title={model.target.display_name}
                    />
                  );
                })}
              </div>
            ) : isDirectProviderToolCapability(selectedCapability) ? (
              <div className="composer-capability-route-selectors">
                <label>
                  <span>{t("settings.providerTools.model")}</span>
                  <Combobox
                    ariaLabel={t("settings.providerTools.model")}
                    placeholder={t("settings.providerTools.selectModel")}
                    searchPlaceholder={t("settings.providerTools.selectModel")}
                    emptyText={t("composer.capability.empty.noTargets")}
                    options={selectedModels.map((target) => ({ value: target.target.target_id, label: target.target.display_name,
                      keywords: [target.target.provider_id, target.connectionDisplayName, target.target.upstream_target_id],
                    }))}
                    disabled={saving || selectedModels.length === 0}
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
                  />
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
                {selectedModels.length === 0 ? <p role="status" className="settings-muted-line">{t("composer.capability.empty.noTargets")}</p> : null}
              </div>
            ) : (
              <div className="composer-capability-dialog-empty">
                {t("composer.capability.empty.noTargets")}
              </div>
            )}
          </section>
        </div> : null}
        </div>
        <DialogFooter className="composer-capability-dialog-footer">
          <span className="composer-capability-dialog-save-hint">
            {!settingsReady
              ? loadStatus === "failed" ? t("composer.capability.loadFailed") : <InlineStreamPlaceholder decorative />
              : hasUnsavedChanges
              ? t("composer.capability.saveHint.unsaved")
              : t("composer.capability.saveHint.clean")}
          </span>
          <button
            className="skill-action-button"
            disabled={!settingsReady || saving || !hasUnsavedChanges || conflicts.length > 0}
            onClick={() => void save()}
            type="button"
          >
            {saving && settingsReady
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
      Object.entries(settings.extensionTargetSets)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([capability, selections]) => [
          capability,
          ...(selections ?? [])
            .map((selection) => `${selection.connection_id.length}:${selection.connection_id}:${selection.target_id.length}:${selection.target_id}`)
            .sort(),
        ]),
      Object.entries(settings.mediaUnderstandingSelections)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([capability, selection]) => selection?.mode === "provider_tool"
          ? [
              capability,
              selection.mode,
              selection.selection.connection_id,
              selection.selection.target_id,
            ]
          : [
              capability,
              selection?.mode ?? "",
              selection?.selection.provider_id ?? "",
              selection?.selection.model_id ?? "",
              selection?.selection.connection_id ?? "",
              selection?.reasoning_effort ?? "",
            ]),
    ],
  );
}

// normalizedComposerProviderToolSettings upgrades an older in-memory draft with the exact route map.
// normalizedComposerProviderToolSettings 使用精确路由映射升级旧版内存草稿。
function normalizedComposerProviderToolSettings(
  settings: ComposerProviderToolSettingsSnapshot,
): ComposerProviderToolSettingsSnapshot {
  return {
    ...settings,
    mediaUnderstandingSelections: settings.mediaUnderstandingSelections ?? {},
  };
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

// orderedProviderToolCapabilities merges provider-native targets with model-replacement-only media backends.
// orderedProviderToolCapabilities 合并供应商原生目标与仅由模型代替提供的媒体后端。
export function orderedProviderToolCapabilities(
  snapshot: PublicAiSettingsSnapshot,
): PublicAiProviderToolCapabilityKind[] {
  // candidateCapabilities exposes every fixed facade backed by any currently executable model route.
  // candidateCapabilities 公开由任一当前可执行模型路由支持的全部固定外壳。
  const candidateCapabilities = ([
    "image_understand",
    "video_understand",
    "audio_understand",
  ] as const).filter((capability) => (snapshot.mediaUnderstandingModelCandidates ?? [])
    .some((candidate) => (
    modelCapabilitiesSupportMediaUnderstanding(candidate.capabilities, capability)
  )));
  // mediaCapabilities includes available routes without treating global defaults as an allowlist.
  // mediaCapabilities 包含可用路由，不把全局默认值当作允许列表。
  const mediaCapabilities = Array.from(new Set([
    ...candidateCapabilities,
  ]))
    .filter(isMediaUnderstandingCapability);
  return orderProviderToolCapabilities(
    [
      ...snapshot.providerToolTargets.map((target) => target.target.contract.capability),
      ...mediaCapabilities,
    ],
  );
}

// composerProviderToolCapabilities retains configurable empty media modes and saved extension categories for recovery.
// composerProviderToolCapabilities 保留可配置的空媒体模式与已保存的扩展分类，供用户恢复配置。
// Parameters: snapshot supplies current targets and settings supplies exact saved choices. Returns ordered unique categories.
// 参数：snapshot 提供当前目标，settings 提供准确已存选择。返回已排序且去重的分类。
function composerProviderToolCapabilities(
  snapshot: PublicAiSettingsSnapshot,
  settings: ComposerProviderToolSettingsSnapshot,
): PublicAiProviderToolCapabilityKind[] {
  return orderProviderToolCapabilities([
    ...orderedProviderToolCapabilities(snapshot),
    "image_understand", "video_understand", "audio_understand",
    ...settings.enabledCapabilities,
    ...Object.keys(settings.extensionTargetSets).filter((capability) =>
      isModelExtensionCapability(capability as PublicAiProviderToolCapabilityKind)),
    ...Object.keys(settings.mediaUnderstandingSelections).filter(isMediaUnderstandingCapability),
  ] as PublicAiProviderToolCapabilityKind[]);
}

// PublicAiMediaUnderstandingCapabilityKind is the closed fixed input-only media facade set.
// PublicAiMediaUnderstandingCapabilityKind 是封闭的固定仅输入型媒体外壳集合。
type PublicAiMediaUnderstandingCapabilityKind =
  | "image_understand"
  | "video_understand"
  | "audio_understand";

// isMediaUnderstandingCapability identifies the three fixed input-only media facades.
// isMediaUnderstandingCapability 标识三个固定的仅输入型媒体外壳。
export function isMediaUnderstandingCapability(
  capability: string,
): capability is PublicAiMediaUnderstandingCapabilityKind {
  return capability === "image_understand"
    || capability === "video_understand"
    || capability === "audio_understand";
}


// composerCapabilityIsEnabled resolves direct switches and extension selections without mixing stores.
// composerCapabilityIsEnabled 在不混用存储的前提下解析直连开关与扩展选择状态。
export function composerCapabilityIsEnabled(
  settings: ComposerProviderToolSettingsSnapshot,
  capability: PublicAiProviderToolCapabilityKind,
): boolean {
  return isDirectProviderToolCapability(capability)
    ? settings.enabledCapabilities.includes(capability)
    : (settings.extensionTargetSets[capability]?.length ?? 0) > 0;
}

// providerToolCapabilitySwitchDisabled blocks saving and unavailable new activation while preserving stale-state recovery.
// providerToolCapabilitySwitchDisabled 在保存期间以及无可用后端的新开启场景禁用开关，同时保留过期状态恢复能力。
export function providerToolCapabilitySwitchDisabled(
  saving: boolean,
  targetCount: number,
  enabled: boolean,
): boolean {
  return saving || (targetCount === 0 && !enabled);
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
    if (isMediaUnderstandingCapability(capability)) {
      // mediaSelection contains the complete session-owned route and never resolves through system defaults.
      // mediaSelection 包含完整会话自有路由，绝不通过系统默认值解析。
      const mediaSelection = settings.mediaUnderstandingSelections[capability];
      if (!mediaSelection) {
        conflicts.push({
          capability,
          kind: "missing_selection",
          message: `Capability ${capability} has no exact media-understanding route.`,
        });
        continue;
      }
      const available = mediaSelection.mode === "provider_tool"
        ? snapshot.providerToolTargets.some((target) => (
            target.connectionId === mediaSelection.selection.connection_id
            && target.target.target_id === mediaSelection.selection.target_id
            && target.target.contract.capability === capability
          ))
        : (snapshot.mediaUnderstandingModelCandidates ?? []).some((candidate) => (
            candidate.selection.provider_id === mediaSelection.selection.provider_id
            && candidate.selection.model_id === mediaSelection.selection.model_id
            && candidate.selection.connection_id === mediaSelection.selection.connection_id
            && modelCapabilitiesSupportMediaUnderstanding(candidate.capabilities, capability)
            && (
              mediaSelection.reasoning_effort === undefined
              || mediaReplacementReasoningEfforts(candidate.capabilities)
                .includes(mediaSelection.reasoning_effort)
            )
          ));
      if (!available) {
        conflicts.push({
          capability,
          kind: "target_unavailable",
          message: `The exact media-understanding route for ${capability} is no longer available.`,
        });
      }
      continue;
    }
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
  for (const [capability, selections] of Object.entries(settings.extensionTargetSets)) {
    if (!isModelExtensionCapability(capability as PublicAiProviderToolCapabilityKind)) {
      conflicts.push({
        capability: capability as PublicAiProviderToolCapabilityKind,
        kind: "invalid_extension_target_set",
        message: `Capability ${capability} cannot use extension targets.`,
      });
      continue;
    }
    for (const selection of selections ?? []) {
      const key = `${selection.connection_id.length}:${selection.connection_id}:${selection.target_id.length}:${selection.target_id}`;
      const target = snapshot.providerToolTargets.find((candidate) => (
        candidate.connectionId === selection.connection_id
        && candidate.target.target_id === selection.target_id
      ));
      if (current.get(key) !== capability || target?.manifestSourceLayer === "custom") {
        conflicts.push({
          capability: capability as PublicAiProviderToolCapabilityKind,
          kind: "invalid_extension_target_set",
          message: `The saved native extension model or account for ${capability} is unavailable.`,
        });
        break;
      }
    }
  }
  return conflicts;
}

// uniqueProviderToolModels collapses account-expanded targets into one row per model.
// uniqueProviderToolModels 将按账号展开的目标折叠为每个模型一行。
function providerToolModelKey(target: PublicAiProviderToolTargetSnapshot): string {
  return JSON.stringify([
    target.target.provider_id,
    target.target.upstream_target_id,
  ]);
}

// mediaUnderstandingModelKey returns one browser-local provider-model identity.
// mediaUnderstandingModelKey 返回一个浏览器本地供应商模型身份。
function mediaUnderstandingModelKey(candidate: PublicAiMediaUnderstandingModelCandidate): string {
  return JSON.stringify([
    candidate.selection.provider_id,
    candidate.selection.model_id,
  ]);
}

// uniqueMediaUnderstandingModels collapses exact account routes into stable model choices.
// uniqueMediaUnderstandingModels 将精确账号路由折叠为稳定模型选择。
function uniqueMediaUnderstandingModels(
  candidates: PublicAiMediaUnderstandingModelCandidate[],
): PublicAiMediaUnderstandingModelCandidate[] {
  const models = new Map<string, PublicAiMediaUnderstandingModelCandidate>();
  for (const candidate of candidates) {
    const modelKey = mediaUnderstandingModelKey(candidate);
    if (!models.has(modelKey)) models.set(modelKey, candidate);
  }
  return Array.from(models.values());
}

// providerMediaUnderstandingSelection freezes one provider target into the closed backend union.
// providerMediaUnderstandingSelection 将一个供应商目标冻结为封闭后端联合类型。
function providerMediaUnderstandingSelection(
  target: PublicAiProviderToolTargetSnapshot,
): PublicAiMediaUnderstandingBackendSelection {
  return {
    mode: "provider_tool",
    selection: selectionForTarget(target),
  };
}

// modelReplacementSelection freezes one model route and optional reasoning level.
// modelReplacementSelection 冻结一条模型路由及其可选推理档位。
function modelReplacementSelection(
  candidate: PublicAiMediaUnderstandingModelCandidate,
  reasoningEffort?: PublicAiReasoningEffort,
): PublicAiMediaUnderstandingBackendSelection {
  return {
    mode: "model_replacement",
    selection: candidate.selection,
    ...(reasoningEffort ? { reasoning_effort: reasoningEffort } : {}),
  };
}

// supportedMediaReplacementReasoningEffort retains an effort only when the exact route supports it.
// supportedMediaReplacementReasoningEffort 仅在精确路由支持时保留推理档位。
export function supportedMediaReplacementReasoningEffort(
  candidate: PublicAiMediaUnderstandingModelCandidate,
  reasoningEffort?: PublicAiReasoningEffort,
): PublicAiReasoningEffort | undefined {
  if (!reasoningEffort) return undefined;
  return mediaReplacementReasoningEfforts(candidate.capabilities).includes(reasoningEffort)
    ? reasoningEffort
    : undefined;
}

// firstAvailableMediaUnderstandingSelection resolves the system default only as a preference.
// firstAvailableMediaUnderstandingSelection 仅把系统默认值作为优先项解析。
function firstAvailableMediaUnderstandingSelection(
  snapshot: PublicAiSettingsSnapshot,
  capability: PublicAiMediaUnderstandingCapabilityKind,
  targets: PublicAiProviderToolTargetSnapshot[],
  candidates: PublicAiMediaUnderstandingModelCandidate[],
): PublicAiMediaUnderstandingBackendSelection | undefined {
  const systemDefault = snapshot.mediaUnderstandingSelections?.[capability];
  if (systemDefault?.mode === "provider_tool") {
    const target = targets.find((candidate) => (
      candidate.connectionId === systemDefault.selection.connection_id
      && candidate.target.target_id === systemDefault.selection.target_id
    ));
    if (target) return providerMediaUnderstandingSelection(target);
  }
  if (systemDefault?.mode === "model_replacement") {
    const candidate = candidates.find((item) => (
      item.selection.provider_id === systemDefault.selection.provider_id
      && item.selection.model_id === systemDefault.selection.model_id
      && item.selection.connection_id === systemDefault.selection.connection_id
    ));
    if (candidate) {
      return modelReplacementSelection(
        candidate,
        supportedMediaReplacementReasoningEffort(candidate, systemDefault.reasoning_effort),
      );
    }
  }
  if (targets[0]) return providerMediaUnderstandingSelection(targets[0]);
  return candidates[0] ? modelReplacementSelection(candidates[0]) : undefined;
}

function uniqueProviderToolModels(
  targets: PublicAiProviderToolTargetSnapshot[],
): PublicAiProviderToolTargetSnapshot[] {
  const models = new Map<string, PublicAiProviderToolTargetSnapshot>();
  for (const target of targets) {
    // modelKey prevents collisions across providers while grouping account-expanded targets.
    // modelKey 在折叠账号展开目标时防止不同供应商之间发生冲突。
    const modelKey = providerToolModelKey(target);
    if (!models.has(modelKey)) models.set(modelKey, target);
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
// Parameters: capability is the closed tool capability; returns a complete existing CSS color-token expression.
// 参数：capability 是封闭工具能力；返回现有 CSS 完整颜色令牌表达式。
function providerToolCapabilityColor(capability: PublicAiProviderToolCapabilityKind): string {
  if (capability === "web_search" || capability === "web_extract") {
    return "var(--chart-1)";
  }
  if (
    capability === "image_understand"
    || capability === "video_understand"
    || capability === "audio_understand"
  ) {
    return "var(--chart-3)";
  }
  if (capability === "image_generate" || capability === "image_edit") {
    return "var(--chart-5)";
  }
  if (capability === "video_generate" || capability === "video_edit" || capability === "video_extend") {
    return "var(--chart-4)";
  }
  if (capability === "speech_synthesize" || capability === "speech_transcribe") {
    return "var(--chart-2)";
  }
  if (capability === "music_cover") {
    return "var(--chart-2)";
  }
  return "var(--chart-3)";
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
    image_understand: "settings.aiUnderstandImage",
    video_understand: "settings.aiUnderstandVideo",
    audio_understand: "settings.aiUnderstandAudio",
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
// Parameters: props supplies the exact selected value, options and change callback; returns a compact selector without inventing a missing selection.
// 参数：props 提供精确选中值、选项及变更回调；返回不伪造失效选择的紧凑选择器。
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
  const selectedOption = props.options.find((option) => option.value === props.value);
  // observedOpenSignal consumes each command once; mounting or re-enabling must not replay it.
  // observedOpenSignal 每条指令只消费一次；挂载或重新启用时不得重放。
  const observedOpenSignal = React.useRef(props.openSignal);

  React.useEffect(() => {
    // changed identifies a new explicit command rather than an availability refresh.
    // changed 区分新显式指令与可用性刷新。
    const changed = observedOpenSignal.current !== props.openSignal;
    observedOpenSignal.current = props.openSignal;
    if (props.disabled) {
      setOpen(false);
      return;
    }
    if (changed && props.openSignal !== undefined && props.openSignal > 0) setOpen(true);
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
      <SelectContent className="composer-select-content" placement="auto">
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
